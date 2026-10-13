//! Bevy meshes and materials for objects, built from the baked cache (pre-decoded meshes and
//! GPU-compressed DDS textures), with caching.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageAddressMode, ImageFormat, ImageSampler, ImageSamplerDescriptor, ImageType};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use s3bake::{BakedModel, Key};

use crate::baked::BakedData;

#[derive(Clone)]
pub struct ModelPart {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub bounds: (Vec3, Vec3),
    /// Lot imposter layer (`s3bake::LAYER_*`), 0 for ordinary models.
    pub layer: u8,
    /// Its texture, how it's blended and whether it's lit (to draw it again in a design).
    pub tex: Option<Key>,
    pub mode: u8,
    pub unlit: bool,
    /// The model whose rig it's skinned to, when it has moving parts (`objanim`).
    pub rig: Option<Key>,
    /// Its other looks: the model's material states (dirty, clogged; see `grime`).
    pub states: Vec<(u32, Handle<StandardMaterial>)>,
}

/// A catalogue object in a design: the texture drawn from it, in place of the object's own
/// composited texture (one of the catalogue's designs, or one the lot was furnished in).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Design(pub Key);

/// The texture of an object's design.
pub fn design_texture(objd: Key, design: u8) -> Key {
    (s3bake::gamedata::T_DESIGN, design as u32, objd.2)
}

/// A part of a lot imposter: its ground, roofs or the rest.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct ImposterLayer(pub u8);

#[derive(Resource, Default)]
pub struct ObjectAssets {
    /// The catalogue's SpeedTree objects (trees, shrubs, flowers), drawn from their species'
    /// billboards (see `set_garden`).
    garden: HashMap<Key, s3bake::TreeKindBaked>,
    models: HashMap<Key, Vec<ModelPart>>,
    objects: HashMap<Key, Vec<ModelPart>>,
    designed: HashMap<(Key, Key), Vec<ModelPart>>,
    textures: HashMap<Key, Option<Handle<Image>>>,
    materials: HashMap<(Option<Key>, u8, bool), Handle<StandardMaterial>>,
    /// Skinned meshes' copies without their skinning (see [`ObjectAssets::still`]).
    still: HashMap<AssetId<Mesh>, Handle<Mesh>>,
}

pub struct AssetCtx<'a> {
    pub baked: &'a BakedData,
    pub meshes: &'a mut Assets<Mesh>,
    pub images: &'a mut Assets<Image>,
    pub materials: &'a mut Assets<StandardMaterial>,
}

pub fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    })
}

/// Loads a baked DDS (BC-compressed, with mips) straight into a GPU image.
pub fn dds_image(bytes: &[u8], srgb: bool) -> Option<Image> {
    Image::from_buffer(
        bytes,
        ImageType::Format(ImageFormat::Dds),
        CompressedImageFormats::BC,
        srgb,
        sampler(),
        RenderAssetUsages::RENDER_WORLD,
    )
    .ok()
}

/// A baked texture as a GPU image (any thread).
pub fn cpu_texture(baked: &BakedData, key: Key) -> Option<Image> {
    dds_image(&baked.texture_bytes(&key)?, true)
}

/// A mesh part ready to upload (any thread).
pub struct CpuPart {
    pub mesh: Mesh,
    pub tex: Option<Key>,
    pub mode: u8,
    pub unlit: bool,
    pub bounds: (Vec3, Vec3),
    pub layer: u8,
    pub states: Vec<(u32, Key)>,
}

/// How far flat parts lying on the floor (rugs, a bowling lane's boards) are raised: just over
/// the floor, which is drawn a whisker above its level.
const FLAT_LIFT: f32 = 0.035;

