//! Pets (the Pets pack): cats, dogs, little dogs and horses, grown and young. Each kind's
//! skeleton and body, its tails, ears and manes, and its coat as the game builds it: the body's
//! grey fur shading under a base colour (the `Base` part's `PeltRegion` preset), with the coat's
//! regions and markings (`Regions…`/`Markings…` parts: a mask each, and the colours their
//! presets give them) laid over it.

use s3formats::sim::{CasPart, Rig};
use s3pkg::{PackageSet, types};
use serde::{Deserialize, Serialize};

use crate::pack::{read_value, write_value};
use crate::types::{Key, SkinMesh};

/// Bumped when the baked pets change.
pub const PETS_VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetsBaked {
    pub version: u32,
    pub kinds: Vec<PetKind>,
    /// The game's breeds (its pet outfits), each with its coat painted.
    pub breeds: Vec<PetBreed>,
}

/// A breed: one of the game's pet outfits (`SIMO`, group 0x48000000), with its coat composited
/// as the game would (its base colours and every region's colours through their masks, over
/// the fur's shading) into one texture.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetBreed {
    /// The outfit's instance.
    pub outfit: u64,
    /// Its kind (`ac`, `ad`...).
    pub kind: String,
    pub age: u32,
    pub gender: u32,
    pub coat: Key,
    /// The tail, ears and mane it wears (part names).
    pub tail: Option<String>,
    pub ears: Option<String>,
    pub mane: Option<String>,
}

/// The texture-store key of a breed's coat.
pub fn coat_key(outfit: u64) -> Key {
    (0x00B2D882, 0x5045_5431, outfit)
}

/// Coats are painted this size.
const COAT_SIZE: usize = 512;

/// A kind of pet at an age: `ac` a cat, `cc` a kitten, `ad` a dog, `cd` a puppy, `al` a little
/// dog, `cl` a little puppy, `ah` a horse, `ch` a foal.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetKind {
    pub code: String,
    pub rig: Option<Rig>,
    /// The body's meshes (fur, nose and pads, eyes...), each with its shader.
    pub body: Vec<SkinMesh>,
    /// The fur's grey shading, the coat drawn over it.
    pub shading: Option<Key>,
    /// The coat's base colour as the game gives it (linear RGB), and a second for horses.
    pub base: Vec<[f32; 3]>,
    pub tails: Vec<PetPart>,
    pub ears: Vec<PetPart>,
    pub manes: Vec<PetPart>,
    /// Coat regions and markings.
    pub regions: Vec<PetRegion>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetPart {
    pub name: String,
    pub meshes: Vec<SkinMesh>,
}

/// A coat region or marking: where (its mask, in the coat's layout) and the colours its preset
/// gives it.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetRegion {
    pub name: String,
    pub mask: Key,
    pub colors: Vec<[f32; 3]>,
}

/// The kinds, their part-name prefix and skeleton.
const KINDS: [(&str, &str); 8] = [("ac", "acRig"), ("cc", "ccRig"), ("ad", "adRig"), ("cd", "cdRig"), ("al", "alRig"), ("cl", "cdRig"), ("ah", "ahRig"), ("ch", "chRig")];

/// CAS part types for pets.
const PT_TAIL: u32 = 48;
const PT_EARS: u32 = 49;
const PT_MANE: u32 = 50;
const PT_REGION: u32 = 53;
const PT_BASE: u32 = 54;
const PT_BODY: u32 = 47;

pub fn pets_ready(root: &crate::BakeRoot) -> bool {
    read_value::<PetsBaked>(&root.global_dir().join("pets.bin")).is_ok_and(|p| p.version == PETS_VERSION)
}

pub fn load_pets(root: &crate::BakeRoot) -> Option<PetsBaked> {
    read_value::<PetsBaked>(&root.global_dir().join("pets.bin")).ok().filter(|p| p.version == PETS_VERSION)
}

/// A preset's `<value key="…" value="…" />`.
fn preset_value<'a>(preset: &'a str, key: &str) -> Option<&'a str> {
    let at = preset.find(&format!("key=\"{key}\""))?;
    let rest = &preset[at..];
    let v = rest.find("value=\"")? + 7;
    rest[v..].split('"').next()
}

