//! Interactions: object kinds, the per-sim action queue, social interactions, autonomy,
//! careers and household money.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use rand::Rng;
use s3bake::Key;

use crate::PlayMode;
use crate::clock::{GameClock, SimDelta};
use crate::loading::CurrentWorld;
use crate::nav::{Floor, NavGrid, PathFollow, UpperFloors, plan_route};
use crate::life::{LifeEvent, LifeEventKind};
use crate::sim::*;

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Notifications>().add_systems(
            Update,
            (comings_and_goings, off_lot_idle, autonomy, run_actions, motive_warnings, pay_bills, parties, crate::social::note_contact, crate::social::fade_relationships)
                .chain()
                .run_if(in_state(PlayMode::Live)),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Objects

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ObjectKind {
    Fridge,
    Stove,
    Microwave,
    BedDouble,
    BedSingle,
    Toilet,
    Shower,
    Bathtub,
    Sink,
    Sofa,
    Chair,
    /// Bar stools (seats at counters).
    Stool,
    Tv,
    Computer,
    Stereo,
    Bookshelf,
    Mirror,
    Easel,
    Guitar,
    Treadmill,
    Chess,
    Crib,
    HighChair,
    ToyBox,
    Xylophone,
    PegBox,
    PottyChair,
    Dresser,
    Telescope,
    SwingSet,
    HotTub,
    DollHouse,
    JungleGym,
    Foosball,
    Table,
    Light,
    Plant,
    Decoration,
    /// A cooked group meal on its serving platter.
    Meal,
    /// Plates left after eating.
    DirtyDishes,
    /// A tombstone (the game's urnstone).
    Tombstone,
    /// A plant growing in the garden.
    GardenPlant,
    /// A gem, metal or space rock lying about, to collect.
    Collectible,
    /// Where the fish are biting.
    FishingSpot,
    /// A butterfly fluttering about, or a beetle in the grass, to catch.
    Butterfly,
    Beetle,
    Fireplace,
    /// A pool's ladder: where Sims get in for a swim.
    PoolLadder,
    /// A birthday cake, candles lit.
    BirthdayCake,
    /// Where the bills arrive.
    Mailbox,
    /// The morning paper.
    Newspaper,
    /// A barbecue grill.
    Grill,
    /// A coffee maker.
    HotBeverage,
    AlarmClock,
    /// A trash can: clearing dishes fills it.
    TrashCan,
    /// A fish bowl: a fish from a Sim's inventory swims in it.
    FishBowl,
    /// A games console.
    VideoGame,
    /// The SimLife Goggles: virtual reality.
    VrGoggles,
    /// A teddy bear or one of the little toys (a toy boat, a pony, a robot...).
    StuffedToy,
    /// A garden sprinkler.
    Sprinkler,
    /// A pool's diving board: Sims dive in for a swim.
    DivingBoard,
    /// A bar: drinks made at it.
    Bar,
    /// A dishwasher: the dishes cleared go in it (quicker than washing up at the sink).
    Dishwasher,
    /// A trash compactor: a trash can that holds three times as much.
    TrashCompactor,
    /// The Teleporter (a lifetime reward): to a community lot at once.
    Teleporter,
    /// The Collection Helper (a lifetime reward): collectibles shown in Map View.
    CollectionHelper,
    /// Lifetime rewards: a plate of food at the push of a button, a new shape, a new mood.
    FoodReplicator,
    BodySculptor,
    MoodletManager,
    Other,
}

impl ObjectKind {
    pub fn from_script(script: &str, name: &str) -> Self {
        let s = script.to_ascii_lowercase();
        let n = name.to_ascii_lowercase();
        let has = |k: &str| s.contains(k);
        if has("crib") {
            Self::Crib
        } else if has("electronics.videogamesystem") {
            Self::VideoGame
        } else if has("electronics.vrgoggles") {
            Self::VrGoggles
        } else if has("miscellaneous.stuffedanimal")
            || ["boat", "rocket", "pony", "car", "rabbit", "sheep", "dragon", "lochness", "yeti", "robot", "alligator"].iter().any(|t| s.ends_with(&format!("objects.toys.{t}")))
        {
            Self::StuffedToy
        } else if has("environment.sprinkler") {
            Self::Sprinkler
        } else if has("pools.divingboard") {
            Self::DivingBoard
        } else if has("objects.counters.bar") && !has("+") {
            Self::Bar
        } else if has("appliances.dishwasher") {
            Self::Dishwasher
        } else if has("miscellaneous.trashcompactor") {
            Self::TrashCompactor
        } else if has("rewards.collectionhelper") {
            Self::CollectionHelper
        } else if has("rewards.teleporter") {
            Self::Teleporter
        } else if has("rewards.foodreplicator") {
            Self::FoodReplicator
        } else if has("rewards.bodysculptor") {
            Self::BodySculptor
        } else if has("rewards.moodmodifier") {
            Self::MoodletManager
        } else if has("barbeque") {
            Self::Grill
        } else if has("hotbeveragemachine") {
            Self::HotBeverage
        } else if has("alarmclock") {
            Self::AlarmClock
        } else if has("trashcan") {
            Self::TrashCan
        } else if has("decorations.fishbowl") {
            Self::FishBowl
        } else if has("highchair") {
            Self::HighChair
        } else if has("toys.toybox") || has("toys.mimics.toybox") {
            Self::ToyBox
        } else if has("toys.xylophone") {
            Self::Xylophone
        } else if has("toypegbox") {
            Self::PegBox
        } else if has("pottychair") {
            Self::PottyChair
        } else if has("shelvesstorage") && (has("dresser") || has("wardrobe") || has("armoire")) {
            Self::Dresser
        } else if has("telescope") {
            Self::Telescope
        } else if has("swingset") {
            Self::SwingSet
        } else if has("hottub") {
            Self::HotTub
        } else if has("toys.dollhouse") {
            Self::DollHouse
        } else if has("junglegym") {
            Self::JungleGym
        } else if has("foosball") {
            Self::Foosball
        } else if has("pools.poolladder") {
            Self::PoolLadder
        } else if has("objects.fireplaces.") {
            Self::Fireplace
        } else if has("urnstone") {
            Self::Tombstone
        } else if has("mailbox") {
            Self::Mailbox
        } else if has("miscellaneous.newspaper") {
            Self::Newspaper
        } else if has("fridge") {
            Self::Fridge
        } else if has("microwave") {
            Self::Microwave
        } else if has("stove") {
            Self::Stove
        } else if has("beds.") && (has("double") || has("bedcircle") || has("heart")) {
            Self::BedDouble
        } else if has("beds.") {
            Self::BedSingle
        } else if has("toilet") {
            Self::Toilet
        } else if has("bathtubshower") || has("shower") {
            Self::Shower
        } else if has("bathtub") || has("hottub") {
            Self::Bathtub
        } else if has("sink") {
            Self::Sink
        } else if has("sofa") || has("loveseat") {
            Self::Sofa
        } else if has("stool") {
            Self::Stool
        } else if has("seating") || has("chair") || has("bench") {
            Self::Chair
        } else if has("electronics.tv") {
            Self::Tv
        } else if has("computer") {
            Self::Computer
        } else if has("stereo") {
            Self::Stereo
        } else if has("bookshelf") {
            Self::Bookshelf
        } else if has("mirror") {
            Self::Mirror
        } else if has("hobbiesskills.") && has("easel") && !has("canvas") {
            Self::Easel
        } else if has("guitar") && !has("decorations") {
            Self::Guitar
        } else if has("treadmill") || has("workout") {
            Self::Treadmill
        } else if has("chess") {
            Self::Chess
        } else if has("tables.") || has("counters.") {
            Self::Table
        } else if has("lighting") {
            Self::Light
        } else if has("flora") || n.contains("plant") || has("tree") {
            Self::Plant
        } else if has("decorations") {
            Self::Decoration
        } else {
            Self::Other
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Fridge | Self::Stove | Self::Microwave | Self::Grill | Self::HotBeverage => "Appliances",
            Self::AlarmClock => "Electronics",
            Self::TrashCan | Self::TrashCompactor => "Misc",
            Self::Dishwasher => "Appliances",
            Self::FishBowl => "Decor",
            Self::BedDouble | Self::BedSingle => "Beds",
            Self::Toilet | Self::Shower | Self::Bathtub | Self::Sink => "Plumbing",
            Self::Sofa | Self::Chair | Self::Stool => "Seating",
            Self::Tv | Self::Computer | Self::Stereo | Self::VideoGame | Self::VrGoggles => "Electronics",
            Self::Bookshelf | Self::Mirror | Self::Easel | Self::Guitar | Self::Treadmill | Self::Chess | Self::Telescope | Self::Foosball => {
                "Hobbies"
            }
            Self::SwingSet | Self::JungleGym | Self::DollHouse | Self::StuffedToy => "Kids",
            Self::Sprinkler => "Outdoors",
            Self::DivingBoard => "Outdoors",
            Self::Bar => "Surfaces",
            Self::FoodReplicator | Self::BodySculptor | Self::MoodletManager | Self::Teleporter | Self::CollectionHelper => "Misc",
            Self::HotTub => "Plumbing",
            Self::Dresser => "Surfaces",
            Self::Table => "Surfaces",
            Self::Light => "Lighting",
            Self::Plant | Self::Decoration => "Decor",
            Self::Meal | Self::DirtyDishes | Self::Tombstone | Self::Mailbox | Self::Newspaper | Self::GardenPlant | Self::Collectible | Self::FishingSpot | Self::Butterfly | Self::Beetle | Self::Fireplace | Self::PoolLadder | Self::BirthdayCake => "Misc",
            Self::Crib | Self::HighChair | Self::ToyBox | Self::Xylophone | Self::PegBox | Self::PottyChair => "Kids",
            Self::Other => "Misc",
        }
    }
}

/// A placed, interactive catalog object.
#[derive(Component, Clone)]
pub struct GameObject {
    pub kind: ObjectKind,
    pub name: String,
    pub objd: Key,
    pub price: i32,
    /// Local XZ bounds centre and half extents.
    pub center: Vec2,
    pub half: Vec2,
    pub height: f32,
}

impl GameObject {
    /// Where a sim stands to use the object (in front, along local +Z).
    pub fn use_point(&self, tf: &Transform) -> Vec2 {
        // (A pool ladder faces into the water: Sims stand behind it, on the side.)
        let ahead = if self.kind == ObjectKind::PoolLadder { -(self.half.y + 0.45) } else { self.half.y + 0.45 };
        let local = Vec3::new(self.center.x, 0.0, self.center.y + ahead);
        let p = tf.transform_point(local);
        Vec2::new(p.x, p.z)
    }

    pub fn world_center(&self, tf: &Transform) -> Vec3 {
        tf.transform_point(Vec3::new(self.center.x, 0.0, self.center.y))
    }

    /// Height of the seat / mattress surface.
    pub fn seat_height(&self) -> f32 {
        match self.kind {
            ObjectKind::BedDouble | ObjectKind::BedSingle => self.height.min(0.75).max(0.45),
            ObjectKind::Bathtub => 0.25,
            _ => 0.45,
        }
    }
}

#[derive(Component, Default)]
pub struct UsedBy(pub Option<Entity>);

/// A broken object: only repairing it is possible.
#[derive(Component)]
pub struct Broken;

/// How likely an object is to break when used (per use).
fn break_chance(kind: ObjectKind, price: i32) -> f64 {
    let base = match kind {
        ObjectKind::Shower | ObjectKind::Bathtub | ObjectKind::Sink | ObjectKind::Dishwasher => 0.04,
        ObjectKind::TrashCompactor => 0.02,
        ObjectKind::Toilet => 0.05,
        ObjectKind::Tv | ObjectKind::Computer | ObjectKind::Stereo => 0.03,
        _ => 0.0,
    };
    // (Better things last longer.)
    base * if price >= 1000 { 0.4 } else if price >= 500 { 0.7 } else { 1.0 }
}

/// What fixing an object is called, and the game's animation for it.
pub fn repair_of(kind: ObjectKind) -> (&'static str, crate::anim::ActionClip) {
    use crate::anim::ActionClip as A;
    match kind {
        ObjectKind::Toilet => ("Unclog", A::new(Some("a2o_toilet_unclog_start_x"), &["a2o_toilet_unclog_loop_x"])),
        ObjectKind::Shower => ("Repair", A::new(Some("a2o_shower_repairShower_start_x"), &["a2o_shower_repairShower_loop1_x", "a2o_shower_repairShower_loop2_x"])),
        ObjectKind::Bathtub | ObjectKind::HotTub => (
            "Repair",
            A::new(Some("a2o_bathtub_repair_start_x"), &["a2o_bathtub_repair_loopTightenLeft_x", "a2o_bathtub_repair_loopTightenRight_x", "a2o_bathtub_repair_loopWhackFaucet_x"]),
        ),
        ObjectKind::Sink => ("Repair", A::new(Some("a2o_sink_repair_start_x"), &["a2o_sink_repair_loop1_x", "a2o_sink_repair_loop2_x"])),
        ObjectKind::Dishwasher => (
            "Repair",
            A::new(Some("a2o_dishwasher_repair_start_kneel_x"), &["a2o_dishwasher_repair_loopTinker_x", "a2o_dishwasher_repair_loopInspect_x"]),
        ),
        ObjectKind::Tv => ("Repair", A::new(Some("a2o_tv_repair_start_x"), &["a2o_tv_repair_loop1_x", "a2o_tv_repair_loop2_x"])),
        ObjectKind::Computer => ("Repair", A::new(Some("a2o_computer_repair_start_x"), &["a2o_computer_repair_loop1_x", "a2o_computer_repair_loop2_x"])),
        ObjectKind::Stereo => ("Repair", A::new(Some("a2o_stereo_repair_start_x"), &["a2o_stereo_repair_loop1_x", "a2o_stereo_repair_loop2_x"])),
        _ => ("Repair", A::new(None, &["a2o_dishwasher_repair_loopTinker_x"])),
    }
}

/// "the Shower of Power" (but not "the The Porcelain Throne").
pub fn the(name: &str) -> String {
    if name.starts_with("The ") || name.starts_with("the ") { name.to_string() } else { format!("the {name}") }
}

pub fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// The repairman's call-out fee.
pub const REPAIRMAN_PRICE: i64 = 75;

/// The repairman, on his way.
#[derive(Resource)]
pub struct RepairmanVisit {
    pub arrive_at: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    None,
    /// Their looks changed at a mirror (hair, facial hair, glasses, make-up).
    ChangeAppearance,
    /// A plate of food from the food replicator.
    ReplicateFood,
    /// Off to a lot by the Teleporter (the menu's `ActionKind::Teleport`).
    Teleport,
    /// A new shape from the body sculptor (fitter, slimmer or fuller, by the interaction).
    Sculpt,
    /// A mood from the moodlet manager (by the interaction).
    SetMood,
    /// A cleared dish washed up at the sink or put in the dishwasher.
    WashDishes,
    /// A garden sprinkler turned on (it runs a couple of hours) or off.
    SprinklerOn,
    SprinklerOff,
    /// Running about in a sprinkler's spray (only while it's on).
    PlayInSprinkler,
    FindJob,
    QuitJob,
    SellPainting,
    Cook,
    /// Change into another of the Sim's outfits (the one planned: `ChangeIntoPlan`).
    ChangeClothes,
    /// Choose the everyday outfit piece by piece.
    PlanOutfit,
    /// Set the alarm clock, or turn it off.
    ToggleAlarm,
    /// Empty a full trash can.
    EmptyTrash,
    /// Put a fish (the one planned: `FishPlan`) in a bowl, or take it out.
    PlaceFish,
    TakeFish,
    /// Cook a group meal and serve it on a platter.
    ServeMeal,
    /// Take a serving from a platter (then sit down to eat it).
    GrabPlate,
    /// Put what's left of a meal in the fridge.
    PutAway,
    /// A plate of leftovers from the fridge.
    Leftovers,
    /// Eat a plate of food at a table (a dining chair's hidden interaction).
    EatMeal,
    /// Clear away dirty dishes.
    CleanUp,
    /// Pay the bills waiting in the mailbox.
    PayBills,
    /// Read the paper (then recycle it).
    ReadPaper,
    /// Garden plants.
    Water,
    Weed,
    Harvest,
    /// Pick up a find for the collection.
    Collect,
    /// Fish (the catch is reckoned at the end).
    Fish,
    /// Try to catch an insect.
    Catch,
    /// Light the fireplace, or put it out.
    LightFire,
    PutOutFire,
    /// Get in the pool and swim about.
    Swim,
    /// Do the day's homework.
    Homework,
    /// Bake a birthday cake (it's set out on a counter).
    BakeCake,
    /// Play a ranked chess match against an opponent of the Sim's rank.
    ChessMatch,
    /// Blow out a birthday cake's candles and grow up.
    BlowOutCandles,
    /// Write (a page at a time) the book under way.
    WriteNovel,
}

pub struct InteractionDef {
    pub name: &'static str,
    pub minutes: f32,
    pub per_hour: [f32; 6],
    pub until_full: Option<usize>,
    pub pose: Pose,
    pub on_object: bool,
    pub autonomous: bool,
    pub decay: [f32; 6],
    pub skill: Option<&'static str>,
    pub special: Special,
}

const N: [f32; 6] = [0.0; 6];
const D1: [f32; 6] = [1.0; 6];
const SLEEP_DECAY: [f32; 6] = [0.35, 0.35, 0.0, 0.3, 0.4, 0.2];

/// Minutes to eat a piece of produce.
const PRODUCE_MINUTES: f32 = 8.0;

const fn def(name: &'static str, minutes: f32, per_hour: [f32; 6], pose: Pose) -> InteractionDef {
    InteractionDef {
        name,
        minutes,
        per_hour,
        until_full: None,
        pose,
        on_object: false,
        autonomous: true,
        decay: D1,
        skill: None,
        special: Special::None,
    }
}

static FRIDGE: [InteractionDef; 4] = [
    InteractionDef { special: Special::Cook, ..def("Have Quick Meal", 30.0, [130.0, -6.0, 0.0, 0.0, -4.0, 4.0], Pose::Use) },
    // (What it leads to: a plate of a proper meal, eaten at the table.)
    InteractionDef { special: Special::Leftovers, ..def("Have Leftovers", 3.0, [5000.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use) },
    def("Grab a Snack", 12.0, [140.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Use),
    InteractionDef { autonomous: false, skill: Some("Cooking"), special: Special::BakeCake, ..def("Bake Birthday Cake", 30.0, [0.0, 0.0, 0.0, 0.0, 0.0, 6.0], Pose::Use) },
];
static CAKE: [InteractionDef; 1] =
    [InteractionDef { autonomous: false, special: Special::BlowOutCandles, ..def("Grow Up", 3.0, [0.0, 0.0, 0.0, 10.0, 0.0, 20.0], Pose::Use) }];
// (Cooking and grabbing a plate don't feed by themselves: the hunger figures are what they lead
// to, for autonomy to weigh. The meal is eaten afterwards.)
static STOVE: [InteractionDef; 1] = [InteractionDef {
    skill: Some("Cooking"),
    special: Special::ServeMeal,
    ..def("Cook Dinner", 60.0, [100.0, 0.0, 0.0, 0.0, -5.0, 10.0], Pose::Use)
}];
static MEAL: [InteractionDef; 2] = [
    InteractionDef { special: Special::GrabPlate, ..def("Grab a Plate", 2.0, [9000.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use) },
    InteractionDef { special: Special::PutAway, ..def("Put Away Leftovers", 3.0, N, Pose::Use) },
];
static MAILBOX: [InteractionDef; 1] = [InteractionDef { special: Special::PayBills, ..def("Pay Bills", 3.0, N, Pose::Use) }];
static NEWSPAPER: [InteractionDef; 2] = [
    InteractionDef { special: Special::ReadPaper, ..def("Read", 20.0, [0.0, 0.0, 0.0, 0.0, 0.0, 45.0], Pose::Use) },
    InteractionDef { special: Special::FindJob, ..def("Look for a Job", 10.0, N, Pose::Use) },
];
static GARDEN: [InteractionDef; 3] = [
    InteractionDef { special: Special::Water, skill: Some("Gardening"), ..def("Water", 12.0, [0.0, 0.0, 0.0, 0.0, -3.0, 4.0], Pose::Use) },
    InteractionDef { special: Special::Weed, skill: Some("Gardening"), ..def("Weed", 15.0, [0.0, 0.0, 0.0, 0.0, -8.0, 2.0], Pose::Use) },
    InteractionDef { special: Special::Harvest, skill: Some("Gardening"), ..def("Harvest", 10.0, [0.0, 0.0, 0.0, 0.0, -3.0, 8.0], Pose::Use) },
];
static COLLECTIBLE: [InteractionDef; 1] = [InteractionDef { special: Special::Collect, ..def("Collect", 4.0, [0.0, 0.0, 0.0, 0.0, -2.0, 10.0], Pose::Use) }];
static FISHING_SPOT: [InteractionDef; 1] =
    [InteractionDef { special: Special::Fish, skill: Some("Fishing"), ..def("Fish", 60.0, [-4.0, -4.0, -3.0, 0.0, -2.0, 25.0], Pose::Use) }];
static FIREPLACE: [InteractionDef; 3] = [
    InteractionDef { special: Special::LightFire, ..def("Light Fire", 2.0, [0.0, 0.0, 0.0, 0.0, 0.0, 4.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::PutOutFire, ..def("Put Out Fire", 2.0, [0.0; 6], Pose::Use) },
    def("Warm Hands", 20.0, [0.0, 0.0, 0.0, 0.0, 0.0, 14.0], Pose::Use),
];
static POOL_LADDER: [InteractionDef; 1] = [InteractionDef {
    special: Special::Swim,
    skill: Some("Athletic"),
    ..def("Swim", 60.0, [-6.0, -4.0, -12.0, 0.0, 10.0, 32.0], Pose::Use)
}];
static INSECT: [InteractionDef; 1] = [InteractionDef { special: Special::Catch, ..def("Catch", 2.0, [0.0, 0.0, 0.0, 0.0, -1.0, 12.0], Pose::Use) }];
static TOMBSTONE: [InteractionDef; 1] = [def("Mourn", 20.0, [0.0, 0.0, -2.0, 15.0, 0.0, -10.0], Pose::Stand)];
static GRILL: [InteractionDef; 1] = [InteractionDef {
    skill: Some("Cooking"),
    special: Special::ServeMeal,
    ..def("Grill", 40.0, [0.0, 0.0, 0.0, 0.0, -4.0, 15.0], Pose::Use)
}];
/// A cup of coffee: a lift for the tired, and a trip to the bathroom later.
static HOT_BEVERAGE: [InteractionDef; 1] = [def("Make Hot Beverage", 15.0, [24.0, -48.0, 80.0, 0.0, 0.0, 20.0], Pose::Use)];
static TRASH: [InteractionDef; 1] =
    [InteractionDef { special: Special::EmptyTrash, ..def("Empty Trash", 6.0, [0.0, 0.0, 0.0, 0.0, -6.0, 0.0], Pose::Use) }];
static FISHBOWL: [InteractionDef; 2] = [
    InteractionDef { autonomous: false, special: Special::PlaceFish, ..def("Place Fish", 2.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::TakeFish, ..def("Take Fish", 2.0, N, Pose::Use) },
];
static ALARM: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::ToggleAlarm, ..def("Set Alarm", 1.0, N, Pose::Use) }];
static DISHES: [InteractionDef; 1] = [InteractionDef { special: Special::CleanUp, ..def("Clean Up", 4.0, [0.0, 0.0, 0.0, 0.0, -2.0, 0.0], Pose::Use) }];
/// How a plate of food fills hunger, per hour, and how long it takes to eat.
const MEAL_PER_HOUR: f32 = 320.0;
const MEAL_MINUTES: f32 = 25.0;
static MICROWAVE: [InteractionDef; 1] = [def("Microwave Dinner", 20.0, [150.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use)];
static BED: [InteractionDef; 3] = [
    InteractionDef {
        until_full: Some(ENERGY),
        on_object: true,
        decay: SLEEP_DECAY,
        ..def("Sleep", 600.0, [0.0, 0.0, 24.0, 0.0, 0.0, 0.0], Pose::Lie)
    },
    InteractionDef { on_object: true, decay: SLEEP_DECAY, ..def("Nap", 90.0, [0.0, 0.0, 26.0, 0.0, 0.0, 0.0], Pose::Lie) },
    InteractionDef { on_object: true, ..def("Relax", 45.0, [0.0, 0.0, 6.0, 0.0, 0.0, 14.0], Pose::Lie) },
];
static TOILET: [InteractionDef; 1] =
    [InteractionDef { until_full: Some(BLADDER), on_object: true, ..def("Use Toilet", 15.0, [0.0, 900.0, 0.0, 0.0, -10.0, 0.0], Pose::Sit) }];
static SHOWER: [InteractionDef; 1] =
    [InteractionDef { until_full: Some(HYGIENE), ..def("Take Shower", 30.0, [0.0, 0.0, 0.0, 0.0, 400.0, 10.0], Pose::Use) }];
static BATHTUB: [InteractionDef; 1] = [InteractionDef {
    until_full: Some(HYGIENE),
    on_object: true,
    ..def("Take Bath", 50.0, [0.0, 0.0, 6.0, 0.0, 260.0, 40.0], Pose::Lie)
}];
static SINK: [InteractionDef; 2] = [
    def("Wash Hands", 6.0, [0.0, 0.0, 0.0, 0.0, 160.0, 0.0], Pose::Use),
    // (Where a dish cleared away is taken without a dishwasher: never on the menu.)
    InteractionDef { autonomous: false, special: Special::WashDishes, ..def("Wash Dishes", 8.0, N, Pose::Use) },
];
/// (Loaded with the dishes cleared away: never on the menu.)
static DISHWASHER: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::WashDishes, ..def("Load Dishes", 3.0, N, Pose::Use) }];
static SOFA: [InteractionDef; 2] = [
    InteractionDef { on_object: true, ..def("Sit", 40.0, [0.0, 0.0, 5.0, 0.0, 0.0, 10.0], Pose::Sit) },
    InteractionDef { on_object: true, decay: SLEEP_DECAY, ..def("Nap", 60.0, [0.0, 0.0, 18.0, 0.0, 0.0, 0.0], Pose::Lie) },
];
static CHAIR: [InteractionDef; 3] = [
    InteractionDef { on_object: true, autonomous: false, ..def("Sit", 30.0, [0.0, 0.0, 4.0, 0.0, 0.0, 4.0], Pose::Sit) },
    InteractionDef {
        on_object: true,
        autonomous: false,
        special: Special::EatMeal,
        ..def("Eat", MEAL_MINUTES, [MEAL_PER_HOUR, -4.0, 0.0, 20.0, -4.0, 6.0], Pose::Sit)
    },
    InteractionDef { on_object: true, special: Special::Homework, ..def("Do Homework", 45.0, [0.0, 0.0, -2.0, 0.0, 0.0, -6.0], Pose::Sit) },
];
/// The dining chair's "Eat" (not offered in its menu).
pub const CHAIR_EAT: usize = 1;
static TV: [InteractionDef; 2] = [
    def("Watch TV", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 55.0], Pose::Stand),
    def("Watch Cooking Channel", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 35.0], Pose::Stand),
];
static COMPUTER: [InteractionDef; 4] = [
    def("Play Computer Games", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 60.0], Pose::Use),
    InteractionDef { autonomous: false, skill: Some("Writing"), special: Special::WriteNovel, ..def("Write Novel", 120.0, [0.0, 0.0, -4.0, 0.0, 0.0, 10.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::FindJob, ..def("Find a Job", 20.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::QuitJob, ..def("Quit Job", 5.0, N, Pose::Use) },
];
static CRIB: [InteractionDef; 1] = [InteractionDef {
    until_full: Some(ENERGY),
    decay: SLEEP_DECAY,
    on_object: true,
    ..def("Nap in Crib", 180.0, [0.0, 0.0, 140.0, 0.0, 0.0, 0.0], Pose::Lie)
}];
static TOYBOX: [InteractionDef; 1] = [def("Play with Toys", 40.0, [0.0, 0.0, -4.0, 30.0, 0.0, 140.0], Pose::Use)];
static XYLOPHONE: [InteractionDef; 1] = [def("Play Xylophone", 40.0, [0.0, 0.0, -4.0, 20.0, 0.0, 130.0], Pose::Use)];
static PEGBOX: [InteractionDef; 1] = [def("Play with Peg Box", 40.0, [0.0, 0.0, -4.0, 10.0, 0.0, 120.0], Pose::Use)];
static POTTY: [InteractionDef; 1] = [InteractionDef { until_full: Some(BLADDER), ..def("Use Potty", 10.0, [0.0, 600.0, 0.0, 0.0, -20.0, 0.0], Pose::Sit) }];
static DRESSER: [InteractionDef; 2] = [
    InteractionDef { autonomous: false, special: Special::ChangeClothes, ..def("Change Into", 4.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::PlanOutfit, ..def("Plan Outfit", 2.0, N, Pose::Use) },
];
static TELESCOPE: [InteractionDef; 1] =
    [InteractionDef { skill: Some("Logic"), ..def("Stargaze", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 50.0], Pose::Use) }];
static SWINGSET: [InteractionDef; 1] = [def("Swing", 30.0, [0.0, 0.0, -6.0, 0.0, -4.0, 75.0], Pose::Use)];
static HOTTUB: [InteractionDef; 1] = [InteractionDef {
    on_object: true,
    ..def("Relax in Hot Tub", 60.0, [0.0, 0.0, 6.0, 10.0, 25.0, 50.0], Pose::Sit)
}];
static DOLLHOUSE: [InteractionDef; 1] = [def("Play with Dollhouse", 45.0, [0.0, 0.0, -3.0, 8.0, 0.0, 85.0], Pose::Use)];
static JUNGLEGYM: [InteractionDef; 1] = [def("Play on Jungle Gym", 40.0, [0.0, 0.0, -8.0, 0.0, -8.0, 95.0], Pose::Use)];
static FOOSBALL: [InteractionDef; 1] = [def("Play Foosball", 40.0, [0.0, 0.0, -4.0, 10.0, 0.0, 65.0], Pose::Use)];
static VIDEOGAME: [InteractionDef; 1] = [def("Play Video Games", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 70.0], Pose::Use)];
static SPRINKLER: [InteractionDef; 3] = [
    InteractionDef { autonomous: false, special: Special::SprinklerOn, ..def("Turn On", 2.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::SprinklerOff, ..def("Turn Off", 2.0, N, Pose::Use) },
    // (Only while it's running.)
    InteractionDef { special: Special::PlayInSprinkler, ..def("Play in Sprinkler", 30.0, [0.0, 0.0, -6.0, 0.0, 0.0, 110.0], Pose::Use) },
];
/// (Its menu lists the lots: `ActionKind::Teleport`.)
static TELEPORTER: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::Teleport, ..def("Teleport", 1.0, N, Pose::Use) }];
static FOOD_REPLICATOR: [InteractionDef; 1] =
    [InteractionDef { special: Special::ReplicateFood, ..def("Replicate Food", 3.0, [6000.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use) }];
/// (A new shape, the sculptor at work for an hour.)
static BODY_SCULPTOR: [InteractionDef; 3] = [
    InteractionDef { autonomous: false, special: Special::Sculpt, ..def("Sculpt Fitter", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 10.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::Sculpt, ..def("Sculpt Slimmer", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 10.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::Sculpt, ..def("Sculpt Fuller", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 10.0], Pose::Use) },
];
/// (The moods it sets.)
static MOODLET_MANAGER: [InteractionDef; 5] = [
    InteractionDef { autonomous: false, special: Special::SetMood, ..def("Feel Flirty", 2.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::SetMood, ..def("Feel Inspired", 2.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::SetMood, ..def("Feel Pumped", 2.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::SetMood, ..def("Feel Like Having Fun", 2.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::SetMood, ..def("Feel Well Rested", 2.0, N, Pose::Use) },
];
/// (Made, then drunk standing at the bar: a juice or smoothie, cheering and a little filling.)
static BAR: [InteractionDef; 1] = [def("Make a Drink", 25.0, [40.0, -30.0, 10.0, 0.0, 0.0, 45.0], Pose::Use)];
/// (A swim, begun with a dive.)
static DIVING_BOARD: [InteractionDef; 1] = [InteractionDef {
    special: Special::Swim,
    on_object: true,
    ..def("Dive In", 60.0, [0.0, 0.0, -10.0, 0.0, 15.0, 70.0], Pose::Use)
}];
static STUFFED_TOY: [InteractionDef; 1] = [def("Play with Toy", 30.0, [0.0, 0.0, -2.0, 6.0, 0.0, 75.0], Pose::Use)];
static VRGOGGLES: [InteractionDef; 1] = [def("Explore Virtual Worlds", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 95.0], Pose::Use)];
static STEREO: [InteractionDef; 1] = [def("Dance", 45.0, [0.0, 0.0, -6.0, 0.0, -6.0, 70.0], Pose::Dance)];
static BOOKSHELF: [InteractionDef; 1] =
    [InteractionDef { skill: Some("Logic"), ..def("Read a Book", 60.0, [0.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Stand) }];
static MIRROR: [InteractionDef; 2] = [
    InteractionDef { autonomous: false, skill: Some("Charisma"), ..def("Practice Speech", 40.0, [0.0, 0.0, -2.0, 6.0, 0.0, 10.0], Pose::Talk) },
    InteractionDef { autonomous: false, special: Special::ChangeAppearance, ..def("Change Appearance", 2.0, N, Pose::Use) },
];
static EASEL: [InteractionDef; 1] = [InteractionDef {
    skill: Some("Painting"),
    special: Special::SellPainting,
    ..def("Paint", 90.0, [0.0, 0.0, -3.0, 0.0, 0.0, 40.0], Pose::Use)
}];
static GUITAR: [InteractionDef; 1] =
    [InteractionDef { skill: Some("Guitar"), ..def("Play Guitar", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 45.0], Pose::Use) }];
static TREADMILL: [InteractionDef; 1] = [InteractionDef {
    autonomous: false,
    skill: Some("Athletic"),
    ..def("Work Out", 60.0, [-10.0, 0.0, -25.0, 0.0, -40.0, 5.0], Pose::Exercise)
}];
static CHESS: [InteractionDef; 2] = [
    InteractionDef { skill: Some("Logic"), ..def("Play Chess", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 40.0], Pose::Use) },
    InteractionDef { autonomous: false, skill: Some("Logic"), special: Special::ChessMatch, ..def("Play a Ranked Match", 60.0, [0.0, 0.0, -2.0, 10.0, 0.0, 35.0], Pose::Use) },
];

/// The animation played while performing an interaction: start clip, then loop variants.
pub fn interaction_clip(name: &str, kind: ObjectKind) -> Option<crate::anim::ActionClip> {
    use crate::anim::ActionClip as A;
    Some(match name {
        "Nap" if kind == ObjectKind::Sofa => A::new(Some("a2o_sofa_sit_trans_nap_x"), &["a2o_sofa_nap_loop1_x"]),
        "Eat" if kind == ObjectKind::Chair => A::new(Some("a2o_eat_diningIn_fork_start_x"), &["a2o_eat_diningIn_fork_neat_x"]),
        "Do Homework" => A::new(Some("a2o_homework_table_start_x"), &["a2o_homework_table_write_x", "a2o_homework_table_read_x", "a2o_homework_table_think_x", "a2o_homework_table_erase_x"]),
        "Pay Bills" => A::new(None, &["a2o_mailbox_getMail_x"]),
        "Water" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_wateringCan_start_x"), &["a2o_gardening_wateringCan_waterPlants_x"]),
        "Weed" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_crouch_start_x"), &["a2o_gardening_crouch_pullWeeds_x"]),
        "Harvest" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_bendover_start_x"), &["a2o_gardening_bendover_harvestmed_x"]),
        "Collect" if kind == ObjectKind::Collectible => A::new(Some("a2o_gardening_crouch_start_x"), &["a2o_gardening_crouch_harvestlow_x"]),
        "Swim" => A::new(Some("a2o_ladder_climbDown_L_x"), &["a_swim_cycle_x"]),
        "Light Fire" => A::new(None, &["a2o_fireplace_light_start_x"]),
        "Put Out Fire" => A::new(None, &["a2o_fireplace_putOut_x"]),
        "Warm Hands" => A::new(Some("a2o_fireplace_warmHands_start_x"), &["a2o_fireplace_warmHands_x"]),
        "Catch" if kind == ObjectKind::Butterfly => A::new(None, &["a2o_butterfly_catch_x"]),
        "Catch" if kind == ObjectKind::Beetle => A::new(None, &["a2o_beetle_catch_x"]),
        "Fish" if kind == ObjectKind::FishingSpot => {
            A::new(Some("a2o_fishHereWith_cast_normal_x"), &["a2o_fishHereWith_idle1_x", "a2o_fishHereWith_idle2_x", "a2o_fishHereWith_idle3_x"])
        }
        "Read" if kind == ObjectKind::Newspaper => A::new(Some("a2o_newspaper_read_standing_start_x"), &["a2o_newspaper_read_standing_loop"]),
        "Eat" if kind == ObjectKind::Stool => A::new(Some("a2o_eat_barStoolIn_fork_start_x"), &["a2o_eat_barStoolIn_fork_neat_x"]),
        "Have Quick Meal" | "Microwave Dinner" => A::new(Some("a2o_fridge_openDoor_x"), &["a2o_eat_stand_fork_neat", "a2o_eat_stand_hand_neat"]),
        "Grab a Snack" => A::new(Some("a2o_fridge_openDoor_x"), &["a2o_eat_stand_hand_neat"]),
        "Bake Birthday Cake" => A::new(Some("a2o_fridge_openDoor_x"), &["a2o_cuttingBoard_chop_loopMedSkill_x"]),
        "Grow Up" => A::new(None, &["a2o_birthdayCake_blowOut_counter_x"]),
        "Cook Dinner" => A::new(
            Some("a2o_stove_fryingPan_start_fromNeutral_x"),
            &["a2o_stove_fryingPan_idle_x", "a2o_stove_fryingPan_spatula_flip_x", "a2o_stove_fryingPan_spatula_poke_x"],
        ),
        "Use Toilet" => A::new(Some("a2o_toilet_useStanding_start_x"), &["a2o_toilet_useStanding_loop_x"]),
        "Take Shower" => A::new(Some("a2o_shower_takeShower_getIn_x"), &["a2o_shower_takeShower_loop"]),
        "Take Bath" => A::new(None, &["a2o_bathtub_relax_loop"]),
        "Wash Hands" => A::new(Some("a2o_sink_washhands_start_x"), &["a2o_sink_washhands_scrubHands_x", "a2o_sink_washhands_rinseHands_x"]),
        // (Only the plain loop: the toss-and-turn ones are authored from beside the bed.)
        "Sleep" => A::new(Some("a2o_bed_getIn_made_x"), &["a2o_bed_sleep_back_loop_x"]),
        "Nap" => A::new(Some("a2o_bed_nap_start_x"), &["a2o_bed_nap_loop_breathe_x"]),
        "Relax" => A::new(Some("a2o_bed_relax_getin_start_x"), &["a2o_bed_relax_loop"]),
        "Sit" => A::new(None, &["a2o_chairLiving_sit_breathe_loop_x", "a2o_chairLiving_sit_crossedLeg_front_loop_x"]),
        "Watch TV" | "Watch Cooking Channel" => A::new(None, &["a2o_tv_watch_idle1_standing", "a2o_tv_watch_idle2_standing", "a2o_tv_watch_idle3_standing", "a2o_tv_watch_active_standing"]),
        "Play Computer Games" => A::new(None, &["a2o_computer_game_loop1_x", "a2o_computer_game_loop2_x"]),
        "Write Novel" | "Find a Job" | "Quit Job" => A::new(None, &["a2o_computer_chess_type_loop_x"]),
        "Dance" => A::new(None, &["a_dance_beg_", "a_dance_med_"]),
        "Read a Book" => A::new(Some("a2o_book_readBook_standing_inInventory_start_x"), &["a2o_book_readBook_standing_loop"]),
        "Practice Speech" | "Change Appearance" => A::new(None, &["a2o_mirror_full_checkSelfOut_loop"]),
        "Paint" => A::new(Some("a2o_painting_start_x"), &["a2o_painting_loopMed", "a2o_painting_loopLarge", "a2o_painting_consider"]),
        "Play Guitar" => A::new(None, &["a2o_guitar_play_med_loop", "a2o_guitar_play_high_loop", "a2o_guitar_play_low_loop"]),
        "Work Out" => A::new(Some("a2o_treadmill_jog_start_x"), &["a2o_treadmill_jog_loop"]),
        "Play Chess" | "Play a Ranked Match" => A::new(None, &["a2o_chessTable_loop", "a2o_chessTable_move"]),
        "Nap in Crib" => A::new(Some("p2o_crib_sleep_start_y"), &["p2o_crib_sleep_loop_y"]),
        "Change Clothes" | "Change Into" | "New Everyday Outfit" | "Plan Outfit" => A::new(Some("a2o_dresser_use_open"), &["a2o_dresser_use_close"]),
        "Grill" => A::new(Some("a2o_bbq_grill_start"), &["a2o_bbq_grill_loopBreathe", "a2o_bbq_grill_loopPokeLeft", "a2o_bbq_grill_loopPokeRight", "a2o_bbq_grill_loopExpert"]),
        "Empty Trash" if kind == ObjectKind::TrashCompactor => A::new(None, &["a2o_trashCompactor_takeOut_x"]),
        "Empty Trash" => A::new(None, &["a2o_trashCan_empty_pullout_x"]),
        "Load Dishes" => A::new(None, &["a2o_dishwasher_use_x"]),
        "Wash Dishes" => A::new(Some("a2o_sink_dishes_scrub_start_x"), &["a2o_sink_dishes_scrub_loop1_x", "a2o_sink_dishes_scrub_loop2_x"]),
        "Clean Up" => A::steps("a2o_plateDinner_pickUp_table_part1_x", &["a2o_plateDinner_pickUp_table_part2_x"], &["a2o_plateDinner_carry_x"]),
        "Make Hot Beverage" => A::new(Some("a2o_hotBeverageMachine_fill"), &["a2o_hotBeverageMachine_drink_loopSip_standing", "a2o_hotBeverageMachine_drink_loopLongSip_standing"]),
        "Stargaze" => A::new(Some("a2o_telescope_start"), &["a2o_telescope_look_loop", "a2o_telescope_look_breathe", "a2o_telescope_react_wonderment"]),
        "Swing" => A::new(Some("a2o_swingset_getIn"), &["a2o_swingset_swing"]),
        "Relax in Hot Tub" => A::new(Some("a2o_hotTub_getIn"), &["a2o_hotTub_idles_relaxing_loop", "a2o_hotTub_idles_playingToe", "a2o_hotTub_splash"]),
        "Play with Dollhouse" => A::new(Some("c2o_dollhouse_play_start"), &["c2o_dollhouse_play_loop"]),
        "Play on Jungle Gym" => A::new(Some("c2o_JungleGymTower_climbUp"), &["c2o_JungleGymTower_loop", "c2o_JungleGymTower_slideDown"]),
        "Play Foosball" => A::new(None, &["a2o_foosballTable_play"]),
        "Play Video Games" => A::new(
            Some("a2o_videoGame_sitFloor_start_x"),
            &[
                "a2o_videoGame_sitFloor_play1_x",
                "a2o_videoGame_sitFloor_play2_x",
                "a2o_videoGame_sitFloor_play3_x",
                "a2o_videoGame_sitFloor_play4_x",
                "a2o_videoGame_sitFloor_play_excited_x",
                "a2o_videoGame_sitFloor_play_bang_x",
            ],
        ),
        // (Children's: grown-ups play with toys only with a little one.)
        "Replicate Food" => A::new(None, &["a2o_lifetimeReward_foodReplicator_pushButton_x"]),
        "Sculpt Fitter" | "Sculpt Slimmer" | "Sculpt Fuller" => A::steps("a2o_bodySculptor_openDoor_x", &["a2o_bodySculptor_getIn_x", "a2o_bodySculptor_closeDoor_x"], &["a2o_bodySculptor_working_x"]),
        "Make a Drink" => A::steps(
            "a2o_bar_makeDrink_start_x",
            &["a2o_bar_makeDrink_pour_x", "a2o_bar_makeDrink_blend_x", "a2o_bar_makeDrink_stop_x"],
            &["a2o_hotBeverageMachine_drink_loopSip_standing", "a2o_hotBeverageMachine_drink_loopLongSip_standing"],
        ),
        "Dive In" => A::new(
            Some("a2o_divingBoard_getIn_x"),
            &["a2o_divingBoard_dive_x", "a2o_divingBoard_diveBasic_x", "a2o_divingBoard_diveBeginner_x", "a2o_divingBoard_cannonBall_x"],
        ),
        "Play in Sprinkler" => A::new(
            Some("a2o_sprinklerGarden_playWith_start_x"),
            &[
                "a2o_sprinklerGarden_playWith_loop1_x",
                "a2o_sprinklerGarden_playWith_loop2_x",
                "a2o_sprinklerGarden_playWith_loop3_x",
                "a2o_sprinklerGarden_playWith_loop4_x",
                "a2o_sprinklerGarden_playWith_jumpOver_x",
            ],
        ),
        "Turn On" | "Turn Off" if kind == ObjectKind::Sprinkler => A::new(None, &["a2o_gardening_crouch_pullWeeds_x"]),
        "Play with Toy" => A::new(
            Some("a2o_stuffedAnimal_play_start_normal_x"),
            &["a2o_stuffedAnimal_play_loop1_x", "a2o_stuffedAnimal_play_loop2_x", "a2o_stuffedAnimal_play_loop3_x"],
        ),
        "Explore Virtual Worlds" => A::new(
            Some("a2o_Vrgoggles_put_on_x"),
            &["a2o_Vrgoggles_adventure_action_x", "a2o_Vrgoggles_adventure_fantasy_x", "a2o_Vrgoggles_adventure_space_x"],
        ),
        "Play with Toys" => A::new(Some("p2o_toybox_playIn_start"), &["p2o_toybox_playIn_breathe", "p2o_toybox_playIn_playWithToy", "p2o_toybox_playIn_peekOut"]),
        "Play Xylophone" => A::new(Some("p2o_toyXylophone_play_start"), &["p2o_toyXylophone_play_loop"]),
        "Play with Peg Box" => A::new(Some("p2o_toyPegBox_play_start"), &["p2o_toyPegBox_play_loopBreathe", "p2o_toyPegBox_play_loopLook", "p2o_toyPegBox_play_insertPeg"]),
        _ => return None,
    })
}

/// Both sides of a social's animation.
/// The clip a care social opens with (picking a toddler up, opening the book).
pub fn social_start(name: &str, little: Option<Age>) -> Option<&'static str> {
    match (name, little) {
        ("Play With" | "Cuddle" | "Change Diaper" | "Put to Bed", Some(Age::Toddler)) => Some("a2p_pickUp"),
        ("Read to", Some(Age::Toddler)) => Some("a2p_book_readWith_start"),
        ("Teach to Walk", _) => Some("a2p_teachToWalk_start_kneelDown"),
        ("Fight", _) => Some("a2a_soc_Bad_Fight_Start"),
        ("Give Back Rub", _) => Some("a2a_soc_Amorous_Massage_Amorous_start"),
        _ => None,
    }
}

pub fn social_clips(name: &str, little: Option<Age>) -> &'static [&'static str] {
    let baby = little == Some(Age::Baby);
    match name {
        // Babies are cradled while they're fed, changed and cuddled. (The game's own feeding
        // and changing clips IK-solve the arms around the baby and a bottle.)
        "Feed" | "Change Diaper" | "Cuddle" | "Play With" if baby => &["a2b_idle_carry_"],
        "Feed" => &["a2p_babyBottle_giveTake", "a2p_highChair_giveToddlerFood"],
        "Change Diaper" => &["a2p_changeDiaper"],
        "Cuddle" | "Play With" => &["a2p_carry_chat_loop", "a2p_idle_carry_breathe_y", "a2p_idle_carry_idle"],
        "Read to" => &["a2p_book_readWith_loop"],
        "Teach to Walk" => &["a2p_teachToWalk_loopBreathe", "a2p_teachToWalk_firstSteps"],
        "Tell Funny Story" => &["a2a_soc_Neutral_TellFunnyStory_Funny_Friendly"],
        "Tell Dramatic Story" => &["a2a_soc_Neutral_TellDramaticStory_Impressive_Neutral"],
        "Brag" => &["a2a_soc_Neutral_BoastAbout_Impressive_Friendly"],
        "Goof Around" => &["a2a_soc_Neutral_GoofAround_Funny_Neutral"],
        "Make Silly Face" => &["a2a_soc_Neutral_MakeSillyFace_Funny_Neutral"],
        "Cry on Shoulder" => &["a2a_soc_Neutral_CryOnShoulder_Friendly_Neutral"],
        "Cheer Up" => &["a2a_soc_Neutral_CheerUp_Friendly_Neutral"],
        "Apologize" => &["a2a_soc_Neutral_Apologize_Awkward_Neutral"],
        "Fight" => &["a2a_soc_Bad_Fight_Grapple_Loop", "a2a_soc_Bad_Fight_HeadLock_Loop"],
        "Yell At" => &["a2a_soc_bad_YellAt_Steamed_Bad"],
        "Irritate" => &["a2a_soc_bad_irritate_insulting_bad"],
        "Declare Nemesis" => &["a2a_soc_Bad_DeclareNemesis_Steamed_Bad"],
        "Embrace" => &["a2a_soc_Amorous_Embrace_Amorous_Amorous"],
        "Gaze Into Eyes" => &["a2a_soc_Amorous_GazeIntoEyes_Amorous_Amorous"],
        "Give Back Rub" => &["a2a_soc_Amorous_Massage_Amorous_Amorous"],
        "Leap Into Arms" => &["a2a_soc_Amorous_LeapIntoArms_Amorous_Amorous"],
        "Dip Kiss" => &["a2a_soc_amorous_dipKiss_amorous_amorous"],
        "Teach to Talk" => &["a2p_teachToTalk_loop", "a2p_teachToTalk_talkLoop", "a2p_teachToTalk_listen"],
        "Put to Bed" if baby => &["a2b_crib_putIn"],
        "Put to Bed" => &["a2p_crib_putIn"],
        "Try for Baby" => &["a2a_soc_amorous_kissMakeOut_accept_loop"],
        "Tell Joke" | "Do Funny Impression" => &["a2a_soc_neutral_tellJoke_accept"],
        "Compliment" => &["a2a_soc_Neutral_Compliment_Friendly"],
        "Compliment Appearance" => &["a2a_soc_Neutral_Compliment_Amorous"],
        "Hug" => &["a2a_soc_friendly_hug_accept", "a2a_soc_Neutral_FriendlyHug_Friendly_Neutral"],
        "High Five" => &["a2a_soc_neutral_highFive_friendly_neutral"],
        "Tickle" => &["a2a_soc_neutral_probe_tickle"],
        "Flirt" => &["a2a_soc_Neutral_Flirt_Neutral_Neutral", "a2a_soc_Neutral_Flirt_Amorous_Amorous"],
        "Hold Hands" => &["a2a_soc_Amorous_HoldHands_Affectionate_Amorous"],
        "Kiss" => &["a2a_soc_Amorous_ShyKiss_Amorous_Amorous", "a2a_soc_amorous_kissRomantic_romantic_amorous"],
        "Make Out" => &["a2a_soc_amorous_kissMakeOut_accept_loop"],
        "Propose Marriage" => &["a2a_soc_Amorous_ProposeMarriage_Amorous_Amorous"],
        "Get Married" => &["a2a_soc_Amorous_Wedding_Amorous_Amorous"],
        "Insult" | "Argue" => &["a2a_soc_Bad_Mock_Insulting_Bad", "a2a_soc_Bad_Accuse_Insulting_Bad"],
        "Slap" => &["a2a_soc_Bad_Slap_Steamed_Bad"],
        "Break Up" => &["a2a_soc_Neutral_BreakUp_Neutral_Neutral"],
        "Dance Together" => &["a2a_danceClub_dance_medSkill_loop1"],
        _ => &["a2a_soc_Neutral_Gossip_Friendly_Neutral", "a2a_soc_Neutral_RambleAimlessly_talk"],
    }
}

impl ObjectKind {
    /// Whether a Sim of this age can use the object themselves: babies use nothing, toddlers
    /// only their own things, and grown-ups leave the toddler toys alone.
    pub fn usable_by(self, age: Age) -> bool {
        use ObjectKind as K;
        match age {
            Age::Baby => false,
            Age::Toddler => matches!(self, K::Crib | K::ToyBox | K::Xylophone | K::PegBox | K::PottyChair | K::StuffedToy),
            // (Children can't cook, and the goggles are for teens and up.)
            Age::Child => !matches!(self, K::Crib | K::Xylophone | K::PegBox | K::PottyChair | K::HighChair | K::HotTub | K::Stove | K::Fireplace | K::VrGoggles),
            _ => !matches!(self, K::Crib | K::ToyBox | K::Xylophone | K::PegBox | K::PottyChair | K::HighChair | K::DollHouse | K::JungleGym | K::StuffedToy),
        }
    }
}

pub fn interactions_for(kind: ObjectKind) -> &'static [InteractionDef] {
    match kind {
        ObjectKind::Fridge => &FRIDGE,
        ObjectKind::Stove => &STOVE,
        ObjectKind::Microwave => &MICROWAVE,
        ObjectKind::BedDouble | ObjectKind::BedSingle => &BED,
        ObjectKind::Toilet => &TOILET,
        ObjectKind::Shower => &SHOWER,
        ObjectKind::Bathtub => &BATHTUB,
        ObjectKind::Sink => &SINK,
        ObjectKind::Sofa => &SOFA,
        ObjectKind::Chair | ObjectKind::Stool => &CHAIR,
        ObjectKind::Meal => &MEAL,
        ObjectKind::DirtyDishes => &DISHES,
        ObjectKind::Tombstone => &TOMBSTONE,
        ObjectKind::GardenPlant => &GARDEN,
        ObjectKind::Collectible => &COLLECTIBLE,
        ObjectKind::FishingSpot => &FISHING_SPOT,
        ObjectKind::Butterfly | ObjectKind::Beetle => &INSECT,
        ObjectKind::Fireplace => &FIREPLACE,
        ObjectKind::PoolLadder => &POOL_LADDER,
        ObjectKind::BirthdayCake => &CAKE,
        ObjectKind::Mailbox => &MAILBOX,
        ObjectKind::Newspaper => &NEWSPAPER,
        ObjectKind::Tv => &TV,
        ObjectKind::Computer => &COMPUTER,
        ObjectKind::Stereo => &STEREO,
        ObjectKind::Bookshelf => &BOOKSHELF,
        ObjectKind::Mirror => &MIRROR,
        ObjectKind::Easel => &EASEL,
        ObjectKind::Guitar => &GUITAR,
        ObjectKind::Treadmill => &TREADMILL,
        ObjectKind::Chess => &CHESS,
        ObjectKind::Crib => &CRIB,
        ObjectKind::ToyBox => &TOYBOX,
        ObjectKind::Xylophone => &XYLOPHONE,
        ObjectKind::PegBox => &PEGBOX,
        ObjectKind::PottyChair => &POTTY,
        ObjectKind::Dresser => &DRESSER,
        ObjectKind::Grill => &GRILL,
        ObjectKind::HotBeverage => &HOT_BEVERAGE,
        ObjectKind::AlarmClock => &ALARM,
        ObjectKind::TrashCan | ObjectKind::TrashCompactor => &TRASH,
        ObjectKind::Dishwasher => &DISHWASHER,
        ObjectKind::FishBowl => &FISHBOWL,
        ObjectKind::Telescope => &TELESCOPE,
        ObjectKind::SwingSet => &SWINGSET,
        ObjectKind::HotTub => &HOTTUB,
        ObjectKind::DollHouse => &DOLLHOUSE,
        ObjectKind::JungleGym => &JUNGLEGYM,
        ObjectKind::Foosball => &FOOSBALL,
        ObjectKind::VideoGame => &VIDEOGAME,
        ObjectKind::VrGoggles => &VRGOGGLES,
        ObjectKind::StuffedToy => &STUFFED_TOY,
        ObjectKind::Sprinkler => &SPRINKLER,
        ObjectKind::DivingBoard => &DIVING_BOARD,
        ObjectKind::Bar => &BAR,
        ObjectKind::FoodReplicator => &FOOD_REPLICATOR,
        ObjectKind::Teleporter => &TELEPORTER,
        ObjectKind::BodySculptor => &BODY_SCULPTOR,
        ObjectKind::MoodletManager => &MOODLET_MANAGER,
        _ => &[],
    }
}

// ---------------------------------------------------------------------------------------------
// Socials

pub use crate::social::{SOCIALS, SocialEffect};
use crate::social::RelStatus;

// ---------------------------------------------------------------------------------------------
// Actions

#[derive(Clone, Debug)]
pub enum ActionKind {
    Object { target: Entity, def: usize },
    Social { target: Entity, social: usize },
    GoHere(Vec2, u8),
    GoToWork,
    /// Apply for a career at the computer.
    JoinCareer { target: Entity, track: usize },
    /// Phone someone and invite them over.
    Invite { target: Entity },
    /// Phone a friend for a chat.
    PhoneChat { target: Entity },
    /// Phone the boss: an elder retires from their career, on a pension.
    Retire,
    /// Spend lifetime happiness on a reward (instant, from the rewards menu).
    BuyReward(usize),
    /// Drive to a community lot's rabbit hole for an activity.
    Visit { lot: usize, activity: usize },
    /// Eat a plate of food standing (no free seat at a table).
    EatHere,
    /// Eat a piece of produce from their inventory.
    EatItem { key: String, quality: u8 },
    /// Phone for a pizza to be delivered.
    OrderPizza,
    /// Phone the adoption agency: a baby (0), toddler (1) or child (2), a girl when `female`.
    Adopt { age: u8, female: bool },
    /// Phone the estate agent: the household moves to a new home.
    MoveHouse,
    /// Fix a broken object.
    Repair { target: Entity },
    /// Upgrade an object (a bit of `upgrades::Upgrade`).
    Upgrade { target: Entity, bit: u8 },
    /// Phone for the repairman.
    CallRepairman,
    /// Phone to hire a maid (or to let her go).
    HireMaid(bool),
    /// A need gone to nothing: 0, an accident (bladder); 1, collapsing from exhaustion and
    /// sleeping on the floor a while.
    MotiveFail(u8),
    /// Plant a seed here.
    PlantSeed { at: Vec2, level: u8, plant: usize },
    /// Phone round to throw a party.
    ThrowParty,
    /// Drive to a community lot and spend time there.
    GoToLot { lot: usize },
    /// Off to a community lot by the Teleporter (from its pad).
    Teleport { pad: Entity, lot: usize },
    /// Drive home from the community lot.
    GoHomeFromLot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Start,
    Routing,
    Running(f32),
}

#[derive(Clone, Debug)]
pub struct Action {
    pub label: String,
    pub kind: ActionKind,
    pub phase: Phase,
    pub autonomous: bool,
    pub cancel: bool,
}

impl Action {
    pub fn new(label: impl Into<String>, kind: ActionKind, autonomous: bool) -> Self {
        Self { label: label.into(), kind, phase: Phase::Start, autonomous, cancel: false }
    }
}

#[derive(Component, Default)]
pub struct ActionQueue(pub VecDeque<Action>);

/// Objects a Sim couldn't find a way to lately (and when): left alone by their autonomy for a
/// while, rather than tried again and again.
#[derive(Component, Default)]
pub struct RouteFailed(pub Vec<(Entity, f64)>);

/// How long a Sim leaves an object they couldn't reach (game minutes).
const ROUTE_FAIL_MINUTES: f64 = 120.0;

impl ActionQueue {
    /// Queues a player-chosen action, dropping autonomous ones first like the original.
    pub fn push_player(&mut self, a: Action) {
        for q in self.0.iter_mut() {
            if q.autonomous {
                q.cancel = true;
            }
        }
        if self.0.len() < 8 {
            self.0.push_back(a);
        }
    }
}

#[derive(Component)]
pub struct AutonomyTimer(pub f32);

#[derive(Component, Default)]
pub struct Skills(pub HashMap<&'static str, f32>);

impl Skills {
    pub fn level(&self, s: &str) -> u32 {
        (self.0.get(s).copied().unwrap_or(0.0) as u32).min(10)
    }
}

pub use crate::careers::{AtWork, Job};

#[derive(Resource)]
pub struct Household {
    pub name: String,
    pub funds: i64,
    pub lot_index: usize,
    pub last_bill_day: u32,
    /// Bills waiting in the mailbox.
    pub bills: Vec<Bill>,
}

/// A bill: how much, and the day it arrived.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Bill {
    pub amount: i64,
    pub day: u32,
}

#[derive(Resource, Default)]
pub struct Notifications(pub Vec<(String, f32)>);

impl Notifications {
    pub fn push(&mut self, s: impl Into<String>) {
        let s = s.into();
        info!("TNS: {s}");
        self.0.push((s, 9.0));
        if self.0.len() > 5 {
            self.0.remove(0);
        }
    }
}

/// The other side of a social: animating with `.0`.
#[derive(Component)]
pub struct SocialPartner(pub Entity);

/// A Sim joining the household (moving in or marrying in).
#[derive(Component)]
pub struct JoinHousehold {
    pub last_name: Option<String>,
}

/// A visitor walking off the lot to go home.
#[derive(Component)]
pub struct GoingHome;

/// Off the lot (at home elsewhere in town); can be phoned and invited over.
#[derive(Component)]
pub struct OffLot;

/// Sims at home elsewhere do nothing here: what they were about is called off (and what they
/// were using let go).
fn off_lot_idle(mut q: Query<&mut ActionQueue, With<OffLot>>) {
    for mut queue in &mut q {
        for a in queue.0.iter_mut().filter(|a| !a.cancel) {
            a.cancel = true;
        }
    }
}

/// Invited over: arrives at the lot exit at this time.
#[derive(Component)]
pub struct Invited {
    pub arrive_at: f64,
}

/// A visitor: leaves in the evening.
#[derive(Component)]
pub struct Visitor {
    pub leave_at: f64,
}

/// Moving in, going home and arriving for visits.
#[allow(clippy::type_complexity)]
fn comings_and_goings(
    mut commands: Commands,
    clock: Res<GameClock>,
    exit: Option<Res<LotExit>>,
    world: Res<CurrentWorld>,
    mut joining: Query<(Entity, &mut Sim, &JoinHousehold)>,
    mut leaving: Query<(Entity, &mut ActionQueue, &Transform), (With<GoingHome>, Without<OffLot>)>,
    mut visitors: Query<(Entity, &Visitor), (Without<GoingHome>, Without<OffLot>, Without<HouseholdMember>)>,
    mut arriving: Query<(Entity, &Invited, &mut Transform), Without<GoingHome>>,
    (town, mut household, mut notes, members): (Option<Res<crate::premade::TownPremades>>, Option<ResMut<Household>>, ResMut<Notifications>, Query<(), With<HouseholdMember>>),
) {
    for (e, mut sim, j) in &mut joining {
        if let Some(l) = &j.last_name {
            sim.last = l.clone();
        }
        // A townie moving in brings their share of their family's funds.
        if !members.contains(e)
            && let Some(h) = town.as_ref().and_then(|t| t.0.households.iter().find(|h| h.members.iter().any(|m| m.id == sim.id)))
        {
            let grown = h.members.iter().filter(|m| m.age & (s3formats::premade::AGE_YOUNG_ADULT | s3formats::premade::AGE_ADULT | s3formats::premade::AGE_ELDER) != 0).count().max(1);
            let share = h.funds.max(0) / grown as i64;
            if share > 0
                && let Some(hh) = household.as_mut()
            {
                hh.funds += share;
                notes.push(format!("{} brought §{} into the household.", sim.first, crate::lifetime::group(share)));
            }
        }
        commands.entity(e).remove::<(JoinHousehold, Visitor, GoingHome, OffLot)>().insert(HouseholdMember);
    }
    let Some(exit) = exit else { return };
    for (e, v) in &mut visitors {
        if clock.minutes >= v.leave_at {
            commands.entity(e).insert(GoingHome);
        }
    }
    for (e, mut queue, tf) in &mut leaving {
        let at_exit = Vec2::new(tf.translation.x, tf.translation.z).distance(exit.0) < 1.5;
        if at_exit {
            queue.0.clear();
            commands.entity(e).remove::<(GoingHome, Visitor, crate::nav::PathFollow)>().insert((OffLot, Visibility::Hidden));
        } else if queue.0.is_empty() {
            queue.0.push_back(Action::new("Go Home", ActionKind::GoHere(exit.0, 1), true));
        }
    }
    for (e, inv, mut tf) in &mut arriving {
        if clock.minutes >= inv.arrive_at {
            tf.translation = Vec3::new(exit.0.x, world.data.heightmap.sample(exit.0.x, exit.0.y), exit.0.y);
            let day_end = (clock.minutes / 1440.0).floor() * 1440.0 + 22.0 * 60.0;
            let leave_at = day_end.max(clock.minutes + 240.0);
            commands.entity(e).remove::<(Invited, OffLot)>().insert((Visibility::Inherited, Visitor { leave_at }));
        }
    }
}

/// A party being planned: guests are phoned and arrive at `start`.
#[derive(Resource)]
pub struct PartyPlan {
    pub by: Entity,
    pub start: f64,
}

/// A party under way, and who came.
#[derive(Resource)]
pub struct Party {
    pub end: f64,
    pub guests: Vec<Entity>,
    /// Conversations had during it.
    pub chats: u32,
}

/// What throwing a party costs (food and drink for the guests).
pub const PARTY_PRICE: i64 = 150;

/// Guests are invited (people the host knows first, then townsfolk); when it ends, the party is
/// judged by how much fun everyone had.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn parties(
    mut commands: Commands,
    clock: Res<GameClock>,
    plan: Option<Res<PartyPlan>>,
    party: Option<ResMut<Party>>,
    mut household: Option<ResMut<Household>>,
    hosts: Query<(&Sim, &Relationships)>,
    away: Query<(Entity, &Sim, Has<crate::town::Townie>), (Or<(With<OffLot>, With<crate::town::Townie>)>, Without<Invited>, Without<HouseholdMember>)>,
    present: Query<Entity, (With<Visitor>, Without<GoingHome>)>,
    mut members: Query<(&Sim, &mut crate::life::Moodlets, &Motives, Option<&crate::wishes::Wishes>), With<HouseholdMember>>,
    mut guests: Query<(&mut Visitor, &mut crate::life::Moodlets, &Motives, &Sim), Without<HouseholdMember>>,
    mut notes: ResMut<Notifications>,
    mut events: MessageReader<LifeEvent>,
    started: Option<Res<PartyPlan>>,
) {
    let _ = started;
    let socials = events.read().filter(|e| matches!(e.kind, LifeEventKind::Socialized { .. })).count() as u32;
    if let Some(p) = plan {
        commands.remove_resource::<PartyPlan>();
        let Ok((host, rels)) = hosts.get(p.by) else { return };
        let Some(h) = household.as_mut() else { return };
        if h.funds < PARTY_PRICE {
            notes.push(format!("There isn't enough money to throw a party (§{PARTY_PRICE})."));
            return;
        }
        h.funds -= PARTY_PRICE;
        // Friends first, then acquaintances, then whoever's about town.
        let mut people: Vec<(f32, Entity, bool)> =
            away.iter().filter(|(_, s, _)| !s.age.is_little()).map(|(e, _, townie)| (rels.friendship(e) + if rels.0.contains_key(&e) { 200.0 } else { 0.0 }, e, townie)).collect();
        people.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut rng = rand::rng();
        let mut guests: Vec<Entity> = present.iter().collect();
        let mut coming = 0;
        for (_, e, townie) in people.into_iter().take(8usize.saturating_sub(guests.len())) {
            // (A townsperson on a stroll becomes a guest.)
            if townie {
                commands.entity(e).remove::<(crate::town::Townie, crate::nav::PathFollow)>().insert(OffLot);
            }
            commands.entity(e).insert(Invited { arrive_at: p.start + rng.random_range(0.0..40.0) });
            guests.push(e);
            coming += 1;
        }
        notes.push(format!("{} is throwing a party! {coming} guests are coming at {}.", host.first, hour_label(((p.start / 60.0) % 24.0) as f32)));
        commands.insert_resource(Party { end: p.start + 300.0, guests, chats: 0 });
        return;
    }
    let Some(party) = party else { return };
    let party = {
        let mut p = party;
        p.chats += socials;
        p
    };
    // Guests stay until it's over.
    for &g in &party.guests {
        if let Ok((mut v, ..)) = guests.get_mut(g) {
            v.leave_at = v.leave_at.max(party.end);
        }
    }
    if clock.minutes < party.end {
        return;
    }
    commands.remove_resource::<Party>();
    // How did it go? The guests' (and hosts') fun and social.
    let mut score = 0.0;
    let mut n: f32 = 0.0;
    for &g in &party.guests {
        if let Ok((_, mut m, motives, sim)) = guests.get_mut(g) {
            score += motives.0[FUN] + motives.0[SOCIAL];
            n += 1.0;
            if sim.traits.contains(&crate::life::Trait::PartyAnimal) {
                m.add(crate::life::MoodletKind::AwesomeParty, clock.minutes);
            }
        }
    }
    debug!("party over: {} chats, {n} guests here, fun+social {:.0}", party.chats, if n > 0.0 { score / n } else { 0.0 });
    // A party is a hit when people talked (each social counts for both Sims in it).
    let great = n > 0.0 && (party.chats as f32 / 2.0 >= (n * 1.5f32).max(3.0) || score / n > 40.0);
    // (A Legendary Host's parties always are.)
    let great = great || members.iter().any(|(.., w)| crate::wishes::has(w, "LegendaryHost"));
    for (sim, mut m, ..) in &mut members {
        if !sim.age.is_little() {
            m.add(if great { crate::life::MoodletKind::GreatParty } else { crate::life::MoodletKind::LameParty }, clock.minutes);
        }
    }
    notes.push(if great { "The party was a hit! The guests had a great time.".to_string() } else { "The party fizzled out. Better luck next time.".to_string() });
}

/// A phone call: the cell phone out, then chatting (the game's clips, with the phone in hand).
const PHONE_CALL: crate::anim::ActionClip = crate::anim::ActionClip::new(
    Some("a2o_phone_makeCall_cellPhone_x"),
    &["a2o_phone_chat_talk_a_x", "a2o_phone_chat_talk_b_x", "a2o_phone_chat_listenRespond_agree_laugh_x", "a2o_phone_chat_talk_c_x", "a2o_phone_chat_talk_d_x"],
);
/// A chat on the phone: how long, and what it does for Social and Fun (an hour's worth) and the
/// friendship (all told).
const PHONE_CHAT_MINUTES: f32 = 30.0;
const PHONE_CHAT_SOCIAL: f32 = 90.0;
const PHONE_CHAT_FUN: f32 = 10.0;
const PHONE_CHAT_FRIENDSHIP: f32 = 6.0;

/// The home lot's walk-off point where sims leave for work and carpools arrive.
#[derive(Resource, Clone, Copy)]
pub struct LotExit(pub Vec2);

// ---------------------------------------------------------------------------------------------
// Systems

#[allow(clippy::type_complexity)]
fn run_actions(
    mut commands: Commands,
    delta: Res<SimDelta>,
    clock: Res<GameClock>,
    grid: Option<Res<NavGrid>>,
    world: Res<CurrentWorld>,
    exit: Option<Res<LotExit>>,
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    mut sims: Query<
        (
            Entity,
            &Sim,
            &mut ActionQueue,
            &mut Transform,
            &mut Motives,
            &mut DecayScale,
            &mut SimAnim,
            &mut Skills,
            &mut Relationships,
            Option<&mut PathFollow>,
            Option<&mut Job>,
            &Floor,
            (Option<&crate::wishes::Wishes>, Option<&crate::paintings::PaintPlan>),
            Option<&crate::opportunities::SimOpportunities>,
            Option<&crate::visit::OnLot>,
        ),
        (Without<GameObject>, Without<AtWork>, Without<crate::rabbitholes::AtRabbitHole>),
    >,
    mut objects: Query<(&GameObject, &Transform, &mut UsedBy, Option<&Floor>), Without<Sim>>,
    (building, upper, visited): (Option<Res<crate::building::ActiveBuilding>>, Option<Res<UpperFloors>>, Option<Res<crate::visit::VisitedLot>>),
    mut life: MessageWriter<LifeEvent>,
    people: Query<(Entity, &Sim, &crate::life::Mood, Has<HouseholdMember>), Without<GameObject>>,
    mut conceive: MessageWriter<crate::little::Conceive>,
    (mut fire, upgraded, baked): (MessageWriter<crate::fire::StartFire>, Query<&crate::upgrades::Upgrades>, Option<Res<crate::baked::Baked>>),
) {
    let Some(grid) = grid else { return };
    let dt = delta.0;
    let ground = |level: u8, x: f32, z: f32| crate::nav::floor_height(&world.data, building.as_deref(), level, Vec3::new(x, 0.0, z));
    // Social effects to apply to partners after the main pass: (target, actor, social, fun, rel, pose talk)
    let mut social_fx: Vec<(Entity, Entity, f32, f32, f32, f32)> = Vec::new();
    // Needs a care social fills for the little one: (target, per-need gain).
    let mut care_fx: Vec<(Entity, [f32; 6])> = Vec::new();
    // Social partners whose animation should end: (target, actor).
    let mut partners_done: Vec<(Entity, Entity)> = Vec::new();
    // Little ones a care social picks up: (little one, grown-up, age, grown-up's transform).
    let mut carry_fx: Vec<(Entity, Entity, Age, Transform, bool)> = Vec::new();
    let positions: HashMap<Entity, (Vec3, u8)> = sims.iter().map(|s| (s.0, (s.3.translation, s.11.0))).collect();
    let partnered: HashMap<Entity, bool> = sims.iter().map(|q| (q.0, q.8.partner().is_some())).collect();
    let who: HashMap<Entity, (Sim, crate::life::Mood, bool, bool)> = people
        .iter()
        .map(|(e, s, m, member)| (e, (s.clone(), *m, member, partnered.get(&e).copied().unwrap_or(false))))
        .collect();
    // Relationship changes to apply to both Sims: (a, b, status, kissed)
    let mut status_fx: Vec<(Entity, Entity, Option<RelStatus>, bool)> = Vec::new();

    for (me, sim, mut queue, mut tf, mut motives, mut decay, mut anim, mut skills, mut rels, path, mut job, floor, (wishes, plan), opps, on_lot) in &mut sims {
        // Out on a community lot: its walk grid and way out.
        let away = on_lot.and_then(|o| visited.as_deref().filter(|v| v.lot == o.0));
        let (my_grid, my_upper): (&NavGrid, Option<&UpperFloors>) = match away {
            Some(v) => (&v.grid, None),
            None => (&grid, upper.as_deref()),
        };
        let way_out = away.map(|v| v.exit).or_else(|| exit.as_ref().map(|e| e.0));
        let Some(action) = queue.0.front_mut() else {
            if anim.pose != Pose::Walk && anim.pose != Pose::Stand && path.is_none() {
                anim.pose = Pose::Stand;
            }
            continue;
        };
        let mut finished = false;
        let mut stand_up_at: Option<Vec2> = None;

        // Cancellation
        if action.cancel {
            if let ActionKind::Object { target, .. } | ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } = action.kind
                && let Ok((obj, otf, mut used, _)) = objects.get_mut(target)
            {
                if used.0 == Some(me) {
                    used.0 = None;
                }
                if matches!(action.phase, Phase::Running(_)) {
                    stand_up_at = Some(obj.use_point(otf));
                }
            }
            finished = true;
        }

        if !finished {
            match action.phase {
                Phase::Start => {
                    let dest: Option<(Vec2, u8)> = match &action.kind {
                        ActionKind::Object { target, def } => match objects.get(*target) {
                            Ok((obj, otf, used, of)) => {
                                if used.0.is_some_and(|u| u != me) {
                                    if !action.autonomous {
                                        notes.push(format!("{} can't use the {}: it's in use.", sim.first, obj.name));
                                    }
                                    None
                                } else {
                                    let _ = def;
                                    Some((obj.use_point(otf), of.map_or(1, |f| f.0)))
                                }
                            }
                            Err(_) => None,
                        },
                        ActionKind::Social { target, .. } => positions.get(target).map(|(p, l)| {
                            let mine = Vec2::new(tf.translation.x, tf.translation.z);
                            let theirs = Vec2::new(p.x, p.z);
                            (theirs + (mine - theirs).normalize_or(Vec2::X) * 0.9, *l)
                        }),
                        ActionKind::GoHere(p, l) => Some((*p, *l)),
                        // (Kneeling beside the spot.)
                        ActionKind::PlantSeed { at, level, .. } => Some((*at + Vec2::new(0.0, 0.7), *level)),
                        ActionKind::GoToWork | ActionKind::Visit { .. } | ActionKind::GoToLot { .. } | ActionKind::GoHomeFromLot => way_out.map(|p| (p, 1)),
                        ActionKind::JoinCareer { target, .. } | ActionKind::Teleport { pad: target, .. } => {
                            objects.get(*target).ok().map(|(obj, otf, _, of)| (obj.use_point(otf), of.map_or(1, |f| f.0)))
                        }
                        ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } => objects.get(*target).ok().map(|(obj, otf, _, of)| (obj.use_point(otf), of.map_or(1, |f| f.0))),
                        ActionKind::Invite { .. }
                        | ActionKind::PhoneChat { .. }
                        | ActionKind::Retire
                        | ActionKind::OrderPizza
                        | ActionKind::Adopt { .. }
                        | ActionKind::MoveHouse
                        | ActionKind::CallRepairman
                        | ActionKind::HireMaid(_)
                        | ActionKind::ThrowParty => {
                            // On the cell phone, as the game's Sims are.
                            action.phase = Phase::Running(0.0);
                            anim.pose = Pose::Talk;
                            commands.entity(me).insert(PHONE_CALL);
                            continue;
                        }
                        ActionKind::MotiveFail(k) => {
                            action.phase = Phase::Running(0.0);
                            anim.pose = Pose::Use;
                            let clip = if *k == 0 {
                                crate::anim::ActionClip::new(None, &["a_motFail_bladder_x"])
                            } else {
                                crate::anim::ActionClip::new(Some("a_motFail_exhausted_x"), &["a_sleeponFloor_breathe_x"])
                            };
                            commands.entity(me).insert(clip);
                            continue;
                        }
                        ActionKind::BuyReward(_) => None,
                        ActionKind::EatHere => {
                            action.phase = Phase::Running(0.0);
                            anim.pose = Pose::Use;
                            commands.entity(me).insert(crate::anim::ActionClip::new(Some("a2o_eat_stand_fork_start_x"), &["a2o_eat_stand_fork_neat_x"]));
                            continue;
                        }
                        ActionKind::EatItem { .. } => {
                            action.phase = Phase::Running(0.0);
                            anim.pose = Pose::Use;
                            commands.entity(me).insert(crate::anim::ActionClip::new(None, &["a2o_eat_stand_hand_neat"]));
                            continue;
                        }
                    };
                    let from = Vec2::new(tf.translation.x, tf.translation.z);
                    // A chair pushed in at a table is reached from behind or beside it.
                    let alternatives: Vec<Vec2> = match &action.kind {
                        ActionKind::Object { target, .. } => objects
                            .get(*target)
                            .ok()
                            .filter(|(o, ..)| matches!(o.kind, ObjectKind::Chair | ObjectKind::Stool | ObjectKind::Sofa))
                            .map(|(o, otf, ..)| {
                                [Vec3::new(o.center.x, 0.0, o.center.y - o.half.y - 0.45), Vec3::new(o.center.x + o.half.x + 0.45, 0.0, o.center.y), Vec3::new(o.center.x - o.half.x - 0.45, 0.0, o.center.y)]
                                    .map(|p| {
                                        let w = otf.transform_point(p);
                                        Vec2::new(w.x, w.z)
                                    })
                                    .to_vec()
                            })
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    let routed = dest.and_then(|(d, l)| {
                        plan_route(my_grid, my_upper, from, floor.0, d, l).or_else(|| alternatives.iter().find_map(|a| plan_route(my_grid, my_upper, from, floor.0, *a, l)))
                    });
                    match routed {
                        Some(wp) => {
                            commands.entity(me).insert(PathFollow::new(wp));
                            action.phase = Phase::Routing;
                            if let ActionKind::Object { target, .. } | ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } = action.kind
                                && let Ok((_, _, mut used, _)) = objects.get_mut(target)
                            {
                                used.0 = Some(me);
                            }
                        }
                        None => {
                            if dest.is_some() {
                                commands.entity(me).insert(crate::balloons::BalloonRequest::thought("t_balloon_routefail"));
                                if !action.autonomous {
                                    notes.push(format!("{} can't find a way to get there.", sim.first));
                                }
                                // (Not tried again for a while.)
                                if let ActionKind::Object { target, .. } = action.kind {
                                    let now = clock.minutes;
                                    commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| {
                                        if e.get::<RouteFailed>().is_none() {
                                            e.insert(RouteFailed::default());
                                        }
                                        if let Some(mut r) = e.get_mut::<RouteFailed>() {
                                            r.0.retain(|(_, t)| now - t < ROUTE_FAIL_MINUTES);
                                            r.0.push((target, now));
                                        }
                                    });
                                }
                            }
                            finished = true;
                        }
                    }
                }
                Phase::Routing => {
                    let arrived = path.as_ref().is_some_and(|p| p.done);
                    if arrived {
                        commands.entity(me).remove::<PathFollow>();
                        action.phase = Phase::Running(0.0);
                        match &action.kind {
                            ActionKind::Object { target, def } => {
                                if let Ok((obj, otf, _, _)) = objects.get(*target) {
                                    let d = &interactions_for(obj.kind)[*def];
                                    anim.pose = d.pose;
                                    if d.special == Special::EatMeal {
                                        commands.entity(me).insert(crate::meals::MealRequest::PlateAt(*target));
                                    }
                                    *decay = DecayScale(d.decay);
                                    // (Toddlers have their own way with a toy.)
                                    let toddler_toy = (sim.age == Age::Toddler && obj.kind == ObjectKind::StuffedToy)
                                        .then(|| {
                                            crate::anim::ActionClip::new(
                                                Some("p2o_stuffedAnimal_play_start_normal_x"),
                                                &["p2o_stuffedAnimal_play_loop1_x", "p2o_stuffedAnimal_play_loop2_x", "p2o_stuffedAnimal_play_loop3_x"],
                                            )
                                        });
                                    if let Some(c) = toddler_toy.or_else(|| interaction_clip(d.name, obj.kind)) {
                                        commands.entity(me).insert(c);
                                    }
                                    let face = otf.rotation * Quat::from_rotation_y(std::f32::consts::PI);
                                    if d.on_object {
                                        let bed = matches!(obj.kind, ObjectKind::BedSingle | ObjectKind::BedDouble);
                                        // In a double bed, one side of it.
                                        let side = if bed && obj.half.x > 0.8 { -obj.half.x * 0.5 } else { 0.0 };
                                        let c = otf.transform_point(Vec3::new(obj.center.x + side, 0.0, obj.center.y));
                                        tf.translation = Vec3::new(c.x, otf.translation.y, c.z);
                                        tf.rotation = otf.rotation;
                                        if d.special == Special::EatMeal {
                                            // The eating clips sit the Sim back from their root,
                                            // which belongs at the table's edge.
                                            let back = if obj.kind == ObjectKind::Stool { 0.67 } else { 0.576 };
                                            tf.translation += (otf.rotation * Vec3::Z).with_y(0.0).normalize_or_zero() * back;
                                        }
                                        if bed && d.name == "Nap" {
                                            // The nap clips are played from beside the bed, facing
                                            // across it (the Sim lies down 1.09 m ahead, head to
                                            // the left).
                                            let p = otf.transform_point(Vec3::new(obj.center.x + side - 1.09, 0.0, obj.center.y));
                                            tf.translation = Vec3::new(p.x, otf.translation.y, p.z);
                                            tf.rotation = otf.rotation * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
                                        }
                                        anim.seat_height = obj.seat_height();
                                    } else {
                                        tf.rotation = face;
                                        anim.seat_height = 0.0;
                                    }
                                }
                            }
                            ActionKind::Social { target, social } => {
                                if let Some(p) = positions.get(target) {
                                    let to = Vec2::new(p.0.x - tf.translation.x, p.0.z - tf.translation.z);
                                    tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                }
                                anim.pose = Pose::Talk;
                                let s = &SOCIALS[*social];
                                let little = who.get(target).map(|w| w.0.age).filter(|a| a.is_little());
                                let clips = social_clips(s.name, little);
                                let start = social_start(s.name, little);
                                let clip = |side| crate::anim::ActionClip { start, ..crate::anim::ActionClip::social(clips, side) };
                                commands.entity(me).insert(clip('x'));
                                commands.entity(*target).insert((clip('y'), SocialPartner(me)));
                                if let Some(age) = little {
                                    carry_fx.push((*target, me, age, *tf, s.effect != SocialEffect::PutToBed));
                                }
                                if let Some((tsim, tmood, _, tpartner)) = who.get(target) {
                                    let rel = rels.get(*target);
                                    let other_partner = *tpartner && rel.status == RelStatus::None;
                                    let mut p = crate::social::acceptance(s, &rel, tsim, tmood, other_partner);
                                    // People simply like the Attractive more.
                                    if s.cat != crate::social::SocialCat::Mean && crate::wishes::has(wishes, "Attractive") {
                                        p = (p + 0.12).min(0.98);
                                    }
                                    if !rand::rng().random_bool(p as f64) {
                                        if !action.autonomous {
                                            notes.push(format!("{} rejected {}'s attempt to {}.", tsim.first, sim.first, s.name.to_lowercase()));
                                        }
                                        let (f, r) = if s.cat == crate::social::SocialCat::Romantic { (-4.0, -8.0) } else { (-5.0, 0.0) };
                                        rels.add(*target, f, r);
                                        social_fx.push((*target, me, 0.0, 0.0, f, r));
                                        life.write(LifeEvent::new(me, LifeEventKind::Socialized { other: *target, social: "Argue" }));
                                        finished = true;
                                    }
                                }
                            }
                            ActionKind::GoHere(..) => finished = true,
                            ActionKind::PlantSeed { at, .. } => {
                                let to = *at - Vec2::new(tf.translation.x, tf.translation.z);
                                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                anim.pose = Pose::Use;
                                commands.entity(me).insert(crate::anim::ActionClip::new(Some("a2o_gardening_crouch_start_x"), &["a2o_gardening_crouch_plantSeeds_x"]));
                            }
                            ActionKind::Invite { .. }
                            | ActionKind::PhoneChat { .. }
                            | ActionKind::Retire
                            | ActionKind::BuyReward(_)
                            | ActionKind::EatHere
                            | ActionKind::EatItem { .. }
                            | ActionKind::OrderPizza
                            | ActionKind::Adopt { .. }
                            | ActionKind::MoveHouse
                            | ActionKind::CallRepairman
                            | ActionKind::HireMaid(_)
                            | ActionKind::MotiveFail(_)
                            | ActionKind::ThrowParty => {}
                            ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } => {
                                if let Ok((obj, otf, _, _)) = objects.get(*target) {
                                    tf.rotation = otf.rotation * Quat::from_rotation_y(std::f32::consts::PI);
                                    commands.entity(me).insert(repair_of(obj.kind).1);
                                }
                                anim.pose = Pose::Use;
                            }
                            ActionKind::Visit { lot, activity } => {
                                if let (Some(l), Some(name)) = (world.data.lots.get(*lot), world.data.lot_names.get(*lot)) {
                                    let acts = crate::rabbitholes::activities(l);
                                    let task = crate::opportunities::opportunity_task(opps, *activity)
                                        .map(|t| t.0)
                                        .or_else(|| (*activity == crate::gardening::SEEDS_TASK).then_some(&crate::gardening::BUY_SEEDS))
                                        .or_else(|| crate::meals::recipe_task(*activity));
                                    if crate::meals::recipe_task(*activity).is_some() {
                                        commands.entity(me).insert(crate::meals::BuyingRecipe(*activity - crate::meals::RECIPE_TASK));
                                    }
                                    if let Some(a) = task.or_else(|| acts.get(*activity)) {
                                        let place = crate::rabbitholes::lot_title(l, name);
                                        let price = crate::wishes::price_factor(wishes, a.name);
                                        crate::rabbitholes::head_out(&mut commands, &clock, me, sim, *lot, a, place, household.as_deref_mut(), &mut notes, price);
                                    }
                                }
                                finished = true;
                            }
                            ActionKind::GoToWork => {
                                if let Some(j) = job.as_deref_mut() {
                                    crate::careers::leave_for_work(&mut commands, &clock, me, sim, j, &mut notes);
                                }
                                finished = true;
                            }
                            ActionKind::GoToLot { lot } => {
                                crate::visit::drive_to(&mut commands, &clock, me, sim, *lot, crate::visit::place_name(&world.data, *lot), &mut notes);
                                finished = true;
                            }
                            ActionKind::Teleport { lot, .. } => {
                                crate::visit::teleport_to(&mut commands, &clock, me, sim, *lot, crate::visit::place_name(&world.data, *lot), &mut notes);
                                finished = true;
                            }
                            ActionKind::GoHomeFromLot => {
                                if let Some(v) = away {
                                    crate::visit::drive_home(&mut commands, &clock, me, v.lot, crate::visit::place_name(&world.data, v.lot));
                                }
                                finished = true;
                            }
                            ActionKind::JoinCareer { target, .. } => {
                                if let Ok((_, otf, _, _)) = objects.get(*target) {
                                    tf.rotation = otf.rotation * Quat::from_rotation_y(std::f32::consts::PI);
                                }
                                anim.pose = Pose::Use;
                                commands.entity(me).insert(crate::anim::ActionClip::new(None, &["a2o_computer_chess_type_loop_x"]));
                            }
                        }
                    } else if path.is_none() {
                        action.phase = Phase::Start;
                    }
                }
                Phase::Running(elapsed) => {
                    let elapsed = elapsed + dt;
                    action.phase = Phase::Running(elapsed);
                    match &action.kind {
                        ActionKind::Object { target, def } => {
                            if let Ok((obj, otf, mut used, of)) = objects.get_mut(*target) {
                                let d = &interactions_for(obj.kind)[*def];
                                let affinity = crate::life::activity_affinity(&sim.traits, d.name);
                                for i in 0..6 {
                                    let mut gain = d.per_hour[i];
                                    if i == HUNGER && matches!(d.special, Special::ServeMeal | Special::GrabPlate) {
                                        gain = 0.0;
                                    }
                                    if i == FUN && gain > 0.0 {
                                        gain *= affinity;
                                    } else if i == FUN && affinity < 0.5 {
                                        gain -= 20.0;
                                    }
                                    if i == ENERGY && d.until_full == Some(ENERGY) {
                                        gain *= crate::life::sleep_rate(&sim.traits);
                                    }
                                    gain = crate::upgrades::boost(obj.kind, upgraded.get(*target).ok(), i, gain);
                                    motives.add(i, gain * dt / 60.0);
                                }
                                // Working out builds fitness and burns off weight.
                                if d.pose == Pose::Exercise {
                                    let h = dt / 60.0;
                                    // (Faster for a Fast Metabolism.)
                                    let burn = if crate::wishes::has(wishes, "FastMetabolism") { 2.0 } else { 1.0 };
                                    commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| crate::aging::reshape(&mut e, -0.03 * h * burn, 0.05 * h));
                                }
                                if let Some(sk) = d.skill {
                                    let rate = crate::life::skill_rate(&sim.traits, sk) * crate::wishes::reward_skill_rate(wishes);
                                    let e = skills.0.entry(sk).or_insert(0.0);
                                    let before = *e as u32;
                                    *e = (*e + dt / 60.0 * 0.6 * rate / (1.0 + *e * 0.25)).min(10.0);
                                    if *e as u32 > before {
                                        notes.push(format!("{} reached level {} in {}!", sim.first, *e as u32, sk));
                                        life.write(LifeEvent::new(me, LifeEventKind::SkillUp { skill: sk, level: *e as u32 }));
                                    }
                                }
                                let full = d.until_full.is_some_and(|m| motives.0[m] >= 98.0);
                                // (A painting takes as long as its canvas.)
                                let minutes = match d.special {
                                    Special::SellPainting => crate::paintings::CANVAS_MINUTES[crate::paintings::canvas(plan, skills.level("Painting")) as usize],
                                    // (Twice as quick for a Speedy Cleaner; homework too for a Multi-Tasker.)
                                    Special::CleanUp | Special::EmptyTrash if crate::wishes::has(wishes, "SpeedyCleaner") => d.minutes * 0.5,
                                    Special::Homework if crate::wishes::has(wishes, "MultiTasker") => d.minutes * 0.5,
                                    _ => d.minutes,
                                };
                                if elapsed >= minutes || full {
                                    finished = true;
                                    life.write(LifeEvent::new(me, LifeEventKind::Finished { activity: d.name, completed: true }));
                                    used.0 = None;
                                    let unbreakable = upgraded.get(*target).is_ok_and(|u| u.has(crate::upgrades::Upgrade::Unbreakable));
                                    if !unbreakable && rand::rng().random_bool(break_chance(obj.kind, obj.price)) {
                                        commands.entity(*target).insert(Broken);
                                        let what = if obj.kind == ObjectKind::Toilet { "is clogged" } else { "broke" };
                                        notes.push(format!("Oh no! {} {what}. Repair it, or call the repairman.", upper_first(&the(&obj.name))));
                                    }
                                    if d.on_object {
                                        stand_up_at = Some(obj.use_point(otf));
                                    }
                                    match d.special {
                                        // (Getting out of the pool is the swim module's.)
                                        Special::FindJob | Special::Swim | Special::WriteNovel | Special::PlayInSprinkler | Special::Teleport | Special::WashDishes => {}
                                        Special::Homework => {
                                            notes.push(format!("{} finished their homework.", sim.first));
                                            commands.entity(me).remove::<crate::rabbitholes::Homework>().queue_silenced(|mut e: EntityWorldMut| {
                                                if let Some(mut g) = e.get_mut::<crate::rabbitholes::SchoolGrades>() {
                                                    g.0 = (g.0 + 6.0).min(100.0);
                                                }
                                            });
                                        }
                                        Special::QuitJob => {
                                            if job.is_some() {
                                                commands.entity(me).remove::<Job>();
                                                notes.push(format!("{} quit their job.", sim.first));
                                            }
                                        }
                                        Special::ChessMatch => {
                                            commands.entity(me).insert(crate::chess::MatchPlayed);
                                        }
                                        Special::SellPainting => {
                                            // One of the game's pictures, for their canvas, skill and traits.
                                            let lvl = skills.level("Painting");
                                            let size = crate::paintings::canvas(plan, lvl);
                                            // (The one on the easel, decided as they started.)
                                            let p = plan.and_then(|p| p.painted.clone()).unwrap_or_else(|| {
                                                crate::paintings::paint(
                                                    baked.as_ref().map(|b| &b.0.paintings),
                                                    size,
                                                    lvl,
                                                    &sim.traits,
                                                    sim.age == Age::Child,
                                                    crate::wishes::has(wishes, "ExtraCreative"),
                                                    &mut rand::rng(),
                                                )
                                            });
                                            commands.entity(me).remove::<crate::paintings::PaintPlan>();
                                            // Into their inventory, to sell, keep or hang.
                                            crate::inventory::give(&mut commands, me, crate::inventory::ItemKind::Painting, p.key, p.name.to_string(), 0, p.worth, 1);
                                            notes.push(format!("{} finished a painting ({}, worth §{}). It's in their inventory.", sim.first, p.name.to_lowercase(), p.worth));
                                        }
                                        Special::ChangeClothes => {
                                            // Into the outfit chosen, kept on until it's time for another.
                                            commands.entity(me).queue_silenced(|mut e: EntityWorldMut| {
                                                use crate::simbody::{ChangeIntoPlan, ChangedInto, OutfitKind, Wearing};
                                                let Some(ChangeIntoPlan(kind)) = e.take::<ChangeIntoPlan>() else { return };
                                                if kind == OutfitKind::Everyday {
                                                    e.remove::<(Wearing, ChangedInto)>();
                                                } else {
                                                    e.insert((Wearing(kind), ChangedInto));
                                                }
                                                e.insert(crate::aging::NeedsNewBody);
                                            });
                                        }
                                        Special::ToggleAlarm => {
                                            commands.queue(|w: &mut World| {
                                                let mut a = w.resource_mut::<crate::appliances::Alarm>();
                                                a.on = !a.on;
                                                let msg = if a.on { "The alarm is set: it'll wake the household for work and school." } else { "The alarm is off." };
                                                w.resource_mut::<Notifications>().push(msg.to_string());
                                            });
                                        }
                                        Special::PlanOutfit => {
                                            commands.insert_resource(crate::planner::OutfitPlanner::open(me));
                                        }
                                        Special::ChangeAppearance => {
                                            commands.insert_resource(crate::planner::OutfitPlanner::looks(me));
                                        }
                                        Special::ReplicateFood => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Replicated);
                                            notes.push(format!("{} replicated a meal.", sim.first));
                                        }
                                        Special::Sculpt => {
                                            let (weight, fit) = match d.name {
                                                "Sculpt Fitter" => (-0.15, 0.5),
                                                "Sculpt Slimmer" => (-0.5, 0.0),
                                                _ => (0.5, 0.0),
                                            };
                                            commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| crate::aging::reshape(&mut e, weight, fit));
                                            notes.push(format!("{} has a new shape, courtesy of the Body Sculptor.", sim.first));
                                        }
                                        Special::SetMood => {
                                            use crate::life::MoodletKind as M;
                                            let mood = match d.name {
                                                "Feel Flirty" => M::Flirty,
                                                "Feel Inspired" => M::Inspired,
                                                "Feel Pumped" => M::Pumped,
                                                "Feel Like Having Fun" => M::HavingFun,
                                                _ => M::WellRested,
                                            };
                                            commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| {
                                                let now = e.world().resource::<crate::clock::GameClock>().minutes;
                                                if let Some(mut m) = e.get_mut::<crate::life::Moodlets>() {
                                                    m.add(mood, now);
                                                }
                                            });
                                        }
                                        Special::ServeMeal => {
                                            // A poor cook may set the stove on fire instead.
                                            let clumsy = sim.traits.contains(&crate::life::Trait::Clumsy);
                                            if crate::fire::cooking_fire(skills.level("Cooking"), clumsy) {
                                                fire.write(crate::fire::StartFire { at: otf.translation + Vec3::Y * obj.height * 0.85, level: of.map_or(1, |f| f.0) });
                                                notes.push(format!("{} set the stove on fire!", sim.first));
                                            } else {
                                                commands.entity(me).insert(crate::meals::MealRequest::Serve(*target));
                                            }
                                        }
                                        Special::GrabPlate => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Grabbed(*target));
                                        }
                                        Special::SprinklerOn => {
                                            commands.entity(*target).insert(crate::gardening::Sprinkling { until: clock.minutes + 120.0 });
                                        }
                                        Special::SprinklerOff => {
                                            commands.entity(*target).remove::<crate::gardening::Sprinkling>();
                                        }
                                        Special::PutAway => {
                                            commands.entity(me).insert(crate::meals::MealRequest::PutAway(*target));
                                        }
                                        Special::Leftovers => {
                                            commands.entity(me).insert(crate::meals::MealRequest::FromFridge);
                                        }
                                        Special::BakeCake => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Cake(*target));
                                        }
                                        Special::BlowOutCandles => {
                                            commands.entity(me).insert(crate::aging::GrowUpNow);
                                            commands.entity(*target).insert(crate::meals::CakeBlownOut);
                                        }
                                        Special::EatMeal => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Ate);
                                        }
                                        Special::CleanUp => {
                                            commands.entity(*target).try_despawn();
                                            // (Scraps in the trash; the dish to the dishwasher or sink.)
                                            commands.entity(me).insert((crate::surroundings::Discarded, crate::surroundings::WashUp));
                                        }
                                        Special::PlaceFish => {
                                            commands.entity(me).insert(crate::fishbowl::BowlRequest::Place(*target));
                                        }
                                        Special::TakeFish => {
                                            commands.entity(me).insert(crate::fishbowl::BowlRequest::Take(*target));
                                        }
                                        Special::EmptyTrash => {
                                            commands.entity(*target).insert(crate::surroundings::TrashFill(0));
                                        }
                                        Special::ReadPaper => {
                                            commands.entity(*target).try_despawn();
                                            commands.entity(me).insert(crate::story::ReadTheNews);
                                        }
                                        Special::Water => {
                                            commands.entity(me).insert(crate::gardening::GardenRequest::Water(*target));
                                        }
                                        Special::Weed => {
                                            commands.entity(me).insert(crate::gardening::GardenRequest::Weed(*target));
                                        }
                                        Special::Harvest => {
                                            commands.entity(me).insert(crate::gardening::GardenRequest::Harvest(*target));
                                        }
                                        Special::Collect => {
                                            commands.entity(me).insert(crate::collecting::CollectRequest::Pick(*target));
                                        }
                                        Special::Fish => {
                                            commands.entity(me).insert(crate::collecting::CollectRequest::Fished { spot: *target, minutes: d.minutes });
                                        }
                                        Special::Catch => {
                                            commands.entity(me).insert(crate::collecting::CollectRequest::Caught(*target));
                                        }
                                        Special::LightFire | Special::PutOutFire => {
                                            let (t, light) = (*target, d.special == Special::LightFire);
                                            commands.queue(move |w: &mut World| {
                                                w.write_message(if light { crate::fireplace::FireplaceRequest::Light(t) } else { crate::fireplace::FireplaceRequest::PutOut(t) });
                                            });
                                        }
                                        Special::PayBills => {
                                            if let Some(h) = household.as_mut() {
                                                let due: i64 = h.bills.iter().map(|b| b.amount).sum();
                                                if due == 0 {
                                                    notes.push("There are no bills to pay.");
                                                } else if h.funds >= due {
                                                    h.funds -= due;
                                                    h.bills.clear();
                                                    notes.push(format!("{} paid the bills: §{due}.", sim.first));
                                                } else {
                                                    notes.push(format!("There isn't enough money to pay the bills (§{due})."));
                                                }
                                            }
                                        }
                                        Special::Cook | Special::None => {}
                                    }
                                }
                            } else {
                                finished = true;
                            }
                        }
                        ActionKind::Social { target, social } => {
                            let s = &SOCIALS[*social];
                            let close = positions.get(target).is_some_and(|p| p.0.distance(tf.translation) < 2.5);
                            if !close {
                                action.phase = Phase::Start;
                            } else {
                                motives.add(SOCIAL, s.social_per_hour * dt / 60.0);
                                motives.add(FUN, s.fun_per_hour * dt / 60.0);
                                let k = dt / s.minutes * crate::life::social_affinity(&sim.traits, s.name);
                                let (f, r) = (s.friendship * k, s.romance * k);
                                rels.add(*target, f, r);
                                if s.cat == crate::social::SocialCat::Care {
                                    care_fx.push((*target, s.care.map(|v| v * dt / 60.0)));
                                }
                                social_fx.push((*target, me, s.social_per_hour * dt / 60.0, s.fun_per_hour * dt / 60.0, f, r));
                                anim.pose = if s.name.contains("Dance") { Pose::Dance } else { Pose::Talk };
                                if elapsed >= s.minutes {
                                    finished = true;
                                    life.write(LifeEvent::new(me, LifeEventKind::Socialized { other: *target, social: s.name }));
                                    life.write(LifeEvent::new(*target, LifeEventKind::Socialized { other: me, social: s.name }));
                                    let rel = rels.get(*target);
                                    let tname = who.get(target).map(|w| w.0.first.clone()).unwrap_or_default();
                                    let t_member = who.get(target).is_some_and(|w| w.2);
                                    match s.effect {
                                        SocialEffect::Kiss if !rel.kissed => {
                                            status_fx.push((me, *target, None, true));
                                            life.write(LifeEvent::new(me, LifeEventKind::FirstKiss));
                                            life.write(LifeEvent::new(*target, LifeEventKind::FirstKiss));
                                            notes.push(format!("{} and {} shared their first kiss!", sim.first, tname));
                                        }
                                        SocialEffect::GoSteady => {
                                            status_fx.push((me, *target, Some(RelStatus::Partner), false));
                                            life.write(LifeEvent::new(me, LifeEventKind::StartedDating));
                                            life.write(LifeEvent::new(*target, LifeEventKind::StartedDating));
                                            notes.push(format!("{} and {} are now going steady!", sim.first, tname));
                                        }
                                        SocialEffect::Propose => {
                                            status_fx.push((me, *target, Some(RelStatus::Engaged), false));
                                            life.write(LifeEvent::new(me, LifeEventKind::Engaged));
                                            life.write(LifeEvent::new(*target, LifeEventKind::Engaged));
                                            notes.push(format!("{} proposed to {}, who said yes!", sim.first, tname));
                                        }
                                        SocialEffect::Marry => {
                                            status_fx.push((me, *target, Some(RelStatus::Married), false));
                                            life.write(LifeEvent::new(me, LifeEventKind::Married));
                                            life.write(LifeEvent::new(*target, LifeEventKind::Married));
                                            notes.push(format!("{} and {} got married!", sim.first, tname));
                                            if !t_member {
                                                commands.entity(*target).insert(JoinHousehold { last_name: Some(sim.last.clone()) });
                                            }
                                        }
                                        SocialEffect::BreakUp => {
                                            status_fx.push((me, *target, Some(RelStatus::Ex), false));
                                            life.write(LifeEvent::new(me, LifeEventKind::BrokeUp));
                                            life.write(LifeEvent::new(*target, LifeEventKind::BrokeUp));
                                            notes.push(format!("{} broke up with {}.", sim.first, tname));
                                        }
                                        SocialEffect::MoveIn => {
                                            commands.entity(*target).insert(JoinHousehold { last_name: None });
                                            notes.push(format!("{} moved in with the household!", tname));
                                        }
                                        SocialEffect::TryForBaby => {
                                            conceive.write(crate::little::Conceive { a: me, b: *target });
                                        }
                                        SocialEffect::PutToBed => {
                                            commands.entity(*target).insert(crate::little::Bedtime);
                                        }
                                        SocialEffect::Fight => {
                                            // The stronger and braver usually win.
                                            let them = who.get(target).map(|w| w.0.clone());
                                            let traits = |s: &Sim| {
                                                [crate::life::Trait::Athletic, crate::life::Trait::Brave].iter().filter(|t| s.traits.contains(t)).count() as f32 * 0.12
                                                    - if s.traits.contains(&crate::life::Trait::Coward) { 0.15 } else { 0.0 }
                                            };
                                            let p = 0.5 + skills.level("Athletic") as f32 * 0.035 + traits(sim) - them.as_ref().map_or(0.0, traits);
                                            let won = rand::rng().random_bool(p.clamp(0.1, 0.9) as f64);
                                            let (winner, loser) = if won { (me, *target) } else { (*target, me) };
                                            let (wn, ln) = if won { (sim.first.clone(), tname.clone()) } else { (tname.clone(), sim.first.clone()) };
                                            notes.push(format!("{wn} won the fight with {ln}!"));
                                            let now = clock.minutes;
                                            for (e, kind) in [(winner, crate::life::MoodletKind::Pumped), (loser, crate::life::MoodletKind::Embarrassed)] {
                                                commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| {
                                                    if let Some(mut m) = w.get_mut::<crate::life::Moodlets>() {
                                                        m.add(kind, now);
                                                    }
                                                });
                                            }
                                        }
                                        SocialEffect::Apologize => {
                                            notes.push(format!("{} apologized to {}.", sim.first, tname));
                                        }
                                        SocialEffect::DeclareNemesis => {
                                            notes.push(format!("{} declared {} their nemesis!", sim.first, tname));
                                        }
                                        SocialEffect::CheerUp | SocialEffect::BackRub => {
                                            let now = clock.minutes;
                                            let kind = if s.effect == SocialEffect::BackRub { crate::life::MoodletKind::Comfy } else { crate::life::MoodletKind::GoodConversation };
                                            commands.entity(*target).queue_silenced(move |mut w: EntityWorldMut| {
                                                if let Some(mut m) = w.get_mut::<crate::life::Moodlets>() {
                                                    m.add(kind, now);
                                                }
                                            });
                                        }
                                        SocialEffect::TeachWalk | SocialEffect::TeachTalk => {
                                            let (toddler, teacher, walk) = (*target, me, s.effect == SocialEffect::TeachWalk);
                                            commands.queue(move |w: &mut World| {
                                                w.write_message(crate::little::Lesson { toddler, teacher, walk });
                                            });
                                        }
                                        SocialEffect::AskToLeave => {
                                            commands.entity(*target).insert(GoingHome);
                                            notes.push(format!("{} said goodbye and is heading home.", tname));
                                        }
                                        SocialEffect::AskOnDate => {
                                            let (a, b) = (me, *target);
                                            commands.queue(move |w: &mut World| {
                                                w.write_message(crate::dates::StartDate { a, b });
                                            });
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        ActionKind::PlantSeed { at, plant, .. } => {
                            if elapsed >= 20.0 {
                                finished = true;
                                let y = ground(floor.0, at.x, at.y);
                                commands.entity(me).insert(crate::gardening::GardenRequest::Plant { at: Vec3::new(at.x, y, at.y), plant: *plant });
                            }
                        }
                        ActionKind::Repair { target } => {
                            let handy = skills.level("Handiness") as f32 + if sim.traits.contains(&crate::life::Trait::Handy) { 3.0 } else { 0.0 };
                            let minutes = 90.0 / (1.0 + handy * 0.35);
                            let e = skills.0.entry("Handiness").or_insert(0.0);
                            let before = *e as u32;
                            *e = (*e + dt / 60.0 * 0.5 * crate::life::skill_rate(&sim.traits, "Handiness") / (1.0 + *e * 0.25)).min(10.0);
                            if *e as u32 > before {
                                notes.push(format!("{} reached level {} in Handiness!", sim.first, *e as u32));
                                life.write(LifeEvent::new(me, LifeEventKind::SkillUp { skill: "Handiness", level: *e as u32 }));
                            }
                            motives.add(FUN, -4.0 * dt / 60.0);
                            if elapsed >= minutes {
                                finished = true;
                                // Electronics can shock the unskilled; a second shock while
                                // still singed stops their heart.
                                let electric = objects.get(*target).is_ok_and(|(o, ..)| matches!(o.kind, ObjectKind::Tv | ObjectKind::Computer | ObjectKind::Stereo));
                                let shocked = electric && rand::rng().random_bool(((0.35 - handy * 0.04) as f64).clamp(0.0, 1.0));
                                if let Ok((o, _, mut used, _)) = objects.get_mut(*target) {
                                    used.0 = None;
                                    if shocked {
                                        commands.entity(me).insert(crate::death::Shocked);
                                        notes.push(format!("{} was electrocuted trying to fix {}!", sim.first, the(&o.name)));
                                    } else {
                                        commands.entity(*target).remove::<Broken>();
                                        notes.push(format!("{} fixed {}.", sim.first, the(&o.name)));
                                    }
                                }
                            }
                        }
                        ActionKind::Upgrade { target, bit } => {
                            let handy = skills.level("Handiness") as f32 + if sim.traits.contains(&crate::life::Trait::Handy) { 3.0 } else { 0.0 };
                            let minutes = crate::upgrades::Upgrade::MINUTES / (1.0 + handy * 0.35);
                            let e = skills.0.entry("Handiness").or_insert(0.0);
                            let before = *e as u32;
                            *e = (*e + dt / 60.0 * 0.6 * crate::life::skill_rate(&sim.traits, "Handiness") / (1.0 + *e * 0.25)).min(10.0);
                            if *e as u32 > before {
                                notes.push(format!("{} reached level {} in Handiness!", sim.first, *e as u32));
                                life.write(LifeEvent::new(me, LifeEventKind::SkillUp { skill: "Handiness", level: *e as u32 }));
                            }
                            motives.add(FUN, -3.0 * dt / 60.0);
                            if elapsed >= minutes {
                                finished = true;
                                let electric = objects.get(*target).is_ok_and(|(o, ..)| matches!(o.kind, ObjectKind::Tv | ObjectKind::Computer | ObjectKind::Stereo));
                                let shocked = electric && rand::rng().random_bool(((0.25 - handy * 0.03) as f64).clamp(0.0, 1.0));
                                if let Ok((o, _, mut used, _)) = objects.get_mut(*target) {
                                    used.0 = None;
                                    let name = crate::upgrades::Upgrade::from_bit(*bit).and_then(|u| u.name(o.kind)).unwrap_or("upgrade");
                                    if shocked {
                                        commands.entity(me).insert(crate::death::Shocked);
                                        notes.push(format!("{} was electrocuted upgrading {}!", sim.first, the(&o.name)));
                                    } else {
                                        let had = upgraded.get(*target).map_or(0, |u| u.0);
                                        commands.entity(*target).insert(crate::upgrades::Upgrades(had | *bit));
                                        notes.push(format!("{} upgraded {}: {name}.", sim.first, the(&o.name)));
                                    }
                                }
                            }
                        }
                        ActionKind::ThrowParty => {
                            if elapsed >= 10.0 {
                                finished = true;
                                commands.insert_resource(PartyPlan { by: me, start: clock.minutes + 120.0 });
                            }
                        }
                        ActionKind::MotiveFail(k) => {
                            const ACCIDENT: f32 = 4.0;
                            // (Asleep on the floor a couple of hours, then up again, still tired.)
                            const FLOOR: f32 = 150.0;
                            const UP: f32 = 4.0;
                            if *k == 0 {
                                finished = elapsed >= ACCIDENT;
                            } else {
                                if elapsed < FLOOR {
                                    motives.add(ENERGY, 0.45 * dt);
                                } else if elapsed - dt < FLOOR {
                                    commands.entity(me).insert(crate::anim::ActionClip::new(None, &["a_sleeponFloor_getUp_x"]));
                                }
                                finished = elapsed >= FLOOR + UP;
                            }
                        }
                        ActionKind::CallRepairman => {
                            if elapsed >= 5.0 {
                                finished = true;
                                commands.insert_resource(RepairmanVisit { arrive_at: clock.minutes + 90.0 });
                                notes.push(format!("{} called the repairman. He'll be by soon (§{REPAIRMAN_PRICE}).", sim.first));
                            }
                        }
                        ActionKind::HireMaid(hire) => {
                            if elapsed >= 5.0 {
                                finished = true;
                                let hire = *hire;
                                commands.queue(move |w: &mut World| w.resource_mut::<crate::services::MaidService>().hired = hire);
                                notes.push(if hire {
                                    format!("{} hired a maid. She'll come every morning at nine (§{} an hour).", sim.first, crate::services::MAID_WAGE)
                                } else {
                                    format!("{} let the maid go.", sim.first)
                                });
                            }
                        }
                        ActionKind::MoveHouse => {
                            if elapsed >= 5.0 {
                                finished = true;
                                commands.insert_resource(crate::home::MoveRequested { at: clock.minutes });
                                notes.push(format!("{} called about moving. Choose the household's new home.", sim.first));
                            }
                        }
                        ActionKind::Adopt { age, female } => {
                            if elapsed >= 5.0 {
                                finished = true;
                                let age = [Age::Baby, Age::Toddler, Age::Child][(*age as usize).min(2)];
                                // The parents-to-be: whoever called, and their spouse.
                                let mut parents = vec![(sim.id, sim.full_name(), sim.female)];
                                if let Some((_, s, ..)) = rels.0.iter().find(|(_, r)| r.status == RelStatus::Married).and_then(|(e, _)| people.get(*e).ok()) {
                                    parents.push((s.id, s.full_name(), s.female));
                                }
                                commands.insert_resource(crate::services::AdoptionOrder { arrive_at: clock.minutes + 120.0, age, female: *female, parents });
                                notes.push(format!(
                                    "{} called the adoption agency. The social worker will bring {} home within a couple of hours.",
                                    sim.first,
                                    crate::services::adoptee(age, *female)
                                ));
                            }
                        }
                        ActionKind::OrderPizza => {
                            if elapsed >= 5.0 {
                                finished = true;
                                match household.as_deref_mut() {
                                    Some(h) if h.funds >= crate::meals::PIZZA_PRICE => {
                                        h.funds -= crate::meals::PIZZA_PRICE;
                                        commands.insert_resource(crate::meals::PizzaOrder { arrive_at: clock.minutes + 60.0 });
                                        notes.push(format!("{} ordered a pizza (§{}). It'll be here within the hour.", sim.first, crate::meals::PIZZA_PRICE));
                                    }
                                    _ => notes.push("There isn't enough money for a pizza."),
                                }
                            }
                        }
                        ActionKind::Retire => {
                            if elapsed >= 10.0 {
                                finished = true;
                                if let Some(j) = job.as_ref() {
                                    let daily = j.pension();
                                    notes.push(format!(
                                        "{} retired from the {} career after years as a {}, and will be paid a pension of §{daily} a day.",
                                        sim.first,
                                        j.career().name,
                                        j.info().title
                                    ));
                                    commands.entity(me).remove::<Job>().insert(crate::careers::Pension { daily, paid: clock.day() });
                                }
                            }
                        }
                        ActionKind::PhoneChat { target } => {
                            // A chat on the phone: a little less than one in person.
                            let (s, f) = (PHONE_CHAT_SOCIAL * dt / 60.0, PHONE_CHAT_FUN * dt / 60.0);
                            motives.add(SOCIAL, s);
                            motives.add(FUN, f);
                            let df = PHONE_CHAT_FRIENDSHIP * dt / PHONE_CHAT_MINUTES * crate::life::social_affinity(&sim.traits, "Chat");
                            rels.add(*target, df, 0.0);
                            social_fx.push((*target, me, s, f, df, 0.0));
                            if elapsed >= PHONE_CHAT_MINUTES {
                                finished = true;
                                life.write(LifeEvent::new(me, LifeEventKind::Socialized { other: *target, social: "Chat" }));
                                life.write(LifeEvent::new(*target, LifeEventKind::Socialized { other: me, social: "Chat" }));
                            }
                        }
                        ActionKind::Invite { target } => {
                            if elapsed >= 8.0 {
                                finished = true;
                                let tname = who.get(target).map(|w| w.0.first.clone()).unwrap_or_default();
                                commands.entity(*target).insert(Invited { arrive_at: clock.minutes + 45.0 });
                                notes.push(format!("{} invited {} over. They'll be here soon.", sim.first, tname));
                            }
                        }
                        ActionKind::JoinCareer { track, .. } => {
                            if elapsed >= 15.0 {
                                finished = true;
                                let j = Job::new(*track);
                                let info = j.info();
                                notes.push(format!(
                                    "{} joined the {} career as a {} (§{}/hr, {}–{}).",
                                    sim.first,
                                    j.career().name,
                                    info.title,
                                    info.hourly,
                                    hour_label(info.start),
                                    hour_label(info.end)
                                ));
                                commands.entity(me).insert(j);
                                life.write(LifeEvent::new(me, LifeEventKind::NewJob));
                            }
                        }
                        ActionKind::EatHere => {
                            motives.add(HUNGER, MEAL_PER_HOUR * dt / 60.0);
                            if elapsed >= MEAL_MINUTES * 0.8 {
                                finished = true;
                                commands.entity(me).insert(crate::meals::MealRequest::AteStanding);
                            }
                        }
                        ActionKind::EatItem { key, quality } => {
                            // A snack's worth, a little more for finer produce.
                            motives.add(HUNGER, (150.0 + *quality as f32 * 12.0) * dt / 60.0);
                            if elapsed >= PRODUCE_MINUTES {
                                finished = true;
                                let (key, quality) = (key.clone(), *quality);
                                commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| {
                                    if let Some(mut inv) = e.get_mut::<crate::inventory::Inventory>()
                                        && let Some(i) = inv.0.iter().position(|s| s.kind == crate::inventory::ItemKind::Produce && s.key == key && s.quality == quality)
                                    {
                                        inv.take_one(i);
                                    }
                                });
                            }
                        }
                        _ => finished = true,
                    }
                }
            }
        }

        if finished {
            if let Some(a) = queue.0.front() {
                debug!("{} finished {} ({:?}{})", sim.first, a.label, a.phase, if a.cancel { ", cancelled" } else { "" });
            }
            commands.entity(me).remove::<crate::anim::ActionClip>();
            if let Some(Action { kind: ActionKind::Social { target, .. }, .. }) = queue.0.front() {
                partners_done.push((*target, me));
            }
            queue.0.pop_front();
            *decay = DecayScale::default();
            anim.pose = Pose::Stand;
            anim.seat_height = 0.0;
            commands.entity(me).remove::<PathFollow>();
            if let Some(p) = stand_up_at {
                tf.translation = Vec3::new(p.x, ground(floor.0, p.x, p.y), p.y);
            }
        }
    }

    for (target, actor) in partners_done {
        commands.entity(target).queue_silenced(move |mut e: EntityWorldMut| {
            if e.get::<SocialPartner>().is_some_and(|p| p.0 == actor) {
                e.remove::<(SocialPartner, crate::anim::ActionClip)>();
            }
        });
    }
    for (a, b, status, kissed) in status_fx {
        for (x, y) in [(a, b), (b, a)] {
            if let Ok(mut q) = sims.get_mut(x) {
                let r = q.8.entry(y);
                if let Some(s) = status {
                    r.status = s;
                    if s == RelStatus::Ex {
                        r.romance = (r.romance - 60.0).max(-100.0);
                    }
                }
                r.kissed |= kissed;
            }
        }
    }
    for (target, care) in care_fx {
        if let Ok(mut q) = sims.get_mut(target) {
            for (i, v) in care.into_iter().enumerate() {
                q.4.add(i, v);
            }
        }
    }
    // Babies go into the grown-up's arms; toddlers stand facing the grown-up, whose clips
    // then lift them.
    for (little, by, age, at, carried_social) in carry_fx {
        debug!("care: {by} looks after {little} ({age:?})");
        if age == Age::Baby {
            if carried_social {
                commands.entity(little).insert(crate::little::Carried { by });
            }
        } else if let Ok(mut q) = sims.get_mut(little) {
            // The toddler stops what they were doing and stays with the grown-up.
            q.2.0.clear();
            commands.entity(little).remove::<PathFollow>();
            let fwd = at.rotation * Vec3::Z;
            q.3.translation = at.translation + fwd * 0.6;
            q.3.rotation = at.rotation * Quat::from_rotation_y(std::f32::consts::PI);
        }
    }
    for (target, actor, social, fun, friendship, romance) in social_fx {
        if let Ok((_, tsim, queue, mut tf, mut motives, _, mut anim, _, mut rels, path, _, _, _, _, _)) = sims.get_mut(target) {
            motives.add(SOCIAL, social);
            motives.add(FUN, fun);
            rels.add(actor, friendship, romance);
            // (A baby in someone's arms is placed by the carry slot.)
            if queue.0.is_empty() && path.is_none() && tsim.age != Age::Baby {
                anim.pose = Pose::Talk;
                if let Some(p) = positions.get(&actor) {
                    let to = Vec2::new(p.0.x - tf.translation.x, p.0.z - tf.translation.z);
                    tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                }
            }
        }
    }
}

