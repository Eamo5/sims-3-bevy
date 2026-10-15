//! Careers: the base game's tracks from the game's own career tables (titles, pay, hours and
//! workdays per level, part-time jobs for teens), with a built-in table as a fallback until
//! those are converted. Sims head off with the carpool before their shift, come home paid,
//! and their performance (mood, the career's skill and personality) earns promotions — or
//! demotions. Some careers branch partway up (Criminal into Thief or Evil, Music into Rock or
//! Symphonic, Law Enforcement into Special Agent or Forensic Analyst): the promotion there
//! asks which path to take.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::*;
use crate::life::{LifeEvent, LifeEventKind, Mood, Trait};
use crate::loading::CurrentWorld;
use crate::sim::*;

pub struct CareersPlugin;

impl Plugin for CareersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (work_flow, work_schedule, roll_career_events, run_career_events, promote_when_agreed, career_path_answers, career_event_answers, pay_pensions, find_colleagues)
                .chain()
                .run_if(in_state(PlayMode::Live)),
        );
    }
}

#[derive(Clone, Copy)]
pub struct CareerLevel {
    pub title: &'static str,
    /// A woman's title, where the game has its own ("Paper Girl"; empty where it's the same).
    pub title_female: &'static str,
    /// Pay per hour worked.
    pub hourly: i64,
    pub start: f32,
    pub end: f32,
    /// When a Sim at this level gets up for work (hour of the day).
    pub wakeup: f32,
    /// Workdays: bit 0 = Monday.
    pub days: u8,
    /// The uniform (an outfit's name; empty for everyday clothes) for men, women, elderly men
    /// and elderly women.
    pub outfits: [&'static str; 4],
    /// How performance is judged at this level, and whether there's a boss.
    pub metrics: &'static [s3bake::gamedata::CareerMetric],
    pub has_boss: bool,
}

pub struct CareerTrack {
    pub name: &'static str,
    /// The skill that improves performance.
    pub skill: &'static str,
    /// The game's career icon.
    pub icon: &'static str,
    /// What the job listings say of it ("{0.SimFirstName}" for the Sim looking).
    pub offer: &'static str,
    /// A teen's after-school job.
    pub part_time: bool,
    /// The ways up the career: each its levels from the first, sharing those before the career
    /// branches (just one for a career that doesn't).
    pub paths: Vec<CareerPath>,
    /// The first level (0-based) that differs between paths.
    pub branch_at: Option<usize>,
    /// Performance an hour at the best and worst metrics (and at the top levels), an hour of
    /// work missed, a promotion's bonus in hours' pay, fun an hour at work, and how many
    /// coworkers (the game's `CareerList`).
    pub flow: [f32; 4],
    pub miss_work: f32,
    pub bonus_hours: f32,
    pub fun_per_hour: f32,
    pub coworkers: (u8, u8),
    /// What can happen at work (the game's `CareerEvents`).
    pub events: &'static [s3bake::gamedata::CareerEventInfo],
}

pub struct CareerPath {
    /// The branch's name in the game's tables ("Thief", "ElectricRock"), or "Base".
    pub branch: &'static str,
    pub levels: Vec<CareerLevel>,
}

impl CareerPath {
    /// The branch's name for show: "Electric Rock".
    pub fn label(&self) -> String {
        branch_label(self.branch)
    }
}

/// A branch's name for show: "ElectricRock" → "Electric Rock".
pub fn branch_label(branch: &str) -> String {
    let mut s = String::new();
    for (i, c) in branch.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            s.push(' ');
        }
        s.push(c);
    }
    s
}

impl crate::dialog::Questions {
    /// Asks which way a Sim's career goes from the level they've just reached.
    pub fn ask_career_path(&mut self, e: Entity, sim: &Sim, job: &Job) {
        let track = job.career();
        let answers = track
            .paths
            .iter()
            .filter_map(|p| {
                let l = p.levels.get(job.level)?;
                // (The path's name, as the game's buttons have it; the job along it below.)
                Some(crate::dialog::Answer {
                    label: p.label(),
                    detail: format!("{}: §{} an hour, {} to {}", l.title_for(sim.female), l.hourly, hour_label(l.start), hour_label(l.end)),
                    icon: String::new(),
                })
            })
            .collect();
        self.ask(crate::dialog::Ask {
            about: crate::dialog::Question::CareerPath { sim: e, paths: (0..track.paths.len()).collect() },
            icon: track.icon.to_string(),
            heading: format!("Career for {}", sim.first),
            title: format!("{} was promoted!", sim.first),
            text: format!("The {} career branches here. Which path should {} take?", track.name, sim.first),
            answers,
        });
    }
}

/// The path the player chose at a branch.
fn career_path_answers(mut answers: MessageReader<crate::dialog::Answered>, mut jobs: Query<(&Sim, &mut Job)>, mut notes: ResMut<Notifications>) {
    for a in answers.read() {
        let crate::dialog::Question::CareerPath { sim, paths } = &a.about else { continue };
        let (Ok((s, mut job)), Some(&p)) = (jobs.get_mut(*sim), paths.get(a.answer)) else { continue };
        job.branch = p;
        notes.push(format!("{} is now a {} on the {} path.", s.first, job.info().title_for(s.female), job.path().label()));
    }
}

impl CareerLevel {
    /// The title for a man or a woman, as the game localizes it for the Sim.
    pub fn title_for(&self, female: bool) -> &'static str {
        if female && !self.title_female.is_empty() { self.title_female } else { self.title }
    }
}

impl CareerTrack {
    /// The first path's levels (all of them, for a career that doesn't branch).
    pub fn levels(&self) -> &[CareerLevel] {
        &self.paths[0].levels
    }
    /// The path along a branch.
    pub fn path_index(&self, branch: &str) -> Option<usize> {
        self.paths.iter().position(|p| p.branch == branch)
    }
}

static TRACKS: std::sync::OnceLock<Vec<CareerTrack>> = std::sync::OnceLock::new();

/// The career tracks: the game's own once converted, else the built-in table.
pub fn careers() -> &'static [CareerTrack] {
    TRACKS.get_or_init(|| BUILTIN.iter().map(BuiltinTrack::track).collect())
}

/// Uses the game's career tables (from `s3bake::gamedata`) for this session.
pub fn install_tracks(data: &s3bake::GameDataBaked) {
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let tracks: Vec<CareerTrack> = data
        .careers
        .iter()
        .filter(|c| !c.name.is_empty())
        .map(|c| {
            // Each branch's path: the base levels, then the branch's own, in order.
            let mut branches: Vec<&str> = Vec::new();
            for l in &c.levels {
                if l.branch != "Base" && !branches.contains(&l.branch.as_str()) {
                    branches.push(&l.branch);
                }
            }
            if branches.is_empty() {
                branches.push("Base");
            }
            let paths: Vec<CareerPath> = branches
                .iter()
                .map(|&b| {
                    let mut levels: Vec<CareerLevel> = Vec::new();
                    for l in c.levels.iter().filter(|l| l.branch == "Base" || l.branch == b) {
                        if l.level as usize != levels.len() + 1 {
                            continue;
                        }
                        let start = l.start;
                        levels.push(CareerLevel {
                            title: leak(if l.title.is_empty() { format!("Level {}", l.level) } else { l.title.clone() }),
                            title_female: leak(if l.title_female == l.title { String::new() } else { l.title_female.clone() }),
                            hourly: l.hourly.round() as i64,
                            start,
                            end: (start + l.hours) % 24.0,
                            // (Two hours before work where the table doesn't say.)
                            wakeup: if l.wakeup > 0.0 { l.wakeup } else { (start + 22.0) % 24.0 },
                            days: l.days,
                            outfits: [0, 1, 2, 3].map(|i| leak(l.outfits[i].to_ascii_lowercase())),
                            metrics: Box::leak(l.metrics.clone().into_boxed_slice()),
                            has_boss: l.has_boss,
                        });
                    }
                    CareerPath { branch: leak(b.to_string()), levels }
                })
                .collect();
            let branch_at = c.levels.iter().filter(|l| l.branch != "Base").map(|l| l.level as usize - 1).min().filter(|_| paths.len() > 1);
            let skill = c.levels.iter().find_map(|l| l.skills.first().cloned()).unwrap_or_else(|| default_skill(&c.hex).to_string());
            CareerTrack {
                name: leak(c.name.clone()),
                skill: leak(skill),
                icon: leak(c.icon.clone()),
                offer: leak(c.offer.clone()),
                part_time: c.part_time,
                paths,
                branch_at,
                flow: c.flow,
                miss_work: c.miss_work,
                bonus_hours: c.bonus_hours,
                fun_per_hour: c.fun_per_hour,
                coworkers: c.coworkers,
                events: Box::leak(c.events.clone().into_boxed_slice()),
            }
        })
        .filter(|t| t.paths.iter().all(|p| !p.levels.is_empty()))
        .collect();
    if !tracks.is_empty() {
        let _ = TRACKS.set(tracks);
    }
}

/// The skill a career rewards when its table doesn't say.
fn default_skill(hex: &str) -> &'static str {
    match hex {
        "Culinary" => "Cooking",
        "Music" => "Guitar",
        "Political" | "Business" => "Charisma",
        "Journalism" => "Writing",
        "ProfessionalSports" | "Military" | "Criminal" => "Athletic",
        "Science" => "Gardening",
        _ => "Logic",
    }
}

