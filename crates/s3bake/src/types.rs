//! Data structures of the baked asset cache. Everything here is serialized with `postcard`.

use serde::{Deserialize, Serialize};

pub use s3formats::sim::{Clip, Rig};
pub use s3formats::world::{Heightmap, LotInfo};

/// Bump whenever any baked format changes; stale caches are rebuilt.
pub const BAKE_VERSION: u32 = 3;
/// Version of `world.bin` alone, so world-only changes don't force a global rebake.
pub const WORLD_VERSION: u32 = 7;

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

pub const ROOM_OUTSIDE: u8 = 0;
pub const ROOM_BATH: u8 = 1;
pub const ROOM_KITCHEN: u8 = 2;
pub const ROOM_BED: u8 = 3;
pub const ROOM_LIVING: u8 = 4;
pub const ROOM_PORCH: u8 = 5;

const fn txtc(g: u32, i: u64) -> Key {
    (0x033A1435, g, i)
}

/// Exterior wall styles (one is picked per lot).
pub const STYLE_EXTERIOR: [Key; 5] = [
    txtc(0x00D74299, 0x71B67700DCD90437), // Wall_Full_Siding
    txtc(0x00F0974C, 0xA9FC84C2F785667C), // Wall_Full_ShinglesShaker
    txtc(0x00F36FB7, 0x9B84A916F214B399), // Wall_Brick_Stretcher
    txtc(0x005DDCDB, 0x85E8AC046BB5C0F9), // Wall_Full_Stackstone
    txtc(0x00D268FC, 0xD4A569451BC53D37), // Wall_Structured_TimberPlanks
];
pub const STYLE_WALL_DADO: Key = txtc(0x0074783D, 0x711337AE3EE8A911);
pub const STYLE_WALL_MOULDING: Key = txtc(0x00C916CA, 0x0241D1C5E6BB47FC);
pub const STYLE_FOUNDATION: Key = txtc(0x0034D846, 0xD553F349EB51DA9C);
pub const STYLE_FLOOR_WOOD: Key = txtc(0x00D10E21, 0xE4D9401C5E2E03C1);
pub const STYLE_FLOOR_PARQUET: Key = txtc(0x0084FCE7, 0x2134A0DC5CF6E85A);
pub const STYLE_FLOOR_TERRACOTTA: Key = txtc(0x003274D4, 0xA4238A6F01598F18);
pub const STYLE_FLOOR_BATH: Key = txtc(0x00ED60C7, 0x75F11D379B9BE092);
pub const STYLE_FLOOR_CARPET: Key = txtc(0x0035D0BA, 0xEE7E52B9B0B42003);
pub const STYLE_FLOOR_DECK: Key = txtc(0x0049B5A2, 0xBBBB05A3610A7C7C);

pub const BUILD_STYLES: [Key; 14] = [
    STYLE_EXTERIOR[0],
    STYLE_EXTERIOR[1],
    STYLE_EXTERIOR[2],
    STYLE_EXTERIOR[3],
    STYLE_EXTERIOR[4],
    STYLE_WALL_DADO,
    STYLE_WALL_MOULDING,
    STYLE_FOUNDATION,
    STYLE_FLOOR_WOOD,
    STYLE_FLOOR_PARQUET,
    STYLE_FLOOR_TERRACOTTA,
    STYLE_FLOOR_BATH,
    STYLE_FLOOR_CARPET,
    STYLE_FLOOR_DECK,
];

/// A wall segment in lot-local tile coordinates, with the kind of room on each side
/// (left = the +normal side, normal = (-dz, dx) of a->b).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WallBaked {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub level: u8,
    pub left: u8,
    pub right: u8,
}

/// Floor triangles of one tile (bit 0 = -Z, 1 = +X, 2 = +Z, 3 = -X triangle).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct FloorBaked {
    pub level: u8,
    pub x: u16,
    pub z: u16,
    pub mask: u8,
    pub kind: u8,
    pub region: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LotObjectBaked {
    pub objd: Key,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub script: String,
    pub level: u8,
    /// Position in lot-local tile coordinates.
    pub local: [f32; 2],
}

/// A pre-built house: walls, floors and furniture of one lot.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LotBuildingBaked {
    /// Index into `WorldBaked::lots`.
    pub lot: u32,
    pub width: u32,
    pub depth: u32,
    /// Floor height of each level (0 = ground under the foundation).
    pub levels: Vec<f32>,
    pub walls: Vec<WallBaked>,
    pub floors: Vec<FloorBaked>,
    /// Foundation outline edges (lot-local).
    pub foundation: Vec<([f32; 2], [f32; 2])>,
    pub objects: Vec<LotObjectBaked>,
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
    pub buildings: Vec<LotBuildingBaked>,
}
