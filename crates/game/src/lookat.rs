//! Sims look at whoever they're talking to: on top of their animation, the head turns (within
//! a natural reach, easing in and out) towards the other Sim's face, as the game's look-at
//! system does in conversation.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::interact::{ActionKind, ActionQueue, Phase, SocialPartner};
use crate::simbody::Skeleton;

pub struct LookAtPlugin;

impl Plugin for LookAtPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            look_at.after(crate::anim::drive_skeletons).run_if(in_state(crate::PlayMode::Live)),
        );
    }
}

/// How far the head turns (radians) side to side and up and down.
const MAX_YAW: f32 = 1.0;
const MAX_PITCH: f32 = 0.45;
/// How quickly the head eases round (per second).
const EASE: f32 = 4.0;

/// Where a Sim's head is turned, eased.
#[derive(Component, Default)]
struct Gaze {
    yaw: f32,
    pitch: f32,
}

#[allow(clippy::type_complexity)]
fn look_at(
    mut commands: Commands,
    time: Res<Time>,
    sims: Query<(Entity, &Skeleton, Option<&ActionQueue>, Option<&SocialPartner>, Option<&mut Gaze>)>,
    mut joints: Query<(&mut Transform, &GlobalTransform)>,
    mut heads: Local<HashMap<Entity, Vec3>>,
) {
    let dt = time.delta_secs().min(0.1);
    // Where each Sim's head is (last frame's).
    let bone = |skel: &Skeleton, name: &str| skel.rig.bones.iter().position(|b| b.name == name).and_then(|i| skel.joints.get(i).copied());
    heads.clear();
    for (e, skel, ..) in &sims {
        if let Some(h) = bone(skel, "b__Head__").and_then(|j| joints.get(j).ok()) {
            heads.insert(e, h.1.translation());
        }
    }
    let mut sims = sims;
    let mut todo = Vec::new();
    for (e, skel, queue, partner, gaze) in sims.iter_mut() {
        // Who they're talking with: their own social, or the Sim who started one with them.
        let talking = queue.and_then(|q| q.0.front()).and_then(|a| match (&a.kind, a.phase) {
            (ActionKind::Social { target, .. }, Phase::Running(_)) => Some(*target),
            _ => None,
        });
        let target = talking.or(partner.map(|p| p.0)).and_then(|t| heads.get(&t).copied());
        let Some(gaze) = gaze else {
            commands.entity(e).insert(Gaze::default());
            continue;
        };
        let (Some(head), Some(neck)) = (bone(skel, "b__Head__"), bone(skel, "b__Neck__")) else { continue };
        todo.push((head, neck, bone(skel, "b__NoseTip__"), target, gaze));
    }
    for (head, neck, nose, target, mut gaze) in todo {
        let (Ok((_, hg)), Ok((_, ng))) = (joints.get(head), joints.get(neck)) else { continue };
        let (hg, ng) = (*hg, *ng);
        // The face's way (head to nose tip), level, from last frame's pose less the turn given.
        let nose = nose.and_then(|j| joints.get(j).ok()).map(|(_, g)| g.translation());
        let face = nose.map(|n| n - hg.translation()).unwrap_or(Vec3::Z);
        let face_yaw = face.x.atan2(face.z) - gaze.yaw;
        // Where they'd look: the other's face, within reach.
        let (want_yaw, want_pitch) = match target {
            Some(t) => {
                let d = t - hg.translation();
                let flat = Vec2::new(d.x, d.z).length().max(0.01);
                let mut yaw = d.x.atan2(d.z) - face_yaw;
                yaw = (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                // (Nobody cranes right round: past the reach, the head stays ahead.)
                if yaw.abs() > MAX_YAW * 1.6 {
                    (0.0, 0.0)
                } else {
                    (yaw.clamp(-MAX_YAW, MAX_YAW), (d.y / flat).atan().clamp(-MAX_PITCH, MAX_PITCH))
                }
            }
            None => (0.0, 0.0),
        };
        if target.is_some() {
            trace!("look-at: want yaw {want_yaw:.2} pitch {want_pitch:.2}, now {:.2}/{:.2}", gaze.yaw, gaze.pitch);
        }
        let k = (EASE * dt).min(1.0);
        gaze.yaw += (want_yaw - gaze.yaw) * k;
        gaze.pitch += (want_pitch - gaze.pitch) * k;
        if gaze.yaw.abs() < 1e-3 && gaze.pitch.abs() < 1e-3 {
            continue;
        }
        // The turn in the world (about the vertical, then tipping up or down), carried into the
        // neck's frame and applied to the head on top of the animation.
        let right = Quat::from_rotation_y(face_yaw + gaze.yaw) * Vec3::X;
        let turn = Quat::from_axis_angle(right, -gaze.pitch) * Quat::from_rotation_y(gaze.yaw);
        let parent = ng.compute_transform().rotation;
        let local = parent.inverse() * turn * parent;
        if let Ok((mut tf, _)) = joints.get_mut(head) {
            tf.rotation = local * tf.rotation;
        }
    }
}
