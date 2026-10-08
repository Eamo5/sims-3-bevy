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
    /// x: darkness (0 day .. 1 night) for the street-light glow; y: snow on the ground, z: how
    /// wet it is, w: frost (0..1).
    #[uniform(110)]
    pub night: Vec4,
    /// Ground painted in build mode: r the paint layer (index / 15), g how much
    /// (`terrain_paint`).
    #[texture(111)]
    #[sampler(112)]
    pub paint: Handle<Image>,
}

/// The terrain material, for per-frame lighting updates.
#[derive(Resource)]
pub struct TerrainMaterialHandle(pub Handle<TerrainMaterial>);

/// The terrain's darkness (for the street lights' glow) and the seasons' ground cover: snow,
/// wetness, frost.
fn terrain_night(night: Res<crate::clock::Night>, weather: Option<Res<crate::weather::Weather>>, handle: Option<Res<TerrainMaterialHandle>>, mut mats: ResMut<Assets<TerrainMaterial>>) {
    let Some(h) = handle else { return };
    let cover = weather.as_ref().map_or(Vec3::ZERO, |w| Vec3::new(w.snow, w.wet, w.frost) / 100.0);
    let want = Vec4::new(night.0, cover.x, cover.y, cover.z);
    let Some(m) = mats.get(&h.0) else { return };
    if (m.extension.night - want).abs().max_element() < 0.004 {
        return;
    }
    if let Some(mut m) = mats.get_mut(&h.0) {
        m.extension.night = want;
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
            .add_systems(Update, dig_pools.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, terrain_night.run_if(in_state(AppState::InGame)));
    }
}

pub struct ChunkMesh {
    pub mesh: Mesh,
    pub center: Vec3,
    pub lod: usize,
    /// The chunk's first heightmap point.
    pub at: (usize, usize),
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
    /// Heightmap cells left open (over pools).
    pub holes: std::collections::HashSet<(i64, i64)>,
    /// The ground round each pool, by lot.
    pub collars: Vec<(usize, Mesh)>,
    /// A small picture of each paint layer (RGBA8, `SWATCH` square), for the build-mode palette.
    pub swatches: Vec<Vec<u8>>,
    /// The size of the weights map (texels across the world).
    pub weights_size: u32,
}

/// The side of a paint layer's swatch (texels).
pub const SWATCH: u32 = 32;

/// Each paint layer's mip of `SWATCH` texels, decoded.
fn layer_swatches(world: &WorldBaked) -> Vec<Vec<u8>> {
    let (w, h, mips, count) = world.layer_dims;
    if count == 0 || world.layer_data.is_empty() {
        return Vec::new();
    }
    let mip_bytes = |m: u32| (((w >> m).max(1) as usize).div_ceil(4)) * (((h >> m).max(1) as usize).div_ceil(4)) * 16;
    let layer_bytes: usize = (0..mips).map(mip_bytes).sum();
    let Some(m) = (0..mips).find(|m| (w >> m) <= SWATCH) else { return Vec::new() };
    let before: usize = (0..m).map(mip_bytes).sum();
    let (mw, mh) = ((w >> m).max(1) as usize, (h >> m).max(1) as usize);
    (0..count as usize)
        .filter_map(|l| {
            let at = l * layer_bytes + before;
            let rgba = s3bake::ddsw::decode_bc3(world.layer_data.get(at..at + mip_bytes(m))?, mw, mh);
            // (Scaled to the swatch size, should the mip be smaller.)
            let s = SWATCH as usize;
            Some((0..s * s).flat_map(|i| { let (x, y) = (i % s * mw / s, i / s * mh / s); let o = (y * mw + x) * 4; [rgba[o], rgba[o + 1], rgba[o + 2], 255] }).collect())
        })
        .collect()
}

/// The ground round a pool, laid along its lot's grid: the world's terrain is opened by whole
/// cells, which on a lot turned to the world overshoot the pool's edge; this covers the
/// overshoot, in the terrain's own material (it takes the same world-wide texture coordinates).
#[derive(Component)]
pub struct PoolCollar(pub usize);

/// How far round a pool its collar reaches (tiles).
const COLLAR: i32 = 2;

