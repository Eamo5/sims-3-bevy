//! Sims blink: every few seconds their upper eyelids (the rig's `b__LeftUpLid__` and
//! `b__RightUpLid__`) close over the eyes for a moment and the lower ones rise to meet them,
//! on top of whatever their animation is doing. Asleep, the eyes stay shut.

use bevy::prelude::*;
use rand::Rng;

use crate::simbody::Skeleton;

pub struct BlinkPlugin;

impl Plugin for BlinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            blink.after(crate::anim::drive_skeletons).run_if(in_state(crate::PlayMode::Live).or_else(in_state(crate::AppState::CreateHousehold))),
        );
    }
}

/// How long a blink takes (seconds), and how far the lids turn shut (radians).
const BLINK: f32 = 0.16;
const UPPER: f32 = 0.55;
const LOWER: f32 = 0.18;

#[derive(Component)]
struct Blinking {
    /// Seconds until the next blink, or into the current one (negative).
    t: f32,
}

#[allow(clippy::type_complexity)]
fn blink(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(Entity, &Skeleton, Option<&mut Blinking>, Option<&crate::sim::SimAnim>)>,
    mut joints: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    let held = std::env::var("BLINK_SHUT").is_ok();
    for (e, skel, blinking, anim) in &mut sims {
        let Some(mut b) = blinking else {
            commands.entity(e).insert(Blinking { t: rng.random_range(0.5..4.0) });
            continue;
        };
        b.t -= dt;
        if b.t < -BLINK {
            b.t = rng.random_range(2.0..6.0);
        }
        // How shut the eyes are: 0 open, 1 closed.
        let asleep = anim.is_some_and(|a| a.pose == crate::sim::Pose::Lie);
        let shut = if held || asleep {
            1.0
        } else if b.t < 0.0 {
            (std::f32::consts::PI * (-b.t / BLINK)).sin()
        } else {
            continue;
        };
        for (bone, angle) in [("b__LeftUpLid__", UPPER), ("b__RightUpLid__", UPPER), ("b__LeftLoLid__", -LOWER), ("b__RightLoLid__", -LOWER)] {
            let Some(i) = skel.rig.bones.iter().position(|x| x.name == bone) else { continue };
            if let Some(&j) = skel.joints.get(i)
                && let Ok(mut tf) = joints.get_mut(j)
            {
                tf.rotation *= Quat::from_rotation_x(angle * shut);
            }
        }
    }
}
