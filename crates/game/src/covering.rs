//! Wall/floor covering targets. Room fills follow actual wall segments, including diagonals,
//! rather than baked room ids that become stale after construction.

use std::collections::HashMap;
use bevy::prelude::*;
use s3bake::{Key, LotBuildingBaked};
use crate::building::PaintOp;

pub(crate) fn triangle(p: Vec2) -> (i32, i32, u8) {
    let tile = p.floor();
    let d = p - tile - Vec2::splat(0.5);
    let t = if d.y.abs() > d.x.abs() { if d.y < 0.0 { 0 } else { 2 } } else if d.x > 0.0 { 1 } else { 3 };
    (tile.x as i32, tile.y as i32, t)
}

fn center(x: i32, z: i32, t: u8) -> Vec2 {
    Vec2::new(x as f32 + 0.5, z as f32 + 0.5) + [Vec2::NEG_Y, Vec2::X, Vec2::Y, Vec2::NEG_X][t as usize] / 3.0
}

fn crosses(p: Vec2, q: Vec2, a: Vec2, b: Vec2) -> bool {
    let (r, s) = (q - p, b - a);
    let det = r.perp_dot(s);
    if det.abs() < 1e-6 { return false; }
    let t = (a - p).perp_dot(s) / det;
    let u = (a - p).perp_dot(r) / det;
    (0.0..=1.0).contains(&t) && (-1e-5..=1.00001).contains(&u)
}

struct Room {
    masks: HashMap<(i32, i32), u8>,
    outdoors: bool,
}

impl Room {
    fn contains(&self, p: Vec2) -> bool {
        let (x, z, t) = triangle(p);
        self.masks.get(&(x, z)).is_some_and(|mask| mask & (1 << t) != 0)
    }
}

fn room(b: &LotBuildingBaked, level: u8, at: Vec2) -> Room {
    let mut walls: HashMap<(i32, i32), Vec<(Vec2, Vec2)>> = HashMap::new();
    for wall in b.walls.iter().filter(|w| w.level.max(1) == level && w.a != w.b) {
        let (a, c) = (Vec2::from(wall.a), Vec2::from(wall.b));
        let (lo, hi) = (a.min(c).floor().as_ivec2(), a.max(c).floor().as_ivec2());
        for z in (lo.y - 1).max(0)..=hi.y.min(b.depth as i32 - 1) {
            for x in (lo.x - 1).max(0)..=hi.x.min(b.width as i32 - 1) {
                walls.entry((x, z)).or_default().push((a, c));
            }
        }
    }
    let mut out = Room { masks: HashMap::new(), outdoors: false };
    let start = triangle(at);
    if start.0 < 0 || start.1 < 0 || start.0 >= b.width as i32 || start.1 >= b.depth as i32 { return out; }
    let mut pending = vec![start];
    out.masks.insert((start.0, start.1), 1 << start.2);
    while let Some((x, z, t)) = pending.pop() {
        let neighbor = [(x, z - 1, 2), (x + 1, z, 3), (x, z + 1, 0), (x - 1, z, 1)][t as usize];
        for (nx, nz, nt) in [(x, z, (t + 1) % 4), (x, z, (t + 3) % 4), neighbor] {
            let (p, q) = (center(x, z, t), center(nx, nz, nt));
            if walls.get(&(x, z)).into_iter().flatten().any(|&(a, c)| crosses(p, q, a, c)) { continue; }
            if nx < 0 || nz < 0 || nx >= b.width as i32 || nz >= b.depth as i32 {
                out.outdoors = true;
                continue;
            }
            let mask = out.masks.entry((nx, nz)).or_default();
            if *mask & (1 << nt) != 0 { continue; }
            *mask |= 1 << nt;
            pending.push((nx, nz, nt));
        }
    }
    out
}

