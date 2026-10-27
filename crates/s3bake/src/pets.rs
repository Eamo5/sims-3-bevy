//! Pets (the Pets pack): cats, dogs, little dogs and horses, grown and young. Each kind's
//! skeleton and body, its tails, ears and manes, and its coat as the game builds it: the body's
//! grey fur shading under a base colour (the `Base` part's `PeltRegion` preset), with the coat's
//! regions and markings (`Regions…`/`Markings…` parts: a mask each, and the colours their
//! presets give them) laid over it.

use s3formats::sim::{CasPart, Rig};
use s3pkg::{Package, PackageSet, types};
use serde::{Deserialize, Serialize};

use crate::pack::{read_value, write_value};
use crate::types::{Key, SkinMesh};

/// Bumped when the baked pets change.
pub const PETS_VERSION: u32 = 7;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetsBaked {
    pub version: u32,
    pub kinds: Vec<PetKind>,
    /// The game's breeds (its pet outfits), each with its coat painted.
    pub breeds: Vec<PetBreed>,
    /// The pets' face and body blends (FACE, group 0x48000000), each one's bones by kind.
    #[serde(default)]
    pub blends: Vec<PetBlend>,
    /// Create a Pet's shapes (its compound blends, CBLN: `CAPFaceBody`'s presets).
    #[serde(default)]
    pub presets: Vec<PetPreset>,
}

/// One of the pets' face or body blends ("muzzleShort"), and its bones' adjustments for each kind
/// of pet it's for (at full strength).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetBlend {
    pub instance: u64,
    pub name: String,
    pub bones: Vec<(String, Vec<crate::gamedata::FaceBone>)>,
}

/// One of Create a Pet's shapes: its name, region (`FacialBlend.PresetFacialRegions`: the face,
/// ears, body, face and body, snout or eyes), the species it's for (its age/species flags), and
/// its blends at their amounts.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetPreset {
    pub instance: u64,
    pub name: String,
    pub region: u32,
    pub flags: u32,
    pub blends: Vec<(u64, f32)>,
}

/// Create a Pet's shapes' regions, as their compound blends have them.
pub const PET_HEAD: u32 = 0xC0;
pub const PET_EARS: u32 = 0x10;
pub const PET_BODY: u32 = 0xE400;
pub const PET_FULL_BODY: u32 = 0x1F_E7D7;
pub const PET_SNOUT: u32 = 0x1F_0206;
pub const PET_EYES: u32 = 0x101;

impl PetsBaked {
    /// A pet's bones' adjustments: its breed's blends, each region's shape chosen laid over them
    /// (a shape's blends at its amounts, in the breed's place), for its kind.
    pub fn shape(&self, kind: &str, breed: &PetBreed, presets: &[u64]) -> Vec<crate::gamedata::FaceBone> {
        let mut amounts: Vec<(u64, f32)> = breed.blends.clone();
        for p in presets.iter().filter_map(|i| self.presets.iter().find(|p| p.instance == *i)) {
            for (b, a) in &p.blends {
                match amounts.iter_mut().find(|x| x.0 == *b) {
                    Some(x) => x.1 = *a,
                    None => amounts.push((*b, *a)),
                }
            }
        }
        let mut out = Vec::new();
        for (b, a) in amounts {
            if let Some(bones) = self.blends.iter().find(|x| x.instance == b).and_then(|x| x.bones.iter().find(|(k, _)| k == kind)) {
                crate::gamedata::add_bones(&mut out, &bones.1, a);
            }
        }
        out
    }

