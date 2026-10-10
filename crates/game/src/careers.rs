//! Careers: the base game's tracks from the game's own career tables (titles, pay, hours and
//! workdays per level, part-time jobs for teens), with a built-in table as a fallback until
//! those are converted. Sims head off with the carpool before their shift, come home paid,
//! and their performance (mood, the career's skill and personality) earns promotions — or
//! demotions. Some careers branch partway up (Criminal into Thief or Evil, Music into Rock or
//! Symphonic, Law Enforcement into Special Agent or Forensic Analyst): the promotion there
//! asks which path to take.

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::*;
use crate::life::{LifeEvent, LifeEventKind, Mood};
use crate::loading::CurrentWorld;
use crate::sim::*;

pub struct CareersPlugin;

impl Plugin for CareersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (work_schedule, career_path_answers, pay_pensions).run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Clone, Copy)]
pub struct CareerLevel {
    pub title: &'static str,
    /// Pay per hour worked.
    pub hourly: i64,
    pub start: f32,
    pub end: f32,
    /// Workdays: bit 0 = Monday.
    pub days: u8,
    /// The uniform (an outfit's name; empty for everyday clothes) for men, women, elderly men
    /// and elderly women.
    pub outfits: [&'static str; 4],
}

pub struct CareerTrack {
    pub name: &'static str,
    /// The skill that improves performance.
    pub skill: &'static str,
    /// The game's career icon.
    pub icon: &'static str,
    /// A teen's after-school job.
    pub part_time: bool,
    /// The ways up the career: each its levels from the first, sharing those before the career
    /// branches (just one for a career that doesn't).
    pub paths: Vec<CareerPath>,
    /// The first level (0-based) that differs between paths.
    pub branch_at: Option<usize>,
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
                Some(crate::dialog::Answer {
                    label: format!("{} — the {} path", l.title, p.label()),
                    detail: format!("§{} an hour, {} to {}", l.hourly, hour_label(l.start), hour_label(l.end)),
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
        notes.push(format!("{} is now a {} on the {} path.", s.first, job.info().title, job.path().label()));
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
                            hourly: l.hourly.round() as i64,
                            start,
                            end: (start + l.hours) % 24.0,
                            days: l.days,
                            outfits: [0, 1, 2, 3].map(|i| leak(l.outfits[i].to_ascii_lowercase())),
                        });
                    }
                    CareerPath { branch: leak(b.to_string()), levels }
                })
                .collect();
            let branch_at = c.levels.iter().filter(|l| l.branch != "Base").map(|l| l.level as usize - 1).min().filter(|_| paths.len() > 1);
            let skill = c.levels.iter().find_map(|l| l.skills.first().cloned()).unwrap_or_else(|| default_skill(&c.hex).to_string());
            CareerTrack { name: leak(c.name.clone()), skill: leak(skill), icon: leak(c.icon.clone()), part_time: c.part_time, paths, branch_at }
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
            part_time: false,
            paths: vec![CareerPath {
                branch: "Base",
                levels: self.levels.iter().map(|&(title, hourly, start, end)| CareerLevel { title, hourly, start, end, days: self.days, outfits: [""; 4] }).collect(),
            }],
            branch_at: None,
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
}

impl WorkTone {
    pub const ALL: [WorkTone; 5] = [WorkTone::Normal, WorkTone::WorkHard, WorkTone::TakeItEasy, WorkTone::Socialize, WorkTone::Study];

