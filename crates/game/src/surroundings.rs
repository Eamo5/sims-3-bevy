//! Clean and dirty surroundings, as the game judges them: dirty dishes left about, food left
//! out until it spoils, and a trash can left full put the household's Sims nearby in Dirty
//! (and then Filthy) Surroundings. Clearing dishes away fills the nearest trash can, which
//! someone then has to empty.

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{GameObject, Notifications, ObjectKind};
use crate::life::{MoodletKind, Moodlets};
use crate::meals::Meal;
use crate::sim::{HouseholdMember, Sim};

pub struct SurroundingsPlugin;

impl Plugin for SurroundingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spoil_food, trash_cans, discard, wash_up, take_out_trash, read_somewhere, read_paper_somewhere, drop_unread_paper, watch_somewhere, stop_watching.before(crate::interact::run_actions), put_down_carried, surroundings).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// How long food keeps once served (game minutes).
const SPOIL_MINUTES: f64 = 8.0 * 60.0;
/// How many loads of scraps a trash can holds (a trash compactor, three times as many).
pub const TRASH_CAPACITY: u8 = 5;

pub fn trash_capacity(kind: ObjectKind) -> u8 {
    if kind == ObjectKind::TrashCompactor { TRASH_CAPACITY * 3 } else { TRASH_CAPACITY }
}

/// A dish just cleared away, to be taken to the dishwasher or washed up at the sink.
#[derive(Component)]
pub struct WashUp;
/// How near (metres, on the same floor) mess counts against a Sim's surroundings.
const NEAR: f32 = 5.0;

/// When a meal was set out.
#[derive(Component)]
pub struct ServedAt(pub f64);

/// Food left out too long.
#[derive(Component)]
pub struct Spoiled;

/// How full a trash can is.
#[derive(Component, Default, Clone, Copy)]
pub struct TrashFill(pub u8);

/// A Sim who has just cleared dishes away: the scraps go in the nearest trash can.
#[derive(Component)]
pub struct Discarded;

fn spoil_food(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut meals: Query<(Entity, &mut GameObject, Option<&ServedAt>), (With<Meal>, Without<Spoiled>)>,
    mut notes: ResMut<Notifications>,
) {
    for (e, mut obj, served) in &mut meals {
        let Some(at) = served else {
            commands.entity(e).insert(ServedAt(clock.minutes));
            continue;
        };
        if clock.minutes - at.0 < SPOIL_MINUTES {
            continue;
        }
        // Left out too long: only fit for clearing away.
        commands.entity(e).remove::<Meal>().insert(Spoiled);
        let dish = obj.name.to_lowercase();
        let has = if dish.ends_with('s') { "have" } else { "has" };
        notes.push(format!("The {dish} left out {has} spoiled."));
        obj.kind = ObjectKind::DirtyDishes;
        obj.name = "Spoiled Food".into();
    }
}

/// Every trash can keeps count of what's in it.
fn trash_cans(mut commands: Commands, cans: Query<(Entity, &GameObject), Without<TrashFill>>) {
    for (e, o) in &cans {
        if matches!(o.kind, ObjectKind::TrashCan | ObjectKind::TrashCompactor) {
            commands.entity(e).insert(TrashFill(0));
        }
    }
}

fn discard(
    mut commands: Commands,
    sims: Query<(Entity, &Transform), With<Discarded>>,
    mut cans: Query<(&GameObject, &Transform, &mut TrashFill)>,
    mut notes: ResMut<Notifications>,
) {
    for (e, tf) in &sims {
        commands.entity(e).remove::<Discarded>();
        let Some((obj, _, mut fill)) = cans.iter_mut().min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation))) else {
            continue;
        };
        let cap = trash_capacity(obj.kind);
        if fill.0 < cap {
            fill.0 += 1;
            if fill.0 == cap {
                notes.push(format!("The {} is full. Someone should empty it.", obj.name.to_lowercase()));
            }
        }
    }
}

