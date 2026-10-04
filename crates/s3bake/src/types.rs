//! Data structures of the baked asset cache. Everything here is serialized with `postcard`.

use serde::{Deserialize, Serialize};

pub use s3formats::sim::{Clip, Rig};
pub use s3formats::world::{Heightmap, LotInfo};

/// Bump whenever any baked format changes; stale caches are rebuilt.
pub const BAKE_VERSION: u32 = 3;
/// Version of `world.bin` alone, so world-only changes don't force a global rebake.
pub const WORLD_VERSION: u32 = 5;

/// A resource key `(type, group, instance)`.
pub type Key = (u32, u32, u64);

pub fn key_of(k: &s3pkg::ResourceKey) -> Key {
    (k.t, k.g, k.i)
}

pub fn rkey(k: Key) -> s3pkg::ResourceKey {
    s3pkg::ResourceKey::new(k.0, k.1, k.2)
}

/// One drawable mesh with its material, ready to upload.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BakedPart {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// Texture id (a file in the texture store), or none for untextured parts.
    pub texture: Option<Key>,
    /// 0 opaque, 1 alpha-tested, 2 alpha-blended.
    pub mode: u8,
    /// Pre-rendered (lot imposters): draw unlit.
    pub unlit: bool,
    pub bmin: [f32; 3],
    pub bmax: [f32; 3],
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BakedModel {
    pub parts: Vec<BakedPart>,
}

/// A catalog object (OBJD) with everything gameplay needs.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CatalogEntry {
    pub objd: Key,
    pub name: String,
    /// Catalog price, or -1 when the object isn't sold in buy mode.
    pub price: i32,
    pub script: String,
    pub instance_name: String,
    pub models: Vec<Key>,
}

/// A skinned body mesh (CAS GEOM) with joints already mapped to rig bone indices.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkinMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub shader: u32,
    /// The mesh's own texture (eyes, lashes), if any.
    pub texture: Option<Key>,
}

/// Index entry for a CAS part; meshes live in the CAS pack.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CasPartInfo {
    pub key: Key,
    pub name: String,
    pub clothing_type: u32,
    pub age_gender: u32,
    pub category: u32,
    /// Whether meshes and a layer texture were baked for this part.
    pub baked: bool,
    /// Composited clothing/hair layer (RGBA with coverage in alpha).
    pub layer: Option<Key>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CasPartMeshes {
    pub meshes: Vec<SkinMesh>,
}

/// Skin detail textures per age/gender and part (2 scalp, 4 face, 8 body), plus the tone ramp.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ToneBaked {
    pub textures: Vec<(u32, u32, Key)>,
    /// Ramp colours from light to dark (linear 0..1 RGB).
    pub ramp: Vec<[f32; 3]>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CasBaked {
    pub parts: Vec<CasPartInfo>,
    pub tone: ToneBaked,
    pub adult_rig: Option<Rig>,
    pub child_rig: Option<Rig>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GlobalManifest {
    pub version: u32,
    pub install_root: String,
    pub catalog_entries: usize,
    pub models: usize,
    pub textures: usize,
    pub cas_parts: usize,
    pub clips: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LotBaked {
    pub info: LotInfo,
    pub display_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstanceBaked {
    pub model: Key,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    /// Lot index for lot imposters.
    pub lot: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TreeBaked {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: f32,
    pub kind: u64,
}

/// One road / sidewalk / intersection mesh, already in world space.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RoadPart {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Base texture coordinates.
    pub uvs: Vec<[f32; 2]>,
    /// Overlay / opacity texture coordinates.
    pub uvs1: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub base: Option<Key>,
    /// Tire tracks, crosswalks, curb corners: blended over the base by their alpha.
    pub overlay: Option<Key>,
    /// Edge fade (red channel) for sidewalks and dirt roads.
    pub opacity: Option<Key>,
}

/// A world-sized texture stitched from the per-sector maps: block-compressed mip chain.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorldMap {
    pub size: u32,
    pub mips: u32,
    /// true: BC3 (16-byte blocks), false: BC1.
    pub bc3: bool,
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorldBaked {
    pub version: u32,
    pub name: String,
    pub heightmap: Heightmap,
    pub sea_level: f32,
    /// BC3 texture array: width, height, mip count, layer count, data (layer-major).
    pub layer_dims: (u32, u32, u32, u32),
    pub layer_data: Vec<u8>,
    /// Average linear colour of each paint layer.
    pub layer_avg: Vec<[f32; 3]>,
    /// LZ4-compressed RGBA8 blend weights: 4 array layers of `weights_size`² texels.
    pub weights_size: u32,
    pub weights_lz4: Vec<u8>,
    pub lots: Vec<LotBaked>,
    pub instances: Vec<InstanceBaked>,
    pub trees: Vec<TreeBaked>,
    pub roads: Vec<RoadPart>,
    /// The game's pre-composited terrain colour (roads, lots and shadows painted in), 1 texel per metre.
    pub overview: Option<WorldMap>,
    /// rgb: night-light glow, a: tree shadows; 1 texel per metre.
    pub lightmap: Option<WorldMap>,
}
