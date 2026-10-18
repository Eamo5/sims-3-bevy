//! The careers' own performance metrics, as the game's (`Business.MetricMeetingsHeld`,
//! `Music.MetricConcertsPerformed`), and what moves them:
//!
//! - Meetings held (Business, levels 6 and up): the last seven days' and today's. "Hold Meetings"
//!   at work holds one every hour and a half of it (`kSimHoursToCallMeetingWothwhile`); "Hold
//!   Meeting" at the business office, between eight and six and not in too foul a mood (-50,
//!   `kMinMoodToHoldMeeting`), takes an hour and a half there and pays §750
//!   (`kMoneyForHoldingMeetings`).
//! - Performances (Music, the Electric Rock path, 6 and up): every concert of the career.
//!   "Perform" at work readies one every four hours (`kPrepareForConcertPerSimHour` 25 an hour);
//!   "Perform Concert" at the theatre (two hours) or the stadium (four), from three in the
//!   afternoon till eleven, for anyone on the Electric Rock path: one more, and at the top of the
//!   career, paid (§600 or §1,600, give or take half: `kPerformConcertPayoff`).
//! - Funding (Political, levels 5 and up: `MetricCampaignMoney`): what's been raised since the
//!   last promotion or demotion. "Ask for Campaign Donation" of someone not of the household,
//!   taken well, brings in a fortieth of their household's worth, more the more the asker likes
//!   them (`kDonationPercent`, `kCampaignDonationLTRMultiplier`), at most §10,000 (`kDonationMax`),
//!   with the game's words.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::careers::Job;
use crate::interact::{Household, Notifications};
use crate::loading::WorldInfo;
use crate::rabbitholes::Activity;
use crate::sim::Sim;

pub struct CareerMetricsPlugin;

impl Plugin for CareerMetricsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (gig_test, gigs_done).run_if(in_state(PlayMode::Live)));
    }
}

/// What a career keeps count of for its metrics.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    /// Meetings held, by day (the last seven days' and today's count: the game's
    /// `mMeetingsOnLastNDays` and `MeetingsHeldToday`), and the hours into the next one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meetings: Vec<(u32, u32)>,
    #[serde(default)]
    pub meeting_hours: f32,
    /// Concerts performed in the career, and the next one's readiness (out of 100).
    #[serde(default)]
    pub concerts: u32,
    #[serde(default)]
    pub concert_prep: f32,
    /// Campaign money raised at this level (Political: `CampaignMoneyRaised`).
    #[serde(default)]
    pub campaign: i64,
}

/// How many days back meetings count (`kWindowForRememberingMeetings`), and today.
const MEETING_DAYS: u32 = 7;
/// Hours of the Hold Meetings tone a meeting takes.
const MEETING_HOURS: f32 = 1.5;
/// A concert's readiness an hour of the Perform tone.
const CONCERT_PREP_PER_HOUR: f32 = 25.0;

impl Tally {
    pub fn is_empty(&self) -> bool {
        *self == Tally::default()
    }

    /// Meetings held over the window to `today`.
    pub fn meetings_held(&self, today: u32) -> u32 {
        self.meetings.iter().filter(|(d, _)| d + MEETING_DAYS >= today && *d <= today).map(|(_, n)| n).sum()
    }

    /// A meeting held today (those too old for the window forgotten).
    pub fn hold_meeting(&mut self, today: u32) {
        self.meetings.retain(|(d, _)| d + MEETING_DAYS >= today);
        match self.meetings.iter_mut().find(|(d, _)| *d == today) {
            Some((_, n)) => *n += 1,
            None => self.meetings.push((today, 1)),
        }
    }

    /// A metric's value from the count, where it's one of these (the game's `FindMetricValue`).
    pub fn value(&self, kind: &str, today: u32) -> Option<f32> {
        match kind {
            "HoldMeetings" => Some(self.meetings_held(today) as f32),
            "ConcertsPerformed" => Some(self.concerts as f32),
            "CampaignMoney" => Some(self.campaign as f32),
            _ => None,
        }
    }

    /// A new level (a promotion, a demotion, a raise at the top): what's counted for it alone
    /// starts again (the game's `SetLevel`).
    pub fn new_level(&mut self) {
        self.campaign = 0;
    }

    /// Hours of the Hold Meetings tone: the meetings they come to.
    pub fn meeting_tone(&mut self, hours: f32, today: u32) -> u32 {
        self.meeting_hours += hours;
        let mut held = 0;
        while self.meeting_hours >= MEETING_HOURS {
            self.meeting_hours -= MEETING_HOURS;
            self.hold_meeting(today);
            held += 1;
        }
        held
    }

