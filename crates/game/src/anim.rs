//! Plays the game's animation clips (CLIP) on sim skeletons.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use s3formats::sim::Clip;

use crate::PlayMode;
use crate::clock::{GameClock, SPEED_RATES};
use crate::baked::Baked;
use crate::sim::{Pose, Sim, SimAnim};
use crate::simbody::Skeleton;

pub struct AnimPlugin;

impl Plugin for AnimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClipLibrary>()
            .add_systems(Update, drive_skeletons.run_if(in_state(PlayMode::Live).or_else(in_state(crate::AppState::CreateHousehold))));
    }
}

#[derive(Resource, Default)]
pub struct ClipLibrary {
    cache: HashMap<String, Option<Arc<Clip>>>,
    /// Clip names matching a (prefix, side, child) request.
    variants: HashMap<(&'static str, Option<char>, bool), Vec<String>>,
}

impl ClipLibrary {
    pub fn get(&mut self, data: &Baked, name: &str) -> Option<Arc<Clip>> {
        if let Some(c) = self.cache.get(name) {
            return c.clone();
        }
        let clip = data.0.clip(name);
        if clip.is_none() {
            warn!("animation clip {name} not found");
        }
        self.cache.insert(name.to_string(), clip.clone());
        clip
    }

    /// Baked clips starting with `prefix` for the Sim's side (`_x` / `_y`), preferring the
    /// child version (`c_` / `c2o_`) for children.
    pub fn variants(&mut self, data: &Baked, prefix: &'static str, side: Option<char>, child: bool) -> &[String] {
        self.variants.entry((prefix, side, child)).or_insert_with(|| {
            let matches = |p: &str| -> Vec<String> {
                // A prefix naming a clip exactly means just that clip (`a_male_walk`, not
                // `a_male_walk_stop_trip`).
                if let Some(exact) = data.0.clip_names.iter().find(|n| n.eq_ignore_ascii_case(p)) {
                    return vec![exact.clone()];
                }
                // (Never a prop's own clip, `…_guitar`.)
                let prop = |n: &str| n.rsplit_once('_').is_some_and(|(_, a)| crate::props::prop_actor(a));
                data.0
                    .clip_names
                    .iter()
                    .filter(|n| n.starts_with(p) && !prop(n) && side.is_none_or(|s| n.ends_with(&format!("_{s}"))))
                    .cloned()
                    .collect()
            };
            if child {
                let kid = if let Some(r) = prefix.strip_prefix("a2o_") {
                    Some(format!("c2o_{r}"))
                } else {
                    prefix.strip_prefix("a_").map(|r| format!("c_{r}"))
                };
                if let Some(k) = kid {
                    let v = matches(&k);
                    if !v.is_empty() {
                        return v;
                    }
                }
            }
            matches(prefix)
        })
    }
}

/// The animation of the current interaction: an optional start clip played once, then loop
/// clips (random variants of the given name prefixes). `side` picks the Sim's half of a
/// two-Sim social (`_x` for the one starting it, `_y` for the other).
#[derive(Component, Clone, PartialEq, Debug)]
pub struct ActionClip {
    pub start: Option<&'static str>,
    pub loops: &'static [&'static str],
    pub side: Option<char>,
}

impl ActionClip {
    pub const fn new(start: Option<&'static str>, loops: &'static [&'static str]) -> Self {
        Self { start, loops, side: None }
    }
    pub const fn social(loops: &'static [&'static str], side: char) -> Self {
        Self { start: None, loops, side: Some(side) }
    }
}

#[derive(Component, Default)]
pub struct ClipPlayer {
    pub name: String,
    pub clip: Option<Arc<Clip>>,
    pub time: f32,
    /// The script being played, and whether its start clip is done.
    script: Option<ActionClip>,
    started: bool,
    /// Pose before the last clip change, faded out over a short blend.
    from: Vec<Transform>,
    blend: f32,
}

pub fn sample_track_vec(keys: &[(f32, [f32; 3])], t: f32) -> Option<Vec3> {
    let first = keys.first()?;
    if t <= first.0 || keys.len() == 1 {
        return Some(Vec3::from(first.1));
    }
    for w in keys.windows(2) {
        if t <= w[1].0 {
            let f = ((t - w[0].0) / (w[1].0 - w[0].0).max(1e-5)).clamp(0.0, 1.0);
            return Some(Vec3::from(w[0].1).lerp(Vec3::from(w[1].1), f));
        }
    }
    Some(Vec3::from(keys.last()?.1))
}