    /// The shapes Create a Pet offers a kind of pet for a region (its species'), in the game's order.
    pub fn presets_for(&self, kind: &str, region: u32) -> Vec<&PetPreset> {
        let (_, species) = crate::gamedata::pet_flags(kind);
        self.presets.iter().filter(|p| p.region == region && p.flags & 0xF00 == species && !p.blends.is_empty()).collect()
    }
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
    /// Its name, as Create a Pet's breeds list has it (`CAPPetBreeds`: the mixed breed's
    /// "Create Cat"…), and its place in that list (the mixed breed first, then by name); none
    /// for an outfit not on it.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub listed: Option<u32>,
    /// The game's picture of it (its Create a Pet thumbnail, PNG, made smaller).
    #[serde(default)]
    pub thumb: Vec<u8>,
    /// Its build: its outfit's face and body blends (FACE instances) at their amounts.
    #[serde(default)]
    pub blends: Vec<(u64, f32)>,
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
    /// A horse's riding tack (`TACK`: the English saddle and the bridle), each mesh in its
    /// default colours.
    #[serde(default)]
    pub tack: Vec<PetPart>,
    /// What Create a Pet's Accessories dresses it in (a cat's or dog's collars; a horse's
    /// bridles, saddles, blankets, leg wraps and the rest), each in its default colours.
    #[serde(default)]
    pub accessories: Vec<PetAccessory>,
}

/// A pet's accessory: its part's name and instance (its picture's, in the game's CAS
/// thumbnails), its kind of part (the CAS part type: 51 a collar or bridle, 52 a saddle...), and
/// its meshes.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PetAccessory {
    pub name: String,
    pub instance: u64,
    pub part_type: u32,
    pub meshes: Vec<SkinMesh>,
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

/// CAS part types for pets (a horse's saddle and bridle too).
const PT_SADDLE: u32 = 52;
const PT_BRIDLE: u32 = 51;
/// The CAS part types Create a Pet's Accessories dresses a pet in: a collar or bridle (51), a
/// saddle (52), and a horse's blankets, leg wraps and the rest (55 to 59).
pub const ACCESSORY_TYPES: [u32; 7] = [51, 52, 55, 56, 57, 58, 59];
/// The riding tack a horse wears ridden: the game's English saddle and plain bridle.
pub const TACK: [&str; 2] = ["ahSaddleEnglishRiding", "ahBridle"];
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

/// The game's breeds list (`CAPPetBreeds`, in UI.package): each species' breeds by outfit
/// name, with their names' string keys, as listed.
pub fn breed_list(xml: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut species = String::new();
    let attr = |tag: &str, a: &str| -> Option<String> {
        let at = tag.find(&format!("{a}=\""))? + a.len() + 2;
        Some(tag[at..].split('"').next()?.to_string())
    };
    for tag in xml.split('<').skip(1) {
        if tag.starts_with("Species ") {
            species = attr(tag, "type").unwrap_or_default();
        } else if tag.starts_with("Breed ")
            && let (Some(o), Some(l)) = (attr(tag, "outfitName"), attr(tag, "localizationID"))
        {
            out.push((species.clone(), o, l));
        }
    }
    out
}

/// How big breeds' pictures are kept.
const THUMB_SIZE: usize = 160;

/// A breed's picture from the game's Create a Pet thumbnails (`0x0580A2CF`, the outfit's
/// instance), made `THUMB_SIZE` square.
fn breed_thumb(thumbs: &[Package], outfit: u64) -> Vec<u8> {
    let Some(png) = thumbs.iter().find_map(|p| p.entries.iter().find(|e| e.key.t == 0x0580_A2CF && e.key.i == outfit).and_then(|e| p.read(e).ok())) else { return Vec::new() };
    let Some(src) = crate::bake::decode_png(&png) else { return Vec::new() };
    let mut out = vec![0u8; THUMB_SIZE * THUMB_SIZE * 4];
    // (A box filter: each output pixel the mean of the pixels it covers.)
    let (sx, sy) = (src.width as f32 / THUMB_SIZE as f32, src.height as f32 / THUMB_SIZE as f32);
    for y in 0..THUMB_SIZE {
        for x in 0..THUMB_SIZE {
            let (x0, y0) = ((x as f32 * sx) as usize, (y as f32 * sy) as usize);
            let (x1, y1) = ((((x + 1) as f32 * sx) as usize).max(x0 + 1).min(src.width), (((y + 1) as f32 * sy) as usize).max(y0 + 1).min(src.height));
            let mut sum = [0u32; 4];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = (yy * src.width + xx) * 4;
                    for k in 0..4 {
                        sum[k] += src.data[i + k] as u32;
                    }
                }
            }
            let n = ((x1 - x0) * (y1 - y0)).max(1) as u32;
            let o = (y * THUMB_SIZE + x) * 4;
            for k in 0..4 {
                out[o + k] = (sum[k] / n) as u8;
            }
        }
    }
    let mut bytes = Vec::new();
    let mut enc = png::Encoder::new(&mut bytes, THUMB_SIZE as u32, THUMB_SIZE as u32);
    enc.set_color(png::ColorType::Rgba);
    if let Ok(mut w) = enc.write_header() {
        let _ = w.write_image_data(&out);
    }
    bytes
}

