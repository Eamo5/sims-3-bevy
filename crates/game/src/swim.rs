//! Swimming: from a pool's ladder a Sim gets into the water and swims about the pool (the
//! game's swim cycle, head and shoulders above the water) until the swim is over, then climbs
//! back out by the ladder.

use bevy::prelude::*;
use rand::Rng;

use crate::anim::ActionClip;
use crate::interact::{ActionKind, ActionQueue, GameObject, Phase, Special, interactions_for};
use crate::loading::CurrentWorld;
use crate::sim::Sim;
use crate::PlayMode;

pub struct SwimPlugin;

impl Plugin for SwimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (swim, pyjamas).run_if(in_state(PlayMode::Live)));
    }
}

/// To bed in pyjamas, and dressed again on getting up.
#[allow(clippy::type_complexity)]
fn pyjamas(
    mut commands: Commands,
    sims: Query<(Entity, &ActionQueue, Option<&crate::simbody::Wearing>), (With<Sim>, With<crate::sim::HouseholdMember>)>,
    beds: Query<&GameObject>,
) {
    use crate::simbody::{OutfitKind, Wearing};
    for (e, queue, wearing) in &sims {
        let asleep = queue.0.front().is_some_and(|a| match (&a.kind, a.phase) {
            (ActionKind::Object { target, def }, Phase::Running(_)) => beds.get(*target).is_ok_and(|o| {
                matches!(o.kind, crate::interact::ObjectKind::BedSingle | crate::interact::ObjectKind::BedDouble)
                    && interactions_for(o.kind).get(*def).is_some_and(|d| d.name == "Sleep")
            }),
            _ => false,
        });
        match (asleep, wearing.map(|w| w.0)) {
            (true, None) => {
                commands.entity(e).insert((Wearing(OutfitKind::Sleepwear), crate::aging::NeedsNewBody));
            }
            (false, Some(OutfitKind::Sleepwear)) => {
                commands.entity(e).remove::<Wearing>().insert(crate::aging::NeedsNewBody);
            }
            _ => {}
        }
    }
}

/// How far below the water a swimmer's feet are (their head and shoulders stay above it).
const SWIM_DROP: f32 = 1.25;
/// Swimming speed (metres per game minute, i.e. per second at normal speed).
const SWIM_SPEED: f32 = 0.9;

/// In the water: the pool's tiles (world centres), its water height, where the swimmer is
/// heading and where they got in.
#[derive(Component)]
pub struct Swimming {
    tiles: Vec<Vec2>,
    water_y: f32,
    target: Vec2,
    exit: Vec3,
}

/// The pool a ladder stands at: its tiles' centres in the world and its water height.
fn pool_at(world: &crate::loading::WorldInfo, at: Vec3) -> Option<(Vec<Vec2>, f32)> {
    world.buildings.values().filter(|b| !b.pool.is_empty()).find_map(|b| {
        let l = world.lots.get(b.lot as usize)?;
        let (s, c) = l.rotation.sin_cos();
        let tiles: Vec<Vec2> = b
            .pool
            .iter()
            .map(|f| {
                let (x, z) = (f.x as f32 + 0.5, f.z as f32 + 0.5);
                Vec2::new(l.corner[0] + x * c + z * s, l.corner[2] - x * s + z * c)
            })
            .collect();
        tiles.iter().any(|t| t.distance(at.xz()) < 2.5).then(|| (tiles, b.levels[0] - 0.22))
    })
}

#[allow(clippy::type_complexity)]
fn swim(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    world: Res<CurrentWorld>,
    mut sims: Query<(Entity, &ActionQueue, &mut Transform, Option<&mut Swimming>), With<Sim>>,
    ladders: Query<(&GameObject, &Transform), Without<Sim>>,
) {
    let mut rng = rand::rng();
    for (e, queue, mut tf, swimming) in &mut sims {
        // Swimming now: the front action is a swim, under way.
        let swim_from = queue.0.front().and_then(|a| match (&a.kind, a.phase) {
            (ActionKind::Object { target, def }, Phase::Running(_)) => {
                let (o, _) = ladders.get(*target).ok()?;
                (interactions_for(o.kind).get(*def).is_some_and(|d| d.special == Special::Swim)).then_some(*target)
            }
            _ => None,
        });
        match (swim_from, swimming) {
            (Some(ladder), None) => {
                let Ok((_, ltf)) = ladders.get(ladder) else { continue };
                let Some((tiles, water_y)) = pool_at(&world.data, ltf.translation) else { continue };
                let Some(&first) = tiles.iter().min_by(|a, b| a.distance(ltf.translation.xz()).total_cmp(&b.distance(ltf.translation.xz()))) else { continue };
                let exit = tf.translation;
                tf.translation = Vec3::new(first.x, water_y - SWIM_DROP, first.y);
                let target = tiles[rng.random_range(0..tiles.len())];
                // (Into swimwear for the swim.)
                commands.entity(e).insert((
                    Swimming { tiles, water_y, target, exit },
                    ActionClip::new(None, &["a_swim_cycle_x"]),
                    crate::simbody::Wearing(crate::simbody::OutfitKind::Swimwear),
                    crate::aging::NeedsNewBody,
                ));
            }
            (Some(_), Some(mut s)) => {
                let here = tf.translation.xz();
                if here.distance(s.target) < 0.6 {
                    s.target = s.tiles[rng.random_range(0..s.tiles.len())];
                }
                let to = s.target - here;
                let step = (SWIM_SPEED * delta.0).min(to.length());
                if to.length_squared() > 1e-6 {
                    let dir = to.normalize();
                    let p = here + dir * step;
                    // (Turning towards where they're heading.)
                    let want = Quat::from_rotation_y(dir.x.atan2(dir.y));
                    tf.rotation = tf.rotation.slerp(want, (delta.0 * 2.0).min(1.0));
                    tf.translation = Vec3::new(p.x, s.water_y - SWIM_DROP, p.y);
                }
            }
            (None, Some(s)) => {
                // Out of the water by the ladder.
                tf.translation = s.exit;
                commands
                    .entity(e)
                    .remove::<(Swimming, crate::simbody::Wearing)>()
                    .insert((ActionClip::new(Some("a2o_ladder_climbUp_L_x"), &[]), crate::aging::NeedsNewBody));
            }
            (None, None) => {}
        }
    }
}
