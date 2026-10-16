//! The computer's interactions for the traits, as the game's `Computer`: a Genius can Solve Hard
//! Problems (three in five times solved, for a prize of up to §1,000: `SolveHardProblems`,
//! `GeniusTraitChanceToSolveProblem`); a Workaholic can Work on Job-Related Issues (a little more
//! performance, once between days at work: `WorkOnJobRelatedIssues`); a Bookworm can Join the
//! Book Club (§200 for life, `JoinBookClub`), and a book comes in the mail for each member every
//! Monday (`Mailbox.SendBookClubBooks`), onto the household's shelves.

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
        app.add_systems(Update, (done, book_club_mail, traits_test).run_if(in_state(PlayMode::Live)));
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

/// In the book club (kept with the game).
#[derive(Component, Clone, Copy)]
pub struct BookClub;

/// Worked on job issues at home since their last day at work (the day it was).
#[derive(Component, Clone, Copy)]
pub struct WorkedIssues(pub Option<u32>);

/// One of these finished at the computer, its outcome to be had.
#[derive(Component)]
pub struct Done(pub Special);

/// The game's `kMinimumReward`/`kMaximumReward`, `GeniusTraitChanceToSolveProblem`,
/// `kPerformanceBonusForWorkaholicSim` and the book club's `kCost`.
const PRIZE: std::ops::RangeInclusive<i64> = 0..=1000;
const SOLVE_CHANCE: f64 = 0.6;
const ISSUES_PERFORMANCE: f32 = 5.0;
pub const BOOK_CLUB_COST: i64 = 200;

/// Whether one's offered: the trait's own (and teens and up), the job and the club as the
/// game's tests have them.
pub fn offers(special: Special, sim: &Sim, job: Option<&crate::careers::Job>, in_club: bool, worked: Option<&WorkedIssues>) -> bool {
    let has = |t: Trait| sim.traits.contains(&t);
    let teen_up = sim.age == Age::Teen || sim.age.is_grown();
    match special {
        Special::SolveProblems => teen_up && has(Trait::Genius),
        Special::JobIssues => teen_up && has(Trait::Workaholic) && job.is_some_and(|j| worked.is_none_or(|w| w.0 != j.last_day)),
        Special::JoinBookClub => !sim.age.is_little() && has(Trait::Bookworm) && !in_club,
        Special::LeaveBookClub => in_club,
        _ => true,
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
                    commands.entity(e).insert(WorkedIssues(j.last_day));
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
    fn each_trait_has_its_own() {
        assert!(offers(Special::SolveProblems, &sim(&[Trait::Genius]), None, false, None));
        assert!(!offers(Special::SolveProblems, &sim(&[]), None, false, None));
        assert!(offers(Special::JoinBookClub, &sim(&[Trait::Bookworm]), None, false, None));
        assert!(!offers(Special::JoinBookClub, &sim(&[Trait::Bookworm]), None, true, None) && offers(Special::LeaveBookClub, &sim(&[]), None, true, None));
        // (No job, nothing to work on.)
        assert!(!offers(Special::JobIssues, &sim(&[Trait::Workaholic]), None, false, None));
    }
}
