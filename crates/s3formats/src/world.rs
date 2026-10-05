//! Sims 3 `.world` resources: terrain heightmap and paint layers.

use crate::util::{R, Reader};
use s3pkg::{Package, ResourceKey};

pub const T_HEIGHTMAP: u32 = 0x2AD195F2;
pub const T_TERRAIN_PAINT: u32 = 0x9063660D;
pub const T_TERRAIN_BLEND: u32 = 0x3D8632D0;
pub const T_LOT_INFO: u32 = 0xD063545B;
pub const T_LOT_THUMB: u32 = 0xD84E7FC6;
pub const T_ROAD_GRAPH: u32 = 0x9063660E;

/// The world's road graph (0x9063660E): intersections, and the roads and walkways between them
/// as cubic beziers (four control points, x/z).
#[derive(Clone, Debug, Default)]
pub struct RoadGraph {
    pub road_intersections: Vec<[f32; 3]>,
    pub road_curves: Vec<[[f32; 2]; 4]>,
    pub walk_curves: Vec<[[f32; 2]; 4]>,
}

impl RoadGraph {
    /// `u32 version; u16 road ints, walk ints, road curves, walk curves`; each intersection
    /// `f32 x, z, angle; 10 bytes; u8 n; n × 4 bytes`; each curve `f32 bezier[8]; 8 bytes; u8 n;
    /// n × 4 bytes`.
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let _version = r.u32()?;
        let (ri, wi, rc, wc) = (r.u16()? as usize, r.u16()? as usize, r.u16()? as usize, r.u16()? as usize);
        let mut out = Self::default();
        for i in 0..ri + wi {
            let (x, z, angle) = (r.f32()?, r.f32()?, r.f32()?);
            r.skip(10)?;
            let n = r.u8()? as usize;
            r.skip(n * 4)?;
            if i < ri {
                out.road_intersections.push([x, z, angle]);
            }
        }
        for i in 0..rc + wc {
            let mut b = [[0f32; 2]; 4];
            for p in &mut b {
                *p = [r.f32()?, r.f32()?];
            }
            r.skip(8)?;
            let n = r.u8()? as usize;
            r.skip(n * 4)?;
            if i < rc {
                out.road_curves.push(b);
            } else {
                out.walk_curves.push(b);
            }
        }
        Ok(out)
    }
}

/// A lot placed in the world. `corner` is the lot origin; the lot extends `width` metres along
/// its local +X and `depth` metres along local +Z after rotating by `rotation` radians about +Y.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LotInfo {
    pub id: u64,
    pub internal_name: String,
    pub corner: [f32; 3],
    pub rotation: f32,
    pub width: u32,
    pub depth: u32,
    /// STBL key strings found in the record, e.g. "World/SV/HouseName:Goth".
    pub string_keys: Vec<String>,
}

impl LotInfo {
    pub fn parse(id: u64, d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let _ver = r.u16()?;
        r.skip(24)?;
        let internal_name = r.utf16_u32()?;
        r.skip(8)?;
        let corner = r.vec3()?;
        let rotation = r.f32()?;
        let width = r.u32()?;
        let depth = r.u32()?;
        // Scan the remainder for length-prefixed UTF-16 "World/..." keys.
        let mut string_keys = Vec::new();
        let mut p = r.pos;
        while p + 8 < d.len() {
            let n = u32::from_le_bytes(d[p..p + 4].try_into().unwrap()) as usize;
            if (6..200).contains(&n) && p + 4 + n * 2 <= d.len() && &d[p + 4..p + 14] == "World".encode_utf16().flat_map(|c| c.to_le_bytes()).collect::<Vec<u8>>().as_slice() {
                let mut q = Reader::at(d, p);
                if let Ok(s) = q.utf16_u32() {
                    string_keys.push(s);
                    p = q.pos;
                    continue;
                }
            }
            p += 1;
        }
        Ok(Self { id, internal_name, corner, rotation, width, depth, string_keys })
    }

