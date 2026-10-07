//! Gameplay data from the game's own tuning tables (`Game/Bin/Gameplay/GameplayData.package`,
//! XML exported from the designers' spreadsheets): the base game's moodlets (buffs), traits and
//! skills with their names and descriptions in English and the interface icons that go with
//! them. The icons (PNG, `0x2F7D0004`, instance = fnv64 of the lowercase name) are copied to
//! `icons.pack` as they are.

use std::collections::{BTreeSet, HashMap, HashSet};

use s3pkg::{Package, PackageSet};
use serde::{Deserialize, Serialize};

use crate::bake::BakeRoot;
use crate::pack::{PackReader, PackWriter, read_value, write_value};

pub const GAMEDATA_VERSION: u32 = 56;
/// Interface images.
pub const T_ICON: u32 = 0x2F7D0004;
const T_XML: u32 = 0x0333406C;
/// Textures (balloon icons are 64 px DDS pictures, instance = fnv64 of the lowercase name).
const T_DDS: u32 = 0x00B2D882;
/// Buy-mode catalogue thumbnails, 128 px PNG (`Thumbnails/AllThumbnails.package`): instance =
/// the object's OBJD instance, group = its colour variant.
const T_THUMB_LARGE: u32 = 0x0580A2B6;

/// Create-a-Sim part thumbnails, 128 px PNG (`Thumbnails/CasThumbnails.package`): instance =
/// the CASP instance, group = its colour preset.
const T_CAS_THUMB: u32 = 0x626F60CD;

/// The icon name an object's catalogue thumbnail is stored under.
pub fn thumb_name(objd_instance: u64) -> String {
    format!("thumb_{objd_instance:016x}")
}

/// The icon name a CAS part's thumbnail is stored under.
pub fn cas_thumb_name(casp_instance: u64) -> String {
    format!("casthumb_{casp_instance:016x}")
}
const T_NMAP: u32 = 0x0166038C;

/// A moodlet: the game's buff table row.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BuffInfo {
    pub hex: String,
    pub name: String,
    pub desc: String,
    pub icon: String,
    /// The mood it affects ("Happy", "Uncomfortable"…) and by how much.
    pub axis: String,
    pub value: i32,
    /// Minutes until it wears off (0 = while its cause lasts).
    pub timeout: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TraitInfo {
    pub hex: String,
    pub name: String,
    pub desc: String,
    /// Large icon (Create-a-Sim) and the small one used in menus.
    pub icon: String,
    pub icon_small: String,
    pub category: String,
    /// A lifetime reward's cost in lifetime happiness (`RewardMotive`, `RewardShop`... traits).
    pub points: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkillInfo {
    pub hex: String,
    pub name: String,
    pub desc: String,
    /// Small icon, the wish icon and the picture of the skill's object.
    pub icon: String,
    pub wish_icon: String,
    pub object_icon: String,
    pub max_level: u32,
}

/// One level of a career (a row of the career's own table).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CareerLevelInfo {
    pub level: u32,
    /// "Base" until the career branches, then the branch's name.
    pub branch: String,
    pub title: String,
    pub title_female: String,
    /// Simoleons per hour.
    pub hourly: f32,
    pub start: f32,
    pub hours: f32,
    /// Workdays: bit 0 = Monday.
    pub days: u8,
    /// Skills that count towards performance at this level.
    pub skills: Vec<String>,
    /// The uniform (an outfit's name, or empty for everyday clothes) for men, women, elderly
    /// men and elderly women.
    pub outfits: [String; 4],
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CareerInfo {
    pub hex: String,
    pub name: String,
    pub desc: String,
    pub icon: String,
    pub part_time: bool,
    pub levels: Vec<CareerLevelInfo>,
}

/// An opportunity done at a rabbit hole (the game's most common kind): go to a venue of a type
/// during its hours and spend some time there, for a reward.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OpportunityInfo {
    pub guid: String,
    pub name: String,
    pub desc: String,
    pub completion: String,
    pub failure: String,
    pub icon: String,
    /// The rabbit hole type (`CityHall`, `Restaurant`, `ScienceLab`...).
    pub rabbit_hole: String,
    /// What the Sim does there, and for how long (minutes), between these hours.
    pub interaction: String,
    pub minutes: f32,
    pub open: f32,
    pub close: f32,
    /// Days to do it in (0: no limit).
    pub days: f32,
    /// A career (its key, like `Business`) or skill (`Logic`, min..max level) it's offered for.
    pub career: String,
    pub skill: String,
    pub skill_min: u32,
    pub skill_max: u32,
    /// Rewards: simoleons, job performance, a raise (percent), skill (percent of a level).
    pub money: i64,
    pub performance: f32,
    pub raise: f32,
    pub skill_reward: f32,
    pub repeat: bool,
}

/// A garden plant from the game's `plants` table, with what it bears (`ingredients`).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PlantInfo {
    pub name: String,
    /// Common, Uncommon or Rare.
    pub rarity: String,
    /// The plant object (`GardenPlantBush`, `PlantLargeTree`, `PlantSmallVine`) and how high its
    /// produce grows (Low, Medium, High: the harvesting animation).
    pub model: String,
    pub height: String,
    /// What it bears and what each is worth.
    pub produce: String,
    pub price: i64,
    pub harvest_min: u32,
    pub harvest_max: u32,
    /// Harvests in its life.
    pub lifetime: u32,
    /// Water lost per hour (percent), and how weedy it gets.
    pub water_decay: f32,
    pub weeds: f32,
    /// Gardening skill points for planting and harvesting.
    pub skill_plant: f32,
    pub skill_harvest: f32,
    /// The produce's model (in `produce.pack`; its group is the geometry state's hash).
    pub produce_model: Option<crate::types::Key>,
}

/// A fence from the build catalogue, with its pieces' models (in `fences.pack`).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FenceStyle {
    /// The CFEN.
    pub key: crate::types::Key,
    pub name: String,
    pub price: i32,
    pub straight: Option<crate::types::Key>,
    pub diagonal: Option<crate::types::Key>,
    pub post: Option<crate::types::Key>,
}

/// A wallpaper or floor covering from the build catalogue.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PatternInfo {
    pub name: String,
    pub price: i32,
    pub floor: bool,
    /// Its rendered texture in the texture store.
    pub texture: crate::types::Key,
}

/// One choice in a balloon list: an icon, or another list to draw from (`refkey`), or one of the
/// game's special pickers (an icon name like "GetSpeechBalloonImageForChat").
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BalloonEntry {
    pub icon: String,
    pub refkey: String,
    /// 0 neutral, 1 like (a smile badge), 2 dislike (crossed out).
    pub axis: u8,
    pub weight: f32,
}

/// The game's balloon table (`Balloons`): what Sims think and say, keyed by need, moodlet or
/// trait (idle thoughts), social interaction, conversation topic, and random sets.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BalloonTable {
    pub idle: HashMap<String, Vec<BalloonEntry>>,
    pub social: HashMap<String, Vec<BalloonEntry>>,
    pub topic: HashMap<String, Vec<BalloonEntry>>,
    pub random: HashMap<String, Vec<BalloonEntry>>,
}

impl BalloonTable {
    /// A list by key from any of the tables (the order references are looked up in).
    pub fn list(&self, key: &str) -> Option<&Vec<BalloonEntry>> {
        self.idle.get(key).or_else(|| self.topic.get(key)).or_else(|| self.random.get(key)).or_else(|| self.social.get(key))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GameDataBaked {
    pub version: u32,
    pub buffs: Vec<BuffInfo>,
    pub traits: Vec<TraitInfo>,
    pub skills: Vec<SkillInfo>,
    pub careers: Vec<CareerInfo>,
    pub patterns: Vec<PatternInfo>,
    pub balloons: BalloonTable,
    pub opportunities: Vec<OpportunityInfo>,
    pub plants: Vec<PlantInfo>,
    pub roofs: Vec<RoofPattern>,
    pub collectibles: Vec<CollectibleInfo>,
    pub spawners: Vec<SpawnerInfo>,
    /// The build catalogue's fences.
    pub fences: Vec<FenceStyle>,
    /// Each object's effect slots (model space): where showers spray, fountains gush and
    /// fires burn.
    pub fx_slots: Vec<(crate::types::Key, Vec<[f32; 3]>)>,
    pub lifetime_wishes: Vec<LifetimeWishInfo>,
    /// The Writing skill's tuning (`kLengthRomanceMin`, `kRoyaltyRomanceMax`, `kRateBasePPM`,
    /// `kQualityLevel5ChanceHit`...).
    pub writing: HashMap<String, f32>,
    /// Titles for the books Sims write, by genre (`Fiction`, `SciFi`, `Romance`...).
    pub book_titles: Vec<(String, Vec<String>)>,
    pub recipes: Vec<RecipeInfo>,
}

/// Meal times a recipe is cooked for (`RecipeInfo::meals`).
pub const MEAL_BREAKFAST: u8 = 1;
pub const MEAL_BRUNCH: u8 = 2;
pub const MEAL_LUNCH: u8 = 4;
pub const MEAL_DINNER: u8 = 8;
pub const MEAL_DESSERT: u8 = 16;

/// A recipe: a base game row of `RecipeMasterList` that's cooked as a meal.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RecipeInfo {
    /// `Spaghetti`, `GoopyCarbonara`...
    pub key: String,
    pub name: String,
    /// The Cooking level it takes.
    pub level: u8,
    /// Known once a Sim reaches its level (the rest are learned from recipe books).
    pub auto: bool,
    /// The meal times it's for (`MEAL_*` bits).
    pub meals: u8,
    pub vegetarian: bool,
    /// What it costs at a restaurant.
    pub cost: i32,
    pub ingredients: Vec<String>,
    /// The serving dish full and emptied, and a plateful and the empty plate: models in
    /// `food.pack` (each key's group is its geometry state's hash).
    pub group: Option<crate::types::Key>,
    pub group_empty: Option<crate::types::Key>,
    /// What its recipe book costs at the bookstore (0: there's none).
    pub book_price: i32,
    pub single: Option<crate::types::Key>,
    pub single_empty: Option<crate::types::Key>,
}

/// A lifetime wish: one of the base game's "Lifetime Dreams" (`DreamsAndPromisesNodes`), with
/// its fulfillment score from `DreamNodeInstanceDefaults`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LifetimeWishInfo {
    pub id: u32,
    /// The game's check for it, e.g. `NSimoleonsInCashMajorDreamCheckFunction` (empty for a
    /// few that are fulfilled by an event instead).
    pub check: String,
    /// The number it asks for (simoleons, friends, career level), when it takes one.
    pub number: f32,
    pub icon: String,
    /// Fulfillment score: a tenth of the lifetime happiness it's worth.
    pub score: u32,
}

/// What kind of find a collectible is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CollectKind {
    Gem,
    Metal,
    SpaceRock,
    Butterfly,
    Beetle,
    Fish,
}

/// Something to find about town: a gem, metal, space rock, insect or fish (base game rows of the
/// `RockGemMetal`, `Insects` and `Fishing` tables).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CollectibleInfo {
    /// The tables' key ("Ruby", "ButterflyMoth", "Minnow").
    pub key: String,
    pub name: String,
    pub kind: CollectKind,
    pub min_price: i32,
    pub max_price: i32,
    pub rarity: String,
    /// The skill level needed to find it (fishing for fish).
    pub level: u8,
    /// The catalogue object showing it (by instance name): "Gem", "Metal", "MeteorMedium",
    /// "fishMinnow"...
    pub model: String,
}

/// What a spawner (placed on lots by the world builders) turns up, by its script class.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SpawnerInfo {
    /// The spawner's class ("RockGemMetalSpawner11", "InsectSpawner3", "FishingSpawner6").
    pub class: String,
    /// Collectible keys with relative weights ("None" = nothing).
    pub items: Vec<(String, f32)>,
    /// How many can be lying about at once.
    pub capacity: u8,
    /// Hours between finds.
    pub hours: (f32, f32),
}

/// A roof pattern from the build catalogue.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoofPattern {
    pub name: String,
    /// The roof material's diffuse map: an atlas of the field of tiles with the ridge and hip
    /// pieces stamped over it.
    pub texture: crate::types::Key,
    /// Just the field of tiles (covering [`ROOF_ATLAS_METRES`] square), for repeating over a
    /// roof (see [`roof_tile`]).
    pub tile: crate::types::Key,
}

