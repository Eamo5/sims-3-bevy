//! Kleptomaniacs, as the game's `TraitFunctions.SwipeSomething`: from seven in the evening till
//! six in the morning (`kKleptomaniacTraitSwipeStartTime`, `kKleptomaniacTraitSwipeEndTime`), on
//! any lot but their own, a Kleptomaniac can "Swipe Something" from the ground's menu: a thing the
//! catalogue lets be stolen, within six tiles (`kKleptomaniacTraitSwipeDistance`) on that floor and
//! in that room, as long as no one else is in the room. Three a day
//! (`kKleptomaniacTraitObjectsStealPerDay`). They walk over and pocket it (`a_trait_kleptomaniac_x`)
//! and it's theirs, in their inventory, to put down at home or sell; the lot has its own back when
//! it's next opened. Their free will has them do it when they can (`SwipeSomethingAutonomous`).
//! Anyone come into the room meanwhile likes them less for it (`kKleptomaniacLTRPenalty`) and
//! knows them for a Kleptomaniac. And a day of work or school brings something home one time in
//! five (`kKleptomaniacTraitChanceToStealObject`): worth no more than §350 a career level
//! (`kKleptomaniacTraitSimoleonsPerCareerLevel`), or §500 from school.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::interact::GameObject;
use crate::life::Trait;
use crate::nav::Floor;
use crate::seasonal::{Outdoor, OutdoorEvent};
use crate::sim::{HouseholdMember, Sim};
use crate::visit::{LotObject, OnLot};

pub struct KleptoPlugin;

impl Plugin for KleptoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Swipeable>().add_systems(Update, (swipeable, swiped, brought_home, klepto_test).run_if(in_state(PlayMode::Live)));
    }
}

/// How the game names it (`Gameplay/ActorSystems/TraitFunctions/SwipeSomething`).
pub const SWIPE: &str = "Swipe Something";

/// The game's tuning (`TraitTuning`).
const START: f32 = 19.0;
const END: f32 = 6.0;
const REACH: f32 = 6.0;
const PER_DAY: u8 = 3;
const SEEN_LIKING: f32 = -30.0;
const DAY_CHANCE: f64 = 0.2;
pub const PER_CAREER_LEVEL: i32 = 350;
pub const FROM_SCHOOL: i32 = 500;
/// How keen their free will is on it (above a Neurotic's freak-out, a performer's TaDa!).
pub const SWIPE_SCORE: f32 = 10.0;

/// Whether it's the hour for it.
pub fn swipe_hours(hour: f32) -> bool {
    hour >= START || hour < END
}

/// How many they've swiped today (the game's `KleptomaniacObjectsStolenToday`).
#[derive(Component, Default)]
pub struct SwipedToday {
    day: u32,
    count: u8,
}

impl SwipedToday {
    fn today(&self, day: u32) -> u8 {
        if self.day == day { self.count } else { 0 }
    }
}

/// A thing on a lot that might be swiped.
pub struct Thing {
    pub e: Entity,
    pub at: Vec3,
    pub level: u8,
    pub room: Option<u16>,
    pub stealable: bool,
}

/// A Sim about a lot, where (for who's in the room).
pub struct Someone {
    pub e: Entity,
    pub level: u8,
    pub room: Option<u16>,
}

/// The things they might swipe from `from`: the stealable on that floor, in that room, within
/// reach.
pub fn within_reach(things: &[Thing], from: Vec3, level: u8, room: Option<u16>) -> Vec<&Thing> {
    things.iter().filter(|t| t.stealable && t.level == level && t.room == room && t.at.xz().distance(from.xz()) <= REACH).collect()
}

/// One of the things within reach, at random (as the game's).
pub fn pick(things: &[Thing], from: Vec3, level: u8, room: Option<u16>) -> Option<&Thing> {
    within_reach(things, from, level, room).choose(&mut rand::rng()).copied()
}

/// Whether anyone else is in the room.
pub fn watched(me: Entity, level: u8, room: Option<u16>, about: &[Someone]) -> bool {
    about.iter().any(|s| s.e != me && s.level == level && s.room == room)
}

/// Whether a Kleptomaniac may swipe something now: at the hour for it, on a lot not their own
/// (`away`), and not three things already today.
pub fn ready(sim: &Sim, swiped: Option<&SwipedToday>, hour: f32, day: u32, away: bool) -> bool {
    sim.traits.contains(&Trait::Kleptomaniac) && away && swipe_hours(hour) && swiped.map_or(0, |s| s.today(day)) < PER_DAY
}

