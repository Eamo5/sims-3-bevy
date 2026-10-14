//! The seasons' holidays, as the game's (its `HolidayManager`, Seasons): Love Day in spring,
//! Leisure Day in summer, Spooky Day in fall and Snowflake Day in winter, two days before a
//! season ends (its last day, for a season shorter than a week). The day before, word of it;
//! on the day at five in the morning, its sting, its notice and its moodlet for everyone, and no
//! school or work (a paid day off).

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::life::MoodletKind;
use crate::sim::Sim;
use crate::weather::{Season, Weather};

pub struct HolidaysPlugin;

impl Plugin for HolidaysPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, holidays.run_if(in_state(PlayMode::Live)));
    }
}

/// Snowflake Day's holiday bonus at work (every career's `HolidayBonusChance` and
/// `HolidayBonusSalaryPercentage`, and the game's interpolations): how likely, its days' pay to
/// start with, what good work adds (a quarter of a day's pay at performance 25, two at 100), what
/// long service adds (taken as the most, two days: hiring dates aren't kept) and what never
/// calling in sick adds (two days), and above how many days' pay it's average, and large.
const BONUS_CHANCE: f64 = 0.75;
const BONUS_DAYS: f32 = 2.0;
const BONUS_SERVICE_DAYS: f32 = 2.0;
const BONUS_HEALTHY_DAYS: f32 = 2.0;
const BONUS_AVERAGE: f32 = 2.0;
const BONUS_LARGE: f32 = 4.5;

/// A working Sim's Snowflake Day bonus: how many days' pay, as the game reckons it.
pub fn bonus_days(performance: f32) -> f32 {
    let perf = 0.25 + (performance.clamp(25.0, 100.0) - 25.0) / 75.0 * 1.75;
    BONUS_DAYS + perf + BONUS_SERVICE_DAYS + BONUS_HEALTHY_DAYS
}

/// How much warmer Sims take to socials on Snowflake Day (and to romance on Love Day): the game's
/// `kWinterHolidaySocialLikingBonus`, `kSpringHolidaySocialLikingBonus`.
pub const HOLIDAY_LIKING: f32 = 10.0;

/// The hour a holiday's notice and moodlet come (the game's `kTimeOfDayToShowStartHoliday`).
const START_HOUR: f32 = 5.0;

/// The day of the current season's holiday (the season of `day`), and the season.
pub fn holiday_of(w: &Weather, day: u32) -> (u32, Season) {
    let days = w.season_days.max(1);
    let k = (day + w.first_day) / days;
    let last = ((k + 1) * days).saturating_sub(w.first_day + 1);
    let holiday = if days >= 7 { last.saturating_sub(2) } else { last };
    (holiday, w.season(day))
}

/// Whether a day is its season's holiday.
pub fn is_holiday(w: &Weather, day: u32) -> bool {
    holiday_of(w, day).0 == day
}

/// The game's name for a season's holiday, its notices' key, its moodlet and its sting.
fn holiday_info(season: Season) -> (&'static str, &'static str, MoodletKind, &'static str) {
    match season {
        Season::Spring => ("Love Day", "SpringHoliday", MoodletKind::SpringFling, "sting_holiday_spring"),
        Season::Summer => ("Leisure Day", "SummerHoliday", MoodletKind::SummerFun, "sting_holiday_summer"),
        Season::Fall => ("Spooky Day", "FallHoliday", MoodletKind::SpookyTime, "sting_holiday_fall"),
        Season::Winter => ("Snowflake Day", "WinterHoliday", MoodletKind::HolidayCheer, "sting_holiday_winter"),
    }
}

/// The name of the holiday today, if it is one.
pub fn holiday_today(w: &Weather, day: u32) -> Option<&'static str> {
    let (d, season) = holiday_of(w, day);
    (d == day).then(|| holiday_info(season).0)
}