    pub fn label(self, skill: &str) -> String {
        match self {
            WorkTone::Normal => "Work Normally".into(),
            WorkTone::WorkHard => "Work Hard".into(),
            WorkTone::TakeItEasy => "Take It Easy".into(),
            WorkTone::Socialize => "Hang With Co-Workers".into(),
            WorkTone::Study => format!("Study {skill}"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            WorkTone::Normal => "Normal",
            WorkTone::WorkHard => "WorkHard",
            WorkTone::TakeItEasy => "TakeItEasy",
            WorkTone::Socialize => "Socialize",
            WorkTone::Study => "Study",
        }
    }

    pub fn from_name(s: &str) -> Self {
        Self::ALL.into_iter().find(|t| t.name() == s).unwrap_or_default()
    }

    /// How much of a shift's performance it earns.
    fn performance(self) -> f32 {
        match self {
            WorkTone::Normal => 1.0,
            WorkTone::WorkHard => 1.5,
            WorkTone::TakeItEasy => 0.5,
            WorkTone::Socialize | WorkTone::Study => 0.8,
        }
    }

    /// What a shift does to energy, fun and social, on top of a normal one's.
    fn motives(self) -> [f32; 3] {
        match self {
            WorkTone::Normal | WorkTone::Study => [0.0; 3],
            WorkTone::WorkHard => [-15.0, -15.0, 0.0],
            WorkTone::TakeItEasy => [15.0, 20.0, 0.0],
            WorkTone::Socialize => [0.0, 10.0, 30.0],
        }
    }
}

impl Job {
    pub fn new(track: usize) -> Self {
        Self { track, level: 0, branch: 0, performance: 0.0, last_day: None, tone: WorkTone::Normal }
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
}

/// Head to work before the shift; come home with pay and a performance review.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn work_schedule(
    mut commands: Commands,
    clock: Res<GameClock>,
    exit: Option<Res<LotExit>>,
    world: Res<CurrentWorld>,
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
        ),
        Without<crate::rabbitholes::AtRabbitHole>,
    >,
    mut session_start: Local<Option<f64>>,
    mut questions: ResMut<crate::dialog::Questions>,
) {
    let h = clock.hour_f();
    let day = clock.day();
    // A shift already under way when play began isn't held against anyone.
    let started = *session_start.get_or_insert(clock.minutes);
    for (e, sim, mut job, mut queue, at_work, mut tf, mut motives, mood, mut skills, ltw, member, wishes) in &mut workers {
        let info = job.info();
        if let Some(w) = at_work {
            if clock.minutes < w.until {
                continue;
            }
            // Home from work: pay, needs, and a performance review.
            let pay = (info.hourly as f32 * job.hours()) as i64;
            if let Some(hh) = household.as_mut() {
                hh.funds += pay;
            }
            let skill = skills.level(job.career().skill) as f32;
            let want = (job.level as f32 + 1.0) * 0.8;
            // (A Professional Slacker's bad moods don't count against them.)
            let mut moody = mood.level() / 100.0 * 22.0;
            if crate::wishes::has(wishes, "ProfessionalSlacker") {
                moody = moody.max(0.0);
            }
            let gain = (moody + (skill - want) * 3.0 + 8.0) * crate::life::work_rate(&sim.traits);
            // (Working hard gets them noticed faster; taking it easy, the other way.)
            let gain = if gain > 0.0 { gain * job.tone.performance() } else { gain / job.tone.performance().max(0.5) };
            // (A Multi-Tasker gets more done.)
            let gain = gain + if crate::wishes::has(wishes, "MultiTasker") { 3.0 } else { 0.0 };
            job.performance = (job.performance + gain).clamp(-100.0, 100.0);
            // Studying the career's skill on the job.
            if job.tone == WorkTone::Study {
                let sk = job.career().skill;
                let v = skills.0.entry(sk).or_insert(0.0);
                let before = *v as u32;
                *v = (*v + job.hours() * 0.12 * crate::life::skill_rate(&sim.traits, sk) * crate::wishes::reward_skill_rate(wishes) * mood.skill_rate() / (1.0 + *v * 0.25)).min(10.0);
                if *v as u32 > before && member {
                    notes.push(format!("{} reached level {} in {}!", sim.first, *v as u32, sk));
                    life.write(LifeEvent::new(e, LifeEventKind::SkillUp { skill: sk, level: *v as u32 }));
                }
            }
            notes.push(format!("{} is home from work and earned §{pay}.", sim.first));
            if job.performance >= 100.0 && job.level + 1 < job.levels().len() {
                job.level += 1;
                job.performance = 0.0;
                let track = job.career();
                let branching = track.branch_at == Some(job.level);
                if branching {
                    // The path their lifetime wish lies along, or the first.
                    job.branch = ltw.and_then(|w| w.def().career_branch(track.name)).and_then(|b| track.path_index(b)).unwrap_or(0);
                }
                let bonus = job.info().hourly * 8;
                if let Some(hh) = household.as_mut() {
                    hh.funds += bonus;
                }
                if branching && member {
                    notes.push(format!("{} was promoted and got a §{bonus} bonus! The {} career branches here.", sim.first, track.name));
                    questions.ask_career_path(e, sim, &job);
                } else {
                    notes.push(format!("{} was promoted to {} and got a §{bonus} bonus!", sim.first, job.info().title));
                }
                life.write(LifeEvent::new(e, LifeEventKind::Promoted));
            } else if job.performance <= -100.0 {
                if job.level == 0 {
                    notes.push(format!("{} was fired from the {} career.", sim.first, job.career().name));
                    commands.entity(e).remove::<Job>();
                    life.write(LifeEvent::new(e, LifeEventKind::Fired));
                } else {
                    job.level -= 1;
                    job.performance = 0.0;
                    notes.push(format!("{} was demoted to {}.", sim.first, job.info().title));
                    life.write(LifeEvent::new(e, LifeEventKind::Demoted));
                }
            }
            motives.0[HUNGER] = (motives.0[HUNGER] - 35.0).max(-90.0);
            motives.0[ENERGY] = (motives.0[ENERGY] - 30.0).max(-80.0);
            motives.0[FUN] = (motives.0[FUN] - 25.0).max(-80.0);
            motives.0[SOCIAL] = (motives.0[SOCIAL] + 40.0).min(100.0);
            motives.0[HYGIENE] = (motives.0[HYGIENE] - 20.0).max(-80.0);
            let [energy, fun, social] = job.tone.motives();
            motives.add(ENERGY, energy);
            motives.add(FUN, fun);
            motives.add(SOCIAL, social);
            if let Some(x) = &exit {
                tf.translation = Vec3::new(x.0.x, world.data.heightmap.sample(x.0.x, x.0.y), x.0.y);
            }
            // (Back on the ground, whichever floor they left from.)
            commands.entity(e).remove::<AtWork>().insert((Visibility::Inherited, crate::nav::Floor(1)));
            continue;
        }
        let workday = job.works_on(clock.weekday());
        let shift_began = day as f64 * 1440.0 + (info.start as f64 - 0.75) * 60.0;
        if workday && started > shift_began && clock.minutes - started < 24.0 * 60.0 && job.last_day != Some(day) && h > info.start + 2.0 {
            job.last_day = Some(day);
            continue;
        }
        // A missed shift costs performance.
        if workday && h > info.start + 2.0 && h < info.start + 3.0 && job.last_day != Some(day) {
            job.last_day = Some(day);
            // TraitTuning reduces a Vacationer's missed-shift penalty to 20%.
            let vacationer = crate::wishes::has(wishes, "Vacationer");
            let penalty = 35.0 * if vacationer { 0.2 } else { 1.0 };
            job.performance = (job.performance - penalty).max(-100.0);
            notes.push(if vacationer {
                format!("{} took the day off work. Vacationer softened the performance penalty.", sim.first)
            } else {
                format!("{} missed work today! Their boss is not pleased.", sim.first)
            });
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
    commands.entity(e).insert((AtWork { until: until.max(clock.minutes + 30.0) }, Visibility::Hidden));
    notes.push(format!("{} left for work as a {}.", sim.first, info.title));
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
        assert!(WorkTone::WorkHard.performance() > WorkTone::Normal.performance());
        assert!(WorkTone::TakeItEasy.performance() < WorkTone::Normal.performance());
    }
}
