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

pub const GAMEDATA_VERSION: u32 = 3;
/// Interface images.
pub const T_ICON: u32 = 0x2F7D0004;
const T_XML: u32 = 0x0333406C;
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GameDataBaked {
    pub version: u32,
    pub buffs: Vec<BuffInfo>,
    pub traits: Vec<TraitInfo>,
    pub skills: Vec<SkillInfo>,
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
    pack.finish().map_err(|e| e.to_string())?;
    write_value(&g.join("gamedata.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

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