    /// Hours of the Perform tone: whether a concert's come of them.
    pub fn perform_tone(&mut self, hours: f32) -> bool {
        self.concert_prep += hours * CONCERT_PREP_PER_HOUR;
        if self.concert_prep >= 100.0 {
            self.concert_prep = 0.0;
            self.concerts += 1;
            return true;
        }
        false
    }
}

/// A campaign donation: a share of the giver's household's worth, the more the asker's liked
/// (liking -100 to 100: none to twice it), at most the game's most.
pub fn donation(worth: i64, liking: f32) -> i64 {
    (worth.max(0) as f32 * DONATION_SHARE * (1.0 + liking.clamp(-100.0, 100.0) / 100.0)).round().min(DONATION_MAX as f32) as i64
}

/// The donation's share and most (the Political tuning's), and what a household's worth where
/// the town's Sims haven't the game's own.
const DONATION_SHARE: f32 = 0.025;
const DONATION_MAX: i64 = 10_000;
pub const TOWNIE_WORTH: i64 = 20_000;

/// Who's asked for campaign money: a Political Sim of level 5 or more (the game's
/// `kCampaignDonationLevel`), the topic their socials go by.
pub fn campaigning(career: Option<usize>, level: usize) -> bool {
    career.and_then(|c| crate::careers::careers().get(c)).is_some_and(|t| t.key == "Political") && level + 1 >= 5
}

/// A donation given (to `asker`, by `giver`): into the campaign fund, with the game's words.
pub fn donate(w: &mut World, asker: Entity, giver: Entity, liking: f32) {
    let Some(id) = w.get::<Sim>(giver).map(|s| s.id) else { return };
    let giver_name = w.get::<Sim>(giver).map(|s| s.full_name()).unwrap_or_default();
    let worth = w.get_resource::<crate::premade::TownPremades>().and_then(|t| t.0.households.iter().find(|h| h.members.iter().any(|m| m.id == id)).map(|h| h.funds)).unwrap_or(TOWNIE_WORTH);
    let n = donation(worth, liking);
    let Some(first) = w.get::<Sim>(asker).map(|s| s.first.clone()) else { return };
    let Some(mut job) = w.get_mut::<Job>(asker) else { return };
    job.tally.campaign += n;
    let total = job.tally.campaign;
    info!("{giver_name} donated §{n} to {first}'s campaign (§{total})");
    let text = w.get_resource::<crate::layout::UiAssets>().and_then(|u| u.localize("Gameplay/Careers/Political:DonateText")).map_or_else(
        || format!("{giver_name} just donated §{n} to {first}'s campaign fund. The fund now has a total of §{total}."),
        |t| t.replace("{0.SimName}", &giver_name).replace("{1.Money}", &format!("§{n}")).replace("{2.SimFirstName}", &first).replace("{3.Money}", &format!("§{total}")),
    );
    w.resource_mut::<Notifications>().push(text);
}

/// The game's words for a concert readied at work, a man's and a woman's.
pub const CONCERT_PERFORMED: (u64, u64) = (0x5e79_5cd9_0151_03cd, 0xbd8a_ddb4_39c5_4188);

/// The visits: a meeting at the business office; a concert at the theatre or the stadium.
pub const MEETING_TASK: usize = 7100;
pub const THEATRE_CONCERT_TASK: usize = 7101;
pub const STADIUM_CONCERT_TASK: usize = 7102;

const fn gig(name: &'static str, minutes: f32) -> Activity {
    Activity { name, minutes, cost: 0, per_hour: [0.0; 6], skill: None, open: 0.0, close: 24.0 }
}
pub static HOLD_MEETING: Activity = gig("Hold Meeting", 90.0);
pub static THEATRE_CONCERT: Activity = gig("Perform Concert", 120.0);
pub static STADIUM_CONCERT: Activity = gig("Perform Concert", 240.0);

/// When meetings can be held (`kEarliestTimeToHoldMeeting`, `kLatestTimeToHoldMeeting`), the mood
/// they take, and what one pays.
const MEETING_HOURS_OPEN: (f32, f32) = (8.0, 18.0);
const MEETING_MOOD: f32 = -50.0;
const MEETING_PAY: i64 = 750;
/// When concerts can be put on, the level they're paid at, and the pay (theatre, stadium) and
/// its spread.
const CONCERT_HOURS: (f32, f32) = (15.0, 23.0);
const CONCERT_PAID_LEVEL: usize = 10;
const CONCERT_PAY: (f32, f32) = (600.0, 1600.0);
const CONCERT_PAY_SPREAD: f32 = 0.5;

