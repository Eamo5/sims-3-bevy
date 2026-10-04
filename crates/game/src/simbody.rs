//! Real Sim bodies built from the game's CAS data: skeleton (rig), skinned GEOM meshes for
//! face, hair, top, bottom and shoes, and composited skin + clothing textures.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use s3formats::dds::Rgba;
use s3formats::sim::*;
use s3pkg::{PackageSet, ResourceKey, types};

use crate::sim::{Age, Sim};

/// Lightweight index entry for one CAS part.
#[derive(Clone, Debug)]
pub struct CasEntry {
    pub key: ResourceKey,
    pub name: String,
    pub clothing_type: u32,
    pub age_gender: u32,
    pub category: u32,
}

/// CAS data needed to dress sims, built on the loading thread.
#[derive(Resource, Clone)]
pub struct CasData {
    pub parts: Arc<Vec<CasEntry>>,
    pub adult_rig: Option<Arc<Rig>>,
    pub child_rig: Option<Arc<Rig>>,
    pub tone: Option<Arc<SkinTone>>,
}

pub fn build_cas(pkgs: &PackageSet) -> CasData {
    let keys: Vec<ResourceKey> = pkgs.keys_of_type(types::CASP).copied().collect();
    let parts: Vec<CasEntry> = crate::world::par_map(&keys, |k| {
        let d = pkgs.read(k)?;
        let c = CasPart::parse(&d).ok()?;
        Some(CasEntry { key: *k, name: c.name, clothing_type: c.clothing_type, age_gender: c.age_gender, category: c.category })
    })
    .into_iter()
    .flatten()
    .collect();
    let rig = |name: &str| {
        pkgs.read_ti(types::RIG, s3pkg::fnv64(name)).and_then(|d| Rig::parse(&d).ok()).map(Arc::new)
    };
    let mut tones: Vec<ResourceKey> = pkgs.keys_of_type(T_TONE).copied().collect();
    tones.sort();
    let tone = tones.iter().find_map(|k| SkinTone::parse(&pkgs.read(k)?).ok()).map(Arc::new);
    CasData { parts: Arc::new(parts), adult_rig: rig("auRig"), child_rig: rig("cuRig"), tone }
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
    pub face: Option<ResourceKey>,
    pub scalp: Option<ResourceKey>,
    pub hair: Option<ResourceKey>,
    pub body: Vec<ResourceKey>,
}

pub fn pick_outfit(cas: &CasData, sim: &Sim, rng: &mut impl Rng) -> Outfit {
    let age = age_bits(sim.age);
    let gender = if sim.female { GENDER_FEMALE } else { GENDER_MALE };
    let fits = |e: &&CasEntry| e.age_gender & age != 0 && e.age_gender & gender != 0 && e.category & CAT_HIDDEN == 0;
    let of_type = |t: u32| cas.parts.iter().filter(|e| e.clothing_type == t).filter(fits).collect::<Vec<_>>();
    let face = of_type(CT_FACE).into_iter().filter(|e| e.name.ends_with("Face")).min_by_key(|e| e.name.len()).map(|e| e.key);
    let scalp = of_type(CT_SCALP).into_iter().filter(|e| e.name.ends_with("Scalp")).min_by_key(|e| e.name.len()).map(|e| e.key);
    let everyday = |t: u32| {
        of_type(t)
            .into_iter()
            .filter(|e| e.category & CAT_EVERYDAY != 0 && e.category & CAT_VALID_RANDOM != 0 && e.category & 0x400000 == 0)
            .filter(|e| !e.name.contains("Nude") && !e.name.to_ascii_lowercase().contains("hat"))
            .collect::<Vec<_>>()
    };
    let hair = everyday(CT_HAIR).choose(rng).map(|e| e.key);
    let mut body = Vec::new();
    let tops = everyday(CT_TOP);
    let bottoms = everyday(CT_BOTTOM);
    let fulls = everyday(CT_BODY);
    if (!fulls.is_empty() && rng.random_bool(0.25)) || tops.is_empty() || bottoms.is_empty() {
        if let Some(f) = fulls.choose(rng) {
            body.push(f.key);
        }
    } else {
        body.push(bottoms.choose(rng).unwrap().key);
        body.push(tops.choose(rng).unwrap().key);
    }
    if let Some(s) = everyday(CT_SHOES).choose(rng) {
        body.push(s.key);
    }
    Outfit { face, scalp, hair, body }
}

/// A sim body decoded on the loading thread.
pub struct SimModelCpu {
    pub rig: Arc<Rig>,
    pub parts: Vec<(Mesh, Image, u8)>,
}

