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
            .init_resource::<SaveSlot>()
            .add_message::<SaveRequest>()
            .add_message::<SnapshotRequest>()
            .add_message::<SaveAsRequest>()
            .add_systems(OnEnter(crate::AppState::Loading), new_game_slot)
            .add_systems(Update, resume_saved_lot.run_if(in_state(PlayMode::ChooseLot)))
            .add_systems(Update, (apply_loaded_game, save_game).run_if(in_state(PlayMode::Live)));
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
    #[serde(default)]
    pub rewards: Vec<String>,
    /// Days into the current life stage, and how long old age lasts.
    #[serde(default)]
    pub aging: Option<(f32, f32)>,
    /// Pregnant since (game minutes), with the other parent's id and the stage shown so far.
    #[serde(default)]
    pub pregnancy: Option<(f64, Option<u64>, u8)>,
    /// Body shape: weight and fitness.
    #[serde(default)]
    pub shape: Option<(f32, f32)>,
    /// Opportunities taken on (by the game's id, with their deadline) and done.
    #[serde(default)]
    pub opportunities: Vec<(String, Option<f64>)>,
    #[serde(default)]
    pub opportunities_done: Vec<String>,
    #[serde(default)]
    pub lifetime_wish: Option<SavedLifetimeWish>,
    /// The book under way and those written.
    #[serde(default)]
    pub author: Option<crate::writing::Author>,
    /// Recipes learned from recipe books.
    #[serde(default)]
    pub recipes: Vec<String>,
    /// Ranked chess: wins, losses and rank.
    #[serde(default)]
    pub chess: Option<crate::chess::ChessRecord>,
    /// Their face shape, as sculpted in Create a Sim or inherited (none: as their look has it).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub face: Vec<f32>,
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
}