pub fn cpu_model(model: BakedModel) -> Vec<CpuPart> {
    model
        .parts
        .into_iter()
        .map(|mut p| {
            // (Flat at the object's foot, or facing up at it, as a bowling lane's boards: lifted
            // in the mesh itself, so the object stays where it's put.)
            if p.layer == 0 && p.bmin[1] > -0.1 && p.bmin[1] < 0.03 {
                let flat = p.bmax[1] - p.bmin[1] < 0.03 && p.bmin[1] > -0.02;
                let mut lifted = false;
                for (v, n) in p.positions.iter_mut().zip(&p.normals) {
                    if flat || (v[1].abs() < 0.006 && n[1] > 0.9) {
                        v[1] += FLAT_LIFT;
                        lifted = true;
                    }
                }
                if lifted {
                    p.bmax[1] = p.bmax[1].max(p.bmin[1] + FLAT_LIFT);
                    if flat {
                        p.bmin[1] += FLAT_LIFT;
                        p.bmax[1] = p.bmax[1].max(p.bmin[1]);
                    }
                }
            }
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, p.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, p.uvs);
            mesh.insert_indices(Indices::U32(p.indices));
            CpuPart { mesh, tex: p.texture, mode: p.mode, unlit: p.unlit, bounds: (Vec3::from(p.bmin), Vec3::from(p.bmax)), layer: p.layer, states: p.states }
        })
        .collect()
}

impl ObjectAssets {
    pub fn texture(&mut self, ctx: &mut AssetCtx, key: Key) -> Option<Handle<Image>> {
        if let Some(h) = self.textures.get(&key) {
            return h.clone();
        }
        let handle = cpu_texture(ctx.baked, key).map(|img| ctx.images.add(img));
        self.textures.insert(key, handle.clone());
        handle
    }

    pub fn ingest_texture(&mut self, images: &mut Assets<Image>, key: Key, img: Option<Image>) {
        if !self.textures.contains_key(&key) {
            let h = img.map(|i| images.add(i));
            self.textures.insert(key, h);
        }
    }

    pub fn ingest_model(&mut self, ctx: &mut AssetCtx, key: Key, cpu: Vec<CpuPart>) -> Vec<ModelPart> {
        let mut parts = Vec::new();
        let skin = ctx.baked.skin(&key);
        for mut p in cpu {
            // Lot imposters cut out railings, fences, shrubs and window frames with their
            // atlas's alpha: smoothed over the pixel's samples (alpha to coverage), so thin
            // fences thin out with distance rather than flicker.
            let mode = if p.layer != 0 && p.mode < 2 { 3 } else { p.mode };
            let material = self.material_for_key(ctx, p.tex, mode, p.unlit);
            // A moving part: skinned to the model's rig (its mesh found by its vertices).
            let rig = skin.as_ref().is_some_and(|s| skin_part(&mut p.mesh, s)).then_some(key);
            let states = p.states.iter().map(|(s, k)| (*s, self.material_for_key(ctx, Some(*k), mode, p.unlit))).collect();
            parts.push(ModelPart { mesh: ctx.meshes.add(p.mesh), material, bounds: p.bounds, layer: p.layer, tex: p.tex, mode, unlit: p.unlit, rig, states });
        }
        self.models.insert(key, parts.clone());
        parts
    }

    /// The unlit materials (the lot imposters' pre-rendered pictures), dimmed with the night.
    pub fn unlit_materials(&self) -> impl Iterator<Item = &Handle<StandardMaterial>> {
        self.materials.iter().filter(|((_, _, unlit), _)| *unlit).map(|(_, h)| h)
    }

    fn material_for_key(&mut self, ctx: &mut AssetCtx, tex_key: Option<Key>, mode: u8, unlit: bool) -> Handle<StandardMaterial> {
        if let Some(h) = self.materials.get(&(tex_key, mode, unlit)) {
            return h.clone();
        }
        let tex = tex_key.and_then(|k| self.texture(ctx, k));
        // Untextured blended parts are the game's glass (windows, doors, vitrines): a faint,
        // glossy tint that lets the room show through.
        let glass = tex.is_none() && mode == 2;
        let handle = ctx.materials.add(StandardMaterial {
            base_color: match (tex.is_some(), glass) {
                (true, _) => Color::WHITE,
                (false, true) => Color::srgba(0.72, 0.8, 0.86, 0.18),
                (false, false) => Color::srgb(0.75, 0.75, 0.72),
            },
            base_color_texture: tex,
            perceptual_roughness: if glass { 0.08 } else { 0.7 },
            reflectance: if glass { 0.6 } else { 0.3 },
            unlit,
            alpha_mode: match mode {
                2 => AlphaMode::Blend,
                1 => AlphaMode::Mask(0.5),
                3 => AlphaMode::AlphaToCoverage,
                _ => AlphaMode::Opaque,
            },
            double_sided: mode != 0,
            cull_mode: if mode != 0 { None } else { Some(bevy::render::render_resource::Face::Back) },
            ..default()
        });
        self.materials.insert((tex_key, mode, unlit), handle.clone());
        handle
    }

