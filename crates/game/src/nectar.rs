//! Nectar making, World Adventures' (`NectarMaker`, the `Nectar` skill): teens and up put fruit
//! from their inventory in the nectar maker's bin ("Add Fruit ›", the game's place-a-grape;
//! "Place Ingredients For ›" a nectar they've made before, greyed without the fruit for it), ten at
//! most (`kMaxFruitInNectar`, the XML's), and once it's full "Squish Fruit": into swimwear and up
//! into the tub, stomping the fruit half an hour (`kSquishGrapesTimeMinutes`; Nectar 450 an hour, Fun
//! 15), stopped part way and picked up where they left off. "Make Nectar ›" (squishing first if
//! it's still to do) presses it at the controls, Basic, or from level four Concentrated (worth a
//! quarter more, a bottle fewer), from seven Mass Produce (worth three quarters, two more) and from
//! nine Extended Nectaration (worth more, half as long again): half an hour at the buttons (Nectar
//! 900 an hour), then the machine runs two hours (`kMachineRunTimeMinutes`) and three bottles come
//! out (`kNumBottlesPerBatch`, a Nectar Master's one more), named in the game's box ("Name Your
//! Nectar", the blend judged as the game does, the suggested name the two fruits most in it). Their
//! worth is the game's: the fruits' nectar values (`Nectar_Value`) times a third, times the maker's
//! skill (three quarters to one and a half), the fruits' quality (`kQualityLevelMultiplier`) and how
//! well the fruits go together (a blend's own multiplier from its fruits' hash, half to one and
//! three quarters, as the game's `CalculateHashMultiplier` draws it), the style's, and Flavorful
//! Feet's (forty batches squished: a tenth more). "Take Nectar" puts the bottles in their
//! inventory, where they're worth more by the day (`kAppreciationPercent` of what they were made
//! worth), their quality rising with it (`kQualityValueThresholds`). The six best nectars they've
//! made are kept, and their journal counts the bottles, batches and fruit.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Skills};
use crate::inventory::{Inventory, ItemKind};
use crate::sim::{Pose, Sim, SimAnim};

pub struct NectarPlugin;

impl Plugin for NectarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (run, machines, named, tasting).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (stock, looks, dome_bottles, appreciate, nectar_test).run_if(in_state(PlayMode::Live)));
    }
}

/// The game's tuning (the XML's, over the script's).
pub const MAX_FRUIT: usize = 10;
const BOTTLES_PER_BATCH: i32 = 3;
const MIN_COMBO: f32 = 0.5;
const MEDIAN_COMBO: f32 = 1.0;
const MAX_COMBO: f32 = 1.75;
const VARIANCE: (f32, f32) = (0.332, 0.334);
const LEVEL0_MULT: f32 = 0.75;
const LEVEL10_MULT: f32 = 1.5;
const QUALITY_MULT: [f32; 11] = [0.2, 0.3, 0.4, 0.6, 1.0, 1.15, 1.3, 1.5, 1.75, 2.05, 2.5];
const SQUISH_MINUTES: f32 = 30.0;
const PUSH_MINUTES: f32 = 30.0;
const MACHINE_MINUTES: f64 = 120.0;
const POOR: f32 = 0.9;
const WELL: f32 = 1.1;
const GREAT: f32 = 1.3;
const AMAZING: f32 = 1.5;
const APPRECIATION: f32 = 0.04;
const QUALITY_THRESHOLDS: [f32; 11] = [2.0, 4.0, 6.0, 10.0, 18.0, 30.0, 46.0, 70.0, 100.0, 160.0, 99999.0];
const FEET_BATCHES: f64 = 40.0;
const FEET_MULT: f32 = 1.1;
const MASTER_BOTTLES: f64 = 200.0;
const MASTER_EXTRA: i32 = 1;
const TOP_NECTARS: usize = 6;
/// Skill points an hour squishing and at the buttons (`SquishGrapes`', `MakeNectar`'s
/// `SkillNectar`), and squishing's fun.
const SQUISH_POINTS: f32 = 450.0;
const SQUISH_FUN: f32 = 15.0;
const MAKE_POINTS: f32 = 900.0;
pub const SKILL: &str = "Nectar";
/// A bottle's name at most (`mNectarNameLengthLimit`).
pub const NAME_LENGTH: usize = 40;

/// The game's words.
pub const ADD_FRUIT: &str = "Add Fruit";
pub const PLACE_FOR: &str = "Place Ingredients For";
pub const SQUISH: &str = "Squish Fruit";
pub const MAKE: &str = "Make Nectar";
pub const TAKE: &str = "Take Nectar";
pub const NOT_FULL: &str = "The bin needs to be full before you can squish";
pub const BOTTLES_IN_IT: &str = "Remove Nectar bottles before making more";
pub const NEED_FRUIT: &str = "Need the ingredients in your inventory";
pub const TASTE: &str = "Taste Nectar";

/// Tasting: glasses drunk before they drink as a connoisseur does (`kNumDrinksForMedSkill`,
/// `...HighSkill`, the XML's), the moods by quality (`kQualityBuffMoodLevels`), a Snob's
/// threshold and bonus, and how much flame fruit or lifefruit a bottle needs for their effects.
const MED_DRINKS: f64 = 10.0;
const HIGH_DRINKS: f64 = 30.0;
const QUALITY_MOOD: [i32; 11] = [-20, -15, -10, -5, 0, 5, 8, 12, 16, 20, 25];
const SNOB_THRESHOLD: i32 = 13;
const SNOB_BONUS: i32 = 5;
const FLAME_SHARE: f32 = 0.25;
const LIFE_SHARE: f32 = 0.5;
/// The drinking clips (a glass in hand), as they've come to drink it: the novice's sniff, the
/// swirl, the snooty connoisseur's; a disgusting one's reaction after.
const SNIFF: &str = "a2o_drinkNectar_sniff_x";
const SWIRL: &str = "a2o_drinkNectar_swirl_x";
const SNOOTY: &str = "a2o_drinkNectar_snooty_x";
const YUCK: &str = "a_react_badNectar_x";
/// The tasting notes (`NectarGlass`'s `adj_`, `hint_`, and its sentences).
const ADJECTIVES: [&str; 15] = ["buttery", "oaky", "sugary", "crisp", "earthy", "scrumptious", "plain", "simple", "spicy", "smokey", "soft", "supple", "ambiguous", "toasty", "bitter"];
const HINTS: [&str; 7] = ["blackberry", "chocolate", "raspberry", "honey", "blueberry", "apple", "coffee"];
const LOW: [&str; 6] = [
    "All nectar tastes the same to me.",
    "Sweet, just like other nectar.",
    "Yum, nectar!",
    "Drinking nectar makes me happy.",
    "There are 3 types of nectar, brown, purple, and green.",
    "Boy, this sure does go well with food!",
];
const HIGH_GOOD: [&str; 6] = [
    "Wow, this Nectar has a {0} taste with strong hints of {1}!",
    "Mmmm mmm, the {0} taste mixes very well with the hint of {1}!",
    "This is great!  The nose is {0} with wonderful {1} undertones!",
    "Delicious!  This is {0} with a strong hint of {1}.",
    "The {0} flavor with hints of {1} would really go well with dinner.",
    "I can't get enough of this!  The {0} tones match really well with the {1}.",
];
const HIGH_BAD: [&str; 6] = [
    "I can't believe they called this Nectar. Eww!",
    "All I can taste is feet!",
    "Disgusting!",
    "This Nectar is swill!",
    "Bleh! The {1} notes overpower the {0} flavor and ruin it.",
    "Yuck!  The {0} flavor is overpowering, washing out the {1} hints.",
];

