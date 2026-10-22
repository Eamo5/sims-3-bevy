//! Saving and loading games: the household (each Sim's looks, personality, needs, skills,
//! moodlets, career and relationships), visitors they know, money, the time, and the changes
//! made to the lot in buy mode. Saves are JSON files in `saves/`.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::clock::GameClock;
use crate::interact::*;
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::loading::{Catalog, CurrentWorld};
use crate::nav::Floor;
use crate::sim::*;
use crate::social::{RelStatus, Relationships};
use crate::PlayMode;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RemovedLotObjects>()
            .init_resource::<PendingBroken>()
            .init_resource::<PendingStates>()
            .init_resource::<SaveSlot>()
            .add_message::<SaveRequest>()
            .add_message::<SnapshotRequest>()
            .add_message::<SaveAsRequest>()
            .add_systems(OnEnter(crate::AppState::Loading), new_game_slot)
            .add_systems(OnEnter(crate::AppState::Loading), |mut pending: ResMut<PendingBroken>, mut states: ResMut<PendingStates>| {
                *pending = PendingBroken::default();
                *states = PendingStates::default();
            })
            .add_systems(Update, (restore_broken, restore_states).after(apply_loaded_game).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, resume_saved_lot.run_if(in_state(PlayMode::ChooseLot)))
            .add_systems(Update, (apply_loaded_game.after(crate::clock::advance_clock), save_game.after(crate::buyhistory::update)).run_if(in_state(PlayMode::Live)));
    }
}

pub const SAVE_VERSION: u32 = 1;

