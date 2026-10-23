//! Careers at their rabbit holes, as the game's: a career joined where its Sims work ("Join
//! Medical Career" at the hospital, "Get Part-Time Job" at the grocery store: the game's
//! `GetJobInRabbitHole`), and a job quit or retired from there (`QuitJobInRabbitHole`,
//! `RetireFromJobInRabbitHole`): each an errand inside, the job taken, left or retired from once
//! it's done.

use bevy::prelude::*;

use crate::PlayMode;
use crate::careers::{Job, careers};
use crate::interact::Notifications;
use crate::life::{LifeEvent, LifeEventKind};
use crate::rabbitholes::Activity;
use crate::sim::{Age, Sim};

pub struct CareerHolesPlugin;

impl Plugin for CareerHolesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, errands_done.run_if(in_state(PlayMode::Live)));
    }
}

/// A visit's task: the career joined (`JOB_TASK` and its track), the job quit, or retired from.
pub const JOB_TASK: usize = 6000;
pub const QUIT_TASK: usize = 6900;
pub const RETIRE_TASK: usize = 6901;

const fn errand(name: &'static str) -> Activity {
    Activity { name, minutes: 15.0, cost: 0, per_hour: [0.0; 6], skill: None, open: 0.0, close: 24.0 }
}
pub static JOIN: Activity = errand("Join a Career");
pub static QUIT: Activity = errand("Quit Their Job");
pub static RETIRE: Activity = errand("Retire");

/// What a Sim's out to do at the rabbit hole.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum Errand {
    Join(usize),
    Quit,
    Retire,
}

/// Back from it.
#[derive(Component)]
pub struct ErrandDone;

impl Errand {
    pub fn of(task: usize) -> Option<Self> {
        match task {
            QUIT_TASK => Some(Self::Quit),
            RETIRE_TASK => Some(Self::Retire),
            t => t.checked_sub(JOB_TASK).filter(|i| *i < careers().len()).map(Self::Join),
        }
    }

    pub fn activity(self) -> &'static Activity {
        match self {
            Self::Join(_) => &JOIN,
            Self::Quit => &QUIT,
            Self::Retire => &RETIRE,
        }
    }
}

pub fn is_errand(a: &Activity) -> bool {
    [&JOIN, &QUIT, &RETIRE].iter().any(|e| std::ptr::eq(a, *e))
}

/// The rabbit hole a career's Sims work at (its lot type: `opportunities::lot_types`), as the
/// game's rabbit holes take on their `CareerLocation`s (the career's key): the packs' careers
/// too, the school's Education, the film studio's Film, the Gypsy wagon's Fortune Teller, the
/// business office's Game Developer, the stadium's Sports Agent, the Hideout's Art Appraiser,
/// the observatory's Astronomer, the Bot Arena's, and the science lab's alien test subjects.
pub fn workplace(key: &str) -> Option<&'static str> {
    Some(match key {
        "Medical" => "Hospital",
        "Criminal" | "ArtAppraiserCareer" => "Hideout",
        "Political" => "CityHall",
        "Culinary" => "Restaurant",
        "LawEnforcement" | "Law Enforcement" => "PoliceStation",
        "Music" => "Theatre",
        "Military" => "MilitaryBase",
        "Science" | "PTAlienTestSubject" => "ScienceLab",
        "Business" | "Journalism" | "VideoGameDeveloper" => "BusinessAndJournalism",
        "ProfessionalSports" | "Professional Sports" | "Athletic" | "SportsAgentCareer" => "Stadium",
        "PTBookstoreClerk" => "Bookstore",
        "PTGroceryStoreClerk" => "Grocery",
        "Education" => "School",
        "Film" | "PTFilm" => "MovieSet",
        "FortuneTellerCareer" => "GypsyCaravan",
        "Astronomer" => "StellarObservatory",
        "BotArena" => "ServoBotArena",
        _ => return None,
    })
}

/// Whether a career's Sims work at a lot: its rabbit hole by the game's `CareerLocation`s (the
/// career's key, the game's `OccupationNames`).
pub fn works_at(world: &crate::loading::WorldInfo, lot: usize, key: &str) -> bool {
    let name = world.lots.get(lot).map(|l| l.internal_name.to_ascii_lowercase()).unwrap_or_default();
    let named = |ks: &[&str]| ks.iter().any(|k| name.contains(k));
    let types = || crate::opportunities::lot_types(world, lot);
    match key {
        "PTMausoleum" => types().contains(&"Mausoleum") || named(&["mausoleum", "graveyard", "cemetery"]),
        "PTDaySpaReceptionist" | "PTDaySpaSpecialist" => types().contains(&"DaySpa") || named(&["spa", "salon"]),
        k => workplace(k).is_some_and(|t| crate::opportunities::lot_types(world, lot).contains(&t)),
    }
}

