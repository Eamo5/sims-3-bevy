//! The computer's interactions for the traits, as the game's `Computer`: a Genius can Solve Hard
//! Problems (two hours; three in five times solved, for a prize of §50 to §225: its tuning's
//! `SolveHardProblems`, `GeniusTraitChanceToSolveProblem`); a Workaholic can Work on Job-Related
//! Issues (four hours, for 4 performance: `WorkOnJobRelatedIssues`), once a phone call to Check
//! Up On Work has found there's
//! something to do (half the time; for two hours: `CallCheckUpOnWork`); a Bookworm can Join the
//! Book Club (§250 for life, `JoinBookClub`), and a book comes in the mail for each member every
//! Monday (`Mailbox.SendBookClubBooks`), onto the household's shelves. A Computer Whiz can Hack
//! from nine at night till seven in the morning (the computers' tuned `HackStartHour`,
//! `HackEndHour`): §8 to §23 into the household funds every quarter hour (one time in ten four
//! times as much: `HackMinPay`, `HackMaxPay`, `HackChanceOfBonus`, `HackBonusMultiplier`), and
//! every half hour a chance in fifty of being caught (`ComputerWhizTraitChanceToGetCaughtWhileHacking`,
//! tuned 0.02): a counter-virus, and the computer broken; fun for them (`kComputerWhizFunMotive`,
//! 30 an hour). A Computer Whiz can Overclock a computer not upgraded yet (an hour: a chance in
//! four of breaking it, one in four of nothing, else the better graphics or unbreakable,
//! `kChanceToBreak`, `kChanceToDoNothing`). A Workaholic can Work from Home out of work hours
//! (not while there's work to sort out on the computer): the computer's tuned performance an
//! hour (`WorkAtHomePerormanceIncPerHour`: the cheap one 3, a laptop 4, the expensive one 5),
//! paid by the hour worked (`FinishWorking`).

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::interact::Special;
use crate::life::Trait;
use crate::sim::{Age, HouseholdMember, Sim};

pub struct ComputerTraitsPlugin;

impl Plugin for ComputerTraitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (done, checked_up, book_club_mail, traits_test, hacking, overclocked, working_from_home).run_if(in_state(PlayMode::Live)));
    }
}

/// SET_TRAITS=<trait name>,... (tests): the selected Sim given those traits, a few seconds in.
fn traits_test(mut sel: Query<&mut Sim, With<crate::sim::Selected>>, time: Res<Time>, mut done: Local<bool>) {
    let Ok(want) = std::env::var("SET_TRAITS") else { return };
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    let Ok(mut sim) = sel.single_mut() else { return };
    *done = true;
    for name in want.split(',') {
        if let Some(t) = Trait::ALL.iter().find(|t| t.name().eq_ignore_ascii_case(name.trim())).copied()
            && !sim.traits.contains(&t)
        {
            sim.traits.push(t);
        }
    }
    info!("traits test: {} is {:?}", sim.first, sim.traits);
}

/// How keen a Workaholic is to work from home, by free will (the game's `TraitWorkaholic` 200).
pub const WORK_FROM_HOME_PULL: f32 = 30.0;
/// `Overclock`'s `kChanceToBreak` and `kChanceToDoNothing`.
const OVERCLOCK_BREAKS: f64 = 0.25;
const OVERCLOCK_NOTHING: f64 = 0.25;

/// Whether it's the hours they'd go to work (the game's `IsAllowedToWork`: from an hour before
/// the shift till it ends, on a workday).
pub fn work_time(job: &crate::careers::Job, clock: &crate::clock::GameClock) -> bool {
    let (h, l) = (clock.hour_f(), job.info());
    let from = l.start - 1.0;
    let during = if l.end > from { h >= from && h < l.end } else { h >= from || h < l.end };
    job.works_on(clock.weekday()) && during
}

/// Whether they'd work from home now: a Workaholic out of work hours, with nothing to sort out.
pub fn may_work_from_home(sim: &Sim, job: Option<&crate::careers::Job>, work_to_do: bool, clock: &crate::clock::GameClock) -> bool {
    offers(Special::WorkFromHome, sim, job, false, work_to_do) && job.is_some_and(|j| !work_time(j, clock))
}

/// A computer's performance an hour worked at home, by its class's tuning.
fn home_performance(script: &str) -> f32 {
    let s = script.to_ascii_lowercase();
    if s.ends_with(".computercheap") {
        3.0
    } else if s.contains(".computerlaptop") {
        4.0
    } else {
        5.0
    }
}

