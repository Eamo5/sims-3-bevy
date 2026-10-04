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
    pub tone_textures: Arc<Vec<(u32, u32, Key)>>,
    pub ramp: Arc<Vec<[f32; 3]>>,
}

impl CasData {
    pub fn from_baked(b: &BakedData) -> Self {
        Self {
            parts: Arc::new(b.cas.parts.clone()),
            adult_rig: b.cas.adult_rig.clone().map(Arc::new),
            child_rig: b.cas.child_rig.clone().map(Arc::new),
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
        Age::Child => AGE_CHILD,
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
    pub body: Vec<CasPartInfo>,
}

pub fn pick_outfit(cas: &CasData, sim: &Sim, rng: &mut impl Rng) -> Outfit {
    let age = age_bits(sim.age);
    let gender = if sim.female { GENDER_FEMALE } else { GENDER_MALE };
    let fits = |e: &&CasPartInfo| e.baked && e.age_gender & age != 0 && e.age_gender & gender != 0;
    let of_type = |t: u32| cas.parts.iter().filter(|e| e.clothing_type == t).filter(fits).collect::<Vec<_>>();
    let face = of_type(CT_FACE).into_iter().min_by_key(|e| e.name.len()).cloned();
    let scalp = of_type(CT_SCALP).into_iter().min_by_key(|e| e.name.len()).cloned();
    let hair = of_type(CT_HAIR).choose(rng).map(|e| (*e).clone());
    let mut body = Vec::new();
    let (tops, bottoms, fulls) = (of_type(CT_TOP), of_type(CT_BOTTOM), of_type(CT_BODY));
    if (!fulls.is_empty() && rng.random_bool(0.25)) || tops.is_empty() || bottoms.is_empty() {
        if let Some(f) = fulls.choose(rng) {
            body.push((*f).clone());
        }
    } else {
        body.push((*bottoms.choose(rng).unwrap()).clone());
        body.push((*tops.choose(rng).unwrap()).clone());
    }
    if let Some(s) = of_type(CT_SHOES).choose(rng) {
        body.push((*s).clone());
    }
    Outfit { face, scalp, hair, body }
}

/// How a sim mesh is shaded.
pub enum SimMat {
    /// Tinted skin texture with clothing layers on top.
    Skin { base: Option<Key>, tint: Vec3, layers: Vec<Key> },
    /// A plain texture (hair, eyes, lashes), alpha-tested when `mask`.
    Plain { tex: Option<Key>, mask: bool },
}

/// A sim body decoded on the loading thread.
pub struct SimModelCpu {
    pub rig: Arc<Rig>,
    pub parts: Vec<(Mesh, SimMat)>,
    pub textures: Vec<(Key, Image)>,
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
    let rig = if sim.age == Age::Child { cas.child_rig.clone() } else { cas.adult_rig.clone() }?;
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
            parts.push((skin_mesh(m), SimMat::Skin { base: body_base, tint, layers: layers.clone() }));
        }
    }
    if let Some(face) = &outfit.face {
        let face_base = cas.skin_texture(age, gender, 4);
        tex_keys.extend(face_base);
        tex_keys.extend(face.layer);
        for m in baked.cas_meshes(&face.key).map(|m| m.meshes).unwrap_or_default() {
            let mat = match m.shader {
                SHADER_SIM_EYES => SimMat::Plain { tex: m.texture, mask: false },
                SHADER_SIM_EYELASHES => SimMat::Plain { tex: m.texture, mask: true },
                _ => SimMat::Skin { base: face_base, tint, layers: face.layer.into_iter().collect() },
            };
            if let SimMat::Plain { tex: Some(t), .. } = &mat {
                tex_keys.push(*t);
            }
            parts.push((skin_mesh(m), mat));
        }
    }
    for p in [&outfit.scalp, &outfit.hair].into_iter().flatten() {
        tex_keys.extend(p.layer);
        for m in baked.cas_meshes(&p.key).map(|m| m.meshes).unwrap_or_default() {
            let tex = p.layer.or(m.texture);
            parts.push((skin_mesh(m), SimMat::Plain { tex, mask: true }));
        }
    }
    tex_keys.sort();
    tex_keys.dedup();
    let textures = tex_keys
        .into_iter()
        .filter_map(|k| Some((k, crate::objects::dds_image(&baked.texture_bytes(&k)?, true)?)))
        .collect();
    Some(SimModelCpu { rig, parts, textures })
}

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
            SimMat::Plain { tex: t, mask } => {
                let m = ctx.mats.add(StandardMaterial {
                    base_color_texture: tex(t, ctx.textures),
                    perceptual_roughness: 0.6,
                    reflectance: 0.25,
                    alpha_mode: if mask { AlphaMode::Mask(0.4) } else { AlphaMode::Opaque },
                    double_sided: mask,
                    cull_mode: if mask { None } else { Some(bevy::render::render_resource::Face::Back) },
                    ..default()
                });
                commands.spawn((mesh, MeshMaterial3d(m), skinned, Transform::default())).id()
            }
        };
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
/// ones) and a few townies.
pub fn prepare_sims(baked: &BakedData, cas: &CasData, members: &[Sim], known: Option<&[Sim]>) -> PreparedSims {
    let mut rng = rand::rng();
    let mut neighbors = Vec::new();
    match known {
        Some(k) => neighbors.extend(k.iter().cloned()),
        None => {
            for k in 0..2 {
                let last = crate::sim::random_last_name(&mut rng);
                neighbors.push(crate::sim::random_sim(&mut rng, &last, None, if k == 0 { Age::Adult } else { Age::YoungAdult }));
            }
        }
    }
    const TOWNIES: usize = 4;
    for k in 0..TOWNIES {
        let last = crate::sim::random_last_name(&mut rng);
        neighbors.push(crate::sim::random_sim(&mut rng, &last, None, if k % 2 == 0 { Age::YoungAdult } else { Age::Adult }));
    }
    let outfits: Vec<(Sim, Outfit)> =
        members.iter().chain(neighbors.iter()).map(|s| (s.clone(), pick_outfit(cas, s, &mut rand::rngs::StdRng::seed_from_u64(s.look)))).collect();
    let mut built: Vec<(Sim, Option<SimModelCpu>)> =
        crate::world::par_map(&outfits, |(s, o)| (s.clone(), build_sim_model(baked, cas, s, o, tone_of(s))));
    let mut neighbors = built.split_off(members.len());
    let townies = neighbors.split_off(neighbors.len() - TOWNIES);
    PreparedSims { members: built, neighbors, townies }
}