fn skin_base(pkgs: &PackageSet, cas: &CasData, age: u32, gender: u32, kind: u32, tone_t: f32) -> Option<Rgba> {
    let tone = cas.tone.as_ref()?;
    let t = tone.find(age, gender, kind)?;
    let mut img = s3formats::dds::decode(&pkgs.read(&t.detail_light?)?, 512)?;
    // Darken along the skin-tone ramp (light at the top, dark at the bottom).
    if let Some(ramp) = tone.ramp.and_then(|k| s3formats::dds::decode(&pkgs.read(&k)?, 64)) {
        let base = ramp.sample(0.5, 0.06);
        let target = ramp.sample(0.5, 0.06 + tone_t * 0.88);
        let f = [target[0] / base[0].max(0.01), target[1] / base[1].max(0.01), target[2] / base[2].max(0.01)];
        for px in img.data.chunks_exact_mut(4) {
            for c in 0..3 {
                px[c] = (px[c] as f32 * f[c]).clamp(0.0, 255.0) as u8;
            }
        }
    }
    Some(img)
}

fn composite_over(pkgs: &PackageSet, txtc: &ResourceKey, base: Option<Rgba>, size: (usize, usize)) -> Option<Rgba> {
    let d = pkgs.read(txtc).or_else(|| pkgs.read_ti(txtc.t, txtc.i))?;
    let t = s3formats::txtc::Txtc::parse(&d).ok()?;
    let mut c = s3formats::compositor::Compositor::new(pkgs);
    c.max_size = 512;
    Some(c.run_with_base(&t, size.0, size.1, base))
}

fn geom_mesh(g: &Geom, rig: &Rig) -> Mesh {
    let palette: Vec<u16> = g.bone_hashes.iter().map(|h| rig.index_of(*h).unwrap_or(0) as u16).collect();
    let mut joints = Vec::with_capacity(g.positions.len());
    let mut weights = Vec::with_capacity(g.positions.len());
    for (bi, bw) in g.bone_indices.iter().zip(&g.weights) {
        let mut j = [0u16; 4];
        let mut w = *bw;
        for k in 0..4 {
            j[k] = palette.get(bi[k] as usize).copied().unwrap_or(0);
            if bi[k] as usize >= palette.len() {
                w[k] = 0.0;
            }
        }
        let s: f32 = w.iter().sum();
        if s > 1e-4 {
            for x in &mut w {
                *x /= s;
            }
        } else {
            w = [1.0, 0.0, 0.0, 0.0];
            j = [1, 0, 0, 0];
        }
        joints.push(j);
        weights.push(w);
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, g.positions.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, g.normals.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(joints));
    m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
    m.insert_indices(Indices::U32(g.indices.clone()));
    m
}

const SHADER_SIM_SKIN: u32 = 0x548394B9;
const SHADER_SIM_EYES: u32 = 0xCF8A70B4;
const SHADER_SIM_EYELASHES: u32 = 0x9D9DA161;
const P_DIFFUSE: u32 = 0x6CC0FD85;

/// Builds a sim's meshes and textures (any thread).
pub fn build_sim_model(pkgs: &PackageSet, cas: &CasData, sim: &Sim, outfit: &Outfit, tone_t: f32) -> Option<SimModelCpu> {
    let child = sim.age == Age::Child;
    let rig = if child { cas.child_rig.clone() } else { cas.adult_rig.clone() }?;
    let age = age_bits(sim.age);
    let gender = if sim.female { GENDER_FEMALE } else { GENDER_MALE };
    let load_part = |k: &ResourceKey| -> Option<(CasPart, Vec<Geom>)> {
        let c = CasPart::parse(&pkgs.read(k)?).ok()?;
        let geoms = c
            .lod0_geoms(pkgs)
            .iter()
            .filter_map(|g| Geom::parse(&pkgs.read(g).or_else(|| pkgs.read_ti(g.t, g.i))?).ok())
            .collect();
        Some((c, geoms))
    };
    let mut parts = Vec::new();
    let image = |rgba: Rgba| crate::objects::rgba_image(rgba);
    let geom_texture = |g: &Geom| -> Option<Image> {
        match g.params.get(&P_DIFFUSE) {
            Some(s3formats::model::ParamValue::Texture(k)) => crate::objects::build_texture_cpu(pkgs, *k, 512),
            _ => None,
        }
    };

    // Body: skin + each clothing layer composited into one shared texture.
    let mut body_tex = skin_base(pkgs, cas, age, gender, 8, tone_t);
    let size = body_tex.as_ref().map(|i| (i.width, i.height)).unwrap_or((512, 512));
    let mut body_geoms = Vec::new();
    for k in &outfit.body {
        if let Some((c, geoms)) = load_part(k) {
            if let Some(tx) = c.diffuse.first() {
                body_tex = composite_over(pkgs, tx, body_tex.take(), size).or(body_tex);
            }
            body_geoms.extend(geoms);
        }
    }
    let body_img = body_tex.map(image).unwrap_or_default();
    for g in &body_geoms {
        parts.push((geom_mesh(g, &rig), body_img.clone(), 0u8));
    }

    // Face: face skin + face CAS layer; eyes and lashes use their own textures.
    if let Some((c, geoms)) = outfit.face.as_ref().and_then(load_part) {
        let mut face_tex = skin_base(pkgs, cas, age, gender, 4, tone_t);
        let fsize = face_tex.as_ref().map(|i| (i.width, i.height)).unwrap_or((512, 512));
        if let Some(tx) = c.diffuse.first() {
            face_tex = composite_over(pkgs, tx, face_tex.take(), fsize).or(face_tex);
        }
        let face_img = face_tex.map(image).unwrap_or_default();
        for g in &geoms {
            match g.shader {
                SHADER_SIM_EYES => parts.push((geom_mesh(g, &rig), geom_texture(g).unwrap_or_default(), 0)),
                SHADER_SIM_EYELASHES => {
                    if let Some(t) = geom_texture(g) {
                        parts.push((geom_mesh(g, &rig), t, 1));
                    }
                }
                _ => parts.push((geom_mesh(g, &rig), face_img.clone(), 0)),
            }
        }
    }

    // Scalp and hair.
    for (k, alpha) in [(outfit.scalp, 1u8), (outfit.hair, 1u8)] {
        let Some((c, geoms)) = k.as_ref().and_then(load_part) else { continue };
        let tex = c.diffuse.first().and_then(|tx| composite_over(pkgs, tx, None, (512, 512))).map(image);
        for g in &geoms {
            let t = tex.clone().or_else(|| geom_texture(g)).unwrap_or_default();
            parts.push((geom_mesh(g, &rig), t, if g.shader == SHADER_SIM_SKIN { alpha } else { 1 }));
        }
    }
    let _ = SHADER_SIM_SKIN;
    Some(SimModelCpu { rig, parts })
}