/// Where the maker's used from (its routing slots, model space; all facing it, -Z): the
/// controls (`_route_1_SW_controls`), the tub (`_route_2_SE_tub`), the bottles
/// (`_route_3_S_collectNectar`).
const CONTROLS: Vec3 = Vec3::new(-0.463701, 0.0, 0.519325);
const TUB: Vec3 = Vec3::new(1.00166, 0.0, 1.06446);
const BOTTLES: Vec3 = Vec3::new(0.0, 0.0, 0.541);

/// The clips.
const PLACE: &str = "a_nectarMaker_placeGrape_x";
const GET_IN: &str = "a2o_nectarMaker_squishFruit_getin_x";
const STOMP: &[&str] = &["a2o_nectarMaker_squishFruit_loop1_x", "a2o_nectarMaker_squishFruit_loop2_x", "a2o_nectarMaker_squishFruit_loop3_x"];
const GET_OUT: &str = "a2o_nectarMaker_squishFruit_getout_x";
const PUSH_START: &str = "a2o_nectarMaker_makeNectar_start_x";
const PUSH: &[&str] = &["a2o_nectarMaker_makeNectar_loop_x"];
const PUSH_STOP: &str = "a2o_nectarMaker_makeNectar_stop_x";
const TAKE_CLIP: &str = "a2o_nectarMaker_takeNectar_x";
/// The machine's own: pressing, and the bottles waiting.
const PROCESS: &str = "o_nectarMaker_makeNectar_process_nectarMaker";
const READY: &str = "o_nectarMaker_ready_pose_nectarMaker";
const PROCESS_SOUND: &str = "nectar_maker_proc_lp";
/// The bin's look with no fruit in it.
const NO_FRUIT: &str = "fruitNone";

/// How nectar's made (`MakeNectarStyle`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Style {
    Basic,
    Concentrated,
    MassProduce,
    Extended,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Basic, Style::Concentrated, Style::MassProduce, Style::Extended];

    /// The game's name for it (`NectarMaker:Basic`...).
    pub fn name(self) -> &'static str {
        ["Basic", "Concentrated", "Mass Produce", "Extended Nectaration"][self as usize]
    }

    /// The Nectar level it comes at (`kConcentratedUnlockLevel`...).
    pub fn level(self) -> u32 {
        [0, 4, 7, 9][self as usize]
    }

    /// What it does to the worth, the bottles and the machine's time.
    fn worth(self) -> f32 {
        [1.0, 1.25, 0.75, 1.15][self as usize]
    }

    fn bottles(self) -> i32 {
        [0, -1, 2, 0][self as usize]
    }

    fn time(self) -> f64 {
        if self == Style::Extended { 1.5 } else { 1.0 }
    }
}

/// What a Sim does at the maker.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Fruit from their inventory put in the bin: each by its produce key and quality tier.
    Add(Vec<(String, u8)>),
    /// The fruit squished (then made into nectar, if that's what was asked).
    Squish { then: Option<Style> },
    /// The buttons pushed, the machine set running.
    Make(Style),
    /// The bottles taken out.
    Take,
}

/// A fruit in the bin: its produce key and quality tier (`gardening::QUALITIES`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fruit {
    pub key: String,
    pub tier: u8,
}

/// A batch being pressed: when it's done, how, the maker's skill level and Sim (by id), and
/// whether they're a Nectar Master.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub until: f64,
    pub style: Style,
    pub level: f32,
    pub by: u64,
    pub master: bool,
    pub feet: f32,
}

/// A batch's bottles: their name, the blend (its fruits' hash and each fruit's share), the week
/// and day made, and what each was worth then (its base for appreciation).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bottle {
    pub name: String,
    pub hash: u32,
    pub week: u32,
    pub day: u32,
    pub base: f32,
    pub mix: Vec<(String, f32)>,
    /// Made by a master of it (level ten: `MadeByLevel10Sim`), its flame fruit and lifefruit
    /// working their magic.
    #[serde(default)]
    pub ten: bool,
}

impl Bottle {
    /// What it's worth on a day (more by its appreciation each day since it was made).
    pub fn worth(&self, today: u32) -> i64 {
        (self.base + (self.base * APPRECIATION * today.saturating_sub(self.day) as f32 + 1e-3).floor()) as i64
    }

    /// Its stack's key in an inventory (`ItemKind::Nectar`), from which it's made again.
    pub fn key(&self) -> String {
        let mix: Vec<String> = self.mix.iter().map(|(k, f)| format!("{k}={f:.3}")).collect();
        format!("{:08x}|{}|{}|{:.4}|{}|{}", self.hash, self.week, self.day, self.base, mix.join(";"), u8::from(self.ten))
    }

    /// A bottle from its stack's key and name.
    pub fn from_key(key: &str, name: &str) -> Option<Bottle> {
        let mut p = key.split('|');
        let hash = u32::from_str_radix(p.next()?, 16).ok()?;
        let week = p.next()?.parse().ok()?;
        let day = p.next()?.parse().ok()?;
        let base = p.next()?.parse().ok()?;
        let mix = p.next().unwrap_or("").split(';').filter_map(|m| m.split_once('=').and_then(|(k, f)| Some((k.to_string(), f.parse().ok()?)))).collect();
        let ten = p.next() == Some("1");
        Some(Bottle { name: name.trim_start_matches(|c: char| c != '-').trim_start_matches("- ").to_string(), hash, week, day, base, mix, ten })
    }

    /// How much of it is a fruit (its share of the blend).
    fn share(&self, key: &str) -> f32 {
        self.mix.iter().find(|m| m.0 == key).map_or(0.0, |m| m.1)
    }

    /// Its name with its week, as the game shows a bottle ("Week: 3 - Apple Nectar").
    pub fn shown(&self) -> String {
        format!("Week: {} - {}", self.week, self.name)
    }
}

/// A nectar maker: the fruit in its bin, whether it's squished (and how much squishing's still to
/// do, a fraction of a whole), the batch being pressed, and the bottles waiting.
#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Maker {
    pub fruit: Vec<Fruit>,
    pub squished: bool,
    pub squish_left: f32,
    pub batch: Option<Batch>,
    pub bottles: Vec<Bottle>,
}

impl Maker {
    pub fn full(&self) -> bool {
        self.fruit.len() >= MAX_FRUIT
    }

    fn room(&self) -> usize {
        MAX_FRUIT.saturating_sub(self.fruit.len())
    }
}

/// The nectars a Sim's made, the best few (`mTopNectarsMade`): name, blend hash, the fruit in it
/// (key and how many) and the best a bottle was worth.
#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Recipes(pub Vec<Recipe>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    pub hash: u32,
    pub fruit: Vec<(String, u32)>,
    pub worth: f32,
}

impl Recipes {
    /// A nectar made (`NectarTypeMade`): kept among the best (its best worth, its latest name).
    pub fn made(&mut self, r: Recipe) {
        match self.0.iter_mut().find(|x| x.hash == r.hash) {
            Some(x) => {
                x.worth = x.worth.max(r.worth);
                x.name = r.name;
            }
            None => self.0.push(r),
        }
        self.0.sort_by(|a, b| b.worth.total_cmp(&a.worth));
        self.0.truncate(TOP_NECTARS);
    }
}