/// A dish cleared away goes in the nearest working dishwasher, or is washed up at the
/// nearest sink (with neither about, the scraps in the trash are the end of it).
#[allow(clippy::type_complexity)]
fn wash_up(
    mut commands: Commands,
    mut sims: Query<(Entity, &Transform, &mut crate::interact::ActionQueue), With<WashUp>>,
    objects: Query<(Entity, &GameObject, &Transform, &crate::interact::UsedBy), Without<crate::interact::Broken>>,
) {
    for (e, tf, mut queue) in &mut sims {
        commands.entity(e).remove::<WashUp>();
        // (The nearest about the lot, sooner on the same floor.)
        let far = |p: Vec3| p.distance(tf.translation) + if (p.y - tf.translation.y).abs() > 2.0 { 15.0 } else { 0.0 };
        let near = |kind: ObjectKind| {
            objects
                .iter()
                .filter(|(_, o, otf, used)| o.kind == kind && used.0.is_none_or(|u| u == e) && otf.translation.distance(tf.translation) < 60.0)
                .min_by(|a, b| far(a.2.translation).total_cmp(&far(b.2.translation)))
                .map(|(o, ..)| (o, kind))
        };
        let Some((target, kind)) = near(ObjectKind::Dishwasher).or_else(|| near(ObjectKind::Sink)) else {
            debug!("no dishwasher or sink near {:.1?} to wash up at", tf.translation);
            continue;
        };
        let Some(def) = crate::interact::interactions_for(kind).iter().position(|d| d.special == crate::interact::Special::WashDishes) else { continue };
        let name = crate::interact::interactions_for(kind)[def].name;
        queue.0.push_front(crate::interact::Action::new(name, crate::interact::ActionKind::Object { target, def }, true));
        // (The dish in hand on the way.)
        commands.entity(e).insert(crate::anim::Carrying(DISH_CARRY));
    }
}

/// The game's walks with a dinner plate held out, and with a trash bag.
pub const DISH_CARRY: &str = "a2o_plateDinner_carry_x";
const TRASH_CARRY: &str = "a2o_trashPile_carry_x";
const BOOK_CARRY: &str = "a2o_book_carry_x";
const PAPER_CARRY: &str = "a2o_newspaper_carry_x";

/// A TV just turned on: where to watch it from.
#[derive(Component)]
pub struct WatchFrom(pub Entity);

/// The TV a Sim is watching from a seat (it stays on for them).
#[derive(Component)]
pub struct WatchingTv(pub Entity);

/// Turned on, the TV is watched from the nearest free sofa or chair facing it (a few metres
/// off); with none, standing before it.
#[allow(clippy::type_complexity)]
fn watch_somewhere(
    mut commands: Commands,
    mut sims: Query<(Entity, &WatchFrom, &mut crate::interact::ActionQueue)>,
    seats: Query<(Entity, &GameObject, &Transform, &crate::interact::UsedBy), Without<crate::interact::Broken>>,
) {
    for (e, from, mut queue) in &mut sims {
        commands.entity(e).remove::<WatchFrom>();
        let Ok((_, _, ttf, _)) = seats.get(from.0) else { continue };
        let tv = ttf.translation;
        let watch = |kind: ObjectKind| crate::interact::interactions_for(kind).iter().position(|d| d.special == crate::interact::Special::WatchTv);
        let seat = seats
            .iter()
            .filter(|(_, o, t, used)| {
                let to_tv = (tv - t.translation).with_y(0.0);
                let ahead = (t.rotation * Vec3::Z).with_y(0.0).normalize_or_zero();
                matches!(o.kind, ObjectKind::Sofa | ObjectKind::Chair)
                    && used.0.is_none()
                    && (t.translation.y - tv.y).abs() < 1.5
                    && (1.2..6.0).contains(&to_tv.length())
                    && to_tv.normalize_or_zero().dot(ahead) > 0.7
            })
            .min_by(|a, b| {
                let far = |t: &Transform, o: &GameObject| t.translation.distance(tv) + if o.kind == ObjectKind::Sofa { 0.0 } else { 1.5 };
                far(a.2, a.1).total_cmp(&far(b.2, b.1))
            });
        match seat.and_then(|(s, o, ..)| Some((s, watch(o.kind)?))) {
            Some((s, def)) => {
                queue.0.push_front(crate::interact::Action::new("Watch the TV", crate::interact::ActionKind::Object { target: s, def }, true));
                commands.entity(e).insert(WatchingTv(from.0));
            }
            None => {
                if let Some(def) = watch(ObjectKind::Tv) {
                    queue.0.push_front(crate::interact::Action::new("Watch the TV", crate::interact::ActionKind::Object { target: from.0, def }, true));
                }
            }
        }
    }
}

