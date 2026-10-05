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
        }
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
}

/// In swimwear (for a swim): the body is dressed from the swimwear parts.
#[derive(Component)]
pub struct InSwimwear;

pub fn pick_outfit(cas: &CasData, sim: &Sim, rng: &mut impl Rng) -> Outfit {
    pick_outfit_for(cas, sim, rng, false)
}

/// What a Sim wears: everyday clothes, or swimwear.
pub fn pick_outfit_for(cas: &CasData, sim: &Sim, rng: &mut impl Rng, swim: bool) -> Outfit {
    let age = age_bits(sim.age);
    let gender = if sim.female { GENDER_FEMALE } else { GENDER_MALE };
    let fits = |e: &&CasPartInfo| e.baked && e.age_gender & age != 0 && e.age_gender & gender != 0;
    // Clothes and shoes by the outfit's category (heads and hair go with anything).
    let worn = |e: &&CasPartInfo| {
        !matches!(e.clothing_type, CT_TOP | CT_BOTTOM | CT_BODY | CT_SHOES)
            || if swim { e.category & s3formats::sim::CAT_SWIM != 0 } else { e.category & s3formats::sim::CAT_EVERYDAY != 0 }
    };
    let of_type = |t: u32| {
        let all: Vec<&CasPartInfo> = cas.parts.iter().filter(|e| e.clothing_type == t).filter(fits).filter(worn).collect();
        if !swim {
            return all;
        }
        // Swimwear: the base game's (the packs' swimwear counts T-shirts and flippers), a bare
        // chest for men and bare feet for everyone.
        let preferred: Vec<&CasPartInfo> = all
            .iter()
            .copied()
            .filter(|e| match t {
                CT_TOP if !sim.female => e.name.contains("TopNude"),
                CT_SHOES => e.name.contains("ShoesNude"),
                _ => e.key.1 == 0,
            })
            .collect();
        if preferred.is_empty() { all } else { preferred }
    };
    let chosen = |k: Option<Key>, t: u32| k.and_then(|k| cas.parts.iter().find(|p| p.key == k && p.clothing_type == t).filter(|p| fits(p)).cloned());
    // A baby is a single body with its own head.
    if sim.age == Age::Baby {
        let body = of_type(CT_BODY).into_iter().next().cloned();
        return Outfit { face: None, scalp: None, hair: None, brows: None, body: body.into_iter().collect() };
    }
    let face = of_type(CT_FACE).into_iter().min_by_key(|e| e.name.len()).cloned();
    let scalp = of_type(CT_SCALP).into_iter().min_by_key(|e| e.name.len()).cloned();
    let random_hair = of_type(CT_HAIR).choose(rng).map(|e| (*e).clone());
    let hair = chosen(sim.outfit.hair, CT_HAIR).or(random_hair);
    // Natural eyebrows (not the novelty ones).
    let brows = of_type(CT_EYEBROW)
        .into_iter()
        .filter(|e| !["Hairless", "Monobrow", "Extreme"].iter().any(|n| e.name.contains(n)))
        .collect::<Vec<_>>()
        .choose(rng)
        .map(|e| (*e).clone());
    let mut body = Vec::new();
    let (tops, bottoms, fulls) = (of_type(CT_TOP), of_type(CT_BOTTOM), of_type(CT_BODY));
    let use_full = rng.random_bool(0.25);
    let random_full = fulls.choose(rng).map(|f| (*f).clone());
    let random_top = tops.choose(rng).map(|f| (*f).clone());
    let random_bottom = bottoms.choose(rng).map(|f| (*f).clone());
    let random_shoes = of_type(CT_SHOES).choose(rng).map(|f| (*f).clone());
    let (ct, cb, cf) = if swim {
        (None, None, None)
    } else {
        (chosen(sim.outfit.top, CT_TOP), chosen(sim.outfit.bottom, CT_BOTTOM), chosen(sim.outfit.full, CT_BODY))
    };
    // (Swimming, men wear trunks and a bare chest; women a swimsuit or a two-piece.)
    let use_full = if swim { sim.female && rng.random_bool(0.5) && !fulls.is_empty() } else { use_full };
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
    body.extend(if swim { random_shoes } else { chosen(sim.outfit.shoes, CT_SHOES).or(random_shoes) });
    Outfit { face, scalp, hair, brows, body }
}

/// How a sim mesh is shaded.
pub enum SimMat {
    /// Tinted skin texture with clothing layers on top.
    Skin { base: Option<Key>, tint: Vec3, layers: Vec<Key> },
    /// A plain texture (hair, eyes, lashes), alpha-tested when `mask`, optionally tinted
    /// (hair: the texture is greyscale, coloured by the Sim's hair colour).
    Plain { tex: Option<Key>, mask: bool, tint: Option<Color> },
}

/// A sim body decoded on the loading thread.
pub struct SimModelCpu {
    pub rig: Arc<Rig>,
    pub parts: Vec<(Mesh, SimMat)>,
    pub textures: Vec<(Key, Image)>,
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

fn skin_mesh(m: SkinMesh) -> Mesh {
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
    let tint = cas.tint(tone_t);
    let mut parts = Vec::new();
    let mut tex_keys: Vec<Key> = Vec::new();

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
        let face_layers: Vec<Key> = face.layer.into_iter().chain(outfit.brows.as_ref().and_then(|b| b.layer)).collect();
        tex_keys.extend(face_base);
        tex_keys.extend(&face_layers);
        for m in baked.cas_meshes(&face.key).map(|m| m.meshes).unwrap_or_default() {
            let mat = match m.shader {
                SHADER_SIM_EYES => SimMat::Plain { tex: m.texture, mask: false, tint: None },
                SHADER_SIM_EYELASHES => SimMat::Plain { tex: m.texture, mask: true, tint: None },
                _ => SimMat::Skin { base: face_base, tint, layers: face_layers.clone() },
            };
            if let SimMat::Plain { tex: Some(t), .. } = &mat {
                tex_keys.push(*t);
            }
            parts.push((skin_mesh(m), mat));
        }
    }
    // Hair is drawn from a greyscale copy of its texture tinted with the Sim's hair colour.
    let mut hair_keys: Vec<(Key, Key)> = Vec::new();
    for (i, p) in [&outfit.scalp, &outfit.hair].into_iter().enumerate() {
        let Some(p) = p else { continue };
        for m in baked.cas_meshes(&p.key).map(|m| m.meshes).unwrap_or_default() {
            let Some(src) = p.layer.or(m.texture) else { continue };
            if i == 1 {
                let grey = hair_grey_key(src);
                hair_keys.push((src, grey));
                parts.push((skin_mesh(m), SimMat::Plain { tex: Some(grey), mask: true, tint: Some(sim.hair) }));
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
    Some(SimModelCpu { rig, parts, textures })
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

/// Sims are drawn in the world and in the portrait studio.
fn sim_layers() -> bevy::camera::visibility::RenderLayers {
    bevy::camera::visibility::RenderLayers::from_layers(&[0, crate::portraits::STUDIO_LAYER])
}

/// A top-level piece of a Sim's body (root joint or mesh), replaced when the body is rebuilt.
#[derive(Component)]
pub struct SimModelPart;

/// The skeleton of a spawned sim: one entity per rig bone.
#[derive(Component)]
pub struct Skeleton {
    pub rig: Arc<Rig>,
    pub joints: Vec<Entity>,
    pub bind: Vec<Transform>,
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
        bind.push(local);
        joints.push(commands.spawn((local, Visibility::default())).id());
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
    commands.entity(parent).insert(Skeleton { rig, joints, bind });
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