struct BuiltinTrack {
    name: &'static str,
    skill: &'static str,
    days: u8,
    levels: [(&'static str, i64, f32, f32); 10],
}

impl BuiltinTrack {
    fn track(&self) -> CareerTrack {
        CareerTrack {
            name: self.name,
            skill: self.skill,
            icon: "",
            offer: "",
            part_time: false,
            paths: vec![CareerPath {
                branch: "Base",
                levels: self
                    .levels
                    .iter()
                    .map(|&(title, hourly, start, end)| CareerLevel { title, title_female: "", hourly, start, end, wakeup: (start + 22.0) % 24.0, days: self.days, outfits: [""; 4], metrics: &[], has_boss: false })
                    .collect(),
            }],
            branch_at: None,
            flow: [14.0, -8.0, 8.0, -10.0],
            miss_work: -10.0,
            bonus_hours: 12.0,
            fun_per_hour: -5.0,
            coworkers: (2, 4),
            events: &[],
        }
    }
}

const WEEKDAYS: u8 = 0b0011111;
const fn lv(title: &'static str, hourly: i64, start: f32, end: f32) -> (&'static str, i64, f32, f32) {
    (title, hourly, start, end)
}

static BUILTIN: [BuiltinTrack; 10] = [
    BuiltinTrack {
        name: "Business",
        skill: "Charisma",
        days: WEEKDAYS,
        levels: [
            lv("Office Assistant", 17, 9.0, 15.0),
            lv("Junior Clerk", 24, 9.0, 15.0),
            lv("Account Analyst", 32, 9.0, 16.0),
            lv("Senior Account Analyst", 42, 9.0, 16.0),
            lv("Middle Manager", 56, 9.0, 17.0),
            lv("Senior Manager", 74, 9.0, 17.0),
            lv("Vice President", 96, 9.0, 17.0),
            lv("Executive Vice President", 124, 10.0, 17.0),
            lv("Chief Executive Officer", 160, 10.0, 16.0),
            lv("Chairman of the Board", 210, 10.0, 15.0),
        ],
    },
    BuiltinTrack {
        name: "Culinary",
        skill: "Cooking",
        days: 0b1111100,
        levels: [
            lv("Dishwasher", 15, 15.0, 21.0),
            lv("Waiter", 20, 15.0, 21.0),
            lv("Prep Cook", 27, 15.0, 22.0),
            lv("Line Cook", 35, 15.0, 22.0),
            lv("Sous Chef", 46, 14.0, 22.0),
            lv("Pastry Chef", 60, 14.0, 21.0),
            lv("Executive Chef", 80, 14.0, 21.0),
            lv("Restaurateur", 104, 13.0, 20.0),
            lv("Celebrity Chef", 136, 13.0, 19.0),
            lv("Culinary Legend", 180, 13.0, 18.0),
        ],
    },
    BuiltinTrack {
        name: "Criminal",
        skill: "Athletic",
        days: 0b0111011,
        levels: [
            lv("Lookout", 16, 20.0, 2.0),
            lv("Thug", 22, 20.0, 2.0),
            lv("Henchman", 29, 20.0, 2.0),
            lv("Getaway Driver", 38, 21.0, 3.0),
            lv("Bagman", 50, 21.0, 3.0),
            lv("Safecracker", 66, 21.0, 3.0),
            lv("Con Artist", 86, 21.0, 3.0),
            lv("Kingpin", 112, 22.0, 3.0),
            lv("Mafia Boss", 146, 22.0, 3.0),
            lv("Criminal Mastermind", 194, 22.0, 2.0),
        ],
    },
    BuiltinTrack {
        name: "Journalism",
        skill: "Writing",
        days: WEEKDAYS,
        levels: [
            lv("Paper Delivery Person", 14, 7.0, 12.0),
            lv("Mail Room Lackey", 20, 8.0, 14.0),
            lv("Copy Editor", 27, 9.0, 15.0),
            lv("Junior Reporter", 35, 9.0, 16.0),
            lv("Reporter", 46, 9.0, 16.0),
            lv("Senior Reporter", 60, 9.0, 17.0),
            lv("Columnist", 78, 10.0, 16.0),
            lv("Editor", 100, 10.0, 17.0),
            lv("Editor-in-Chief", 130, 10.0, 16.0),
            lv("Media Mogul", 172, 10.0, 15.0),
        ],
    },
    BuiltinTrack {
        name: "Law Enforcement",
        skill: "Logic",
        days: WEEKDAYS,
        levels: [
            lv("Desk Jockey", 16, 9.0, 15.0),
            lv("Mall Security", 22, 9.0, 15.0),
            lv("Night Shift Patrol", 30, 18.0, 2.0),
            lv("Lead Officer", 40, 9.0, 16.0),
            lv("Detective", 52, 9.0, 17.0),
            lv("Sergeant", 68, 9.0, 17.0),
            lv("Captain", 88, 9.0, 17.0),
            lv("Special Agent", 114, 9.0, 18.0),
            lv("Chief of Police", 148, 9.0, 17.0),
            lv("Superintendent", 192, 10.0, 16.0),
        ],
    },
    BuiltinTrack {
        name: "Medical",
        skill: "Logic",
        days: WEEKDAYS,
        levels: [
            lv("Orderly", 17, 8.0, 15.0),
            lv("Medical Records Clerk", 23, 8.0, 15.0),
            lv("Paramedic", 31, 8.0, 16.0),
            lv("Intern", 40, 7.0, 16.0),
            lv("Resident", 53, 7.0, 17.0),
            lv("General Practitioner", 70, 8.0, 17.0),
            lv("Specialist", 92, 8.0, 17.0),
            lv("Surgeon", 120, 8.0, 17.0),
            lv("Head of Surgery", 156, 9.0, 17.0),
            lv("Chief of Staff", 204, 9.0, 16.0),
        ],
    },
    BuiltinTrack {
        name: "Military",
        skill: "Athletic",
        days: WEEKDAYS,
        levels: [
            lv("Recruit", 18, 6.0, 14.0),
            lv("Private", 24, 6.0, 14.0),
            lv("Corporal", 32, 6.0, 14.0),
            lv("Sergeant", 42, 7.0, 15.0),
            lv("Lieutenant", 55, 7.0, 15.0),
            lv("Captain", 72, 7.0, 15.0),
            lv("Major", 94, 8.0, 16.0),
            lv("Lieutenant Colonel", 122, 8.0, 16.0),
            lv("Colonel", 158, 8.0, 15.0),
            lv("General", 206, 9.0, 15.0),
        ],
    },
    BuiltinTrack {
        name: "Music",
        skill: "Guitar",
        days: 0b0111110,
        levels: [
            lv("Roadie", 15, 11.0, 18.0),
            lv("Ticket Taker", 20, 11.0, 18.0),
            lv("Lighting Technician", 27, 12.0, 19.0),
            lv("Session Musician", 36, 13.0, 20.0),
            lv("Backup Musician", 47, 14.0, 21.0),
            lv("Opening Act", 62, 16.0, 22.0),
            lv("Lead Guitarist", 82, 16.0, 22.0),
            lv("Headliner", 108, 17.0, 23.0),
            lv("Rock Star", 142, 18.0, 23.0),
            lv("Rock God", 190, 18.0, 23.0),
        ],
    },
    BuiltinTrack {
        name: "Political",
        skill: "Charisma",
        days: WEEKDAYS,
        levels: [
            lv("Podium Polisher", 15, 9.0, 15.0),
            lv("Campaign Intern", 21, 9.0, 15.0),
            lv("Campaign Worker", 28, 9.0, 16.0),
            lv("Legislative Aide", 37, 9.0, 16.0),
            lv("City Council Member", 49, 10.0, 17.0),
            lv("Mayor", 65, 10.0, 17.0),
            lv("State Senator", 86, 10.0, 17.0),
            lv("Governor", 112, 10.0, 17.0),
            lv("Vice President", 148, 10.0, 16.0),
            lv("Leader of the Free World", 196, 10.0, 16.0),
        ],
    },
    BuiltinTrack {
        name: "Science",
        skill: "Logic",
        days: WEEKDAYS,
        levels: [
            lv("Test Subject", 16, 9.0, 15.0),
            lv("Lab Assistant", 22, 9.0, 15.0),
            lv("Lab Technician", 30, 9.0, 16.0),
            lv("Field Researcher", 40, 9.0, 16.0),
            lv("Research Scientist", 53, 9.0, 17.0),
            lv("Senior Scientist", 70, 9.0, 17.0),
            lv("Project Leader", 92, 10.0, 17.0),
            lv("Research Director", 120, 10.0, 17.0),
            lv("Visionary", 156, 10.0, 16.0),
            lv("Mad Scientist", 204, 10.0, 16.0),
        ],
    },
];

#[derive(Component, Clone, Debug)]
pub struct Job {
    pub track: usize,
    /// 0-based career level.
    pub level: usize,
    /// The path taken (index into the career's paths).
    pub branch: usize,
    /// -100 (about to be demoted) .. 100 (promotion).
    pub performance: f32,
    /// The last day the Sim was at work (for missed-day penalties).
    pub last_day: Option<u32>,
    /// How they go about their work.
    pub tone: WorkTone,
    /// Their boss and coworkers (Sims of the town, by id).
    pub boss: Option<u64>,
    pub coworkers: Vec<u64>,
    /// The highest level reached in this career (0-based; a promotion's bonus only the first
    /// time), and raises (an hour's pay more, kept through promotions).
    pub highest: usize,
    pub raise: i64,
    /// When they were warned their performance was dangerously low (demoted 48 hours on if it
    /// still is).
    pub warned_at: Option<f64>,
    /// How fast their performance is moving at work (an hour; for the Career panel's arrows).
    pub rate: f32,
    /// To be promoted now (the boss agreed to it: the game's `ShouldPromote`).
    pub promote: bool,
}

/// A retired Sim's pension: paid each morning (half of what the job paid, on average, a day).
#[derive(Component, Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Pension {
    pub daily: i64,
    /// The last day it was paid.
    pub paid: u32,
}

impl Job {
    /// The pension retiring from this job brings: half its average day's pay.
    pub fn pension(&self) -> i64 {
        let l = self.info();
        let days = l.days.count_ones() as f32;
        ((l.hourly as f32 * self.hours() * days / 7.0) * 0.5).round() as i64
    }
}

/// Pensions are paid each morning (into the household's funds).
fn pay_pensions(clock: Res<GameClock>, mut household: Option<ResMut<Household>>, mut retired: Query<&mut Pension, With<HouseholdMember>>) {
    let day = clock.day();
    if clock.hour_f() < 9.0 {
        return;
    }
    for mut p in &mut retired {
        if p.paid != day {
            p.paid = day;
            if let Some(h) = household.as_mut() {
                h.funds += p.daily;
            }
        }
    }
}

/// Skill points a minute studying the career's skill at work, as the game's study tones have
/// them: Journalism's `PracticeWriting` and Law Enforcement's `WorkoutTone` 5, the Culinary,
/// Criminal and Music ones 4 (and so the rest).
pub fn study_points(career: &str) -> f32 {
    match career {
        "Journalism" | "Law Enforcement" => 5.0,
        _ => 4.0,
    }
}

/// How a Sim goes about their work, as the game's work tones: hard (faster to promotion, but
/// tiring and no fun), easy (slower, but restful), with the co-workers (sociable), or studying
/// the career's skill.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WorkTone {
    #[default]
    Normal,
    WorkHard,
    TakeItEasy,
    Socialize,
    Study,
    SuckUp,
}