pub fn sample_track_quat(keys: &[(f32, [f32; 4])], t: f32) -> Option<Quat> {
    let q = |a: [f32; 4]| Quat::from_xyzw(a[0], a[1], a[2], a[3]).normalize();
    let first = keys.first()?;
    if t <= first.0 || keys.len() == 1 {
        return Some(q(first.1));
    }
    for w in keys.windows(2) {
        if t <= w[1].0 {
            let f = ((t - w[0].0) / (w[1].0 - w[0].0).max(1e-5)).clamp(0.0, 1.0);
            return Some(q(w[0].1).slerp(q(w[1].1), f));
        }
    }
    Some(q(keys.last()?.1))
}

/// Default animation for a baby or toddler (crawling about until they've learned to walk).
fn little_script(pose: Pose, age: crate::sim::Age, walks: bool) -> ActionClip {
    use crate::sim::Age;
    match (age, pose) {
        (Age::Baby, Pose::Lie) => ActionClip::new(Some("b2o_crib_sleep_start_y"), &["b2o_crib_sleep_loop_y"]),
        (Age::Baby, _) => ActionClip::new(None, &["b2o_crib_idle_breathe"]),
        (_, Pose::Walk) if walks => ActionClip::new(None, &["p_walk"]),
        (_, Pose::Walk) => ActionClip::new(None, &["p_crawl"]),
        (_, Pose::Lie) => ActionClip::new(Some("p2o_crib_sleep_start_y"), &["p2o_crib_sleep_loop_y"]),
        (_, Pose::Talk) => ActionClip::new(None, &["p_idle_friendly_loop"]),
        _ => ActionClip::new(None, &["p_idle_neutral_loop"]),
    }
}

/// Default animation for a pose when the interaction doesn't name one.
fn pose_script(pose: Pose, female: bool, child: bool) -> ActionClip {
    const STAND: &[&str] = &["a_idle_neutral_loop_"];
    const TALK: &[&str] = &["a_idle_friendly_loop_"];
    const SIT: &[&str] = &["a2o_chairLiving_sit_breathe_loop_x", "a2o_chairLiving_sit_crossedLeg_front_loop_x"];
    const LIE: &[&str] = &["a2o_bed_sleep_back"];
    const DANCE: &[&str] = &["a_dance_beg_", "a_dance_med_"];
    const RUN: &[&str] = &["a2o_treadmill_jog_loop"];
    match pose {
        Pose::Walk => ActionClip::new(None, if child { &["c_walk"] } else if female { &["a_female_walk"] } else { &["a_male_walk"] }),
        Pose::Sit => ActionClip::new(None, SIT),
        Pose::Lie => ActionClip::new(None, LIE),
        Pose::Talk => ActionClip::new(None, TALK),
        Pose::Dance => ActionClip::new(None, DANCE),
        Pose::Exercise => ActionClip::new(None, RUN),
        Pose::Use | Pose::Stand => ActionClip::new(None, STAND),
    }
}

/// Picks the next clip of a script: the start clip once, then a random loop variant.
fn next_clip(lib: &mut ClipLibrary, data: &Baked, script: &ActionClip, child: bool, started: bool) -> Option<String> {
    if !started && let Some(s) = script.start {
        if let Some(n) = lib.variants(data, s, script.side, child).first() {
            return Some(n.clone());
        }
    }
    let mut all: Vec<String> = Vec::new();
    for p in script.loops {
        all.extend(lib.variants(data, p, script.side, child).iter().cloned());
    }
    if all.is_empty() {
        return None;
    }
    use rand::seq::IndexedRandom;
    all.choose(&mut rand::rng()).cloned()
}

