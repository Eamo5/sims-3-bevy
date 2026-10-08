//! The premade households a world ships with (the Goths, the Landgraabs, …), read from the
//! scripts' saved object graph ([`crate::objs`]).

use std::collections::HashMap;

use s3pkg::ResourceKey;
use serde::{Deserialize, Serialize};

use crate::enums;
use crate::objs::{ObjStream, Value};

/// `CASAgeGenderFlags`.
pub const AGE_BABY: u32 = 0x1;
pub const AGE_TODDLER: u32 = 0x2;
pub const AGE_CHILD: u32 = 0x4;
pub const AGE_TEEN: u32 = 0x8;
pub const AGE_YOUNG_ADULT: u32 = 0x10;
pub const AGE_ADULT: u32 = 0x20;
pub const AGE_ELDER: u32 = 0x40;
pub const FEMALE: u32 = 0x2000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PremadeSim {
    /// The game's `SimDescriptionId`.
    pub id: u64,
    /// Localization keys (`Gameplay/Excel/PV/Sims:Gunther`); look up FNV-64 in the strings.
    pub first_name: String,
    pub last_name: String,
    pub bio: String,
    /// `CASAgeGenderFlags` age bit.
    pub age: u32,
    pub female: bool,
    pub traits: Vec<String>,
    /// Skin tone (TONE resource) and shade within it (0–1).
    pub skin_tone: Option<(u32, u32, u64)>,
    pub skin_shade: f32,
    /// Natural hair colour, 0xAARRGGBB.
    pub hair_color: Option<u32>,
    pub fat: f32,
    pub thin: f32,
    pub fit: f32,
    pub voice_pitch: f32,
    /// Which of the three voices (0..2).
    pub voice: u32,
    /// Ids of the Sim's partner and spouse.
    pub partner: Option<u64>,
    pub spouse: Option<u64>,
    pub parents: Vec<u64>,
    pub children: Vec<u64>,
    /// Career class (`Business`, `LawEnforcement`, …) and level.
    pub career: Option<(String, i32)>,
    pub skills: Vec<(String, i32)>,
    pub favourite_color: Option<u32>,
    /// `CASAgeGenderFlags` species: 0 or 1 a person, 2 a horse, 3 a cat, 4 a dog, 5 a little dog.
    #[serde(default)]
    pub species: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PremadeHousehold {
    pub id: u64,
    /// Localization keys of the household's name and description.
    pub name: String,
    pub bio: String,
    pub lot_id: u64,
    pub funds: i64,
    pub members: Vec<PremadeSim>,
    /// The household's pets (the Pets pack's `mPetSimDescriptions`).
    #[serde(default)]
    pub pets: Vec<PremadeSim>,
}

/// A relationship between two premade Sims.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PremadeRelationship {
    pub a: u64,
    pub b: u64,
    /// Long-term liking, −100…100.
    pub liking: f32,
    /// The game's relationship state: `Spouse`, `Fiancee`, `Partner`, `GoodFriend`, …
    pub state: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Premades {
    pub households: Vec<PremadeHousehold>,
    pub relationships: Vec<PremadeRelationship>,
}

/// `LongTermRelationshipTypes`, by value, named as older worlds store them.
const LTR_STATES: [&str; 19] = [
    "",
    "Stranger",
    "Acquaintance",
    "Disliked",
    "Distant Friend",
    "Friend",
    "Good Friend",
    "Best Friend",
    "Old Friend",
    "BFF",
    "Romantic Interest",
    "Ex Spouse",
    "Ex",
    "Enemy",
    "Old Enemies",
    "Partner",
    "Fiancee",
    "Spouse",
    "",
];

fn struct_field(objs: &ObjStream, v: &Value, name: &str) -> Option<Value> {
    let Value::Struct(c, vals) = v else { return None };
    let i = objs.classes.get(*c)?.fields.iter().position(|f| f.0 == name)?;
    vals.get(i).cloned()
}

/// Reads every household with members from an `OBJS` stream.
pub fn read(objs: &ObjStream) -> Premades {
    // SimDescription object id -> game id, filled as Sims are read.
    let mut ids: HashMap<u32, u64> = HashMap::new();
    for id in objs.objects_of("Sims3.Gameplay.CAS.SimDescription") {
        if let Some(Value::Int(v)) = objs.field(id, "mSimDescriptionId") {
            ids.insert(id, v as u64);
        }
    }
    // Genealogy object -> its Sim.
    let mut genealogy_sim: HashMap<u32, u64> = HashMap::new();
    for id in objs.objects_of("Sims3.Gameplay.Socializing.Genealogy") {
        if let Some(sim) = objs.field_ref(id, "mSim").and_then(|s| ids.get(&s)) {
            genealogy_sim.insert(id, *sim);
        }
    }
    let string = |id: Option<u32>| id.and_then(|i| objs.string(i)).unwrap_or_default();
    let mut households = Vec::new();
    for hid in objs.objects_of("Sims3.Gameplay.CAS.Household") {
        let Some(f) = objs.fields(hid) else { continue };
        let get = |n: &str| f.iter().find(|x| x.0 == n).map(|x| x.1.clone());
        let members_obj = get("mMembers").and_then(|v| v.as_ref());
        let list = members_obj.and_then(|m| objs.field_ref(m, "mSimDescriptions"));
        let sim_ids = list.map(|l| objs.elements(l)).unwrap_or_default();
        if sim_ids.is_empty() {
            continue;
        }
        let mut members = Vec::new();
        for sid in sim_ids {
            if let Some(s) = read_sim(objs, sid, &ids, &genealogy_sim, &string) {
                members.push(s);
            }
        }
        let pets: Vec<PremadeSim> = members_obj
            .and_then(|m| objs.field_ref(m, "mPetSimDescriptions"))
            .map(|l| objs.elements(l))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|sid| read_sim(objs, sid, &ids, &genealogy_sim, &string))
            .collect();
        if members.is_empty() {
            continue;
        }
        households.push(PremadeHousehold {
            id: get("mHouseholdId").and_then(|v| v.as_int()).unwrap_or(0) as u64,
            name: string(get("mName").and_then(|v| v.as_ref())),
            bio: string(get("mBioText").and_then(|v| v.as_ref())),
            lot_id: get("mLotId").and_then(|v| v.as_int()).unwrap_or(0) as u64,
            funds: get("mFamilyFunds").and_then(|v| v.as_int()).unwrap_or(0),
            members,
            pets,
        });
    }
    let mut relationships = Vec::new();
    for rid in objs.objects_of("Sims3.Gameplay.Socializing.Relationship") {
        let a = objs.field_ref(rid, "SimDescriptionA").and_then(|s| ids.get(&s)).copied();
        let b = objs.field_ref(rid, "SimDescriptionB").and_then(|s| ids.get(&s)).copied();
        let (Some(a), Some(b)) = (a, b) else { continue };
        let ltr = objs.field_ref(rid, "mLTR");
        let liking = ltr.and_then(|l| objs.field(l, "mLiking")).and_then(|v| v.as_f32()).unwrap_or(0.0);
        // Older worlds store the state's name, later ones an enum.
        let state = match ltr.and_then(|l| objs.field(l, "mCurrentLtr")) {
            Some(Value::Enum(_, v)) => LTR_STATES.get(v as usize).copied().unwrap_or("").to_string(),
            _ => string(ltr.and_then(|l| objs.field_ref(l, "mCurrentLTR"))),
        };
        relationships.push(PremadeRelationship { a, b, liking, state });
    }
    Premades { households, relationships }
}