/// .NET's `System.Random` (the subtractive generator the game's scripts seed with a blend's hash),
/// to draw what the game draws from it.
pub struct NetRandom {
    seeds: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl NetRandom {
    pub fn new(seed: i32) -> Self {
        const MBIG: i32 = i32::MAX;
        const MSEED: i32 = 161_803_398;
        let mut sa = [0i32; 56];
        let subtraction = if seed == i32::MIN { i32::MAX } else { seed.abs() };
        let mut mj = MSEED.wrapping_sub(subtraction);
        sa[55] = mj;
        let mut mk: i32 = 1;
        for i in 1..55 {
            let ii = (21 * i) % 55;
            sa[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(MBIG);
            }
            mj = sa[ii];
        }
        for _ in 1..5 {
            for i in 1..56 {
                sa[i] = sa[i].wrapping_sub(sa[1 + (i + 30) % 55]);
                if sa[i] < 0 {
                    sa[i] = sa[i].wrapping_add(MBIG);
                }
            }
        }
        Self { seeds: sa, inext: 0, inextp: 21 }
    }

    fn sample(&mut self) -> i32 {
        let a = if self.inext + 1 >= 56 { 1 } else { self.inext + 1 };
        let b = if self.inextp + 1 >= 56 { 1 } else { self.inextp + 1 };
        let mut r = self.seeds[a].wrapping_sub(self.seeds[b]);
        if r == i32::MAX {
            r -= 1;
        }
        if r < 0 {
            r = r.wrapping_add(i32::MAX);
        }
        self.seeds[a] = r;
        self.inext = a;
        self.inextp = b;
        r
    }

    pub fn next_double(&mut self) -> f64 {
        self.sample() as f64 * (1.0 / i32::MAX as f64)
    }
}

/// How well a blend's fruits go together (`CalculateHashMultiplier`): three draws seeded by its
/// hash, averaged, below the middle between the least and the median, above it up to the most.
pub fn combo(hash: u32) -> f32 {
    let mut r = NetRandom::new(hash as i32);
    let n = ((r.next_double() + r.next_double() + r.next_double()) / 3.0) as f32;
    if n < 0.5 { MIN_COMBO + (MEDIAN_COMBO - MIN_COMBO) * n * 2.0 } else { MEDIAN_COMBO + (MAX_COMBO - MEDIAN_COMBO) * (n - 0.5) * 2.0 }
}

/// A blend's grade, as the menu and the naming box have it (none for a single fruit).
pub fn grade(combo: f32) -> &'static str {
    if combo < POOR {
        "Terrible"
    } else if combo < WELL {
        "Poor"
    } else if combo < GREAT {
        "Good"
    } else if combo < AMAZING {
        "Great"
    } else {
        "Amazing"
    }
}

/// The naming box's words for a blend (`NameBottleDialog...`).
fn naming_words(single: bool, combo: f32) -> &'static str {
    if single {
        "What would you like to name this batch of Nectar?"
    } else if combo < POOR {
        "This blend of Nectar is absolutely terrible!  What would you like to call it?"
    } else if combo < WELL {
        "This is not a very good blend of Nectar. What would you like to call it?"
    } else if combo < GREAT {
        "This Nectar blend is pretty good. What do you want to name it?"
    } else if combo < AMAZING {
        "This Nectar blend is great!  What do you want to name it?"
    } else {
        "This bottle of Nectar is absolutely amazing! What do you want to name it?"
    }
}

/// A bottle's quality by its worth (`NectarBottle.GetQuality`: the first threshold it's under,
/// 1 to 11).
pub fn quality(worth: f32) -> u8 {
    let v = worth.clamp(QUALITY_THRESHOLDS[0], QUALITY_THRESHOLDS[10]);
    QUALITY_THRESHOLDS.iter().position(|t| v <= *t).unwrap_or(10) as u8 + 1
}

/// A blend's hash (`ResourceUtils.HashString32` of its fruits' keys, in order).
pub fn blend_hash(fruit: &[Fruit]) -> u32 {
    let mut keys: Vec<&str> = fruit.iter().map(|f| f.key.as_str()).collect();
    keys.sort();
    s3pkg::fnv32(&keys.concat())
}

/// What a bottle of a batch is worth (`NectarFinishTask`): the fruits' nectar values (`nectar`,
/// by key), the variance drawn, the maker's level, how well they go together, the fruits'
/// average quality, the feet's and the style's.
pub fn batch_worth(fruit: &[Fruit], nectar: impl Fn(&str) -> f32, variance: f32, level: f32, feet: f32, style: Style) -> f32 {
    if fruit.is_empty() {
        return 0.0;
    }
    let sum: f32 = fruit.iter().map(|f| nectar(&f.key)).sum();
    let single = fruit.iter().all(|f| f.key == fruit[0].key);
    let mix = if single { 1.0 } else { combo(blend_hash(fruit)) };
    let level_mult = (LEVEL10_MULT - LEVEL0_MULT) * (level.floor().clamp(0.0, 10.0) / 10.0) + LEVEL0_MULT;
    let avg = fruit.iter().map(|f| crate::gardening::GAME_QUALITY[(f.tier as usize).min(9)] as f32).sum::<f32>() / fruit.len() as f32;
    let q = QUALITY_MULT[((avg as usize).max(1) - 1).min(10)];
    sum * variance * level_mult * mix * q * feet * style.worth()
}

/// How many bottles a batch makes.
pub fn batch_bottles(style: Style, master: bool) -> i32 {
    (BOTTLES_PER_BATCH + style.bottles() + if master { MASTER_EXTRA } else { 0 }).max(1)
}

