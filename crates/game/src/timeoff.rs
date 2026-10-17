//! Time off work by phone, as the game's `Phone.CallToTakeUnpaidTimeOff`: within four hours of a
//! shift (`kCallInSickHoursBeforeWork`) a sick Sim can Call In Sick, a paid day off, and a well
//! one Call In Fake Sick, the same, with a chance at the day's end of being found out (the
//! game's `kFakeSickDayCaughtChanceBase` 15%, more for the absent-minded, the unlucky and anyone
//! who went out, less for a Workaholic or the lucky, more each time they've called in sick: then
//! their performance down 25). Or Take Unpaid Time Off... a day, two or five (not again for 48,
//! 72 or 240 hours, `kUnpaidDaysOffCooldownInHours`), and none while they've days off still.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::careers::Job;
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{HouseholdMember, Sim};

pub struct TimeOffPlugin;

impl Plugin for TimeOffPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (called, days_off.before(crate::careers::work_schedule), found_out).run_if(in_state(PlayMode::Live)));
    }
}

/// `kCallInSickHoursBeforeWork`.
const SICK_CALL_HOURS: f32 = 4.0;
/// `kUnpaidDaysOff` and `kUnpaidDaysOffCooldownInHours`.
const UNPAID: [(u8, f64); 3] = [(1, 48.0), (2, 72.0), (5, 240.0)];
/// The fake sick day's reckoning (`kFakeSickDayCaughtChance...`).
const CAUGHT_BASE: f64 = 0.15;
const CAUGHT_TRAIT: f64 = 0.05;
const CAUGHT_LEFT_HOME: f64 = 0.1;
const CAUGHT_PER_SICK_CALL: f64 = 0.02;
const CAUGHT_SICK_CALLS_CAP: f64 = 0.2;
const CAUGHT_PERFORMANCE: f32 = -25.0;
/// How long the call takes (game minutes).
pub const CALL_MINUTES: f32 = 5.0;

/// A Sim's time off work (kept with the game).
#[derive(Component, Clone, Default, Debug, Serialize, Deserialize)]
pub struct TimeOff {
    /// Work days still to be had off, and whether with pay (sick leave).
    pub days: u8,
    pub paid: bool,
    /// A fake sick day being had (which day), and whether they've been out on it.
    pub fake_day: Option<u32>,
    pub left_home: bool,
    /// How many times they've called in sick.
    pub sick_calls: u32,
    /// Not to ask for unpaid time off again till then (game minutes).
    pub cooldown_until: f64,
}

/// What's asked for on the phone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ask {
    Sick,
    FakeSick,
    /// Unpaid days off: an index into `UNPAID`.
    Unpaid(u8),
}

impl Ask {
    pub fn label(self) -> String {
        match self {
            Ask::Sick => "Call In Sick".to_string(),
            Ask::FakeSick => "Call In Fake Sick".to_string(),
            Ask::Unpaid(i) => match UNPAID[i as usize].0 {
                1 => "1 Day".to_string(),
                n => format!("{n} Days"),
            },
        }
    }
}

/// Being sick (the game's Germy, Nauseous and Allergy Haze: the base game's Nauseous).
fn sick(moodlets: Option<&Moodlets>) -> bool {
    moodlets.is_some_and(|m| m.has(MoodletKind::Nauseous))
}

/// What the phone offers them: sick (or fake sick) within four hours of today's shift; and
/// unpaid time off when they've none to come and it's not too soon since the last.
pub fn offers(job: &Job, off: Option<&TimeOff>, moodlets: Option<&Moodlets>, weekday: usize, hour: f32, now: f64) -> (Option<Ask>, Vec<Ask>) {
    let pending = off.is_some_and(|o| o.days > 0);
    let start = job.info().start;
    let before = job.works_on(weekday) && hour < start && start - hour < SICK_CALL_HOURS && !pending;
    let sick_call = before.then(|| if sick(moodlets) { Ask::Sick } else { Ask::FakeSick });
    let unpaid = if pending || off.is_some_and(|o| now < o.cooldown_until) { Vec::new() } else { (0..UNPAID.len() as u8).map(Ask::Unpaid).collect() };
    (sick_call, unpaid)
}

/// The call made: what was asked for (its outcome to be had).
#[derive(Component)]
pub struct Called(pub Ask);