impl WorkTone {
    pub const ALL: [WorkTone; 6] = [WorkTone::Normal, WorkTone::WorkHard, WorkTone::TakeItEasy, WorkTone::Socialize, WorkTone::SuckUp, WorkTone::Study];

    /// Whether the job offers it (the game's `ShouldAddTone`: sucking up takes a boss).
    pub fn offered(self, job: &Job) -> bool {
        self != WorkTone::SuckUp || (job.info().has_boss && job.boss.is_some())
    }

    pub fn label(self, skill: &str) -> String {
        match self {
            WorkTone::Normal => "Work Normally".into(),
            WorkTone::WorkHard => "Work Hard".into(),
            WorkTone::TakeItEasy => "Take It Easy".into(),
            WorkTone::Socialize => "Hang With Co-Workers".into(),
            WorkTone::SuckUp => "Suck Up to Boss".into(),
            WorkTone::Study => format!("Study {skill}"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            WorkTone::Normal => "Normal",
            WorkTone::WorkHard => "WorkHard",
            WorkTone::TakeItEasy => "TakeItEasy",
            WorkTone::Socialize => "Socialize",
            WorkTone::SuckUp => "SuckUp",
            WorkTone::Study => "Study",
        }
    }

    pub fn from_name(s: &str) -> Self {
        Self::ALL.into_iter().find(|t| t.name() == s).unwrap_or_default()
    }

    /// Performance an hour it adds (the game's tones: Work Hard +3, Take It Easy -1.5, nothing
    /// for a Professional Slacker).
    fn performance(self, slacker: bool) -> f32 {
        match self {
            WorkTone::WorkHard => 3.0,
            WorkTone::TakeItEasy if !slacker => -1.5,
            _ => 0.0,
        }
    }
}

/// How performance moves while at work, as the game's `Career.UpdatePerformance` has it: each
/// metric of the level scored -3 to +3 against its mappings (`PerfMetric.CalcMetric`), their
/// average scaling the career's best or worst flow an hour.
pub fn metric_score(value: f32, [n3, z, p3]: [f32; 3]) -> f32 {
    if value == z {
        0.0
    } else if value < z {
        if value <= n3 {
            -3.0
        } else if value < (n3 + z) / 2.0 {
            -2.0
        } else {
            -1.0
        }
    } else if value >= p3 {
        3.0
    } else if value > (p3 + z) / 2.0 {
        2.0
    } else {
        1.0
    }
}

/// The hour's performance at work from the metrics' average score (`MaxPerfFlow` at +3,
/// `MinPerfFlow` at -3; their top-level variants from level 10).
pub fn performance_rate(avg: f32, flow: [f32; 4], top: bool) -> f32 {
    let (max, min) = if top { (flow[2], flow[3]) } else { (flow[0], flow[1]) };
    if avg > 0.0 { avg / 3.0 * max } else { avg / -3.0 * min }
}

/// How much more Charisma makes of time spent on the boss and coworkers (the Charisma skill's
/// `Improve_Relatinoship_With_Boss` and `Improve_Coworker_Relatinoships`, by level).
fn charisma_boost(level: u32, boss: bool) -> f32 {
    const BOSS: [f32; 10] = [1.0, 1.01, 1.02, 1.03, 1.05, 1.10, 1.20, 1.32, 1.49, 1.72];
    const COWORKERS: [f32; 10] = [1.0, 1.01, 1.02, 1.03, 1.05, 1.10, 1.20, 1.35, 1.55, 1.77];
    if level == 0 {
        return 1.0;
    }
    let i = (level as usize - 1).min(9);
    if boss { BOSS[i] } else { COWORKERS[i] }
}

impl Job {
    pub fn new(track: usize) -> Self {
        Self { track, level: 0, branch: 0, performance: 0.0, last_day: None, tone: WorkTone::Normal, boss: None, coworkers: Vec::new(), highest: 0, raise: 0, warned_at: None, rate: 0.0, promote: false }
    }
    /// A raise (the game's `GiveRaise`): 15% of the level's pay an hour more, 10% at the top
    /// levels; what it came to.
    pub fn give_raise(&mut self) -> i64 {
        let r = ((self.info().hourly as f32 * if self.level + 1 >= 10 { 0.10 } else { 0.15 }) as i64).max(1);
        self.raise += r;
        r
    }
    pub fn career(&self) -> &'static CareerTrack {
        &careers()[self.track.min(careers().len() - 1)]
    }
    pub fn path(&self) -> &'static CareerPath {
        let paths = &self.career().paths;
        &paths[self.branch.min(paths.len() - 1)]
    }
    pub fn levels(&self) -> &'static [CareerLevel] {
        &self.path().levels
    }
    pub fn info(&self) -> &'static CareerLevel {
        let levels = self.levels();
        &levels[self.level.min(levels.len() - 1)]
    }
    /// Past the branch: the path's name ("Thief").
    pub fn branch_label(&self) -> Option<String> {
        self.career().branch_at.filter(|&b| self.level >= b).map(|_| self.path().label())
    }
    /// The uniform a Sim wears to work at their level, if it has one.
    pub fn uniform(&self, sim: &Sim) -> Option<&'static str> {
        let o = &self.info().outfits;
        let i = sim.female as usize;
        let elder = (sim.age == Age::Elder).then(|| o[2 + i]).filter(|s| !s.is_empty());
        elder.or(Some(o[i])).filter(|s| !s.is_empty())
    }
    pub fn hours(&self) -> f32 {
        let l = self.info();
        if l.end > l.start { l.end - l.start } else { l.end + 24.0 - l.start }
    }
    pub fn works_on(&self, weekday: usize) -> bool {
        self.info().days & (1 << weekday) != 0
    }
    /// e.g. "Business — Junior Clerk (level 2)".
    pub fn describe(&self) -> String {
        format!("{} — {} (level {})", self.career().name, self.info().title, self.level + 1)
    }
}

#[derive(Component)]
pub struct AtWork {
    pub until: f64,
    /// When they got there, and their performance then.
    pub from: f64,
    pub start_perf: f32,
}

/// How long a warning about poor work stands before a demotion (the game's 48 hours).
const DEMOTION_WARNING_MINUTES: f64 = 48.0 * 60.0;