impl SavedObject {
    /// The design's texture.
    pub fn design_texture(&self) -> Option<s3bake::Key> {
        self.design_key.or(self.design.map(|d| crate::objects::design_texture(self.objd, d)))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SaveGame {
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
    /// Tombstones on the lot, with who lies there.
    #[serde(default)]
    pub graves: Vec<SavedGrave>,
    /// Leftovers in the fridge (servings, by recipe).
    #[serde(default)]
    pub leftovers: Vec<String>,
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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedGrave {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub cause: String,
    pub sim: SavedSim,
}

/// How a Sim looks, for saving someone no longer about (the rest left empty).
fn saved_look(sim: &Sim) -> SavedSim {
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
        rewards: Vec::new(),
        aging: None,
        pregnancy: None,
        shape: Some((sim.weight, sim.fitness)),
        opportunities: Vec::new(),
        opportunities_done: Vec::new(),
        lifetime_wish: None,
        author: None,
        recipes: Vec::new(),
        chess: None,
        inventory: Default::default(),
        toddler: None,
        eyes: Some(rgb(sim.eyes)),
        voice: Some(sim.voice),
        pension: None,
        favorites: Some(sim.favorites.clone()),
        journal: Default::default(),
        designs: sim.outfit.designs.clone(),
        styles: sim.outfit.styles.clone(),
        face: sim.face.clone(),
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
                other: [other(0), other(1), other(2), other(3)],
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
            eyes: s.eyes.map_or_else(|| crate::sim::eyes_by_look(s.look), c),
            voice: s.voice.unwrap_or((s.look % 3) as u8).min(2),
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

pub const SKILLS: [&str; 10] = ["Athletic", "Charisma", "Cooking", "Fishing", "Gardening", "Guitar", "Handiness", "Logic", "Painting", "Writing"];

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
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn save_game(
    mut commands: Commands,
    (mut requests, mut snapshots, mut save_as, dormant): (MessageReader<SaveRequest>, MessageReader<SnapshotRequest>, MessageReader<SaveAsRequest>, Res<crate::household::Dormant>),
    clock: Res<GameClock>,
    world: Res<CurrentWorld>,
    household: Option<Res<Household>>,
    (removed, paint, garden, plants, collection): (
        Res<RemovedLotObjects>,
        Option<Res<crate::building::LotPaint>>,
        Option<Res<crate::gardening::Garden>>,
        Query<(&crate::gardening::GrowingPlant, &Transform)>,
        Res<crate::collecting::Collection>,
    ),
    sims: Query<
        (
            Entity,
            &Sim,
            &Transform,
            &Floor,
            &Motives,
            &Skills,
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
            ),
        ),
        (Without<crate::town::Townie>, Without<crate::visit::LotGuest>, Without<crate::services::ServiceNpc>),
    >,
    (bought, exit, graves): (
        Query<(&GameObject, &Transform, Option<&crate::objects::Design>, Option<&crate::paintings::Hung>), With<Bought>>,
        Option<Res<crate::interact::LotExit>>,
        Query<(&crate::ghosts::Grave, &Transform)>,
    ),
    ui: Option<Res<crate::icons::GameUi>>,
    mut notes: ResMut<Notifications>,
    (story, alarm, bowls, leftovers): (
        Res<crate::story::TownStory>,
        Res<crate::appliances::Alarm>,
        Query<(&crate::fishbowl::BowlFish, &Transform), Without<crate::visit::LotObject>>,
        Res<crate::meals::Leftovers>,
    ),
    (mut slot, maid, mail_due, upgraded, family, strokes, sculpted): (
        ResMut<SaveSlot>,
        Res<crate::services::MaidService>,
        Option<Res<crate::services::MailDue>>,
        Query<(&GameObject, &Transform, &crate::upgrades::Upgrades)>,
        Res<crate::family::Genealogy>,
        Res<crate::terrain_paint::Strokes>,
        Res<crate::terrain_paint::Sculpted>,
    ),
) {
    let new_file = save_as.read().count() > 0;
    let write = requests.read().count() > 0 || new_file;
    let snapshot = snapshots.read().count() > 0;
    if !write && !snapshot {
        return;
    }
    let Some(hh) = household else { return };
    let ids: HashMap<Entity, u64> = sims.iter().map(|q| (q.0, q.1.id)).collect();
    let mut saved = Vec::new();
    for (_, sim, tf, floor, motives, skills, moodlets, job, rels, member, selected, away, visiting, wishes, (aging, pregnancy, opps, out, ltw, author, recipes, chess, inventory, toddler, pension, journal)) in &sims {
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
            skills: skills.0.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            moodlets: moodlets.0.iter().filter(|m| m.until.is_finite()).map(|m| (m.kind.def().name.to_string(), m.until)).collect(),
            job: job.map(|j| SavedJob {
                track: j.career().name.into(),
                level: j.level,
                performance: j.performance,
                branch: j.path().branch.into(),
                tone: (j.tone != crate::careers::WorkTone::Normal).then(|| j.tone.name().into()),
            }),
            relationships: rels
                .0
                .iter()
                .filter_map(|(e, r)| {
                    Some(SavedRel { with: *ids.get(e)?, friendship: r.friendship, romance: r.romance, status: status_name(r.status).into(), kissed: r.kissed, last: r.last })
                })
                .collect(),
            lifetime_happiness: wishes.map_or(0, |w| w.points),
            outfit: saved_outfit(&sim.outfit),
            rewards: wishes.map(|w| w.rewards.clone()).unwrap_or_default(),
            aging: aging.map(|a| (a.days, a.elder_span)),
            pregnancy: pregnancy.map(|p| (p.since, p.other_parent.and_then(|o| ids.get(&o).copied()), p.stage)),
            shape: Some((sim.weight, sim.fitness)),
            opportunities: opps.map(|o| o.active.iter().filter_map(|a| Some((guid(a.index)?, a.deadline))).collect()).unwrap_or_default(),
            opportunities_done: opps.map(|o| o.done.clone()).unwrap_or_default(),
            lifetime_wish: ltw.map(|l| SavedLifetimeWish { check: l.def().check.into(), fulfilled: l.fulfilled, careers: l.careers.clone(), raised: l.raised, rich_spouse: l.rich_spouse }),
            author: author.cloned(),
            recipes: recipes.map(|r| r.0.clone()).unwrap_or_default(),
            chess: chess.copied(),
            inventory: inventory.cloned().unwrap_or_default(),
            toddler: toddler.copied(),
            eyes: Some(rgb(sim.eyes)),
            voice: Some(sim.voice),
            pension: pension.copied(),
            favorites: Some(sim.favorites.clone()),
            journal: journal.cloned().unwrap_or_default(),
            designs: sim.outfit.designs.clone(),
            styles: sim.outfit.styles.clone(),
            face: sim.face.clone(),
        });
    }
    let game = SaveGame {
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
        upgrades: crate::upgrades::saved(&upgraded),
        family: family.saved(),
        terrain: strokes.saved(),
        heights: sculpted.saved(),
        collection: collection.clone(),
        graves: graves
            .iter()
            .map(|(g, tf)| SavedGrave { position: tf.translation.to_array(), rotation: tf.rotation.to_array(), cause: g.cause.clone(), sim: saved_look(&g.sim) })
            .collect(),
        leftovers: leftovers.0.clone(),
        dormant: dormant.kept(&sims.iter().filter(|q| q.9).map(|q| q.1.id).collect::<Vec<_>>()),
        homeless: false,
        deck: true,
    };
    if snapshot {
        commands.insert_resource(Snapshot(game.clone()));
    }
    if !write {
        return;
    }
    let dir = saves_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = slot.0.clone().filter(|_| !new_file).unwrap_or_else(|| free_save_path(&dir, &game));
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
    if *done {
        return;
    }
    *done = true;
    commands.insert_resource(crate::home::MoveInRequest(p.0.lot_index));
    next.set(PlayMode::Live);
}

/// Once the household is in, restores everything the save remembers.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn apply_loaded_game(
    mut commands: Commands,
    pending: Option<Res<PendingLoad>>,
    mut clock: ResMut<GameClock>,
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
    if let Some(h) = household.as_mut() {
        h.funds = game.funds;
        h.last_bill_day = game.last_bill_day;
        h.bills = game.bills.clone();
        h.name = game.household.clone();
    }
    let by_id: HashMap<u64, Entity> = sims.iter().map(|q| (q.1.id, q.0)).collect();
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
                moodlets.0.push(crate::life::Moodlet { kind: k, value: k.def().value, until: *until });
            }
        }
        rels.0.clear();
        for r in &s.relationships {
            if let Some(&other) = by_id.get(&r.with) {
                let x = rels.entry(other);
                x.friendship = r.friendship;
                x.romance = r.romance;
                x.status = status_from(&r.status);
                x.kissed = r.kissed;
                x.last = r.last;
            }
        }
        let mut ec = commands.entity(e);
        if let Some((days, elder_span)) = s.aging {
            ec.insert(crate::aging::Aging { days, elder_span });
        }
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
            ec.insert(crate::opportunities::PendingOpportunities(s.opportunities.clone(), s.opportunities_done.clone()));
        }
        if let Some((since, other, stage)) = s.pregnancy {
            let other_parent = other.and_then(|o| by_id.get(&o).copied());
            ec.insert(crate::little::Pregnancy { since, other_parent, stage });
        }
        match &s.job {
            Some(j) => {
                if let Some(track) = crate::careers::careers().iter().position(|c| c.name == j.track) {
                    let mut job = crate::careers::Job::new(track);
                    job.branch = crate::careers::careers()[track].path_index(&j.branch).unwrap_or(0);
                    job.level = j.level.min(job.levels().len() - 1);
                    job.performance = j.performance;
                    job.tone = j.tone.as_deref().map(crate::careers::WorkTone::from_name).unwrap_or_default();
                    ec.insert(job);
                }
            }
            None => {
                ec.remove::<crate::careers::Job>();
            }
        }
        if s.member {
            ec.insert(HouseholdMember).remove::<(Visitor, OffLot)>();
            ec.insert(crate::wishes::Wishes::restored(
                s.lifetime_happiness,
                s.rewards.clone(),
                game.minutes,
            ));
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
            commands.entity(e).despawn();
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
    commands.insert_resource(crate::fishbowl::PendingBowls(game.fishbowls.clone()));
    commands.insert_resource(game.collection.clone());
    commands.insert_resource(crate::meals::Leftovers(game.leftovers.clone()));
    commands.insert_resource(crate::household::Dormant(game.dormant.clone()));
    // The household's dead, back in their graves.
    if let Some(entry) = data.0.catalog.iter().find(|c| c.instance_name == "UrnstoneHuman") {
        for g in &game.graves {
            let sim = SaveGame::sim(&g.sim);
            let label = format!("{}'s Tombstone", sim.full_name());
            if let Some(o) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, entry.objd, lift(Vec3::from(g.position)), Quat::from_array(g.rotation)) {
                commands.entity(o.entity).insert(crate::ghosts::Grave { sim, cause: g.cause.clone() }).queue_silenced(move |mut w: EntityWorldMut| {
                    if let Some(mut obj) = w.get_mut::<GameObject>() {
                        obj.name = label;
                    }
                });
            }
        }
    }
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
        assert_eq!(saved.outfit.len(), 25);
        let back = SaveGame::sim(&serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap());
        assert_eq!(back.outfit, sim.outfit);
        assert_eq!(back.outfit.clothes(crate::simbody::OutfitKind::Formal).full, Some((1, 0, 7)));
        // An old save's nine: the other outfits as their look gives them.
        let mut old = saved.clone();
        old.outfit.truncate(9);
        assert_eq!(SaveGame::sim(&old).outfit.other, [crate::sim::Clothes::default(); 4]);
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
