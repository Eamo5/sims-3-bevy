//! Terrain rendering from the world's heightmap and paint layers.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::VisibilityRange;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, TextureDataOrder, TextureDimension, TextureFormat, TextureViewDescriptor,
    TextureViewDimension,
};
use bevy::shader::ShaderRef;
use s3bake::{Heightmap, WorldBaked, WorldMap};

use crate::AppState;

const CHUNK: usize = 128;
/// Metres covered by one repeat of a terrain layer texture.
const LAYER_TILE_METRES: f32 = 5.0;

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExt>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExt {
    /// x: world size in metres, y: 1 / layer tile size, z: layer count.
    #[uniform(100)]
    pub params: Vec4,
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub layers: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    #[sampler(104)]
    pub weights: Handle<Image>,
    /// The game's own pre-composited terrain colour, used in the distance.
    #[texture(105)]
    #[sampler(106)]
    pub overview: Option<Handle<Image>>,
    /// a: baked tree shadows.
    #[texture(107)]
    #[sampler(108)]
    pub lightmap: Option<Handle<Image>>,
    /// Average linear colour of each paint layer (rgb).
    #[uniform(109)]
    pub layer_avg: [Vec4; 16],
    /// x: darkness (0 day .. 1 night) for the street-light glow.
    #[uniform(110)]
    pub night: Vec4,
}

/// The terrain material, for per-frame lighting updates.
#[derive(Resource)]
pub struct TerrainMaterialHandle(pub Handle<TerrainMaterial>);

fn terrain_night(night: Res<crate::clock::Night>, handle: Option<Res<TerrainMaterialHandle>>, mut mats: ResMut<Assets<TerrainMaterial>>) {
    let Some(h) = handle else { return };
    if !night.is_changed() {
        return;
    }
    if let Some(mut m) = mats.get_mut(&h.0) {
        m.extension.night = Vec4::new(night.0, 0.0, 0.0, 0.0);
    }
}

impl MaterialExtension for TerrainExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://sims3/shaders/terrain.wgsl".into()
    }
}

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/terrain.wgsl");
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .add_systems(OnEnter(AppState::InGame), spawn_terrain)
            .add_systems(Update, terrain_night.run_if(in_state(AppState::InGame)));
    }
}

pub struct ChunkMesh {
    pub mesh: Mesh,
    pub center: Vec3,
    pub lod: usize,
}

/// CPU-side terrain data produced on the loading thread.
#[derive(Resource)]
pub struct TerrainBuild {
    pub chunks: Vec<ChunkMesh>,
    pub layers: Option<Image>,
    pub weights: Image,
    pub layer_count: u32,
    pub world_size: f32,
    pub sea_level: f32,
    pub overview: Option<Image>,
    pub lightmap: Option<Image>,
    pub layer_avg: [Vec4; 16],
    /// The water of each pond.
    pub ponds: Vec<Mesh>,
    /// The heights, for the water's depth.
    pub heights: Image,
    pub height_scale: f32,
}

/// A pond's water surface: every lot-grid cell touching water, flat at its water level (the
/// basin's rising ground cuts the shoreline).
fn pond_mesh(p: &s3bake::PondBaked, lot: &s3bake::LotInfo) -> Mesh {
    let (s, c) = lot.rotation.sin_cos();
    let (nx, nz) = (p.nx as usize, p.nz as usize);
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for x in 0..nx.saturating_sub(1) {
        for z in 0..nz.saturating_sub(1) {
            let corners = [(x, z), (x + 1, z), (x + 1, z + 1), (x, z + 1)];
            let level = corners.iter().map(|&(a, b)| p.water[a * nz + b]).filter(|v| !v.is_nan()).fold(f32::NAN, f32::max);
            if level.is_nan() {
                continue;
            }
            let base = positions.len() as u32;
            for (a, b) in corners {
                let (lx, lz) = (a as f32, b as f32);
                positions.push([lot.corner[0] + lx * c + lz * s, level, lot.corner[2] - lx * s + lz * c]);
            }
            indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }
    let n = positions.len();
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p[0] * 0.25, p[2] * 0.25]).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// A stitched world map (block-compressed mip chain) as a GPU image.
fn world_map_image(m: &WorldMap, srgb: bool) -> Image {
    let mut img = Image::default();
    img.data = Some(m.data.clone());
    img.texture_descriptor.size = Extent3d { width: m.size, height: m.size, depth_or_array_layers: 1 };
    img.texture_descriptor.mip_level_count = m.mips;
    img.texture_descriptor.format = match (m.bc3, srgb) {
        (true, true) => TextureFormat::Bc3RgbaUnormSrgb,
        (true, false) => TextureFormat::Bc3RgbaUnorm,
        (false, true) => TextureFormat::Bc1RgbaUnormSrgb,
        (false, false) => TextureFormat::Bc1RgbaUnorm,
    };
    img.texture_descriptor.dimension = TextureDimension::D2;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    img.asset_usage = RenderAssetUsages::RENDER_WORLD;
    img
}