pub fn saves_dir() -> PathBuf {
    std::env::var_os("SIMS3_SAVES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("saves"))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedSim {
    pub id: u64,
    pub look: u64,
    pub first: String,
    pub last: String,
    pub female: bool,
    pub age: String,
    pub traits: Vec<String>,
    pub skin: [f32; 3],
    pub hair: [f32; 3],
    pub top: [f32; 3],
    pub bottom: [f32; 3],
    pub member: bool,
    pub selected: bool,
    /// "home", "visiting" or "away".
    pub whereabouts: String,
    pub position: [f32; 3],
    pub yaw: f32,
    pub floor: u8,
    pub motives: [f32; 6],
    pub skills: Vec<(String, f32)>,
    pub moodlets: Vec<(String, f64)>,
    pub job: Option<SavedJob>,
    pub relationships: Vec<SavedRel>,
    #[serde(default)]
    pub lifetime_happiness: u32,
    /// Chosen hair, top, bottom, outfit and shoes, then beard, glasses, lipstick and eye shadow,
    /// then the top, bottom, outfit and shoes of their formal wear, sleepwear, athletic wear and
    /// swimwear (saves from before those could be chosen stop at the eye shadow).
    #[serde(default)]
    pub outfit: Vec<Option<(u32, u32, u64)>>,
    /// Their jewellery, as chosen (none: as their look has it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessories: Option<Vec<(u32, u32, u64)>>,
    /// Their eyeliner and blush, as chosen (none: as their look has it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eyeliner: Option<(u32, u32, u64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blush: Option<(u32, u32, u64)>,
    /// Their eyebrows, as chosen (none: as their look has it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brows: Option<(u32, u32, u64)>,
    /// Their moles, freckles and age weathering, as chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skin_details: Vec<(u32, u32, u64)>,
    /// Their body hair, as chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_hair: Vec<(u32, u32, u64)>,
    #[serde(default)]
    pub wearing: Option<crate::simbody::OutfitKind>,
    #[serde(default)]
    pub outfit_chosen: bool,
    #[serde(default)]
    pub rewards: Vec<String>,
    /// Days into the current life stage, and how long old age lasts.
    #[serde(default)]
    pub aging: Option<(f32, f32)>,
    #[serde(default)]
    pub elder_risk: Option<f64>,
    #[serde(default)]
    pub school_grade: Option<f32>,
    #[serde(default)]
    pub homework: bool,
    #[serde(default)]
    pub homework_progress: f32,
    #[serde(default)]
    pub body_temperature: crate::weather::BodyTemperature,
    #[serde(default)]
    pub happiness_fraction: f64,
    #[serde(default)]
    pub offered_wishes: Vec<crate::wishes::Wish>,
    #[serde(default)]
    pub promised_wishes: Vec<crate::wishes::Wish>,
    #[serde(default)]
    pub next_wish_offer: Option<f64>,
    /// The game's wishes as they stood: armed, offered, promised, how far along (`dreams`).
    #[serde(default)]
    pub dream_state: Option<crate::dreams::SavedDreams>,
    /// Classroom start and home arrival for a school day currently in progress.
    #[serde(default)]
    pub school_visit: Option<(f64, f64)>,
    /// Pregnant since (game minutes), with the other parent's id and the stage shown so far.
    #[serde(default)]
    pub pregnancy: Option<(f64, Option<u64>, u8)>,
    /// What sways the baby: the sex (towards a girl), and hours of children's TV and radio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pregnancy_sway: Option<(f32, f64)>,
    /// Her mood over the pregnancy (mood × minutes), for the baby's traits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pregnancy_mood: Option<f64>,
    /// Body shape: weight and fitness.
    #[serde(default)]
    pub shape: Option<(f32, f32)>,
    /// Opportunities taken on (by the game's id, with their deadline) and done.
    #[serde(default)]
    pub opportunities: Vec<(String, Option<f64>)>,
    #[serde(default)]
    pub opportunities_done: Vec<String>,
    /// Those whose venue task is done, to be reported on at work.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opportunities_visited: Vec<String>,
    /// The Sims they're to be done with (the opportunity's id, the Sim's id and name).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opportunities_with: Vec<(String, u64, String)>,
    #[serde(default)]
    pub lifetime_wish: Option<SavedLifetimeWish>,
    /// The book under way and those written.
    #[serde(default)]
    pub author: Option<crate::writing::Author>,
    /// Recipes learned from recipe books.
    #[serde(default)]
    pub recipes: Vec<String>,
    /// The skills whose classes they've taken.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    /// Their skill books (on the household's shelves) and the pages they've read of each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_books: Vec<(String, f32)>,
    /// Their books read for their own sake: those they brought home, and how far they are.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub novels: Vec<crate::novels::NovelRead>,
    /// In the book club (a Bookworm's, the game's `InBookClub`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub book_club: bool,
    /// Time off work asked for by phone (`timeoff`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_off: Option<crate::timeoff::TimeOff>,
    /// This week's charities, and when they last gave (`charity`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charities: Option<crate::charity::Charities>,
    /// Ranked chess: wins, losses and rank.
    #[serde(default)]
    pub chess: Option<crate::chess::ChessRecord>,
    /// Their face shape, as sculpted in Create a Sim or inherited (none: as their look has it).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub face: Vec<f32>,
    /// Their preset faces, as chosen or inherited (none: as their look has it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_presets: Option<Vec<u16>>,
    /// Their life state, if supernatural.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occult: Option<String>,
    /// What they carry.
    #[serde(default)]
    pub inventory: crate::inventory::Inventory,
    /// A toddler's walking and talking.
    #[serde(default)]
    pub toddler: Option<crate::little::ToddlerSkills>,
    /// Eye colour (saves from before eye colours: by the Sim's look).
    #[serde(default)]
    pub eyes: Option<[f32; 3]>,
    /// Their voice (saves from before voices could be chosen: the one their look gave them).
    #[serde(default)]
    pub voice: Option<u8>,
    /// Their eyebrows' colour, when not their hair's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brow_hair: Option<[f32; 3]>,
    /// Their voice's pitch (saves from before: the middle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_pitch: Option<f32>,
    /// Their biography.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bio: String,
    /// A retired Sim's pension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pension: Option<crate::careers::Pension>,
    /// Their favourite food, music and colour (saves from before favourites: by their look).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorites: Option<crate::sim::Favorites>,
    /// Their skill journal: what they've done with their skills, and the challenges earned.
    #[serde(default, skip_serializing_if = "crate::journal::SkillJournal::is_empty")]
    pub journal: crate::journal::SkillJournal,
    /// The colourways chosen for their clothes (part, preset).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub designs: Vec<((u32, u32, u64), u8)>,
    /// The styles made for their clothes in Create a Style.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<crate::style::CustomStyle>,
}

/// A lifetime wish (by the game's check for it), and what's counted towards it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedLifetimeWish {
    pub check: String,
    pub fulfilled: bool,
    #[serde(default)]
    pub careers: Vec<String>,
    #[serde(default)]
    pub raised: u32,
    #[serde(default)]
    pub rich_spouse: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedJob {
    pub track: String,
    pub level: usize,
    pub performance: f32,
    /// The career path ("Thief"; "Base" for one that doesn't branch).
    #[serde(default)]
    pub branch: String,
    /// How they go about their work ("WorkHard"...; none: normally).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    /// Their boss and coworkers (Sims' ids), the highest level they've reached, raises, and
    /// when they were warned about their work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boss: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coworkers: Vec<u64>,
    #[serde(default)]
    pub highest: usize,
    #[serde(default, skip_serializing_if = "no_raise")]
    pub raise: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warned_at: Option<f64>,
    /// What the career keeps count of (meetings, concerts).
    #[serde(default, skip_serializing_if = "crate::careermetrics::Tally::is_empty")]
    pub tally: crate::careermetrics::Tally,
}

fn no_raise(v: &i64) -> bool {
    *v == 0
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedRel {
    pub with: u64,
    pub friendship: f32,
    pub romance: f32,
    pub status: String,
    pub kissed: bool,
    /// When they last spent time together (game minutes; older saves: taken as when loaded).
    #[serde(default)]
    pub last: f64,
    /// Their conversation's context (commodity and how far), and whether they're sworn enemies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stc: Option<(String, f32)>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enemy: bool,
    /// Romantically involved (older saves: kissed, or romance under way).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub romantic: bool,
    /// Feeling betrayed by them: until when, and the liking it left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub betrayed: Option<(f64, f32)>,
    /// Their traits known (the game's names).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_traits: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedObject {
    pub objd: (u32, u32, u64),
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    /// The design it's in: one of the catalogue's (by number), or one a lot was furnished in
    /// (its texture); neither: as the game ships it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_key: Option<(u32, u32, u64)>,
    /// A painting hung on a wall: the inventory item it is (its picture and worth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<crate::inventory::Stack>,
    /// In the family inventory: what it had lost of its price (on the lot, its state keeps that).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lost: i32,
}

impl SavedObject {
    /// The design's texture.
    pub fn design_texture(&self) -> Option<s3bake::Key> {
        self.design_key.or(self.design.map(|d| crate::objects::design_texture(self.objd, d)))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedBroken {
    pub objd: (u32, u32, u64),
    pub position: [f32; 3],
    /// Burnt in a fire (charred until it's mended).
    #[serde(default)]
    pub charred: bool,
}

/// An object's wear that isn't breakage: a bathroom's grime, a bed left unmade.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedObjectState {
    pub objd: (u32, u32, u64),
    pub position: [f32; 3],
    #[serde(default)]
    pub grime: f32,
    #[serde(default)]
    pub unmade: bool,
    /// What it's lost of its price since it was bought.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lost: i32,
    /// A door's lock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<crate::doorlocks::DoorLock>,
    /// A bed's owners (`bedowners`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bed_owners: Option<crate::bedowners::BedOwners>,
    /// A dollhouse smashed (`dollhouse`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub smashed: bool,
    /// An outdoor trash can knocked over (`trashcans`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tipped: bool,
    /// An unfinished painting left on an easel (`paintings`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<crate::paintings::SavedCanvas>,
    /// A lamp switched on or off by hand, and its brightness other than Normal (`lights`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switched: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimmer: Option<f32>,
    /// The door set as the front door (`frontdoor`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub front: bool,
    /// What's left in a pet bowl (`petneeds`: bites).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bowl: Option<u8>,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

#[derive(Resource, Default)]
struct PendingStates(Vec<SavedObjectState>, Option<f32>);

/// Grime and unmade beds put back on their objects once the furniture's there (and, for grime,
/// once it's been given its clean slate).
fn restore_states(
    mut commands: Commands,
    time: Res<Time>,
    mut pending: ResMut<PendingStates>,
    mut objects: Query<(Entity, &GameObject, &Transform, Option<&mut crate::grime::Grime>), (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>)>,
) {
    if pending.0.is_empty() {
        return;
    }
    let started = *pending.1.get_or_insert(time.elapsed_secs());
    pending.0.retain(|saved| {
        let Some((e, o, _, grime)) = objects.iter_mut().find(|(_, o, tf, _)| o.objd == saved.objd && tf.translation.distance(Vec3::from(saved.position)) < 0.2) else { return true };
        let cleanable = crate::grime::cleanable(o.kind);
        if cleanable && saved.grime > 0.0 {
            let Some(mut g) = grime else { return true };
            g.0 = saved.grime;
        }
        if saved.unmade {
            commands.entity(e).insert(crate::interact::Unmade);
        }
        if saved.lost > 0 {
            commands.entity(e).insert(crate::interact::Depreciated(saved.lost));
        }
        if let Some(lock) = saved.lock.clone() {
            commands.entity(e).insert(lock);
        }
        if let Some(owners) = saved.bed_owners {
            commands.entity(e).insert(owners);
        }
        if saved.smashed {
            commands.entity(e).insert(crate::dollhouse::Smashed);
        }
        if saved.tipped {
            commands.entity(e).insert(crate::trashcans::TippedOver);
        }
        if let Some(c) = &saved.canvas {
            commands.entity(e).insert(crate::paintings::LeftOnEasel::from_saved(c));
        }
        if let Some(on) = saved.switched {
            commands.entity(e).insert(crate::lights::Switched(on));
        }
        if let Some(v) = saved.dimmer {
            commands.entity(e).insert(crate::lights::Dimmer(v));
        }
        if saved.front {
            commands.entity(e).insert(crate::frontdoor::FrontDoor);
        }
        if let Some(b) = saved.bowl {
            commands.entity(e).insert(crate::petneeds::BowlFood(b));
        }
        debug!("{} as it was: grime {}, unmade {}, lost {}", o.name, saved.grime, saved.unmade, saved.lost);
        false
    });
    if time.elapsed_secs() - started > 10.0 {
        pending.0.clear();
    }
}

#[derive(Resource, Default)]
struct PendingBroken(Vec<SavedBroken>, Option<f32>);

fn restore_broken(
    mut commands: Commands,
    time: Res<Time>,
    mut pending: ResMut<PendingBroken>,
    objects: Query<(Entity, &GameObject, &Transform), (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>)>,
) {
    if pending.0.is_empty() { return; }
    let started = *pending.1.get_or_insert(time.elapsed_secs());
    pending.0.retain(|saved| {
        if let Some((e, ..)) = objects.iter().find(|(_, o, tf)| o.objd == saved.objd && tf.translation.distance(Vec3::from(saved.position)) < 0.2) {
            commands.entity(e).insert(crate::interact::Broken);
            if saved.charred {
                commands.entity(e).insert(crate::grime::Charred);
            }
            false
        } else { true }
    });
    if time.elapsed_secs() - started > 10.0 { pending.0.clear(); }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SaveGame {
    #[serde(default)]
    pub broken_objects: Vec<SavedBroken>,
    /// Grime and unmade beds.
    #[serde(default)]
    pub object_states: Vec<SavedObjectState>,
    /// Water on the floor.
    #[serde(default)]
    pub puddles: Vec<crate::puddles::SavedPuddle>,
    /// The laundry: what's waiting, the piles, and the machines' loads.
    #[serde(default)]
    pub laundry: crate::laundry::SavedLaundry,
    /// Gems and metals away being cut or smelted (`rgmmail`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub posted: Vec<crate::rgmmail::Away>,
    /// The household's doorbell (`frontdoor`).
    #[serde(default)]
    pub doorbell: u8,
    /// What's owed the repo man (`repoman`).
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub repo_debt: i64,
    pub version: u32,
    pub world: String,
    pub lot_index: usize,
    pub lot_name: String,
    pub household: String,
    pub funds: i64,
    pub last_bill_day: u32,
    /// Bills waiting in the mailbox.
    #[serde(default)]
    pub bills: Vec<crate::interact::Bill>,
    pub minutes: f64,
    pub sims: Vec<SavedSim>,
    /// Objects bought in buy mode.
    pub bought: Vec<SavedObject>,
    /// The lot's own furniture that was sold or moved.
    pub removed: Vec<SavedObject>,
    /// Walls and floors built and repainted in build mode.
    #[serde(default)]
    pub paint: Vec<crate::building::PaintOp>,
    /// The garden: seeds in hand and what's planted.
    #[serde(default)]
    pub seeds: Vec<(String, u32)>,
    #[serde(default)]
    pub plants: Vec<crate::gardening::SavedPlant>,
    /// Produce the household has grown to perfection.
    #[serde(default)]
    pub perfect_produce: Vec<String>,
    /// How the rest of the town has moved on.
    #[serde(default)]
    pub town: crate::story::TownStory,
    /// Whether the alarm clock is set.
    #[serde(default)]
    pub alarm: crate::appliances::Alarm,
    /// The fish in the household's fish bowls.
    #[serde(default)]
    pub fishbowls: Vec<crate::fishbowl::SavedBowl>,
    /// Whether the household has a maid.
    #[serde(default)]
    pub maid: bool,
    /// Whether the household's stopped the paper.
    #[serde(default)]
    pub paper_cancelled: bool,
    /// Objects' handiness upgrades.
    #[serde(default)]
    pub upgrades: Vec<crate::upgrades::SavedUpgrades>,
    /// Who is whose parent.
    #[serde(default)]
    pub family: Vec<crate::family::Person>,
    /// The ground painted on the lot in build mode.
    #[serde(default)]
    pub terrain: Vec<crate::terrain_paint::Stroke>,
    /// The ground sculpted on the lot in build mode.
    #[serde(default)]
    pub heights: Vec<crate::terrain_paint::SavedHeight>,
    /// The household's collection journal.
    #[serde(default)]
    pub collection: crate::collecting::Collection,
    /// What's been discovered through telescopes (`telescope`).
    #[serde(default)]
    pub discoveries: crate::telescope::Discoveries,
    /// The household's stakes in the town's rabbit holes (`realestate`).
    #[serde(default)]
    pub real_estate: crate::realestate::RealEstate,
    /// Tombstones on the lot, with who lies there.
    #[serde(default)]
    pub graves: Vec<SavedGrave>,
    /// Leftovers in the fridge (servings, by recipe).
    #[serde(default)]
    pub leftovers: Vec<String>,
    /// When each was put away (game minutes; missing in older saves: as loaded).
    #[serde(default)]
    pub leftovers_at: Vec<f64>,
    /// The ingredients in the fridge (none: saves from before ingredients, given the start's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fridge: Option<Vec<crate::inventory::Stack>>,
    /// Meals still on the lot, including pizza and cut cakes, with remaining servings.
    #[serde(default)]
    pub meals: Vec<crate::meals::SavedMeal>,
    /// The season and the weather (`None` in saves from before there was weather).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather: Option<crate::weather::Weather>,
    /// The household's pets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pets: Vec<crate::pets::SavedPet>,
    /// The town's other households played before, as they were left (to play again).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dormant: Vec<SaveGame>,
    /// (A household played before:) evicted in Edit Town, in the household bin with no home.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub homeless: bool,
    /// Saved with the starter deck raised over the ground (older saves kept what stood on an
    /// empty lot's deck at the ground's height: lifted onto it when loaded).
    #[serde(default)]
    pub deck: bool,
    /// The family inventory: furniture owned, not set out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub family_inventory: Vec<SavedObject>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedGrave {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub cause: String,
    pub sim: SavedSim,
    /// What's engraved on it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub epitaph: String,
}

/// A saved Sim for tests: a random one, with this id and name.
#[cfg(test)]
pub fn saved_sim_for_test(id: u64, first: &str) -> SavedSim {
    let mut s = crate::sim::random_sim(&mut rand::rng(), "Test", None, crate::sim::Age::Adult);
    s.id = id;
    s.first = first.to_string();
    saved_look(&s)
}

/// A Sim's relationships as saved: with whom (by id), and how they stand.
pub fn saved_rels(rels: &Relationships, ids: &HashMap<Entity, u64>) -> Vec<SavedRel> {
    rels.0
        .iter()
        .filter_map(|(e, r)| {
            Some(SavedRel {
                with: *ids.get(e)?,
                friendship: r.friendship,
                romance: r.romance,
                status: status_name(r.status).into(),
                kissed: r.kissed,
                last: r.last,
                stc: (r.stc.value > 0.0).then(|| (r.stc.commodity.name().to_string(), r.stc.value)),
                enemy: r.enemy,
                romantic: r.romantic,
                betrayed: r.betrayed,
                known_traits: crate::life::Trait::ALL.iter().filter(|t| r.knows(**t)).map(|t| t.game_id()).collect(),
            })
        })
        .collect()
}

/// A saved relationship back as it stood.
pub fn restore_rel(x: &mut crate::social::Relationship, r: &SavedRel) {
    x.friendship = r.friendship;
    x.romance = r.romance;
    x.status = status_from(&r.status);
    x.kissed = r.kissed;
    x.last = r.last;
    x.enemy = r.enemy;
    x.romantic = r.romantic || r.kissed || r.romance >= 20.0;
    x.betrayed = r.betrayed;
    for t in r.known_traits.iter().filter_map(|n| crate::life::Trait::ALL.iter().find(|t| t.game_id().eq_ignore_ascii_case(n))) {
        x.learn(*t);
    }
    if let Some((c, v)) = &r.stc
        && let Some(c) = crate::conversation::Commodity::from_name(c)
    {
        x.stc = crate::conversation::Stc { commodity: c, value: *v };
    }
}

/// How a Sim looks, for saving someone no longer about (the rest left empty).
pub(crate) fn saved_look(sim: &Sim) -> SavedSim {
    let rgb = |c: Color| {
        let s = c.to_srgba();
        [s.red, s.green, s.blue]
    };
    SavedSim {
        id: sim.id,
        look: sim.look,
        first: sim.first.clone(),
        last: sim.last.clone(),
        female: sim.female,
        age: age_name(sim.age).into(),
        traits: sim.traits.iter().map(|t| t.name().to_string()).collect(),
        skin: rgb(sim.skin),
        hair: rgb(sim.hair),
        top: rgb(sim.top),
        bottom: rgb(sim.bottom),
        member: false,
        selected: false,
        whereabouts: "away".into(),
        position: [0.0; 3],
        yaw: 0.0,
        floor: 1,
        motives: [0.0; 6],
        skills: Vec::new(),
        moodlets: Vec::new(),
        job: None,
        relationships: Vec::new(),
        lifetime_happiness: 0,
        outfit: saved_outfit(&sim.outfit),
        accessories: sim.outfit.accessories.clone(),
        eyeliner: sim.outfit.eyeliner,
        blush: sim.outfit.blush,
        brows: sim.outfit.brows,
        skin_details: sim.outfit.skin_details.clone(),
        body_hair: sim.outfit.body_hair.clone(),
        wearing: None,
        outfit_chosen: false,
        rewards: Vec::new(),
        aging: None,
        elder_risk: None,
        school_grade: None,
        homework: false,
        homework_progress: 0.0,
        happiness_fraction: 0.0,
        body_temperature: default(),
        offered_wishes: Vec::new(),
        promised_wishes: Vec::new(),
        next_wish_offer: None,
        dream_state: None,
        school_visit: None,
        pregnancy: None,
        pregnancy_mood: None,
        pregnancy_sway: None,
        shape: Some((sim.weight, sim.fitness)),
        opportunities: Vec::new(),
        opportunities_done: Vec::new(),
        opportunities_visited: Vec::new(),
        opportunities_with: Vec::new(),
        lifetime_wish: None,
        author: None,
        recipes: Vec::new(),
        classes: Vec::new(),
        skill_books: Vec::new(),
        novels: Vec::new(),
        book_club: false,
        time_off: None,
        charities: None,
        chess: None,
        inventory: Default::default(),
        toddler: None,
        eyes: Some(rgb(sim.eyes)),
        brow_hair: sim.brow_hair.map(rgb),
        voice: Some(sim.voice),
        voice_pitch: (sim.voice_pitch != 0.5).then_some(sim.voice_pitch),
        bio: sim.bio.clone(),
        pension: None,
        favorites: Some(sim.favorites.clone()),
        journal: Default::default(),
        designs: sim.outfit.designs.clone(),
        styles: sim.outfit.styles.clone(),
        face: sim.face.clone(),
        face_presets: sim.face_presets.clone(),
        occult: sim.occult.map(|o| o.name().to_string()),
    }
}

/// A Sim's chosen parts as saved (the other outfits' only when any were chosen, as before).
fn saved_outfit(o: &OutfitChoice) -> Vec<Option<(u32, u32, u64)>> {
    let mut v = vec![o.hair, o.top, o.bottom, o.full, o.shoes, o.beard, o.glasses, o.lipstick, o.eyeshadow];
    if o.other.iter().any(|c| *c != crate::sim::Clothes::default()) {
        v.extend(o.other.iter().flat_map(|c| [c.top, c.bottom, c.full, c.shoes]));
    }
    v
}

impl SaveGame {
    pub fn sim(s: &SavedSim) -> Sim {
        let c = |v: [f32; 3]| Color::srgb(v[0], v[1], v[2]);
        let o = |i: usize| s.outfit.get(i).copied().flatten();
        let other = |n: usize| crate::sim::Clothes { top: o(9 + n * 4), bottom: o(10 + n * 4), full: o(11 + n * 4), shoes: o(12 + n * 4) };
        Sim {
            id: s.id,
            look: s.look,
            favorites: s.favorites.clone().unwrap_or_else(|| crate::sim::Favorites::by_look(s.look)),
            face: s.face.clone(),
            face_presets: s.face_presets.clone(),
            occult: s.occult.as_deref().and_then(crate::sim::Occult::from_name),
            outfit: OutfitChoice {
                hair: o(0),
                top: o(1),
                bottom: o(2),
                full: o(3),
                shoes: o(4),
                beard: o(5),
                glasses: o(6),
                lipstick: o(7),
                eyeshadow: o(8),
                eyeliner: s.eyeliner,
                blush: s.blush,
                brows: s.brows,
                accessories: s.accessories.clone(),
                skin_details: s.skin_details.clone(),
                body_hair: s.body_hair.clone(),
                other: [other(0), other(1), other(2), other(3), other(4)],
                designs: s.designs.clone(),
                styles: s.styles.clone(),
            },
            first: s.first.clone(),
            last: s.last.clone(),
            female: s.female,
            age: age_from_name(&s.age),
            traits: s.traits.iter().filter_map(|t| Trait::from_name(t)).collect(),
            weight: s.shape.map_or(0.0, |x| x.0),
            fitness: s.shape.map_or(0.0, |x| x.1),
            skin: c(s.skin),
            hair: c(s.hair),
            brow_hair: s.brow_hair.map(c),
            eyes: s.eyes.map_or_else(|| crate::sim::eyes_by_look(s.look), c),
            voice: s.voice.unwrap_or((s.look % 3) as u8).min(2),
            voice_pitch: s.voice_pitch.unwrap_or(0.5).clamp(0.0, 1.0),
            bio: s.bio.clone(),
            top: c(s.top),
            bottom: c(s.bottom),
        }
    }

    pub fn members(&self) -> Vec<Sim> {
        self.sims.iter().filter(|s| s.member).map(Self::sim).collect()
    }

    /// Sims the household knows (visitors and friends living elsewhere).
    pub fn known_sims(&self) -> Vec<Sim> {
        self.sims.iter().filter(|s| !s.member).map(Self::sim).collect()
    }

    pub fn file_name(&self) -> String {
        let clean = |s: &str| s.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect::<String>();
        format!("{} - {}.json", clean(&self.household), clean(&self.world))
    }
}

/// A save being loaded: its Sims are built while loading, the rest applied once in play.
#[derive(Resource, Clone)]
pub struct PendingLoad(pub SaveGame);

/// The lot's original furniture removed in buy mode (to remove again when loading).
#[derive(Resource, Default)]
pub struct RemovedLotObjects(pub Vec<SavedObject>);

/// Marks an object the household bought.
#[derive(Component)]
pub struct Bought;

#[derive(Message)]
pub struct SaveRequest;

/// A save into a new file of the game's own (one no other save has), where it goes on saving.
#[derive(Message)]
pub struct SaveAsRequest;

/// The file a `SaveAsRequest` saves to, named by the player (`gamepopup`); without one, a new
/// file no save has.
#[derive(Resource)]
pub struct SaveTo(pub PathBuf);

/// The game as it stands wanted, without saving it (changing household starts from it).
#[derive(Message)]
pub struct SnapshotRequest;

/// The game as it stood when asked for by a `SnapshotRequest`.
#[derive(Resource)]
pub struct Snapshot(pub SaveGame);

/// The game as last saved (moving house starts from it).
#[derive(Resource, Clone)]
pub struct LastSave(pub SaveGame);

/// The file this game is saved to: the one it was loaded from, or (for a new game) one chosen
/// at its first save that no other save had. A save never writes over another game's file.
#[derive(Resource, Default, Clone)]
pub struct SaveSlot(pub Option<PathBuf>);

/// A new game (not one loaded, nor one going on with another household) starts with no file
/// of its own.
fn new_game_slot(pending: Option<Res<PendingLoad>>, carry: Option<Res<crate::household::CarryOver>>, mut slot: ResMut<SaveSlot>) {
    if pending.is_none() && carry.is_none() {
        slot.0 = None;
    }
}

/// A file name in `dir` for `game` that no save has yet ("Goth - Sunset Valley (2).json", ...).
fn free_save_path(dir: &std::path::Path, game: &SaveGame) -> PathBuf {
    let base = game.file_name();
    let stem = base.trim_end_matches(".json");
    (1..)
        .map(|n| dir.join(if n == 1 { base.clone() } else { format!("{stem} ({n}).json") }))
        .find(|p| !p.exists() && !p.with_extension("json.bak").exists())
        .expect("a free file name")
}

/// Writes a save without ever leaving it half-written: the game goes to a temporary file first,
/// the save it replaces (this game's own, from before) is kept as `.json.bak`, and only then
/// does the new one take its place.
fn write_save(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    if path.exists() {
        std::fs::copy(path, path.with_extension("json.bak"))?;
    }
    std::fs::rename(&tmp, path)
}

/// Every save on disk, newest first.
pub fn list_saves() -> Vec<(PathBuf, SaveGame)> {
    let mut out: Vec<(PathBuf, SaveGame, std::time::SystemTime)> = std::fs::read_dir(saves_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()? != "json" {
                return None;
            }
            let g: SaveGame = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
            let t = e.metadata().ok()?.modified().ok()?;
            Some((p, g, t))
        })
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2));
    out.into_iter().map(|(p, g, _)| (p, g)).collect()
}

fn rgb(c: Color) -> [f32; 3] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue]
}