pub fn floors(b: &LotBuildingBaked, level: u8, at: Vec2, texture: Key, fill: bool) -> Vec<PaintOp> {
    let room = room(b, level, at);
    let (x, z, _) = triangle(at);
    let level = if level == 1 && !b.floors.iter().any(|f| f.level == 1 && f.x as i32 == x && f.z as i32 == z) { 0 } else { level };
    let fill = fill && !room.outdoors;
    b.floors.iter().filter(|f| f.level == level && (fill || f.x as i32 == x && f.z as i32 == z)).filter_map(|f| {
        let mask = room.masks.get(&(f.x as i32, f.z as i32)).copied().unwrap_or(0) & f.mask;
        let mask = (0..4).fold(0, |changed, t| {
            if mask & (1 << t) != 0 && b.covers.get(f.cover[t] as usize) != Some(&texture) { changed | (1 << t) } else { changed }
        });
        (mask != 0).then_some(PaintOp::FloorTriangles { level, x: f.x, z: f.z, mask, texture })
    }).collect()
}

#[cfg(test)]
fn paving(b: &LotBuildingBaked, at: Vec2, texture: Key, heights: [f32; 4]) -> Vec<PaintOp> {
    pave_room(b, at, texture, false, |_| heights)
}

/// Fill only a bounded enclosure. Outside it, even the room tool affects one tile.
pub fn pave_room(b: &LotBuildingBaked, at: Vec2, texture: Key, fill: bool, heights: impl Fn(Vec2) -> [f32; 4]) -> Vec<PaintOp> {
    let room = room(b, 1, at);
    let start = triangle(at);
    let fill = fill && !room.outdoors;
    let mut cells: Vec<_> = room.masks.into_iter().filter(|((x, z), _)| fill || (*x, *z) == (start.0, start.1)).collect();
    cells.sort_by_key(|((x, z), _)| (*z, *x));
    cells.into_iter().filter_map(|((x, z), mask)| pave_tile(b, x, z, mask, texture, &heights)).collect()
}

fn pave_tile(b: &LotBuildingBaked, x: i32, z: i32, mask: u8, texture: Key, heights: &impl Fn(Vec2) -> [f32; 4]) -> Option<PaintOp> {
    if x < 0 || z < 0 || x >= b.width as i32 || z >= b.depth as i32
        || b.pool.iter().any(|f| f.x as i32 == x && f.z as i32 == z)
        || b.floors.iter().any(|f| f.level == 1 && f.x as i32 == x && f.z as i32 == z)
    { return None; }
    let tile = b.floors.iter().find(|f| f.level == 0 && f.x as i32 == x && f.z as i32 == z);
    let changed = (0..4).fold(0, |changed, t| {
        let already = tile.is_some_and(|f| f.mask & (1 << t) != 0 && b.covers.get(f.cover[t] as usize) == Some(&texture));
        if mask & (1 << t) != 0 && !already { changed | (1 << t) } else { changed }
    });
    if changed == 0 { return None; }
    Some(PaintOp::Pave { x: x as u16, z: z as u16, mask: changed, texture, heights: heights(Vec2::new(x as f32, z as f32)) })
}

pub fn walls(b: &LotBuildingBaked, wall: u32, side: u8, texture: Key, fill: bool) -> Vec<PaintOp> {
    let Some(target) = b.walls.get(wall as usize) else { return Vec::new() };
    let sample = |w: &s3bake::WallBaked, side: u8| {
        let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
        (a + c) * 0.5 + (c - a).normalize_or_zero().perp() * if side == 0 { 0.12 } else { -0.12 }
    };
    let room = fill.then(|| room(b, target.level.max(1), sample(target, side)));
    b.walls.iter().enumerate().filter(|(_, w)| w.a != w.b && w.level.max(1) == target.level.max(1)).flat_map(|(i, w)| {
        let room = &room;
        (0..2).filter_map(move |s| {
            let selected = room.as_ref().map_or(i as u32 == wall && s == side, |r| r.contains(sample(w, s)));
            (selected && b.covers.get(w.cover[s as usize] as usize) != Some(&texture))
                .then_some(PaintOp::Wall { wall: i as u32, side: s, texture })
        })
    }).collect()
}