/// The blend's shares, most first, and its suggested name ("Apple Lime Nectar", the fruits' own
/// names; a single fruit's alone, or the first's where both won't fit).
pub fn blend(fruit: &[Fruit], name_of: impl Fn(&str) -> String) -> (Vec<(String, f32)>, String) {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for f in fruit {
        match counts.iter_mut().find(|c| c.0 == f.key) {
            Some(c) => c.1 += 1,
            None => counts.push((f.key.clone(), 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let n = fruit.len().max(1) as f32;
    let mix: Vec<(String, f32)> = counts.iter().map(|(k, c)| (k.clone(), *c as f32 / n)).collect();
    let first = counts.first().map(|c| name_of(&c.0)).unwrap_or_default();
    let name = match counts.get(1) {
        Some(c) => {
            let two = format!("{first} {} Nectar", name_of(&c.0));
            if two.chars().count() > NAME_LENGTH { format!("{first} Nectar") } else { two }
        }
        None => format!("{first} Nectar"),
    };
    (mix, name)
}

/// The produce that's fruit for nectar, with its nectar value.
fn nectar_value(ui: &crate::icons::GameUi, key: &str) -> f32 {
    ui.data.ingredients.iter().find(|i| i.key == key).map_or(0.0, |i| i.nectar as f32)
}

fn fruit_name(ui: &crate::icons::GameUi, key: &str) -> String {
    ui.data.ingredients.iter().find(|i| i.key == key).map_or(key.to_string(), |i| i.name.clone())
}

/// Where a Sim stands at a slot of the maker, and the way they face.
fn spot(mtf: &Transform, slot: Vec3) -> (Vec2, Vec2) {
    let at = mtf.transform_point(slot).xz();
    (at, (mtf.rotation * Vec3::NEG_Z).xz().normalize_or(Vec2::NEG_Y))
}

/// A step at the maker, from the slot it's done at.
pub fn step(maker: Entity, mtf: &Transform, step: Step) -> ActionKind {
    let slot = match step {
        Step::Add(_) | Step::Squish { .. } => TUB,
        Step::Make(_) => CONTROLS,
        Step::Take => BOTTLES,
    };
    let (at, face) = spot(mtf, slot);
    ActionKind::Nectar { maker, step, at, face }
}

/// A nectar maker's menu for a Sim (teens and up), as the game's: "Add Fruit ›" their fruit,
/// "Place Ingredients For ›" a nectar they've made, "Squish Fruit" once it's full, "Make Nectar ›"
/// as their skill allows, "Take Nectar" with bottles in it.
#[allow(clippy::too_many_arguments)]
pub fn options(
    maker: Entity,
    mtf: &Transform,
    m: &Maker,
    sim: &Sim,
    level: f32,
    inv: Option<&Inventory>,
    recipes: Option<&Recipes>,
    ui: &crate::icons::GameUi,
    pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>,
    submenu: impl Fn(usize) -> ActionKind,
) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    if sim.age.is_little() || sim.age == crate::sim::Age::Child {
        return out;
    }
    let pressing = m.batch.is_some();
    let fruit: Vec<&crate::inventory::Stack> = inv.map(|i| i.0.iter().filter(|s| s.kind == ItemKind::Produce && nectar_value(ui, &s.key) > 0.0).collect()).unwrap_or_default();
    // (Fruit in, while there's room and it's not squished or pressing.)
    if !pressing && !m.squished && !m.full() && !fruit.is_empty() {
        let list: Vec<(String, ActionKind)> = fruit
            .iter()
            .map(|s| {
                let n = (s.count as usize).min(m.room());
                (format!("{} ({})", s.name, s.count), step(maker, mtf, Step::Add(vec![(s.key.clone(), s.quality); n])))
            })
            .collect();
        out.push((format!("{ADD_FRUIT} ›"), submenu(pie_submenus.len())));
        pie_submenus.push((ADD_FRUIT.to_string(), list));
    }
    // (A nectar made before, its fruit put in at once.)
    if !pressing && !m.squished && m.fruit.is_empty()
        && let Some(r) = recipes.filter(|r| !r.0.is_empty())
    {
        let list: Vec<(String, ActionKind)> = r
            .0
            .iter()
            .map(|r| {
                let label = if r.fruit.len() > 1 { format!("{} ({})", r.name, grade(combo(r.hash))) } else { r.name.clone() };
                match pick_for(inv, &r.fruit) {
                    Some(picked) => (label, step(maker, mtf, Step::Add(picked))),
                    None => (label, ActionKind::Greyed(NEED_FRUIT.into())),
                }
            })
            .collect();
        out.push((format!("{PLACE_FOR}... ›"), submenu(pie_submenus.len())));
        pie_submenus.push((format!("{PLACE_FOR}..."), list));
    }
    // (Squishing: once it's full.)
    if !pressing && !m.squished && !m.fruit.is_empty() {
        out.push((SQUISH.to_string(), if m.full() { step(maker, mtf, Step::Squish { then: None }) } else { ActionKind::Greyed(NOT_FULL.into()) }));
    }
    // (Making it: squished, or full to squish first; not with bottles still in it.)
    if !pressing && (m.squished || m.full()) {
        if !m.bottles.is_empty() {
            out.push((format!("{MAKE}..."), ActionKind::Greyed(BOTTLES_IN_IT.into())));
        } else {
            let list: Vec<(String, ActionKind)> = Style::ALL
                .into_iter()
                .filter(|s| level.floor() as u32 >= s.level())
                .map(|s| (s.name().to_string(), step(maker, mtf, if m.squished { Step::Make(s) } else { Step::Squish { then: Some(s) } })))
                .collect();
            out.push((format!("{MAKE}... ›"), submenu(pie_submenus.len())));
            pie_submenus.push((format!("{MAKE}..."), list));
        }
    }
    if !m.bottles.is_empty() {
        out.push((TAKE.to_string(), step(maker, mtf, Step::Take)));
    }
    out
}

/// The fruit for a nectar from an inventory, the best of each first; none if there isn't enough.
fn pick_for(inv: Option<&Inventory>, want: &[(String, u32)]) -> Option<Vec<(String, u8)>> {
    let inv = inv?;
    let mut out = Vec::new();
    for (key, n) in want {
        let mut stacks: Vec<&crate::inventory::Stack> = inv.0.iter().filter(|s| s.kind == ItemKind::Produce && s.key == *key).collect();
        stacks.sort_by(|a, b| b.quality.cmp(&a.quality));
        let mut left = *n;
        for s in stacks {
            let take = left.min(s.count);
            out.extend((0..take).map(|_| (key.clone(), s.quality)));
            left -= take;
        }
        if left > 0 {
            return None;
        }
    }
    Some(out)
}

/// New makers given their bins.
fn stock(mut commands: Commands, makers: Query<(Entity, &GameObject), (Without<Maker>, Without<crate::visit::LotObject>)>) {
    for (e, o) in &makers {
        if o.kind == ObjectKind::NectarMaker {
            commands.entity(e).insert(Maker { squish_left: 1.0, ..default() });
        }
    }
}

/// The bin's look (`UpdateVisualState`): no fruit, fruit, or squished; drawn afresh as it changes.
#[derive(Component, PartialEq, Clone, Copy)]
struct Shown(u8);

/// A bottle waiting under the maker's dome (its `_bind_8_bottleDome`, model space), while there
/// are bottles to take.
#[derive(Component)]
struct DomeBottle(Entity);

const DOME: Vec3 = Vec3::new(0.241, 1.0, -0.217);

#[allow(clippy::type_complexity)]
fn dome_bottles(
    mut commands: Commands,
    makers: Query<(Entity, &Maker, Option<&DomeBottle>), Changed<Maker>>,
    data: Option<Res<crate::baked::Baked>>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(data) = data else { return };
    for (e, m, shown) in &makers {
        match (m.bottles.is_empty(), shown) {
            (true, Some(d)) => {
                commands.entity(d.0).try_despawn();
                commands.entity(e).remove::<DomeBottle>();
            }
            (false, None) => {
                let Some(objd) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("NectarBottle")).map(|c| c.objd) else { continue };
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let parts = assets.object(&mut ctx, objd);
                let b = crate::objects::spawn_parts(&mut commands, &parts, Transform::from_translation(DOME));
                commands.entity(e).add_child(b).insert(DomeBottle(b));
            }
            _ => {}
        }
    }
}

#[allow(clippy::type_complexity)]
fn looks(
    mut commands: Commands,
    makers: Query<(Entity, &GameObject, &Maker, Option<&Shown>, Option<&crate::objects::Design>), Changed<Maker>>,
    data: Option<Res<crate::baked::Baked>>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(data) = data else { return };
    for (e, o, m, shown, design) in &makers {
        let want = if m.squished && m.batch.is_none() {
            2
        } else if m.fruit.is_empty() || m.batch.is_some() {
            0
        } else {
            1
        };
        if shown.is_some_and(|s| s.0 == want) {
            continue;
        }
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let state = (want == 0).then(|| s3pkg::fnv32(NO_FRUIT));
        let mut parts = assets.object_state_design(&mut ctx, o.objd, state, design.map(|d| d.0));
        if want == 2 {
            for p in &mut parts {
                if let Some(s) = p.states.iter().find(|s| s.0 == s3formats::model::STATE_FRUIT_SQUISHED) {
                    p.material = s.1.clone();
                }
            }
        }
        crate::objects::reshape_parts(&mut commands, e, &parts);
        commands.entity(e).insert(Shown(want));
    }
}

/// At the maker: what they're at (the clip's moment, how long), and how long they've squished
/// or pushed; whether the outfit's been seen to.
#[derive(Component)]
pub struct AtMaker {
    stage: u8,
    t: f32,
    len: f32,
    minutes: f32,
    spun: bool,
}

/// Sims at nectar makers, step by step.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    mut sims: Query<
        (
            Entity,
            &Sim,
            &mut ActionQueue,
            &mut Transform,
            &mut SimAnim,
            (&mut Skills, &mut crate::sim::Motives, Option<&mut Inventory>, Option<&crate::journal::SkillJournal>),
            Option<&mut AtMaker>,
            (Option<&crate::simbody::Wearing>, Has<crate::dressing::ClothesSpin>),
        ),
        Without<GameObject>,
    >,
    mut makers: Query<(&mut Maker, &Transform), Without<Sim>>,
    (mut notes, mut did, mut life): (ResMut<crate::interact::Notifications>, MessageWriter<crate::journal::Did>, MessageWriter<crate::life::LifeEvent>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let minutes = delta.0;
    let now = clock.minutes;
    let len = |lib: &mut crate::anim::ClipLibrary, c: &str| lib.get(&data, c).map_or(2.0, |x| x.duration);
    for (e, sim, mut q, mut tf, mut anim, (mut skills, mut motives, inv, journal), at, (wearing, spinning)) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(|a| match &a.kind {
            ActionKind::Nectar { maker, step, at, face } => Some((*maker, step.clone(), *at, *face, a.cancel)),
            _ => None,
        });
        let Some((maker, step, spot, face, cancel)) = front else {
            if at.is_some() {
                commands.entity(e).remove::<(AtMaker, ActionClip)>();
            }
            continue;
        };
        let Ok((mut m, mtf)) = makers.get_mut(maker) else {
            q.0.pop_front();
            continue;
        };
        let mtf = *mtf;
        let Some(mut a) = at else {
            tf.translation.x = spot.x;
            tf.translation.z = spot.y;
            tf.rotation = Quat::from_rotation_y(face.x.atan2(face.y));
            anim.pose = Pose::Use;
            commands.entity(e).insert(AtMaker { stage: 0, t: 0.0, len: 0.0, minutes: 0.0, spun: false });
            continue;
        };
        a.t += dt;
        let done = a.t >= a.len;
        let mut finish = false;
        match &step {
            // Fruit in: the place-a-grape, then the fruit's in the bin.
            Step::Add(fruit) => match a.stage {
                0 => {
                    a.stage = 1;
                    a.t = 0.0;
                    a.len = len(&mut lib, PLACE);
                    commands.entity(e).insert(ActionClip::new(Some(PLACE), &[]));
                }
                _ if done => {
                    if let Some(mut inv) = inv {
                        for (key, tier) in fruit.iter().take(m.room()) {
                            if inv.consume_produce(key, *tier) {
                                m.fruit.push(Fruit { key: key.clone(), tier: *tier });
                            }
                        }
                    }
                    info!("nectar: {} puts fruit in the maker ({} in it)", sim.first, m.fruit.len());
                    finish = true;
                }
                _ => {}
            },
            // Squishing: into swimwear, up into the tub, stomping as long as it takes, and out.
            Step::Squish { then } => match a.stage {
                0 => {
                    if spinning {
                        continue;
                    }
                    let dressed = wearing.is_some_and(|w| w.0 == crate::simbody::OutfitKind::Swimwear);
                    if !dressed && !a.spun && crate::dressing::spins(sim) {
                        a.spun = true;
                        crate::dressing::start(&mut commands, e, Some(crate::simbody::OutfitKind::Swimwear), false);
                        continue;
                    }
                    a.stage = 1;
                    a.t = 0.0;
                    a.len = len(&mut lib, GET_IN);
                    commands.entity(e).insert(ActionClip::new(Some(GET_IN), STOMP));
                }
                1 => {
                    if a.t < a.len {
                        continue;
                    }
                    // (Stomping, the fruit squished as they go.)
                    let rate = crate::life::skill_rate(&sim.traits, SKILL);
                    if let Some(level) = skills.add_points(SKILL, SQUISH_POINTS * rate * minutes / 60.0) {
                        notes.push(format!("{} reached level {level} in Nectar Making!", sim.first));
                        life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: SKILL, level }));
                    }
                    motives.add(crate::sim::FUN, SQUISH_FUN * minutes / 60.0);
                    a.minutes += minutes;
                    let need = SQUISH_MINUTES * m.squish_left.max(0.05);
                    if a.minutes >= need || cancel {
                        if a.minutes >= need {
                            m.squished = true;
                            m.squish_left = 1.0;
                            did.write(crate::journal::Did::count(e, crate::journal::Stat::NectarSquishes, 1.0));
                            info!("nectar: {} has squished the fruit", sim.first);
                        } else {
                            m.squish_left *= 1.0 - a.minutes / need;
                        }
                        a.stage = 2;
                        a.t = 0.0;
                        a.len = len(&mut lib, GET_OUT);
                        commands.entity(e).insert(ActionClip::new(Some(GET_OUT), &[]));
                    }
                }
                _ if done => {
                    finish = true;
                    // (Then made into nectar, if that's what was asked: `PushAsContinuation`.)
                    if let Some(style) = then.filter(|_| m.squished && !cancel) {
                        let at = 1.min(q.0.len());
                        q.0.insert(at, Action::new(style.name(), self::step(maker, &mtf, Step::Make(style)), false));
                    }
                }
                _ => {}
            },
            // At the buttons; then the machine's set running.
            Step::Make(style) => match a.stage {
                0 => {
                    if !m.squished || !m.bottles.is_empty() || m.batch.is_some() {
                        finish = true;
                    } else {
                        a.stage = 1;
                        a.t = 0.0;
                        commands.entity(e).insert(ActionClip::new(Some(PUSH_START), PUSH).ending(&[PUSH_STOP], 1.5));
                    }
                }
                1 => {
                    let rate = crate::life::skill_rate(&sim.traits, SKILL);
                    if let Some(level) = skills.add_points(SKILL, MAKE_POINTS * rate * minutes / 60.0) {
                        notes.push(format!("{} reached level {level} in Nectar Making!", sim.first));
                        life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: SKILL, level }));
                    }
                    a.minutes += minutes;
                    if cancel {
                        finish = true;
                    } else if a.minutes >= PUSH_MINUTES {
                        let level = skills.0.get(SKILL).copied().unwrap_or(0.0);
                        let squishes = journal.map_or(0.0, |j| j.get(crate::journal::Stat::NectarSquishes));
                        let bottles = journal.map_or(0.0, |j| j.get(crate::journal::Stat::NectarBottles));
                        m.batch = Some(Batch {
                            until: now + MACHINE_MINUTES * style.time(),
                            style: *style,
                            level,
                            by: sim.id,
                            master: bottles >= MASTER_BOTTLES,
                            feet: if squishes >= FEET_BATCHES { FEET_MULT } else { 1.0 },
                        });
                        info!("nectar: {} sets the maker pressing ({})", sim.first, style.name());
                        a.stage = 2;
                        a.t = 0.0;
                        a.len = len(&mut lib, PUSH_STOP);
                        commands.entity(e).insert(ActionClip::new(Some(PUSH_STOP), &[]));
                    }
                }
                _ if done => finish = true,
                _ => {}
            },
            // The bottles out and into their inventory.
            Step::Take => match a.stage {
                0 => {
                    a.stage = 1;
                    a.t = 0.0;
                    a.len = len(&mut lib, TAKE_CLIP);
                    commands.entity(e).insert(ActionClip::new(Some(TAKE_CLIP), &[]));
                }
                _ if done => {
                    let today = clock.day();
                    let bottles = std::mem::take(&mut m.bottles);
                    let n = bottles.len();
                    for b in bottles {
                        let worth = b.worth(today);
                        crate::inventory::give(&mut commands, e, ItemKind::Nectar, b.key(), b.shown(), quality(worth as f32), worth, 1);
                    }
                    info!("nectar: {} takes {n} bottles", sim.first);
                    finish = true;
                }
                _ => {}
            },
        }
        if finish {
            q.0.pop_front();
            anim.pose = Pose::Stand;
            commands.entity(e).remove::<(AtMaker, ActionClip)>();
        }
    }
}