/// The rabbit hole on a lot, by its class ("Theatre", "Stadium",
/// "BusinessAndJournalismRabbitHole").
fn hole_class(world: &WorldInfo, lot: usize) -> Option<&str> {
    world.buildings.get(&lot)?.objects.iter().find_map(|o| o.script.split("RabbitHoles.").nth(1))
}

/// What a Sim's job lets them do here: hold a meeting at the business office, put on a concert
/// at the theatre or stadium.
pub fn options(world: &WorldInfo, lot: usize, job: Option<&Job>, hour: f32, mood: f32) -> Vec<(String, usize)> {
    let (Some(job), Some(class)) = (job, hole_class(world, lot)) else { return Vec::new() };
    let mut out = Vec::new();
    let has = |kind: &str| job.info().metrics.iter().any(|m| m.kind == kind);
    if class == "BusinessAndJournalismRabbitHole" && has("HoldMeetings") && (MEETING_HOURS_OPEN.0..=MEETING_HOURS_OPEN.1).contains(&hour) && mood >= MEETING_MOOD {
        out.push((HOLD_MEETING.name.to_string(), MEETING_TASK));
    }
    let rocker = job.career().key == "Music" && job.path().branch == "ElectricRock";
    if rocker && (CONCERT_HOURS.0..CONCERT_HOURS.1).contains(&hour) {
        match class {
            "Theatre" => out.push((THEATRE_CONCERT.name.to_string(), THEATRE_CONCERT_TASK)),
            "Stadium" => out.push((STADIUM_CONCERT.name.to_string(), STADIUM_CONCERT_TASK)),
            _ => {}
        }
    }
    out
}

/// A visit's gig, from its task.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum Gig {
    Meeting,
    TheatreConcert,
    StadiumConcert,
}

impl Gig {
    pub fn of(task: usize) -> Option<Self> {
        Some(match task {
            MEETING_TASK => Self::Meeting,
            THEATRE_CONCERT_TASK => Self::TheatreConcert,
            STADIUM_CONCERT_TASK => Self::StadiumConcert,
            _ => return None,
        })
    }

    pub fn activity(self) -> &'static Activity {
        match self {
            Self::Meeting => &HOLD_MEETING,
            Self::TheatreConcert => &THEATRE_CONCERT,
            Self::StadiumConcert => &STADIUM_CONCERT,
        }
    }
}

pub fn is_gig(a: &Activity) -> bool {
    [&HOLD_MEETING, &THEATRE_CONCERT, &STADIUM_CONCERT].iter().any(|g| std::ptr::eq(a, *g))
}

/// Back from it.
#[derive(Component)]
pub struct GigDone;

/// GIG=<lot name or rabbit hole class part>:<option> (tests): ten seconds in, the selected Sim off to that gig for
/// their job (set with CAREER_REWARD=); WORK_TONE=<tone> sets how they work.
#[allow(clippy::type_complexity)]
fn gig_test(
    time: Res<Time>,
    mut sel: Query<(&mut Job, &mut crate::interact::ActionQueue, &crate::life::Mood), With<crate::sim::Selected>>,
    world: Res<crate::loading::CurrentWorld>,
    clock: Res<crate::clock::GameClock>,
    mut done: Local<bool>,
) {
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    let Ok((mut job, mut q, mood)) = sel.single_mut() else { return };
    *done = true;
    if let Ok(t) = std::env::var("WORK_TONE") {
        job.tone = crate::careers::WorkTone::from_name(&t);
        info!("gig test: working {:?}", job.tone);
    }
    let Some((place, want)) = std::env::var("GIG").ok().and_then(|v| v.split_once(':').map(|(p, w)| (p.to_ascii_lowercase(), w.to_string()))) else { return };
    let found = (0..world.data.lots.len())
        .filter(|&l| world.data.lots[l].internal_name.to_ascii_lowercase().contains(&place) || hole_class(&world.data, l).is_some_and(|c| c.to_ascii_lowercase().contains(&place)))
        .find_map(|l| options(&world.data, l, Some(&job), clock.hour_f(), mood.0).into_iter().find(|(label, _)| label.starts_with(&want)).map(|o| (l, o)));
    match found {
        Some((lot, (label, task))) => {
            info!("gig test: {label} at lot {lot}");
            q.0.clear();
            q.push_player(crate::interact::Action::new(label, crate::interact::ActionKind::Visit { lot, activity: task }, false));
        }
        None => warn!("gig test: no {want} at {place}"),
    }
}