/// Done watching (or off to something else), the Sim's TV is theirs no longer.
fn stop_watching(
    mut commands: Commands,
    mut sims: Query<(Entity, &mut crate::interact::ActionQueue, &WatchingTv)>,
    objects: Query<&GameObject>,
    unavailable: Query<(), Or<(With<crate::interact::Broken>, With<crate::buy::HeldObject>, With<crate::buyhistory::HistoryHidden>)>>,
) {
    for (e, mut queue, watching) in &mut sims {
        let active = queue.0.front_mut().is_some_and(|a| {
            let crate::interact::ActionKind::Object { target, def } = a.kind else { return false };
            if a.cancel || a.completed || !objects.get(target).ok().and_then(|o| crate::interact::interactions_for(o.kind).get(def))
                .is_some_and(|d| d.special == crate::interact::Special::WatchTv) { return false; }
            if unavailable.contains(watching.0) || !objects.get(watching.0).is_ok_and(|o| o.kind == ObjectKind::Tv) {
                a.cancel = true;
                return false;
            }
            true
        });
        if !active {
            commands.entity(e).remove::<WatchingTv>();
        }
    }
}

/// The newspaper picked up (hidden where it lay until it's read).
#[derive(Component)]
pub struct PaperInHand(pub Entity);

/// The paper picked up is carried to the nearest free sofa or chair and read sitting down;
/// with none free, it's read standing where it lay.
#[allow(clippy::type_complexity)]
fn read_paper_somewhere(
    mut commands: Commands,
    mut sims: Query<(Entity, &Transform, &PaperInHand, &mut crate::interact::ActionQueue), Added<PaperInHand>>,
    seats: Query<(Entity, &GameObject, &Transform, &crate::interact::UsedBy), Without<crate::interact::Broken>>,
) {
    for (e, tf, paper, mut queue) in &mut sims {
        let read = |kind: ObjectKind| crate::interact::interactions_for(kind).iter().position(|d| d.special == crate::interact::Special::ReadPaper);
        let seat = seats
            .iter()
            .filter(|(_, o, t, used)| {
                matches!(o.kind, ObjectKind::Sofa | ObjectKind::Chair) && used.0.is_none() && (t.translation.y - tf.translation.y).abs() < 1.5 && t.translation.distance(tf.translation) < 30.0
            })
            .min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
        match seat.and_then(|(s, o, ..)| Some((s, read(o.kind)?))) {
            Some((s, def)) => {
                queue.0.push_front(crate::interact::Action::new("Read the Paper", crate::interact::ActionKind::Object { target: s, def }, true));
                commands.entity(e).insert(crate::anim::Carrying(PAPER_CARRY));
            }
            None => {
                commands.entity(paper.0).insert(Visibility::Inherited);
                commands.entity(e).remove::<PaperInHand>();
                if let Some(def) = read(ObjectKind::Newspaper) {
                    queue.0.push_front(crate::interact::Action::new("Read the Paper", crate::interact::ActionKind::Object { target: paper.0, def }, true));
                }
            }
        }
    }
}

/// A paper carried off and not read after all is put back where it lay.
fn drop_unread_paper(mut commands: Commands, sims: Query<(Entity, &PaperInHand, &crate::interact::ActionQueue)>) {
    for (e, paper, queue) in &sims {
        if !queue.current().is_some_and(|a| a.label == "Read the Paper") {
            commands.entity(paper.0).try_insert(Visibility::Inherited);
            commands.entity(e).remove::<PaperInHand>();
        }
    }
}

/// A book just taken from a shelf: where to read it.
#[derive(Component)]
pub struct ReadSomewhere(pub Entity);

/// A book taken from the shelf is carried to the nearest free sofa or chair (on the same floor)
/// and read sitting down; with none free, it's read standing at the shelf.
#[allow(clippy::type_complexity)]
fn read_somewhere(
    mut commands: Commands,
    mut sims: Query<(Entity, &Transform, &ReadSomewhere, &mut crate::interact::ActionQueue)>,
    seats: Query<(Entity, &GameObject, &Transform, &crate::interact::UsedBy), Without<crate::interact::Broken>>,
) {
    for (e, tf, shelf, mut queue) in &mut sims {
        commands.entity(e).remove::<ReadSomewhere>();
        let read = |kind: ObjectKind| crate::interact::interactions_for(kind).iter().position(|d| d.special == crate::interact::Special::ReadBook);
        // (Sofas sooner than chairs.)
        let seat = seats
            .iter()
            .filter(|(_, o, t, used)| {
                matches!(o.kind, ObjectKind::Sofa | ObjectKind::Chair) && used.0.is_none() && (t.translation.y - tf.translation.y).abs() < 1.5 && t.translation.distance(tf.translation) < 15.0
            })
            .min_by(|a, b| {
                let far = |(_, o, t, _): &(Entity, &GameObject, &Transform, &crate::interact::UsedBy)| t.translation.distance(tf.translation) + if o.kind == ObjectKind::Sofa { 0.0 } else { 4.0 };
                far(a).total_cmp(&far(b))
            });
        match seat.and_then(|(s, o, ..)| Some((s, read(o.kind)?))) {
            Some((s, def)) => {
                queue.0.push_front(crate::interact::Action::new("Read Book", crate::interact::ActionKind::Object { target: s, def }, true));
                commands.entity(e).insert(crate::anim::Carrying(BOOK_CARRY));
            }
            None => {
                if let Some(def) = read(ObjectKind::Bookshelf) {
                    queue.0.push_front(crate::interact::Action::new("Read Book", crate::interact::ActionKind::Object { target: shelf.0, def }, true));
                }
            }
        }
    }
}