/// Working from home: the performance it's worth an hour, and since when.
#[derive(Component)]
pub struct WorkingFromHome {
    rate: f32,
    since: f64,
    last: f64,
}

/// Working from home: performance as they go, paid for the hours when they stop.
#[allow(clippy::type_complexity)]
fn working_from_home(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &crate::interact::ActionQueue, Option<&mut WorkingFromHome>, Option<&mut crate::careers::Job>)>,
    objects: Query<&crate::interact::GameObject>,
    data: Option<Res<crate::baked::Baked>>,
    mut household: Option<ResMut<crate::interact::Household>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let now = clock.minutes;
    for (e, sim, queue, working, job) in &mut sims {
        let at = queue.0.front().filter(|a| matches!(a.phase, crate::interact::Phase::Running(_)) && !a.cancel).and_then(|a| match a.kind {
            crate::interact::ActionKind::Object { target, def } => objects
                .get(target)
                .ok()
                .filter(|o| crate::interact::interactions_for(o.kind).get(def).is_some_and(|d| d.special == Special::WorkFromHome))
                .map(|o| o.objd),
            _ => None,
        });
        match (at, working, job) {
            (Some(objd), None, Some(_)) => {
                let script = data.as_ref().and_then(|d| d.0.catalog.iter().find(|c| c.objd == objd).map(|c| c.script.clone())).unwrap_or_default();
                commands.entity(e).insert(WorkingFromHome { rate: home_performance(&script), since: now, last: now });
            }
            (Some(_), Some(mut w), Some(mut j)) => {
                let dt = (now - w.last).max(0.0) as f32;
                w.last = now;
                j.performance = (j.performance + w.rate * dt / 60.0).clamp(-100.0, 100.0);
            }
            (None, Some(w), job) => {
                let hours = (now - w.since).max(0.0) / 60.0;
                let pay = job.map_or(0, |j| (j.info().hourly as f64 * hours) as i64);
                if pay > 0 {
                    if let Some(h) = household.as_mut() {
                        h.funds += pay;
                    }
                    notes.push(format!("{} earned §{pay} working from home.", sim.first));
                }
                commands.entity(e).remove::<WorkingFromHome>();
            }
            _ => {}
        }
    }
}

/// A computer overclocked (its outcome to be had).
#[derive(Component)]
pub struct Overclocked(pub Entity);

/// Overclocked: broken, no different, or upgraded (better graphics, or unbreakable).
fn overclocked(
    mut commands: Commands,
    sims: Query<(Entity, &Sim, &Overclocked)>,
    mut upgrades: Query<&mut crate::upgrades::Upgrades>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    use crate::upgrades::Upgrade;
    let mut rng = rand::rng();
    for (e, sim, o) in &sims {
        commands.entity(e).remove::<Overclocked>();
        let computer = o.0;
        let roll: f64 = rng.random();
        let (key, or) = if roll < OVERCLOCK_BREAKS {
            commands.entity(computer).insert(crate::interact::Broken);
            ("BreakComputer", format!("Success! Or not. {} has now thoroughly broken the computer.", sim.first))
        } else if roll < OVERCLOCK_BREAKS + OVERCLOCK_NOTHING {
            ("NothingHappens", "YES! No! It doesn't look like the computer's running any differently...".to_string())
        } else {
            let (up, key, or) = if rng.random_bool(0.5) {
                (Upgrade::Improved, "GraphicsUpgrade", "Woohoo! The computer isn't running any faster but the graphics look better somehow.")
            } else {
                (Upgrade::Unbreakable, "UnbreakableUpgrade", "Well, the computer didn't get any faster but it certainly looks a lot sturdier. That's a good thing, right?")
            };
            match upgrades.get_mut(computer) {
                Ok(mut u) => u.0 |= up.bit(),
                Err(_) => {
                    commands.entity(computer).insert(crate::upgrades::Upgrades(up.bit()));
                }
            }
            (key, or.to_string())
        };
        let text = ui
            .as_deref()
            .and_then(|u| u.localize(&format!("Gameplay/Objects/Electronics/Computer/Overclock:{key}")))
            .map_or(or, |t| t.replace("{0.SimFirstName}", &sim.first));
        notes.push(text);
    }
}

/// The computers' tuned hacking: its hours, its pay, and the chance of being caught.
const HACK_START: f32 = 21.0;
const HACK_END: f32 = 7.0;
const HACK_PAY_EVERY: f64 = 15.0;
const HACK_PAY: std::ops::RangeInclusive<i64> = 8..=23;
const HACK_BONUS_CHANCE: f64 = 0.1;
const HACK_BONUS: i64 = 4;
const CAUGHT_EVERY: f64 = 30.0;
const CAUGHT_CHANCE: f64 = 0.02;