/// The makers pressing: the machine's running clip and sound till it's done, then the bottles
/// (`NectarFinishTask`), and the box to name them.
#[allow(clippy::type_complexity)]
fn machines(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    time: Res<Time>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut makers: Query<(Entity, &mut Maker, Option<&crate::objanim::PlayingClip>, Has<crate::sound::AmbientSound>, Has<crate::objanim::HeldPose>)>,
    mut sims: Query<(Entity, &Sim, Option<&mut Recipes>, Has<crate::sim::HouseholdMember>)>,
    mut did: MessageWriter<crate::journal::Did>,
    mut questions: ResMut<crate::dialog::Questions>,
) {
    let Some(ui) = ui else { return };
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, mut m, playing, sounding, held) in &mut makers {
        match m.batch.clone() {
            Some(b) if now < b.until => {
                // (Pressing: its own clip looped, its sound.)
                let t = playing.map_or(0.0, |p| p.time) + time.delta_secs();
                commands.entity(e).insert(crate::objanim::PlayingClip { name: PROCESS, time: t, looping: true });
                if !sounding {
                    commands.entity(e).insert(crate::sound::AmbientSound(PROCESS_SOUND.into()));
                }
            }
            Some(b) => {
                // Done: the bottles, their worth the game's.
                let variance = rng.random_range(VARIANCE.0..VARIANCE.1);
                let worth = batch_worth(&m.fruit, |k| nectar_value(&ui, k), variance, b.level, b.feet, b.style);
                let n = batch_bottles(b.style, b.master);
                let (mix, name) = blend(&m.fruit, |k| fruit_name(&ui, k));
                let hash = blend_hash(&m.fruit);
                let single = mix.len() == 1;
                let maker_sim = sims.iter().find(|s| s.1.id == b.by).map(|s| s.0);
                // (A name they've given this blend before.)
                let name = maker_sim.and_then(|s| sims.get(s).ok()).and_then(|s| s.2.and_then(|r| r.0.iter().find(|x| x.hash == hash).map(|x| x.name.clone()))).unwrap_or(name);
                let day = clock.day();
                let week = day / 7 + 1;
                let ten = b.level >= 10.0;
                m.bottles = (0..n).map(|_| Bottle { name: name.clone(), hash, week, day, base: worth, mix: mix.clone(), ten }).collect();
                let mut fruit_count: Vec<(String, u32)> = Vec::new();
                for f in &m.fruit {
                    match fruit_count.iter_mut().find(|c| c.0 == f.key) {
                        Some(c) => c.1 += 1,
                        None => fruit_count.push((f.key.clone(), 1)),
                    }
                }
                let used = m.fruit.len();
                m.fruit.clear();
                m.squished = false;
                m.batch = None;
                if let Some(s) = maker_sim {
                    did.write(crate::journal::Did::count(s, crate::journal::Stat::NectarBottles, n as f64));
                    did.write(crate::journal::Did::count(s, crate::journal::Stat::NectarFruit, used as f64));
                    let recipe = Recipe { name: name.clone(), hash, fruit: fruit_count, worth };
                    if let Ok((_, _, recipes, member)) = sims.get_mut(s) {
                        match recipes {
                            Some(mut r) => r.made(recipe),
                            None => {
                                let mut r = Recipes::default();
                                r.made(recipe);
                                commands.entity(s).insert(r);
                            }
                        }
                        // (The game's box to name them: the household's own.)
                        if member {
                            questions.ask(crate::dialog::Ask {
                                about: crate::dialog::Question::NameNectar { maker: e, suggested: name.clone() },
                                icon: String::new(),
                                heading: String::new(),
                                title: "Name Your Nectar".into(),
                                text: naming_words(single, if single { 1.0 } else { combo(hash) }).into(),
                                answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
                            });
                        }
                    }
                }
                info!("nectar: a batch done: {n} bottles of {name}, §{worth:.1} each");
                commands.entity(e).remove::<(crate::objanim::PlayingClip, crate::sound::AmbientSound)>().insert(crate::objanim::HeldPose(READY));
            }
            None => {
                if playing.is_some_and(|p| p.name == PROCESS) {
                    commands.entity(e).remove::<crate::objanim::PlayingClip>();
                }
                if sounding {
                    commands.entity(e).remove::<crate::sound::AmbientSound>();
                }
                if held && m.bottles.is_empty() {
                    commands.entity(e).remove::<crate::objanim::HeldPose>();
                }
            }
        }
    }
}