    pub fn model(&mut self, ctx: &mut AssetCtx, key: Key) -> Vec<ModelPart> {
        if let Some(p) = self.models.get(&key) {
            return p.clone();
        }
        let cpu = ctx.baked.model(&key).map(cpu_model).unwrap_or_default();
        self.ingest_model(ctx, key, cpu)
    }

    /// How many designs a catalogue object comes in (0 or 1: just the one).
    pub fn design_count(ctx: &AssetCtx, objd: Key) -> u8 {
        ctx.baked.designs.get(&objd).map_or(0, |d| d.count)
    }

    /// Whether an object can be drawn in a design (it has a composited texture for designs to
    /// stand in for, and the design's been drawn).
    pub fn design_applies(&mut self, ctx: &mut AssetCtx, objd: Key, design: Key) -> bool {
        ctx.baked.designs.contains_key(&objd) && self.texture(ctx, design).is_some()
    }

    /// A catalogue object's parts in a design (`None`: as the game ships it).
    pub fn object_design(&mut self, ctx: &mut AssetCtx, objd: Key, design: Option<Key>) -> Vec<ModelPart> {
        let parts = self.object(ctx, objd);
        let (Some(tex), Some(texture)) = (design, ctx.baked.designs.get(&objd).map(|d| d.texture)) else { return parts };
        if let Some(p) = self.designed.get(&(objd, tex)) {
            return p.clone();
        }
        let out: Vec<ModelPart> = if self.texture(ctx, tex).is_none() {
            parts
        } else {
            parts
                .into_iter()
                .map(|mut p| {
                    if p.tex == Some(texture) {
                        p.material = self.material_for_key(ctx, Some(tex), p.mode, p.unlit);
                        p.tex = Some(tex);
                    }
                    p
                })
                .collect()
        };
        self.designed.insert((objd, tex), out.clone());
        out
    }

    /// All model parts of a catalog object (OBJD key).
    pub fn object(&mut self, ctx: &mut AssetCtx, objd: Key) -> Vec<ModelPart> {
        if let Some(p) = self.objects.get(&objd) {
            return p.clone();
        }
        let models = ctx.baked.catalog_entry(&objd).map(|e| e.models.clone()).unwrap_or_default();
        let mut parts = Vec::new();
        for m in models {
            parts.extend(self.model(ctx, m));
        }
        // (A tree, shrub or flower: its species' pictures, crossed.)
        if parts.is_empty()
            && let Some(k) = self.garden.get(&objd).cloned()
            && let Some(tex) = self.texture(ctx, k.billboard)
        {
            let material = ctx.materials.add(StandardMaterial {
                base_color_texture: Some(tex),
                alpha_mode: AlphaMode::Mask(0.45),
                double_sided: true,
                cull_mode: None,
                perceptual_roughness: 0.95,
                reflectance: 0.1,
                ..default()
            });
            // (It takes up the ground at its trunk or stem, as the game's: a tile at most.)
            let r = (k.radius * 0.25).clamp(0.2, 0.5);
            parts.push(ModelPart {
                mesh: ctx.meshes.add(crate::world::tree_mesh(&k)),
                material,
                bounds: (Vec3::new(-r, 0.0, -r), Vec3::new(r, k.height.max(0.3), r)),
                layer: 0,
                tex: None,
                mode: 0,
                unlit: false,
                rig: None,
                states: Vec::new(),
            });
        }
        self.objects.insert(objd, parts.clone());
        parts
    }

    /// The catalogue's SpeedTree objects and their species (from the game data).
    pub fn set_garden(&mut self, garden: &[(Key, s3bake::TreeKindBaked)]) {
        for (k, kind) in garden {
            self.objects.remove(k);
            self.garden.insert(*k, kind.clone());
        }
    }

