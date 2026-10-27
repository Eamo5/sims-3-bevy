//! Pets' needs, as the Pets pack has them: a household's cats, dogs and horses get hungry, need
//! the bathroom, tire, get lonely, dirty and bored, and cats itch to scratch, dogs to chew and
//! horses get thirsty, each falling as the game's `Motives` tuning has it for the species (its
//! bands' rates: a cat's hunger 7.5 an hour, a dog's 11.25, slowing as they run low).
//!
//! They see to them themselves, from what the household has bought them:
//! - eating from a pet bowl (the game's `PetBowl.EatFood`: Hunger 200 an hour, a bite of its
//!   eight every few minutes, until full or the bowl's empty; a bottomless bowl never is);
//! - a cat's litter box (`UseLitterBox`: Bladder 700 an hour), which gets dirtier with each use
//!   and is cleaned out by the household (`CleanLitterBox`); a very dirty one put off for the
//!   floor four times in five. Otherwise out into the yard (`DogPee`, `CatPee`: ten minutes,
//!   indoors five times in a hundred, thirty for the young), a puddle where it's on a floor;
//! - sleeping in a pet bed or dog house (`SleepPet`: Energy 21.33 an hour), or on the floor;
//! - a cat at its scratching post or cat condo (`Scratch`: CatScratch 200 and Fun 10 an hour), a
//!   dog at its chew toy (`Chew`: DogDestruction 200 and Fun 50), a horse at the water trough
//!   (`DrinkWater`: HorseThirst 250) and grazing; a cat grooming itself (`CatGroomSelf`: Hygiene
//!   200).
//!
//! Sims' attention (petting, praise, hugs) is company and fun for them too. Grown-ups of the
//! household fill an empty bowl (`FillBowl`: §25 a bite it's short) on their own once it's down
//! to three bites, and the bowl can be filled from its menu any time. Pets let it be known what
//! they need with thought balloons.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::{GameClock, SPEED_RATES};
use crate::interact::{GameObject, ObjectKind};
use crate::pets::{HomePet, Pet, PetAnim, PetBusy, PetTraits};

pub struct PetNeedsPlugin;

impl Plugin for PetNeedsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (bowls, fall, attention, choose, act, filled, chores, thoughts, needs_test).chain().run_if(in_state(PlayMode::Live)));
    }
}

pub const HUNGER: usize = 0;
pub const BLADDER: usize = 1;
pub const ENERGY: usize = 2;
pub const SOCIAL: usize = 3;
pub const HYGIENE: usize = 4;
pub const FUN: usize = 5;
/// A cat's urge to scratch (`CatScratch`), a dog's to chew (`DogDestruction`), a horse's to run
/// (`HorseExercise`).
pub const URGE: usize = 6;
/// A horse's thirst (`HorseThirst`).
pub const THIRST: usize = 7;

/// A pet's needs, -100 to 100 as a Sim's.
#[derive(Component, Clone, Copy, Debug)]
pub struct PetMotives(pub [f32; 8]);

impl PetMotives {
    /// A pet come home: as the game starts them (`initialMin`..`initialMax`), hunger part way.
    pub fn new(kind: &str, rng: &mut impl Rng) -> Self {
        let _ = kind;
        PetMotives([rng.random_range(20.0..60.0), rng.random_range(75.0..100.0), 83.0, rng.random_range(95.0..100.0), rng.random_range(80.0..95.0), rng.random_range(0.0..30.0), rng.random_range(75.0..100.0), rng.random_range(75.0..100.0)])
    }

    pub fn add(&mut self, i: usize, v: f32) {
        self.0[i] = (self.0[i] + v).clamp(-100.0, 100.0);
    }
}

fn species(kind: &str) -> char {
    kind.chars().nth(1).unwrap_or('c')
}

fn grown(kind: &str) -> bool {
    kind.starts_with('a')
}