/// Where each Kleptomaniac could swipe something now, of their own accord (for their free will):
/// the thing's place and floor.
#[derive(Resource, Default)]
pub struct Swipeable(pub HashMap<Entity, (Vec2, u8)>);

/// The things of the visited lot, and who's about it, as `within_reach` and `watched` need them.
#[allow(clippy::type_complexity)]
pub fn lot_scene(
    data: &s3bake::GameDataBaked,
    building: Option<&crate::building::ActiveBuilding>,
    objects: &Query<(Entity, &GameObject, &GlobalTransform, &LotObject, Option<&Floor>, Option<&crate::interact::UsedBy>)>,
    sims: &Query<(Entity, &Transform, Option<&Floor>, Option<&OnLot>), With<Sim>>,
    lot: usize,
) -> (Vec<Thing>, Vec<Someone>) {
    let room = |level: u8, p: Vec3| building.and_then(|b| b.room_id(level, p));
    use crate::interact::ObjectKind as K;
    // (Not what's lying about or growing, spilled or served: the game's spawned finds and plants
    // aren't for swiping.)
    let things = objects
        .iter()
        .filter(|o| o.3.0 == lot && o.5.is_none_or(|u| u.0.is_none()) && o.1.objd.2 != 0)
        .filter(|o| !matches!(o.1.kind, K::Collectible | K::FishingSpot | K::Butterfly | K::Beetle | K::Puddle | K::Meal | K::DirtyDishes | K::GardenPlant))
        .map(|(e, o, tf, _, floor, _)| {
            let level = floor.map_or(1, |f| f.0);
            Thing { e, at: tf.translation(), level, room: room(level, tf.translation()), stealable: data.stealable.binary_search(&o.objd.2).is_ok() }
        })
        .collect();
    let about = sims
        .iter()
        .filter(|s| s.3.is_some_and(|l| l.0 == lot))
        .map(|(e, tf, floor, _)| {
            let level = floor.map_or(1, |f| f.0);
            Someone { e, level, room: room(level, tf.translation) }
        })
        .collect();
    (things, about)
}

/// Each Kleptomaniac of the household out on another lot: what they could swipe now, unwatched.
#[allow(clippy::type_complexity)]
fn swipeable(
    clock: Res<crate::clock::GameClock>,
    mut out: ResMut<Swipeable>,
    mut every: Local<f32>,
    time: Res<Time>,
    ui: Option<Res<crate::icons::GameUi>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    klepto: Query<(Entity, &Sim, &Transform, Option<&Floor>, &OnLot, Option<&SwipedToday>), With<HouseholdMember>>,
    objects: Query<(Entity, &GameObject, &GlobalTransform, &LotObject, Option<&Floor>, Option<&crate::interact::UsedBy>)>,
    sims: Query<(Entity, &Transform, Option<&Floor>, Option<&OnLot>), With<Sim>>,
) {
    *every -= time.delta_secs();
    if *every > 0.0 {
        return;
    }
    *every = 1.0;
    out.0.clear();
    let Some(ui) = ui else { return };
    for (me, sim, tf, floor, lot, swiped) in &klepto {
        if !ready(sim, swiped, clock.hour_f(), clock.day(), true) {
            continue;
        }
        let (things, about) = lot_scene(&ui.data, building.as_deref(), &objects, &sims, lot.0);
        let level = floor.map_or(1, |f| f.0);
        let room = building.as_deref().and_then(|b| b.room_id(level, tf.translation));
        // (KLEPTO=1: why not, now and then.)
        if std::env::var("KLEPTO").is_ok() && rand::rng().random_bool(0.1) {
            info!(
                "klepto test: {} in room {room:?} on floor {level}: {} things on the lot ({} stealable), {} within reach, {} others in the room",
                sim.first,
                things.len(),
                things.iter().filter(|t| t.stealable).count(),
                within_reach(&things, tf.translation, level, room).len(),
                about.iter().filter(|s| s.e != me && s.level == level && s.room == room).count()
            );
        }
        if watched(me, level, room, &about) {
            continue;
        }
        if let Some(t) = pick(&things, tf.translation, level, room) {
            out.0.insert(me, (t.at.xz(), t.level));
        }
    }
}

/// The action: over to the thing, and into a pocket with it.
pub fn swipe_action(at: Vec2, level: u8, autonomous: bool) -> crate::interact::Action {
    crate::interact::Action::new(SWIPE, crate::interact::ActionKind::Outdoor { at, level, what: Outdoor::Swipe }, autonomous)
}