/// How much roof (metres) a roof atlas covers.
pub const ROOF_ATLAS_METRES: f32 = 3.0;

/// The plain field of tiles from a roof atlas (covering as much roof as the atlas). The field
/// repeats along one axis or both, and the ridge and hip pieces sit over parts of it, so the
/// median of every repeat of each pixel leaves just the tiles.
pub fn roof_tile(atlas: &s3formats::dds::Rgba) -> Option<s3formats::dds::Rgba> {
    let (w, h) = (atlas.width, atlas.height);
    if w < 64 || h < 64 {
        return None;
    }
    let lum: Vec<f32> = atlas.data.chunks_exact(4).map(|p| p[0] as f32 * 0.3 + p[1] as f32 * 0.59 + p[2] as f32 * 0.11).collect();
    // The repeat along an axis: the shift (16 px or more) where the atlas best matches itself,
    // if that's clearly better than shifts in general; with how much better (lower is clearer).
    let period = |horizontal: bool| -> Option<(usize, f32)> {
        let n = if horizontal { w } else { h };
        let diff = |p: usize| {
            let (mut sum, mut cnt) = (0.0f32, 0usize);
            let (xs, ys) = if horizontal { (w - p, h) } else { (w, h - p) };
            for y in (0..ys).step_by(2) {
                for x in (0..xs).step_by(2) {
                    let (x2, y2) = if horizontal { (x + p, y) } else { (x, y + p) };
                    sum += (lum[y * w + x] - lum[y2 * w + x2]).abs();
                    cnt += 1;
                }
            }
            sum / cnt.max(1) as f32
        };
        let diffs: Vec<f32> = (0..=n / 2).map(|p| if p >= 15 { diff(p) } else { 0.0 }).collect();
        let mut sorted: Vec<f32> = diffs[16..].to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let med = sorted[sorted.len() / 2].max(1e-3);
        (16..n / 2)
            .filter(|&p| diffs[p] <= diffs[p - 1] && diffs[p] <= diffs[p + 1] && diffs[p] < 0.85 * med)
            .min_by(|&a, &b| diffs[a].total_cmp(&diffs[b]))
            .map(|p| (p, diffs[p] / med))
    };
    let (mut px, mut py) = (period(true), period(false));
    // The clearer repeat, and the other only when it's clear too.
    match (px, py) {
        (Some(x), Some(y)) if x.1 <= y.1 && y.1 >= 0.8 => py = None,
        (Some(x), Some(y)) if y.1 < x.1 && x.1 >= 0.8 => px = None,
        _ => {}
    }
    let tx = px.map_or(w, |p| p.0);
    let ty = py.map_or(h, |p| p.0);
    let (kx, ky) = (w / tx, h / ty);
    let mut tile = vec![0u8; tx * ty * 4];
    let mut samples: Vec<u8> = Vec::with_capacity(kx * ky);
    for y in 0..ty {
        for x in 0..tx {
            for c in 0..4 {
                samples.clear();
                for j in 0..ky {
                    for i in 0..kx {
                        samples.push(atlas.data[((y + j * ty) * w + x + i * tx) * 4 + c]);
                    }
                }
                samples.sort_unstable();
                tile[(y * tx + x) * 4 + c] = samples[samples.len() / 2];
            }
        }
    }
    // Repeated over the atlas's area again, at 256 x 256.
    let (rx, ry) = (((w as f32 / tx as f32).round() as usize).max(1), ((h as f32 / ty as f32).round() as usize).max(1));
    let size = 256usize;
    let mut data = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let sx = (x * tx * rx / size) % tx;
            let sy = (y * ty * ry / size) % ty;
            let o = (y * size + x) * 4;
            data[o..o + 4].copy_from_slice(&tile[(sy * tx + sx) * 4..(sy * tx + sx) * 4 + 4]);
        }
    }
    Some(s3formats::dds::Rgba { width: size, height: size, data })
}


impl GameDataBaked {
    pub fn buff(&self, hex: &str) -> Option<&BuffInfo> {
        self.buffs.iter().find(|b| b.hex.eq_ignore_ascii_case(hex))
    }
    pub fn trait_info(&self, hex: &str) -> Option<&TraitInfo> {
        self.traits.iter().find(|t| t.hex.eq_ignore_ascii_case(hex))
    }
    pub fn skill(&self, hex: &str) -> Option<&SkillInfo> {
        self.skills.iter().find(|s| s.hex.eq_ignore_ascii_case(hex))
    }
}

pub fn gamedata_ready(root: &BakeRoot) -> bool {
    let g = root.global_dir();
    g.join("icons.pack").exists() && read_value::<GameDataBaked>(&g.join("gamedata.bin")).is_ok_and(|d| d.version == GAMEDATA_VERSION)
}

pub fn load_gamedata(root: &BakeRoot) -> Option<GameDataBaked> {
    read_value(&root.global_dir().join("gamedata.bin")).ok().filter(|d: &GameDataBaked| d.version == GAMEDATA_VERSION)
}

/// The baked interface icons (PNG bytes by name).
pub struct Icons(PackReader);

impl Icons {
    pub fn open(root: &BakeRoot) -> Option<Self> {
        PackReader::open(&root.global_dir().join("icons.pack")).ok().map(Self)
    }
    /// The PNG of an icon, by name.
    pub fn png(&self, name: &str) -> Option<Vec<u8>> {
        self.0.get(&icon_key(name))
    }
}

pub fn icon_key(name: &str) -> crate::types::Key {
    (T_ICON, 0, s3pkg::fnv64(&name.to_ascii_lowercase()))
}

/// Decodes an icon's PNG into RGBA8 (width, height, pixels).
pub fn decode_icon(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = crate::bake::decode_png(png)?;
    Some((img.width as u32, img.height as u32, img.data))
}

/// The records of a tuning table: each `<row>` element's child tags and their text.
fn records(xml: &str, row: &str) -> Vec<HashMap<String, String>> {
    let (open, close) = (format!("<{row}>"), format!("</{row}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(a) = rest.find(&open) {
        let body_start = a + open.len();
        let Some(b) = rest[body_start..].find(&close) else { break };
        let body = &rest[body_start..body_start + b];
        let mut f = HashMap::new();
        let mut s = body;
        while let Some(lt) = s.find('<') {
            let Some(gt) = s[lt..].find('>') else { break };
            let tag = &s[lt + 1..lt + gt];
            if tag.starts_with('/') || tag.ends_with('/') || tag.starts_with('!') {
                s = &s[lt + gt + 1..];
                continue;
            }
            let after = &s[lt + gt + 1..];
            let end_tag = format!("</{tag}>");
            match after.find(&end_tag) {
                Some(e) => {
                    let v = after[..e].trim();
                    f.insert(tag.to_string(), unescape(v));
                    s = &after[e + end_tag.len()..];
                }
                None => s = after,
            }
        }
        out.push(f);
        rest = &rest[body_start + b + close.len()..];
    }
    out
}

/// A tuning file's current values: `<kName value="1.5">` → ("kName", 1.5).
fn tuning_values(xml: &str) -> HashMap<String, f32> {
    let cur = xml.split("<Current_Tuning>").nth(1).unwrap_or(xml);
    let cur = cur.split("</Current_Tuning>").next().unwrap_or(cur);
    let mut out = HashMap::new();
    let mut rest = cur;
    while let Some(i) = rest.find("<k") {
        rest = &rest[i + 1..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        let name = tag.split_whitespace().next().unwrap_or("");
        if let Some(v) = tag.split("value=\"").nth(1).and_then(|v| v.split('"').next()).and_then(|v| v.trim().parse::<f32>().ok()) {
            out.insert(name.to_string(), v);
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Converts the base game's buffs, traits and skills and their icons.
/// The careers' uniforms (`outfits.bin`): each outfit's parts with their designs rendered from
/// the outfit's presets into the texture store, and the meshes of parts the everyday wardrobe
/// doesn't have (`outfits.pack`). Formal wear that isn't everyday wear too (suits, tuxedos,
/// cocktail dresses) joins the wardrobe the same way (`wardrobe.bin`).
/// The face sliders every Sim's face is shaped with, in opposing pairs (a Sim leans one way or
/// the other on each, by their look). The game shapes faces by moving the face's bones: a
/// slider is one bone adjustment (BOND) per age and sex, `<age><sex>FaceDeformations<Slider>`.
pub const FACE_SLIDERS: [(&str, &str); 17] = [
    ("JawWide", "JawThin"),
    ("JawChinScaleUp", "JawChinScaleDown"),
    ("JawChinUp", "JawChinDown"),
    ("MouthWide", "MouthThin"),
    ("MouthOut", "MouthIn"),
    ("TranslateMouthUp", "TranslateMouthDown"),
    ("EyesScaleUp", "EyesScaleDown"),
    ("EyesApart", "EyesIn"),
    ("TranslateEyesUp", "TranslateEyesDown"),
    ("EyesBrowsUp", "EyesBrowsDown"),
    ("NoseScaleUp", "NoseScaleDown"),
    ("NoseWide", "NoseThin"),
    ("NoseUp", "NoseDown"),
    ("NoseTipScaleUp", "NoseTipScaleDown"),
    ("JawCheeksBoneUp", "JawCheeksBoneDown"),
    ("JawCheeksOut", "JawCheeksIn"),
    ("HeadWide", "HeadThin"),
];

/// The age-and-sex prefixes of the face sliders' bone adjustments.
pub const FACE_PREFIXES: [&str; 10] = ["am", "af", "ym", "yf", "em", "ef", "tm", "tf", "cu", "pu"];

/// One bone's adjustment by a face slider (at full strength).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct FaceBone {
    /// FNV-32 of the bone's name (lowercase).
    pub bone: u32,
    pub offset: [f32; 3],
    pub scale: [f32; 3],
    pub rotation: [f32; 4],
}

/// The face sliders' bone adjustments by age-and-sex prefix: per slider (index into the
/// flattened `FACE_SLIDERS`), its bones.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FaceBones {
    pub prefix: String,
    pub sliders: Vec<(u8, Vec<FaceBone>)>,
}

/// Eye colours, as the game draws them: the iris (a greyscale image, its alpha the iris's
/// shape) in the eye colour, at twice the image's shade times the colour, over the eyes' own
/// texture (every face's eyes have their iris in the same place); and Create a Sim's presets.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EyeColors {
    /// The iris (in the texture store).
    pub overlay: Option<crate::types::Key>,
    pub presets: Vec<[f32; 3]>,
}

/// Texture-store type of a catalogue object's designs: `(T_DESIGN, design, OBJD instance)`.
pub const T_DESIGN: u32 = 0x0DE5_1600;
/// How big designs are drawn (the longer side).
const DESIGN_SIZE: usize = 256;

/// A catalogue object's designs (the game's colour and pattern presets for it): how many drawn
/// (none when it comes in just the one), and the texture of its meshes designs stand in for
/// (the object's composited texture; the lots' furniture is in designs of its own too).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ObjectDesigns {
    pub objd: crate::types::Key,
    pub count: u8,
    pub texture: crate::types::Key,
}

/// Every buyable object's designs, each drawn from its complate into the texture store (objects
/// whose meshes share one composited texture: the designs are recipes for it).
fn bake_object_designs(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    let g = root.global_dir();
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog.bin: {e}"))?;
    let models = PackReader::open(&g.join("models.pack")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.textures_dir()).ok();
    let buyable: Vec<&crate::types::CatalogEntry> = catalog.iter().filter(|c| c.price >= 0).collect();
    let out: Vec<ObjectDesigns> = crate::bake::par_map(&buyable, |c| {
        let o = s3formats::object::parse_objd(&pkgs.read(&crate::types::rkey(c.objd))?).ok()?;
        if o.presets.is_empty() {
            return None;
        }
        // The one composited texture the designs are recipes for.
        let mut txtcs: Vec<crate::types::Key> = c
            .models
            .iter()
            .filter_map(|m| models.get::<crate::types::BakedModel>(m))
            .flat_map(|m| m.parts.into_iter().filter_map(|p| p.texture))
            .filter(|t| t.0 == s3pkg::types::TXTC)
            .collect();
        txtcs.sort();
        txtcs.dedup();
        let [texture] = txtcs[..] else { return None };
        // (Drawn in the shape of that texture.)
        let (w, h) = pkgs
            .read(&crate::types::rkey(texture))
            .and_then(|d| s3formats::txtc::Txtc::parse(&d).ok())
            .map(|t| s3formats::compositor::Compositor::new(pkgs).output_size(&t))
            .filter(|(w, h)| *w > 0 && *h > 0)
            .unwrap_or((DESIGN_SIZE, DESIGN_SIZE));
        let scale = DESIGN_SIZE as f32 / w.max(h) as f32;
        let (w, h) = (((w as f32 * scale) as usize).max(16), ((h as f32 * scale) as usize).max(16));
        let mut count = 0u8;
        for (i, p) in o.presets.iter().enumerate().take(16).filter(|_| o.presets.len() > 1) {
            let key = (T_DESIGN, i as u32, c.objd.2);
            if !root.tex_path(key).exists() {
                let Some(img) = s3formats::complate::render(pkgs, &p.complate, &p.keys, w, h) else { break };
                let _ = std::fs::write(root.tex_path(key), crate::ddsw::encode_dds(&img));
            }
            count = i as u8 + 1;
        }
        Some(ObjectDesigns { objd: c.objd, count, texture })
    })
    .into_iter()
    .flatten()
    .collect();
    let n = out.len();
    write_value(&g.join("object_designs.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// One of the game's painting pictures (a row of the Painting skill's picture table, base game).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PaintingInfo {
    /// The canvas: 0 small, 1 medium, 2 large.
    pub size: u8,
    /// The picture's name (its texture is the DDS of instance fnv64 of it).
    pub name: String,
    /// The Painting skill levels it's painted at.
    pub min: u8,
    pub max: u8,
    /// 0 an ordinary painting, 1 a dabble, 2 a brilliant painting, 3 a masterpiece.
    pub kind: u8,
    /// The versions painted by Sims of a trait: (the table's trait column, picture).
    pub traits: Vec<(String, String)>,
}

/// A canvas size: the finished painting's catalogue object, its model (drawn at its size) and
/// where on a picture's texture the painted face shows it (u0, v0, u1, v1).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CanvasInfo {
    pub objd: crate::types::Key,
    pub model: crate::types::Key,
    pub uv: [f32; 4],
}

/// The paintings Sims paint, and the canvases they're painted on (small, medium, large).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PaintingsBaked {
    pub paintings: Vec<PaintingInfo>,
    pub canvases: Vec<CanvasInfo>,
    /// The texture of the canvas's painted face (the picture stands in for it, as a design).
    pub face: crate::types::Key,
    /// Where each easel holds the canvas being painted.
    pub easels: Vec<EaselSlots>,
}

/// Where an easel holds a canvas (its rig's canvas slots, in its model space): the small one,
/// and the medium and large ones. (Translation, and rotation as x, y, z, w.)
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EaselSlots {
    pub objd: crate::types::Key,
    pub small: ([f32; 3], [f32; 4]),
    pub large: ([f32; 3], [f32; 4]),
}

/// A rig bone's place in the rig's (the object's model) space: its parents' transforms
/// composed with its own.
fn bone_transform(rig: &s3formats::sim::Rig, i: usize) -> ([f32; 3], [f32; 4]) {
    let qmul = |a: [f32; 4], b: [f32; 4]| {
        [
            a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
            a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
            a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
            a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
        ]
    };
    let rotate = |q: [f32; 4], v: [f32; 3]| {
        let p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]]);
        [p[0], p[1], p[2]]
    };
    let b = &rig.bones[i];
    match usize::try_from(b.parent).ok().filter(|&p| p < rig.bones.len() && p != i) {
        Some(p) => {
            let (pp, pr) = bone_transform(rig, p);
            let d = rotate(pr, b.position);
            ([pp[0] + d[0], pp[1] + d[1], pp[2] + d[2]], qmul(pr, b.rotation))
        }
        None => (b.position, b.rotation),
    }
}

