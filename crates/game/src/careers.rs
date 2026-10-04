//! Careers: the base game's tracks, each ten levels with their own titles, pay and hours.
//! Sims head off with the carpool before their shift, come home paid, and their performance
//! (mood, the career's skill and personality) earns promotions — or demotions.

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
        app.add_systems(Update, work_schedule.run_if(in_state(PlayMode::Live)));
    }
}

pub struct CareerLevel {
    pub title: &'static str,
    /// Pay per hour worked.
    pub hourly: i64,
    pub start: f32,
    pub end: f32,
}

pub struct CareerTrack {
    pub name: &'static str,
    /// The skill that improves performance.
    pub skill: &'static str,
    /// Workdays: bit 0 = Monday.
    pub days: u8,
    pub levels: [CareerLevel; 10],
}

const WEEKDAYS: u8 = 0b0011111;
const fn lv(title: &'static str, hourly: i64, start: f32, end: f32) -> CareerLevel {
    CareerLevel { title, hourly, start, end }
}

pub static CAREERS: [CareerTrack; 10] = [
    CareerTrack {
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
    CareerTrack {
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
    CareerTrack {
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
    CareerTrack {
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
    CareerTrack {
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
    CareerTrack {
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
    CareerTrack {
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
    CareerTrack {
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
    CareerTrack {
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
    CareerTrack {
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
    /// -100 (about to be demoted) .. 100 (promotion).
    pub performance: f32,
    /// The last day the Sim was at work (for missed-day penalties).
    pub last_day: Option<u32>,
}

impl Job {
    pub fn new(track: usize) -> Self {
        Self { track, level: 0, performance: 0.0, last_day: None }
    }
    pub fn career(&self) -> &'static CareerTrack {
        &CAREERS[self.track]
    }
    pub fn info(&self) -> &'static CareerLevel {
        &CAREERS[self.track].levels[self.level]
    }
    pub fn hours(&self) -> f32 {
        let l = self.info();
        if l.end > l.start { l.end - l.start } else { l.end + 24.0 - l.start }
    }
    pub fn works_on(&self, weekday: usize) -> bool {
        self.career().days & (1 << weekday) != 0
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
    mut workers: Query<(Entity, &Sim, &mut Job, &mut ActionQueue, Option<&AtWork>, &mut Transform, &mut Motives, &Mood, &Skills)>,
) {
    let h = clock.hour_f();
    let day = clock.day();
    for (e, sim, mut job, mut queue, at_work, mut tf, mut motives, mood, skills) in &mut workers {
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
            let gain = (mood.level() / 100.0 * 22.0 + (skill - want) * 3.0 + 8.0) * crate::life::work_rate(&sim.traits);
            job.performance = (job.performance + gain).clamp(-100.0, 100.0);
            notes.push(format!("{} is home from work and earned §{pay}.", sim.first));
            if job.performance >= 100.0 && job.level < 9 {
                job.level += 1;
                job.performance = 0.0;
                let bonus = job.info().hourly * 8;
                if let Some(hh) = household.as_mut() {
                    hh.funds += bonus;
                }
                notes.push(format!("{} was promoted to {} and got a §{bonus} bonus!", sim.first, job.info().title));
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
            if let Some(x) = &exit {
                tf.translation = Vec3::new(x.0.x, world.data.heightmap.sample(x.0.x, x.0.y), x.0.y);
            }
            commands.entity(e).remove::<AtWork>().insert(Visibility::Inherited);
            continue;
        }
        let workday = job.works_on(clock.weekday());
        // A missed shift costs performance.
        if workday && h > info.start + 2.0 && h < info.start + 3.0 && job.last_day != Some(day) {
            job.last_day = Some(day);
            job.performance = (job.performance - 35.0).max(-100.0);
            notes.push(format!("{} missed work today! Their boss is not pleased.", sim.first));
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
