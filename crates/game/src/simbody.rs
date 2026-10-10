//! Sim bodies from the baked CAS data: skeleton (rig), skinned meshes for face, hair, top,
//! bottom and shoes, and a GPU skin material that layers pre-baked clothing over tinted skin.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use rand::{Rng, SeedableRng};
use rand::seq::IndexedRandom;
use s3bake::{CasPartInfo, Key, Rig, SkinMesh};
use s3formats::sim::*;

use crate::baked::BakedData;
use crate::sim::{Age, Sim};

pub type SimSkinMaterial = ExtendedMaterial<StandardMaterial, SimSkinExt>;

/// Skin tint and up to four clothing layers blended over the skin texture.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct SimSkinExt {
    /// rgb: skin-tone tint, w: number of layers in use.
    #[uniform(100)]
    pub params: Vec4,
    #[texture(101)]
    #[sampler(102)]
    pub layer0: Handle<Image>,
    #[texture(103)]
    #[sampler(104)]
    pub layer1: Handle<Image>,
    #[texture(105)]
    #[sampler(106)]
    pub layer2: Handle<Image>,
    #[texture(107)]
    #[sampler(108)]
    pub layer3: Handle<Image>,
}

impl MaterialExtension for SimSkinExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://sims3/shaders/sim_skin.wgsl".into()
    }
}

pub struct SimBodyPlugin;

impl Plugin for SimBodyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sim_skin.wgsl");
        app.add_plugins(MaterialPlugin::<SimSkinMaterial>::default())
            .init_resource::<SimTextures>()
            .add_systems(Startup, init_blank);
    }
}

/// GPU textures shared between sims, keyed by baked texture id.
#[derive(Resource, Default)]
pub struct SimTextures {
    pub map: HashMap<Key, Handle<Image>>,
    pub blank: Handle<Image>,
}

fn init_blank(mut tex: ResMut<SimTextures>, mut images: ResMut<Assets<Image>>) {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    tex.blank = images.add(Image::new(
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));
}

/// CAS data needed to dress sims.
#[derive(Resource, Clone)]
pub struct CasData {
    pub parts: Arc<Vec<CasPartInfo>>,
    pub adult_rig: Option<Arc<Rig>>,
    pub child_rig: Option<Arc<Rig>>,
    pub toddler_rig: Option<Arc<Rig>>,
    pub baby_rig: Option<Arc<Rig>>,
    pub tone_textures: Arc<Vec<(u32, u32, Key)>>,
    pub ramp: Arc<Vec<[f32; 3]>>,
    /// The careers' uniforms by name.
    pub outfits: Arc<HashMap<String, s3bake::OutfitInfo>>,
    /// The face sliders' bone adjustments by age-and-sex prefix ("am", "tf", "cu"...).
    pub face_bones: Arc<HashMap<String, Vec<(u8, Vec<s3bake::gamedata::FaceBone>)>>>,
    /// The iris, drawn over the eyes in a Sim's eye colour.
    pub eye_overlay: Option<Key>,
    /// The clothes' colourways: each part's presets' swatch colours (none: one that's not
    /// offered).
    pub colourways: Arc<HashMap<Key, s3bake::gamedata::CasColourways>>,
}

impl CasData {
    pub fn from_baked(b: &BakedData) -> Self {
        Self {
            parts: Arc::new(b.cas.parts.clone()),
            adult_rig: b.cas.adult_rig.clone().map(Arc::new),
            child_rig: b.cas.child_rig.clone().map(Arc::new),
            toddler_rig: b.cas.toddler_rig.clone().map(Arc::new),
            baby_rig: b.cas.baby_rig.clone().map(Arc::new),
            tone_textures: Arc::new(b.cas.tone.textures.clone()),
            ramp: Arc::new(b.cas.tone.ramp.clone()),
            outfits: Arc::new(b.outfits.iter().map(|o| (o.name.clone(), o.clone())).collect()),
            face_bones: Arc::new(b.face_bones.iter().map(|f| (f.prefix.clone(), f.sliders.clone())).collect()),
            eye_overlay: b.eye_colors.overlay.filter(|k| b.texture_bytes(k).is_some()),
            colourways: Arc::new(b.colourways.iter().map(|w| (w.part, w.clone())).collect()),
        }
    }

    /// The layer of `part` in colourway `i` (its own for 0, or for one that isn't offered).
    pub fn colourway_layer(&self, part: &CasPartInfo, i: u8) -> Option<Key> {
        if i == 0 || !self.colourways.get(&part.key).is_some_and(|w| w.swatches.get(i as usize).is_some_and(|s| s.is_some())) {
            return part.layer;
        }
        Some((s3bake::gamedata::T_CAS_PRESET, i as u32, part.key.2))
    }

    fn skin_texture(&self, age: u32, gender: u32, kind: u32) -> Option<Key> {
        self.tone_textures.iter().find(|(ag, t, _)| ag & age != 0 && ag & gender != 0 && t & kind != 0).map(|x| x.2)
    }

    /// Skin tint for a position on the tone ramp (mostly darkening, little hue shift).
    fn tint(&self, t: f32) -> Vec3 {
        if self.ramp.len() < 2 {
            return Vec3::ONE;
        }
        let at = |t: f32| {
            let f = t.clamp(0.0, 1.0) * (self.ramp.len() - 1) as f32;
            let i = f.floor() as usize;
            let j = (i + 1).min(self.ramp.len() - 1);
            Vec3::from(self.ramp[i]).lerp(Vec3::from(self.ramp[j]), f - i as f32)
        };
        let base = at(0.0).max(Vec3::splat(0.01));
        let f = at(t) / base;
        let avg = (f.x + f.y + f.z) / 3.0;
        Vec3::splat(avg) + (f - Vec3::splat(avg)) * 0.3
    }
}

/// A Sim's age as the CAS parts' age flag.
pub fn age_flag(a: Age) -> u32 {
    age_bits(a)
}

fn age_bits(a: Age) -> u32 {
    match a {
        Age::Baby => AGE_BABY,
        Age::Toddler => AGE_TODDLER,
        Age::Child => AGE_CHILD,
        Age::Teen => AGE_TEEN,
        Age::YoungAdult => AGE_YOUNG_ADULT,
        Age::Adult => AGE_ADULT,
        Age::Elder => AGE_ELDER,
    }
}

