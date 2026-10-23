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
        app.init_resource::<Notifications>()
            .add_systems(
                Update,
                (comings_and_goings, off_lot_idle, autonomy, run_actions, release_partners, motive_warnings, pay_bills, crate::social::note_contact, crate::social::fade_relationships)
                    .chain()
                    .run_if(in_state(PlayMode::Live)),
            )
            .add_systems(Update, (route_objects, crate::social::load_game_socials).run_if(in_state(crate::AppState::InGame)))
            .init_resource::<RepairTunings>()
            .init_resource::<ObjectRates>()
            .init_resource::<crate::conversation::SocialMemories>()
            .add_systems(Update, note_recent.run_if(in_state(PlayMode::Live)))
            .init_resource::<DepreciationDay>()
            .add_systems(OnEnter(crate::AppState::Loading), |mut day: ResMut<DepreciationDay>| day.0 = None)
            .add_systems(Update, depreciate.after(crate::save::apply_loaded_game).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (load_repair_tunings, wear_new_objects, floor_new_objects, second_places).chain().run_if(in_state(crate::AppState::InGame)))
            .add_systems(Update, load_skill_points.run_if(in_state(crate::AppState::InGame)));
    }
}

/// An object looked at for its routing slot.
#[derive(Component)]
struct Routed;

/// Objects whose clips are played from one of their routing slots get it (the game's data,
/// once it's in).
fn route_objects(
    mut commands: Commands,
    ui: Option<Res<crate::icons::GameUi>>,
    mut objects: Query<(Entity, &mut GameObject), Without<Routed>>,
    mut slots: Local<Option<std::collections::HashMap<s3bake::Key, Vec<(u8, [f32; 3], [f32; 3])>>>>,
) {
    let Some(ui) = ui else { return };
    let slots = slots.get_or_insert_with(|| ui.data.route_slots.iter().cloned().collect());
    for (e, mut o) in &mut objects {
        commands.entity(e).insert(Routed);
        let Some(n) = o.kind.route_slot() else { continue };
        if let Some((_, p, f)) = slots.get(&o.objd).and_then(|s| s.iter().find(|s| s.0 == n)) {
            o.route = Some((Vec2::new(p[0], p[2]), Vec2::new(f[0], f[2]).normalize_or(Vec2::Y)));
            debug!("{} used from its routing slot {n}: {:?}", o.name, o.route);
        }
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
    /// A shower over a tub: showered in, or bathed in.
    ShowerTub,
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
    /// A workout bench (the game's `WorkoutBench`: The Exercise Queen): pumping iron.
    WorkoutBench,
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
    /// A playground slide: up its ladder and down it.
    Slide,
    /// Water on the floor (see `puddles`).
    Puddle,
    Foosball,
    Table,
    Light,
    Plant,
    /// A painting or a sculpture: looked over, and liked or not.
    Art,
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
    /// A tray of hot drinks served from it (`hotdrinks`).
    DrinkTray,
    AlarmClock,
    /// A trash can: clearing dishes fills it.
    TrashCan,
    /// The outdoor trash can (the game's `TrashcanOutside`): where the bags go, never full;
    /// knocked over, picked up and rummaged through (`trashcans`).
    OutdoorTrashCan,
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
    /// A candle: lit, it burns a few hours with a little flame and a warm light.
    Candle,
    /// A trash compactor: a trash can that holds three times as much.
    TrashCompactor,
    /// An apartment's trash chute (Late Night): where the bag goes when there's no can outdoors.
    WasteChute,
    /// Laundry (Generations): the washer, the dryer, the clothesline, and a pile of dirty
    /// clothes on the floor (see `laundry`).
    Washer,
    Dryer,
    Clothesline,
    ClothingPile,
    /// A clothes hamper: where clothes taken off by it go, for the laundry.
    Hamper,
    /// The Teleporter (a lifetime reward): to a community lot at once.
    Teleporter,
    /// The Collection Helper (a lifetime reward): collectibles shown in Map View.
    CollectionHelper,
    /// Lifetime rewards: a plate of food at the push of a button, a new shape, a new mood.
    FoodReplicator,
    BodySculptor,
    MoodletManager,
    /// The future's kitchen (Into the Future): a meal programmed and made at the touch of a
    /// screen, where a house has no fridge or stove.
    FoodSynthesizer,
    /// A urinal (for men and boys).
    Urinal,
    /// Vending machines (Late Night): soda, candy, or both (the healthy one).
    VendingSoda,
    VendingCandy,
    VendingBoth,
    /// A child's toy oven, to bake pretend cakes in.
    ToyOven,
    /// An outdoor fire pit: lit, hands warmed by it and marshmallows roasted over it.
    FirePit,
    /// A buffet table: a plate of food served from it.
    Buffet,
    /// A picnic basket: a picnic out of it.
    PicnicBasket,
    /// A picnic table: four places on its benches (the game's `PicnicTable`).
    PicnicTable,
    /// The household's own car (the game's `CarOwnable`): driven to work, or to a community lot.
    Car,
    /// A house phone, on the wall or a table: the phone's menu.
    Phone,
    /// Bubble bath, and a rubber duck: baths with them, while they're about the house.
    BubbleBath,
    RubberDucky,
    /// A football or a baseball: catch played with it.
    Ball,
    /// Seasons: a snowman, a snow angel, a pile of fall leaves.
    Snowman,
    SnowAngel,
    LeafPile,
    /// The packs' games: a pool table, a dartboard, a bowling lane, a karaoke machine, the
    /// arcade machines, the claw machine, Whack-a-Gnome, skee-ball, a horseshoe pit,
    /// shuffleboard, the mechanical bull, a trampoline and a sandbox.
    PoolTable,
    Dartboard,
    BowlingLane,
    Karaoke,
    ArcadeMachine,
    ClawMachine,
    WhackAGnome,
    SkeeBall,
    Horseshoes,
    Shuffleboard,
    MechanicalBull,
    Trampoline,
    Sandbox,
    /// The packs' instruments: a grand piano, a keyboard, a drum kit, the laser harp.
    Piano,
    Keyboard,
    Drums,
    LaserHarp,
    /// A door or gate (locked from its menu: see `doorlocks`).
    Door,
    /// The Pets pack's (see `petneeds`): a pet bowl, a litter box, a pet bed or dog house, a
    /// scratching post or cat condo, a chew toy, a water trough.
    PetBowl,
    LitterBox,
    PetBed,
    ScratchingPost,
    ChewToy,
    WaterTrough,
    /// A minor pet's terrarium (`terrarium`).
    Terrarium,
    /// A bird cage (the Pets pack's: a terrarium for a bird, `terrarium`).
    BirdCage,
    /// A slippy slide (Generations'): run at and slid down.
    SlippySlide,
    /// A horse's box stall (the Pets pack's): eaten and napped in, cleaned out with a pitchfork.
    BoxStall,
    Other,
}

impl ObjectKind {
    pub fn from_script(script: &str, name: &str) -> Self {
        let s = script.to_ascii_lowercase();
        let n = name.to_ascii_lowercase();
        let has = |k: &str| s.contains(k);
        if has("crib") {
            Self::Crib
        } else if has("objects.pets.petbowl") {
            Self::PetBowl
        } else if has("objects.pets.litterbox") {
            Self::LitterBox
        } else if has("objects.beds.petbed") || has("objects.pets.pethouse") {
            Self::PetBed
        } else if has("objects.pets.scratchingpost") || has("objects.pets.catcondo") {
            Self::ScratchingPost
        } else if (has("objects.pets.chewtoy") || has("objects.pets.squeakychewtoy")) && !has("jig") {
            Self::ChewToy
        } else if has("objects.pets.watertrough") {
            Self::WaterTrough
        } else if has("objects.minorpets.terrarium") {
            Self::Terrarium
        } else if has("objects.decorations.birdcage") {
            Self::BirdCage
        } else if has("objects.environment.slippyslide") {
            Self::SlippySlide
        } else if has("objects.pets.boxstall") {
            Self::BoxStall
        } else if has("hobbiesskills.pooltable") {
            Self::PoolTable
        } else if has("hobbiesskills.dartboard") || has("hobbiesskills.expensivedartboard") {
            Self::Dartboard
        } else if has("hobbiesskills.bowlinglane") {
            Self::BowlingLane
        } else if has("entertainment.karaokemachine") {
            Self::Karaoke
        } else if has("hobbiesskills.arcademachine") {
            Self::ArcadeMachine
        } else if has("hobbiesskills.arcadeclawmachine") {
            Self::ClawMachine
        } else if has("hobbiesskills.whackasupernatural") {
            Self::WhackAGnome
        } else if has("electronics.skeeball") {
            Self::SkeeBall
        } else if has("hobbiesskills.horseshoecourt") {
            Self::Horseshoes
        } else if has("hobbiesskills.shuffleboard") {
            Self::Shuffleboard
        } else if has("entertainment.mechanicalbull") {
            Self::MechanicalBull
        } else if has("hobbiesskills.trampoline") {
            Self::Trampoline
        } else if has("environment.sandbox") {
            Self::Sandbox
        } else if has("hobbiesskills.grandpiano") {
            Self::Piano
        } else if has("hobbiesskills.keyboard") {
            Self::Keyboard
        } else if has("hobbiesskills.drums") {
            Self::Drums
        } else if has("hobbiesskills.laserharp") {
            Self::LaserHarp
        } else if has("electronics.jukebox") {
            // (A jukebox plays music to dance to, as a stereo does.)
            Self::Stereo
        } else if has("urinal") {
            Self::Urinal
        } else if has("toys.toyoven") {
            Self::ToyOven
        } else if has("firepit") && !has("+") {
            Self::FirePit
        } else if has("buffettable") {
            Self::Buffet
        } else if has("cookingobjects.picnicbasket") {
            Self::PicnicBasket
        } else if has("environment.picnictable") || has("mimics.picnictable") || has("mimics.tablepicnic") {
            Self::PicnicTable
        } else if s.starts_with("sims3.gameplay.objects.vehicles.car") && !["service", "taxi", "bus", "police", "limo"].iter().any(|x| s.contains(x)) {
            Self::Car
        } else if s.ends_with("cookingobjects.cake") {
            // (The birthday cake bought in buy mode, as the game sells it.)
            Self::BirthdayCake
        } else if (has("electronics.phonetable") || has("electronics.phonewall")) && !has("handset") {
            Self::Phone
        } else if has("miscellaneous.bubblebath") {
            Self::BubbleBath
        } else if has("miscellaneous.rubberducky") {
            Self::RubberDucky
        } else if has("miscellaneous.recyclebin") {
            Self::TrashCan
        } else if has("toys.football") || has("toys.baseball") {
            Self::Ball
        } else if has("environment.snowman") {
            Self::Snowman
        } else if has("environment.snowangel") {
            Self::SnowAngel
        } else if has("environment.leafpile") {
            Self::LeafPile
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
        } else if has("miscellaneous.wastechute") {
            Self::WasteChute
        } else if has("appliances.washingmachine") || has("appliances.mimics.washer") {
            Self::Washer
        } else if has("appliances.dryer") || has("appliances.mimics.dryer") {
            Self::Dryer
        } else if has("appliances.clothesline") {
            Self::Clothesline
        } else if has("miscellaneous.clothingpiledry") {
            Self::ClothingPile
        } else if has("miscellaneous.hamper") {
            Self::Hamper
        } else if has("rewards.collectionhelper") {
            Self::CollectionHelper
        } else if has("rewards.teleporter") {
            Self::Teleporter
        } else if has("rewards.foodreplicator") {
            Self::FoodReplicator
        } else if has("appliances.futurefoodsynthesizer") {
            Self::FoodSynthesizer
        } else if has("environment.vendingmachinecandy") {
            Self::VendingCandy
        } else if has("environment.vendingmachinehealthy") {
            Self::VendingBoth
        } else if has("environment.vendingmachine") {
            Self::VendingSoda
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
        } else if has("miscellaneous.trashcanoutside") {
            Self::OutdoorTrashCan
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
        } else if has("junglegymslide") {
            Self::Slide
        } else if has("junglegym") {
            Self::JungleGym
        } else if has("foosball") {
            Self::Foosball
        } else if has("pools.poolladder") {
            Self::PoolLadder
        } else if has("objects.fireplaces.") {
            Self::Fireplace
        } else if has("miscellaneous.candle") {
            Self::Candle
        } else if has("urnstone") {
            Self::Tombstone
        } else if has("mailbox") {
            Self::Mailbox
        } else if has("miscellaneous.newspaper") {
            Self::Newspaper
        } else if has("fridge") || has("appliances.mimics.refrigerator") {
            Self::Fridge
        } else if has("microwave") {
            Self::Microwave
        } else if has("stove") {
            Self::Stove
        } else if has("beds.") && (has("double") || has("bedcircle") || has("heart")) {
            Self::BedDouble
        } else if has("beds.") {
            Self::BedSingle
        } else if has("toilet") && !has("clutter") {
            Self::Toilet
        } else if has("bathtubshower") || has("showertub") {
            Self::ShowerTub
        } else if has("shower") {
            Self::Shower
        } else if has("bathtub") || has("hottub") {
            Self::Bathtub
        } else if has("sink") {
            Self::Sink
        } else if has("sofa") || has("loveseat") {
            Self::Sofa
        } else if has("stool") {
            Self::Stool
        } else if has("seating") || has("chair") || (has("bench") && !has("workoutbench")) {
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
        } else if has("workoutbench") || has("exercisemachine") {
            Self::WorkoutBench
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
        } else if (has("decorations") && is_art(&n)) || has("fishing.mountedfish") {
            Self::Art
        } else if has("decorations") {
            Self::Decoration
        } else if has("objects.door.") && !has("shelldoor") {
            Self::Door
        } else {
            Self::Other
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Fridge | Self::Stove | Self::Microwave | Self::Grill | Self::HotBeverage | Self::FoodSynthesizer => "Appliances",
            Self::VendingSoda | Self::VendingCandy | Self::VendingBoth => "Misc",
            Self::AlarmClock => "Electronics",
            Self::TrashCan | Self::OutdoorTrashCan | Self::TrashCompactor | Self::WasteChute | Self::Candle | Self::ClothingPile => "Misc",
            Self::Washer | Self::Dryer | Self::Clothesline => "Appliances",
            Self::Hamper => "Misc",
            Self::Dishwasher => "Appliances",
            Self::FishBowl => "Decor",
            Self::BedDouble | Self::BedSingle => "Beds",
            Self::Toilet | Self::Shower | Self::Bathtub | Self::ShowerTub | Self::Sink => "Plumbing",
            Self::Sofa | Self::Chair | Self::Stool => "Seating",
            Self::Tv | Self::Computer | Self::Stereo | Self::VideoGame | Self::VrGoggles => "Electronics",
            Self::Bookshelf | Self::Mirror | Self::Easel | Self::Guitar | Self::Treadmill | Self::WorkoutBench | Self::Chess | Self::Telescope | Self::Foosball => {
                "Hobbies"
            }
            Self::SwingSet | Self::JungleGym | Self::Slide | Self::SlippySlide | Self::DollHouse | Self::StuffedToy => "Kids",
            Self::Puddle => "Misc",
            Self::Sprinkler => "Outdoors",
            Self::DivingBoard => "Outdoors",
            Self::Bar => "Surfaces",
            Self::FoodReplicator | Self::BodySculptor | Self::MoodletManager | Self::Teleporter | Self::CollectionHelper => "Misc",
            Self::Urinal | Self::BubbleBath | Self::RubberDucky => "Plumbing",
            Self::ToyOven | Self::Ball | Self::Sandbox => "Kids",
            Self::PoolTable
            | Self::Dartboard
            | Self::BowlingLane
            | Self::Karaoke
            | Self::ArcadeMachine
            | Self::ClawMachine
            | Self::WhackAGnome
            | Self::SkeeBall
            | Self::Horseshoes
            | Self::Shuffleboard
            | Self::MechanicalBull
            | Self::Trampoline
            | Self::Piano
            | Self::Keyboard
            | Self::Drums
            | Self::LaserHarp => "Hobbies",
            Self::Snowman | Self::SnowAngel | Self::LeafPile => "Outdoors",
            Self::FirePit | Self::PicnicBasket | Self::PicnicTable | Self::Car => "Outdoors",
            Self::Buffet => "Surfaces",
            Self::Phone => "Electronics",
            Self::HotTub => "Plumbing",
            Self::Dresser => "Surfaces",
            Self::Table => "Surfaces",
            Self::Light => "Lighting",
            Self::Plant | Self::Decoration | Self::Art => "Decor",
            // (Build mode's, not a buy-mode tab: see `loading::buy_category`.)
            Self::Door => "Misc",
            Self::Meal | Self::DirtyDishes | Self::DrinkTray | Self::Tombstone | Self::Mailbox | Self::Newspaper | Self::GardenPlant | Self::Collectible | Self::FishingSpot | Self::Butterfly | Self::Beetle | Self::Fireplace | Self::PoolLadder | Self::BirthdayCake => "Misc",
            Self::Crib | Self::HighChair | Self::ToyBox | Self::Xylophone | Self::PegBox | Self::PottyChair => "Kids",
            Self::PetBowl | Self::LitterBox | Self::PetBed | Self::ScratchingPost | Self::ChewToy | Self::WaterTrough | Self::Terrarium | Self::BirdCage | Self::BoxStall => "Misc",
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
    /// The routing slot Sims use it from (model space), and the way they face there: the
    /// game's clips for it are played from that spot (a piano's bench side, the bowling
    /// lane's end).
    pub route: Option<(Vec2, Vec2)>,
}

impl ObjectKind {
    /// The routing slot (`routingSlot_N`) this kind's clips are played from, if they need one.
    pub fn route_slot(self) -> Option<u8> {
        use ObjectKind as K;
        matches!(
            self,
            K::PoolTable
                | K::Dartboard
                | K::BowlingLane
                | K::Karaoke
                | K::ArcadeMachine
                | K::ClawMachine
                | K::WhackAGnome
                | K::SkeeBall
                | K::Horseshoes
                | K::Shuffleboard
                | K::MechanicalBull
                | K::Trampoline
                | K::Sandbox
                | K::Piano
                | K::Keyboard
                | K::Drums
                | K::LaserHarp
                // (A bath's: its open side, as the game's tubs set against walls have it.)
                | K::Bathtub
                // (A telescope's: behind it, looking through it.)
                | K::Telescope
                // (A shower's: beside its door, the game's clips walking the Sim in from there;
                // a treadmill's, behind it, stepping up onto the belt.)
                | K::Shower
                // (A shower-tub's: beside it, its open side.)
                | K::ShowerTub
                // (A toilet's and a sink's: in front, close enough to clean them, wash or brush
                // teeth at.)
                | K::Toilet
                | K::Sink
                | K::Treadmill
                // (The workout bench's, beside it, for the way there; a dollhouse's, at its open
                // side.)
                | K::WorkoutBench
                | K::DollHouse
                // (A jungle gym's and a slide's: at the foot of the ladder.)
                | K::JungleGym
                | K::Slide
                // (A slippy slide's: at its foot, facing down it.)
                | K::SlippySlide
        )
        .then_some(0)
    }
}

impl GameObject {
    /// Where a sim stands to use the object (its routing slot, or in front, along local +Z).
    /// Where a Sim works out to the TV: a metre and a half out in front of it.
    pub fn workout_spot(&self, tf: &Transform) -> Vec2 {
        let p = tf.transform_point(Vec3::new(self.center.x, 0.0, self.center.y + self.half.y + 1.5));
        Vec2::new(p.x, p.z)
    }

    pub fn use_point(&self, tf: &Transform) -> Vec2 {
        if self.kind == ObjectKind::PicnicTable {
            return Self::picnic_places(tf)[0].0;
        }
        if let Some((p, _)) = self.route {
            let w = tf.transform_point(Vec3::new(p.x, 0.0, p.y));
            return Vec2::new(w.x, w.z);
        }
        // (A pool ladder and a diving board face into the water: Sims stand behind them, on the
        // side.)
        // (Art's looked at from two metres off, as the game's `MinDistanceToView`.)
        let ahead = match self.kind {
            ObjectKind::PoolLadder | ObjectKind::DivingBoard => -(self.half.y + 0.45),
            ObjectKind::Art => self.half.y + 2.0,
            _ => self.half.y + 0.45,
        };
        // (A chess game's watched from beside the table, its players' chairs before and behind
        // it: within the game's `kDistanceToWatch`.)
        let local = if self.kind == ObjectKind::Chess {
            Vec3::new(self.center.x + self.half.x + 0.8, 0.0, self.center.y)
        } else {
            Vec3::new(self.center.x, 0.0, self.center.y + ahead)
        };
        let p = tf.transform_point(local);
        Vec2::new(p.x, p.z)
    }

    /// Where else it's got into or sat on from, when its front can't be walked to: a chair
    /// pushed in at a table from behind or beside it, a hot tub from any side, a bath from its
    /// other side or either end.
    pub fn other_approaches(&self, tf: &Transform) -> Vec<Vec2> {
        let (c, h) = (self.center, self.half);
        let local = match self.kind {
            ObjectKind::Chair | ObjectKind::Stool | ObjectKind::Sofa | ObjectKind::HotTub => {
                vec![Vec2::new(c.x, c.y - h.y - 0.45), Vec2::new(c.x + h.x + 0.45, c.y), Vec2::new(c.x - h.x - 0.45, c.y)]
            }
            ObjectKind::Bathtub | ObjectKind::ShowerTub => vec![Vec2::new(c.x + h.x + 0.45, c.y), Vec2::new(c.x, c.y + h.y + 0.45), Vec2::new(c.x, c.y - h.y - 0.45)],
            ObjectKind::PicnicTable => return Self::picnic_places(tf)[1..].iter().map(|p| p.0).collect(),
            ObjectKind::Chess => vec![Vec2::new(c.x - h.x - 0.8, c.y), Vec2::new(c.x + h.x + 0.7, c.y + h.y + 0.5), Vec2::new(c.x - h.x - 0.7, c.y - h.y - 0.5)],
            ObjectKind::BedSingle | ObjectKind::BedDouble => Self::bed_sides(self.center, self.half).to_vec(),
            // (Art from wherever it can be seen, two or three metres off: a sculpture in a flower
            // bed from its sides or behind; in a narrow room, nearer.)
            ObjectKind::Art => vec![
                Vec2::new(c.x - h.x - 1.4, c.y + h.y + 1.4),
                Vec2::new(c.x + h.x + 1.4, c.y + h.y + 1.4),
                Vec2::new(c.x, c.y + h.y + 2.8),
                Vec2::new(c.x + h.x + 2.0, c.y),
                Vec2::new(c.x - h.x - 2.0, c.y),
                Vec2::new(c.x, c.y - h.y - 2.0),
                Vec2::new(c.x, c.y + h.y + 1.0),
                Vec2::new(c.x - h.x - 0.7, c.y + h.y + 0.7),
                Vec2::new(c.x + h.x + 0.7, c.y + h.y + 0.7),
            ],
            _ => Vec::new(),
        };
        local
            .into_iter()
            .map(|p| {
                let w = tf.transform_point(Vec3::new(p.x, 0.0, p.y));
                Vec2::new(w.x, w.z)
            })
            .collect()
    }

    /// Beside a bed, either side (where the game's routing slots stand Sims to make it), in its
    /// own space.
    pub fn bed_sides(c: Vec2, h: Vec2) -> [Vec2; 2] {
        [Vec2::new(c.x - h.x - 0.45, c.y - 0.35), Vec2::new(c.x + h.x + 0.45, c.y - 0.35)]
    }

    /// A picnic table's four places (the game's routing slots: two each side, facing it): where
    /// a Sim stands to sit down, and where they sit, on the bench.
    pub fn picnic_places(tf: &Transform) -> [(Vec2, Vec2); 4] {
        [(-0.5, -1.0), (0.5, -1.0), (0.5, 1.0), (-0.5, 1.0)].map(|(x, side): (f32, f32)| {
            let at = |z: f32| {
                let w = tf.transform_point(Vec3::new(x, 0.0, z * side));
                Vec2::new(w.x, w.z)
            };
            (at(1.26), at(0.72))
        })
    }

    pub fn world_center(&self, tf: &Transform) -> Vec3 {
        tf.transform_point(Vec3::new(self.center.x, 0.0, self.center.y))
    }

    /// A sofa's two places to sit, along it from its middle: the middle and the next cushion on
    /// a three-seater, a cushion each on a loveseat (an armchair, the one).
    pub fn sofa_seats(&self) -> Option<[f32; 2]> {
        (self.kind == ObjectKind::Sofa && self.half.x >= 0.7).then(|| if self.half.x >= 1.05 { [0.0, 0.62 * self.half.x] } else { [-0.48 * self.half.x, 0.48 * self.half.x] })
    }

    /// Height of the seat / mattress surface.
    pub fn seat_height(&self) -> f32 {
        match self.kind {
            ObjectKind::BedDouble | ObjectKind::BedSingle => self.height.min(0.75).max(0.45),
            ObjectKind::Bathtub | ObjectKind::ShowerTub => 0.25,
            ObjectKind::Shower => 0.0,
            _ => 0.45,
        }
    }
}

#[derive(Component, Default)]
pub struct UsedBy(pub Option<Entity>);

/// A bed slept, napped or lazed in and not made since: its covers left thrown back (the game's
/// unmade bed, got into with its own clip) until someone makes it.
#[derive(Component)]
pub struct Unmade;

/// A double bed's other side, or a sofa's other seat: whoever's on it (the first's is its
/// `UsedBy`). Two share a double bed, each on a side, as the game's couples do, and sit side by
/// side on a sofa.
#[derive(Component, Default)]
pub struct SecondPlace(pub Option<Entity>);

/// Double beds have a second side, sofas a second seat; a claim on it lapses once its Sim is
/// doing something else.
fn second_places(mut commands: Commands, beds: Query<(Entity, &GameObject), Without<SecondPlace>>, mut claimed: Query<(Entity, &mut SecondPlace)>, sims: Query<&ActionQueue>) {
    for (e, o) in &beds {
        if o.kind == ObjectKind::BedDouble || o.sofa_seats().is_some() {
            commands.entity(e).insert(SecondPlace(None));
        }
    }
    for (bed, mut second) in &mut claimed {
        let Some(who) = second.0 else { continue };
        // (Through a talk had where they sit, still theirs.)
        let still = sims.get(who).is_ok_and(|q| {
            let on = |a: &Action| matches!(a.kind, ActionKind::Object { target, .. } if target == bed) || matches!(a.kind, ActionKind::Outro { target, .. } if target == bed);
            q.0.front().is_some_and(on) || (q.0.front().is_some_and(|a| a.seated) && q.0.get(1).is_some_and(on))
        });
        if !still {
            second.0 = None;
        }
    }
}

/// Who has a double bed's side, to the Sim asking: the side's taken when the first is someone
/// else's and so is the second (a single bed, or anything else, has only the one).
fn effective_owner(me: Entity, used: Option<Entity>, second: Option<Option<Entity>>) -> Option<Entity> {
    match second {
        Some(s) if used == Some(me) || s == Some(me) => Some(me),
        Some(s) if used.is_none() || s.is_none() => None,
        _ => used,
    }
}

/// Who has it, for what's done with the whole of it (a nap along a sofa): anyone else in
/// either place.
fn owner_for(me: Entity, kind: ObjectKind, def: usize, used: Option<Entity>, second: Option<Option<Entity>>) -> Option<Entity> {
    if whole_of(kind, def) {
        return used.filter(|u| *u != me).or(second.flatten().filter(|u| *u != me)).or(used);
    }
    effective_owner(me, used, second)
}

/// Lying along a sofa takes both its places.
fn whole_of(kind: ObjectKind, def: usize) -> bool {
    kind == ObjectKind::Sofa && interactions_for(kind).get(def).is_some_and(|d| d.pose == Pose::Lie)
}

/// A broken object: only repairing it is possible.
#[derive(Component)]
pub struct Broken;

/// An object's wear, as the game's `RepairableComponent` keeps it: how often it has broken
/// (each break makes the next likelier), and until when (game minutes) it can't break: a day
/// after it's put in the world (bought, or the lot loaded) or repaired (`HoursUnbreakable`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Wear {
    pub breaks: u32,
    pub sturdy_until: f64,
}

/// How long an object can't break after it's new or repaired (game minutes).
pub const STURDY_MINUTES: f64 = 24.0 * 60.0;

/// Every object put in the world starts out sturdy a while: as long as its class is tuned to
/// (a day for most).
fn wear_new_objects(mut commands: Commands, clock: Res<GameClock>, tunings: Res<RepairTunings>, added: Query<(Entity, &GameObject), Without<Wear>>) {
    for (e, o) in &added {
        commands.entity(e).insert(Wear { breaks: 0, sturdy_until: clock.minutes + tunings.sturdy_minutes(o.objd) });
    }
}

/// What a Sim has done lately for the fun of it (each pastime, and when it finished), so they
/// don't take up the same one again and again.
#[derive(Component, Default)]
pub struct RecentDone(pub Vec<(&'static str, f64)>);

/// How long a pastime stays fresh in mind (game minutes), and how much less keen a Sim is on it
/// for each time they've done it in that while.
const RECENT_MINUTES: f64 = 180.0;
const RECENT_KEEN: f32 = 0.55;

impl RecentDone {
    pub fn times(&self, activity: &str, now: f64) -> usize {
        self.0.iter().filter(|(a, t)| *a == activity && now - t < RECENT_MINUTES).count()
    }
}

/// Pastimes finished, kept in mind a few hours.
fn note_recent(mut commands: Commands, clock: Res<GameClock>, mut events: MessageReader<LifeEvent>, mut sims: Query<Option<&mut RecentDone>, With<Sim>>) {
    for ev in events.read() {
        let LifeEventKind::Finished { activity, .. } = ev.kind else { continue };
        match sims.get_mut(ev.sim) {
            Ok(Some(mut r)) => {
                r.0.retain(|(_, t)| clock.minutes - t < RECENT_MINUTES);
                r.0.push((activity, clock.minutes));
            }
            Ok(None) => {
                commands.entity(ev.sim).insert(RecentDone(vec![(activity, clock.minutes)]));
            }
            Err(_) => {}
        }
    }
}

/// Each catalogue object's breakage and repair, as the game tunes its class (by its OBJD).
#[derive(Resource, Default)]
pub struct RepairTunings(pub HashMap<Key, s3bake::gamedata::RepairTuning>);

impl RepairTunings {
    pub fn get(&self, objd: Key) -> Option<&s3bake::gamedata::RepairTuning> {
        self.0.get(&objd)
    }

    /// How long it can't break once new or repaired (game minutes).
    pub fn sturdy_minutes(&self, objd: Key) -> f64 {
        self.get(objd).map_or(STURDY_MINUTES, |t| t.hours_unbreakable as f64 * 60.0)
    }
}

/// The catalogue's objects matched to their classes' tuning, once the game data's there.
/// How much one of our interactions does for needs on each catalogue object, as the game tunes
/// the object's class (`WatchTV_TVCheap`: a cheap TV's fun) or the nearest class it's a kind of
/// (a `ShowerRomantic` a `Shower`'s), and how much faster its bed restores energy.
#[derive(Resource, Default)]
pub struct ObjectRates(pub HashMap<Key, ObjectNeedRates>);

#[derive(Clone, Debug)]
pub struct ObjectNeedRates {
    pub by: HashMap<&'static str, s3bake::gamedata::InteractionRates>,
    pub energy: f32,
    /// What it adds to the look of a room (its class's `EnvironmentScore`).
    pub environment: f32,
    /// A stove's or grill's cooking.
    pub cooking: Option<s3bake::gamedata::ApplianceCooking>,
}

impl Default for ObjectNeedRates {
    fn default() -> Self {
        Self { by: HashMap::new(), energy: 1.0, environment: 0.0, cooking: None }
    }
}

/// The game's sleepiness: energy down to its Energy motive's Sleepy buff (Tired and Exhausted
/// below it).
pub const SLEEPY: f32 = -60.0;
/// When a Sim gets up when work doesn't say (`Sim.kDefaultWakeupTime`), how long a night's
/// sleep is (`kTypicalNumHoursToSleep`: bedtime is that long before waking), how tired they
/// must be to go to bed then (`kEnergyThresholdBelowWhichTheyWantToSleepWhenSleepyDuringTheirBedTime`),
/// and how long before waking they no longer nap (`kNapAvailabilityHoursBeforeWakeupTime`).
const WAKE_HOUR: f32 = 6.0;
const NIGHT_HOURS: f32 = 8.0;
const BEDTIME_ENERGY: f32 = 50.0;
const NO_NAP_HOURS: f32 = 10.0;

/// Hours till a Sim gets up: for work, at their level's wake-up time, or at six.
pub fn hours_until_waking(hour: f32, job: Option<&Job>) -> f32 {
    let wake = job.map_or(WAKE_HOUR, |j| j.info().wakeup);
    let h = (wake - hour).rem_euclid(24.0);
    if h == 0.0 { 24.0 } else { h }
}

/// Whether a Sim goes to bed of their own accord (the game's `BedSleep` test).
pub fn may_sleep(energy: f32, hours_until_waking: f32) -> bool {
    energy <= SLEEPY || (hours_until_waking <= NIGHT_HOURS && energy <= BEDTIME_ENERGY)
}

/// Whether a Sim naps of their own accord (the game's `AbleToNap`).
pub fn may_nap(energy: f32, hours_until_waking: f32) -> bool {
    energy <= SLEEPY && hours_until_waking > NO_NAP_HOURS
}

/// The game's tuning name for those of our interactions it tunes object by object.
fn itun_name(def: &str) -> Option<&'static str> {
    Some(match def {
        "Sleep" => "BedSleep",
        "Nap" => "BedNap",
        "Take Shower" => "TakeShower",
        "Take Bath" => "TakeBath",
        "Take Bubble Bath" => "TakeBubbleBath",
        "Watch TV" | "Watch the TV" => "WatchTV",
        "Play Computer Games" => "PlayComputerGames",
        "Browse Web" => "BrowseWeb",
        "Use Toilet" => "UseToilet",
        _ => return None,
    })
}

const ITUN_NAMES: [&str; 9] = ["BedSleep", "BedNap", "TakeShower", "TakeBath", "TakeBubbleBath", "WatchTV", "PlayComputerGames", "BrowseWeb", "UseToilet"];

/// A class and the classes it's named a kind of (`BedDoubleBrass`, `BedDouble`, `Bed`): the
/// game's object classes are named so.
fn class_chain(class: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(class), |c| c.char_indices().rev().find(|(i, ch)| *i > 0 && ch.is_ascii_uppercase()).map(|(i, _)| &c[..i]))
}

impl ObjectRates {
    /// An interaction's change to each need an hour on this object.
    pub fn per_hour(&self, d: &InteractionDef, objd: Key) -> [f32; 6] {
        let mut r = d.per_hour;
        let Some(o) = self.0.get(&objd) else { return r };
        if let Some(t) = itun_name(d.name).and_then(|n| o.by.get(n)) {
            for i in 0..6 {
                if t.per_hour[i] != 0.0 {
                    r[i] = t.per_hour[i];
                } else if t.once[i] != 0.0 {
                    // (What it does at once, over the time it takes.)
                    r[i] = t.once[i] / d.minutes.max(1.0) * 60.0;
                }
            }
        }
        if matches!(d.name, "Sleep" | "Nap") && r[ENERGY] > 0.0 {
            r[ENERGY] *= o.energy;
        }
        r
    }
}

fn load_object_rates<'a>(rates: &mut ObjectRates, data: &s3bake::gamedata::GameDataBaked, catalog: impl IntoIterator<Item = (Key, &'a str)>) {
    let mut by_class: HashMap<(&str, &str), &s3bake::gamedata::InteractionRates> = HashMap::new();
    for r in &data.interaction_rates {
        by_class.insert((r.class.as_str(), r.interaction.as_str()), r);
    }
    let energy: HashMap<&str, f32> = data.energy_multipliers.iter().map(|(c, v)| (c.as_str(), *v)).collect();
    let environment: HashMap<&str, f32> = data.environment_scores.iter().map(|(c, v)| (c.as_str(), *v)).collect();
    for (objd, script) in catalog {
        let class = script.rsplit('.').next().unwrap_or("");
        if class.is_empty() {
            continue;
        }
        let mut o = ObjectNeedRates::default();
        for n in ITUN_NAMES {
            if let Some(r) = class_chain(class).find_map(|k| by_class.get(&(k, n))) {
                o.by.insert(n, (*r).clone());
            }
        }
        if let Some(e) = class_chain(class).find_map(|k| energy.get(k)) {
            o.energy = *e;
        }
        if let Some(e) = class_chain(class).find_map(|k| environment.get(k)) {
            o.environment = *e;
        }
        o.cooking = class_chain(class).find_map(|k| data.appliance_cooking.iter().find(|a| a.class == k)).cloned();
        if !o.by.is_empty() || o.energy != 1.0 || o.environment != 0.0 || o.cooking.is_some() {
            rates.0.insert(objd, o);
        }
    }
}

fn load_repair_tunings(
    mut tunings: ResMut<RepairTunings>,
    mut rates: ResMut<ObjectRates>,
    ui: Option<Res<crate::icons::GameUi>>,
    baked: Option<Res<crate::baked::Baked>>,
    mut done: Local<bool>,
) {
    let (false, Some(ui), Some(baked)) = (*done, ui, baked) else { return };
    *done = true;
    load_object_rates(&mut rates, &ui.data, baked.0.catalog.iter().map(|c| (c.objd, c.script.as_str())));
    info!("need rates: {} catalogue objects tuned by their class", rates.0.len());
    let by: HashMap<String, &s3bake::gamedata::RepairTuning> = ui.data.repairs.iter().map(|r| (r.class.to_ascii_lowercase(), r)).collect();
    for c in &baked.0.catalog {
        let class = c.script.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if let Some(r) = by.get(&class) {
            tunings.0.insert(c.objd, (*r).clone());
        }
    }
    info!("repairs: {} kinds of object tuned, {} catalogue objects", ui.data.repairs.len(), tunings.0.len());
}

/// The game's chance of a shock repairing something (per cent): from its most at no Handiness
/// to its least at the repair's complexity.
pub fn shock_chance(t: &s3bake::gamedata::RepairTuning, handiness: f32) -> f32 {
    let f = (handiness / t.complexity.max(1.0)).clamp(0.0, 1.0);
    t.max_shock + (t.min_shock - t.max_shock) * f
}

/// The game's time for a repair (game minutes), likewise from its longest to its shortest.
pub fn repair_time(t: &s3bake::gamedata::RepairTuning, handiness: f32) -> f32 {
    let f = (handiness / t.complexity.max(1.0)).clamp(0.0, 1.0);
    t.max_time + (t.min_time - t.max_time) * f
}

/// What's put in the world as the game goes (a plate laid at a table, a platter served) stands
/// on the floor it's put on, upstairs too: not the ground floor every object without a floor is
/// taken for (a Sim who ate upstairs went down to clear their plate from under the table).
fn floor_new_objects(mut commands: Commands, building: Option<Res<crate::building::ActiveBuilding>>, added: Query<(Entity, &Transform), (Added<GameObject>, Without<Floor>)>) {
    let Some(b) = building else { return };
    for (e, tf) in &added {
        let level = b.level_at_point(tf.translation);
        if level != 1 {
            commands.entity(e).insert(Floor(level));
        }
    }
}

/// How likely an object is to break when used (per use): as the game tunes its class (a chance
/// growing with each break), else the game's usual 5% (less for what's built better), a point
/// more for each time it has broken before.
fn break_chance_tuned(tuning: Option<&s3bake::gamedata::RepairTuning>, kind: ObjectKind, price: i32, breaks: u32) -> f64 {
    match tuning {
        Some(t) => ((t.start_break + t.break_increase * breaks as f32) as f64 / 100.0).clamp(0.0, 1.0),
        None => break_chance(kind, price, breaks),
    }
}

fn break_chance(kind: ObjectKind, price: i32, breaks: u32) -> f64 {
    let base = match kind {
        ObjectKind::Shower | ObjectKind::Bathtub | ObjectKind::ShowerTub | ObjectKind::Sink | ObjectKind::Dishwasher | ObjectKind::Toilet => 0.05,
        ObjectKind::TrashCompactor => 0.02,
        ObjectKind::Tv | ObjectKind::Computer | ObjectKind::Stereo => 0.03,
        _ => 0.0,
    };
    if base == 0.0 {
        return 0.0;
    }
    // (Better things last longer.)
    let quality = if price >= 1000 { 0.4 } else if price >= 500 { 0.7 } else { 1.0 };
    ((base + 0.01 * breaks as f64) * quality).min(1.0)
}

#[cfg(test)]
mod wear_tests {
    use super::*;

    #[test]
    fn repairs_go_by_the_games_tuning_for_the_thing() {
        let tv = s3bake::gamedata::RepairTuning {
            class: "TVCheap".into(),
            hours_unbreakable: 48.0,
            start_break: 6.0,
            break_increase: 3.0,
            complexity: 5.0,
            min_time: 25.0,
            max_time: 120.0,
            min_shock: 0.0,
            max_shock: 7.0,
            shock_in_puddle: true,
            repair_skill: 1000.0,
            upgrade_skill: 3600.0,
        };
        assert_eq!((repair_time(&tv, 0.0), repair_time(&tv, 5.0), repair_time(&tv, 9.0)), (120.0, 25.0, 25.0));
        assert!((shock_chance(&tv, 0.0) - 7.0).abs() < 1e-6 && shock_chance(&tv, 5.0).abs() < 1e-6);
        assert!((break_chance_tuned(Some(&tv), ObjectKind::Tv, 300, 2) - 0.12).abs() < 1e-9, "6% and 3% a break before");
    }

    #[test]
    fn breaking_gets_likelier_and_better_things_last() {
        assert_eq!(break_chance(ObjectKind::Toilet, 300, 0), 0.05);
        assert!((break_chance(ObjectKind::Toilet, 300, 3) - 0.08).abs() < 1e-9);
        assert!(break_chance(ObjectKind::Shower, 1500, 0) < break_chance(ObjectKind::Shower, 300, 0));
        assert_eq!(break_chance(ObjectKind::Other, 300, 9), 0.0);
    }
}

/// What fixing an object is called, and the game's animation for it.
pub fn repair_of(kind: ObjectKind) -> (&'static str, crate::anim::ActionClip) {
    use crate::anim::ActionClip as A;
    match kind {
        ObjectKind::Toilet => ("Unclog", A::new(Some("a2o_toilet_unclog_start_x"), &["a2o_toilet_unclog_loop_x"])),
        ObjectKind::Shower => ("Repair", A::new(Some("a2o_shower_repairShower_start_x"), &["a2o_shower_repairShower_loop1_x", "a2o_shower_repairShower_loop2_x"])),
        ObjectKind::Bathtub | ObjectKind::ShowerTub | ObjectKind::HotTub => (
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
        // (A door's bell changed: its own tinkering, the game's `genericdoor` "Upgrade".)
        ObjectKind::Door => ("Repair", A::new(Some("a2o_door_upgrade_start_x"), &["a2o_door_upgrade_loop1_x", "a2o_door_upgrade_loop2_x"])),
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
pub const REPAIRMAN_PRICE: i64 = 50;

/// The repairman, on his way.
#[derive(Resource)]
pub struct RepairmanVisit {
    pub arrive_at: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    None,
    /// Chatting online at a computer (`chatonline`).
    ChatOnline,
    /// An epitaph engraved on a tombstone, or read (`ghosts`).
    Engrave,
    ReadEpitaph,
    /// Their looks changed at a mirror (hair, facial hair, glasses, make-up).
    ChangeAppearance,
    /// A plate of food from the food replicator.
    ReplicateFood,
    /// A soda or a candy bar from a vending machine, paid for away from home.
    Vend,
    /// A snack stuck in the machine instead (not on the menu: a purchase turns into it).
    VendStuck,
    /// The machine shaken or slammed, for a snack stuck in it or one for free.
    VendShake,
    /// Art looked over, then liked or not, by taste (`art_taste`).
    ViewArt,
    /// The laundry's steps (see `laundry`): a pile picked up, a load put on, the wet clothes
    /// taken out of the washer, put in the dryer (or on the line), and the dry clothes out.
    PickUpClothes,
    DoLaundry,
    /// The clothes taken out of the hamper, for a washer.
    EmptyHamper,
    MoveLaundry,
    LoadDryer,
    EmptyDryer,
    /// Off to a lot by the Teleporter (the menu's `ActionKind::Teleport`).
    Teleport,
    /// A new shape from the body sculptor (fitter, slimmer or fuller, by the interaction).
    Sculpt,
    /// A mood from the moodlet manager (by the interaction).
    SetMood,
    /// A cleared dish washed up at the sink or put in the dishwasher.
    WashDishes,
    /// The trash bag from an indoor can put out in the one outdoors.
    DropTrash,
    /// A book taken from the shelf, to read in a comfy seat (or standing, with none free).
    GetBook,
    /// A book read (in a seat, or at the shelf).
    ReadBook,
    /// A book read elsewhere carried back and put on its shelf (the game's `Book.PutBookAway`;
    /// never on the menu: `surroundings`).
    ReturnBook,
    /// The paper picked up, to read in a seat (or where it lay, with none free).
    GetPaper,
    /// Before cooking: the ingredients from the fridge, and chopped at a counter.
    GetIngredients,
    PrepFood,
    /// The cooked meal carried from the stove and set down on a counter or table.
    PlaceMeal,
    /// A dinner put in the microwave to heat (then taken out, and eaten at the table).
    Microwave,
    /// The TV turned on, to watch from a seat facing it (or standing, with none free).
    TurnOnTv,
    /// A stereo turned on (to a radio station: it plays until it's turned off) or off.
    StereoOn,
    StereoOff,
    /// The TV watched (from a seat, or standing before it).
    WatchTv,
    /// A garden sprinkler turned on (it runs a couple of hours) or off.
    SprinklerOn,
    SprinklerOff,
    /// Running about in a sprinkler's spray (only while it's on).
    PlayInSprinkler,
    FindJob,
    QuitJob,
    SellPainting,
    Cook,
    /// Change into the outfit selected for this queued action.
    ChangeClothes,
    /// Choose the everyday outfit piece by piece.
    PlanOutfit,
    /// Set the alarm clock, or turn it off.
    ToggleAlarm,
    /// Empty a full trash can.
    EmptyTrash,
    /// Put the fish selected for this action in a bowl, or take it out.
    PlaceFish,
    TakeFish,
    /// A bowl's fish fed, and a dead one cleaned out (`fishbowl`).
    FeedFish,
    CleanBowl,
    /// The dollhouse smashed in a temper, and thrown away smashed (`dollhouse`).
    SmashDollhouse,
    DisposeDollhouse,
    /// Searching the galaxy through a telescope, for something new to name (`telescope`).
    SearchGalaxy,
    /// An outdoor trash can knocked over, set up again, rummaged through (`trashcans`).
    KnockOverCan,
    PickUpCan,
    Rummage,
    /// Cook a group meal and serve it on a platter.
    ServeMeal,
    /// Take a serving from a platter (then sit down to eat it).
    GrabPlate,
    /// Put what's left of a meal in the fridge.
    PutAway,
    /// A plate of leftovers from the fridge.
    Leftovers,
    /// The spoiled leftovers thrown out of the fridge (the game's `Fridge_CleanOutBadFood`).
    CleanOutFridge,
    /// Eat a plate of food at a table (a dining chair's hidden interaction).
    EatMeal,
    /// Clear away dirty dishes.
    CleanUp,
    /// Pay the bills waiting in the mailbox.
    PayBills,
    /// Read the paper (then recycle it).
    ReadPaper,
    /// A Frugal Sim clipping the paper's coupon (`coupons`).
    ClipCoupon,
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
    /// Play chess (alone, or against whoever has the other chair: see `chess`).
    PlayChess,
    /// Watch a chess game from beside the table.
    WatchChess,
    /// Play an instrument for tips (outdoors, or on a community lot: see `tips`).
    PlayForTips,
    /// Watch someone play an instrument.
    WatchMusic,
    /// Dancing to someone playing (`tips`).
    DanceToMusic,
    /// Perform a composition known on an instrument (see `instruments`).
    Perform,
    /// Daydream lying in bed, from relaxing there (see `daydream`).
    Daydream,
    /// Serve a tray of hot beverages from the machine (see `hotdrinks`).
    ServeDrinks,
    /// Take a drink from a tray of them.
    GrabDrink,
    /// A toddler sat in a high chair, eating there, put in it, fed and taken out (see
    /// `highchair`).
    HighChairSit,
    PutInHighChair,
    GiveBabyFood,
    TakeOutOfHighChair,
    /// Potty train a toddler at the potty chair, the toddler's sitting for it, and emptying it
    /// (see `potty`).
    PottyTrain,
    PottyLesson,
    EmptyPotty,
    /// A pet bowl filled, for what it's short (`petneeds`).
    FillPetBowl,
    /// A terrarium stocked, its dish filled, its dead pet taken out (`terrarium`).
    StockTerrarium,
    FillTerrarium,
    WatchMinorPet,
    /// A large bird talked with, and taught a way of talking (`terrarium`).
    TalkWithBird,
    TeachBirdToTalk,
    /// Down the slippy slide (one run; then again, while it's fun).
    SlippySlide,
    MinorPetInfo,
    MournMinorPet,
    /// Lock or unlock a door (from its menu, at once: see `doorlocks`).
    LockDoor,
    /// A lamp switched, its room's or the lot's (from its menu, at once: see `lights`).
    LightSwitch,
    /// The testing cheats' "Make All Happy" (at once: see `cheats`).
    MakeAllHappy,
    /// Something posted at the mailbox (`rgmmail`).
    PostAway,
    /// A garden plant fertilized, revived, talked to, or disposed of (`gardening`).
    Fertilize,
    RevivePlant,
    TalkToPlant,
    DisposePlant,
    /// Blow out a birthday cake's candles and grow up.
    BlowOutCandles,
    /// Write (a page at a time) the book under way.
    WriteNovel,
    /// A bath with bubble bath, or with the rubber duck: offered while there's one about.
    BubbleBath,
    Ducky,
    /// Only for vampires.
    VampireOnly,
    /// Gone once it's done (a snowman knocked down, a leaf pile raked up).
    Clear,
    /// Making an unmade bed (offered only then).
    MakeBed,
    /// A puddle mopped up.
    MopUp,
    /// Cleaning a dirty toilet, shower, tub or sink (offered once it needs it: `grime`).
    Scrub,
    /// WooHoo in a double bed with their partner, or trying for a baby (the socials of those
    /// names, accepted, as the game's `MakeSimsWooHooOnMe`).
    WooHoo,
    TryForBaby,
    /// At a mirror (`mirrors`): admiring themselves, gussying up, making faces, checking
    /// themselves out.
    AdmireSelf,
    GussyUp,
    PlayWithMirror,
    CheckSelfOut,
    /// At the computer, the game's trait ones: a Genius's hard problems, a Workaholic's job
    /// issues, a Bookworm's book club (`computertraits`).
    SolveProblems,
    JobIssues,
    JoinBookClub,
    LeaveBookClub,
    /// Checking on the household's real estate: its dialog after (`estatedialog`).
    CheckRealEstate,
    /// A policeman's report, a journalist's story, written up (`careermetrics`).
    WriteReport,
    /// A Technophobe sabotaging a TV, computer or stereo (`sabotage`).
    Sabotage,
    /// A Computer Whiz hacking (`computertraits`: night only).
    Hack,
    /// A Workaholic working from home out of work hours, and a Computer Whiz overclocking
    /// (`computertraits`).
    WorkFromHome,
    Overclock,
    /// A Neurotic's Check Sink, Check Stove, Check Fireplace (`neurotic`).
    NeuroticCheck,
    /// The toilet used, and one left unflushed flushed (`toilets`).
    UseToilet,
    Flush,
    /// Sick in the toilet when nausea's run its course (the game's `Toilet.Vomit`; never on the
    /// menu: `nausea`).
    Vomit,
    /// A workout to the stereo (the game's `AthleticGameObject.WorkOut` there: Cardio Exercise,
    /// and Strength Training for teens and up), on its workout station (`music`).
    CardioWorkOut,
    StrengthWorkOut,
    /// The TV turned on, left on, or off, there and then (the game's immediate `TurnOnTV`,
    /// `TurnOffTV`: never queued, `hud::pie_buttons`).
    TvTurnOn,
    TvTurnOff,
    /// The fire poked up (the game's `PokeFireInteraction`: it rages a while, `fireplace`).
    PokeFire,
    /// A wash at the sink (the game's `Sink.SpongeBath`: teens and up).
    SpongeBath,
    /// A donation at the mailbox (`charity`: chosen from its causes' menus, never as itself).
    Donate,
}

impl Special {
    /// What has to be about the house for this to be offered (bubble bath for a bubble bath).
    pub fn needs(self) -> Option<ObjectKind> {
        match self {
            Special::BubbleBath => Some(ObjectKind::BubbleBath),
            Special::Ducky => Some(ObjectKind::RubberDucky),
            _ => None,
        }
    }
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
/// How long a nap in bed lasts (the game's `kNapLengthInMinutes`).
const NAP_MINUTES: f32 = 120.0;

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

static FRIDGE: [InteractionDef; 7] = [
    // (Vampires: a pack of plasma juice, instead of food.)
    InteractionDef { special: Special::VampireOnly, ..def("Drink Plasma Juice", 6.0, [900.0, -4.0, 0.0, 0.0, 0.0, 10.0], Pose::Use) },
    InteractionDef { special: Special::Cook, ..def("Have Quick Meal", 30.0, [130.0, -6.0, 0.0, 0.0, -4.0, 4.0], Pose::Use) },
    // (What it leads to: a plate of a proper meal, eaten at the table.)
    InteractionDef { special: Special::Leftovers, ..def("Eat Leftovers", 3.0, [5000.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use) },
    def("Grab a Snack", 12.0, [140.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Use),
    InteractionDef { autonomous: false, skill: Some("Cooking"), special: Special::BakeCake, ..def("Bake Birthday Cake", 30.0, [0.0, 0.0, 0.0, 0.0, 0.0, 6.0], Pose::Use) },
    // (Before cooking at the stove: never on the menu.)
    InteractionDef { autonomous: false, special: Special::GetIngredients, ..def("Get Ingredients", 6.0, N, Pose::Use) },
    // (The game's `Fridge_CleanOutBadFood`: twenty fun and twenty-five hygiene an hour.)
    InteractionDef { special: Special::CleanOutFridge, ..def("Clean Out", CLEAN_OUT_MINUTES, [0.0, 0.0, 0.0, 0.0, -25.0, -20.0], Pose::Use) },
];
/// How long cleaning out the fridge takes (game minutes: its clip's).
const CLEAN_OUT_MINUTES: f32 = 4.0;
/// (A counter, where the ingredients are chopped before cooking: never on the menu.)
static COUNTER: [InteractionDef; 2] = [
    InteractionDef { autonomous: false, skill: Some("Cooking"), special: Special::PrepFood, ..def("Prepare Food", 15.0, [0.0, 0.0, 0.0, 0.0, -2.0, 4.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::PlaceMeal, ..def("Set Down Meal", 2.0, N, Pose::Use) },
];
static CAKE: [InteractionDef; 1] =
    [InteractionDef { autonomous: false, special: Special::BlowOutCandles, ..def("Grow Up", 3.0, [0.0, 0.0, 0.0, 10.0, 0.0, 20.0], Pose::Use) }];
// (Cooking and grabbing a plate don't feed by themselves: the hunger figures are what they lead
// to, for autonomy to weigh. The meal is eaten afterwards.)
static STOVE: [InteractionDef; 2] = [
    InteractionDef { skill: Some("Cooking"), special: Special::ServeMeal, ..def("Cook Dinner", 60.0, [100.0, 0.0, 0.0, 0.0, -5.0, 10.0], Pose::Use) },
    InteractionDef { special: Special::NeuroticCheck, ..def("Check Stove", 5.3, N, Pose::Use) },
];
static MEAL: [InteractionDef; 2] = [
    InteractionDef { special: Special::GrabPlate, ..def("Grab a Plate", 2.0, [9000.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use) },
    InteractionDef { special: Special::PutAway, ..def("Put Away Leftovers", 3.0, N, Pose::Use) },
];
static MAILBOX: [InteractionDef; 8] = [InteractionDef { special: Special::PayBills, ..def("Pay Bills", 3.0, N, Pose::Use) },
    // (The game's `DonateToCharity`, offered as its causes' menus.)
    InteractionDef { autonomous: false, special: Special::Donate, ..def("Donate to Charity", 2.0, N, Pose::Use) },
    // (With the testing cheats on: `cheats`.)
    InteractionDef { autonomous: false, special: Special::MakeAllHappy, ..def(crate::cheats::MAKE_ALL_HAPPY, 0.0, N, Pose::Use) },
    // (A gem or metal posted off to be cut or smelted, from the inventory: `rgmmail`.)
    InteractionDef { autonomous: false, special: Special::PostAway, ..def("Post", 2.0, N, Pose::Use) },
    // (The testing cheats' others: `cheats`.)
    InteractionDef { autonomous: false, special: Special::MakeAllHappy, ..def(crate::cheats::MAKE_FRIENDS, 0.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::MakeAllHappy, ..def(crate::cheats::KNOW_EVERYONE, 0.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::MakeAllHappy, ..def(crate::cheats::MOTIVE_DECAY, 0.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::MakeAllHappy, ..def(crate::cheats::FORCE_VISITOR, 0.0, N, Pose::Use) },
];
static NEWSPAPER: [InteractionDef; 4] = [
    InteractionDef { special: Special::GetPaper, ..def("Read", 20.0, [0.0, 0.0, 0.0, 0.0, 0.0, 45.0], Pose::Use) },
    InteractionDef { special: Special::FindJob, ..def("Find a Job", 10.0, N, Pose::Use) },
    // (Read where it lay when there's no seat free: never on the menu.)
    READ_PAPER,
    // (The Frugal's, while the paper has its coupon: `coupons`.)
    InteractionDef { special: Special::ClipCoupon, ..def("Clip Coupon", 5.0, N, Pose::Use) },
];
/// (The paper read: in a seat, or standing.)
const READ_PAPER: InteractionDef = InteractionDef { autonomous: false, special: Special::ReadPaper, ..def("Read the Paper", 20.0, [0.0, 0.0, 0.0, 0.0, 0.0, 45.0], Pose::Use) };
const READ_PAPER_SEATED: InteractionDef = InteractionDef { on_object: true, pose: Pose::Sit, ..READ_PAPER };
static GARDEN: [InteractionDef; 7] = [
    InteractionDef { special: Special::Water, skill: Some("Gardening"), ..def("Water", 12.0, [0.0, 0.0, 0.0, 0.0, -3.0, 4.0], Pose::Use) },
    InteractionDef { special: Special::Weed, skill: Some("Gardening"), ..def("Weed", 15.0, [0.0, 0.0, 0.0, 0.0, -8.0, 2.0], Pose::Use) },
    InteractionDef { special: Special::Harvest, skill: Some("Gardening"), ..def("Harvest", 10.0, [0.0, 0.0, 0.0, 0.0, -3.0, 8.0], Pose::Use) },
    // (The game's `FertilizePlant`, `RevivePlant`, `DisposeAlivePlant`/`DisposeDeadPlant`, and a
    // Green Thumb's `TalkToPlant`: see `gardening`.)
    InteractionDef { autonomous: false, special: Special::Fertilize, skill: Some("Gardening"), ..def("Fertilize", 8.0, [0.0, 0.0, 0.0, 0.0, -6.0, 2.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::RevivePlant, skill: Some("Gardening"), ..def("Revive", 20.0, [0.0, 0.0, 0.0, 0.0, -4.0, 4.0], Pose::Use) },
    InteractionDef { special: Special::TalkToPlant, ..def("Talk to Plant", 30.0, [0.0, 0.0, 0.0, 20.0, 0.0, 10.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::DisposePlant, ..def("Dispose", 5.0, [0.0, 0.0, 0.0, 0.0, -4.0, 0.0], Pose::Use) },
];
static COLLECTIBLE: [InteractionDef; 1] = [InteractionDef { special: Special::Collect, ..def("Collect", 4.0, [0.0, 0.0, 0.0, 0.0, -2.0, 10.0], Pose::Use) }];
static FISHING_SPOT: [InteractionDef; 1] =
    [InteractionDef { special: Special::Fish, skill: Some("Fishing"), ..def("Fish", 60.0, [-4.0, -4.0, -3.0, 0.0, -2.0, 25.0], Pose::Use) }];
static FIREPLACE: [InteractionDef; 5] = [
    InteractionDef { special: Special::LightFire, ..def("Light Fire", 2.0, [0.0, 0.0, 0.0, 0.0, 0.0, 4.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::PutOutFire, ..def("Put Out Fire", 2.0, [0.0; 6], Pose::Use) },
    def("Warm Hands", 20.0, [0.0, 0.0, 0.0, 0.0, 0.0, 14.0], Pose::Use),
    InteractionDef { special: Special::NeuroticCheck, ..def("Check Fireplace", 5.1, N, Pose::Use) },
    // (The game's `PokeFireStageTime`, 15 minutes; its ITUN's Fun 10 advertised.)
    InteractionDef { special: Special::PokeFire, ..def("Poke Fire", 15.0, [0.0, 0.0, 0.0, 0.0, 0.0, 10.0], Pose::Use) },
];
static POOL_LADDER: [InteractionDef; 1] = [InteractionDef {
    special: Special::Swim,
    skill: Some("Athletic"),
    ..def("Swim", 60.0, [-6.0, -4.0, -12.0, 0.0, 10.0, 32.0], Pose::Use)
}];
static INSECT: [InteractionDef; 1] = [InteractionDef { special: Special::Catch, ..def("Catch", 2.0, [0.0, 0.0, 0.0, 0.0, -1.0, 12.0], Pose::Use) }];
static TOMBSTONE: [InteractionDef; 3] = [
    def("Mourn", 20.0, [0.0, 0.0, -2.0, 15.0, 0.0, -10.0], Pose::Stand),
    // (The game's `Urnstone.Engrave` and `ReadEpitaph`: the player's own.)
    InteractionDef { autonomous: false, special: Special::Engrave, ..def("Engrave Epitaph", 1.0, N, Pose::Stand) },
    InteractionDef { autonomous: false, special: Special::ReadEpitaph, ..def("Read Epitaph", 1.0, N, Pose::Stand) },
];
static GRILL: [InteractionDef; 1] = [InteractionDef {
    skill: Some("Cooking"),
    special: Special::ServeMeal,
    ..def("Grill", 40.0, [0.0, 0.0, 0.0, 0.0, -4.0, 15.0], Pose::Use)
}];
/// A cup of coffee: a lift for the tired, and a trip to the bathroom later.
static HOT_BEVERAGE: [InteractionDef; 2] = [
    def("Make Hot Beverage", 15.0, [24.0, -48.0, 80.0, 0.0, 0.0, 20.0], Pose::Use),
    // (The game's `ServeHotBeverages`: four cups on a tray, the machine's clips.)
    InteractionDef { autonomous: false, special: Special::ServeDrinks, ..def("Serve Hot Beverages", SERVE_DRINKS_MINUTES, N, Pose::Use) },
];
/// How long serving hot beverages takes (game minutes: its clips', the machine started and the
/// cups filled; the tray's taken after).
const SERVE_DRINKS_MINUTES: f32 = 4.7;
/// A drink from a tray (`BarTray.GetDrink`): as one made, without the making.
static DRINK_TRAY: [InteractionDef; 1] = [InteractionDef { special: Special::GrabDrink, ..def("Drink", 15.0, [24.0, -48.0, 80.0, 0.0, 0.0, 20.0], Pose::Use) }];
static TRASH: [InteractionDef; 2] =
    [
        InteractionDef { special: Special::EmptyTrash, ..def("Empty Trash", 6.0, [0.0, 0.0, 0.0, 0.0, -6.0, 0.0], Pose::Use) },
        // (Where the bag goes, outdoors: never on the menu.)
        InteractionDef { autonomous: false, special: Special::DropTrash, ..def("Throw Out Trash", 3.0, N, Pose::Use) },
    ];
/// How long knocking a can over and setting it up again take (game minutes: their clips').
const KNOCK_OVER_MINUTES: f32 = 6.0;
const PICK_UP_MINUTES: f32 = 7.4;
static OUTDOOR_TRASH: [InteractionDef; 4] = [
    // (Where the bag goes, outdoors: never on the menu.)
    InteractionDef { autonomous: false, special: Special::DropTrash, ..def("Throw Out Trash", 3.0, N, Pose::Use) },
    // (The game's `KnockOver`: fun 15 at once; `PickUp`; `Rummage`, a fifty of hygiene an hour,
    // till the seventh thing's turned up.)
    InteractionDef { special: Special::KnockOverCan, ..def("Knock Over", KNOCK_OVER_MINUTES, [0.0, 0.0, 0.0, 0.0, 0.0, 15.0 * 60.0 / KNOCK_OVER_MINUTES], Pose::Use) },
    InteractionDef { special: Special::PickUpCan, ..def("Pick Up", PICK_UP_MINUTES, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::Rummage, ..def("Rummage", 57.0, [0.0, 0.0, 0.0, 0.0, -50.0, 0.0], Pose::Use) },
];
static CLOTHING_PILE: [InteractionDef; 1] = [InteractionDef { special: Special::PickUpClothes, ..def("Pick Up", 1.5, [0.0, 0.0, 0.0, 0.0, -1.0, 0.0], Pose::Use) }];
static HAMPER: [InteractionDef; 1] = [InteractionDef { special: Special::EmptyHamper, ..def("Do Laundry", 1.5, N, Pose::Use) }];
static WASHER: [InteractionDef; 2] = [
    InteractionDef { special: Special::DoLaundry, ..def("Do Laundry", 4.0, [0.0, 0.0, 0.0, 0.0, -2.0, 0.0], Pose::Use) },
    InteractionDef { special: Special::MoveLaundry, ..def("Dry Clothes", 2.0, N, Pose::Use) },
];
static DRYER: [InteractionDef; 2] = [
    // (From the washer: never on the menu.)
    InteractionDef { autonomous: false, special: Special::LoadDryer, ..def("Load the Dryer", 2.0, N, Pose::Use) },
    InteractionDef { special: Special::EmptyDryer, ..def("Empty Dryer", 2.0, N, Pose::Use) },
];
static CLOTHESLINE: [InteractionDef; 2] = [
    InteractionDef { autonomous: false, special: Special::LoadDryer, ..def("Hang Clothes", 5.0, N, Pose::Use) },
    InteractionDef { special: Special::EmptyDryer, ..def("Take Down Clothes", 4.0, N, Pose::Use) },
];
/// (Down the chute with it: never on the menu.)
static WASTE_CHUTE: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::DropTrash, ..def("Throw Out Trash", 2.0, N, Pose::Use) }];
static FISHBOWL: [InteractionDef; 4] = [
    // (As long as the game's stocking and scooping out take: 4.3 and 5.7 seconds.)
    InteractionDef { autonomous: false, special: Special::PlaceFish, ..def("Place Fish", 4.3, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::TakeFish, ..def("Take Fish", 5.7, N, Pose::Use) },
    // (The game's `Feed` and `CleanOut`: fed when hungry of their own accord too.)
    InteractionDef { special: Special::FeedFish, ..def("Feed Fish", 2.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::CleanBowl, ..def("Clean Out", 3.0, N, Pose::Use) },
];
static ALARM: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::ToggleAlarm, ..def("Set Alarm", 1.0, N, Pose::Use) }];
static DISHES: [InteractionDef; 1] = [InteractionDef { special: Special::CleanUp, ..def("Clean Up", 4.0, [0.0, 0.0, 0.0, 0.0, -2.0, 0.0], Pose::Use) }];
/// How a plate of food fills hunger, per hour, and how long it takes to eat.
const MEAL_PER_HOUR: f32 = 320.0;
const MEAL_MINUTES: f32 = 25.0;
static MICROWAVE: [InteractionDef; 2] = [
    InteractionDef { special: Special::Microwave, ..def("Microwave Dinner", 15.0, [150.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use) },
    // (Once it's heated: never on the menu.)
    InteractionDef { autonomous: false, special: Special::Cook, ..def("Take Out Dinner", 2.0, N, Pose::Use) },
];
static BED: [InteractionDef; 7] = [
    InteractionDef {
        until_full: Some(ENERGY),
        on_object: true,
        decay: SLEEP_DECAY,
        ..def("Sleep", 600.0, [0.0, 0.0, 24.0, 0.0, 0.0, 0.0], Pose::Lie)
    },
    InteractionDef { on_object: true, decay: SLEEP_DECAY, ..def("Nap", NAP_MINUTES, [0.0, 0.0, 26.0, 0.0, 0.0, 0.0], Pose::Lie) },
    InteractionDef { on_object: true, ..def("Relax", 45.0, [0.0, 0.0, 6.0, 0.0, 0.0, 14.0], Pose::Lie) },
    // (Never on the bed's menu: the socials' when they're accepted.)
    InteractionDef { autonomous: false, on_object: true, special: Special::WooHoo, ..def("WooHoo", 25.0, WOOHOO_RATES, Pose::Lie) },
    InteractionDef { autonomous: false, on_object: true, special: Special::TryForBaby, ..def("Try for Baby", 25.0, WOOHOO_RATES, Pose::Lie) },
    // (From beside it, the game's routing slot: the covers pulled up, smoothed and patted.)
    InteractionDef { special: Special::MakeBed, ..def("Make Bed", 7.0, [0.0, 0.0, 0.0, 0.0, -2.0, 0.0], Pose::Use) },
    // (The game's `DayDream`: offered while relaxing there, lying on; ten fun an hour, and the
    // relaxing's rest.)
    InteractionDef {
        autonomous: false,
        on_object: true,
        until_full: Some(FUN),
        special: Special::Daydream,
        ..def("Daydream", 60.0, [0.0, 0.0, 6.0, 0.0, 0.0, 10.0], Pose::Lie)
    },
];
/// WooHoo's needs per hour: fun and company, at some cost to energy and hygiene.
const WOOHOO_RATES: [f32; 6] = [0.0, 0.0, -8.0, 180.0, -20.0, 160.0];
static TOILET: [InteractionDef; 4] = [
    InteractionDef { until_full: Some(BLADDER), on_object: true, special: Special::UseToilet, ..def("Use Toilet", 15.0, [0.0, 900.0, 0.0, 0.0, -10.0, 0.0], Pose::Sit) },
    SCRUB,
    // (The game's `Toilet.Flush`: offered while it needs one.)
    InteractionDef { special: Special::Flush, ..def("Flush", 1.6, N, Pose::Use) },
    // (The game's `Toilet.Vomit`: only when nausea runs out, `nausea`.)
    InteractionDef { autonomous: false, special: Special::Vomit, ..def(crate::nausea::THROW_UP, 4.0, N, Pose::Use) },
];
/// How long a rummage's putting the lid back takes (seconds, its clip's).
const RUMMAGE_CLOSE_SECS: f32 = 4.5;
/// Cleaning something dirty (the game's `CleanableComponent.Clean`: its grime gone in ten
/// minutes or so).
const SCRUB: InteractionDef = InteractionDef { special: Special::Scrub, ..def("Clean", 10.0, [0.0, 0.0, -2.0, 0.0, -10.0, -4.0], Pose::Use) };
const TAKE_SHOWER: InteractionDef = InteractionDef { until_full: Some(HYGIENE), on_object: true, ..def("Take Shower", 30.0, [0.0, 0.0, 0.0, 0.0, 400.0, 10.0], Pose::Use) };
const TAKE_BATH: InteractionDef = InteractionDef { until_full: Some(HYGIENE), on_object: true, ..def("Take Bath", 50.0, [0.0, 0.0, 6.0, 0.0, 260.0, 40.0], Pose::Lie) };
// (With bubble bath in the house: a longer soak, and more fun.)
const BUBBLE_BATH: InteractionDef =
    InteractionDef { until_full: Some(HYGIENE), on_object: true, special: Special::BubbleBath, ..def("Take Bubble Bath", 60.0, [0.0, 0.0, 10.0, 0.0, 300.0, 70.0], Pose::Lie) };
// (With a rubber duck in the house.)
const DUCKY: InteractionDef = InteractionDef { on_object: true, special: Special::Ducky, ..def("Play with Rubber Ducky", 30.0, [0.0, 0.0, 4.0, 0.0, 160.0, 100.0], Pose::Lie) };
static SHOWER: [InteractionDef; 2] = [TAKE_SHOWER, SCRUB];
static BATHTUB: [InteractionDef; 4] = [TAKE_BATH, BUBBLE_BATH, DUCKY, SCRUB];
/// A shower over a tub (the game's shower-tubs): a shower or a bath.
static SHOWER_TUB: [InteractionDef; 5] = [TAKE_SHOWER, TAKE_BATH, BUBBLE_BATH, DUCKY, SCRUB];
static SINK: [InteractionDef; 6] = [
    def("Wash Hands", 6.0, [0.0, 0.0, 0.0, 0.0, 160.0, 0.0], Pose::Use),
    // (Where a dish cleared away is taken without a dishwasher: never on the menu.)
    InteractionDef { autonomous: false, special: Special::WashDishes, ..def("Wash Dishes", 8.0, N, Pose::Use) },
    // (The game's `Sink.BrushTeeth`: Minty Breath after, coffee and garlic breath gone.)
    def("Brush Teeth", 5.0, [0.0, 0.0, 0.0, 0.0, 120.0, 0.0], Pose::Use),
    SCRUB,
    InteractionDef { special: Special::NeuroticCheck, ..def("Check Sink", 3.9, N, Pose::Use) },
    // (The game's `SpongeBath`: its ITUN's Hygiene 400 an hour, till clean; advertised weakly
    // beside a shower's, left to the player here.)
    InteractionDef { autonomous: false, until_full: Some(HYGIENE), special: Special::SpongeBath, ..def("Sponge Bath", 30.0, [0.0, 0.0, 0.0, 0.0, 400.0, 0.0], Pose::Use) },
];
/// (A flame lit and blown out: the fireplace's, smaller.)
static CANDLE: [InteractionDef; 2] = [
    InteractionDef { autonomous: false, special: Special::LightFire, ..def("Light Candle", 1.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::PutOutFire, ..def("Blow Out", 1.0, N, Pose::Use) },
];
/// (Loaded with the dishes cleared away: never on the menu.)
static DISHWASHER: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::WashDishes, ..def("Load Dishes", 3.0, N, Pose::Use) }];
static SOFA: [InteractionDef; 5] = [
    InteractionDef { on_object: true, ..def("Sit", 40.0, [0.0, 0.0, 5.0, 0.0, 0.0, 10.0], Pose::Sit) },
    InteractionDef { on_object: true, decay: SLEEP_DECAY, ..def("Nap", 60.0, [0.0, 0.0, 18.0, 0.0, 0.0, 0.0], Pose::Lie) },
    READ_SEATED,
    READ_PAPER_SEATED,
    WATCH_TV_SEATED,
];
/// (A book from the shelf, read sitting down: never on the menu.)
const READ_SEATED: InteractionDef =
    InteractionDef { on_object: true, autonomous: false, special: Special::ReadBook, ..def("Read Book", 60.0, [0.0, 0.0, 4.0, 0.0, 0.0, 30.0], Pose::Sit) };
static CHAIR: [InteractionDef; 8] = [
    InteractionDef { on_object: true, autonomous: false, ..def("Sit", 30.0, [0.0, 0.0, 4.0, 0.0, 0.0, 4.0], Pose::Sit) },
    InteractionDef {
        on_object: true,
        autonomous: false,
        special: Special::EatMeal,
        ..def("Eat", MEAL_MINUTES, [MEAL_PER_HOUR, -4.0, 0.0, 20.0, -4.0, 6.0], Pose::Sit)
    },
    InteractionDef { on_object: true, special: Special::Homework, ..def("Do Homework", 100.0 / crate::rabbitholes::HOMEWORK_BASE_RATE, [0.0, 0.0, -2.0, 0.0, 0.0, -6.0], Pose::Sit) },
    READ_SEATED,
    READ_PAPER_SEATED,
    WATCH_TV_SEATED,
    // (At a chess table: never on the chair's menu.)
    InteractionDef { on_object: true, autonomous: false, skill: Some("Logic"), special: Special::PlayChess, ..def("Play Chess", 90.0, CHESS_PER_HOUR, Pose::Sit) },
    InteractionDef { on_object: true, autonomous: false, skill: Some("Logic"), special: Special::ChessMatch, ..def("Play a Ranked Match", 120.0, RANKED_PER_HOUR, Pose::Sit) },
];
/// The dining chair's "Eat" (not offered in its menu).
pub const CHAIR_EAT: usize = 1;
static TV: [InteractionDef; 6] = [
    InteractionDef { special: Special::TurnOnTv, ..def("Watch TV", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 55.0], Pose::Stand) },
    // (Fit Net: working out along with it, as the game's TV "Work Out".)
    InteractionDef { skill: Some("Athletic"), ..def("Work Out", 60.0, [-10.0, 0.0, -25.0, 0.0, -40.0, 5.0], Pose::Exercise) },
    // (Watched standing when there's no seat facing it free: never on the menu.)
    WATCH_TV,
    InteractionDef { autonomous: false, special: Special::TvTurnOn, ..def("Turn On", 0.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::TvTurnOff, ..def("Turn Off", 0.0, N, Pose::Use) },
    SABOTAGE,
];
/// (A Technophobe's, at a TV, computer or stereo: the game's `SabotageTV`, half an hour.)
const SABOTAGE: InteractionDef = InteractionDef { autonomous: false, special: Special::Sabotage, ..def("Sabotage", 30.0, N, Pose::Use) };
/// (The TV watched: from a seat facing it, or standing.)
const WATCH_TV: InteractionDef = InteractionDef { autonomous: false, special: Special::WatchTv, ..def("Watch the TV", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 55.0], Pose::Stand) };
const WATCH_TV_SEATED: InteractionDef = InteractionDef { on_object: true, pose: Pose::Sit, per_hour: [0.0, 0.0, 3.0, 0.0, 0.0, 55.0], ..WATCH_TV };
static COMPUTER: [InteractionDef; 18] = [
    def("Play Computer Games", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 60.0], Pose::Use),
    // (The game's `ChatRandom`: its ITUN's Social 50, Fun 15 an hour.)
    InteractionDef { special: Special::ChatOnline, ..def("Chat Online", 60.0, [0.0, 0.0, -2.0, 50.0, 0.0, 15.0], Pose::Use) },
    InteractionDef { autonomous: false, skill: Some("Writing"), special: Special::WriteNovel, ..def("Write Novel", 120.0, [0.0, 0.0, -4.0, 0.0, 0.0, 10.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::FindJob, ..def("Find Job", 20.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::QuitJob, ..def("Quit Job", 5.0, N, Pose::Use) },
    InteractionDef { skill: Some("Logic"), ..def("Play Chess", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 40.0], Pose::Use) },
    // (The game's `BrowseWeb` and `PracticeWriting`.)
    def("Browse Web", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 35.0], Pose::Use),
    InteractionDef { skill: Some("Writing"), ..def("Practice Writing", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 5.0], Pose::Use) },
    // (The game's trait ones, as its tuning has them: `SolveHardProblems` (two hours, Logic,
    // fun 10 an hour), `WorkOnJobRelatedIssues` (four hours), `JoinBookClub`, `LeaveBookClub`.)
    InteractionDef { autonomous: false, skill: Some("Logic"), special: Special::SolveProblems, ..def("Solve Hard Problems", 120.0, [0.0, 0.0, -3.0, 0.0, 0.0, 10.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::JobIssues, ..def("Work on Job-Related Issues", 240.0, [0.0, 0.0, -4.0, 0.0, 0.0, 0.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::JoinBookClub, ..def("Join Book Club", 5.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::LeaveBookClub, ..def("Leave Book Club", 5.0, N, Pose::Use) },
    // (The game's `Hack`: till morning; its fun for a Computer Whiz.)
    InteractionDef { autonomous: false, special: Special::Hack, ..def("Hack", 600.0, [0.0, 0.0, -3.0, 0.0, 0.0, 30.0], Pose::Use) },
    // (The game's `WorkFromHome`, its four-hour stage, a little less fun; `Overclock`, an hour.)
    InteractionDef { special: Special::WorkFromHome, ..def("Work from Home", 240.0, [0.0, 0.0, -3.0, 0.0, 0.0, -10.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::Overclock, ..def("Overclock", 60.0, N, Pose::Use) },
    // (The game's `CheckRealEstate`: two minutes' typing, then its dialog.)
    InteractionDef { autonomous: false, special: Special::CheckRealEstate, ..def("Check Real Estate", 2.0, N, Pose::Use) },
    SABOTAGE,
    // (The game's `WriteReport`: a report or story typed up, till it's done.)
    InteractionDef { autonomous: false, special: Special::WriteReport, ..def("Write Report", 150.0, N, Pose::Use) },
];
static CRIB: [InteractionDef; 1] = [InteractionDef {
    until_full: Some(ENERGY),
    decay: SLEEP_DECAY,
    on_object: true,
    ..def("Nap in Crib", 180.0, [0.0, 0.0, 140.0, 0.0, 0.0, 0.0], Pose::Lie)
}];
static TOYBOX: [InteractionDef; 1] = [def("Play with Toys", 40.0, [0.0, 0.0, -4.0, 30.0, 0.0, 140.0], Pose::Use)];
static SNOWMAN: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::Clear, ..def("Destroy Snowman", 9.0, [0.0, 0.0, -4.0, 0.0, 0.0, 40.0], Pose::Use) }];
static SNOW_ANGEL: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::Clear, ..def("Destroy Snow Angel", 6.0, [0.0, 0.0, -2.0, 0.0, 0.0, 30.0], Pose::Use) }];
/// (Raked up, the pile's gone.)
static LEAF_PILE: [InteractionDef; 3] = [
    def("Play in Leaves", 20.0, [0.0, 0.0, -6.0, 0.0, -10.0, 110.0], Pose::Use),
    def("Jump In", 6.0, [0.0, 0.0, -4.0, 0.0, -10.0, 140.0], Pose::Use),
    InteractionDef { special: Special::Clear, ..def("Rake Up", 15.0, [0.0, 0.0, -6.0, 0.0, -6.0, -4.0], Pose::Use) },
];
static URINAL: [InteractionDef; 1] = [InteractionDef { until_full: Some(BLADDER), ..def("Use Urinal", 10.0, [0.0, 1100.0, 0.0, 0.0, -10.0, 0.0], Pose::Use) }];
static TOY_OVEN: [InteractionDef; 1] = [def("Play with Toy Oven", 40.0, [0.0, 0.0, -4.0, 6.0, 0.0, 110.0], Pose::Use)];
/// (Lit as a fireplace is, its fire burning a few hours.)
static FIRE_PIT: [InteractionDef; 4] = [
    InteractionDef { special: Special::LightFire, ..def("Light Fire", 6.0, [0.0, 0.0, 0.0, 0.0, 0.0, 4.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::PutOutFire, ..def("Put Out Fire", 7.0, [0.0; 6], Pose::Use) },
    def("Warm Hands", 20.0, [0.0, 0.0, 4.0, 0.0, 0.0, 16.0], Pose::Use),
    def("Roast Marshmallows", 25.0, [110.0, 0.0, 0.0, 0.0, -4.0, 45.0], Pose::Use),
];
static BUFFET: [InteractionDef; 1] = [def("Have a Plate", 15.0, [220.0, 0.0, 0.0, 8.0, 0.0, 6.0], Pose::Use)];
/// (Sat at by up to four, each at a place of their own: see `GameObject::picnic_places`; eaten
/// at as a dining chair is, its Eat at the same place in the list, `CHAIR_EAT`.)
static PICNIC_TABLE: [InteractionDef; 2] = [
    InteractionDef { on_object: true, autonomous: false, ..def("Sit", 30.0, [0.0, 0.0, 4.0, 0.0, 0.0, 4.0], Pose::Sit) },
    InteractionDef {
        on_object: true,
        autonomous: false,
        special: Special::EatMeal,
        ..def("Eat", MEAL_MINUTES, [MEAL_PER_HOUR, -4.0, 0.0, 20.0, -4.0, 6.0], Pose::Sit)
    },
];
/// (Its menu is the trips it can take: see `hud::world_click`.)
static CAR: [InteractionDef; 1] = [InteractionDef { autonomous: false, ..def("Go to Work", 1.0, N, Pose::Use) }];
static PICNIC: [InteractionDef; 1] = [def("Have a Picnic", 30.0, [150.0, 0.0, 0.0, 10.0, 0.0, 25.0], Pose::Use)];
static XYLOPHONE: [InteractionDef; 1] = [def("Play Xylophone", 40.0, [0.0, 0.0, -4.0, 20.0, 0.0, 130.0], Pose::Use)];
static PEGBOX: [InteractionDef; 1] = [def("Play with Peg Box", 40.0, [0.0, 0.0, -4.0, 10.0, 0.0, 120.0], Pose::Use)];
/// (Its menu's the locks: see `hud::world_click` and `doorlocks`.)
static DOOR: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::LockDoor, ..def("Lock", 0.0, N, Pose::Use) }];
/// (Its menu's the switches: see `hud::world_click` and `lights`.)
static LIGHT: [InteractionDef; 1] = [InteractionDef { autonomous: false, special: Special::LightSwitch, ..def("Lights", 0.0, N, Pose::Use) }];
/// (Sat on, by toddlers potty trained, or a grown-up's to teach them on and empty: see `potty`.)
static POTTY: [InteractionDef; 4] = [
    InteractionDef { until_full: Some(BLADDER), on_object: true, ..def("Use Potty Chair", 15.0, [0.0, 600.0, 0.0, 0.0, -20.0, 0.0], Pose::Sit) },
    // (Waiting there while the toddler comes and sits: done when they are.)
    InteractionDef { autonomous: false, special: Special::PottyTrain, ..def("Potty Train", 120.0, [0.0, 0.0, -2.0, 10.0, 0.0, 5.0], Pose::Stand) },
    InteractionDef { special: Special::EmptyPotty, ..def("Empty", 3.0, N, Pose::Use) },
    // (Sat on to be taught, the quarter hour through, for the company: never on the menu.)
    InteractionDef { autonomous: false, on_object: true, special: Special::PottyLesson, ..def("Use Potty Chair", 15.0, [0.0, 600.0, 0.0, 30.0, -20.0, 0.0], Pose::Sit) },
];
/// (The game's `HighChair`: sat in by toddlers put in it, and eaten in once fed, never by their
/// own choice; put in, fed and taken out by teens and up.)
static HIGH_CHAIR: [InteractionDef; 4] = [
    // (Eating there, once fed, is `highchair`'s: hunger filled to the full over its half hour.)
    InteractionDef { autonomous: false, on_object: true, special: Special::HighChairSit, ..def("Sit in High Chair", 240.0, [0.0, 0.0, 2.0, 0.0, 0.0, 0.0], Pose::Sit) },
    InteractionDef { autonomous: false, special: Special::PutInHighChair, ..def("Put in High Chair", 60.0, N, Pose::Stand) },
    InteractionDef { autonomous: false, special: Special::GiveBabyFood, ..def("Give Baby Food", 3.0, [0.0, 0.0, 0.0, 6.0, 0.0, 0.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::TakeOutOfHighChair, ..def("Take Out", 2.0, N, Pose::Use) },
];
static DRESSER: [InteractionDef; 2] = [
    InteractionDef { autonomous: false, special: Special::ChangeClothes, ..def("Change Into", 4.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::PlanOutfit, ..def("Plan Outfit", 2.0, N, Pose::Use) },
];
static TELESCOPE: [InteractionDef; 2] = [
    InteractionDef { skill: Some("Logic"), ..def("Stargaze", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 50.0], Pose::Use) },
    // (The game's `SearchGalaxyInteraction`: till something new's found, a few hours at most.)
    InteractionDef { skill: Some("Logic"), autonomous: false, special: Special::SearchGalaxy, ..def("Search Galaxy", 360.0, [0.0, 0.0, -3.0, 0.0, 0.0, 30.0], Pose::Use) },
];
static SWINGSET: [InteractionDef; 1] = [def("Swing", 30.0, [0.0, 0.0, -6.0, 0.0, -4.0, 75.0], Pose::Use)];
static HOTTUB: [InteractionDef; 1] = [InteractionDef {
    on_object: true,
    ..def("Relax in Hot Tub", 60.0, [0.0, 0.0, 6.0, 10.0, 25.0, 50.0], Pose::Sit)
}];
static DOLLHOUSE: [InteractionDef; 3] = [
    def("Play with Dollhouse", 45.0, [0.0, 0.0, -3.0, 8.0, 0.0, 85.0], Pose::Use),
    // (The game's `Smash`: in a temper, eight minutes of it; `DisposeDollHouse` once it's smashed.)
    InteractionDef { special: Special::SmashDollhouse, ..def("Smash", crate::dollhouse::ADULT_MINUTES, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::DisposeDollhouse, ..def("Throw Away", 2.0, N, Pose::Use) },
];
static JUNGLEGYM: [InteractionDef; 1] = [def("Play on Jungle Gym", 40.0, [0.0, 0.0, -8.0, 0.0, -8.0, 95.0], Pose::Use)];
/// (Up the ladder and down the slide, once: played again and again, as children do.)
static SLIDE: [InteractionDef; 1] = [def("Play on Slide", 9.6, [0.0, 0.0, -8.0, 0.0, -6.0, 110.0], Pose::Use)];
/// (The game's `kTimeToMopPuddle`: five minutes with the mop.)
/// A pet bowl filled (the game's `FillBowl`, §25 a bite it's short: `petneeds`), by grown-ups of the
/// household of their own accord once it's down to three bites; a litter box cleaned out once it
/// needs it (the game's `CleanLitterBox`).
static PET_BOWL: [InteractionDef; 1] = [InteractionDef { special: Special::FillPetBowl, ..def("Fill Bowl", 3.0, N, Pose::Use) }];
/// A terrarium's menu: Stock, Fill Food, Watch, Show Info, Clean, Mourn (the game's).
static TERRARIUM: [InteractionDef; 6] = [
    InteractionDef { autonomous: false, special: Special::StockTerrarium, ..def("Stock", 3.0, N, Pose::Use) },
    InteractionDef { special: Special::FillTerrarium, ..def("Fill Food", 2.0, N, Pose::Use) },
    InteractionDef { special: Special::WatchMinorPet, ..def("Watch", 10.0, [0.0, 0.0, 0.0, 0.0, 0.0, crate::terrarium::WATCH_FUN], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::MinorPetInfo, ..def("Show Info", 0.1, N, Pose::Use) },
    InteractionDef { special: Special::Scrub, ..def("Clean", 8.0, [0.0, 0.0, 0.0, 0.0, -10.0, 0.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::MournMinorPet, ..def("Mourn", 4.0, N, Pose::Use) },
];
/// A bird cage's menu: the terrarium's, and a large bird talked with and taught to talk (the
/// pack's `TalkWith`, `TeachToTalk`: ten minutes each).
static BIRD_CAGE: [InteractionDef; 8] = [
    InteractionDef { autonomous: false, special: Special::StockTerrarium, ..def("Stock", 3.0, N, Pose::Use) },
    InteractionDef { special: Special::FillTerrarium, ..def("Fill Food", 2.0, N, Pose::Use) },
    InteractionDef { special: Special::WatchMinorPet, ..def("Watch", 10.0, [0.0, 0.0, 0.0, 0.0, 0.0, crate::terrarium::WATCH_FUN], Pose::Use) },
    InteractionDef { special: Special::TalkWithBird, ..def("Talk With", 10.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::TeachBirdToTalk, ..def("Teach To Talk", 10.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::MinorPetInfo, ..def("Show Info", 0.1, N, Pose::Use) },
    InteractionDef { special: Special::Scrub, ..def("Clean", 8.0, [0.0, 0.0, 0.0, 0.0, -10.0, 0.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::MournMinorPet, ..def("Mourn", 4.0, N, Pose::Use) },
];
/// A slippy slide: a run at it and down it on their front (the game's `Slide`: Fun 75 an hour),
/// again and again till they've had their fun.
static SLIPPY_SLIDE: [InteractionDef; 1] = [InteractionDef { special: Special::SlippySlide, ..def("Slide", 8.0, [0.0, 0.0, -6.0, 0.0, 0.0, 75.0], Pose::Use) }];
/// A box stall's: cleaned out (the game's `Clean`: three minutes a dirty spot, five at least).
static BOX_STALL: [InteractionDef; 1] = [InteractionDef { special: Special::Scrub, ..def("Clean Stall", 15.0, [0.0, 0.0, -4.0, 0.0, -15.0, 0.0], Pose::Use) }];
static LITTER_BOX: [InteractionDef; 1] = [InteractionDef { special: Special::Scrub, ..def("Clean", 6.0, [0.0, 0.0, 0.0, 0.0, -20.0, 0.0], Pose::Use) }];
static PUDDLE: [InteractionDef; 1] = [InteractionDef { special: Special::MopUp, ..def("Mop Up", 5.0, [0.0, 0.0, -2.0, 0.0, -4.0, -4.0], Pose::Use) }];
static FOOSBALL: [InteractionDef; 1] = [def("Play Foosball", 40.0, [0.0, 0.0, -4.0, 10.0, 0.0, 65.0], Pose::Use)];
static POOL_TABLE: [InteractionDef; 1] = [def("Play Pool", 45.0, [0.0, 0.0, -3.0, 0.0, 0.0, 60.0], Pose::Use)];
static DARTBOARD: [InteractionDef; 1] = [def("Throw Darts", 30.0, [0.0, 0.0, -3.0, 0.0, 0.0, 55.0], Pose::Use)];
static BOWLING: [InteractionDef; 1] = [def("Bowl", 45.0, [0.0, 0.0, -6.0, 0.0, -4.0, 75.0], Pose::Use)];
static KARAOKE: [InteractionDef; 1] = [def("Sing Karaoke", 30.0, [0.0, 0.0, -4.0, 6.0, 0.0, 60.0], Pose::Use)];
/// An arcade machine played (its game the Arcade Machine skill's: `ArcadeMachineBase`).
static ARCADE: [InteractionDef; 1] = [InteractionDef { skill: Some("ArcadeMachine"), ..def("Play Arcade Game", 40.0, [0.0, 0.0, -3.0, 0.0, 0.0, 70.0], Pose::Use) }];
static CLAW: [InteractionDef; 1] = [def("Play Claw Machine", 15.0, [0.0, 0.0, -2.0, 0.0, 0.0, 60.0], Pose::Use)];
static WHACK: [InteractionDef; 1] = [def("Play Whack-a-Gnome", 20.0, [0.0, 0.0, -4.0, 0.0, 0.0, 70.0], Pose::Use)];
static SKEEBALL: [InteractionDef; 1] = [def("Play Skee-Ball", 25.0, [0.0, 0.0, -3.0, 0.0, 0.0, 65.0], Pose::Use)];
static HORSESHOES: [InteractionDef; 1] = [def("Practice Horseshoes", 30.0, [0.0, 0.0, -4.0, 0.0, -3.0, 55.0], Pose::Use)];
static SHUFFLEBOARD: [InteractionDef; 1] = [def("Practice Shuffleboard", 30.0, [0.0, 0.0, -3.0, 0.0, 0.0, 55.0], Pose::Use)];
static MECH_BULL: [InteractionDef; 1] = [InteractionDef { skill: Some("Athletic"), ..def("Ride Mechanical Bull", 15.0, [0.0, 0.0, -10.0, 0.0, -8.0, 90.0], Pose::Use) }];
static TRAMPOLINE: [InteractionDef; 1] = [InteractionDef { skill: Some("Athletic"), ..def("Jump on Trampoline", 30.0, [0.0, 0.0, -10.0, 0.0, -8.0, 90.0], Pose::Use) }];
static SANDBOX: [InteractionDef; 1] = [def("Play in Sandbox", 40.0, [0.0, 0.0, -4.0, 0.0, -10.0, 90.0], Pose::Use)];
/// (Each instrument's play, its play for tips (the same, named "Play for Tips" on the menu: see
/// `tips`), and watching whoever's playing it, the game's Fun 25 to playing's 10, until
/// they've had their fill or the playing's over.)
const PLAY_PIANO: InteractionDef = InteractionDef { skill: Some("Piano"), ..def("Play Piano", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 45.0], Pose::Use) };
const PLAY_KEYBOARD: InteractionDef = InteractionDef { skill: Some("Piano"), ..def("Play Keyboard", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 45.0], Pose::Use) };
const PLAY_DRUMS: InteractionDef = InteractionDef { skill: Some("Drums"), ..def("Play Drums", 60.0, [0.0, 0.0, -5.0, 0.0, -2.0, 50.0], Pose::Use) };
const PLAY_LASER_HARP: InteractionDef = InteractionDef { skill: Some("LaserHarp"), ..def("Play Laser Harp", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 50.0], Pose::Use) };
const PLAY_GUITAR: InteractionDef = InteractionDef { skill: Some("Guitar"), ..def("Play Guitar", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 45.0], Pose::Use) };
const WATCH_MUSIC: InteractionDef = InteractionDef { special: Special::WatchMusic, until_full: Some(FUN), ..def("Watch", 120.0, [0.0, 0.0, -2.0, 0.0, 0.0, 60.0], Pose::Stand) };
/// Dancing to someone playing well (the game's `MusicalInstrument.DanceBase`, from their fifth
/// level: `LevelDanceBecomesAvailable`), 2 to 4 m off: as dancing to the stereo.
const DANCE_MUSIC: InteractionDef = InteractionDef { special: Special::DanceToMusic, until_full: Some(FUN), ..def("Dance", 45.0, [0.0, 0.0, -6.0, 0.0, -6.0, 70.0], Pose::Dance) };
const fn for_tips(play: InteractionDef) -> InteractionDef {
    InteractionDef { special: Special::PlayForTips, ..play }
}
/// (The game's `Perform`: by teens and up, ten fun an hour, till the song's over.)
const fn perform(play: InteractionDef) -> InteractionDef {
    InteractionDef { special: Special::Perform, autonomous: false, minutes: 240.0, per_hour: [0.0, 0.0, play.per_hour[2], 0.0, 0.0, 10.0], ..play }
}
static PIANO: [InteractionDef; 5] = [PLAY_PIANO, for_tips(PLAY_PIANO), WATCH_MUSIC, perform(PLAY_PIANO), DANCE_MUSIC];
static KEYBOARD: [InteractionDef; 5] = [PLAY_KEYBOARD, for_tips(PLAY_KEYBOARD), WATCH_MUSIC, perform(PLAY_KEYBOARD), DANCE_MUSIC];
static DRUMS: [InteractionDef; 5] = [PLAY_DRUMS, for_tips(PLAY_DRUMS), WATCH_MUSIC, perform(PLAY_DRUMS), DANCE_MUSIC];
static LASER_HARP: [InteractionDef; 5] = [PLAY_LASER_HARP, for_tips(PLAY_LASER_HARP), WATCH_MUSIC, perform(PLAY_LASER_HARP), DANCE_MUSIC];
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
/// (Art: looked over a while, for the fun of it.)
static ART: [InteractionDef; 1] = [InteractionDef { special: Special::ViewArt, ..def("View", 10.0, [0.0, 0.0, 0.0, 0.0, 0.0, 70.0], Pose::Use) }];
/// (A plate of whatever's programmed, as the replicator's: the screen tapped, a wait, and out.)
static FOOD_SYNTHESIZER: [InteractionDef; 1] =
    [InteractionDef { special: Special::ReplicateFood, ..def("Synthesize Food", 4.0, [4500.0, 0.0, 0.0, 0.0, 0.0, 2.0], Pose::Use) }];
/// (A soda or a candy bar, had there and then: the game's `kHungerFillAmount`, 5, over the
/// quarter of an hour it takes.)
const VEND_SODA: InteractionDef = InteractionDef { special: Special::Vend, ..def("Buy Soda", 12.0, [25.0, 0.0, 0.0, 0.0, 0.0, 5.0], Pose::Use) };
const VEND_CANDY: InteractionDef = InteractionDef { special: Special::Vend, ..def("Buy Candy", 12.0, [25.0, 0.0, 0.0, 0.0, 0.0, 5.0], Pose::Use) };
const VEND_STUCK: InteractionDef = InteractionDef { autonomous: false, special: Special::VendStuck, ..def("Stuck", 2.0, N, Pose::Use) };
const VEND_SHAKE: InteractionDef = InteractionDef { autonomous: false, special: Special::VendShake, ..def("Shake Machine", 2.0, N, Pose::Use) };
const VEND_SLAM: InteractionDef = InteractionDef { autonomous: false, special: Special::VendShake, ..def("Slam Machine", 2.0, N, Pose::Use) };
static VENDING_SODA: [InteractionDef; 4] = [VEND_SODA, VEND_SHAKE, VEND_SLAM, VEND_STUCK];
static VENDING_CANDY: [InteractionDef; 4] = [VEND_CANDY, VEND_SHAKE, VEND_SLAM, VEND_STUCK];
static VENDING_BOTH: [InteractionDef; 5] = [VEND_SODA, VEND_CANDY, VEND_SHAKE, VEND_SLAM, VEND_STUCK];
/// How often a purchase sticks in the machine (the game's `kChanceItemStuck`).
const VEND_STUCK_CHANCE: f64 = 0.1;
/// Shaking or slamming a vending machine (the game's tuning): the chance of a snack for free,
/// of shaking loose one that's stuck, and of the machine toppling onto them.
const SHAKE_ODDS: (f64, f64, f64) = (0.05, 0.2, 0.05);
const SLAM_ODDS: (f64, f64, f64) = (0.15, 0.4, 0.15);

/// A vending machine with a snack stuck in it.
#[derive(Component)]
pub struct SnackStuck;
/// What a mooched snack does for hunger.
const MOOCHED_SNACK: f32 = 20.0;
/// What a vending machine's snack costs away from home (the game's `kSodaPrice`, `kCandyPrice`).
pub const VEND_PRICE: i64 = 5;
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
static STEREO: [InteractionDef; 6] = [
    def("Dance", 45.0, [0.0, 0.0, -6.0, 0.0, -6.0, 70.0], Pose::Dance),
    InteractionDef { skill: Some("Athletic"), special: Special::CardioWorkOut, ..def("Cardio Exercise", 60.0, [-10.0, 0.0, -25.0, 0.0, -40.0, 5.0], Pose::Exercise) },
    InteractionDef { skill: Some("Athletic"), special: Special::StrengthWorkOut, ..def("Strength Training", 60.0, [-10.0, 0.0, -25.0, 0.0, -40.0, 5.0], Pose::Exercise) },
    // (The game's `TurnOnOff`: a button pressed.)
    InteractionDef { autonomous: false, special: Special::StereoOn, ..def("Turn On", 1.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::StereoOff, ..def("Turn Off", 1.0, N, Pose::Use) },
    SABOTAGE,
];
/// (A book taken down to read in a seat; read standing there when no seat is free.)
static BOOKSHELF: [InteractionDef; 3] = [
    InteractionDef { special: Special::GetBook, ..def("Read a Book", 60.0, [0.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Stand) },
    InteractionDef { autonomous: false, special: Special::ReadBook, ..def("Read Book", 60.0, [0.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Stand) },
    InteractionDef { autonomous: false, special: Special::ReturnBook, ..def(crate::surroundings::PUT_BOOK_AWAY, 2.5, N, Pose::Use) },
];
static MIRROR: [InteractionDef; 6] = [
    InteractionDef { autonomous: false, skill: Some("Charisma"), ..def("Practice Speech", 40.0, [0.0, 0.0, -2.0, 6.0, 0.0, 10.0], Pose::Talk) },
    InteractionDef { autonomous: false, special: Special::ChangeAppearance, ..def("Change Appearance", 2.0, N, Pose::Use) },
    // (The game's, ten minutes each; making faces eight, fun 25 an hour.)
    InteractionDef { special: Special::AdmireSelf, ..def("Admire Self", 10.0, N, Pose::Use) },
    InteractionDef { special: Special::GussyUp, ..def("Gussy Up", 10.0, N, Pose::Use) },
    InteractionDef { special: Special::PlayWithMirror, ..def("Play with Mirror", 8.0, [0.0, 0.0, 0.0, 0.0, 0.0, 25.0], Pose::Use) },
    InteractionDef { special: Special::CheckSelfOut, ..def("Check Self Out", 10.0, N, Pose::Use) },
];
static EASEL: [InteractionDef; 1] = [InteractionDef {
    skill: Some("Painting"),
    special: Special::SellPainting,
    ..def("Paint", 90.0, [0.0, 0.0, -3.0, 0.0, 0.0, 40.0], Pose::Use)
}];
static GUITAR: [InteractionDef; 5] = [PLAY_GUITAR, for_tips(PLAY_GUITAR), WATCH_MUSIC, perform(PLAY_GUITAR), DANCE_MUSIC];
static TREADMILL: [InteractionDef; 1] = [InteractionDef {
    autonomous: false,
    skill: Some("Athletic"),
    ..def("Work Out", 60.0, [-10.0, 0.0, -25.0, 0.0, -40.0, 5.0], Pose::Exercise)
}];
/// (The game's `WorkOut.StrengthSingleton`, for teens and up.)
/// (Walked to from beside it, its routing slot; then on the seat, its clips played from the
/// bench's own origin.)
static WORKOUT_BENCH: [InteractionDef; 1] = [InteractionDef {
    autonomous: false,
    on_object: true,
    skill: Some("Athletic"),
    ..def("Pump Iron", 60.0, [-10.0, 0.0, -25.0, 0.0, -40.0, 5.0], Pose::Exercise)
}];
/// (Played from a chair at the table, the chair's own "Play Chess" or ranked match: see
/// `chess::take_seats`; watched from beside it, `kDistanceToWatch`, for the game's Fun 10 to
/// playing's 15, until they've had their fill or the game's over.)
static CHESS: [InteractionDef; 3] = [
    InteractionDef { skill: Some("Logic"), special: Special::PlayChess, ..def("Play Chess", 90.0, CHESS_PER_HOUR, Pose::Use) },
    InteractionDef { autonomous: false, skill: Some("Logic"), special: Special::ChessMatch, ..def("Play a Ranked Match", 120.0, RANKED_PER_HOUR, Pose::Use) },
    InteractionDef { special: Special::WatchChess, until_full: Some(FUN), ..def("Watch Chess Match", 240.0, [0.0, 0.0, -2.0, 0.0, 0.0, 27.0], Pose::Stand) },
];
const CHESS_PER_HOUR: [f32; 6] = [0.0, 0.0, -2.0, 0.0, 0.0, 40.0];
const RANKED_PER_HOUR: [f32; 6] = [0.0, 0.0, -2.0, 10.0, 0.0, 35.0];

/// Taking something out of the fridge, after opening it.
const FRIDGE_STEPS: &[&str] = &["a2o_fridge_takeFoodOut_plateDinner_x", "a2o_fridge_closeDoor_x"];

/// Sitting down into a seat and getting up out of it (and how long getting up takes): a chair's
/// or sofa's, or a bar stool's.
fn seat_clips(kind: ObjectKind) -> Option<(&'static str, &'static str, f32)> {
    match kind {
        ObjectKind::Chair | ObjectKind::Sofa => Some(("a2o_chairDining_getIn_x", "a2o_chairDining_getOut_x", 1.6)),
        ObjectKind::Stool => Some(("a2o_barstool_getIn_N_x", "a2o_barstool_getOut_N_x", 1.3)),
        ObjectKind::PicnicTable => Some(("a2o_picnicTable_getIn_x", "a2o_picnicTable_getOut_x", 1.6)),
        _ => None,
    }
}

/// The animation played while performing an interaction: start clip, then loop variants.
/// The workout bench's lifts, as the game's state machine plays them: bench presses, leg
/// extensions and pec flys, each started, done a few times and put down.
const BENCH_LIFTS: &[&str] = &[
    "a2o_workoutBench_benchPress_start_x",
    "a2o_workoutBench_benchPress_easy_x",
    "a2o_workoutBench_benchPress_easy_x",
    "a2o_workoutBench_benchPress_easy_x",
    "a2o_workoutBench_benchPress_stop_x",
    "a2o_workoutBench_legExtension_start_x",
    "a2o_workoutBench_legExtension_easy_x",
    "a2o_workoutBench_legExtension_easy_x",
    "a2o_workoutBench_legExtension_easy_x",
    "a2o_workoutBench_legExtension_stop_x",
    "a2o_workoutBench_pecFly_start_x",
    "a2o_workoutBench_pecFly_easy_x",
    "a2o_workoutBench_pecFly_easy_x",
    "a2o_workoutBench_pecFly_easy_x",
    "a2o_workoutBench_pecFly_stop_x",
];
/// How long getting off the bench takes (seconds).
const BENCH_OFF: f32 = 2.0;

/// WooHoo under the covers: into the bed and sat up, then the game's WooHoo (the bed's covers
/// rolling with it), its end, and out again; the one on the far side plays the other's part.
pub fn woohoo_clip(far_side: bool) -> crate::anim::ActionClip {
    use crate::anim::ActionClip as A;
    let (loops, end): (&'static [&'static str], &'static [&'static str]) = if far_side {
        (&["a2a_bed_cuddle_woohoo_loop_y"], &["a2a_bed_cuddle_woohoo_stop_y", "a2o_bed_relax_getout_x"])
    } else {
        (&["a2a_bed_cuddle_woohoo_loop_x"], &["a2a_bed_cuddle_woohoo_stop_x", "a2o_bed_relax_getout_x"])
    };
    A::steps("a2o_bed_relax_getin_start_x", &["a2o_bed_relax_getin_stop_x"], loops).ending(end, WOOHOO_END)
}
/// How long WooHoo's end and getting out of bed take (seconds).
const WOOHOO_END: f32 = 8.6;

/// A bath's start, the taps on: sat down, arms and legs washed, then lying back.
const BATH_WASH: &[&str] = &[
    "a2o_bathtub_takeBath_getIn_x",
    "a2o_bathtub_takeBath_washArm_start_x",
    "a2o_bathtub_takeBath_washArm_loop1_x",
    "a2o_bathtub_takeBath_washArm_loop2_x",
    "a2o_bathtub_takeBath_washArm_stop_x",
    "a2o_bathtub_takeBath_washLeg_start_x",
    "a2o_bathtub_takeBath_washLeg_loop1_x",
    "a2o_bathtub_takeBath_washLeg_loop2_x",
    "a2o_bathtub_takeBath_washLeg_stop_x",
    "a2o_bathtub_takeBath_trans_relax_x",
];

/// Whether a decoration (by its catalogue name) is art to be looked at: a painting or a
/// sculpture, not the towel rails, shelves and partitions the catalogue files with them.
fn is_art(name: &str) -> bool {
    name.starts_with("painting")
        || (name.starts_with("sculpture") && !["towel", "hutch", "clutter", "partition", "shelf", "fern", "plant", "light", "mirror", "rack"].iter().any(|k| name.contains(k)))
}

/// What a Sim makes of a piece of art (the game's `ViewHate`, `ViewSoSo`, `ViewLike` and
/// `ViewLove` reactions), by its price and their taste: those who can't stand art hate it all;
/// the Snobs and the Artistic love what's fine (and the Snobs think little of what's cheap);
/// others like it as it's dear. `roll` (0 to 1) settles the near things.
fn art_taste(sim: &crate::sim::Sim, price: i32, roll: f32) -> &'static str {
    use crate::life::Trait;
    let has = |t: Trait| sim.traits.contains(&t);
    if has(Trait::CantStandArt) {
        "hate"
    } else if has(Trait::Snob) && price < 300 {
        "soso"
    } else if has(Trait::Snob) || has(Trait::Artistic) {
        if price >= 800 || roll < 0.3 { "love" } else { "like" }
    } else if price >= 1500 && roll < 0.6 {
        "love"
    } else if price >= 300 || roll < 0.5 {
        "like"
    } else {
        "soso"
    }
}

/// A Sim's own roll for a thing (0 to 1), the same each time it's asked: what they make of a
/// piece of art doesn't change between looking and reacting.
fn taste_roll(sim: Entity, thing: Entity) -> f32 {
    let h = (sim.to_bits() ^ thing.to_bits().rotate_left(29)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Looking art over (a curious look, an admiring idle), then the reaction to it: the game's
/// `a_react_view_` clips (a child's `c_`).
fn view_clip(sim: &crate::sim::Sim, taste: &'static str) -> crate::anim::ActionClip {
    let child = sim.age == Age::Child;
    let ending: &'static [&'static str] = match (child, taste) {
        (false, "hate") => &["a_react_view_hate_x"],
        (false, "soso") => &["a_react_view_soso_x"],
        (false, "love") => &["a_react_view_love_x"],
        (false, _) => &["a_react_view_like_x"],
        (true, "hate") => &["c_react_view_hate_x"],
        (true, "soso") => &["c_react_view_soso_x"],
        (true, "love") => &["c_react_view_love_x"],
        (true, _) => &["c_react_view_like_x"],
    };
    let (start, looks): (&'static str, &'static [&'static str]) =
        if child { ("c_idle_neutral_look_upAndOver_x", &["c_idle_neutral_loop_1", "c_idle_neutral_look_rightLeft_x"]) } else { ("a_react_view_curious_x", &["a_soc_idle_neutral_admire_x", "a_idle_neutral_look_upAndOver_x"]) };
    crate::anim::ActionClip::new(Some(start), looks).ending(ending, 3.5)
}

/// The game's chance of falling on a slippy slide (`kChanceFail`: one in five).
pub const SLIDE_FALL_CHANCE: f32 = 0.2;

/// A Sim who fell on the slide this run (`Slipped` after).
#[derive(Component)]
pub struct SlideFall;

/// A run down the slippy slide: one of the game's three, or its fall; then stood at the end.
pub fn slide_clip(fell: bool, roll: f32) -> crate::anim::ActionClip {
    const RUNS: [&[&str]; 3] = [&["a2o_waterSlide_succeedSlide1_x"], &["a2o_waterSlide_succeedSlide2_x"], &["a2o_waterSlide_succeedSlide3_x"]];
    let run: &'static [&'static str] = if fell { &["a2o_waterSlide_failSlide1_x"] } else { RUNS[((roll * 3.0) as usize).min(2)] };
    crate::anim::ActionClip::new(Some(run[0]), &["a_idle_neutral_loop_"])
}

pub fn interaction_clip(name: &str, kind: ObjectKind) -> Option<crate::anim::ActionClip> {
    use crate::anim::ActionClip as A;
    Some(match name {
        // (Scrubbed with the game's sponge, or the toilet brush.)
        "Clean" if kind == ObjectKind::Toilet => A::new(Some("a2o_toilet_clean_start_x"), &["a2o_toilet_clean_loopA_x", "a2o_toilet_clean_loopB_x"]).ending(&["a2o_toilet_clean_stop_x"], 1.5),
        "Clean" if kind == ObjectKind::Shower => {
            A::new(Some("a2o_shower_cleanShower_start_x"), &["a2o_shower_cleanShower_loop1_x", "a2o_shower_cleanShower_loop2_x"]).ending(&["a2o_shower_cleanShower_stop_x"], 1.5)
        }
        "Clean" if matches!(kind, ObjectKind::Bathtub | ObjectKind::ShowerTub) => {
            A::new(Some("a2o_bathtub_clean_start_x"), &["a2o_bathtub_clean_loop1_x", "a2o_bathtub_clean_loop2_x", "a2o_bathtub_clean_loop3_x"]).ending(&["a2o_bathtub_clean_stop_x"], 1.5)
        }
        "Clean" if kind == ObjectKind::LitterBox => A::new(None, &["a2o_litterbox_clean_x"]),
        "Clean Stall" => A::new(Some("a2o_boxStall_clean_start_x"), &["a2o_boxStall_clean_loop1_x", "a2o_boxStall_clean_loop2_x"]).ending(&["a2o_boxStall_clean_stop_x"], 1.5),
        "Fill Bowl" => A::new(None, &["a2o_petBowl_putFood_x"]),
        "Stock" if kind == ObjectKind::Terrarium => A::new(None, &["a2o_minorPet_putInTerrarium_x"]),
        // (A bird cage's own: stocked, fed, watched, cleaned; a bird taught to talk.)
        "Stock" if kind == ObjectKind::BirdCage => A::new(None, &["a2o_birdcage_stock_x"]),
        "Fill Food" if kind == ObjectKind::BirdCage => A::new(None, &["a2o_birdcage_fillFood_x"]),
        "Watch" if kind == ObjectKind::BirdCage => A::new(Some("a2o_birdcage_watch_start_x"), &["a2o_birdcage_watch_breathe_x", "a2o_birdcage_watch_playful1_x", "a2o_birdcage_watch_playful2_x", "a2o_birdcage_watch_playful3_x", "a2o_birdcage_watch_tap_x", "a2o_birdcage_watch_touch_x"]).ending(&["a2o_birdcage_watch_stop_x"], 1.5),
        "Clean" if kind == ObjectKind::BirdCage => A::new(Some("a2o_birdcage_clean_start_x"), &["a2o_birdcage_clean_loop_x"]).ending(&["a2o_birdcage_clean_stop_x"], 1.5),
        "Talk With" | "Teach To Talk" if kind == ObjectKind::BirdCage => A::new(Some("a2o_minorPetBirds_teachToTalk_start_x"), &["a2o_minorPetBirds_teachToTalk_loop1_x", "a2o_minorPetBirds_teachToTalk_loop2_x", "a2o_minorPetBirds_teachToTalk_loop3_x"]).ending(&["a2o_minorPetBirds_teachToTalk_stop_x"], 1.5),
        "Mourn" if kind == ObjectKind::BirdCage => A::new(Some("a_react_cry_start"), &["a_react_cry_loop", "a_react_cry_loop2"]).ending(&["a_react_cry_stop"], 1.5),
        "Fill Food" => A::new(None, &["a2o_minorPet_feed_x"]),
        "Watch" if kind == ObjectKind::Terrarium => A::new(Some("a2o_terrarium_watch_start_x"), &["a2o_terrarium_watch_loopBreathe_x", "a2o_terrarium_watch_loopMakeNoise_x", "a2o_terrarium_watch_loopPeekaboo_x"]).ending(&["a2o_terrarium_watch_stop_x"], 1.5),
        "Clean" if kind == ObjectKind::Terrarium => A::new(Some("a2o_terrarium_clean_start_x"), &["a2o_terrarium_clean_loop_x"]).ending(&["a2o_terrarium_clean_stop_x"], 1.5),
        "Mourn" if kind == ObjectKind::Terrarium => A::new(Some("a_react_cry_start"), &["a_react_cry_loop", "a_react_cry_loop2"]).ending(&["a_react_cry_stop"], 1.5),
        "Clean" if kind == ObjectKind::Sink => A::new(Some("a2o_sink_clean_start_x"), &["a2o_sink_clean_loop1_x", "a2o_sink_clean_loop2_x"]).ending(&["a2o_sink_clean_stop_x"], 1.5),
        "Mop Up" => A::new(Some("a2o_puddle_mopUp_start_x"), &["a2o_puddle_mopUp_loop_1_x", "a2o_puddle_mopUp_loop_2_x"]).ending(&["a2o_puddle_mopUp_stop_x"], 1.2),
        "Make Bed" => A::steps("a2o_bed_make_start_x", &["a2o_bed_make_smooth_x", "a2o_bed_make_pat_x"], &["a2o_bed_make_smooth_x"]).ending(&["a2o_bed_make_stop_x"], 1.0),
        "Nap" if kind == ObjectKind::Sofa => A::new(Some("a2o_sofa_sit_trans_nap_x"), &["a2o_sofa_nap_loop1_x"]),
        "Eat" if matches!(kind, ObjectKind::Chair | ObjectKind::PicnicTable) => A::new(Some("a2o_eat_diningIn_fork_start_x"), &["a2o_eat_diningIn_fork_neat_x"]).ending(&["a2o_eat_diningIn_fork_stop_x"], 0.9),
        "Do Homework" => A::new(Some("a2o_homework_table_start_x"), &["a2o_homework_table_write_x", "a2o_homework_table_read_x", "a2o_homework_table_think_x", "a2o_homework_table_erase_x"]),
        "Pay Bills" | "Donate to Charity" => A::new(None, &["a2o_mailbox_getMail_x"]),
        "Water" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_wateringCan_start_x"), &["a2o_gardening_wateringCan_waterPlants_x"]),
        "Weed" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_crouch_start_x"), &["a2o_gardening_crouch_pullWeeds_x"]),
        "Harvest" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_bendover_start_x"), &["a2o_gardening_bendover_harvestmed_x"]),
        "Fertilize" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_crouch_start_x"), &["a2o_gardening_crouch_fertilize_x"]).ending(&["a2o_gardening_crouch_stop_x"], 1.2),
        "Revive" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_crouch_start_x"), &["a2o_gardening_crouch_revive_med_x"]).ending(&["a2o_gardening_crouch_stop_x"], 1.2),
        "Dispose" if kind == ObjectKind::GardenPlant => A::new(Some("a2o_gardening_crouch_start_x"), &["a2o_gardening_crouch_pullWeeds_x"]).ending(&["a2o_gardening_crouch_stop_x"], 1.2),
        // (The plant graph's "Loop Talk": the friendly social talk.)
        "Talk to Plant" => A::new(None, &["a2a_soc_neutral_talkFriendly_accept_x", "a2a_soc_neutral_tellJoke_accept_x"]),
        "Collect" if kind == ObjectKind::Collectible => A::new(Some("a2o_gardening_crouch_start_x"), &["a2o_gardening_crouch_harvestlow_x"]),
        "Swim" => A::new(Some("a2o_ladder_climbDown_L_x"), &["a_swim_cycle_x"]),
        "Destroy Snowman" => A::new(None, &["a2o_snowman_destroy_x"]),
        "Drink Plasma Juice" => A::new(None, &["a2o_vampires_drinkPotion"]),
        "Destroy Snow Angel" => A::new(Some("a2o_snowAngel_destroy_start_x"), &["a2o_snowAngel_destroy_loopKick_x", "a2o_snowAngel_destroy_loopStomp_x"]).ending(&["a2o_snowAngel_destroy_stop_x"], 2.7),
        "Play in Leaves" => A::new(Some("a2o_leafPile_playIn_start_x"), &["a2o_leafPile_playIn_loopKick_x", "a2o_leafPile_playIn_loopThrow_x", "a2o_leafPile_playIn_loopThrowInAir_x"])
            .ending(&["a2o_leafPile_playIn_stop_x"], 1.5),
        "Jump In" if kind == ObjectKind::LeafPile => A::new(None, &["a2o_leafPile_jumpCannonBall_x"]),
        "Rake Up" => A::new(Some("a2o_leafPile_rake_start_x"), &["a2o_leafPile_rake_loop_x"]).ending(&["a2o_leafPile_rake_stop_x"], 1.5),
        "Light Fire" if kind == ObjectKind::FirePit => A::new(None, &["a2o_firePit_light_start_x"]),
        "Put Out Fire" if kind == ObjectKind::FirePit => A::new(None, &["a2o_firePit_putOut_x"]),
        "Warm Hands" if kind == ObjectKind::FirePit => {
            A::new(Some("a2o_firePit_warmHands_start_x"), &["a2o_firePit_warmHandsLoop1_x", "a2o_firePit_warmHandsLoop2_x"]).ending(&["a2o_firePit_warmHands_stop_x"], 2.4)
        }
        // (Roasted on the skewer, then eaten off it.)
        "Roast Marshmallows" => A::new(Some("a2o_firePit_roast_start_x"), &["a2o_firePit_roast_loop1_x", "a2o_firePit_roast_loop2_x"])
            .ending(&["a2o_firePit_roast_stop_x", "a2o_firePit_roast_eat_start_x", "a2o_firePit_roast_eat_loop_x", "a2o_firePit_roast_eat_stop_x"], 9.4),
        "Use Urinal" => A::new(None, &["a2o_urinal_useStanding_whistle_loop_x", "a2o_urinal_useStanding_fart_loop_x"]).ending(&["a2o_urinal_flush_x"], 2.0),
        "Play with Toy Oven" => A::new(Some("c2o_easyBakeOven_prep_start_x"), &["c2o_easyBakeOven_prep_loop_x"]).ending(&["c2o_easyBakeOven_prep_stopFinished_x"], 1.5),
        // (Served from the table, eaten standing.)
        "Have a Plate" => A::steps("a2o_buffetTable_serve_x", &[], &["a2o_eat_stand_fork_neat_x"]),
        "Have a Picnic" => A::steps("a2o_picnicBasket_getPlate_part1_x", &["a2o_picnicBasket_eat_start_x"], &["a2o_picnicBasket_eat_x"]).ending(&["a2o_picnicBasket_eat_stop_finished_x"], 0.7),
        // (Stood in the tub, the taps turned on and the water up, sat down, washed, then lying
        // back; sat up and out at the end.)
        "WooHoo" | "Try for Baby" if matches!(kind, ObjectKind::BedDouble | ObjectKind::BedSingle) => woohoo_clip(false),
        // (Over a tub's side and back, its curtain or door drawn.)
        "Take Shower" if kind == ObjectKind::ShowerTub => {
            A::new(Some("a2o_showertub_takeShower_getIn_x"), &["a2o_shower_takeShower_loop"]).ending(&["a2o_showertub_takeShower_getOut_x"], 2.2)
        }
        "Take Bath" | "Take Bubble Bath" => A::steps("a2o_bathtub_takeBath_turnOn_x", BATH_WASH, &["a2o_bathtub_relax_loop1_x", "a2o_bathtub_relax_loop2_x"])
            .ending(&["a2o_bathtub_takeBath_trans_sitUp_x", "a2o_bathtub_takeBath_getOut_x"], 5.2),
        "Play with Rubber Ducky" => A::steps(
            "a2o_bathtub_takeBath_turnOn_x",
            &["a2o_bathtub_takeBath_getIn_x", "a2o_bathtub_takeBath_trans_relax_x", "a2o_bathtub_ducky_start_x"],
            &["a2o_bathtub_ducky_loop1_x", "a2o_bathtub_ducky_loop2_x", "a2o_bathtub_ducky_loop3_x"],
        )
        .ending(&["a2o_bathtub_ducky_stop_x", "a2o_bathtub_takeBath_trans_sitUp_x", "a2o_bathtub_takeBath_getOut_x"], 7.5),
        "Light Fire" => A::new(None, &["a2o_fireplace_light_start_x"]),
        // (The game's `TraitNeurotic` checks, in the sink's, stove's and fireplace's machines: each
        // played once through, as long as its interaction.)
        "Check Sink" => A::new(None, &["a2o_trait_neurotic_check_sink_x"]),
        // (The bowl's `Stock`, `ScoopFish`, `Feed` and `CleanOut` states.)
        "Place Fish" => A::new(None, &["a2o_fishBowl_stock_x"]),
        "Take Fish" => A::new(None, &["a2o_fishBowl_scoopOut_x"]),
        "Feed Fish" => A::new(None, &["a2o_fishBowl_feed_x"]),
        "Clean Out" if kind == ObjectKind::FishBowl => A::new(None, &["a2o_fishBowl_clean_x"]),
        "Check Stove" => A::new(None, &["a2o_trait_neurotic_check_stove_x"]),
        "Check Fireplace" => A::new(None, &["a2o_trait_neurotic_check_fireplace_x"]),
        // (The poker in hand, jabbing and catching their breath: the fireplace's `Loop PokeFire`.)
        "Poke Fire" => A::new(Some("a2o_fireplace_poke_start_x"), &["a2o_fireplace_poke_loopjab_x", "a2o_fireplace_poke_loopbreathe_x"]),
        "Light Candle" | "Blow Out" => A::new(None, &["a2o_object_genericSwipe_x"]),
        "Put Out Fire" => A::new(None, &["a2o_fireplace_putOut_x"]),
        "Warm Hands" => A::new(Some("a2o_fireplace_warmHands_start_x"), &["a2o_fireplace_warmHands_x"]),
        "Catch" if kind == ObjectKind::Butterfly => A::new(None, &["a2o_butterfly_catch_x"]),
        "Catch" if kind == ObjectKind::Beetle => A::new(None, &["a2o_beetle_catch_x"]),
        "Fish" if kind == ObjectKind::FishingSpot => {
            A::new(Some("a2o_fishHereWith_cast_normal_x"), &["a2o_fishHereWith_idle1_x", "a2o_fishHereWith_idle2_x", "a2o_fishHereWith_idle3_x"])
        }
        "Clip Coupon" if kind == ObjectKind::Newspaper => A::new(None, &["a2o_newspaper_clipCoupon_standing_x"]),
        "Read" if kind == ObjectKind::Newspaper => A::steps("a2o_newspaper_pickUp_floor_part1_x", &["a2o_newspaper_pickUp_floor_part2_x"], &["a2o_newspaper_carry_x"]),
        "Read the Paper" if matches!(kind, ObjectKind::Sofa | ObjectKind::Chair | ObjectKind::Stool) => {
            A::new(Some("a2o_newspaper_read_seated_start_x"), &["a2o_newspaper_read_seated_loop"]).ending(&["a2o_newspaper_read_seated_stop_x"], 3.0)
        }
        "Read the Paper" => A::new(Some("a2o_newspaper_read_standing_start_x"), &["a2o_newspaper_read_standing_loop"]).ending(&["a2o_newspaper_read_standing_stop_x"], 3.0),
        "Eat" if kind == ObjectKind::Stool => A::new(Some("a2o_eat_barStoolIn_fork_start_x"), &["a2o_eat_barStoolIn_fork_neat_x"]).ending(&["a2o_eat_barStoolIn_fork_stop_x"], 0.9),
        // (The fridge door opened, something taken out and the door shut again.)
        "Have Quick Meal" => A::steps("a2o_fridge_openDoor_x", FRIDGE_STEPS, &["a2o_plateDinner_carry_x"]),
        "Microwave Dinner" => A::new(Some("a2o_microwave_put_x"), &["a2o_microwave_check_loop_x"]),
        "Take Out Dinner" => A::new(None, &["a2o_microwave_get_x"]),
        "Grab a Snack" => A::steps("a2o_fridge_openDoor_x", FRIDGE_STEPS, &["a2o_eat_stand_hand_neat"]),
        "Bake Birthday Cake" => A::steps("a2o_fridge_openDoor_x", FRIDGE_STEPS, &["a2o_cuttingBoard_chop_loopMedSkill_x"]),
        "Grow Up" => A::new(None, &["a2o_birthdayCake_blowOut_counter_x"]),
        "Cook Dinner" => A::new(
            Some("a2o_stove_fryingPan_start_fromNeutral_x"),
            &["a2o_stove_fryingPan_idle_x", "a2o_stove_fryingPan_spatula_flip_x", "a2o_stove_fryingPan_spatula_poke_x"],
        ),
        "Use Toilet" => A::new(Some("a2o_toilet_useStanding_start_x"), &["a2o_toilet_useStanding_loop_x"]),
        "Flush" => A::new(None, &["a2o_toilet_flush_x"]),
        "Throw Up" if kind == ObjectKind::Toilet => A::new(Some("a2o_toilet_vomit_start_x"), &["a2o_toilet_vomit_loop_x"]).ending(&["a2o_toilet_vomit_stop_x"], 1.5),
        "Take Shower" => A::new(Some("a2o_shower_takeShower_getIn_x"), &["a2o_shower_takeShower_loop"]).ending(&["a2o_shower_takeShower_getOut_x"], 2.4),
        "Wash Hands" => A::new(Some("a2o_sink_washhands_start_x"), &["a2o_sink_washhands_scrubHands_x", "a2o_sink_washhands_rinseHands_x"]).ending(&["a2o_sink_washhands_stop_x"], 2.6),
        // (The sponge in hand: the sink's `SpongeBath` states.)
        "Sponge Bath" => A::new(Some("a2o_sink_spongeBath_wash_start_x"), &["a2o_sink_spongeBath_wash_loop_x", "a2o_sink_spongeBath_wash_loop1_x", "a2o_sink_spongeBath_wash_loop2_x"])
            .ending(&["a2o_sink_spongeBath_wash_stop_x"], 1.5),
        "Brush Teeth" => A::new(Some("a2o_sink_brushTeeth_start_x"), &["a2o_sink_brushTeeth_Loop"]).ending(&["a2o_sink_brushTeeth_stop_x"], 2.6),
        // (Only the plain loop: the toss-and-turn ones are authored from beside the bed.)
        "Sleep" => A::new(Some("a2o_bed_getIn_made_x"), &["a2o_bed_sleep_back_loop_x"]).ending(&["a2o_bed_getout_start_x", "a2o_bed_getout_stop_x"], 7.3),
        "Nap" => A::new(Some("a2o_bed_nap_start_x"), &["a2o_bed_nap_loop_breathe_x"]),
        "Relax" | "Daydream" => A::new(Some("a2o_bed_relax_getin_start_x"), &["a2o_bed_relax_loop"]),
        "Sit" => A::new(None, &["a2o_chairLiving_sit_breathe_loop_x", "a2o_chairLiving_sit_crossedLeg_front_loop_x"]),
        "Watch TV" if kind == ObjectKind::Tv => A::new(None, &["a2o_tv_watch_turnOnOff_standing_x"]),
        "Watch the TV" if kind == ObjectKind::Sofa => A::new(
            None,
            &["a2o_tv_watch_idle1_seated", "a2o_tv_watch_idle2_seated", "a2o_tv_watch_idle3_seated", "a2o_tv_watch_idle4_seated", "a2o_tv_watch_active_seated"],
        ),
        "Watch the TV" if matches!(kind, ObjectKind::Chair | ObjectKind::Stool) => A::new(
            None,
            &[
                "a2o_tv_watch_idle1_livingChairSeated",
                "a2o_tv_watch_idle2_livingChairSeated",
                "a2o_tv_watch_idle3_livingChairSeated",
                "a2o_tv_watch_active_livingChairSeated",
            ],
        ),
        "Watch the TV" => {
            A::new(None, &["a2o_tv_watch_idle1_standing", "a2o_tv_watch_idle2_standing", "a2o_tv_watch_idle3_standing", "a2o_tv_watch_active_standing"])
        }
        "Play Computer Games" => A::new(None, &["a2o_computer_game_loop1_x", "a2o_computer_game_loop2_x"]),
        "Play Chess" if kind == ObjectKind::Computer => A::new(None, &["a2o_computer_chess_type_loop_x"]),
        "Browse Web" => A::new(None, &["a2o_computer_generic_type_loop1_x", "a2o_computer_generic_type_loop2_x", "a2o_computer_generic_type_breathe_x"]),
        "Practice Writing" | "Solve Hard Problems" => A::new(None, &["a2o_computer_serious_Type_chinRub_x", "a2o_computer_serious_Type_templeTab_x", "a2o_computer_serious_ahHa_x"]),
        "Work on Job-Related Issues" | "Join Book Club" | "Leave Book Club" | "Hack" | "Work from Home" | "Check Real Estate" | "Write Report" => A::new(None, &["a2o_computer_generic_type_loop1_x", "a2o_computer_generic_type_loop2_x"]),
        "Write Novel" | "Find a Job" | "Quit Job" => A::new(None, &["a2o_computer_chess_type_loop_x"]),
        "Dance" => A::new(None, &["a_dance_beg_", "a_dance_med_"]),
        "Read a Book" => A::new(None, &["a2o_bookshelf_getBook_Carry_x"]),
        "Get Ingredients" => A::steps("a2o_fridge_openDoor_x", &["a2o_fridge_takeFoodOut_foodTray_x", "a2o_fridge_closeDoor_x"], &["a2o_foodTray_carry_x"]),
        "Set Down Meal" => A::new(None, &["a2o_carryObject_putDown_counter_x"]),
        "Prepare Food" => A::new(Some("a2o_cuttingBoard_start_fromCarry_x"), &["a2o_cuttingBoard_chop_loopMedSkill_x", "a2o_cuttingBoard_chop_loopLowSkill_x"]).ending(&["a2o_cuttingBoard_chop_stop_x"], 0.7),
        "Read Book" if matches!(kind, ObjectKind::Sofa | ObjectKind::Chair | ObjectKind::Stool) => {
            A::new(Some("a2o_book_readBook_sitting_fromCarry_start_x"), &["a2o_book_readBook_sitting_loopRead_x", "a2o_book_readBook_sitting_loopTurnPage_x"])
                .ending(&["a2o_book_readBook_sitting_fromCarry_stop_x"], 0.9)
        }
        "Read Book" => A::new(Some("a2o_book_readBook_standing_fromCarry_start_x"), &["a2o_book_readBook_standing_loop"]).ending(&["a2o_book_readBook_standing_fromCarry_stop_x"], 0.8),
        "Put Book Away" => A::new(Some("a2o_bookshelf_putBook_Carry_x"), &[]),
        // (The game's `Mirror` state machine's: a speech practised as talk, a look over in the
        // glass, primping, faces pulled and pointed at.)
        "Practice Speech" => A::new(None, &["a_soc_talkTalk_gen_b_x", "a_soc_talkTalk_gen_e_x", "a_soc_talkRespond_disagree_correction_x", "a_soc_talkTalk_gen_j_x"]),
        "Change Appearance" | "Check Self Out" => A::new(None, &["a2o_mirror_full_checkSelfOut_loop"]),
        "Admire Self" => A::new(None, &["a2o_mirror_full_checkSelfOut_loop1_x", "a_soc_idle_neutral_admire_x", "a2o_mirror_full_checkSelfOut_loop5_x"]),
        "Gussy Up" => A::new(Some("a2o_mirror_face_primp_start_x"), &["a2o_mirror_face_primp_Loop"]).ending(&["a2o_mirror_face_primp_stop_x"], 1.0),
        "Play with Mirror" => A::new(None, &["a2o_mirror_play_makeFaces_x", "a2o_mirror_play_point_x"]),
        "Paint" => A::new(Some("a2o_painting_start_x"), &["a2o_painting_loopMed", "a2o_painting_loopLarge", "a2o_painting_consider"]),
        "Play Guitar" => A::new(None, &["a2o_guitar_play_med_loop", "a2o_guitar_play_high_loop", "a2o_guitar_play_low_loop"]),
        // (To Fit Net on the TV: jumping jacks, knee bends and stretches.)
        "Work Out" | "Cardio Exercise" | "Strength Training" if matches!(kind, ObjectKind::Tv | ObjectKind::Stereo) => A::new(Some("a_workOut_start_x"), &["a_workOut_jumpingJack_loop_x", "a_workOut_kneeBend_loop_x", "a_workOut_stretchSide_loop_x"])
            .ending(&["a_workOut_stop_x"], 1.0),
        "Work Out" => A::new(Some("a2o_treadmill_jog_start_x"), &["a2o_treadmill_jog_loop"]),
        // (On the bench, then the game's three lifts in turn, each started and put down.)
        "Pump Iron" => A::new(Some("a2o_workoutBench_getIn_W_x"), BENCH_LIFTS).in_order().ending(&["a2o_workoutBench_getOut_W_x"], BENCH_OFF),
        // (In the chair at the chess table, pulled up to it; its moves are `chess::play_chess`'s.)
        "Play Chess" | "Play a Ranked Match" if kind == ObjectKind::Chair => A::new(Some("a2o_chessTable_start_x"), &["a2o_chessTable_loop"]).ending(&["a2o_chessTable_stop_x"], 1.5),
        "Watch Chess Match" => A::new(None, crate::chess::WATCHING),
        "Watch" => A::new(None, crate::tips::WATCHING),
        // (Sat on it, the game's `PottyChair` toddler's; emptied into its bag.)
        "Use Potty Chair" => A::new(None, &["p2o_pottyChair_use_loopBreath_y", "p2o_pottyChair_use_loopPat_y"]),
        "Sit in High Chair" => A::new(None, crate::highchair::SITTING),
        "Put in High Chair" => A::new(None, &["a_idle_neutral_loop_"]),
        "Give Baby Food" => A::new(None, &["a2p_highChair_serveFood_x"]),
        "Take Out" if kind == ObjectKind::HighChair => A::new(None, &["a2p_highChair_hold_x"]),
        "Empty" if kind == ObjectKind::PottyChair => A::new(None, &["a2o_pottyChair_empty_x"]),
        "Potty Train" => A::new(None, &["a_idle_neutral_loop_"]),
        "Nap in Crib" => A::new(Some("p2o_crib_sleep_start_y"), &["p2o_crib_sleep_loop_y"]),
        "Change Clothes" | "Change Into" | "New Everyday Outfit" | "Plan Outfit" => A::new(Some("a2o_dresser_use_open"), &["a2o_dresser_use_close"]),
        "Grill" => A::new(Some("a2o_bbq_grill_start"), &["a2o_bbq_grill_loopBreathe", "a2o_bbq_grill_loopPokeLeft", "a2o_bbq_grill_loopPokeRight", "a2o_bbq_grill_loopExpert"]),
        "Empty Trash" if kind == ObjectKind::TrashCompactor => A::new(None, &["a2o_trashCompactor_takeOut_x"]),
        "Empty Trash" => A::new(None, &["a2o_trashCan_empty_pullout_x"]),
        "Load Dishes" => A::new(None, &["a2o_dishwasher_use_x"]),
        "Throw Out Trash" if kind == ObjectKind::WasteChute => A::new(None, &["a2o_wasteChute_dispose_x"]),
        "Pick Up" if kind == ObjectKind::ClothingPile => A::new(None, &["a2o_clothingPile_get_floor_x"]),
        "Do Laundry" if kind == ObjectKind::Hamper => A::new(None, &["a2o_clothingPile_get_hamper_x"]),
        "Do Laundry" => A::new(None, &["a2o_washMachine_operate_x"]),
        "Dry Clothes" => A::new(None, &["a2o_washMachine_empty_easy_x"]),
        "Load the Dryer" => A::new(None, &["a2o_dryer_operate_x"]),
        "Empty Dryer" => A::new(None, &["a2o_dryer_empty_succeed_x"]),
        "Hang Clothes" => A::new(None, &["a2o_clothesLine_hang_x"]),
        "Take Down Clothes" => A::new(None, &["a2o_clothesLine_remove_x"]),
        "Throw Out Trash" => A::new(None, &["a2o_trashPile_drop_outdoors_x"]),
        "Clean Out" if kind == ObjectKind::Fridge => A::new(None, &["a2o_fridge_cleanOutBadFood_x"]),
        // (Into its works, as an upgrade's done.)
        "Overclock" => A::new(Some("a2o_computer_repair_start_x"), &["a2o_computer_repair_loop1_x", "a2o_computer_repair_loop2_x"]),
        // (Tinkered with as it'd be repaired: the game's "Upgrade" state.)
        "Sabotage" => repair_of(kind).1,
        // (The can's own halves knock it down and stand it up; rummaging, its lid's off.)
        "Knock Over" if kind == ObjectKind::OutdoorTrashCan => A::new(None, &["a2o_trashCan_knock_over_x"]),
        "Pick Up" if kind == ObjectKind::OutdoorTrashCan => A::new(None, &["a2o_trashCan_clean_up_x"]),
        "Rummage" => A::new(Some("a2o_trashCan_rummage_open_x"), &["a2o_trashCan_rummage_loop"]).ending(&["a2o_trashCan_rummage_close_x"], RUMMAGE_CLOSE_SECS),
        "Wash Dishes" => A::new(Some("a2o_sink_dishes_scrub_start_x"), &["a2o_sink_dishes_scrub_loop1_x", "a2o_sink_dishes_scrub_loop2_x"]).ending(&["a2o_sink_dishes_scrub_stop_x"], 3.5),
        "Clean Up" => A::steps("a2o_plateDinner_pickUp_table_part1_x", &["a2o_plateDinner_pickUp_table_part2_x"], &["a2o_plateDinner_carry_x"]),
        "Make Hot Beverage" => A::new(Some("a2o_hotBeverageMachine_fill"), &["a2o_hotBeverageMachine_drink_loopSip_standing", "a2o_hotBeverageMachine_drink_loopLongSip_standing"]),
        "Serve Hot Beverages" => A::new(Some("a2o_hotBeverageMachine_start_x"), &["a2o_hotBeverageMachine_fill_x"]).ending(&["a2o_hotBeverageMachine_serve_get_x"], 2.9),
        "Drink" if kind == ObjectKind::DrinkTray => A::new(None, &["a2o_hotBeverageMachine_drink_loopSip_standing", "a2o_hotBeverageMachine_drink_loopLongSip_standing"]),
        "Stargaze" | "Search Galaxy" => A::new(Some("a2o_telescope_start"), &["a2o_telescope_look_loop", "a2o_telescope_look_breathe", "a2o_telescope_react_wonderment"]),
        "Swing" => A::new(Some("a2o_swingset_getIn"), &["a2o_swingset_swing"]),
        "Relax in Hot Tub" => A::new(Some("a2o_hotTub_getIn"), &["a2o_hotTub_idles_relaxing_loop", "a2o_hotTub_idles_playingToe", "a2o_hotTub_splash"]),
        "Play with Dollhouse" => A::new(Some("a2o_dollhouse_play_start_x"), &["a2o_dollhouse_play_loop"]),
        // (Kicking it in: the dollhouse's `Smash` state, its last blow and getting up.)
        "Smash" if kind == ObjectKind::DollHouse => A::new(Some("a2o_dollhouse_smash_start_x"), &["a2o_dollhouse_smash_loop1_x", "a2o_dollhouse_smash_loop2_x"])
            .ending(&["a2o_dollhouse_smash_smash_x", "a2o_dollhouse_smash_stop_x"], crate::dollhouse::ADULT_END_SECS),
        // (Up the tower's ladder, about in its lookout, and down its pole at the end.)
        "Play on Jungle Gym" => A::new(Some("c2o_JungleGymTower_climbUp_x"), &["c2o_JungleGymTower_loop1_x", "c2o_JungleGymTower_loop2_x"]).ending(&["c2o_JungleGymTower_slideDown_x"], 6.1),
        "Play on Slide" => A::steps("c2o_JungleGymSlide_climbUp_x", &["c2o_JungleGymSlide_breathe_x"], &["c2o_JungleGymSlide_succeed_x"]),
        "Play Foosball" => A::new(None, &["a2o_foosballTable_play"]),
        // (The cue fetched from the rack, then shots round the table, the balls rolling.)
        "Play Pool" => A::new(
            Some("a2o_poolTable_play8_p1_getCue_x"),
            &["a2o_poolTable_play8_p1_make1_x", "a2o_poolTable_play8_p1_make2_x", "a2o_poolTable_play8_p1_make5_x", "a2o_poolTable_play8_p1_make6_x", "a2o_poolTable_play8_p1_miss1_x"],
        ),
        "Throw Darts" => A::new(Some("a2o_dartboard_throw_start_x"), &["a2o_dartboard_throw_practice_loop1_x", "a2o_dartboard_throw_practice_loop2_x", "a2o_dartboard_throw_normal_x"]),
        // (The ball fetched from the return, bowled, and the pins' fall seen: in turn.)
        "Bowl" => A::new(
            None,
            &[
                "a2o_bowlingLane_getBall_x",
                "a2o_bowlingLane_lowSkillThrow_throw1_x",
                "a2o_bowlingLane_endGood_x",
                "a2o_bowlingLane_getBall_x",
                "a2o_bowlingLane_lowSkillThrow_throw2_x",
                "a2o_bowlingLane_endStrike_x",
                "a2o_bowlingLane_getBall_x",
                "a2o_bowlingLane_throw_slow_x",
                "a2o_bowlingLane_endBad_x",
            ],
        )
        .in_order(),
        "Sing Karaoke" => A::new(
            Some("a2o_karaoke_singSolo_start_x"),
            &[
                "a2o_karaoke_singSolo_loop01_med_x",
                "a2o_karaoke_singSolo_loop02_med_x",
                "a2o_karaoke_singSolo_loop03_med_x",
                "a2o_karaoke_singSolo_loop04_med_x",
                "a2o_karaoke_singSolo_loop01_high_x",
                "a2o_karaoke_singSolo_loop02_high_x",
            ],
        )
        .ending(&["a2o_karaoke_singSolo_stop_x"], 0.9),
        "Play Arcade Game" => A::new(Some("a2o_arcadeMachine_start_x"), &["a2o_arcadeMachine_loop1_x", "a2o_arcadeMachine_loop2_x", "a2o_arcadeMachine_loop3_x", "a2o_arcadeMachine_loop4_x"])
            .ending(&["a2o_arcadeMachine_stop_x"], 1.3),
        "Play Claw Machine" => A::new(Some("a2o_arcadeClawMachine_start_x"), &["a2o_arcadeClawMachine_lose_01_x", "a2o_arcadeClawMachine_lose_02_x", "a2o_arcadeClawMachine_win_01_x"]),
        "Play Whack-a-Gnome" => A::new(
            Some("a2o_arcadeWhackAGnome_start_x"),
            &["a2o_arcadeWhackAGnome_skillLow1_x", "a2o_arcadeWhackAGnome_skillLow2_x", "a2o_arcadeWhackAGnome_skillMed1_x", "a2o_arcadeWhackAGnome_skillMed2_x"],
        )
        .ending(&["a2o_arcadeWhackAGnome_stop_x"], 1.37),
        "Play Skee-Ball" => A::new(
            Some("a2o_skeeBall_play_start_x"),
            &[
                "a2o_skeeBall_play_make1_x",
                "a2o_skeeBall_play_make2_x",
                "a2o_skeeBall_play_make3_x",
                "a2o_skeeBall_play_make4_x",
                "a2o_skeeBall_play_make5_x",
                "a2o_skeeBall_play_make6_x",
                "a2o_skeeBall_play_make7_x",
                "a2o_skeeBall_play_miss_x",
            ],
        ),
        "Practice Horseshoes" => A::new(
            Some("a2o_horseshoeCourt_start_x"),
            &[
                "a2o_horseshoeCourt_loopPractice_x",
                "a2o_horseshoeCourt_throwNormal_01_x",
                "a2o_horseshoeCourt_throwNormal_02_x",
                "a2o_horseshoeCourt_throwNormal_03_x",
                "a2o_horseshoeCourt_throwWringer_01_x",
                "a2o_horseshoeCourt_throwFail_01_x",
            ],
        )
        .ending(&["a2o_horseshoeCourt_stop_x"], 1.53),
        "Practice Shuffleboard" => A::new(Some("a2o_shuffleboard_shoot_start_x"), &["a2o_shuffleboard_shoot_practice_loop1_x", "a2o_shuffleboard_shoot_practice_loop2_x", "a2o_shuffleboard_shoot_x"]),
        // (Up into the saddle, the bull started slow, and off with a flourish at the end.)
        "Ride Mechanical Bull" => A::steps(
            "a2o_mechanicalBull_ride_getOn_x",
            &["a2o_mechanicalBull_ride_low_start_x"],
            &["a2o_mechanicalBull_ride_low_loop1_x", "a2o_mechanicalBull_ride_low_loop2_x", "a2o_mechanicalBull_ride_low_loop3_x"],
        )
        .ending(&["a2o_mechanicalBull_ride_low_stop_x", "a2o_mechanicalBull_dismount_success_x"], 5.6),
        "Jump on Trampoline" => A::new(
            Some("a2o_trampoline_start_x"),
            &["a2o_trampoline_bounce_normal_loop1_x", "a2o_trampoline_bounce_normal_loop2_x", "a2o_trampoline_bounce_highSkill_loop1_x"],
        )
        .ending(&["a2o_trampoline_stop_x"], 3.57),
        "Play in Sandbox" => A::new(Some("a2o_sandbox_play_start_x"), &["a2o_sandbox_play_loop1_x", "a2o_sandbox_play_loop2_x", "a2o_sandbox_play_loop3_x", "a2o_sandbox_play_find_x"])
            .ending(&["a2o_sandbox_play_stop_x"], 1.27),
        // (Onto the bench and off again.)
        "Play Piano" => A::new(
            Some("a2o_piano_play_start_x"),
            &["a2o_piano_play_med_loop1_x", "a2o_piano_play_med_loop2_x", "a2o_piano_play_high_loop1_x", "a2o_piano_play_low_loop1_x"],
        )
        .ending(&["a2o_piano_play_stop_x"], 2.6),
        "Play Keyboard" => A::new(None, &["a2o_keyboard_play_med_loop1_x", "a2o_keyboard_play_med_loop2_x", "a2o_keyboard_play_high_loop1_x", "a2o_keyboard_play_low_loop1_x"]),
        "Play Drums" => A::new(Some("a2o_drum_play_start_x"), &["a2o_drum_play_med_loop1_x", "a2o_drum_play_med_loop2_x", "a2o_drum_play_high_loop1_x", "a2o_drum_play_low_loop1_x"])
            .ending(&["a2o_drum_play_stop_x"], 2.63),
        "Play Laser Harp" => A::new(
            Some("a2o_laserHarp_start_x"),
            &["a2o_laserHarp_play_loopMed1_x", "a2o_laserHarp_play_loopMed2_x", "a2o_laserHarp_play_loopHigh1_x", "a2o_laserHarp_play_loopLow1_x"],
        )
        .ending(&["a2o_laserHarp_stop_x"], 1.97),
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
        // (The coins in, the can or the bar out of the tray, and had there and then.)
        "Buy Soda" => A::steps("a2o_vendingMachine_start_x", &["a2o_vendingMachine_retrieveSoda_x"], &["a2o_vendingMachine_drinkSoda_x"]),
        "Buy Candy" => A::steps("a2o_vendingMachine_start_x", &["a2o_vendingMachine_retrieveCandy_x"], &["a2o_vendingMachine_eatCandy_x", "a2o_vendingMachine_eatCandy_reactNormal_x"]),
        // (Nothing comes out: a glare at it.)
        "Stuck" => A::steps("a2o_vendingMachine_start_x", &["a2o_vendingMachine_retrieve_fail_x"], &["a2o_vendingMachine_reactUpset_x"]),
        "Shake Machine" => A::new(None, &["a2o_vendingMachine_shakeMachine_x"]),
        "Slam Machine" => A::new(None, &["a2o_vendingMachine_slamMachine_x"]),
        "Synthesize Food" => A::steps(
            "a2o_foodSynthesizer_start1_x",
            &["a2o_foodSynthesizer_program_loop1_x", "a2o_foodSynthesizer_program_loop2_x", "a2o_foodSynthesizer_celebrate_x"],
            &["a2o_plateDinner_carry_x"],
        ),
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
            Some("a2o_stuffedAnimal_play_start_x"),
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
/// The game's generic conversation, for a social with a child that has no clips of its own for
/// their two ages: talking, and listening as it goes down (a child plays the children's, the
/// clip library's `c_` for `a_`).
const TALK: &[&str] = &[
    "a_soc_talkTalk_gen_a_x",
    "a_soc_talkTalk_gen_b_x",
    "a_soc_talkTalk_gen_d_x",
    "a_soc_talkTalk_gen_e_x",
    "a_soc_talkTalk_gen_g_x",
    "a_soc_talkTalk_gen_i_x",
    "a_soc_talkTalk_gen_j_x",
    "a_soc_talkTalk_gen_k_x",
    "a_soc_talkTalk_gen_l_x",
];
const AGREE: &[&str] = &[
    "a_soc_talkListen_agreeAha_x",
    "a_soc_talkListen_agreeBobble1_x",
    "a_soc_talkListen_agreeBobblePoint_x",
    "a_soc_talkListen_agreeEager_x",
    "a_soc_talkListen_agree_funny_x",
    "a_soc_talkListen_agree_wow_x",
    "a_soc_talkRespond_agree_enlightened_x",
    "a_soc_talkRespond_agree_laugh_x",
    "a_soc_talkRespond_agree_totally_x",
];
const DISAGREE: &[&str] = &[
    "a_soc_talkListen_disagree_high_x",
    "a_soc_talkListen_disagree_kiddingMe_x",
    "a_soc_talkListen_disagree_low_x",
    "a_soc_talkListen_disagree_med_x",
    "a_soc_talkListen_disagree_wince_x",
    "a_soc_talkRespond_disagree_correction_x",
    "a_soc_talkRespond_disagree_disgust_x",
    "a_soc_talkRespond_disagree_holdOn_x",
];

/// A social's clips with a child in it, each side's: the game's own for their two ages (its
/// `c2c_`, `c2a_` and `a2c_` for the grown-ups' `a2a_`) where it has them, else its generic
/// talk; none (the clips as they are) between those of a grown-up's build.
pub fn child_social_clips(clips: &'static [&'static str], actor: Age, target: Age, mean: bool, names: &[String]) -> Option<(crate::anim::ActionClip, crate::anim::ActionClip)> {
    use std::sync::{Mutex, OnceLock};
    let kid = |a: Age| a == Age::Child;
    if !kid(actor) && !kid(target) {
        return None;
    }
    let (p, q) = (if kid(actor) { 'c' } else { 'a' }, if kid(target) { 'c' } else { 'a' });
    static PAIRS: OnceLock<Mutex<HashMap<String, &'static [&'static str]>>> = OnceLock::new();
    let mut pairs = PAIRS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    let mapped = *pairs.entry(format!("{p}{q}:{}", clips.join(","))).or_insert_with(|| {
        let has = |base: &str, side: &str| {
            let base = base.to_ascii_lowercase();
            names.iter().any(|n| n.to_ascii_lowercase().starts_with(&base) && n.ends_with(side))
        };
        let v: Vec<&'static str> = clips
            .iter()
            .filter_map(|c| c.strip_prefix("a2a_"))
            .map(|r| format!("{p}2{q}_{r}"))
            .filter(|m| has(m, "_x") && has(m, "_y"))
            .map(|m| &*Box::leak(m.into_boxed_str()))
            .collect();
        Box::leak(v.into_boxed_slice())
    });
    if !mapped.is_empty() {
        return Some((crate::anim::ActionClip::social(mapped, 'x'), crate::anim::ActionClip::social(mapped, 'y')));
    }
    Some((crate::anim::ActionClip::new(None, TALK), crate::anim::ActionClip::new(None, if mean { DISAGREE } else { AGREE })))
}

pub fn social_start(name: &str, little: Option<Age>, held: bool) -> Option<&'static str> {
    match (name, little) {
        // (Held already: picked up beforehand, as the game's `CarryingChild` socials.)
        ("Play With" | "Cuddle" | "Change Diaper" | "Put to Bed", Some(Age::Toddler)) if held => None,
        ("Frolic", _) => Some("a2a_soc_fairy_frolic_start"),
        ("Cast Charm Spell" | "Cast Curse", _) => Some("a2a_magicWand_castSpell_lowSkill_start"),
        ("Play With" | "Cuddle" | "Change Diaper" | "Put to Bed", Some(Age::Toddler)) => Some("a2p_pickUp"),
        ("Read to", Some(Age::Toddler)) => Some("a2p_book_readWith_start"),
        ("Teach to Walk", _) => Some("a2p_teachToWalk_start_kneelDown"),
        ("Fight", _) => Some("a2a_soc_Bad_Fight_Start"),
        ("Give Back Rub", _) => Some("a2a_soc_Amorous_Massage_Amorous_start"),
        _ => None,
    }
}

pub fn social_clips(name: &str, little: Option<Age>, held: bool) -> &'static [&'static str] {
    let baby = little == Some(Age::Baby);
    // A little one in the grown-up's arms: the game's own clips for it there (`CarryToddler`'s
    // GiveBottle, ChangeDiaper, Snuggle, PlayWith, TossInAir, Tickle and Chat).
    if held {
        match (name, baby) {
            ("Feed", true) => return &["a2b_babyBottle_feed"],
            ("Change Diaper", true) => return &["a2b_changeDiaper"],
            ("Cuddle", true) => return &["a2b_soc_cuddleBaby"],
            ("Play With", true) => return &["a2b_soc_playWithBaby"],
            ("Change Diaper", false) => return &["a2p_changeDiaper"],
            ("Cuddle", false) => return &["a2p_soc_hugSnuggle"],
            ("Play With", false) => return &["a2p_soc_tossInAir", "a2p_soc_tickleInArms", "a2p_carry_chat_loop"],
            _ => {}
        }
    }
    // (The game's own socials bring their clips with them.)
    if let Some(g) = crate::social::social_index(name).and_then(|i| crate::social::social(i).game) {
        return g.clips;
    }
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
        "Hypnotic Gaze" => &["a2a_soc_vampire_hypnoticGaze_success"],
        "Talk About the Joy of Plasma" => &["a2a_soc_vampire_talkAboutJoyOfPlasma_accept"],
        "Frolic" => &["a2a_soc_fairy_frolic_loop"],
        "Play Fairy Trick" => &["a2a_fairy_playTrick_hotHead"],
        "Brag About Wings" => &["a2a_soc_fairy_bragAboutWings_accept", "a2a_soc_fairy_talkAboutFlightExperience_accept"],
        "Practice Fighting" => &["a2a_soc_werewolf_practiceFighting_accept"],
        // (The wand out, the spell cast, and it takes.)
        "Cast Charm Spell" | "Cast Curse" => &["a2a_magicWand_castSpell_lowSkill_loop", "a2a_magicWand_castSpell_lowSkill_success"],
        // (The one throws, the other catches: soft, hard, wimpy and showing off.)
        "Play Catch" => &["a2a_soc_playCatch_throwSoft", "a2a_soc_playCatch_throwHard", "a2a_soc_playCatch_throwWimpy", "a2a_soc_playCatch_throwHighSkill"],
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
        "Greet" => &["a2a_soc_Neutral_Greet_Friendly_Friendly", "a2a_soc_neutral_greet"],
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
            Age::Toddler => matches!(self, K::Crib | K::ToyBox | K::Xylophone | K::PegBox | K::PottyChair | K::StuffedToy | K::HighChair),
            // (Children can't cook or light fires, and the goggles are for teens and up.)
            Age::Child => !matches!(self, K::Crib | K::Xylophone | K::PegBox | K::PottyChair | K::HighChair | K::HotTub | K::Stove | K::Fireplace | K::FirePit | K::VrGoggles | K::WorkoutBench | K::Puddle),
            _ => !matches!(self, K::Crib | K::ToyBox | K::Xylophone | K::PegBox | K::JungleGym | K::Slide | K::StuffedToy | K::ToyOven),
        }
    }

    /// Whether this Sim can use it: as their age allows, and urinals are for men and boys.
    pub fn suits(self, sim: &crate::sim::Sim) -> bool {
        self.usable_by(sim.age) && !(self == Self::Urinal && sim.female)
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
        ObjectKind::ShowerTub => &SHOWER_TUB,
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
        ObjectKind::Candle => &CANDLE,
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
        ObjectKind::WorkoutBench => &WORKOUT_BENCH,
        ObjectKind::Chess => &CHESS,
        ObjectKind::Crib => &CRIB,
        ObjectKind::ToyBox => &TOYBOX,
        ObjectKind::Xylophone => &XYLOPHONE,
        ObjectKind::PegBox => &PEGBOX,
        ObjectKind::PottyChair => &POTTY,
        ObjectKind::Dresser => &DRESSER,
        ObjectKind::Grill => &GRILL,
        ObjectKind::HotBeverage => &HOT_BEVERAGE,
        ObjectKind::DrinkTray => &DRINK_TRAY,
        ObjectKind::HighChair => &HIGH_CHAIR,
        ObjectKind::AlarmClock => &ALARM,
        ObjectKind::TrashCan | ObjectKind::TrashCompactor => &TRASH,
        ObjectKind::OutdoorTrashCan => &OUTDOOR_TRASH,
        ObjectKind::WasteChute => &WASTE_CHUTE,
        ObjectKind::Washer => &WASHER,
        ObjectKind::Dryer => &DRYER,
        ObjectKind::Clothesline => &CLOTHESLINE,
        ObjectKind::ClothingPile => &CLOTHING_PILE,
        ObjectKind::Hamper => &HAMPER,
        ObjectKind::Dishwasher => &DISHWASHER,
        ObjectKind::FishBowl => &FISHBOWL,
        ObjectKind::Telescope => &TELESCOPE,
        ObjectKind::SwingSet => &SWINGSET,
        ObjectKind::HotTub => &HOTTUB,
        ObjectKind::DollHouse => &DOLLHOUSE,
        ObjectKind::JungleGym => &JUNGLEGYM,
        ObjectKind::Slide => &SLIDE,
        ObjectKind::Puddle => &PUDDLE,
        ObjectKind::Foosball => &FOOSBALL,
        ObjectKind::PoolTable => &POOL_TABLE,
        ObjectKind::Dartboard => &DARTBOARD,
        ObjectKind::BowlingLane => &BOWLING,
        ObjectKind::Karaoke => &KARAOKE,
        ObjectKind::ArcadeMachine => &ARCADE,
        ObjectKind::ClawMachine => &CLAW,
        ObjectKind::WhackAGnome => &WHACK,
        ObjectKind::SkeeBall => &SKEEBALL,
        ObjectKind::Horseshoes => &HORSESHOES,
        ObjectKind::Shuffleboard => &SHUFFLEBOARD,
        ObjectKind::MechanicalBull => &MECH_BULL,
        ObjectKind::Trampoline => &TRAMPOLINE,
        ObjectKind::Sandbox => &SANDBOX,
        ObjectKind::Piano => &PIANO,
        ObjectKind::Keyboard => &KEYBOARD,
        ObjectKind::Drums => &DRUMS,
        ObjectKind::LaserHarp => &LASER_HARP,
        ObjectKind::VideoGame => &VIDEOGAME,
        ObjectKind::VrGoggles => &VRGOGGLES,
        ObjectKind::StuffedToy => &STUFFED_TOY,
        ObjectKind::Sprinkler => &SPRINKLER,
        ObjectKind::DivingBoard => &DIVING_BOARD,
        ObjectKind::Bar => &BAR,
        ObjectKind::FoodReplicator => &FOOD_REPLICATOR,
        ObjectKind::FoodSynthesizer => &FOOD_SYNTHESIZER,
        ObjectKind::VendingSoda => &VENDING_SODA,
        ObjectKind::VendingCandy => &VENDING_CANDY,
        ObjectKind::VendingBoth => &VENDING_BOTH,
        ObjectKind::Art => &ART,
        ObjectKind::Teleporter => &TELEPORTER,
        ObjectKind::BodySculptor => &BODY_SCULPTOR,
        ObjectKind::MoodletManager => &MOODLET_MANAGER,
        ObjectKind::Table => &COUNTER,
        ObjectKind::Urinal => &URINAL,
        ObjectKind::ToyOven => &TOY_OVEN,
        ObjectKind::FirePit => &FIRE_PIT,
        ObjectKind::Buffet => &BUFFET,
        ObjectKind::PicnicBasket => &PICNIC,
        ObjectKind::PicnicTable => &PICNIC_TABLE,
        ObjectKind::Car => &CAR,
        ObjectKind::Snowman => &SNOWMAN,
        ObjectKind::SnowAngel => &SNOW_ANGEL,
        ObjectKind::LeafPile => &LEAF_PILE,
        ObjectKind::Door => &DOOR,
        ObjectKind::Light => &LIGHT,
        ObjectKind::PetBowl => &PET_BOWL,
        ObjectKind::Terrarium => &TERRARIUM,
        ObjectKind::BirdCage => &BIRD_CAGE,
        ObjectKind::SlippySlide => &SLIPPY_SLIDE,
        ObjectKind::BoxStall => &BOX_STALL,
        ObjectKind::LitterBox => &LITTER_BOX,
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
    /// An inventory stack's action chosen from its pie menu (never queued).
    InventoryItem(crate::inventory::ItemButton, Entity, crate::inventory::Stack),
    Object { target: Entity, def: usize },
    Social { target: Entity, social: usize },
    GoHere(Vec2, u8),
    GoToWork,
    /// Off to school by themselves (the career panel's Go to School).
    GoToSchool,
    /// Out jogging along the street, then back to where they set out from (`home`, filled in
    /// as they go).
    Jog { home: Vec2 },
    /// Apply for a career at the computer.
    JoinCareer { target: Entity, track: usize },
    /// Phone someone and invite them over.
    Invite { target: Entity },
    /// Phone a friend for a chat.
    PhoneChat { target: Entity },
    /// A prank call to someone they know (`pranks`).
    PrankCall { target: Entity },
    /// A Workaholic's call to Check Up On Work (`computertraits`).
    CheckUpOnWork,
    /// A call to check on the household's real estate: its dialog (`estatedialog`).
    CheckRealEstate,
    /// The phone's "Services...": the game's Telephone Services dialog once through
    /// (`servicesdialog`).
    CallServices,
    /// A sleeping Sim woken (the game's immediate `Sim.WakeUp`: their sleep cut short; never
    /// queued).
    WakeUp { target: Entity },
    /// A baby or toddler picked up, put down or put in a crib (`carry`); how long its clips
    /// take, once they've begun.
    Carry { child: Entity, step: crate::carry::CarryStep, secs: f32 },
    /// Calls for the police (a burglar about), the fire department (a fire at home), and
    /// everybody out to come home (the game's `CallPolice`, `CallFirefighters`,
    /// `CallEverybodyHome`).
    CallPolice,
    CallFirefighters,
    CallEverybodyHome,
    /// Asking someone out to a community lot by phone, or on a date there (`meetups`).
    InviteOut { target: Entity, lot: usize, date: bool },
    /// A call for time off work: sick, fake sick, or unpaid days (`timeoff`).
    TimeOff(crate::timeoff::Ask),
    /// A call to quit their job (the game's `CallToQuitWork`).
    QuitJobCall,
    /// The phones silenced, or not, there and then (`calls::PhonesSilenced`: never queued).
    SilencePhone,
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
    /// Something done for who they are (`traitacts`): talking to themselves, freaking out, a
    /// fake faint, a TaDa!; and how long it takes (game minutes).
    TraitAct { act: crate::traitacts::TraitAct, minutes: f32 },
    /// A doctor reading the day's medical journal from their inventory (`careermetrics`).
    ReadJournal,
    /// Plant a seed here.
    PlantSeed { at: Vec2, level: u8, plant: usize },
    /// Something done out on the ground (Seasons: a snowman, a snow angel, catching snowflakes).
    Outdoor { at: Vec2, level: u8, what: crate::seasonal::Outdoor },
    /// A pet adopted by phone (its kind: `ac` a cat, `cd` a puppy, `ah` a horse...).
    AdoptPet { kind: &'static str },
    /// A social with a pet (`pets::PET_SOCIALS`): where the pet was, its kind.
    PetSocial { target: Entity, social: usize, at: Vec2, kind: &'static str },
    /// A ride on a horse (`riding`): which, and where to get on it (or only to take its saddle
    /// off).
    Ride { horse: Entity, slot: Vec2, unsaddle: bool },
    /// Off the horse they're riding.
    Dismount,
    /// A dog of the household's taught a trick, or asked for one it knows (`dogtricks`): which,
    /// the trick (`dogtricks::TRICKS`), where it was, its kind.
    DogTrick { target: Entity, trick: u8, teach: bool, at: Vec2, kind: &'static str },
    /// Putting a fire out (`fire`): which, where, and on what floor.
    Extinguish { target: Entity, at: Vec3, level: u8 },
    /// Phone round to throw a party.
    ThrowParty,
    /// Drive to a community lot and spend time there.
    GoToLot { lot: usize },
    /// Off to a community lot by the Teleporter (from its pad).
    Teleport { pad: Entity, lot: usize },
    /// An interaction's ending played where the Sim is (out of bed, the book closed), then up
    /// to where they stand.
    Outro { clips: &'static [&'static str], then: Option<&'static str>, secs: f32, stand_at: Option<Vec2>, target: Entity },
    /// Drive home from the community lot.
    GoHomeFromLot,
    /// An option greyed on a menu, and why (`stress`): never chosen.
    Greyed(String),
    /// Something learned at once, from a menu (Inspect Water): its words, a notice (never
    /// queued).
    Notice(String),
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
    /// An activity managed by another system (such as novel writing) finished its work.
    pub completed: bool,
    /// Genre selected for this queued writing action, consumed when it starts writing.
    pub novel_genre: Option<usize>,
    pub outfit_choice: Option<crate::simbody::OutfitKind>,
    pub canvas_choice: Option<u8>,
    /// The TV channel chosen to watch ("Change to..."; an index into `tvchannels::CHANNELS`).
    pub channel_choice: Option<u8>,
    /// The skill book chosen to read (an index into the game data's `skill_books`).
    pub book_choice: Option<u16>,
    pub fish_choice: Option<(String, u8)>,
    pub recipe_choice: Option<usize>,
    /// The composition chosen to perform (an index into the game data's `compositions`).
    pub song_choice: Option<u16>,
    /// The recipe's made for one (to eat themselves: "Have Dinner"), not everyone.
    pub for_one: bool,
    /// Where the Sim came up to the object (a hot tub got into from a side, a chair at a table
    /// from behind): where they get up and away from it again.
    pub approach: Option<Vec2>,
    /// What a social came to, decided as it began (applied as it ends).
    pub social_outcome: Option<crate::conversation::Outcome>,
    /// A social had where the two sit (`seatedtalk`), not walked over for.
    pub seated: bool,
    /// Gone at a jog (the game's "Jog Here"): `jog`.
    pub jog: bool,
    /// Who they chat with online, chosen from "Chat..." (`chatonline`).
    pub chat_with: Option<Entity>,
    /// A donation chosen at the mailbox: to what, and how much (`charity`).
    pub donation: Option<(String, i64)>,
    /// Where in the pool a swim's to (the pool's water clicked: "Swim"), to tread water there.
    pub swim_to: Option<Vec2>,
}

impl Action {
    pub fn new(label: impl Into<String>, kind: ActionKind, autonomous: bool) -> Self {
        Self { label: label.into(), kind, phase: Phase::Start, autonomous, cancel: false, completed: false, novel_genre: None, outfit_choice: None, canvas_choice: None, channel_choice: None, book_choice: None, fish_choice: None, recipe_choice: None, song_choice: None, for_one: false, approach: None, social_outcome: None, seated: false, jog: false, chat_with: None, donation: None, swim_to: None }
    }
}

/// Whether a Sim's asleep: in bed (or napping), sleeping now.
pub fn asleep(q: &ActionQueue) -> bool {
    q.0.front().is_some_and(|a| matches!(a.label.as_str(), "Sleep" | "Nap") && matches!(a.phase, Phase::Running(_)))
}

#[derive(Component, Default)]
pub struct ActionQueue(pub VecDeque<Action>);

/// Objects a Sim couldn't find a way to lately (and when): left alone by their autonomy for a
/// while, rather than tried again and again.
#[derive(Component, Default)]
pub struct RouteFailed(pub Vec<(Entity, f64)>);

/// A need this low comes before chores and pastimes that do nothing for it.
const CRITICAL_NEED: f32 = -60.0;

/// How long a Sim leaves an object they couldn't reach (game minutes).
const ROUTE_FAIL_MINUTES: f64 = 120.0;

impl ActionQueue {
    /// What the Sim is about, looking past an ending still playing (out of bed, the book
    /// closed) to the action it ends or the one after.
    pub fn current(&self) -> Option<&Action> {
        self.0.iter().find(|a| !matches!(a.kind, ActionKind::Outro { .. }))
    }

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

    /// Practice: that many of the game's skill points more (already for traits and mood); the
    /// level reached if it's a new one.
    pub fn add_points(&mut self, skill: &'static str, points: f32) -> Option<u32> {
        let table = skill_points(skill);
        let v = self.0.entry(skill).or_insert(0.0);
        let before = *v as u32;
        *v = points_to_level(table, level_to_points(table, *v) + points.max(0.0));
        (*v as u32 > before).then_some(*v as u32)
    }

    /// The game's skill points a Sim has in a skill.
    pub fn points(&self, skill: &str) -> f32 {
        level_to_points(skill_points(skill), self.0.get(skill).copied().unwrap_or(0.0))
    }
}

/// The skill points it takes to reach each level (running totals, level 1 first), as the game's
/// `Skills` table has them for most skills (its first row's).
pub const SKILL_POINTS: [u32; 10] = [1800, 3150, 5550, 9450, 15150, 23550, 35250, 50850, 71250, 98250];

/// Each skill's own points per level, once the game's data is in.
static SKILL_TABLES: std::sync::OnceLock<HashMap<String, Vec<u32>>> = std::sync::OnceLock::new();

fn load_skill_points(ui: Option<Res<crate::icons::GameUi>>, mut done: Local<bool>) {
    let (false, Some(ui)) = (*done, ui) else { return };
    *done = true;
    let tables: HashMap<String, Vec<u32>> = ui.data.skills.iter().filter(|s| s.points.len() == 10).map(|s| (s.hex.clone(), s.points.clone())).collect();
    info!("skill points: {} skills' levels", tables.len());
    let _ = SKILL_TABLES.set(tables);
}

/// A skill's points per level.
/// Whether the game has a skill of that name (a social group's influence, asked for by some of
/// the packs' careers, isn't one); any, before the game's tables are in.
pub fn known_skill(skill: &str) -> bool {
    SKILL_TABLES.get().is_none_or(|t| t.contains_key(skill))
}

pub fn skill_points(skill: &str) -> &'static [u32] {
    SKILL_TABLES.get().and_then(|t| t.get(skill)).map_or(&SKILL_POINTS, |v| v.as_slice())
}

/// A skill level (with the way to the next as its fraction) as the game's skill points.
pub fn level_to_points(table: &[u32], level: f32) -> f32 {
    let n = table.len();
    let l = (level.max(0.0) as usize).min(n);
    if l >= n {
        return table[n - 1] as f32;
    }
    let lo = if l == 0 { 0.0 } else { table[l - 1] as f32 };
    lo + level.fract() * (table[l] as f32 - lo)
}

/// The game's skill points as a level (and the way to the next).
pub fn points_to_level(table: &[u32], points: f32) -> f32 {
    let mut lo = 0.0;
    for (l, &next) in table.iter().enumerate() {
        let next = next as f32;
        if points < next {
            return l as f32 + (points - lo) / (next - lo).max(1.0);
        }
        lo = next;
    }
    table.len() as f32
}

/// The skill points an hour performing a composition (`Perform`'s `SkillGuitar`: 780).
const PERFORM_POINTS: f32 = 780.0;

/// The game's skill points an hour practising (each interaction's tuning, its `Skill*`
/// change: `ContinuousFlow` points an hour).
pub fn practice_points(name: &str) -> f32 {
    match name {
        // (Play_Piano, Play_Guitar, Play_Drums, Play_LaserHarp.)
        "Play Piano" | "Play Keyboard" | "Play Guitar" | "Play Drums" | "Play Laser Harp" => 420.0,
        // (Paint_Easel.)
        "Paint" => 430.0,
        // (PracticeChess_ChessTable, PlayChess_Computer; against someone, the chess table's
        // kPlayWithLogicMultiplier, half again.)
        "Play Chess" => 400.0,
        // (An arcade machine's game: `ArcadeMachineBase.PlayGame`'s SkillArcadeMachine.)
        "Play Arcade Game" => 1200.0,
        "Play a Ranked Match" => 600.0,
        // (PracticeWriting_Computer; a novel, the Computer's kWritingNovelPointsPerHour.)
        "Practice Writing" => 650.0,
        // (SolveHardProblems_Computer's SkillLogic.)
        "Solve Hard Problems" => 200.0,
        "Write Novel" => 500.0,
        // (LookThroughOrStargaze_Telescope.)
        "Stargaze" => 800.0,
        // (RideBull_MechanicalBull; Play_Trampoline's.)
        "Ride Mechanical Bull" => 800.0,
        "Jump on Trampoline" => 100.0,
        // (PracticeSpeech_Mirror.)
        "Practice Speech" => 420.0,
        // (WorkOut_Treadmill, WorkOut_WorkoutBench, SwimmingInPool's kSkillGainRatePerMinute.)
        "Work Out" => 780.0,
        "Pump Iron" => 1014.0,
        "Swim" => 480.0,
        // (Cooking at the stove or the grill: FryingPan_CookOnStove, Grillable_Grill.)
        "Cook Dinner" | "Bake Birthday Cake" | "Prepare Food" | "Grill" => 600.0,
        // (FishHere_Terrain.)
        "Fish" => 300.0,
        // (Gardening's comes of the plants themselves: WaterPlant_Plant, WeedPlant_Plant and
        // Harvest_HarvestPlant have none.)
        "Water" | "Weed" | "Harvest" | "Fertilize" | "Revive" | "Dispose" | "Talk to Plant" => 0.0,
        _ => 420.0,
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

/// The notices lately given (each with the seconds it's left to the old interface), and how
/// many there have been (the live HUD's panel takes the new ones from it: `livehud::NoteLog`).
#[derive(Resource, Default)]
pub struct Notifications(pub Vec<(String, f32)>, pub u64);

impl Notifications {
    pub fn push(&mut self, s: impl Into<String>) {
        let s = s.into();
        info!("TNS: {s}");
        self.1 += 1;
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

/// How often a visitor has set off home and not got to the way off the lot.
#[derive(Component, Default)]
pub struct HomeTries(u8);

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

/// Where someone arriving at the lot's edge is set down: about the way off it, a little to one
/// side or the other (never all on one spot, two coming home together).
pub fn arrival_spot(exit: Vec2) -> Vec2 {
    let mut rng = rand::rng();
    exit + Vec2::new(rng.random_range(-0.9..0.9), rng.random_range(-0.9..0.9))
}

/// How badly a little one must need something for their care to come before almost anything
/// else, and how much before.
const CARE_URGENT: f32 = -40.0;
const CARE_URGENT_SCORE: f32 = 1000.0;

/// Invited over: arrives at the lot exit at this time.
#[derive(Component)]
pub struct Invited {
    pub arrive_at: f64,
}

/// What a guest says, going home (the game's `Gameplay/Core/Lot/GoHome:NPCLeaving1`, `2`), and
/// going home in need (`VisitLot:GuestInMotiveDistress`).
const GOODBYES: [&str; 2] = ["I need to go home now. Bye!", "Looks like it's time to leave. See you!"];
const GUEST_IN_DISTRESS: &str = "I'd like to stick around, but I really need to go home and take care of a few things. Call me again soon!";

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
    mut leaving: Query<(Entity, &mut ActionQueue, &Transform, Option<&HomeTries>), (With<GoingHome>, Without<OffLot>)>,
    mut visitors: Query<(Entity, &Visitor, &Motives, &Sim, Has<crate::doorbell::AtTheDoor>, Has<crate::guests::StayingOver>), (Without<GoingHome>, Without<OffLot>, Without<HouseholdMember>, Without<JoinHousehold>)>,
    mut arriving: Query<(Entity, &Invited, &mut Transform, Option<&mut ActionQueue>), Without<GoingHome>>,
    (town, mut household, mut notes, members): (Option<Res<crate::premade::TownPremades>>, Option<ResMut<Household>>, ResMut<Notifications>, Query<(), With<HouseholdMember>>),
    (building, party): (Option<Res<crate::building::ActiveBuilding>>, Option<Res<Party>>),
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
    for (e, v, motives, sim, at_door, staying) in &mut visitors {
        // (Badly in need of something, a guest goes home to see to it: the game's
        // `GuestInMotiveDistress`; not one staying the night.)
        let distress = !staying && [HUNGER, BLADDER, ENERGY, HYGIENE].iter().any(|&i| motives.0[i] < CRITICAL_NEED);
        if clock.minutes >= v.leave_at || distress {
            commands.entity(e).insert(GoingHome);
            // (Saying goodbye, once let in: `Lot.GoHome`'s lines.)
            if !at_door {
                let line = if distress {
                    GUEST_IN_DISTRESS
                } else {
                    GOODBYES[rand::rng().random_range(0..GOODBYES.len())]
                };
                notes.push(format!("{}: \"{line}\"", sim.full_name()));
            }
        }
    }
    for (e, mut queue, tf, tries) in &mut leaving {
        let at_exit = Vec2::new(tf.translation.x, tf.translation.z).distance(exit.0) < 1.5;
        // (Three times set off and still not there, with no way to the street (a houseboat's guests
        // come and go by water): they're off from where they are.)
        let given_up = queue.0.is_empty() && tries.is_some_and(|t| t.0 >= 3);
        if at_exit || given_up {
            queue.0.clear();
            commands.entity(e).remove::<(GoingHome, Visitor, crate::nav::PathFollow, HomeTries, crate::guests::StayingOver)>().insert((OffLot, Visibility::Hidden));
        } else if queue.0.is_empty() {
            queue.0.push_back(Action::new("Go Home", ActionKind::GoHere(exit.0, 1), true));
            commands.entity(e).insert(HomeTries(tries.map_or(1, |t| t.0 + 1)));
        }
    }
    for (e, inv, mut tf, queue) in &mut arriving {
        if clock.minutes >= inv.arrive_at {
            let at = arrival_spot(exit.0);
            tf.translation = Vec3::new(at.x, world.data.heightmap.sample(at.x, at.y), at.y);
            let day_end = (clock.minutes / 1440.0).floor() * 1440.0 + 22.0 * 60.0;
            let leave_at = day_end.max(clock.minutes + 240.0);
            // (Someone from about town, come over: with a mind of their own while they're here, as
            // the town's Sims met out and about have.)
            commands
                .entity(e)
                .remove::<(Invited, OffLot)>()
                .insert((Visibility::Inherited, Visitor { leave_at }))
                .insert_if_new((AutonomyTimer(rand::rng().random_range(0.5..3.0)), Skills::default()));
            // To the front door, to ring and be let in (a party's guests come straight in).
            let partying = party.is_some();
            let door = building.as_ref().and_then(|b| b.front_door(exit.0)).filter(|_| !partying);
            let back = building.as_ref().and_then(|b| b.front_door_out(exit.0, crate::doorbell::STAND_BACK));
            match (queue, door) {
                (Some(mut queue), Some((door, door_e))) => crate::doorbell::come_to_door(&mut commands, e, &mut queue, door, back, door_e.unwrap_or(e)),
                (Some(_), None) => {}
                (None, door) => {
                    let mut queue = ActionQueue::default();
                    if let Some((door, door_e)) = door {
                        crate::doorbell::come_to_door(&mut commands, e, &mut queue, door, back, door_e.unwrap_or(e));
                    }
                    commands.entity(e).insert(queue);
                }
            }
        }
    }
}

pub use crate::parties::Party;

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
pub(crate) fn run_actions(
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
            (
                Option<&crate::wishes::Wishes>,
                Option<&crate::paintings::PaintPlan>,
                Option<&crate::life::Moodlets>,
                Option<&crate::little::Pregnancy>,
                Option<&mut crate::rabbitholes::Homework>,
                (Option<&crate::simbody::Wearing>, Has<crate::dressing::ClothesSpin>),
            ),
            Option<&crate::opportunities::SimOpportunities>,
            Option<&crate::visit::OnLot>,
        ),
        (Without<GameObject>, Without<AtWork>, Without<crate::rabbitholes::AtRabbitHole>, Without<crate::riding::Mounted>),
    >,
    mut objects: Query<(&GameObject, &Transform, &mut UsedBy, Option<&Floor>), Without<Sim>>,
    (building, upper, visited, privacy, locks, cars): (
        Option<Res<crate::building::ActiveBuilding>>,
        Option<Res<UpperFloors>>,
        Option<Res<crate::visit::VisitedLot>>,
        Res<crate::privacy::Privacy>,
        Query<(Entity, &crate::doorlocks::DoorLock)>,
        Query<(&GameObject, &Transform), (Without<Sim>, Without<crate::visit::LotObject>, Without<crate::traffic::CarOut>)>,
    ),
    mut life: MessageWriter<LifeEvent>,
    people: Query<(Entity, &Sim, &crate::life::Mood, Has<HouseholdMember>), Without<GameObject>>,
    mut conceive: MessageWriter<crate::little::Conceive>,
    (upgraded, baked, paper_in_hand, prepped, serving_from, broken, unavailable, real_estate): (
        Query<&crate::upgrades::Upgrades>,
        Option<Res<crate::baked::Baked>>,
        Query<&crate::surroundings::PaperInHand>,
        Query<(), With<crate::meals::CookPrepped>>,
        Query<&crate::meals::ServingFrom>,
        Query<(), With<Broken>>,
        Query<(), Or<(With<crate::buy::HeldObject>, With<crate::buyhistory::HistoryHidden>)>>,
        Res<crate::realestate::RealEstate>,
    ),
    (mut did, journals, mut inventories, mut wear, mut seconds, double_beds, unmade, (tunings, puddles_q, weather, object_rates, mut memories, (partners_of, at_door, cuddles, convs, ghost_spots, held_q))): (
        MessageWriter<crate::journal::Did>,
        Query<&crate::journal::SkillJournal>,
        Query<&mut crate::inventory::Inventory>,
        Query<&mut Wear>,
        Query<&mut SecondPlace>,
        Query<Entity, (With<SecondPlace>, Without<Broken>)>,
        Query<(), With<Unmade>>,
        (
            Res<RepairTunings>,
            Query<&Transform, (With<crate::puddles::Puddle>, Without<Sim>)>,
            Option<Res<crate::weather::Weather>>,
            Res<ObjectRates>,
            ResMut<crate::conversation::SocialMemories>,
            (
                Query<&SocialPartner>,
                Query<(), With<crate::doorbell::AtTheDoor>>,
                Query<&crate::seatedtalk::Cuddling>,
                Res<crate::groups::Conversations>,
                Query<(Entity, &Transform, &crate::nav::Floor), (With<crate::ghosts::Ghost>, Without<crate::ghosts::Resting>, Without<crate::sim::Motives>)>,
                Query<&crate::little::Carried>,
            ),
        ),
    ),
) {
    let Some(grid) = grid else { return };
    let dt = delta.0;
    let ground = |level: u8, x: f32, z: f32| crate::nav::floor_height(&world.data, building.as_deref(), level, Vec3::new(x, 0.0, z));
    // Social effects to apply to partners after the main pass: (target, actor, social, fun, rel, pose talk)
    // (Who's talked to whom: social and fun, liking and romance, and their conversation's context.)
    let mut social_fx: Vec<(Entity, Entity, f32, f32, f32, f32, Option<crate::conversation::Shared>)> = Vec::new();
    // Needs a care social fills for the little one: (target, per-need gain).
    let mut care_fx: Vec<(Entity, [f32; 6])> = Vec::new();
    // Social partners whose animation should end: (target, actor).
    let mut partners_done: Vec<(Entity, Entity)> = Vec::new();
    // Little ones a care social picks up: (little one, grown-up, age, grown-up's transform).
    let mut carry_fx: Vec<(Entity, Entity, Age, Transform, bool)> = Vec::new();
    // (The ghosts out at night too: someone to go up to and talk with.)
    let positions: HashMap<Entity, (Vec3, u8)> = sims.iter().map(|s| (s.0, (s.3.translation, s.11.0))).chain(ghost_spots.iter().map(|(e, t, f)| (e, (t.translation, f.0)))).collect();
    let sitting: std::collections::HashSet<Entity> = sims.iter().filter(|s| s.6.pose == Pose::Sit).map(|s| s.0).collect();
    let lying: std::collections::HashSet<Entity> = sims.iter().filter(|s| s.6.pose == Pose::Lie).map(|s| s.0).collect();
    // (The places Sims have gone to at an object: a picnic table's taken.)
    let places_taken: Vec<(Entity, Entity, Vec2)> = sims
        .iter()
        .filter_map(|s| {
            let a = s.2.0.front()?;
            match a.kind {
                ActionKind::Object { target, .. } => Some((s.0, target, a.approach?)),
                _ => None,
            }
        })
        .collect();
    let partnered: HashMap<Entity, bool> = sims.iter().map(|q| (q.0, q.8.partner().is_some())).collect();
    let who: HashMap<Entity, (Sim, crate::life::Mood, bool, bool)> = people
        .iter()
        .map(|(e, s, m, member)| (e, (s.clone(), *m, member, partnered.get(&e).copied().unwrap_or(false))))
        .collect();
    // Relationship changes to apply to both Sims: (a, b, status, kissed)
    let mut status_fx: Vec<(Entity, Entity, Option<RelStatus>, bool)> = Vec::new();

    for (me, sim, mut queue, mut tf, mut motives, mut decay, mut anim, mut skills, mut rels, path, mut job, floor, (wishes, plan, moodlets, pregnancy, mut homework, (wearing, spinning)), opps, on_lot) in &mut sims {
        let mood_learning = who.get(&me).map_or(1.0, |(_, mood, _, _)| mood.skill_rate());
        // Out on a community lot: its walk grid and way out.
        let away = on_lot.and_then(|o| visited.as_deref().filter(|v| v.lot == o.0));
        let (my_grid, my_upper): (&NavGrid, Option<&UpperFloors>) = match away {
            Some(v) => (&v.grid, Some(&v.upper)),
            None => (&grid, upper.as_deref()),
        };
        let way_out = away.map(|v| v.exit).or_else(|| exit.as_ref().map(|e| e.0));
        // (A talk had lying in bed: the double bed they're relaxing in, and whether on its near
        // side, for a cuddle there.)
        let in_bed: Option<bool> = queue.0.get(1).and_then(|a| match a.kind {
            ActionKind::Object { target, .. } if objects.get(target).is_ok_and(|(o, ..)| o.kind == ObjectKind::BedDouble) => Some(!seconds.get(target).is_ok_and(|s| s.0 == Some(me))),
            _ => None,
        });
        let Some(action) = queue.0.front_mut() else {
            if anim.pose != Pose::Walk && anim.pose != Pose::Stand && path.is_none() {
                anim.pose = Pose::Stand;
            }
            continue;
        };
        let mut finished = false;
        let mut stand_up_at: Option<Vec2> = None;
        // (Its clips, what follows, how long, the object, and whether played mirrored: the far
        // side of a double bed.)
        let mut outro: Option<(&'static [&'static str], Option<&'static str>, f32, Entity, bool)> = None;

        if let ActionKind::Object { target, def } = action.kind {
            let second = seconds.get(target).ok().map(|s| s.0);
            let valid = !unavailable.contains(target) && objects.get(target).is_ok_and(|(obj, _, used, _)| {
                object_action_available(me, obj.kind, def, owner_for(me, obj.kind, def, used.0, second), broken.contains(target), action.phase)
            });
            if !valid { action.cancel = true; }
        }

        let maintenance = match action.kind {
            ActionKind::Repair { target } => Some((target, None)),
            ActionKind::Upgrade { target, bit } => Some((target, Some(bit))),
            _ => None,
        };
        if let Some((target, bit)) = maintenance {
            let valid = !unavailable.contains(target) && objects.get(target).is_ok_and(|(obj, _, used, _)| {
                maintenance_available(me, obj.kind, used.0, broken.contains(target), upgraded.get(target).ok(), bit)
            });
            if !valid { action.cancel = true; }
        }

        // (A caller at the door who's given up and gone home isn't greeted.)
        if let ActionKind::Social { target, social } = action.kind
            && crate::social::social(social).effect == SocialEffect::Greet
            && !at_door.contains(target)
        {
            action.cancel = true;
        }

        // Cancellation
        // (Not part way up or down the stairs: they go on to the end of the flight first, rather
        // than stand there on the steps.)
        if action.cancel && matches!(action.phase, Phase::Routing) && path.as_ref().is_some_and(|p| !p.done && p.waypoints.first().is_some_and(|w| w.climb.is_some())) {
            continue;
        }
        if action.cancel {
            if let ActionKind::Outro { stand_at, .. } = action.kind {
                stand_up_at = stand_at;
            }
            if let ActionKind::Object { target, .. } | ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } = action.kind
                && let Ok((obj, otf, mut used, _)) = objects.get_mut(target)
            {
                if used.0 == Some(me) {
                    used.0 = None;
                }
                if let Ok(mut s) = seconds.get_mut(target)
                    && s.0 == Some(me)
                {
                    s.0 = None;
                }
                if matches!(action.phase, Phase::Running(_)) {
                    stand_up_at = Some(action.approach.unwrap_or_else(|| obj.use_point(otf)));
                }
            }
            finished = true;
        }

        if !finished {
            // (Setting out to talk to someone who's already come over to talk to them: they wait
            // where they are, the other side of it, and have theirs once it's done, rather than
            // each walking after the other for ever.)
            if let ActionKind::Social { target, .. } = action.kind
                && matches!(action.phase, Phase::Start | Phase::Routing)
                && partners_of.get(me).is_ok_and(|p| p.0 == target)
            {
                if matches!(action.phase, Phase::Routing) {
                    commands.entity(me).remove::<PathFollow>();
                    action.phase = Phase::Start;
                }
                continue;
            }
            // (Had where they sit, or with one of a conversation they're standing in: no walk to it.)
            // (On the same floor, within reach: a floor apart, they'd never be close enough.)
            let grouped = matches!(action.kind, ActionKind::Social { target, .. }
                if convs.together(me, target) && positions.get(&target).is_some_and(|p| p.1 == floor.0 && p.0.distance(tf.translation) < crate::groups::IN_RING));
            if (action.seated || grouped) && matches!(action.phase, Phase::Start) {
                action.phase = Phase::Routing;
            }
            match action.phase {
                Phase::Start => {
                    // (While the walk grids are being made afresh, a way is planned a frame on,
                    // not given up for one that's still to be there.)
                    if !my_grid.ready() || my_upper.is_some_and(|u| !u.ready()) {
                        continue;
                    }
                    let dest: Option<(Vec2, u8)> = match &action.kind {
                        ActionKind::Object { target, def } => match objects.get(*target) {
                            Ok((obj, otf, used, of)) => {
                                // (A group meal's served one at a time: the rest wait their turn.)
                                if used.0.is_some_and(|u| u != me) && obj.kind == ObjectKind::Meal {
                                    continue;
                                }
                                // (A bathroom someone's using is theirs: the rest wait till it's free.)
                                if privacy.objects.get(target).is_some_and(|u| *u != me) {
                                    commands.entity(me).insert_if_new(crate::privacy::WaitingForPrivacy::new(clock.minutes));
                                    continue;
                                }
                                if owner_for(me, obj.kind, *def, used.0, seconds.get(*target).ok().map(|s| s.0)).is_some_and(|u| u != me) && !watches(obj.kind, *def) {
                                    if !action.autonomous {
                                        notes.push(format!("{} can't use the {}: it's in use.", sim.first, obj.name));
                                    }
                                    None
                                } else if obj.kind == ObjectKind::PicnicTable {
                                    // The nearest place no one else has.
                                    let me_at = Vec2::new(tf.translation.x, tf.translation.z);
                                    GameObject::picnic_places(otf)
                                        .into_iter()
                                        .filter(|(stand, seat)| {
                                            !places_taken.iter().any(|(who, t, at)| *who != me && t == target && at.distance(*stand) < 0.3)
                                                && !positions.iter().any(|(who, (p, _))| *who != me && Vec2::new(p.x, p.z).distance(*seat) < 0.35)
                                        })
                                        .min_by(|a, b| a.0.distance(me_at).total_cmp(&b.0.distance(me_at)))
                                        .map(|(stand, _)| (stand, of.map_or(1, |f| f.0)))
                                } else if interactions_for(obj.kind).get(*def).is_some_and(|d| d.special == Special::MakeBed) {
                                    // (Made from beside it: the nearer side, the other if that can't be reached.)
                                    let me_at = Vec2::new(tf.translation.x, tf.translation.z);
                                    let sides = GameObject::bed_sides(obj.center, obj.half).map(|p| {
                                        let w = otf.transform_point(Vec3::new(p.x, 0.0, p.y));
                                        Vec2::new(w.x, w.z)
                                    });
                                    let near = if sides[0].distance(me_at) <= sides[1].distance(me_at) { sides[0] } else { sides[1] };
                                    Some((near, of.map_or(1, |f| f.0)))
                                } else if interactions_for(obj.kind).get(*def).is_some_and(|d| d.special == Special::DanceToMusic) {
                                    // (Dancing to it: a few metres off.)
                                    Some((crate::tips::dance_spot(obj, otf, me), of.map_or(1, |f| f.0)))
                                } else if watches(obj.kind, *def) {
                                    // (Watching someone play: before them, a couple of metres off.)
                                    Some((crate::tips::watch_spot(obj, otf, me), of.map_or(1, |f| f.0)))
                                } else if obj.kind == ObjectKind::Tv && interactions_for(obj.kind).get(*def).is_some_and(|d| d.name == "Work Out") {
                                    // (Out in front of the set, to work out along with it.)
                                    Some((obj.workout_spot(otf), of.map_or(1, |f| f.0)))
                                } else if let Some(seats) = obj.sofa_seats().filter(|_| !whole_of(obj.kind, *def)) {
                                    // (Before the cushion that'll be theirs: the second, someone on the first.)
                                    let seat = seats[used.0.is_some_and(|u| u != me) as usize];
                                    let p = obj.use_point(otf) + (otf.rotation * Vec3::X).xz() * seat;
                                    Some((p, of.map_or(1, |f| f.0)))
                                } else if obj.kind == ObjectKind::BedDouble && interactions_for(obj.kind).get(*def).is_some_and(|d| d.on_object) && used.0.is_some_and(|u| u != me) {
                                    // (A double bed's far side, someone in the first: walked to on that side.)
                                    let local = otf.compute_affine().inverse().transform_point3(Vec3::new(obj.use_point(otf).x, otf.translation.y, obj.use_point(otf).y));
                                    let w = otf.transform_point(Vec3::new(2.0 * obj.center.x - local.x, 0.0, local.z));
                                    Some((Vec2::new(w.x, w.z), of.map_or(1, |f| f.0)))
                                } else {
                                    Some((obj.use_point(otf), of.map_or(1, |f| f.0)))
                                }
                            }
                            Err(_) => None,
                        },
                        ActionKind::Social { target, social } => positions.get(target).map(|(p, l)| {
                            let mine = Vec2::new(tf.translation.x, tf.translation.z);
                            let theirs = Vec2::new(p.x, p.z);
                            // (Someone talking in a group: a place in its ring, as the game's
                            // conversations stand.)
                            let ring: Vec<Vec2> = std::iter::once(*target).chain(convs.others(*target)).filter(|m| *m != me).filter_map(|m| positions.get(&m).map(|q| q.0.xz())).collect();
                            if crate::groups::huddled(&ring) && crate::social::apart(crate::social::social(*social)) < 1.0 {
                                (crate::groups::ring_spot(&ring, mine), *l)
                            } else {
                                (theirs + (mine - theirs).normalize_or(Vec2::X) * crate::social::apart(crate::social::social(*social)), *l)
                            }
                        }),
                        ActionKind::GoHere(p, l) => Some((*p, *l)),
                        // (Kneeling beside the spot.)
                        ActionKind::PlantSeed { at, level, .. } => Some((*at + Vec2::new(0.0, 0.7), *level)),
                        ActionKind::Outdoor { at, level, what } => Some((what.stand(*at), *level)),
                        // (Before the little one, or at the crib; put down where they stand, there
                        // and then.)
                        ActionKind::Carry { child, step, .. } => {
                            let mine = Vec2::new(tf.translation.x, tf.translation.z);
                            let apart = people.get(*child).map_or(0.7, |p| crate::carry::apart(p.1.age));
                            match step {
                                crate::carry::CarryStep::PutDown(None) => {
                                    action.phase = Phase::Running(0.0);
                                    commands.entity(me).insert(crate::carry::CarryMove::new(*child, *step));
                                    continue;
                                }
                                crate::carry::CarryStep::PickUp { crib: Some(c) } | crate::carry::CarryStep::InCrib(c) => {
                                    objects.get(*c).ok().map(|(obj, otf, _, of)| (obj.use_point(otf), of.map_or(1, |f| f.0)))
                                }
                                crate::carry::CarryStep::PickUp { crib: None } => positions.get(child).map(|p| (crate::carry::beside(p.0.xz(), mine, apart), p.1)),
                                crate::carry::CarryStep::PutDown(Some(at)) => Some((crate::carry::beside(*at, mine, apart), floor.0)),
                            }
                        }
                        // (Beside a horse to get on it, where the game's jig stands them.)
                        ActionKind::Ride { slot, .. } => Some((*slot, 1)),
                        // (Standing before the pet, as far off as the game's jig has them.)
                        ActionKind::PetSocial { at, kind, .. } | ActionKind::DogTrick { at, kind, .. } => {
                            let mine = Vec2::new(tf.translation.x, tf.translation.z);
                            // (A belly rub further off: the dog rolls over between them.)
                            let apart = match &action.kind {
                                ActionKind::PetSocial { social, .. } => crate::pets::PET_SOCIALS.get(*social).map_or(0.0, |s| s.apart),
                                _ => 0.0,
                            };
                            Some((*at + (mine - *at).normalize_or(Vec2::X) * (crate::pets::social_distance(kind) + apart), 1))
                        }
                        // (Before the fire, as near as the game's `PlanRouteToExtinguish` has them.)
                        ActionKind::Extinguish { at, level, .. } => {
                            let (mine, at) = (Vec2::new(tf.translation.x, tf.translation.z), at.xz());
                            Some((at + (mine - at).normalize_or(Vec2::X) * crate::fire::extinguish_distance(&sim.traits), *level))
                        }
                        // (Off out from home with a car of their own: to it, to drive off in it, as
                        // the game's `CarOwnable` has them; else to the street.)
                        ActionKind::GoToWork | ActionKind::Visit { .. } | ActionKind::GoToLot { .. } if away.is_none() && cars.iter().any(|(o, _)| o.kind == ObjectKind::Car) => {
                            cars.iter().find(|(o, _)| o.kind == ObjectKind::Car).map(|(o, ctf)| (o.use_point(ctf), 1))
                        }
                        ActionKind::GoToWork | ActionKind::GoToSchool | ActionKind::Visit { .. } | ActionKind::GoToLot { .. } | ActionKind::GoHomeFromLot | ActionKind::Jog { .. } => way_out.map(|p| (p, 1)),
                        ActionKind::JoinCareer { target, .. } | ActionKind::Teleport { pad: target, .. } => {
                            objects.get(*target).ok().map(|(obj, otf, _, of)| (obj.use_point(otf), of.map_or(1, |f| f.0)))
                        }
                        ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } => objects.get(*target).ok().map(|(obj, otf, _, of)| (obj.use_point(otf), of.map_or(1, |f| f.0))),
                        ActionKind::Invite { .. }
                        | ActionKind::PhoneChat { .. }
                        | ActionKind::PrankCall { .. }
                        | ActionKind::CheckUpOnWork
                        | ActionKind::CheckRealEstate
                        | ActionKind::CallServices
                        | ActionKind::CallPolice
                        | ActionKind::CallFirefighters
                        | ActionKind::CallEverybodyHome
                        | ActionKind::InviteOut { .. }
                        | ActionKind::TimeOff(_)
                        | ActionKind::QuitJobCall
                        | ActionKind::SilencePhone
                        | ActionKind::Retire
                        | ActionKind::OrderPizza
                        | ActionKind::Adopt { .. }
                        | ActionKind::AdoptPet { .. }
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
                        ActionKind::ReadJournal => {
                            action.phase = Phase::Running(0.0);
                            anim.pose = Pose::Use;
                            if let Some(clip) = interaction_clip("Read Book", ObjectKind::Bookshelf) {
                                commands.entity(me).insert(clip);
                            }
                            continue;
                        }
                        ActionKind::TraitAct { act, .. } => {
                            action.phase = Phase::Running(0.0);
                            let (pose, clip) = act.clip(sim.age == Age::Child);
                            anim.pose = pose;
                            commands.entity(me).insert((clip, crate::traitacts::Began(*act)));
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
                        ActionKind::BuyReward(_) | ActionKind::InventoryItem(..) | ActionKind::WakeUp { .. } | ActionKind::Greyed(_) | ActionKind::Notice(_) | ActionKind::Dismount => None,
                        ActionKind::Outro { .. } => {
                            action.phase = Phase::Running(0.0);
                            continue;
                        }
                        ActionKind::EatHere => {
                            action.phase = Phase::Running(0.0);
                            anim.pose = Pose::Use;
                            commands.entity(me).insert(crate::anim::ActionClip::new(Some("a2o_eat_stand_fork_start_x"), &["a2o_eat_stand_fork_neat_x"]));
                            continue;
                        }
                        ActionKind::EatItem { key, quality } => {
                            if inventories.get_mut(me).is_ok_and(|mut inv| inv.consume_produce(key, *quality)) {
                                action.phase = Phase::Running(0.0);
                                anim.pose = Pose::Use;
                                commands.entity(me).insert(crate::anim::ActionClip::new(None, &["a2o_eat_stand_hand_neat"]));
                                continue;
                            }
                            notes.push(format!("{} no longer has that produce to eat.", sim.first));
                            None
                        }
                    };
                    let from = Vec2::new(tf.translation.x, tf.translation.z);
                    // (A jog comes back to where it set out from.)
                    if let ActionKind::Jog { home } = &mut action.kind {
                        *home = from;
                    }
                    // A chair pushed in at a table is reached from behind or beside it; a hot tub
                    // or a bath, got into from more than one side, from whichever can be walked
                    // to (the Wolffs' roof has a hot tub's front over the edge).
                    let alternatives: Vec<Vec2> = match &action.kind {
                        ActionKind::Object { target, .. } => objects.get(*target).ok().map(|(o, otf, ..)| o.other_approaches(otf)).unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    // (A pool's ladder or diving board filed a storey low, under the deck it stands
                    // on at a walk-out: reached from the storey above.)
                    let pool_thing = matches!(&action.kind, ActionKind::Object { target, .. } if objects.get(*target).is_ok_and(|(o, ..)| matches!(o.kind, ObjectKind::PoolLadder | ObjectKind::DivingBoard)));
                    // (At home, through no door locked against them.)
                    let member = who.get(&me).is_some_and(|w| w.2);
                    let shut: Vec<(u8, [Vec2; 2])> = if away.is_none() {
                        locks.iter().filter(|(_, l)| !l.lets(sim, member)).filter_map(|(e, _)| building.as_deref()?.door_span(e)).collect()
                    } else {
                        Vec::new()
                    };
                    if !shut.is_empty() {
                        debug!("{}'s way planned round {} locked doors", sim.first, shut.len());
                    }
                    let routed = crate::nav::with_locked(&shut, || {
                        dest.and_then(|(d, l)| {
                            plan_route(my_grid, my_upper, from, floor.0, d, l)
                                .or_else(|| {
                                    alternatives.iter().find_map(|a| {
                                        let wp = plan_route(my_grid, my_upper, from, floor.0, *a, l)?;
                                        action.approach = Some(*a);
                                        Some(wp)
                                    })
                                })
                                .or_else(|| pool_thing.then(|| plan_route(my_grid, my_upper, from, floor.0, d, l + 1)).flatten())
                        })
                    });
                    match routed {
                        Some(wp) => {
                            // Gone in the game's walk style for the way.
                            let to = wp.last().map_or(from, |w| w.p);
                            // (Where they'll get up from it again: where the walk there ends, on
                            // walkable floor, not a front pushed in under a table.)
                            if matches!(action.kind, ActionKind::Object { .. }) {
                                action.approach = Some(to);
                            }
                            let at = |p: Vec2| Vec3::new(p.x, 0.0, p.y);
                            let indoors = |p: Vec2| building.as_deref().is_some_and(|b| b.is_indoors(at(p)));
                            let lot = |p: Vec2| crate::rabbitholes::lot_at(&world.data.lots, at(p));
                            let sense = crate::nav::RouteSense {
                                distance: PathFollow::length_from(&wp, from),
                                autonomous: action.autonomous,
                                to_object: !matches!(action.kind, ActionKind::GoHere(..)),
                                same_lot: lot(from) == lot(to),
                                indoors: indoors(from) && indoors(to),
                                age: sim.age,
                                athletic: skills.level("Athletic"),
                                fatigued: moodlets.is_some_and(|m| m.has(crate::life::MoodletKind::Fatigued)),
                                pregnant: pregnancy.is_some_and(|p| p.stage >= 2),
                            };
                            let mut style = crate::nav::walk_style(&sense, rand::random::<f32>());
                            // (Jogging there: the fast jog for the athletic, as the game's.)
                            if action.jog {
                                style = if skills.level("Athletic") >= crate::jog::FAST_JOG_LEVEL { crate::nav::WalkStyle::FastJog } else { crate::nav::WalkStyle::Jog };
                            }
                            // (WALK_STYLE=<style>: the player's routes so, for a test.)
                            if !action.autonomous && let Ok(s) = std::env::var("WALK_STYLE") {
                                use crate::nav::WalkStyle::*;
                                style = [Walk, FastWalk, FastJog, Run, FastRun, Jog, OnFire].into_iter().find(|w| format!("{w:?}").eq_ignore_ascii_case(&s)).unwrap_or(style);
                            }
                            debug!("{} goes {:?} ({:.0} m)", sim.first, style, sense.distance);
                            commands.entity(me).insert(PathFollow::new(wp).with_style(style));
                            action.phase = Phase::Routing;
                            if let ActionKind::Object { target, .. } | ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } = action.kind
                                && let Ok((o, _, mut used, _)) = objects.get_mut(target)
                                && !matches!(o.kind, ObjectKind::PicnicTable | ObjectKind::Chess)
                                && !matches!(action.kind, ActionKind::Object { def, .. } if watches(o.kind, def))
                                // (A meal's taken once they're at it: one coming from the far end
                                // of the lot doesn't keep the cook beside it waiting.)
                                && o.kind != ObjectKind::Meal
                            {
                                // (A double bed's other side, when someone has the first.)
                                match seconds.get_mut(target) {
                                    // (Along a sofa: both its places.)
                                    Ok(mut s) if matches!(action.kind, ActionKind::Object { def, .. } if whole_of(o.kind, def)) => {
                                        used.0 = Some(me);
                                        s.0 = Some(me);
                                    }
                                    Ok(mut s) if matches!(action.kind, ActionKind::Object { .. }) && used.0.is_some_and(|u| u != me) => s.0 = Some(me),
                                    _ => used.0 = Some(me),
                                }
                            }
                        }
                        None => {
                            if dest.is_some() {
                                debug!("{} can't route from {from} on floor {} to {:?}", sim.first, floor.0, dest);
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
                                // (Stopped part way up the stairs or in a lift by something else coming
                                // up, they're between floors: set down on the nearest one, where its
                                // walks reach.)
                                let off_floor = (tf.translation.y - ground(floor.0, from.x, from.y)).abs() > 0.6;
                                if off_floor {
                                    let grid_of = |l: u8| if l == 1 { Some(my_grid) } else { my_upper.and_then(|u| u.grids.get(&l)) };
                                    let nearest = (0..8u8)
                                        .filter(|l| grid_of(*l).is_some())
                                        .min_by(|a, b| (tf.translation.y - ground(*a, from.x, from.y)).abs().total_cmp(&(tf.translation.y - ground(*b, from.x, from.y)).abs()));
                                    if let Some(l) = nearest
                                        && let Some(g) = grid_of(l)
                                        && let Some((x, z)) = g.nearest_free(from)
                                    {
                                        let p = g.center_of(x, z);
                                        info!("{} was between floors at {from} (floor {}, {:.1} m up): set down on floor {l} at {p}", sim.first, floor.0, tf.translation.y);
                                        tf.translation = Vec3::new(p.x, ground(l, p.x, p.y), p.y);
                                        commands.entity(me).insert(Floor(l));
                                    }
                                }
                                // (With no way even off the lot from where they stand (put out on a
                                // ledge), they're set back on their feet where the lot's walks reach,
                                // as the game resets a stuck Sim.)
                                if !off_floor
                                    && let Some(out) = way_out
                                    && plan_route(my_grid, my_upper, from, floor.0, out, 1).is_none()
                                {
                                    // (Upstairs, on their own floor where its stairs reach; else on
                                    // the ground, where the way off the lot does.)
                                    let upstairs = (floor.0 != 1).then(|| my_upper.and_then(|u| u.grids.get(&floor.0).map(|g| (u, g)))).flatten().and_then(|(u, g)| {
                                        let mut reach = vec![false; g.w * g.h];
                                        for s in &u.stairs {
                                            let p = if s.upper == floor.0 {
                                                s.top
                                            } else if s.level == floor.0 {
                                                s.bottom
                                            } else {
                                                continue;
                                            };
                                            for (i, r) in g.reachable_from(p).into_iter().enumerate() {
                                                reach[i] |= r;
                                            }
                                        }
                                        (0..g.h)
                                            .flat_map(|z| (0..g.w).map(move |x| (x, z)))
                                            .filter(|&(x, z)| reach[z * g.w + x])
                                            .map(|(x, z)| g.center_of(x, z))
                                            .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
                                    });
                                    let reach = my_grid.reachable_from(out);
                                    let near = (0..my_grid.h)
                                        .flat_map(|z| (0..my_grid.w).map(move |x| (x, z)))
                                        .filter(|&(x, z)| reach[z * my_grid.w + x])
                                        .map(|(x, z)| my_grid.center_of(x, z))
                                        .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)));
                                    if let Some(p) = upstairs {
                                        info!("{} was stuck at {from} on floor {}: set back on that floor at {p}", sim.first, floor.0);
                                        tf.translation = Vec3::new(p.x, ground(floor.0, p.x, p.y), p.y);
                                    } else if let Some(p) = near {
                                        info!("{} was stuck at {from} on floor {}: set back at {p}", sim.first, floor.0);
                                        tf.translation = Vec3::new(p.x, ground(1, p.x, p.y), p.y);
                                        commands.entity(me).insert(Floor(1));
                                    }
                                }
                            }
                            finished = true;
                        }
                    }
                }
                Phase::Routing => {
                    let arrived = path.as_ref().is_some_and(|p| p.done) || action.seated || grouped;
                    // (Changed for it first, with a spin: into pyjamas by the bed, out of their
                    // clothes by the bath, before getting in.)
                    if arrived && spinning {
                        continue;
                    }
                    // (At a meal someone else is taking a plate from: their turn first, then theirs.)
                    if arrived
                        && let ActionKind::Object { target, .. } = &action.kind
                        && let Ok((o, _, mut used, _)) = objects.get_mut(*target)
                        && o.kind == ObjectKind::Meal
                    {
                        if used.0.is_some_and(|u| u != me) {
                            continue;
                        }
                        used.0 = Some(me);
                    }
                    if arrived
                        && crate::dressing::spins(sim)
                        && let ActionKind::Object { target, def } = &action.kind
                        && let Ok((obj, ..)) = objects.get(*target)
                        && let Some(want) = interactions_for(obj.kind).get(*def).and_then(|d| crate::swim::outfit_for_use(obj.kind, d))
                        && wearing.map(|w| w.0) != Some(want)
                    {
                        crate::dressing::start(&mut commands, me, Some(want), false);
                        continue;
                    }
                    if arrived {
                        commands.entity(me).remove::<PathFollow>();
                        action.phase = Phase::Running(0.0);
                        // (Now and then a purchase sticks in the vending machine.)
                        let stuck = match &action.kind {
                            ActionKind::Object { target, def } => objects.get(*target).ok().and_then(|(obj, ..)| {
                                let defs = interactions_for(obj.kind);
                                (defs.get(*def).is_some_and(|d| d.special == Special::Vend) && rand::rng().random_bool(VEND_STUCK_CHANCE))
                                    .then(|| defs.iter().position(|d| d.special == Special::VendStuck))
                                    .flatten()
                                    .map(|i| (*target, i))
                            }),
                            _ => None,
                        };
                        if let Some((target, def)) = stuck {
                            action.kind = ActionKind::Object { target, def };
                            commands.entity(target).insert(SnackStuck);
                            // (Told of it when it was theirs to ask for.)
                            if !action.autonomous {
                                notes.push(format!("{}'s snack got stuck in the machine.", sim.first));
                            }
                        }
                        match &action.kind {
                            ActionKind::Object { target, def } => {
                                if let Ok((obj, otf, _, _)) = objects.get(*target) {
                                    let d = &interactions_for(obj.kind)[*def];
                                    anim.pose = d.pose;
                                    if d.special == Special::EatMeal {
                                        commands.entity(me).insert(crate::meals::MealRequest::PlateAt(*target));
                                    }
                                    // (A cup taken off the tray, to drink.)
                                    if d.special == Special::GrabDrink {
                                        commands.entity(me).insert(crate::hotdrinks::TookDrink(*target));
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
                                    // (Into the seat first.)
                                    let seat_in = (d.on_object && d.pose == Pose::Sit).then(|| seat_clips(obj.kind)).flatten().map(|(into, ..)| into);
                                    let clip = toddler_toy.or_else(|| interaction_clip(d.name, obj.kind)).or_else(|| seat_in.map(|_| crate::anim::ActionClip::new(None, crate::anim::SIT_LOOPS)));
                                    // (Art looked over, and then what they make of it.)
                                    let clip = if d.special == Special::ViewArt { Some(view_clip(sim, art_taste(sim, obj.price, taste_roll(me, *target)))) } else { clip };
                                    // (Down the slippy slide: one of the game's runs, or a fall one time in five.)
                                    let clip = if d.special == Special::SlippySlide {
                                        let fell = rand::random::<f32>() < SLIDE_FALL_CHANCE;
                                        if fell {
                                            commands.entity(me).insert(SlideFall);
                                        }
                                        Some(slide_clip(fell, rand::random()))
                                    } else {
                                        clip
                                    };
                                    // (A double bed's second side: the first's, mirrored.)
                                    let far_side = (obj.kind == ObjectKind::BedDouble || (obj.kind == ObjectKind::Sofa && d.pose == Pose::Sit)) && seconds.get(*target).is_ok_and(|s| s.0 == Some(me));
                                    // (WooHoo's other part, on the far side.)
                                    let clip = if far_side && matches!(d.special, Special::WooHoo | Special::TryForBaby) { Some(woohoo_clip(true)) } else { clip };
                                    // (A bed got into is unmade after; an unmade one's got into as it is.)
                                    let in_bed = matches!(obj.kind, ObjectKind::BedSingle | ObjectKind::BedDouble) && d.on_object;
                                    let clip = clip.map(|mut c| {
                                        if in_bed && c.start == Some("a2o_bed_getIn_made_x") && unmade.contains(*target) {
                                            c.start = Some("a2o_bed_getIn_unmade_x");
                                        }
                                        c
                                    });
                                    // (Only under the covers: not relaxing on top of them, as the game's
                                    // `GetOutOfBed` unmakes it only from sleep.)
                                    if in_bed && (matches!(d.name, "Sleep" | "Nap") || matches!(d.special, Special::WooHoo | Special::TryForBaby)) {
                                        commands.entity(*target).try_insert(Unmade);
                                    }
                                    // (Making the bed from its right: the game's clips mirrored.)
                                    let came = action.approach.unwrap_or(Vec2::new(tf.translation.x, tf.translation.z));
                                    let right_of_bed = otf.compute_affine().inverse().transform_point3(Vec3::new(came.x, otf.translation.y, came.y)).x > obj.center.x;
                                    let making = d.special == Special::MakeBed;
                                    if let Some(mut c) = clip {
                                        c.before = c.before.or(seat_in);
                                        c.mirror = far_side || (making && right_of_bed);
                                        commands.entity(me).insert(c);
                                    }
                                    if making {
                                        // Across the bed from where they came to its side.
                                        let across = otf.rotation * if right_of_bed { Vec3::NEG_X } else { Vec3::X };
                                        tf.rotation = Quat::from_rotation_y(across.x.atan2(across.z));
                                    }
                                    let face = otf.rotation * Quat::from_rotation_y(std::f32::consts::PI);
                                    if matches!(d.special, Special::WatchMusic | Special::DanceToMusic) {
                                        // (Standing where they came to, turned to the player: see `tips`.)
                                        anim.seat_height = 0.0;
                                    } else if obj.kind == ObjectKind::Tv && d.name == "Work Out" {
                                        let w = obj.workout_spot(otf);
                                        if tf.translation.xz().distance(w) < 1.0 {
                                            tf.translation = Vec3::new(w.x, tf.translation.y, w.y);
                                        }
                                        tf.rotation = face;
                                        anim.seat_height = 0.0;
                                    } else if d.on_object {
                                        let bed = matches!(obj.kind, ObjectKind::BedSingle | ObjectKind::BedDouble);
                                        // In a double bed, one side of it (the other's the far side).
                                        let side = if bed && obj.half.x > 0.8 { -obj.half.x * 0.5 } else { 0.0 };
                                        let side = if far_side { -side } else { side };
                                        // (On a sofa, the cushion that's theirs.)
                                        let side = match obj.sofa_seats() {
                                            Some(seats) if d.pose == Pose::Sit => seats[far_side as usize],
                                            _ => side,
                                        };
                                        let c = otf.transform_point(Vec3::new(obj.center.x + side, 0.0, obj.center.y));
                                        tf.translation = Vec3::new(c.x, otf.translation.y, c.z);
                                        tf.rotation = otf.rotation;
                                        if d.special == Special::EatMeal {
                                            // The eating clips sit the Sim back from their root,
                                            // which belongs at the table's edge.
                                            let back = if obj.kind == ObjectKind::Stool { 0.67 } else { 0.576 };
                                            tf.translation += (otf.rotation * Vec3::Z).with_y(0.0).normalize_or_zero() * back;
                                        }
                                        if obj.kind == ObjectKind::PicnicTable {
                                            // On the bench at the place come to, facing the table.
                                            let came = action.approach.unwrap_or(Vec2::new(tf.translation.x, tf.translation.z));
                                            if let Some((stand, seat)) = GameObject::picnic_places(otf).into_iter().min_by(|a, b| a.0.distance(came).total_cmp(&b.0.distance(came))) {
                                                let to = (seat - stand).normalize_or_zero();
                                                // (Eating, rooted at the table's edge as at a dining chair.)
                                                let at = if d.special == Special::EatMeal { seat + to * 0.576 } else { seat };
                                                tf.translation = Vec3::new(at.x, otf.translation.y, at.y);
                                                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                            }
                                        }
                                        if bed && d.name == "Nap" {
                                            // The nap clips are played from beside the bed, facing
                                            // across it (the Sim lies down 1.09 m ahead, head to
                                            // the left; on a double bed's far side, all of it the
                                            // other way about).
                                            let way = if far_side { -1.0 } else { 1.0 };
                                            let p = otf.transform_point(Vec3::new(obj.center.x + side - 1.09 * way, 0.0, obj.center.y));
                                            tf.translation = Vec3::new(p.x, otf.translation.y, p.z);
                                            tf.rotation = otf.rotation * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2 * way);
                                        }
                                        if (obj.kind == ObjectKind::Shower || (obj.kind == ObjectKind::ShowerTub && d.name == "Take Shower"))
                                            && let Some((p, f)) = obj.route
                                        {
                                            // In the stall: the game puts the Sim there once in (its
                                            // clips don't walk them in), on the way in from beside
                                            // its door, at the middle, facing that way.
                                            let inside = p + f * (obj.center - p).dot(f).max(0.0);
                                            let w = otf.transform_point(Vec3::new(inside.x, 0.0, inside.y));
                                            tf.translation = Vec3::new(w.x, otf.translation.y, w.z);
                                            tf.rotation = otf.rotation * Quat::from_rotation_y(f.x.atan2(f.y));
                                        }
                                        if obj.kind == ObjectKind::Bathtub || (obj.kind == ObjectKind::ShowerTub && d.name != "Take Shower") {
                                            // Stood in the tub where they'll sit, back to the taps
                                            // (the bath's clips brought round that spot: see
                                            // `anim::clip_anchor`).
                                            let hips = otf.transform_point(Vec3::new(obj.center.x, 0.0, obj.center.y - obj.half.y + 0.5));
                                            tf.translation = Vec3::new(hips.x, otf.translation.y, hips.z);
                                        }
                                        anim.seat_height = obj.seat_height();
                                        debug!("{} on the {}: centre {:?} half {:?}, object at {:?} turned {:?}", d.name, obj.name, obj.center, obj.half, otf.translation, otf.rotation.to_euler(EulerRot::YXZ).0);
                                    } else if let Some((p, f)) = obj.route {
                                        // (At the routing slot, facing its way: the game's clips
                                        // are played from there.)
                                        let w = otf.transform_point(Vec3::new(p.x, 0.0, p.y));
                                        // (Not through a wall or a window to it, nor from far off:
                                        // a slot the walk couldn't reach is used from where it
                                        // ended, not stood in, stranded, behind the glass.)
                                        let here = tf.translation.xz();
                                        let floor_grid = if floor.0 == 1 { Some(my_grid) } else { my_upper.and_then(|u| u.grids.get(&floor.0)) };
                                        let walled = floor_grid.is_some_and(|g| g.crosses(here, w.xz()));
                                        if !walled && here.distance(w.xz()) < 1.5 {
                                            tf.translation = Vec3::new(w.x, tf.translation.y, w.z);
                                        }
                                        tf.rotation = otf.rotation * Quat::from_rotation_y(f.x.atan2(f.y));
                                        anim.seat_height = 0.0;
                                    } else if !making {
                                        tf.rotation = face;
                                        anim.seat_height = 0.0;
                                    }
                                }
                            }
                            ActionKind::Social { target, social } => {
                                let s = crate::social::social(*social);
                                let little = who.get(target).map(|w| w.0.age).filter(|a| a.is_little());
                                // (Talked to where they sit, a friendly talk: they listen sat there,
                                // as the game's seated socials, not pulled up into it.)
                                let listens_sat = !action.seated && little.is_none() && sitting.contains(target) && crate::social::seated_ok(s);
                                // (Seated: they stay as they are, talk played over them below.)
                                if !action.seated {
                                    // (Turned to them; not to one in their own arms.)
                                    if let Some(p) = positions.get(target).filter(|_| !held_q.get(*target).is_ok_and(|c| c.by == me)) {
                                        let to = Vec2::new(p.0.x - tf.translation.x, p.0.z - tf.translation.z);
                                        tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                    }
                                    anim.pose = Pose::Talk;
                                    // (In their arms: the game's own clips for it there.)
                                    let held = held_q.get(*target).is_ok_and(|c| c.by == me);
                                    let clips = social_clips(s.name, little, held);
                                    let start = social_start(s.name, little, held);
                                    let clip = |side| crate::anim::ActionClip { start, ..crate::anim::ActionClip::social(clips, side) };
                                    // (A child in it: the children's own clips.)
                                    let kids = who
                                        .get(target)
                                        .filter(|_| little.is_none())
                                        .zip(baked.as_deref())
                                        .and_then(|(w, b)| child_social_clips(clips, sim.age, w.0.age, s.cat == crate::social::SocialCat::Mean, &b.0.clip_names));
                                    let (mine, theirs) = kids.unwrap_or_else(|| (clip('x'), clip('y')));
                                    commands.entity(me).insert(mine);
                                    if !listens_sat {
                                        commands.entity(*target).try_insert((theirs, SocialPartner(me)));
                                    }
                                    if let Some(age) = little {
                                        carry_fx.push((*target, me, age, *tf, s.effect != SocialEffect::PutToBed));
                                    }
                                }
                                if let Some((tsim, tmood, t_member, tpartner)) = who.get(target) {
                                    let rel = rels.get(*target);
                                    // (On Snowflake Day everyone's warmer, and on Love Day to romance:
                                    // the game's holiday liking and mood, 20, as the rules see them.)
                                    let holiday = weather.as_deref().and_then(|w| crate::holidays::holiday_today(w, clock.day()));
                                    let warm = holiday == Some("Snowflake Day") || (holiday == Some("Love Day") && s.cat == crate::social::SocialCat::Romantic);
                                    let mine: Vec<crate::life::MoodletKind> = moodlets.map(|m| m.0.iter().map(|x| x.kind).collect()).unwrap_or_default();
                                    let repetition = memories.0.entry(me).or_default().before(*social, clock.minutes);
                                    let judging = crate::conversation::Judging {
                                        actor: sim,
                                        target: tsim,
                                        actor_moodlets: &mine,
                                        target_moodlets: &[],
                                        liking: rel.friendship,
                                        relationship: rel.state(),
                                        stc: rel.stc,
                                        target_mood: tmood.0,
                                        charisma: skills.0.get("Charisma").map(|v| *v as u32),
                                        partner_elsewhere: *tpartner && rel.status == RelStatus::None,
                                        partners: matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married),
                                        kin: false,
                                        repetition,
                                        actor_member: who.get(&me).is_some_and(|w| w.2),
                                        target_member: *t_member,
                                        holiday: warm,
                                        boss_hears: job.as_deref().filter(|j| j.boss == Some(tsim.id)).map(|j| j.performance),
                                    };
                                    // (Caught up with again after the other moved off: as it was
                                    // already going.)
                                    let decided = action.social_outcome;
                                    let mut outcome = decided.unwrap_or_else(|| crate::conversation::decide(*social, &judging));
                                    if decided.is_none() {
                                        // (A Comedian's jokes always land; told to do their homework,
                                        // they hear it out, whether they do it being the effect's.)
                                        if (s.name == "Tell Joke" && crate::journal::earned(journals.get(me).ok(), "Comedian")) || s.effect == SocialEffect::RequestHomework {
                                            outcome.accepted = true;
                                        }
                                        debug!("{} to {}: {} comes across {:?} (in {}, liking {:.0})", sim.first, tsim.first, s.name, outcome.result, rel.stc.name(), rel.friendship);
                                    }
                                    action.social_outcome = Some(outcome);
                                    // (Seated: the talker's talk and the listener's, over the top of them;
                                    // to one sat, theirs.)
                                    // (A cuddle on the sofa: the game's seated cuddle, leaning in (or turned
                                    // away), each as they sit to the other.)
                                    // (A kiss, or making out, while cuddling: the game's own, back to the
                                    // cuddle after; turned away, the cuddle's over.)
                                    let cuddling = cuddles.get(me).ok().filter(|c| c.with == *target && c.accepted).copied();
                                    if let (true, Some(c)) = (action.seated && crate::seatedtalk::WHILE_CUDDLING.contains(&s.name), cuddling) {
                                        let make_out = s.name == "Make Out";
                                        // (In bed, turned away, they're still cuddling; on the sofa
                                        // it's over.)
                                        if c.bed {
                                            commands.entity(me).insert(crate::seatedtalk::bed_cuddle_kiss_clip(c.x, outcome.accepted, make_out));
                                            commands.entity(*target).try_insert(crate::seatedtalk::bed_cuddle_kiss_clip(!c.x, outcome.accepted, make_out));
                                        } else {
                                            commands.entity(me).insert(crate::seatedtalk::cuddle_kiss_clip(c.x, c.teen, outcome.accepted, make_out));
                                            commands.entity(*target).try_insert(crate::seatedtalk::cuddle_kiss_clip(!c.x, c.teen, outcome.accepted, make_out));
                                            if !outcome.accepted {
                                                for (who, with, x) in [(me, *target, c.x), (*target, me, !c.x)] {
                                                    commands.entity(who).insert(crate::seatedtalk::Cuddling { with, x, accepted: false, ..c });
                                                }
                                            }
                                        }
                                    } else if action.seated && s.name == crate::seatedtalk::CUDDLE {
                                        // (In bed, each from their side of it; on the sofa, each as
                                        // they sit to the other.)
                                        let teen = sim.age == Age::Teen;
                                        let (x, bed) = match in_bed {
                                            Some(near) => (near, true),
                                            None => (positions.get(target).is_some_and(|p| crate::seatedtalk::cuddles_as_x(&tf, p.0)), false),
                                        };
                                        let accepted = outcome.accepted;
                                        let clip = |x: bool| if bed { crate::seatedtalk::bed_cuddle_clip(x, accepted) } else { crate::seatedtalk::cuddle_clip(x, teen, accepted) };
                                        commands.entity(me).insert((clip(x), crate::seatedtalk::Cuddling { with: *target, x, teen, accepted, bed }));
                                        commands.entity(*target).try_insert((clip(!x), crate::seatedtalk::Cuddling { with: me, x: !x, teen, accepted, bed }));
                                        // (Turned away in bed, the far side lies back as it lay.)
                                        if bed && !accepted {
                                            commands.entity(if x { *target } else { me }).insert(crate::seatedtalk::LieBack);
                                        }
                                    } else if action.seated || listens_sat {
                                        let child = [sim.age == crate::sim::Age::Child, who.get(target).is_some_and(|w| w.0.age == crate::sim::Age::Child)];
                                        let (talk, listen) = crate::seatedtalk::talk_clips(outcome.accepted, child, &mut rand::rng());
                                        if action.seated {
                                            commands.entity(me).insert(crate::seatedtalk::TalkOverlay { clip: talk, time: 0.0, with: *target, talking: true });
                                        }
                                        commands.entity(*target).try_insert(crate::seatedtalk::TalkOverlay { clip: listen, time: 0.0, with: me, talking: false });
                                    }
                                    if crate::conversation::rebuffed(*social, &outcome) {
                                        if !action.autonomous {
                                            notes.push(format!("{} rejected {}'s attempt to {}.", tsim.first, sim.first, s.name.to_lowercase()));
                                        }
                                        // (The rebuff: the conversation turns, and liking with it.)
                                        let (f, stc) = crate::conversation::effect(*social, rel.friendship, rel.stc, &outcome, sim, tsim);
                                        let r = if s.cat == crate::social::SocialCat::Romantic { -8.0 } else { 0.0 };
                                        rels.entry(*target).stc = stc;
                                        if rels.add(*target, f, r) {
                                            life.write(LifeEvent::new(me, LifeEventKind::MadeFriend { other: *target }));
                                        }
                                        social_fx.push((*target, me, 0.0, 0.0, 0.0, r, Some(crate::conversation::Shared::of(&rels.get(*target)))));
                                        memories.0.entry(me).or_default().after(*social, clock.minutes);
                                        life.write(LifeEvent::new(me, LifeEventKind::Socialized { other: *target, social: "Argue" }));
                                        finished = true;
                                    }
                                }
                                // WooHoo and trying for a baby, accepted: off to the nearest free double
                                // bed together (the game's `FindNearestBed`), she on one side and he on
                                // the other; with none, there and then as before.
                                if !finished && matches!(s.effect, SocialEffect::WooHoo | SocialEffect::TryForBaby) {
                                    let partner = *target;
                                    let free = |b: Entity| {
                                        objects.get(b).is_ok_and(|(_, _, used, _)| used.0.is_none_or(|u| u == me || u == partner))
                                            && seconds.get(b).is_ok_and(|s| s.0.is_none_or(|u| u == me || u == partner))
                                            && !unavailable.contains(b)
                                    };
                                    let bed = double_beds
                                        .iter()
                                        .filter(|b| objects.get(*b).is_ok_and(|(o, ..)| o.kind == ObjectKind::BedDouble))
                                        .filter(|b| free(*b))
                                        .filter_map(|b| objects.get(b).ok().map(|(_, btf, ..)| (b, btf.translation.distance(tf.translation))))
                                        .filter(|(_, d)| *d < 40.0)
                                        .min_by(|a, b| a.1.total_cmp(&b.1))
                                        .map(|(b, _)| b);
                                    let special = if s.effect == SocialEffect::WooHoo { Special::WooHoo } else { Special::TryForBaby };
                                    if let Some(bed) = bed
                                        && let Some(def) = interactions_for(ObjectKind::BedDouble).iter().position(|d| d.special == special)
                                    {
                                        let label = interactions_for(ObjectKind::BedDouble)[def].name;
                                        commands.entity(me).remove::<crate::anim::ActionClip>();
                                        commands.entity(partner).remove::<(crate::anim::ActionClip, SocialPartner)>();
                                        anim.pose = Pose::Stand;
                                        action.kind = ActionKind::Object { target: bed, def };
                                        action.phase = Phase::Start;
                                        action.label = label.to_string();
                                        let autonomous = action.autonomous;
                                        commands.entity(partner).queue_silenced(move |mut e: EntityWorldMut| {
                                            if let Some(mut q) = e.get_mut::<ActionQueue>() {
                                                q.0.push_front(Action::new(label, ActionKind::Object { target: bed, def }, autonomous));
                                            }
                                        });
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
                            ActionKind::Extinguish { at, .. } => {
                                let to = at.xz() - Vec2::new(tf.translation.x, tf.translation.z);
                                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                anim.pose = Pose::Use;
                                commands.entity(me).insert(crate::fire::extinguisher_clip());
                            }
                            ActionKind::Carry { child, step, .. } => {
                                commands.entity(me).insert(crate::carry::CarryMove::new(*child, *step));
                            }
                            ActionKind::PetSocial { at, social, kind, .. } => {
                                let to = *at - Vec2::new(tf.translation.x, tf.translation.z);
                                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                anim.pose = Pose::Talk;
                                let s = &crate::pets::PET_SOCIALS[*social];
                                let child = sim.age == Age::Child;
                                match baked.as_deref().and_then(|b| s.clip_for(b, child, kind, 'x')) {
                                    Some(c) => {
                                        // (Turned down by a pet that dislikes them: the pack's
                                        // rejection, both sides, as its temperament has it.)
                                        let rejections: Vec<(&'static str, Option<&'static str>)> = crate::pettraits::REJECT_CLIPS.iter().map(|n| (*n, baked.as_deref().and_then(|b| crate::pets::social_clip(b, child, kind, n, 'x')))).collect();
                                        // (A set in parts: its start, loops and end.)
                                        let suite = baked.as_deref().and_then(|b| s.suite(b, child, kind));
                                        let (target, id, roll) = (match &action.kind {
                                            ActionKind::PetSocial { target, .. } => *target,
                                            _ => Entity::PLACEHOLDER,
                                        }, sim.id, rand::random::<f32>());
                                        commands.queue(move |w: &mut World| {
                                            let home = w.get::<crate::pets::HomePet>(target).is_some();
                                            let liking = w.get::<crate::pets::PetBonds>(target).map_or(if home { crate::pets::FAMILY_LIKING } else { 0.0 }, |b| b.liking(id, home));
                                            let how = w.get::<crate::pets::PetTraits>(target).map_or(crate::pets::REJECT_CLIP, |t| crate::pettraits::reject_clip(&t.0));
                                            let reject = rejections.iter().find(|r| r.0 == how && r.1.is_some()).or_else(|| rejections.iter().find(|r| r.0 == crate::pets::REJECT_CLIP)).and_then(|r| r.1.map(|c| (r.0, c)));
                                            let turned_down = reject.filter(|_| crate::pets::rejects(liking, roll));
                                            let clip = turned_down.map_or(c, |r| r.1);
                                            let clips: &'static [&'static str] = Box::leak(Box::new([clip]));
                                            if let Ok(mut e) = w.get_entity_mut(me) {
                                                match suite.filter(|_| turned_down.is_none()) {
                                                    Some(suite) => e.insert(suite),
                                                    None => e.insert(crate::anim::ActionClip::new(None, clips)),
                                                };
                                                if let Some((how, _)) = turned_down {
                                                    e.insert(crate::pets::Rejected(how));
                                                }
                                            }
                                        });
                                    }
                                    None => finished = true,
                                }
                            }
                            ActionKind::Outdoor { at, what, .. } if *what == crate::seasonal::Outdoor::Contemplate => {
                                // (Turned to something of worth about them, at random, and thinking
                                // of it: the game's `Contemplate`.)
                                let here = tf.translation;
                                let things = objects.iter().filter(|(_, _, _, of)| of.map_or(1, |f| f.0) == floor.0).map(|(o, otf, _, _)| (o.world_center(otf), o.price, o.objd));
                                match crate::traitacts::contemplated(here, things, &mut rand::rng()) {
                                    Some((p, objd)) => {
                                        let to = p.xz() - here.xz();
                                        if to.length() > 0.2 {
                                            tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                        }
                                        commands.entity(me).insert(crate::balloons::BalloonRequest::thought(&s3bake::gamedata::thumb_name(objd.2)));
                                        anim.pose = Pose::Use;
                                        commands.entity(me).insert(what.clip());
                                    }
                                    // (Nothing there after all: bored.)
                                    None => {
                                        let _ = at;
                                        commands.entity(me).insert(crate::reactions::Pending::new(&["Bored"], None, clock.minutes + 10.0));
                                        finished = true;
                                    }
                                }
                            }
                            ActionKind::Outdoor { at, what, .. } => {
                                // (Facing the snowman being built; facing away, to lie back in the snow.)
                                let to = *at - Vec2::new(tf.translation.x, tf.translation.z);
                                if to.length() > 0.2 {
                                    tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                }
                                anim.pose = Pose::Use;
                                commands.entity(me).insert(what.clip());
                                if *what == crate::seasonal::Outdoor::Snowman {
                                    let y = ground(floor.0, at.x, at.y);
                                    commands.entity(me).insert(crate::seasonal::OutdoorEvent::Begun { what: *what, at: Vec3::new(at.x, y, at.y), minutes: what.minutes() });
                                }
                            }
                            ActionKind::Invite { .. }
                            | ActionKind::PhoneChat { .. }
                            | ActionKind::PrankCall { .. }
                            | ActionKind::CheckUpOnWork
                            | ActionKind::CheckRealEstate
                            | ActionKind::CallServices
                            | ActionKind::CallPolice
                            | ActionKind::CallFirefighters
                            | ActionKind::CallEverybodyHome
                            | ActionKind::InviteOut { .. }
                            | ActionKind::TimeOff(_)
                            | ActionKind::QuitJobCall
                            | ActionKind::SilencePhone
                            | ActionKind::Retire
                            | ActionKind::BuyReward(_)
                            | ActionKind::WakeUp { .. }
                            | ActionKind::InventoryItem(..)
                            | ActionKind::Greyed(_)
                            | ActionKind::Notice(_)
                            | ActionKind::EatHere
                            | ActionKind::EatItem { .. }
                            | ActionKind::Outro { .. }
                            | ActionKind::OrderPizza
                            | ActionKind::Adopt { .. }
                            | ActionKind::AdoptPet { .. }
                            | ActionKind::MoveHouse
                            | ActionKind::CallRepairman
                            | ActionKind::HireMaid(_)
                            | ActionKind::MotiveFail(_)
                            | ActionKind::TraitAct { .. }
                            | ActionKind::ReadJournal
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
                                        .or_else(|| (*activity == crate::little::BIRTH_TASK).then_some(&crate::little::HAVE_BABY))
                                        .or_else(|| crate::meals::recipe_task(*activity))
                                        .or_else(|| crate::classes::class_task(*activity))
                                        .or_else(|| (crate::skillbooks::BOOK_TASK..crate::skillbooks::BOOK_TASK + 1000).contains(activity).then_some(&crate::skillbooks::BUY_BOOK))
                                        .or_else(|| crate::venues::task(*activity))
                                        .or_else(|| crate::careerholes::Errand::of(*activity).map(|e| e.activity()))
                                        .or_else(|| crate::realestate::Deal::of(*activity).map(|d| d.activity()))
                                        .or_else(|| crate::careermetrics::Gig::of(*activity).map(|g| g.activity()))
                                        .or_else(|| crate::careermetrics::review_task(*activity));
                                    if let Some(r) = crate::careermetrics::Reviewing::new(*activity, *lot) {
                                        commands.entity(me).insert(r);
                                    }
                                    if let Some(g) = crate::careermetrics::Gig::of(*activity) {
                                        commands.entity(me).insert(g);
                                    }
                                    if let Some(e) = crate::careerholes::Errand::of(*activity) {
                                        commands.entity(me).insert(e);
                                    }
                                    if let Some(d) = crate::realestate::Deal::of(*activity) {
                                        commands.entity(me).insert(d);
                                    }
                                    if crate::meals::recipe_task(*activity).is_some() {
                                        commands.entity(me).insert(crate::meals::BuyingRecipe(*activity - crate::meals::RECIPE_TASK));
                                    }
                                    if (crate::skillbooks::BOOK_TASK..crate::skillbooks::BOOK_TASK + 1000).contains(activity) {
                                        commands.entity(me).insert(crate::skillbooks::BuyingBook(*activity - crate::skillbooks::BOOK_TASK));
                                    }
                                    if let Some(a) = task.or_else(|| acts.get(*activity)) {
                                        let place = crate::rabbitholes::lot_title(l, name);
                                        let mut price = crate::wishes::price_factor(wishes, a.name);
                                        // (The day's discounted class, a fifth off.)
                                        if let Some(i) = activity.checked_sub(crate::classes::CLASS_TASK).filter(|i| *i < crate::classes::CLASSES.len()) {
                                            price *= crate::classes::price(i, clock.day()) as f32 / a.cost.max(1) as f32;
                                        }
                                        // (A meal out's price by the hour: dinner dearer, early birds less.)
                                        if let Some(i) = activity.checked_sub(crate::venues::VENUE_TASK).filter(|i| *i < crate::venues::ACTS.len()) {
                                            price *= crate::venues::meal_price(i, clock.hour_f()) as f32 / a.cost.max(1) as f32;
                                        }
                                        // (On the house where the household has a stake: `realestate`.)
                                        if real_estate.free_meal(*lot, *activity) {
                                            price = 0.0;
                                        }
                                        // (A reviewer sees it free: `careermetrics`.)
                                        if crate::careermetrics::review_task(*activity).is_some() {
                                            price = 0.0;
                                        }
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
                            ActionKind::GoToSchool => {
                                crate::rabbitholes::walk_to_school(&mut commands, &clock, me, sim, &mut notes);
                                finished = true;
                            }
                            // (Up on the horse: the ride takes it from here.)
                            ActionKind::Ride { horse, unsaddle, .. } => {
                                commands.entity(me).insert(crate::riding::Mounted::new(*horse, *unsaddle));
                            }
                            ActionKind::Dismount => finished = true,
                            // (Before the dog: `dogtricks` plays it from here.)
                            ActionKind::DogTrick { at, .. } => {
                                let to = *at - Vec2::new(tf.translation.x, tf.translation.z);
                                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                anim.pose = Pose::Use;
                            }
                            // (Off along the street: the jog takes it from here.)
                            ActionKind::Jog { home } => {
                                commands.entity(me).insert(crate::jog::Jogging::new(*home, clock.minutes));
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
                            if let Ok((obj, otf, mut used, _)) = objects.get_mut(*target) {
                                let d = &interactions_for(obj.kind)[*def];
                                let affinity = crate::life::activity_affinity(&sim.traits, d.name);
                                // (As the game tunes this object: a cheap TV less fun than a
                                // flatscreen.)
                                let per_hour = object_rates.per_hour(d, obj.objd);
                                for i in 0..6 {
                                    let mut gain = per_hour[i];
                                    // (Making or fetching a meal isn't eating it: what it does for hunger is
                                    // what draws them to it, and the eating does it.)
                                    if i == HUNGER && matches!(d.special, Special::ServeMeal | Special::GrabPlate | Special::Leftovers | Special::Cook | Special::Microwave | Special::ReplicateFood) {
                                        gain = 0.0;
                                    }
                                    // (No fun in technology for a Technophobe, the game's zeroed
                                    // commodity update; and the water's less than none for the
                                    // Hydrophobic, `kSwimmingInPoolFunAmount` the other way.)
                                    if i == FUN && gain > 0.0 && sim.traits.contains(&crate::life::Trait::Technophobe) && (matches!(obj.kind, ObjectKind::Tv | ObjectKind::Computer) || d.name == "Watch the TV") {
                                        gain = 0.0;
                                    }
                                    if i == FUN && d.name == "Swim" && sim.traits.contains(&crate::life::Trait::Hydrophobic) {
                                        gain = -20.0;
                                    }
                                    if i == FUN && gain > 0.0 {
                                        gain *= affinity;
                                    } else if i == FUN && affinity < 0.5 {
                                        gain -= 20.0;
                                    }
                                    // TraitTuning.kWorkaholicTraitHomeworkFunPerHour.
                                    if i == FUN && d.special == Special::Homework && sim.traits.contains(&crate::life::Trait::Workaholic) {
                                        gain = 20.0;
                                    }
                                    if i == ENERGY && d.until_full == Some(ENERGY) {
                                        gain *= crate::life::sleep_rate(&sim.traits);
                                    }
                                    // (A Fitness Nut's cardio doesn't tire them.)
                                    if i == ENERGY && gain < 0.0 && d.pose == Pose::Exercise && crate::journal::earned(journals.get(me).ok(), "Fitness Nut") {
                                        gain = 0.0;
                                    }
                                    gain = crate::upgrades::boost(obj.kind, upgraded.get(*target).ok(), i, gain);
                                    motives.add(i, gain * dt / 60.0);
                                }
                                // Working out builds fitness and burns off weight.
                                // (Kept in their skill journal.)
                                match (d.pose, d.name) {
                                    (Pose::Exercise, _) => {
                                        did.write(crate::journal::Did::count(me, crate::journal::Stat::CardioHours, dt as f64 / 60.0));
                                    }
                                    (_, "Play Guitar") => {
                                        did.write(crate::journal::Did::count(me, crate::journal::Stat::GuitarHours, dt as f64 / 60.0));
                                    }
                                    _ => {}
                                }
                                if d.pose == Pose::Exercise {
                                    let h = dt / 60.0;
                                    // (Faster for a Fast Metabolism.)
                                    let burn = if crate::wishes::has(wishes, "FastMetabolism") { 1.25 } else { 1.0 };
                                    commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| crate::aging::reshape(&mut e, -0.03 * h * burn, 0.05 * h));
                                }
                                if let Some(sk) = d.skill {
                                    let computer_chess = if obj.kind == ObjectKind::Computer && d.name == "Play Chess" && sim.traits.contains(&crate::life::Trait::ComputerWhiz) { 1.1 } else { 1.0 };
                                    let rate = crate::life::skill_rate(&sim.traits, sk) * crate::wishes::reward_skill_rate(wishes) * mood_learning * computer_chess;
                                    let points = if d.special == Special::Perform { PERFORM_POINTS } else { practice_points(d.name) };
                                    if let Some(level) = skills.add_points(sk, dt / 60.0 * points * rate) {
                                        notes.push(format!("{} reached level {level} in {sk}!", sim.first));
                                        life.write(LifeEvent::new(me, LifeEventKind::SkillUp { skill: sk, level }));
                                    }
                                    // (Hours of games played, a game developer's: `careermetrics`.)
                                    if sk == "ArcadeMachine" {
                                        let h = dt / 60.0;
                                        commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| {
                                            if let Some(mut j) = e.get_mut::<crate::careers::Job>() {
                                                j.tally.game_hours += h;
                                            }
                                        });
                                    }
                                }
                                let full = d.until_full.is_some_and(|m| motives.0[m] >= 98.0);
                                // (A painting takes as long as its canvas.)
                                let minutes = match d.special {
                                    Special::SellPainting => crate::paintings::painting_minutes(crate::paintings::canvas(plan, skills.level("Painting")), journals.get(me).ok()),
                                    // (Only taking the book down, or picking the paper up: they're read elsewhere.)
                                    Special::GetBook | Special::GetPaper => 2.0,
                                    // (A quick meal is only taken out at the fridge: it's eaten at the table.)
                                    Special::Cook if d.name == "Have Quick Meal" => 6.0,
                                    // (Only turning it on: it's watched from a seat.)
                                    Special::TurnOnTv => 1.0,
                                    // (Less time at the stove once the food's been prepared.)
                                    Special::ServeMeal if prepped.contains(me) && d.minutes > 30.0 => d.minutes - 15.0,
                                    // Original reward and trait completion rates.
                                    Special::CleanUp | Special::EmptyTrash if crate::wishes::has(wishes, "SpeedyCleaner") => d.minutes / 3.0,
                                    Special::Homework => crate::rabbitholes::homework_minutes(&sim.traits, crate::wishes::has(wishes, "MultiTasker")),
                                    Special::SmashDollhouse => crate::dollhouse::smash_minutes(sim.age),
                                    _ => d.minutes,
                                };
                                let complete = if d.special == Special::WriteNovel {
                                    action.completed
                                } else if d.special == Special::Perform {
                                    // (Till the song's played through.)
                                    action.completed || elapsed >= minutes
                                } else if cuddles.get(me).is_ok_and(|c| c.accepted) {
                                    // (Cuddling there: not up till the cuddle's over.)
                                    false
                                } else if d.special == Special::Homework {
                                    homework.as_mut().is_none_or(|assignment| assignment.advance(dt, minutes))
                                } else {
                                    elapsed >= minutes || full
                                };
                                if complete {
                                    finished = true;
                                    life.write(LifeEvent::new(me, LifeEventKind::Finished { activity: d.name, completed: true }));
                                    // (Down the slide: a fall smarts; and back for another run while it's fun.)
                                    if d.special == Special::SlippySlide {
                                        let (t, def) = (*target, *def);
                                        let again = motives.0[FUN] < 95.0;
                                        commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| {
                                            if e.take::<SlideFall>().is_some() {
                                                let now = e.world().resource::<GameClock>().minutes;
                                                if let Some(mut m) = e.get_mut::<crate::life::Moodlets>() {
                                                    m.add(crate::life::MoodletKind::Slipped, now);
                                                }
                                            }
                                            if again && let Some(mut q) = e.get_mut::<ActionQueue>().filter(|q| q.0.len() <= 1) {
                                                q.0.push_back(Action::new("Slide", ActionKind::Object { target: t, def }, true));
                                            }
                                        });
                                    }
                                    // (A bathroom thing used is that much dirtier.)
                                    if crate::grime::cleanable(obj.kind) && !matches!(obj.kind, ObjectKind::Terrarium | ObjectKind::BirdCage) && d.special != Special::Scrub && !upgraded.get(*target).is_ok_and(|u| u.has(crate::upgrades::Upgrade::SelfCleaning)) {
                                        let traits = sim.traits.clone();
                                        commands.entity(*target).queue_silenced(move |mut e: EntityWorldMut| {
                                            if let Some(mut g) = e.get_mut::<crate::grime::Grime>() {
                                                g.used(&traits);
                                            }
                                        });
                                    }
                                    // (WooHoo's done: as the social's, with whoever's on the bed's other side;
                                    // a baby tried for once, by the first side's.)
                                    if matches!(d.special, Special::WooHoo | Special::TryForBaby) {
                                        let second = seconds.get(*target).ok().and_then(|s| s.0);
                                        let other = if used.0 == Some(me) { second } else { used.0 };
                                        if let Some(other) = other.filter(|o| *o != me) {
                                            if rels.add(other, 6.0, 15.0) {
                                                life.write(LifeEvent::new(me, LifeEventKind::MadeFriend { other }));
                                            }
                                            life.write(LifeEvent::new(me, LifeEventKind::Socialized { other, social: d.name }));
                                            // (The game's `kWooHooed`.)
                                            commands.queue(move |w: &mut World| {
                                                w.write_message(crate::dreams::DreamEvent { other: Some(other), ..crate::dreams::DreamEvent::new(me, "kWooHooed") });
                                            });
                                            if d.special == Special::TryForBaby && used.0 == Some(me) {
                                                conceive.write(crate::little::Conceive { a: me, b: other });
                                            }
                                        }
                                    }
                                    if used.0 == Some(me) {
                                        used.0 = None;
                                    }
                                    let unbreakable = upgraded.get(*target).is_ok_and(|u| u.has(crate::upgrades::Upgrade::Unbreakable));
                                    // (Not within a day of being new or repaired.)
                                    let worn = wear.get(*target).ok().copied().filter(|w| clock.minutes >= w.sturdy_until);
                                    if !unbreakable
                                        && let Some(w) = worn
                                        && rand::rng().random_bool((break_chance_tuned(tunings.get(obj.objd), obj.kind, obj.price, w.breaks) * crate::traitmoods::luck(moodlets, crate::traitmoods::BREAK_LUCK.0, crate::traitmoods::BREAK_LUCK.1)).min(1.0))
                                    {
                                        if let Ok(mut w) = wear.get_mut(*target) {
                                            w.breaks += 1;
                                        }
                                        commands.entity(*target).try_insert(Broken);
                                        let what = if obj.kind == ObjectKind::Toilet { "is clogged" } else { "broke" };
                                        notes.push(format!("Oh no! {} {what}. Repair it, or call the repairman.", upper_first(&the(&obj.name))));
                                    }
                                    if d.on_object {
                                        stand_up_at = Some(action.approach.unwrap_or_else(|| obj.use_point(otf)));
                                    }
                                    // (Its ending, played before they're up and away.)
                                    // (Then up out of the seat.)
                                    let up = (d.on_object && d.pose == Pose::Sit).then(|| seat_clips(obj.kind)).flatten().map(|(_, out, secs)| (out, secs));
                                    // (On a double bed's far side: mirrored, and WooHoo's other part.)
                                    let far_side = obj.kind == ObjectKind::BedDouble && seconds.get(*target).is_ok_and(|s| s.0 == Some(me));
                                    let ending = if far_side && matches!(d.special, Special::WooHoo | Special::TryForBaby) {
                                        Some(woohoo_clip(true))
                                    } else if d.special == Special::ViewArt {
                                        Some(view_clip(sim, art_taste(sim, obj.price, taste_roll(me, *target))))
                                    } else {
                                        interaction_clip(d.name, obj.kind)
                                    };
                                    outro = match (ending.filter(|c| !c.end.is_empty()), up) {
                                        (Some(c), up) => Some((c.end, up.map(|u| u.0), c.end_secs + up.map_or(0.0, |u| u.1), *target, far_side)),
                                        (None, Some((out, secs))) => Some((&[], Some(out), secs, *target, far_side)),
                                        (None, None) => None,
                                    };
                                    // (A child's last blow and getting up take longer: their own clips.)
                                    if d.special == Special::SmashDollhouse && sim.age == Age::Child {
                                        if let Some(o) = outro.as_mut() {
                                            o.2 = crate::dollhouse::CHILD_END_SECS;
                                        }
                                    }
                                    match d.special {
                                        // (Getting out of the pool is the swim module's.)
                                        Special::FindJob => {
                                            // (The day's openings, in the game's listings.)
                                            commands.insert_resource(crate::jobpaper::JobHunt { sim: me, computer: obj.kind == ObjectKind::Computer });
                                        }
                                        Special::Swim
                                        | Special::WriteNovel
                                        | Special::PlayInSprinkler
                                        | Special::Teleport
                                        | Special::WashDishes
                                        | Special::DropTrash
                                        | Special::ReadBook
                                        | Special::WatchTv
                                        | Special::WooHoo
                                        | Special::TryForBaby
                                        | Special::ViewArt
                                        | Special::PlayChess
                                        | Special::WatchChess
                                        | Special::PlayForTips
                                        | Special::WatchMusic
                                        | Special::DanceToMusic
                                        | Special::Perform
                                        | Special::Daydream
                                        | Special::GrabDrink
                                        | Special::HighChairSit
                                        | Special::PutInHighChair
                                        | Special::GiveBabyFood
                                        | Special::TakeOutOfHighChair
                                        | Special::PottyTrain
                                        | Special::PottyLesson
                                        | Special::EmptyPotty
                                        | Special::LockDoor
                                        | Special::LightSwitch
                                        | Special::TalkToPlant
                                        | Special::MakeAllHappy
                                        | Special::CardioWorkOut
                                        | Special::StrengthWorkOut
                                        | Special::TvTurnOn
                                        | Special::TvTurnOff => {}
                                        Special::SpongeBath => {}
                                        Special::Donate => {
                                            if let Some((cause, amount)) = action.donation.clone() {
                                                commands.entity(me).insert(crate::charity::Donated { cause, amount });
                                            }
                                        }
                                        Special::PostAway => {
                                            commands.entity(me).insert(crate::rgmmail::PostedNow);
                                        }
                                        Special::PokeFire => {
                                            commands.entity(*target).try_insert(crate::fireplace::Raging(clock.minutes + crate::fireplace::RAGING_MINUTES));
                                        }
                                        // (Chatting's done as it goes: `chatonline`.)
                                        Special::ChatOnline => {}
                                        // (At a mirror: what it leaves them feeling.)
                                        Special::AdmireSelf | Special::GussyUp | Special::PlayWithMirror | Special::CheckSelfOut => {
                                            let now = clock.minutes;
                                            commands.queue(move |w: &mut World| {
                                                w.resource_mut::<crate::mirrors::LastLooked>().0.insert(me, now);
                                            });
                                            if let Some(kind) = crate::mirrors::moodlet(d.special) {
                                                let now = clock.minutes;
                                                commands.entity(me).queue_silenced(move |mut w: EntityWorldMut| {
                                                    if let Some(mut m) = w.get_mut::<crate::life::Moodlets>() {
                                                        m.add(kind, now);
                                                    }
                                                });
                                            }
                                        }
                                        Special::Engrave => {
                                            commands.entity(me).insert(crate::ghosts::EngraveAt(*target));
                                        }
                                        Special::ReadEpitaph => {
                                            commands.entity(me).insert(crate::ghosts::ReadEpitaphAt(*target));
                                        }
                                        Special::GetBook => {
                                            commands.entity(me).insert(crate::surroundings::ReadSomewhere(*target));
                                            // (A skill book taken down: read for its skill.)
                                            if let Some(b) = action.book_choice {
                                                // (Or a book for its own sake.)
                                                match b.checked_sub(crate::novels::NOVEL_CHOICE) {
                                                    Some(n) => commands.entity(me).insert(crate::novels::ReadingNovel(n as usize)),
                                                    None => commands.entity(me).insert(crate::skillbooks::ReadingBook(b as usize)),
                                                };
                                            }
                                        }
                                        Special::MakeBed => {
                                            commands.entity(*target).remove::<Unmade>();
                                        }
                                        Special::MopUp => {
                                            commands.entity(*target).try_despawn();
                                        }
                                        Special::Scrub => {
                                            commands.entity(*target).try_insert(crate::grime::Grime(0.0));
                                        }
                                        Special::StockTerrarium => {
                                            if let Some((key, _)) = action.fish_choice.take() {
                                                commands.entity(me).insert(crate::terrarium::TerrariumRequest::Stock(*target, key));
                                            }
                                        }
                                        Special::FillTerrarium => {
                                            commands.entity(me).insert(crate::terrarium::TerrariumRequest::Fill(*target));
                                        }
                                        Special::WatchMinorPet | Special::SlippySlide => {}
                                        // (Talked with, or taught: what it's learned, the Sim's charisma, the
                                        // two fonder: `terrarium`.)
                                        Special::TalkWithBird | Special::TeachBirdToTalk => {
                                            let speech = action.fish_choice.take().map(|(k, _)| k);
                                            commands.entity(me).insert(crate::terrarium::TerrariumRequest::Talk(*target, speech));
                                        }
                                        Special::MinorPetInfo => {
                                            let t = *target;
                                            commands.queue(move |w: &mut World| {
                                                let (Some(p), Some(ui)) = (w.get::<crate::terrarium::TerrariumPet>(t).cloned(), w.get_resource::<crate::icons::GameUi>()) else { return };
                                                let now = w.resource::<crate::clock::GameClock>().minutes;
                                                let text = crate::terrarium::info(&p, ui, now);
                                                w.resource_mut::<Notifications>().push(text);
                                            });
                                        }
                                        Special::MournMinorPet => {
                                            commands.entity(me).insert(crate::terrarium::TerrariumRequest::TakeOut(*target));
                                        }
                                        Special::FillPetBowl => {
                                            commands.entity(*target).insert(crate::petneeds::BowlFilled);
                                        }
                                        Special::GetIngredients => {
                                            commands.entity(me).insert(crate::anim::Carrying(crate::meals::FOOD_CARRY));
                                        }
                                        Special::PrepFood => {
                                            commands.entity(me).insert(crate::anim::Carrying(crate::meals::PAN_CARRY));
                                        }
                                        Special::PlaceMeal => {
                                            if let Ok(s) = serving_from.get(me) {
                                                commands.entity(me).insert(crate::meals::MealRequest::Serve(s.0));
                                            }
                                        }
                                        Special::GetPaper => {
                                            commands.entity(*target).try_insert(Visibility::Hidden);
                                            commands.entity(me).insert(crate::surroundings::PaperInHand(*target));
                                        }
                                        Special::Homework => {
                                            commands.queue(move |world: &mut World| {
                                                crate::rabbitholes::complete_homework(world, me, None, 0.0);
                                            });
                                        }
                                        Special::Hack | Special::WorkFromHome => {}
                                        Special::Overclock => {
                                            commands.entity(me).insert(crate::computertraits::Overclocked(*target));
                                        }
                                        Special::SolveProblems | Special::JobIssues | Special::JoinBookClub | Special::LeaveBookClub => {
                                            commands.entity(me).insert(crate::computertraits::Done(d.special));
                                        }
                                        Special::Sabotage => {
                                            commands.entity(me).insert(crate::sabotage::Sabotaged(*target));
                                        }
                                        // (Written up as they type: `careermetrics::write_on`.)
                                        Special::WriteReport => {}
                                        Special::CheckRealEstate => {
                                            commands.queue(move |w: &mut World| w.resource_mut::<crate::estatedialog::EstateDialog>().open(me));
                                        }
                                        Special::SearchGalaxy => {}
                                        Special::NeuroticCheck => {
                                            if let Some(about) = crate::neurotic::About::of(obj.kind) {
                                                commands.entity(me).insert(crate::neurotic::Checked(about));
                                            }
                                        }
                                        // (Flushed after, or not; hands washed after: `toilets`.)
                                        Special::UseToilet => {
                                            commands.entity(me).insert(crate::toilets::UsedToilet { toilet: *target, autonomous: action.autonomous });
                                        }
                                        Special::Flush => {
                                            commands.entity(me).insert(crate::toilets::Flushed(*target));
                                        }
                                        Special::Vomit => {
                                            commands.entity(me).insert(crate::nausea::ThrewUp(*target));
                                        }
                                        // (The book's on its shelf: the carry's let go of, `surroundings`.)
                                        Special::ReturnBook => {}
                                        Special::QuitJob => {
                                            if job.is_some() {
                                                commands.entity(me).remove::<Job>();
                                                notes.push(format!("{} quit their job.", sim.first));
                                            }
                                        }
                                        // (Its result's the game's at the table: see `chess::play_chess`.)
                                        Special::ChessMatch => {}
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
                                                    crate::journal::earned(journals.get(me).ok(), "Proficient Painter"),
                                                    &mut rand::rng(),
                                                )
                                            });
                                            commands.entity(me).remove::<crate::paintings::PaintPlan>();
                                            // (Off the easel: done.)
                                            commands.entity(*target).remove::<crate::paintings::LeftOnEasel>();
                                            let p = crate::paintings::finish_value(p, size, journals.get(me).ok(), crate::wishes::has(wishes, "ExtraCreative"));
                                            did.write(crate::journal::Did::count(me, crate::journal::Stat::Paintings, 1.0));
                                            did.write(crate::journal::Did::count(me, crate::paintings::canvas_stat(size), 1.0));
                                            // (For the game's `kFinishedPainting`: its worth and quality.)
                                            let grade = match p.name {
                                                "Brilliant Painting" => 1,
                                                "Masterpiece" => 2,
                                                _ => 0,
                                            };
                                            did.write(crate::journal::Did { sim: me, what: crate::journal::Deed::Painted(p.worth, grade) });
                                            match p.name {
                                                "Brilliant Painting" => {
                                                    did.write(crate::journal::Did::count(me, crate::journal::Stat::BrilliantPaintings, 1.0));
                                                }
                                                "Masterpiece" => {
                                                    did.write(crate::journal::Did::count(me, crate::journal::Stat::Masterpieces, 1.0));
                                                }
                                                _ => {}
                                            }
                                            // Into their inventory, to sell, keep or hang.
                                            crate::inventory::give(&mut commands, me, crate::inventory::ItemKind::Painting, p.key, p.name.to_string(), 0, p.worth, 1);
                                            notes.push(format!("{} finished a painting ({}, worth §{}). It's in their inventory.", sim.first, p.name.to_lowercase(), p.worth));
                                        }
                                        Special::ChangeClothes => {
                                            // Into the outfit chosen, with a spin, kept on until it's time for another.
                                            if let Some(kind) = action.outfit_choice.take() {
                                                let to = (kind != crate::simbody::OutfitKind::Everyday).then_some(kind);
                                                if crate::dressing::spins(sim) {
                                                    crate::dressing::start(&mut commands, me, to, true);
                                                } else {
                                                    crate::dressing::put_on(&mut commands.entity(me), to, true);
                                                }
                                            }
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
                                        // (Nothing to pay for a snack that stuck.)
                                        Special::VendStuck => {}
                                        // (Shaken loose, for free, or the machine toppling onto them.)
                                        Special::VendShake => {
                                            let (free, loose, topple) = if d.name == "Slam Machine" { SLAM_ODDS } else { SHAKE_ODDS };
                                            let (machine, first) = (*target, sim.first.clone());
                                            commands.queue(move |w: &mut World| {
                                                let mut rng = rand::rng();
                                                let stuck = w.get::<SnackStuck>(machine).is_some();
                                                let pregnant = w.get::<crate::little::Pregnancy>(me).is_some();
                                                if !pregnant && rng.random_bool(topple) {
                                                    w.entity_mut(me).insert(crate::death::Dying::crushed());
                                                } else if rng.random_bool(if stuck { loose } else { free }) {
                                                    w.entity_mut(machine).remove::<SnackStuck>();
                                                    if let Some(mut m) = w.get_mut::<Motives>(me) {
                                                        m.add(HUNGER, 5.0);
                                                    }
                                                    w.resource_mut::<Notifications>().push(format!("{first} got a snack out of the machine."));
                                                }
                                            });
                                        }
                                        // (Paid for away from home.)
                                        Special::Vend => {
                                            commands.queue(move |w: &mut World| {
                                                if w.get::<crate::visit::OnLot>(me).is_some() && w.get::<HouseholdMember>(me).is_some()
                                                    && let Some(mut h) = w.get_resource_mut::<Household>()
                                                {
                                                    h.funds -= VEND_PRICE;
                                                }
                                            });
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
                                            // (Whether it burns, or sets the stove alight, is the meal's.)
                                            if let Some(recipe) = action.recipe_choice.take() {
                                                commands.entity(me).insert(crate::meals::MealPlan(recipe));
                                            } else {
                                                commands.entity(me).remove::<crate::meals::MealPlan>();
                                            }
                                            if action.for_one {
                                                commands.entity(me).insert(crate::meals::ForOne);
                                            } else {
                                                commands.entity(me).remove::<crate::meals::ForOne>();
                                            }
                                            commands.entity(me).insert(crate::meals::MealRequest::Serve(*target));
                                        }
                                        Special::GrabPlate => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Grabbed(*target));
                                        }
                                        Special::SprinklerOn => {
                                            commands.entity(*target).try_insert(crate::gardening::Sprinkling { until: clock.minutes + 120.0 });
                                        }
                                        Special::SprinklerOff => {
                                            commands.entity(*target).remove::<crate::gardening::Sprinkling>();
                                        }
                                        Special::PutAway => {
                                            commands.entity(me).insert(crate::meals::MealRequest::PutAway(*target));
                                        }
                                        Special::CleanOutFridge => {
                                            // (What's gone off thrown out, and the bag taken out to the trash.)
                                            let now = clock.minutes;
                                            commands.queue(move |w: &mut World| {
                                                let keeps = w.resource::<crate::meals::FridgeKeeps>().0;
                                                let n = w.resource_mut::<crate::meals::Leftovers>().clean_out(now, keeps);
                                                info!("cleaned {n} spoiled servings out of the fridge");
                                            });
                                            commands.entity(me).insert(crate::surroundings::TakeOutTrash(*target));
                                        }
                                        Special::Leftovers => {
                                            // (The dish chosen, else the oldest.)
                                            match action.recipe_choice.take() {
                                                Some(r) => commands.entity(me).insert(crate::meals::MealPlan(r)),
                                                None => commands.entity(me).remove::<crate::meals::MealPlan>(),
                                            };
                                            commands.entity(me).insert(crate::meals::MealRequest::FromFridge);
                                        }
                                        Special::BakeCake => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Cake(*target));
                                        }
                                        Special::BlowOutCandles => {
                                            commands.entity(me).insert(crate::aging::GrowUpNow);
                                            commands.entity(*target).try_insert(crate::meals::CakeBlownOut);
                                        }
                                        Special::EatMeal => {
                                            commands.entity(me).insert(crate::meals::MealRequest::Ate);
                                        }
                                        Special::ServeDrinks => {
                                            commands.entity(me).insert(crate::hotdrinks::ServedAt(*target));
                                        }
                                        Special::CleanUp => {
                                            commands.entity(*target).try_despawn();
                                            // (Scraps in the trash; the dish to the dishwasher or sink.)
                                            commands.entity(me).insert((crate::surroundings::Discarded, crate::surroundings::WashUp));
                                        }
                                        Special::PlaceFish => {
                                            if let Some((key, quality)) = action.fish_choice.take() {
                                                commands.entity(me).insert(if key == crate::fishbowl::BUY_GOLDFISH {
                                                    crate::fishbowl::BowlRequest::Buy(*target)
                                                } else {
                                                    crate::fishbowl::BowlRequest::Place { bowl: *target, key, quality }
                                                });
                                            }
                                        }
                                        Special::TakeFish => {
                                            commands.entity(me).insert(crate::fishbowl::BowlRequest::Take(*target));
                                        }
                                        Special::SmashDollhouse => {
                                            commands.entity(me).insert(crate::dollhouse::DollhouseDone::Smashed(*target));
                                        }
                                        Special::DisposeDollhouse => {
                                            commands.entity(me).insert(crate::dollhouse::DollhouseDone::ThrownAway(*target));
                                        }
                                        Special::KnockOverCan => {
                                            commands.entity(me).insert(crate::trashcans::CanDone::KnockedOver(*target));
                                        }
                                        Special::PickUpCan => {
                                            commands.entity(me).insert(crate::trashcans::CanDone::PickedUp(*target));
                                        }
                                        Special::Rummage => {}
                                        Special::FeedFish => {
                                            commands.entity(me).insert(crate::fishbowl::BowlRequest::Feed(*target));
                                        }
                                        Special::CleanBowl => {
                                            commands.entity(me).insert(crate::fishbowl::BowlRequest::Clean(*target));
                                        }
                                        Special::PickUpClothes => {
                                            commands.entity(me).insert(crate::laundry::LaundryRequest::PickedUp(*target));
                                        }
                                        Special::DoLaundry => {
                                            commands.entity(me).insert(crate::laundry::LaundryRequest::Wash(*target));
                                        }
                                        Special::EmptyHamper => {
                                            commands.entity(me).insert(crate::laundry::LaundryRequest::FromHamper);
                                        }
                                        Special::MoveLaundry => {
                                            commands.entity(me).insert(crate::laundry::LaundryRequest::Unload(*target));
                                        }
                                        Special::LoadDryer => {
                                            let to = if obj.kind == ObjectKind::Clothesline { crate::laundry::LaundryRequest::Hang(*target) } else { crate::laundry::LaundryRequest::Dry(*target) };
                                            commands.entity(me).insert(to);
                                        }
                                        Special::EmptyDryer => {
                                            commands.entity(me).insert(crate::laundry::LaundryRequest::Fresh(*target, obj.kind == ObjectKind::Clothesline));
                                        }
                                        Special::EmptyTrash => {
                                            commands.entity(*target).try_insert(crate::surroundings::TrashFill(0));
                                            // (The bag to the can outdoors.)
                                            commands.entity(me).insert(crate::surroundings::TakeOutTrash(*target));
                                        }
                                        Special::ReadPaper => {
                                            // (The paper read: its own, or the one carried to the seat.)
                                            let paper = paper_in_hand.get(me).ok().map_or(*target, |p| p.0);
                                            commands.entity(paper).try_despawn();
                                            commands.entity(me).remove::<crate::surroundings::PaperInHand>().insert(crate::story::ReadTheNews);
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
                                        Special::Fertilize => {
                                            if let Some((key, quality)) = action.fish_choice.take() {
                                                commands.entity(me).insert(crate::gardening::GardenRequest::Fertilize(*target, key, quality));
                                            }
                                        }
                                        Special::RevivePlant => {
                                            commands.entity(me).insert(crate::gardening::GardenRequest::Revive(*target));
                                        }
                                        Special::DisposePlant => {
                                            commands.entity(me).insert(crate::gardening::GardenRequest::Dispose(*target));
                                        }
                                        Special::Collect => {
                                            commands.entity(me).insert(crate::collecting::CollectRequest::Pick(*target));
                                        }
                                        Special::Fish => {
                                            commands.entity(me).insert(crate::collecting::CollectRequest::Fished { spot: *target, minutes: d.minutes, bait: action.fish_choice.take() });
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
                                        Special::Cook => {
                                            // (The snack chosen; taken out of the microwave, what went in.)
                                            if let Some(recipe) = action.recipe_choice.take() {
                                                commands.entity(me).insert(crate::meals::MealPlan(recipe));
                                            } else if d.name != "Take Out Dinner" {
                                                commands.entity(me).remove::<crate::meals::MealPlan>();
                                            }
                                            commands.entity(me).insert(crate::meals::MealRequest::Quick);
                                        }
                                        Special::TurnOnTv => {
                                            commands.entity(me).insert(crate::surroundings::WatchFrom(*target));
                                            commands.entity(*target).try_insert(crate::tvchannels::Tune { channel: action.channel_choice.map(|c| c as usize), by: me });
                                        }
                                        Special::StereoOn => {
                                            commands.entity(*target).try_insert(crate::music::StereoOn(None));
                                        }
                                        Special::StereoOff => {
                                            commands.entity(*target).remove::<crate::music::StereoOn>();
                                        }
                                        Special::Microwave => {
                                            match action.recipe_choice.take() {
                                                Some(recipe) => commands.entity(me).insert(crate::meals::MealPlan(recipe)),
                                                None => commands.entity(me).remove::<crate::meals::MealPlan>(),
                                            };
                                            commands.entity(me).insert(crate::meals::MicrowaveDone(*target));
                                        }
                                        Special::ClipCoupon => {
                                            commands.entity(me).insert(crate::coupons::ClipNow(*target));
                                        }
                                        Special::None | Special::BubbleBath | Special::Ducky | Special::VampireOnly => {}
                                        Special::Clear => {
                                            commands.entity(*target).try_despawn();
                                        }
                                    }
                                }
                            } else {
                                finished = true;
                            }
                        }
                        ActionKind::Social { target, social } => {
                            let s = crate::social::social(*social);
                            let reach = if action.seated { crate::seatedtalk::SEATED_RANGE + 0.5 } else { crate::social::apart(s) + 1.6 };
                            let close = positions.get(target).is_some_and(|p| p.0.distance(tf.translation) < reach);
                            // (Had where they sit, or lie in bed: over once either gets up.)
                            let there = |e: &Entity| if in_bed.is_some() { lying.contains(e) } else { sitting.contains(e) };
                            if action.seated && !(close && there(target) && there(&me)) {
                                finished = true;
                            } else if !close {
                                action.phase = Phase::Start;
                            } else {
                                motives.add(SOCIAL, s.social_per_hour * dt / 60.0);
                                motives.add(FUN, s.fun_per_hour * dt / 60.0);
                                let k = dt / s.minutes * crate::life::social_affinity(&sim.traits, s.name);
                                // (Liking comes of how it went, as it ends: the game's. Romance builds
                                // through it, ours.)
                                let (f, r) = match action.social_outcome {
                                    // (Romance only of a flirt that's taken as one.)
                                    Some(o) => (0.0, if o.result == crate::conversation::Commodity::Amorous || s.romance <= 0.0 { s.romance * k } else { 0.0 }),
                                    None => (s.friendship * k, s.romance * k),
                                };
                                if rels.add(*target, f, r) {
                                    life.write(LifeEvent::new(me, LifeEventKind::MadeFriend { other: *target }));
                                }
                                if s.cat == crate::social::SocialCat::Care {
                                    care_fx.push((*target, s.care.map(|v| v * dt / 60.0)));
                                }
                                social_fx.push((*target, me, s.social_per_hour * dt / 60.0, s.fun_per_hour * dt / 60.0, f, r, None));
                                if !action.seated {
                                    anim.pose = if s.name.contains("Dance") { Pose::Dance } else { Pose::Talk };
                                }
                                // (A Teacher Extraordinaire gets through homework twice as fast.)
                                let minutes = if s.effect == SocialEffect::HelpHomework && crate::journal::earned(journals.get(me).ok(), "Teacher Extraordinaire") { s.minutes * 0.5 } else { s.minutes };
                                // (A cuddle on the sofa goes on a while.)
                                let minutes = if action.seated && s.name == crate::seatedtalk::CUDDLE { crate::seatedtalk::CUDDLE_MINUTES } else { minutes };
                                if elapsed >= minutes {
                                    finished = true;
                                    // How it went: the conversation pushed its way, liking with it, and
                                    // the relationship's flags.
                                    if let Some(o) = action.social_outcome
                                        && let Some((tsim, ..)) = who.get(target)
                                    {
                                        let before = rels.get(*target);
                                        // (Charisma for one taken well, once they've the skill.)
                                        let charm = crate::conversation::charisma_points(&o, &before.stc);
                                        if charm > 0.0 && skills.0.contains_key("Charisma") {
                                            let rate = crate::life::skill_rate(&sim.traits, "Charisma") * crate::wishes::reward_skill_rate(wishes) * mood_learning;
                                            if let Some(level) = skills.add_points("Charisma", charm * rate) {
                                                notes.push(format!("{} reached level {level} in Charisma!", sim.first));
                                                life.write(LifeEvent::new(me, LifeEventKind::SkillUp { skill: "Charisma", level }));
                                            }
                                        }
                                        let (f, stc) = crate::conversation::effect(*social, before.friendship, before.stc, &o, sim, tsim);
                                        let e = rels.entry(*target);
                                        e.stc = stc;
                                        crate::conversation::apply_bits(e, &o);
                                        if rels.add(*target, f, 0.0) {
                                            life.write(LifeEvent::new(me, LifeEventKind::MadeFriend { other: *target }));
                                        }
                                        if rels.get(*target).friendship >= 0.0 {
                                            rels.entry(*target).enemy = false;
                                        }
                                        social_fx.push((*target, me, 0.0, 0.0, 0.0, 0.0, Some(crate::conversation::Shared::of(&rels.get(*target)))));
                                        memories.0.entry(me).or_default().after(*social, clock.minutes);
                                        // (A trait of theirs the way it went turned on: learned.)
                                        if o.learned.is_some() || o.learn_random || o.teaches.iter().any(|t| !t.is_empty()) {
                                            let (learner, about, what, random, teaches) = (me, *target, o.learned, o.learn_random, o.teaches);
                                            commands.queue(move |w: &mut World| {
                                                w.write_message(crate::learning::Learned { learner, about, what, random, teaches });
                                            });
                                        }
                                        // (The topics it raises: the one's, the other's, and both of
                                        // theirs, the conversation's.)
                                        let [mine, theirs, both] = o.topics;
                                        let other = *target;
                                        for (who, name) in [(me, mine), (other, theirs), (me, both), (other, both)] {
                                            if !name.is_empty() {
                                                commands.queue(move |w: &mut World| {
                                                    w.write_message(crate::topics::AddTopic::new(who, name));
                                                });
                                            }
                                        }
                                        debug!("{} and {}: {:?}, liking {:+.2} to {:.1}, now {}", sim.first, tsim.first, o.result, f, rels.get(*target).friendship, stc.name());
                                    }
                                    if s.name == "Tell Joke" {
                                        did.write(crate::journal::Did::count(me, crate::journal::Stat::Jokes, 1.0));
                                    }
                                    life.write(LifeEvent::new(me, LifeEventKind::Socialized { other: *target, social: s.name }));
                                    life.write(LifeEvent::new(*target, LifeEventKind::Socialized { other: me, social: s.name }));
                                    // (The game's `kSocialInteraction`, both sides, by the game's name; a
                                    // fight's once it's fought.)
                                    if s.effect != SocialEffect::Fight {
                                        let (target, name, accepted) = (*target, s.name, action.social_outcome.is_none_or(|o| o.accepted));
                                        commands.queue(move |w: &mut World| crate::dreams::social(w, me, target, name, accepted, None));
                                    }
                                    // (For the traits that feel it: `traitmoods`.)
                                    {
                                        let done = crate::traitmoods::Socialised { actor: me, target: *target, kiss: s.effect == SocialEffect::Kiss, accepted: action.social_outcome.is_none_or(|o| o.accepted) };
                                        commands.queue(move |w: &mut World| {
                                            w.write_message(done);
                                        });
                                    }
                                    // (Romance in front of others: those it's an affront to may see
                                    // it, the game's `JealousyLevel`.)
                                    if let Some(level) = crate::conversation::model(*social).map(|m| m.jealousy).filter(|l| *l > 0) {
                                        let (actor, target, accepted) = (me, *target, action.social_outcome.is_none_or(|o| o.accepted));
                                        commands.queue(move |w: &mut World| {
                                            w.write_message(crate::jealousy::Romanced { actor, target, level, accepted });
                                        });
                                    }
                                    let rel = rels.get(*target);
                                    let tname = who.get(target).map(|w| w.0.first.clone()).unwrap_or_default();
                                    let t_member = who.get(target).is_some_and(|w| w.2);
                                    match s.effect {
                                        // (An opportunity's delivery made: `opportunities`.)
                                        // (Questioned or interviewed, for a report or story: `careermetrics`.)
                                        SocialEffect::Interview if action.social_outcome.is_none_or(|o| o.accepted) => {
                                            let subject = *target;
                                            commands.queue(move |w: &mut World| crate::careermetrics::interviewed(w, me, subject));
                                        }
                                        // (Money for the campaign, asked for and given: `careermetrics`.)
                                        SocialEffect::CampaignDonation if action.social_outcome.is_none_or(|o| o.accepted) => {
                                            let (giver, liking) = (*target, rel.friendship);
                                            commands.queue(move |w: &mut World| crate::careermetrics::donate(w, me, giver, liking));
                                        }
                                        SocialEffect::DeliverOpportunity => {
                                            debug!("{} delivered to {tname}", sim.first);
                                            life.write(LifeEvent::new(me, LifeEventKind::Delivered { other: *target }));
                                        }
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
                                                commands.entity(*target).try_insert(JoinHousehold { last_name: Some(sim.last.clone()) });
                                            }
                                        }
                                        SocialEffect::BreakUp => {
                                            status_fx.push((me, *target, Some(RelStatus::Ex), false));
                                            life.write(LifeEvent::new(me, LifeEventKind::BrokeUp));
                                            life.write(LifeEvent::new(*target, LifeEventKind::BrokeUp));
                                            notes.push(format!("{} broke up with {}.", sim.first, tname));
                                        }
                                        SocialEffect::MoveIn => {
                                            commands.entity(*target).try_insert(JoinHousehold { last_name: None });
                                            notes.push(format!("{} moved in with the household!", tname));
                                        }
                                        SocialEffect::TryForBaby => {
                                            conceive.write(crate::little::Conceive { a: me, b: *target });
                                        }
                                        SocialEffect::PutToBed => {
                                            commands.entity(*target).try_insert(crate::little::Bedtime);
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
                                            let fought = *target;
                                            commands.queue(move |w: &mut World| crate::dreams::social(w, me, fought, "Fight", true, Some(won)));
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
                                        SocialEffect::HelpHomework => {
                                            let student = *target;
                                            commands.queue(move |world: &mut World| {
                                                crate::rabbitholes::complete_homework(world, student, Some(me), minutes);
                                            });
                                        }
                                        SocialEffect::MoochSmall | SocialEffect::MoochLarge => {
                                            let (small, large) = crate::social::MOOCH_AMOUNTS;
                                            let mut rng = rand::rng();
                                            let base = if s.effect == SocialEffect::MoochLarge { large } else { small } as f32;
                                            let times = if rng.random_bool(0.25) { 4.0 } else if rng.random_bool(0.5) { 2.0 } else { 1.0 };
                                            let n = (base * times * rng.random_range(0.7..1.3)).round().max(1.0) as i64;
                                            if people.get(me).is_ok_and(|p| p.3)
                                                && let Some(h) = household.as_mut()
                                            {
                                                h.funds += n;
                                                notes.push(format!("{} mooched §{n} from {tname}.", sim.first));
                                            }
                                        }
                                        SocialEffect::LearnRecipe => {
                                            let (first, other) = (sim.first.clone(), tname.clone());
                                            commands.queue(move |w: &mut World| {
                                                let Some(data) = w.get_resource::<crate::icons::GameUi>().map(|u| u.data.clone()) else { return };
                                                let cooking = w.get::<Skills>(me).map_or(0, |s| s.level("Cooking"));
                                                let known = w.get::<crate::meals::KnownRecipes>(me).cloned();
                                                let books = crate::meals::books_for(&data, cooking, known.as_ref());
                                                use rand::seq::IndexedRandom;
                                                let Some(&i) = books.choose(&mut rand::rng()) else { return };
                                                let r = &data.recipes[i];
                                                let mut known = known.unwrap_or_default();
                                                known.0.push(r.key.clone());
                                                w.write_message(crate::dreams::DreamEvent { key: Some(r.key.clone()), ..crate::dreams::DreamEvent::new(me, "kLearnedRecipe") });
                                                let member = w.get::<HouseholdMember>(me).is_some();
                                                w.entity_mut(me).insert(known);
                                                if member {
                                                    w.resource_mut::<Notifications>().push(format!("{first} learned to make {} from {other}.", r.name));
                                                }
                                            });
                                        }
                                        SocialEffect::MoochFood => {
                                            commands.entity(me).queue_silenced(|mut w: EntityWorldMut| {
                                                if let Some(mut m) = w.get_mut::<Motives>() {
                                                    m.add(HUNGER, MOOCHED_SNACK);
                                                }
                                            });
                                            if people.get(me).is_ok_and(|p| p.3) {
                                                notes.push(format!("{} mooched a snack from {tname}.", sim.first));
                                            }
                                        }
                                        // (Done if they're of the household or like the one asking, as the
                                        // game's: liking 25.)
                                        SocialEffect::RequestHomework => {
                                            if t_member || rel.friendship >= crate::social::HOMEWORK_LIKING {
                                                notes.push(format!("{tname} agreed to do their homework."));
                                                commands.entity(*target).queue_silenced(|mut w: EntityWorldMut| {
                                                    if let Some(mut h) = w.get_mut::<crate::rabbitholes::Homework>() {
                                                        h.1 = true;
                                                    }
                                                });
                                            } else {
                                                notes.push(format!("{tname} won't do their homework for {}.", sim.first));
                                            }
                                        }
                                        // (The boss agrees, or doesn't: the game's `JobPromotion` and
                                        // `PayRise`, on a friendly answer.)
                                        SocialEffect::AskPromotion | SocialEffect::AskRaise => {
                                            let agreed = action.social_outcome.is_some_and(|o| o.result == crate::conversation::Commodity::Friendly);
                                            match job.as_deref_mut() {
                                                Some(j) if agreed && s.effect == SocialEffect::AskPromotion => j.promote = true,
                                                Some(j) if agreed => {
                                                    j.give_raise();
                                                    notes.push(format!("{} asked {tname} for a raise and got one: §{} an hour now.", sim.first, j.info().hourly + j.raise));
                                                }
                                                _ => notes.push(format!("{tname} turned {} down.", sim.first)),
                                            }
                                        }
                                        // (Having it out, or owning up: the one cheated on finds out.)
                                        SocialEffect::Accuse | SocialEffect::Confess => {
                                            let (victim, naughty, level) = if s.effect == SocialEffect::Accuse { (me, *target, 3) } else { (*target, me, 2) };
                                            commands.queue(move |w: &mut World| {
                                                w.write_message(crate::jealousy::CheatedOn { victim, naughty, other: None, level });
                                            });
                                        }
                                        SocialEffect::Greet => {
                                            notes.push(format!("{} let {tname} in.", sim.first));
                                            commands.entity(*target).remove::<crate::doorbell::AtTheDoor>();
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
                                        // (Asked to stay the night: till the morning, at home here.)
                                        SocialEffect::StayOver => {
                                            let full = who.get(target).map(|w| w.0.full_name()).unwrap_or_default();
                                            if action.social_outcome.is_none_or(|o| o.accepted) {
                                                let (guest, until) = (*target, crate::guests::stay_until(clock.minutes));
                                                commands.queue(move |w: &mut World| {
                                                    if let Ok(mut e) = w.get_entity_mut(guest) {
                                                        e.insert(crate::guests::StayingOver);
                                                        if let Some(mut v) = e.get_mut::<Visitor>() {
                                                            v.leave_at = v.leave_at.max(until);
                                                        }
                                                    }
                                                });
                                                notes.push(format!("{full}: \"{}\"", crate::guests::STAYING));
                                            } else {
                                                notes.push(format!("{full}: \"{}\"", crate::guests::NOT_STAYING));
                                            }
                                        }
                                        SocialEffect::AskToLeave => {
                                            commands.entity(*target).try_insert(GoingHome);
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
                        // (Out once they've had the extinguisher on it long enough; then on to the next.)
                        ActionKind::Extinguish { target, .. } => {
                            if elapsed >= crate::fire::extinguish_minutes(&sim.traits) {
                                finished = true;
                                commands.entity(*target).try_despawn();
                                commands.entity(me).insert(crate::fire::PutOutOne);
                                notes.push(format!("{} put the fire out!", sim.first));
                                outro = Some((crate::fire::EXTINGUISHER_STOP, None, 2.4, Entity::PLACEHOLDER, false));
                            }
                        }
                        // (Done once its clips are: `carry` says how long they take.)
                        ActionKind::Carry { secs, .. } => {
                            if elapsed >= *secs {
                                finished = true;
                            }
                        }
                        ActionKind::PetSocial { social, kind, .. } => {
                            let s = &crate::pets::PET_SOCIALS[*social];
                            // (An Animal Lover warms to any, a Cat or Dog Person to theirs most: the
                            // game's kAnimalLoverLtrMultiplier and kPreferredPetLtrMultiplier.)
                            let keen = crate::pets::pet_person(&sim.traits, kind);
                            motives.add(FUN, s.fun * keen * dt / 60.0);
                            motives.add(SOCIAL, s.social * keen * dt / 60.0);
                            if elapsed >= s.minutes {
                                finished = true;
                                // (A set in parts ended with its end, the pet's half with it.)
                                if let (Some(suite), ActionKind::PetSocial { target, .. }) = (baked.as_deref().and_then(|b| s.suite(b, sim.age == Age::Child, kind)), &action.kind)
                                    && !suite.end.is_empty()
                                {
                                    outro = Some((suite.end, None, suite.end_secs, *target, false));
                                }
                                // (How the pet takes to them for it: the social's liking, warmer for
                                // pet people; turned down, a little less.)
                                let (id, liking) = (sim.id, s.liking * keen);
                                // (A Friendly pet warms to them faster: the game's
                                // `kFriendlyPetTraitWithHumansLtrGainMultiplier`.)
                                if let ActionKind::PetSocial { target, .. } = action.kind {
                                    commands.queue(move |w: &mut World| {
                                        let rejected = w.get_entity_mut(me).ok().is_some_and(|mut e| e.take::<crate::pets::Rejected>().is_some());
                                        let home = w.get::<crate::pets::HomePet>(target).is_some();
                                        let warmth = w.get::<crate::pets::PetTraits>(target).map_or(1.0, |t| crate::pettraits::liking_multiplier(&t.0));
                                        if let Ok(mut e) = w.get_entity_mut(target) {
                                            let mut b = e.take::<crate::pets::PetBonds>().unwrap_or_default();
                                            b.add(id, home, if rejected { -1.0 } else if liking > 0.0 { liking * warmth } else { liking });
                                            e.insert(b);
                                        }
                                    });
                                }
                            }
                        }
                        ActionKind::Outdoor { what, .. } => {
                            motives.add(FUN, what.fun() * dt / 60.0);
                            if elapsed >= what.minutes() {
                                finished = true;
                                // (A contemplation's end: Fascinated, as the game's.)
                                if *what == crate::seasonal::Outdoor::Contemplate {
                                    let now = clock.minutes;
                                    commands.entity(me).queue_silenced(move |mut w: EntityWorldMut| {
                                        if let Some(mut m) = w.get_mut::<crate::life::Moodlets>() {
                                            m.add(crate::life::MoodletKind::Fascinated, now);
                                        }
                                    });
                                }
                                let p = tf.translation;
                                commands.entity(me).insert(crate::seasonal::OutdoorEvent::Done { what: *what, at: p, facing: tf.rotation, child: sim.age.is_little() || sim.age == Age::Child });
                                // (Then up out of the snow.)
                                let c = what.clip();
                                if !c.end.is_empty() {
                                    outro = Some((c.end, None, c.end_secs, Entity::PLACEHOLDER, false));
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
                            // (As long as the game tunes the repair of the thing, by Handiness.)
                            let tuning = objects.get(*target).ok().and_then(|(o, ..)| tunings.get(o.objd));
                            let minutes = match tuning {
                                Some(t) => maintenance_minutes(repair_time(t, handy), 0.0, &sim.traits, objects.get(*target).ok().map(|(o, ..)| o.kind), false),
                                None => maintenance_minutes(90.0, handy, &sim.traits, objects.get(*target).ok().map(|(o, ..)| o.kind), false),
                            };
                            // (The game's points for the whole repair, as far as it's got: the
                            // thing's RepairSkillGain.)
                            let whole = tuning.map_or(1000.0, |t| t.repair_skill);
                            let rate = crate::life::skill_rate(&sim.traits, "Handiness") * crate::wishes::reward_skill_rate(wishes) * mood_learning;
                            if let Some(level) = skills.add_points("Handiness", dt / minutes.max(1.0) * whole * rate) {
                                notes.push(format!("{} reached level {level} in Handiness!", sim.first));
                                life.write(LifeEvent::new(me, LifeEventKind::SkillUp { skill: "Handiness", level }));
                            }
                            motives.add(FUN, -4.0 * dt / 60.0);
                            if elapsed >= minutes {
                                finished = true;
                                // Electronics can shock the unskilled, at the game's odds for the
                                // thing (none for the Handy, the pregnant or an Electrician); one
                                // still singed, or standing in water, surely; a second shock while
                                // singed stops their heart.
                                let electric = objects.get(*target).is_ok_and(|(o, ..)| matches!(o.kind, ObjectKind::Tv | ObjectKind::Computer | ObjectKind::Stereo));
                                let journal = journals.get(me).ok();
                                let safe = sim.traits.contains(&crate::life::Trait::Handy) || pregnancy.is_some() || crate::journal::earned(journal, "Electrician");
                                let singed = moodlets.is_some_and(|m| m.has(crate::life::MoodletKind::Singed));
                                let wet = tuning.is_some_and(|t| t.shock_in_puddle) && puddles_q.iter().any(|p| p.translation.xz().distance(tf.translation.xz()) < 0.6);
                                let chance = match tuning {
                                    Some(t) => shock_chance(t, skills.level("Handiness") as f32) as f64 / 100.0,
                                    None => (0.35 - handy as f64 * 0.04).clamp(0.0, 1.0),
                                };
                                let shocked = electric && !safe && (singed || wet || rand::rng().random_bool(chance.clamp(0.0, 1.0)));
                                if let Ok((o, _, mut used, _)) = objects.get_mut(*target) {
                                    used.0 = None;
                                    if shocked {
                                        commands.entity(me).insert(crate::death::Shocked);
                                        notes.push(format!("{} was electrocuted trying to fix {}!", sim.first, the(&o.name)));
                                    } else {
                                        commands.entity(*target).remove::<(Broken, crate::grime::Charred)>();
                                        if let Ok(mut w) = wear.get_mut(*target) {
                                            w.sturdy_until = clock.minutes + tunings.sturdy_minutes(o.objd);
                                        }
                                        notes.push(format!("{} fixed {}.", sim.first, the(&o.name)));
                                        let plumbing = matches!(o.kind, ObjectKind::Shower | ObjectKind::Bathtub | ObjectKind::ShowerTub | ObjectKind::Sink | ObjectKind::Dishwasher | ObjectKind::Toilet);
                                        if electric {
                                            did.write(crate::journal::Did::count(me, crate::journal::Stat::ElectricalRepairs, 1.0));
                                        } else if plumbing {
                                            did.write(crate::journal::Did::count(me, crate::journal::Stat::PlumbingRepairs, 1.0));
                                            // (Fixed by a Plumber, it never breaks again.)
                                            if crate::journal::earned(journal, "Plumber") {
                                                let had = upgraded.get(*target).map_or(0, |u| u.0);
                                                commands.entity(*target).try_insert(crate::upgrades::Upgrades(had | crate::upgrades::Upgrade::Unbreakable.bit()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ActionKind::Upgrade { target, bit } => {
                            let handy = skills.level("Handiness") as f32 + if sim.traits.contains(&crate::life::Trait::Handy) { 3.0 } else { 0.0 };
                            let bell = crate::frontdoor::bell_of(*bit);
                            let base = if bell.is_some() { crate::frontdoor::MINUTES } else { crate::upgrades::Upgrade::MINUTES };
                            let minutes = maintenance_minutes(base, handy, &sim.traits, objects.get(*target).ok().map(|(o, ..)| o.kind), true);
                            // (The game's points for the whole upgrade, as far as it's got.)
                            let whole = objects.get(*target).ok().and_then(|(o, ..)| tunings.get(o.objd)).map_or(3600.0, |t| t.upgrade_skill);
                            let rate = crate::life::skill_rate(&sim.traits, "Handiness") * crate::wishes::reward_skill_rate(wishes) * mood_learning;
                            if let Some(level) = skills.add_points("Handiness", dt / minutes.max(1.0) * whole * rate) {
                                notes.push(format!("{} reached level {level} in Handiness!", sim.first));
                                life.write(LifeEvent::new(me, LifeEventKind::SkillUp { skill: "Handiness", level }));
                            }
                            motives.add(FUN, -3.0 * dt / 60.0);
                            if elapsed >= minutes {
                                finished = true;
                                let electric = objects.get(*target).is_ok_and(|(o, ..)| matches!(o.kind, ObjectKind::Tv | ObjectKind::Computer | ObjectKind::Stereo));
                                let shocked = electric && !crate::journal::earned(journals.get(me).ok(), "Electrician") && rand::rng().random_bool(((0.25 - handy * 0.03) as f64).clamp(0.0, 1.0));
                                if let Some(bell) = bell {
                                    // (The household's bell from now on, rung to hear it.)
                                    let door = *target;
                                    commands.queue(move |w: &mut World| crate::frontdoor::bell_changed(w, door, bell));
                                    if let Ok((_, _, mut used, _)) = objects.get_mut(*target) {
                                        used.0 = None;
                                    }
                                } else if let Ok((o, _, mut used, _)) = objects.get_mut(*target) {
                                    used.0 = None;
                                    let name = crate::upgrades::Upgrade::from_bit(*bit).and_then(|u| u.name(o.kind)).unwrap_or("upgrade");
                                    if shocked {
                                        commands.entity(me).insert(crate::death::Shocked);
                                        notes.push(format!("{} was electrocuted upgrading {}!", sim.first, the(&o.name)));
                                    } else {
                                        let had = upgraded.get(*target).map_or(0, |u| u.0);
                                        commands.entity(*target).try_insert(crate::upgrades::Upgrades(had | *bit));
                                        notes.push(format!("{} upgraded {}: {name}.", sim.first, the(&o.name)));
                                        did.write(crate::journal::Did::kind(me, crate::journal::Kinds::Upgrades, format!("{name} ({:?})", o.kind)));
                                    }
                                }
                            }
                        }
                        // (On the phone till the party's arranged in its picker: `parties`.)
                        ActionKind::ThrowParty => {}
                        ActionKind::ReadJournal => {
                            let minutes = if sim.traits.contains(&crate::life::Trait::Bookworm) { crate::careermetrics::BOOKWORM_JOURNAL_MINUTES } else { crate::careermetrics::JOURNAL_MINUTES };
                            if elapsed >= minutes {
                                finished = true;
                                commands.queue(move |w: &mut World| crate::careermetrics::journal_read(w, me));
                            }
                        }
                        ActionKind::TraitAct { act, minutes } => {
                            let per_hour = act.per_hour();
                            for (i, g) in per_hour.iter().enumerate() {
                                motives.add(i, g * dt / 60.0);
                            }
                            // (Up off the floor after a fake faint.)
                            let up = minutes - crate::traitacts::GET_UP_MINUTES;
                            if matches!(act, crate::traitacts::TraitAct::FakePassOut | crate::traitacts::TraitAct::Faint) && elapsed >= up && elapsed - dt < up {
                                commands.entity(me).insert(crate::anim::ActionClip::new(Some("a_sleeponFloor_getUp_x"), &["a_idle_neutral_loop_"]));
                            }
                            finished = elapsed >= *minutes;
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
                                    format!("{} hired a maid. She'll come every morning at nine (§{} a day).", sim.first, crate::services::MAID_COST)
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
                        ActionKind::AdoptPet { kind } => {
                            if elapsed >= 5.0 {
                                finished = true;
                                let fee = crate::pets::adoption_fee(kind);
                                match household.as_deref_mut() {
                                    Some(h) if h.funds >= fee => {
                                        h.funds -= fee;
                                        let name = crate::pets::random_pet_name(&mut rand::rng());
                                        commands.insert_resource(crate::pets::AdoptPetOrder { kind, name: name.clone() });
                                        notes.push(format!("{} adopted {name}, a {} (§{fee}), who's on the way home.", sim.first, crate::pets::kind_name(kind).to_lowercase()));
                                    }
                                    _ => notes.push(format!("There isn't enough money to adopt (§{fee}).")),
                                }
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
                                        // (The game's `kOrderedService`.)
                                        commands.queue(move |w: &mut World| {
                                            w.write_message(crate::dreams::DreamEvent { key: Some("PizzaDelivery".into()), ..crate::dreams::DreamEvent::new(me, "kOrderedService") });
                                        });
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
                                        "{} retired from the {} career after years as {}, and will be paid a pension of §{daily} a day.",
                                        sim.first,
                                        j.career().name,
                                        crate::careers::a_title(j.info(), sim)
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
                            if rels.add(*target, df, 0.0) {
                                life.write(LifeEvent::new(me, LifeEventKind::MadeFriend { other: *target }));
                            }
                            social_fx.push((*target, me, s, f, df, 0.0, None));
                            if elapsed >= PHONE_CHAT_MINUTES {
                                finished = true;
                                life.write(LifeEvent::new(me, LifeEventKind::Socialized { other: *target, social: "Chat" }));
                                life.write(LifeEvent::new(*target, LifeEventKind::Socialized { other: me, social: "Chat" }));
                            }
                        }
                        ActionKind::CheckUpOnWork => {
                            if elapsed >= crate::computertraits::CHECK_UP_MINUTES {
                                finished = true;
                                commands.entity(me).insert(crate::computertraits::CheckedUp);
                            }
                        }
                        ActionKind::CheckRealEstate => {
                            if elapsed >= crate::timeoff::CALL_MINUTES {
                                finished = true;
                                commands.queue(move |w: &mut World| w.resource_mut::<crate::estatedialog::EstateDialog>().open(me));
                            }
                        }
                        ActionKind::CallServices => {
                            if elapsed >= crate::timeoff::CALL_MINUTES {
                                finished = true;
                                commands.queue(move |w: &mut World| w.resource_mut::<crate::servicesdialog::ServicesDialog>().open(me));
                            }
                        }
                        ActionKind::CallPolice | ActionKind::CallFirefighters | ActionKind::CallEverybodyHome => {
                            if elapsed >= crate::timeoff::CALL_MINUTES {
                                finished = true;
                                let (kind, name, now) = (action.kind.clone(), sim.first.clone(), clock.minutes);
                                commands.queue(move |world: &mut World| crate::calls::emergency_call(world, me, kind, &name, now));
                            }
                        }
                        ActionKind::QuitJobCall => {
                            if elapsed >= crate::timeoff::CALL_MINUTES {
                                finished = true;
                                if job.is_some() {
                                    commands.entity(me).remove::<Job>();
                                    notes.push(format!("{} called their boss and quit their job.", sim.first));
                                }
                            }
                        }
                        ActionKind::SilencePhone => finished = true,
                        ActionKind::TimeOff(ask) => {
                            if elapsed >= crate::timeoff::CALL_MINUTES {
                                finished = true;
                                commands.entity(me).insert(crate::timeoff::Called(*ask));
                            }
                        }
                        ActionKind::PrankCall { target } => {
                            if elapsed >= crate::pranks::PRANK_MINUTES {
                                finished = true;
                                commands.entity(me).insert(crate::pranks::PrankMade(*target));
                            }
                        }
                        ActionKind::InviteOut { target, lot, date } => {
                            if elapsed >= crate::meetups::CALL_MINUTES {
                                finished = true;
                                let (t, l, d) = (*target, *lot, *date);
                                let tname = who.get(&t).map(|w| w.0.first.clone()).unwrap_or_default();
                                let place = crate::visit::place_name(&world.data, l);
                                commands.entity(t).insert(crate::meetups::Meeting::new(me, l, d, clock.minutes));
                                notes.push(if d {
                                    format!("{tname} will meet {} at {place} for a date.", sim.first)
                                } else {
                                    format!("{tname} will meet {} at {place}.", sim.first)
                                });
                                // (And off they go to meet them.)
                                commands.entity(me).queue_silenced(move |mut w: EntityWorldMut| {
                                    if let Some(mut q) = w.get_mut::<ActionQueue>() {
                                        q.0.push_back(Action::new("Visit Lot", ActionKind::GoToLot { lot: l }, false));
                                    }
                                });
                            }
                        }
                        ActionKind::Invite { target } => {
                            if elapsed >= 8.0 {
                                finished = true;
                                let tname = who.get(target).map(|w| w.0.first.clone()).unwrap_or_default();
                                commands.entity(*target).try_insert(Invited { arrive_at: clock.minutes + 45.0 });
                                notes.push(format!("{} invited {} over. They'll be here soon.", sim.first, tname));
                                // (The game's `kInvitedSimOver`.)
                                let invited = *target;
                                commands.queue(move |w: &mut World| {
                                    w.write_message(crate::dreams::DreamEvent { other: Some(invited), ..crate::dreams::DreamEvent::new(me, "kInvitedSimOver") });
                                });
                            }
                        }
                        ActionKind::JoinCareer { track, .. } => {
                            if elapsed >= 15.0 {
                                finished = true;
                                let (j, note) = crate::careers::new_job(sim, *track);
                                notes.push(note);
                                commands.entity(me).insert(j);
                                life.write(LifeEvent::new(me, LifeEventKind::NewJob));
                            }
                        }
                        ActionKind::Outro { secs, stand_at, .. } => {
                            if elapsed >= *secs {
                                finished = true;
                                stand_up_at = *stand_at;
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
                                // (A Lifefruit: a day more of life.)
                                if key == "Lifefruit" {
                                    commands.entity(me).insert(crate::aging::AteLifefruit);
                                }
                                // (Pregnant: a watermelon sways towards a girl, an apple a boy.)
                                let key = key.clone();
                                commands.entity(me).queue_silenced(move |mut w: EntityWorldMut| {
                                    if let Some(mut p) = w.get_mut::<crate::little::Pregnancy>() {
                                        p.ate(&key);
                                    }
                                });
                            }
                        }
                        // (Out on the street, the jog runs itself: `jog`; a ride: `riding`.)
                        ActionKind::Jog { .. } | ActionKind::Ride { .. } | ActionKind::DogTrick { .. } => {}
                        _ => finished = true,
                    }
                }
            }
        }

        if finished {
            if let Some(a) = queue.0.front() {
                debug!("{} finished {} ({:?}{})", sim.first, a.label, a.phase, if a.cancel { ", cancelled" } else { "" });
            }
            // (A seated talk over: the talk off them both, and on with what they sat at.)
            if let Some(Action { kind: ActionKind::Social { target, social }, seated: true, .. }) = queue.0.front() {
                commands.entity(me).remove::<crate::seatedtalk::TalkOverlay>();
                commands.entity(*target).remove::<crate::seatedtalk::TalkOverlay>();
                // (Out of a cuddle: back to sitting, or lying back, those still there.)
                if crate::social::social(*social).name == crate::seatedtalk::CUDDLE {
                    for who in [me, *target] {
                        let (sat, lies) = (sitting.contains(&who), lying.contains(&who));
                        commands.entity(who).queue_silenced(move |mut w: EntityWorldMut| {
                            if let Some(c) = w.take::<crate::seatedtalk::Cuddling>()
                                && c.accepted
                            {
                                if c.bed && lies {
                                    w.insert(crate::seatedtalk::bed_uncuddle_clip(c.x));
                                    if !c.x {
                                        w.insert(crate::seatedtalk::LieBack);
                                    }
                                } else if !c.bed && sat {
                                    w.insert(crate::seatedtalk::uncuddle_clip(c.x, c.teen));
                                }
                            }
                        });
                    }
                }
            } else {
                commands.entity(me).remove::<crate::anim::ActionClip>();
            }
            if let Some(Action { kind: ActionKind::Social { target, .. }, .. }) = queue.0.front() {
                partners_done.push((*target, me));
            }
            // (A talk had sat down: still sat, on with what they sat at.)
            let sat_on = queue.0.front().is_some_and(|a| a.seated);
            // (Up from a sleep, the bed made after, as the game's `ShouldMakeBed`: by the Neat and
            // the Perfectionist always, a Slob never, anyone else half the time.)
            let make_bed = queue
                .0
                .front()
                .and_then(|a| match a.kind {
                    ActionKind::Object { target, .. } if matches!(a.label.as_str(), "Sleep" | "Nap") => Some(target),
                    _ => None,
                })
                .filter(|_| {
                    let has = |t: crate::life::Trait| sim.traits.contains(&t);
                    !has(crate::life::Trait::Slob) && (has(crate::life::Trait::Neat) || has(crate::life::Trait::Perfectionist) || rand::random_bool(0.5))
                })
                .and_then(|bed| objects.get(bed).ok().and_then(|(o, ..)| interactions_for(o.kind).iter().position(|d| d.special == Special::MakeBed)).map(|def| (bed, def)));
            queue.0.pop_front();
            *decay = DecayScale::default();
            commands.entity(me).remove::<PathFollow>();
            match outro {
                // (Still where they were, their interaction's ending plays; then they're up.)
                Some((clips, then, secs, target, mirror)) => {
                    queue.0.push_front(Action::new("Finish Up", ActionKind::Outro { clips, then, secs, stand_at: stand_up_at, target }, true));
                    let mut c = match clips.split_first() {
                        Some((first, rest)) => crate::anim::ActionClip::steps(first, rest, &[]),
                        None => crate::anim::ActionClip::new(None, &[]),
                    };
                    c.after = then;
                    c.mirror = mirror;
                    commands.entity(me).insert(c);
                }
                None if sat_on => {}
                None => {
                    anim.pose = Pose::Stand;
                    anim.seat_height = 0.0;
                    if let Some(p) = stand_up_at {
                        tf.translation = Vec3::new(p.x, ground(floor.0, p.x, p.y), p.y);
                    }
                }
            }
            if let Some((bed, def)) = make_bed {
                let at = queue.0.iter().take_while(|a| matches!(a.kind, ActionKind::Outro { .. })).count();
                queue.0.insert(at, Action::new("Make Bed", ActionKind::Object { target: bed, def }, true));
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
                    } else {
                        r.romantic = true;
                    }
                }
                r.kissed |= kissed;
                r.romantic |= kissed;
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
    for (target, actor, social, fun, friendship, romance, stc) in social_fx {
        if let Ok((_, tsim, queue, mut tf, mut motives, _, mut anim, _, mut rels, path, _, _, _, _, _)) = sims.get_mut(target) {
            motives.add(SOCIAL, social);
            motives.add(FUN, fun);
            // (Their conversation, and liking, the same both ways.)
            if let Some(shared) = stc {
                let liking = shared.liking;
                let r = rels.entry(actor);
                let was = r.friendship;
                r.stc = shared.stc;
                r.friendship = liking;
                r.enemy = shared.enemy;
                r.romantic = shared.romantic;
                if was < crate::social::FRIEND_THRESHOLD && liking >= crate::social::FRIEND_THRESHOLD {
                    life.write(LifeEvent::new(target, LifeEventKind::MadeFriend { other: actor }));
                }
            }
            if rels.add(actor, friendship, romance) {
                life.write(LifeEvent::new(target, LifeEventKind::MadeFriend { other: actor }));
            }
            // (A baby in someone's arms is placed by the carry slot.)
            if queue.0.is_empty() && path.is_none() && tsim.age != Age::Baby {
                anim.pose = Pose::Talk;
                if let Some(p) = positions.get(&actor) {
                    let to = Vec2::new(p.0.x - tf.translation.x, p.0.z - tf.translation.z);
                    tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                }
            }
        } else {
            // (A ghost talked to: liking the same both ways, kept by it; turned to whoever's
            // talking.)
            let toward = positions.get(&actor).map(|p| p.0);
            commands.entity(target).queue_silenced(move |mut w: EntityWorldMut| {
                if let Some(mut rels) = w.get_mut::<crate::social::Relationships>() {
                    if let Some(shared) = stc {
                        let r = rels.entry(actor);
                        r.stc = shared.stc;
                        r.friendship = shared.liking;
                        r.enemy = shared.enemy;
                        r.romantic = shared.romantic;
                    }
                    rels.add(actor, friendship, romance);
                }
                if let (Some(at), Some(mut tf)) = (toward, w.get_mut::<Transform>()) {
                    let to = (at - tf.translation).xz();
                    if to.length() > 0.1 {
                        tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                    }
                }
            });
        }
    }
}

/// Revalidate maintenance on every action tick, including object reservations.
/// Watching someone play (the instrument's theirs still, watched by any number); and a grown-up
/// beside the potty chair their toddler's on, or the high chair (it's the toddler's).
fn watches(kind: ObjectKind, def: usize) -> bool {
    interactions_for(kind).get(def).is_some_and(|d| matches!(d.special, Special::WatchMusic | Special::DanceToMusic | Special::PottyTrain | Special::PutInHighChair | Special::GiveBabyFood | Special::TakeOutOfHighChair))
}

fn object_action_available(actor: Entity, kind: ObjectKind, def: usize, owner: Option<Entity>, broken: bool, phase: Phase) -> bool {
    // (A group meal's served one at a time: the rest wait their turn. A picnic table is
    // shared, a place each.)
    !broken && interactions_for(kind).get(def).is_some()
        && (matches!(kind, ObjectKind::PicnicTable | ObjectKind::Chess) || watches(kind, def) || if matches!(phase, Phase::Running(_)) { owner == Some(actor) } else { kind == ObjectKind::Meal || owner.is_none_or(|e| e == actor) })
}

fn maintenance_available(actor: Entity, kind: ObjectKind, owner: Option<Entity>, broken: bool, upgrades: Option<&crate::upgrades::Upgrades>, bit: Option<u8>) -> bool {
    if owner.is_some_and(|e| e != actor) { return false; }
    match bit {
        None => broken,
        // (A doorbell changed: the front door's, checked when it's asked for.)
        Some(bit) if crate::frontdoor::bell_of(bit).is_some() => kind == ObjectKind::Door,
        Some(bit) => crate::upgrades::Upgrade::from_bit(bit).is_some_and(|u| {
            !broken && u.name(kind).is_some() && !upgrades.is_some_and(|installed| installed.has(u))
        }),
    }
}

/// Computer Whiz's TraitTuning duration modifiers apply only to computer work.
fn maintenance_minutes(base: f32, handiness: f32, traits: &[crate::life::Trait], kind: Option<ObjectKind>, upgrade: bool) -> f32 {
    let trait_time = if kind == Some(ObjectKind::Computer) && traits.contains(&crate::life::Trait::ComputerWhiz) {
        if upgrade { 0.75 } else { 0.4 }
    } else { 1.0 };
    base / (1.0 + handiness * 0.35) * trait_time
}

#[cfg(test)]
mod maintenance_tests {
    use super::*;

    #[test]
    fn cancelled_and_rejected_fish_choices_do_not_replace_earlier_choices() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let place = |key: &str, quality| {
            let mut action = Action::new("Place Fish", ActionKind::Object { target, def: 0 }, false);
            action.fish_choice = Some((key.to_string(), quality));
            action
        };
        let mut queue = ActionQueue::default();
        queue.push_player(place("Minnow", 9));
        queue.push_player(place("Goldfish", 3));
        queue.0.pop_back();
        for _ in 1..8 {
            queue.push_player(Action::new("Wait", ActionKind::Object { target, def: 0 }, false));
        }
        queue.push_player(place("Salmon", 5));
        assert_eq!(queue.0.len(), 8);
        assert_eq!(queue.0.front_mut().unwrap().fish_choice.take(), Some(("Minnow".into(), 9)));
        assert!(queue.0.iter().all(|a| a.fish_choice.is_none()));
    }

    #[test]
    fn queued_canvas_sizes_are_consumed_independently() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let mut queue = ActionQueue::default();
        for size in [0, 2, 1] {
            let mut action = Action::new("Paint", ActionKind::Object { target, def: 0 }, false);
            action.canvas_choice = Some(size);
            queue.push_player(action);
        }
        assert_eq!(queue.0.front_mut().unwrap().canvas_choice.take(), Some(0));
        assert_eq!(queue.0[1].canvas_choice, Some(2));
        assert_eq!(queue.0[2].canvas_choice, Some(1));
        queue.0.remove(1); // Cancel the large canvas before it starts.
        queue.0.pop_front();
        assert_eq!(queue.0.front_mut().unwrap().canvas_choice.take(), Some(1));
        assert_eq!(queue.0.front().unwrap().canvas_choice, None);
    }

    #[test]
    fn queued_outfit_choices_remain_independent_when_later_choices_are_cancelled_or_rejected() {
        use crate::simbody::OutfitKind;
        let target = World::new().spawn_empty().id();
        let mut queue = ActionQueue::default();
        for outfit in [OutfitKind::Formal, OutfitKind::Swimwear] {
            let mut action = Action::new("Change Into", ActionKind::Object { target, def: 0 }, false);
            action.outfit_choice = Some(outfit);
            queue.push_player(action);
        }
        assert_eq!(queue.0[0].outfit_choice, Some(OutfitKind::Formal));
        assert_eq!(queue.0[1].outfit_choice, Some(OutfitKind::Swimwear));
        queue.0.pop_back(); // Cancel the later choice.
        for _ in 1..8 {
            queue.push_player(Action::new("Wait", ActionKind::Object { target, def: 0 }, false));
        }
        let mut rejected = Action::new("Change Into", ActionKind::Object { target, def: 0 }, false);
        rejected.outfit_choice = Some(OutfitKind::Outerwear);
        queue.push_player(rejected);
        assert_eq!(queue.0.len(), 8);
        assert_eq!(queue.0.front_mut().unwrap().outfit_choice.take(), Some(OutfitKind::Formal));
        assert!(queue.0.iter().all(|a| a.outfit_choice.is_none()), "cancelled/rejected choices must not leak into another action");
    }

    #[test]
    fn object_actions_revalidate_definition_condition_and_reservation_before_work() {
        let mut world = World::new();
        let actor = world.spawn_empty().id();
        let other = world.spawn_empty().id();
        let def = interactions_for(ObjectKind::Computer).iter().position(|d| d.special == Special::WriteNovel).unwrap();
        assert!(object_action_available(actor, ObjectKind::Computer, def, None, false, Phase::Start));
        for phase in [Phase::Start, Phase::Routing, Phase::Running(300.0)] {
            assert!(object_action_available(actor, ObjectKind::Computer, def, Some(actor), false, phase));
            assert!(!object_action_available(actor, ObjectKind::Computer, def, Some(other), false, phase));
            assert!(!object_action_available(actor, ObjectKind::Computer, def, Some(actor), true, phase));
            assert!(!object_action_available(actor, ObjectKind::Computer, usize::MAX, Some(actor), false, phase));
        }
        assert!(!object_action_available(actor, ObjectKind::Computer, def, None, false, Phase::Running(300.0)), "lost reservation must stop skill and motive gains as well as novel pages");
    }

    #[test]
    fn a_double_bed_has_a_side_for_each_of_two() {
        let mut world = World::new();
        let (a, b, c) = (world.spawn_empty().id(), world.spawn_empty().id(), world.spawn_empty().id());
        // (Anything else, or a single bed, has the one place.)
        assert_eq!(effective_owner(b, Some(a), None), Some(a));
        // A double bed: the second side's free while one has the first.
        assert_eq!(effective_owner(b, Some(a), Some(None)), None);
        assert_eq!(effective_owner(b, Some(a), Some(Some(b))), Some(b));
        assert_eq!(effective_owner(a, Some(a), Some(Some(b))), Some(a));
        // Both sides taken: in use to a third.
        assert_eq!(effective_owner(c, Some(a), Some(Some(b))), Some(a));
        assert_eq!(effective_owner(c, None, Some(Some(b))), None);
    }

    #[test]
    fn a_sofa_seats_two_side_by_side_and_a_nap_takes_all_of_it() {
        let sofa = |half: f32| GameObject { kind: ObjectKind::Sofa, name: "Sofa".into(), objd: (0, 0, 0), price: 1000, center: Vec2::ZERO, half: Vec2::new(half, 0.38), height: 1.0, route: None };
        // (An armchair the one; a loveseat a cushion each; a three-seater the middle and the next.)
        assert!(sofa(0.5).sofa_seats().is_none());
        let [l, r] = sofa(0.9).sofa_seats().unwrap();
        assert!(l < -0.3 && r > 0.3 && (l + r).abs() < 1e-5);
        let [m, n] = sofa(1.4).sofa_seats().unwrap();
        assert!(m == 0.0 && n > 0.7 && n < 1.0);
        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        let (sit, nap) = (0, 1);
        assert_eq!(SOFA[nap].pose, Pose::Lie);
        // Sat on by one: room beside them for another, not to lie along it.
        assert_eq!(owner_for(b, ObjectKind::Sofa, sit, Some(a), Some(None)), None);
        assert_eq!(owner_for(b, ObjectKind::Sofa, nap, Some(a), Some(None)), Some(a));
        assert_eq!(owner_for(b, ObjectKind::Sofa, nap, None, Some(Some(a))), Some(a));
        assert_eq!(owner_for(a, ObjectKind::Sofa, nap, None, None), None);
        // One napping on it has both places.
        assert_eq!(owner_for(b, ObjectKind::Sofa, sit, Some(a), Some(Some(a))), Some(a));
    }

    #[test]
    fn paintings_and_sculptures_are_art_and_tastes_differ() {
        let art = "sims3.gameplay.objects.decorations.mimics.paintingflowers";
        assert_eq!(ObjectKind::from_script(art, "PaintingFlowers"), ObjectKind::Art);
        assert_eq!(ObjectKind::from_script("Sims3.Gameplay.Objects.Decorations.Mimics.SculptureVaseDaffodil", "SculptureVaseDaffodil"), ObjectKind::Art);
        assert_eq!(ObjectKind::from_script("Sims3.Gameplay.Objects.Decorations.Mimics.SculptureWallTowelRing", "SculptureWallTowelRing"), ObjectKind::Decoration);
        let mut sim = crate::sim::random_sim(&mut rand::rng(), "Test", Some(true), Age::Adult);
        sim.traits = vec![crate::life::Trait::CantStandArt];
        assert_eq!(art_taste(&sim, 5000, 0.0), "hate");
        sim.traits = vec![crate::life::Trait::Snob];
        assert_eq!(art_taste(&sim, 100, 0.0), "soso");
        assert_eq!(art_taste(&sim, 2000, 0.9), "love");
        sim.traits = vec![];
        assert_eq!(art_taste(&sim, 100, 0.9), "soso");
        assert_eq!(art_taste(&sim, 600, 0.9), "like");
        let (a, b) = (Entity::from_raw_u32(7).unwrap(), Entity::from_raw_u32(9).unwrap());
        assert_eq!(taste_roll(a, b), taste_roll(a, b));
        assert!((0.0..1.0).contains(&taste_roll(a, b)));
    }

    #[test]
    fn maintenance_rejects_stale_work_and_other_sims_reservations() {
        let mut world = World::new();
        let actor = world.spawn_empty().id();
        let other = world.spawn_empty().id();
        use crate::upgrades::{Upgrade, Upgrades};
        let bit = Upgrade::Improved.bit();
        assert!(maintenance_available(actor, ObjectKind::Computer, None, true, None, None));
        assert!(maintenance_available(actor, ObjectKind::Computer, Some(actor), true, None, None));
        assert!(!maintenance_available(actor, ObjectKind::Computer, Some(other), true, None, None));
        assert!(!maintenance_available(actor, ObjectKind::Computer, Some(actor), false, None, None));
        assert!(maintenance_available(actor, ObjectKind::Computer, Some(actor), false, None, Some(bit)));
        assert!(!maintenance_available(actor, ObjectKind::Computer, Some(other), false, None, Some(bit)));
        assert!(!maintenance_available(actor, ObjectKind::Computer, None, false, Some(&Upgrades(bit)), Some(bit)));
        assert!(!maintenance_available(actor, ObjectKind::Computer, None, true, None, Some(bit)));
        assert!(!maintenance_available(actor, ObjectKind::Computer, None, false, None, Some(255)));
        assert!(!maintenance_available(actor, ObjectKind::Chair, None, false, None, Some(bit)));
    }

    #[test]
    fn computer_chess_teaches_logic_with_keyboard_not_table_animations() {
        let chess = interactions_for(ObjectKind::Computer).iter().find(|d| d.name == "Play Chess").unwrap();
        assert_eq!(chess.skill, Some("Logic"));
        let computer = interaction_clip(chess.name, ObjectKind::Computer).unwrap();
        assert_eq!(computer.loops, &["a2o_computer_chess_type_loop_x"]);
        let table = interaction_clip("Play Chess", ObjectKind::Chair).unwrap();
        assert!(table.loops.iter().all(|s| s.contains("chessTable")));
    }

    #[test]
    fn computer_whiz_stacks_with_handiness_without_speeding_up_other_objects() {
        for handiness in [0.0, 5.0, 10.0] {
            for upgrade in [false, true] {
                let plain = maintenance_minutes(90.0, handiness, &[], Some(ObjectKind::Computer), upgrade);
                let traits = [crate::life::Trait::ComputerWhiz];
                let skilled = maintenance_minutes(90.0, handiness, &traits, Some(ObjectKind::Computer), upgrade);
                assert!((skilled / plain - if upgrade { 0.75 } else { 0.4 }).abs() < 0.0001);
                for kind in [Some(ObjectKind::Tv), Some(ObjectKind::Stereo), Some(ObjectKind::Sink), None] {
                    assert_eq!(maintenance_minutes(90.0, handiness, &traits, kind, upgrade), plain);
                }
            }
        }
    }
}

pub fn hour_label(h: f32) -> String {
    // (Midnight at the day's end too.)
    let h = h as u32 % 24;
    if !crate::clock::TWELVE_HOUR.load(std::sync::atomic::Ordering::Relaxed) {
        return format!("{h}:00");
    }
    match h {
        0 => "12 AM".into(),
        1..=11 => format!("{h} AM"),
        12 => "12 PM".into(),
        _ => format!("{} PM", h - 12),
    }
}

/// How near a stereo someone else must be to be listening to it, and how keen a Frugal Sim is to
/// turn it off when nobody is.
const STEREO_EARSHOT: f32 = 10.0;
const FRUGAL_TURN_OFF: f32 = 18.0;
/// How keen a Frugal Sim is to clip the paper's coupon.
const CLIP_COUPON: f32 = 16.0;

/// Sims with free will pick something to satisfy their lowest needs when idle.
#[allow(clippy::type_complexity)]
fn autonomy(
    delta: Res<SimDelta>,
    clock: Res<GameClock>,
    (settings, household, object_rates): (Res<crate::options::Settings>, Query<Has<crate::sim::Selected>, With<HouseholdMember>>, Res<ObjectRates>),
    mut sims: Query<
        (Entity, &Transform, &Motives, &mut ActionQueue, &mut AutonomyTimer, &Relationships, Option<&Job>, &Sim, Has<SocialPartner>, Option<&crate::visit::OnLot>, Option<&RecentDone>, Option<&crate::life::Moodlets>),
        // (Nor a cook between the stove and serving: their queue's empty a moment, with the meal
        // still in their hands, and they'd set about cooking it again.)
        (
            Without<AtWork>,
            Without<crate::rabbitholes::AtRabbitHole>,
            Without<OffLot>,
            Without<crate::meals::MealRequest>,
            Without<crate::meals::ServingFrom>,
            // (Nor anyone in a panic at a fire, or burning.)
            Without<crate::fire::Panicking>,
            Without<crate::fire::OnFire>,
            // (Nor a little one in someone's arms, or being picked up or put down.)
            Without<crate::little::Carried>,
            Without<crate::carry::BeingMoved>,
        ),
    >,
    (objects, seconds, unmade_q, grime_q, staying_q): (
        Query<(Entity, &GameObject, &Transform, &UsedBy, Option<&crate::visit::LotObject>)>,
        Query<&SecondPlace>,
        Query<(), With<Unmade>>,
        Query<&crate::grime::Grime>,
        Query<(), With<crate::guests::StayingOver>>,
    ),
    hh: Option<Res<Household>>,
    (broken, plant_q, lit_q, hw_q, stereos_on, coupons): (
        Query<(), With<Broken>>,
        Query<&crate::gardening::GrowingPlant>,
        Query<(), With<crate::fireplace::Lit>>,
        Query<(), With<crate::rabbitholes::Homework>>,
        Query<(), With<crate::music::StereoOn>>,
        Res<crate::coupons::Coupons>,
    ),
    (party_on, trash_q, served_q, leftovers, sprinkling, route_failed): (
        Option<Res<Party>>,
        Query<&crate::surroundings::TrashFill>,
        Query<&crate::surroundings::ServedAt>,
        Res<crate::meals::Leftovers>,
        Query<(), With<crate::gardening::Sprinkling>>,
        Query<&RouteFailed>,
    ),
    (called, repairmen, laundry, privacy): (Option<Res<RepairmanVisit>>, Query<(), With<crate::services::Repairman>>, Res<crate::laundry::Laundry>, Res<crate::privacy::Privacy>),
    friends_away: Query<(Entity, &Sim), (With<OffLot>, Without<Invited>)>,
    (callers, residents, mut solve, (cleaners, mut commands)): (Query<(Entity, &crate::doorbell::AtTheDoor)>, Query<&crate::homes::Resident>, ResMut<crate::livehud::SolveNeed>, (Query<(), With<crate::traitacts::CleaningHouse>>, Commands)),
    (weather, building, chess, music, potties, swipeable, looked): (
        Res<crate::weather::Weather>,
        Option<Res<crate::building::ActiveBuilding>>,
        Res<crate::chess::ChessTables>,
        Res<crate::tips::Performances>,
        Res<crate::potty::Potties>,
        Res<crate::klepto::Swipeable>,
        Res<crate::mirrors::LastLooked>,
    ),
    (convs, toilets, bed_owners, bowls, (moods, smashed_q), (residents_q, can_owners, tipped_q, visited_lot, work_to_do_q, fridge_keeps, left_on_easel)): (
        Res<crate::groups::Conversations>,
        Res<crate::toilets::Unflushed>,
        Query<&crate::bedowners::BedOwners>,
        Query<&crate::fishbowl::BowlFish>,
        (Query<&crate::life::Mood>, Query<(), With<crate::dollhouse::Smashed>>),
        (
            Query<(), With<crate::homes::Resident>>,
            Res<crate::trashcans::CanOwners>,
            Query<(), With<crate::trashcans::TippedOver>>,
            Option<Res<crate::visit::VisitedLot>>,
            Query<&crate::computertraits::WorkToDo>,
            Res<crate::meals::FridgeKeeps>,
            Query<&crate::paintings::LeftOnEasel>,
        ),
    ),
) {
    if delta.0 <= 0.0 {
        return;
    }
    // (In rain or hail, what's out in it is left alone: `kRainHailObjectAutonomyScoringMultiplier`.)
    let rained_on = weather.raining() && weather.falling(clock.minutes) > 0.2;
    // (The repairman's been called: repairs are left to him.)
    let repairman = called.is_some() || !repairmen.is_empty();
    let others: Vec<(Entity, Vec3, Age, [f32; 6], Option<usize>)> = sims.iter().map(|s| (s.0, s.1.translation, s.7.age, s.2.0, s.9.map(|l| l.0))).collect();
    // (Sims' ids, for whose bed's whose.)
    let ids: HashMap<Entity, u64> = sims.iter().map(|s| (s.0, s.7.id)).collect();
    let bed_owner_ids: std::collections::HashSet<u64> = bed_owners.iter().flat_map(|o| o.0.into_iter().flatten()).collect();
    // A meal already out is eaten before anyone cooks another.
    let meal_out = objects.iter().any(|(_, o, ..)| o.kind == ObjectKind::Meal);
    // (Bills to pay that the household can pay: left alone on their own when it can't.)
    let bills_due = hh.is_some_and(|h| !h.bills.is_empty() && h.funds >= h.bills.iter().map(|b| b.amount).sum::<i64>());
    let mut rng = rand::rng();
    // Callers at the door, and who's already going to let them in.
    let greet = crate::social::social_index("Greet");
    let mut greeted: Vec<Entity> = sims
        .iter()
        .flat_map(|s| s.3.0.iter().filter_map(|a| match a.kind {
            ActionKind::Social { target, social } if Some(social) == greet => Some(target),
            _ => None,
        }))
        .collect();
    for (me, tf, motives, mut queue, mut timer, rels, job, sim, partner, on_lot, recent, moodlets) in &mut sims {
        let my_lot = on_lot.map(|l| l.0);
        // (A caller at the door waits there to be let in.)
        if callers.contains(me) {
            continue;
        }
        timer.0 -= delta.0;
        // (A moodlet clicked: what helps that need, now, whatever their free will: `livehud`.)
        let solving = solve.0.filter(|s| s.0 == me).map(|s| s.1);
        if solving.is_some() {
            solve.0 = None;
        }
        // (A Neat Sim cleaning house: their next chore, once the last's done (`traitacts`); not if
        // they've been given something else to do meanwhile.)
        let cleaning = cleaners.contains(me);
        if cleaning && !queue.0.is_empty() {
            // (Its Clean House still finishing: waited for.)
            if queue.0.iter().any(|a| !matches!(a.kind, ActionKind::TraitAct { act: crate::traitacts::TraitAct::CleanHouse, .. })) {
                commands.entity(me).remove::<crate::traitacts::CleaningHouse>();
            }
            continue;
        }
        // (Someone else's social partner waits for them to finish.)
        if solving.is_none() && !cleaning && (timer.0 > 0.0 || !queue.0.is_empty() || sim.age == Age::Baby || partner) {
            continue;
        }
        let free_will = settings.free_will_of(household.contains(me), household.get(me).unwrap_or(false));
        timer.0 = match free_will {
            crate::options::FreeWill::High => rng.random_range(2.0..5.0),
            _ => rng.random_range(4.0..10.0),
        };
        if free_will == crate::options::FreeWill::Off && solving.is_none() && !cleaning {
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
        // (A need gone critical comes first: chores and pastimes that do nothing for it wait. Those
        // that fail (hunger, bladder, energy: starving, an accident, passing out) before hygiene,
        // which doesn't: an exhausted Sim goes to bed before the bath.)
        let failing = [HUNGER, BLADDER, ENERGY].into_iter().filter(|&i| motives.0[i] < CRITICAL_NEED).min_by(|a, b| motives.0[*a].total_cmp(&motives.0[*b]));
        let critical = solving.or(failing).or_else(|| (motives.0[HYGIENE] < CRITICAL_NEED).then_some(HYGIENE));
        // (A guest at the household's home keeps to what their footing with its friendliest
        // allows: see `guests`.)
        let guest = (my_lot.is_none() && !household.contains(me)).then(|| {
            if staying_q.contains(me) {
                crate::guests::STAY_OVER_PRIVILEGE
            } else {
                others.iter().filter(|o| household.contains(o.0)).filter_map(|o| rels.0.get(&o.0)).map(|r| crate::guests::privilege(r.state())).reduce(f32::max).unwrap_or(1.0)
            }
        });
        for (oe, obj, otf, used, obj_lot) in &objects {
            // (A double bed with their partner in it has room for them, on its other side; a sofa
            // someone's sat on, for anyone.)
            let beside_partner = seconds.get(oe).is_ok_and(|s| s.0.is_none()) && used.0.is_some() && (used.0 == rels.partner().map(|p| p.0) || obj.sofa_seats().is_some());
            // (Someone else's: only to watch them play it.)
            let theirs = used.0.is_some_and(|u| u != me) && !beside_partner;
            // (A sofa shared: only sat on.)
            let shared = obj.sofa_seats().is_some() && (used.0.is_some_and(|u| u != me) || seconds.get(oe).is_ok_and(|s| s.0.is_some_and(|u| u != me)));
            if (theirs && !music.watchable(oe, me)) || obj_lot.map(|l| l.0) != my_lot {
                continue;
            }
            // (Not in a bathroom someone else is using.)
            if privacy.objects.get(&oe).is_some_and(|u| *u != me) {
                continue;
            }
            if !obj.kind.suits(sim) {
                continue;
            }
            if rained_on && !crate::weather::sheltered(building.as_deref(), otf.translation) {
                continue;
            }
            let dist = obj.world_center(otf).distance(tf.translation);
            if broken.contains(oe) {
                if cleaning || used.0.is_some_and(|u| u != me) {
                    continue;
                }
                // Handy (or neat) household Sims see to repairs on their own; and anyone who
                // badly needs the bathroom unclogs the toilet (the plunger needs no skill).
                let keen = sim.traits.contains(&crate::life::Trait::Handy) || sim.traits.contains(&crate::life::Trait::Neat);
                let desperate = obj.kind == ObjectKind::Toilet && motives.0[BLADDER] < -20.0;
                // (A leak's fixed before its puddles are mopped, by anyone grown now and then.)
                let leaking = crate::puddles::spills(obj.kind);
                // (At home: the town's own things are the town's to fix.)
                if household.contains(me) && my_lot.is_none() && sim.age.is_grown() && sim.age != Age::Child && (keen || desperate || leaking) && !repairman {
                    let score = if desperate {
                        urgency(BLADDER) * 40.0
                    } else if leaking {
                        if keen { 35.0 } else { 10.0 }
                    } else {
                        25.0
                    } / (1.0 + dist / 25.0);
                    if best.as_ref().is_none_or(|b| score > b.0) {
                        best = Some((score, Action::new(repair_of(obj.kind).0, ActionKind::Repair { target: oe }, true)));
                    }
                }
                continue;
            }
            for (di, d) in interactions_for(obj.kind).iter().enumerate() {
                if guest.is_some_and(|p| crate::guests::minds(p, crate::guests::check(obj.kind, d))) {
                    continue;
                }
                if cleaning && !crate::traitacts::cleans(d.special) {
                    continue;
                }
                // (A stereo playing to nobody else: off with it, for the Frugal and the
                // Environmentally Conscious, the game's `FrugalTurnOff`.)
                if d.special == Special::StereoOff
                    && stereos_on.contains(oe)
                    && (sim.traits.contains(&crate::life::Trait::Frugal) || sim.traits.contains(&crate::life::Trait::EnvironmentallyConscious))
                    && !others.iter().any(|o| o.0 != me && o.1.distance(otf.translation) < STEREO_EARSHOT)
                {
                    let score = FRUGAL_TURN_OFF / (1.0 + dist / 25.0);
                    if best.as_ref().is_none_or(|b| score > b.0) {
                        best = Some((score, Action::new(d.name, ActionKind::Object { target: oe, def: di }, true)));
                    }
                    continue;
                }
                if !d.autonomous || (shared && whole_of(obj.kind, di)) {
                    continue;
                }
                // (Chores, skills and work not while they're in no state for them: `stress`.)
                if crate::stress::is_stress_exit(d) && (moods.get(me).is_ok_and(|m| m.0 < -50.0) || crate::stress::low_need(motives, &d.per_hour, false).is_some()) {
                    continue;
                }
                // (The paper's coupon: the Frugal's to clip, once.)
                if d.special == Special::ClipCoupon && !coupons.offers(sim, oe) {
                    continue;
                }
                // (A potty chair: sat on by toddlers trained to, emptied by grown-ups once used.)
                if obj.kind == ObjectKind::PottyChair
                    && (!crate::potty::suits(d, sim) || (d.special == Special::None && !potties.trained.contains(&me)) || (d.special == Special::EmptyPotty && !potties.needs_emptying(oe)))
                {
                    continue;
                }
                // (Someone playing: watched; played for tips only outdoors or on a community lot.)
                let audience = matches!(d.special, Special::WatchMusic | Special::DanceToMusic);
                if (theirs && !audience) || (d.special == Special::WatchMusic && !music.watchable(oe, me)) || (d.special == Special::DanceToMusic && !music.danceable(oe, me)) || (d.special == Special::PlayForTips && !music.tips_ok(oe)) {
                    continue;
                }
                // (Chess from a free chair at the table; a game watched only while there's one.)
                if obj.kind == ObjectKind::Chess
                    && ((d.special == Special::PlayChess && (chess.free_chairs(oe) == 0 || chess.playing(me))) || (d.special == Special::WatchChess && !chess.watchable(oe, me)))
                {
                    continue;
                }
                // Guests don't cook or do the chores.
                // A fireplace is lit when cold, and warmed by when lit.
                if matches!(obj.kind, ObjectKind::Fireplace | ObjectKind::FirePit | ObjectKind::Candle) && d.special != Special::NeuroticCheck && (d.special == Special::LightFire) == lit_q.contains(oe) {
                    continue;
                }
                // (A bubble bath with bubble bath in the house.)
                if d.special.needs().is_some_and(|k| !objects.iter().any(|(_, o, ..)| o.kind == k)) {
                    continue;
                }
                // (At a mirror, what's theirs: the Snobs admire themselves, children make faces.)
                if !crate::mirrors::offers(d.special, sim) {
                    continue;
                }
                // (A Neurotic's own checks.)
                if !crate::neurotic::offers(d.special, sim) {
                    continue;
                }
                // (Never by choice to the TV or a computer for a Technophobe, but to turn it off;
                // nor into the water for the Hydrophobic: the game's `TraitAntiTV` commodity, and
                // its pool's and diving board's tests.)
                if sim.traits.contains(&crate::life::Trait::Technophobe) && (matches!(obj.kind, ObjectKind::Tv | ObjectKind::Computer) || d.name == "Watch the TV") && d.name != "Turn Off" {
                    continue;
                }
                if sim.traits.contains(&crate::life::Trait::Hydrophobic) && (d.name == "Swim" || obj.kind == ObjectKind::DivingBoard) {
                    continue;
                }
                // (Strength training's for teens and up, as a sponge bath is.)
                if matches!(d.special, Special::StrengthWorkOut | Special::SpongeBath) && !(sim.age == Age::Teen || sim.age.is_grown()) {
                    continue;
                }
                // (Plasma for vampires, and vampires drink it rather than eat.)
                if (d.special == Special::VampireOnly) != (sim.occult == Some(crate::sim::Occult::Vampire)) && (d.special == Special::VampireOnly || d.per_hour[0] > 50.0) {
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
                // (Not another's unfinished painting to paint over.)
                if d.special == Special::SellPainting && left_on_easel.get(oe).is_ok_and(|l| l.painter != sim.id) {
                    continue;
                }
                // (Spoiled food cleaned out of the household's fridge: by teens and up, the Neat keen
                // to, as the game's `TraitNeat` 200.)
                if d.special == Special::CleanOutFridge {
                    let grown = sim.age == Age::Teen || (sim.age.is_grown() && sim.age != Age::Child);
                    if !household.contains(me) || my_lot.is_some() || !grown || !leftovers.any_spoiled(clock.minutes, fridge_keeps.0) {
                        continue;
                    }
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
                // (And what it does for the other needs only lasts as long as it does; on this
                // object, as the game tunes it.)
                let per_hour = object_rates.per_hour(d, obj.objd);
                let minutes = match d.until_full {
                    Some(m) if per_hour[m] > 0.0 => d.minutes.min((100.0 - motives.0[m]).max(0.0) / (per_hour[m] / 60.0)),
                    _ => d.minutes,
                };
                let mut score = 0.0;
                for i in 0..6 {
                    let gain = per_hour[i] * minutes / 60.0;
                    let room = (100.0 - motives.0[i]).max(0.0);
                    score += gain.min(room).max(-50.0) * urgency(i);
                }
                // (Someone at the chess table alone: a game with them, the more inviting, as the
                // game's `IncreaseScoreIfSomeonePlayingAtTheTable`.)
                if d.special == Special::PlayChess && chess.players(oe).len() == 1 && !sim.traits.contains(&crate::life::Trait::Loner) {
                    score *= 2.0;
                }
                // Sleep and naps as the game's: to bed when sleepy, or at bedtime once tired enough;
                // a nap only when sleepy, and not this near waking.
                let waking = hours_until_waking(clock.hour_f(), job);
                if d.until_full == Some(ENERGY) && !may_sleep(motives.0[ENERGY], waking) {
                    continue;
                }
                if d.name == "Nap" && !may_nap(motives.0[ENERGY], waking) {
                    continue;
                }
                // (Not in a bed that's someone else's on every side: the game's `BedSleep` test.)
                if d.until_full == Some(ENERGY)
                    && let Ok(owners) = bed_owners.get(oe)
                    && !owners.lets(obj.kind, sim.id, rels.partner().and_then(|p| ids.get(&p.0).copied()))
                {
                    continue;
                }
                // Homework gets done (by the bookish sooner), unless they're miserable.
                if d.special == Special::Homework && motives.0.iter().all(|m| *m > -30.0) {
                    let keen = [crate::life::Trait::Bookworm, crate::life::Trait::Genius, crate::life::Trait::Perfectionist].iter().any(|t| sim.traits.contains(t));
                    score = if keen { 45.0 } else { 22.0 };
                }
                // (A Green Thumb talks to their plants now and then, as the game's `TalkToPlant` advertises.)
                if d.special == Special::TalkToPlant {
                    let ok = household.contains(me) && plant_q.get(oe).is_ok_and(|p| crate::gardening::offers(p, d.special)) && crate::gardening::sim_offers(sim, 0, d.special);
                    score = if ok { 12.0 } else { 0.0 };
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
                // (A dollhouse: played with whole; smashed by the bad-tempered in a temper.)
                if obj.kind == ObjectKind::DollHouse {
                    let smashed = smashed_q.contains(oe);
                    if smashed || (d.special == Special::DisposeDollhouse) || (d.special == Special::None && !crate::dollhouse::may_play(sim)) {
                        continue;
                    }
                    if d.special == Special::SmashDollhouse {
                        let Some(pull) = crate::dollhouse::drawn(sim, moods.get(me).map_or(0.0, |m| m.0)) else { continue };
                        score = pull;
                    }
                }
                if d.special == Special::CleanOutFridge {
                    score = if sim.traits.contains(&crate::life::Trait::Neat) { 60.0 } else { 8.0 } * rng.random_range(0.8..1.2);
                }
                // (A Workaholic works from home out of work hours, keen to: the game's `WorkFromHome`.)
                if d.special == Special::WorkFromHome {
                    let to_do = work_to_do_q.get(me).is_ok_and(|w| w.0 > clock.minutes);
                    if !household.contains(me) || !crate::computertraits::may_work_from_home(sim, job, to_do, &clock) {
                        continue;
                    }
                    score = crate::computertraits::WORK_FROM_HOME_PULL * rng.random_range(0.8..1.2);
                }
                // (An outdoor trash can: knocked over by those it's for; set up again by its family,
                // the Neat keen to.)
                if obj.kind == ObjectKind::OutdoorTrashCan {
                    let owner = can_owners.0.get(&oe).cloned().unwrap_or_default();
                    let home = crate::trashcans::home_of(household.contains(me), residents_q.contains(me), visited_lot.as_ref().map(|v| v.lot));
                    let at_home = home == Some(owner.lot);
                    let tipped = tipped_q.contains(oe);
                    match d.special {
                        Special::KnockOverCan if tipped || !crate::trashcans::may_knock_over(sim, at_home, crate::trashcans::enemy_of(rels, &owner)) => continue,
                        Special::PickUpCan if !tipped || !crate::trashcans::may_pick_up(sim, at_home) => continue,
                        Special::PickUpCan => score = crate::trashcans::pick_up_pull(sim) * rng.random_range(0.8..1.2),
                        _ => {}
                    }
                }
                // (A hungry fish fed; none other's.)
                if d.special == Special::FeedFish {
                    if !bowls.get(oe).is_ok_and(|b| b.hungry(clock.minutes)) || !household.contains(me) {
                        continue;
                    }
                    score = 18.0;
                }
                // (A toilet left unflushed: flushed by anyone but a Slob, the Neat sooner.)
                if d.special == Special::Flush {
                    let Some(pull) = crate::toilets::drawn(sim).filter(|_| toilets.0.contains_key(&oe)) else { continue };
                    score = pull;
                }
                // (A Neurotic checks what they're anxious about; now and then otherwise.)
                if d.special == Special::NeuroticCheck {
                    let Some(pull) = crate::neurotic::drawn(obj.kind, moodlets) else { continue };
                    score = pull;
                }
                // (A mirror draws the vain and the childish, as their traits have it.)
                if matches!(d.special, Special::AdmireSelf | Special::GussyUp | Special::PlayWithMirror | Special::CheckSelfOut) {
                    // (Their pull met for a while once they've looked.)
                    score += crate::mirrors::drawn(d.special, sim).filter(|_| looked.due(me, clock.minutes)).unwrap_or(0.0);
                }
                // The laundry, when there's some to do: a chore like the dishes.
                if matches!(d.special, Special::PickUpClothes | Special::DoLaundry | Special::EmptyHamper | Special::MoveLaundry | Special::LoadDryer | Special::EmptyDryer) {
                    // (Grown-ups' work: the game's clips are theirs.)
                    if !household.contains(me) || !sim.age.is_grown() || sim.age == Age::Child || used.0.is_some() || !laundry.offers(d.special, obj.kind, oe) || !laundry.worth_doing(d.special, clock.minutes) {
                        continue;
                    }
                    trace!("{} could {} ({} waiting, {} piles)", sim.first, d.name, laundry.waiting, laundry.piles);
                    score = if sim.traits.contains(&crate::life::Trait::Slob) {
                        0.0
                    } else if sim.traits.contains(&crate::life::Trait::Neat) {
                        26.0
                    } else if d.special == Special::PickUpClothes {
                        3.0
                    } else {
                        7.0
                    };
                }
                // A full trash can is a chore like dishes; an emptier one isn't.
                if d.special == Special::EmptyTrash && !trash_q.get(oe).is_ok_and(|f| f.0 >= crate::surroundings::trash_capacity(obj.kind)) {
                    continue;
                }
                if d.special == Special::Scrub {
                    let Some(g) = grime_q.get(oe).ok().filter(|g| g.needs_cleaning()) else { continue };
                    if used.0.is_some() {
                        continue;
                    }
                    let filthy = g.0 >= 60.0;
                    score = if sim.traits.contains(&crate::life::Trait::Slob) {
                        0.0
                    } else if sim.traits.contains(&crate::life::Trait::Neat) {
                        28.0
                    } else if filthy {
                        8.0
                    } else {
                        1.5
                    };
                }
                // (Puddles mopped: by Neat Sims as soon as they see one, others now and then,
                // slobs never.)
                if d.special == Special::MopUp {
                    if used.0.is_some() {
                        continue;
                    }
                    score = if sim.traits.contains(&crate::life::Trait::Slob) {
                        0.0
                    } else if sim.traits.contains(&crate::life::Trait::Neat) {
                        30.0
                    } else {
                        4.0
                    };
                }
                // (A used potty chair emptied as a chore, the game's environment penalty till then.)
                if d.special == Special::EmptyPotty {
                    score = if sim.traits.contains(&crate::life::Trait::Slob) {
                        0.0
                    } else if sim.traits.contains(&crate::life::Trait::Neat) {
                        26.0
                    } else {
                        9.0
                    };
                }
                if d.special == Special::MakeBed {
                    if !unmade_q.contains(oe) || used.0.is_some() {
                        continue;
                    }
                    score = if sim.traits.contains(&crate::life::Trait::Slob) {
                        0.0
                    } else if sim.traits.contains(&crate::life::Trait::Neat) {
                        24.0
                    } else {
                        1.5
                    };
                }
                if d.special == Special::ClipCoupon {
                    score = CLIP_COUPON;
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
                // (A bed of their own: any other's hardly thought of, the game's tuned
                // `kAutonomyScoreMultiplierForBedAwayFromOwnedBed`.)
                if d.until_full == Some(ENERGY) && bed_owner_ids.contains(&sim.id) && !bed_owners.get(oe).is_ok_and(|o| o.owns(sim.id)) {
                    score *= crate::bedowners::AWAY_FROM_OWN_BED;
                }
                score /= 1.0 + dist / 25.0;
                score *= rng.random_range(0.85..1.15);
                // (Not the same pastime again and again.)
                if d.per_hour[FUN] > 0.0
                    && let Some(r) = recent
                {
                    score *= RECENT_KEEN.powi(r.times(d.name, clock.minutes) as i32);
                }
                if critical.is_some_and(|m| d.per_hour[m] <= 0.0) {
                    score *= if solving.is_some() { 0.0 } else { 0.15 };
                }
                // (Cleaning house: the nearest thing to clean, as the game's
                // `FindNearestObjectToClean`, whatever else they'd rather.)
                if cleaning {
                    score = if score > 0.0 { 1000.0 / (1.0 + dist) } else { 0.0 };
                }
                if best.as_ref().is_none_or(|b| score > b.0) {
                    let label = if d.special == Special::PlayForTips { crate::tips::PLAY_FOR_TIPS } else { d.name };
                    best = Some((score, Action::new(label, ActionKind::Object { target: oe, def: di }, true)));
                }
            }
        }
        // (Cleaning house: that chore, then Clean House again, as the game's continuation; nothing
        // left, they're done.)
        if cleaning {
            commands.entity(me).remove::<crate::traitacts::CleaningHouse>();
            if let Some((score, mut action)) = best.take()
                && score > 0.0
            {
                action.autonomous = false;
                queue.0.push_back(action);
                queue.0.push_back(crate::traitacts::TraitAct::CleanHouse.action(false));
            }
            continue;
        }
        // (Solving a need other than Social: only what the lot's objects offer for it.)
        if let Some(need) = solving.filter(|n| *n != SOCIAL) {
            match best {
                Some((score, mut action)) if score > 0.0 && satisfies_kind(&action.kind, need, &objects) => {
                    action.autonomous = false;
                    queue.0.push_back(action);
                }
                _ => debug!("{} found nothing to see to need {need}", sim.first),
            }
            continue;
        }
        // Someone at the door of a home visited is let in by one of the family living there (the
        // player's household never invites anyone in of its own accord: the game's
        // `NotAutonomousForSelectableSims`, "Invite Inside" is the player's to choose).
        let at_home = residents.get(me).is_ok_and(|r| Some(r.0) == my_lot);
        if at_home && sim.age.is_grown() && sim.age != Age::Child
            && let Some(social) = greet
            && let Some((caller, _)) = callers.iter().find(|(c, d)| d.since.is_some() && !greeted.contains(c))
        {
            greeted.push(caller);
            best = Some((80.0, Action::new("Greet", ActionKind::Social { target: caller, social }, true)));
        }
        // What they do for who they are (`traitacts`): the Insane talk to themselves, the
        // Neurotic freak out, a Natural-Born Performer's TaDa! (with someone about to see it).
        let audience = others.iter().any(|o| o.0 != me && o.4 == my_lot && !o.2.is_little() && o.1.distance(tf.translation) < 8.0);
        let gain = |per_hour: [f32; 6], minutes: f32| (0..6).map(|i| (per_hour[i] * minutes / 60.0).min((100.0 - motives.0[i]).max(0.0)).max(-50.0) * urgency(i)).sum::<f32>();
        if let Some((score, act)) = crate::traitacts::autonomous(sim, moodlets, gain, audience)
            && best.as_ref().is_none_or(|b| score > b.0)
        {
            best = Some((score * rng.random_range(0.85..1.15), act.action(true)));
        }
        // A Kleptomaniac out after dark swipes something when no one's looking (the game's
        // `SwipeSomethingAutonomous`, its urge strongly advertised).
        if let Some((at, level)) = swipeable.0.get(&me)
            && best.as_ref().is_none_or(|b| crate::klepto::SWIPE_SCORE > b.0)
        {
            best = Some((crate::klepto::SWIPE_SCORE * rng.random_range(0.85..1.15), crate::klepto::swipe_action(*at, *level, true)));
        }
        // An athletic Sim goes jogging now and then (by day, at home, with the energy for it).
        if household.contains(me)
            && my_lot.is_none()
            && crate::jog::can_jog(sim)
            && sim.traits.contains(&crate::life::Trait::Athletic)
            && (7.0..20.0).contains(&clock.hour_f())
            && motives.0[ENERGY] > 10.0
            && motives.0[HYGIENE] > 0.0
        {
            let score = (12.0 + urgency(FUN) * 30.0) * rng.random_range(0.85..1.15);
            if best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((score, Action::new("Go Jogging", ActionKind::Jog { home: Vec2::ZERO }, true)));
            }
        }
        let social_need = if sim.traits.contains(&crate::life::Trait::Loner) { 0.0 } else { 30.0 };
        let social_need = if sim.traits.contains(&crate::life::Trait::PartyAnimal) || sim.traits.contains(&crate::life::Trait::Friendly) { 50.0 } else { social_need };
        // A little one in need comes first for the teens and grown-ups (the game's
        // `LookAfterBabyOrToddler`, every teen and older of the household's: not a visitor's).
        if crate::carry::can_carry(sim.age) && household.contains(me) {
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
                    // (A little one in real need comes before almost anything: the game's household
                    // motive `LookAfterBabyOrToddler` is held at its worst with an intensity of
                    // 10000.)
                    let urgent = if v < CARE_URGENT { CARE_URGENT_SCORE } else { 0.0 };
                    let mut score = ((-v / 10.0).powi(2) + urgent) / (1.0 + pos.distance(tf.translation) / 25.0);
                    if sim.traits.contains(&crate::life::Trait::FamilyOriented) {
                        score *= 1.5;
                    }
                    if best.as_ref().is_none_or(|b| score > b.0) {
                        best = Some((score, Action::new(crate::social::social(si).name, ActionKind::Social { target: other, social: si }, true)));
                    }
                }
            }
        }
        // At a party everyone mingles.
        let partying = party_on.as_deref().is_some_and(|p| p.on()) && sim.age.is_grown();
        if (motives.0[SOCIAL] < social_need || partying) && !sim.age.is_little() {
            for &(other, pos, age, _, lot) in &others {
                if other == me || age.is_little() || lot != my_lot {
                    continue;
                }
                if partying && pos.distance(tf.translation) > 20.0 {
                    continue;
                }
                let rel = rels.get(other);
                // (Someone they barely know: getting to know them; else whatever the game would
                // have them say, as things stand between them, in the tone they're after.)
                let pick = if rel.friendship < 5.0 && rng.random_bool(0.5) { None } else { crate::social::pick_autonomous(sim, age, &rel, &mut rng) };
                let si = pick.or_else(|| crate::social::social_index(if rel.friendship < 5.0 { "Get to Know" } else { "Chat" })).unwrap_or(0);
                let s = crate::social::social(si);
                let mut score = (s.social_per_hour * s.minutes / 60.0) * urgency(SOCIAL) * rng.random_range(0.8..1.2);
                if partying {
                    score = score.max(30.0 * rng.random_range(0.8..1.2));
                }
                if best.as_ref().is_none_or(|b| score > b.0) {
                    best = Some((score, Action::new(s.name, ActionKind::Social { target: other, social: si }, true)));
                }
            }
        }
        // In a conversation, they keep it going among themselves a while (the game's 2 to 4
        // socials before they think of leaving), with whoever in it wasn't just talking.
        // (Not while something's pressing: a need first.)
        if let Some(g) = convs.group(me).filter(|g| g.socials_left > 0 && !sim.traits.contains(&crate::life::Trait::Loner) && motives.0.iter().all(|m| *m > -20.0)) {
            for &other in g.members.iter().filter(|m| **m != me) {
                // (Not the one just talking, when there's someone else in it to turn to.)
                if g.members.len() > 2 && g.talk.is_some_and(|(a, b)| a == other || b == other) {
                    continue;
                }
                let Some(&(_, _, age, _, _)) = others.iter().find(|o| o.0 == other) else { continue };
                let rel = rels.get(other);
                let si = crate::social::pick_autonomous(sim, age, &rel, &mut rng).or_else(|| crate::social::social_index("Chat")).unwrap_or(0);
                let s = crate::social::social(si);
                let score = 28.0 * rng.random_range(0.8..1.2);
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
        // (Solving a need: only what sees to it, as the Sim's own choice, after what's queued.)
        if let Some(need) = solving {
            match best {
                Some((score, mut action)) if score > 0.0 && satisfies_kind(&action.kind, need, &objects) => {
                    action.autonomous = false;
                    queue.0.push_back(action);
                }
                _ => debug!("{} found nothing to see to need {need}", sim.first),
            }
            continue;
        }
        if let Some((score, action)) = best
            && score > 2.0
        {
            queue.0.push_back(action);
        }
    }
}

/// Whether an action sees to a need (an object's interaction that gives it, a social or a call
/// for Social, a jog for Fun).
fn satisfies_kind(kind: &ActionKind, need: usize, objects: &Query<(Entity, &GameObject, &Transform, &UsedBy, Option<&crate::visit::LotObject>)>) -> bool {
    match kind {
        ActionKind::Object { target, def } => objects.get(*target).ok().and_then(|o| interactions_for(o.1.kind).get(*def)).is_some_and(|d| d.per_hour[need] > 0.0),
        ActionKind::Social { .. } | ActionKind::PhoneChat { .. } => need == SOCIAL,
        ActionKind::Jog { .. } => need == crate::sim::FUN,
        _ => false,
    }
}

/// Whether a queued action is seeing to a need already, by what it's called (the moodlet's click
/// leaves it be: `livehud`).
pub fn satisfies(a: &Action, need: usize) -> bool {
    let words = a.label.to_ascii_lowercase();
    let by: &[&str] = match need {
        HUNGER => &["eat", "meal", "snack", "dinner", "lunch", "breakfast", "cook", "serve"],
        BLADDER => &["toilet", "pee"],
        ENERGY => &["sleep", "nap", "bed", "relax"],
        SOCIAL => &["chat", "talk", "call"],
        HYGIENE => &["shower", "bath", "wash", "clean up", "brush"],
        _ => &["play", "watch", "fun", "dance", "jog"],
    };
    by.iter().any(|w| words.contains(w))
}

/// One talked to whose other is no longer at it with them (gone home, called away, onto
/// something else without finishing) is free again.
fn release_partners(mut commands: Commands, partners: Query<(Entity, &SocialPartner)>, actors: Query<&ActionQueue>) {
    for (e, p) in &partners {
        let actor = p.0;
        let at_it = actors.get(actor).is_ok_and(|q| q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Social { target, .. } if target == e)));
        if !at_it {
            debug!("{e} let go: {actor} is no longer at the social with them");
            commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| {
                if w.get::<SocialPartner>().is_some_and(|p| p.0 == actor) {
                    w.remove::<(SocialPartner, crate::anim::ActionClip)>();
                }
            });
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

/// What an object has lost of its price since the household bought it (the game's
/// depreciation): 15% once bought, 10% of its price each day after, and at most 40%.
#[derive(Component, Clone, Copy, Default, Debug, PartialEq)]
pub struct Depreciated(pub i32);

const INITIAL_DEPRECIATION: f32 = 0.15;
const DAILY_DEPRECIATION: f32 = 0.1;
const DEPRECIATION_LIMIT: f32 = 0.4;

impl Depreciated {
    /// Just bought (once the shopping's done).
    pub fn initial(price: i32) -> Self {
        Self((price as f32 * INITIAL_DEPRECIATION) as i32).limited(price)
    }

    /// A day older.
    pub fn day_older(self, price: i32) -> Self {
        Self(self.0 + ((price as f32 * DAILY_DEPRECIATION) as i32).max(1)).limited(price)
    }

    fn limited(self, price: i32) -> Self {
        Self(self.0.min((price as f32 * DEPRECIATION_LIMIT) as i32))
    }
}

/// What an object is worth now: its price less what it's lost, a painting what it's valued at,
/// and nothing once burnt (the game's `GameObject.Value`).
pub fn object_value(o: &GameObject, lost: Option<&Depreciated>, charred: bool, hung: Option<&crate::paintings::Hung>) -> i64 {
    if charred {
        return 0;
    }
    if let Some(h) = hung {
        return h.0.each();
    }
    (o.price - lost.map_or(0, |d| d.0)).max(0) as i64
}

/// The day things last lost worth (none yet since loading: a restored date isn't a midnight).
#[derive(Resource, Default)]
pub struct DepreciationDay(pub Option<u32>);

/// At midnight everything on the home lot is worth a little less (the game's
/// `AppreciationDepreciationCallback`).
fn depreciate(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut last: ResMut<DepreciationDay>,
    household: Option<Res<Household>>,
    mut objects: Query<
        (Entity, &GameObject, Option<&mut Depreciated>),
        (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>, Without<crate::paintings::Hung>),
    >,
) {
    let day = clock.day();
    let before = *last.0.get_or_insert(day);
    if day <= before {
        last.0 = Some(day);
        return;
    }
    last.0 = Some(day);
    if household.is_none() {
        return;
    }
    // (Days slept through count too, though no more than it takes to reach the limit.)
    let days = (day - before).min(5);
    let mut total = 0;
    for (e, o, lost) in &mut objects {
        if o.price <= 0 {
            continue;
        }
        let mut d = lost.as_deref().copied().unwrap_or_default();
        for _ in 0..days {
            d = d.day_older(o.price);
        }
        total += d.0 as i64;
        match lost {
            Some(mut l) => *l = d,
            None => {
                commands.entity(e).insert(d);
            }
        }
    }
    info!("depreciation: the home lot's things have lost §{total} in all");
}

/// The share of what's on the home lot is worth that each bill comes to (the game's
/// `kPercentageOfWealthBilled`).
const BILLED_SHARE: f64 = 0.045;

/// A bill for the home lot's things: their worth, as the game bills it.
pub fn bill_for(worth: i64) -> i64 {
    (worth as f64 * BILLED_SHARE) as i64
}

fn pay_bills(
    mut commands: Commands,
    clock: Res<GameClock>,
    household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    objects: Query<
        (Entity, &GameObject, Option<&Depreciated>, Has<crate::grime::Charred>, Option<&crate::paintings::Hung>),
        (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>),
    >,
    rewards: Query<&crate::wishes::Wishes, With<HouseholdMember>>,
    mut repo: ResMut<crate::repoman::RepoDue>,
) {
    let Some(mut h) = household else { return };
    let day = clock.day();
    let mailbox = objects.iter().any(|(_, o, ..)| o.kind == ObjectKind::Mailbox);
    // Bills arrive Monday and Thursday mornings.
    if day != h.last_bill_day && (day % 7 == 0 || day % 7 == 3) && clock.hour_f() >= 9.0 {
        h.last_bill_day = day;
        if day == 0 {
            return;
        }
        if !rewards.iter().any(|w| w.has_reward("NoBillsEver")) {
            let worth: i64 = objects.iter().map(|(_, o, lost, charred, hung)| object_value(o, lost, charred, hung)).sum();
            let bill = bill_for(worth);
            let mut each: Vec<(i64, &str)> = objects.iter().map(|(_, o, lost, charred, hung)| (object_value(o, lost, charred, hung), o.name.as_str())).collect();
            each.sort_by_key(|(w, _)| -w);
            debug!("bill §{bill} for §{worth} in {} things, the dearest {:?}", each.len(), &each[..each.len().min(12)]);
            if bill <= 0 {
                // (Nothing worth billing.)
            } else if mailbox {
                // (The mail carrier brings them.)
                commands.insert_resource(crate::services::MailDue(Bill { amount: bill, day }));
            } else {
                h.funds -= bill;
                notes.push(format!("The bills arrived: §{bill} was paid automatically."));
            }
        }
    }
    // Three days late: a debt, and the repo man on his way for it (`repoman`).
    let overdue: i64 = h.bills.iter().filter(|b| day >= b.day + 3).map(|b| b.amount).sum();
    if overdue > 0 && clock.hour_f() >= 10.0 {
        h.bills.retain(|b| day < b.day + 3);
        crate::repoman::owed(&mut repo, overdue, clock.minutes);
        notes.push(crate::repoman::ON_HIS_WAY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CATALOG_TSV=<file> (the `s3tool catalog` listing): what the catalogue's script classes come
    /// to, those with nothing to do counted by class (a survey, not a check).
    #[test]
    #[ignore]
    fn catalogue_kinds_survey() {
        let Ok(path) = std::env::var("CATALOG_TSV") else { return };
        let text = std::fs::read_to_string(path).unwrap();
        let mut idle: std::collections::BTreeMap<String, (usize, String)> = Default::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let (Some(name), Some(script)) = (f.get(4), f.get(5)) else { continue };
            let kind = ObjectKind::from_script(script, name);
            if matches!(kind, ObjectKind::Other | ObjectKind::Decoration | ObjectKind::Table | ObjectKind::Light | ObjectKind::Plant) || interactions_for(kind).is_empty() {
                let class = script.rsplit_once('.').map_or(*script, |(ns, _)| ns).to_string() + "." + script.rsplit('.').next().unwrap_or("").trim_end_matches(char::is_numeric);
                let e = idle.entry(format!("{kind:?}\t{class}")).or_insert((0, name.to_string()));
                e.0 += 1;
            }
        }
        for (k, (n, example)) in idle {
            println!("{n}\t{k}\t{example}");
        }
    }

    #[test]
    fn children_play_their_own_social_clips() {
        let names: Vec<String> = ["c2c_soc_Neutral_FriendlyHug_Friendly_Neutral_x", "c2c_soc_Neutral_FriendlyHug_Friendly_Neutral_y", "a2c_soc_Neutral_FriendlyHug_Friendly_Neutral_x"].map(String::from).to_vec();
        let hug: &'static [&'static str] = &["a2a_soc_Neutral_FriendlyHug_Friendly_Neutral"];
        assert!(child_social_clips(hug, Age::Adult, Age::Teen, false, &names).is_none(), "grown-ups' clips between the grown-up built");
        let (x, y) = child_social_clips(hug, Age::Child, Age::Child, false, &names).unwrap();
        assert_eq!((x.loops, y.loops), (["c2c_soc_Neutral_FriendlyHug_Friendly_Neutral"].as_slice(), ["c2c_soc_Neutral_FriendlyHug_Friendly_Neutral"].as_slice()));
        assert_eq!((x.side, y.side), (Some('x'), Some('y')));
        // (A grown-up with a child, and only one side of theirs there: the generic talk.)
        let (x, y) = child_social_clips(hug, Age::Adult, Age::Child, false, &names).unwrap();
        assert_eq!((x.loops, y.loops), (TALK, AGREE));
        let (_, y) = child_social_clips(hug, Age::Adult, Age::Child, true, &names).unwrap();
        assert_eq!(y.loops, DISAGREE, "a mean one goes down badly");
    }

    #[test]
    fn no_bills_ever_exempts_the_household_but_not_visitors() {
        for mailbox in [false, true] {
            let mut app = App::new();
            let mut clock = GameClock::default();
            clock.minutes = 3.0 * 1440.0 + 9.0 * 60.0;
            app.insert_resource(clock)
                .insert_resource(Household { name: "Test".into(), funds: 1000, lot_index: 0, last_bill_day: 0, bills: Vec::new() })
                .init_resource::<Notifications>()
                .init_resource::<crate::repoman::RepoDue>()
                .add_systems(Update, pay_bills);
            if mailbox {
                app.world_mut().spawn(GameObject { kind: ObjectKind::Mailbox, name: "Mailbox".into(), objd: (0, 0, 0), price: 0,
                    center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None });
            }
            // (Worth §2,000 when new, and a third of that lost: billed §60. Another lot's
            // things aren't the household's.)
            app.world_mut().spawn((GameObject { kind: ObjectKind::Sofa, name: "Sofa".into(), objd: (0, 0, 0), price: 2000,
                center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None }, Depreciated(2000 / 3)));
            app.world_mut().spawn((GameObject { kind: ObjectKind::Sofa, name: "Sofa".into(), objd: (0, 0, 0), price: 50000,
                center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None }, crate::visit::LotObject(1)));
            let owner = app.world_mut().spawn((HouseholdMember,
                crate::wishes::Wishes::restored(0, vec!["NoBillsEver".into()], 0.0))).id();
            app.update();
            assert_eq!(app.world().resource::<Household>().funds, 1000);
            assert_eq!(app.world().resource::<Household>().last_bill_day, 3);
            assert!(!app.world().contains_resource::<crate::services::MailDue>());
            // Once the owner moves out, their reward no longer exempts this household.
            app.world_mut().entity_mut(owner).remove::<HouseholdMember>();
            app.world_mut().resource_mut::<GameClock>().minutes = 7.0 * 1440.0 + 9.0 * 60.0;
            app.update();
            assert_eq!(app.world().contains_resource::<crate::services::MailDue>(), mailbox);
            assert_eq!(app.world().resource::<Household>().funds, if mailbox { 1000 } else { 940 });
            app.update();
            assert_eq!(app.world().resource::<Household>().funds, if mailbox { 1000 } else { 940 }, "only one bill per scheduled day");
        }
    }

    #[test]
    fn objects_do_for_needs_as_their_class_is_tuned() {
        assert_eq!(class_chain("BedDoubleBrass").collect::<Vec<_>>(), ["BedDoubleBrass", "BedDouble", "Bed"]);
        assert_eq!(class_chain("TVModerateFlatscreenHETech").find(|c| *c == "TVModerateFlatscreen"), Some("TVModerateFlatscreen"));
        let rate = |class: &str, interaction: &str, per_hour: [f32; 6], once: [f32; 6]| s3bake::gamedata::InteractionRates { class: class.into(), interaction: interaction.into(), per_hour, once };
        let data = s3bake::gamedata::GameDataBaked {
            interaction_rates: vec![
                rate("TVCheap", "WatchTV", [0.0, 0.0, 0.0, 0.0, 0.0, 25.0], [0.0; 6]),
                rate("Toilet", "UseToilet", [0.0; 6], [0.0, 200.0, 0.0, 0.0, -10.0, 0.0]),
            ],
            energy_multipliers: vec![("BedDoubleCeleb".into(), 1.4)],
            ..Default::default()
        };
        let catalog = [((1, 0, 1), "Sims3.Gameplay.Objects.Electronics.TVCheap"), ((1, 0, 2), "Sims3.Gameplay.Objects.Plumbing.ToiletRanch"), ((1, 0, 3), "Sims3.Gameplay.Objects.Beds.BedDoubleCeleb"), ((1, 0, 4), "Sims3.Gameplay.Objects.Chair")];
        let mut rates = ObjectRates::default();
        load_object_rates(&mut rates, &data, catalog);
        let def = |name: &str| interactions_for(match name { "Watch TV" => ObjectKind::Tv, "Use Toilet" => ObjectKind::Toilet, _ => ObjectKind::BedDouble }).iter().find(|d| d.name == name).unwrap();
        let tv = def("Watch TV");
        assert_eq!(rates.per_hour(tv, (1, 0, 1))[FUN], 25.0, "a cheap TV's fun");
        assert_eq!(rates.per_hour(tv, (1, 0, 4))[FUN], tv.per_hour[FUN], "what the game doesn't tune keeps ours");
        let toilet = def("Use Toilet");
        assert!((rates.per_hour(toilet, (1, 0, 2))[BLADDER] * toilet.minutes / 60.0 - 200.0).abs() < 1e-3, "a Toilet's, at once, over the time it takes");
        let sleep = def("Sleep");
        assert!((rates.per_hour(sleep, (1, 0, 3))[ENERGY] - sleep.per_hour[ENERGY] * 1.4).abs() < 1e-3, "a better bed");
        assert!(!rates.0.contains_key(&(1, 0, 4)));
    }

    #[test]
    fn sleep_and_naps_come_as_the_games() {
        // (Waking at six: at four in the afternoon, fourteen hours off; at eleven at night, seven.)
        assert_eq!(hours_until_waking(16.0, None), 14.0);
        assert_eq!(hours_until_waking(23.0, None), 7.0);
        assert!(!may_sleep(0.0, 14.0), "not in the afternoon, not sleepy");
        assert!(may_sleep(-65.0, 14.0), "sleepy");
        assert!(may_sleep(40.0, 7.0) && !may_sleep(60.0, 7.0), "at bedtime once tired enough");
        assert!(may_nap(-70.0, 14.0));
        assert!(!may_nap(-70.0, 7.0), "not this near waking: bed instead");
        assert!(!may_nap(-30.0, 14.0));
    }

    #[test]
    fn skills_grow_by_the_games_points() {
        // (Level 1 at 1,800 points, 2 at 3,150; halfway between them is 1.5.)
        assert_eq!(level_to_points(&SKILL_POINTS, 0.0), 0.0);
        assert_eq!(level_to_points(&SKILL_POINTS, 1.5), 2475.0);
        assert_eq!(points_to_level(&SKILL_POINTS, 2475.0), 1.5);
        assert_eq!(points_to_level(&SKILL_POINTS, 200000.0), 10.0);
        assert_eq!(level_to_points(&SKILL_POINTS, 10.0), 98250.0);
        let mut skills = Skills::default();
        assert_eq!(skills.add_points("Piano", 900.0), None);
        assert_eq!(skills.add_points("Piano", 900.0), Some(1), "a level at 1,800");
        // (An hour at the piano is 420 points: level 10 takes the game's weeks.)
        let hours = (SKILL_POINTS[9] as f32 - skills.points("Piano")) / practice_points("Play Piano");
        assert!(hours > 200.0);
        assert!((skills.points("Piano") - 1800.0).abs() < 0.5);
    }

    #[test]
    fn things_lose_worth_as_the_game_depreciates_them() {
        // (15% once bought, 10% a day after, never past 40%.)
        let d = Depreciated::initial(1000);
        assert_eq!(d.0, 150);
        assert_eq!(d.day_older(1000).0, 250);
        assert_eq!(d.day_older(1000).day_older(1000).day_older(1000).day_older(1000).0, 400);
        assert_eq!(Depreciated::default().day_older(5).0, 1, "something cheap still loses a simoleon");
        assert_eq!(Depreciated::default().day_older(0).0, 0);
        let sofa = GameObject { kind: ObjectKind::Sofa, name: "Sofa".into(), objd: (0, 0, 0), price: 1000, center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None };
        assert_eq!(object_value(&sofa, Some(&d), false, None), 850);
        assert_eq!(object_value(&sofa, Some(&d), true, None), 0, "burnt");
        assert_eq!(bill_for(20000), 900);
    }

    #[test]
    fn the_home_lot_depreciates_at_midnight() {
        let mut app = App::new();
        let mut clock = GameClock::default();
        clock.minutes = 2.0 * 1440.0 + 23.0 * 60.0;
        app.insert_resource(clock)
            .insert_resource(Household { name: "Test".into(), funds: 1000, lot_index: 0, last_bill_day: 0, bills: Vec::new() })
            .init_resource::<DepreciationDay>()
            .add_systems(Update, depreciate);
        let object = |price| GameObject { kind: ObjectKind::Sofa, name: "Sofa".into(), objd: (0, 0, 0), price, center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None };
        let home = app.world_mut().spawn(object(1000)).id();
        let away = app.world_mut().spawn((object(1000), crate::visit::LotObject(1))).id();
        app.update();
        assert!(app.world().get::<Depreciated>(home).is_none(), "not till midnight");
        app.world_mut().resource_mut::<GameClock>().minutes = 3.0 * 1440.0 + 1.0;
        app.update();
        assert_eq!(app.world().get::<Depreciated>(home), Some(&Depreciated(100)));
        assert!(app.world().get::<Depreciated>(away).is_none());
        app.update();
        assert_eq!(app.world().get::<Depreciated>(home), Some(&Depreciated(100)), "once a night");
    }

    /// The game's social rules as baked (`cargo test -p sims3 social_rules_audit -- --ignored
    /// --nocapture`).
    #[test]
    #[ignore]
    fn social_rules_audit() {
        let data = s3bake::gamedata::load_gamedata(&s3bake::default_root()).expect("game data");
        let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
        for r in &data.general_rules {
            *by.entry(r.commodity.as_str()).or_default() += 1;
        }
        println!("general rules {} {:?}", data.general_rules.len(), by);
        let with = data.socials.iter().filter(|s| !s.rules.is_empty()).count();
        println!("socials {} with rules {} rules {} outcomes {}", data.socials.len(), with, data.socials.iter().map(|s| s.rules.len()).sum::<usize>(), data.socials.iter().map(|s| s.outcomes.len()).sum::<usize>());
        println!("stcs {:?}", data.stcs.iter().map(|s| format!("{} {} {}-{}", s.name, s.commodity, s.min, s.max)).collect::<Vec<_>>());
        if let Some(chat) = data.socials.iter().find(|s| s.key == "Chat") {
            for r in &chat.rules {
                println!("Chat {} spec {} stc {:?} com {:?} rep {}", r.result, r.spec, r.st_context, r.st_commodity, r.repetition);
            }
        }
    }

    /// Which catalogue objects the game tunes need rates for, and what (`cargo test -p sims3
    /// need_rates_audit -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn need_rates_audit() {
        let root = s3bake::default_root();
        let data = s3bake::gamedata::load_gamedata(&root).expect("game data");
        let baked = crate::baked::BakedData::open(root, None).unwrap();
        let mut rates = ObjectRates::default();
        load_object_rates(&mut rates, &data, baked.catalog.iter().map(|c| (c.objd, c.script.as_str())));
        let mut rows: Vec<String> = baked
            .catalog
            .iter()
            .filter_map(|c| rates.0.get(&c.objd).map(|o| (c, o)))
            .map(|(c, o)| {
                let mut by: Vec<String> = o.by.iter().map(|(n, r)| format!("{n} {:?} {:?}", r.per_hour, r.once)).collect();
                by.sort();
                format!("{:40} {:28} energy x{} {}", c.name, c.script.rsplit('.').next().unwrap_or(""), o.energy, by.join("; "))
            })
            .collect();
        rows.sort();
        for r in rows {
            println!("{r}");
        }
    }

    /// How long clips are (`CLIPS=<name>,<name> cargo test -p sims3 clip_lengths -- --ignored
    /// --nocapture`), for an interaction's ending.
    #[test]
    #[ignore]
    fn clip_lengths() {
        let data = crate::baked::BakedData::open(s3bake::default_root(), None).unwrap();
        for name in std::env::var("CLIPS").unwrap_or_default().split(',').filter(|n| !n.is_empty()) {
            println!("{name}: {:?} parents {:?}", data.clip(name).map(|c| c.duration), data.clip(name).map(|c| c.parents.clone()));
            // (Where the root and hips are at its start and end.)
            if let Some(c) = data.clip(name) {
                for bone in ["b__ROOT__", "b__ROOT_bind__", "b__Pelvis__", "world_offset"] {
                    if let Some(t) = c.tracks.get(&s3pkg::fnv32(bone)) {
                        if std::env::var("TRACK").is_ok_and(|v| v == "1" || v == bone) {
                            let n = t.translation.len();
                            for k in (0..n).step_by((n / 8).max(1)) {
                                println!("    t {:.2} {:?}", t.translation[k].0, t.translation[k].1);
                            }
                        }
                        println!("  {bone}: {:?} .. {:?} turned {:?} .. {:?}", t.translation.first().map(|p| p.1), t.translation.last().map(|p| p.1), t.rotation.first().map(|r| Quat::from_array(r.1).to_euler(EulerRot::YXZ).0.to_degrees()), t.rotation.last().map(|r| Quat::from_array(r.1).to_euler(EulerRot::YXZ).0.to_degrees()));
                    }
                }
            }
        }
    }

    /// An object model's rig and skinning, and a clip's tracks for it (`SKIN=<modl instance hex>
    /// CLIPS=<clip> cargo test -p sims3 object_skin -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn object_skin() {
        let data = crate::baked::BakedData::open(s3bake::default_root(), None).unwrap();
        let inst = u64::from_str_radix(&std::env::var("SKIN").unwrap(), 16).unwrap();
        let skin = data.skin(&(0x0166_1233, 1, inst)).or_else(|| data.skin(&(0x0166_1233, 0, inst)));
        for name in std::env::var("CLIPS").unwrap_or_default().split(',').filter(|n| !n.is_empty()) {
            let Some(c) = data.clip(name) else { continue };
            for (h, t) in &c.tracks {
                println!("{name} {h:08X}: {:?} .. {:?}", t.translation.first(), t.translation.last());
            }
        }
        for k in [(0x0166_1233, 1, inst), (0x0166_1233, 0, inst)] {
            for p in data.model(&k).map(|m| m.parts).unwrap_or_default() {
                println!("part: {} verts, first {:?}, texture {:?}, bounds {:?}..{:?}, states {:?}", p.positions.len(), p.positions.first(), p.texture, p.bmin, p.bmax, p.states);
            }
        }
        let Some(skin) = skin else { return println!("not skinned") };
        for (i, b) in skin.bones.iter().enumerate() {
            println!("bone {i}: {:08X} parent {} at {:?}", b.0, b.1, b.2);
        }
        for (m, mesh) in skin.meshes.iter().enumerate() {
            let mut used: Vec<u16> = mesh.bones.iter().zip(&mesh.weights).flat_map(|(b, w)| b.iter().zip(w).filter(|(_, w)| **w > 0).map(|(b, _)| *b)).collect();
            used.sort();
            used.dedup();
            println!("mesh {m}: {} verts, first {:?}, bones {used:?}", mesh.verts, mesh.first);
        }
        for name in std::env::var("CLIPS").unwrap_or_default().split(',').filter(|n| !n.is_empty()) {
            let Some(c) = data.clip(name) else { continue };
            for (h, t) in &c.tracks {
                println!("{name} {h:08X}: {:?} .. {:?}", t.translation.first(), t.translation.last());
            }
        }
    }

    /// Every clip the catalogue's objects' interactions play is baked (needs the bake:
    /// `SIMS3_CACHE=<baked> cargo test -p sims3 interaction_clips_baked -- --ignored`).
    #[test]
    fn a_slippy_slide_run_is_one_of_the_games() {
        assert_eq!(slide_clip(true, 0.5).start, Some("a2o_waterSlide_failSlide1_x"));
        assert_eq!(slide_clip(false, 0.0).start, Some("a2o_waterSlide_succeedSlide1_x"));
        assert_eq!(slide_clip(false, 0.99).start, Some("a2o_waterSlide_succeedSlide3_x"));
        assert!(interactions_for(ObjectKind::SlippySlide).iter().any(|d| d.name == "Slide"));
        assert_eq!(ObjectKind::from_script("Sims3.Gameplay.Objects.Environment.SlippySlideGeneric", "SlippySlideFrog"), ObjectKind::SlippySlide);
    }

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
    /// from: `cargo test -p sims3 unused_objects -- --ignored --nocapture`; `ALL_PACKS=1`, the
    /// packs' too).
    #[test]
    #[ignore]
    fn unused_objects() {
        let catalog: Vec<s3bake::CatalogEntry> = s3bake::read_value(&s3bake::default_root().global_dir().join("catalog.bin")).unwrap();
        let mut by_class: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        let all = std::env::var("ALL_PACKS").is_ok();
        for c in catalog.iter().filter(|c| c.price > 0 && (all || c.objd.1 == 0)) {
            let kind = ObjectKind::from_script(&c.script, &c.instance_name);
            // (A ball, bubble bath, the rubber duck and a house phone are used through something
            // else: catch, the bath, the phone's menu.)
            let elsewhere = matches!(kind, ObjectKind::Ball | ObjectKind::BubbleBath | ObjectKind::RubberDucky | ObjectKind::Phone);
            if interactions_for(kind).is_empty() && !elsewhere && !c.script.contains("Decorations") && !c.script.contains("Lighting") {
                by_class.entry(c.script.rsplit('.').take(2).collect::<Vec<_>>().join(" < ")).or_default().push(c.instance_name.clone());
            }
        }
        for (class, names) in &by_class {
            println!("{class}: {} ({})", names.len(), names.iter().take(4).cloned().collect::<Vec<_>>().join(", "));
        }
    }
}

#[cfg(test)]
mod clip_names {
    /// Every animation clip the game's code names is one the bake keeps (its name starts with
    /// one of `CLIP_PREFIXES`); otherwise it silently never plays.
    #[test]
    fn every_clip_named_is_baked() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            for lit in src.split('"').skip(1).step_by(2) {
                let clip = ["a2o_", "a2a_", "c2o_", "p2o_", "b2o_", "a2b_", "a2p_", "a_", "c_", "p_"].iter().any(|p| lit.starts_with(p))
                    && lit.len() > 6
                    && lit.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if clip && !s3bake::clips::CLIP_PREFIXES.iter().any(|p| lit.starts_with(p)) {
                    missing.push(format!("{}: {lit}", path.file_name().unwrap().to_string_lossy()));
                }
            }
        }
        assert!(missing.is_empty(), "clips named but not baked: {missing:?}");
    }
}

#[cfg(test)]
mod kind_audit {
    use super::*;

    /// The buyable catalogue objects that come out as nothing in particular (no interactions):
    /// `cargo test -p sims3 kind_audit -- --ignored --nocapture`, with the cache baked.
    #[test]
    #[ignore]
    fn buyable_objects_with_no_kind() {
        let cat: Vec<s3bake::types::CatalogEntry> = s3bake::pack::read_value(&s3bake::default_root().global_dir().join("catalog.bin")).unwrap_or_default();
        let mut by: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        for c in cat.iter().filter(|c| c.price > 0 && !c.script.is_empty()) {
            if ObjectKind::from_script(&c.script, &c.name) == ObjectKind::Other {
                let class = c.script.split(".Mimics.").next().unwrap_or(&c.script).to_string();
                by.entry(class).or_default().push(c.name.clone());
            }
        }
        for (class, names) in &by {
            println!("KIND	{class}	{}	{}", names.len(), names.iter().take(3).cloned().collect::<Vec<_>>().join(" | "));
        }
    }

    /// Every base-game script class and the kind it comes out as, to look over for ones taken
    /// for something else (a workout bench for a treadmill): `... kind_audit_all ...`.
    #[test]
    #[ignore]
    fn kind_audit_all() {
        let cat: Vec<s3bake::types::CatalogEntry> = s3bake::pack::read_value(&s3bake::default_root().global_dir().join("catalog.bin")).unwrap_or_default();
        let mut by: std::collections::BTreeMap<String, (ObjectKind, usize)> = std::collections::BTreeMap::new();
        for c in cat.iter().filter(|c| c.price > 0 && !c.script.is_empty() && c.objd.1 == 0) {
            let kind = ObjectKind::from_script(&c.script, &c.name);
            if kind == ObjectKind::Other {
                continue;
            }
            let class = format!("{kind:?}	{}", c.script.trim_start_matches("Sims3.Gameplay.Objects."));
            by.entry(class).or_insert((kind, 0)).1 += 1;
        }
        for (class, (_, n)) in &by {
            println!("ALL	{class}	{n}");
        }
    }
}
