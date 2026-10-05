//! Gameplay data from the game's own tuning tables (`Game/Bin/Gameplay/GameplayData.package`,
//! XML exported from the designers' spreadsheets): the base game's moodlets (buffs), traits and
//! skills with their names and descriptions in English and the interface icons that go with
//! them. The icons (PNG, `0x2F7D0004`, instance = fnv64 of the lowercase name) are copied to
//! `icons.pack` as they are.

use std::collections::{BTreeSet, HashMap};

use s3pkg::{Package, PackageSet};
use serde::{Deserialize, Serialize};

use crate::bake::BakeRoot;
use crate::pack::{PackReader, PackWriter, read_value, write_value};

pub const GAMEDATA_VERSION: u32 = 10;
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

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Converts the base game's buffs, traits and skills and their icons.
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
    let xml = |name: &str| -> Option<String> {
        let e = pkg.of_type(T_XML).find(|e| names.get(&e.key.i).is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
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
    wanted.extend(EXTRA_ICONS.iter().map(|s| s.to_string()));
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
const EXTRA_ICONS: &[&str] = &[
    "hud_icon_plumbob_r2",
    "hud_icon_plumbobGlow_r2",
    "moodlet_itsGirl",
    "moodlet_itsTwins",
    "skill_journal_full_star_r2",
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
