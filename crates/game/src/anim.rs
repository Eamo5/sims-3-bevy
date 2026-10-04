//! Plays the game's animation clips (CLIP) on sim skeletons.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use s3formats::sim::Clip;
use s3pkg::types;

use crate::PlayMode;
use crate::clock::{GameClock, SPEED_RATES};
use crate::data::GameData;
use crate::sim::{Pose, Sim, SimAnim};
use crate::simbody::Skeleton;

pub struct AnimPlugin;

impl Plugin for AnimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClipLibrary>()
            .add_systems(Update, drive_skeletons.run_if(in_state(PlayMode::Live)));
    }
}

/// Instance id of an adult / object clip (the low 63 bits of FNV-64 of its name).
fn clip_instance(name: &str) -> u64 {
    s3pkg::fnv64(name) & 0x7FFF_FFFF_FFFF_FFFF
}

#[derive(Resource, Default)]
pub struct ClipLibrary {
    cache: HashMap<String, Option<Arc<Clip>>>,
}

impl ClipLibrary {
    pub fn get(&mut self, data: &GameData, name: &str) -> Option<Arc<Clip>> {
        if let Some(c) = self.cache.get(name) {
            return c.clone();
        }
        let clip = data
            .0
            .read_ti(types::CLIP, clip_instance(name))
            .and_then(|d| Clip::parse(&d).ok())
            .map(Arc::new);
        if clip.is_none() {
            warn!("animation clip {name} not found");
        }
        self.cache.insert(name.to_string(), clip.clone());
        clip
    }
}

/// An explicit clip requested by the current interaction (e.g. showering, playing chess).
#[derive(Component, Clone)]
pub struct ActionClip(pub &'static str);

#[derive(Component, Default)]
pub struct ClipPlayer {
    pub name: String,
    pub clip: Option<Arc<Clip>>,
    pub time: f32,
    /// Pose before the last clip change, faded out over a short blend.
    from: Vec<Transform>,
    blend: f32,
}

fn sample_track_vec(keys: &[(f32, [f32; 3])], t: f32) -> Option<Vec3> {
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

fn sample_track_quat(keys: &[(f32, [f32; 4])], t: f32) -> Option<Quat> {
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

/// Default clip for a pose when the interaction doesn't name one.
fn pose_clip(pose: Pose, female: bool) -> &'static str {
    match pose {
        Pose::Walk => {
            if female {
                "a_female_walk"
            } else {
                "a_male_walk"
            }
        }
        Pose::Sit => "a2o_chairLiving_sit_breathe_loop_x",
        Pose::Lie => "a2o_bed_sleep_back_loop_x",
        Pose::Talk => "a_idle_friendly_loop_1",
        Pose::Dance => "a_dance_beg_posAHeadBob_x",
        Pose::Exercise => "a2o_treadmill_jog_loop_x",
        Pose::Use => "a_idle_neutral_loop_2",
        Pose::Stand => "a_idle_neutral_loop_1",
    }
}

#[allow(clippy::type_complexity)]
fn drive_skeletons(
    time: Res<Time>,
    clock: Res<GameClock>,
    data: Res<GameData>,
    mut lib: ResMut<ClipLibrary>,
    mut sims: Query<(&Sim, &SimAnim, &Skeleton, Option<&ActionClip>, &mut ClipPlayer)>,
    mut joints: Query<&mut Transform, Without<Sim>>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed];
    for (sim, anim, skel, action, mut player) in &mut sims {
        let want = match action {
            Some(a) if anim.pose != Pose::Walk => a.0,
            _ => pose_clip(anim.pose, sim.female),
        };
        if player.name != want {
            // Snapshot the current pose for a cross-fade.
            player.from = skel.joints.iter().map(|j| joints.get(*j).copied().unwrap_or_default()).collect();
            player.blend = 1.0;
            player.name = want.to_string();
            player.clip = lib.get(&data, want);
            player.time = 0.0;
        }
        player.time += dt;
        player.blend = (player.blend - dt / 0.25).max(0.0);
        let Some(clip) = player.clip.clone() else { continue };
        let t = if clip.duration > 0.0 { player.time % clip.duration } else { 0.0 };
        for (i, bone) in skel.rig.bones.iter().enumerate() {
            let Ok(mut tf) = joints.get_mut(skel.joints[i]) else { continue };
            let bind = skel.bind[i];
            let mut target = bind;
            if let Some(track) = clip.tracks.get(&bone.hash) {
                // The root's translation is root motion; movement is driven by the pathfinder.
                if i != 0
                    && let Some(p) = sample_track_vec(&track.translation, t)
                {
                    target.translation = p;
                }
                if let Some(r) = sample_track_quat(&track.rotation, t) {
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