/// Whether it's the hours for hacking.
pub fn hack_hours(hour: f32) -> bool {
    hour >= HACK_START || hour < HACK_END
}

/// Hacking at a computer: when it next pays, when they might next be caught, and what it's made.
#[derive(Component)]
pub struct Hacking {
    next_pay: f64,
    next_check: f64,
    total: i64,
}

/// Hacking: paid as it goes, till morning, or till they're caught (the computer broken).
#[allow(clippy::type_complexity)]
fn hacking(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &mut crate::interact::ActionQueue, Option<&mut Hacking>)>,
    objects: Query<&crate::interact::GameObject>,
    mut household: Option<ResMut<crate::interact::Household>>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    let say = |key: &str, sim: &Sim, n: i64, or: String| {
        ui.as_deref()
            .and_then(|u| u.localize(&format!("Gameplay/Objects/Electronics/Computer/Hack:{key}")))
            .map_or(or, |t| t.replace("{0.SimFirstName}", &sim.first).replace("{1.Money}", &format!("§{n}")))
    };
    for (e, sim, mut queue, hack) in &mut sims {
        let at = queue.0.front().filter(|a| matches!(a.phase, crate::interact::Phase::Running(_)) && !a.cancel).and_then(|a| match a.kind {
            crate::interact::ActionKind::Object { target, def } => objects
                .get(target)
                .ok()
                .and_then(|o| crate::interact::interactions_for(o.kind).get(def))
                .filter(|d| d.special == Special::Hack)
                .map(|_| target),
            _ => None,
        });
        match (at, hack) {
            (Some(_), None) => {
                commands.entity(e).insert(Hacking { next_pay: now + HACK_PAY_EVERY, next_check: now + CAUGHT_EVERY, total: 0 });
            }
            (Some(computer), Some(mut h)) => {
                if now >= h.next_pay {
                    h.next_pay += HACK_PAY_EVERY;
                    let pay = rng.random_range(HACK_PAY) * if rng.random_bool(HACK_BONUS_CHANCE) { HACK_BONUS } else { 1 };
                    h.total += pay;
                    if let Some(hh) = household.as_mut() {
                        hh.funds += pay;
                    }
                }
                let caught = now >= h.next_check && {
                    h.next_check += CAUGHT_EVERY;
                    rng.random_bool(CAUGHT_CHANCE)
                };
                if caught {
                    notes.push(say("CaughtHackingText", sim, 0, format!("{} was caught hacking: a counter-virus has damaged the computer.", sim.first)));
                    commands.entity(computer).insert(crate::interact::Broken);
                }
                if caught || !hack_hours(clock.hour_f()) {
                    if let Some(a) = queue.0.front_mut() {
                        a.cancel = true;
                    }
                }
            }
            (None, Some(h)) => {
                if h.total > 0 {
                    notes.push(say("MoneyEarned", sim, h.total, format!("{} has hacked a total of §{} into the household funds.", sim.first, h.total)));
                }
                commands.entity(e).remove::<Hacking>();
            }
            (None, None) => {}
        }
    }
}

/// In the book club (kept with the game).
#[derive(Component, Clone, Copy)]
pub struct BookClub;

/// Work to be done from home, found by checking up on work: until when (game minutes).
#[derive(Component, Clone, Copy)]
pub struct WorkToDo(pub f64);

/// The game's `kMinuesToDisableWorkaholicInteraction`: how long the work's there to be done.
const WORK_OPEN_MINUTES: f64 = 120.0;
/// How long the call takes (game minutes).
pub const CHECK_UP_MINUTES: f32 = 5.0;

/// One of these finished at the computer, its outcome to be had.
#[derive(Component)]
pub struct Done(pub Special);

/// The game's `kMinimumReward`/`kMaximumReward`, `GeniusTraitChanceToSolveProblem`,
/// `kPerformanceBonusForWorkaholicSim` and the book club's `kCost`.
const PRIZE: std::ops::RangeInclusive<i64> = 50..=225;
const SOLVE_CHANCE: f64 = 0.6;
const ISSUES_PERFORMANCE: f32 = 4.0;
pub const BOOK_CLUB_COST: i64 = 250;