const LODS: [(usize, f32, f32); 3] = [(1, 0.0, 260.0), (4, 260.0, 900.0), (16, 900.0, 1e6)];

pub fn build_terrain(world: &WorldBaked) -> TerrainBuild {
    let hm = &world.heightmap;
    let cells = hm.width - 1;
    let n = cells / CHUNK;
    let tiles: Vec<(usize, usize, usize)> =
        (0..n).flat_map(|cz| (0..n).flat_map(move |cx| (0..LODS.len()).map(move |lod| (cx, cz, lod)))).collect();
    let chunks = crate::world::par_map(&tiles, |&(cx, cz, lod)| {
        let (mesh, center) = chunk_mesh(hm, cx * CHUNK, cz * CHUNK, LODS[lod].0);
        ChunkMesh { mesh, center, lod }
    });

    // Paint layers: a pre-baked BC3 texture array.
    let (lw, lh, lm, lc) = world.layer_dims;
    let layers = (lc > 0 && !world.layer_data.is_empty()).then(|| {
        let mut img = Image::default();
        img.data = Some(world.layer_data.clone());
        img.data_order = TextureDataOrder::LayerMajor;
        img.texture_descriptor.size = Extent3d { width: lw, height: lh, depth_or_array_layers: lc };
        img.texture_descriptor.mip_level_count = lm;
        img.texture_descriptor.format = TextureFormat::Bc3RgbaUnormSrgb;
        img.texture_descriptor.dimension = TextureDimension::D2;
        img.texture_descriptor.label = Some("terrain_layers");
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            anisotropy_clamp: 16,
            ..default()
        });
        img.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..default() });
        img.asset_usage = RenderAssetUsages::RENDER_WORLD;
        img
    });

    let size = world.weights_size;
    let wdata = lz4_flex::decompress_size_prepended(&world.weights_lz4)
        .ok()
        .filter(|d| d.len() == (size * size * 16) as usize)
        .unwrap_or_else(|| vec![0u8; (size * size * 16) as usize]);
    let mut weights = Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 4 },
        TextureDimension::D2,
        wdata,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    weights.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    weights.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..default() });

    TerrainBuild {
        chunks,
        layers,
        weights,
        layer_count: lc,
        world_size: cells as f32,
        sea_level: world.sea_level,
        overview: world.overview.as_ref().filter(|m| m.size as usize == cells).map(|m| world_map_image(m, true)),
        lightmap: world.lightmap.as_ref().filter(|m| m.size as usize == cells).map(|m| world_map_image(m, false)),
        layer_avg: std::array::from_fn(|i| world.layer_avg.get(i).map_or(Vec4::splat(0.2), |c| Vec3::from(*c).extend(1.0))),
        ponds: world.ponds.iter().filter_map(|p| Some(pond_mesh(p, &world.lots.get(p.lot as usize)?.info))).collect(),
        heights: crate::water::height_image(hm),
        height_scale: hm.scale,
    }
}