pub fn collar_mesh(hm: &Heightmap, lot: &s3bake::LotInfo, b: &s3bake::LotBuildingBaked) -> Option<Mesh> {
    let pool = &b.pool;
    if pool.is_empty() {
        return None;
    }
    let tiles: std::collections::HashSet<(i32, i32)> = pool.iter().map(|f| (f.x as i32, f.z as i32)).collect();
    let (s, c) = lot.rotation.sin_cos();
    let to_world = |x: f32, z: f32| (lot.corner[0] + x * c + z * s, lot.corner[2] - x * s + z * c);
    let wsize = (hm.width - 1) as f32;
    let mut ring = std::collections::BTreeSet::new();
    for &(x, z) in &tiles {
        for dz in -COLLAR..=COLLAR {
            for dx in -COLLAR..=COLLAR {
                let t = (x + dx, z + dz);
                if !tiles.contains(&t) {
                    ring.insert(t);
                }
            }
        }
    }
    let (mut pos, mut nrm, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    const SUB: usize = 4;
    for (x, z) in ring {
        let base = pos.len() as u32;
        for j in 0..=SUB {
            for i in 0..=SUB {
                let (lx, lz) = (x as f32 + i as f32 / SUB as f32, z as f32 + j as f32 / SUB as f32);
                let (wx, wz) = to_world(lx, lz);
                // (The lot's own ground, a whisker above the terrain it covers: the heights
                // round a pool are dug down a little past its edge.)
                pos.push([wx, b.ground_at(lx, lz).unwrap_or(b.levels[0]) + 0.03, wz]);
                nrm.push(hm.normal(wx.round() as i64, wz.round() as i64));
                uv.push([wx / wsize, wz / wsize]);
            }
        }
        let w = SUB as u32 + 1;
        for j in 0..SUB as u32 {
            for i in 0..SUB as u32 {
                let a = base + j * w + i;
                let (b, cc, d) = (a + 1, a + w, a + w + 1);
                idx.extend_from_slice(&[a, cc, b, b, cc, d]);
            }
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    // (Wound the other way round from the terrain's if the lot's grid is mirrored: never.)
    Some(mesh)
}

/// The heightmap cells the nearest terrain leaves open (over pools).
#[derive(Resource, Default)]
pub struct TerrainHoles(pub std::collections::HashSet<(i64, i64)>);

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
    // (The nearest detail leaves the ground open where pools are let into it.)
    let holes: std::collections::HashSet<(i64, i64)> = world.terrain_holes.iter().map(|c| (c[0] as i64, c[1] as i64)).collect();
    let chunks = crate::world::par_map(&tiles, |&(cx, cz, lod)| {
        let (mesh, center) = chunk_mesh(hm, cx * CHUNK, cz * CHUNK, LODS[lod].0, if lod == 0 { Some(&holes) } else { None });
        ChunkMesh { mesh, center, lod, at: (cx * CHUNK, cz * CHUNK) }
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
        swatches: layer_swatches(world),
        weights_size: size,
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
        holes,
        collars: world
            .buildings
            .iter()
            .filter(|b| !b.pool.is_empty())
            .filter_map(|b| Some((b.lot as usize, collar_mesh(hm, &world.lots.get(b.lot as usize)?.info, b)?)))
            .collect(),
    }
}

fn chunk_mesh(hm: &Heightmap, x0: usize, z0: usize, step: usize, holes: Option<&std::collections::HashSet<(i64, i64)>>) -> (Mesh, Vec3) {
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
            if holes.is_some_and(|h| h.contains(&((x0 + i * step) as i64, (z0 + j * step) as i64))) {
                continue;
            }
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

/// A piece of the terrain at one level of detail, from heightmap point `at`.
#[derive(Component)]
pub struct TerrainChunk {
    pub at: (usize, usize),
    pub lod: usize,
}

/// A pool dug (or filled in) in build mode: the nearest terrain opens over its cells and the
/// heights under it go down to its floor (keeping Sims out), or close up and come back to the
/// lot's ground; swimmers find it in the world's copy of the house.
#[allow(clippy::too_many_arguments)]
fn dig_pools(
    mut changed: MessageReader<crate::building::PoolChanged>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut world: ResMut<crate::loading::CurrentWorld>,
    mut holes: ResMut<TerrainHoles>,
    chunks: Query<(&TerrainChunk, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut grid: Option<ResMut<crate::nav::NavGrid>>,
    (mut commands, collars, material): (Commands, Query<(Entity, &PoolCollar)>, Option<Res<TerrainMaterialHandle>>),
    (time, mut last): (Res<Time>, Local<f32>),
) {
    // (Also now and then: a pool loaded from a save was dug before this ran.)
    let now_s = time.elapsed_secs();
    if changed.read().count() == 0 && now_s - *last < 1.0 {
        return;
    }
    *last = now_s;
    let Some(b) = building else { return };
    let Some(lot) = world.data.lots.get(b.lot).cloned() else { return };
    let (s, c) = lot.rotation.sin_cos();
    let to_world = |x: f32, z: f32| (lot.corner[0] + x * c + z * s, lot.corner[2] - x * s + z * c);
    // The lot's cells now under water, and those it had before (every hole inside the lot).
    // (Every cell a tile reaches into: the collar covers what they overshoot.)
    let mut now = std::collections::HashSet::new();
    for f in &b.data.pool {
        for j in 0..6 {
            for i in 0..6 {
                let (u, v) = (0.02 + i as f32 * 0.192, 0.02 + j as f32 * 0.192);
                let (wx, wz) = to_world(f.x as f32 + u, f.z as f32 + v);
                now.insert((wx.floor() as i64, wz.floor() as i64));
            }
        }
    }
    let inside = |(x, z): (i64, i64)| {
        let (wx, wz) = (x as f32 + 0.5 - lot.corner[0], z as f32 + 0.5 - lot.corner[2]);
        let (lx, lz) = (wx * c - wz * s, wx * s + wz * c);
        lx >= 0.0 && lz >= 0.0 && lx <= b.data.width as f32 && lz <= b.data.depth as f32
    };
    let before: std::collections::HashSet<(i64, i64)> = holes.0.iter().copied().filter(|h| inside(*h)).collect();
    debug!("pool: lot corner {:?} rotation {:.3}, {} cells (had {})", lot.corner, lot.rotation, now.len(), before.len());
    if before == now {
        return;
    }
    let touched: Vec<(i64, i64)> = before.symmetric_difference(&now).copied().collect();
    for h in &before {
        holes.0.remove(h);
    }
    holes.0.extend(now.iter().copied());
    // Heights: down to the pool's floor where all four cells round a point are pool, else
    // the lot's ground.
    let ground = b.data.levels[0];
    let floor_y = ground + b.data.pool_depth;
    let data = std::sync::Arc::make_mut(&mut world.data);
    let hm = &mut data.heightmap;
    for &(x, z) in &touched {
        for (px, pz) in [(x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1)] {
            if px < 0 || pz < 0 || px as usize >= hm.width || pz as usize >= hm.height {
                continue;
            }
            let pool = [(px - 1, pz - 1), (px, pz - 1), (px - 1, pz), (px, pz)].iter().all(|c| now.contains(c));
            let y = if pool { floor_y } else { ground };
            hm.data[pz as usize * hm.width + px as usize] = (y / hm.scale).round().clamp(0.0, 65535.0) as u16;
        }
    }
    data.buildings.insert(b.lot, b.data.clone());
    // The ground round the pool, laid anew.
    for (e, c) in &collars {
        if c.0 == b.lot {
            commands.entity(e).despawn();
        }
    }
    if let (Some(m), Some(mat)) = (collar_mesh(&data.heightmap, &lot, &b.data), material) {
        commands.spawn((PoolCollar(b.lot), Mesh3d(meshes.add(m)), MeshMaterial3d(mat.0.clone()), Transform::default(), DespawnOnExit(AppState::InGame)));
    }
    rebuild_chunks(&touched, &data.heightmap, &chunks, &mut meshes, &holes.0);
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
}

/// The nearest terrain chunks over these heightmap cells, rebuilt from the heights.
pub fn rebuild_chunks(
    cells: &[(i64, i64)],
    hm: &Heightmap,
    chunks: &Query<(&TerrainChunk, &Mesh3d)>,
    meshes: &mut Assets<Mesh>,
    holes: &std::collections::HashSet<(i64, i64)>,
) {
    for (chunk, mesh) in chunks {
        if chunk.lod != 0 {
            continue;
        }
        let (x0, z0) = (chunk.at.0 as i64, chunk.at.1 as i64);
        if cells.iter().any(|&(x, z)| x >= x0 - 1 && z >= z0 - 1 && x <= x0 + CHUNK as i64 && z <= z0 + CHUNK as i64) {
            let (m, _) = chunk_mesh(hm, chunk.at.0, chunk.at.1, LODS[0].0, Some(holes));
            let _ = meshes.insert(mesh.0.id(), m);
        }
    }
}

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
    // The ground painted in build mode, a texel to the weights' (nothing at first).
    let paint = images.add(crate::terrain_paint::blank(build.weights_size));
    commands.insert_resource(crate::terrain_paint::PaintMap { image: paint.clone(), size: build.weights_size, world_size: build.world_size });
    let swatches: Vec<Handle<Image>> = build.swatches.drain(..).map(|s| images.add(crate::terrain_paint::swatch_image(s))).collect();
    commands.insert_resource(crate::terrain_paint::Swatches(swatches));
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
            paint,
        },
    });
    commands.insert_resource(TerrainMaterialHandle(material.clone()));
    commands.insert_resource(TerrainHoles(std::mem::take(&mut build.holes)));
    for (lot, mesh) in build.collars.drain(..) {
        commands.spawn((PoolCollar(lot), Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), Transform::default(), DespawnOnExit(AppState::InGame)));
    }
    for c in build.chunks.drain(..) {
        let (_, start, end) = LODS[c.lod];
        let margin = 30.0;
        commands.spawn((
            TerrainChunk { at: c.at, lod: c.lod },
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
    let pond = water_mats.add(crate::water::water_material(heights.clone(), ripples.clone(), samples, build.height_scale, true));
    commands.insert_resource(crate::water::PoolWaterMaterial(water_mats.add(crate::water::pool_material(heights, ripples, samples, build.height_scale))));
    for m in build.ponds.drain(..) {
        crate::water::spawn_water(&mut commands, meshes.add(m), pond.clone(), Transform::default());
    }
}