/// A batch named: its bottles still in the maker, and the recipe, called that.
fn named(mut answers: MessageReader<crate::dialog::Answered>, typed: Res<crate::dialog::Typed>, mut makers: Query<&mut Maker>, mut recipes: Query<&mut Recipes>) {
    for a in answers.read() {
        let crate::dialog::Question::NameNectar { maker, .. } = &a.about else { continue };
        let t: String = typed.0.trim().chars().take(NAME_LENGTH).collect();
        if a.answer != 0 || t.is_empty() {
            continue;
        }
        let Ok(mut m) = makers.get_mut(*maker) else { continue };
        let Some(hash) = m.bottles.first().map(|b| b.hash) else { continue };
        for b in &mut m.bottles {
            b.name = t.clone();
        }
        for mut r in &mut recipes {
            if let Some(x) = r.0.iter_mut().find(|x| x.hash == hash) {
                x.name = t.clone();
            }
        }
        info!("nectar: named {t}");
    }
}

/// A glass of nectar being tasted: the bottle it's from, its quality, whether it's begun, and
/// whether it was their own idea.
#[derive(Component)]
pub struct Tasting {
    bottle: Bottle,
    quality: u8,
    begun: bool,
}

/// A bottle from the inventory opened and a glass of it drunk ("Taste Nectar", `ServeNectar`'s
/// `TasteDefinition`): the bottle's gone at once.
pub fn taste(commands: &mut Commands, sim: Entity, queue: &mut ActionQueue, stack: crate::inventory::Stack, who: &Sim) {
    let Some(bottle) = Bottle::from_key(&stack.key, &stack.name) else { return };
    commands.entity(sim).insert(Tasting { bottle, quality: stack.quality.max(1), begun: false });
    queue.push_player(Action::new(TASTE, ActionKind::Outro { clips: &[SNIFF], then: None, secs: 8.0, stand_at: None, target: sim }, false));
    info!("nectar: {} tastes {}", who.first, stack.name);
}

/// How they drink it (`SetParameters`): a Snob as a connoisseur, the Insane any way at all, the
/// rest as they've learned (glasses drunk).
pub fn drink_clip(snob: bool, insane: bool, glasses: f64, roll: u32) -> &'static str {
    if snob {
        SNOOTY
    } else if insane {
        [SNIFF, SWIRL, SNOOTY][(roll % 3) as usize]
    } else if glasses < MED_DRINKS {
        SNIFF
    } else if glasses < HIGH_DRINKS {
        SWIRL
    } else {
        SNOOTY
    }
}

/// What a glass does to them (`DoPostDrink`): its mood (by its quality; a Snob's better for a
/// fine one, and only a fine one pleases them; the Insane's anything), and what they make of it.
/// `rolls`: the bottle's own draws (the buff, adjective, hint), a base sentence, and the Insane's.
pub fn verdict(quality: u8, snob: bool, insane: bool, glasses: f64, rolls: (usize, usize, usize, usize), crazy: (usize, u32)) -> (i32, String) {
    let q = (quality.clamp(1, 11) - 1) as usize;
    let (_, adj, hint, base) = rolls;
    let fill = |s: &str| s.replace("{0}", ADJECTIVES[adj % ADJECTIVES.len()]).replace("{1}", HINTS[hint % HINTS.len()]);
    let low = LOW[base % LOW.len()].to_string();
    let good = fill(HIGH_GOOD[base % HIGH_GOOD.len()]);
    let bad = fill(HIGH_BAD[base % HIGH_BAD.len()]);
    if snob {
        let m = QUALITY_MOOD[q];
        if m > SNOB_THRESHOLD { (m + SNOB_BONUS, good) } else { (m.min(0), bad) }
    } else if insane {
        let m = QUALITY_MOOD[crazy.0 % QUALITY_MOOD.len()];
        (m, [low, bad, good][(crazy.1 % 3) as usize].clone())
    } else {
        let m = QUALITY_MOOD[q];
        let said = if m < 0 { if glasses < HIGH_DRINKS { low } else { bad } } else if glasses < MED_DRINKS { low } else { good };
        (m, said)
    }
}