/// Head to work before the shift; come home with pay and a performance review.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn work_schedule(
    mut commands: Commands,
    clock: Res<GameClock>,
    exit: Option<Res<LotExit>>,
    world: Option<Res<CurrentWorld>>,
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    mut life: MessageWriter<LifeEvent>,
    mut workers: Query<
        (
            Entity,
            &Sim,
            &mut Job,
            &mut ActionQueue,
            Option<&AtWork>,
            &mut Transform,
            &mut Motives,
            &Mood,
            &mut Skills,
            Option<&crate::lifetime::LifetimeWish>,
            Has<HouseholdMember>,
            Option<&crate::wishes::Wishes>,
            &mut crate::social::Relationships,
        ),
        Without<crate::rabbitholes::AtRabbitHole>,
    >,
    mut session_start: Local<Option<f64>>,
    mut questions: ResMut<crate::dialog::Questions>,
    weather: Option<Res<crate::weather::Weather>>,
    people: Query<(Entity, &Sim)>,
) {
    let h = clock.hour_f();
    let day = clock.day();
    let ids: std::collections::HashMap<u64, Entity> = people.iter().map(|(e, s)| (s.id, e)).collect();
    // (The boss and coworkers have seen them today: theirs, applied after.)
    let mut seen: Vec<(Entity, Entity)> = Vec::new();
    // A shift already under way when play began isn't held against anyone.
    let started = *session_start.get_or_insert(clock.minutes);
    for (e, sim, mut job, mut queue, at_work, mut tf, mut motives, mood, mut skills, ltw, member, wishes, mut rels) in &mut workers {
        let info = job.info();
        if let Some(w) = at_work {
            if clock.minutes < w.until {
                continue;
            }
            if let (Some(x), Some(world)) = (&exit, &world) {
                tf.translation = Vec3::new(x.0.x, world.data.heightmap.sample(x.0.x, x.0.y), x.0.y);
            }
            // (Back on the ground, whichever floor they left from.)
            commands.entity(e).remove::<AtWork>().insert((Visibility::Inherited, crate::nav::Floor(1)));
            // (The town's careers move only with the story, as the game's.)
            if !member {
                continue;
            }
            // Home from work: pay for the hours worked, needs, and the performance the shift
            // earned, as the game's careers have it.
            let worked = ((w.until - w.from) / 60.0).clamp(0.0, job.hours() as f64) as f32;
            let pay = ((info.hourly + job.raise) as f32 * worked) as i64;
            if let Some(hh) = household.as_mut() {
                hh.funds += pay;
            }
            let track = job.career();
            let workaholic = sim.traits.contains(&Trait::Workaholic);
            // (Who they spent the day with, for the needs below.)
            let boss = job.boss.and_then(|b| ids.get(&b).copied());
            let coworkers: Vec<Entity> = job.coworkers.iter().filter_map(|c| ids.get(c).copied()).collect();
            let gain = job.performance - w.start_perf;
            life.write(LifeEvent::new(e, LifeEventKind::HomeFromWork { worse: gain < 0.0 }));
            // Studying the career's skill on the job.
            if job.tone == WorkTone::Study {
                let sk = track.skill;
                let points = worked * 60.0 * study_points(track.name) * crate::life::skill_rate(&sim.traits, sk) * crate::wishes::reward_skill_rate(wishes) * mood.skill_rate();
                if let Some(level) = skills.add_points(sk, points)
                    && member
                {
                    notes.push(format!("{} reached level {level} in {sk}!", sim.first));
                    life.write(LifeEvent::new(e, LifeEventKind::SkillUp { skill: sk, level }));
                }
            }
            notes.push(format!("{} is home from work and earned §{pay}.", sim.first));
            // (A Kleptomaniac's pockets: see `klepto`.)
            commands.entity(e).insert(crate::klepto::DayDone((job.level as i32 + 1) * crate::klepto::PER_CAREER_LEVEL));
            if job.performance >= 100.0 && job.level + 1 < job.levels().len() {
                promote(&mut job, e, sim, ltw, member, household.as_deref_mut(), &mut notes, &mut questions, &mut life);
            } else if job.performance >= 100.0 {
                // (At the top, a raise instead, and back to work at it.)
                job.give_raise();
                job.performance = 0.0;
                notes.push(format!("{} got a raise for outstanding work: §{} an hour now.", sim.first, info.hourly + job.raise));
            }
            demotion(&mut commands, &mut notes, &mut life, e, sim, &mut job, &mut motives, clock.minutes, member);
            // Needs: lunch halfway through (the game's, so hungry no earlier than half a shift's
            // worth), the bathroom there, the tone's fun (the job's own drains as they work) and the
            // company.
            motives.0[HUNGER] = (motives.0[HUNGER] - 11.25 * worked).max(100.0 - 11.25 * worked / 2.0);
            motives.0[BLADDER] = 100.0;
            let tone_fun = match job.tone {
                WorkTone::TakeItEasy => 20.0 * worked,
                WorkTone::WorkHard => -10.0 * if workaholic { 0.75 } else { 1.0 } * worked,
                _ => 0.0,
            };
            motives.add(FUN, tone_fun);
            motives.add(SOCIAL, (10.0 + if job.tone == WorkTone::Socialize { 50.0 } else { 0.0 }) * worked);
            // (They've seen their boss and coworkers today.)
            for x in boss.iter().chain(&coworkers) {
                rels.entry(*x).last = clock.minutes;
                seen.push((*x, e));
            }
            continue;
        }
        let workday = job.works_on(clock.weekday());
        let shift_began = day as f64 * 1440.0 + (info.start as f64 - 0.75) * 60.0;
        if workday && started > shift_began && clock.minutes - started < 24.0 * 60.0 && job.last_day != Some(day) && h > info.start + 2.0 {
            job.last_day = Some(day);
            continue;
        }
        // A missed shift costs performance: the career's for every hour of it (a fifth of it
        // for a Vacationer), and a Workaholic misses it.
        if workday && h > info.start + 2.0 && h < info.start + 3.0 && job.last_day != Some(day) {
            job.last_day = Some(day);
            if !member {
                continue;
            }
            let vacationer = crate::wishes::has(wishes, "Vacationer");
            let penalty = job.career().miss_work * job.hours() * if vacationer { 0.2 } else { 1.0 };
            job.performance = (job.performance + penalty).max(-100.0);
            if sim.traits.contains(&Trait::Workaholic) {
                motives.add(FUN, -15.0 * job.hours());
            }
            notes.push(if vacationer {
                format!("{} took the day off work. Vacationer softened the performance penalty.", sim.first)
            } else {
                format!("{} missed work today! Their boss is not pleased.", sim.first)
            });
            demotion(&mut commands, &mut notes, &mut life, e, sim, &mut job, &mut motives, clock.minutes, member);
            continue;
        }
        // (A holiday's a paid day off, as the game's `TryGiveDayOffForHoliday`.)
        if workday
            && job.last_day != Some(day)
            && h >= info.start - 0.75
            && let Some(name) = weather.as_deref().and_then(|w| crate::holidays::holiday_today(w, day))
        {
            job.last_day = Some(day);
            let pay = (info.hourly as f32 * job.hours()) as i64;
            if member {
                if let Some(hh) = household.as_mut() {
                    hh.funds += pay;
                }
                notes.push(format!("{} has {name} off work, with pay (§{pay}).", sim.first));
            }
            continue;
        }
        let going = queue.0.iter().any(|a| matches!(a.kind, ActionKind::GoToWork));
        if workday && h >= info.start - 0.75 && h < info.start + 1.0 && !going && job.last_day != Some(day) {
            for a in queue.0.iter_mut() {
                a.cancel = true;
            }
            queue.0.push_back(Action::new("Go to Work", ActionKind::GoToWork, false));
        }
    }
    let now = clock.minutes;
    for (x, worker) in seen {
        commands.entity(x).queue(move |mut ew: EntityWorldMut| {
            if let Some(mut r) = ew.get_mut::<crate::social::Relationships>() {
                r.entry(worker).last = now;
            }
        });
    }
}

/// What a performance metric's called, plainly.
pub fn metric_label(m: &s3bake::gamedata::CareerMetric) -> String {
    match m.kind.as_str() {
        "Mood" => "mood".into(),
        "RelBoss" => "the boss".into(),
        "RelCoworkers" | "RelPartner" => "co-workers".into(),
        "SkillX" => m.arg.clone(),
        k => k.into(),
    }
}

/// What the level's metrics come to now: each this game can judge, with its score (-3 to +3;
/// the game's `PerfMetric.FindMetricValue`).
pub fn metric_scores(info: &CareerLevel, mood: f32, skills: &Skills, rels: &crate::social::Relationships, boss: Option<Entity>, coworkers: &[Entity]) -> Vec<(&'static s3bake::gamedata::CareerMetric, f32)> {
    let liking = |x: Entity| rels.get(x).friendship;
    info.metrics
        .iter()
        .filter_map(|m| {
            let v = match m.kind.as_str() {
                "Mood" => mood.clamp(-100.0, 150.0),
                "RelBoss" => boss.map_or(0.0, liking),
                "RelCoworkers" | "RelPartner" => {
                    if coworkers.is_empty() { 0.0 } else { coworkers.iter().map(|c| liking(*c)).sum::<f32>() / coworkers.len() as f32 }
                }
                "SkillX" => skills.level(&m.arg) as f32,
                // (What there's no way to do here yet isn't held against them.)
                _ => return None,
            };
            Some((m, metric_score(v, m.mappings)))
        })
        .collect()
}

/// Performance an hour at work: the metrics' average through the career's flow, then the
/// tone's, the Ambitious's and a Multi-Tasker's.
pub fn hourly_performance(job: &Job, scores: &[(&s3bake::gamedata::CareerMetric, f32)], traits: &[Trait], wishes: Option<&crate::wishes::Wishes>) -> f32 {
    let avg = if scores.is_empty() { 0.0 } else { scores.iter().map(|s| s.1).sum::<f32>() / scores.len() as f32 };
    let mut rate = performance_rate(avg, job.career().flow, job.level + 1 >= 10) + job.tone.performance(crate::wishes::has(wishes, "ProfessionalSlacker"));
    if traits.contains(&Trait::Ambitious) {
        rate += 2.0;
    }
    if crate::wishes::has(wishes, "MultiTasker") {
        rate += 3.0;
    }
    rate
}

