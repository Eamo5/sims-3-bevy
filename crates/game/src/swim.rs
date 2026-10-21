//! Swimming: from a pool's ladder a Sim gets into the water (or dives in off the diving board,
//! with one of the game's dives) and swims about the pool (the game's swim cycle, head and
//! shoulders above the water) until the swim is over, then climbs back out. The pool's water
//! clicked, it's the game's "Swim" ("Extreme Swim" for a Daredevil): in by the pool's ladder and
//! over to there, to tread water (`a2o_swim_idle_x`); clicked again while they swim, "Swim Here".

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
        app.add_systems(Update, (swim, pyjamas).after(crate::save::apply_loaded_game).run_if(in_state(PlayMode::Live)));
    }
}

/// To bed in pyjamas, a workout or a jog in athletic wear, and dressed again after; off to work in
/// the career's uniform (still worn home, until it's time for something else); out in the cold
/// (below 50°F, the game's `kTemperatureToConsiderCold`) in outerwear, and out of it indoors.
#[allow(clippy::type_complexity)]
fn pyjamas(
    mut commands: Commands,
    sims: Query<
        (
            Entity,
            &Sim,
            &ActionQueue,
            Option<&crate::simbody::Wearing>,
            Option<&crate::careers::Job>,
            Has<crate::simbody::ChangedInto>,
            &Transform,
            (Has<ActionClip>, Has<crate::dressing::ClothesSpin>, &InheritedVisibility, Option<&crate::nav::PathFollow>, Option<&crate::nav::Floor>),
        ),
        With<crate::sim::HouseholdMember>,
    >,
    beds: Query<&GameObject>,
    cas: Option<Res<crate::simbody::CasData>>,
    (weather, building, clock): (Res<crate::weather::Weather>, Option<Res<crate::building::ActiveBuilding>>, Res<crate::clock::GameClock>),
) {
    use crate::simbody::OutfitKind;
    let cold = weather.temperature < COLD;
    for (e, sim, queue, wearing, job, chosen, tf, (acting, spinning, shown, path, floor)) in &sims {
        // (Not turning round on the stairs or in the lift: once they're off them, on a floor.)
        let changing_floor = path.is_some_and(|p| !p.done && p.waypoints.first().is_some_and(|w| w.climb.is_some() || floor.is_some_and(|f| f.0 != w.level)));
        if spinning || changing_floor {
            continue;
        }
        let uniform = job.and_then(|j| j.uniform(sim)).is_some_and(|u| cas.as_ref().is_some_and(|c| c.outfits.contains_key(u)));
        // (On their way to something with its own clothes, left as they are: they change there,
        // and stay changed for it.)
        let on_the_way = queue.0.front().is_some_and(|a| match (&a.kind, a.phase) {
            (ActionKind::Object { target, def }, Phase::Start | Phase::Routing) => {
                beds.get(*target).ok().and_then(|o| interactions_for(o.kind).get(*def).and_then(|d| outfit_for_use(o.kind, d))).is_some()
            }
            _ => false,
        });
        if on_the_way {
            continue;
        }
        let want = queue.0.front().and_then(|a| match (&a.kind, a.phase) {
            (ActionKind::GoToWork, _) if uniform => Some(OutfitKind::Career),
            (ActionKind::Jog { .. }, Phase::Running(_)) => Some(OutfitKind::Athletic),
            (ActionKind::Object { target, def }, Phase::Running(_)) => {
                let o = beds.get(*target).ok()?;
                outfit_for_use(o.kind, interactions_for(o.kind).get(*def)?)
            }
            _ => None,
        });
        // (Babies and toddlers are carried in and out as they are.)
        let want = want.or_else(|| (cold && !sim.age.is_little() && !crate::weather::sheltered(building.as_deref(), tf.translation)).then_some(OutfitKind::Outerwear));
        let has = wearing.map(|w| w.0);
        // (Out of the pool, swimwear's kept on until there's something else to do; the uniform
        // stays on after work, and an outfit chosen at the dresser until it's time for another.)
        let keep = want.is_none() && (chosen || (has == Some(OutfitKind::Career) && uniform) || (has == Some(OutfitKind::Swimwear) && queue.0.is_empty()));
        if want == has || keep {
            continue;
        }
        // Out of the bath at night, into pyjamas (the game's `GettingOutOfBath`: within ten hours
        // of six in the morning, at home), kept on until it's time for something else.
        let night = (6.0 - clock.hour_f()).rem_euclid(24.0) < 10.0;
        let (to, keep_on) = if want.is_none() && has == Some(OutfitKind::Naked) && !sim.age.is_little() && night { (Some(OutfitKind::Sleepwear), true) } else { (want, false) };
        if to == has {
            continue;
        }
        // With a spin, when they're free to turn round; in the middle of something (climbing out
        // of the bath) they're dressed once it's done, and what's worn for it is put on as it is.
        if crate::dressing::spins(sim) && shown.get() && !acting {
            crate::dressing::start(&mut commands, e, to, keep_on);
        } else if acting && want.is_none() && shown.get() {
            continue;
        } else {
            crate::dressing::put_on(&mut commands.entity(e), to, keep_on);
        }
    }
}

