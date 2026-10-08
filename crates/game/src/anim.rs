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

/// The animation of the current interaction: an optional start clip played once (and then
/// any steps, once each in order: making a drink, pour, blend, stop), then loop clips (random
/// variants of the given name prefixes). `side` picks the Sim's half of a two-Sim social (`_x`
/// for the one starting it, `_y` for the other).
#[derive(Component, Clone, PartialEq, Debug)]
pub struct ActionClip {
    pub start: Option<&'static str>,
    pub steps: &'static [&'static str],
    pub loops: &'static [&'static str],
    pub side: Option<char>,
    /// How it ends, played once the interaction's done (getting out of bed, closing the book),
    /// and how long that takes (seconds).
    pub end: &'static [&'static str],
    pub end_secs: f32,
    /// A clip before the start (sitting down in the chair first), and one after the steps
    /// (getting up out of it at the end).
    pub before: Option<&'static str>,
    pub after: Option<&'static str>,
    /// The loops played in turn rather than at random (a ball fetched, bowled, the pins seen).
    pub in_order: bool,
}

/// Sitting about: a living chair's breathing and crossed legs.
pub const SIT_LOOPS: &[&str] = &["a2o_chairLiving_sit_breathe_loop_x", "a2o_chairLiving_sit_crossedLeg_front_loop_x"];