/// At work: performance flows by the level's metrics, and the boss and coworkers warm to time
/// spent on them (Suck Up 3 an hour, Hanging With Co-Workers 5 among them; more for the
/// charismatic, two and a half times for a Schmoozer), as the game's careers have it.
#[allow(clippy::type_complexity)]
#[allow(clippy::type_complexity)]
fn work_flow(
    delta: Res<crate::clock::SimDelta>,
    clock: Res<GameClock>,
    mut commands: Commands,
    mut workers: Query<
        (Entity, &Sim, &mut Job, &Mood, &Skills, &mut crate::social::Relationships, Option<&crate::wishes::Wishes>, &Motives, &mut crate::life::Moodlets),
        (With<AtWork>, With<HouseholdMember>),
    >,
    people: Query<(Entity, &Sim)>,
    mut notes: ResMut<Notifications>,
) {
    let hours = delta.0 / 60.0;
    if hours <= 0.0 || workers.is_empty() {
        return;
    }
    let ids: std::collections::HashMap<u64, Entity> = people.iter().map(|(e, s)| (s.id, e)).collect();
    for (e, sim, mut job, mood, skills, mut rels, wishes, motives, mut moodlets) in &mut workers {
        // (Working hard till there's no fun left: Overworked, and back to working normally, as
        // the game's Work Hard tone; a Workaholic never minds.)
        if job.tone == WorkTone::WorkHard && motives.0[FUN] < -40.0 && !sim.traits.contains(&Trait::Workaholic) {
            job.tone = WorkTone::Normal;
            moodlets.add(crate::life::MoodletKind::Overworked, clock.minutes);
            notes.push(format!("{} is overworked and can't keep working this hard.", sim.first));
        }
        let boss = job.boss.and_then(|b| ids.get(&b).copied());
        let coworkers: Vec<Entity> = job.coworkers.iter().filter_map(|c| ids.get(c).copied()).collect();
        let charm = if sim.traits.contains(&Trait::Schmoozer) { 2.5 } else { 1.0 };
        let charisma = skills.level("Charisma");
        let mut warmed: Vec<(Entity, f32)> = Vec::new();
        if job.tone == WorkTone::SuckUp
            && let Some(b) = boss
        {
            warmed.push((b, 3.0 * charisma_boost(charisma, true) * charm * hours));
        }
        if job.tone == WorkTone::Socialize && !coworkers.is_empty() {
            let d = 5.0 * charisma_boost(charisma, false) * charm * hours / coworkers.len() as f32;
            warmed.extend(coworkers.iter().map(|c| (*c, d)));
        }
        for (x, d) in warmed {
            rels.add(x, d, 0.0);
            commands.entity(x).queue(move |mut ew: EntityWorldMut| {
                if let Some(mut r) = ew.get_mut::<crate::social::Relationships>() {
                    r.add(e, d, 0.0);
                }
            });
        }
        let scores = metric_scores(job.info(), mood.0, skills, &rels, boss, &coworkers);
        let rate = hourly_performance(&job, &scores, &sim.traits, wishes);
        job.rate = rate;
        job.performance = (job.performance + rate * hours).clamp(-100.0, 100.0);
    }
}

/// The boss and coworkers a job comes with (the game's `CheckBoss` and `CheckCoworkers`): Sims
/// of the town outside the household, those in the same career first, then anyone grown and
/// out of work, who's hired; the boss is at least level 5 in it. New ones are acquaintances.
#[allow(clippy::type_complexity)]
fn find_colleagues(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut workers: Query<(Entity, &Sim, &mut Job, &mut crate::social::Relationships), With<HouseholdMember>>,
    town: Query<(Entity, &Sim, Option<&Job>, Has<Pension>, Option<&crate::town::Townie>), (Or<(With<crate::interact::OffLot>, With<crate::town::Townie>)>, Without<HouseholdMember>, Without<crate::ghosts::Ghost>)>,
    mut next: Local<f64>,
) {
    // (Now and then is often enough.)
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 60.0;
    use rand::seq::SliceRandom;
    let mut rng = rand::rng();
    // (Townspeople out on a stroll aren't taken till they're home again.)
    let people: std::collections::HashMap<u64, (Entity, Option<(usize, usize)>, bool, bool)> = town
        .iter()
        .filter(|(_, s, _, _, t)| s.age.is_grown() && !t.is_some_and(|t| t.walking))
        .map(|(e, s, j, retired, t)| (s.id, (e, j.map(|j| (j.track, j.level)), retired, t.is_some())))
        .collect();
    // (Whoever's been taken this round, so two of the household don't hire the same Sim twice.)
    let mut hired: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for (e, sim, mut job, mut rels) in &mut workers {
        let track = job.track;
        let has_boss = job.info().has_boss;
        let (min, max) = job.career().coworkers;
        let still = |id: &u64| people.contains_key(id);
        let stale = job.boss.is_some_and(|b| !still(&b) || !has_boss) || job.coworkers.iter().any(|c| !still(c));
        let short = (has_boss && job.boss.is_none()) || job.coworkers.len() < min as usize;
        if !stale && !short {
            continue;
        }
        if !has_boss {
            job.boss = None;
        }
        job.boss = job.boss.filter(still);
        job.coworkers.retain(still);
        // The candidates: the career's own, then the out of work; each shuffled.
        let taken: Vec<u64> = job.boss.iter().chain(&job.coworkers).copied().collect();
        let mut same = Vec::new();
        let mut idle = Vec::new();
        for (&id, &(_, j, retired, _)) in &people {
            if id == sim.id || taken.contains(&id) {
                continue;
            }
            match j.map(|(t, _)| t).or(hired.get(&id).copied()) {
                Some(t) if t == track => same.push(id),
                None if !retired => idle.push(id),
                _ => {}
            }
        }
        same.shuffle(&mut rng);
        idle.shuffle(&mut rng);
        let mut pool = same.into_iter().map(|id| (id, false)).chain(idle.into_iter().map(|id| (id, true)));
        let mut new = Vec::new();
        if has_boss && job.boss.is_none()
            && let Some((id, hire)) = pool.next()
        {
            job.boss = Some(id);
            new.push((id, hire, true));
        }
        if job.coworkers.len() < min as usize {
            let want = rng.random_range(min..=max.max(min)) as usize;
            while job.coworkers.len() < want {
                let Some((id, hire)) = pool.next() else { break };
                job.coworkers.push(id);
                new.push((id, hire, false));
            }
        }
        let now = clock.minutes;
        let levels = job.levels().len();
        if !new.is_empty() {
            info!("{} works for {:?} with {} coworkers ({} new)", sim.first, job.boss, job.coworkers.len(), new.len());
        }
        for (id, hire, boss) in new {
            let (other, j, _, townie) = people[&id];
            rels.entry(other).last = now;
            // (Someone they know now: kept with the household's save, as a party's guests are.)
            if townie {
                commands.entity(other).remove::<(crate::town::Townie, crate::nav::PathFollow)>().insert((crate::interact::OffLot, Visibility::Hidden)).insert_if_new(Skills::default());
            }
            let worker = e;
            // (A boss is at least level 5; whoever's hired starts at the bottom.)
            let at_least = if boss { 4.min(levels - 1) } else { 0 };
            if hire {
                hired.insert(id, track);
                let mut j = Job::new(track);
                j.level = at_least;
                j.highest = at_least;
                commands.entity(other).insert(j);
            } else if let Some((_, level)) = j
                && level < at_least
            {
                commands.entity(other).queue(move |mut ew: EntityWorldMut| {
                    if let Some(mut j) = ew.get_mut::<Job>() {
                        j.level = j.level.max(at_least);
                    }
                });
            }
            commands.entity(other).queue(move |mut ew: EntityWorldMut| {
                if let Some(mut r) = ew.get_mut::<crate::social::Relationships>() {
                    r.entry(worker).last = now;
                }
            });
        }
    }
}

/// A promotion (the game's `PromoteIfShould`): the bonus at the old pay, the first time at the
/// new level only; the path to take where the career branches; performance back to nothing.
#[allow(clippy::too_many_arguments)]
fn promote(
    job: &mut Job,
    e: Entity,
    sim: &Sim,
    ltw: Option<&crate::lifetime::LifetimeWish>,
    member: bool,
    household: Option<&mut Household>,
    notes: &mut Notifications,
    questions: &mut crate::dialog::Questions,
    life: &mut MessageWriter<LifeEvent>,
) {
    let track = job.career();
    let bonus = if job.level + 1 > job.highest { (track.bonus_hours * (job.info().hourly + job.raise) as f32) as i64 } else { 0 };
    job.level += 1;
    job.highest = job.highest.max(job.level);
    job.performance = 0.0;
    job.warned_at = None;
    let branching = track.branch_at == Some(job.level);
    if branching {
        // The path their lifetime wish lies along, or the first.
        job.branch = ltw.and_then(|w| w.def().career_branch(track.name)).and_then(|b| track.path_index(b)).unwrap_or(0);
    }
    if let Some(hh) = household {
        hh.funds += bonus;
    }
    let got = if bonus > 0 { format!(" and got a §{bonus} bonus") } else { String::new() };
    if branching && member {
        notes.push(format!("{} was promoted{got}! The {} career branches here.", sim.first, track.name));
        questions.ask_career_path(e, sim, job);
    } else {
        notes.push(format!("{} was promoted to {}{got}!", sim.first, job.info().title_for(sim.female)));
    }
    life.write(LifeEvent::new(e, LifeEventKind::Promoted));
}

/// Promoted at once when the boss agreed to it ("Ask for a Promotion" taken well).
fn promote_when_agreed(
    mut jobs: Query<(Entity, &Sim, &mut Job, Option<&crate::lifetime::LifetimeWish>, Has<HouseholdMember>)>,
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    mut questions: ResMut<crate::dialog::Questions>,
    mut life: MessageWriter<LifeEvent>,
) {
    for (e, sim, mut job, ltw, member) in &mut jobs {
        if !job.promote {
            continue;
        }
        job.promote = false;
        if job.level + 1 < job.levels().len() {
            promote(&mut job, e, sim, ltw, member, household.as_deref_mut(), &mut notes, &mut questions, &mut life);
        }
    }
}

/// One of what a work day may bring (the game's `CareerEventManager`): a raise for good work, a
/// Loser's bad luck, or one of the career's own events (an index into its `events`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Due {
    Raise,
    Loser,
    Event(usize),
}

/// The day's events, each at its time (game minutes), while they're at work.
#[derive(Component, Default, Debug)]
pub struct EventsDue(pub Vec<(f64, Due)>);

/// A choice made in a career event's dialog, to be carried out.
#[derive(Component)]
pub struct EventChoice {
    pub event: usize,
    pub yes: bool,
}