/// The CAS parts a sim is wearing.
#[derive(Clone, Debug, Default)]
pub struct Outfit {
    pub face: Option<CasPartInfo>,
    pub scalp: Option<CasPartInfo>,
    pub hair: Option<CasPartInfo>,
    pub brows: Option<CasPartInfo>,
    pub body: Vec<CasPartInfo>,
    /// A hat's own layer, over the hair it's worn with.
    pub hat: Option<Key>,
    /// Glasses (drawn with their own texture), and a beard (drawn as hair is).
    pub glasses: Option<CasPartInfo>,
    pub beard: Option<CasPartInfo>,
    /// Layers over the face beyond its own and the brows' (a burglar's mask).
    pub face_layers: Vec<Key>,
    /// A werewolf in their wolf form (darker, furrier, amber-eyed).
    pub wolf: bool,
}

/// Which of a Sim's outfits they have on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum OutfitKind {
    #[default]
    Everyday,
    Formal,
    Swimwear,
    Sleepwear,
    Athletic,
    /// Their career's uniform.
    Career,
    /// Coats and boots for the cold (Seasons).
    Outerwear,
}

impl OutfitKind {
    /// The outfits a dresser offers to change into.
    pub const CHOICES: [OutfitKind; 7] = [Self::Everyday, Self::Formal, Self::Sleepwear, Self::Athletic, Self::Swimwear, Self::Outerwear, Self::Career];
    pub fn label(self) -> &'static str {
        match self {
            Self::Everyday => "Everyday",
            Self::Formal => "Formal",
            Self::Swimwear => "Swimwear",
            Self::Sleepwear => "Sleepwear",
            Self::Athletic => "Athletic",
            Self::Career => "Career",
            Self::Outerwear => "Outerwear",
        }
    }

    /// The CAS parts' category flag for clothes of this outfit.
    pub fn category(self) -> u32 {
        match self {
            Self::Everyday | Self::Career => s3formats::sim::CAT_EVERYDAY,
            Self::Swimwear => s3formats::sim::CAT_SWIM,
            Self::Sleepwear => s3formats::sim::CAT_SLEEP,
            Self::Athletic => s3formats::sim::CAT_ATHLETIC,
            Self::Formal => s3formats::sim::CAT_FORMAL,
            Self::Outerwear => s3formats::sim::CAT_OUTERWEAR,
        }
    }
}

/// The outfit a Sim chose to change into at a dresser: kept on until it's time for another.
#[derive(Component)]
pub struct ChangedInto;

/// Wearing something other than everyday clothes (for a swim, for bed).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Wearing(pub OutfitKind);

/// The town's uniforms (the Reaper's robe, firefighters', police officers', burglars', the
/// maid's, the repairman's, the mail carrier's, the pizza delivery's): worn only on duty.
pub fn is_uniform(name: &str) -> bool {
    ["Reaper", "Firefighter", "Ninja", "Police", "FrenchMaid", "MaidLowLevel", "BodyRepair", "MailCarrier", "PizzaDelivery"].iter().any(|n| name.contains(n))
}

/// A service Sim's uniform.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ServiceUniform {
    Maid,
    Repair,
    MailCarrier,
    PizzaDelivery,
}

impl ServiceUniform {
    /// Its body, and the hair that goes with it (the maid's bun under her cap).
    fn parts(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Maid => ("BodyFrenchMaid", Some("HairFrenchMaidBunLoose")),
            Self::Repair => ("BodyRepair", None),
            Self::MailCarrier => ("BodyMailCarrier", None),
            Self::PizzaDelivery => ("BodyPizzaDeliveryOutfit", None),
        }
    }
}

/// A service Sim's everyday look with their uniform on (and their own shoes).
pub fn service_outfit(cas: &CasData, sim: &Sim, uniform: ServiceUniform, rng: &mut impl Rng) -> Outfit {
    let mut o = pick_outfit_for(cas, sim, rng, OutfitKind::Everyday);
    let (age, gender) = (age_bits(sim.age), if sim.female { GENDER_FEMALE } else { GENDER_MALE });
    let find = |n: &str, t: u32| cas.parts.iter().find(|p| p.baked && p.clothing_type == t && p.age_gender & age != 0 && p.age_gender & gender != 0 && p.name.contains(n)).cloned();
    let (body, hair) = uniform.parts();
    if let Some(b) = find(body, CT_BODY) {
        o.body.retain(|p| p.clothing_type == CT_SHOES);
        o.body.insert(0, b);
    }
    if let Some(h) = hair.and_then(|h| find(h, CT_HAIR)) {
        o.hair = Some(h);
    }
    o
}

pub fn pick_outfit(cas: &CasData, sim: &Sim, rng: &mut impl Rng) -> Outfit {
    pick_outfit_for(cas, sim, rng, OutfitKind::Everyday)
}