fn chunk_mesh(hm: &Heightmap, x0: usize, z0: usize, step: usize) -> (Mesh, Vec3) {
    let side = CHUNK / step + 1;
    let center = Vec3::new((x0 + CHUNK / 2) as f32, 0.0, (z0 + CHUNK / 2) as f32);
    let wsize = (hm.width - 1) as f32;
    let mut pos = Vec::with_capacity(side * side + side * 4);
    let mut nrm = Vec::with_capacity(pos.capacity());
    let mut uv = Vec::with_capacity(pos.capacity());
    for j in 0..side {
        for i in 0..side {
            let (x, z) = ((x0 + i * step) as i64, (z0 + j * step) as i64);
            let h = hm.at(x, z);
            pos.push([x as f32 - center.x, h, z as f32 - center.z]);
            nrm.push(hm.normal(x, z));
            uv.push([x as f32 / wsize, z as f32 / wsize]);
        }
    }
    let mut idx: Vec<u32> = Vec::with_capacity((side - 1) * (side - 1) * 6 + side * 24);
    for j in 0..side - 1 {
        for i in 0..side - 1 {
            let a = (j * side + i) as u32;
            let b = a + 1;
            let c = a + side as u32;
            let d = c + 1;
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // Skirts hide cracks between neighbouring chunks of different LOD.
    let skirt = 2.0 + step as f32 * 1.5;
    let mut edge = |ring: Vec<usize>| {
        let base = pos.len() as u32;
        for &v in &ring {
            let p = pos[v];
            pos.push([p[0], p[1] - skirt, p[2]]);
            nrm.push(nrm[v]);
            uv.push(uv[v]);
        }
        for k in 0..ring.len() - 1 {
            let (t0, t1) = (ring[k] as u32, ring[k + 1] as u32);
            let (b0, b1) = (base + k as u32, base + k as u32 + 1);
            idx.extend_from_slice(&[t0, b0, t1, t1, b0, b1, t0, t1, b0, t1, b1, b0]);
        }
    };
    edge((0..side).collect());
    edge((0..side).map(|i| (side - 1) * side + i).collect());
    edge((0..side).map(|j| j * side).collect());
    edge((0..side).map(|j| j * side + side - 1).collect());

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    (mesh, center)
}

#[derive(Component)]
pub struct TerrainChunk;

fn spawn_terrain(
    mut commands: Commands,
    mut build: ResMut<TerrainBuild>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut terrain_mats: ResMut<Assets<TerrainMaterial>>,
    mut water_mats: ResMut<Assets<crate::water::WaterMaterial>>,
) {
    let layers = images.add(build.layers.take().unwrap_or_else(|| {
        let mut img = Image::new(
            Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            TextureDimension::D2,
            vec![90, 120, 60, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        img.texture_view_descriptor = Some(TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..default()
        });
        img
    }));
    let weights = images.add(std::mem::take(&mut build.weights));
    let flags = build.overview.is_some() as u32 | (build.lightmap.is_some() as u32) << 1;
    let overview = build.overview.take().map(|i| images.add(i));
    let lightmap = build.lightmap.take().map(|i| images.add(i));
    let material = terrain_mats.add(TerrainMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.95,
            reflectance: 0.15,
            ..default()
        },
        extension: TerrainExt {
            params: Vec4::new(build.world_size, 1.0 / LAYER_TILE_METRES, build.layer_count as f32, flags as f32),
            layers,
            weights,
            overview,
            lightmap,
            layer_avg: build.layer_avg,
            night: Vec4::ZERO,
        },
    });
    commands.insert_resource(TerrainMaterialHandle(material.clone()));
    for c in build.chunks.drain(..) {
        let (_, start, end) = LODS[c.lod];
        let margin = 30.0;
        commands.spawn((
            TerrainChunk,
            Mesh3d(meshes.add(c.mesh)),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(c.center),
            VisibilityRange {
                start_margin: (start - margin).max(0.0)..start,
                end_margin: end..end + margin,
                use_aabb: false,
            },
            DespawnOnExit(AppState::InGame),
        ));
    }

    // The sea, out past the world's edge, and the ponds (greener, stiller).
    let world = build.world_size;
    let heights = images.add(std::mem::replace(&mut build.heights, Image::default()));
    let ripples = images.add(crate::water::ripple_image());
    let samples = world as u32 + 1;
    let sea = water_mats.add(crate::water::water_material(heights.clone(), ripples.clone(), samples, build.height_scale, false));
    let plane = meshes.add(Plane3d::default().mesh().size(world * 6.0, world * 6.0).subdivisions(64));
    crate::water::spawn_water(&mut commands, plane, sea, Transform::from_xyz(world * 0.5, build.sea_level, world * 0.5));
    let pond = water_mats.add(crate::water::water_material(heights, ripples, samples, build.height_scale, true));
    for m in build.ponds.drain(..) {
        crate::water::spawn_water(&mut commands, meshes.add(m), pond.clone(), Transform::default());
    }
}