/// What's carried is put down once it's where it was going (being washed, eaten, thrown out),
/// or once that's given up.
fn put_down_carried(mut commands: Commands, sims: Query<(Entity, &crate::anim::Carrying, &crate::interact::ActionQueue, Has<crate::anim::ActionClip>, &crate::sim::SimAnim)>, objects: Query<&GameObject>) {
    for (e, c, queue, acting, anim) in &sims {
        let goes_to: &[&str] = match c.0 {
            DISH_CARRY => &["Wash Dishes", "Load Dishes", "Eat"],
            TRASH_CARRY => &["Throw Out Trash"],
            BOOK_CARRY => &["Read Book"],
            PAPER_CARRY => &["Read the Paper"],
            crate::meals::FOOD_CARRY => &["Prepare Food"],
            crate::meals::PAN_CARRY => &["Cook Dinner"],
            crate::meals::PLATTER_CARRY => &["Set Down Meal"],
            _ => continue,
        };
        let going = queue.current().is_some_and(|a| {
            if a.cancel || a.completed { return false; }
            if c.0 == crate::meals::PAN_CARRY {
                let crate::interact::ActionKind::Object { target, def } = a.kind else { return false };
                return objects.get(target).ok().and_then(|o| crate::interact::interactions_for(o.kind).get(def))
                    .is_some_and(|d| d.special == crate::interact::Special::ServeMeal);
            }
            goes_to.contains(&a.label.as_str())
        });
        if !going || (acting && anim.pose != crate::sim::Pose::Walk) {
            commands.entity(e).remove::<crate::anim::Carrying>();
        }
    }
}

#[cfg(test)]
mod carry_tests {
    use super::*;
    use crate::interact::{Action, ActionKind, ActionQueue, Phase, Special, interactions_for};

    #[test]
    fn seated_viewing_stops_when_television_is_deleted_broken_or_removed_from_play() {
        for failure in 0..5 {
            let mut app = App::new();
            app.add_systems(Update, stop_watching);
            let object = |kind| GameObject { kind, name: "Test".into(), objd: (0, 0, 0), price: 0,
                center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None };
            let tv = app.world_mut().spawn(object(ObjectKind::Tv)).id();
            let chair = app.world_mut().spawn(object(ObjectKind::Chair)).id();
            let def = interactions_for(ObjectKind::Chair).iter().position(|d| d.special == Special::WatchTv).unwrap();
            let mut action = Action::new("Watch the TV", ActionKind::Object { target: chair, def }, false);
            action.phase = Phase::Running(10.0);
            let mut queue = ActionQueue::default();
            queue.push_player(action);
            let sim = app.world_mut().spawn((queue, WatchingTv(tv))).id();
            app.update();
            assert!(!app.world().get::<ActionQueue>(sim).unwrap().0[0].cancel);
            assert!(app.world().get::<WatchingTv>(sim).is_some());
            match failure {
                0 => { app.world_mut().despawn(tv); }
                1 => { app.world_mut().entity_mut(tv).insert(crate::interact::Broken); }
                2 => { app.world_mut().entity_mut(tv).insert(crate::buy::HeldObject); }
                3 => { app.world_mut().entity_mut(tv).insert(crate::buyhistory::HistoryHidden); }
                _ => { app.world_mut().get_mut::<GameObject>(tv).unwrap().kind = ObjectKind::Stereo; }
            }
            app.update();
            assert!(app.world().get::<ActionQueue>(sim).unwrap().0[0].cancel, "failure {failure}");
            assert!(app.world().get::<WatchingTv>(sim).is_none());
        }
    }