    pub fn is_residential(&self) -> bool {
        let name = self.internal_name.to_ascii_lowercase();
        self.string_keys.iter().any(|k| k.contains("HouseName"))
            || name.contains("empty")
            || name.starts_with("res_")
            || name.starts_with("res ")
            || name.contains("residential")
    }

    pub fn name_key(&self) -> Option<&str> {
        self.string_keys
            .iter()
            .find(|k| k.contains("HouseName") || k.contains("LotName"))
            .map(|s| s.as_str())
    }

    pub fn address_key(&self) -> Option<&str> {
        self.string_keys.iter().find(|k| k.contains("LotAddress")).map(|s| s.as_str())
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Heightmap {
    pub width: usize,
    pub height: usize,
    pub scale: f32,
    pub data: Vec<u16>,
}

impl Heightmap {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let width = r.u32()? as usize;
        let height = r.u32()? as usize;
        let scale = r.f32()?;
        let _unk = r.f32()?;
        let _unk2 = r.u32()?;
        let raw = r.bytes(width * height * 2)?;
        let data = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(Self { width, height, scale, data })
    }

    /// Height in metres at integer grid coordinates (clamped).
    pub fn at(&self, x: i64, z: i64) -> f32 {
        let x = x.clamp(0, self.width as i64 - 1) as usize;
        let z = z.clamp(0, self.height as i64 - 1) as usize;
        self.data[z * self.width + x] as f32 * self.scale
    }

    /// Bilinearly interpolated height at world position (x, z) in metres.
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        let x0 = x.floor();
        let z0 = z.floor();
        let fx = x - x0;
        let fz = z - z0;
        let (xi, zi) = (x0 as i64, z0 as i64);
        let h00 = self.at(xi, zi);
        let h10 = self.at(xi + 1, zi);
        let h01 = self.at(xi, zi + 1);
        let h11 = self.at(xi + 1, zi + 1);
        let a = h00 + (h10 - h00) * fx;
        let b = h01 + (h11 - h01) * fx;
        a + (b - a) * fz
    }

    pub fn normal(&self, x: i64, z: i64) -> [f32; 3] {
        let dx = self.at(x + 1, z) - self.at(x - 1, z);
        let dz = self.at(x, z + 1) - self.at(x, z - 1);
        let n = [-dx, 2.0, -dz];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        [n[0] / l, n[1] / l, n[2] / l]
    }
}

