//! The town's rabbit holes: community lots Sims drive to and spend time inside — work out at
//! the gym, study at the library, eat at the diner, see a show, relax at the spa, hang out at
//! the park — and the school children attend on weekdays.

use bevy::prelude::*;
use rand::Rng;
use s3formats::world::LotInfo;

use crate::PlayMode;
use crate::clock::{GameClock, SimDelta};
use crate::interact::*;
use crate::life::{LifeEvent, LifeEventKind};
use crate::loading::CurrentWorld;
use crate::sim::*;

pub struct RabbitHolePlugin;

impl Plugin for RabbitHolePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (school_bus, outings).chain().run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Debug)]
pub struct Activity {
    pub name: &'static str,
    pub minutes: f32,
    pub cost: i64,
    /// Need changes per hour inside.
    pub per_hour: [f32; 6],
    pub skill: Option<&'static str>,
    /// The hours it can be started in.
    pub open: f32,
    pub close: f32,
}

pub const fn act(name: &'static str, minutes: f32, cost: i64, per_hour: [f32; 6], skill: Option<&'static str>) -> Activity {
    Activity { name, minutes, cost, per_hour, skill, open: 0.0, close: 24.0 }
}

//                      hunger bladder energy social hygiene fun
static GYM: [Activity; 2] = [
    act("Work Out", 120.0, 0, [-8.0, -5.0, -14.0, 6.0, -20.0, 18.0], Some("Athletic")),
    act("Take a Yoga Class", 90.0, 25, [-5.0, -4.0, -4.0, 12.0, -8.0, 20.0], Some("Athletic")),
];
static LIBRARY: [Activity; 2] = [
    act("Read", 120.0, 0, [-5.0, -5.0, -4.0, 0.0, 0.0, 22.0], Some("Logic")),
    act("Write in the Quiet Room", 120.0, 0, [-5.0, -5.0, -5.0, 0.0, 0.0, 12.0], Some("Writing")),
];
static EATERY: [Activity; 2] = [
    act("Eat a Meal", 60.0, 35, [150.0, -8.0, 0.0, 30.0, 0.0, 30.0], None),
    act("Have a Drink with Friends", 90.0, 25, [10.0, -15.0, -5.0, 70.0, 0.0, 45.0], None),
];
static SHOW: [Activity; 1] = [act("See a Show", 120.0, 50, [-6.0, -8.0, -3.0, 25.0, 0.0, 60.0], None)];
static SPA: [Activity; 2] = [
    act("Get a Massage", 90.0, 100, [-4.0, -4.0, 25.0, 10.0, 40.0, 40.0], None),
    act("Get a Makeover", 60.0, 60, [-4.0, -4.0, 0.0, 20.0, 60.0, 30.0], None),
];
static PARK: [Activity; 2] = [
    act("Hang Out", 90.0, 0, [-6.0, -8.0, -4.0, 50.0, -5.0, 35.0], None),
    act("Go Fishing", 120.0, 0, [-5.0, -6.0, -3.0, 5.0, -5.0, 35.0], Some("Fishing")),
];
static POOL: [Activity; 1] = [act("Go Swimming", 90.0, 0, [-10.0, -6.0, -12.0, 20.0, 20.0, 45.0], Some("Athletic"))];
static MUSEUM: [Activity; 1] = [act("View Art", 90.0, 15, [-5.0, -5.0, -3.0, 15.0, 0.0, 35.0], Some("Painting"))];
static SCIENCE: [Activity; 1] = [act("Volunteer as a Test Subject", 120.0, -80, [-8.0, -8.0, -8.0, 5.0, -10.0, -10.0], None)];
static HOSPITAL: [Activity; 1] = [act("Get a Checkup", 60.0, 40, [-4.0, -4.0, 5.0, 5.0, 5.0, -5.0], None)];
static SHOPS: [Activity; 1] = [act("Browse the Shelves", 60.0, 0, [-4.0, -4.0, -2.0, 15.0, 0.0, 25.0], Some("Writing"))];
// (With lunch at school.)
pub static SCHOOL: Activity = act("School", 0.0, 0, [-6.0, -3.0, -6.0, 25.0, -6.0, -8.0], Some("Logic"));