pub fn hour_label(h: f32) -> String {
    let h = h as u32;
    match h {
        0 => "12 AM".into(),
        1..=11 => format!("{h} AM"),
        12 => "12 PM".into(),
        _ => format!("{} PM", h - 12),
    }
}

/// Sims with free will pick something to satisfy their lowest needs when idle.
#[allow(clippy::type_complexity)]
fn autonomy(
    delta: Res<SimDelta>,
    clock: Res<GameClock>,
    (settings, household): (Res<crate::options::Settings>, Query<(), With<HouseholdMember>>),
    mut sims: Query<
        (Entity, &Transform, &Motives, &mut ActionQueue, &mut AutonomyTimer, &Relationships, Option<&Job>, &Sim, Has<SocialPartner>, Option<&crate::visit::OnLot>),
        (Without<AtWork>, Without<crate::rabbitholes::AtRabbitHole>, Without<OffLot>),
    >,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy, Option<&crate::visit::LotObject>)>,
    hh: Option<Res<Household>>,
    (broken, plant_q, lit_q, hw_q): (Query<(), With<Broken>>, Query<&crate::gardening::GrowingPlant>, Query<(), With<crate::fireplace::Lit>>, Query<(), With<crate::rabbitholes::Homework>>),
    (party_on, trash_q, served_q, leftovers, sprinkling, route_failed): (
        Option<Res<Party>>,
        Query<&crate::surroundings::TrashFill>,
        Query<&crate::surroundings::ServedAt>,
        Res<crate::meals::Leftovers>,
        Query<(), With<crate::gardening::Sprinkling>>,
        Query<&RouteFailed>,
    ),
    (called, repairmen): (Option<Res<RepairmanVisit>>, Query<(), With<crate::services::Repairman>>),
    friends_away: Query<(Entity, &Sim), (With<OffLot>, Without<Invited>)>,
) {
    if delta.0 <= 0.0 {
        return;
    }
    // (The repairman's been called: repairs are left to him.)
    let repairman = called.is_some() || !repairmen.is_empty();
    let others: Vec<(Entity, Vec3, Age, [f32; 6], Option<usize>)> = sims.iter().map(|s| (s.0, s.1.translation, s.7.age, s.2.0, s.9.map(|l| l.0))).collect();
    // A meal already out is eaten before anyone cooks another.
    let meal_out = objects.iter().any(|(_, o, ..)| o.kind == ObjectKind::Meal);
    let bills_due = hh.is_some_and(|h| !h.bills.is_empty());
    let mut rng = rand::rng();
    for (me, tf, motives, mut queue, mut timer, rels, job, sim, partner, on_lot) in &mut sims {
        let my_lot = on_lot.map(|l| l.0);
        timer.0 -= delta.0;
        // (Someone else's social partner waits for them to finish.)
        if timer.0 > 0.0 || !queue.0.is_empty() || sim.age == Age::Baby || partner {
            continue;
        }
        let free_will = if household.contains(me) { settings.free_will } else { crate::options::FreeWill::Normal };
        timer.0 = match free_will {
            crate::options::FreeWill::High => rng.random_range(2.0..5.0),
            _ => rng.random_range(4.0..10.0),
        };
        if free_will == crate::options::FreeWill::Off {
            continue;
        }
        // Don't start long activities right before work.
        if let Some(j) = job {
            let h = clock.hour_f();
            let start = j.info().start;
            if j.works_on(clock.weekday()) && h > start - 1.2 && h < start {
                continue;
            }
        }
        let urgency = |i: usize| {
            let v = motives.0[i];
            let u = ((100.0 - v) / 200.0).clamp(0.0, 1.0);
            u * u * u * 4.0 + u * 0.3
        };
        let mut best: Option<(f32, Action)> = None;
        for (oe, obj, otf, used, obj_lot) in &objects {
            if used.0.is_some_and(|u| u != me) || obj_lot.map(|l| l.0) != my_lot {
                continue;
            }
            if !obj.kind.usable_by(sim.age) {
                continue;
            }
            // Visitors to the family's home don't sleep in its beds or bathe there.
            if my_lot.is_none() && !household.contains(me) && matches!(obj.kind, ObjectKind::BedSingle | ObjectKind::BedDouble | ObjectKind::Shower | ObjectKind::Bathtub) {
                continue;
            }
            let dist = obj.world_center(otf).distance(tf.translation);
            if broken.contains(oe) {
                if used.0.is_some_and(|u| u != me) {
                    continue;
                }
                // Handy (or neat) household Sims see to repairs on their own; and anyone who
                // badly needs the bathroom unclogs the toilet (the plunger needs no skill).
                let keen = sim.traits.contains(&crate::life::Trait::Handy) || sim.traits.contains(&crate::life::Trait::Neat);
                let desperate = obj.kind == ObjectKind::Toilet && motives.0[BLADDER] < -20.0;
                // (At home: the town's own things are the town's to fix.)
                if household.contains(me) && my_lot.is_none() && sim.age.is_grown() && sim.age != Age::Child && (keen || desperate) && !repairman {
                    let score = if desperate { urgency(BLADDER) * 40.0 } else { 25.0 } / (1.0 + dist / 25.0);
                    if best.as_ref().is_none_or(|b| score > b.0) {
                        best = Some((score, Action::new(repair_of(obj.kind).0, ActionKind::Repair { target: oe }, true)));
                    }
                }
                continue;
            }
            for (di, d) in interactions_for(obj.kind).iter().enumerate() {
                if !d.autonomous {
                    continue;
                }
                // Guests don't cook or do the chores.
                // A fireplace is lit when cold, and warmed by when lit.
                if obj.kind == ObjectKind::Fireplace && (d.special == Special::LightFire) == lit_q.contains(oe) {
                    continue;
                }
                // Homework is for those who have some.
                if d.special == Special::Homework && !hw_q.contains(me) {
                    continue;
                }
                // Leftovers: once a meal's been out a while, a grown-up of the house puts them
                // in the fridge (not a slob); and they're there to be had when someone's hungry.
                if d.special == Special::PutAway
                    && (!household.contains(me)
                        || !sim.age.is_grown()
                        || sim.age == Age::Child
                        || sim.traits.contains(&crate::life::Trait::Slob)
                        || !served_q.get(oe).is_ok_and(|s| clock.minutes - s.0 >= 90.0))
                {
                    continue;
                }
                if d.special == Special::Leftovers && (leftovers.0.is_empty() || my_lot.is_some()) {
                    continue;
                }
                if d.special == Special::PlayInSprinkler && !sprinkling.contains(oe) {
                    continue;
                }
                // (Not what they couldn't get to just now.)
                if route_failed.get(me).is_ok_and(|f| f.0.iter().any(|(x, t)| *x == oe && clock.minutes - t < ROUTE_FAIL_MINUTES)) {
                    continue;
                }
                if matches!(d.special, Special::ServeMeal | Special::CleanUp | Special::PayBills) && (meal_out && d.special == Special::ServeMeal || !household.contains(me)) {
                    continue;
                }
                // (What lasts until a need is full is no use while it's full already: a bath for
                // its fun would end as soon as it began.)
                if d.until_full.is_some_and(|m| motives.0[m] >= 95.0) {
                    continue;
                }
                // (And what it does for the other needs only lasts as long as it does.)
                let minutes = match d.until_full {
                    Some(m) if d.per_hour[m] > 0.0 => d.minutes.min((100.0 - motives.0[m]).max(0.0) / (d.per_hour[m] / 60.0)),
                    _ => d.minutes,
                };
                let mut score = 0.0;
                for i in 0..6 {
                    let gain = d.per_hour[i] * minutes / 60.0;
                    let room = (100.0 - motives.0[i]).max(0.0);
                    score += gain.min(room).max(-50.0) * urgency(i);
                }
                // Only sleep when actually tired.
                if d.until_full == Some(ENERGY) && motives.0[ENERGY] > -10.0 {
                    score *= 0.1;
                }
                // Homework gets done (by the bookish sooner), unless they're miserable.
                if d.special == Special::Homework && motives.0.iter().all(|m| *m > -30.0) {
                    let keen = [crate::life::Trait::Bookworm, crate::life::Trait::Genius, crate::life::Trait::Perfectionist].iter().any(|t| sim.traits.contains(t));
                    score = if keen { 45.0 } else { 22.0 };
                }
                if matches!(d.special, Special::Water | Special::Weed | Special::Harvest) {
                    let keen = household.contains(me) && (sim.traits.contains(&crate::life::Trait::GreenThumb) || plant_q.get(oe).is_ok());
                    let ok = plant_q.get(oe).is_ok_and(|p| crate::gardening::offers(p, d.special));
                    score = if !ok || !keen {
                        0.0
                    } else {
                        let base = if d.special == Special::Harvest { 30.0 } else { 14.0 };
                        base * if sim.traits.contains(&crate::life::Trait::GreenThumb) { 2.0 } else { 1.0 }
                    };
                }
                if d.special == Special::PayBills {
                    score = if bills_due { 25.0 } else { 0.0 };
                }
                // A full trash can is a chore like dishes; an emptier one isn't.
                if d.special == Special::EmptyTrash && !trash_q.get(oe).is_ok_and(|f| f.0 >= crate::surroundings::trash_capacity(obj.kind)) {
                    continue;
                }
                if d.special == Special::PutAway {
                    score = if sim.traits.contains(&crate::life::Trait::Neat) { 30.0 } else { 12.0 };
                }
                if matches!(d.special, Special::CleanUp | Special::EmptyTrash) {
                    score = if sim.traits.contains(&crate::life::Trait::Slob) {
                        0.0
                    } else if sim.traits.contains(&crate::life::Trait::Neat) {
                        30.0
                    } else {
                        8.0
                    };
                }
                score *= crate::life::activity_affinity(&sim.traits, d.name).sqrt();
                score /= 1.0 + dist / 25.0;
                score *= rng.random_range(0.85..1.15);
                if best.as_ref().is_none_or(|b| score > b.0) {
                    best = Some((score, Action::new(d.name, ActionKind::Object { target: oe, def: di }, true)));
                }
            }
        }
        let social_need = if sim.traits.contains(&crate::life::Trait::Loner) { 0.0 } else { 30.0 };
        let social_need = if sim.traits.contains(&crate::life::Trait::PartyAnimal) || sim.traits.contains(&crate::life::Trait::Friendly) { 50.0 } else { social_need };
        // A little one in need comes first for the grown-ups.
        if sim.age.is_grown() && sim.age != Age::Child {
            for &(other, pos, age, needs, lot) in &others {
                if !age.is_little() || lot != my_lot {
                    continue;
                }
                // (A change cleans them up too.)
                let want = [("Feed", needs[HUNGER]), ("Change Diaper", needs[BLADDER].min(needs[HYGIENE])), ("Play With", needs[SOCIAL].min(needs[FUN]))]
                    .into_iter()
                    .filter(|(_, v)| *v < 0.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((name, v)) = want {
                    let si = crate::social::social_index(name).unwrap_or(0);
                    let mut score = (-v / 10.0).powi(2) / (1.0 + pos.distance(tf.translation) / 25.0);
                    if sim.traits.contains(&crate::life::Trait::FamilyOriented) {
                        score *= 1.5;
                    }
                    if best.as_ref().is_none_or(|b| score > b.0) {
                        best = Some((score, Action::new(SOCIALS[si].name, ActionKind::Social { target: other, social: si }, true)));
                    }
                }
            }
        }
        // At a party everyone mingles.
        let partying = party_on.is_some() && sim.age.is_grown();
        if (motives.0[SOCIAL] < social_need || partying) && !sim.age.is_little() {
            for &(other, pos, age, _, lot) in &others {
                if other == me || age.is_little() || lot != my_lot {
                    continue;
                }
                if partying && pos.distance(tf.translation) > 20.0 {
                    continue;
                }
                let rel = rels.friendship(other);
                let name = match () {
                    _ if rel < 5.0 => "Get to Know",
                    _ if rel > 10.0 && rng.random_bool(0.35) => "Tell Joke",
                    _ if rel > 20.0 && rng.random_bool(0.3) => "Talk About Hobbies",
                    _ => "Chat",
                };
                let si = crate::social::social_index(name).unwrap_or(0);
                let s = &SOCIALS[si];
                let mut score = (s.social_per_hour * s.minutes / 60.0) * urgency(SOCIAL) * rng.random_range(0.8..1.2);
                if partying {
                    score = score.max(30.0 * rng.random_range(0.8..1.2));
                }
                if best.as_ref().is_none_or(|b| score > b.0) {
                    best = Some((score, Action::new(s.name, ActionKind::Social { target: other, social: si }, true)));
                }
            }
        }
        // Lonely at home with nobody to talk to: a friend on the phone (a chat in person is
        // better, when there's someone about).
        let grown = matches!(sim.age, Age::Teen | Age::YoungAdult | Age::Adult | Age::Elder);
        if motives.0[SOCIAL] < social_need && grown && my_lot.is_none() && household.contains(me)
            && let Some((_, friend, name)) =
                friends_away.iter().filter_map(|(e, s)| Some((rels.0.get(&e)?.friendship, e, s.full_name()))).filter(|x| x.0 > 15.0).max_by(|a, b| a.0.total_cmp(&b.0))
        {
            let score = (PHONE_CHAT_SOCIAL * PHONE_CHAT_MINUTES / 60.0) * urgency(SOCIAL) * 0.6 * rng.random_range(0.8..1.2);
            if best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((score, Action::new(format!("Chat with {name}"), ActionKind::PhoneChat { target: friend }, true)));
            }
        }
        if let Some((score, action)) = best
            && score > 2.0
        {
            queue.0.push_back(action);
        }
    }
}