/// Whether one of the career's events can come today: at their level and on their path, and,
/// one that offers an opportunity, with it there to offer and no other career opportunity on.
pub fn event_open(e: &s3bake::gamedata::CareerEventInfo, job: &Job, opportunity_ok: impl Fn(&str) -> bool) -> bool {
    let level = job.level as u32 + 1;
    e.likelihood > 0.0
        && (e.levels.is_empty() || e.levels.contains(&level))
        && (e.branch.eq_ignore_ascii_case("All") || e.branch.eq_ignore_ascii_case(job.path().branch))
        && (e.kind != "Opportunity" || opportunity_ok(&e.opportunity))
}

/// The day's event (the game's `RollForEvent`): none as likely as every candidate not coming
/// (the product of their chances against), else one by its likelihood; a sure one besides.
pub fn roll_events(cands: &[(Due, f32)], sure: &[Due], rng: &mut impl rand::Rng) -> Vec<Due> {
    let mut out = Vec::new();
    let none: f32 = cands.iter().map(|c| 1.0 - c.1.clamp(0.0, 1.0)).product();
    if !cands.is_empty() && rng.random::<f32>() >= none {
        let total: f32 = cands.iter().map(|c| c.1).sum();
        let mut x = rng.random::<f32>() * total;
        let pick = cands.iter().find(|c| {
            x -= c.1;
            x <= 0.0
        });
        out.extend(pick.or(cands.last()).map(|c| c.0));
    }
    out.extend_from_slice(sure);
    out
}

/// At the start of a shift, what the day will bring: the raise for good work (1% likely at
/// nothing to 25% at the best, twice as likely for the Lucky), a Loser's bad luck (half the
/// time), the career's own events by their likelihood; each at a time in the shift.
#[allow(clippy::type_complexity)]
fn roll_career_events(
    mut commands: Commands,
    clock: Res<GameClock>,
    started: Query<(Entity, &Sim, &Job, &AtWork, Option<&crate::opportunities::SimOpportunities>), (Added<AtWork>, With<HouseholdMember>)>,
    ui: Option<Res<crate::icons::GameUi>>,
) {
    let mut rng = rand::rng();
    for (e, sim, job, w, opps) in &started {
        let hours_left = ((w.until - clock.minutes) / 60.0) as f32;
        if hours_left < 0.5 {
            continue;
        }
        let track = job.career();
        let data = ui.as_ref().map(|u| u.data.clone());
        // (An opportunity's to offer if it's one of those there are, not done already, and no
        // other career opportunity is on.)
        let on_career = opps.is_some_and(|o| o.active.iter().any(|a| data.as_ref().and_then(|d| d.opportunities.get(a.index)).is_some_and(|x| !x.career.is_empty())));
        let opportunity_ok = |guid: &str| {
            !on_career
                && data.as_ref().is_some_and(|d| d.opportunities.iter().any(|o| o.guid == guid && (o.repeat || !opps.is_some_and(|x| x.done.iter().any(|g| g == guid)))))
        };
        let luck = if sim.traits.contains(&Trait::Lucky) { 2.0 } else if sim.traits.contains(&Trait::Unlucky) { 0.5 } else { 1.0 };
        let mut cands: Vec<(Due, f32)> = Vec::new();
        if job.performance > 0.0 {
            cands.push((Due::Raise, (job.performance / 100.0 * 0.24 + 0.01) * luck));
        }
        if sim.traits.contains(&Trait::Loser) {
            cands.push((Due::Loser, 0.5));
        }
        let mut sure = Vec::new();
        for (i, ev) in track.events.iter().enumerate() {
            if event_open(ev, job, opportunity_ok) {
                if ev.sure { sure.push(Due::Event(i)) } else { cands.push((Due::Event(i), ev.likelihood)) }
            }
        }
        // (CAREER_EVENT=<key>: that one, twenty minutes in, for tests.)
        if let Ok(k) = std::env::var("CAREER_EVENT")
            && let Some(i) = track.events.iter().position(|ev| ev.key == k)
        {
            commands.entity(e).insert(EventsDue(vec![(clock.minutes + 20.0, Due::Event(i))]));
            continue;
        }
        debug!("{}'s work day: {} events could come, {:.0}% likely none", sim.first, cands.len() + sure.len(), cands.iter().map(|c| 1.0 - c.1.clamp(0.0, 1.0)).product::<f32>() * 100.0);
        let due: Vec<(f64, Due)> = roll_events(&cands, &sure, &mut rng)
            .into_iter()
            .map(|d| {
                // (An opportunity comes an hour in at the earliest; the rest any time but the end.)
                let opportunity = matches!(d, Due::Event(i) if track.events[i].kind == "Opportunity");
                let h = if opportunity { rng.random_range(1.0..hours_left.max(1.01)) } else { rng.random_range(0.0..(hours_left - 0.2).max(0.01)) };
                (clock.minutes + h as f64 * 60.0, d)
            })
            .collect();
        if !due.is_empty() {
            debug!("{}'s work day: {:?}", sim.first, due);
            commands.entity(e).insert(EventsDue(due));
        }
    }
}

/// A career event's words for this Sim (a woman's where the game has them).
fn event_words(t: &[String; 2], sim: &Sim) -> String {
    t[sim.female as usize].replace("{0.SimFirstName}", &sim.first).replace("{0.SimName}", &sim.full_name())
}

/// A choice's chance of coming off (the game's `CheckForWin`): its own, plus each modifier's
/// share by how they stand (mood, a skill's level of ten, liking with the boss or co-workers).
pub fn win_chance(e: &s3bake::gamedata::CareerEventInfo, mood: f32, skills: &Skills, boss: f32, coworkers: Option<f32>) -> f32 {
    if e.chance >= 100.0 {
        return 100.0;
    }
    let mut c = e.chance;
    for m in &e.modifiers {
        let p: Vec<&str> = m.split(',').map(str::trim).collect();
        let amount = |i: usize| p.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        c += match p[0] {
            "Mood" => mood / 100.0 * amount(1),
            "Skill" => skills.level(p.get(1).copied().unwrap_or("")) as f32 / 10.0 * amount(2),
            "RelationshipWithBoss" => boss / 100.0 * amount(1),
            "RelationshipWithCoworkers" => coworkers.map_or(0.0, |l| l / 100.0 * amount(1)),
            _ => 0.0,
        };
    }
    c.min(100.0)
}

/// The moodlet a game buff gives, by its name.
fn buff_moodlet(name: &str) -> Option<crate::life::MoodletKind> {
    let name = if name == "Singed" { "SingedElectricity" } else { name };
    crate::life::MoodletKind::ALL.into_iter().find(|k| k.buff() == (name, true))
}

/// What a career event gives (the game's `RewardsManager`): performance, a raise (a percent of
/// the level's pay), a promotion, demotion or the sack, liking with the boss or co-workers (both
/// ways), money, a moodlet.
#[allow(clippy::too_many_arguments)]
fn give_rewards(
    rewards: &[String],
    e: Entity,
    sim: &Sim,
    job: &mut Job,
    rels: &mut crate::social::Relationships,
    moodlets: &mut crate::life::Moodlets,
    colleagues: (Option<Entity>, &[Entity]),
    now: f64,
    commands: &mut Commands,
    household: Option<&mut Household>,
    notes: &mut Notifications,
    life: &mut MessageWriter<LifeEvent>,
) {
    let mut funds = household;
    let warm = |x: Entity, d: f32, rels: &mut crate::social::Relationships, commands: &mut Commands| {
        rels.add(x, d, 0.0);
        commands.entity(x).queue(move |mut ew: EntityWorldMut| {
            if let Some(mut r) = ew.get_mut::<crate::social::Relationships>() {
                r.add(e, d, 0.0);
            }
        });
    };
    for r in rewards {
        let p: Vec<&str> = r.split(',').map(str::trim).collect();
        let n = |i: usize| p.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        match p[0] {
            "CareerPerformance" => job.performance = (job.performance + n(1)).clamp(-100.0, 100.0),
            "CareerRaise" => {
                let r = ((job.info().hourly as f32 * n(1) / 100.0) as i64).max(1);
                job.raise += r;
            }
            "CareerPromotion" => job.promote = true,
            "CareerDemotion" if job.level > 0 => {
                job.level -= 1;
                job.performance = 0.0;
                notes.push(format!("{} was demoted to {}.", sim.first, job.info().title_for(sim.female)));
                life.write(LifeEvent::new(e, LifeEventKind::Demoted));
            }
            "CareerFired" | "CareerDemotion" => {
                notes.push(format!("{} was fired from the {} career.", sim.first, job.career().name));
                commands.entity(e).remove::<Job>();
                life.write(LifeEvent::new(e, LifeEventKind::Fired));
            }
            "RelationshipWithBoss" => {
                if let Some(b) = colleagues.0 {
                    warm(b, n(1), rels, commands);
                }
            }
            "RelationshipWithCoworkers" => {
                for &c in colleagues.1 {
                    warm(c, n(1), rels, commands);
                }
            }
            "Money" => {
                if let Some(h) = funds.as_deref_mut() {
                    h.funds += n(1) as i64;
                }
            }
            "Buff" => {
                if let Some(k) = p.get(1).and_then(|b| buff_moodlet(b)) {
                    moodlets.add(k, now);
                }
            }
            // (Objects, fish, a group meal, strangers, gigs: not had here.)
            _ => {}
        }
    }
}