fn read_sim(
    objs: &ObjStream,
    sid: u32,
    ids: &HashMap<u32, u64>,
    genealogy_sim: &HashMap<u32, u64>,
    string: &dyn Fn(Option<u32>) -> String,
) -> Option<PremadeSim> {
    let f = objs.fields(sid)?;
    let get = |n: &str| f.iter().find(|x| x.0 == n).map(|x| x.1.clone());
    let flags = match get("mSimFlags") {
        Some(Value::Enum(_, v)) => v as u32,
        _ => 0,
    };
    let key = |v: Option<Value>| match v {
        Some(Value::Key(k)) => objs.keys.get(k as usize).map(|k: &ResourceKey| (k.t, k.g, k.i)),
        _ => None,
    };
    let traits = get("mTraitManager")
        .and_then(|v| v.as_ref())
        .and_then(|tm| objs.field_ref(tm, "mValues"))
        .map(|d| objs.dictionary(d, 8))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, _)| enums::trait_name(k).map(str::to_string))
        .collect();
    let hair_color = get("mHairColors").and_then(|v| v.as_ref()).and_then(|arr| {
        let first = *objs.elements(arr).first()?;
        let genetic = objs.field(first, "Genetic")?;
        match struct_field(objs, &genetic, "ARGB").or_else(|| match &genetic {
            Value::Struct(_, vals) => vals.first().cloned(),
            _ => None,
        }) {
            Some(Value::Int(c)) => Some(c as u32),
            _ => None,
        }
    });
    let shape = get("mCurrentShape");
    let shape_f = |n: &str| shape.as_ref().and_then(|s| struct_field(objs, s, n)).and_then(|v| v.as_f32()).unwrap_or(0.0);
    let genealogy = get("mGenealogy").and_then(|v| v.as_ref());
    let sims_of = |list: Option<u32>| -> Vec<u64> {
        list.map(|l| objs.elements(l)).unwrap_or_default().iter().filter_map(|g| genealogy_sim.get(g).copied()).collect()
    };
    let spouse = genealogy.and_then(|g| objs.field_ref(g, "mSpouse")).and_then(|g| genealogy_sim.get(&g).copied());
    let parents = sims_of(genealogy.and_then(|g| objs.field_ref(g, "mNaturalParents")));
    let children = sims_of(genealogy.and_then(|g| objs.field_ref(g, "mChildren")));
    let career = get("CareerManager").and_then(|v| v.as_ref()).and_then(|cm| objs.field_ref(cm, "mJob")).and_then(|job| {
        let class = objs.class_name(job)?.rsplit('.').next()?.to_string();
        let level = objs.field(job, "mCurLevelVal").and_then(|v| v.as_int()).unwrap_or(1) as i32;
        Some((class, level))
    });
    let skills = get("SkillManager")
        .and_then(|v| v.as_ref())
        .and_then(|sm| objs.field_ref(sm, "mValues"))
        .map(|d| objs.dictionary(d, 8))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, skill)| {
            let level = objs.field(skill, "SkillLevel").and_then(|v| v.as_int()).unwrap_or(0) as i32;
            Some((enums::skill_name(k)?.to_string(), level))
        })
        .collect();
    let favourite_color = get("mFavouriteColor").and_then(|c| match c {
        Value::Struct(_, vals) => vals.first().and_then(|v| v.as_int()).map(|v| v as u32),
        _ => None,
    });
    Some(PremadeSim {
        id: get("mSimDescriptionId").and_then(|v| v.as_int()).unwrap_or(0) as u64,
        first_name: string(get("mFirstName").and_then(|v| v.as_ref())),
        last_name: string(get("mLastName").and_then(|v| v.as_ref())),
        bio: string(get("mBio").and_then(|v| v.as_ref())),
        age: flags & 0x7F,
        female: flags & FEMALE != 0,
        species: (flags >> 8) & 0xF,
        traits,
        skin_tone: key(get("mSkinToneKey")),
        skin_shade: get("mSkinToneIndex").and_then(|v| v.as_f32()).unwrap_or(0.5),
        hair_color,
        fat: shape_f("Fat"),
        thin: shape_f("Thin"),
        fit: shape_f("mFit"),
        voice_pitch: get("mVoicePitchModifier").and_then(|v| v.as_f32()).unwrap_or(0.5),
        voice: match get("mVoiceVariation") {
            Some(Value::Enum(_, v)) => v as u32,
            _ => 0,
        },
        partner: get("mPartner").and_then(|v| v.as_ref()).and_then(|p| ids.get(&p).copied()),
        spouse,
        parents,
        children,
        career,
        skills,
        favourite_color,
    })
}