/// The careers worked somewhere in a world (the game lists only those with a `CareerLocation`:
/// `ShowRandomJobs`'s `Locations.Count > 0`), by track; every one, for a world whose rabbit
/// holes aren't known.
pub fn located(world: &crate::loading::WorldInfo) -> Vec<bool> {
    let all = careers();
    let mut out = vec![false; all.len()];
    for lot in 0..world.lots.len() {
        for (i, c) in all.iter().enumerate() {
            if !out[i] && works_at(world, lot, c.key) {
                out[i] = true;
            }
        }
    }
    if !out.iter().any(|x| *x) {
        out.fill(true);
    }
    out
}

/// How the game names joining a career there.
fn join_label(key: &str, name: &str, part_time: bool) -> String {
    match key {
        "PTDaySpaReceptionist" => "Get Part-Time Job as a Receptionist".to_string(),
        "PTDaySpaSpecialist" => "Get Part-Time Job as a Spa Specialist".to_string(),
        _ if part_time => "Get Part-Time Job".to_string(),
        _ => format!("Join {name} Career"),
    }
}

/// A rabbit hole's career errands for a Sim: joining those worked there that suit their age
/// (after-school jobs for teens, the rest for grown-ups) and that they haven't got, and quitting
/// (or, an elder, retiring from) their own job if it's there.
pub fn options(world: &crate::loading::WorldInfo, lot: usize, sim: &Sim, job: Option<&Job>) -> Vec<(String, usize)> {
    let teen = sim.age == Age::Teen;
    let mut out = Vec::new();
    for (i, c) in careers().iter().enumerate() {
        if !works_at(world, lot, c.key) {
            continue;
        }
        if job.is_some_and(|j| j.track == i) {
            out.push(("Quit Job".to_string(), QUIT_TASK));
            if sim.age == Age::Elder {
                out.push(("Retire".to_string(), RETIRE_TASK));
            }
        } else if (teen || sim.age.is_grown()) && c.part_time == teen {
            out.push((join_label(c.key, c.name, c.part_time), JOB_TASK + i));
        }
    }
    out
}

/// Back from the errand: the career joined (its first level), the job quit, or retired from on
/// a pension, as the game's are done inside.
fn errands_done(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    sims: Query<(Entity, &Sim, &Errand, Option<&Job>), With<ErrandDone>>,
    mut notes: ResMut<Notifications>,
    mut life: MessageWriter<LifeEvent>,
) {
    for (e, sim, errand, job) in &sims {
        commands.entity(e).remove::<(Errand, ErrandDone)>();
        match *errand {
            Errand::Join(track) => {
                let (j, note) = crate::careers::new_job(sim, track);
                notes.push(note);
                commands.entity(e).insert(j);
                life.write(LifeEvent::new(e, LifeEventKind::NewJob));
            }
            Errand::Quit if job.is_some() => {
                commands.entity(e).remove::<Job>();
                notes.push(format!("{} quit their job.", sim.first));
            }
            Errand::Retire => {
                let Some(j) = job else { continue };
                let daily = j.pension();
                notes.push(format!(
                    "{} retired from the {} career after years as {}, and will be paid a pension of §{daily} a day.",
                    sim.first,
                    j.career().name,
                    crate::careers::a_title(j.info(), sim)
                ));
                commands.entity(e).remove::<Job>().insert(crate::careers::Pension { daily, paid: clock.day() });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_visits_task_is_the_errand_it_stands_for() {
        assert_eq!(Errand::of(JOB_TASK), Some(Errand::Join(0)));
        assert_eq!(Errand::of(QUIT_TASK), Some(Errand::Quit));
        assert_eq!(Errand::of(RETIRE_TASK), Some(Errand::Retire));
        assert_eq!(Errand::of(JOB_TASK + 800), None);
        assert!(is_errand(Errand::Quit.activity()) && !is_errand(&crate::rabbitholes::SCHOOL));
    }

    #[test]
    fn joining_is_named_as_the_games() {
        assert_eq!(join_label("LawEnforcement", "Law Enforcement", false), "Join Law Enforcement Career");
        assert_eq!(join_label("PTGroceryStoreClerk", "Grocery Store", true), "Get Part-Time Job");
        assert_eq!(join_label("PTDaySpaReceptionist", "Day Spa", true), "Get Part-Time Job as a Receptionist");
    }

    #[test]
    fn the_packs_careers_are_worked_at_their_rabbit_holes() {
        // (As the game's rabbit holes take on their CareerLocations.)
        assert_eq!(workplace("Education"), Some("School"));
        assert_eq!(workplace("PTFilm"), Some("MovieSet"));
        assert_eq!(workplace("FortuneTellerCareer"), Some("GypsyCaravan"));
        assert_eq!(workplace("VideoGameDeveloper"), workplace("Business"));
        assert_eq!(workplace("SportsAgentCareer"), workplace("ProfessionalSports"));
        assert_eq!(workplace("ArtAppraiserCareer"), workplace("Criminal"));
        assert_eq!(workplace("Astronomer"), Some("StellarObservatory"));
        assert_eq!(workplace("BotArena"), Some("ServoBotArena"));
        assert_eq!(workplace("PTAlienTestSubject"), workplace("Science"));
    }
}