    pub fn has_garden(&self) -> bool {
        !self.garden.is_empty()
    }

    /// A mesh to draw without a skeleton: a skinned part's, its copy without bones and weights
    /// (at rest it's the same shape), else itself. A mesh with bones drawn without its skin
    /// fails the GPU's validation, which ends the game.
    pub fn still(&mut self, meshes: &mut Assets<Mesh>, mesh: &Handle<Mesh>) -> Handle<Mesh> {
        if let Some(h) = self.still.get(&mesh.id()) {
            return h.clone();
        }
        let Some(m) = meshes.get(mesh) else { return mesh.clone() };
        if m.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_none() && m.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_none() {
            return mesh.clone();
        }
        let mut copy = m.clone();
        copy.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
        copy.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
        let h = meshes.add(copy);
        self.still.insert(mesh.id(), h.clone());
        h
    }
}

/// Gives a mesh its skinning from a model's (the mesh with its vertex count and first vertex):
/// each vertex's rig bones and weights. Whether it had any.
fn skin_part(mesh: &mut Mesh, skin: &s3bake::gamedata::ObjectSkin) -> bool {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return false };
    let Some(first) = pos.first().copied() else { return false };
    // (A flat part may have been lifted over the floor.)
    let same = |a: Vec3, b: Vec3| a.distance(b) < 1e-5 || (a + Vec3::Y * FLAT_LIFT).distance(b) < 1e-5;
    let Some(s) = skin.meshes.iter().find(|m| m.verts as usize == pos.len() && same(Vec3::from(m.first), Vec3::from(first))) else { return false };
    let weights: Vec<[f32; 4]> = s
        .weights
        .iter()
        .map(|w| {
            let sum: f32 = w.iter().map(|x| *x as f32).sum();
            if sum <= 0.0 { [1.0, 0.0, 0.0, 0.0] } else { w.map(|x| x as f32 / sum) }
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, bevy::mesh::VertexAttributeValues::Uint16x4(s.bones.clone()));
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
    true
}

/// Spawns an object's meshes as children of a new entity.
pub fn spawn_parts(commands: &mut Commands, parts: &[ModelPart], transform: Transform) -> Entity {
    commands
        .spawn((transform, Visibility::default()))
        .with_children(|c| {
            for p in parts {
                let mut e = c.spawn((Mesh3d(p.mesh.clone()), MeshMaterial3d(p.material.clone())));
                if let Some(rig) = p.rig {
                    e.insert(crate::objanim::SkinPart(rig));
                }
                if !p.states.is_empty() {
                    e.insert(crate::grime::MaterialStates { states: p.states.clone(), own: None, shown: 0 });
                }
                if p.layer != 0 {
                    e.insert(ImposterLayer(p.layer));
                }
                // (A lot imposter's pre-lit ground picture is never drawn: see `building`.)
                if p.layer == s3bake::LAYER_GROUND {
                    e.insert(Visibility::Hidden);
                }
            }
        })
        .id()
}

/// Recolour the object's mesh children in place. Its rig, lights and contents remain attached.
pub fn restyle_parts(commands: &mut Commands, root: Entity, parts: &[ModelPart]) {
    let materials: Vec<_> = parts.iter().map(|p| p.material.clone()).collect();
    commands.queue(move |world: &mut World| {
        let children: Vec<Entity> = world.get::<Children>(root).map(|c| c.iter().collect()).unwrap_or_default();
        let meshes: Vec<Entity> = children.into_iter().filter(|e| world.get::<Mesh3d>(*e).is_some()).collect();
        for (e, material) in meshes.into_iter().zip(materials) {
            world.entity_mut(e).insert(MeshMaterial3d(material));
        }
    });
}

/// Combined local-space bounds of a set of parts.
pub fn parts_bounds(parts: &[ModelPart]) -> Option<(Vec3, Vec3)> {
    let mut it = parts.iter();
    let first = it.next()?;
    Some(it.fold(first.bounds, |(a, b), p| (a.min(p.bounds.0), b.max(p.bounds.1))))
}