/// A life stage from its saved name.
pub fn age_from_name(s: &str) -> Age {
    match s {
        "Baby" => Age::Baby,
        "Toddler" => Age::Toddler,
        "Child" => Age::Child,
        "Teen" => Age::Teen,
        "Adult" => Age::Adult,
        "Elder" => Age::Elder,
        _ => Age::YoungAdult,
    }
}

pub fn age_name(a: Age) -> &'static str {
    match a {
        Age::Baby => "Baby",
        Age::Toddler => "Toddler",
        Age::Child => "Child",
        Age::Teen => "Teen",
        Age::YoungAdult => "YoungAdult",
        Age::Adult => "Adult",
        Age::Elder => "Elder",
    }
}

fn status_name(s: RelStatus) -> &'static str {
    match s {
        RelStatus::None => "None",
        RelStatus::Partner => "Partner",
        RelStatus::Engaged => "Engaged",
        RelStatus::Married => "Married",
        RelStatus::Ex => "Ex",
    }
}

pub(crate) fn status_from(s: &str) -> RelStatus {
    match s {
        "Partner" => RelStatus::Partner,
        "Engaged" => RelStatus::Engaged,
        "Married" => RelStatus::Married,
        "Ex" => RelStatus::Ex,
        _ => RelStatus::None,
    }
}