/// What a Sim wears: everyday clothes, swimwear or sleepwear (everyday clothes when there's
/// nothing of the kind for them).
pub fn pick_outfit_for(cas: &CasData, sim: &Sim, rng: &mut impl Rng, kind: OutfitKind) -> Outfit {
    // (A uniform is worn over the everyday look: see `uniform`.)
    if kind == OutfitKind::Career {
        return pick_outfit_for(cas, sim, rng, OutfitKind::Everyday);
    }
    let swim = kind != OutfitKind::Everyday;
    let cat = kind.category();
    let age = age_bits(sim.age);
    let gender = if sim.female { GENDER_FEMALE } else { GENDER_MALE };
    let fits = |e: &&CasPartInfo| e.baked && e.age_gender & age != 0 && e.age_gender & gender != 0;
    // Clothes and shoes by the outfit's category (heads and hair go with anything); never the
    // townsfolk's uniforms, nor everyday clothes that are swimwear too (a seashell top).
    let worn = |e: &&CasPartInfo| {
        let clothes = matches!(e.clothing_type, CT_TOP | CT_BOTTOM | CT_BODY | CT_SHOES);
        !is_uniform(&e.name)
            && (!clothes || e.category & cat != 0)
            && !(clothes && kind == OutfitKind::Everyday && e.category & s3formats::sim::CAT_SWIM != 0)
    };
    let of_type = |t: u32| {
        let mut all: Vec<&CasPartInfo> = cas.parts.iter().filter(|e| e.clothing_type == t).filter(fits).filter(worn).collect();
        // (A coat over everyday trousers, when there are no snow pants for them.)
        if kind == OutfitKind::Outerwear && all.is_empty() && matches!(t, CT_TOP | CT_BOTTOM | CT_SHOES) {
            all = cas.parts.iter().filter(|e| e.clothing_type == t).filter(fits).filter(|e| !is_uniform(&e.name) && e.category & s3formats::sim::CAT_EVERYDAY != 0).collect();
        }
        if !swim {
            return all;
        }
        // Swimwear and sleepwear: the base game's (the packs' swimwear counts T-shirts and
        // flippers); swimming, a bare chest for men; bare feet for everyone.
        let preferred: Vec<&CasPartInfo> = all
            .iter()
            .copied()
            .filter(|e| match t {
                CT_TOP if !sim.female && kind == OutfitKind::Swimwear => e.name.contains("TopNude"),
                CT_SHOES if matches!(kind, OutfitKind::Swimwear | OutfitKind::Sleepwear) => e.name.contains("ShoesNude"),
                _ => e.key.1 == 0,
            })
            .collect();
        if preferred.is_empty() { all } else { preferred }
    };
    // (The baked copy: a part baked with the wardrobe is listed a second time, after its
    // unbaked entry.)
    let chosen = |k: Option<Key>, t: u32| k.and_then(|k| cas.parts.iter().filter(|p| p.key == k && p.clothing_type == t).find(|p| fits(p)).cloned());
    // A baby is a single body with its own head.
    if sim.age == Age::Baby {
        let body = of_type(CT_BODY).into_iter().next().cloned();
        return Outfit { body: body.into_iter().collect(), ..default() };
    }
    let face = of_type(CT_FACE).into_iter().min_by_key(|e| e.name.len()).cloned();
    let scalp = of_type(CT_SCALP).into_iter().min_by_key(|e| e.name.len()).cloned();
    // Natural eyebrows (not the novelty ones).
    let natural = |e: &&CasPartInfo| !["Hairless", "Monobrow", "Extreme"].iter().any(|n| e.name.contains(n));
    // (Drawn as before, to keep the clothes picked after them the same.)
    let _ = of_type(CT_HAIR).choose(rng);
    let _ = of_type(CT_EYEBROW).into_iter().filter(natural).collect::<Vec<_>>().choose(rng);
    // Hair and brows are theirs whatever they wear: drawn from everything that fits, as for
    // their everyday look (swimwear and pyjamas narrow the clothes to the base game's, which
    // gave them other hair).
    let everyday = |t: u32| cas.parts.iter().filter(|e| e.clothing_type == t).filter(fits).filter(worn).collect::<Vec<_>>();
    let mut own = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look);
    let random_hair = everyday(CT_HAIR).choose(&mut own).map(|e| (*e).clone());
    let hair = chosen(sim.outfit.hair, CT_HAIR).or(random_hair);
    let brows = everyday(CT_EYEBROW).into_iter().filter(natural).collect::<Vec<_>>().choose(&mut own).map(|e| (*e).clone());
    // Facial hair, glasses and make-up, as chosen (or now and then, by their look).
    let mut extra = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look ^ 0x5EED_FACE);
    let grown = matches!(sim.age, Age::YoungAdult | Age::Adult | Age::Elder);
    let mut face_extra = |choice: Option<Key>, t: u32, chance: f64| -> Option<CasPartInfo> {
        let roll = extra.random_bool(chance.clamp(0.0, 1.0));
        let pick = extra.random::<u32>() as usize;
        match choice {
            Some(k) if k == crate::sim::OutfitChoice::NONE => None,
            Some(k) => chosen(Some(k), t),
            None if roll => {
                let list = everyday(t);
                (!list.is_empty()).then(|| list[pick % list.len()].clone())
            }
            None => None,
        }
    };
    let beard = face_extra(sim.outfit.beard, CT_BEARD, if !sim.female && grown { 0.15 } else { 0.0 });
    let glasses = face_extra(sim.outfit.glasses, CT_GLASSES, match sim.age {
        Age::Elder => 0.3,
        Age::Adult => 0.08,
        Age::YoungAdult | Age::Teen | Age::Child => 0.04,
        _ => 0.0,
    });
    let lipstick = face_extra(sim.outfit.lipstick, CT_LIPSTICK, if sim.female && (grown || sim.age == Age::Teen) { 0.3 } else { 0.0 });
    let eyeshadow = face_extra(sim.outfit.eyeshadow, CT_EYESHADOW, if sim.female && (grown || sim.age == Age::Teen) { 0.2 } else { 0.0 });
    let mut body = Vec::new();
    let (tops, bottoms, fulls) = (of_type(CT_TOP), of_type(CT_BOTTOM), of_type(CT_BODY));
    // (Nothing of the kind for them: their everyday clothes.)
    if swim && fulls.is_empty() && (tops.is_empty() || bottoms.is_empty()) {
        return pick_outfit_for(cas, sim, rng, OutfitKind::Everyday);
    }
    let use_full = rng.random_bool(0.25);
    let random_full = fulls.choose(rng).map(|f| (*f).clone());
    let random_top = tops.choose(rng).map(|f| (*f).clone());
    let random_bottom = bottoms.choose(rng).map(|f| (*f).clone());
    let random_shoes = of_type(CT_SHOES).choose(rng).map(|f| (*f).clone());
    // (The clothes chosen for the outfit in Create a Sim or at a dresser.)
    let picked = sim.outfit.clothes(kind);
    let (ct, cb, cf) = (chosen(picked.top, CT_TOP), chosen(picked.bottom, CT_BOTTOM), chosen(picked.full, CT_BODY));
    // (Swimming, men wear trunks and a bare chest, women a swimsuit or a two-piece; pyjamas
    // are mostly all-in-ones.)
    let use_full = match kind {
        OutfitKind::Everyday => use_full,
        OutfitKind::Swimwear => sim.female && rng.random_bool(0.5) && !fulls.is_empty(),
        OutfitKind::Sleepwear => !fulls.is_empty() && rng.random_bool(0.7),
        OutfitKind::Athletic => !fulls.is_empty() && rng.random_bool(0.3),
        // (Suits and dresses as often as separates.)
        OutfitKind::Formal => !fulls.is_empty() && rng.random_bool(0.5),
        // (Snowsuits now and then; mostly a coat.)
        OutfitKind::Outerwear => !fulls.is_empty() && rng.random_bool(0.25),
        OutfitKind::Career => use_full,
    };
    if let Some(f) = cf {
        body.push(f);
    } else if ct.is_some() || cb.is_some() {
        body.extend(cb.or(random_bottom));
        body.extend(ct.or(random_top));
    } else if (random_full.is_some() && use_full) || random_top.is_none() || random_bottom.is_none() {
        body.extend(random_full);
    } else {
        body.extend(random_bottom);
        body.extend(random_top);
    }
    body.extend(chosen(picked.shoes, CT_SHOES).or(random_shoes));
    // (Each in the colourway chosen for it, or the style made for it in Create a Style.)
    for p in body.iter_mut() {
        if let Some(&(_, i)) = sim.outfit.designs.iter().find(|(k, _)| *k == p.key) {
            p.layer = cas.colourway_layer(p, i);
        }
        if let Some(s) = sim.outfit.styles.iter().find(|s| s.part == p.key).filter(|s| s.ready()) {
            p.layer = Some(s.texture());
        }
    }
    // (Glasses come off for a swim.)
    let glasses = glasses.filter(|_| kind != OutfitKind::Swimwear);
    let face_layers = [lipstick, eyeshadow].into_iter().flatten().filter_map(|p| p.layer).collect();
    Outfit { face, scalp, hair, brows, body, glasses, beard, face_layers, ..default() }
}

