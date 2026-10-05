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
            (comings_and_goings, autonomy, run_actions, motive_warnings, pay_bills)
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
    /// Where the bills arrive.
    Mailbox,
    /// The morning paper.
    Newspaper,
    Other,
}

impl ObjectKind {
    pub fn from_script(script: &str, name: &str) -> Self {
        let s = script.to_ascii_lowercase();
        let n = name.to_ascii_lowercase();
        let has = |k: &str| s.contains(k);
        if has("crib") {
            Self::Crib
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
        } else if has("easel") && !has("canvas") {
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
            Self::Fridge | Self::Stove | Self::Microwave => "Appliances",
            Self::BedDouble | Self::BedSingle => "Beds",
            Self::Toilet | Self::Shower | Self::Bathtub | Self::Sink => "Plumbing",
            Self::Sofa | Self::Chair | Self::Stool => "Seating",
            Self::Tv | Self::Computer | Self::Stereo => "Electronics",
            Self::Bookshelf | Self::Mirror | Self::Easel | Self::Guitar | Self::Treadmill | Self::Chess | Self::Telescope | Self::Foosball => {
                "Hobbies"
            }
            Self::SwingSet | Self::JungleGym | Self::DollHouse => "Kids",
            Self::HotTub => "Plumbing",
            Self::Dresser => "Surfaces",
            Self::Table => "Surfaces",
            Self::Light => "Lighting",
            Self::Plant | Self::Decoration => "Decor",
            Self::Meal | Self::DirtyDishes | Self::Tombstone | Self::Mailbox | Self::Newspaper => "Misc",
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
        let local = Vec3::new(self.center.x, 0.0, self.center.y + self.half.y + 0.45);
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    None,
    FindJob,
    QuitJob,
    SellPainting,
    Cook,
    /// Put on a different outfit.
    ChangeClothes,
    /// Cook a group meal and serve it on a platter.
    ServeMeal,
    /// Take a serving from a platter (then sit down to eat it).
    GrabPlate,
    /// Eat a plate of food at a table (a dining chair's hidden interaction).
    EatMeal,
    /// Clear away dirty dishes.
    CleanUp,
    /// Pay the bills waiting in the mailbox.
    PayBills,
    /// Read the paper (then recycle it).
    ReadPaper,
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

static FRIDGE: [InteractionDef; 2] = [
    InteractionDef { special: Special::Cook, ..def("Have Quick Meal", 30.0, [130.0, -6.0, 0.0, 0.0, -4.0, 4.0], Pose::Use) },
    def("Grab a Snack", 12.0, [140.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Use),
];
// (Cooking and grabbing a plate don't feed by themselves: the hunger figures are what they lead
// to, for autonomy to weigh. The meal is eaten afterwards.)
static STOVE: [InteractionDef; 1] = [InteractionDef {
    skill: Some("Cooking"),
    special: Special::ServeMeal,
    ..def("Cook Dinner", 60.0, [100.0, 0.0, 0.0, 0.0, -5.0, 10.0], Pose::Use)
}];
static MEAL: [InteractionDef; 1] = [InteractionDef { special: Special::GrabPlate, ..def("Grab a Plate", 2.0, [9000.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use) }];
static MAILBOX: [InteractionDef; 1] = [InteractionDef { special: Special::PayBills, ..def("Pay Bills", 3.0, N, Pose::Use) }];
static NEWSPAPER: [InteractionDef; 2] = [
    InteractionDef { special: Special::ReadPaper, ..def("Read", 20.0, [0.0, 0.0, 0.0, 0.0, 0.0, 45.0], Pose::Use) },
    InteractionDef { special: Special::FindJob, ..def("Look for a Job", 10.0, N, Pose::Use) },
];
static TOMBSTONE: [InteractionDef; 1] = [def("Mourn", 20.0, [0.0, 0.0, -2.0, 15.0, 0.0, -10.0], Pose::Stand)];
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
static SINK: [InteractionDef; 1] = [def("Wash Hands", 6.0, [0.0, 0.0, 0.0, 0.0, 160.0, 0.0], Pose::Use)];
static SOFA: [InteractionDef; 2] = [
    InteractionDef { on_object: true, ..def("Sit", 40.0, [0.0, 0.0, 5.0, 0.0, 0.0, 10.0], Pose::Sit) },
    InteractionDef { on_object: true, decay: SLEEP_DECAY, ..def("Nap", 60.0, [0.0, 0.0, 18.0, 0.0, 0.0, 0.0], Pose::Lie) },
];
static CHAIR: [InteractionDef; 2] = [
    InteractionDef { on_object: true, autonomous: false, ..def("Sit", 30.0, [0.0, 0.0, 4.0, 0.0, 0.0, 4.0], Pose::Sit) },
    InteractionDef {
        on_object: true,
        autonomous: false,
        special: Special::EatMeal,
        ..def("Eat", MEAL_MINUTES, [MEAL_PER_HOUR, -4.0, 0.0, 20.0, -4.0, 6.0], Pose::Sit)
    },
];
/// The dining chair's "Eat" (not offered in its menu).
pub const CHAIR_EAT: usize = 1;
static TV: [InteractionDef; 2] = [
    def("Watch TV", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 55.0], Pose::Stand),
    def("Watch Cooking Channel", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 35.0], Pose::Stand),
];
static COMPUTER: [InteractionDef; 4] = [
    def("Play Computer Games", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 60.0], Pose::Use),
    InteractionDef { autonomous: false, skill: Some("Writing"), ..def("Write Novel", 90.0, [0.0, 0.0, -4.0, 0.0, 0.0, 10.0], Pose::Use) },
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
static DRESSER: [InteractionDef; 1] =
    [InteractionDef { autonomous: false, special: Special::ChangeClothes, ..def("Change Clothes", 4.0, N, Pose::Use) }];
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
static STEREO: [InteractionDef; 1] = [def("Dance", 45.0, [0.0, 0.0, -6.0, 0.0, -6.0, 70.0], Pose::Dance)];
static BOOKSHELF: [InteractionDef; 1] =
    [InteractionDef { skill: Some("Logic"), ..def("Read a Book", 60.0, [0.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Stand) }];
static MIRROR: [InteractionDef; 1] = [InteractionDef {
    autonomous: false,
    skill: Some("Charisma"),
    ..def("Practice Speech", 40.0, [0.0, 0.0, -2.0, 6.0, 0.0, 10.0], Pose::Talk)
}];
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
static CHESS: [InteractionDef; 1] =
    [InteractionDef { skill: Some("Logic"), ..def("Play Chess", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 40.0], Pose::Use) }];

/// The animation played while performing an interaction: start clip, then loop variants.
pub fn interaction_clip(name: &str, kind: ObjectKind) -> Option<crate::anim::ActionClip> {
    use crate::anim::ActionClip as A;
    Some(match name {
        "Nap" if kind == ObjectKind::Sofa => A::new(Some("a2o_sofa_sit_trans_nap_x"), &["a2o_sofa_nap_loop1_x"]),
        "Eat" if kind == ObjectKind::Chair => A::new(Some("a2o_eat_diningIn_fork_start_x"), &["a2o_eat_diningIn_fork_neat_x"]),
        "Pay Bills" => A::new(None, &["a2o_mailbox_getMail_x"]),
        "Read" if kind == ObjectKind::Newspaper => A::new(Some("a2o_newspaper_read_standing_start_x"), &["a2o_newspaper_read_standing_loop"]),
        "Eat" if kind == ObjectKind::Stool => A::new(Some("a2o_eat_barStoolIn_fork_start_x"), &["a2o_eat_barStoolIn_fork_neat_x"]),
        "Have Quick Meal" | "Microwave Dinner" => A::new(Some("a2o_fridge_openDoor_x"), &["a2o_eat_stand_fork_neat", "a2o_eat_stand_hand_neat"]),
        "Grab a Snack" => A::new(Some("a2o_fridge_openDoor_x"), &["a2o_eat_stand_hand_neat"]),
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
        "Practice Speech" => A::new(None, &["a2o_mirror_full_checkSelfOut_loop"]),
        "Paint" => A::new(Some("a2o_painting_start_x"), &["a2o_painting_loopMed", "a2o_painting_loopLarge", "a2o_painting_consider"]),
        "Play Guitar" => A::new(None, &["a2o_guitar_play_med_loop", "a2o_guitar_play_high_loop", "a2o_guitar_play_low_loop"]),
        "Work Out" => A::new(Some("a2o_treadmill_jog_start_x"), &["a2o_treadmill_jog_loop"]),
        "Play Chess" => A::new(None, &["a2o_chessTable_loop", "a2o_chessTable_move"]),
        "Nap in Crib" => A::new(Some("p2o_crib_sleep_start_y"), &["p2o_crib_sleep_loop_y"]),
        "Change Clothes" => A::new(Some("a2o_dresser_use_open"), &["a2o_dresser_use_close"]),
        "Stargaze" => A::new(Some("a2o_telescope_start"), &["a2o_telescope_look_loop", "a2o_telescope_look_breathe", "a2o_telescope_react_wonderment"]),
        "Swing" => A::new(Some("a2o_swingset_getIn"), &["a2o_swingset_swing"]),
        "Relax in Hot Tub" => A::new(Some("a2o_hotTub_getIn"), &["a2o_hotTub_idles_relaxing_loop", "a2o_hotTub_idles_playingToe", "a2o_hotTub_splash"]),
        "Play with Dollhouse" => A::new(Some("c2o_dollhouse_play_start"), &["c2o_dollhouse_play_loop"]),
        "Play on Jungle Gym" => A::new(Some("c2o_JungleGymTower_climbUp"), &["c2o_JungleGymTower_loop", "c2o_JungleGymTower_slideDown"]),
        "Play Foosball" => A::new(None, &["a2o_foosballTable_play"]),
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
            Age::Toddler => matches!(self, K::Crib | K::ToyBox | K::Xylophone | K::PegBox | K::PottyChair),
            // (Children can't cook.)
            Age::Child => !matches!(self, K::Crib | K::Xylophone | K::PegBox | K::PottyChair | K::HighChair | K::HotTub | K::Stove),
            _ => !matches!(self, K::Crib | K::ToyBox | K::Xylophone | K::PegBox | K::PottyChair | K::HighChair | K::DollHouse | K::JungleGym),
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
        ObjectKind::Telescope => &TELESCOPE,
        ObjectKind::SwingSet => &SWINGSET,
        ObjectKind::HotTub => &HOTTUB,
        ObjectKind::DollHouse => &DOLLHOUSE,
        ObjectKind::JungleGym => &JUNGLEGYM,
        ObjectKind::Foosball => &FOOSBALL,
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
    /// Spend lifetime happiness on a reward (instant, from the rewards menu).
    BuyReward(usize),
    /// Drive to a community lot's rabbit hole for an activity.
    Visit { lot: usize, activity: usize },
    /// Eat a plate of food standing (no free seat at a table).
    EatHere,
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
) {
    for (e, mut sim, j) in &mut joining {
        if let Some(l) = &j.last_name {
            sim.last = l.clone();
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
            Option<&crate::wishes::Wishes>,
            Option<&crate::opportunities::SimOpportunities>,
        ),
        (Without<GameObject>, Without<AtWork>, Without<crate::rabbitholes::AtRabbitHole>),
    >,
    mut objects: Query<(&GameObject, &Transform, &mut UsedBy, Option<&Floor>), Without<Sim>>,
    (building, upper): (Option<Res<crate::building::ActiveBuilding>>, Option<Res<UpperFloors>>),
    mut life: MessageWriter<LifeEvent>,
    people: Query<(Entity, &Sim, &crate::life::Mood, Has<HouseholdMember>), Without<GameObject>>,
    mut conceive: MessageWriter<crate::little::Conceive>,
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

    for (me, sim, mut queue, mut tf, mut motives, mut decay, mut anim, mut skills, mut rels, path, mut job, floor, wishes, opps) in &mut sims {
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
            if let ActionKind::Object { target, .. } = action.kind
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
                        ActionKind::GoToWork | ActionKind::Visit { .. } => exit.as_ref().map(|e| (e.0, 1)),
                        ActionKind::JoinCareer { target, .. } => objects.get(*target).ok().map(|(obj, otf, _, of)| (obj.use_point(otf), of.map_or(1, |f| f.0))),
                        ActionKind::Invite { .. } => {
                            action.phase = Phase::Running(0.0);
                            anim.pose = Pose::Talk;
                            continue;
                        }
                        ActionKind::BuyReward(_) => None,
                        ActionKind::EatHere => {
                            action.phase = Phase::Running(0.0);
                            anim.pose = Pose::Use;
                            commands.entity(me).insert(crate::anim::ActionClip::new(Some("a2o_eat_stand_fork_start_x"), &["a2o_eat_stand_fork_neat_x"]));
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
                        plan_route(&grid, upper.as_deref(), from, floor.0, d, l).or_else(|| alternatives.iter().find_map(|a| plan_route(&grid, upper.as_deref(), from, floor.0, *a, l)))
                    });
                    match routed {
                        Some(wp) => {
                            commands.entity(me).insert(PathFollow::new(wp));
                            action.phase = Phase::Routing;
                            if let ActionKind::Object { target, .. } = action.kind
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
                                    if let Some(c) = interaction_clip(d.name, obj.kind) {
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
                                    let p = crate::social::acceptance(s, &rel, tsim, tmood, other_partner);
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
                            ActionKind::Invite { .. } | ActionKind::BuyReward(_) | ActionKind::EatHere => {}
                            ActionKind::Visit { lot, activity } => {
                                if let (Some(l), Some(name)) = (world.data.lots.get(*lot), world.data.lot_names.get(*lot)) {
                                    let acts = crate::rabbitholes::activities(l);
                                    let task = crate::opportunities::opportunity_task(opps, *activity).map(|t| t.0);
                                    if let Some(a) = task.or_else(|| acts.get(*activity)) {
                                        let place = crate::rabbitholes::lot_title(l, name);
                                        crate::rabbitholes::head_out(&mut commands, &clock, me, sim, *lot, a, place, household.as_deref_mut(), &mut notes);
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
                            if let Ok((obj, otf, mut used, _)) = objects.get_mut(*target) {
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
                                    motives.add(i, gain * dt / 60.0);
                                }
                                // Working out builds fitness and burns off weight.
                                if d.pose == Pose::Exercise {
                                    let h = dt / 60.0;
                                    commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| crate::aging::reshape(&mut e, -0.03 * h, 0.05 * h));
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
                                if elapsed >= d.minutes || full {
                                    finished = true;
                                    life.write(LifeEvent::new(me, LifeEventKind::Finished { activity: d.name, completed: true }));
                                    used.0 = None;
                                    if d.on_object {
                                        stand_up_at = Some(obj.use_point(otf));
                                    }
                                    match d.special {
                                        Special::FindJob => {}
                                        Special::QuitJob => {
                                            if job.is_some() {
                                                commands.entity(me).remove::<Job>();
                                                notes.push(format!("{} quit their job.", sim.first));
                                            }
                                        }
                                        Special::SellPainting => {
                                            let lvl = skills.level("Painting") as i64;
                                            let value = 15 + lvl * lvl * 12 + rand::rng().random_range(0..20);
                                            if let Some(h) = household.as_mut() {
                                                h.funds += value;
                                            }
                                            notes.push(format!("{} finished a painting and sold it for §{value}.", sim.first));
                                        }
                                        Special::ChangeClothes => {
                                            // A different outfit from the wardrobe (the Sim's chosen
                                            // hairstyle stays).
                                            commands.entity(me).queue_silenced(|mut e: EntityWorldMut| {
                                                if let Some(mut s) = e.get_mut::<Sim>() {
                                                    s.look = rand::random();
                                                    let hair = s.outfit.hair;
                                                    s.outfit = OutfitChoice { hair, ..default() };
                                                }
                                                e.insert(crate::aging::NeedsNewBody);
                                            });
                                        }
                                        Special::ServeMeal => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Serve(*target));
                                        }
                                        Special::GrabPlate => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Grabbed(*target));
                                        }
                                        Special::EatMeal => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Ate);
                                        }
                                        Special::CleanUp | Special::ReadPaper => {
                                            commands.entity(*target).try_despawn();
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
                                        SocialEffect::AskToLeave => {
                                            commands.entity(*target).insert(GoingHome);
                                            notes.push(format!("{} said goodbye and is heading home.", tname));
                                        }
                                        _ => {}
                                    }
                                }
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
                        _ => finished = true,
                    }
                }
            }
        }

        if finished {
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
        if let Ok((_, tsim, queue, mut tf, mut motives, _, mut anim, _, mut rels, path, _, _, _, _)) = sims.get_mut(target) {
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
        (Entity, &Transform, &Motives, &mut ActionQueue, &mut AutonomyTimer, &Relationships, Option<&Job>, &Sim, Has<SocialPartner>),
        (Without<AtWork>, Without<crate::rabbitholes::AtRabbitHole>),
    >,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
    hh: Option<Res<Household>>,
) {
    if delta.0 <= 0.0 {
        return;
    }
    let others: Vec<(Entity, Vec3, Age, [f32; 6])> = sims.iter().map(|s| (s.0, s.1.translation, s.7.age, s.2.0)).collect();
    // A meal already out is eaten before anyone cooks another.
    let meal_out = objects.iter().any(|(_, o, _, _)| o.kind == ObjectKind::Meal);
    let bills_due = hh.is_some_and(|h| !h.bills.is_empty());
    let mut rng = rand::rng();
    for (me, tf, motives, mut queue, mut timer, rels, job, sim, partner) in &mut sims {
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
        for (oe, obj, otf, used) in &objects {
            if used.0.is_some_and(|u| u != me) {
                continue;
            }
            if !obj.kind.usable_by(sim.age) {
                continue;
            }
            let dist = obj.world_center(otf).distance(tf.translation);
            for (di, d) in interactions_for(obj.kind).iter().enumerate() {
                if !d.autonomous {
                    continue;
                }
                // Guests don't cook or do the chores.
                if matches!(d.special, Special::ServeMeal | Special::CleanUp | Special::PayBills) && (meal_out && d.special == Special::ServeMeal || !household.contains(me)) {
                    continue;
                }
                let mut score = 0.0;
                for i in 0..6 {
                    let gain = d.per_hour[i] * d.minutes / 60.0;
                    let room = (100.0 - motives.0[i]).max(0.0);
                    score += gain.min(room).max(-50.0) * urgency(i);
                }
                // Only sleep when actually tired.
                if d.until_full == Some(ENERGY) && motives.0[ENERGY] > -10.0 {
                    score *= 0.1;
                }
                if d.special == Special::PayBills {
                    score = if bills_due { 25.0 } else { 0.0 };
                }
                if d.special == Special::CleanUp {
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
            for &(other, pos, age, needs) in &others {
                if !age.is_little() {
                    continue;
                }
                let want = [("Feed", needs[HUNGER]), ("Change Diaper", needs[BLADDER]), ("Play With", needs[SOCIAL].min(needs[FUN]))]
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
        if motives.0[SOCIAL] < social_need && !sim.age.is_little() {
            for &(other, _, age, _) in &others {
                if other == me || age.is_little() {
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
                let score = (s.social_per_hour * s.minutes / 60.0) * urgency(SOCIAL) * rng.random_range(0.8..1.2);
                if best.as_ref().is_none_or(|b| score > b.0) {
                    best = Some((score, Action::new(s.name, ActionKind::Social { target: other, social: si }, true)));
                }
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
    mut warned: Local<HashMap<(String, usize), f64>>,
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
    for (sim, m) in &sims {
        for i in 0..6 {
            if m.0[i] < -75.0 {
                let key = (sim.full_name(), i);
                let last = warned.get(&key).copied().unwrap_or(-1e9);
                if clock.minutes - last > 180.0 {
                    warned.insert(key, clock.minutes);
                    notes.push(format!("{} {}", sim.first, msgs[i]));
                }
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
            h.bills.push(Bill { amount: bill, day });
            notes.push(format!("The bills have arrived in the mailbox: §{bill}. Pay them within three days."));
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
