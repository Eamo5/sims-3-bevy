//! Building Bevy meshes and materials from Sims 3 object models, with caching.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageAddressMode, ImageFormat, ImageSampler, ImageSamplerDescriptor, ImageType};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use s3formats::model::{self, MeshData, P_DIFFUSE_MAP};
use s3pkg::{PackageSet, ResourceKey, types};

#[derive(Clone)]
pub struct ModelPart {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub bounds: (Vec3, Vec3),
}

#[derive(Resource, Default)]
pub struct ObjectAssets {
    models: HashMap<ResourceKey, Vec<ModelPart>>,
    objects: HashMap<ResourceKey, Vec<ModelPart>>,
    textures: HashMap<ResourceKey, Option<Handle<Image>>>,
    materials: HashMap<(Option<ResourceKey>, u8), Handle<StandardMaterial>>,
}

pub struct AssetCtx<'a> {
    pub pkgs: &'a PackageSet,
    pub meshes: &'a mut Assets<Mesh>,
    pub images: &'a mut Assets<Image>,
    pub materials: &'a mut Assets<StandardMaterial>,
}

fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    })
}

/// Decodes a DDS resource into a GPU image.
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

/// Uploads a CPU-composited RGBA image with a generated mip chain.
pub fn rgba_image(img: s3formats::dds::Rgba) -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let (data, levels) = s3formats::dds::build_mips(&img);
    let mut out = Image::default();
    out.data = Some(data);
    out.texture_descriptor.size = Extent3d { width: img.width as u32, height: img.height as u32, depth_or_array_layers: 1 };
    out.texture_descriptor.mip_level_count = levels;
    out.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
    out.texture_descriptor.dimension = TextureDimension::D2;
    out.sampler = sampler();
    out.asset_usage = RenderAssetUsages::RENDER_WORLD;
    out
}

/// Picks a representative DDS from a texture compositor (TXTC) resource.
fn txtc_fallback(d: &[u8]) -> Option<ResourceKey> {
    if d.len() < 8 {
        return None;
    }
    let off = u32::from_le_bytes(d[4..8].try_into().ok()?) as usize;
    let p = 8 + off;
    let n = *d.get(p)? as usize;
    let mut keys = Vec::new();
    for k in 0..n {
        let b = d.get(p + 1 + k * 16..p + 17 + k * 16)?;
        let i = u64::from_le_bytes(b[0..8].try_into().ok()?);
        let g = u32::from_le_bytes(b[8..12].try_into().ok()?);
        let t = u32::from_le_bytes(b[12..16].try_into().ok()?);
        keys.push(ResourceKey::new(t, g, i));
    }
    keys.into_iter().find(|k| k.t == types::DDS)
}

/// Builds a texture (DDS or composited TXTC) without touching Bevy's asset storage.
pub fn build_texture_cpu(pkgs: &PackageSet, key: ResourceKey, max_size: usize) -> Option<Image> {
    let data = pkgs.read(&key).or_else(|| pkgs.read_ti(key.t, key.i));
    match key.t {
        types::TXTC => data
            .as_deref()
            .and_then(|d| s3formats::compositor::composite(pkgs, d, max_size))
            .map(rgba_image)
            .or_else(|| {
                // Fall back to a representative DDS if compositing fails.
                let k = data.as_deref().and_then(txtc_fallback)?;
                dds_image(&pkgs.read(&k)?, true)
            }),
        types::DDS => data.and_then(|d| dds_image(&d, true)),
        _ => None,
    }
}

/// A mesh part decoded on any thread.
pub struct CpuPart {
    pub mesh: Mesh,
    pub tex: Option<ResourceKey>,
    pub mode: u8,
    pub bounds: (Vec3, Vec3),
}

pub const P_IMPOSTER_TEXTURE: u32 = 0xBDCF71C5;

fn diffuse_key(m: &MeshData) -> Option<ResourceKey> {
    if m.material.shader == model::SHADER_LOT_IMPOSTER {
        m.material.texture(P_IMPOSTER_TEXTURE).or_else(|| m.material.texture(P_DIFFUSE_MAP))
    } else {
        m.material.texture(P_DIFFUSE_MAP)
    }
}

/// Decodes a MODL's meshes into Bevy meshes (any thread).
pub fn build_model_cpu(pkgs: &PackageSet, modl: ResourceKey) -> Vec<CpuPart> {
    let meshes = model::load_model(pkgs, &modl).unwrap_or_default();
    let mut parts = Vec::new();
    for m in &meshes {
        if m.indices.is_empty() || !bounds_ok(m) {
            continue;
        }
        let mat = &m.material;
        let mode: u8 = if mat.is_alpha_blended() {
            2
        } else if mat.is_alpha_tested() {
            1
        } else {
            0
        };
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone());
        mesh.insert_indices(Indices::U32(m.indices.clone()));
        parts.push(CpuPart {
            mesh,
            tex: diffuse_key(m),
            mode,
            bounds: (Vec3::from(m.bounds_min), Vec3::from(m.bounds_max)),
        });
    }
    parts
}