/// A Sim in their work uniform (the outfit so named): its clothes, and any hat or mask, with
/// their own face and hair (under the hat).
pub fn uniform(cas: &CasData, sim: &Sim, name: &str, rng: &mut impl Rng) -> Option<Outfit> {
    let info = cas.outfits.get(name)?;
    let mut o = pick_outfit_for(cas, sim, rng, OutfitKind::Everyday);
    let worn = |p: &&s3bake::OutfitPartInfo| match p.part.clothing_type {
        CT_TOP | CT_BOTTOM | CT_BODY | CT_SHOES => !p.layer_only,
        CT_GLOVES | CT_STOCKINGS => p.layer_only,
        _ => false,
    };
    let mut body: Vec<&s3bake::OutfitPartInfo> = info.parts.iter().filter(worn).collect();
    // (Clothes made for another age aren't worn: their everyday clothes instead.)
    if body.is_empty() || body.iter().any(|p| p.part.age_gender & age_bits(sim.age) == 0) {
        return None;
    }
    // Layered from the skin out: stockings, then the clothes and shoes (gloves last, dropped
    // when there are too many layers).
    body.sort_by_key(|p| match p.part.clothing_type {
        CT_STOCKINGS => 0,
        CT_BOTTOM | CT_BODY => 1,
        CT_TOP => 2,
        CT_SHOES => 3,
        _ => 4,
    });
    o.body = body.into_iter().map(|p| p.part.clone()).collect();
    o.face_layers = info.parts.iter().filter(|p| p.layer_only && p.part.clothing_type == CT_MASK).filter_map(|p| p.part.layer).collect();
    if let Some(h) = info.parts.iter().find(|p| p.part.clothing_type == CT_HAIR && !p.layer_only) {
        o.hair = Some(h.part.clone());
        o.hat = h.hat;
    }
    Some(o)
}

/// How a sim mesh is shaded.
pub enum SimMat {
    /// Tinted skin texture with clothing layers on top.
    Skin { base: Option<Key>, tint: Vec3, layers: Vec<Key> },
    /// A plain texture (hair, eyes, lashes), alpha-tested when `mask`, optionally tinted
    /// (hair: the texture is greyscale, coloured by the Sim's hair colour).
    Plain { tex: Option<Key>, mask: bool, tint: Option<Color> },
    /// Hair under a hat: the (greyscale) hair in the Sim's colour, the hat's layer over it.
    Hat { tex: Key, tint: Color, hat: Key },
}

/// A sim body decoded on the loading thread.
pub struct SimModelCpu {
    pub rig: Arc<Rig>,
    pub parts: Vec<(Mesh, SimMat)>,
    pub textures: Vec<(Key, Image)>,
    /// The face's shape: adjustments to its bones (by name hash): offset, scale and rotation
    /// added.
    pub face: HashMap<u32, BoneShape>,
}

/// What's added to a bone's offset, scale and rotation (a quaternion's x, y, z, w).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoneShape {
    pub offset: Vec3,
    pub scale: Vec3,
    pub rotation: Vec4,
}

impl BoneShape {
    pub fn apply_rotation(&self, q: Quat) -> Quat {
        if self.rotation == Vec4::ZERO {
            return q;
        }
        Quat::from_xyzw(q.x + self.rotation.x, q.y + self.rotation.y, q.z + self.rotation.z, q.w + self.rotation.w).normalize()
    }

    pub fn apply(&self, t: Transform) -> Transform {
        Transform { translation: t.translation + self.offset, rotation: self.apply_rotation(t.rotation), scale: t.scale + self.scale }
    }
}

/// Where a Sim is on each of the game's face sliders (-1 to 1, `FACE_SLIDERS`' pairs): as
/// sculpted or inherited, or else as their look has it (anywhere along each slider's range, as
/// Create a Sim's randomising does).
pub fn face_sliders(sim: &Sim) -> Vec<f32> {
    let n = s3bake::gamedata::FACE_SLIDERS.len();
    if sim.face.len() == n {
        return sim.face.clone();
    }
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look ^ 0xFACE_B0E5);
    (0..n).map(|_| rng.random_range(-1.0f32..1.0)).collect()
}

/// A baby's face, from their parents': on each slider somewhere between theirs, give or take a
/// little (as the game's genetics blend them).
pub fn inherited_face(mother: &Sim, father: Option<&Sim>, rng: &mut impl Rng) -> Vec<f32> {
    let m = face_sliders(mother);
    let f = father.map_or_else(|| m.clone(), face_sliders);
    m.iter().zip(&f).map(|(a, b)| (a + (b - a) * rng.random_range(0.0f32..1.0) + rng.random_range(-0.15f32..0.15)).clamp(-1.0, 1.0)).collect()
}