/// How much a need falls in a game hour on its own, by the game's `Motives` tuning for the
/// species (each band's `Decay`).
pub fn decay(kind: &str, i: usize, v: f32) -> f32 {
    let v = v.trunc();
    let sp = species(kind);
    match i {
        HUNGER => match v {
            v if v >= -35.0 => if sp == 'c' { 7.5 } else { 11.25 },
            v if v >= -60.0 => if sp == 'c' { 4.0 } else { 8.0 },
            _ => 0.791_666_7,
        },
        BLADDER => match v {
            v if v >= -60.0 => 13.333_34,
            v if v >= -90.0 => 14.5,
            _ => 9.0,
        },
        ENERGY => match v {
            v if v >= 100.0 => 0.0,
            v if v >= -59.0 => 10.6,
            v if v > -100.0 => 6.333_333,
            _ => 0.0,
        },
        SOCIAL => match v {
            v if v >= -60.0 => 6.666_667,
            v if v >= -88.0 => 2.7,
            _ => 1.375,
        },
        // (Grown pets only: the young have no hygiene tuning.)
        HYGIENE if grown(kind) => 200.0 / 84.0,
        FUN if v > 0.0 => 7.0,
        URGE if sp == 'h' => match v {
            v if v >= -60.0 => 11.428_57,
            v if v >= -98.0 => 6.166_667,
            _ => 0.0,
        },
        URGE => 200.0 / 15.0,
        THIRST if sp == 'h' && grown(kind) => match v {
            v if v >= -60.0 => 11.428_57,
            v if v >= -98.0 => 3.7,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

/// How a pet feels on the whole (-100 to 100): its needs together, as the ring round its face.
pub fn mood(m: &PetMotives) -> f32 {
    [HUNGER, BLADDER, ENERGY, SOCIAL, HYGIENE, FUN].iter().map(|i| m.0[*i]).sum::<f32>() / 6.0
}

/// The game's pet moodlets' names for needs running low, worst first (`HungryPet`,
/// `VeryHungryPet`, `HasToPeePet`, `TiredPet`, `LonelyPet`, `GrungyPet`, `StrainedPet`; a cat's
/// `ItchingToScratch`, a dog's `PentUp`, a horse's `ThirstyHorse`).
pub fn feelings(kind: &str, m: &PetMotives) -> Vec<&'static str> {
    let sp = species(kind);
    let mut out: Vec<(f32, &'static str)> = Vec::new();
    let mut low = |i: usize, bands: &[(f32, &'static str)]| {
        if let Some((_, n)) = bands.iter().find(|(at, _)| m.0[i] < *at) {
            out.push((m.0[i], n));
        }
    };
    low(HUNGER, &[(-80.0, "Starving"), (-60.0, "Very Hungry"), (-35.0, "Hungry")]);
    low(BLADDER, &[(-90.0, "Really Has To Pee"), (-60.0, "Has To Pee")]);
    low(ENERGY, &[(-99.0, "Exhausted"), (-79.0, "Tired"), (-59.0, "Sleepy")]);
    low(SOCIAL, &[(-88.0, "Desolate"), (-60.0, "Lonely")]);
    low(HYGIENE, &[(-80.0, "Smelly"), (-60.0, "Grungy")]);
    low(FUN, &[(-50.0, "Stressed"), (-20.0, "Strained")]);
    match sp {
        'c' => low(URGE, &[(-98.0, "Claws Out"), (-60.0, "Itching To Scratch")]),
        'd' | 'l' => low(URGE, &[(-98.0, "Going Nuts"), (-60.0, "Pent Up")]),
        'h' => {
            low(URGE, &[(-98.0, "Frustrated"), (-60.0, "Restless")]);
            low(THIRST, &[(-98.0, "Dehydrated"), (-60.0, "Thirsty")]);
        }
        _ => {}
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().map(|o| o.1).collect()
}

/// A pet's skewer tooltip: its name and kind, its traits, and how it's feeling.
pub fn tooltip(pet: &Pet, m: &PetMotives, traits: &[String], gu: Option<&crate::icons::GameUi>) -> String {
    let f = feelings(&pet.kind, m);
    let how = if f.is_empty() { "Content".to_string() } else { f.join(", ") };
    let names: Vec<String> = traits.iter().map(|t| crate::pettraits::name(t, gu)).collect();
    let traits = if names.is_empty() { String::new() } else { format!("\n{}", names.join(", ")) };
    format!("{} ({}){traits}\n{how}", pet.name, crate::pets::kind_name(&pet.kind))
}

/// What's in a pet bowl: bites left (`kFullBowlFoodUnits`, eight).
#[derive(Component, Clone, Copy, Debug)]
pub struct BowlFood(pub u8);

pub const FULL_BOWL: u8 = 8;
/// Down to this, the household fills it of its own accord (`kMinFoodUnitsForAutonomousFill`).
pub const REFILL_AT: u8 = 3;
/// What filling a bite costs (`kBaseCostToFillBowl`).
pub const BITE_COST: i64 = 25;

/// Whether a bowl never empties (the Bottomless Pet Bowl).
pub fn bottomless(o: &GameObject) -> bool {
    o.name.to_ascii_lowercase().contains("bottomless")
}

/// What filling a bowl costs now: §25 a bite it's short (`GetCostToFillPetBowl`).
pub fn fill_cost(food: u8) -> i64 {
    BITE_COST * (FULL_BOWL.saturating_sub(food)) as i64
}

/// A bowl just filled (by `interact`'s Fill Bowl).
#[derive(Component)]
pub struct BowlFilled;

/// Bowls bought start empty, as the game's do.
fn bowls(mut commands: Commands, q: Query<(Entity, &GameObject), (Without<BowlFood>, Without<crate::visit::LotObject>)>) {
    for (e, o) in &q {
        if o.kind == ObjectKind::PetBowl {
            commands.entity(e).insert(BowlFood(if bottomless(o) { FULL_BOWL } else { 0 }));
        }
    }
}

/// A bowl filled: paid for, full.
fn filled(mut commands: Commands, mut q: Query<(Entity, &mut BowlFood), With<BowlFilled>>, mut household: Option<ResMut<crate::interact::Household>>, mut notes: ResMut<crate::interact::Notifications>) {
    for (e, mut food) in &mut q {
        commands.entity(e).remove::<BowlFilled>();
        let cost = fill_cost(food.0);
        if let Some(h) = household.as_mut() {
            if h.funds < cost {
                notes.push("Not enough Simoleons to fill the bowl.");
                continue;
            }
            h.funds -= cost;
        }
        food.0 = FULL_BOWL;
    }
}

/// The game's rates for what pets do: the pet bowl's, the litter box's, peeing's (ten minutes to
/// full), a pet bed's and the floor's, the scratching post's and the chew toy's, the trough's,
/// grazing's and grooming's (an hour).
const EAT_RATE: f32 = 200.0;
const LITTER_RATE: f32 = 700.0;
const PEE_MINUTES: f64 = 10.0;
const BED_RATE: f32 = 21.33;
const FLOOR_RATE: f32 = 16.0;
const SCRATCH_RATE: f32 = 200.0;
const CHEW_RATE: f32 = 200.0;
const DRINK_RATE: f32 = 250.0;
const GRAZE_RATE: f32 = 100.0;
/// A box stall's hay (`EatHay`: Hunger 100 an hour) and a nap in it (`HorseNap`: Energy 24); a
/// pee in it one of the dirty spots that want it cleaned (the game's `kPointsBeforeDirtyLow`,
/// five; a quarter of the way at each), and its hay rack (in the stall's own frame).
const HAY_RATE: f32 = 100.0;
const STALL_NAP_RATE: f32 = 24.0;
const STALL_DIRT: f32 = crate::grime::NEEDS_CLEANING / 5.0 * 1.01;
const HAY_RACK: Vec2 = Vec2::new(-1.67, -2.0);

/// Where a horse stands to eat at a box stall's hay rack (its head, at the front of its rig, a
/// metre short of the rack, along the stall's side wall), and which way it faces.
fn hay_spot(stall: &Transform) -> (Vec2, Quat) {
    let at = stall.transform_point(Vec3::new(HAY_RACK.x, 0.0, HAY_RACK.y + 0.95));
    (at.xz(), stall.rotation * Quat::from_rotation_y(std::f32::consts::PI))
}
const GROOM_RATE: f32 = 200.0;
/// How often a bite's taken (game minutes).
const BITE_MINUTES: f64 = 5.0;
/// What a use of the litter box leaves (its grime: dirty after three uses, very dirty six).
const LITTER_DIRT: f32 = 10.0;
/// The longest a pet keeps at something before thinking again (game minutes).
const LONGEST: f64 = 180.0;

/// What a pet's seeing to, and where.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PetDo {
    Eat(Entity),
    Litter(Entity),
    Pee,
    Sleep(Option<Entity>),
    Scratch(Entity),
    Chew(Entity),
    Drink(Entity),
    Graze,
    Groom,
    /// A horse at its box stall's hay (`BoxStall.EatHay`), or peeing in it (a dirty spot).
    Hay(Entity),
    StallPee(Entity),
}

impl PetDo {
    /// The need it's for.
    fn need(self) -> usize {
        match self {
            PetDo::Eat(_) | PetDo::Graze | PetDo::Hay(_) => HUNGER,
            PetDo::Litter(_) | PetDo::Pee | PetDo::StallPee(_) => BLADDER,
            PetDo::Sleep(_) => ENERGY,
            PetDo::Scratch(_) | PetDo::Chew(_) => URGE,
            PetDo::Drink(_) => THIRST,
            PetDo::Groom => HYGIENE,
        }
    }

    fn object(self) -> Option<Entity> {
        match self {
            PetDo::Eat(e) | PetDo::Litter(e) | PetDo::Scratch(e) | PetDo::Chew(e) | PetDo::Drink(e) | PetDo::Sleep(Some(e)) | PetDo::Hay(e) | PetDo::StallPee(e) => Some(e),
            _ => None,
        }
    }
}

/// A pet at something: on its way (its path), then at it since.
#[derive(Component, Debug)]
pub struct PetTask {
    pub what: PetDo,
    path: Vec<Vec2>,
    since: Option<f64>,
    next_bite: f64,
    /// A roll for how it goes about it (its eating style, a Piggy cat's litter kicked about).
    roll: f64,
}

/// When each pet thinks again about what it needs (game minutes).
#[derive(Component)]
struct PetThink(f64);

/// Needs fall as the game's tuning has them; a pet asleep (curled up on the floor at night)
/// rests.
fn fall(time: Res<Time>, clock: Res<GameClock>, mut q: Query<(&Pet, &mut PetMotives, &PetAnim, Option<&PetTask>, Option<&PetTraits>)>) {
    let hours = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed] / 60.0;
    if hours <= 0.0 || crate::cheats::no_decay() {
        return;
    }
    for (pet, mut m, anim, task, traits) in &mut q {
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        for i in 0..8 {
            let d = decay(&pet.kind, i, m.0[i]) * crate::pettraits::decay_multiplier(&pet.kind, traits, i);
            m.add(i, -d * hours);
        }
        // (Asleep without a bed: on the floor, at night as `pets` has them.)
        if task.is_none() && anim.clip.contains("sleep") {
            let d = decay(&pet.kind, ENERGY, m.0[ENERGY]) * crate::pettraits::decay_multiplier(&pet.kind, traits, ENERGY);
            m.add(ENERGY, (FLOOR_RATE * crate::pettraits::sleep_multiplier(traits) + d) * hours);
        }
    }
}

/// What a game of tug of war takes out of a dog an hour (50 over its quarter hour).
const TUG_ENERGY: f32 = -200.0;

/// A Sim's time with a pet (petting, praise, a hug, a treat): company and fun for the pet too.
fn attention(time: Res<Time>, clock: Res<GameClock>, sims: Query<&crate::interact::ActionQueue>, mut pets: Query<(&mut PetMotives, Option<&PetTraits>)>) {
    let hours = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed] / 60.0;
    for q in &sims {
        let Some(a) = q.0.front() else { continue };
        let (crate::interact::ActionKind::PetSocial { target, social, .. }, crate::interact::Phase::Running(_)) = (&a.kind, a.phase) else { continue };
        let (Ok((mut m, traits)), Some(s)) = (pets.get_mut(*target), crate::pets::PET_SOCIALS.get(*social)) else { continue };
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        m.add(SOCIAL, s.social * crate::pettraits::gain_multiplier(traits, SOCIAL) * hours);
        m.add(FUN, s.fun * crate::pettraits::gain_multiplier(traits, FUN) * hours);
        // (Brushed: cleaner.)
        m.add(HYGIENE, s.hygiene * hours);
        // (Tug of war: tiring, the pack's `kTugOfWarEnergyCommodityUpdate` over the game.)
        if s.name == "Tug of War" {
            m.add(ENERGY, TUG_ENERGY * hours);
        }
    }
}

/// How pressing a need is for a pet (none: not at all), by how low it's run.
fn urgent(i: usize, v: f32, night: bool) -> Option<f32> {
    let at = match i {
        HUNGER => 0.0,
        BLADDER => -20.0,
        ENERGY if night => 60.0,
        ENERGY => -40.0,
        URGE => -30.0,
        THIRST => -10.0,
        HYGIENE => -20.0,
        _ => return None,
    };
    (v < at).then_some(at - v)
}

/// The pets on the lot that are free think now and then about what they need most, and set off
/// for what's there to see to it: what the household's bought them first, else what they can do
/// anywhere.
#[allow(clippy::type_complexity)]
fn choose(
    mut commands: Commands,
    clock: Res<GameClock>,
    grid: Option<Res<crate::nav::NavGrid>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    pets: Query<(Entity, &Pet, &PetMotives, &Transform, Option<&PetThink>, Option<&PetTraits>), (With<HomePet>, Without<PetTask>, Without<PetBusy>, Without<crate::petcompany::Greeting>, Without<crate::petcompany::Greeted>)>,
    objects: Query<(Entity, &GameObject, &Transform, Option<&BowlFood>, Option<&crate::grime::Grime>), Without<crate::visit::LotObject>>,
    taken: Query<&PetTask>,
) {
    let Some(grid) = grid else { return };
    let now = clock.minutes;
    let night = !(6.0..21.0).contains(&clock.hour_f());
    let mut rng = rand::rng();
    let busy: Vec<Entity> = taken.iter().filter_map(|t| t.what.object()).collect();
    for (e, pet, m, tf, think, traits) in &pets {
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        if think.is_some_and(|t| now < t.0) {
            continue;
        }
        commands.entity(e).insert(PetThink(now + rng.random_range(10.0..20.0)));
        let sp = species(&pet.kind);
        let here = tf.translation.xz();
        let nearest = |want: &dyn Fn(&GameObject, Option<&BowlFood>, Option<&crate::grime::Grime>) -> bool| {
            objects
                .iter()
                .filter(|(oe, o, _, f, g)| !busy.contains(oe) && want(o, *f, *g))
                .min_by(|a, b| a.2.translation.xz().distance(here).total_cmp(&b.2.translation.xz().distance(here)))
                .map(|(oe, o, otf, ..)| (oe, approach(o, otf, here, &pet.kind)))
        };
        // The most pressing need, and what's there for it.
        // (A Proud cat grooms itself from half clean.)
        let mut needs: Vec<(f32, usize)> = (0..8)
            .filter_map(|i| {
                let u = match i {
                    HYGIENE if sp == 'c' => {
                        let at = crate::pettraits::groom_at(traits);
                        (m.0[i] < at).then_some(at - m.0[i])
                    }
                    _ => urgent(i, m.0[i], night),
                };
                u.map(|u| (u, i))
            })
            .collect();
        needs.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut pick: Option<(PetDo, Option<Vec2>)> = None;
        for (_, i) in needs {
            pick = match (i, sp) {
                // (A horse to its box stall's hay, napping and peeing in it, where it has one.)
                (HUNGER, 'h') => nearest(&|o, _, _| o.kind == ObjectKind::BoxStall)
                    .and_then(|(s, _)| objects.get(s).ok().map(|(_, _, stf, ..)| (PetDo::Hay(s), Some(hay_spot(stf).0))))
                    .or(Some((PetDo::Graze, None))),
                (BLADDER, 'h') if !night => nearest(&|o, _, _| o.kind == ObjectKind::BoxStall).map(|(s, at)| (PetDo::StallPee(s), Some(at))).or_else(|| Some((PetDo::Pee, outdoors(&grid, building.as_deref(), here, pet, traits, &mut rng)))),
                (HUNGER, _) => nearest(&|o, f, _| o.kind == ObjectKind::PetBowl && f.is_some_and(|f| f.0 > 0)).map(|(b, at)| (PetDo::Eat(b), Some(at))),
                (BLADDER, 'c') => match nearest(&|o, _, _| o.kind == ObjectKind::LitterBox) {
                    Some((b, at)) => Some((PetDo::Litter(b), Some(at))),
                    None => Some((PetDo::Pee, outdoors(&grid, building.as_deref(), here, pet, traits, &mut rng))),
                },
                (BLADDER, _) => Some((PetDo::Pee, outdoors(&grid, building.as_deref(), here, pet, traits, &mut rng))),
                (ENERGY, 'h') => Some(nearest(&|o, _, _| o.kind == ObjectKind::BoxStall).map_or((PetDo::Sleep(None), None), |(s, at)| (PetDo::Sleep(Some(s)), Some(at)))),
                (ENERGY, _) => match nearest(&|o, _, _| o.kind == ObjectKind::PetBed) {
                    Some((b, at)) => Some((PetDo::Sleep(Some(b)), Some(at))),
                    None => Some((PetDo::Sleep(None), None)),
                },
                (URGE, 'c') => nearest(&|o, _, _| o.kind == ObjectKind::ScratchingPost).map(|(s, at)| (PetDo::Scratch(s), Some(at))),
                (URGE, 'd' | 'l') => nearest(&|o, _, _| o.kind == ObjectKind::ChewToy).map(|(s, at)| (PetDo::Chew(s), Some(at))),
                (THIRST, 'h') => nearest(&|o, _, _| o.kind == ObjectKind::WaterTrough).map(|(s, at)| (PetDo::Drink(s), Some(at))),
                (HYGIENE, 'c') => Some((PetDo::Groom, None)),
                _ => None,
            };
            if pick.is_some() {
                break;
            }
        }
        let Some((what, to)) = pick else { continue };
        // (Into a box or a bed: to beside it on the grid, then in, as the game's slots step in.)
        let path = match to {
            Some(to) => match grid.find_path(here, to).or_else(|| grid.nearest_free(to).and_then(|(x, z)| grid.find_path(here, grid.center_of(x, z)))) {
                Some(mut p) => {
                    if p.last().is_none_or(|l| l.distance(to) > 0.05) {
                        p.push(to);
                    }
                    p
                }
                None => continue,
            },
            None => Vec::new(),
        };
        info!("pet {} sets off to {what:?} ({:.0} m)", pet.name, path_len(here, &path));
        commands.entity(e).insert(PetTask { what, path, since: None, next_bite: 0.0, roll: rng.random() });
    }
}

/// Where a pet goes to use something (the game's pet routing slots): into a litter box or a
/// bed, its head over a bowl, up to a scratching post, a chew toy or a trough, from its own side.
fn approach(o: &GameObject, tf: &Transform, from: Vec2, kind: &str) -> Vec2 {
    let c = o.world_center(tf).xz();
    let reach = match (o.kind, species(kind)) {
        (ObjectKind::LitterBox | ObjectKind::PetBed | ObjectKind::BoxStall, _) => return c,
        (ObjectKind::WaterTrough, _) => o.half.max_element() + 0.9,
        (_, 'd') => o.half.max_element() + 0.45,
        _ => o.half.max_element() + 0.25,
    };
    c + (from - c).normalize_or(Vec2::Y) * reach
}

/// How far a path goes.
fn path_len(from: Vec2, path: &[Vec2]) -> f32 {
    let mut at = from;
    path.iter().map(|p| {
        let d = p.distance(at);
        at = *p;
        d
    }).sum()
}

/// Somewhere out in the yard near a pet, to pee (and now and then, the young ones especially, not:
/// where they are).
fn outdoors(grid: &crate::nav::NavGrid, building: Option<&crate::building::ActiveBuilding>, here: Vec2, pet: &Pet, traits: &[String], rng: &mut impl Rng) -> Option<Vec2> {
    let indoors = |p: Vec2| building.is_some_and(|b| b.is_indoors(Vec3::new(p.x, 0.0, p.y)));
    if !indoors(here) || rng.random_bool(crate::pettraits::pee_inside_chance(&pet.kind, traits)) {
        return None;
    }
    for r in [4.0, 7.0, 11.0, 16.0, 22.0] {
        for k in 0..12 {
            let a = k as f32 / 12.0 * std::f32::consts::TAU + rng.random_range(0.0..0.5);
            let p = here + Vec2::from_angle(a) * r;
            if let Some((x, z)) = grid.nearest_free(p) {
                let c = grid.center_of(x, z);
                if !indoors(c) {
                    return Some(c);
                }
            }
        }
    }
    None
}

/// The clips for what a pet's doing (its kind's own, where it has them).
fn clip_for(kind: &str, what: PetDo, step: u32, traits: &[String], roll: f64) -> String {
    let k = kind;
    match what {
        // (In one of its ways, the same all meal.)
        PetDo::Eat(_) => {
            let styles = crate::pettraits::eating_styles(k, traits);
            crate::pettraits::eat_clip(k, styles[(roll * styles.len() as f64) as usize % styles.len()], step)
        }
        PetDo::Litter(_) if crate::pettraits::piggy_litter(traits, roll) => format!("{k}_litterBox_use_piggy_x"),
        PetDo::Litter(_) => format!("{k}_litterBox_use_normal_x"),
        PetDo::Pee if k.ends_with('c') => format!("{k}_pee_x"),
        PetDo::Pee if k.ends_with('h') => format!("{k}_pee_breathe_x"),
        PetDo::Pee => format!("{}_pee_male_x", if k == "cl" { "cd" } else { k }),
        PetDo::Sleep(Some(_)) if !k.ends_with('h') => format!("{}2o_petBed_sleep_back_loopBreathe_x", if k == "cl" { "cd" } else { k }),
        PetDo::Sleep(_) if k.ends_with('h') => format!("{k}_nap_loop_x"),
        PetDo::Sleep(_) => format!("{k}_sleep_loop1_x"),
        PetDo::Scratch(_) => format!("ac2o_scratchingPost_scratchPost_loop{}_x", 1 + step % 2),
        PetDo::Chew(_) => format!("{}2o_chewToy_play_loop{}_x", if k == "cl" { "cd" } else { k }, 1 + step % 2),
        PetDo::Drink(_) => format!("ah2o_waterTrough_drink_loop{}_x", 1 + step % 3),
        PetDo::Graze => format!("ah2o_hayPile_eat_loop{}_x", 1 + step % 4),
        PetDo::Hay(_) => format!("ah2o_boxStall_eatHay_loop{}_x", 1 + step % 3),
        PetDo::StallPee(_) => format!("{k}_pee_breathe_x"),
        PetDo::Groom => format!("{k}_idle_sit_groomSelf_loop{}_x", 1 + step % 2),
    }
}

/// Pets at what they've set off for: along the way, then at it, the need filling at the game's
/// rate until it's full (or the bowl's empty, or it's gone on too long); a litter box dirtier
/// for each use, a puddle where a pet pees on a floor.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn act(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    world: Res<crate::loading::CurrentWorld>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut pets: Query<(Entity, &Pet, &mut PetTask, &mut PetMotives, &mut Transform, &mut PetAnim, Option<&PetTraits>), Without<PetBusy>>,
    mut objects: Query<(&GameObject, &Transform, Option<&mut BowlFood>, Option<&mut crate::grime::Grime>), Without<Pet>>,
    mut puddles: MessageWriter<crate::puddles::AddPuddle>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed].min(3.0);
    let hours = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed] / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, pet, mut task, mut m, mut tf, mut anim, traits) in &mut pets {
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        let here = tf.translation.xz();
        // On the way.
        if let Some(&next) = task.path.first() {
            let d = next - here;
            let step = pet.walk_speed() * 1.6 * dt;
            if d.length() <= step.max(0.05) {
                task.path.remove(0);
            } else {
                let p = here + d.normalize() * step;
                let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(p.x, tf.translation.y, p.y));
                tf.translation = Vec3::new(p.x, y, p.y);
                tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
                anim.play(format!("{}_trot_x", pet.kind));
            }
            continue;
        }
        let what = task.what;
        // There: turned to what it's at, and at it.
        let since = *task.since.get_or_insert_with(|| {
            if let Some(o) = what.object().and_then(|o| objects.get(o).ok()) {
                // (In a box or a bed: along it; else turned to it.)
                if o.0.kind == ObjectKind::BoxStall && matches!(what, PetDo::Hay(_)) {
                    // (At the stall's hay rack, its back corner, head to it.)
                    let (at, turn) = hay_spot(o.1);
                    tf.translation = Vec3::new(at.x, tf.translation.y, at.y);
                    tf.rotation = turn;
                } else if matches!(o.0.kind, ObjectKind::LitterBox | ObjectKind::PetBed | ObjectKind::BoxStall) {
                    tf.rotation = o.1.rotation;
                } else {
                    let to = o.0.world_center(o.1).xz() - here;
                    if to.length() > 0.05 {
                        tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                    }
                }
            }
            now
        });
        let step = ((now - since) / BITE_MINUTES) as u32;
        anim.play(clip_for(&pet.kind, what, step, traits, task.roll));
        let need = what.need();
        let mut done = now - since > LONGEST;
        match what {
            PetDo::Eat(b) => {
                // (A bite at a time, from the bowl: none left, done.)
                let Ok((o, _, Some(mut food), _)) = objects.get_mut(b) else {
                    finish(&mut commands, e, &mut anim, pet, &mut rng);
                    continue;
                };
                if now >= task.next_bite {
                    task.next_bite = now + BITE_MINUTES;
                    if food.0 == 0 {
                        done = true;
                    } else if !bottomless(o) {
                        food.0 -= 1;
                    }
                }
                m.add(HUNGER, EAT_RATE * hours);
            }
            PetDo::Litter(b) => {
                if task.next_bite == 0.0 {
                    task.next_bite = now;
                    // (A box too dirty for it put off for the floor, as its traits have it; a
                    // Skittish cat startled off it half the time.)
                    let grime = objects.get(b).ok().and_then(|o| o.3.map(|g| g.0)).unwrap_or(0.0);
                    if crate::pettraits::refuses_box(traits, grime, rng.random()) || crate::pettraits::startled_off_box(traits, rng.random()) {
                        task.what = PetDo::Pee;
                        task.since = None;
                        continue;
                    }
                    if let Ok((_, _, _, Some(mut g))) = objects.get_mut(b) {
                        g.0 = (g.0 + LITTER_DIRT).min(100.0);
                    }
                }
                m.add(BLADDER, LITTER_RATE * hours);
            }
            PetDo::Pee => {
                if task.next_bite == 0.0 {
                    task.next_bite = now;
                    // (On a floor, a puddle: the game's `AddPuddle`, a little one for cats and
                    // little dogs.)
                    if building.as_deref().is_some_and(|b| b.is_indoors(tf.translation)) {
                        puddles.write(crate::puddles::AddPuddle { at: tf.translation, level: 1, lasts: None });
                    }
                }
                // (`LerpToFill` over ten minutes.)
                let left = (100.0 - m.0[BLADDER]).max(0.0);
                let minutes_left = (PEE_MINUTES - (now - since)).max(0.5) as f32;
                m.add(BLADDER, left * (hours * 60.0 / minutes_left).min(1.0));
                done |= now - since >= PEE_MINUTES;
            }
            PetDo::Sleep(bed) => {
                let rate = if bed.is_some() { if pet.kind.ends_with('h') { STALL_NAP_RATE } else { BED_RATE } } else { FLOOR_RATE } * crate::pettraits::sleep_multiplier(traits);
                let d = decay(&pet.kind, ENERGY, m.0[ENERGY]) * crate::pettraits::decay_multiplier(&pet.kind, traits, ENERGY);
                m.add(ENERGY, (rate + d) * hours);
                // (Up with the morning once rested enough.)
                let day = (6.0..21.0).contains(&clock.hour_f());
                done |= day && m.0[ENERGY] > 80.0;
            }
            PetDo::Scratch(_) => {
                m.add(URGE, SCRATCH_RATE * hours);
                m.add(FUN, 10.0 * hours);
            }
            PetDo::Chew(_) => {
                m.add(URGE, CHEW_RATE * hours);
                m.add(FUN, 50.0 * hours);
            }
            PetDo::Drink(_) => m.add(THIRST, DRINK_RATE * hours),
            PetDo::Graze => m.add(HUNGER, GRAZE_RATE * hours),
            PetDo::Hay(_) => m.add(HUNGER, HAY_RATE * hours),
            PetDo::StallPee(s) => {
                // (A dirty spot in the stall, to be cleaned out: `kPointsBeforeDirty`.)
                if task.next_bite == 0.0 {
                    task.next_bite = now;
                    if let Ok((_, _, _, Some(mut g))) = objects.get_mut(s) {
                        g.0 = (g.0 + STALL_DIRT).min(100.0);
                    }
                }
                let left = (100.0 - m.0[BLADDER]).max(0.0);
                let minutes_left = (PEE_MINUTES - (now - since)).max(0.5) as f32;
                m.add(BLADDER, left * (hours * 60.0 / minutes_left).min(1.0));
                done |= now - since >= PEE_MINUTES;
            }
            PetDo::Groom => m.add(HYGIENE, GROOM_RATE * hours),
        }
        if done || m.0[need] >= 99.0 {
            info!("pet {} done with {what:?}: {:?}", pet.name, m.0.map(|v| v.round()));
            finish(&mut commands, e, &mut anim, pet, &mut rng);
        }
    }
}