/// The skeleton of a spawned sim: one entity per rig bone.
#[derive(Component)]
pub struct Skeleton {
    pub rig: Arc<Rig>,
    pub joints: Vec<Entity>,
    pub bind: Vec<Transform>,
}

/// Spawns skeleton + skinned meshes under `parent`.
pub fn spawn_sim_model(
    commands: &mut Commands,
    parent: Entity,
    model: SimModelCpu,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    mats: &mut Assets<StandardMaterial>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) -> Entity {
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
        let e = commands.spawn((local, Visibility::default())).id();
        joints.push(e);
    }
    for (i, b) in rig.bones.iter().enumerate() {
        let p = if b.parent >= 0 && (b.parent as usize) < joints.len() { joints[b.parent as usize] } else { parent };
        commands.entity(p).add_child(joints[i]);
    }
    let inverse: Vec<Mat4> = world.iter().map(|m| m.inverse()).collect();
    let ibp = bindposes.add(SkinnedMeshInverseBindposes::from(inverse));
    let mut cache: HashMap<usize, Handle<StandardMaterial>> = HashMap::new();
    for (i, (mesh, img, mode)) in model.parts.into_iter().enumerate() {
        let tex = images.add(img);
        let mat = cache.entry(i).or_insert_with(|| {
            mats.add(StandardMaterial {
                base_color_texture: Some(tex),
                perceptual_roughness: 0.65,
                reflectance: 0.25,
                alpha_mode: if mode == 1 { AlphaMode::Mask(0.4) } else { AlphaMode::Opaque },
                double_sided: mode == 1,
                cull_mode: if mode == 1 { None } else { Some(bevy::render::render_resource::Face::Back) },
                ..default()
            })
        });
        let m = commands
            .spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(mat.clone()),
                SkinnedMesh { inverse_bindposes: ibp.clone(), joints: joints.clone() },
                Transform::default(),
            ))
            .id();
        commands.entity(parent).add_child(m);
    }
    let _ = types::DDS;
    commands.entity(parent).insert(Skeleton { rig, joints, bind });
    parent
}

/// Household members and visiting neighbours with their bodies, prepared while loading.
#[derive(Resource, Default)]
pub struct PreparedSims {
    pub members: Vec<(Sim, Option<SimModelCpu>)>,
    pub neighbors: Vec<(Sim, Option<SimModelCpu>)>,
}

/// Maps the stand-in skin colour presets onto the game's skin-tone ramp.
pub fn tone_of(sim: &Sim) -> f32 {
    let l = sim.skin.to_srgba();
    let lum = 0.3 * l.red + 0.59 * l.green + 0.11 * l.blue;
    ((0.85 - lum) / 0.55).clamp(0.0, 1.0)
}

pub fn prepare_sims(pkgs: &PackageSet, cas: &CasData, members: &[Sim]) -> PreparedSims {
    let mut rng = rand::rng();
    let mut neighbors = Vec::new();
    for k in 0..2 {
        let last = crate::sim::random_last_name(&mut rng);
        neighbors.push(crate::sim::random_sim(&mut rng, &last, None, if k == 0 { Age::Adult } else { Age::YoungAdult }));
    }
    let outfits: Vec<(Sim, Outfit)> =
        members.iter().chain(neighbors.iter()).map(|s| (s.clone(), pick_outfit(cas, s, &mut rng))).collect();
    let mut built: Vec<(Sim, Option<SimModelCpu>)> =
        crate::world::par_map(&outfits, |(s, o)| (s.clone(), build_sim_model(pkgs, cas, s, o, tone_of(s))));
    let neighbors = built.split_off(members.len());
    PreparedSims { members: built, neighbors }
}