/// A Sim's face shape, by their look: on each of the game's face sliders (in opposing pairs,
/// as Create a Sim's are) they lean one way or the other, some a lot, most a little.
pub fn face_shape(cas: &CasData, sim: &Sim) -> HashMap<u32, BoneShape> {
    let mut out: HashMap<u32, BoneShape> = HashMap::new();
    let prefix = match (sim.age, sim.female) {
        (Age::Baby, _) => return out,
        (Age::Toddler, _) => "pu",
        (Age::Child, _) => "cu",
        (Age::Teen, f) => if f { "tf" } else { "tm" },
        (Age::YoungAdult, f) => if f { "yf" } else { "ym" },
        (Age::Adult, f) => if f { "af" } else { "am" },
        (Age::Elder, f) => if f { "ef" } else { "em" },
    };
    // (NO_FACE_SHAPE=1, for comparisons: every face as the game models it.)
    let Some(sliders) = cas.face_bones.get(prefix).filter(|_| std::env::var("NO_FACE_SHAPE").is_err()) else { return out };
    for (pair, w) in face_sliders(sim).into_iter().enumerate() {
        let slider = if w >= 0.0 { pair * 2 } else { pair * 2 + 1 };
        let Some((_, bones)) = sliders.iter().find(|(i, _)| *i as usize == slider) else { continue };
        for b in bones {
            let s = out.entry(b.bone).or_default();
            s.offset += Vec3::from(b.offset) * w.abs();
            s.scale += Vec3::from(b.scale) * w.abs();
            s.rotation += Vec4::from(b.rotation) * w.abs();
        }
    }
    out
}

/// The mesh with the Sim's body shape applied (the part's heavy, fit and thin morphs).
fn shaped(mut m: SkinMesh, sim: &Sim) -> SkinMesh {
    let w = [sim.weight.max(0.0), sim.fitness.clamp(0.0, 1.0), (-sim.weight).max(0.0)];
    for (morph, amount) in m.morphs.iter().zip(w) {
        if amount <= 0.0 || morph.len() != m.positions.len() {
            continue;
        }
        for (p, d) in m.positions.iter_mut().zip(morph) {
            for k in 0..3 {
                p[k] += d[k] * amount;
            }
        }
    }
    m
}

pub(crate) fn skin_mesh(m: SkinMesh) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, m.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(m.joints));
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, m.weights);
    mesh.insert_indices(Indices::U32(m.indices));
    mesh
}

const SHADER_SIM_EYES: u32 = 0xCF8A70B4;
const SHADER_SIM_EYELASHES: u32 = 0x9D9DA161;

/// Assembles a sim's meshes and materials from the cache (any thread).
pub fn build_sim_model(baked: &BakedData, cas: &CasData, sim: &Sim, outfit: &Outfit, tone_t: f32) -> Option<SimModelCpu> {
    let rig = match sim.age {
        Age::Baby => cas.baby_rig.clone(),
        Age::Toddler => cas.toddler_rig.clone(),
        Age::Child => cas.child_rig.clone(),
        _ => cas.adult_rig.clone(),
    }?;
    let age = age_bits(sim.age);
    let gender = if sim.female { GENDER_FEMALE } else { GENDER_MALE };
    // (Supernaturals: a vampire's pallor, a zombie's grey-green, a werewolf's fur; their eyes.)
    let (occult_tint, occult_eyes) = crate::supernatural::occult_look(sim.occult, outfit.wolf);
    let tint = cas.tint(tone_t) * occult_tint;
    let eye_colour = occult_eyes.unwrap_or(sim.eyes);
    let mut parts = Vec::new();
    let mut tex_keys: Vec<Key> = Vec::new();
    // Face layers made at load in a colour: (source, copy, colour).
    let mut beard_tints: Vec<(Key, Key, Color)> = Vec::new();
    // The eyes' texture with the iris in the Sim's eye colour: (source, copy).
    let mut eye_tints: Vec<(Key, Key)> = Vec::new();

    // Body: shared skin texture + one clothing layer per worn part.
    let body_base = cas.skin_texture(age, gender, 8);
    let layers: Vec<Key> = outfit.body.iter().filter_map(|p| p.layer).take(4).collect();
    tex_keys.extend(body_base);
    tex_keys.extend(&layers);
    for p in &outfit.body {
        for m in baked.cas_meshes(&p.key).map(|m| m.meshes).unwrap_or_default() {
            parts.push((skin_mesh(shaped(m, sim)), SimMat::Skin { base: body_base, tint, layers: layers.clone() }));
        }
    }
    if let Some(face) = &outfit.face {
        let face_base = cas.skin_texture(age, gender, 4);
        // A beard drawn on the face (no mesh of its own) is a layer in the Sim's hair colour.
        let painted_beard = outfit.beard.as_ref().filter(|b| baked.cas_meshes(&b.key).is_none_or(|m| m.meshes.is_empty())).and_then(|b| b.layer).map(|src| {
            let tinted = beard_tint_key(src, sim.hair);
            beard_tints.push((src, tinted, sim.hair));
            tinted
        });
        let face_layers: Vec<Key> =
            face.layer.into_iter().chain(outfit.brows.as_ref().and_then(|b| b.layer)).chain(painted_beard).chain(outfit.face_layers.iter().copied()).take(4).collect();
        tex_keys.extend(face_base);
        tex_keys.extend(&face_layers);
        for m in baked.cas_meshes(&face.key).map(|m| m.meshes).unwrap_or_default() {
            let mat = match m.shader {
                SHADER_SIM_EYES => {
                    let tex = match (m.texture, cas.eye_overlay) {
                        (Some(src), Some(_)) => {
                            let coloured = eye_tint_key(src, eye_colour);
                            eye_tints.push((src, coloured));
                            Some(coloured)
                        }
                        _ => m.texture,
                    };
                    SimMat::Plain { tex, mask: false, tint: None }
                }
                SHADER_SIM_EYELASHES => SimMat::Plain { tex: m.texture, mask: true, tint: None },
                _ => SimMat::Skin { base: face_base, tint, layers: face_layers.clone() },
            };
            if let SimMat::Plain { tex: Some(t), .. } = &mat
                && !eye_tints.iter().any(|(_, c)| c == t)
            {
                tex_keys.push(*t);
            }
            parts.push((skin_mesh(m), mat));
        }
    }
    // Glasses, in their own texture (the lenses see-through).
    if let Some(g) = &outfit.glasses {
        for m in baked.cas_meshes(&g.key).map(|m| m.meshes).unwrap_or_default() {
            let tex = m.texture.or(g.layer);
            tex_keys.extend(tex);
            parts.push((skin_mesh(m), SimMat::Plain { tex, mask: true, tint: None }));
        }
    }
    // Hair is drawn from a greyscale copy of its texture tinted with the Sim's hair colour.
    let mut hair_keys: Vec<(Key, Key)> = Vec::new();
    for (i, p) in [&outfit.scalp, &outfit.hair, &outfit.beard].into_iter().enumerate() {
        let Some(p) = p else { continue };
        // (A beard with no mesh is painted on the face instead.)
        for m in baked.cas_meshes(&p.key).map(|m| m.meshes).unwrap_or_default() {
            let Some(src) = p.layer.or(m.texture) else { continue };
            // (A beard is never under the hat.)
            if i == 2 {
                let grey = hair_grey_key(src);
                hair_keys.push((src, grey));
                parts.push((skin_mesh(m), SimMat::Plain { tex: Some(grey), mask: true, tint: Some(sim.hair) }));
            } else if i == 1 {
                let grey = hair_grey_key(src);
                hair_keys.push((src, grey));
                let mat = match outfit.hat {
                    Some(hat) => {
                        tex_keys.push(hat);
                        SimMat::Hat { tex: grey, tint: sim.hair, hat }
                    }
                    None => SimMat::Plain { tex: Some(grey), mask: true, tint: Some(sim.hair) },
                };
                parts.push((skin_mesh(m), mat));
            } else {
                tex_keys.push(src);
                parts.push((skin_mesh(m), SimMat::Plain { tex: Some(src), mask: true, tint: None }));
            }
        }
    }
    tex_keys.sort();
    tex_keys.dedup();
    hair_keys.sort();
    hair_keys.dedup();
    let mut textures: Vec<(Key, Image)> = tex_keys
        .into_iter()
        .filter_map(|k| Some((k, crate::objects::dds_image(&baked.texture_bytes(&k)?, true)?)))
        .collect();
    for (src, grey) in hair_keys {
        if let Some(img) = baked.texture_bytes(&src).and_then(|b| greyscale_hair(&b)) {
            textures.push((grey, img));
        }
    }
    for (src, tinted, colour) in beard_tints {
        if let Some(img) = baked.texture_bytes(&src).and_then(|b| tinted_layer(&b, colour)) {
            textures.push((tinted, img));
        }
    }
    eye_tints.dedup();
    let iris = cas.eye_overlay.and_then(|k| baked.texture_bytes(&k));
    for (src, coloured) in eye_tints {
        if let Some(img) = iris.as_deref().zip(baked.texture_bytes(&src)).and_then(|(iris, eyes)| coloured_eyes(&eyes, iris, eye_colour)) {
            textures.push((coloured, img));
        }
    }
    Some(SimModelCpu { rig, parts, textures, face: face_shape(cas, sim) })
}