/// The day's events as their times come, while they're still at work: the raise, a Loser's
/// luck, a message with its rewards, a choice put to the player, an opportunity offered; and the
/// choices made, carried out.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn run_career_events(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut workers: Query<
        (
            Entity,
            &Sim,
            &mut Job,
            Option<&mut EventsDue>,
            Has<AtWork>,
            Option<&EventChoice>,
            &mut crate::social::Relationships,
            &mut crate::life::Moodlets,
            &Skills,
            &Mood,
        ),
        (With<HouseholdMember>, Or<(With<EventsDue>, With<EventChoice>)>),
    >,
    people: Query<(Entity, &Sim)>,
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    mut questions: ResMut<crate::dialog::Questions>,
    mut life: MessageWriter<LifeEvent>,
    mut offers: MessageWriter<crate::opportunities::OfferThis>,
    ui: Option<Res<crate::layout::UiAssets>>,
) {
    let now = clock.minutes;
    for (e, sim, mut job, due, at_work, choice, mut rels, mut moodlets, skills, mood) in &mut workers {
        let find = |id: u64| people.iter().find(|(_, s)| s.id == id).map(|(x, _)| x);
        let boss = job.boss.and_then(find);
        let coworkers: Vec<Entity> = job.coworkers.iter().filter_map(|c| find(*c)).collect();
        let events = job.career().events;
        // A choice made: it comes off or it doesn't, and its rewards.
        if let Some(c) = choice {
            commands.entity(e).remove::<EventChoice>();
            if let Some(ev) = events.get(c.event) {
                let (text, rewards) = if c.yes {
                    let mean = (!coworkers.is_empty()).then(|| coworkers.iter().map(|x| rels.get(*x).friendship).sum::<f32>() / coworkers.len() as f32);
                    let chance = win_chance(ev, mood.0, skills, boss.map_or(0.0, |b| rels.get(b).friendship), mean);
                    if rand::rng().random_range(0.0..100.0) < chance { (&ev.yes_result, &ev.rewards_yes) } else { (&ev.yes_lose_result, &ev.rewards_yes_lose) }
                } else {
                    (&ev.no_result, &ev.rewards_no)
                };
                let words = event_words(text, sim);
                if !words.is_empty() {
                    notes.push(words);
                }
                give_rewards(rewards, e, sim, &mut job, &mut rels, &mut moodlets, (boss, &coworkers), now, &mut commands, household.as_deref_mut(), &mut notes, &mut life);
            }
        }
        let Some(mut due) = due else { continue };
        if !at_work {
            commands.entity(e).remove::<EventsDue>();
            continue;
        }
        let Some(i) = due.0.iter().position(|(t, _)| *t <= now) else { continue };
        let (_, d) = due.0.remove(i);
        match d {
            // (A raise only if they're doing well and getting better.)
            Due::Raise if job.performance > 0.0 && job.rate > 0.0 => {
                let r = job.give_raise();
                let line = ui.as_ref().and_then(|u| u.localize(&format!("Gameplay/Careers/Career:EventRaise{}", if sim.female { "_Female" } else { "" })));
                notes.push(line.map_or_else(
                    || format!("{}'s hard work has paid off: a raise of §{r} an hour.", sim.first),
                    |t| t.replace("{0.SimFirstName}", &sim.first).replace("{1.Money}", &format!("§{r}")),
                ));
            }
            Due::Raise => {}
            Due::Loser => notes.push(format!("{} had another bad day at work. Nothing ever seems to go right!", sim.first)),
            Due::Event(i) => {
                let Some(ev) = events.get(i) else { continue };
                match ev.kind.as_str() {
                    "Choice" => {
                        let track = job.career();
                        let answer = |t: &[String; 2], plain: &str| crate::dialog::Answer { label: if t[0].is_empty() { plain.into() } else { event_words(t, sim) }, detail: String::new(), icon: String::new() };
                        questions.ask(crate::dialog::Ask {
                            about: crate::dialog::Question::CareerEvent { sim: e, event: i },
                            icon: track.icon.to_string(),
                            heading: format!("{} at work", sim.first),
                            title: job.info().title_for(sim.female).to_string(),
                            text: event_words(&ev.text, sim),
                            answers: vec![answer(&ev.yes, "Yes"), answer(&ev.no, "No")],
                        });
                    }
                    "Opportunity" => {
                        offers.write(crate::opportunities::OfferThis { sim: e, guid: ev.opportunity.clone() });
                    }
                    _ => {
                        let words = event_words(&ev.text, sim);
                        if !words.is_empty() {
                            notes.push(words);
                        }
                        give_rewards(&ev.rewards_yes, e, sim, &mut job, &mut rels, &mut moodlets, (boss, &coworkers), now, &mut commands, household.as_deref_mut(), &mut notes, &mut life);
                    }
                }
            }
        }
    }
}

/// The player's answer to a career event's choice: yes (the first) or no.
fn career_event_answers(mut commands: Commands, mut answers: MessageReader<crate::dialog::Answered>) {
    for a in answers.read() {
        if let crate::dialog::Question::CareerEvent { sim, event } = a.about {
            commands.entity(sim).insert(EventChoice { event, yes: a.answer == 0 });
        }
    }
}

/// What a performance review comes to (the game's `DemotionChecker`).
#[derive(Debug, PartialEq)]
pub enum Review {
    Fine,
    Warned,
    Demoted,
}

/// Demotion as the game's: at -100 at once; below -50 a warning, then demoted if still at -50
/// or below when it's run out (48 hours).
pub fn review(job: &mut Job, now: f64) -> Review {
    if job.performance <= -100.0 {
        job.warned_at = None;
        return Review::Demoted;
    }
    match job.warned_at {
        Some(t) if now - t >= DEMOTION_WARNING_MINUTES => {
            job.warned_at = None;
            if job.performance <= -50.0 { Review::Demoted } else { Review::Fine }
        }
        Some(_) => Review::Fine,
        None if job.performance < -50.0 => {
            job.warned_at = Some(now);
            Review::Warned
        }
        None => Review::Fine,
    }
}

/// The review, and a demotion (fun -50) or firing at the bottom (fun -100) if it comes to that.
#[allow(clippy::too_many_arguments)]
fn demotion(commands: &mut Commands, notes: &mut Notifications, life: &mut MessageWriter<LifeEvent>, e: Entity, sim: &Sim, job: &mut Job, motives: &mut Motives, now: f64, member: bool) {
    match review(job, now) {
        Review::Fine => {}
        Review::Warned => {
            if member {
                notes.push(format!("{}'s boss has warned them: their work had better improve, or they'll be demoted.", sim.first));
            }
        }
        Review::Demoted if job.level == 0 => {
            notes.push(format!("{} was fired from the {} career.", sim.first, job.career().name));
            motives.add(FUN, -100.0);
            commands.entity(e).remove::<Job>();
            life.write(LifeEvent::new(e, LifeEventKind::Fired));
        }
        Review::Demoted => {
            job.level -= 1;
            job.performance = 0.0;
            motives.add(FUN, -50.0);
            notes.push(format!("{} was demoted to {}.", sim.first, job.info().title_for(sim.female)));
            life.write(LifeEvent::new(e, LifeEventKind::Demoted));
        }
    }
}

/// Called when a Sim reaches the carpool: off to work until the shift ends.
pub fn leave_for_work(commands: &mut Commands, clock: &GameClock, e: Entity, sim: &Sim, job: &mut Job, notes: &mut Notifications) {
    let info = job.info();
    let day = clock.day();
    job.last_day = Some(day);
    let h = clock.hour_f();
    // The shift ends `hours` after it starts; leaving late still ends on time.
    let start_minute = (clock.minutes / 1440.0).floor() * 1440.0 + info.start as f64 * 60.0;
    let start_minute = if (h as f64) < info.start as f64 - 12.0 { start_minute - 1440.0 } else { start_minute };
    let until = start_minute + job.hours() as f64 * 60.0;
    // (Paid from when they get there.)
    commands.entity(e).insert((AtWork { until: until.max(clock.minutes + 30.0), from: clock.minutes, start_perf: job.performance }, Visibility::Hidden));
    notes.push(format!("{} left for work as a {}.", sim.first, info.title));
}