/// Glasses being tasted: the drink as they've come to drink it (and a disgusted face after a
/// foul one, for those who know better), then the moodlets and what they make of it.
#[allow(clippy::type_complexity)]
fn tasting(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Tasting, Option<&mut crate::life::Moodlets>, Option<&crate::journal::SkillJournal>, Has<crate::sim::Selected>)>,
    (mut notes, mut did): (ResMut<crate::interact::Notifications>, MessageWriter<crate::journal::Did>),
) {
    let mut rng = rand::rng();
    for (e, sim, mut q, mut t, moodlets, journal, selected) in &mut sims {
        let now_tasting = q.0.front().filter(|a| a.label == TASTE && matches!(a.kind, ActionKind::Outro { target, .. } if target == e));
        let glasses = journal.map_or(0.0, |j| j.get(crate::journal::Stat::NectarGlasses));
        let snob = sim.traits.contains(&crate::life::Trait::Snob);
        let insane = sim.traits.contains(&crate::life::Trait::Insane);
        match now_tasting {
            Some(a) if !t.begun => {
                if !matches!(a.phase, Phase::Running(_)) {
                    continue;
                }
                t.begun = true;
                let clip = drink_clip(snob, insane, glasses, rng.random());
                // (A connoisseur knows a foul one: `NectarDisgusting`.)
                let yuck = clip == SNOOTY && t.quality < 5;
                let secs = lib.get(&data, clip).map_or(5.0, |c| c.duration) + if yuck { lib.get(&data, YUCK).map_or(3.0, |c| c.duration) } else { 0.0 };
                commands.entity(e).insert(if yuck { ActionClip::steps(clip, &[YUCK], &[]) } else { ActionClip::new(Some(clip), &[]) });
                if let Some(a) = q.0.front_mut()
                    && let ActionKind::Outro { secs: s, .. } = &mut a.kind
                {
                    *s = secs;
                }
            }
            Some(_) => {}
            None if t.begun => {
                // Drunk: the sugar, the fruits' magic, how good it was.
                let now = clock.minutes;
                let b = t.bottle.clone();
                let mut r = NetRandom::new((b.hash as i32).wrapping_add(sim.id as i32));
                let mut draw = |n: usize| (r.next_double() * n as f64) as usize;
                let rolls = (draw(4), draw(ADJECTIVES.len()), draw(HINTS.len()), rng.random_range(0..LOW.len()));
                let (mood, said) = verdict(t.quality, snob, insane, glasses, rolls, (rng.random_range(0..11), rng.random()));
                if let Some(mut m) = moodlets {
                    m.add(crate::life::MoodletKind::SugarRush, now);
                    if b.ten && b.share("Flame Fruit") > FLAME_SHARE && t.quality > 5 {
                        m.add(crate::life::MoodletKind::FlameOn, now);
                    }
                    if b.ten && b.share("Lifefruit") > 0.0 {
                        m.add(crate::life::MoodletKind::FullOfLife, now);
                    }
                    if mood > 0 {
                        m.add_valued(crate::life::MoodletKind::GoodNectar, now, mood, 8.0 * 60.0);
                    } else if mood < 0 {
                        m.add_valued(crate::life::MoodletKind::BadNectar, now, mood, 2.0 * 60.0);
                    }
                }
                // (Lifefruit enough in it: a day younger, as eating one.)
                if b.ten && b.share("Lifefruit") > LIFE_SHARE {
                    commands.entity(e).insert(crate::aging::AteLifefruit);
                }
                did.write(crate::journal::Did::count(e, crate::journal::Stat::NectarGlasses, 1.0));
                if selected {
                    notes.push(format!("{}: \"{said}\"", sim.first));
                }
                info!("nectar: {} drinks {} (mood {mood}): {said}", sim.first, b.name);
                commands.entity(e).remove::<Tasting>();
            }
            None => {
                // (Never begun: put off, it's still to come; gone from the queue, it's lost.)
                if !q.0.iter().any(|a| a.label == TASTE) {
                    commands.entity(e).remove::<Tasting>();
                }
            }
        }
    }
}

/// Bottles worth more by the day (`AppreciateDepreciateDailyUpdate`): those in inventories, and
/// their quality with it.
fn appreciate(clock: Res<crate::clock::GameClock>, mut invs: Query<&mut Inventory>, mut last: Local<Option<u32>>) {
    let today = clock.day();
    if *last == Some(today) {
        return;
    }
    *last = Some(today);
    for mut inv in &mut invs {
        for s in inv.0.iter_mut().filter(|s| s.kind == ItemKind::Nectar) {
            let Some(b) = Bottle::from_key(&s.key, &s.name) else { continue };
            let each = b.worth(today);
            s.worth = each * s.count as i64;
            s.quality = quality(each as f32);
        }
    }
}