/// Charge for changed surface area, not for triangles already in the chosen design.
pub fn cost(b: &LotBuildingBaked, ops: &[PaintOp], price: u32) -> i64 {
    let units: f32 = ops.iter().map(|op| match *op {
        PaintOp::FloorTriangles { mask, .. } | PaintOp::Pave { mask, .. } => mask.count_ones() as f32 * 0.25,
        PaintOp::Wall { wall, .. } => b.walls.get(wall as usize).map_or(0.0, |w| (Vec2::from(w.b) - Vec2::from(w.a)).abs().max_element()),
        _ => 0.0,
    }).sum();
    (units * price as f32).ceil() as i64
}

/// Boundary segments of changed floor triangles, without internal spokes.
fn tile_edges(mask: u8) -> Vec<(Vec2, Vec2)> {
    let corners = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y];
    let mut edges = Vec::new();
    for t in 0..4 {
        if mask & (1 << t) == 0 { continue; }
        edges.push((corners[t], corners[(t + 1) % 4]));
        if mask & (1 << ((t + 3) % 4)) == 0 { edges.push((Vec2::splat(0.5), corners[t])); }
        if mask & (1 << ((t + 1) % 4)) == 0 { edges.push((corners[(t + 1) % 4], Vec2::splat(0.5))); }
    }
    edges
}

