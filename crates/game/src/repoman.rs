//! The repo man, as the game's `Repoman` service: bills left unpaid three days are a debt, and an
//! hour later he pulls up in his truck, in his uniform, to take things worth it. One by one he
//! picks whatever's nearest in worth to what's still owed, stands a few metres off and zaps it
//! away with his repo gun (the game's `repoman` clips, ten minutes a thing); the household is
//! upset at each (Stuff Taken). With nothing left worth taking, the household's funds pay the
//! rest, if they can. Then he's off. The debt's kept in saves.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{ActionKind, ActionQueue, GameObject, Household, Notifications, ObjectKind, UsedBy};
use crate::nav::PathFollow;
use crate::sim::{Age, HouseholdMember, Sim};

pub struct RepomanPlugin;

impl Plugin for RepomanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RepoDue>()
            .add_systems(OnEnter(crate::AppState::InGame), |mut commands: Commands| commands.insert_resource(RepoDue::default()))
            .add_systems(Update, (repo_man, repo_test).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// What the household owes for bills gone unpaid (the game's `DelinquentFunds`), and when the
/// repo man comes for it.
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct RepoDue {
    pub debt: i64,
    #[serde(skip)]
    pub arrive_at: Option<f64>,
}

/// The game's tunables: how long after the bills he comes (game minutes), how far off he stands
/// (metres), and how long a thing takes him.
const DELAY_MINUTES: f64 = 60.0;
const STAND_OFF: f32 = 3.0;
const ZAP_MINUTES: f64 = 10.0;
/// How long his gun takes to stop and vanish (game minutes: its clips').
const PUT_AWAY_MINUTES: f64 = 3.0;

/// The game's words (`Bill`, `RepomanSituation`).
pub const ON_HIS_WAY: &str = "Looks like your household bills have gone unpaid for too long. The repoman is on his way to repossess your objects in order to cover your debt.";
const ON_LOT: &str = "Repoman! Repoman! The \"friendly\" neighborhood Repoman is on your lot!";
const NOTHING_WORTH: &str = "Your family has nothing worthwhile to even repossess.";

const ZAP: crate::anim::ActionClip = crate::anim::ActionClip::steps("a2o_repoMan_repossess_appear_x", &["a2o_repoMan_repossess_start_x"], &["a2o_repoMan_repossess_loop1_x", "a2o_repoMan_repossess_loop2_x"]);
const PUT_AWAY: crate::anim::ActionClip = crate::anim::ActionClip::steps("a2o_repoMan_repossess_stop_x", &["a2o_repoMan_repossess_disappear_x"], &[]);

/// The repo man at work: what he's taking now, and since when.
#[derive(Component)]
pub struct Repoman {
    target: Option<Entity>,
    zapping: Option<f64>,
    putting_away: Option<f64>,
    leaving: bool,
    since: f64,
}

/// Bills gone unpaid: owed now, and the repo man's on his way.
pub fn owed(due: &mut RepoDue, amount: i64, now: f64) {
    due.debt += amount;
    due.arrive_at.get_or_insert(now + DELAY_MINUTES);
}

/// Of the things he might take (their worth), the one nearest what's owed (the dearer of two as
/// near): the game's `PickObjectToRepo`.
pub fn pick(things: &[(Entity, i64)], owed: i64) -> Option<Entity> {
    things
        .iter()
        .filter(|(_, worth)| *worth > 0)
        .min_by(|a, b| (a.1 - owed).abs().cmp(&(b.1 - owed).abs()).then(b.1.cmp(&a.1)))
        .map(|(e, _)| *e)
}

/// Whether he'd take a thing of this kind (not what the house is built of, nor the mailbox,
/// graves, food or the household's car).
fn takeable(kind: ObjectKind) -> bool {
    !matches!(
        kind,
        ObjectKind::Mailbox | ObjectKind::Tombstone | ObjectKind::Meal | ObjectKind::DirtyDishes | ObjectKind::Door | ObjectKind::Car | ObjectKind::Newspaper | ObjectKind::Puddle | ObjectKind::ClothingPile | ObjectKind::BirthdayCake
    )
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn repo_man(
    mut commands: Commands,
    mut clock: ResMut<GameClock>,
    mut due: ResMut<RepoDue>,
    mut household: Option<ResMut<Household>>,
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<crate::loading::CurrentWorld>,
    things: Query<
        (Entity, &GameObject, &Transform, &UsedBy, Option<&crate::interact::Depreciated>, Has<crate::grime::Charred>, Option<&crate::paintings::Hung>, Option<&crate::nav::Floor>),
        (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>, Without<crate::building::WallObject>, Without<Sim>),
    >,
    mut men: Query<(Entity, &mut Repoman, &mut ActionQueue, &mut Transform, Option<&PathFollow>), (With<Sim>, Without<GameObject>)>,
    mut members: Query<(&Sim, &mut crate::life::Moodlets), With<HouseholdMember>>,
    (mut rides, mut notes, mut sounds): (ResMut<crate::traffic::PendingRides>, ResMut<Notifications>, MessageWriter<crate::sound::PlaySound>),
    loading: Option<Res<crate::save::PendingLoad>>,
) {
    let Some(exit) = exit else { return };
    // (Owed when a save came in: he's on his way again.)
    if loading.is_none() && due.debt > 0 && due.arrive_at.is_none() && men.is_empty() {
        due.arrive_at = Some(clock.minutes + DELAY_MINUTES);
    }
    if due.arrive_at.is_some_and(|t| clock.minutes >= t) {
        due.arrive_at = None;
        if men.is_empty() && due.debt > 0 {
            let mut rng = rand::rng();
            let female = rng.random_bool(0.5);
            let age = if rng.random_bool(0.5) { Age::Adult } else { Age::YoungAdult };
            let sim = crate::sim::random_sim(&mut rng, "Repoman", Some(female), age);
            let p = crate::interact::arrival_spot(exit.0);
            let at = Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y);
            let e = crate::services::arrive(&mut commands, sim, at, Some(crate::simbody::ServiceUniform::Repo), crate::interact::Skills::default());
            commands.entity(e).insert(Repoman { target: None, zapping: None, putting_away: None, leaving: false, since: clock.minutes });
            rides.0.push((exit.0, "CarServiceRepomanPickup"));
            // (The game slows to normal for him, with his sting.)
            if clock.speed > 1 && clock.speed != crate::clock::SKIP {
                clock.set_speed(1);
            }
            sounds.write(crate::sound::PlaySound { name: "sting_repoman_arrive".into(), at: None, volume: 1.0, speed: 1.0 });
            notes.push(ON_LOT);
            info!("the repo man is here for §{}", due.debt);
        }
    }
    for (me, mut man, mut queue, mut tf, path) in &mut men {
        if man.leaving {
            crate::services::leave(&mut commands, me, &mut queue, &tf, path, exit.0);
            continue;
        }
        // Putting the gun away: the thing gone, and on to the next.
        if let Some(t) = man.putting_away {
            if clock.minutes - t >= PUT_AWAY_MINUTES {
                man.putting_away = None;
                commands.entity(me).remove::<crate::anim::ActionClip>();
            }
            continue;
        }
        // Zapping it: ten minutes, then it's gone.
        if let Some(t) = man.zapping {
            let Some(target) = man.target else { continue };
            let Ok((_, o, otf, _, lost, charred, hung, _)) = things.get(target) else {
                // (Gone from under him: on to the next.)
                man.zapping = None;
                man.target = None;
                commands.entity(me).remove::<crate::anim::ActionClip>();
                continue;
            };
            // (Facing it as he works.)
            let to = otf.translation.xz() - tf.translation.xz();
            if to.length_squared() > 1e-4 {
                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
            }
            if clock.minutes - t >= ZAP_MINUTES {
                let worth = crate::interact::object_value(o, lost, charred, hung);
                due.debt = (due.debt - worth).max(0);
                info!("the repo man took {} (§{worth}): §{} still owed", o.name, due.debt);
                sounds.write(crate::sound::PlaySound { name: "repoman_gun_vanish".into(), at: Some(otf.translation), volume: 1.0, speed: 1.0 });
                commands.entity(target).try_despawn();
                // (Everyone of the household upset: the game's `StuffTaken`.)
                for (s, mut moodlets) in &mut members {
                    if !s.age.is_little() {
                        moodlets.add(crate::life::MoodletKind::StuffTaken, clock.minutes);
                    }
                }
                man.zapping = None;
                man.target = None;
                man.putting_away = Some(clock.minutes);
                commands.entity(me).insert(PUT_AWAY);
            }
            continue;
        }
        let idle = queue.0.is_empty() && path.is_none_or(|p| p.done);
        if !idle {
            // (Stuck on the way an hour: he gives up on it.)
            if clock.minutes - man.since > 60.0 {
                queue.0.clear();
                commands.entity(me).remove::<PathFollow>();
            }
            continue;
        }
        // Arrived where he means to stand: at it.
        if let Some(target) = man.target {
            match things.get(target) {
                Ok((_, _, otf, used, ..)) if used.0.is_none() && tf.translation.xz().distance(otf.translation.xz()) <= STAND_OFF + 1.5 => {
                    man.zapping = Some(clock.minutes);
                    commands.entity(me).insert(ZAP);
                    sounds.write(crate::sound::PlaySound { name: "repoman_gun_appear".into(), at: Some(tf.translation), volume: 1.0, speed: 1.0 });
                }
                // (In use, or out of reach: another.)
                _ => man.target = None,
            }
            if man.zapping.is_some() {
                continue;
            }
        }
        // The next thing: the nearest in worth to what's owed.
        let candidates: Vec<(Entity, i64)> = if due.debt > 0 {
            things
                .iter()
                .filter(|(e, o, _, used, ..)| takeable(o.kind) && used.0.is_none() && o.price > 0 && Some(*e) != man.target)
                .map(|(e, o, _, _, lost, charred, hung, _)| (e, crate::interact::object_value(o, lost, charred, hung)))
                .collect()
        } else {
            Vec::new()
        };
        match pick(&candidates, due.debt) {
            Some(target) => {
                let Ok((_, _, otf, .., floor)) = things.get(target) else { continue };
                // (A few metres off, on his side of it.)
                let from = tf.translation.xz();
                let at = otf.translation.xz();
                let spot = at + (from - at).normalize_or(Vec2::X) * STAND_OFF;
                let floor = floor.map_or(1, |f| f.0);
                man.target = Some(target);
                man.since = clock.minutes;
                queue.0.push_back(crate::interact::Action::new("Repossess", ActionKind::GoHere(spot, floor), false));
            }
            None => {
                // With nothing left worth taking, the household's funds make it up, if they can.
                if due.debt > 0 {
                    match household.as_mut() {
                        Some(h) if h.funds >= due.debt => {
                            h.funds -= due.debt;
                            notes.push(format!("The repoman took §{} from your household funds.", due.debt));
                        }
                        _ => notes.push(NOTHING_WORTH),
                    }
                    due.debt = 0;
                }
                man.leaving = true;
                info!("the repo man is done");
            }
        }
    }
}

/// REPO=<amount> (tests): that much owed, and the repo man on his way at once.
fn repo_test(mut due: ResMut<RepoDue>, clock: Res<GameClock>, time: Res<Time>, mut done: Local<bool>, mut notes: ResMut<Notifications>) {
    let Some(amount) = std::env::var("REPO").ok().and_then(|v| v.parse::<i64>().ok()) else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    *done = true;
    due.debt += amount;
    due.arrive_at = Some(clock.minutes + 1.0);
    notes.push(ON_HIS_WAY);
    info!("repo test: §{amount} owed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_nearest_in_worth() {
        let (a, b, c) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let things = [(a, 100), (b, 900), (c, 450)];
        assert_eq!(pick(&things, 500), Some(c));
        assert_eq!(pick(&things, 2000), Some(b));
        // (As near either way: the dearer.)
        assert_eq!(pick(&[(a, 400), (b, 600)], 500), Some(b));
        // (Nothing of worth: nothing.)
        assert_eq!(pick(&[(a, 0)], 500), None);
    }
}