/// Word of a holiday the day before; on the day, its notice, sting and moodlet.
#[allow(clippy::too_many_arguments)]
fn holidays(
    clock: Res<GameClock>,
    weather: Option<Res<Weather>>,
    layouts: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut sound: MessageWriter<crate::sound::PlaySound>,
    mut sims: Query<(&Sim, &mut crate::life::Moodlets, Option<&crate::careers::Job>, Has<crate::sim::HouseholdMember>)>,
    mut told: Local<(Option<u32>, Option<u32>)>,
    mut household: Option<ResMut<crate::interact::Household>>,
) {
    let Some(w) = weather else { return };
    let day = clock.day();
    let (holiday, season) = holiday_of(&w, day);
    let (name, key, moodlet, sting) = holiday_info(season);
    let say = |kind: &str, fallback: String| layouts.as_ref().and_then(|l| l.localize(&format!("Gameplay/Excel/Notifications/Notifications:{key}{kind}"))).unwrap_or(fallback);
    // (The day before.)
    if holiday == day + 1 && told.0 != Some(holiday) {
        told.0 = Some(holiday);
        notes.push(say("Warning", format!("{name} is coming up!")));
    }
    // (On the day, from five in the morning.)
    if holiday == day && clock.hour_f() >= START_HOUR && told.1 != Some(holiday) {
        told.1 = Some(holiday);
        notes.push(say("Starting", format!("{name} is here!")));
        sound.write(crate::sound::PlaySound::ui(sting));
        let mut rng = rand::rng();
        for (sim, mut m, job, member) in &mut sims {
            m.add(moodlet, clock.minutes);
            // (Snowflake Day: the working household's holiday bonus, or word there's none.)
            let Some(job) = job.filter(|_| member && season == Season::Winter) else { continue };
            use rand::Rng;
            let bonus = |key: &str, amount: i64| {
                layouts
                    .as_ref()
                    .and_then(|l| l.localize(&format!("Gameplay/Careers/HolidayBonus:{key}")))
                    .map(|t| t.replace("{0.SimFirstName}", &sim.first).replace("{1.Money}", &format!("§{amount}")))
            };
            if !rng.random_bool(BONUS_CHANCE) {
                notes.push(bonus("NoBonusTNS", 0).unwrap_or_else(|| format!("{} didn't get a holiday bonus this year.", sim.first)));
                continue;
            }
            let days = bonus_days(job.performance);
            let amount = (days * job.hours() * job.info().hourly as f32) as i64;
            if let Some(h) = household.as_mut() {
                h.funds += amount;
            }
            let key = if days < BONUS_AVERAGE { "SmallBonusTNS" } else if days < BONUS_LARGE { "AverageBonusTNS" } else { "LargeBonusTNS" };
            notes.push(bonus(key, amount).unwrap_or_else(|| format!("{} got a holiday bonus of §{amount}.", sim.first)));
            sound.write(crate::sound::PlaySound::ui("sting_winter_holiday_bonus"));
        }
        info!("holiday: {name}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holiday_bonuses_go_by_work_done() {
        // (Never sick and long employed: six days' pay and a quarter, to eight at the best work.)
        assert!((bonus_days(0.0) - 6.25).abs() < 1e-4);
        assert!((bonus_days(100.0) - 8.0).abs() < 1e-4);
    }

    #[test]
    fn a_holiday_comes_two_days_before_the_seasons_end() {
        let mut w = Weather::default();
        w.season_days = 7;
        w.first_day = 0;
        // (Days 0..6 the first season: its holiday the fifth, day 4.)
        assert_eq!(holiday_of(&w, 0).0, 4);
        assert_eq!(holiday_of(&w, 6).0, 4);
        assert!(is_holiday(&w, 4) && !is_holiday(&w, 5));
        assert_eq!(holiday_of(&w, 7).0, 11, "the next season's");
        // (A short season: its last day.)
        w.season_days = 5;
        assert_eq!(holiday_of(&w, 0).0, 4);
    }
}