/// The preset's enabled colours (sRGB as written, made linear).
fn preset_colors(preset: &str) -> Vec<[f32; 3]> {
    (1..=4)
        .filter(|i| preset_value(preset, &format!("Color{i} Enabled")).is_some_and(|v| v.eq_ignore_ascii_case("true")))
        .filter_map(|i| {
            let v: Vec<f32> = preset_value(preset, &format!("Color{i}"))?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            let lin = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            (v.len() >= 3).then(|| [lin(v[0]), lin(v[1]), lin(v[2])])
        })
        .collect()
}

pub fn bake_pets(root: &crate::BakeRoot, pkgs: &PackageSet, progress: crate::bake::Progress) -> Result<usize, String> {
    progress("Converting: pets…");
    let casps: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(types::CASP).copied().collect();
    let parsed: Vec<CasPart> = crate::bake::par_map(&casps, |k| CasPart::parse(&pkgs.read(k)?).ok()).into_iter().flatten().collect();
    let species = |c: &CasPart| (c.age_gender >> 8) & 0xF;
    let pets: Vec<&CasPart> = parsed.iter().filter(|c| (2..=5).contains(&species(c))).collect();
    let mut textures: Vec<(Key, bool)> = Vec::new();
    let mut kinds = Vec::new();
    for (code, rig_name) in KINDS {
        let rig = pkgs.read_ti(types::RIG, s3pkg::fnv64(rig_name)).and_then(|d| Rig::parse(&d).ok());
        let Some(r) = rig.as_ref() else { continue };
        let named = |c: &&&CasPart| c.name.starts_with(code);
        let meshes = |c: &CasPart| crate::bake::cas_part_meshes(pkgs, c, r, None).meshes;
        let Some(body) = pets.iter().filter(named).find(|c| c.clothing_type == PT_BODY && c.name == format!("{code}Body")) else { continue };
        let body_meshes = meshes(body);
        let shading = body_meshes.iter().find_map(|m| m.texture);
        textures.extend(shading.map(|t| (t, false)));
        // (A young one's coat comes from its grown-up kind's parts: kittens wear cats' markings.)
        let grown = format!("a{}", &code[1..]);
        let coat = |c: &&&CasPart| c.name.starts_with(code) || c.name.starts_with(&grown);
        let base = pets.iter().filter(coat).find(|c| c.clothing_type == PT_BASE).and_then(|c| c.presets.first()).map(|p| preset_colors(p)).unwrap_or_default();
        let parts = |t: u32| -> Vec<PetPart> {
            pets.iter().filter(named).filter(|c| c.clothing_type == t).map(|c| PetPart { name: c.name.clone(), meshes: meshes(c) }).filter(|p| !p.meshes.is_empty()).collect()
        };
        let regions: Vec<PetRegion> = pets
            .iter()
            .filter(coat)
            .filter(|c| c.clothing_type == PT_REGION)
            .filter_map(|c| {
                let preset = c.presets.first()?;
                let mask = preset_value(preset, "Mask")?.strip_prefix("key:")?;
                let p: Vec<&str> = mask.split(':').collect();
                let k = (u32::from_str_radix(p.first()?, 16).ok()?, u32::from_str_radix(p.get(1)?, 16).ok()?, u64::from_str_radix(p.get(2)?, 16).ok()?);
                Some(PetRegion { name: c.name.clone(), mask: k, colors: preset_colors(preset) })
            })
            .collect();
        textures.extend(regions.iter().map(|r| (r.mask, false)));
        let (tails, ears, manes) = (parts(PT_TAIL), parts(PT_EARS), parts(PT_MANE));
        for m in tails.iter().chain(&ears).chain(&manes).flat_map(|p| &p.meshes) {
            textures.extend(m.texture.map(|t| (t, false)));
        }
        kinds.push(PetKind { code: code.to_string(), rig, body: body_meshes, shading, base, tails, ears, manes, regions });
    }
    // The breeds: each outfit's coat painted.
    progress("Converting: pet breeds…");
    let read = |k: &s3pkg::ResourceKey| pkgs.read(k).or_else(|| pkgs.read_ti(k.t, k.i));
    let outfits: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(s3formats::sim::T_OUTFIT).copied().filter(|k| k.g == 0x4800_0000).collect();
    let decode = |k: Key| -> Option<s3formats::dds::Rgba> { s3formats::dds::decode(&read(&s3pkg::ResourceKey::new(k.0, k.1, k.2))?, 1024) };
    let shading_of: std::collections::HashMap<String, Option<s3formats::dds::Rgba>> = kinds.iter().map(|k| (k.code.clone(), k.shading.and_then(decode))).collect();
    let breeds: Vec<PetBreed> = crate::bake::par_map(&outfits, |k| {
        let o = s3formats::sim::SimOutfit::parse(&read(k)?).ok()?;
        let name = |p: &s3formats::sim::OutfitPart| read(&p.casp).and_then(|d| CasPart::parse(&d).ok()).map(|c| (c.name, c.clothing_type));
        let named: Vec<(String, u32, &s3formats::sim::OutfitPart)> = o.parts.iter().filter_map(|p| name(p).map(|(n, t)| (n, t, p))).collect();
        let body = named.iter().find(|(_, t, _)| *t == PT_BODY)?;
        let kind = body.0.strip_suffix("Body")?.to_string();
        let shading = shading_of.get(&kind)?.as_ref()?;
        let part = |t: u32| named.iter().find(|(_, pt, _)| *pt == t).map(|(n, ..)| n.clone());
        // The coat: the base's colours, then each region's in turn, through its mask's channels.
        let layers: Vec<(s3formats::dds::Rgba, Vec<(usize, [f32; 3])>, f32)> = named
            .iter()
            .filter(|(_, t, _)| matches!(*t, PT_BASE | PT_REGION))
            .filter_map(|(_, _, p)| {
                let mask = preset_value(&p.preset, "Mask")?.strip_prefix("key:")?;
                let q: Vec<&str> = mask.split(':').collect();
                let mk = (u32::from_str_radix(q.first()?, 16).ok()?, u32::from_str_radix(q.get(1)?, 16).ok()?, u64::from_str_radix(q.get(2)?, 16).ok()?);
                let opacity = preset_value(&p.preset, "Opacity").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
                let colors: Vec<(usize, [f32; 3])> = (1..=4usize)
                    .filter(|i| preset_value(&p.preset, &format!("Color{i} Enabled")).is_some_and(|v| v.eq_ignore_ascii_case("true")))
                    .filter_map(|i| {
                        let v: Vec<f32> = preset_value(&p.preset, &format!("Color{i}"))?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                        (v.len() >= 3).then(|| (i - 1, [v[0], v[1], v[2]]))
                    })
                    .collect();
                Some((decode(mk)?, colors, opacity))
            })
            .collect();
        let mut img = s3formats::dds::Rgba { width: COAT_SIZE, height: COAT_SIZE, data: vec![255; COAT_SIZE * COAT_SIZE * 4] };
        for y in 0..COAT_SIZE {
            for x in 0..COAT_SIZE {
                let (u, v) = ((x as f32 + 0.5) / COAT_SIZE as f32, (y as f32 + 0.5) / COAT_SIZE as f32);
                let mut c = [0.5f32, 0.4, 0.3];
                for (mask, colors, opacity) in &layers {
                    let m = mask.sample(u, v);
                    for (ch, col) in colors {
                        let a = (m[*ch] * opacity).clamp(0.0, 1.0);
                        for k in 0..3 {
                            c[k] += (col[k] - c[k]) * a;
                        }
                    }
                }
                // (Over the fur's shading, which sits about mid-grey.)
                let s = shading.sample(u, v)[0] * 2.0;
                let i = (y * COAT_SIZE + x) * 4;
                for k in 0..3 {
                    img.data[i + k] = ((c[k] * s).clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
        }
        let coat = coat_key(k.i);
        let _ = std::fs::write(root.tex_path(coat), crate::ddsw::encode_dds(&img));
        Some(PetBreed { outfit: k.i, kind, age: o.age, gender: o.gender, coat, tail: part(PT_TAIL), ears: part(PT_EARS), mane: part(PT_MANE) })
    })
    .into_iter()
    .flatten()
    .collect();
    textures.sort();
    textures.dedup();
    let n = crate::bake::bake_textures(root, pkgs, &textures, 1024, "Converting pets", progress);
    progress(&format!("Converting: {} pet breeds", breeds.len()));
    write_value(&root.global_dir().join("pets.bin"), &PetsBaked { version: PETS_VERSION, kinds, breeds }).map_err(|e| e.to_string())?;
    Ok(n)
}