pub fn preview(gizmos: &mut Gizmos, b: &crate::building::ActiveBuilding, ops: &[PaintOp], affordable: bool) {
    let color = if affordable { Color::srgb(0.35, 1.0, 0.15) } else { Color::srgb(1.0, 0.15, 0.1) };
    for op in ops {
        let (x, z, mask, heights) = match *op {
            PaintOp::FloorTriangles { level, x, z, mask, .. } => {
                let y = b.data.floors.iter().find(|f| f.level == level && f.x == x && f.z == z).and_then(|f| f.y)
                    .unwrap_or_else(|| b.levels.get(level as usize).copied().unwrap_or(0.0));
                let heights = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y].map(|p| {
                    if level == 0 { b.data.ground_at(x as f32 + p.x, z as f32 + p.y).unwrap_or(y) } else { y }
                });
                (x, z, mask, heights)
            }
            PaintOp::Pave { x, z, mask, heights, .. } => (x, z, mask, heights),
            PaintOp::Wall { wall, side, .. } => {
                let Some(w) = b.data.walls.get(wall as usize) else { continue };
                let level = w.level.max(1) as usize;
                let y = b.levels.get(level).copied().unwrap_or(0.0);
                let top = b.levels.get(level + 1).copied().unwrap_or(y + 3.0);
                let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
                let offset = (c - a).normalize_or_zero().perp() * if side == 0 { 0.025 } else { -0.025 };
                let (a, c) = (a + offset, c + offset);
                let points = [b.world(a.x, a.y, y + 0.04), b.world(c.x, c.y, y + 0.04), b.world(c.x, c.y, top - 0.04), b.world(a.x, a.y, top - 0.04)];
                for i in 0..4 { gizmos.line(points[i], points[(i + 1) % 4], color); }
                continue;
            }
            _ => continue,
        };
        let point = |p: Vec2| {
            let near = heights[0] + (heights[1] - heights[0]) * p.x;
            let far = heights[3] + (heights[2] - heights[3]) * p.x;
            b.world(x as f32 + p.x, z as f32 + p.y, near + (far - near) * p.y + 0.05)
        };
        for (a, c) in tile_edges(mask) { gizmos.line(point(a), point(c), color); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_outlines_only_changed_triangle_boundaries() {
        assert!(tile_edges(0).is_empty());
        assert_eq!(tile_edges(1).len(), 3);
        assert_eq!(tile_edges(3).len(), 4);
        let full = tile_edges(15);
        assert_eq!(full.len(), 4);
        assert!(full.iter().all(|(a, b)| *a != Vec2::splat(0.5) && *b != Vec2::splat(0.5)));
        assert_eq!(tile_edges(5).len(), 6, "opposite changed triangles remain separate");
    }

    fn house() -> LotBuildingBaked {
        let mut b = LotBuildingBaked { lot: 0, width: 5, depth: 5, levels: vec![0.0, 0.0], walls: Vec::new(), floors: Vec::new(),
            foundation: Vec::new(), foundation_top: Vec::new(), objects: Vec::new(), covers: Vec::new(), ground: Vec::new(), pool: Vec::new(),
            pool_depth: 0.0, fences: Vec::new(), stairs: Vec::new() };
        for (a, c) in [([1.0, 1.0], [3.0, 1.0]), ([3.0, 1.0], [3.0, 3.0]), ([3.0, 3.0], [1.0, 3.0]), ([1.0, 3.0], [1.0, 1.0])] {
            crate::building::apply_paint(&mut b, &PaintOp::AddWall { a, b: c, level: 1 });
        }
        for z in 0..5 { for x in 0..5 {
            crate::building::apply_paint(&mut b, &PaintOp::AddFloor { level: 1, x, z, region: 99 });
        }}
        b
    }

    #[test]
    fn bare_room_fill_samples_each_tile_and_skips_existing_designs() {
        let mut b = house();
        b.floors.clear();
        let texture = (1, 2, 3);
        let heights = |at: Vec2| [at.x, at.x + 1.0, at.x + 1.0, at.x];
        let ops = pave_room(&b, Vec2::splat(1.5), texture, true, heights);
        assert_eq!(ops.len(), 4);
        assert_eq!(cost(&b, &ops, 8), 32);
        for op in &ops { crate::building::apply_paint(&mut b, op); }
        assert_eq!(b.ground_at(1.0, 1.0), Some(1.0));
        assert_eq!(b.ground_at(3.0, 3.0), Some(3.0));
        assert!(pave_room(&b, Vec2::splat(1.5), texture, true, heights).is_empty());
        assert_eq!(pave_room(&b, Vec2::splat(0.5), texture, true, heights).len(), 1);
    }

    #[test]
    fn bare_room_fill_respects_diagonals_and_pool_cells() {
        let mut b = house();
        b.floors.clear();
        crate::building::apply_paint(&mut b, &PaintOp::AddWall { a: [1.0, 1.0], b: [3.0, 3.0], level: 1 });
        let ops = pave_room(&b, Vec2::new(1.8, 1.2), (1, 2, 3), true, |_| [0.0; 4]);
        assert_eq!(ops.len(), 3);
        assert_eq!(cost(&b, &ops, 8), 16);
        assert!(ops.iter().any(|op| matches!(op, PaintOp::Pave { x: 1, z: 1, mask: 3, .. })));
        crate::building::apply_paint(&mut b, &PaintOp::AddPool { x: 2, z: 1 });
        let ops = pave_room(&b, Vec2::new(1.8, 1.2), (1, 2, 3), true, |_| [0.0; 4]);
        assert_eq!(cost(&b, &ops, 8), 8);
        assert!(!ops.iter().any(|op| matches!(op, PaintOp::Pave { x: 2, z: 1, .. })));
    }

    #[test]
    fn normal_click_covers_one_tile_shift_fills_only_the_enclosed_room() {
        let mut b = house();
        let texture = (1, 2, 3);
        assert_eq!(floors(&b, 1, Vec2::splat(1.5), texture, false).len(), 1);
        let ops = floors(&b, 1, Vec2::splat(1.5), texture, true);
        assert_eq!(ops.len(), 4, "stale shared room ids must not leak into the yard");
        for op in ops { crate::building::apply_paint(&mut b, &op); }
        assert!(floors(&b, 1, Vec2::splat(1.5), texture, true).is_empty(), "identical coverings cost nothing");
        assert_eq!(floors(&b, 1, Vec2::splat(0.5), texture, true).len(), 1, "Shift outdoors must not tile the entire lot");
    }

    #[test]
    fn diagonal_partition_limits_floor_triangles_and_wall_sides() {
        let mut b = house();
        crate::building::apply_paint(&mut b, &PaintOp::AddWall { a: [1.0, 1.0], b: [3.0, 3.0], level: 1 });
        let texture = (1, 2, 3);
        let ops = floors(&b, 1, Vec2::new(1.8, 1.2), texture, true);
        assert_eq!(ops.len(), 3);
        assert_eq!(cost(&b, &ops, 4), 8, "two full tiles of surface, spread over three cells");
        assert!(ops.iter().any(|op| matches!(op, PaintOp::FloorTriangles { x: 1, z: 1, mask: 3, .. })));
        for op in ops { crate::building::apply_paint(&mut b, &op); }
        let tile = b.floors.iter().find(|f| f.x == 1 && f.z == 1).unwrap();
        assert_eq!(tile.cover[0], tile.cover[1]);
        assert_ne!(tile.cover[1], tile.cover[2]);
        let ops = walls(&b, 0, 0, texture, true);
        assert_eq!(ops.len(), 3);
        assert!(ops.iter().any(|op| matches!(op, PaintOp::Wall { wall: 4, side: 1, .. })));
        assert!(!ops.iter().any(|op| matches!(op, PaintOp::Wall { wall: 4, side: 0, .. })));
    }

    #[test]
    fn room_wall_fill_keeps_the_other_side_and_other_floors_unchanged() {
        let mut b = house();
        crate::building::apply_paint(&mut b, &PaintOp::AddWall { a: [1.0, 1.0], b: [3.0, 1.0], level: 2 });
        let texture = (1, 2, 3);
        let ops = walls(&b, 0, 0, texture, true);
        assert_eq!(ops.len(), 4);
        assert_eq!(cost(&b, &ops, 3), 24);
        for op in ops { crate::building::apply_paint(&mut b, &op); }
        assert!(walls(&b, 0, 0, texture, true).is_empty());
        assert_eq!(b.walls[0].cover[1], s3bake::NO_COVER);
        assert_eq!(b.walls[4].cover, [s3bake::NO_COVER; 2]);
    }

    #[test]
    fn paving_follows_current_terrain_and_replays_from_a_saved_operation() {
        let mut b = house();
        b.floors.clear();
        let mut reloaded = b.clone();
        let heights = [4.0, 4.5, 5.0, 4.5];
        let texture = (1, 2, 3);
        let ops = paving(&b, Vec2::splat(0.4), texture, heights);
        assert_eq!(ops.len(), 1);
        assert_eq!(cost(&b, &ops, 8), 8);
        let saved = serde_json::to_string(&ops).unwrap();
        for op in &ops { crate::building::apply_paint(&mut b, op); }
        let restored: Vec<PaintOp> = serde_json::from_str(&saved).unwrap();
        for op in &restored { crate::building::apply_paint(&mut reloaded, op); }
        assert_eq!(serde_json::to_string(&b).unwrap(), serde_json::to_string(&reloaded).unwrap());
        assert_eq!(b.floors[0].level, 0);
        assert_eq!(b.ground_at(0.0, 0.0), Some(4.0));
        assert_eq!(b.ground_at(0.5, 0.5), Some(4.5));
        assert!(paving(&b, Vec2::splat(0.4), texture, heights).is_empty());
    }

    #[test]
    fn paving_cannot_cross_lot_boundaries_pools_or_existing_house_floors() {
        let mut b = house();
        let texture = (1, 2, 3);
        assert!(paving(&b, Vec2::splat(1.5), texture, [0.0; 4]).is_empty());
        b.floors.clear();
        crate::building::apply_paint(&mut b, &PaintOp::AddPool { x: 0, z: 0 });
        assert!(paving(&b, Vec2::splat(0.5), texture, [0.0; 4]).is_empty());
        assert!(paving(&b, Vec2::splat(-0.1), texture, [0.0; 4]).is_empty());
        assert!(paving(&b, Vec2::splat(5.1), texture, [0.0; 4]).is_empty());
    }
}
