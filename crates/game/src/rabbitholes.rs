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
        app.add_systems(Update, (school_bus, outings).chain().after(crate::save::apply_loaded_game).run_if(in_state(PlayMode::Live)));
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

impl Activity {
    fn apply_needs(&self, motives: &mut Motives, minutes: f32, wishes: Option<&crate::wishes::Wishes>, body_builder: bool) {
        for i in 0..6 {
            if i == BLADDER && crate::wishes::has(wishes, "SteelBladder") {
                motives.0[i] = 100.0;
                continue;
            }
            let mut rate = self.per_hour[i];
            if body_builder && i == ENERGY && rate < 0.0 { continue; }
            // The visit replaces ordinary hunger/bladder decay while off lot. Rewards
            // still apply to those losses, but must not reduce a meal's benefits or
            // prevent getting dirty from exercise.
            if rate < 0.0 && matches!(i, HUNGER | BLADDER) {
                rate *= crate::wishes::reward_decay(wishes, i);
            }
            motives.add(i, rate * minutes / 60.0);
        }
    }
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
pub static SCHOOL: Activity = act("School", 0.0, 0, [-6.0, -3.0, -6.0, 25.0, -6.0, -8.0], None);

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
    // (The name's first word that says what it is: past "Com_", "RH_", "Shell_" and the like,
    // whatever their case.)
    let raw = lot
        .internal_name
        .split('_')
        .find(|p| !p.is_empty() && !["com", "res", "rh", "shell", "bb"].contains(&p.to_ascii_lowercase().as_str()))
        .unwrap_or(&lot.internal_name);
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

impl AtRabbitHole {
    /// The overlap of this simulation tick with the visit, excluding both drives.
    fn inside_minutes(&self, now: f64, delta: f32) -> f32 {
        if delta <= 0.0 { return 0.0; }
        let start = (now - delta as f64).max(self.inside_from);
        let end = now.min(self.until - DRIVE_MINUTES);
        (end - start).max(0.0) as f32
    }
}

#[cfg(test)]
mod visit_time_tests {
    use super::*;

    #[test]
    fn visit_rewards_reduce_decay_without_weakening_meals_or_preventing_exercise_grime() {
        let wishes = crate::wishes::Wishes::restored(0,
            vec!["SteelBladder".into(), "HardlyHungry".into(), "PermaClean".into()], 0.0);
        let mut ordinary = Motives([50.0; 6]);
        let mut rewarded = ordinary.clone();
        GYM[0].apply_needs(&mut ordinary, 60.0, None, false);
        GYM[0].apply_needs(&mut rewarded, 60.0, Some(&wishes), false);
        assert_eq!(ordinary.0[BLADDER], 45.0);
        assert_eq!(rewarded.0[BLADDER], 100.0);
        assert_eq!(ordinary.0[HUNGER], 42.0);
        assert_eq!(rewarded.0[HUNGER], 48.0);
        assert_eq!(rewarded.0[HYGIENE], ordinary.0[HYGIENE]);
        assert_eq!(rewarded.0[HYGIENE], 30.0);
        let mut meal = Motives([0.0; 6]);
        EATERY[0].apply_needs(&mut meal, 30.0, Some(&wishes), false);
        assert_eq!(meal.0[HUNGER], 75.0, "Hardly Hungry must not reduce food satisfaction");
        let mut builder = Motives([50.0; 6]);
        GYM[0].apply_needs(&mut builder, 60.0, Some(&wishes), true);
        assert_eq!(builder.0[ENERGY], 50.0);
        assert_eq!(builder.0[HUNGER], rewarded.0[HUNGER]);
    }

    fn visit() -> AtRabbitHole {
        AtRabbitHole { lot: 0, activity: &GYM[0], inside_from: 25.0, until: 170.0, place: "Gym".into() }
    }

    #[test]
    fn arrival_departure_and_paused_ticks_exclude_driving() {
        let at = visit();
        assert_eq!(at.inside_minutes(20.0, 10.0), 0.0);
        assert_eq!(at.inside_minutes(30.0, 10.0), 5.0);
        assert_eq!(at.inside_minutes(145.0, 10.0), 10.0);
        assert_eq!(at.inside_minutes(150.0, 10.0), 5.0);
        assert_eq!(at.inside_minutes(170.0, 10.0), 0.0);
        assert_eq!(at.inside_minutes(50.0, 0.0), 0.0);
        assert_eq!(at.inside_minutes(50.0, -1.0), 0.0);
    }

