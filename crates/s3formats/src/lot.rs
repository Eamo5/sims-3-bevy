//! Lot buildings: the wall / room graphs (0x312E7545) and roofs (0x11E32896) stored per lot in
//! a world package. Coordinates are lot-local tiles (1 m), +X along the lot width and +Z along
//! its depth; levels are 0 = foundation, 1 = ground floor, 2 = first floor, ...

use std::collections::HashMap;

use crate::util::{Eof, R, Reader};
use s3pkg::{Package, ResourceKey};

pub const T_WALL_GRAPH: u32 = 0x312E7545;
pub const T_ROOFS: u32 = 0x11E32896;
/// Group of the graph holding the actual walls.
pub const G_WALLS: u32 = 0x002E7B1A;
/// Group of the graph holding every room boundary (walls, fences, foundation and floor edges).
pub const G_ROOMS: u32 = 0x002E7B1C;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphVertex {
    pub x: f32,
    pub z: f32,
    pub level: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct GraphEdge {
    pub id: u32,
    pub a: u32,
    pub b: u32,
    /// Room ids on either side (0 = outdoors).
    pub left: u32,
    pub right: u32,
}

#[derive(Clone, Debug, Default)]
pub struct WallGraph {
    pub width: u32,
    pub depth: u32,
    pub vertices: HashMap<u32, GraphVertex>,
    pub rooms: Vec<u32>,
    pub edges: Vec<GraphEdge>,
}

impl WallGraph {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let _ver = r.u32()?;
        r.u32()?;
        let width = r.u32()?;
        let depth = r.u32()?;
        r.u32()?;
        let nv = r.u32()? as usize;
        r.u32()?;
        r.u32()?;
        if nv > 100_000 {
            return Err(Eof);
        }
        let mut vertices = HashMap::with_capacity(nv);
        for _ in 0..nv {
            let id = r.u32()?;
            let x = r.f32()?;
            let z = r.f32()?;
            let level = r.u32()?;
            vertices.insert(id, GraphVertex { x, z, level });
        }
        let nr = r.u32()? as usize;
        if nr > 100_000 {
            return Err(Eof);
        }
        let rooms = (0..nr).map(|_| r.u32()).collect::<R<Vec<_>>>()?;
        let ne = r.u32()? as usize;
        if ne > 100_000 {
            return Err(Eof);
        }
        let mut edges = Vec::with_capacity(ne);
        for _ in 0..ne {
            let id = r.u32()?;
            let a = r.u32()?;
            let left = r.u32()?;
            let b = r.u32()?;
            let right = r.u32()?;
            edges.push(GraphEdge { id, a, b, left, right });
        }
        Ok(Self { width, depth, vertices, rooms, edges })
    }

    /// Edges as coordinate pairs with their level (the level of the first vertex).
    pub fn segments(&self) -> impl Iterator<Item = ([f32; 2], [f32; 2], u32, &GraphEdge)> + '_ {
        self.edges.iter().filter_map(|e| {
            let (a, b) = (self.vertices.get(&e.a)?, self.vertices.get(&e.b)?);
            Some(([a.x, a.z], [b.x, b.z], a.level, e))
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RoofRect {
    pub style: u32,
    pub slope: f32,
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub level: u32,
}

/// Parses the lot's roofs: rectangles (two opposite corners) at a level with a slope.
/// Layout: header (id, slope, style of the roof tool), a count, then per roof
/// `[u32 style][corner][corner][u16 id][f32 slope]`.
pub fn parse_roofs(d: &[u8]) -> R<Vec<RoofRect>> {
    let mut r = Reader::new(d);
    let _ver = r.u32()?;
    r.u16()?;
    r.u16()?;
    r.f32()?;
    r.u32()?;
    r.u8()?;
    let n = r.u32()? as usize;
    if n > 1000 {
        return Err(Eof);
    }
    let coord_ok = |v: f32| v.is_finite() && (-1.0..=256.0).contains(&v) && (v == 0.0 || v.abs() > 1e-3);
    let rect_at = |p: usize| -> Option<(RoofRect, usize)> {
        let mut q = Reader::at(d, p);
        let (x1, z1, l1) = (q.f32().ok()?, q.f32().ok()?, q.u32().ok()?);
        let (x2, z2, l2) = (q.f32().ok()?, q.f32().ok()?, q.u32().ok()?);
        (coord_ok(x1) && coord_ok(z1) && coord_ok(x2) && coord_ok(z2) && l1 < 16 && l2 < 16).then_some((
            RoofRect { style: 0, slope: 0.0, a: [x1, z1], b: [x2, z2], level: l1.max(l2) },
            q.pos,
        ))
    };
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let style = r.u32()?;
        // Some packs store an extra byte before the corners.
        let (mut rect, end) = rect_at(r.pos).or_else(|| rect_at(r.pos + 1)).ok_or(Eof)?;
        r.pos = end;
        r.u16()?;
        rect.style = style;
        rect.slope = r.f32()?;
        out.push(rect);
    }
    Ok(out)
}

/// Everything about a lot's building found in the world package.
#[derive(Clone, Debug, Default)]
pub struct LotBuildData {
    pub walls: WallGraph,
    pub rooms: WallGraph,
    pub roofs: Vec<RoofRect>,
}

impl LotBuildData {
    pub fn load(pkg: &Package, lot_id: u64) -> Option<Self> {
        let read = |t: u32, g: u32| pkg.find(&ResourceKey::new(t, g, lot_id)).and_then(|e| pkg.read(e).ok());
        let walls = WallGraph::parse(&read(T_WALL_GRAPH, G_WALLS)?).ok()?;
        let rooms = read(T_WALL_GRAPH, G_ROOMS).and_then(|d| WallGraph::parse(&d).ok()).unwrap_or_default();
        let roofs = read(T_ROOFS, 0).and_then(|d| parse_roofs(&d).ok()).unwrap_or_default();
        Some(Self { walls, rooms, roofs })
    }

    pub fn is_empty(&self) -> bool {
        self.walls.edges.is_empty() && self.rooms.edges.is_empty()
    }
}

pub const T_LOT_GRID: u32 = 0xB125533A;
/// Grid channel holding each level's vertex heights.
pub const G_LEVEL_HEIGHTS: u32 = 0x0093D6D4;
pub const T_LOT_ARRAY: u32 = 0x05FF6BA4;
/// Array channel holding the water table.
pub const G_WATER_TABLE: u32 = 0x00FF2067;

/// A lot's own terrain. The world heightmap is flattened under lots; the lot keeps its
/// sculpted ground (pond basins and the like) as a vertex grid per level, heights relative to
/// the lot, and a water table: wherever the ground dips below it there is water (ponds).
/// Grids are `(width+1) × (depth+1)` vertices, x-major (`[x][z]`).
#[derive(Clone, Debug, Default)]
pub struct LotTerrain {
    pub nx: usize,
    pub nz: usize,
    /// Vertex heights of each level, lowest (basements) first.
    pub levels: Vec<Vec<f32>>,
    pub water: Option<Vec<f32>>,
}

impl LotTerrain {
    pub fn load(pkg: &Package, lot_id: u64) -> Option<Self> {
        let read = |t: u32, g: u32| pkg.find(&ResourceKey::new(t, g, lot_id)).and_then(|e| pkg.read(e).ok());
        let d = read(T_LOT_GRID, G_LEVEL_HEIGHTS)?;
        let mut r = Reader::new(&d);
        let _ver = r.u32().ok()?;
        let (nx, nz, n) = (r.u32().ok()? as usize, r.u32().ok()? as usize, r.u32().ok()? as usize);
        if nx * nz * n == 0 || nx > 600 || nz > 600 || n > 16 {
            return None;
        }
        let floats = |r: &mut Reader, k: usize| -> Option<Vec<f32>> { (0..k).map(|_| r.f32().ok()).collect() };
        let levels: Vec<Vec<f32>> = (0..n).map(|_| floats(&mut r, nx * nz)).collect::<Option<_>>()?;
        let water = read(T_LOT_ARRAY, G_WATER_TABLE).and_then(|d| {
            let mut r = Reader::new(&d);
            let _ver = r.u32().ok()?;
            let (wx, wz) = (r.u32().ok()? as usize, r.u32().ok()? as usize);
            (wx == nx && wz == nz).then(|| floats(&mut r, nx * nz)).flatten()
        });
        Some(Self { nx, nz, levels, water })
    }

    pub fn at(&self, level: usize, x: usize, z: usize) -> f32 {
        self.levels[level][x.min(self.nx - 1) * self.nz + z.min(self.nz - 1)]
    }
}

/// Which of a tile's four triangles (split along both diagonals) are covered by floor:
/// bit 0 = -Z side, bit 1 = +X side, bit 2 = +Z side, bit 3 = -X side.
#[derive(Clone, Copy, Debug)]
pub struct FloorCell {
    pub x: u32,
    pub z: u32,
    pub mask: u8,
    /// Region id (enclosed area), shared by every triangle of one room.
    pub region: u32,
}

/// Finds the floor at `level`: every tile triangle enclosed by the boundary edges given
/// (axis-aligned tile edges or tile diagonals), found by flooding the outside from the lot edge.
pub fn enclosed_floor(edges: &[([f32; 2], [f32; 2])], width: u32, depth: u32) -> Vec<FloorCell> {
    let (w, d) = (width as i64, depth as i64);
    if w <= 0 || d <= 0 || w > 512 || d > 512 {
        return Vec::new();
    }
    let idx = |x: i64, z: i64, t: usize| ((z * w + x) as usize) * 4 + t;
    // Blocked crossings: tile boundaries (horizontal / vertical unit edges) and in-tile diagonals.
    let mut h_block = vec![false; ((w + 1) * (d + 1)) as usize]; // edge (x,z)-(x+1,z)
    let mut v_block = vec![false; ((w + 1) * (d + 1)) as usize]; // edge (x,z)-(x,z+1)
    let mut diag_main = vec![false; (w * d) as usize]; // (x,z)-(x+1,z+1)
    let mut diag_anti = vec![false; (w * d) as usize]; // (x+1,z)-(x,z+1)
    for (a, b) in edges {
        let (ax, az, bx, bz) = (a[0].round() as i64, a[1].round() as i64, b[0].round() as i64, b[1].round() as i64);
        let (dx, dz) = (bx - ax, bz - az);
        let steps = dx.abs().max(dz.abs());
        if steps == 0 {
            continue;
        }
        let (sx, sz) = (dx.signum(), dz.signum());
        for k in 0..steps {
            let (x, z) = (ax + sx * k, az + sz * k);
            match (sx, sz) {
                (1, 0) | (-1, 0) => {
                    let x0 = if sx > 0 { x } else { x - 1 };
                    if (0..=w).contains(&x0) && (0..=d).contains(&z) {
                        h_block[(z * (w + 1) + x0) as usize] = true;
                    }
                }
                (0, 1) | (0, -1) => {
                    let z0 = if sz > 0 { z } else { z - 1 };
                    if (0..=w).contains(&x) && (0..=d).contains(&z0) {
                        v_block[(z0 * (w + 1) + x) as usize] = true;
                    }
                }
                _ => {
                    let (tx, tz) = (x.min(x + sx), z.min(z + sz));
                    if (0..w).contains(&tx) && (0..d).contains(&tz) {
                        if sx == sz {
                            diag_main[(tz * w + tx) as usize] = true;
                        } else {
                            diag_anti[(tz * w + tx) as usize] = true;
                        }
                    }
                }
            }
        }
    }
    // Triangles: 0 = -Z, 1 = +X, 2 = +Z, 3 = -X.
    let n = (w * d * 4) as usize;
    let mut region = vec![u32::MAX; n];
    let neighbours = |x: i64, z: i64, t: usize| -> Vec<(i64, i64, usize)> {
        let mut out = Vec::with_capacity(3);
        let main = diag_main[(z * w + x) as usize];
        let anti = diag_anti[(z * w + x) as usize];
        for u in 0..4usize {
            if u == t || (u + 2) % 4 == t {
                continue;
            }
            // Adjacent triangle pairs: (0,1) and (2,3) meet on the anti diagonal; (1,2) and (3,0)
            // on the main diagonal.
            let blocked = if matches!((t, u), (0, 1) | (1, 0) | (2, 3) | (3, 2)) { anti } else { main };
            if !blocked {
                out.push((x, z, u));
            }
        }
        // Across tile boundaries.
        match t {
            0 => {
                if z > 0 && !h_block[(z * (w + 1) + x) as usize] {
                    out.push((x, z - 1, 2));
                }
            }
            2 => {
                if z + 1 < d && !h_block[((z + 1) * (w + 1) + x) as usize] {
                    out.push((x, z + 1, 0));
                }
            }
            3 => {
                if x > 0 && !v_block[(z * (w + 1) + x) as usize] {
                    out.push((x - 1, z, 1));
                }
            }
            _ => {
                if x + 1 < w && !v_block[(z * (w + 1) + x + 1) as usize] {
                    out.push((x + 1, z, 3));
                }
            }
        }
        out
    };
    // Region 0: everything reachable from the lot border.
    let mut next_region = 0u32;
    let flood = |seeds: Vec<(i64, i64, usize)>, id: u32, region: &mut Vec<u32>| {
        let mut stack = seeds;
        while let Some((x, z, t)) = stack.pop() {
            let i = idx(x, z, t);
            if region[i] != u32::MAX {
                continue;
            }
            region[i] = id;
            stack.extend(neighbours(x, z, t));
        }
    };
    let mut border = Vec::new();
    for x in 0..w {
        if !h_block[x as usize] {
            border.push((x, 0, 0));
        }
        if !h_block[(d * (w + 1) + x) as usize] {
            border.push((x, d - 1, 2));
        }
    }
    for z in 0..d {
        if !v_block[(z * (w + 1)) as usize] {
            border.push((0, z, 3));
        }
        if !v_block[(z * (w + 1) + w) as usize] {
            border.push((w - 1, z, 1));
        }
    }
    flood(border, 0, &mut region);
    next_region += 1;
    for z in 0..d {
        for x in 0..w {
            for t in 0..4 {
                if region[idx(x, z, t)] == u32::MAX {
                    flood(vec![(x, z, t)], next_region, &mut region);
                    next_region += 1;
                }
            }
        }
    }
    let mut out = Vec::new();
    for z in 0..d {
        for x in 0..w {
            let mut mask = 0u8;
            let mut reg = 0;
            for t in 0..4 {
                let r = region[idx(x, z, t)];
                if r != 0 {
                    mask |= 1 << t;
                    reg = r;
                }
            }
            if mask != 0 {
                out.push(FloorCell { x: x as u32, z: z as u32, mask, region: reg });
            }
        }
    }
    out
}