/// A swipe done: the nearest stealable thing out of the lot and into their inventory; whoever's
/// come into the room thinks less of them.
#[allow(clippy::type_complexity)]
fn swiped(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    baked: Res<crate::baked::Baked>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    done: Query<(Entity, &OutdoorEvent, &Sim, Option<&Floor>, Option<&OnLot>, Option<&SwipedToday>)>,
    objects: Query<(Entity, &GameObject, &GlobalTransform, &LotObject, Option<&Floor>, Option<&crate::interact::UsedBy>)>,
    sims: Query<(Entity, &Transform, Option<&Floor>, Option<&OnLot>), With<Sim>>,
    mut rels: Query<&mut crate::social::Relationships>,
    mut learned: MessageWriter<crate::learning::Learned>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut swipeable: ResMut<Swipeable>,
) {
    for (me, ev, sim, floor, lot, swiped) in &done {
        let OutdoorEvent::Done { what: Outdoor::Swipe, at, .. } = *ev else { continue };
        commands.entity(me).remove::<OutdoorEvent>();
        swipeable.0.remove(&me);
        // (Three a day, however many were set about.)
        let day = clock.day();
        if swiped.map_or(0, |s| s.today(day)) >= PER_DAY {
            continue;
        }
        let (Some(ui), Some(lot)) = (ui.as_ref(), lot) else { continue };
        let (things, about) = lot_scene(&ui.data, building.as_deref(), &objects, &sims, lot.0);
        let level = floor.map_or(1, |f| f.0);
        let room = building.as_deref().and_then(|b| b.room_id(level, at));
        let Some(t) = within_reach(&things, at, level, room).into_iter().min_by(|a, b| a.at.distance(at).total_cmp(&b.at.distance(at))) else { continue };
        let Ok(o) = objects.get(t.e) else { continue };
        let Some(c) = baked.0.catalog.iter().find(|c| c.objd == o.1.objd) else { continue };
        let name = if c.name.is_empty() { o.1.name.clone() } else { c.name.clone() };
        commands.entity(t.e).try_despawn();
        crate::inventory::give(&mut commands, me, crate::inventory::ItemKind::Object, c.instance_name.clone(), name.clone(), 0, c.price.max(0) as i64, 1);
        let count = swiped.map_or(0, |s| s.today(day)) + 1;
        commands.entity(me).insert(SwipedToday { day, count });
        info!("klepto: {} swiped the {name} ({} today)", sim.first, count);
        // ("{0.SimName} just swiped someone's {1.String}. It is now in your family inventory.":
        // the first of the day.)
        if count == 1 {
            notes.push(format!("{} just swiped someone's {name}. It is now in {} inventory.", sim.full_name(), if sim.female { "her" } else { "his" }));
        }
        // Seen at it: liked less, and known for what they are.
        for s in about.iter().filter(|s| s.e != me && s.level == level && s.room == room) {
            for (a, b) in [(s.e, me), (me, s.e)] {
                if let Ok(mut r) = rels.get_mut(a) {
                    let r = r.entry(b);
                    r.friendship = (r.friendship + SEEN_LIKING).clamp(-100.0, 100.0);
                }
            }
            learned.write(crate::learning::Learned { learner: s.e, about: me, what: Some(Trait::Kleptomaniac), random: false, teaches: ["", ""] });
        }
    }
}

/// A day of work or school done: what a Kleptomaniac may bring home may be worth no more than
/// this.
#[derive(Component)]
pub struct DayDone(pub i32);

/// Home from work or school: one time in five, something of a Kleptomaniac's pocketed there.
fn brought_home(
    mut commands: Commands,
    ui: Option<Res<crate::icons::GameUi>>,
    baked: Res<crate::baked::Baked>,
    done: Query<(Entity, &DayDone, &Sim)>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let mut rng = rand::rng();
    for (me, d, sim) in &done {
        commands.entity(me).remove::<DayDone>();
        // (KLEPTO=1, for a test: always.)
        let chance = if std::env::var("KLEPTO").is_ok() { 1.0 } else { DAY_CHANCE };
        if !sim.traits.contains(&Trait::Kleptomaniac) || !rng.random_bool(chance) {
            continue;
        }
        let Some(ui) = ui.as_ref() else { continue };
        let worth: Vec<&s3bake::types::CatalogEntry> = baked.0.catalog.iter().filter(|c| c.price > 0 && c.price <= d.0 && ui.data.stealable.binary_search(&c.objd.2).is_ok()).collect();
        let Some(c) = worth.choose(&mut rng) else { continue };
        crate::inventory::give(&mut commands, me, crate::inventory::ItemKind::Object, c.instance_name.clone(), c.name.clone(), 0, c.price as i64, 1);
        let a = if c.name.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
        notes.push(format!("{} came home with {a} {} that isn't {}.", sim.first, c.name, if sim.female { "hers" } else { "his" }));
    }
}