/// `NECTAR_TEST=1` (tests): ten seconds in, a nectar maker put down near the selected Sim, the
/// camera on it; `NECTAR_FRUIT=<produce>/<tier>*<n>,...` given them; `NECTAR_LEVEL=<n>`;
/// `NECTAR_DO=add|squish|make|take|all|taste` (all: fruit in, then made, then taken; taste: a
/// bottle worth `NECTAR_WORTH` tasted);
/// `NECTAR_FAST=1`: the machine done in a minute; `NECTAR_PIE=1`: its menu.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn nectar_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(Entity, &Transform, &mut ActionQueue, &mut Skills, Option<&Inventory>, &Sim), With<crate::sim::Selected>>,
    mut makers: Query<(Entity, &GameObject, &Transform, Option<&mut Maker>), Without<crate::sim::Selected>>,
    (catalog, data, grid, ui): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Option<Res<crate::nav::NavGrid>>, Option<Res<crate::icons::GameUi>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    (world, building, clock): (Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>, Res<crate::clock::GameClock>),
    mut stage: Local<(u8, f32)>,
) {
    if std::env::var_os("NECTAR_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let maker = makers.iter().find(|m| m.1.kind == ObjectKind::NectarMaker).map(|m| (m.0, *m.2));
    if let (Ok(mut c), Some((_, mtf))) = (cams.single_mut(), maker) {
        let f = |k: &str, d: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(mtf.translation);
        c.height_offset = f("NECTAR_LOOK_Y", 1.0);
        c.distance = f("NECTAR_DIST", 5.0);
        c.pitch = f("NECTAR_PITCH", 0.35);
        let fwd = mtf.rotation * Vec3::Z;
        c.yaw = fwd.x.atan2(fwd.z) + f("NECTAR_YAW", 0.5);
    }
    // (The machine hurried.)
    if env("NECTAR_FAST").is_some() {
        for (.., m) in &mut makers {
            if let Some(mut m) = m
                && let Some(b) = m.batch.as_mut()
                && b.until > clock.minutes + 1.0
            {
                b.until = clock.minutes + 1.0;
            }
        }
    }
    let Ok((e, tf, mut q, mut skills, inv, sim)) = sims.single_mut() else { return };
    let do_ = env("NECTAR_DO").unwrap_or_default();
    match stage.0 {
        // (NECTAR_DO=taste: a bottle of a worth, `NECTAR_WORTH`, tasted; no maker.)
        0 if do_ == "taste" => {
            *stage = (9, now);
            let worth: f32 = env("NECTAR_WORTH").and_then(|v| v.parse().ok()).unwrap_or(40.0);
            let b = Bottle { name: "Apple Lime Nectar".into(), hash: 0x5eed_1234, week: 1, day: 0, base: worth, mix: vec![("Apple".into(), 0.5), ("Lime".into(), 0.5)], ten: false };
            let stack = crate::inventory::Stack { kind: ItemKind::Nectar, key: b.key(), name: b.shown(), quality: quality(worth), count: 1, worth: worth as i64 };
            taste(&mut commands, e, &mut q, stack, sim);
        }
        0 => {
            *stage = (1, now);
            if let Some(l) = env("NECTAR_LEVEL").and_then(|v| v.parse::<f32>().ok()) {
                skills.0.insert(SKILL, l);
            }
            // The fruit.
            if let (Some(v), Some(ui)) = (env("NECTAR_FRUIT"), ui.as_deref()) {
                for item in v.split(',') {
                    let (what, n) = item.split_once('*').map_or((item, 1), |(w, n)| (w, n.parse().unwrap_or(1)));
                    let (key, tier) = what.split_once('/').map_or((what, 3), |(k, t)| (k, t.parse().unwrap_or(3)));
                    let Some(p) = ui.data.plants.iter().find(|p| p.produce.eq_ignore_ascii_case(key)) else { continue };
                    let (word, mult) = crate::gardening::QUALITIES[tier.min(9)];
                    crate::inventory::give(&mut commands, e, ItemKind::Produce, p.produce.clone(), format!("{word} {}", p.produce), tier.min(9) as u8, (p.price as f32 * mult).round() as i64, n);
                }
            }
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("NectarMaker")).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = tf.translation.xz();
            let spot = (4..30).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32 * 0.5)).find(|c| (-8..=8).all(|i| (-8..=8).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.4)))).unwrap_or(here + Vec2::X * 3.0);
            let y = crate::building::walk_height(&world.data, building.as_deref(), Vec3::new(spot.x, tf.translation.y, spot.y));
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, Vec3::new(spot.x, y, spot.y), 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("nectar test: put down");
            }
        }
        1 if now - stage.1 > 3.0 => {
            let Some((m, mtf)) = maker else { return };
            *stage = (2, now);
            if env("NECTAR_PIE").is_some() {
                commands.insert_resource(crate::hud::PieFor(m));
                return;
            }
            let fruit: Vec<(String, u8)> = inv.map(|i| i.0.iter().filter(|s| s.kind == ItemKind::Produce).flat_map(|s| (0..s.count).map(move |_| (s.key.clone(), s.quality))).take(MAX_FRUIT).collect()).unwrap_or_default();
            if matches!(do_.as_str(), "add" | "all" | "squish" | "make") {
                q.push_player(Action::new(ADD_FRUIT, step(m, &mtf, Step::Add(fruit)), false));
            }
            match do_.as_str() {
                "squish" => q.push_player(Action::new(SQUISH, step(m, &mtf, Step::Squish { then: None }), false)),
                "make" | "all" => q.push_player(Action::new(MAKE, step(m, &mtf, Step::Squish { then: Some(Style::Basic) }), false)),
                "take" => q.push_player(Action::new(TAKE, step(m, &mtf, Step::Take), false)),
                _ => {}
            }
            info!("nectar test: {do_}");
        }
        2 if do_ == "all" && maker.is_some_and(|(m, _)| makers.get(m).is_ok_and(|x| x.3.is_some_and(|x| !x.bottles.is_empty()))) && q.0.is_empty() => {
            *stage = (3, now);
            let (m, mtf) = maker.unwrap_or((Entity::PLACEHOLDER, Transform::IDENTITY));
            q.push_player(Action::new(TAKE, step(m, &mtf, Step::Take), false));
            info!("nectar test: take");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_nets_random_draws_as_the_game_does() {
        // (.NET's `new Random(0).NextDouble()`.)
        let mut r = NetRandom::new(0);
        assert!((r.next_double() - 0.7262432699679598).abs() < 1e-12);
        let c = combo(0x1234_5678);
        assert!((MIN_COMBO..=MAX_COMBO).contains(&c));
        assert_eq!(combo(42), combo(42), "a blend's always as good");
    }

    #[test]
    fn nectar_is_worth_what_the_game_makes_it() {
        let apples = |tier: u8| vec![Fruit { key: "Apple".into(), tier }; 10];
        // (Ten Normal apples at level 0: 30 a third, three quarters.)
        let w = batch_worth(&apples(3), |_| 3.0, 0.333, 0.0, 1.0, Style::Basic);
        assert!((w - 30.0 * 0.333 * 0.75).abs() < 1e-3, "{w}");
        // (Perfect ones at level ten, concentrated.)
        let w = batch_worth(&apples(9), |_| 3.0, 0.333, 10.0, 1.0, Style::Concentrated);
        assert!((w - 30.0 * 0.333 * 1.5 * 2.5 * 1.25).abs() < 1e-3, "{w}");
        assert_eq!(batch_bottles(Style::Basic, false), 3);
        assert_eq!(batch_bottles(Style::MassProduce, true), 6);
        assert_eq!(batch_bottles(Style::Concentrated, false), 2);
        assert_eq!(quality(7.5), 4);
        assert_eq!(quality(1.0), 1);
        assert_eq!(quality(500.0), 11);
    }

    #[test]
    fn bottles_keep_and_gain_their_worth() {
        let b = Bottle { name: "Apple Lime Nectar".into(), hash: 0xdead_beef, week: 2, day: 9, base: 20.0, mix: vec![("Apple".into(), 0.6), ("Lime".into(), 0.4)], ten: true };
        assert_eq!(b.worth(9), 20);
        assert_eq!(b.worth(14), 24, "four in a hundred of its worth a day");
        let back = Bottle::from_key(&b.key(), &b.shown()).unwrap();
        assert_eq!(back.hash, b.hash);
        assert_eq!(back.day, 9);
        assert_eq!(back.name, "Apple Lime Nectar");
        assert_eq!(back.mix.len(), 2);
        assert!(back.ten && (back.share("Lime") - 0.4).abs() < 1e-3);
        assert_eq!(b.shown(), "Week: 2 - Apple Lime Nectar");
    }

    #[test]
    fn a_glass_is_taken_as_the_game_has_it() {
        assert_eq!(drink_clip(false, false, 3.0, 0), SNIFF);
        assert_eq!(drink_clip(false, false, 12.0, 0), SWIRL);
        assert_eq!(drink_clip(false, false, 40.0, 0), SNOOTY);
        assert_eq!(drink_clip(true, false, 0.0, 0), SNOOTY, "a Snob's a connoisseur");
        // (A novice: the moods by quality, and a plain word for it.)
        let (m, said) = verdict(7, false, false, 0.0, (0, 0, 0, 2), (0, 0));
        assert_eq!(m, 8);
        assert_eq!(said, "Yum, nectar!");
        // (A connoisseur's notes.)
        let (_, said) = verdict(9, false, false, 50.0, (0, 1, 3, 3), (0, 0));
        assert_eq!(said, "Delicious!  This is oaky with a strong hint of honey.");
        // (A Snob: only a fine one pleases, the better for it.)
        assert_eq!(verdict(10, true, false, 0.0, (0, 0, 0, 0), (0, 0)).0, 25);
        assert_eq!(verdict(7, true, false, 0.0, (0, 0, 0, 0), (0, 0)).0, 0);
        assert_eq!(verdict(2, false, false, 0.0, (0, 0, 0, 0), (0, 0)).0, -15);
    }

    #[test]
    fn a_blend_is_named_for_its_fruit() {
        let f = |k: &str| Fruit { key: k.into(), tier: 3 };
        let (mix, name) = blend(&[f("Lime"), f("Apple"), f("Apple")], |k| k.to_string());
        assert_eq!(name, "Apple Lime Nectar");
        assert!((mix[0].1 - 2.0 / 3.0).abs() < 1e-5);
        let (_, name) = blend(&[f("Apple")], |k| k.to_string());
        assert_eq!(name, "Apple Nectar");
        assert_eq!(grade(0.6), "Terrible");
        assert_eq!(grade(1.6), "Amazing");
        let mut r = Recipes::default();
        for i in 0..8 {
            r.made(Recipe { name: format!("N{i}"), hash: i, fruit: vec![], worth: i as f32 });
        }
        assert_eq!(r.0.len(), TOP_NECTARS);
        assert_eq!(r.0[0].name, "N7");
    }
}