    #[test]
    fn named_recipe_carries_pan_until_cancelled_or_stove_disappears() {
        let mut app = App::new();
        app.add_systems(Update, put_down_carried);
        let stove = app.world_mut().spawn(GameObject { kind: ObjectKind::Stove, name: "Stove".into(), objd: (0, 0, 0), price: 0,
            center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None }).id();
        let def = interactions_for(ObjectKind::Stove).iter().position(|d| d.special == Special::ServeMeal).unwrap();
        let mut action = Action::new("Cook: Pancakes", ActionKind::Object { target: stove, def }, false);
        action.phase = Phase::Routing;
        let mut queue = ActionQueue::default();
        queue.push_player(action);
        let sim = app.world_mut().spawn((queue, crate::anim::Carrying(crate::meals::PAN_CARRY),
            crate::sim::SimAnim { pose: crate::sim::Pose::Walk, ..default() })).id();
        app.update();
        assert!(app.world().get::<crate::anim::Carrying>(sim).is_some());
        app.world_mut().get_mut::<ActionQueue>(sim).unwrap().0[0].cancel = true;
        app.update();
        assert!(app.world().get::<crate::anim::Carrying>(sim).is_none());
        app.world_mut().get_mut::<ActionQueue>(sim).unwrap().0[0].cancel = false;
        app.world_mut().entity_mut(sim).insert(crate::anim::Carrying(crate::meals::PAN_CARRY));
        app.world_mut().despawn(stove);
        app.update();
        assert!(app.world().get::<crate::anim::Carrying>(sim).is_none());
    }
}

/// An indoor trash can just emptied: its bag to take out.
#[derive(Component)]
pub struct TakeOutTrash(pub Entity);

/// The bag from an indoor trash can is carried out to the nearest one outdoors (with none,
/// it's gone with the emptying).
fn take_out_trash(
    mut commands: Commands,
    mut sims: Query<(Entity, &Transform, &TakeOutTrash, &mut crate::interact::ActionQueue)>,
    cans: Query<(Entity, &GameObject, &Transform), Without<crate::interact::Broken>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
) {
    for (e, tf, from, mut queue) in &mut sims {
        commands.entity(e).remove::<TakeOutTrash>();
        let Some(b) = building.as_ref() else { continue };
        let indoors = |p: Vec3| b.is_indoors(p);
        if cans.get(from.0).is_ok_and(|(_, _, t)| !indoors(t.translation)) {
            continue;
        }
        let Some((can, ..)) = cans
            .iter()
            .filter(|(_, o, t)| o.kind == ObjectKind::TrashCan && !indoors(t.translation) && t.translation.distance(tf.translation) < 60.0)
            .min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)))
        else {
            continue;
        };
        let Some(def) = crate::interact::interactions_for(ObjectKind::TrashCan).iter().position(|d| d.special == crate::interact::Special::DropTrash) else { continue };
        queue.0.push_front(crate::interact::Action::new("Throw Out Trash", crate::interact::ActionKind::Object { target: can, def }, true));
        commands.entity(e).insert(crate::anim::Carrying(TRASH_CARRY));
    }
}

/// Dirty and Filthy Surroundings for household Sims near mess.
#[allow(clippy::type_complexity)]
fn surroundings(
    time: Res<Time>,
    mut last: Local<f32>,
    mut sims: Query<(&Transform, &mut Moodlets, &Visibility), (With<Sim>, With<HouseholdMember>)>,
    mess: Query<(&GameObject, &Transform, Has<Spoiled>, Option<&TrashFill>)>,
) {
    let now = time.elapsed_secs();
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    // Each mess and how bad it is: dishes, spoiled food, a full trash can.
    let spots: Vec<(Vec3, u32)> = mess
        .iter()
        .filter_map(|(o, tf, spoiled, fill)| {
            let w = match o.kind {
                ObjectKind::DirtyDishes if spoiled => 3,
                ObjectKind::DirtyDishes => 1,
                ObjectKind::TrashCan | ObjectKind::TrashCompactor if fill.is_some_and(|f| f.0 >= trash_capacity(o.kind)) => 3,
                _ => return None,
            };
            Some((tf.translation, w))
        })
        .collect();
    for (tf, mut moodlets, vis) in &mut sims {
        let score: u32 = if *vis == Visibility::Hidden {
            0
        } else {
            spots.iter().filter(|(p, _)| (p.y - tf.translation.y).abs() < 1.8 && p.with_y(0.0).distance(tf.translation.with_y(0.0)) < NEAR).map(|s| s.1).sum()
        };
        let filthy = score >= 6;
        moodlets.set_while(MoodletKind::FilthySurroundings, filthy);
        moodlets.set_while(MoodletKind::DirtySurroundings, score >= 2 && !filthy);
    }
}
