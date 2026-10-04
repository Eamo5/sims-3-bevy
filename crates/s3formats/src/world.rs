//! Sims 3 `.world` resources: terrain heightmap and paint layers.

use crate::util::{R, Reader};
use s3pkg::{Package, ResourceKey};

pub const T_HEIGHTMAP: u32 = 0x2AD195F2;
pub const T_TERRAIN_PAINT: u32 = 0x9063660D;
pub const T_TERRAIN_BLEND: u32 = 0x3D8632D0;

#[derive(Clone)]
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
        let nt = r.u32()? as usize;
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
            let tl = r.bytes(nl)?.to_vec();
            tiles.push(PaintTile { x0, z0, w, h, layers: tl });
            r.pos = start + size;
        }
        Ok(Self { width, height, layers, tiles })
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
        Ok(Self { heightmap, paint, blend })
    }
}