pub const SKILLS: [&str; 13] = ["Athletic", "Charisma", "Cooking", "Fishing", "Gardening", "Guitar", "Handiness", "Logic", "Painting", "Writing", "Piano", "Drums", "LaserHarp"];

fn saved_object(o: &GameObject, tf: &Transform, design: Option<&crate::objects::Design>, hung: Option<&crate::paintings::Hung>) -> SavedObject {
    let key = design.map(|d| d.0);
    let preset = key.filter(|k| k.0 == s3bake::gamedata::T_DESIGN && k.2 == o.objd.2).map(|k| k.1 as u8);
    SavedObject {
        objd: o.objd,
        position: tf.translation.to_array(),
        rotation: tf.rotation.to_array(),
        design: preset,
        design_key: key.filter(|_| preset.is_none()),
        item: hung.map(|h| h.0.clone()),
        lost: 0,
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn save_game(
    mut commands: Commands,
    (mut requests, mut snapshots, mut save_as, dormant): (MessageReader<SaveRequest>, MessageReader<SnapshotRequest>, MessageReader<SaveAsRequest>, Res<crate::household::Dormant>),
    clock: Res<GameClock>,
    world: Res<CurrentWorld>,
    (household, save_to): (Option<Res<Household>>, Option<Res<SaveTo>>),
    (removed, paint, garden, plants, collection, meals, discoveries): (
        Res<RemovedLotObjects>,
        Option<Res<crate::building::LotPaint>>,
        Option<Res<crate::gardening::Garden>>,
        Query<(&crate::gardening::GrowingPlant, &Transform)>,
        Res<crate::collecting::Collection>,
        crate::meals::SavableMeals,
        Res<crate::telescope::Discoveries>,
    ),
    sims: Query<
        (
            Entity,
            &Sim,
            &Transform,
            &Floor,
            &Motives,
            Option<&Skills>,
            &Moodlets,
            Option<&Job>,
            &Relationships,
            Has<HouseholdMember>,
            Has<Selected>,
            Has<OffLot>,
            Has<Visitor>,
            Option<&crate::wishes::Wishes>,
            (
                Option<&crate::aging::Aging>,
                Option<&crate::little::Pregnancy>,
                Option<&crate::opportunities::SimOpportunities>,
                Has<crate::visit::OnLot>,
                Option<&crate::lifetime::LifetimeWish>,
                Option<&crate::writing::Author>,
                Option<&crate::meals::KnownRecipes>,
                Option<&crate::chess::ChessRecord>,
                Option<&crate::inventory::Inventory>,
                Option<&crate::little::ToddlerSkills>,
                Option<&crate::careers::Pension>,
                Option<&crate::journal::SkillJournal>,
                Option<&crate::rabbitholes::SchoolGrades>,
                Option<&crate::rabbitholes::Homework>,
                (
                    Option<&crate::rabbitholes::AtRabbitHole>,
                    Option<&crate::weather::BodyTemperature>,
                    Option<&crate::simbody::Wearing>,
                    Has<crate::simbody::ChangedInto>,
                    Option<&crate::portraits::Staged>,
                    Option<&crate::classes::ClassesTaken>,
                    Option<&crate::skillbooks::SkillBooks>,
                    (Option<&crate::novels::Novels>, Has<crate::computertraits::BookClub>, Option<&crate::timeoff::TimeOff>, Option<&crate::charity::Charities>),
                    Has<crate::town::Townie>,
                    Has<crate::visit::LotGuest>,
                ),
            ),
        ),
        Without<crate::services::ServiceNpc>,
    >,
    (bought, exit, graves, ghosts): (
        Query<(&GameObject, &Transform, Option<&crate::objects::Design>, Option<&crate::paintings::Hung>), With<Bought>>,
        Option<Res<crate::interact::LotExit>>,
        Query<(Entity, &crate::ghosts::Grave, &Transform), Without<crate::buyhistory::HistoryHidden>>,
        Query<(Entity, &Sim, &crate::ghosts::Ghost, &Relationships)>,
    ),
    ui: Option<Res<crate::icons::GameUi>>,
    mut notes: ResMut<Notifications>,
    (story, alarm, bowls, (leftovers, fridge, real_estate), weather, pets, pet_data): (
        Res<crate::story::TownStory>,
        Res<crate::appliances::Alarm>,
        Query<(&crate::fishbowl::BowlFish, &Transform), (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>)>,
        (Res<crate::meals::Leftovers>, Res<crate::ingredients::Fridge>, Res<crate::realestate::RealEstate>),
        Res<crate::weather::Weather>,
        Query<(&crate::pets::Pet, &crate::pets::HomePet, &Transform, Option<&crate::petneeds::PetMotives>, Option<&crate::pets::PetBonds>)>,
        Res<crate::pets::PetData>,
    ),
    (mut slot, (maid, paper), mail_due, upgraded, family, strokes, sculpted, broken, family_inventory, puddles): (
        ResMut<SaveSlot>,
        (Res<crate::services::MaidService>, Res<crate::mail::PaperRound>),
        Option<Res<crate::services::MailDue>>,
        Query<(&GameObject, &Transform, &crate::upgrades::Upgrades)>,
        Res<crate::family::Genealogy>,
        Res<crate::terrain_paint::Strokes>,
        Res<crate::terrain_paint::Sculpted>,
        Query<
            (&GameObject, &Transform, Has<crate::interact::Broken>, Has<crate::grime::Charred>, Option<&crate::grime::Grime>, Has<crate::interact::Unmade>, Option<&crate::interact::Depreciated>, Option<&crate::doorlocks::DoorLock>, Option<&crate::bedowners::BedOwners>, Has<crate::dollhouse::Smashed>, Has<crate::trashcans::TippedOver>, Option<&crate::paintings::LeftOnEasel>, (Option<&crate::lights::Switched>, Option<&crate::lights::Dimmer>, Has<crate::frontdoor::FrontDoor>, Option<&crate::petneeds::BowlFood>)),
            (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>),
        >,
        Res<crate::familyinv::FamilyInventory>,
        Query<(&Transform, &crate::nav::Floor, &crate::puddles::Puddle)>,
    ),
    (laundry, posted, doorbell, repo): (crate::laundry::LaundrySave, Res<crate::rgmmail::Posted>, Res<crate::frontdoor::Doorbell>, Res<crate::repoman::RepoDue>),
) {
    let new_file = save_as.read().count() > 0;
    let write = requests.read().count() > 0 || new_file;
    let snapshot = snapshots.read().count() > 0;
    if !write && !snapshot {
        return;
    }
    let Some(hh) = household else { return };
    // (The ghosts too: whom the living know among the dead.)
    let ids: HashMap<Entity, u64> = sims.iter().map(|q| (q.0, q.1.id)).chain(ghosts.iter().map(|g| (g.0, g.1.id))).collect();
    // (Passers-by are kept only once the household knows them: then as Sims met, away.)
    let known: std::collections::HashSet<Entity> = sims.iter().filter(|q| q.9).flat_map(|q| q.8.0.keys().copied()).collect();
    let mut saved = Vec::new();
    for (e, sim, tf, floor, motives, skills, moodlets, job, rels, member, selected, away, visiting, wishes, (aging, pregnancy, opps, out, ltw, author, recipes, chess, inventory, toddler, pension, journal, school, homework, (rabbit, temperature, wearing, outfit_chosen, staged, classes, books, (novels, book_club, time_off, charities), townie, guest))) in &sims {
        if (townie || guest) && !known.contains(&e) {
            continue;
        }
        if let Some(s) = staged {
            debug!("saving {} during portrait: scene {:?}, gameplay {:?}", sim.first, tf.translation, s.original.translation);
        }
        let tf = staged.map_or(tf, |s| &s.original);
        // Out on a community lot: saved as back at home (the lot isn't kept).
        let (position, level) = match (out, exit.as_ref()) {
            (true, Some(x)) => ([x.0.x, world.data.heightmap.sample(x.0.x, x.0.y), x.0.y], 1),
            _ => (tf.translation.to_array(), floor.0),
        };
        let guid = |i: usize| ui.as_ref().and_then(|u| u.data.opportunities.get(i)).map(|o| o.guid.clone());
        saved.push(SavedSim {
            id: sim.id,
            look: sim.look,
            first: sim.first.clone(),
            last: sim.last.clone(),
            female: sim.female,
            age: age_name(sim.age).into(),
            traits: sim.traits.iter().map(|t| t.name().to_string()).collect(),
            skin: rgb(sim.skin),
            hair: rgb(sim.hair),
            top: rgb(sim.top),
            bottom: rgb(sim.bottom),
            member,
            selected,
            whereabouts: if member { "home" } else if away { "away" } else if visiting { "visiting" } else { "away" }.into(),
            position,
            yaw: tf.rotation.to_euler(EulerRot::YXZ).0,
            floor: level,
            motives: motives.0,
            skills: skills.map_or_else(Vec::new, |k| k.0.iter().map(|(k, v)| (k.to_string(), *v)).collect()),
            moodlets: moodlets.0.iter().filter(|m| m.until.is_finite()).map(|m| (m.kind.def().name.to_string(), m.until)).collect(),
            job: job.map(|j| SavedJob {
                track: j.career().name.into(),
                level: j.level,
                performance: j.performance,
                branch: j.path().branch.into(),
                tone: (j.tone != crate::careers::WorkTone::Normal).then(|| j.tone.name().into()),
                boss: j.boss,
                coworkers: j.coworkers.clone(),
                highest: j.highest,
                raise: j.raise,
                warned_at: j.warned_at,
                tally: j.tally.clone(),
            }),
            relationships: saved_rels(rels, &ids),
            lifetime_happiness: wishes.map_or(0, |w| w.points),
            happiness_fraction: wishes.map_or(0.0, |w| w.mood_fraction),
            body_temperature: temperature.copied().unwrap_or_default(),
            offered_wishes: wishes.map(|w| w.offered.clone()).unwrap_or_default(),
            promised_wishes: wishes.map(|w| w.promised.clone()).unwrap_or_default(),
            next_wish_offer: wishes.map(|w| w.next_offer),
            dream_state: wishes.and_then(|w| w.dream_state.clone()),
            outfit: saved_outfit(&sim.outfit),
        accessories: sim.outfit.accessories.clone(),
        eyeliner: sim.outfit.eyeliner,
        blush: sim.outfit.blush,
        brows: sim.outfit.brows,
        skin_details: sim.outfit.skin_details.clone(),
        body_hair: sim.outfit.body_hair.clone(),
            wearing: wearing.map(|w| w.0),
            outfit_chosen,
            rewards: wishes.map(|w| w.rewards.clone()).unwrap_or_default(),
            aging: aging.map(|a| (a.days, a.elder_span)),
            elder_risk: aging.and_then(|a| a.elder_risk),
            school_grade: school.map(|g| g.0),
            homework: homework.is_some(),
            homework_progress: homework.map_or(0.0, |h| h.0),
            school_visit: rabbit.filter(|r| std::ptr::eq(r.activity, &crate::rabbitholes::SCHOOL)).map(|r| (r.inside_from, r.until)),
            pregnancy: pregnancy.map(|p| (p.since, p.other_parent.and_then(|o| ids.get(&o).copied()), p.stage)),
            pregnancy_mood: pregnancy.map(|p| p.mood),
            pregnancy_sway: pregnancy.map(|p| (p.girl_bias, p.kids_hours)),
            shape: Some((sim.weight, sim.fitness)),
            opportunities: opps.map(|o| o.active.iter().filter_map(|a| Some((guid(a.index)?, a.deadline))).collect()).unwrap_or_default(),
            opportunities_done: opps.map(|o| o.done.clone()).unwrap_or_default(),
            opportunities_visited: opps.map(|o| o.active.iter().filter(|a| a.visited).filter_map(|a| guid(a.index)).collect()).unwrap_or_default(),
            opportunities_with: opps.map(|o| o.active.iter().filter_map(|a| Some((guid(a.index)?, a.with.as_ref()?.0, a.with.as_ref()?.1.clone()))).collect()).unwrap_or_default(),
            lifetime_wish: ltw.map(|l| SavedLifetimeWish { check: l.def().check.into(), fulfilled: l.fulfilled, careers: l.careers.clone(), raised: l.raised, rich_spouse: l.rich_spouse }),
            author: author.cloned(),
            recipes: recipes.map(|r| r.0.clone()).unwrap_or_default(),
            classes: classes.map(|c| c.0.clone()).unwrap_or_default(),
            skill_books: books.map(|b| b.0.clone()).unwrap_or_default(),
            novels: novels.map(|b| b.0.clone()).unwrap_or_default(),
            book_club,
            time_off: time_off.cloned(),
            charities: charities.cloned(),
            chess: chess.copied(),
            inventory: inventory.cloned().unwrap_or_default(),
            toddler: toddler.copied(),
            eyes: Some(rgb(sim.eyes)),
            brow_hair: sim.brow_hair.map(rgb),
            voice: Some(sim.voice),
            voice_pitch: (sim.voice_pitch != 0.5).then_some(sim.voice_pitch),
            bio: sim.bio.clone(),
            pension: pension.copied(),
            favorites: Some(sim.favorites.clone()),
            journal: journal.cloned().unwrap_or_default(),
            designs: sim.outfit.designs.clone(),
            styles: sim.outfit.styles.clone(),
            face: sim.face.clone(),
            face_presets: sim.face_presets.clone(),
            occult: sim.occult.map(|o| o.name().to_string()),
        });
    }
    let game = SaveGame {
        broken_objects: broken.iter().filter(|b| b.2).map(|(o, tf, _, charred, ..)| SavedBroken { objd: o.objd, position: tf.translation.to_array(), charred }).collect(),
        puddles: crate::puddles::saved(&clock, &puddles),
        laundry: laundry.saved(),
        posted: posted.0.clone(),
        doorbell: doorbell.0,
        repo_debt: repo.debt,
        object_states: broken
            .iter()
            .filter(|(.., grime, unmade, lost, lock, owners, smashed, tipped, canvas, (switched, dimmer, front, bowl))| {
                grime.is_some_and(|g| g.0 > 0.0) || *unmade || lost.is_some_and(|d| d.0 > 0) || lock.is_some() || owners.is_some_and(|o| o.0.iter().any(|s| s.is_some())) || *smashed || *tipped || canvas.is_some() || switched.is_some() || dimmer.is_some() || *front || bowl.is_some()
            })
            .map(|(o, tf, _, _, grime, unmade, lost, lock, owners, smashed, tipped, canvas, (switched, dimmer, front, bowl))| SavedObjectState {
                objd: o.objd,
                position: tf.translation.to_array(),
                grime: grime.map_or(0.0, |g| g.0),
                unmade,
                lost: lost.map_or(0, |d| d.0),
                lock: lock.cloned(),
                bed_owners: owners.copied().filter(|o| o.0.iter().any(|s| s.is_some())),
                smashed,
                tipped,
                canvas: canvas.map(|c| c.saved()),
                switched: switched.map(|s| s.0),
                dimmer: dimmer.map(|d| d.0),
                front,
                bowl: bowl.map(|b| b.0),
            })
            .collect(),
        version: SAVE_VERSION,
        world: world.name.clone(),
        lot_index: hh.lot_index,
        lot_name: world.data.lot_names.get(hh.lot_index).cloned().unwrap_or_default(),
        household: hh.name.clone(),
        funds: hh.funds,
        last_bill_day: hh.last_bill_day,
        // (Bills still in the mail count as come.)
        bills: hh.bills.iter().cloned().chain(mail_due.as_ref().map(|m| m.0.clone())).collect(),
        minutes: clock.minutes,
        sims: saved,
        bought: bought.iter().map(|(o, tf, d, h)| saved_object(o, tf, d, h)).collect(),
        removed: removed.0.clone(),
        paint: paint.map(|p| p.0.clone()).unwrap_or_default(),
        seeds: match (garden.as_deref(), ui.as_deref()) {
            (Some(g), Some(u)) => crate::gardening::saved_seeds(g, &u.data),
            _ => Vec::new(),
        },
        plants: ui.as_deref().map(|u| crate::gardening::saved_plants(&plants, &u.data)).unwrap_or_default(),
        perfect_produce: garden.as_deref().map(|g| g.perfect.clone()).unwrap_or_default(),
        town: story.clone(),
        alarm: *alarm,
        fishbowls: crate::fishbowl::saved(&bowls),
        maid: maid.hired,
        paper_cancelled: paper.cancelled,
        upgrades: crate::upgrades::saved(&upgraded),
        family: family.saved(),
        terrain: strokes.saved(),
        heights: sculpted.saved(),
        collection: collection.clone(),
        discoveries: discoveries.clone(),
        real_estate: real_estate.clone(),
        graves: graves
            .iter()
            .map(|(ge, g, tf)| {
                // (Whom the ghost knows, kept with its grave.)
                let known = ghosts.iter().find(|x| x.2.grave == ge).map_or_else(|| g.known.clone(), |x| saved_rels(x.3, &ids));
                SavedGrave { position: tf.translation.to_array(), rotation: tf.rotation.to_array(), cause: g.cause.clone(), sim: SavedSim { relationships: known, ..saved_look(&g.sim) }, epitaph: g.epitaph.clone() }
            })
            .collect(),
        leftovers: leftovers.0.clone(),
        leftovers_at: leftovers.1.clone(),
        fridge: fridge.0.clone(),
        meals: ui.as_deref().map(|u| crate::meals::saved_meals(&meals, &u.data)).unwrap_or_default(),
        weather: Some(weather.clone()),
        pets: crate::pets::saved(&pets, pet_data.0.as_deref()),
        dormant: dormant.kept(&sims.iter().filter(|q| q.9).map(|q| q.1.id).collect::<Vec<_>>()),
        homeless: false,
        deck: true,
        family_inventory: family_inventory.0.clone(),
    };
    if snapshot {
        commands.insert_resource(Snapshot(game.clone()));
    }
    if !write {
        return;
    }
    let dir = saves_dir();
    let _ = std::fs::create_dir_all(&dir);
    let named = save_to.filter(|_| new_file).map(|s| s.0.clone());
    commands.remove_resource::<SaveTo>();
    let path = named.or_else(|| slot.0.clone().filter(|_| !new_file)).unwrap_or_else(|| free_save_path(&dir, &game));
    match serde_json::to_vec_pretty(&game).map_err(|e| e.to_string()).and_then(|d| write_save(&path, &d).map_err(|e| e.to_string())) {
        Ok(()) => {
            slot.0 = Some(path);
            notes.push(format!("Game saved: the {} household in {}.", game.household, game.world));
        }
        Err(e) => notes.push(format!("Couldn't save the game: {e}")),
    }
    commands.insert_resource(LastSave(game));
}

/// A loaded game skips the lot chooser.
fn resume_saved_lot(
    mut commands: Commands,
    pending: Option<Res<PendingLoad>>,
    mut next: ResMut<NextState<PlayMode>>,
    mut done: Local<bool>,
) {
    let Some(p) = pending else {
        *done = false;
        return;
    };
    // (Another game to load, straight after one: from live play, the chooser never saw it go.)
    if p.is_added() {
        *done = false;
    }
    if *done {
        return;
    }
    *done = true;
    commands.insert_resource(crate::home::MoveInRequest(p.0.lot_index));
    next.set(PlayMode::Live);
}

fn restore_clothing(ec: &mut EntityCommands, saved: &SavedSim) {
    ec.remove::<(crate::simbody::Wearing, crate::simbody::ChangedInto)>();
    if let Some(outfit) = saved.wearing {
        ec.insert(crate::simbody::Wearing(outfit));
    }
    if saved.outfit_chosen {
        ec.insert(crate::simbody::ChangedInto);
    }
    ec.insert(crate::aging::NeedsNewBody);
}

/// Restore school progress, including absence of homework or student status.
fn restore_school(ec: &mut EntityCommands, saved: &SavedSim) {
    // Explicit removal also handles a restored student whose homework was completed.
    ec.remove::<(crate::rabbitholes::SchoolGrades, crate::rabbitholes::Homework, crate::rabbitholes::AtRabbitHole)>()
        .insert(Visibility::Inherited);
    if matches!(age_from_name(&saved.age), Age::Child | Age::Teen) {
        if let Some(grade) = saved.school_grade {
            ec.insert(crate::rabbitholes::SchoolGrades(grade.clamp(0.0, 100.0)));
        }
        if saved.homework {
            ec.insert(crate::rabbitholes::Homework(saved.homework_progress.clamp(0.0, 100.0), false));
        }
        if let Some((inside_from, until)) = saved.school_visit {
            ec.insert((crate::rabbitholes::AtRabbitHole {
                lot: usize::MAX,
                activity: &crate::rabbitholes::SCHOOL,
                inside_from,
                until,
                place: "school".into(),
            }, Visibility::Hidden));
        }
    }
}

/// Once the household is in, restores everything the save remembers.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn apply_loaded_game(
    mut commands: Commands,
    pending: Option<Res<PendingLoad>>,
    mut clock: ResMut<GameClock>,
    mut delta: ResMut<crate::clock::SimDelta>,
    mut household: Option<ResMut<Household>>,
    mut removed: ResMut<RemovedLotObjects>,
    mut sims: Query<
        (Entity, &mut Sim, &mut Transform, &mut Floor, &mut Motives, &mut Skills, &mut Moodlets, &mut Relationships),
        (Without<crate::town::Townie>, Without<GameObject>),
    >,
    objects: Query<(Entity, &GameObject, &Transform), (Without<Bought>, Without<Sim>)>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut building, mut faces): (Option<ResMut<crate::building::ActiveBuilding>>, Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>),
    mut grid: Option<ResMut<crate::nav::NavGrid>>,
    (mut notes, world): (ResMut<Notifications>, Res<crate::loading::CurrentWorld>),
) {
    let Some(p) = pending else { return };
    if household.is_none() || sims.is_empty() {
        return;
    }
    let game = &p.0;
    clock.minutes = game.minutes;
    // The clock was replaced, so this frame's pre-load elapsed time must not
    // advance restored temperature, recovery timers, or other simulation state.
    delta.0 = 0.0;
    commands.insert_resource(PendingBroken(game.broken_objects.clone(), None));
    commands.insert_resource(PendingStates(game.object_states.clone(), None));
    commands.insert_resource(crate::puddles::PendingPuddles(game.puddles.clone()));
    commands.insert_resource(crate::laundry::PendingLaundry::of(game.laundry.clone()));
    commands.insert_resource(crate::rgmmail::Posted(game.posted.clone()));
    commands.insert_resource(crate::frontdoor::Doorbell(game.doorbell));
    commands.insert_resource(crate::repoman::RepoDue { debt: game.repo_debt, arrive_at: None });
    // A restored date is not a midnight tick, even when loading within a session.
    commands.insert_resource(crate::aging::AgingDay::default());
    commands.insert_resource(crate::interact::DepreciationDay::default());
    if let Some(h) = household.as_mut() {
        h.funds = game.funds;
        h.last_bill_day = game.last_bill_day;
        h.bills = game.bills.clone();
        h.name = game.household.clone();
    }
    let by_id: HashMap<u64, Entity> = sims.iter().map(|q| (q.1.id, q.0)).collect();
    let mut unmet: Vec<(Entity, SavedRel)> = Vec::new();
    for s in &game.sims {
        let Some(&e) = by_id.get(&s.id) else { continue };
        let Ok((_, _, mut tf, mut floor, mut motives, mut skills, mut moodlets, mut rels)) = sims.get_mut(e) else { continue };
        tf.translation = match building.as_deref() {
            Some(b) if !game.deck && s.floor <= 1 => crate::building::onto_deck(b, &world.data.heightmap, Vec3::from(s.position)),
            _ => Vec3::from(s.position),
        };
        tf.rotation = Quat::from_rotation_y(s.yaw);
        floor.0 = s.floor.max(1);
        motives.0 = s.motives;
        skills.0 = s.skills.iter().filter_map(|(k, v)| SKILLS.iter().find(|x| *x == k).map(|x| (*x, *v))).collect();
        moodlets.0.clear();
        for (name, until) in &s.moodlets {
            if let Some(k) = MoodletKind::from_name(name) {
                moodlets.0.push(crate::life::Moodlet { kind: k, value: k.def().value, until: *until, felt: false });
            }
        }
        rels.0.clear();
        for r in &s.relationships {
            match by_id.get(&r.with) {
                Some(&other) => restore_rel(rels.entry(other), r),
                // (Someone not about yet, a ghost in its grave: known again once they rise.)
                None => unmet.push((e, r.clone())),
            }
        }
        let mut ec = commands.entity(e);
        if let Some((days, elder_span)) = s.aging {
            ec.insert(crate::aging::Aging { days, elder_span, elder_risk: s.elder_risk });
        }
        restore_school(&mut ec, s);
        restore_clothing(&mut ec, s);
        ec.insert(s.body_temperature);
        if let Some(a) = &s.author {
            ec.insert(a.clone());
        }
        if let Some(p) = s.pension {
            ec.insert(p);
        }
        if !s.journal.is_empty() {
            ec.insert(s.journal.clone());
        }
        if !s.recipes.is_empty() {
            ec.insert(crate::meals::KnownRecipes(s.recipes.clone()));
        }
        if !s.classes.is_empty() {
            ec.insert(crate::classes::ClassesTaken(s.classes.clone()));
        }
        if !s.novels.is_empty() {
            ec.insert(crate::novels::Novels(s.novels.clone()));
            if let Some(t) = &s.time_off {
                ec.insert(t.clone());
            }
            if let Some(c) = &s.charities {
                ec.insert(c.clone());
            }
            if s.book_club {
                ec.insert(crate::computertraits::BookClub);
            }
        }
        if !s.skill_books.is_empty() {
            ec.insert(crate::skillbooks::SkillBooks(s.skill_books.clone()));
        }
        if let Some(c) = s.chess {
            ec.insert(c);
        }
        if !s.inventory.0.is_empty() {
            ec.insert(s.inventory.clone());
        }
        if let Some(t) = s.toddler {
            ec.insert(t);
        }
        if let Some(l) = &s.lifetime_wish
            && let Some(wish) = crate::lifetime::LifetimeWish::by_check(&l.check)
        {
            ec.insert(crate::lifetime::LifetimeWish { fulfilled: l.fulfilled, careers: l.careers.clone(), raised: l.raised, rich_spouse: l.rich_spouse, ..crate::lifetime::LifetimeWish::new(wish) });
        }
        if !s.opportunities.is_empty() || !s.opportunities_done.is_empty() {
            ec.insert(crate::opportunities::PendingOpportunities(s.opportunities.clone(), s.opportunities_done.clone(), s.opportunities_visited.clone(), s.opportunities_with.clone()));
        }
        if let Some((since, other, stage)) = s.pregnancy {
            let other_parent = other.and_then(|o| by_id.get(&o).copied());
            // (The hours gone by aren't gone over again: what's due comes at the next.)
            let hour = (((game.minutes - since) / 60.0).max(0.0) as u32).min(crate::little::HOURS_OF_PREGNANCY - 1);
            let (girl_bias, kids_hours) = s.pregnancy_sway.unwrap_or_default();
            ec.insert(crate::little::Pregnancy { since, other_parent, stage, mood: s.pregnancy_mood.unwrap_or(0.0), hour, girl_bias, kids_hours });
            if stage >= 4 {
                ec.insert(crate::little::Labour { next: game.minutes });
            }
        }
        match &s.job {
            Some(j) => {
                if let Some(track) = crate::careers::careers().iter().position(|c| c.name == j.track) {
                    let mut job = crate::careers::Job::new(track);
                    job.branch = crate::careers::careers()[track].path_index(&j.branch).unwrap_or(0);
                    job.level = j.level.min(job.levels().len() - 1);
                    job.performance = j.performance;
                    job.tone = j.tone.as_deref().map(crate::careers::WorkTone::from_name).unwrap_or_default();
                    job.boss = j.boss;
                    job.coworkers = j.coworkers.clone();
                    job.highest = j.highest.max(job.level);
                    job.raise = j.raise;
                    job.warned_at = j.warned_at;
                    job.tally = j.tally.clone();
                    ec.insert(job);
                }
            }
            None => {
                ec.remove::<crate::careers::Job>();
            }
        }
        if s.member || s.next_wish_offer.is_some() {
            let mut wishes = crate::wishes::Wishes::restored(
                s.lifetime_happiness,
                s.rewards.clone(),
                game.minutes,
            );
            wishes.mood_fraction = s.happiness_fraction.clamp(0.0, 1.0);
            wishes.offered = s.offered_wishes.clone();
            wishes.promised = s.promised_wishes.clone();
            wishes.next_offer = s.next_wish_offer.unwrap_or(game.minutes);
            wishes.dream_state = s.dream_state.clone();
            ec.insert(wishes);
        }
        if s.member {
            ec.insert(HouseholdMember).remove::<(Visitor, OffLot)>();
        } else if s.whereabouts == "visiting" {
            ec.insert(Visitor { leave_at: game.minutes + 180.0 });
        } else {
            ec.remove::<Visitor>().insert((OffLot, Visibility::Hidden));
        }
        if s.selected {
            ec.insert(Selected);
        } else {
            ec.remove::<Selected>();
        }
    }
    // Furniture: take away what was sold, put back what was bought.
    // (By where it stood on the ground: the starter deck's furniture stands higher than it did
    // in older saves.)
    for r in &game.removed {
        if let Some((e, _, _)) = objects
            .iter()
            .find(|(_, o, tf)| o.objd == r.objd && tf.translation.xz().distance(Vec3::from(r.position).xz()) < 0.1 && (tf.translation.y - r.position[1]).abs() < 3.0)
        {
            commands.entity(e).try_despawn();
        }
    }
    // (A copy of the building as it stands, for lifting old positions onto the deck.)
    let deck = building.as_deref().filter(|b| !game.deck && !b.data.is_house()).map(|b| (b.corner, b.rot, b.data.clone(), b.levels.clone()));
    let lift = |p: Vec3| match &deck {
        Some((corner, rot, data, levels)) => {
            let l = rot.inverse() * (p - *corner);
            let on = data.floors.iter().any(|f| f.level == 1 && f.x as i32 == l.x.floor() as i32 && f.z as i32 == l.z.floor() as i32);
            if on { p + Vec3::Y * (levels.get(1).copied().unwrap_or(p.y) - world.data.heightmap.sample(p.x, p.z)).max(0.0) } else { p }
        }
        None => p,
    };
    removed.0 = game.removed.clone();
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    // The walls and floors as the household left them.
    if let Some(b) = building.as_deref_mut() {
        crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &game.paint, &mut faces);
    }
    commands.insert_resource(crate::building::LotPaint(game.paint.clone()));
    commands.insert_resource(crate::gardening::PendingPlants(game.plants.clone(), game.seeds.clone(), game.perfect_produce.clone()));
    commands.insert_resource(game.town.clone());
    commands.insert_resource(game.alarm);
    commands.insert_resource(crate::upgrades::PendingUpgrades(game.upgrades.clone(), 0.0));
    let heights = game.heights.clone();
    commands.queue(move |w: &mut World| w.resource_mut::<crate::terrain_paint::Sculpted>().pending = heights);
    let painted = game.terrain.clone();
    commands.queue(move |w: &mut World| w.resource_mut::<crate::terrain_paint::Strokes>().restore(&painted));
    let people = game.family.clone();
    commands.queue(move |w: &mut World| w.resource_mut::<crate::family::Genealogy>().restore(&people));
    let hired = game.maid;
    commands.queue(move |w: &mut World| w.resource_mut::<crate::services::MaidService>().hired = hired);
    let stopped = game.paper_cancelled;
    commands.queue(move |w: &mut World| w.resource_mut::<crate::mail::PaperRound>().cancelled = stopped);
    commands.insert_resource(crate::fishbowl::PendingBowls(game.fishbowls.clone()));
    commands.insert_resource(game.collection.clone());
    commands.insert_resource(game.discoveries.clone());
    commands.insert_resource(game.real_estate.clone());
    let mut at = game.leftovers_at.clone();
    at.resize(game.leftovers.len(), game.minutes);
    commands.insert_resource(crate::meals::Leftovers(game.leftovers.clone(), at));
    commands.insert_resource(crate::ingredients::Fridge(game.fridge.clone()));
    commands.insert_resource(crate::meals::PendingMeals(game.meals.clone()));
    commands.insert_resource(game.weather.clone().unwrap_or_default());
    commands.insert_resource(crate::pets::PendingPets(game.pets.clone()));
    commands.insert_resource(crate::household::Dormant(game.dormant.clone()));
    // The household's dead, back in their graves.
    if let Some(entry) = data.0.catalog.iter().find(|c| c.instance_name == "UrnstoneHuman") {
        for g in &game.graves {
            let sim = SaveGame::sim(&g.sim);
            let label = format!("{}'s Tombstone", sim.full_name());
            if let Some(o) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, entry.objd, lift(Vec3::from(g.position)), Quat::from_array(g.rotation)) {
                commands.entity(o.entity).insert(crate::ghosts::Grave { sim, cause: g.cause.clone(), epitaph: g.epitaph.clone(), known: g.sim.relationships.clone() }).queue_silenced(move |mut w: EntityWorldMut| {
                    if let Some(mut obj) = w.get_mut::<GameObject>() {
                        obj.name = label;
                    }
                });
            }
        }
    }
    commands.insert_resource(crate::ghosts::PendingRels(unmet));
    commands.insert_resource(crate::familyinv::FamilyInventory(game.family_inventory.clone()));
    for b in &game.bought {
        let rot = Quat::from_array(b.rotation);
        if let Some(o) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, b.objd, lift(Vec3::from(b.position)), rot, b.design_texture()) {
            commands.entity(o.entity).insert(Bought);
            if let Some(item) = &b.item {
                commands.entity(o.entity).insert(crate::paintings::Hung(item.clone()));
            }
        }
    }
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
    notes.0.clear();
    notes.push(format!("Welcome back to the {} household!", game.household));
    commands.remove_resource::<PendingLoad>();
}