fn finish(commands: &mut Commands, e: Entity, anim: &mut PetAnim, pet: &Pet, rng: &mut impl Rng) {
    commands.entity(e).remove::<PetTask>();
    anim.play(crate::pets::idle_clip(&pet.kind, false, rng));
}

/// The household's grown-ups fill a pet bowl that's down to three bites, of their own accord
/// (the game's `FillBowl` autonomy: at home, with a cat or dog in the household to feed).
#[allow(clippy::type_complexity)]
fn chores(
    clock: Res<GameClock>,
    household: Option<Res<crate::interact::Household>>,
    pets: Query<&Pet, With<HomePet>>,
    bowls: Query<(Entity, &GameObject, &BowlFood), Without<crate::visit::LotObject>>,
    mut sims: Query<(&crate::sim::Sim, &mut crate::interact::ActionQueue, Option<&crate::visit::OnLot>), With<crate::sim::HouseholdMember>>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 30.0;
    if !pets.iter().any(|p| matches!(species(&p.kind), 'c' | 'd' | 'l')) {
        return;
    }
    let Some(h) = household else { return };
    let Some((bowl, _, food)) = bowls.iter().find(|(_, o, f)| f.0 <= REFILL_AT && !bottomless(o)) else { return };
    if h.funds < fill_cost(food.0) {
        return;
    }
    let def = crate::interact::interactions_for(ObjectKind::PetBowl).iter().position(|d| d.special == crate::interact::Special::FillPetBowl).unwrap_or(0);
    let already = sims.iter().any(|(_, q, _)| q.0.iter().any(|a| matches!(a.kind, crate::interact::ActionKind::Object { target, .. } if target == bowl)));
    if already {
        return;
    }
    if let Some((sim, mut q, _)) = sims.iter_mut().find(|(s, q, lot)| s.age.is_grown() && lot.is_none() && q.0.is_empty()) {
        info!("{} fills the pet bowl", sim.first);
        q.0.push_back(crate::interact::Action::new("Fill Bowl", crate::interact::ActionKind::Object { target: bowl, def }, true));
    }
}

