//! A lot's wall and floor designs: which pattern and colours cover each wall side and each
//! floor quarter-tile.
//!
//! * Wall sides: `0xB1422971` channels hold, per edge of the `0x2E7B1E` graph, a palette id for
//!   each side; the `0xF12E5E12` palettes turn ids into indices of the lot's REFS table. Channel
//!   `0x002FDACF` lands on catalogue patterns (CWAL), channel `0x00DD33E4` on the lot's own
//!   designs (COMP).
//! * Floors: grid `0xB125533A:0x0093DEB7` holds four palette ids per tile (one per triangle),
//!   and `0x0553EAD4` maps them to (CWAL, TXTC, TXTC, COMP) REFS indices.
//! * Designs: `0x0563919E` holds the lot's COMP resources inline, keyed by their REFS index —
//!   each a complate in the catalogue's compact encoding plus the REFS indices its texture and
//!   pattern references resolve through.

use std::collections::HashMap;

use crate::catalog::Complate;
use crate::util::{Eof, R, Reader};

pub const T_WALL_SIDES: u32 = 0xB1422971;
pub const T_PALETTE: u32 = 0xF12E5E12;
pub const T_DESIGNS: u32 = 0x0563919E;
pub const T_FLOOR_PALETTE: u32 = 0x0553EAD4;
pub const T_GRID: u32 = 0xB125533A;

/// Wall channel and palette groups: catalogue patterns and the lot's designs.
pub const G_WALL_PATTERNS: u32 = 0x002FDACF;
pub const G_WALL_PATTERN_PALETTE: u32 = 0x002E7DF7;
pub const G_WALL_DESIGNS: u32 = 0x00DD33E4;
pub const G_WALL_DESIGN_PALETTE: u32 = 0x00DD3460;
pub const G_FLOOR_PALETTE: u32 = 0x0093D9D1;
pub const G_FLOOR_GRID: u32 = 0x0093DEB7;

/// One wall-graph edge's coverings: palette ids of its two sides (`None` = bare).
#[derive(Clone, Copy, Debug)]
pub struct WallSides {
    pub edge: u32,
    pub a: Option<u32>,
    pub b: Option<u32>,
}

pub fn parse_wall_sides(d: &[u8]) -> R<Vec<WallSides>> {
    let mut r = Reader::new(d);
    let _ver = r.u32()?;
    let n = r.u32()? as usize;
    if n > 100_000 {
        return Err(Eof);
    }
    let id = |v: u16| (v != 0xFFFF).then_some(v as u32);
    (0..n)
        .map(|_| {
            let edge = r.u32()?;
            let _style = r.u16()?;
            let (a, b) = (r.u16()?, r.u16()?);
            Ok(WallSides { edge, a: id(a), b: id(b) })
        })
        .collect()
}

/// Palette id → REFS index.
pub fn parse_palette(d: &[u8]) -> R<HashMap<u32, u16>> {
    let mut r = Reader::new(d);
    let _ver = r.u32()?;
    r.skip(16)?;
    let n = r.u32()? as usize;
    if n > 100_000 {
        return Err(Eof);
    }
    let mut out = HashMap::with_capacity(n);
    for _ in 0..n {
        let idx = r.u16()?;
        let id = r.u32()?;
        let _area = r.u32()?;
        out.insert(id, idx);
    }
    Ok(out)
}

/// Floor palette id → REFS indices of (pattern CWAL, design COMP).
pub fn parse_floor_palette(d: &[u8]) -> R<HashMap<u32, (u16, u16)>> {
    let mut r = Reader::new(d);
    let _ver = r.u32()?;
    r.skip(16)?;
    let n = r.u32()? as usize;
    if n > 100_000 {
        return Err(Eof);
    }
    let mut out = HashMap::with_capacity(n);
    for _ in 0..n {
        let cwal = r.u16()?;
        let _txtc = r.u16()?;
        let _txtc2 = r.u16()?;
        let comp = r.u16()?;
        let id = r.u32()?;
        let _area = r.u32()?;
        out.insert(id, (cwal, comp));
    }
    Ok(out)
}

/// A per-tile grid of the lot: `width × depth × levels` cells of `cell` bytes.
pub struct Grid<'a> {
    pub width: u32,
    pub depth: u32,
    pub levels: u32,
    cell: usize,
    data: &'a [u8],
}

impl<'a> Grid<'a> {
    pub fn parse(d: &'a [u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let _ver = r.u32()?;
        let (width, depth, levels) = (r.u32()?, r.u32()?, r.u32()?);
        let cells = (width * depth * levels) as usize;
        let data = &d[16.min(d.len())..];
        if cells == 0 || data.len() % cells != 0 {
            return Err(Eof);
        }
        Ok(Self { width, depth, levels, cell: data.len() / cells, data })
    }

    /// The four u16 values of a cell. Cells are stored x-major: `[level][x][z]`.
    pub fn quad(&self, level: u32, x: u32, z: u32) -> Option<[u16; 4]> {
        if x >= self.width || z >= self.depth || level >= self.levels || self.cell < 8 {
            return None;
        }
        let i = ((level * self.width + x) * self.depth + z) as usize * self.cell;
        let b = self.data.get(i..i + 8)?;
        Some([0, 1, 2, 3].map(|k| u16::from_le_bytes([b[k * 2], b[k * 2 + 1]])))
    }
}

/// One of the lot's designs: a complate and the REFS indices its references resolve through.
#[derive(Clone, Debug, Default)]
pub struct Design {
    pub complate: Complate,
    pub refs: Vec<u16>,
}

/// The lot's designs by REFS index.
pub fn parse_designs(d: &[u8]) -> HashMap<u16, Design> {
    let mut out = HashMap::new();
    let mut r = Reader::new(d);
    let (Ok(_ver), Ok(next)) = (r.u32(), r.u32()) else { return out };
    let next = next as usize;
    if next > d.len() {
        return out;
    }
    // The design section is the one whose records parse cleanly up to the next section.
    for o in 12..next.saturating_sub(4) {
        let n = u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as usize;
        if n == 0 || n > 4096 {
            continue;
        }
        // Quick check of the first record's material header (or an empty record).
        let first = o + 4;
        let Some(h) = d.get(first + 10..first + 16) else { continue };
        let ln = u32::from_le_bytes(h[0..4].try_into().unwrap());
        if !(ln == 0 || (h[4] == 0x42 && h[5] == 0 && ln > 16)) {
            continue;
        }
        if let Some((designs, end)) = parse_records(d, first, n)
            && end == next
        {
            return designs;
        }
    }
    out.clear();
    out
}

fn parse_records(d: &[u8], start: usize, n: usize) -> Option<(HashMap<u16, Design>, usize)> {
    let mut out = HashMap::new();
    let mut r = Reader::at(d, start);
    for _ in 0..n {
        let id = r.u16().ok()?;
        let _a = r.u32().ok()?;
        let _b = r.u32().ok()?;
        let len = r.u32().ok()? as usize;
        if len == 0 {
            continue;
        }
        let end = r.pos + len;
        if end > d.len() {
            return None;
        }
        let _unknown = r.u16().ok()?;
        let _rel = r.u32().ok()?;
        let _size = r.u32().ok()?;
        let complate = crate::catalog::read_complate(&mut r).ok()?;
        r.pos = end;
        let nr = r.u8().ok()? as usize;
        let refs = (0..nr).map(|_| r.u16()).collect::<R<Vec<_>>>().ok()?;
        out.insert(id, Design { complate, refs });
    }
    Some((out, r.pos))
}