/// Texture-store key of the eyes' texture with the iris in a colour.
fn eye_tint_key(k: Key, c: Color) -> Key {
    let c = c.to_srgba();
    let rgb = ((c.red * 255.0) as u32) << 16 | ((c.green * 255.0) as u32) << 8 | (c.blue * 255.0) as u32;
    (k.0 ^ 0x4000_0000 ^ rgb, k.1, k.2)
}

/// The eyes' texture (DDS bytes) with the iris in `colour`, as the game's eye colour overlay
/// draws it: the iris image at twice its shade times the colour, over the eyes by its alpha.
fn coloured_eyes(eyes: &[u8], iris: &[u8], colour: Color) -> Option<Image> {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut img = s3formats::dds::decode(eyes, 512)?;
    let ov = s3formats::dds::decode(iris, 512)?;
    let c = colour.to_srgba();
    let c = [c.red, c.green, c.blue];
    let (w, h) = (img.width as usize, img.height as usize);
    let (ow, oh) = (ov.width as usize, ov.height as usize);
    if ow == 0 || oh == 0 {
        return None;
    }
    for y in 0..h {
        let oy = y * oh / h;
        for x in 0..w {
            let o = &ov.data[(oy * ow + x * ow / w) * 4..][..4];
            let a = o[3] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let p = &mut img.data[(y * w + x) * 4..][..4];
            for k in 0..3 {
                let iris = (2.0 * c[k] * o[k] as f32 / 255.0).min(1.0);
                p[k] = ((p[k] as f32 / 255.0 * (1.0 - a) + iris * a) * 255.0).round() as u8;
            }
        }
    }
    let (mips, levels) = s3formats::dds::build_mips(&img);
    let mut out = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        mips,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    out.texture_descriptor.mip_level_count = levels;
    out.sampler = crate::objects::sampler();
    Some(out)
}

/// Texture-store key of a face layer's copy in a colour.
fn beard_tint_key(k: Key, c: Color) -> Key {
    let c = c.to_srgba();
    let rgb = ((c.red * 255.0) as u32) << 16 | ((c.green * 255.0) as u32) << 8 | (c.blue * 255.0) as u32;
    (k.0 ^ 0x2000_0000 ^ rgb, k.1, k.2)
}