fn called(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    callers: Query<(Entity, &Sim, &Called, Option<&TimeOff>)>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    for (e, sim, c, off) in &callers {
        commands.entity(e).remove::<Called>();
        let mut off = off.cloned().unwrap_or_default();
        match c.0 {
            Ask::Sick | Ask::FakeSick => {
                off.days = 1;
                off.paid = true;
                off.sick_calls += 1;
                if c.0 == Ask::FakeSick {
                    off.fake_day = Some(clock.day());
                    off.left_home = false;
                }
                notes.push(format!("{} called in sick, and has today off work.", sim.first));
            }
            Ask::Unpaid(i) => {
                let (days, hours) = UNPAID[i as usize];
                off.days = days;
                off.paid = false;
                off.cooldown_until = clock.minutes + hours * 60.0;
                notes.push(format!("Congratulations! {}'s request for unpaid time off has been approved.", sim.full_name()));
            }
        }
        info!("time off: {} has {} day(s) off ({})", sim.first, off.days, if off.paid { "paid" } else { "unpaid" });
        commands.entity(e).insert(off);
    }
}

/// A work day had off: not gone to, not missed; paid for, when it's sick leave.
fn days_off(
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(&Sim, &mut Job, &mut TimeOff, Has<HouseholdMember>, Option<&crate::visit::OnLot>)>,
    mut household: Option<ResMut<crate::interact::Household>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let (day, weekday, hour) = (clock.day(), clock.weekday(), clock.hour_f());
    for (sim, mut job, mut off, member, on_lot) in &mut sims {
        // (Out and about on a fake sick day.)
        if off.fake_day == Some(day) && on_lot.is_some() {
            off.left_home = true;
        }
        if off.days == 0 || !job.works_on(weekday) || job.last_day == Some(day) || hour < job.info().start - 1.0 {
            continue;
        }
        job.last_day = Some(day);
        off.days -= 1;
        if !member {
            continue;
        }
        if off.paid {
            let pay = (job.info().hourly as f32 * job.hours()) as i64;
            if let Some(h) = household.as_mut() {
                h.funds += pay;
            }
            notes.push(format!("{} has been paid §{pay} for their sick day.", sim.first));
        } else {
            notes.push(format!("{} has the day off work (unpaid; {} more to come).", sim.first, off.days));
        }
    }
}

/// The fake sick day's end: found out, maybe.
fn found_out(
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(&Sim, &mut Job, &mut TimeOff)>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let (day, hour) = (clock.day(), clock.hour_f());
    for (sim, mut job, mut off) in &mut sims {
        let Some(fake) = off.fake_day else { continue };
        let end = job.info().start + job.hours();
        if fake == day && hour < end.min(23.9) {
            continue;
        }
        off.fake_day = None;
        if rand::rng().random_bool(caught_chance(sim, &off)) {
            job.performance = (job.performance + CAUGHT_PERFORMANCE).max(-100.0);
            notes.push(format!(
                "Uh-oh. Looks like one of {}'s co-workers might have caught them out and about on their \"sick day.\" This is not going to go over well with the boss.",
                sim.first
            ));
        }
        off.left_home = false;
    }
}

/// The chance a fake sick day's found out.
pub fn caught_chance(sim: &Sim, off: &TimeOff) -> f64 {
    let has = |t: Trait| sim.traits.contains(&t);
    let mut p = CAUGHT_BASE;
    if [Trait::Absentminded, Trait::Unlucky].into_iter().any(has) {
        p += CAUGHT_TRAIT;
    }
    if [Trait::Workaholic, Trait::Lucky].into_iter().any(has) {
        p -= CAUGHT_TRAIT;
    }
    if off.left_home {
        p += CAUGHT_LEFT_HOME;
    }
    p += (off.sick_calls as f64 * CAUGHT_PER_SICK_CALL).min(CAUGHT_SICK_CALLS_CAP);
    p.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn going_out_and_calling_in_often_get_found_out() {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "T", Some(true), crate::sim::Age::Adult);
        s.traits = vec![Trait::Workaholic];
        let mut off = TimeOff { sick_calls: 1, ..Default::default() };
        assert!((caught_chance(&s, &off) - 0.12).abs() < 1e-9);
        off.left_home = true;
        off.sick_calls = 20;
        assert!((caught_chance(&s, &off) - 0.4).abs() < 1e-9);
    }
}