impl ObjectAssets {
    pub fn texture(&mut self, ctx: &mut AssetCtx, key: ResourceKey) -> Option<Handle<Image>> {
        if let Some(h) = self.textures.get(&key) {
            return h.clone();
        }
        let handle = build_texture_cpu(ctx.pkgs, key, 512).map(|img| ctx.images.add(img));
        self.textures.insert(key, handle.clone());
        handle
    }

    pub fn ingest_texture(&mut self, images: &mut Assets<Image>, key: ResourceKey, img: Option<Image>) {
        if !self.textures.contains_key(&key) {
            let h = img.map(|i| images.add(i));
            self.textures.insert(key, h);
        }
    }

    pub fn ingest_model(&mut self, ctx: &mut AssetCtx, modl: ResourceKey, cpu: Vec<CpuPart>) -> Vec<ModelPart> {
        let mut parts = Vec::new();
        for p in cpu {
            let material = self.material_for_key(ctx, p.tex, p.mode);
            parts.push(ModelPart { mesh: ctx.meshes.add(p.mesh), material, bounds: p.bounds });
        }
        self.models.insert(modl, parts.clone());
        parts
    }

    pub fn has_model(&self, modl: &ResourceKey) -> bool {
        self.models.contains_key(modl)
    }

    fn material_for_key(&mut self, ctx: &mut AssetCtx, tex_key: Option<ResourceKey>, mode: u8) -> Handle<StandardMaterial> {
        if let Some(h) = self.materials.get(&(tex_key, mode)) {
            return h.clone();
        }
        let tex = tex_key.and_then(|k| self.texture(ctx, k));
        let handle = ctx.materials.add(StandardMaterial {
            base_color: if tex.is_some() { Color::WHITE } else { Color::srgb(0.75, 0.75, 0.72) },
            base_color_texture: tex,
            perceptual_roughness: 0.7,
            reflectance: 0.3,
            alpha_mode: match mode {
                2 => AlphaMode::Blend,
                1 => AlphaMode::Mask(0.5),
                _ => AlphaMode::Opaque,
            },
            double_sided: mode != 0,
            cull_mode: if mode != 0 { None } else { Some(bevy::render::render_resource::Face::Back) },
            ..default()
        });
        self.materials.insert((tex_key, mode), handle.clone());
        handle
    }

    pub fn model(&mut self, ctx: &mut AssetCtx, modl: ResourceKey) -> Vec<ModelPart> {
        if let Some(p) = self.models.get(&modl) {
            return p.clone();
        }
        let cpu = build_model_cpu(ctx.pkgs, modl);
        self.ingest_model(ctx, modl, cpu)
    }

    /// All model parts of a catalog object (OBJD key).
    pub fn object(&mut self, ctx: &mut AssetCtx, objd: ResourceKey) -> Vec<ModelPart> {
        if let Some(p) = self.objects.get(&objd) {
            return p.clone();
        }
        let mut parts = Vec::new();
        for modl in s3formats::object::object_models(ctx.pkgs, &objd) {
            parts.extend(self.model(ctx, modl));
        }
        self.objects.insert(objd, parts.clone());
        parts
    }
}

/// Rejects meshes whose decoded positions don't match their stored bounds (unsupported encodings).
fn bounds_ok(m: &MeshData) -> bool {
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for p in &m.positions {
        for a in 0..3 {
            mn[a] = mn[a].min(p[a]);
            mx[a] = mx[a].max(p[a]);
        }
    }
    (0..3).all(|a| (mn[a] - m.bounds_min[a]).abs() < 0.05 && (mx[a] - m.bounds_max[a]).abs() < 0.05)
}

/// Spawns an object's meshes as children of a new entity.
pub fn spawn_parts(commands: &mut Commands, parts: &[ModelPart], transform: Transform) -> Entity {
    commands
        .spawn((transform, Visibility::default()))
        .with_children(|c| {
            for p in parts {
                c.spawn((Mesh3d(p.mesh.clone()), MeshMaterial3d(p.material.clone())));
            }
        })
        .id()
}

/// Combined local-space bounds of a set of parts.
pub fn parts_bounds(parts: &[ModelPart]) -> Option<(Vec3, Vec3)> {
    let mut it = parts.iter();
    let first = it.next()?;
    Some(it.fold(first.bounds, |(a, b), p| (a.min(p.bounds.0), b.max(p.bounds.1))))
}
