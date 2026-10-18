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
//! - Game record (Athletic, levels 5 and up: `ProSports.MetricWinLossRecord`): the season's wins
//!   less its losses, a season eight games. Game days are Sundays and Wednesdays (no work
//!   otherwise those days), three in the afternoon till six at the stadium ("Play In Stadium",
//!   the game's `PlayGame`), the morning's notice and an hour's warning, off on their own at two.
//!   The score's played out five minutes at a time (half the time nothing; else a point to the
//!   home side as often as the athlete's chance: 0.4, a quarter of their mood, a third and more
//!   of their preparation: `kGameBaseWinChance`...), a tie settled by one more. A win: Winner,
//!   performance +10, ten hours' pay; a loss: -5, an hour's. A game missed is all but lost
//!   (`kGameMissingWinChance` 0.05). "Prepare For Game" at work readies them (34 an hour).
//! - Research (Medical, levels 4 and up: `Medical.MetricMedicalJournals`): the medical journals
//!   read since the last promotion or demotion. Each work day brings that day's edition home in
//!   the doctor's inventory ("Darcy's Medical Journal: Organ Donation: Volume 3", the game's
//!   `MedicalJournal` for their level), the last one gone; "Read" there takes an hour and a half
//!   (ninety pages, a page a minute; a Bookworm two), and counts once an edition.
//! - Reports (Law Enforcement, levels 4 and up: `LawEnforcement.MetricReportsWritten`) and
//!   stories (Journalism, 3 to 6 and 8 to 10: `MetricStoriesAndReviews`): the last seven days'
//!   and today's. "Question" (the policeman's) or "Interview" (the journalist's), the game's
//!   socials for anyone not of the household not already written up, taken well, makes them a
//!   subject and teaches their traits. On the computer, "Report On... ›" a subject (the game's
//!   `WriteReport`: an hour and a half, `kCompletionRateReport`) or "Write Positive/Negative Story
//!   On... ›" (two hours); "Continue..." and "Scrap..." one begun. A report on a criminal or a
//!   Kleptomaniac is +15 performance; a negative story +20, the subject liking the writer 30 less,
//!   a positive one 10 more; the game's words. Promotions and demotions start the subjects
//!   afresh. Rummaging through a family's trash at their home turns up information on one of
//!   them (the policeman's subject; the journalist's, for a negative story only).
//! - Reviews (Journalism, the same count): "Review Movie", "Review Game", "Review Restaurant"...
//!   at the theatre, stadium, bistro or diner, free, for a place not reviewed in the last week
//!   (`CanReviewRabbitHole`); seen through, it's there to write up at the computer, "Write
//!   Positive/Negative Review On... ›" (two hours). A negative one is +20 performance and the
//!   place bans them a week (`BadReviewWrittenOnRabbitHole`); the game's words.

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
        app.add_systems(Update, (gig_test, gigs_done, game_days, write_on, report_test, reviews_seen).run_if(in_state(PlayMode::Live)));
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
    /// The season's games, wins and losses, the career's, the next game's readiness (out of
    /// 100) and the day of the last game (Athletic).
    #[serde(default)]
    pub games: u32,
    #[serde(default)]
    pub wins: u32,
    #[serde(default)]
    pub losses: u32,
    #[serde(default)]
    pub total_wins: u32,
    #[serde(default)]
    pub total_losses: u32,
    #[serde(default)]
    pub game_prep: f32,
    #[serde(default)]
    pub last_game: Option<u32>,
    /// The medical journal brought home (its day), the last one read, and those read at this
    /// level (Medical: `mNumMedicalJournalsRead`).
    #[serde(default)]
    pub journal: Option<u32>,
    #[serde(default)]
    pub journal_read: Option<u32>,
    #[serde(default)]
    pub journals: u32,
    /// Those a policeman or journalist has questioned (and, a journalist, dug into the trash of),
    /// by id and name; those written up at this level; the report or story under way; and how
    /// many were finished, by day (`SimsInterviewed`, `CompletedStorySubjects`...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subjects: Vec<(u64, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub written: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<Report>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<(u32, u32)>,
    /// What a journalist dug out of the trash on (for negative stories only), the shows, games
    /// and meals they've seen to review, and where they're banned since a bad review (and when).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trash: Vec<(u64, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviews: Vec<Review>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bans: Vec<(usize, u32)>,
    /// The careers' own tones under way: a policeman's case (minutes of it left), a criminal's
    /// side jobs (hours at them, and the next one's), a scientist's experiment and a
    /// politician's proposal (hours at them).
    #[serde(default)]
    pub case_left: f32,
    #[serde(default)]
    pub side_hours: f32,
    #[serde(default)]
    pub side_due: f32,
    #[serde(default)]
    pub experiment_hours: f32,
    #[serde(default)]
    pub risk_hours: f32,
}

/// What came of a career's own tone: words, money, and the boss's and coworkers' liking.
#[derive(Debug, Default, PartialEq)]
pub struct ToneCame {
    pub words: String,
    pub money: i64,
    pub boss: f32,
    pub coworkers: f32,
}

/// A policeman's case: minutes of the tone it takes (`kMinimumMinutesToComplete`...), the
/// performance a conviction brings, and the department's liking lost on failure.
const CASE_MINUTES: std::ops::RangeInclusive<f32> = 600.0..=1080.0;
const CASE_SUCCESS: f32 = 75.0;
const CASE_FAILURE_LIKING: f32 = -5.0;
/// A criminal's side jobs: hours at them each (give or take a quarter), and the pay, at level 1
/// and more a level above it (give or take a quarter).
const SIDE_JOB_HOURS: f32 = 8.0;
const SIDE_JOB_PAY: (f32, f32) = (80.0, 80.0);
/// A scientist's experiment: hours at it, its chances (a performance boost, a promotion) and
/// the boost.
const EXPERIMENT_HOURS: f32 = 12.0;
const EXPERIMENT_CHANCES: (f32, f32) = (0.8, 0.9);
const EXPERIMENT_PERFORMANCE: f32 = 20.0;
/// A politician's proposal: hours at it (`kRiskDuration`), and the liking it wins or costs.
const RISK_HOURS: f32 = 16.0;
const RISK_BOSS: (f32, f32) = (40.0, -30.0);
const RISK_COWORKERS: f32 = -40.0;

/// The chance a policeman's case sticks, by their Logic (the game's `Skill.GetSuccessChance`
/// over 0 to 7, in whole steps: 1 in 7, a third, a half, then sure).
pub fn case_chance(logic: u32) -> f32 {
    if logic == 0 { 0.0 } else { (1.0 / (7 / logic.min(7)) as f32).min(1.0) }
}