/// What's worn to use something: pyjamas to sleep, nothing in the shower or the bath (the game's
/// `GoingToBathe`; not to clean them), swimwear for a swim or the hot tub (`GoingToSwim`),
/// workout clothes to work out.
pub fn outfit_for_use(kind: crate::interact::ObjectKind, d: &crate::interact::InteractionDef) -> Option<crate::simbody::OutfitKind> {
    use crate::interact::ObjectKind as K;
    use crate::simbody::OutfitKind;
    if matches!(kind, K::BedSingle | K::BedDouble) && d.name == "Sleep" {
        Some(OutfitKind::Sleepwear)
    } else if matches!(kind, K::Shower | K::Bathtub | K::ShowerTub) && d.on_object {
        Some(OutfitKind::Naked)
    } else if d.special == Special::Swim || kind == K::HotTub {
        Some(OutfitKind::Swimwear)
    } else if d.pose == crate::sim::Pose::Exercise {
        Some(OutfitKind::Athletic)
    } else {
        None
    }
}

/// Below this (°F) it's cold out: coats on.
const COLD: f32 = 50.0;

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
    /// Since when they've had no way out (the ladders gone): they tire, and drown.
    stranded: Option<f64>,
    /// Swimming to somewhere chosen ("Swim Here"), to tread water there: and treading now.
    hold: bool,
    treading: bool,
}

/// How the game names a swim over to somewhere in the pool, for those swimming in it.
pub const SWIM_HERE: &str = "Swim Here";
/// Swimming to a spot in the pool and treading water there.
const TREAD: &str = "a2o_swim_idle_x";
const SWIM_CYCLE: &str = "a_swim_cycle_x";

/// How long a swimmer with no way out lasts (game minutes), if their energy doesn't give out
/// first.
const STRANDED_MINUTES: f64 = 240.0;

/// The pool a ladder stands at (only that one, of all a lot's pools): its tiles' centres in the
/// world and its water height.
fn pool_at(world: &crate::loading::WorldInfo, at: Vec3) -> Option<(Vec<Vec2>, f32)> {
    pool_near(world, at.xz(), 2.5)
}

