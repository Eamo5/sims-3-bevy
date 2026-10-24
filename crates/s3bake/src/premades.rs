//! The world's premade households (`s3formats::premade`), with names and descriptions resolved
//! from the string tables and the family portraits converted to DDS.

use std::collections::HashMap;

use s3formats::premade::{PremadeRelationship, PremadeSim};
use s3pkg::Package;
use serde::{Deserialize, Serialize};

use crate::bake::{BakeRoot, decode_png};
use crate::ddsw::encode_dds;
use crate::pack::{read_value, write_value};
use crate::types::Key;

pub const PREMADES_VERSION: u32 = 6;
/// Household portraits in the world file (by household id).
pub const T_HOUSEHOLD_PORTRAIT: u32 = 0x6B6D837E;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HouseholdBaked {
    pub id: u64,
    pub name: String,
    pub bio: String,
    /// Id of the home lot (0: no home).
    pub lot_id: u64,
    pub funds: i64,
    /// Family portrait in the texture store.
    pub portrait: Option<Key>,
    /// Sims with their names and descriptions resolved.
    pub members: Vec<PremadeSim>,
    /// Their pets, likewise.
    #[serde(default)]
    pub pets: Vec<PremadeSim>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PremadesBaked {
    pub version: u32,
    pub households: Vec<HouseholdBaked>,
    pub relationships: Vec<PremadeRelationship>,
}

impl PremadesBaked {
    /// Households living on a lot, which the player can pick.
    pub fn playable(&self) -> impl Iterator<Item = &HouseholdBaked> {
        self.households.iter().filter(|h| h.lot_id != 0)
    }

    pub fn sim(&self, id: u64) -> Option<&PremadeSim> {
        self.households.iter().flat_map(|h| &h.members).find(|s| s.id == id)
    }
}

pub fn premades_path(root: &BakeRoot, world: &str) -> std::path::PathBuf {
    root.world_dir(world).join("premades.bin")
}

pub fn premades_ready(root: &BakeRoot, world: &str) -> bool {
    read_value::<PremadesBaked>(&premades_path(root, world)).is_ok_and(|p| p.version == PREMADES_VERSION)
}

pub fn load_premades(root: &BakeRoot, world: &str) -> Option<PremadesBaked> {
    read_value::<PremadesBaked>(&premades_path(root, world)).ok().filter(|p| p.version == PREMADES_VERSION)
}

/// Reads the premade households of a world package; `strings` is the English string table.
pub fn bake_premades(root: &BakeRoot, pkg: &Package, world: &str, strings: &HashMap<u64, String>) -> Result<usize, String> {
    let Some(e) = pkg.of_type(s3formats::objs::T_OBJS).next() else {
        // Worlds without saved households still get an (empty) file.
        let empty = PremadesBaked { version: PREMADES_VERSION, ..Default::default() };
        std::fs::create_dir_all(root.world_dir(world)).map_err(|e| e.to_string())?;
        write_value(&premades_path(root, world), &empty).map_err(|e| e.to_string())?;
        return Ok(0);
    };
    let d = pkg.read(e).map_err(|e| e.to_string())?;
    let objs = s3formats::objs::ObjStream::parse(&d).ok_or("unreadable object stream")?;
    let premades = s3formats::premade::read(&objs);
    // A key like "Gameplay/Excel/PV/Sims:Gunther" is looked up by its FNV-64; household names
    // without a key ("Landgraab") are already plain text.
    let tr = |k: &str| -> String {
        if k.is_empty() {
            return String::new();
        }
        strings.get(&s3pkg::fnv64(k)).cloned().unwrap_or_else(|| {
            let tail = k.rsplit(':').next().unwrap_or(k);
            tail.trim_end_matches("_Female").trim_end_matches("Description").to_string()
        })
    };
    std::fs::create_dir_all(root.textures_dir()).map_err(|e| e.to_string())?;
    let mut households = Vec::new();
    for h in premades.households {
        let portrait_key: Key = (T_HOUSEHOLD_PORTRAIT, 0, h.id);
        let path = root.tex_path(portrait_key);
        let portrait = if path.exists() {
            Some(portrait_key)
        } else {
            let res = s3pkg::ResourceKey::new(T_HOUSEHOLD_PORTRAIT, 0, h.id);
            pkg.find(&res).and_then(|e| pkg.read(e).ok()).and_then(|d| decode_png(&d)).and_then(|img| {
                std::fs::write(&path, encode_dds(&img)).ok()?;
                Some(portrait_key)
            })
        };
        let named = |list: Vec<PremadeSim>| -> Vec<PremadeSim> {
            list.into_iter()
                .map(|mut s| {
                    s.first_name = tr(&s.first_name);
                    s.last_name = tr(&s.last_name);
                    s.bio = if s.bio.is_empty() { String::new() } else { strings.get(&s3pkg::fnv64(&s.bio)).cloned().unwrap_or_default() };
                    s
                })
                .collect()
        };
        let members = named(h.members);
        let pets = named(h.pets);
        households.push(HouseholdBaked {
            id: h.id,
            name: tr(&h.name),
            bio: strings.get(&s3pkg::fnv64(&h.bio)).cloned().unwrap_or_default(),
            lot_id: h.lot_id,
            funds: h.funds,
            portrait,
            members,
            pets,
        });
    }
    // What the world's `CharacterImport` table says of them as the game loads it (its
    // `SetGameplayInformationMode`): celebrity levels, and life states (`SupernaturalData`).
    let import = character_import(pkg);
    for m in households.iter_mut().flat_map(|h| h.members.iter_mut()) {
        if let Some(i) = import.get(&m.id) {
            m.celebrity = i.0;
            if i.1.is_some() {
                m.occult = i.1.clone();
            }
        }
    }
    // Moonlight Falls is the Supernatural pack's town: where its table says nothing of them, the
    // game fills it with vampires, werewolves, witches and fairies as it runs. A third of its
    // families are one kind or another, all of them alike, and one townie in four (the kinds
    // dealt round in turn, so there are some of each).
    if world == "Moonlight Falls" && !import.values().any(|i| i.1.is_some()) {
        const KINDS: [&str; 4] = ["Vampire", "Werewolf", "Witch", "Fairy"];
        let mut next = 0;
        for h in &mut households {
            let townies = h.lot_id == 0;
            let family = s3pkg::fnv64(&h.name.to_ascii_lowercase());
            let kind = KINDS[next % KINDS.len()];
            if !townies && family % 3 == 0 {
                next += 1;
            }
            for m in &mut h.members {
                if townies && (m.id.rotate_left(29) ^ 0x51ED_270B) % 4 == 0 {
                    m.occult = Some(KINDS[next % KINDS.len()].to_string());
                    next += 1;
                } else if !townies && family % 3 == 0 {
                    m.occult = Some(kind.to_string());
                }
            }
        }
    }
    let n = households.len();
    let baked = PremadesBaked { version: PREMADES_VERSION, households, relationships: premades.relationships };
    std::fs::create_dir_all(root.world_dir(world)).map_err(|e| e.to_string())?;
    write_value(&premades_path(root, world), &baked).map_err(|e| e.to_string())?;
    Ok(n)
}

/// The world's `CharacterImport` table (an XML table in its package, as the game reads it on
/// load): each Sim's celebrity level and life state, by their `SimDescriptionId` (its
/// `SupernaturalData` rows by the Sims' spreadsheet ids).
pub fn character_import(pkg: &Package) -> HashMap<u64, (u8, Option<String>)> {
    let Some(x) = pkg.of_type(0x0333_406C).filter_map(|e| pkg.read(e).ok()).map(|d| String::from_utf8_lossy(&d).into_owned()).find(|x| x.contains("<CharacterImport>")) else { return HashMap::new() };
    import_rows(&x)
}

/// A `CharacterImport` table's rows: celebrity levels and life states by Sim id.
fn import_rows(x: &str) -> HashMap<u64, (u8, Option<String>)> {
    let mut out = HashMap::new();
    let occults: HashMap<String, String> = crate::gamedata::records(x, "SupernaturalData")
        .into_iter()
        .filter_map(|r| Some((r.get("SpreadSheetUniqueID")?.clone(), r.get("OccultType").filter(|o| !o.is_empty())?.clone())))
        .collect();
    for table in ["Sims", "TownieSims", "ServiceNPCSims"] {
        for r in crate::gamedata::records(x, table) {
            let Some(id) = r.get("SimDescriptionId").and_then(|v| v.parse::<u64>().ok()) else { continue };
            let celebrity = r.get("CelebrityLevel").and_then(|v| v.parse::<u8>().ok()).unwrap_or(0).min(5);
            let occult = r
                .get("OccultType")
                .filter(|o| !o.is_empty())
                .or_else(|| r.get("SpreadSheetUniqueID").and_then(|s| occults.get(s)))
                .filter(|o| matches!(o.as_str(), "Vampire" | "Werewolf" | "Witch" | "Fairy" | "Zombie"))
                .cloned();
            out.insert(id, (celebrity, occult));
        }
    }
    out
}

/// Bakes a world's premade households if they aren't yet, using the cached string table.
pub fn ensure_premades(root: &BakeRoot, world_path: &std::path::Path, world: &str) -> Result<(), String> {
    if premades_ready(root, world) {
        return Ok(());
    }
    let strings: HashMap<u64, String> = read_value(&root.global_dir().join("strings.bin")).map_err(|e| format!("strings: {e}"))?;
    let pkg = Package::open(world_path).map_err(|e| e.to_string())?;
    bake_premades(root, &pkg, world, &strings).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worlds_import_says_who_is_famous_and_what_they_are() {
        let x = "<CharacterImport><Sims><SpreadSheetUniqueID>Sim_1</SpreadSheetUniqueID><SimDescriptionId>11</SimDescriptionId><CelebrityLevel>5</CelebrityLevel><OccultType></OccultType></Sims>\
            <TownieSims><SpreadSheetUniqueID>Sim_2</SpreadSheetUniqueID><SimDescriptionId>22</SimDescriptionId><CelebrityLevel></CelebrityLevel></TownieSims>\
            <SupernaturalData><SpreadSheetUniqueID>Sim_2</SpreadSheetUniqueID><OccultType>Werewolf</OccultType></SupernaturalData></CharacterImport>";
        let rows = import_rows(x);
        assert_eq!(rows.get(&11), Some(&(5, None)));
        assert_eq!(rows.get(&22), Some(&(0, Some("Werewolf".to_string()))));
    }
}