/// Whether one's offered: the trait's own (and teens and up), the job and the club as the
/// game's tests have them.
pub fn offers(special: Special, sim: &Sim, job: Option<&crate::careers::Job>, in_club: bool, work_to_do: bool) -> bool {
    let has = |t: Trait| sim.traits.contains(&t);
    let teen_up = sim.age == Age::Teen || sim.age.is_grown();
    match special {
        Special::SolveProblems => teen_up && has(Trait::Genius),
        Special::JobIssues => teen_up && has(Trait::Workaholic) && job.is_some() && work_to_do,
        Special::JoinBookClub => !sim.age.is_little() && has(Trait::Bookworm) && !in_club,
        Special::LeaveBookClub => in_club,
        Special::Hack | Special::Overclock => teen_up && has(Trait::ComputerWhiz),
        Special::WorkFromHome => teen_up && has(Trait::Workaholic) && job.is_some() && !work_to_do,
        _ => true,
    }
}

/// Called to check up on work (its outcome to be had).
#[derive(Component)]
pub struct CheckedUp;

/// Checked up on work by phone: half the time there's something to be getting on with at the
/// computer, for a couple of hours; else there's nothing to worry about.
fn checked_up(
    mut commands: Commands,
    callers: Query<(Entity, &Sim), With<CheckedUp>>,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    for (e, sim) in &callers {
        commands.entity(e).remove::<CheckedUp>();
        check_up(&mut commands, e, sim, clock.minutes, ui.as_deref(), &mut notes);
    }
}

fn check_up(commands: &mut Commands, e: Entity, sim: &Sim, now: f64, ui: Option<&crate::layout::UiAssets>, notes: &mut crate::interact::Notifications) {
    let mut rng = rand::rng();
    let found = rng.random_bool(0.5);
    let k = rng.random_range(1..=2);
    let key = if found { format!("WorkaholicCheckonWorkInteracitonText{k}") } else { format!("WorkaholicDoNotWorry{k}") };
    let or = if found { format!("There's a lot going on at work: {} can help sort it out on the computer.", sim.first) } else { format!("Things are under control at work. {} needn't worry!", sim.first) };
    let text = ui
        .and_then(|u| u.localize(&format!("Gameplay/Objects/Electronics/Phone/CallCheckUpOnWork:{key}")))
        .map_or(or, |t| t.replace("{0.SimName}", &sim.full_name()).replace("{0.SimFirstName}", &sim.first).replace('\u{fffd}', "-"));
    notes.push(text);
    if found {
        commands.entity(e).insert(WorkToDo(now + WORK_OPEN_MINUTES));
    }
}

/// What comes of it: the problem solved (or not), the performance, the club joined or left.
#[allow(clippy::type_complexity)]
fn done(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &Done, Option<&mut crate::careers::Job>)>,
    mut household: Option<ResMut<crate::interact::Household>>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let mut rng = rand::rng();
    let say = |key: &str, or: String, name: &str, n: i64| {
        ui.as_deref()
            .and_then(|u| u.localize(key))
            .map_or(or, |t| t.replace("{0.SimFirstName}", name).replace("{0.SimName}", name).replace("{1.Money}", &format!("§{n}")).replace("{1}", &n.to_string()))
    };
    for (e, sim, d, job) in &mut sims {
        commands.entity(e).remove::<Done>();
        match d.0 {
            Special::SolveProblems => {
                if rng.random_bool(SOLVE_CHANCE) {
                    let prize = rng.random_range(PRIZE);
                    if let Some(h) = household.as_mut() {
                        h.funds += prize;
                    }
                    let k = rng.random_range(1..=4);
                    notes.push(say(
                        &format!("Gameplay/Objects/Electronics/Computer/SolveHardProblems:SuccessMessage{k}"),
                        format!("{} solved an unsolvable problem, and won §{prize}!", sim.first),
                        &sim.first,
                        prize,
                    ));
                } else {
                    notes.push(say("Gameplay/Objects/Electronics/Computer/SolveHardProblems:InteractionFailure", format!("{} couldn't solve the problem.", sim.first), &sim.first, 0));
                }
            }
            Special::JobIssues => {
                if let Some(mut j) = job {
                    j.performance = (j.performance + ISSUES_PERFORMANCE).clamp(-100.0, 100.0);
                    commands.entity(e).remove::<WorkToDo>();
                    notes.push(say(
                        "Gameplay/Objects/Electronics/Computer/WorkOnJobRelatedProblems:PerformanceBonusTns",
                        format!("{} got some work done from home: their boss will be pleased.", sim.first),
                        &sim.first,
                        0,
                    ));
                }
            }
            Special::JoinBookClub => match household.as_mut() {
                Some(h) if h.funds >= BOOK_CLUB_COST => {
                    h.funds -= BOOK_CLUB_COST;
                    commands.entity(e).insert(BookClub);
                    notes.push(format!("{} joined the book club (§{BOOK_CLUB_COST}): a book will come in the mail every Monday.", sim.first));
                }
                _ => notes.push(format!("There isn't enough money for the book club (§{BOOK_CLUB_COST}).")),
            },
            Special::LeaveBookClub => {
                commands.entity(e).remove::<BookClub>();
                notes.push(format!("{} left the book club.", sim.first));
            }
            _ => {}
        }
    }
}