#[derive(Clone, Debug)]
pub struct TerrainLayer {
    pub texture: ResourceKey,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct PaintTile {
    pub x0: u32,
    pub z0: u32,
    pub w: u32,
    pub h: u32,
    pub layers: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct TerrainPaint {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<TerrainLayer>,
    pub tiles: Vec<PaintTile>,
}

impl TerrainPaint {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let _version = r.u32()?;
        let _u0 = r.u16()?;
        let width = r.u32()?;
        let height = r.u32()?;
        let _u1 = r.u32()?;
        let _u2 = r.u32()?;
        let n = r.u32()? as usize;
        let mut layers = Vec::with_capacity(n);
        for _ in 0..n {
            let t = r.u32()?;
            let g = r.u32()?;
            let i = r.u64()?;
            let name = r.utf16_u32()?;
            layers.push(TerrainLayer { texture: ResourceKey::new(t, g, i), name });
        }
        // Tile records vary between game versions; they're optional for rendering.
        let tiles = Self::parse_tiles(&mut r).unwrap_or_default();
        Ok(Self { width, height, layers, tiles })
    }

    fn parse_tiles(r: &mut Reader) -> R<Vec<PaintTile>> {
        let nt = r.u32()? as usize;
        if nt > 4096 {
            return Err(crate::util::Eof);
        }
        let mut tiles = Vec::with_capacity(nt);
        for _ in 0..nt {
            let size = r.u32()? as usize;
            let start = r.pos;
            let x0 = r.u32()?;
            let z0 = r.u32()?;
            let w = r.u32()?;
            let h = r.u32()?;
            let _cx = r.f32()?;
            let _cz = r.f32()?;
            let nl = r.u32()? as usize;
            let tl = r.bytes(nl.min(64))?.to_vec();
            tiles.push(PaintTile { x0, z0, w, h, layers: tl });
            r.pos = start + size;
        }
        Ok(tiles)
    }
}

/// Tiled 8-bit mask layers (magic `00 03 FF A5`), used for terrain paint blend weights.
///
/// Layout: magic, u32 width, u32 height, u32 tile_w, u32 tile_h, u32 layer_count, then per layer
/// (4-byte aligned, sometimes preceded by an extra u32): u32 size (bytes from the count field),
/// u32 tile_count, u8 layer_index, then tile_count descriptors of `u16 BE tile index, u8 kind`
/// where kind 1 = constant tile followed by one value byte and kind 0 = raw tile_w*tile_h bytes.
/// Tiles not listed are zero.
pub struct MaskLayers {
    pub width: usize,
    pub height: usize,
    pub layers: Vec<Vec<u8>>,
}

impl MaskLayers {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let magic = r.u32()?;
        if magic != 0xA5FF0300 {
            return Err(crate::util::Eof);
        }
        let width = r.u32()? as usize;
        let height = r.u32()? as usize;
        let tw = r.u32()? as usize;
        let th = r.u32()? as usize;
        let n = r.u32()? as usize;
        let tiles_x = width / tw;
        let total_tiles = tiles_x * (height / th);
        let mut layers = Vec::with_capacity(n);
        let mut p = r.pos;
        for li in 0..n {
            // Find the layer header: aligned, plausible counts, matching index.
            let mut found = None;
            for _ in 0..4 {
                p = (p + 3) & !3;
                let mut h = Reader::at(d, p);
                let size = h.u32()? as usize;
                let count = h.u32()? as usize;
                let idx = h.u8()? as usize;
                if count <= total_tiles && size <= d.len() - p && idx == li {
                    found = Some(count);
                    p = h.pos;
                    break;
                }
                p += 4;
            }
            let Some(count) = found else { break };
            let mut m = vec![0u8; width * height];
            let mut r = Reader::at(d, p);
            for _ in 0..count {
                let hi = r.u8()? as usize;
                let lo = r.u8()? as usize;
                let idx = (hi << 8) | lo;
                let kind = r.u8()?;
                let (ty, tx) = (idx / tiles_x, idx % tiles_x);
                if kind == 1 {
                    let v = r.u8()?;
                    for y in 0..th {
                        let row = (ty * th + y) * width + tx * tw;
                        m[row..row + tw].fill(v);
                    }
                } else {
                    let raw = r.bytes(tw * th)?;
                    for y in 0..th {
                        let row = (ty * th + y) * width + tx * tw;
                        m[row..row + tw].copy_from_slice(&raw[y * tw..(y + 1) * tw]);
                    }
                }
            }
            p = r.pos;
            layers.push(m);
        }
        Ok(Self { width, height, layers })
    }
}

/// Everything we currently understand about a world file.
pub struct WorldData {
    pub heightmap: Heightmap,
    pub paint: Option<TerrainPaint>,
    pub blend: Option<MaskLayers>,
    pub lots: Vec<LotInfo>,
}

impl WorldData {
    pub fn load(pkg: &Package) -> Result<Self, String> {
        let read = |t: u32| -> Option<Vec<u8>> {
            let e = pkg.of_type(t).next()?;
            pkg.read(e).ok()
        };
        let hm = read(T_HEIGHTMAP).ok_or("world has no heightmap")?;
        let heightmap = Heightmap::parse(&hm).map_err(|e| e.to_string())?;
        let paint = read(T_TERRAIN_PAINT).and_then(|d| TerrainPaint::parse(&d).ok());
        let blend = read(T_TERRAIN_BLEND).and_then(|d| MaskLayers::parse(&d).ok());
        let mut lots: Vec<LotInfo> = pkg
            .of_type(T_LOT_INFO)
            .filter_map(|e| LotInfo::parse(e.key.i, &pkg.read(e).ok()?).ok())
            .collect();
        lots.sort_by(|a, b| a.internal_name.cmp(&b.internal_name));
        Ok(Self { heightmap, paint, blend, lots })
    }
}