/// A face layer (DDS bytes) recoloured: its light and shade kept, in `colour` (a beard in the
/// Sim's hair colour), its alpha as it was.
fn tinted_layer(dds: &[u8], colour: Color) -> Option<Image> {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut img = s3formats::dds::decode(dds, 512)?;
    let (mut sum, mut n) = (0.0f64, 0u64);
    for px in img.data.chunks_exact(4) {
        if px[3] > 60 {
            sum += (0.3 * px[0] as f64 + 0.59 * px[1] as f64 + 0.11 * px[2] as f64) / 255.0;
            n += 1;
        }
    }
    let mean = if n > 0 { (sum / n as f64).max(0.05) } else { 0.5 };
    let c = colour.to_srgba();
    for px in img.data.chunks_exact_mut(4) {
        let l = (0.3 * px[0] as f64 + 0.59 * px[1] as f64 + 0.11 * px[2] as f64) / 255.0;
        let shade = (l / mean).min(1.6) as f32;
        px[0] = (c.red * shade * 255.0).min(255.0) as u8;
        px[1] = (c.green * shade * 255.0).min(255.0) as u8;
        px[2] = (c.blue * shade * 255.0).min(255.0) as u8;
    }
    let (mips, levels) = s3formats::dds::build_mips(&img);
    let mut out = Image::new(
        Extent3d { width: img.width as u32, height: img.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        mips,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    out.texture_descriptor.mip_level_count = levels;
    out.sampler = crate::objects::sampler();
    Some(out)
}

/// Texture-store key of a hair texture's greyscale copy.
fn hair_grey_key(k: Key) -> Key {
    (k.0 ^ 0x4000_0000, k.1, k.2)
}

/// Scales the alpha of each smaller mip level so as many texels pass the alpha test (`cut`) as
/// at full size: averaged-down strands otherwise fall under it and hair thins out to nothing
/// when seen small.
fn keep_coverage(data: &mut [u8], width: usize, height: usize, cut: u8) {
    let coverage = |px: &[u8], scale: f32| px.chunks_exact(4).filter(|p| p[3] as f32 * scale >= cut as f32).count() as f32 / (px.len() / 4).max(1) as f32;
    let (mut w, mut h, mut at) = (width, height, 0usize);
    let target = coverage(&data[..w * h * 4], 1.0);
    while w > 1 || h > 1 {
        at += w * h * 4;
        (w, h) = ((w / 2).max(1), (h / 2).max(1));
        let level = &mut data[at..at + w * h * 4];
        if coverage(level, 1.0) >= target {
            continue;
        }
        // The smallest scale that brings the level back up to the full-size coverage.
        let (mut lo, mut hi) = (1.0f32, 16.0f32);
        for _ in 0..12 {
            let mid = (lo + hi) * 0.5;
            if coverage(level, mid) < target { lo = mid } else { hi = mid }
        }
        for p in level.chunks_exact_mut(4) {
            p[3] = (p[3] as f32 * hi).min(255.0) as u8;
        }
    }
}

/// A hair texture (DDS bytes) as a greyscale RGBA image, brightened so its average is a light
/// grey: the material's tint colour then sets the hair colour.
fn greyscale_hair(dds: &[u8]) -> Option<Image> {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut img = s3formats::dds::decode(dds, 512)?;
    let (mut sum, mut n) = (0.0f64, 0u64);
    for px in img.data.chunks_exact(4) {
        if px[3] > 100 {
            sum += (0.3 * px[0] as f64 + 0.59 * px[1] as f64 + 0.11 * px[2] as f64) / 255.0;
            n += 1;
        }
    }
    let mean = if n > 0 { (sum / n as f64).max(0.05) } else { 0.5 };
    let gain = (0.75 / mean).min(4.0);
    for px in img.data.chunks_exact_mut(4) {
        let l = (0.3 * px[0] as f64 + 0.59 * px[1] as f64 + 0.11 * px[2] as f64) / 255.0;
        let v = ((l * gain).min(1.0) * 255.0) as u8;
        px[0] = v;
        px[1] = v;
        px[2] = v;
    }
    let (mut mips, levels) = s3formats::dds::build_mips(&img);
    keep_coverage(&mut mips, img.width, img.height, 102);
    let mut out = Image::new(
        Extent3d { width: img.width as u32, height: img.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        mips,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    out.texture_descriptor.mip_level_count = levels;
    out.sampler = crate::objects::sampler();
    Some(out)
}

/// Sims are drawn in the world (and in the portrait studio while their picture's taken: see
/// `portraits`).
pub fn sim_layers() -> bevy::camera::visibility::RenderLayers {
    bevy::camera::visibility::RenderLayers::layer(0)
}

/// A top-level piece of a Sim's body (root joint or mesh), replaced when the body is rebuilt.
#[derive(Component)]
pub struct SimModelPart;

/// The skeleton of a spawned sim: one entity per rig bone.
#[derive(Component)]
pub struct Skeleton {
    pub rig: Arc<Rig>,
    pub joints: Vec<Entity>,
    /// Each bone's rest pose (with the face's shape in it).
    pub bind: Vec<Transform>,
    /// The face's shape on each bone, put back over animations that move it.
    pub shape: Vec<Option<BoneShape>>,
}

pub struct SimRenderCtx<'a> {
    pub meshes: &'a mut Assets<Mesh>,
    pub images: &'a mut Assets<Image>,
    pub mats: &'a mut Assets<StandardMaterial>,
    pub skin_mats: &'a mut Assets<SimSkinMaterial>,
    pub bindposes: &'a mut Assets<SkinnedMeshInverseBindposes>,
    pub textures: &'a mut SimTextures,
}

/// Spawns skeleton + skinned meshes under `parent`.
pub fn spawn_sim_model(commands: &mut Commands, parent: Entity, model: SimModelCpu, ctx: &mut SimRenderCtx) -> Entity {
    for (k, img) in model.textures {
        if !ctx.textures.map.contains_key(&k) {
            let h = ctx.images.add(img);
            ctx.textures.map.insert(k, h);
        }
    }
    let rig = model.rig.clone();
    let mut joints = Vec::with_capacity(rig.bones.len());
    let mut bind = Vec::with_capacity(rig.bones.len());
    let mut shape = Vec::with_capacity(rig.bones.len());
    let mut world: Vec<Mat4> = Vec::with_capacity(rig.bones.len());
    for b in &rig.bones {
        let local = Transform {
            translation: Vec3::from(b.position),
            rotation: Quat::from_xyzw(b.rotation[0], b.rotation[1], b.rotation[2], b.rotation[3]).normalize(),
            scale: Vec3::from(b.scale),
        };
        let w = if b.parent >= 0 && (b.parent as usize) < world.len() {
            world[b.parent as usize] * local.to_matrix()
        } else {
            local.to_matrix()
        };
        world.push(w);
        // (The skin is bound to the rig as it is; the face's shape moves its bones from there.)
        let s = model.face.get(&s3pkg::fnv32(&b.name)).copied();
        let posed = s.map_or(local, |s| s.apply(local));
        shape.push(s);
        bind.push(posed);
        joints.push(commands.spawn((posed, Visibility::default())).id());
    }
    for (i, b) in rig.bones.iter().enumerate() {
        let p = if b.parent >= 0 && (b.parent as usize) < joints.len() { joints[b.parent as usize] } else { parent };
        if p == parent {
            commands.entity(joints[i]).insert(SimModelPart);
        }
        commands.entity(p).add_child(joints[i]);
    }
    let inverse: Vec<Mat4> = world.iter().map(|m| m.inverse()).collect();
    let ibp = ctx.bindposes.add(SkinnedMeshInverseBindposes::from(inverse));
    for (mesh, mat) in model.parts {
        let skinned = SkinnedMesh { inverse_bindposes: ibp.clone(), joints: joints.clone() };
        let mesh = Mesh3d(ctx.meshes.add(mesh));
        let tex = |k: Option<Key>, t: &SimTextures| k.and_then(|k| t.map.get(&k).cloned());
        let e = match mat {
            SimMat::Skin { base, tint, layers } => {
                let layer = |i: usize| tex(layers.get(i).copied(), ctx.textures).unwrap_or(ctx.textures.blank.clone());
                let m = ctx.skin_mats.add(SimSkinMaterial {
                    base: StandardMaterial {
                        base_color_texture: tex(base, ctx.textures),
                        perceptual_roughness: 0.65,
                        reflectance: 0.25,
                        ..default()
                    },
                    extension: SimSkinExt {
                        params: tint.extend(layers.len().min(4) as f32),
                        layer0: layer(0),
                        layer1: layer(1),
                        layer2: layer(2),
                        layer3: layer(3),
                    },
                });
                commands.spawn((mesh, MeshMaterial3d(m), skinned, Transform::default())).id()
            }
            SimMat::Hat { tex: t, tint, hat } => {
                let l = tint.to_linear();
                let material = |alpha_mode: AlphaMode| SimSkinMaterial {
                    base: StandardMaterial {
                        base_color_texture: tex(Some(t), ctx.textures),
                        perceptual_roughness: 0.6,
                        reflectance: 0.25,
                        alpha_mode,
                        double_sided: true,
                        cull_mode: None,
                        ..default()
                    },
                    extension: SimSkinExt {
                        // (Ten more than its one layer: the hair keeps its alpha.)
                        params: Vec4::new(l.red, l.green, l.blue, 11.0),
                        layer0: tex(Some(hat), ctx.textures).unwrap_or(ctx.textures.blank.clone()),
                        layer1: ctx.textures.blank.clone(),
                        layer2: ctx.textures.blank.clone(),
                        layer3: ctx.textures.blank.clone(),
                    },
                };
                // (Solid core alpha-tested, soft edges blended over it, as for hair.)
                let soft = ctx.skin_mats.add(material(AlphaMode::Blend));
                let e = commands
                    .spawn((mesh.clone(), MeshMaterial3d(soft), skinned.clone(), Transform::default(), SimModelPart, sim_layers(), bevy::camera::visibility::NoFrustumCulling))
                    .id();
                commands.entity(parent).add_child(e);
                let m = ctx.skin_mats.add(material(AlphaMode::Mask(0.5)));
                commands.spawn((mesh, MeshMaterial3d(m), skinned, Transform::default())).id()
            }
            SimMat::Plain { tex: t, mask, tint } => {
                let material = |alpha_mode: AlphaMode| StandardMaterial {
                    base_color: tint.unwrap_or(Color::WHITE),
                    base_color_texture: tex(t, ctx.textures),
                    perceptual_roughness: 0.6,
                    reflectance: 0.25,
                    alpha_mode,
                    double_sided: mask,
                    cull_mode: if mask { None } else { Some(bevy::render::render_resource::Face::Back) },
                    ..default()
                };
                let m = ctx.mats.add(material(if mask { AlphaMode::Mask(0.5) } else { AlphaMode::Opaque }));
                if mask {
                    // Hair and see-through clothes: the solid core is alpha-tested (in depth
                    // order), then the same mesh is blended over it for the soft edges, which
                    // short hairstyles are mostly made of.
                    let soft = ctx.mats.add(material(AlphaMode::Blend));
                    let e = commands
                        .spawn((mesh.clone(), MeshMaterial3d(soft), skinned.clone(), Transform::default(), SimModelPart, sim_layers(), bevy::camera::visibility::NoFrustumCulling))
                        .id();
                    commands.entity(parent).add_child(e);
                }
                commands.spawn((mesh, MeshMaterial3d(m), skinned, Transform::default())).id()
            }
        };
        // (Skinned bounds are the bind pose's: a Sim crouching, sitting or crawling would lose
        // their head to culling, worst in the portrait camera's narrow view.)
        commands.entity(e).insert((SimModelPart, sim_layers(), bevy::camera::visibility::NoFrustumCulling));
        commands.entity(parent).add_child(e);
    }
    commands.entity(parent).insert(Skeleton { rig, joints, bind, shape });
    parent
}

/// Household members and visiting neighbours with their bodies, prepared while loading.
#[derive(Resource, Default)]
pub struct PreparedSims {
    pub members: Vec<(Sim, Option<SimModelCpu>)>,
    pub neighbors: Vec<(Sim, Option<SimModelCpu>)>,
    /// Townies who stroll past the lot.
    pub townies: Vec<(Sim, Option<SimModelCpu>)>,
}

/// Maps the stand-in skin colour presets onto the game's skin-tone ramp.
pub fn tone_of(sim: &Sim) -> f32 {
    let l = sim.skin.to_srgba();
    let lum = 0.3 * l.red + 0.59 * l.green + 0.11 * l.blue;
    ((0.85 - lum) / 0.55).clamp(0.0, 1.0)
}

/// Builds bodies for the household, the visiting neighbours (`known`, from a save, or two new
/// ones) and a few townies. `town` are the world's own Sims, used before random ones.
pub fn prepare_sims(baked: &BakedData, cas: &CasData, members: &[Sim], known: Option<&[Sim]>, town: &[Sim]) -> PreparedSims {
    let mut rng = rand::rng();
    let mut town = town.iter().filter(|s| !members.iter().any(|m| m.id == s.id) && !known.is_some_and(|k| k.iter().any(|m| m.id == s.id)));
    let mut next_townie = |rng: &mut rand::rngs::ThreadRng, age: Age| {
        town.next().cloned().unwrap_or_else(|| {
            let last = crate::sim::random_last_name(rng);
            crate::sim::random_sim(rng, &last, None, age)
        })
    };
    let mut neighbors = Vec::new();
    match known {
        Some(k) => neighbors.extend(k.iter().cloned()),
        None => {
            for k in 0..2 {
                neighbors.push(next_townie(&mut rng, if k == 0 { Age::Adult } else { Age::YoungAdult }));
            }
        }
    }
    const TOWNIES: usize = 6;
    for k in 0..TOWNIES {
        neighbors.push(next_townie(&mut rng, if k % 2 == 0 { Age::YoungAdult } else { Age::Adult }));
    }
    let outfits: Vec<(Sim, Outfit)> =
        members.iter().chain(neighbors.iter()).map(|s| (s.clone(), pick_outfit(cas, s, &mut rand::rngs::StdRng::seed_from_u64(s.look)))).collect();
    for (s, o) in &outfits {
        debug!("outfit {} ({:?}): body {:?} hair {:?}", s.full_name(), s.age, o.body.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), o.hair.as_ref().map(|h| &h.name));
    }
    let mut built: Vec<(Sim, Option<SimModelCpu>)> =
        crate::world::par_map(&outfits, |(s, o)| (s.clone(), build_sim_model(baked, cas, s, o, tone_of(s))));
    let mut neighbors = built.split_off(members.len());
    let townies = neighbors.split_off(neighbors.len() - TOWNIES);
    PreparedSims { members: built, neighbors, townies }
}