/// KLEPTO=1 (tests): the selected Sim a Kleptomaniac, who swipes whatever they can as soon as
/// they can, and always brings something home from work or school.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn klepto_test(
    mut sel: Query<(Entity, &mut Sim, &mut crate::interact::ActionQueue, &Transform, Option<&OnLot>), With<crate::sim::Selected>>,
    swipeable: Res<Swipeable>,
    time: Res<Time>,
    mut walked: Local<bool>,
    ui: Option<Res<crate::icons::GameUi>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    objects: Query<(Entity, &GameObject, &GlobalTransform, &LotObject, Option<&Floor>, Option<&crate::interact::UsedBy>)>,
    sims: Query<(Entity, &Transform, Option<&Floor>, Option<&OnLot>), With<Sim>>,
) {
    if std::env::var("KLEPTO").is_err() || time.elapsed_secs() < 8.0 {
        return;
    }
    let Ok((me, mut sim, mut q, tf, lot)) = sel.single_mut() else { return };
    // (Out on a lot, over to the nearest thing to swipe.)
    if !*walked
        && q.0.is_empty()
        && let (Some(lot), Some(ui)) = (lot, ui.as_ref())
    {
        let (things, _) = lot_scene(&ui.data, building.as_deref(), &objects, &sims, lot.0);
        if let Some(t) = things.iter().filter(|t| t.stealable && t.level == 1).min_by(|a, b| a.at.distance(tf.translation).total_cmp(&b.at.distance(tf.translation))) {
            info!("klepto test: {} goes over to {:?} at {}", sim.first, objects.get(t.e).map(|o| o.1.name.clone()).ok(), t.at);
            q.push_player(crate::interact::Action::new("Go Here", crate::interact::ActionKind::GoHere(t.at.xz(), 1), false));
            *walked = true;
        }
    }
    if !sim.traits.contains(&Trait::Kleptomaniac) {
        sim.traits.push(Trait::Kleptomaniac);
        info!("klepto test: {} is a Kleptomaniac", sim.first);
    }
    if let Some((at, level)) = swipeable.0.get(&me)
        && !q.0.iter().any(|a| a.label == SWIPE)
    {
        info!("klepto test: {} swipes something at {at}", sim.first);
        q.0.clear();
        q.push_player(swipe_action(*at, *level, false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swipes_are_for_the_night_unwatched_and_near() {
        assert!(swipe_hours(19.0) && swipe_hours(2.0) && !swipe_hours(6.0) && !swipe_hours(12.0));
        let mut w = World::new();
        let ids: Vec<Entity> = (0..12).map(|_| w.spawn_empty().id()).collect();
        let e = |i: usize| ids[i];
        let things = [
            Thing { e: e(1), at: Vec3::new(3.0, 0.0, 0.0), level: 1, room: Some(2), stealable: true },
            Thing { e: e(2), at: Vec3::new(9.0, 0.0, 0.0), level: 1, room: Some(2), stealable: true },
            Thing { e: e(3), at: Vec3::new(1.0, 0.0, 0.0), level: 1, room: Some(2), stealable: false },
            Thing { e: e(4), at: Vec3::new(1.0, 0.0, 0.0), level: 1, room: Some(3), stealable: true },
        ];
        let near = within_reach(&things, Vec3::ZERO, 1, Some(2));
        assert_eq!(near.iter().map(|t| t.e).collect::<Vec<_>>(), vec![e(1)]);
        let about = [Someone { e: e(10), level: 1, room: Some(2) }, Someone { e: e(11), level: 1, room: None }];
        assert!(!watched(e(10), 1, Some(2), &about) && watched(e(11), 1, Some(2), &about) && watched(e(10), 1, None, &about));
        let mut rng = rand::rng();
        let mut sim = crate::sim::random_sim(&mut rng, "Test", Some(true), crate::sim::Age::Adult);
        sim.traits = vec![Trait::Kleptomaniac];
        assert!(ready(&sim, None, 22.0, 3, true) && !ready(&sim, None, 22.0, 3, false) && !ready(&sim, None, 13.0, 3, true));
        assert!(!ready(&sim, Some(&SwipedToday { day: 3, count: 3 }), 22.0, 3, true) && ready(&sim, Some(&SwipedToday { day: 2, count: 3 }), 22.0, 3, true));
    }
}