/// The texture of a painting picture.
pub fn painting_texture(name: &str) -> crate::types::Key {
    (T_DDS, 0, s3pkg::fnv64(name))
}

/// Model-store group of the canvases drawn at each size (over the canvas model's instance).
pub const CANVAS_GROUP: u32 = 0x0CA4_7A50;

/// The Painting skill's picture table and the pictures, and the canvas model at each size
/// (one model for all three, its geometry states the sizes).
/// A catalogue model with parts that move (a fridge's door, a swing's seat): its rig, and how
/// its meshes are skinned to it (in `skins.pack`, by MODL key).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ObjectSkin {
    /// The rig's bones: FNV-32 of the name (as the clips' tracks name them), parent (-1 for
    /// none) and bind pose relative to it (translation; rotation x, y, z, w).
    pub bones: Vec<(u32, i16, [f32; 3], [f32; 4])>,
    /// Each skinned mesh of the model, in model order.
    pub meshes: Vec<SkinMesh>,
}

/// A mesh's skinning: its vertex count and first vertex (to find its baked part), and each
/// vertex's four rig bones (indices into `ObjectSkin::bones`) and weights (in 255ths).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkinMesh {
    pub verts: u32,
    pub first: [f32; 3],
    pub bones: Vec<[u16; 4]>,
    pub weights: Vec<[u8; 4]>,
}

/// An object's rig (OBJD → OBJK → its VPXY, which names the rig).
fn object_rig(pkgs: &PackageSet, objd: crate::types::Key) -> Option<s3formats::sim::Rig> {
    pkgs.read(&crate::types::rkey(objd))
        .and_then(|d| s3formats::object::objd_objk(pkgs, &d))
        .and_then(|o| o.model_key)
        .and_then(|k| pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)))
        .and_then(|v| s3formats::model::vpxy_keys(&v).into_iter().find(|k| k.t == 0x8EAF13DE))
        .and_then(|k| pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)))
        .and_then(|d| s3formats::sim::Rig::parse(&d).ok())
}

/// The catalogue's models with moving parts (meshes skinned to more than the object's root
/// bones), with their rigs, for the object animations the clips play: `skins.pack`.
fn bake_object_skins(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    let g = root.global_dir();
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog.bin: {e}"))?;
    let fixed = ["transformBone", "b__ROOT__", "offsetBone"].map(s3pkg::fnv32);
    let mut done: HashSet<crate::types::Key> = HashSet::new();
    let mut n = 0;
    let mut pack = PackWriter::create(&g.join("skins.pack")).map_err(|e| e.to_string())?;
    for c in &catalog {
        if c.models.iter().all(|m| done.contains(m)) {
            continue;
        }
        let Some(rig) = object_rig(pkgs, c.objd) else { continue };
        if rig.bones.len() < 2 {
            continue;
        }
        let hashes: Vec<u32> = rig.bones.iter().map(|b| s3pkg::fnv32(&b.name)).collect();
        for &modl in &c.models {
            if !done.insert(modl) {
                continue;
            }
            let meshes = s3formats::model::load_model(pkgs, &crate::types::rkey(modl)).unwrap_or_default();
            let mut moves = false;
            let mut out = ObjectSkin {
                bones: rig.bones.iter().zip(&hashes).map(|(b, h)| (*h, b.parent.clamp(-1, i16::MAX as i32) as i16, b.position, b.rotation)).collect(),
                meshes: Vec::new(),
            };
            for m in meshes.iter().filter(|m| !m.blend_indices.is_empty() && !m.positions.is_empty()) {
                // (Each of the mesh's joints as a rig bone: the root when the rig hasn't it.)
                let joints: Vec<u16> = m.joints.iter().map(|j| hashes.iter().position(|h| h == j).unwrap_or(0) as u16).collect();
                let mut bones = Vec::with_capacity(m.blend_indices.len());
                let mut weights = Vec::with_capacity(m.blend_indices.len());
                for (ix, w) in m.blend_indices.iter().zip(&m.blend_weights) {
                    let b = ix.map(|i| joints.get(i as usize).copied().unwrap_or(0));
                    let w = w.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8);
                    if b.iter().zip(&w).any(|(b, w)| *w > 0 && !fixed.contains(&hashes[*b as usize])) {
                        moves = true;
                    }
                    bones.push(b);
                    weights.push(w);
                }
                out.meshes.push(SkinMesh { verts: m.positions.len() as u32, first: m.positions[0], bones, weights });
            }
            if moves {
                pack.add(modl, &out).map_err(|e| e.to_string())?;
                n += 1;
            }
        }
    }
    pack.finish().map_err(|e| e.to_string())?;
    Ok(n)
}