/// A pet lets it be known what it needs, now and then, in a thought balloon (the game's needs'
/// pictures), the hungrier the more often.
fn thoughts(mut commands: Commands, clock: Res<GameClock>, pets: Query<(Entity, &PetMotives, Option<&PetTask>, &InheritedVisibility), With<HomePet>>, mut next: Local<std::collections::HashMap<Entity, f64>>) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, m, task, vis) in &pets {
        if !vis.get() || task.is_some_and(|t| t.since.is_some()) {
            continue;
        }
        let at = next.entry(e).or_insert(now + rng.random_range(5.0..30.0));
        if now < *at {
            continue;
        }
        let Some((i, v)) = [HUNGER, BLADDER, ENERGY, SOCIAL, HYGIENE, FUN].into_iter().map(|i| (i, m.0[i])).min_by(|a, b| a.1.total_cmp(&b.1)) else { continue };
        *at = now + if v < -60.0 { 30.0 } else { 90.0 } * rng.random_range(0.8..1.3);
        if v > -30.0 {
            continue;
        }
        let icon = ["balloon_needfood", "balloon_needbladder", "balloon_needenergy", "balloon_needsocial", "balloon_needhygiene", "balloon_needfun"][i];
        commands.entity(e).insert(crate::balloons::BalloonRequest::thought(icon));
    }
}