#[cfg(test)]
mod audit {
    /// The careers as baked (`SIMS3_CACHE=<baked> cargo test -p sims3 careers_audit -- --ignored
    /// --nocapture`).
    #[test]
    #[ignore]
    fn careers_audit() {
        let data = s3bake::gamedata::load_gamedata(&s3bake::default_root()).expect("game data");
        for c in &data.careers {
            println!("{} part {} flow {:?} miss {} bonus {} fun {} coworkers {:?} levels {}", c.hex, c.part_time, c.flow, c.miss_work, c.bonus_hours, c.fun_per_hour, c.coworkers, c.levels.len());
            for l in c.levels.iter().take(3) {
                println!("   L{} {} ${} boss {} {:?}", l.level, l.branch, l.hourly, l.has_boss, l.metrics.iter().map(|m| format!("{} {:?} {}", m.kind, m.mappings, m.arg)).collect::<Vec<_>>());
            }
            for e in c.events.iter().filter(|e| e.kind != "Opportunity") {
                println!("   EVENT {} {} p {} levels {:?} {} sure {} | {} | yes {:?} no {:?} | {:?} {:?} {:?} chance {} {:?}", e.kind, e.key, e.likelihood, e.levels, e.branch, e.sure, e.text[1], e.yes[0], e.no[0], e.rewards_yes, e.rewards_yes_lose, e.rewards_no, e.chance, e.modifiers);
            }
            let opps: Vec<&str> = c.events.iter().filter(|e| e.kind == "Opportunity").map(|e| e.opportunity.as_str()).collect();
            let known: Vec<&&str> = opps.iter().filter(|g| data.opportunities.iter().any(|o| o.guid == **g)).collect();
            println!("   {} opportunity events, {} of opportunities we have: {:?}", opps.len(), known.len(), known.iter().take(4).collect::<Vec<_>>());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_tones_keep_their_names() {
        for t in WorkTone::ALL {
            assert_eq!(WorkTone::from_name(t.name()), t);
        }
        // (Saves from before work tones, or with one this doesn't know: normally.)
        assert_eq!(WorkTone::from_name(""), WorkTone::Normal);
        assert!(WorkTone::WorkHard.performance(false) > WorkTone::Normal.performance(false));
        assert!(WorkTone::TakeItEasy.performance(false) < WorkTone::Normal.performance(false));
        // (A Professional Slacker takes it easy at no cost.)
        assert_eq!(WorkTone::TakeItEasy.performance(true), 0.0);
    }

    #[test]
    fn performance_flows_by_the_levels_metrics() {
        // The game's Business level 1: in a good mood (+3) with a new boss (0), 7 an hour; working
        // hard, 10, so promoted after the second shift.
        let flow = [14.0, -8.0, 8.0, -10.0];
        let avg = (metric_score(40.0, [-500.0, 0.0, 30.0]) + metric_score(0.0, [-50.0, 0.0, 50.0])) / 2.0;
        assert_eq!(avg, 1.5);
        assert_eq!(performance_rate(avg, flow, false) + WorkTone::WorkHard.performance(false), 10.0);
        // (Halfway to a mapping's end is the lesser score; exactly halfway below, the lesser still.)
        assert_eq!(metric_score(15.0, [-500.0, 0.0, 30.0]), 1.0);
        assert_eq!(metric_score(16.0, [-500.0, 0.0, 30.0]), 2.0);
        assert_eq!(metric_score(-250.0, [-500.0, 0.0, 30.0]), -1.0);
        assert_eq!(metric_score(-251.0, [-500.0, 0.0, 30.0]), -2.0);
        assert_eq!(metric_score(-600.0, [-500.0, 0.0, 30.0]), -3.0);
        // (Level 2 wants the boss warmer: a new one counts against them.)
        assert_eq!(metric_score(0.0, [-50.0, 12.5, 75.0]), -1.0);
        assert_eq!(performance_rate(-3.0, flow, false), -8.0);
        assert_eq!(performance_rate(3.0, flow, true), 8.0);
        assert_eq!(charisma_boost(0, true), 1.0);
        assert_eq!(charisma_boost(10, false), 1.77);
    }

    #[test]
    fn poor_work_is_warned_then_demoted() {
        let mut job = Job::new(0);
        job.performance = -60.0;
        assert_eq!(review(&mut job, 0.0), Review::Warned);
        assert_eq!(review(&mut job, 47.0 * 60.0), Review::Fine);
        assert_eq!(review(&mut job, 48.0 * 60.0), Review::Demoted);
        // (Better by the time it runs out, and it's forgotten.)
        job.performance = -60.0;
        assert_eq!(review(&mut job, 0.0), Review::Warned);
        job.performance = -40.0;
        assert_eq!(review(&mut job, 48.0 * 60.0), Review::Fine);
        assert_eq!(job.warned_at, None);
        // (At the very bottom, at once.)
        job.performance = -100.0;
        assert_eq!(review(&mut job, 0.0), Review::Demoted);
    }

    fn shift(job: Job, hours: f64) -> (App, Entity) {
        use crate::sim::{Age, random_sim};
        let mut app = App::new();
        let clock = GameClock::default();
        let now = clock.minutes;
        app.insert_resource(clock)
            .init_resource::<Notifications>()
            .init_resource::<crate::dialog::Questions>()
            .add_message::<LifeEvent>()
            .insert_resource(Household { name: "Test".into(), funds: 0, lot_index: 0, last_bill_day: 0, bills: Vec::new() })
            .add_systems(Update, work_schedule);
        let mut sim = random_sim(&mut rand::rng(), "Test", Some(false), Age::Adult);
        sim.traits.clear();
        let e = app
            .world_mut()
            .spawn((
                sim,
                HouseholdMember,
                job,
                ActionQueue::default(),
                AtWork { until: now, from: now - hours * 60.0, start_perf: 0.0 },
                Transform::default(),
                Motives([0.0; 6]),
                Skills::default(),
                crate::social::Relationships::default(),
            ))
            .id();
        (app, e)
    }

    #[test]
    fn a_shift_pays_its_hours_and_a_first_promotion_its_bonus() {
        let mut job = Job::new(0);
        job.performance = 100.0;
        let hourly = job.info().hourly;
        let (mut app, e) = shift(job, 4.0);
        app.update();
        let job = app.world().get::<Job>(e).unwrap();
        assert_eq!(job.level, 1);
        assert_eq!(job.highest, 1);
        assert_eq!(job.performance, 0.0);
        // Four hours' pay, and the bonus at the old pay.
        assert_eq!(app.world().resource::<Household>().funds, hourly * 4 + (12.0 * hourly as f32) as i64);
        assert_eq!(app.world().get::<Motives>(e).unwrap().0[BLADDER], 100.0);
        // (Back to a level they've been at before: no bonus.)
        let mut job = Job::new(0);
        job.highest = 3;
        job.performance = 100.0;
        let (mut app, e) = shift(job, 1.0);
        app.update();
        assert_eq!(app.world().get::<Job>(e).unwrap().level, 1);
        assert_eq!(app.world().resource::<Household>().funds, hourly);
    }

    #[test]
    fn a_promotion_the_boss_agreed_to_comes_at_once() {
        let mut job = Job::new(0);
        job.promote = true;
        job.performance = 30.0;
        let hourly = job.info().hourly;
        let (mut app, e) = shift(job, 0.0);
        app.world_mut().entity_mut(e).remove::<AtWork>();
        app.add_systems(Update, promote_when_agreed);
        app.update();
        let job = app.world().get::<Job>(e).unwrap();
        assert_eq!((job.level, job.performance, job.promote), (1, 0.0, false));
        assert_eq!(app.world().resource::<Household>().funds, (12.0 * hourly as f32) as i64);
        // (A raise: 15% of the level's pay an hour.)
        let mut job = Job::new(0);
        let r = job.give_raise();
        assert_eq!((r, job.raise), (((hourly as f32 * 0.15) as i64).max(1), r));
    }

    #[test]
    fn career_events_come_by_the_games_roll() {
        use s3bake::gamedata::CareerEventInfo;
        let mut job = Job::new(0);
        job.level = 3;
        let ev = |levels: Vec<u32>, branch: &str, kind: &str| CareerEventInfo { kind: kind.into(), likelihood: 0.05, branch: branch.into(), levels, chance: 100.0, ..Default::default() };
        // (At their level, on their path; an opportunity only if there's one to offer.)
        assert!(event_open(&ev(vec![3, 4, 5], "All", "Reward"), &job, |_| false));
        assert!(!event_open(&ev(vec![1, 2], "All", "Reward"), &job, |_| false));
        assert!(event_open(&ev(vec![], "Base", "Choice"), &job, |_| false));
        assert!(!event_open(&ev(vec![], "Thief", "Choice"), &job, |_| false));
        assert!(!event_open(&ev(vec![], "All", "Opportunity"), &job, |_| false));
        assert!(event_open(&ev(vec![], "All", "Opportunity"), &job, |_| true));
        // (Nothing as likely as all of them not coming; the sure one besides.)
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(7);
        let cands = [(Due::Raise, 0.25), (Due::Event(0), 0.05), (Due::Event(1), 0.05)];
        let n = 20000;
        let none = (0..n).filter(|_| roll_events(&cands, &[], &mut rng).is_empty()).count() as f32 / n as f32;
        assert!((none - 0.75 * 0.95 * 0.95).abs() < 0.015, "{none}");
        assert_eq!(roll_events(&[], &[Due::Event(5)], &mut rng), vec![Due::Event(5)]);
        // (A choice's chance: its own, the mood's share, a skill's by its level of ten.)
        let mut choice = ev(vec![], "All", "Choice");
        choice.chance = 50.0;
        choice.modifiers = vec!["Mood,25".into(), "Skill,Athletic,20".into()];
        let mut skills = Skills::default();
        skills.0.insert("Athletic", 5.0);
        assert_eq!(win_chance(&choice, 80.0, &skills, 0.0, None), 50.0 + 20.0 + 10.0);
        assert_eq!(buff_moodlet("Singed"), Some(crate::life::MoodletKind::Singed));
        assert_eq!(buff_moodlet("Stressed"), Some(crate::life::MoodletKind::Stressed));
    }

    #[test]
    fn sucking_up_warms_the_boss_both_ways() {
        let boss_sim = crate::sim::random_sim(&mut rand::rng(), "Boss", Some(true), crate::sim::Age::Adult);
        let mut job = Job::new(0);
        job.tone = WorkTone::SuckUp;
        job.boss = Some(boss_sim.id);
        let (mut app, e) = shift(job, 4.0);
        // (Four hours of it, before the shift's end.)
        app.world_mut().get_mut::<AtWork>(e).unwrap().until += 1.0;
        app.insert_resource(crate::clock::SimDelta(240.0)).add_systems(Update, work_flow.before(work_schedule));
        let boss = app.world_mut().spawn((boss_sim, crate::social::Relationships::default())).id();
        app.update();
        // Three an hour (no Charisma, not a Schmoozer).
        assert_eq!(app.world().get::<crate::social::Relationships>(e).unwrap().friendship(boss), 12.0);
        assert_eq!(app.world().get::<crate::social::Relationships>(boss).unwrap().friendship(e), 12.0);
    }
}