/// The rabbit-hole activities a community lot offers, from its name.
pub fn activities(lot: &LotInfo) -> &'static [Activity] {
    let n = lot.internal_name.to_ascii_lowercase();
    let has = |k: &str| n.contains(k);
    if has("gym") {
        &GYM
    } else if has("library") {
        &LIBRARY
    } else if has("bistro") || has("diner") || has("dinner") || has("restaurant") || has("divebar") || has("bar_") {
        &EATERY
    } else if has("theater") || has("theatre") || has("amphitheater") || has("stadium") {
        &SHOW
    } else if has("spa") || has("salon") {
        &SPA
    } else if has("park") || has("beach") || has("pond") || has("waterfall") || has("springs") || has("esplanade") || has("townsquare") || has("garden") || has("fishing") || has("strand") {
        &PARK
    } else if has("pool") {
        &POOL
    } else if has("museum") {
        &MUSEUM
    } else if has("science") {
        &SCIENCE
    } else if has("hospital") {
        &HOSPITAL
    } else if has("book") || has("grocery") || has("consignment") {
        &SHOPS
    } else {
        &[]
    }
}

/// A readable name for a community lot.
pub fn lot_title(lot: &LotInfo, display: &str) -> String {
    if !display.is_empty() && display != lot.internal_name {
        return display.split(" — ").next().unwrap_or(display).to_string();
    }
    let raw = lot.internal_name.trim_start_matches("Com_").trim_start_matches("com_");
    let raw = raw.split('_').next().unwrap_or(raw);
    let mut out = String::new();
    for (i, ch) in raw.chars().enumerate() {
        if i > 0 && ch.is_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Which lot a point is on.
pub fn lot_at(lots: &[LotInfo], p: Vec3) -> Option<usize> {
    lots.iter().position(|l| {
        let local = Quat::from_rotation_y(l.rotation).inverse() * (p - Vec3::from(l.corner));
        local.x >= 0.0 && local.z >= 0.0 && local.x <= l.width as f32 && local.z <= l.depth as f32
    })
}

/// Away at a rabbit hole (driving there, inside, driving back).
#[derive(Component)]
pub struct AtRabbitHole {
    pub lot: usize,
    pub activity: &'static Activity,
    pub inside_from: f64,
    pub until: f64,
    pub place: String,
}

const DRIVE_MINUTES: f64 = 25.0;

/// Called when a Sim reaches the lot exit on the way to a rabbit hole.
pub fn head_out(
    commands: &mut Commands,
    clock: &GameClock,
    e: Entity,
    sim: &Sim,
    lot: usize,
    activity: &'static Activity,
    place: String,
    household: Option<&mut Household>,
    notes: &mut Notifications,
    price: f32,
) -> bool {
    let h = clock.hour_f();
    let cost = (activity.cost as f32 * price).round() as i64;
    if h < activity.open || h >= activity.close {
        notes.push(format!(
            "{} can only {} between {} and {}.",
            sim.first,
            activity.name.to_lowercase(),
            crate::interact::hour_label(activity.open),
            crate::interact::hour_label(activity.close)
        ));
        return false;
    }
    if cost > 0 {
        let Some(h) = household else { return false };
        if h.funds < cost {
            notes.push(format!("{} can't afford to {} (§{cost}).", sim.first, activity.name.to_lowercase()));
            return false;
        }
        h.funds -= cost;
    } else if cost < 0
        && let Some(h) = household
    {
        h.funds -= cost;
    }
    let inside_from = clock.minutes + DRIVE_MINUTES;
    let until = inside_from + activity.minutes as f64 + DRIVE_MINUTES;
    notes.push(format!("{} headed to {} to {}.", sim.first, place, activity.name.to_lowercase()));
    commands.entity(e).insert((AtRabbitHole { lot, activity, inside_from, until, place }, Visibility::Hidden));
    true
}

/// Time inside a rabbit hole changes needs and skills; Sims come home when it's over.
#[allow(clippy::type_complexity)]
fn outings(
    mut commands: Commands,
    clock: Res<GameClock>,
    delta: Res<SimDelta>,
    exit: Option<Res<LotExit>>,
    world: Res<CurrentWorld>,
    mut notes: ResMut<Notifications>,
    mut life: MessageWriter<LifeEvent>,
    mut q: Query<(Entity, &Sim, &AtRabbitHole, &mut Motives, &mut Skills, &mut Transform, Option<&mut SchoolGrades>, &mut crate::nav::Floor, Option<&crate::journal::SkillJournal>)>,
    mut household: Option<ResMut<Household>>,
    mut did: MessageWriter<crate::journal::Did>,
) {
    let dt = delta.0;
    for (e, sim, at, mut motives, mut skills, mut tf, grades, mut floor, journal) in &mut q {
        if clock.minutes >= at.inside_from && clock.minutes < at.until - DRIVE_MINUTES {
            // (Strength training at the gym, kept in their journal; a Body Builder isn't tired by it.)
            let strength = at.activity.name == "Work Out";
            let builder = strength && crate::journal::earned(journal, "Body Builder");
            for i in 0..6 {
                if builder && i == crate::sim::ENERGY && at.activity.per_hour[i] < 0.0 {
                    continue;
                }
                motives.add(i, at.activity.per_hour[i] * dt / 60.0);
            }
            if strength {
                did.write(crate::journal::Did::count(e, crate::journal::Stat::StrengthHours, dt as f64 / 60.0));
            }
            if let Some(sk) = at.activity.skill {
                let v = skills.0.entry(sk).or_insert(0.0);
                let before = *v as u32;
                *v = (*v + dt / 60.0 * 0.35 * crate::life::skill_rate(&sim.traits, sk) / (1.0 + *v * 0.25)).min(10.0);
                if *v as u32 > before {
                    notes.push(format!("{} reached level {} in {}!", sim.first, *v as u32, sk));
                    life.write(LifeEvent::new(e, LifeEventKind::SkillUp { skill: sk, level: *v as u32 }));
                }
            }
        }
        if clock.minutes >= at.until {
            // Back at the lot's edge, on the ground (whichever floor they left from: the bus
            // takes children from wherever they are).
            if let Some(x) = &exit {
                tf.translation = Vec3::new(x.0.x, world.data.heightmap.sample(x.0.x, x.0.y), x.0.y);
                floor.0 = 1;
            }
            if std::ptr::eq(at.activity, &SCHOOL) {
                if let Some(mut g) = grades {
                    let mood_ok = motives.0.iter().all(|m| *m > -40.0);
                    g.0 = (g.0 + if mood_ok { 8.0 } else { -12.0 }).clamp(0.0, 100.0);
                }
                // With homework for tomorrow (on school nights).
                if clock.weekday() < 4 {
                    commands.entity(e).insert(Homework);
                    notes.push(format!("{} is home from school, with homework.", sim.first));
                } else {
                    notes.push(format!("{} is home from school.", sim.first));
                }
            } else if std::ptr::eq(at.activity, &crate::meals::BUY_RECIPE) {
                commands.entity(e).insert(crate::meals::RecipeBookBought);
            } else if std::ptr::eq(at.activity, &crate::gardening::BUY_SEEDS) {
                commands.entity(e).insert(crate::gardening::GardenRequest::BoughtSeeds);
                notes.push(format!("{} is back from the grocery store.", sim.first));
            } else if at.activity.name == "Go Fishing" {
                // The catch, sold: more and better fish with skill.
                let level = skills.level("Fishing") as f32 + if sim.traits.contains(&crate::life::Trait::Angler) { 2.0 } else { 0.0 };
                let mut rng = rand::rng();
                let caught = rng.random_range(1..=(2 + level as u32 / 2));
                let fish = ["minnows", "anchovies", "goldfish", "perch", "rainbow trout", "salmon", "tuna", "swordfish", "lobster", "angelfish"];
                let best = fish[(level as usize + rng.random_range(0..3)).min(fish.len() - 1)];
                let worth: i64 = (0..caught).map(|_| rng.random_range(5..15) + level as i64 * 6).sum();
                did.write(crate::journal::Did::count(e, crate::journal::Stat::Fish, caught as f64));
                if let Some(h) = household.as_deref_mut() {
                    h.funds += worth;
                }
                notes.push(format!("{} is back from fishing with {caught} fish (the best: {best}), sold for §{worth}.", sim.first));
                life.write(LifeEvent::new(e, LifeEventKind::Finished { activity: at.activity.name, completed: true }));
            } else {
                notes.push(format!("{} is back from {}.", sim.first, at.place));
                life.write(LifeEvent::new(e, LifeEventKind::Finished { activity: at.activity.name, completed: true }));
            }
            commands.entity(e).remove::<AtRabbitHole>().insert(Visibility::Inherited);
        }
    }
}

/// Homework from today's school, not yet done.
#[derive(Component)]
pub struct Homework;

/// A child's school performance, 0..100 (A+ at the top).
#[derive(Component, Clone, Copy)]
pub struct SchoolGrades(pub f32);

impl Default for SchoolGrades {
    fn default() -> Self {
        Self(55.0)
    }
}

impl SchoolGrades {
    pub fn letter(self) -> &'static str {
        match self.0 {
            g if g >= 90.0 => "A+",
            g if g >= 75.0 => "A",
            g if g >= 60.0 => "B",
            g if g >= 45.0 => "C",
            g if g >= 30.0 => "D",
            _ => "F",
        }
    }
}

/// Children catch the school bus on weekday mornings and come home mid-afternoon.
#[allow(clippy::type_complexity)]
fn school_bus(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut kids: Query<(Entity, &Sim, &mut ActionQueue, Option<&mut SchoolGrades>, Option<&AtRabbitHole>, Has<Homework>), With<HouseholdMember>>,
    mut last_day: Local<std::collections::HashMap<Entity, u32>>,
    mut notes: ResMut<Notifications>,
) {
    let h = clock.hour_f();
    let day = clock.day();
    for (e, sim, mut queue, mut grades, away, homework) in &mut kids {
        if !matches!(sim.age, Age::Child | Age::Teen) {
            continue;
        }
        if grades.is_none() {
            commands.entity(e).insert(SchoolGrades::default());
        }
        if away.is_some() || clock.weekday() >= 5 || !(7.25..9.0).contains(&h) || last_day.get(&e) == Some(&day) {
            continue;
        }
        last_day.insert(e, day);
        for a in queue.0.iter_mut() {
            a.cancel = true;
        }
        // Homework left undone goes against them.
        if homework {
            commands.entity(e).remove::<Homework>();
            if let Some(g) = grades.as_mut() {
                g.0 = (g.0 - 10.0).max(0.0);
            }
            notes.push(format!("{} didn't do their homework.", sim.first));
        }
        let day_start = (clock.minutes / 1440.0).floor() * 1440.0;
        let until = day_start + 15.0 * 60.0 + DRIVE_MINUTES;
        notes.push(format!("{} caught the school bus.", sim.first));
        commands.entity(e).insert((
            AtRabbitHole { lot: usize::MAX, activity: &SCHOOL, inside_from: clock.minutes + DRIVE_MINUTES, until, place: "school".into() },
            Visibility::Hidden,
        ));
    }
}