/// Every Monday, a book in the mail for each of the household in the book club: one they've not
/// got, onto the shelves.
fn book_club_mail(
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut members: Query<(&Sim, Option<&mut crate::novels::Novels>), (With<BookClub>, With<HouseholdMember>)>,
    mut last: Local<Option<u32>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut commands: Commands,
    entities: Query<Entity, (With<BookClub>, With<HouseholdMember>, Without<crate::novels::Novels>)>,
) {
    // (The game's `kBookClubDay`: Monday, first thing.)
    if clock.weekday() != 0 || *last == Some(clock.day()) {
        return;
    }
    *last = Some(clock.day());
    let Some(ui) = ui else { return };
    let mut rng = rand::rng();
    for e in &entities {
        commands.entity(e).insert(crate::novels::Novels::default());
    }
    for (sim, novels) in &mut members {
        let Some(mut novels) = novels else { continue };
        let new: Vec<&s3bake::gamedata::NovelInfo> = ui.data.novels.iter().filter(|n| novels.get(&n.key).is_none_or(|r| !r.owned)).collect();
        if let Some(book) = new.choose(&mut rng) {
            novels.own(&book.key);
            notes.push(format!("A book from the book club came in the mail for {}: {}.", sim.first, book.name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(traits: &[Trait]) -> Sim {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "T", Some(true), Age::Adult);
        s.traits = traits.to_vec();
        s
    }

    #[test]
    fn workaholics_work_from_home_out_of_work_hours_on_the_computers_tuning() {
        let job = crate::careers::Job::new(0);
        let l = job.info();
        // (A workday at the shift's middle, and the same day late at night.)
        let day = (0..7u32).find(|d| job.works_on(*d as usize)).unwrap() as f64;
        let mid = (l.start + job.hours() / 2.0) % 24.0;
        let at = |h: f32| {
            let mut c = crate::clock::GameClock::default();
            c.minutes = day * 1440.0 + h as f64 * 60.0;
            c
        };
        let w = sim(&[Trait::Workaholic]);
        assert!(work_time(&job, &at(mid)) && !may_work_from_home(&w, Some(&job), false, &at(mid)));
        let off = (l.end + 3.0) % 24.0;
        assert!(may_work_from_home(&w, Some(&job), false, &at(off)));
        assert!(!may_work_from_home(&w, Some(&job), true, &at(off)), "not with work to sort out");
        assert!(!may_work_from_home(&sim(&[Trait::Friendly]), Some(&job), false, &at(off)));
        assert!(!may_work_from_home(&w, None, false, &at(off)));
        assert_eq!(home_performance("Sims3.Gameplay.Objects.Electronics.ComputerCheap"), 3.0);
        assert_eq!(home_performance("Sims3.Gameplay.Objects.Electronics.ComputerLaptopModern"), 4.0);
        assert_eq!(home_performance("Sims3.Gameplay.Objects.Electronics.ComputerExpensive"), 5.0);
        assert!(offers(Special::Overclock, &sim(&[Trait::ComputerWhiz]), None, false, false) && !offers(Special::Overclock, &w, None, false, false));
    }

    #[test]
    fn each_trait_has_its_own() {
        assert!(offers(Special::SolveProblems, &sim(&[Trait::Genius]), None, false, false));
        assert!(!offers(Special::SolveProblems, &sim(&[]), None, false, false));
        assert!(offers(Special::JoinBookClub, &sim(&[Trait::Bookworm]), None, false, false));
        assert!(!offers(Special::JoinBookClub, &sim(&[Trait::Bookworm]), None, true, false) && offers(Special::LeaveBookClub, &sim(&[]), None, true, false));
        // (No job, nothing to work on.)
        assert!(!offers(Special::JobIssues, &sim(&[Trait::Workaholic]), None, false, true));
    }
}