/// Starts loading a saved game: the world, then the household. It saves back to `path`, the
/// file it came from.
pub fn begin_load(commands: &mut Commands, worlds: &crate::data::WorldList, game: SaveGame, path: Option<PathBuf>) -> bool {
    let Some(w) = worlds.0.iter().find(|w| w.path.file_stem().is_some_and(|s| s.to_string_lossy() == game.world) || w.name == game.world) else {
        return false;
    };
    commands.insert_resource(crate::data::SelectedWorld(w.clone()));
    commands.insert_resource(crate::home::PendingHousehold { last_name: game.household.clone(), members: game.members(), premade: None, ties: Vec::new() });
    commands.insert_resource(PendingLoad(game));
    commands.insert_resource(SaveSlot(path));
    true
}

/// Records the lot's own furniture being picked up or sold.
pub fn note_removed(removed: &mut RemovedLotObjects, o: &GameObject, tf: &Transform) {
    removed.0.push(saved_object(o, tf, None, None));
}

pub fn request_save(w: &mut MessageWriter<SaveRequest>) {
    w.write(SaveRequest);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worn_outfit_and_dresser_choice_restore_and_legacy_saves_clear_them() {
        use crate::simbody::{ChangedInto, OutfitKind, Wearing};
        let mut rng = rand::rng();
        let mut saved = saved_look(&random_sim(&mut rng, "Clothing", Some(true), Age::Adult));
        let mut world = World::new();
        let e = world.spawn_empty().id();
        let apply = |world: &mut World, saved: &SavedSim| {
            let mut queue = bevy::ecs::world::CommandQueue::default();
            restore_clothing(&mut Commands::new(&mut queue, world).entity(e), saved);
            queue.apply(world);
        };
        for outfit in OutfitKind::CHOICES {
            for chosen in [false, true] {
                saved.wearing = Some(outfit);
                saved.outfit_chosen = chosen;
                let restored: SavedSim = serde_json::from_value(serde_json::to_value(&saved).unwrap()).unwrap();
                apply(&mut world, &restored);
                assert_eq!(world.get::<Wearing>(e).unwrap().0, outfit);
                assert_eq!(world.get::<ChangedInto>(e).is_some(), chosen);
                assert!(world.get::<crate::aging::NeedsNewBody>(e).is_some());
            }
        }
        let mut json = serde_json::to_value(&saved).unwrap();
        json.as_object_mut().unwrap().remove("wearing");
        json.as_object_mut().unwrap().remove("outfit_chosen");
        let legacy: SavedSim = serde_json::from_value(json).unwrap();
        apply(&mut world, &legacy);
        assert!(world.get::<Wearing>(e).is_none());
        assert!(world.get::<ChangedInto>(e).is_none());
    }

    #[test]
    fn weather_exposure_survives_save_and_legacy_saves_start_comfortable() {
        let mut rng = rand::rng();
        let mut saved = saved_look(&random_sim(&mut rng, "Weather", Some(true), Age::Adult));
        saved.body_temperature = crate::weather::BodyTemperature { value: -94.75, in_rain: 49.5 };
        let mut json = serde_json::to_value(&saved).unwrap();
        let restored: SavedSim = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored.body_temperature.value, -94.75);
        assert_eq!(restored.body_temperature.in_rain, 49.5);
        json.as_object_mut().unwrap().remove("body_temperature");
        let legacy: SavedSim = serde_json::from_value(json).unwrap();
        assert_eq!(legacy.body_temperature.value, 0.0);
        assert_eq!(legacy.body_temperature.in_rain, 0.0);
    }

    #[test]
    fn broken_objects_restore_after_spawn_without_breaking_neighbours_or_repaired_objects() {
        let saved = SavedBroken { objd: (1, 2, 3), position: [4.0, 0.0, 8.0], charred: false };
        let saved: SavedBroken = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        let mut app = App::new();
        app.init_resource::<Time>().insert_resource(PendingBroken(vec![saved], None)).add_systems(Update, restore_broken);
        app.update();
        assert_eq!(app.world().resource::<PendingBroken>().0.len(), 1, "wait for bought furniture to spawn");
        let obj = GameObject { kind: crate::interact::ObjectKind::Computer, name: "Computer".into(), objd: (1, 2, 3), price: 100, center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None };
        let neighbour = app.world_mut().spawn((obj.clone(), Transform::from_xyz(5.0, 0.0, 8.0))).id();
        let target = app.world_mut().spawn((obj, Transform::from_xyz(4.0, 0.0, 8.0))).id();
        app.update();
        assert!(app.world().get::<crate::interact::Broken>(target).is_some());
        assert!(app.world().get::<crate::interact::Broken>(neighbour).is_none());
        assert!(app.world().resource::<PendingBroken>().0.is_empty());
        app.world_mut().entity_mut(target).remove::<crate::interact::Broken>();
        app.update();
        assert!(app.world().get::<crate::interact::Broken>(target).is_none(), "restoration must not replay after a repair");
    }

    #[test]
    fn grime_and_unmade_beds_come_back_once_the_furniture_has() {
        let toilet = SavedObjectState { objd: (1, 2, 3), position: [4.0, 0.0, 8.0], grime: 45.0, unmade: false, lost: 0, lock: None, bed_owners: None, smashed: false, tipped: false, canvas: None, switched: None, dimmer: None, front: false, bowl: None };
        let bed = SavedObjectState { objd: (1, 2, 4), position: [9.0, 0.0, 8.0], grime: 0.0, unmade: true, lost: 0, lock: None, bed_owners: None, smashed: false, tipped: false, canvas: None, switched: None, dimmer: None, front: false, bowl: None };
        let mut app = App::new();
        app.init_resource::<Time>().insert_resource(PendingStates(vec![toilet, bed], None)).add_systems(Update, restore_states);
        let obj = |kind, objd| GameObject { kind, name: "x".into(), objd, price: 100, center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None };
        let t = app.world_mut().spawn((obj(crate::interact::ObjectKind::Toilet, (1, 2, 3)), Transform::from_xyz(4.0, 0.0, 8.0))).id();
        let b = app.world_mut().spawn((obj(crate::interact::ObjectKind::BedDouble, (1, 2, 4)), Transform::from_xyz(9.0, 0.0, 8.0))).id();
        app.update();
        assert!(app.world().get::<crate::interact::Unmade>(b).is_some());
        assert_eq!(app.world().resource::<PendingStates>().0.len(), 1, "the toilet waits for its clean slate");
        app.world_mut().entity_mut(t).insert(crate::grime::Grime::default());
        app.update();
        assert_eq!(app.world().get::<crate::grime::Grime>(t).map(|g| g.0), Some(45.0));
        assert!(app.world().resource::<PendingStates>().0.is_empty());
    }

    #[test]
    fn saved_sim_keeps_wish_order_rewards_and_offer_deadline() {
        use crate::wishes::{Wish, WishKind};
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(18);
        let mut saved = saved_look(&random_sim(&mut rng, "Wishes", Some(true), Age::Adult));
        saved.offered_wishes = vec![Wish { kind: WishKind::Social("Chat".into()), points: 100 }];
        saved.promised_wishes = vec![Wish { kind: WishKind::MakeFriend, points: 400 }, Wish { kind: WishKind::Activity("Paint".into()), points: 250 }];
        saved.next_wish_offer = Some(1777.0);
        let mut json = serde_json::to_value(&saved).unwrap();
        let restored: SavedSim = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored.offered_wishes, saved.offered_wishes);
        assert_eq!(restored.promised_wishes, saved.promised_wishes);
        assert_eq!(restored.next_wish_offer, Some(1777.0));
        for field in ["offered_wishes", "promised_wishes", "next_wish_offer"] { json.as_object_mut().unwrap().remove(field); }
        let old: SavedSim = serde_json::from_value(json).unwrap();
        assert!(old.offered_wishes.is_empty() && old.promised_wishes.is_empty());
        assert_eq!(old.next_wish_offer, None);
    }

    #[test]
    fn school_progress_survives_serialization_and_restores_component_presence() {
        use crate::rabbitholes::{Homework, SchoolGrades};
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(8);
        let sim = random_sim(&mut rng, "Student", Some(true), Age::Teen);
        let mut saved = saved_look(&sim);
        saved.school_grade = Some(83.0);
        saved.homework = true;
        saved.school_visit = Some((540.0, 865.0));
        saved.homework_progress = 42.5;
        saved.happiness_fraction = 0.625;
        let mut saved: SavedSim = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(saved.happiness_fraction, 0.625);
        let mut world = World::new();
        let e = world.spawn_empty().id();
        let apply = |world: &mut World, saved: &SavedSim| {
            let mut queue = bevy::ecs::world::CommandQueue::default();
            restore_school(&mut Commands::new(&mut queue, world).entity(e), saved);
            queue.apply(world);
        };
        apply(&mut world, &saved);
        assert_eq!(world.get::<SchoolGrades>(e).unwrap().0, 83.0);
        assert!(world.get::<Homework>(e).is_some());
        assert_eq!(world.get::<Homework>(e).unwrap().0, 42.5);
        let visit = world.get::<crate::rabbitholes::AtRabbitHole>(e).unwrap();
        assert!(std::ptr::eq(visit.activity, &crate::rabbitholes::SCHOOL));
        assert_eq!((visit.inside_from, visit.until), (540.0, 865.0));
        assert_eq!(*world.get::<Visibility>(e).unwrap(), Visibility::Hidden);
        saved.homework = false;
        saved.school_visit = None;
        apply(&mut world, &saved);
        assert!(world.get::<Homework>(e).is_none());
        assert!(world.get::<crate::rabbitholes::AtRabbitHole>(e).is_none());
        assert_eq!(*world.get::<Visibility>(e).unwrap(), Visibility::Inherited);
        assert_eq!(world.get::<SchoolGrades>(e).unwrap().0, 83.0);
        saved.homework = true;
        saved.age = "YoungAdult".into();
        apply(&mut world, &saved);
        assert!(world.get::<SchoolGrades>(e).is_none());
        assert!(world.get::<Homework>(e).is_none());
        let mut old = serde_json::to_value(&saved).unwrap();
        old.as_object_mut().unwrap().remove("school_grade");
        old.as_object_mut().unwrap().remove("homework");
        old.as_object_mut().unwrap().remove("homework_progress");
        old.as_object_mut().unwrap().remove("happiness_fraction");
        old.as_object_mut().unwrap().remove("school_visit");
        let old: SavedSim = serde_json::from_value(old).unwrap();
        assert_eq!(old.school_grade, None);
        assert!(!old.homework);
        assert_eq!(old.homework_progress, 0.0);
        assert_eq!(old.happiness_fraction, 0.0);
        assert_eq!(old.school_visit, None);
    }

    fn close(a: Color, b: Color) -> bool {
        let (a, b) = (a.to_srgba(), b.to_srgba());
        (a.red - b.red).abs() < 1e-4 && (a.green - b.green).abs() < 1e-4 && (a.blue - b.blue).abs() < 1e-4
    }

    #[test]
    fn eye_colour_saved_and_restored() {
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(7);
        let mut sim = random_sim(&mut rng, "Test", Some(true), Age::Adult);
        let (r, g, b) = EYES[3];
        sim.eyes = Color::srgb(r, g, b);
        let json = serde_json::to_string(&saved_look(&sim)).unwrap();
        let back: SavedSim = serde_json::from_str(&json).unwrap();
        assert!(close(SaveGame::sim(&back).eyes, sim.eyes));
    }

    #[test]
    fn bought_objects_keep_their_design() {
        let old: SavedObject = serde_json::from_str(r#"{"objd":[1,0,2],"position":[1.0,2.0,3.0],"rotation":[0.0,0.0,0.0,1.0]}"#).unwrap();
        assert_eq!(old.design, None);
        assert_eq!(old.design_texture(), None);
        let o = SavedObject { design: Some(3), ..old.clone() };
        let back: SavedObject = serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(back.design_texture(), Some(crate::objects::design_texture((1, 0, 2), 3)));
        // (A lot's own design, by its texture.)
        let o = SavedObject { design_key: Some((7, 0, 9)), ..old.clone() };
        let back: SavedObject = serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(back.design_texture(), Some((7, 0, 9)));
        // (A hung painting keeps what it is; other things say nothing of it.)
        assert!(!serde_json::to_string(&old).unwrap().contains("item"));
        let item = crate::inventory::Stack { kind: crate::inventory::ItemKind::Painting, key: "painting:1:3_2_Medium#7".into(), name: "Fine Painting".into(), quality: 0, count: 1, worth: 210 };
        let o = SavedObject { item: Some(item), ..old };
        let back: SavedObject = serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert!(back.item.is_some_and(|i| i.key == "painting:1:3_2_Medium#7" && i.worth == 210));
    }

    #[test]
    fn other_outfits_saved_and_old_saves_load() {
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(3);
        let mut sim = random_sim(&mut rng, "Test", Some(true), Age::Adult);
        // (Nothing chosen beyond the everyday look: saved as before.)
        assert_eq!(saved_outfit(&sim.outfit).len(), 9);
        sim.outfit.wear(crate::simbody::OutfitKind::Formal, s3formats::sim::CT_BODY, (1, 0, 7));
        sim.outfit.wear(crate::simbody::OutfitKind::Swimwear, s3formats::sim::CT_SHOES, (1, 0, 9));
        let saved = saved_look(&sim);
        assert_eq!(saved.outfit.len(), 29);
        let back = SaveGame::sim(&serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap());
        assert_eq!(back.outfit, sim.outfit);
        assert_eq!(back.outfit.clothes(crate::simbody::OutfitKind::Formal).full, Some((1, 0, 7)));
        // (A save from before outerwear: its four other outfits, outerwear as their look gives it.)
        let mut four = saved.clone();
        four.outfit.truncate(25);
        let four = SaveGame::sim(&four);
        assert_eq!(four.outfit.clothes(crate::simbody::OutfitKind::Formal).full, Some((1, 0, 7)));
        assert_eq!(four.outfit.clothes(crate::simbody::OutfitKind::Outerwear), crate::sim::Clothes::default());
        // An old save's nine: the other outfits as their look gives them.
        let mut old = saved.clone();
        old.outfit.truncate(9);
        assert_eq!(SaveGame::sim(&old).outfit.other, [crate::sim::Clothes::default(); 5]);
        // Favourites kept; from before them, the same ones each time by the Sim's look.
        assert_eq!(back.favorites, sim.favorites);
        old.favorites = None;
        assert_eq!(SaveGame::sim(&old).favorites, SaveGame::sim(&old).favorites);
        assert!(crate::sim::FAVORITE_FOODS.contains(&SaveGame::sim(&old).favorites.food.as_str()));
    }

    #[test]
    fn saves_from_before_eye_colours_still_load() {
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(8);
        let sim = random_sim(&mut rng, "Test", Some(false), Age::Adult);
        let mut v = serde_json::to_value(saved_look(&sim)).unwrap();
        v.as_object_mut().unwrap().remove("eyes");
        let old: SavedSim = serde_json::from_value(v).unwrap();
        let loaded = SaveGame::sim(&old);
        assert!(close(loaded.eyes, eyes_by_look(sim.look)));
        // (And a household's Sims don't all get the same eyes.)
        let colours: std::collections::HashSet<usize> = (0..64u64)
            .map(|look| {
                let c = eyes_by_look(look.wrapping_mul(0x1234_5678_9ABC_DEF1)).to_srgba();
                EYES.iter().position(|e| (e.0 - c.red).abs() < 1e-4 && (e.1 - c.green).abs() < 1e-4).unwrap()
            })
            .collect();
        assert_eq!(colours.len(), EYES.len());
    }
}
