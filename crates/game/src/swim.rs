//! Swimming: from a pool's ladder a Sim gets into the water (or dives in off the diving board,
//! with one of the game's dives) and swims about the pool (the game's swim cycle, head and
//! shoulders above the water) until the swim is over, then climbs back out.

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

/// To bed in pyjamas, a workout in athletic wear, and dressed again after; off to work in
/// the career's uniform (still worn home, until it's time for something else).
#[allow(clippy::type_complexity)]
fn pyjamas(
    mut commands: Commands,
    sims: Query<(Entity, &Sim, &ActionQueue, Option<&crate::simbody::Wearing>, Option<&crate::careers::Job>, Has<crate::simbody::ChangedInto>), With<crate::sim::HouseholdMember>>,
    beds: Query<&GameObject>,
    cas: Option<Res<crate::simbody::CasData>>,
) {
    use crate::simbody::{OutfitKind, Wearing};
    for (e, sim, queue, wearing, job, chosen) in &sims {
        let uniform = job.and_then(|j| j.uniform(sim)).is_some_and(|u| cas.as_ref().is_some_and(|c| c.outfits.contains_key(u)));
        let want = queue.0.front().and_then(|a| match (&a.kind, a.phase) {
            (ActionKind::GoToWork, _) if uniform => Some(OutfitKind::Career),
            (ActionKind::Object { target, def }, Phase::Running(_)) => {
                let o = beds.get(*target).ok()?;
                let d = interactions_for(o.kind).get(*def)?;
                if matches!(o.kind, crate::interact::ObjectKind::BedSingle | crate::interact::ObjectKind::BedDouble) && d.name == "Sleep" {
                    Some(OutfitKind::Sleepwear)
                } else if d.pose == crate::sim::Pose::Exercise {
                    Some(OutfitKind::Athletic)
                } else {
                    None
                }
            }
            _ => None,
        });
        let has = wearing.map(|w| w.0);
        // (Swimwear is the swim's to change; the uniform stays on after work, and an outfit
        // chosen at the dresser until it's time for another.)
        let keep = want.is_none() && (chosen || (has == Some(OutfitKind::Career) && uniform));
        if has == Some(OutfitKind::Swimwear) || want == has || keep {
            continue;
        }
        match want {
            Some(k) => {
                commands.entity(e).remove::<crate::simbody::ChangedInto>().insert((Wearing(k), crate::aging::NeedsNewBody));
            }
            None => {
                commands.entity(e).remove::<Wearing>().insert(crate::aging::NeedsNewBody);
            }
        }
    }
}

/// How far below the water a swimmer is held: the game's swim cycle swims level with its root,
/// the head a little above it, so this keeps the head at the surface and the body just under.
const SWIM_DROP: f32 = 0.12;
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

/// The pool a ladder stands at (only that one, of all a lot's pools): its tiles' centres in the
/// world and its water height.
fn pool_at(world: &crate::loading::WorldInfo, at: Vec3) -> Option<(Vec<Vec2>, f32)> {
    world.buildings.values().filter(|b| !b.pool.is_empty()).find_map(|b| {
        let l = world.lots.get(b.lot as usize)?;
        let (s, c) = l.rotation.sin_cos();
        let centre = |(x, z): (i32, i32)| {
            let (x, z) = (x as f32 + 0.5, z as f32 + 0.5);
            Vec2::new(l.corner[0] + x * c + z * s, l.corner[2] - x * s + z * c)
        };
        let all: std::collections::HashSet<(i32, i32)> = b.pool.iter().map(|f| (f.x as i32, f.z as i32)).collect();
        let start = all.iter().copied().map(|t| (t, centre(t).distance(at.xz()))).filter(|(_, d)| *d < 2.5).min_by(|a, b| a.1.total_cmp(&b.1))?.0;
        // The tiles joined to the ladder's.
        let mut pool = std::collections::HashSet::from([start]);
        let mut open = vec![start];
        while let Some((x, z)) = open.pop() {
            for n in [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)] {
                if all.contains(&n) && pool.insert(n) {
                    open.push(n);
                }
            }
        }
        Some((pool.into_iter().map(centre).collect(), b.levels[0] - 0.22))
    })
}

#[allow(clippy::type_complexity)]
fn swim(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    world: Res<CurrentWorld>,
    mut sims: Query<(Entity, &ActionQueue, &mut Transform, Option<&mut Swimming>, &crate::anim::ClipPlayer), With<Sim>>,
    ladders: Query<(&GameObject, &Transform), Without<Sim>>,
) {
    let mut rng = rand::rng();
    for (e, queue, mut tf, swimming, player) in &mut sims {
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
                let Ok((lo, ltf)) = ladders.get(ladder) else { continue };
                // (Off the diving board: in once the dive is done.)
                let dived = player.name.contains("divingBoard_")
                    && !player.name.contains("getIn")
                    && player.clip.as_ref().is_some_and(|c| player.time >= c.duration * 0.8);
                if lo.kind == crate::interact::ObjectKind::DivingBoard && !dived {
                    continue;
                }
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