/// Time at a career's own tone: what comes of it, when it does (the tone over for a case or a
/// proposal).
pub fn tone_time(job: &mut Job, sim: &Sim, hours: f32, logic: u32, ui: Option<&crate::layout::UiAssets>, rng: &mut impl Rng) -> Option<ToneCame> {
    // (A woman's words where the game has its own: the key's `_Female`, as its `LocalizeString`.)
    let say = |key: &str, or: String| {
        let female = sim.female.then(|| ui.and_then(|u| u.localize(&format!("{key}_Female")))).flatten();
        female.or_else(|| ui.and_then(|u| u.localize(key))).map_or(or, |t| crate::layout::gendered(&t, sim.female).replace("{0.SimFirstName}", &sim.first).replace("{0.SimName}", &sim.full_name()))
    };
    let t = &mut job.tally;
    match job.tone {
        crate::careers::WorkTone::BuildCase => {
            if t.case_left <= 0.0 {
                t.case_left = rng.random_range(CASE_MINUTES);
            }
            t.case_left -= hours * 60.0;
            if t.case_left > 0.0 {
                return None;
            }
            t.case_left = 0.0;
            job.tone = crate::careers::WorkTone::Normal;
            if rng.random::<f32>() < case_chance(logic) {
                job.performance = (job.performance + CASE_SUCCESS).min(100.0);
                return Some(ToneCame { words: format!("{}'s independent case stuck: the suspect's been convicted, and the department's impressed.", sim.first), ..Default::default() });
            }
            Some(ToneCame {
                words: say("Gameplay/Careers/LawEnforcement/BuildCaseTone:BuildCaseSuccessText", format!("{}'s case didn't stick.", sim.first)),
                coworkers: CASE_FAILURE_LIKING,
                boss: CASE_FAILURE_LIKING,
                ..Default::default()
            })
        }
        crate::careers::WorkTone::SideJob => {
            if t.side_due <= 0.0 {
                t.side_due = SIDE_JOB_HOURS * rng.random_range(0.75..=1.25);
            }
            t.side_hours += hours;
            if t.side_hours < t.side_due {
                return None;
            }
            t.side_hours = 0.0;
            t.side_due = SIDE_JOB_HOURS * rng.random_range(0.75..=1.25);
            let pay = ((SIDE_JOB_PAY.0 + SIDE_JOB_PAY.1 * job.level as f32) * rng.random_range(0.75..=1.25)) as i64;
            let pay = pay - pay % 5;
            Some(ToneCame {
                words: say("Gameplay/Careers/Criminal/DoASideJob:SideJobBonusText", format!("{} earned §{pay} on the side.", sim.first)).replace("{1.Money}", &format!("§{pay}")).replace(char::REPLACEMENT_CHARACTER, "'"),
                money: pay,
                ..Default::default()
            })
        }
        crate::careers::WorkTone::Experiment => {
            t.experiment_hours += hours;
            if t.experiment_hours <= EXPERIMENT_HOURS {
                return None;
            }
            t.experiment_hours = 0.0;
            let roll = rng.random::<f32>();
            let (name, boost) = if roll < EXPERIMENT_CHANCES.0 {
                ("IncreasedPerformance", EXPERIMENT_PERFORMANCE)
            } else if roll < EXPERIMENT_CHANCES.1 {
                ("Promotion", 100.0)
            } else {
                ("Failure", 0.0)
            };
            job.performance = (job.performance + boost).min(100.0);
            Some(ToneCame { words: say(&format!("Gameplay/Careers/Science/DoIndependentExperiment:{name}"), format!("{}'s experiment is done.", sim.first)), ..Default::default() })
        }
        crate::careers::WorkTone::TakeARisk => {
            t.risk_hours += hours;
            if t.risk_hours < RISK_HOURS {
                return None;
            }
            t.risk_hours = 0.0;
            job.tone = crate::careers::WorkTone::Normal;
            // (The game's chances: a tenth promoted, three tenths the boss won over, four the boss
            // won over and the coworkers lost, the rest a flop.)
            let roll = rng.random::<f32>();
            let (key, boss, coworkers) = if roll < 0.1 {
                job.promote = true;
                ("Research", 0.0, 0.0)
            } else if roll < 0.4 {
                ("Proposal", RISK_BOSS.0, 0.0)
            } else if roll < 0.8 {
                ("BrownNose", RISK_BOSS.0, RISK_COWORKERS)
            } else {
                ("ProposalFail", RISK_BOSS.1, RISK_COWORKERS)
            };
            Some(ToneCame { words: say(&format!("Gameplay/Careers/Political:{key}"), format!("{}'s proposal is in.", sim.first)), boss, coworkers, ..Default::default() })
        }
        _ => None,
    }
}

/// Something seen to be reviewed: where (the lot, and its name), what ("Movie", "Restaurant"),
/// and when.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub lot: usize,
    pub place: String,
    pub event: String,
    pub day: u32,
}

impl Review {
    pub fn label(&self) -> String {
        format!("{} at {}", self.event, self.place)
    }
}

/// A report or story under way: on whom, whether it's a hatchet job, and how far along (out of
/// 100).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub id: u64,
    pub name: String,
    pub negative: bool,
    pub done: f32,
    /// A review's lot, and its place's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<(usize, String)>,
}

/// A day-by-day count's total over the last seven days and today.
fn window(days: &[(u32, u32)], today: u32) -> u32 {
    days.iter().filter(|(d, _)| d + MEETING_DAYS >= today && *d <= today).map(|(_, n)| n).sum()
}