pub fn bake_pets(root: &crate::BakeRoot, pkgs: &PackageSet, install_root: &std::path::Path, progress: crate::bake::Progress) -> Result<usize, String> {
    progress("Converting: pets…");
    let casps: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(types::CASP).copied().collect();
    let parsed: Vec<(u64, CasPart)> = crate::bake::par_map(&casps, |k| CasPart::parse(&pkgs.read(k)?).ok().map(|c| (k.i, c))).into_iter().flatten().collect();
    let instance_of: std::collections::HashMap<String, u64> = parsed.iter().map(|(i, c)| (c.name.clone(), *i)).collect();
    let parsed: Vec<CasPart> = parsed.into_iter().map(|(_, c)| c).collect();
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
        // (The tack, its default preset drawn as its texture.)
        let tack: Vec<PetPart> = pets
            .iter()
            .filter(named)
            .filter(|c| matches!(c.clothing_type, PT_SADDLE | PT_BRIDLE) && TACK.contains(&c.name.as_str()))
            .filter_map(|c| {
                let key: Key = (0x00B2_D882, 0x5441_434B, s3pkg::fnv64(&c.name));
                if !root.tex_path(key).exists()
                    && let Some(img) = c.presets.first().and_then(|x| s3formats::complate::render_preset(pkgs, x, 512, false))
                {
                    let _ = std::fs::write(root.tex_path(key), crate::ddsw::encode_dds(&img));
                }
                let mut meshes = meshes(c);
                for m in &mut meshes {
                    m.texture = root.tex_path(key).exists().then_some(key).or(m.texture);
                }
                (!meshes.is_empty()).then(|| PetPart { name: c.name.clone(), meshes })
            })
            .collect();
        // (Its accessories likewise, with their parts' instances for their pictures.)
        let mut seen = std::collections::HashSet::new();
        let accessories: Vec<PetAccessory> = pets
            .iter()
            .filter(|c| c.name.starts_with(code))
            .filter(|c| ACCESSORY_TYPES.contains(&c.clothing_type) && seen.insert(c.name.clone()))
            .filter_map(|c| {
                let key: Key = (0x00B2_D882, 0x4143_4353, s3pkg::fnv64(&c.name));
                if !root.tex_path(key).exists()
                    && let Some(img) = c.presets.first().and_then(|x| s3formats::complate::render_preset(pkgs, x, 256, false))
                {
                    let _ = std::fs::write(root.tex_path(key), crate::ddsw::encode_dds(&img));
                }
                let mut meshes = meshes(c);
                for m in &mut meshes {
                    m.texture = root.tex_path(key).exists().then_some(key).or(m.texture);
                }
                (!meshes.is_empty()).then(|| PetAccessory { name: c.name.clone(), instance: instance_of.get(&c.name).copied().unwrap_or(0), part_type: c.clothing_type, meshes })
            })
            .collect();
        kinds.push(PetKind { code: code.to_string(), rig, body: body_meshes, shading, base, tails, ears, manes, regions, tack, accessories });
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
        // (PET_PNG=<dir>: each coat as a picture too, to look at.)
        if let Ok(dir) = std::env::var("PET_PNG") {
            let path = std::path::Path::new(&dir).join(format!("{kind}_{:016X}.png", k.i));
            if let Ok(f) = std::fs::File::create(path) {
                let mut enc = png::Encoder::new(std::io::BufWriter::new(f), COAT_SIZE as u32, COAT_SIZE as u32);
                enc.set_color(png::ColorType::Rgba);
                if let Ok(mut w) = enc.write_header() {
                    let _ = w.write_image_data(&img.data);
                }
            }
        }
        // Its build: the outfit's face and body blends, at their amounts.
        let blends = o.faces.iter().map(|(k, a)| (k.i, *a)).collect();
        Some(PetBreed { outfit: k.i, kind, age: o.age, gender: o.gender, coat, tail: part(PT_TAIL), ears: part(PT_EARS), mane: part(PT_MANE), name: String::new(), listed: None, thumb: Vec::new(), blends })
    })
    .into_iter()
    .flatten()
    .collect();
    let mut breeds = breeds;
    // Their names and order, as Create a Pet lists them, and the game's pictures of them.
    let ui = Package::open(crate::ui::ui_package(install_root)).ok();
    let list_xml = ui.as_ref().and_then(|p| p.entries.iter().find(|e| e.key.t == 0x0333_406C && e.key.i == s3pkg::fnv64("CAPPetBreeds")).and_then(|e| p.read(e).ok()));
    if let Some(xml) = list_xml {
        let strings = s3formats::stbl::load_english(pkgs);
        let list = breed_list(&String::from_utf8_lossy(&xml));
        let mut species: Vec<&str> = list.iter().map(|l| l.0.as_str()).collect();
        species.dedup();
        for sp in species {
            // (The mixed breed first, then by name: the game's `BreedComparison`.)
            let mut these: Vec<(usize, String, bool)> = list
                .iter()
                .filter(|l| l.0 == sp)
                .filter_map(|(_, o, loc)| {
                    let b = breeds.iter().position(|b| b.outfit == s3pkg::fnv64(o))?;
                    Some((b, strings.get(&s3pkg::fnv64(loc)).cloned().unwrap_or_default(), o.contains("Delta")))
                })
                .collect();
            these.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(&b.1)));
            for (n, (b, name, _)) in these.into_iter().enumerate() {
                breeds[b].name = name;
                breeds[b].listed = Some(n as u32);
            }
        }
    }
    let thumbs: Vec<Package> = s3pkg::install::product_roots(install_root).into_iter().filter_map(|r| Package::open(r.join("Thumbnails").join("CasThumbnails.package")).ok()).collect();
    let pics = crate::bake::par_map(&breeds, |b| Some(if b.listed.is_some() { breed_thumb(&thumbs, b.outfit) } else { Vec::new() }));
    for (b, p) in breeds.iter_mut().zip(pics) {
        b.thumb = p.unwrap_or_default();
    }
    progress(&format!("Converting: {} breeds named, {} pictured", breeds.iter().filter(|b| b.listed.is_some()).count(), breeds.iter().filter(|b| !b.thumb.is_empty()).count()));
    textures.sort();
    textures.dedup();
    let n = crate::bake::bake_textures(root, pkgs, &textures, 1024, "Converting pets", progress);
    progress(&format!("Converting: {} pet breeds", breeds.len()));
    // Create a Pet's shapes (the pets' compound blends), and every blend the breeds and shapes
    // use, with its bones for each kind it's for.
    progress("Converting: pet shapes…");
    let mut presets: Vec<PetPreset> = Vec::new();
    let mut cblns: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(0x051D_F2DD).filter(|k| k.g != 0).copied().collect();
    cblns.sort();
    cblns.dedup_by_key(|k| k.i);
    for k in cblns {
        let Some((name, region, flags, entries)) = read(&k).and_then(|d| crate::gamedata::parse_cbln(&d)) else { continue };
        if !matches!(flags & 0xF00, 0x200..=0x500) {
            continue;
        }
        presets.push(PetPreset { instance: k.i, name, region, flags, blends: entries.into_iter().map(|(a, i)| (i, a)).collect() });
    }
    // (In their natural order: "Preset2" before "Preset10".)
    presets.sort_by_key(|p| {
        let digits: String = p.name.chars().filter(|c| c.is_ascii_digit()).collect();
        (p.region, p.name.to_ascii_lowercase().trim_end_matches(|c: char| c.is_ascii_digit()).to_string(), digits.parse::<u32>().unwrap_or(0))
    });
    let mut wanted: Vec<u64> = breeds.iter().flat_map(|b| b.blends.iter().map(|x| x.0)).chain(presets.iter().flat_map(|p| p.blends.iter().map(|x| x.0))).collect();
    wanted.sort();
    wanted.dedup();
    const SHAPED: [&str; 8] = ["ac", "cc", "ad", "cd", "al", "cl", "ah", "ch"];
    let blends: Vec<PetBlend> = crate::bake::par_map(&wanted, |i| {
        let face = pkgs.read_ti(0x0358_B08A, *i).and_then(|d| s3formats::sim::BlendInfo::parse(&d).ok())?;
        let bones: Vec<(String, Vec<crate::gamedata::FaceBone>)> = SHAPED
            .iter()
            .map(|k| (k.to_string(), crate::gamedata::blend_bones(pkgs, &face, |f| crate::gamedata::for_pet(k, f))))
            .filter(|(_, b)| !b.is_empty())
            .collect();
        (!bones.is_empty()).then(|| PetBlend { instance: *i, name: face.name.clone(), bones })
    })
    .into_iter()
    .flatten()
    .collect();
    write_value(&root.global_dir().join("pets.bin"), &PetsBaked { version: PETS_VERSION, kinds, breeds, blends, presets }).map_err(|e| e.to_string())?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_lays_its_blends_over_the_breeds() {
        let bone = |b: u32, y: f32| crate::gamedata::FaceBone { bone: b, offset: [0.0, y, 0.0], ..Default::default() };
        let blend = |i: u64, bones: Vec<crate::gamedata::FaceBone>| PetBlend { instance: i, name: String::new(), bones: vec![("ad".into(), bones)] };
        let data = PetsBaked {
            blends: vec![blend(1, vec![bone(7, 1.0)]), blend(2, vec![bone(8, 1.0)])],
            presets: vec![PetPreset { instance: 9, name: "petSnoutDogShort".into(), region: PET_SNOUT, flags: 0x1420, blends: vec![(1, 0.25), (2, 1.0)] }],
            ..Default::default()
        };
        let breed = PetBreed { kind: "ad".into(), blends: vec![(1, 0.5)], ..Default::default() };
        let y = |bones: &[crate::gamedata::FaceBone], b: u32| bones.iter().find(|x| x.bone == b).map_or(0.0, |x| x.offset[1]);
        let plain = data.shape("ad", &breed, &[]);
        assert_eq!((y(&plain, 7), y(&plain, 8)), (0.5, 0.0));
        // (The shape's amounts in place of the breed's; its own blends besides.)
        let shaped = data.shape("ad", &breed, &[9]);
        assert_eq!((y(&shaped, 7), y(&shaped, 8)), (0.25, 1.0));
        assert!(data.shape("ac", &breed, &[9]).is_empty(), "a dog's blends aren't a cat's");
        assert_eq!(data.presets_for("ad", PET_SNOUT).len(), 1);
        assert!(data.presets_for("ac", PET_SNOUT).is_empty());
    }

    #[test]
    fn the_games_breeds_list() {
        let xml = r#"<PetBreeds><Species type="Horse"><Breed outfitName="breedahDelta" localizationID="Ui/Tooltip/CAS/Puck:CreateHorse"><Coat coatName="breedahDelta" outfit="true"/></Breed></Species><Species type="LittleDog"><Breed outfitName="breedalBeagle" localizationID="Ui/Tooltip/CAS/CAP/Breeds/LittleDog:Beagle"/></Species></PetBreeds>"#;
        let l = breed_list(xml);
        assert_eq!(l.len(), 2);
        assert_eq!(l[1], ("LittleDog".to_string(), "breedalBeagle".to_string(), "Ui/Tooltip/CAS/CAP/Breeds/LittleDog:Beagle".to_string()));
        assert_eq!(s3pkg::fnv64("CAPPetBreeds"), 0x37A7_B686_58F2_7357, "as UI.package keys it");
    }
}