/// The pool with a tile this near the place (its tiles' centres and water height).
fn pool_near(world: &crate::loading::WorldInfo, at: Vec2, reach: f32) -> Option<(Vec<Vec2>, f32)> {
    world.buildings.values().filter(|b| !b.pool.is_empty()).find_map(|b| {
        let l = world.lots.get(b.lot as usize)?;
        let (s, c) = l.rotation.sin_cos();
        let centre = |(x, z): (i32, i32)| {
            let (x, z) = (x as f32 + 0.5, z as f32 + 0.5);
            Vec2::new(l.corner[0] + x * c + z * s, l.corner[2] - x * s + z * c)
        };
        let all: std::collections::HashSet<(i32, i32)> = b.pool.iter().map(|f| (f.x as i32, f.z as i32)).collect();
        let start = all.iter().copied().map(|t| (t, centre(t).distance(at))).filter(|(_, d)| *d < reach).min_by(|a, b| a.1.total_cmp(&b.1))?.0;
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

/// A place in a pool's water: the way into that pool (its nearest ladder, or else its diving
/// board) and its swim's place in the ladder's menu.
pub fn ladder_for(world: &crate::loading::WorldInfo, at: Vec2, objects: impl Iterator<Item = (Entity, crate::interact::ObjectKind, Vec3)>) -> Option<(Entity, usize)> {
    use crate::interact::ObjectKind;
    let (tiles, _) = pool_near(world, at, 0.72)?;
    let by = |p: Vec3| tiles.iter().map(|t| t.distance(p.xz())).fold(f32::MAX, f32::min);
    let (e, kind, _) = objects
        .filter(|(_, k, p)| matches!(k, ObjectKind::PoolLadder | ObjectKind::DivingBoard) && by(*p) < 2.5)
        .min_by(|a, b| (a.1 != ObjectKind::PoolLadder, a.2.xz().distance(at)).partial_cmp(&(b.1 != ObjectKind::PoolLadder, b.2.xz().distance(at))).unwrap_or(std::cmp::Ordering::Equal))?;
    let def = interactions_for(kind).iter().position(|d| d.special == Special::Swim)?;
    Some((e, def))
}

/// The tiles of the pool by a ladder (their centres in the world), and its water's height.
pub fn tiles_near(world: &crate::loading::WorldInfo, at: Vec3) -> Vec<Vec2> {
    pool_at(world, at).map(|p| p.0).unwrap_or_default()
}
pub fn water_near(world: &crate::loading::WorldInfo, at: Vec3) -> Option<f32> {
    pool_at(world, at).map(|p| p.1)
}

/// Where a ray (the pointer's) meets a pool's water, as it's seen: its surface, in the pool.
pub fn water_hit(world: &crate::loading::WorldInfo, ray: Ray3d) -> Option<Vec2> {
    if ray.direction.y >= -1e-4 {
        return None;
    }
    let mut levels: Vec<f32> = world.buildings.values().filter(|b| !b.pool.is_empty()).filter_map(|b| b.levels.first().map(|l| l - 0.22)).collect();
    levels.sort_by(|a, b| b.total_cmp(a));
    levels.into_iter().find_map(|y| {
        let t = (y - ray.origin.y) / ray.direction.y;
        let p = (ray.origin + *ray.direction * t).xz();
        (t > 0.0 && pool_near(world, p, 0.72).is_some()).then_some(p)
    })
}

/// "Swim Here": a swimmer off to somewhere else in their pool, to tread water there.
pub fn swim_here(w: &mut World, e: Entity, to: Vec2) {
    let Some(mut s) = w.get_mut::<Swimming>(e) else { return };
    if !s.tiles.iter().any(|t| t.distance(to) < 0.72) {
        return;
    }
    s.target = to;
    s.hold = true;
    if std::mem::replace(&mut s.treading, false) {
        w.entity_mut(e).insert(ActionClip::new(None, &[SWIM_CYCLE]));
    }
}

#[allow(clippy::type_complexity)]
fn swim(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    world: Res<CurrentWorld>,
    mut sims: Query<
        (Entity, &Sim, &mut ActionQueue, &mut Transform, Option<&mut Swimming>, &crate::anim::ClipPlayer, &mut crate::sim::Motives, Option<&crate::simbody::Wearing>),
        Without<crate::death::Dying>,
    >,
    ladders: Query<(&GameObject, &Transform), Without<Sim>>,
    clock: Res<crate::clock::GameClock>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let mut rng = rand::rng();
    for (e, sim, mut queue, mut tf, swimming, player, mut motives, wearing) in &mut sims {
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
                // (Over to where they were sent, if anywhere in this pool; else about it.)
                let to = queue.0.front().and_then(|a| a.swim_to).filter(|p| tiles.iter().any(|t| t.distance(*p) < 0.72));
                let target = to.unwrap_or_else(|| tiles[rng.random_range(0..tiles.len())]);
                commands.entity(e).insert((Swimming { tiles, water_y, target, exit, stranded: None, hold: to.is_some(), treading: false }, ActionClip::new(None, &[SWIM_CYCLE])));
                // (In swimwear for the swim: spun into at the ladder, or put on now.)
                if wearing.map(|w| w.0) != Some(crate::simbody::OutfitKind::Swimwear) {
                    commands.entity(e).insert((crate::simbody::Wearing(crate::simbody::OutfitKind::Swimwear), crate::aging::NeedsNewBody));
                }
            }
            (Some(_), Some(mut s)) => {
                let here = tf.translation.xz();
                if here.distance(s.target) < 0.6 {
                    // (There: treading water, till sent on.)
                    if s.hold {
                        if !s.treading {
                            s.treading = true;
                            commands.entity(e).insert(ActionClip::new(None, &[TREAD]));
                        }
                        continue;
                    }
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
            (None, Some(mut s)) => {
                // With the ladders gone there's no way out: they swim on, tiring, until they drown.
                let way_out = ladders.iter().any(|(o, t)| {
                    matches!(o.kind, crate::interact::ObjectKind::PoolLadder | crate::interact::ObjectKind::DivingBoard)
                        && s.tiles.iter().any(|p| p.distance(t.translation.xz()) < 2.5)
                });
                if !way_out {
                    let since = *s.stranded.get_or_insert_with(|| {
                        notes.push(format!("{} can't get out of the pool!", sim.first));
                        clock.minutes
                    });
                    queue.0.clear();
                    motives.add(crate::sim::ENERGY, -60.0 * delta.0 / 60.0);
                    motives.add(crate::sim::FUN, -20.0 * delta.0 / 60.0);
                    if clock.minutes - since > STRANDED_MINUTES || motives.0[crate::sim::ENERGY] <= -95.0 {
                        commands.entity(e).insert(crate::death::Dying::drowned(s.exit));
                        continue;
                    }
                    let here = tf.translation.xz();
                    if here.distance(s.target) < 0.6 {
                        s.target = s.tiles[rng.random_range(0..s.tiles.len())];
                    }
                    let to = s.target - here;
                    let step = (SWIM_SPEED * 0.6 * delta.0).min(to.length());
                    if to.length_squared() > 1e-6 {
                        let dir = to.normalize();
                        let p = here + dir * step;
                        tf.rotation = tf.rotation.slerp(Quat::from_rotation_y(dir.x.atan2(dir.y)), (delta.0 * 2.0).min(1.0));
                        tf.translation = Vec3::new(p.x, s.water_y - SWIM_DROP, p.y);
                    }
                    commands.entity(e).insert(ActionClip::new(None, &["a_swim_cycle_x"]));
                    continue;
                }
                // Out of the water by the ladder (still in swimwear, until there's something else
                // to do).
                tf.translation = s.exit;
                commands.entity(e).remove::<Swimming>().insert(ActionClip::new(Some("a2o_ladder_climbUp_L_x"), &[]));
            }
            (None, None) => {}
        }
    }
}