/// PET_NEEDS=hungry|bladder|tired (tests): the household's pets that need, a few seconds in;
/// PET_BOWL=1 puts a filled pet bowl and a litter box by the first of them (`stall`: a box stall).
#[allow(clippy::type_complexity)]
fn needs_test(
    mut commands: Commands,
    time: Res<Time>,
    mut pets: Query<(&Pet, &mut PetMotives, &Transform), With<HomePet>>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut done: Local<bool>,
) {
    let want = std::env::var("PET_NEEDS").ok();
    let bowl = std::env::var_os("PET_BOWL").is_some();
    if *done || (want.is_none() && !bowl) || time.elapsed_secs() < 10.0 || pets.is_empty() {
        return;
    }
    *done = true;
    let mut first = None;
    for (pet, mut m, tf) in &mut pets {
        first.get_or_insert(tf.translation);
        match want.as_deref() {
            Some("hungry") => m.0[HUNGER] = -50.0,
            Some("bladder") => m.0[BLADDER] = -50.0,
            Some("tired") => m.0[ENERGY] = -70.0,
            _ => {}
        }
        info!("pet needs test: {} {:?}", pet.name, m.0);
    }
    // (PET_BOWL=stall: a box stall instead, a few metres off.)
    let stall = std::env::var("PET_BOWL").is_ok_and(|v| v == "stall");
    if bowl && let Some(at) = first {
        let things: &[&str] = if stall { &["boxStallAllEnglish"] } else { &["petBowlCheap", "litterBoxCheap"] };
        for (i, name) in things.iter().copied().enumerate() {
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(name)).map(|c| c.objd) else {
                warn!("pet needs test: no {name} in the catalogue");
                continue;
            };
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
            let p = at + if stall { Vec3::new(5.0, 0.0, 4.0) } else { Vec3::new(1.5 + i as f32 * 1.5, 0.0, 1.0) };
            if let Some(s) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0) {
                if i == 0 && !stall {
                    commands.entity(s.entity).insert(BowlFood(FULL_BOWL));
                }
                info!("pet needs test: {name} put down at {p}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_games_pet_tuning() {
        // (A cat's hunger slower than a dog's; slowing as it runs low.)
        assert_eq!(decay("ac", HUNGER, 50.0), 7.5);
        assert_eq!(decay("ad", HUNGER, 50.0), 11.25);
        assert_eq!(decay("ad", HUNGER, -50.0), 8.0);
        assert_eq!(decay("ah", HUNGER, -90.0), 0.791_666_7);
        // (Only horses thirst; kittens have no hygiene to lose.)
        assert_eq!(decay("ac", THIRST, 50.0), 0.0);
        assert!(decay("ah", THIRST, 50.0) > 11.0);
        assert_eq!(decay("cc", HYGIENE, 50.0), 0.0);
        assert!((decay("ac", URGE, 0.0) - 13.333).abs() < 0.01);
        // (A bowl's fill: §25 a bite.)
        assert_eq!(fill_cost(0), 200);
        assert_eq!(fill_cost(3), 125);
        assert_eq!(fill_cost(8), 0);
    }

    #[test]
    fn what_presses() {
        assert!(urgent(HUNGER, 10.0, false).is_none());
        assert!(urgent(HUNGER, -10.0, false).is_some());
        assert!(urgent(ENERGY, 50.0, true).is_some(), "bedtime");
        assert!(urgent(ENERGY, 50.0, false).is_none());
        assert!(urgent(SOCIAL, -90.0, false).is_none(), "company comes from Sims");
    }
}