#[allow(clippy::type_complexity)]
pub fn drive_skeletons(
    time: Res<Time>,
    clock: Res<GameClock>,
    data: Res<Baked>,
    mut lib: ResMut<ClipLibrary>,
    mut sims: Query<(Entity, &Sim, &SimAnim, &Skeleton, Option<&ActionClip>, &mut ClipPlayer, Has<crate::little::Carried>, Option<&crate::little::ToddlerSkills>)>,
    mut joints: Query<&mut Transform, Without<Sim>>,
    mut cues: MessageWriter<crate::sound::ClipCue>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed];
    for (entity, sim, anim, skel, action, mut player, carried, toddler) in &mut sims {
        let child = sim.age == crate::sim::Age::Child;
        let script = match action {
            Some(a) if anim.pose != Pose::Walk => a.clone(),
            _ if sim.age.is_little() => little_script(anim.pose, sim.age, toddler.is_some_and(|t| t.walks())),
            _ => pose_script(anim.pose, sim.female, child),
        };
        let ended = player.clip.as_ref().is_some_and(|c| player.time >= c.duration.max(0.1));
        let changed = player.script.as_ref() != Some(&script);
        // Walks keep looping one clip; everything else moves on to a new variant when a clip ends.
        let cycles = !matches!(anim.pose, Pose::Walk);
        let mut from_time = player.time;
        if changed || (ended && cycles) {
            if changed {
                player.started = false;
                // Sounds the old animation left looping stop with it.
                cues.write(crate::sound::ClipCue { sim: entity, name: String::new(), action: s3formats::sim::SoundAction::StopLoop });
            }
            // The start clip plays once; after that, loop variants.
            let play_start = !player.started && script.start.is_some();
            player.started = true;
            if let Some(name) = next_clip(&mut lib, &data, &script, child, !play_start) {
                if name != player.name || changed {
                    // Snapshot the current pose for a cross-fade.
                    player.from = skel.joints.iter().map(|j| joints.get(*j).copied().unwrap_or_default()).collect();
                    player.blend = 1.0;
                }
                player.clip = lib.get(&data, &name);
                debug!("{} plays {name}", sim.first);
                player.name = name;
                player.time = 0.0;
                from_time = -1e-3;
            }
            player.script = Some(script);
        }
        player.time += dt;
        player.blend = (player.blend - dt / 0.25).max(0.0);
        let Some(clip) = player.clip.clone() else { continue };
        // Sound cues passed this frame (walk clips loop, so count whole cycles).
        if dt > 0.0 && clip.duration > 0.0 && !clip.sounds.is_empty() {
            let d = clip.duration;
            let (k0, k1) = ((from_time / d).floor() as i32, (player.time / d).floor() as i32);
            for k in k0..=k1.min(k0 + 2) {
                for s in &clip.sounds {
                    let at = s.time + k as f32 * d;
                    if at > from_time && at <= player.time {
                        cues.write(crate::sound::ClipCue { sim: entity, name: s.name.clone(), action: s.action });
                    }
                }
            }
        }
        // (Walks cycle; any other clip holds its last pose until the next one takes over,
        // rather than flicking back to its first for a frame.)
        let t = match (clip.duration > 0.0, cycles) {
            (false, _) => 0.0,
            (true, true) => player.time.min(clip.duration),
            (true, false) => player.time % clip.duration,
        };
        for (i, bone) in skel.rig.bones.iter().enumerate() {
            let Ok(mut tf) = joints.get_mut(skel.joints[i]) else { continue };
            let bind = skel.bind[i];
            let mut target = bind;
            let placed = carried && bone.name == "transformBone";
            if let Some(track) = clip.tracks.get(&bone.hash).filter(|_| !placed) {
                // (The face's shape goes back over what the animation does to a bone.)
                let shape = skel.shape.get(i).copied().flatten();
                // The root's translation is root motion; movement is driven by the pathfinder.
                if i != 0
                    && let Some(p) = sample_track_vec(&track.translation, t)
                {
                    target.translation = p + shape.map_or(Vec3::ZERO, |s| s.offset);
                }
                if let Some(r) = sample_track_quat(&track.rotation, t) {
                    target.rotation = shape.map_or(r, |s| s.apply_rotation(r));
                }
            }
            if i == 0 {
                target.rotation = bind.rotation;
            }
            if player.blend > 0.0
                && let Some(from) = player.from.get(i)
            {
                target.translation = target.translation.lerp(from.translation, player.blend);
                target.rotation = target.rotation.slerp(from.rotation, player.blend);
            }
            *tf = target;
        }
    }
}