/// The gig counted on the way out: a meeting held and paid for, a concert performed (and paid,
/// at the top).
fn gigs_done(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &Gig, Option<&mut Job>), With<GigDone>>,
    mut household: Option<ResMut<Household>>,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    for (e, sim, gig, job) in &mut sims {
        commands.entity(e).remove::<(Gig, GigDone)>();
        let Some(mut job) = job else { continue };
        let say = |key: &str| ui.as_deref().and_then(|u| u.localize(key));
        match gig {
            Gig::Meeting => {
                job.tally.hold_meeting(clock.day());
                if let Some(h) = household.as_mut() {
                    h.funds += MEETING_PAY;
                }
                info!("{} held a meeting ({} this week)", sim.first, job.tally.meetings_held(clock.day()));
                notes.push(
                    say("Gameplay/Careers/Business/HoldMeetingInAtRabbitHole:MoneyEarned")
                        .map_or_else(|| format!("{} just earned §{MEETING_PAY}.", sim.full_name()), |t| t.replace("{0.SimName}", &sim.full_name()).replace("{1.Money}", &format!("§{MEETING_PAY}"))),
                );
            }
            Gig::TheatreConcert | Gig::StadiumConcert => {
                job.tally.concerts += 1;
                info!("{} performed a concert ({} in all)", sim.first, job.tally.concerts);
                if job.level + 1 >= CONCERT_PAID_LEVEL {
                    let base = if *gig == Gig::StadiumConcert { CONCERT_PAY.1 } else { CONCERT_PAY.0 };
                    let pay = (base * rand::rng().random_range(1.0 - CONCERT_PAY_SPREAD..=1.0 + CONCERT_PAY_SPREAD)).round() as i64;
                    if let Some(h) = household.as_mut() {
                        h.funds += pay;
                    }
                    notes.push(
                        say("Gameplay/Objects/RabbitHoles/ShowVenue/PerformConcert:ConcertPay")
                            .map_or_else(|| format!("Awesome Concert! {} just got paid §{pay} big bucks for that performance!", sim.first), |t| t.replace("{0.SimFirstName}", &sim.first).replace("{1.Money}", &format!("§{pay}"))),
                    );
                    play.write(crate::sound::PlaySound::ui("sting_career_positive"));
                } else {
                    notes.push(concert_words(ui.as_deref(), sim));
                }
            }
        }
    }
}

/// The game's words for a concert performed.
pub fn concert_words(ui: Option<&crate::layout::UiAssets>, sim: &Sim) -> String {
    ui.and_then(|u| u.localize_key(if sim.female { CONCERT_PERFORMED.1 } else { CONCERT_PERFORMED.0 }))
        .map_or_else(|| format!("{} has completed a concert! Their total performances have gone up by 1.", sim.first), |t| t.replace("{0.SimFirstName}", &sim.first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meetings_count_over_the_week() {
        let mut t = Tally::default();
        t.hold_meeting(10);
        t.hold_meeting(10);
        t.hold_meeting(14);
        assert_eq!(t.meetings_held(14), 3);
        assert_eq!(t.meetings_held(17), 3, "a week back and today");
        assert_eq!(t.meetings_held(18), 1, "the tenth's gone");
        assert_eq!(t.meeting_tone(1.0, 18), 0);
        assert_eq!(t.meeting_tone(2.5, 18), 2);
        assert_eq!(t.meetings_held(18), 3);
        assert_eq!(t.value("HoldMeetings", 18), Some(3.0));
    }

    #[test]
    fn donations_by_worth_and_liking() {
        assert_eq!(donation(100_000, 0.0), 2500);
        assert_eq!(donation(100_000, 100.0), 5000);
        assert_eq!(donation(100_000, -100.0), 0);
        assert_eq!(donation(10_000_000, 50.0), 10_000, "at most the game's most");
        let mut t = Tally { campaign: 4000, concerts: 3, ..Default::default() };
        t.new_level();
        assert_eq!((t.campaign, t.concerts), (0, 3), "concerts are the career's");
    }

    #[test]
    fn a_concert_every_four_hours_of_performing() {
        let mut t = Tally::default();
        assert!(!t.perform_tone(3.0));
        assert!(t.perform_tone(1.0));
        assert_eq!((t.concerts, t.concert_prep), (1, 0.0));
        assert_eq!(t.value("ConcertsPerformed", 0), Some(1.0));
        assert_eq!(t.value("PoliceReports", 0), None);
    }
}