fn bake_paintings(root: &BakeRoot, pkgs: &PackageSet, table: Option<String>) -> Result<usize, String> {
    let g = root.global_dir();
    let mut out = PaintingsBaked::default();
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog.bin: {e}"))?;
    let has_tex = |name: &str| pkgs.read_ti(T_DDS, s3pkg::fnv64(name)).is_some();
    let traits = ["Artistic", "CantStandArt", "ComputerWhiz", "Evil", "Genius", "Grumpy", "Insane", "Neurotic", "Virtuoso"];
    for (size, tag) in ["Small", "Medium", "Large"].into_iter().enumerate() {
        let rows = table.as_deref().map(|x| records(x, tag)).unwrap_or_default();
        let Some((defaults, rows)) = rows.split_first() else { continue };
        let field = |f: &HashMap<String, String>, k: &str| f.get(k).or_else(|| defaults.get(k)).cloned().unwrap_or_default();
        for f in rows {
            // (The base game's: the expansions' rows name theirs.)
            if field(f, "SKU") != "BaseGame" || field(f, "Skill") != "Painting" || field(f, "Occupation") != "Undefined" {
                continue;
            }
            let name = field(f, "PaintingNames");
            if name.is_empty() || !has_tex(&name) {
                continue;
            }
            let flag = |k: &str| field(f, k) == "True";
            let kind = if flag("Masterpiece") {
                3
            } else if flag("Brilliant") {
                2
            } else if flag("Dabble") {
                1
            } else {
                0
            };
            out.paintings.push(PaintingInfo {
                size: size as u8,
                min: field(f, "MinLevelToPaint").parse().unwrap_or(0),
                max: field(f, "MaxLevelToPaint").parse().unwrap_or(10),
                kind,
                traits: traits
                    .iter()
                    .filter_map(|t| {
                        let v = f.get(*t)?;
                        (!v.is_empty() && has_tex(v)).then(|| (t.to_string(), v.clone()))
                    })
                    .collect(),
                name,
            });
        }
    }
    std::fs::create_dir_all(root.textures_dir()).ok();
    let names: Vec<String> = out.paintings.iter().flat_map(|p| std::iter::once(p.name.clone()).chain(p.traits.iter().map(|(_, n)| n.clone()))).collect();
    crate::bake::par_map(&names, |n| {
        let k = painting_texture(n);
        if !root.tex_path(k).exists()
            && let Some(dds) = crate::bake::bake_texture(pkgs, k, 256, false)
        {
            let _ = std::fs::write(root.tex_path(k), dds);
        }
    });
    // The canvas: its painted face's three states, smallest to largest, are the sizes.
    let objd = |n: &str| catalog.iter().find(|c| c.instance_name == n).map(|c| (c.objd, c.models.first().copied()));
    if let (Some((small, Some(modl))), Some((medium, _)), Some((large, _))) =
        (objd("EaselCanvasPaintingSmall"), objd("EaselCanvasPaintingMedium"), objd("EaselCanvasPaintingLarge"))
    {
        let meshes = s3formats::model::load_model(pkgs, &crate::types::rkey(modl)).unwrap_or_default();
        if let Some(face) = meshes.iter().filter(|m| m.states.len() == 3).min_by_key(|m| m.indices.len()) {
            out.face = face.material.texture(s3formats::model::P_DIFFUSE_MAP).map(|k| crate::types::key_of(&k)).unwrap_or_default();
            let mut sizes: Vec<(f32, u32, [f32; 4])> = face
                .states
                .iter()
                .map(|(h, ix)| {
                    let (mut lo, mut hi, mut uv) = ([f32::MAX; 2], [f32::MIN; 2], [f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
                    for &i in ix {
                        let (p, t) = (face.positions[i as usize], face.uvs[i as usize]);
                        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
                        uv = [uv[0].min(t[0]), uv[1].min(t[1]), uv[2].max(t[0]), uv[3].max(t[1])];
                    }
                    ((hi[0] - lo[0]) * (hi[1] - lo[1]), *h, uv.map(|v| v.clamp(0.0, 1.0)))
                })
                .collect();
            sizes.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut pack = PackWriter::create(&g.join("canvases.pack")).map_err(|e| e.to_string())?;
            for ((_, state, uv), objd) in sizes.into_iter().zip([small, medium, large]) {
                let model = (modl.0, CANVAS_GROUP + out.canvases.len() as u32, modl.2);
                pack.add(model, &crate::bake::bake_model_state(pkgs, &crate::types::rkey(modl), Some(state))).map_err(|e| e.to_string())?;
                out.canvases.push(CanvasInfo { objd, model, uv });
            }
            pack.finish().map_err(|e| e.to_string())?;
        }
    }
    // The easels' canvas slots (their rigs' `_cntm_…Painting` bones).
    for c in catalog.iter().filter(|c| c.script.contains("HobbiesSkills") && c.script.contains("Easel") && !c.script.contains("Canvas")) {
        // (OBJD → OBJK → its VPXY, which names the rig.)
        let rig = pkgs
            .read(&crate::types::rkey(c.objd))
            .and_then(|d| s3formats::object::objd_objk(pkgs, &d))
            .and_then(|o| o.model_key)
            .and_then(|k| pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)))
            .and_then(|v| s3formats::model::vpxy_keys(&v).into_iter().find(|k| k.t == 0x8EAF13DE))
            .and_then(|k| pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)))
            .and_then(|d| s3formats::sim::Rig::parse(&d).ok());
        let Some(rig) = rig else { continue };
        let slot = |want: &dyn Fn(&str) -> bool| {
            rig.bones.iter().position(|b| {
                let n = b.name.to_ascii_lowercase();
                n.starts_with("_cntm_") && n.contains("painting") && want(&n)
            })
        };
        if let (Some(s), Some(l)) = (slot(&|n| n.contains("small")), slot(&|n| n.contains("large") || n.contains("med"))) {
            out.easels.push(EaselSlots { objd: c.objd, small: bone_transform(&rig, s), large: bone_transform(&rig, l) });
        }
    }
    let n = out.paintings.len();
    write_value(&g.join("paintings.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// The eye colour part's presets: its iris overlay and colours.
fn bake_eye_colors(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    use s3formats::catalog::CValue;
    let mut out = EyeColors::default();
    let part = pkgs.read_ti(s3pkg::types::CASP, s3pkg::fnv64("amFaceEyeColor")).and_then(|d| s3formats::sim::CasPart::parse(&d).ok());
    for p in part.map(|c| c.presets).unwrap_or_default() {
        let Some((c, keys)) = s3formats::complate::preset(&p) else { continue };
        let value = |c: &s3formats::catalog::Complate, name: &str| c.overrides.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        if out.overlay.is_none()
            && let Some(CValue::Tgi(i)) = value(&c, "Face Overlay")
            && let Some(k) = keys.get(i as usize)
        {
            out.overlay = Some((k.t, k.g, k.i));
        }
        let colour = c.blocks.iter().find_map(|b| match value(b, "Color") {
            Some(CValue::Str(s)) => {
                let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (v.len() >= 3).then(|| [v[0], v[1], v[2]])
            }
            _ => None,
        });
        out.presets.extend(colour);
    }
    if let Some(k) = out.overlay
        && !root.tex_path(k).exists()
        && let Some(dds) = crate::bake::bake_texture(pkgs, k, 512, false)
    {
        let _ = std::fs::write(root.tex_path(k), dds);
    }
    let n = out.presets.len();
    write_value(&root.global_dir().join("eye_colors.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

fn bake_face_bones(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    let names: Vec<&str> = FACE_SLIDERS.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let out: Vec<FaceBones> = FACE_PREFIXES
        .iter()
        .map(|prefix| {
            let sliders = names
                .iter()
                .enumerate()
                .filter_map(|(i, n)| {
                    let key = s3pkg::ResourceKey::new(s3formats::sim::T_BOND, 1, s3pkg::fnv32(&format!("{prefix}FaceDeformations{n}")) as u64);
                    let adj = s3formats::sim::parse_bond(&pkgs.read(&key)?).ok()?;
                    let bones: Vec<FaceBone> = adj.iter().map(|a| FaceBone { bone: a.bone, offset: a.offset, scale: a.scale, rotation: a.rotation }).collect();
                    (!bones.is_empty()).then_some((i as u8, bones))
                })
                .collect();
            FaceBones { prefix: prefix.to_string(), sliders }
        })
        .collect();
    let n = out.iter().map(|f| f.sliders.len()).sum();
    write_value(&root.global_dir().join("face_bones.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// Create a Sim's Face tab: beards, glasses, lipsticks and eye shadows.
pub const FACE_TYPES: [u32; 4] = [s3formats::sim::CT_BEARD, s3formats::sim::CT_GLASSES, s3formats::sim::CT_LIPSTICK, s3formats::sim::CT_EYESHADOW];

/// The CAS parts of the town's service uniforms, by name.
pub const SERVICE_UNIFORMS: [&str; 5] = ["BodyFrenchMaid", "HairFrenchMaidBunLoose", "BodyRepair", "BodyMailCarrier", "BodyPizzaDeliveryOutfit"];

fn bake_outfits(root: &BakeRoot, pkgs: &PackageSet, careers: &[CareerInfo]) -> Result<(), String> {
    use crate::types::{CasBaked, CasPartInfo, CasPartMeshes, Key, OutfitInfo, OutfitPartInfo, key_of};
    use s3formats::sim::{CasPart, SimOutfit, T_OUTFIT};
    let g = root.global_dir();
    let mut names: Vec<String> = careers.iter().flat_map(|c| c.levels.iter().flat_map(|l| l.outfits.iter().cloned())).filter(|n| !n.is_empty()).collect();
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let cas: CasBaked = read_value(&g.join("cas.bin")).map_err(|e| format!("cas.bin: {e}"))?;
    let rig = cas.adult_rig.as_ref().ok_or("no adult rig")?;
    let have: std::collections::HashSet<Key> = cas.parts.iter().filter(|p| p.baked).map(|p| p.key).collect();
    // (Of those, the ones with meshes: the rest are layers alone, like eyebrows.)
    let cas_pack = PackReader::open(&g.join("cas.pack")).map_err(|e| e.to_string())?;
    let have_meshes: std::collections::HashSet<Key> =
        have.iter().copied().filter(|k| cas_pack.get::<CasPartMeshes>(k).is_some_and(|m| !m.meshes.is_empty())).collect();
    std::fs::create_dir_all(root.textures_dir()).ok();
    type Baked = (OutfitInfo, Vec<(Key, CasPartMeshes)>);
    let baked: Vec<Option<Baked>> = crate::bake::par_map(&names, |name| {
        let o = SimOutfit::parse(&pkgs.read_ti(T_OUTFIT, s3pkg::fnv64(name))?).ok()?;
        let mut info = OutfitInfo { name: name.to_ascii_lowercase(), parts: Vec::new() };
        let mut meshes = Vec::new();
        for (i, p) in o.parts.iter().enumerate() {
            let Some(c) = pkgs.read(&p.casp).or_else(|| pkgs.read_ti(p.casp.t, p.casp.i)).and_then(|d| CasPart::parse(&d).ok()) else { continue };
            let key = key_of(&p.casp);
            let mut layer_only = have.contains(&key) && !have_meshes.contains(&key);
            if !have.contains(&key) {
                let m = crate::bake::cas_part_meshes(pkgs, &c, rig, cas.baby_rig.as_ref());
                layer_only = m.meshes.is_empty();
                for t in m.meshes.iter().filter_map(|m| m.texture) {
                    if !root.tex_path(t).exists()
                        && let Some(dds) = crate::bake::bake_texture(pkgs, t, 512, false)
                    {
                        let _ = std::fs::write(root.tex_path(t), dds);
                    }
                }
                meshes.push((key, m));
            }
            // The part's layer in the outfit's colours: clothes as a layer over the skin (the
            // compositor's second target), hair whole (its hat drawn over it there).
            let hair = c.clothing_type == s3formats::sim::CT_HAIR;
            let layer: Key = (T_OUTFIT, i as u32, s3pkg::fnv64(name));
            let img = s3formats::complate::render_preset(pkgs, &p.preset, 512, !hair);
            if let Some(img) = &img {
                let _ = std::fs::write(root.tex_path(layer), crate::ddsw::encode_dds(img));
            }
            // (Without a design of its own, the part's default.)
            let fallback = c.diffuse.first().map(key_of).filter(|_| img.is_none());
            if let Some(k) = fallback
                && !root.tex_path(k).exists()
                && let Some(dds) = crate::bake::bake_texture(pkgs, k, 512, !hair)
            {
                let _ = std::fs::write(root.tex_path(k), dds);
            }
            // A hat's own layer, so the hair under it can take the Sim's colour.
            let mut hat = None;
            if hair && p.preset.contains(r#"key="IsHat" value="true""#) {
                let k: Key = (T_OUTFIT, 0x100 + i as u32, s3pkg::fnv64(name));
                if let Some(img) = s3formats::complate::render_preset(pkgs, &p.preset, 512, true) {
                    let _ = std::fs::write(root.tex_path(k), crate::ddsw::encode_dds(&img));
                    hat = Some(k);
                }
            }
            info.parts.push(OutfitPartInfo {
                part: CasPartInfo {
                    key,
                    name: c.name,
                    clothing_type: c.clothing_type,
                    age_gender: c.age_gender,
                    category: c.category,
                    baked: true,
                    layer: img.map(|_| layer).or(fallback),
                },
                layer_only,
                hat,
            });
        }
        Some((info, meshes))
    });
    // Formal wear, with its default design; and the town's service uniforms (the maid's dress
    // and capped bun, the repairman's overalls, the mail carrier's and the pizza delivery's).
    let formal: Vec<(s3pkg::ResourceKey, CasPart)> = crate::bake::par_map(&pkgs.keys_of_type(s3pkg::types::CASP).copied().collect::<Vec<_>>(), |k| {
        let c = CasPart::parse(&pkgs.read(k)?).ok()?;
        let human = matches!((c.age_gender >> 8) & 0xF, 0 | 1) && c.age_gender & 0x7E != 0;
        let formal = c.category & s3formats::sim::CAT_FORMAL != 0
            && c.category & s3formats::sim::CAT_VALID_RANDOM != 0
            && c.category & s3formats::sim::CAT_HIDDEN == 0
            && matches!(c.clothing_type, s3formats::sim::CT_BODY | s3formats::sim::CT_TOP | s3formats::sim::CT_BOTTOM | s3formats::sim::CT_SHOES);
        let service = SERVICE_UNIFORMS.iter().any(|n| c.name.contains(n))
            && !c.name.to_ascii_lowercase().ends_with("_unlock")
            && matches!(c.clothing_type, s3formats::sim::CT_BODY | s3formats::sim::CT_HAIR);
        // The base game's beards, glasses, lipsticks and eye shadows (for Create a Sim's Face tab).
        let face = FACE_TYPES.contains(&c.clothing_type)
            && k.g == 0
            && c.category & s3formats::sim::CAT_HIDDEN == 0
            && !c.name.to_ascii_lowercase().ends_with("_unlock")
            && (matches!(c.clothing_type, s3formats::sim::CT_GLASSES | s3formats::sim::CT_BEARD) || !c.diffuse.is_empty());
        let wanted = human && (formal || service || face) && !have.contains(&key_of(k));
        wanted.then_some((*k, c))
    })
    .into_iter()
    .flatten()
    .collect();
    let formal: Vec<(CasPartInfo, CasPartMeshes)> = crate::bake::par_map(&formal, |(k, c)| {
        let m = crate::bake::cas_part_meshes(pkgs, c, rig, cas.baby_rig.as_ref());
        let layer = c.diffuse.first().map(key_of);
        let textures = m.meshes.iter().filter_map(|m| m.texture).map(|t| (t, false)).chain(layer.map(|l| (l, true)));
        for (t, as_layer) in textures {
            if !root.tex_path(t).exists()
                && let Some(dds) = crate::bake::bake_texture(pkgs, t, 512, as_layer)
            {
                let _ = std::fs::write(root.tex_path(t), dds);
            }
        }
        // (Make-up is layers drawn onto the face, with no mesh of its own, and so are the beards
        // with no mesh (goatees, chinstraps...: the game tints them with the hair colour); glasses
        // and the other beards are meshes.)
        let layer_only = matches!(c.clothing_type, s3formats::sim::CT_LIPSTICK | s3formats::sim::CT_EYESHADOW)
            || c.clothing_type == s3formats::sim::CT_BEARD && m.meshes.is_empty();
        let info = CasPartInfo { key: key_of(k), name: c.name.clone(), clothing_type: c.clothing_type, age_gender: c.age_gender, category: c.category, baked: !m.meshes.is_empty() || layer_only && layer.is_some(), layer };
        Some((info, m))
    })
    .into_iter()
    .flatten()
    .collect();
    let mut w = PackWriter::create(&g.join("outfits.pack")).map_err(|e| e.to_string())?;
    let mut seen = std::collections::HashSet::new();
    let mut wardrobe = Vec::new();
    for (info, m) in formal {
        if seen.insert(info.key) {
            w.add(info.key, &m).map_err(|e| e.to_string())?;
        }
        wardrobe.push(info);
    }
    write_value(&g.join("wardrobe.bin"), &wardrobe).map_err(|e| e.to_string())?;
    let mut outfits = Vec::new();
    for (info, meshes) in baked.into_iter().flatten() {
        for (k, m) in meshes {
            if seen.insert(k) {
                w.add(k, &m).map_err(|e| e.to_string())?;
            }
        }
        outfits.push(info);
    }
    w.finish().map_err(|e| e.to_string())?;
    write_value(&g.join("outfits.bin"), &outfits).map_err(|e| e.to_string())
}

/// The catalogue's fences (`fences.pack` holds their pieces).
fn bake_fence_styles(root: &BakeRoot, pkgs: &PackageSet, strings: &HashMap<u64, String>) -> Result<Vec<FenceStyle>, String> {
    let mut keys: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(0x0418FE2A).copied().collect();
    keys.sort();
    keys.dedup_by_key(|k| k.i);
    let styles: Vec<Option<FenceStyle>> = crate::bake::par_map(&keys, |k| {
        let d = pkgs.read(k)?;
        let (guid, price, shown) = crate::fences::catalog_entry(&d)?;
        let name = strings.get(&guid).cloned().filter(|n| !n.is_empty())?;
        if !shown {
            return None;
        }
        let p = crate::fences::pieces(pkgs, k);
        p.straight?;
        Some(FenceStyle { key: crate::types::key_of(k), name, price: price.round() as i32, straight: p.straight, diagonal: p.diagonal, post: p.post })
    });
    let mut styles: Vec<FenceStyle> = styles.into_iter().flatten().collect();
    styles.sort_by_key(|s| (s.price, s.name.clone()));
    let g = root.global_dir();
    let mut w = PackWriter::create(&g.join("fences.pack")).map_err(|e| e.to_string())?;
    let mut seen = BTreeSet::new();
    for s in &styles {
        for m in [s.straight, s.diagonal, s.post].into_iter().flatten() {
            if !seen.insert(m) {
                continue;
            }
            let b = crate::bake::bake_model(pkgs, &crate::types::rkey(m));
            for t in b.parts.iter().filter_map(|p| p.texture) {
                if !root.tex_path(t).exists()
                    && let Some(dds) = crate::bake::bake_texture(pkgs, t, crate::bake::OBJECT_TEX_MAX, false)
                {
                    let _ = std::fs::write(root.tex_path(t), dds);
                }
            }
            w.add(m, &b).map_err(|e| e.to_string())?;
        }
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(styles)
}

pub fn bake_gamedata(root: &BakeRoot, pkgs: &PackageSet, install_root: &std::path::Path, progress: &dyn Fn(&str)) -> Result<usize, String> {
    progress("Converting: moodlets, traits and skills…");
    let path = install_root.join("Game").join("Bin").join("Gameplay").join("GameplayData.package");
    let pkg = Package::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    // The package's XML by name.
    let mut names: HashMap<u64, String> = HashMap::new();
    for e in pkg.of_type(T_NMAP) {
        if let Ok(d) = pkg.read(e) {
            names.extend(s3formats::audio::parse_name_map(&d));
        }
    }
    // (Tuning for a class is named after it with a hash: `Writing_0x49f73ba878b269dc`.)
    let xml = |name: &str| -> Option<String> {
        let named = |n: &String| n.eq_ignore_ascii_case(name) || n.strip_prefix(name).is_some_and(|rest| rest.starts_with("_0x"));
        let e = pkg.of_type(T_XML).find(|e| names.get(&e.key.i).is_some_and(named))?;
        pkg.read(e).ok().map(|d| String::from_utf8_lossy(&d).into_owned())
    };
    let strings = s3formats::stbl::load_english(pkgs);
    let text = |sheet: &str, key: &str| -> String {
        if key.is_empty() {
            return String::new();
        }
        strings.get(&s3pkg::fnv64(&format!("Gameplay/Excel/{sheet}:{key}").to_ascii_lowercase())).cloned().unwrap_or_default()
    };
    let num = |f: &HashMap<String, String>, k: &str| f.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    let get = |f: &HashMap<String, String>, k: &str| f.get(k).cloned().unwrap_or_default();

    let mut out = GameDataBaked { version: GAMEDATA_VERSION, ..Default::default() };
    progress("Converting: object effect slots…");
    let mut objds: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(s3pkg::types::OBJD).copied().collect();
    objds.sort();
    out.fx_slots = crate::bake::par_map(&objds, |k| {
        let fx = s3formats::object::object_fx_slots(pkgs, k);
        (!fx.is_empty()).then(|| (crate::types::key_of(k), fx))
    })
    .into_iter()
    .flatten()
    .collect();
    // Buffs: rows without a SKU belong to the base game.
    for f in records(&xml("Buffs").ok_or("no Buffs table")?, "BuffList") {
        let hex = get(&f, "Hex");
        if hex.is_empty() || f.get("SKU").is_some_and(|s| s != "BaseGame") {
            continue;
        }
        let name_key = get(&f, "BuffName");
        out.buffs.push(BuffInfo {
            name: text("Buffs/BuffList", &name_key),
            desc: text("Buffs/BuffList", &get(&f, "BuffDescription")),
            icon: get(&f, "ThumbFilename"),
            axis: get(&f, "AxisEffected"),
            value: num(&f, "EffectValue") as i32,
            timeout: num(&f, "TimeoutLength"),
            hex,
        });
    }
    for f in records(&xml("Traits").ok_or("no Traits table")?, "TraitList") {
        let hex = get(&f, "Hex");
        if hex.is_empty() {
            continue;
        }
        let name_key = get(&f, "Name");
        out.traits.push(TraitInfo {
            name: text("Traits/TraitList", &name_key),
            desc: text("Traits/TraitList", &get(&f, "Description")),
            icon: get(&f, "ThumbFilename"),
            icon_small: get(&f, "ThumbPieMenu"),
            category: get(&f, "Category"),
            points: num(&f, "Points") as u32,
            hex,
        });
    }
    if let Some(skills) = xml("Skills") {
        for f in records(&skills, "SkillList") {
            let hex = get(&f, "Hex");
            if hex.is_empty() || f.get("CodeVersion").is_some_and(|s| s != "BaseGame") {
                continue;
            }
            out.skills.push(SkillInfo {
                name: text("Skills/SkillList", &get(&f, "SkillName")),
                desc: text("Skills/SkillList", &get(&f, "SkillDescription")),
                icon: get(&f, "IconKey"),
                wish_icon: get(&f, "DreamsAndPromisesIcon"),
                object_icon: get(&f, "SkillUIIcon"),
                max_level: num(&f, "MaxSkillLevel") as u32,
                hex,
            });
        }
    }

    // Careers: the base game's full-time careers and part-time jobs, each with its own table
    // of levels (and branches).
    if let Some(careers) = xml("Careers") {
        let day_bit = |d: &str| match d.trim() {
            "M" => 1u8,
            "T" => 2,
            "W" => 4,
            "R" => 8,
            "F" => 16,
            "S" => 32,
            "U" => 64,
            _ => 0,
        };
        for f in records(&careers, "CareerList") {
            let name_key = get(&f, "CareerName");
            let table = get(&f, "TableName");
            let category = get(&f, "Category");
            if name_key.is_empty() || table.is_empty() || f.get("ProductVersion").is_some_and(|s| s != "BaseGame") {
                continue;
            }
            if category != "FullTime" && category != "PartTime" {
                continue;
            }
            let mut levels = Vec::new();
            let mut last_skill: Vec<String> = Vec::new();
            for r in records(&careers, &table) {
                let level = num(&r, "Level") as u32;
                if level == 0 {
                    continue;
                }
                let title_key = get(&r, "Title");
                let skills: Vec<String> = (1..=4)
                    .filter(|i| r.get(&format!("Metric{i}")).is_some_and(|m| m == "SkillX"))
                    .filter_map(|i| r.get(&format!("Args{i}")).cloned())
                    .collect();
                // (Rows that don't name the skill keep the previous level's.)
                if !skills.is_empty() {
                    last_skill = skills;
                }
                let start = num(&r, "StartTime");
                levels.push(CareerLevelInfo {
                    level,
                    branch: r.get("BranchName").cloned().unwrap_or_else(|| "Base".into()),
                    title: text(&format!("Careers/{table}"), &title_key),
                    title_female: text(&format!("Careers/{table}"), &format!("{title_key}_Female")),
                    hourly: num(&r, "BasePay"),
                    start,
                    hours: num(&r, "DayLength"),
                    days: get(&r, "DaysToWork").split(',').map(day_bit).fold(0, |a, b| a | b),
                    skills: last_skill.clone(),
                    outfits: ["OutfitMale", "OutfitFemale", "OutfitMaleElder", "OutfitFemaleElder"].map(|c| get(&r, c)),
                });
            }
            if levels.is_empty() {
                continue;
            }
            out.careers.push(CareerInfo {
                name: text("Careers/CareerList", &name_key),
                desc: text("Careers/CareerList", &get(&f, "CareerDescription")),
                icon: get(&f, "DreamsAndPromisesIcon"),
                part_time: category == "PartTime",
                hex: name_key,
                levels,
            });
        }
    }

    // The build catalogue's wallpapers and floors, each rendered in its first swatch (under the
    // key lot coverings use for catalogue patterns).
    progress("Converting: wallpapers and floors…");
    {
        let mut keys: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(s3formats::catalog::T_CWAL).copied().collect();
        keys.sort();
        keys.dedup_by_key(|k| k.i);
        let done = std::sync::atomic::AtomicUsize::new(0);
        let found: Vec<PatternInfo> = crate::bake::par_map(&keys, |k| {
            let p = s3formats::catalog::WallFloorPattern::parse(&pkgs.read(k)?).ok()?;
            done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let floor = match p.pattern_type {
                s3formats::catalog::PATTERN_FLOOR => true,
                s3formats::catalog::PATTERN_WALL => false,
                _ => return None,
            };
            let name = strings.get(&p.name_guid).cloned().unwrap_or_default();
            if !p.in_catalog || p.price <= 0.0 || name.is_empty() {
                return None;
            }
            let texture = (crate::types::T_COVER, if floor { 4 } else { 3 }, k.i);
            let path = root.tex_path(texture);
            if !path.exists() {
                let m = p.materials.first()?;
                let (w, h) = if floor { (256, 256) } else { (256, 512) };
                let img = s3formats::complate::render(pkgs, &m.complate, &m.keys, w, h)?;
                std::fs::write(&path, crate::ddsw::encode_dds(&img)).ok()?;
            }
            Some(PatternInfo { name, price: p.price.round() as i32, floor, texture })
        })
        .into_iter()
        .flatten()
        .collect();
        out.patterns = found;
        out.patterns.sort_by(|a, b| a.floor.cmp(&b.floor).then(a.price.cmp(&b.price)).then(a.name.cmp(&b.name)));
        out.patterns.dedup_by(|a, b| a.floor == b.floor && a.name == b.name);
    }

    // Roof patterns: each one's first visual proxy holds its roof material, whose diffuse map
    // is the tiles' texture. The name is a string key after the header (UTF-16BE, 7-bit length).
    progress("Converting: roof patterns…");
    {
        let mut keys: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(s3formats::catalog::T_ROOF_PATTERN).copied().collect();
        keys.sort();
        keys.dedup_by_key(|k| k.i);
        for k in keys {
            let Some(d) = pkgs.read(&k) else { continue };
            if d.len() < 0x22 {
                continue;
            }
            let n = d[0x20] as usize;
            let key_name: String = if d.len() >= 0x21 + n {
                String::from_utf16_lossy(&d[0x21..0x21 + n].chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>())
            } else {
                String::new()
            };
            let guid = u64::from_le_bytes(d[0x10..0x18].try_into().unwrap());
            let name = strings
                .get(&guid)
                .or_else(|| strings.get(&s3pkg::fnv64(&key_name.to_ascii_lowercase())))
                .cloned()
                .unwrap_or_else(|| key_name.rsplit(':').next().unwrap_or("").trim_start_matches("Roof").trim_start_matches('_').to_string());
            let Ok(tgis) = s3formats::model::tgi_table_at(&d, 4) else { continue };
            let Some(vk) = tgis.iter().find(|t| t.t == s3pkg::types::VPXY) else { continue };
            let Some(vd) = pkgs.read(vk).or_else(|| pkgs.read_ti(vk.t, vk.i)) else { continue };
            let Some(m) = s3formats::model::vpxy_keys(&vd).iter().filter(|t| t.t == 0x01D0E75D).find_map(|mk| s3formats::model::load_matd_resource(pkgs, mk)) else { continue };
            let Some(tex) = m.texture(s3formats::model::P_DIFFUSE_MAP) else { continue };
            let texture: crate::types::Key = (tex.t, tex.g, tex.i);
            let path = root.tex_path(texture);
            if !path.exists() {
                let Some(dds) = crate::bake::bake_texture(pkgs, texture, 512, false) else { continue };
                if std::fs::write(&path, dds).is_err() {
                    continue;
                }
            }
            let tile: crate::types::Key = (crate::types::T_COVER, 6, k.i);
            let tile_path = root.tex_path(tile);
            if !tile_path.exists() {
                let Some(atlas) = std::fs::read(&path).ok().and_then(|d| s3formats::dds::decode(&d, 512)) else { continue };
                let Some(img) = roof_tile(&atlas) else { continue };
                if std::fs::write(&tile_path, crate::ddsw::encode_dds(&img)).is_err() {
                    continue;
                }
            }
            out.roofs.push(RoofPattern { name, texture, tile });
        }
        out.roofs.sort_by(|a, b| a.name.cmp(&b.name));
        out.roofs.dedup_by(|a, b| a.texture == b.texture);
    }

    // Collectibles and the spawners that turn them up. (Base game: rows without a CodeVersion.)
    let base = |f: &HashMap<String, String>| get(f, "CodeVersion").is_empty() && get(f, "RequiredWorld").is_empty();
    let pretty = |key: &str| {
        let mut out = String::new();
        for (i, c) in key.chars().enumerate() {
            if i > 0 && c.is_uppercase() {
                out.push(' ');
            }
            out.push(c);
        }
        out
    };
    let list = |s: &str| -> Vec<String> { s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect() };
    let weights = |items: &str, probs: &str| -> Vec<(String, f32)> {
        let w: Vec<f32> = list(probs).iter().map(|p| p.parse().unwrap_or(0.0)).collect();
        list(items).into_iter().enumerate().map(|(i, k)| (k, w.get(i).copied().unwrap_or(1.0))).collect()
    };
    let short = |class: &str| class.rsplit('.').next().unwrap_or(class).to_string();
    if let Some(x) = xml("RockGemMetal") {
        for (row, kind, sheet) in [("Gems", CollectKind::Gem, "RockGemMetal/Gems"), ("Metals", CollectKind::Metal, "RockGemMetal/Metals"), ("Rocks", CollectKind::SpaceRock, "RockGemMetal/Rocks")] {
            for f in records(&x, row) {
                let label = get(&f, "Name");
                // (Spawners name rocks by their Hex: "SpaceRockSmall".)
                let key = Some(get(&f, "Hex")).filter(|h| !h.is_empty() && h != "0").unwrap_or_else(|| label.clone());
                if label.is_empty() || !base(&f) {
                    continue;
                }
                let name = text(sheet, &label);
                let model = match kind {
                    CollectKind::Gem => "Gem".to_string(),
                    CollectKind::Metal => "Metal".to_string(),
                    _ => if key.contains("Large") { "MeteorLarge" } else { "MeteorMedium" }.to_string(),
                };
                out.collectibles.push(CollectibleInfo {
                    name: if name.is_empty() { pretty(&key) } else { name },
                    key,
                    kind,
                    min_price: num(&f, "MinPrice") as i32,
                    max_price: num(&f, "MaxPrice") as i32,
                    rarity: get(&f, "Rarity"),
                    level: num(&f, "MinHuntSkillLevel") as u8,
                    model,
                });
            }
        }
        for f in records(&x, "Spawners") {
            let class = short(&get(&f, "SpawnerClassName"));
            if class.is_empty() {
                continue;
            }
            out.spawners.push(SpawnerInfo {
                class,
                items: weights(&get(&f, "Spawns"), &get(&f, "Probability")),
                capacity: num(&f, "MaxSpawnCapacity").max(1.0) as u8,
                hours: (num(&f, "SpawnMinTime"), num(&f, "SpawnMaxTime")),
            });
        }
    }
    if let Some(x) = xml("Insects") {
        for (row, kind, sheet) in [("Butterflies", CollectKind::Butterfly, "Insects/Butterflies"), ("Beetles", CollectKind::Beetle, "Insects/Beetles")] {
            for f in records(&x, row) {
                let key = get(&f, "Type");
                let label = get(&f, "LocalizedName");
                if key.is_empty() || label.is_empty() || !base(&f) {
                    continue;
                }
                let name = text(sheet, &label);
                let noun = if kind == CollectKind::Butterfly { "Butterfly" } else { "Beetle" };
                let name = if name.is_empty() { format!("{} {noun}", pretty(&label)) } else { name };
                let value = num(&f, "Value") as i32;
                out.collectibles.push(CollectibleInfo {
                    key,
                    name,
                    kind,
                    min_price: value,
                    max_price: value,
                    rarity: get(&f, "Rarity"),
                    level: 0,
                    model: get(&f, "ModelMaterial").split(':').next().unwrap_or("").to_string(),
                });
            }
        }
        for f in records(&x, "Spawners") {
            let class = short(&get(&f, "SpawnerClassName"));
            if class.is_empty() {
                continue;
            }
            out.spawners.push(SpawnerInfo {
                class,
                items: weights(&get(&f, "Spawns"), &get(&f, "Probability")),
                capacity: num(&f, "MaxSpawnCapacity").max(1.0) as u8,
                hours: (num(&f, "MinSpawnFrequency"), num(&f, "MaxSpawnFrequency")),
            });
        }
    }
    // Writing: the skill's tuning, and the titles written books get.
    if let Some(x) = xml("Writing") {
        out.writing = tuning_values(&x);
    }
    if let Some(x) = xml("Books") {
        let mut by_genre: Vec<(String, Vec<String>)> = Vec::new();
        for f in records(&x, "WrittenBookTitles") {
            let mut genres: Vec<(&String, &String)> = f.iter().collect();
            genres.sort();
            for (genre, key) in genres {
                let title = text("Books/WrittenBookTitles", key);
                if title.is_empty() {
                    continue;
                }
                match by_genre.iter_mut().find(|(g, _)| g == genre) {
                    Some((_, v)) => v.push(title),
                    None => by_genre.push((genre.clone(), vec![title])),
                }
            }
        }
        out.book_titles = by_genre;
    }
    // Recipes, with the food they make (the dish and a plate of it, full and emptied).
    if let Some(x) = xml("RecipeMasterList") {
        progress("Converting: recipes and food…");
        // Food models by name (the name maps), each drawn in the geometry state a recipe names:
        // `foodServeRatatouille#foodFull:Default` is the `foodFull` state of that model.
        let mut food: HashMap<String, u64> = HashMap::new();
        for k in pkgs.keys_of_type(T_NMAP) {
            if let Some(d) = pkgs.read(k) {
                for (inst, n) in s3formats::audio::parse_name_map(&d) {
                    if n.starts_with("food") && !n.contains('_') && pkgs.find_ti(s3pkg::types::MODL, inst).is_some() {
                        food.insert(n.to_ascii_lowercase(), inst);
                    }
                }
            }
        }
        // (Some are hidden catalogue objects instead: `FoodEatHamburger`.)
        let catalog: Vec<crate::types::CatalogEntry> = read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        let mut models: Vec<(crate::types::Key, s3pkg::ResourceKey, u32)> = Vec::new();
        let mut model_of = |spec: &str| -> Option<crate::types::Key> {
            let (name, rest) = spec.split_once('#').unwrap_or((spec, ""));
            let state = rest.split(':').next().filter(|s| !s.is_empty());
            let modl = match food.get(&name.to_ascii_lowercase()) {
                Some(&inst) => pkgs.find_ti(s3pkg::types::MODL, inst)?,
                None => crate::types::rkey(*catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(name))?.models.first()?),
            };
            let inst = modl.i;
            let hash = state.map_or(0, s3pkg::fnv32);
            let key = (s3pkg::types::MODL, hash, inst);
            if !models.iter().any(|(k, ..)| *k == key) {
                models.push((key, modl, hash));
            }
            Some(key)
        };
        for f in records(&x, "Data") {
            let code = get(&f, "CodeVersion");
            let key = get(&f, "Recipe_Key");
            let meals = [("Breakfast", MEAL_BREAKFAST), ("Brunch", MEAL_BRUNCH), ("Lunch", MEAL_LUNCH), ("Dinner", MEAL_DINNER), ("Dessert", MEAL_DESSERT)]
                .iter()
                .filter(|(m, _)| !get(&f, m).is_empty())
                .fold(0, |a, (_, b)| a | b);
            if key.is_empty() || !(code.is_empty() || code == "BaseGame") || meals == 0 || get(&f, "Learnable") == "False" {
                continue;
            }
            let name = text("RecipeMasterList/Data", &key);
            out.recipes.push(RecipeInfo {
                name: if name.is_empty() { pretty(&key) } else { name },
                level: num(&f, "Level") as u8,
                auto: !get(&f, "Auto_Learn").is_empty(),
                meals,
                vegetarian: !get(&f, "Is_Vegetarian").is_empty(),
                cost: num(&f, "RegisterCost") as i32,
                ingredients: ["Ingredient_1", "Ingredient_2", "Ingredient_3"].iter().map(|i| get(&f, i)).filter(|i| !i.is_empty()).collect(),
                group: model_of(&get(&f, "Group_Full")),
                group_empty: model_of(&get(&f, "Group_Empty")),
                single: model_of(&get(&f, "Single_Full")),
                single_empty: model_of(&get(&f, "Single_Empty")),
                book_price: 0,
                key,
            });
        }
        // Recipe books (`Books`' `BookRecipe` rows) and their prices.
        if let Some(b) = xml("Books") {
            for f in records(&b, "BookRecipe") {
                let key = get(&f, "Recipe");
                if let Some(r) = out.recipes.iter_mut().find(|r| r.key == key) {
                    r.book_price = num(&f, "Value") as i32;
                }
            }
        }
        // The food models, and their textures.
        let g = root.global_dir();
        std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
        let mut fpack = PackWriter::create(&g.join("food.pack")).map_err(|e| e.to_string())?;
        let mut tex = BTreeSet::new();
        for (key, modl, state) in &models {
            let m = crate::bake::bake_model_state(pkgs, modl, (*state != 0).then_some(*state));
            tex.extend(m.parts.iter().filter_map(|p| p.texture));
            fpack.add(*key, &m).map_err(|e| e.to_string())?;
        }
        fpack.finish().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.textures_dir()).ok();
        for t in tex {
            if !root.tex_path(t).exists()
                && let Some(dds) = crate::bake::bake_texture(pkgs, t, crate::bake::OBJECT_TEX_MAX, false)
            {
                let _ = std::fs::write(root.tex_path(t), dds);
            }
        }
    }
    progress("Converting: career outfits…");
    bake_outfits(root, pkgs, &out.careers)?;
    let faces = bake_face_bones(root, pkgs)?;
    progress(&format!("Converting: face shapes ({faces} slider bones)…"));
    let eyes = bake_eye_colors(root, pkgs)?;
    progress(&format!("Converting: eye colours ({eyes} presets)…"));
    progress("Converting: object designs…");
    let designs = bake_object_designs(root, pkgs)?;
    progress(&format!("Converting: designs for {designs} objects…"));
    let paintings = bake_paintings(root, pkgs, xml("PaintingSkillDB"))?;
    progress(&format!("Converting: {paintings} paintings…"));
    let skins = bake_object_skins(root, pkgs)?;
    progress(&format!("Converting: {skins} objects with moving parts…"));
    progress("Converting: fences…");
    out.fences = bake_fence_styles(root, pkgs, &strings)?;
    // Catalogue models with alternative geometry states, drawn in their fullest (the objects'
    // models pack has every state at once: a chess table's every game piled on its board).
    progress("Converting: object states…");
    {
        let catalog: Vec<crate::types::CatalogEntry> = read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        let mut keys: Vec<crate::types::Key> = catalog.iter().flat_map(|c| c.models.iter().copied()).collect();
        keys.sort();
        keys.dedup();
        let baked = crate::bake::par_map(&keys, |k| {
            let rk = crate::types::rkey(*k);
            let meshes = s3formats::model::load_model(pkgs, &rk)?;
            let state = crate::bake::default_state(&meshes)?;
            Some((*k, crate::bake::bake_model_state(pkgs, &rk, Some(state))))
        });
        let g = root.global_dir();
        std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
        let mut spack = PackWriter::create(&g.join("states.pack")).map_err(|e| e.to_string())?;
        for (k, m) in baked.into_iter().flatten() {
            spack.add(k, &m).map_err(|e| e.to_string())?;
        }
        spack.finish().map_err(|e| e.to_string())?;
    }
    // Lifetime wishes: the base game's lifetime dreams, with their instances' scores.
    if let (Some(nodes), Some(inst)) = (xml("DreamsAndPromisesNodes"), xml("DreamNodeInstanceDefaults")) {
        let mut scores: HashMap<String, u32> = HashMap::new();
        for f in records(&inst, "DreamNodeInstance") {
            if let Some(s) = f.get("FulfillmentScore").and_then(|s| s.parse().ok()) {
                scores.entry(get(&f, "PrototypeId")).or_insert(s);
            }
        }
        for f in records(&nodes, "Primitives") {
            if get(&f, "LifeEventIsLifetimeDream") != "True" || get(&f, "RequiredProductVersions") != "BaseGame" {
                continue;
            }
            let id = get(&f, "Id");
            out.lifetime_wishes.push(LifetimeWishInfo {
                id: id.parse().unwrap_or(0),
                check: get(&f, "CheckFunction"),
                number: num(&f, "LifeEventInputNumber"),
                icon: get(&f, "PrimaryIcon"),
                score: scores.get(&id).copied().unwrap_or(0),
            });
        }
    }
    if let Some(x) = xml("Fishing") {
        for f in records(&x, "Fish") {
            let key = get(&f, "Name");
            if key.is_empty() || !base(&f) {
                continue;
            }
            let name = text("Fishing/Fish", &key);
            out.collectibles.push(CollectibleInfo {
                name: if name.is_empty() { pretty(&key) } else { name },
                min_price: num(&f, "MinPrice") as i32,
                max_price: num(&f, "MaxPrice") as i32,
                rarity: get(&f, "Rarity"),
                level: num(&f, "Level") as u8,
                model: get(&f, "Model_Name"),
                kind: CollectKind::Fish,
                key,
            });
        }
        for f in records(&x, "Spawner") {
            let class = short(&get(&f, "SpawnerClassName"));
            if class.is_empty() {
                continue;
            }
            out.spawners.push(SpawnerInfo {
                class,
                items: weights(&get(&f, "ActiveFish"), &get(&f, "ActiveProbability")),
                capacity: num(&f, "ActiveMaxFish").max(1.0) as u8,
                hours: (0.25, 1.0),
            });
        }
    }

    // Garden plants (the base game's everyday ones) and their produce.
    if let (Some(p), Some(ing)) = (xml("plants"), xml("ingredients")) {
        let heights: HashMap<String, String> = records(&p, "MedatorInstanceList").into_iter().map(|f| (get(&f, "MedatorName"), get(&f, "PlantHeight"))).collect();
        let produce: HashMap<String, (String, i64, String)> = records(&ing, "Data")
            .into_iter()
            .filter(|f| f.get("CodeVersion").is_none_or(|v| v == "BaseGame"))
            .filter_map(|f| Some((f.get("Plant_Name").filter(|s| !s.is_empty())?.clone(), (get(&f, "Ingredient_Key"), num(&f, "Price") as i64, get(&f, "Model_Name")))))
            .collect();
        // The produce's models by name (`tomatoesWhole#tomatoWhole`: that model in that state).
        // (A name is shared by a model's rig and footprint: only the model's instance counts.)
        let mut names: HashMap<String, u64> = HashMap::new();
        for k in pkgs.keys_of_type(T_NMAP) {
            if let Some(d) = pkgs.read(k) {
                for (i, n) in s3formats::audio::parse_name_map(&d) {
                    if pkgs.find_ti(s3pkg::types::MODL, i).is_some() {
                        names.insert(n.to_ascii_lowercase(), i);
                    }
                }
            }
        }
        // (Or a catalogue object of that name: `apple`.)
        let catalog: Vec<crate::types::CatalogEntry> = read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        let g = root.global_dir();
        std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
        let mut ppack = PackWriter::create(&g.join("produce.pack")).map_err(|e| e.to_string())?;
        let mut baked_models: BTreeSet<crate::types::Key> = BTreeSet::new();
        let mut produce_model = |spec: &str| -> Option<crate::types::Key> {
            let (name, state) = spec.split_once('#').map_or((spec, None), |(n, s)| (n, Some(s)));
            let modl = match names.get(&name.to_ascii_lowercase()) {
                Some(&i) => pkgs.find_ti(s3pkg::types::MODL, i)?,
                None => crate::types::rkey(*catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(name))?.models.first()?),
            };
            let hash = state.map_or(0, s3pkg::fnv32);
            let key = (s3pkg::types::MODL, hash, modl.i);
            if baked_models.insert(key) {
                let m = crate::bake::bake_model_state(pkgs, &modl, state.map(s3pkg::fnv32));
                for t in m.parts.iter().filter_map(|p| p.texture) {
                    if !root.tex_path(t).exists()
                        && let Some(dds) = crate::bake::bake_texture(pkgs, t, crate::bake::OBJECT_TEX_MAX, false)
                    {
                        let _ = std::fs::write(root.tex_path(t), dds);
                    }
                }
                ppack.add(key, &m).ok()?;
            }
            Some(key)
        };
        for f in records(&p, "PlantList") {
            let name = get(&f, "PlantName");
            let rarity = get(&f, "Rarity");
            if name.is_empty() || f.get("CodeVersion").is_some_and(|v| v != "BaseGame") || !matches!(rarity.as_str(), "Common" | "Uncommon" | "Rare") {
                continue;
            }
            let Some((produce, price, spec)) = produce.get(&name).cloned() else { continue };
            let model = get(&f, "MedatorName");
            let produce_model = produce_model(&spec);
            out.plants.push(PlantInfo {
                height: heights.get(&model).cloned().unwrap_or_else(|| "Medium".into()),
                name,
                rarity,
                model,
                produce,
                price,
                harvest_min: num(&f, "NumHarvestablesMin") as u32,
                harvest_max: num(&f, "NumHarvestablesMax") as u32,
                lifetime: num(&f, "NumLifetimeHarvestables") as u32,
                water_decay: num(&f, "WaterDecay"),
                weeds: num(&f, "WeedProblem"),
                skill_plant: num(&f, "SkillPointsPlant"),
                skill_harvest: num(&f, "SkillPointsHarvest"),
                produce_model,
            });
        }
        ppack.finish().map_err(|e| e.to_string())?;
    }

    // Opportunities (the base game's), the ones done at a rabbit hole.
    if let Some(x) = xml("Opportunities_BaseGame") {
        let by_guid = |sheet: &str| -> HashMap<String, HashMap<String, String>> {
            records(&x, sheet).into_iter().filter_map(|f| Some((f.get("GUID").filter(|g| !g.is_empty())?.clone(), f))).collect()
        };
        let (setup, reqs, done, names) = (by_guid("OpportunitiesSetup"), by_guid("OpportunitiesRequirements"), by_guid("OpportunitiesCompletion"), by_guid("Names"));
        // The text keys' workbook isn't the XML's name: try the likely ones.
        let opp_text = |key: &str| -> String {
            if key.is_empty() {
                return String::new();
            }
            ["Opportunities_BaseGame/Names", "Opportunities/Names", "Opportunities_BaseGame/OpportunitiesSetup", "Opportunities/OpportunitiesSetup"]
                .iter()
                .find_map(|t| strings.get(&s3pkg::fnv64(&format!("Gameplay/Excel/{t}:{key}").to_ascii_lowercase())).cloned())
                .unwrap_or_default()
        };
        let hour = |t: &str| -> f32 {
            let t = t.trim().to_ascii_uppercase();
            let pm = t.ends_with("PM");
            let core = t.trim_end_matches("AM").trim_end_matches("PM");
            let (h, m) = core.split_once(':').unwrap_or((core, "0"));
            let h: f32 = h.parse().unwrap_or(0.0);
            let m: f32 = m.parse().unwrap_or(0.0);
            (h % 12.0) + if pm { 12.0 } else { 0.0 } + m / 60.0
        };
        let careers = [
            ("BusinessCareer", "Business"),
            ("CriminalCareer", "Criminal"),
            ("CulinaryCareer", "Culinary"),
            ("PoliticalCareer", "Political"),
            ("AthleticCareer", "ProfessionalSports"),
            ("ScienceCareer", "Science"),
            ("JournalismCareer", "Journalism"),
            ("LawEnforcement", "LawEnforcement"),
            ("MilitaryCareer", "Military"),
            ("MusicCareer", "Music"),
            ("MedicalCareer", "Medical"),
        ];
        let mut guids: Vec<&String> = setup.keys().collect();
        guids.sort();
        for g in guids {
            let s = &setup[g];
            // (Only the ones that are done by going there: some also need a feat first, which
            // the game listens for with its event listeners.)
            // (Nor ones that need things brought along: paintings, fish, produce.)
            if get(s, "CompletionEvent") != "kVisitedRabbitHole"
                || get(s, "Target") != "RabbitHole"
                || !get(s, "Object").is_empty()
                || !get(s, "EventListenerInfo1").is_empty()
                || !get(s, "TargetInteractionItemRequired").is_empty()
                || !get(s, "TargetInteractionNumberItemsRequired").is_empty()
            {
                continue;
            }
            let mut o = OpportunityInfo { guid: g.clone(), ..Default::default() };
            o.rabbit_hole = get(s, "TargetData");
            o.icon = get(s, "Icon");
            o.minutes = num(s, "TargetInteractionLength").max(15.0);
            o.open = s.get("TargetInteractionStartTime").map_or(0.0, |t| hour(t));
            o.close = s.get("TargetInteractionEndTime").map_or(24.0, |t| hour(t));
            if o.close <= o.open {
                o.close = 24.0;
            }
            o.days = if get(s, "Timeout") == "SimDays" { num(s, "TimeoutData") } else { 0.0 };
            o.repeat = get(s, "RepeatLevel") == "Always";
            let prefix = g.split('_').next().unwrap_or("");
            if get(s, "OpportunityType") == "Skill" {
                // (The skill comes from the requirements.)
            } else if let Some((_, c)) = careers.iter().find(|(p, _)| *p == prefix) {
                o.career = c.to_string();
            } else {
                continue;
            }
            // Requirements: only the ones this game can tell.
            let mut ok = true;
            for (k, v) in reqs.get(g).into_iter().flatten() {
                if !k.starts_with("Requirement") || v.is_empty() {
                    continue;
                }
                let p: Vec<&str> = v.split(',').map(|x| x.trim()).collect();
                match p[0] {
                    "Skill" if p.len() >= 4 => {
                        o.skill = p[1].to_string();
                        o.skill_min = p[2].parse().unwrap_or(0);
                        o.skill_max = p[3].parse().unwrap_or(10);
                    }
                    "WorldHasRabbitHoleType" => {}
                    _ => ok = false,
                }
            }
            if !ok || (o.career.is_empty() && o.skill.is_empty()) {
                continue;
            }
            for (k, v) in done.get(g).into_iter().flatten() {
                if !k.starts_with("CompletionWinReward") && !k.starts_with("CompletionModifier") {
                    continue;
                }
                let p: Vec<&str> = v.split(',').map(|x| x.trim()).collect();
                let n = |i: usize| p.get(i).and_then(|x| x.parse::<f32>().ok()).unwrap_or(0.0);
                match p[0] {
                    "Money" => o.money += n(1) as i64,
                    "CareerPerformance" => o.performance += n(1),
                    "CareerRaise" => o.raise += n(1),
                    "SkillPercentage" => o.skill_reward += n(2).max(n(1)),
                    "Skill" => o.skill_reward += n(2),
                    _ => {}
                }
            }
            let nm = names.get(g);
            let key = |k: &str| nm.map(|f| get(f, k)).unwrap_or_default();
            o.name = opp_text(&key("OpportunityName"));
            o.desc = opp_text(&key("OpportunityDescription"));
            o.completion = opp_text(&key("CompletionText"));
            o.failure = opp_text(&key("FailureText"));
            o.interaction = opp_text(&get(s, "TargetInteractionName"));
            if o.name.is_empty() {
                continue;
            }
            out.opportunities.push(o);
        }
    }

    // Balloons: each table's rows in order; a row without a key continues the list above it.
    if let Some(b) = xml("Balloons") {
        for (row, key_tag, table) in [
            ("Idle", "IdleKey", &mut out.balloons.idle),
            ("Social", "ActionKey", &mut out.balloons.social),
            ("Topic", "Key", &mut out.balloons.topic),
            ("Random", "Key", &mut out.balloons.random),
        ] {
            let mut current = String::new();
            for f in records(&b, row) {
                let key = get(&f, key_tag);
                if !key.is_empty() {
                    current = key;
                }
                let (icon, refkey) = (get(&f, "BalloonName"), get(&f, "ReferencedKey"));
                if current.is_empty() || (icon.is_empty() && refkey.is_empty()) {
                    continue;
                }
                let axis = match f.get("BalloonAxis").map(String::as_str) {
                    Some("kLike") => 1,
                    Some("kDislike") => 2,
                    _ => 0,
                };
                let weight = f.get("Weight").and_then(|w| w.parse().ok()).unwrap_or(1.0);
                table.entry(current.clone()).or_default().push(BalloonEntry { icon, refkey, axis, weight });
            }
        }
    }

    // Icons: everything these tables name, plus interface pieces used directly.
    progress("Converting: interface icons…");
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for b in &out.buffs {
        wanted.insert(b.icon.clone());
    }
    for t in &out.traits {
        wanted.insert(t.icon.clone());
        wanted.insert(t.icon_small.clone());
    }
    for s in &out.skills {
        wanted.extend([s.icon.clone(), s.wish_icon.clone(), s.object_icon.clone()]);
    }
    for c in &out.careers {
        wanted.insert(c.icon.clone());
    }
    for o in &out.opportunities {
        wanted.insert(o.icon.clone());
    }
    for w in &out.lifetime_wishes {
        wanted.insert(w.icon.clone());
    }
    wanted.extend(EXTRA_ICONS.iter().map(|s| s.to_string()));
    // Create a Sim's favourites: a picture of each food (by recipe), music and colour.
    for r in &out.recipes {
        wanted.insert(format!("cas_favorites_food_i_{}_r2", r.key.to_ascii_lowercase()));
    }
    for m in FAVORITE_MUSIC {
        wanted.insert(format!("cas_favorites_music_i_{m}_r2"));
    }
    for c in FAVORITE_COLORS {
        wanted.insert(format!("cas_favorites_color_i_{c}_r2"));
    }
    wanted.remove("");
    let g = root.global_dir();
    std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
    let mut pack = PackWriter::create(&g.join("icons.pack")).map_err(|e| e.to_string())?;
    let mut n = 0;
    for name in &wanted {
        let lower = name.to_ascii_lowercase();
        if let Some(png) = pkgs.read_ti(T_ICON, s3pkg::fnv64(&lower)) {
            pack.add(icon_key(name), &png).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // The catalogue's object thumbnails (the first colour variant of each), for buy mode: the
    // base game's and each installed pack's (`EP*/Thumbnails`, `SP*/Thumbnails`).
    progress("Converting: catalogue thumbnails…");
    let mut dirs = vec![install_root.join("Thumbnails")];
    if let Ok(rd) = std::fs::read_dir(install_root) {
        let mut packs: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path().join("Thumbnails")).filter(|p| p.is_dir()).collect();
        packs.sort();
        dirs.extend(packs);
    }
    // (Objects, then Create-a-Sim parts.)
    for (file, kind, name) in [
        ("AllThumbnails.package", T_THUMB_LARGE, thumb_name as fn(u64) -> String),
        ("CasThumbnails.package", T_CAS_THUMB, cas_thumb_name as fn(u64) -> String),
    ] {
        let mut seen: BTreeSet<u64> = BTreeSet::new();
        for dir in &dirs {
            let Ok(tp) = Package::open(dir.join(file)) else { continue };
            let mut first: HashMap<u64, &s3pkg::IndexEntry> = HashMap::new();
            for e in tp.of_type(kind) {
                let keep = !seen.contains(&e.key.i) && first.get(&e.key.i).is_none_or(|f| e.key.g < f.key.g);
                if keep {
                    first.insert(e.key.i, e);
                }
            }
            for (i, e) in first {
                if let Ok(png) = tp.read(e) {
                    pack.add(icon_key(&name(i)), &png).map_err(|e| e.to_string())?;
                    seen.insert(i);
                    n += 1;
                }
            }
        }
    }

    // Map tags: the venue glyphs cut from the game's map-tag atlas (it has no layout table of its
    // own; these are the glyphs' places in it), stored as `maptag_<venue>`.
    if let Some(atlas) = pkgs.read_ti(T_ICON, s3pkg::fnv64(&"ATLAS_MapTagColors_00".to_ascii_lowercase())).and_then(|p| crate::bake::decode_png(&p)) {
        for (name, [x0, y0, x1, y1]) in MAP_TAG_GLYPHS {
            let (x0, y0) = (x0.saturating_sub(2), y0.saturating_sub(2));
            let (x1, y1) = ((x1 + 2).min(atlas.width), (y1 + 2).min(atlas.height));
            // Square, centred.
            let side = (x1 - x0).max(y1 - y0);
            let mut img = s3formats::dds::Rgba { width: side, height: side, data: vec![0; side * side * 4] };
            let (ox, oy) = ((side - (x1 - x0)) / 2, (side - (y1 - y0)) / 2);
            for y in y0..y1 {
                for x in x0..x1 {
                    let s = (y * atlas.width + x) * 4;
                    let d = ((y - y0 + oy) * side + (x - x0 + ox)) * 4;
                    img.data[d..d + 4].copy_from_slice(&atlas.data[s..s + 4]);
                }
            }
            pack.add(icon_key(&format!("maptag_{name}")), &encode_png(&img)).map_err(|e| e.to_string())?;
            n += 1;
        }
    }

    // Balloon pictures are textures: decoded and stored as PNG beside the interface icons.
    let mut balloon_icons: BTreeSet<String> = BALLOON_FRAMES.iter().chain(SKY_TEXTURES).map(|s| s.to_string()).collect();
    for t in [&out.balloons.idle, &out.balloons.social, &out.balloons.topic, &out.balloons.random] {
        for e in t.values().flatten() {
            balloon_icons.insert(e.icon.clone());
        }
    }
    balloon_icons.retain(|n| !n.is_empty() && !wanted.contains(n));
    let mut have: BTreeSet<String> = BTreeSet::new();
    for name in &balloon_icons {
        let Some(dds) = pkgs.read_ti(T_DDS, s3pkg::fnv64(&name.to_ascii_lowercase())) else { continue };
        let Some(img) = s3formats::dds::decode(&dds, 128) else { continue };
        pack.add(icon_key(name), &encode_png(&img)).map_err(|e| e.to_string())?;
        have.insert(name.clone());
        n += 1;
    }
    // Lists keep only the icons there are pictures for (and the special pickers).
    let known = |e: &BalloonEntry| !e.refkey.is_empty() || have.contains(&e.icon) || wanted.contains(&e.icon) || SPECIAL_PICKERS.contains(&e.icon.as_str());
    for t in [&mut out.balloons.idle, &mut out.balloons.social, &mut out.balloons.topic, &mut out.balloons.random] {
        for list in t.values_mut() {
            list.retain(known);
        }
        t.retain(|_, l| !l.is_empty());
    }
    pack.finish().map_err(|e| e.to_string())?;
    write_value(&g.join("gamedata.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// Encodes RGBA8 as PNG.
fn encode_png(img: &s3formats::dds::Rgba) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width as u32, img.height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    if let Ok(mut w) = enc.write_header() {
        let _ = w.write_image_data(&img.data);
    }
    out
}

/// The balloons themselves: thought clouds (the second for dreams) with the little bubbles that
/// lead up to them, speech balloons, and the like / dislike marks.
pub const BALLOON_FRAMES: &[&str] = &[
    "thought_balloon",
    "thought_balloon2",
    "thought_balloonLead",
    "speech_balloon",
    "speech_balloon2",
    "sb_like",
    "sb_dislike",
    "t_balloon_routefail",
];

/// Venue glyphs in `ATLAS_MapTagColors_00` (pixel rectangles x0, y0, x1, y1).
const MAP_TAG_GLYPHS: &[(&str, [usize; 4])] = &[
    ("home_active", [395, 512, 435, 547]),
    ("collectible", [232, 242, 262, 262]),
    ("home", [457, 461, 485, 486]),
    ("lot_empty", [344, 406, 373, 432]),
    ("gym", [64, 424, 93, 445]),
    ("library", [175, 463, 205, 485]),
    ("eatery", [14, 139, 31, 170]),
    ("show", [176, 573, 205, 600]),
    ("spa", [119, 459, 150, 490]),
    ("park", [10, 699, 35, 730]),
    ("pool", [61, 369, 95, 387]),
    ("museum", [119, 347, 149, 379]),
    ("cityhall", [285, 570, 320, 603]),
    ("science", [349, 348, 367, 379]),
    ("hospital", [455, 178, 487, 210]),
    ("grocery", [400, 408, 429, 429]),
    ("bookstore", [175, 463, 205, 485]),
    ("school", [61, 588, 96, 616]),
    ("police", [11, 362, 35, 394]),
    ("firestation", [285, 519, 320, 542]),
    ("military", [343, 522, 374, 539]),
    ("business", [17, 20, 48, 45]),
    ("graveyard", [293, 403, 312, 434]),
    ("stadium", [120, 292, 149, 321]),
    ("bar", [73, 304, 85, 342]),
];

/// The sky's textures: cloud noise, the night's stars, the sun and its halo, the moon's halo.
pub const SKY_TEXTURES: &[&str] = &["CloudNoiseBase", "NightSkyStarsFlat", "Sky_Sun", "Sky_SunHalo", "Sky_MoonHalo"];

/// Balloon "icons" that are really the game's pickers, resolved while playing.
pub const SPECIAL_PICKERS: &[&str] = &[
    "GetSpeechBalloonImageForChat",
    "Thumbnail Target",
    "Thumbnail Actor",
    "Actor Career Topic",
    "Target Career Topic",
    "GetSpeechBalloonIconForCareer",
    "GetSpeechBalloonIconForTargetCareer",
];

/// Interface pieces the game draws directly (not named by the tables).
/// The favourites Create a Sim offers besides foods (which are the recipes): music, and
/// colours (as the game's pictures name them).
pub const FAVORITE_MUSIC: [&str; 10] = ["classical", "custom", "electronica", "indie", "kids", "latin", "pop", "rockabilly", "roots", "soul"];
pub const FAVORITE_COLORS: [&str; 20] = [
    "aqua", "black", "blue", "green", "grey", "hotpink", "irishgreen", "lilac", "lime", "orange", "pink", "purple", "red", "seafoam", "spiceberry", "spicebrown",
    "turquoise", "violet", "white", "yellow",
];

const EXTRA_ICONS: &[&str] = &[
    "hud_icon_plumbob_r2",
    "hud_icon_plumbobGlow_r2",
    "moodlet_itsGirl",
    "moodlet_itsTwins",
    "skill_journal_full_star_r2",
    "hud_icon_maptagbase_r2",
    "hud_icon_maptagbase_shell",
    "skill_journal_empty_star_r2",
    // Wishes.
    "w_tv",
    "w_book",
    "w_chess",
    "w_stereo",
    "w_stove",
    "w_bathtub",
    "w_workout_bench",
    "w_painting",
    "w_guitar",
    "W_computer",
    "w_friend",
    "w_joke_around",
    "w_first_kiss",
    "w_wedding_arch",
    "w_career_cityhall",
    "w_simoleon",
    "w_simoleon_32",
];