impl ActionClip {
    pub const fn new(start: Option<&'static str>, loops: &'static [&'static str]) -> Self {
        Self { start, steps: &[], loops, side: None, end: &[], end_secs: 0.0, before: None, after: None, in_order: false }
    }
    /// A start clip, steps after it in order, then loops.
    pub const fn steps(start: &'static str, steps: &'static [&'static str], loops: &'static [&'static str]) -> Self {
        Self { start: Some(start), steps, loops, side: None, end: &[], end_secs: 0.0, before: None, after: None, in_order: false }
    }
    pub const fn social(loops: &'static [&'static str], side: char) -> Self {
        Self { start: None, steps: &[], loops, side: Some(side), end: &[], end_secs: 0.0, before: None, after: None, in_order: false }
    }
    /// With its loops played in turn.
    pub const fn in_order(mut self) -> Self {
        self.in_order = true;
        self
    }
    /// With the clips it ends with (and their length).
    pub const fn ending(mut self, end: &'static [&'static str], secs: f32) -> Self {
        self.end = end;
        self.end_secs = secs;
        self
    }
}

/// Something carried in the arms while walking (a dish to the sink): the game's carry clip,
/// which holds the arms (and the prop) over whatever the legs are doing.
#[derive(Component, Clone, Copy)]
pub struct Carrying(pub &'static str);

impl Carrying {
    /// Whether the carry shows over this Sim's animation now: walking, or standing about
    /// (not while using something, which has its own animation).
    pub fn shows(pose: Pose, acting: bool) -> bool {
        pose == Pose::Walk || !acting
    }
}

/// The bones a carry clip moves: the shoulders, arms, hands and fingers.
fn arm_bone(name: &str) -> bool {
    ["Clavicle", "UpperArm", "Bicep", "Forearm", "Wrist", "Hand", "Index", "_Mid", "Pinky", "Ring", "Thumb", "Shoulder"].iter().any(|k| name.contains(k))
}

#[derive(Component, Default)]
pub struct ClipPlayer {
    pub name: String,
    pub clip: Option<Arc<Clip>>,
    pub time: f32,
    /// How far through its carry clip (when carrying something).
    pub carry_time: f32,
    /// The script being played, and how far through its start and steps it is.
    script: Option<ActionClip>,
    step: usize,
    /// Pose before the last clip change, faded out over a short blend.
    from: Vec<Transform>,
    blend: f32,
}

impl ClipPlayer {
    /// The body rebuilt (a change of clothes, say): the clip plays on where it was, only the
    /// cross-fade from the old skeleton's pose is dropped.
    pub fn rebuilt(&mut self) {
        self.from.clear();
        self.blend = 0.0;
    }
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
    const SIT: &[&str] = SIT_LOOPS;
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

/// Picks the next clip of a script, and the step after it: the start clip and the steps once
/// each, in order, then a random loop variant.
fn next_clip(lib: &mut ClipLibrary, data: &Baked, script: &ActionClip, child: bool, mut step: usize) -> (Option<String>, usize) {
    let sequence: Vec<&'static str> = script.before.into_iter().chain(script.start).chain(script.steps.iter().copied()).chain(script.after).collect();
    while let Some(s) = sequence.get(step) {
        step += 1;
        if let Some(n) = lib.variants(data, s, script.side, child).first() {
            return (Some(n.clone()), step);
        }
    }
    if script.in_order && !script.loops.is_empty() {
        let s = script.loops[(step - sequence.len()) % script.loops.len()];
        return (lib.variants(data, s, script.side, child).first().cloned(), step + 1);
    }
    (next_loop(lib, data, script, child), step)
}

/// A random loop variant of a script.
fn next_loop(lib: &mut ClipLibrary, data: &Baked, script: &ActionClip, child: bool) -> Option<String> {
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
    mut sims: Query<(
        Entity,
        &Sim,
        &SimAnim,
        &Skeleton,
        Option<&ActionClip>,
        &mut ClipPlayer,
        Has<crate::little::Carried>,
        Option<&crate::little::ToddlerSkills>,
        Option<&Carrying>,
        Has<crate::jog::Jogging>,
        Option<&crate::nav::PathFollow>,
        Option<&crate::little::Pregnancy>,
        Has<crate::supernatural::WolfForm>,
    )>,
    mut joints: Query<&mut Transform, Without<Sim>>,
    mut cues: MessageWriter<crate::sound::ClipCue>,
    mut arms: Local<HashMap<String, Arc<Vec<bool>>>>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed];
    for (entity, sim, anim, skel, action, mut player, carried, toddler, carrying, jogging, path, pregnancy, wolf) in &mut sims {
        use crate::sim::Occult;
        let fast = matches!(path.filter(|p| !p.done).map(|p| p.now), Some(crate::nav::WalkStyle::Run | crate::nav::WalkStyle::FastRun | crate::nav::WalkStyle::FastJog));
        let child = sim.age == crate::sim::Age::Child;
        let style = path.filter(|p| !p.done).map_or(crate::nav::WalkStyle::Walk, |p| p.now);
        let script = match action {
            // (Supernaturals: a werewolf's prowl, a zombie's shamble, a vampire's dash.)
            _ if anim.pose == Pose::Walk && wolf => ActionClip::new(None, if fast { &["a_werewolf_run"] } else { &["a_werewolf_walk"] }),
            _ if anim.pose == Pose::Walk && sim.occult == Some(Occult::Zombie) => ActionClip::new(None, &["a_zombie_walk"]),
            _ if anim.pose == Pose::Walk && fast && sim.occult == Some(Occult::Vampire) => ActionClip::new(None, &["a_vampires_run"]),
            _ if matches!(anim.pose, Pose::Stand) && action.is_none() && wolf => ActionClip::new(None, &["a_werewolf_idle_", "a_werewolf_idleSniff"]),
            _ if matches!(anim.pose, Pose::Stand) && action.is_none() && sim.occult == Some(Occult::Zombie) => ActionClip::new(None, &["a_zombie_idle", "a_zombie_graaains_loop"]),
            // (Jogging: the game's jog, for everyone.)
            _ if jogging && anim.pose == Pose::Walk => ActionClip::new(None, &["a_male_jog"]),
            // (The route's walk style: hurrying, jogging, running.)
            _ if anim.pose == Pose::Walk && !sim.age.is_little() && let Some(clip) = style.clip(child, sim.female) => ActionClip::new(None, clip),
            // (Heavily pregnant, the game's waddle.)
            _ if anim.pose == Pose::Walk && sim.female && pregnancy.is_some_and(|p| p.stage >= 2) => ActionClip::new(None, &["a_female_walk_pregnant"]),
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
                player.step = 0;
                // Sounds the old animation left looping stop with it.
                cues.write(crate::sound::ClipCue { sim: entity, name: String::new(), action: s3formats::sim::SoundAction::StopLoop });
            }
            // The start clip (and steps) play once; after that, loop variants.
            let (next, step) = next_clip(&mut lib, &data, &script, child, player.step);
            player.step = step;
            if let Some(name) = next {
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
        // (A walk cycle in step with the Sim's pace, so their feet stay planted.)
        let step = match (anim.pose, path.filter(|p| !p.done && p.pace > 0.0), crate::nav::cycle_speed(&player.name)) {
            (Pose::Walk, Some(p), Some(cycle)) => (p.pace / cycle).clamp(0.5, 2.0),
            _ => 1.0,
        };
        player.time += dt * step;
        player.blend = (player.blend - dt / 0.25).max(0.0);
        let Some(clip) = player.clip.clone() else { continue };
        // A carry: its clip over the arms (looping on its own time).
        let carry = carrying.filter(|_| Carrying::shows(anim.pose, action.is_some())).and_then(|c| lib.get(&data, c.0));
        let carry_t = match &carry {
            Some(c) => {
                player.carry_time += dt;
                if c.duration > 0.0 { player.carry_time % c.duration } else { 0.0 }
            }
            None => {
                player.carry_time = 0.0;
                0.0
            }
        };
        let arm = carry.as_ref().map(|_| arms.entry(skel.rig.name.clone()).or_insert_with(|| Arc::new(skel.rig.bones.iter().map(|b| arm_bone(&b.name)).collect())).clone());
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
            if let (Some(c), Some(arm)) = (&carry, &arm)
                && arm.get(i).copied().unwrap_or(false)
                && let Some(track) = c.tracks.get(&bone.hash)
            {
                if let Some(p) = sample_track_vec(&track.translation, carry_t) {
                    target.translation = p;
                }
                if let Some(r) = sample_track_quat(&track.rotation, carry_t) {
                    target.rotation = r;
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
