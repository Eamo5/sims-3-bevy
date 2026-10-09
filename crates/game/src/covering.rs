//! Wall/floor covering targets. Room fills follow actual wall segments, including diagonals,
//! rather than baked room ids that become stale after construction.

use std::collections::HashMap;
use bevy::prelude::*;
use s3bake::{Key, LotBuildingBaked};
use crate::building::PaintOp;

fn triangle(p: Vec2) -> (i32, i32, u8) {
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
    let fill = fill && !room.outdoors;
    b.floors.iter().filter(|f| f.level == level && (fill || f.x as i32 == x && f.z as i32 == z)).filter_map(|f| {
        let mask = room.masks.get(&(f.x as i32, f.z as i32)).copied().unwrap_or(0) & f.mask;
        let mask = (0..4).fold(0, |changed, t| {
            if mask & (1 << t) != 0 && b.covers.get(f.cover[t] as usize) != Some(&texture) { changed | (1 << t) } else { changed }
        });
        (mask != 0).then_some(PaintOp::FloorTriangles { level, x: f.x, z: f.z, mask, texture })
    }).collect()
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
        PaintOp::FloorTriangles { mask, .. } => mask.count_ones() as f32 * 0.25,
        PaintOp::Wall { wall, .. } => b.walls.get(wall as usize).map_or(0.0, |w| (Vec2::from(w.b) - Vec2::from(w.a)).abs().max_element()),
        _ => 0.0,
    }).sum();
    (units * price as f32).ceil() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