fn motive_warnings(
    clock: Res<GameClock>,
    mut last_check: Local<f64>,
    mut notes: ResMut<Notifications>,
    sims: Query<(&Sim, &Motives), With<HouseholdMember>>,
    mut warned: Local<std::collections::HashSet<(u64, usize)>>,
) {
    if clock.minutes - *last_check < 15.0 {
        return;
    }
    *last_check = clock.minutes;
    let msgs = [
        "is starving!",
        "really needs to use the bathroom!",
        "is exhausted!",
        "is lonely!",
        "is filthy!",
        "is bored out of their mind!",
    ];
    // Once as a need runs out (again once it's been seen to).
    for (sim, m) in &sims {
        for i in 0..6 {
            let key = (sim.id, i);
            if m.0[i] < -75.0 {
                if warned.insert(key) {
                    notes.push(format!("{} {}", sim.first, msgs[i]));
                }
            } else if m.0[i] > -40.0 {
                warned.remove(&key);
            }
        }
    }
}

fn pay_bills(
    mut commands: Commands,
    clock: Res<GameClock>,
    household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    objects: Query<(Entity, &GameObject)>,
) {
    let Some(mut h) = household else { return };
    let day = clock.day();
    let mailbox = objects.iter().any(|(_, o)| o.kind == ObjectKind::Mailbox);
    // Bills arrive Monday and Thursday mornings.
    if day != h.last_bill_day && (day % 7 == 0 || day % 7 == 3) && clock.hour_f() >= 9.0 {
        h.last_bill_day = day;
        if day == 0 {
            return;
        }
        let value: i64 = objects.iter().map(|(_, o)| o.price as i64).sum();
        let bill = 60 + value / 60;
        if mailbox {
            // (The mail carrier brings them.)
            commands.insert_resource(crate::services::MailDue(Bill { amount: bill, day }));
        } else {
            h.funds -= bill;
            notes.push(format!("The bills arrived: §{bill} was paid automatically."));
        }
    }
    // Three days late: the repo man takes things worth what's owed.
    let overdue: i64 = h.bills.iter().filter(|b| day >= b.day + 3).map(|b| b.amount).sum();
    if overdue > 0 && clock.hour_f() >= 10.0 {
        h.bills.retain(|b| day < b.day + 3);
        let mut items: Vec<(Entity, &GameObject)> = objects
            .iter()
            .filter(|(_, o)| o.price > 0 && !matches!(o.kind, ObjectKind::Mailbox | ObjectKind::Tombstone | ObjectKind::Meal | ObjectKind::DirtyDishes))
            .collect();
        items.sort_by_key(|(_, o)| -o.price);
        let mut taken = 0i64;
        let mut names = Vec::new();
        for (e, o) in items {
            if taken >= overdue {
                break;
            }
            taken += o.price as i64;
            names.push(o.name.clone());
            commands.entity(e).despawn();
        }
        notes.push(format!("The repo man came for the unpaid bills (§{overdue}) and took: {}.", names.join(", ")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every clip the catalogue's objects' interactions play is baked (needs the bake:
    /// `SIMS3_CACHE=<baked> cargo test -p sims3 interaction_clips_baked -- --ignored`).
    #[test]
    #[ignore]
    fn interaction_clips_baked() {
        let g = s3bake::default_root().global_dir();
        let catalog: Vec<s3bake::CatalogEntry> = s3bake::read_value(&g.join("catalog.bin")).unwrap();
        let names: Vec<String> = s3bake::read_value::<Vec<String>>(&g.join("clip_names.bin")).unwrap().iter().map(|n| n.to_ascii_lowercase()).collect();
        let mut missing = std::collections::BTreeSet::new();
        for c in &catalog {
            let kind = ObjectKind::from_script(&c.script, &c.instance_name);
            for d in interactions_for(kind) {
                let Some(clip) = interaction_clip(d.name, kind) else { continue };
                for p in clip.start.iter().chain(clip.steps).chain(clip.loops) {
                    // (A grown-up's clip named only for its child version is fine.)
                    let child = p.strip_prefix("a2o_").map(|r| format!("c2o_{r}"));
                    let found = |p: &str| names.iter().any(|n| n.starts_with(&p.to_ascii_lowercase()));
                    if !found(p) && !child.is_some_and(|c| found(&c)) {
                        missing.insert(format!("{} ({:?}): {p}", d.name, kind));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "clips not baked: {missing:#?}");
    }

    /// The buyable base-game objects nothing can be done with, by script class (a list to work
    /// from: `cargo test -p sims3 unused_objects -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn unused_objects() {
        let catalog: Vec<s3bake::CatalogEntry> = s3bake::read_value(&s3bake::default_root().global_dir().join("catalog.bin")).unwrap();
        let mut by_class: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for c in catalog.iter().filter(|c| c.price > 0 && c.objd.1 == 0) {
            let kind = ObjectKind::from_script(&c.script, &c.instance_name);
            if interactions_for(kind).is_empty() && !c.script.contains("Decorations") && !c.script.contains("Lighting") {
                by_class.entry(c.script.rsplit('.').take(2).collect::<Vec<_>>().join(" < ")).or_default().push(c.instance_name.clone());
            }
        }
        for (class, names) in &by_class {
            println!("{class}: {} ({})", names.len(), names.iter().take(4).cloned().collect::<Vec<_>>().join(", "));
        }
    }
}