/// One more today (those too old for the window forgotten).
fn count_today(days: &mut Vec<(u32, u32)>, today: u32) {
    days.retain(|(d, _)| d + MEETING_DAYS >= today);
    match days.iter_mut().find(|(d, _)| *d == today) {
        Some((_, n)) => *n += 1,
        None => days.push((today, 1)),
    }
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
        window(&self.meetings, today)
    }

    /// A meeting held today.
    pub fn hold_meeting(&mut self, today: u32) {
        count_today(&mut self.meetings, today);
    }

    /// Whether someone's been questioned, or written up, already.
    pub fn knows(&self, id: u64) -> bool {
        self.subjects.iter().any(|s| s.0 == id) || self.trash.iter().any(|s| s.0 == id) || self.written.contains(&id)
    }

    /// Whether a journalist's banned from a place since a bad review there (a week).
    pub fn banned(&self, lot: usize, today: u32) -> bool {
        self.bans.iter().any(|(l, d)| *l == lot && d + MEETING_DAYS >= today)
    }

    /// Whether a place can be reviewed: not banned there, nothing seen there in the last week.
    pub fn may_review(&self, lot: usize, today: u32) -> bool {
        !self.banned(lot, today) && !self.reviews.iter().any(|r| r.lot == lot && r.day + MEETING_DAYS >= today)
    }

    /// A metric's value from the count, where it's one of these (the game's `FindMetricValue`).
    pub fn value(&self, kind: &str, today: u32) -> Option<f32> {
        match kind {
            "HoldMeetings" => Some(self.meetings_held(today) as f32),
            "ConcertsPerformed" => Some(self.concerts as f32),
            "CampaignMoney" => Some(self.campaign as f32),
            "WinLossRecord" => Some(self.wins as f32 - self.losses as f32),
            "MedicalJournals" => Some(self.journals as f32),
            "PoliceReports" | "StoriesAndReviews" => Some(window(&self.reports, today) as f32),
            _ => None,
        }
    }

    /// A new level (a promotion, a demotion, a raise at the top): what's counted for it alone
    /// starts again (the game's `SetLevel`).
    pub fn new_level(&mut self) {
        self.campaign = 0;
        self.journals = 0;
        self.subjects.clear();
        self.trash.clear();
        self.written.clear();
        self.report = None;
    }

    /// The day's journal read: one more, once an edition.
    pub fn journal_finished(&mut self) -> bool {
        if self.journal.is_none() || self.journal_read == self.journal {
            return false;
        }
        self.journal_read = self.journal;
        self.journals += 1;
        true
    }

    /// Hours of the Prepare For Game tone.
    pub fn prepare(&mut self, hours: f32) {
        self.game_prep = (self.game_prep + hours * GAME_PREP_PER_HOUR).min(100.0);
    }

    /// A game played (on `day`): the season's record (a new season after eight), the career's,
    /// and readiness for the next.
    pub fn record_game(&mut self, won: bool, day: u32) {
        self.games += 1;
        if self.games > SEASON_GAMES {
            self.games = 1;
            self.wins = 0;
            self.losses = 0;
        }
        if won {
            self.wins += 1;
            self.total_wins += 1;
        } else {
            self.losses += 1;
            self.total_losses += 1;
        }
        self.game_prep = 0.0;
        self.last_game = Some(day);
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

/// Athletic game days (Wednesday, Sunday: weekdays from Monday), the game's hours, the
/// season's length, the readiness an hour of preparing, and the chances that make the score.
const GAME_DAYS: [usize; 2] = [2, 6];
const GAME_START: f32 = 15.0;
const GAME_END: f32 = 18.0;
const SEASON_GAMES: u32 = 8;
const GAME_PREP_PER_HOUR: f32 = 34.0;
const SCORE_UPDATES: u32 = 36;
const NO_SCORE: f64 = 0.5;
const WIN_BASE: f32 = 0.4;
const WIN_MOOD: f32 = 0.25;
const WIN_PREP: f32 = 0.35;
const WIN_MISSING: f32 = 0.05;
/// Performance and hours' pay for a win and a loss.
const WIN_PERFORMANCE: f32 = 10.0;
const LOSS_PERFORMANCE: f32 = -5.0;
const WIN_PAY_HOURS: f32 = 10.0;
const LOSS_PAY_HOURS: f32 = 1.0;

/// Whether it's a game day for an athlete (a level that counts the record).
pub fn game_day(job: &Job, weekday: usize) -> bool {
    GAME_DAYS.contains(&weekday) && job.info().metrics.iter().any(|m| m.kind == "WinLossRecord")
}

/// The home side's chance at each point: the athlete's, by mood and readiness, there or not.
pub fn win_chance(there: bool, mood: f32, prep: f32) -> f32 {
    if !there {
        return WIN_MISSING;
    }
    (WIN_BASE + WIN_MOOD * mood.clamp(-100.0, 100.0) / 100.0 + WIN_PREP * prep.clamp(0.0, 100.0) / 100.0).clamp(0.0, 1.0)
}

/// A game played out (the game's `GameScoreUpdate` every five minutes): home and away.
pub fn play_game(p: f32, rng: &mut impl Rng) -> (u32, u32) {
    let (mut home, mut away) = (0, 0);
    let p = p.clamp(0.0, 1.0) as f64;
    for i in 0..=SCORE_UPDATES {
        // (The last, a tie's settling point.)
        if i == SCORE_UPDATES && home != away || i < SCORE_UPDATES && rng.random_bool(NO_SCORE) {
            continue;
        }
        if rng.random_bool(p) {
            home += 1;
        } else {
            away += 1;
        }
    }
    (home, away)
}

/// The visits: a meeting at the business office; a concert at the theatre or the stadium.
pub const MEETING_TASK: usize = 7100;
pub const THEATRE_CONCERT_TASK: usize = 7101;
pub const STADIUM_CONCERT_TASK: usize = 7102;
pub const PLAY_GAME_TASK: usize = 7103;

const fn gig(name: &'static str, minutes: f32) -> Activity {
    Activity { name, minutes, cost: 0, per_hour: [0.0; 6], skill: None, open: 0.0, close: 24.0 }
}
pub static HOLD_MEETING: Activity = gig("Hold Meeting", 90.0);
pub static THEATRE_CONCERT: Activity = gig("Perform Concert", 120.0);
pub static STADIUM_CONCERT: Activity = gig("Perform Concert", 240.0);
/// Warming up and the game (till six, from half past two).
pub static PLAY_GAME: Activity = gig("Play In Stadium", 210.0);

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
pub fn options(world: &WorldInfo, lot: usize, job: Option<&Job>, (day, weekday, hour): (u32, usize, f32), mood: f32) -> Vec<(String, usize)> {
    let (Some(job), Some(class)) = (job, hole_class(world, lot)) else { return Vec::new() };
    let mut out = Vec::new();
    let has = |kind: &str| job.info().metrics.iter().any(|m| m.kind == kind);
    if class == "BusinessAndJournalismRabbitHole" && has("HoldMeetings") && (MEETING_HOURS_OPEN.0..=MEETING_HOURS_OPEN.1).contains(&hour) && mood >= MEETING_MOOD {
        out.push((HOLD_MEETING.name.to_string(), MEETING_TASK));
    }
    if class == "Stadium" && game_day(job, weekday) && (GAME_START - 1.0..GAME_START + 0.5).contains(&hour) && job.tally.last_game != Some(day) {
        out.push((PLAY_GAME.name.to_string(), PLAY_GAME_TASK));
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
    Game,
}

impl Gig {
    pub fn of(task: usize) -> Option<Self> {
        Some(match task {
            MEETING_TASK => Self::Meeting,
            THEATRE_CONCERT_TASK => Self::TheatreConcert,
            STADIUM_CONCERT_TASK => Self::StadiumConcert,
            PLAY_GAME_TASK => Self::Game,
            _ => return None,
        })
    }

    pub fn activity(self) -> &'static Activity {
        match self {
            Self::Meeting => &HOLD_MEETING,
            Self::TheatreConcert => &THEATRE_CONCERT,
            Self::StadiumConcert => &STADIUM_CONCERT,
            Self::Game => &PLAY_GAME,
        }
    }
}

pub fn is_gig(a: &Activity) -> bool {
    [&HOLD_MEETING, &THEATRE_CONCERT, &STADIUM_CONCERT, &PLAY_GAME].iter().any(|g| std::ptr::eq(a, *g))
}

/// Back from it.
#[derive(Component)]
pub struct GigDone;

/// GIG=<lot name or rabbit hole class part>:<option> (tests): ten seconds in, the selected Sim off to that gig for
/// their job (set with CAREER_REWARD=); WORK_TONE=<tone> sets how they work; CLOCK_AT=<day>:<hour>
/// moves the clock there (days from the first Monday); READ_JOURNAL=1, the day's journal brought
/// home and read.
#[allow(clippy::type_complexity)]
fn gig_test(
    time: Res<Time>,
    mut sel: Query<(Entity, &Sim, &mut Job, &mut crate::interact::ActionQueue, &crate::life::Mood), With<crate::sim::Selected>>,
    mut commands: Commands,
    world: Res<crate::loading::CurrentWorld>,
    mut clock: ResMut<crate::clock::GameClock>,
    mut done: Local<bool>,
) {
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    let Ok((e, sim, mut job, mut q, mood)) = sel.single_mut() else { return };
    *done = true;
    if std::env::var("READ_JOURNAL").is_ok() {
        job.tally.journal = Some(clock.day());
        bring_journal(&mut commands, e, journal_title(None, sim, job.level, clock.day()), clock.day());
        q.0.clear();
        q.push_player(crate::interact::Action::new("Read Medical Journal", crate::interact::ActionKind::ReadJournal, false));
    }
    if let Some((d, h)) = std::env::var("CLOCK_AT").ok().and_then(|v| v.split_once(':').and_then(|(d, h)| Some((d.parse::<f64>().ok()?, h.parse::<f64>().ok()?)))) {
        clock.minutes = d * 1440.0 + h * 60.0;
        info!("gig test: the clock at day {d}, {h}:00");
    }
    if let Ok(t) = std::env::var("WORK_TONE") {
        job.tone = crate::careers::WorkTone::from_name(&t);
        info!("gig test: working {:?}", job.tone);
    }
    let Some((place, want)) = std::env::var("GIG").ok().and_then(|v| v.split_once(':').map(|(p, w)| (p.to_ascii_lowercase(), w.to_string()))) else { return };
    let found = (0..world.data.lots.len())
        .filter(|&l| world.data.lots[l].internal_name.to_ascii_lowercase().contains(&place) || hole_class(&world.data, l).is_some_and(|c| c.to_ascii_lowercase().contains(&place)))
        .find_map(|l| {
            options(&world.data, l, Some(&job), (clock.day(), clock.weekday(), clock.hour_f()), mood.0)
                .into_iter()
                .chain(review_options(&world.data, l, Some(&job), (clock.day(), clock.hour_f())))
                .find(|(label, _)| label.starts_with(&want))
                .map(|o| (l, o))
        });
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
    mut sims: Query<(Entity, &Sim, &Gig, Option<&mut Job>, Option<&crate::life::Mood>, Option<&mut crate::life::Moodlets>), With<GigDone>>,
    mut household: Option<ResMut<Household>>,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    for (e, sim, gig, job, mood, moodlets) in &mut sims {
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
            Gig::Game => {
                let p = win_chance(true, mood.map_or(0.0, |m| m.0), job.tally.game_prep);
                let words = game_over(&mut job, sim, p, clock.day(), household.as_deref_mut(), ui.as_deref());
                if words.1 {
                    if let Some(mut m) = moodlets {
                        m.add(crate::life::MoodletKind::Winner, clock.minutes);
                    }
                    play.write(crate::sound::PlaySound::ui("sting_career_positive"));
                }
                notes.push(words.0);
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

/// A game's result for an athlete (`p`: the home side's chance at each point): the record, the
/// performance (a loss counts against them there or not; a win only if they played), the pay,
/// and the game's words, with whether it was won.
pub fn game_over(job: &mut Job, sim: &Sim, p: f32, day: u32, household: Option<&mut Household>, ui: Option<&crate::layout::UiAssets>) -> (String, bool) {
    let (home, away) = play_game(p, &mut rand::rng());
    let won = home > away;
    let there = p > WIN_MISSING;
    job.tally.record_game(won, day);
    if won && there {
        job.performance = (job.performance + WIN_PERFORMANCE).min(100.0);
    } else if !won {
        job.performance = (job.performance + LOSS_PERFORMANCE).max(-100.0);
    }
    let pay = ((job.info().hourly + job.raise) as f32 * if won { WIN_PAY_HOURS } else { LOSS_PAY_HOURS }) as i64;
    if let Some(h) = household {
        h.funds += pay;
    }
    info!("{}'s team {} {home}-{away} (season {}-{})", sim.first, if won { "won" } else { "lost" }, job.tally.wins, job.tally.losses);
    let key = if won { "Gameplay/Careers/ProfessionalSports:WinTNS" } else { "Gameplay/Careers/ProfessionalSports:LoseTNS" };
    let words = ui.and_then(|u| u.localize(key)).map_or_else(
        || format!("{}'s team {} the game! Final Score, Home: {home} - Away: {away}.", sim.first, if won { "won" } else { "lost" }),
        |t| t.replace("{0.Number}", &home.to_string()).replace("{1.Number}", &away.to_string()).replace("{2.SimFirstName}", &sim.first),
    );
    (format!("{words} (§{pay})"), won)
}

/// An athlete's game days: the morning's notice, off to the stadium at two (an hour's warning),
/// and, at six, a game missed counted as one.
#[allow(clippy::type_complexity)]
fn game_days(
    mut sims: Query<(Entity, &Sim, &mut Job, &mut crate::interact::ActionQueue, Has<crate::rabbitholes::AtRabbitHole>), With<crate::sim::HouseholdMember>>,
    clock: Res<crate::clock::GameClock>,
    world: Res<crate::loading::CurrentWorld>,
    mut household: Option<ResMut<Household>>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<Notifications>,
    mut told: Local<Vec<(Entity, u32, u8)>>,
) {
    let (day, weekday, hour) = (clock.day(), clock.weekday(), clock.hour_f());
    told.retain(|t| t.1 == day);
    for (e, sim, mut job, mut queue, away) in &mut sims {
        if !game_day(&job, weekday) || job.tally.last_game == Some(day) {
            continue;
        }
        let mut once = |n: u8| {
            let first = !told.iter().any(|t| t.0 == e && t.2 == n);
            if first {
                told.push((e, day, n));
            }
            first
        };
        let say = |key: &str, or: String| ui.as_deref().and_then(|u| u.localize(key)).map_or(or, |t| t.replace("{0.SimName}", &sim.full_name()).replace("{1.HourOfDay}", "3:00 pm"));
        if (9.0..GAME_START - 1.0).contains(&hour) && once(0) {
            notes.push(say("Gameplay/Careers/ProfessionalSports:GameToday", format!("{} has a game today at 3:00 pm!", sim.full_name())));
        }
        if (GAME_START - 1.0..GAME_START).contains(&hour) && once(1) {
            notes.push(say("Gameplay/Careers/ProfessionalSports:GoingToGame", format!("{} has a game in 1 hour.", sim.full_name())));
            let stadium = (0..world.data.lots.len()).find(|&l| hole_class(&world.data, l) == Some("Stadium"));
            let going = queue.0.iter().any(|a| matches!(a.kind, crate::interact::ActionKind::Visit { activity: PLAY_GAME_TASK, .. }));
            if let Some(lot) = stadium
                && !going
                && !away
            {
                for a in queue.0.iter_mut() {
                    a.cancel = true;
                }
                queue.push_player(crate::interact::Action::new(PLAY_GAME.name, crate::interact::ActionKind::Visit { lot, activity: PLAY_GAME_TASK }, false));
            }
        }
        // (Not there by the end: all but lost without them.)
        if hour >= GAME_END + 0.5 && !away && once(2) {
            let words = game_over(&mut job, sim, WIN_MISSING, day, household.as_deref_mut(), ui.as_deref());
            notes.push(words.0);
        }
    }
}

/// The medical journals' titles, levels 4 to 10 (the Books table's `MedicalJournal`), and how
/// long one takes to read (ninety pages, a page a minute; a Bookworm two).
const JOURNAL_TITLES: [(&str, &str); 7] = [
    ("SellingOrgansForFunAndProfit", "Organ Donation"),
    ("ManagingMedicalMalpractice", "Managing Malpractice"),
    ("EtTuNurse", "Be Nice to the Nurse"),
    ("SurgeyForDummiesUnratedEdition", "Surgery for Dummies"),
    ("101ThingsToTryOutInTheOperatingRoom", "Things to Try Out in the Operating Room"),
    ("CleverInterpretationsOfTheHippocraticOath", "Fun with the Hippocratic Oath"),
    ("FashionablePagersThatBling", "Fashionable Pages"),
];
pub const JOURNAL_MINUTES: f32 = 90.0;
pub const BOOKWORM_JOURNAL_MINUTES: f32 = 45.0;

/// A doctor's journal for the day: its title at their level, its volume the day's.
pub fn journal_title(ui: Option<&crate::layout::UiAssets>, sim: &Sim, level: usize, day: u32) -> String {
    let (key, or) = JOURNAL_TITLES[level.clamp(3, 9) - 3];
    let volume = (day + 1).to_string();
    ui.and_then(|u| u.localize(&format!("Gameplay/Excel/Books/MedicalJournal:{key}")))
        .map_or_else(|| format!("{}'s Medical Journal: {or}: Volume {volume}", sim.full_name()), |t| t.replace("{0.SimName}", &sim.full_name()).replace("{1.Number}", &volume))
}

/// The day's journal into a doctor's inventory, the last one gone.
pub fn bring_journal(commands: &mut Commands, e: Entity, title: String, day: u32) {
    commands.entity(e).queue(move |mut w: EntityWorldMut| {
        if w.get::<crate::inventory::Inventory>().is_none() {
            w.insert(crate::inventory::Inventory::default());
        }
        if let Some(mut inv) = w.get_mut::<crate::inventory::Inventory>() {
            inv.0.retain(|s| s.kind != crate::inventory::ItemKind::Journal);
            inv.0.push(crate::inventory::Stack { kind: crate::inventory::ItemKind::Journal, key: format!("MedicalJournal{day}"), name: title, quality: 0, count: 1, worth: 0 });
        }
    });
}

/// A journal read through: counted, if it's the day's and not read before.
pub fn journal_read(w: &mut World, e: Entity) {
    let Some(first) = w.get::<Sim>(e).map(|s| s.first.clone()) else { return };
    let Some(mut job) = w.get_mut::<Job>(e) else { return };
    if job.tally.journal_finished() {
        let n = job.tally.journals;
        info!("{first} read a medical journal ({n} at this level)");
        w.resource_mut::<Notifications>().push(format!("{first} finished reading the day's medical journal."));
    }
}

/// What a policeman's or journalist's level counts them by, if it's their line.
fn writes_up(career: &str) -> Option<&'static str> {
    match career {
        "LawEnforcement" => Some("PoliceReports"),
        "Journalism" => Some("StoriesAndReviews"),
        _ => None,
    }
}

/// Whether a Sim questions or interviews others (the game's `CanInterview`: a level that counts
/// what they write), by their career and level.
pub fn interviews(career: Option<usize>, level: usize) -> bool {
    let Some(t) = career.and_then(|c| crate::careers::careers().get(c)) else { return false };
    let Some(kind) = writes_up(t.key) else { return false };
    t.paths.iter().any(|p| p.levels.get(level).is_some_and(|l| l.metrics.iter().any(|m| m.kind == kind)))
}

/// Someone questioned or interviewed, taken well: a subject to write up, and their traits learned.
pub fn interviewed(w: &mut World, asker: Entity, subject: Entity) {
    let Some((id, name, traits)) = w.get::<Sim>(subject).map(|s| (s.id, s.full_name(), s.traits.clone())) else { return };
    let Some(mut job) = w.get_mut::<Job>(asker) else { return };
    if job.tally.knows(id) {
        return;
    }
    job.tally.subjects.push((id, name.clone()));
    info!("questioned {name} for a report");
    for t in traits {
        w.write_message(crate::learning::Learned { learner: asker, about: subject, what: Some(t), random: false, teaches: ["", ""] });
    }
}

/// The computer's menu titles for writing someone up.
pub const REPORT_ON: &str = "Report On...";
pub const POSITIVE_STORY: &str = "Write Positive Story On...";
pub const NEGATIVE_STORY: &str = "Write Negative Story On...";
/// The computer's action, as named while it's under way.
pub const WRITE_REPORT: &str = "Write Report";
const WRITE_STORY: &str = "Write Story";
const WRITE_REVIEW: &str = "Write Review";
pub const POSITIVE_REVIEW: &str = "Write Positive Review On...";
pub const NEGATIVE_REVIEW: &str = "Write Negative Review On...";

/// The computer's writing options for a job: a report or story to go on with or scrap, else
/// those that can be begun (a menu of subjects each).
pub fn writing_options(job: &Job) -> Vec<(String, Option<Vec<String>>)> {
    let Some(kind) = writes_up(job.career().key) else { return Vec::new() };
    if !job.info().metrics.iter().any(|m| m.kind == kind) {
        return Vec::new();
    }
    if let Some(r) = &job.tally.report {
        let what = if r.review.is_some() { "Review" } else if kind == "PoliceReports" { "Report" } else { "Story" };
        return vec![(format!("Continue {what} on {}", r.name), None), (format!("Scrap {what} on {}", r.name), None)];
    }
    let names: Vec<String> = job.tally.subjects.iter().map(|s| s.1.clone()).collect();
    let mut out = Vec::new();
    if kind == "PoliceReports" {
        if !names.is_empty() {
            out.push((REPORT_ON.into(), Some(names)));
        }
        return out;
    }
    let dirt: Vec<String> = names.iter().cloned().chain(job.tally.trash.iter().map(|s| s.1.clone())).collect();
    if !names.is_empty() {
        out.push((POSITIVE_STORY.into(), Some(names)));
    }
    if !dirt.is_empty() {
        out.push((NEGATIVE_STORY.into(), Some(dirt)));
    }
    let seen: Vec<String> = job.tally.reviews.iter().map(Review::label).collect();
    if !seen.is_empty() {
        out.push((POSITIVE_REVIEW.into(), Some(seen.clone())));
        out.push((NEGATIVE_REVIEW.into(), Some(seen)));
    }
    out
}

/// What was picked from the computer's writing options: one begun (on whom, and the action's
/// name), one gone on with, one scrapped, or none of these.
#[derive(Debug, PartialEq)]
pub enum Pick {
    Begun(&'static str),
    Continued(&'static str),
    Scrapped,
    Other,
}

pub fn pick_writing(tally: &mut Tally, title: &str, label: &str) -> Pick {
    if label.starts_with("Scrap Report on ") || label.starts_with("Scrap Story on ") || label.starts_with("Scrap Review on ") {
        tally.report = None;
        return Pick::Scrapped;
    }
    if label.starts_with("Continue Report on ") {
        return Pick::Continued(WRITE_REPORT);
    }
    if label.starts_with("Continue Story on ") {
        return Pick::Continued(WRITE_STORY);
    }
    if label.starts_with("Continue Review on ") {
        return Pick::Continued(WRITE_REVIEW);
    }
    if (title == POSITIVE_REVIEW || title == NEGATIVE_REVIEW)
        && let Some(r) = tally.reviews.iter().find(|r| r.label() == label).cloned()
    {
        tally.report = Some(Report { id: r.lot as u64, name: label.to_string(), negative: title == NEGATIVE_REVIEW, done: 0.0, review: Some((r.lot, r.place)) });
        return Pick::Begun(WRITE_REVIEW);
    }
    let negative = title == NEGATIVE_STORY;
    let found = tally.subjects.iter().chain(tally.trash.iter().filter(|_| negative)).find(|s| s.1 == label).cloned();
    if (title == REPORT_ON || title == POSITIVE_STORY || negative)
        && let Some((id, name)) = found
    {
        tally.report = Some(Report { id, name, negative, done: 0.0, review: None });
        return Pick::Begun(if title == REPORT_ON { WRITE_REPORT } else { WRITE_STORY });
    }
    Pick::Other
}

/// How far a report (an hour and a half) or story (two hours) comes along a minute.
const REPORT_RATE: f32 = 100.0 / 90.0;
const STORY_RATE: f32 = 100.0 / 120.0;
/// What a report on a criminal, and a negative story, do for performance; how a story changes
/// its subject's liking for the writer.
const CRIMINAL_REPORT_PERFORMANCE: f32 = 15.0;
const NEGATIVE_STORY_PERFORMANCE: f32 = 20.0;
const NEGATIVE_STORY_LIKING: f32 = -30.0;
const POSITIVE_STORY_LIKING: f32 = 10.0;

/// At the computer writing someone up: the report or story along, and finished at last.
#[allow(clippy::type_complexity)]
fn write_on(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    mut sims: Query<(Entity, &mut Job, &mut crate::interact::ActionQueue), With<crate::sim::HouseholdMember>>,
    objects: Query<&crate::interact::GameObject>,
) {
    if delta.0 <= 0.0 {
        return;
    }
    for (e, mut job, mut queue) in &mut sims {
        let Some(a) = queue.0.front_mut() else { continue };
        let crate::interact::ActionKind::Object { target, def } = a.kind else { continue };
        if !matches!(a.phase, crate::interact::Phase::Running(_)) {
            continue;
        }
        let Some(kind) = objects.get(target).ok().map(|o| o.kind) else { continue };
        if crate::interact::interactions_for(kind).get(def).map(|d| d.special) != Some(crate::interact::Special::WriteReport) {
            continue;
        }
        let rate = if job.career().key == "LawEnforcement" { REPORT_RATE } else { STORY_RATE };
        let Some(r) = job.tally.report.as_mut() else {
            a.cancel = true;
            continue;
        };
        r.done += rate * delta.0;
        if r.done >= 100.0 {
            a.cancel = true;
            commands.queue(move |w: &mut World| written_up(w, e));
        }
    }
}

/// WRITE_REPORT=report|good|bad|review (tests): once the selected Sim has a subject (or something
/// seen to review), off to the computer to write it up.
fn report_test(
    mut sel: Query<(&mut Job, &mut crate::interact::ActionQueue), With<crate::sim::Selected>>,
    objects: Query<(Entity, &crate::interact::GameObject)>,
    mut done: Local<bool>,
) {
    let Ok(how) = std::env::var("WRITE_REPORT") else { return };
    if *done {
        return;
    }
    let Ok((mut job, mut q)) = sel.single_mut() else { return };
    let name = if how == "review" { job.tally.reviews.first().map(Review::label) } else { job.tally.subjects.first().map(|s| s.1.clone()) };
    let Some(name) = name else { return };
    *done = true;
    let title = match how.as_str() {
        "good" => POSITIVE_STORY,
        "bad" => NEGATIVE_STORY,
        "review" => NEGATIVE_REVIEW,
        _ => REPORT_ON,
    };
    let Pick::Begun(label) = pick_writing(&mut job.tally, title, &name) else { return };
    let computer = objects.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Computer).map(|(e, _)| e);
    let def = crate::interact::interactions_for(crate::interact::ObjectKind::Computer).iter().position(|d| d.special == crate::interact::Special::WriteReport);
    match (computer, def) {
        (Some(target), Some(def)) => {
            info!("report test: {label} on {name}");
            q.push_player(crate::interact::Action::new(label, crate::interact::ActionKind::Object { target, def }, false));
        }
        _ => warn!("report test: no computer"),
    }
}

/// A report or story finished: counted today, its subject done with, and what it does.
fn written_up(w: &mut World, writer: Entity) {
    let Some(me) = w.get::<Sim>(writer).cloned() else { return };
    let today = w.resource::<crate::clock::GameClock>().day();
    let Some(mut job) = w.get_mut::<Job>(writer) else { return };
    let Some(r) = job.tally.report.take() else { return };
    let police = job.career().key == "LawEnforcement";
    count_today(&mut job.tally.reports, today);
    // (A review: done with the place's, and, a bad one, banned there.)
    if let Some((lot, place)) = r.review.clone() {
        job.tally.reviews.retain(|x| x.label() != r.name);
        let n = rand::rng().random_range(1..=3);
        let key = if r.negative { "ReviewBad" } else { "ReviewGood" };
        if r.negative {
            job.tally.bans.push((lot, today));
            job.performance = (job.performance + NEGATIVE_STORY_PERFORMANCE).min(100.0);
        }
        let text = w.get_resource::<crate::layout::UiAssets>().and_then(|u| u.localize(&format!("Gameplay/Careers/Journalism:{key}{n}"))).map_or_else(
            || format!("{} finished the review of {place}.", me.first),
            |t| t.replace("{0.SimFirstName}", &me.first).replace("{1.String}", &place),
        );
        info!("{} reviewed {} ({key})", me.first, r.name);
        w.resource_mut::<Notifications>().push(text);
        return;
    }
    job.tally.subjects.retain(|s| s.0 != r.id);
    job.tally.trash.retain(|s| s.0 != r.id);
    job.tally.written.push(r.id);
    let subject = w.query::<(Entity, &Sim)>().iter(w).find(|(_, s)| s.id == r.id).map(|(e, _)| e);
    let criminal = subject.is_some_and(|s| {
        w.get::<Sim>(s).is_some_and(|x| x.traits.contains(&crate::life::Trait::Kleptomaniac)) || w.get::<Job>(s).is_some_and(|j| j.career().key == "Criminal")
    });
    let (key, change) = match (police, r.negative) {
        (true, _) if criminal => ("LawEnforcement:ReportCompleteCriminal", Some(CRIMINAL_REPORT_PERFORMANCE)),
        (true, _) => ("LawEnforcement:ReportComplete", None),
        (false, true) => ("Journalism:ReportBad", Some(NEGATIVE_STORY_PERFORMANCE)),
        (false, false) => ("Journalism:ReportGood", None),
    };
    if let Some(mut job) = w.get_mut::<Job>(writer)
        && let Some(p) = change
    {
        job.performance = (job.performance + p).min(100.0);
    }
    // (The subject's liking for whoever wrote about them.)
    if !police
        && let Some(s) = subject
        && let Some(mut rels) = w.get_mut::<crate::social::Relationships>(s)
    {
        rels.add(writer, if r.negative { NEGATIVE_STORY_LIKING } else { POSITIVE_STORY_LIKING }, 0.0);
    }
    let n = rand::rng().random_range(1..=3);
    let text = w.get_resource::<crate::layout::UiAssets>().and_then(|u| u.localize(&format!("Gameplay/Careers/{key}{n}"))).map_or_else(
        || format!("{} finished the {} on {}.", me.first, if police { "report" } else { "story" }, r.name),
        |t| t.replace("{0.SimFirstName}", &me.first).replace("{1.SimName}", &r.name),
    );
    info!("{} wrote up {} ({key})", me.first, r.name);
    w.resource_mut::<Notifications>().push(text);
}

/// The reviewers' visits: this plus the venue act seen (`venues::ACTS`).
pub const REVIEW_TASK: usize = 7200;

/// What a venue act is reviewed as: the menu's words and the event's.
fn reviewed_as(act: &str) -> Option<(&'static str, &'static str)> {
    Some(match act {
        "See a Movie" => ("Review Movie", "Movie"),
        "See a Play" | "See a Matinee" => ("Review Play", "Play"),
        "See a Symphony" => ("Review Symphony", "Symphony"),
        "See a Concert" | "Attend a Concert" => ("Review Concert", "Concert"),
        "Attend the Game" => ("Review Game", "Sporting Event"),
        "Eat Here" => ("Review Restaurant", "Restaurant"),
        _ => return None,
    })
}

/// A journalist's ways to see something to review here, free: the venue's shows, games and
/// meals on now, where they may review.
pub fn review_options(world: &WorldInfo, lot: usize, job: Option<&Job>, (day, hour): (u32, f32)) -> Vec<(String, usize)> {
    let Some(job) = job.filter(|j| j.career().key == "Journalism" && j.info().metrics.iter().any(|m| m.kind == "StoriesAndReviews")) else { return Vec::new() };
    if !job.tally.may_review(lot, day) {
        return Vec::new();
    }
    crate::venues::world_lot_options(world, lot, day, hour, crate::sim::Age::Adult)
        .into_iter()
        .filter_map(|(_, task)| {
            let i = task.checked_sub(crate::venues::VENUE_TASK)?;
            let (label, _) = reviewed_as(crate::venues::ACTS.get(i)?.activity.name)?;
            Some((label.to_string(), REVIEW_TASK + i))
        })
        .collect()
}

/// A reviewer's visit's activity (the venue act's own).
pub fn review_task(task: usize) -> Option<&'static Activity> {
    let i = task.checked_sub(REVIEW_TASK).filter(|i| *i < crate::venues::ACTS.len())?;
    Some(&crate::venues::ACTS[i].activity)
}

/// Seeing something to review: the act, where, and whether they've got there yet.
#[derive(Component)]
pub struct Reviewing {
    pub act: usize,
    pub lot: usize,
    went: bool,
}

impl Reviewing {
    pub fn new(task: usize, lot: usize) -> Option<Self> {
        review_task(task)?;
        Some(Self { act: task - REVIEW_TASK, lot, went: false })
    }
}

/// Back from seeing it through: there to write up.
fn reviews_seen(
    mut commands: Commands,
    mut sims: Query<(Entity, &mut Reviewing, Has<crate::rabbitholes::AtRabbitHole>, Option<&mut Job>)>,
    world: Res<crate::loading::CurrentWorld>,
    clock: Res<crate::clock::GameClock>,
) {
    for (e, mut r, away, job) in &mut sims {
        if away {
            r.went = true;
            continue;
        }
        commands.entity(e).remove::<Reviewing>();
        let (Some(mut job), true) = (job, r.went) else { continue };
        let Some((_, event)) = crate::venues::ACTS.get(r.act).and_then(|a| reviewed_as(a.activity.name)) else { continue };
        let review = Review { lot: r.lot, place: crate::visit::place_name(&world.data, r.lot), event: event.to_string(), day: clock.day() };
        info!("saw {} to review", review.label());
        job.tally.reviews.push(review);
    }
}

/// A family's trash rummaged through at their home: information on one of them for a policeman
/// or journalist (`LawEnforcementRummage`, `JournalismRummage`).
pub fn dug_up(w: &mut World, digger: Entity, family: Vec<Entity>) {
    let Some(job) = w.get::<Job>(digger) else { return };
    if !interviews(Some(job.track), job.level) {
        return;
    }
    let police = job.career().key == "LawEnforcement";
    let known = job.tally.clone();
    let found: Vec<(u64, String)> = family
        .iter()
        .filter_map(|e| w.get::<Sim>(*e))
        .filter(|s| s.age.is_grown() && s.age != crate::sim::Age::Child && !known.knows(s.id))
        .map(|s| (s.id, s.full_name()))
        .collect();
    use rand::seq::IndexedRandom;
    let Some((id, name)) = found.choose(&mut rand::rng()).cloned() else { return };
    let Some(first) = w.get::<Sim>(digger).map(|s| s.first.clone()) else { return };
    let Some(mut job) = w.get_mut::<Job>(digger) else { return };
    if police {
        job.tally.subjects.push((id, name.clone()));
    } else {
        job.tally.trash.push((id, name.clone()));
    }
    let n = rand::rng().random_range(1..=3);
    let text = w.get_resource::<crate::layout::UiAssets>().and_then(|u| u.localize(&format!("Gameplay/Objects/Miscellaneous/TrashcanOutside:RummageForInfo{n}"))).map_or_else(
        || format!("After sifting through numerous piles of slimy trash, {first} has found some solid information on {name}."),
        |t| t.replace("{0.SimFirstName}", &first).replace("{1.SimName}", &name),
    );
    w.resource_mut::<Notifications>().push(text);
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
    fn cases_stick_by_logic() {
        assert_eq!(case_chance(0), 0.0);
        assert!((case_chance(1) - 1.0 / 7.0).abs() < 1e-6);
        assert!((case_chance(2) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(case_chance(3), 0.5);
        assert_eq!(case_chance(4), 1.0);
        assert_eq!(case_chance(10), 1.0);
    }

    #[test]
    fn reviews_seen_written_and_banned_for() {
        let mut t = Tally { reviews: vec![Review { lot: 4, place: "Hogan's".into(), event: "Restaurant".into(), day: 10 }], ..Default::default() };
        assert!(!t.may_review(4, 12), "seen there this week");
        assert!(t.may_review(5, 12));
        assert_eq!(pick_writing(&mut t, NEGATIVE_REVIEW, "Restaurant at Hogan's"), Pick::Begun(WRITE_REVIEW));
        assert!(t.report.as_ref().is_some_and(|r| r.negative && r.review == Some((4, "Hogan's".into()))));
        t.bans.push((4, 12));
        assert!(t.banned(4, 19) && !t.banned(4, 20));
        assert_eq!(reviewed_as("Attend the Game"), Some(("Review Game", "Sporting Event")));
        assert_eq!(reviewed_as("Get Drinks"), None);
        t.trash.push((9, "Nancy Landgraab".into()));
        assert!(t.knows(9));
        assert_eq!(pick_writing(&mut t, POSITIVE_STORY, "Nancy Landgraab"), Pick::Other, "dirt's for negative stories");
    }

    #[test]
    fn reports_begun_on_subjects_and_counted() {
        let mut t = Tally { subjects: vec![(7, "Bella Goth".into())], ..Default::default() };
        assert!(t.knows(7) && !t.knows(8));
        assert_eq!(pick_writing(&mut t, NEGATIVE_STORY, "Bella Goth"), Pick::Begun(WRITE_STORY));
        assert!(t.report.as_ref().is_some_and(|r| r.negative && r.id == 7));
        assert_eq!(pick_writing(&mut t, "", "Continue Story on Bella Goth"), Pick::Continued(WRITE_STORY));
        assert_eq!(pick_writing(&mut t, "", "Scrap Story on Bella Goth"), Pick::Scrapped);
        assert!(t.report.is_none());
        count_today(&mut t.reports, 5);
        count_today(&mut t.reports, 6);
        assert_eq!(t.value("PoliceReports", 6), Some(2.0));
        t.new_level();
        assert!(t.subjects.is_empty());
    }

    #[test]
    fn a_journal_counts_once_an_edition() {
        let mut t = Tally::default();
        assert!(!t.journal_finished(), "none brought home");
        t.journal = Some(3);
        assert!(t.journal_finished());
        assert!(!t.journal_finished(), "read already");
        t.journal = Some(4);
        assert!(t.journal_finished());
        assert_eq!(t.value("MedicalJournals", 4), Some(2.0));
        t.new_level();
        assert_eq!(t.journals, 0);
    }

    #[test]
    fn games_make_a_season() {
        let mut t = Tally::default();
        for i in 0..8 {
            t.record_game(i % 4 != 0, i);
        }
        assert_eq!((t.wins, t.losses, t.value("WinLossRecord", 0)), (6, 2, Some(4.0)));
        t.record_game(false, 9);
        assert_eq!((t.games, t.wins, t.losses), (1, 0, 1), "a new season");
        assert_eq!((t.total_wins, t.total_losses), (6, 3));
        assert!(win_chance(true, 100.0, 100.0) > win_chance(true, 0.0, 0.0));
        assert_eq!(win_chance(false, 100.0, 100.0), WIN_MISSING);
        let mut rng = rand::rng();
        let (h, a) = play_game(1.0, &mut rng);
        assert!(h > a && a == 0);
        let wins = (0..200).filter(|_| { let (h, a) = play_game(0.75, &mut rng); h > a }).count();
        assert!(wins > 150, "a strong side mostly wins: {wins}");
    }

    #[test]
    fn a_concert_every_four_hours_of_performing() {
        let mut t = Tally::default();
        assert!(!t.perform_tone(3.0));
        assert!(t.perform_tone(1.0));
        assert_eq!((t.concerts, t.concert_prep), (1, 0.0));
        assert_eq!(t.value("ConcertsPerformed", 0), Some(1.0));
        assert_eq!(t.value("ArtWorkScanned", 0), None, "not a base game metric");
    }
}