    #[test]
    fn full_visit_time_is_identical_across_simulation_speeds() {
        let at = visit();
        for step in [0.25, 1.0, 7.0, 30.0, 200.0] {
            let mut now = 0.0;
            let mut total = 0.0;
            while now < 200.0 {
                now += step;
                total += at.inside_minutes(now, step as f32);
            }
            assert_eq!(total, 120.0, "tick size {step}");
        }
    }

    #[test]
    fn paid_visit_cancellation_cannot_generate_upfront_income() {
        let mut world = World::new();
        let me = world.spawn_empty().id();
        let sim = random_sim(&mut rand::rng(), "Test", Some(false), Age::Adult);
        let clock = GameClock::default();
        let mut notes = Notifications::default();
        let mut household = Household { name: "Test".into(), funds: 1000, lot_index: 0, last_bill_day: 0, bills: Vec::new() };
        let mut state = bevy::ecs::system::SystemState::<Commands>::new(&mut world);
        for _ in 0..3 {
            {
                let mut commands = state.get_mut(&mut world).unwrap();
                assert!(head_out(&mut commands, &clock, me, &sim, 0, &SCIENCE[0], "Science Lab".into(), Some(&mut household), &mut notes, 1.0));
            }
            state.apply(&mut world);
            assert!(world.get::<AtRabbitHole>(me).is_some());
            assert_eq!(household.funds, 1000, "starting paid work must not grant its completion stipend");
            world.entity_mut(me).remove::<AtRabbitHole>();
        }
        {
            let mut commands = state.get_mut(&mut world).unwrap();
            assert!(head_out(&mut commands, &clock, me, &sim, 0, &SHOW[0], "Theater".into(), Some(&mut household), &mut notes, 0.5));
        }
        state.apply(&mut world);
        assert_eq!(household.funds, 975, "discounted admission is still charged before entry");
    }
}

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
    mut q: Query<(Entity, &Sim, &AtRabbitHole, &mut Motives, &mut Skills, &mut Transform, Option<&mut SchoolGrades>, &mut crate::nav::Floor, Option<&crate::journal::SkillJournal>, Option<&crate::wishes::Wishes>)>,
    mut household: Option<ResMut<Household>>,
    mut did: MessageWriter<crate::journal::Did>,
) {
    for (e, sim, at, mut motives, mut skills, mut tf, grades, mut floor, journal, wishes) in &mut q {
        let dt = at.inside_minutes(clock.minutes, delta.0);
        if dt > 0.0 {
            // (Strength training at the gym, kept in their journal; a Body Builder isn't tired by it.)
            let strength = at.activity.name == "Work Out";
            let builder = strength && crate::journal::earned(journal, "Body Builder");
            at.activity.apply_needs(&mut motives, dt, wishes, builder);
            if strength {
                did.write(crate::journal::Did::count(e, crate::journal::Stat::StrengthHours, dt as f64 / 60.0));
            }
            if let Some(sk) = at.activity.skill {
                let v = skills.0.entry(sk).or_insert(0.0);
                let before = *v as u32;
                *v = (*v + dt / 60.0 * 0.35 * crate::life::skill_rate(&sim.traits, sk) * crate::wishes::reward_skill_rate(wishes) / (1.0 + *v * 0.25)).min(10.0);
                if *v as u32 > before {
                    notes.push(format!("{} reached level {} in {}!", sim.first, *v as u32, sk));
                    life.write(LifeEvent::new(e, LifeEventKind::SkillUp { skill: sk, level: *v as u32 }));
                }
            }
        }
        if clock.minutes >= at.until {
            // Paid work earns its stipend only after the visit finishes. Admission
            // discounts affect fees, not earnings; cancelling a visit cannot mint money.
            if at.activity.cost < 0 && let Some(h) = household.as_deref_mut() {
                let earned = at.activity.cost.saturating_neg();
                h.funds += earned;
                notes.push(format!("{} earned §{earned} at {}.", sim.first, at.place));
            }
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
                // Friday's assignment is due Monday, not omitted for the weekend.
                // A student who aged out of school during the visit gets no new work.
                if matches!(sim.age, Age::Child | Age::Teen) {
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

pub const HOMEWORK_BASE_RATE: f32 = 0.556;

/// Homework.xml base completion percentage per minute, with additive trait tuning.
pub fn homework_minutes(traits: &[crate::life::Trait], multitasker: bool) -> f32 {
    use crate::life::Trait;
    let mut rate = HOMEWORK_BASE_RATE;
    if traits.contains(&Trait::Bookworm) { rate += 0.278; }
    if traits.contains(&Trait::Workaholic) { rate += 0.278; }
    if traits.contains(&Trait::Perfectionist) { rate -= 0.15; }
    if multitasker { rate *= 1.5; }
    100.0 / rate
}

/// Claim an assignment in the exclusive command phase before granting any rewards.
/// Queued self-study and helpers can finish in the same update; only one succeeds.
pub fn complete_homework(world: &mut World, student: Entity, helper: Option<Entity>, minutes: f32) -> bool {
    let Some(sim) = world.get::<Sim>(student) else { return false };
    if !matches!(sim.age, Age::Child | Age::Teen) || world.get::<Homework>(student).is_none() {
        return false;
    }
    let student_name = sim.first.clone();
    let helper_name = match helper {
        Some(e) => {
            let Some(sim) = world.get::<Sim>(e) else { return false };
            if e == student || matches!(sim.age, Age::Baby | Age::Toddler | Age::Child) { return false; }
            Some(sim.first.clone())
        }
        None => None,
    };
    world.entity_mut(student).remove::<Homework>();
    if let Some(mut grade) = world.get_mut::<SchoolGrades>(student) {
        grade.0 = (grade.0 + if helper.is_some() { 9.0 } else { 6.0 }).min(100.0);
    }
    let message = if let (Some(helper), Some(name)) = (helper, helper_name) {
        world.write_message(crate::journal::Did::count(helper, crate::journal::Stat::TutoringHours, minutes as f64 / 60.0));
        if let Some(mut skills) = world.get_mut::<Skills>(helper) {
            let logic = skills.0.entry("Logic").or_insert(0.0);
            *logic = (*logic + minutes / 60.0 * 0.3 / (1.0 + *logic * 0.25)).min(10.0);
        }
        format!("{name} helped {student_name} with their homework.")
    } else {
        format!("{student_name} finished their homework.")
    };
    world.resource_mut::<Notifications>().push(message);
    true
}

/// A student's school performance, normalized to 0..100 (A at the top).
#[derive(Component, Clone, Copy)]
pub struct SchoolGrades(pub f32);

impl Default for SchoolGrades {
    fn default() -> Self {
        Self(55.0)
    }
}

impl SchoolGrades {
    /// Saves store a normalized 0–100 score; the original school performance bar
    /// and tuning use -100–100, just like career performance.
    pub fn performance(self) -> f32 {
        (self.0 - 50.0) * 2.0
    }

    pub fn letter(self) -> &'static str {
        // SchoolElementary and SchoolHigh share these GradeThreshold values.
        match self.performance() {
            g if g >= 90.0 => "A",
            g if g >= 30.0 => "B",
            g if g >= -30.0 => "C",
            g if g >= -90.0 => "D",
            _ => "F",
        }
    }
}

#[cfg(test)]
mod school_grade_tests {
    use super::SchoolGrades;

    #[test]
    fn original_school_thresholds_match_the_displayed_performance() {
        for (score, performance, letter) in [
            (100.0, 100.0, "A"), (95.0, 90.0, "A"), (94.5, 89.0, "B"),
            (65.0, 30.0, "B"), (64.5, 29.0, "C"), (35.0, -30.0, "C"),
            (34.5, -31.0, "D"), (5.0, -90.0, "D"), (4.5, -91.0, "F"), (0.0, -100.0, "F"),
        ] {
            let grade = SchoolGrades(score);
            assert_eq!(grade.performance(), performance);
            assert_eq!(grade.letter(), letter, "performance {performance}");
        }
        assert_eq!(SchoolGrades::default().letter(), "C");
    }
}

#[cfg(test)]
mod school_schedule_tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn homework_completion_uses_additive_trait_rates_and_multitasker() {
        use crate::life::Trait::*;
        for (traits, rate) in [
            (vec![], 0.556), (vec![Bookworm], 0.834), (vec![Workaholic], 0.834),
            (vec![Perfectionist], 0.406), (vec![Bookworm, Workaholic], 1.112),
            (vec![Bookworm, Workaholic, Perfectionist], 0.962),
        ] {
            assert!((homework_minutes(&traits, false) - 100.0 / rate).abs() < 0.001);
            assert!((homework_minutes(&traits, true) * 1.5 - homework_minutes(&traits, false)).abs() < 0.001);
        }
    }

    #[test]
    fn homework_and_tutoring_rewards_require_a_unique_live_assignment() {
        let mut app = App::new();
        app.init_resource::<Notifications>().add_message::<crate::journal::Did>();
        let mut rng = rand::rngs::StdRng::seed_from_u64(45);
        let student = app.world_mut().spawn((
            crate::sim::random_sim(&mut rng, "Student", Some(false), Age::Child),
            Homework, SchoolGrades(55.0),
        )).id();
        let helper = app.world_mut().spawn((
            crate::sim::random_sim(&mut rng, "Helper", Some(false), Age::Adult), Skills::default(),
        )).id();
        let world = app.world_mut();
        assert!(complete_homework(world, student, None, 0.0));
        assert!(!complete_homework(world, student, Some(helper), 60.0));
        assert_eq!(world.get::<SchoolGrades>(student).unwrap().0, 61.0);
        assert_eq!(world.get::<Skills>(helper).unwrap().0.get("Logic"), None);
        assert_eq!(world.resource::<Messages<crate::journal::Did>>().len(), 0);
        world.entity_mut(student).insert(Homework);
        assert!(complete_homework(world, student, Some(helper), 60.0));
        assert!(!complete_homework(world, student, None, 0.0));
        assert!(!complete_homework(world, student, Some(helper), 60.0));
        assert_eq!(world.get::<SchoolGrades>(student).unwrap().0, 70.0);
        assert_eq!(world.get::<Skills>(helper).unwrap().0.get("Logic"), Some(&0.3));
        assert_eq!(world.resource::<Messages<crate::journal::Did>>().len(), 1);
        world.entity_mut(student).insert(Homework);
        world.get_mut::<Sim>(student).unwrap().age = Age::YoungAdult;
        assert!(!complete_homework(world, student, Some(helper), 60.0));
        world.despawn(student);
        assert!(!complete_homework(world, student, None, 0.0));
    }

    #[test]
    fn bus_classroom_hours_and_weekends_follow_student_age() {
        for (age, end, classroom_minutes) in [(Age::Child, 15.0, 360.0), (Age::Teen, 14.0, 300.0)] {
            let mut app = App::new();
            app.init_resource::<GameClock>().init_resource::<Notifications>().add_systems(Update, school_bus);
            let mut rng = rand::rngs::StdRng::seed_from_u64(33);
            let sim = crate::sim::random_sim(&mut rng, "School", Some(false), age);
            let e = app.world_mut().spawn((sim, HouseholdMember, ActionQueue::default())).id();
            app.world_mut().resource_mut::<GameClock>().minutes = 7.5 * 60.0;
            app.update();
            assert!(app.world().get::<AtRabbitHole>(e).is_none(), "no 7:15 departure");
            app.world_mut().resource_mut::<GameClock>().minutes = 8.0 * 60.0;
            app.update();
            let at = app.world().get::<AtRabbitHole>(e).unwrap();
            assert_eq!(at.inside_from, 9.0 * 60.0);
            assert_eq!(at.until, end * 60.0 + DRIVE_MINUTES);
            assert_eq!(at.inside_minutes(8.5 * 60.0, 60.0), 0.0);
            assert_eq!(at.inside_minutes(at.until, 1000.0), classroom_minutes);
            app.world_mut().entity_mut(e).remove::<AtRabbitHole>();
            app.update();
            assert!(app.world().get::<AtRabbitHole>(e).is_none(), "only one bus per day");
            app.world_mut().resource_mut::<GameClock>().minutes = 5.0 * 1440.0 + 8.0 * 60.0;
            app.update();
            assert!(app.world().get::<AtRabbitHole>(e).is_none(), "no Saturday classes");
            app.world_mut().resource_mut::<GameClock>().minutes = 7.0 * 1440.0 + 8.0 * 60.0;
            app.update();
            assert!(app.world().get::<AtRabbitHole>(e).is_some(), "classes resume Monday");
        }
    }
}

/// Careers.xml: ElementaryStudent starts at 9 for six hours; HighSchoolStudent
/// starts at 9 for five hours. Commute time is separate from classroom time.
pub fn school_hours(age: Age) -> (f32, f32) {
    (9.0, if age == Age::Teen { 14.0 } else { 15.0 })
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
        let (start, end) = school_hours(sim.age);
        if away.is_some() || clock.weekday() >= 5 || !(start - 1.0..start).contains(&h) || last_day.get(&e) == Some(&day) {
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
        let until = day_start + end as f64 * 60.0 + DRIVE_MINUTES;
        let inside_from = (clock.minutes + DRIVE_MINUTES).max(day_start + start as f64 * 60.0);
        notes.push(format!("{} caught the school bus.", sim.first));
        commands.entity(e).insert((
            AtRabbitHole { lot: usize::MAX, activity: &SCHOOL, inside_from, until, place: "school".into() },
            Visibility::Hidden,
        ));
    }
}
