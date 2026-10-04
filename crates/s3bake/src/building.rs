//! Lot buildings: walls, floors, foundations and furniture of the pre-built houses, converted
//! from the lot's wall/room graphs and placed objects into a compact baked form.

use std::collections::HashMap;

use s3formats::lot::{LotBuildData, enclosed_floor};
use s3formats::objn::PlacedObject;
use s3formats::world::LotInfo;
use s3pkg::Package;

use crate::types::*;

/// Height of one storey and of a foundation (metres).
pub const LEVEL_HEIGHT: f32 = 3.0;
pub const FOUNDATION_HEIGHT: f32 = 0.75;

const OBJD: u32 = 0x319E4F1D;

/// What a room is used for, judged by its furniture; picks floor and wall styles.
fn room_kind_of(script: &str) -> Option<u8> {
    let s = script.to_ascii_lowercase();
    if s.contains("toilet") || s.contains("shower") || s.contains("bathtub") || s.contains("sinkpedestal") {
        Some(ROOM_BATH)
    } else if s.contains("fridge") || s.contains("stove") || s.contains("counter") || s.contains("dishwasher") {
        Some(ROOM_KITCHEN)
    } else if s.contains(".beds.") || s.contains("crib") {
        Some(ROOM_BED)
    } else {
        None
    }
}

pub fn bake_building(pkg: &Package, lot_index: usize, lot: &LotInfo, objects: &[PlacedObject]) -> Option<LotBuildingBaked> {
    let data = LotBuildData::load(pkg, lot.id).unwrap_or_default();
    if data.walls.edges.is_empty() && objects.is_empty() {
        return None;
    }
    let (w, d) = (
        data.rooms.width.max(data.walls.width).max(lot.width + 1).saturating_sub(1),
        data.rooms.depth.max(data.walls.depth).max(lot.depth + 1).saturating_sub(1),
    );
    let has_foundation = data.rooms.segments().any(|s| s.2 == 0);
    let ground = lot.corner[1];
    let base = ground + if has_foundation { FOUNDATION_HEIGHT } else { 0.0 };
    let top_level = data
        .walls
        .vertices
        .values()
        .chain(data.rooms.vertices.values())
        .map(|v| v.level)
        .filter(|&l| l < 64)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut levels = vec![ground];
    for l in 1..=top_level + 1 {
        levels.push(base + (l - 1) as f32 * LEVEL_HEIGHT);
    }

    // World <-> lot-local transform: world = corner + rotY(rotation) * (x, 0, z).
    let (s, c) = lot.rotation.sin_cos();
    let to_local = |p: [f32; 3]| {
        let (dx, dz) = (p[0] - lot.corner[0], p[2] - lot.corner[2]);
        [dx * c - dz * s, dx * s + dz * c]
    };

    // Furniture. Objects slotted into another (chairs at tables, sinks in counters) carry no
    // position of their own: put them by their parent instead.
    let by_guid: HashMap<u64, &PlacedObject> = objects.iter().map(|o| (o.guid, o)).collect();
    let mut sibling_count: HashMap<u64, usize> = HashMap::new();
    let mut objs = Vec::new();
    for o in objects {
        let (Some(cat), Some(mut p)) = (o.catalog, o.position) else { continue };
        // Trees on the lot are drawn with the world's trees.
        if cat.t != OBJD || !o.trees.is_empty() || o.speedtree.is_some() {
            continue;
        }
        let mut rotation = o.rotation;
        if p == [0.0, 0.0, 0.0] {
            let Some(parent) = by_guid.get(&o.parent).filter(|_| o.parent != 0) else { continue };
            let Some(pp) = parent.position.filter(|pp| *pp != [0.0, 0.0, 0.0]) else { continue };
            let script = o.script.as_deref().unwrap_or("").to_ascii_lowercase();
            let [qx, qy, qz, qw] = parent.rotation;
            // Parent axes on the ground (rotation of +X and +Z by the parent quaternion).
            let rot = |v: [f32; 3]| -> [f32; 3] {
                let (x, y, z) = (v[0], v[1], v[2]);
                let (ix, iy, iz, iw) = (qw * x + qy * z - qz * y, qw * y + qz * x - qx * z, qw * z + qx * y - qy * x, -qx * x - qy * y - qz * z);
                [ix * qw + iw * -qx + iy * -qz - iz * -qy, iy * qw + iw * -qy + iz * -qx - ix * -qz, iz * qw + iw * -qz + ix * -qy - iy * -qx]
            };
            if script.contains("seating") || script.contains("chair") || script.contains("stool") {
                // Chairs go round the table: front, back, left, right.
                let k = sibling_count.entry(o.parent).or_default();
                let (off, face) = match *k % 4 {
                    0 => ([0.0, 0.0, -0.75], 0.0f32),
                    1 => ([0.0, 0.0, 0.75], std::f32::consts::PI),
                    2 => ([-0.75, 0.0, 0.0], std::f32::consts::FRAC_PI_2),
                    _ => ([0.75, 0.0, 0.0], -std::f32::consts::FRAC_PI_2),
                };
                *k += 1;
                let w = rot(off);
                p = [pp[0] + w[0], pp[1], pp[2] + w[2]];
                // Parent yaw plus the turn to face the table.
                let yaw = 2.0 * qy.atan2(qw) + face;
                rotation = [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()];
            } else {
                p = pp;
                rotation = parent.rotation;
            }
        }
        let mut level = if p[1] < base - 0.3 { 0 } else { (((p[1] - base) / LEVEL_HEIGHT).round() as i32 + 1).max(1) as u8 };
        // Ceiling lights hang from the floor above but light (and belong to) the room below.
        if level > 1 && o.script.as_deref().unwrap_or("").contains("LightingCeiling") {
            level -= 1;
        }
        objs.push(LotObjectBaked {
            objd: key_of(&cat),
            position: p,
            rotation,
            script: o.script.clone().unwrap_or_default(),
            level,
            local: to_local(p),
        });
    }

    // Floors: per level, the triangles enclosed by room boundaries (the foundation outline
    // counts for the ground floor). Rooms enclosed only by railings / foundation are porches.
    let mut floors = Vec::new();
    for level in 1..=top_level {
        let bounds: Vec<([f32; 2], [f32; 2])> = data
            .rooms
            .segments()
            .filter(|s| s.2 == level || (level == 1 && s.2 == 0))
            .map(|s| (s.0, s.1))
            .collect();
        let wall_edges: Vec<([f32; 2], [f32; 2])> = data.walls.segments().filter(|s| s.2 == level).map(|s| (s.0, s.1)).collect();
        let cells = enclosed_floor(&bounds, w, d);
        let inside = enclosed_floor(&wall_edges, w, d);
        let indoor: HashMap<(u32, u32), (u8, u32)> = inside.iter().map(|c| ((c.x, c.z), (c.mask, c.region))).collect();
        // Room kinds from the furniture standing in each indoor region.
        let mut kinds: HashMap<u32, u8> = HashMap::new();
        for o in objs.iter().filter(|o| o.level == level as u8) {
            let (x, z) = (o.local[0].floor() as i64, o.local[1].floor() as i64);
            if x < 0 || z < 0 {
                continue;
            }
            if let (Some(&(_, region)), Some(k)) = (indoor.get(&(x as u32, z as u32)), room_kind_of(&o.script)) {
                let e = kinds.entry(region).or_insert(k);
                // Bathrooms win over kitchens (sinks), kitchens over bedrooms.
                *e = (*e).min(k);
            }
        }
        for cell in cells {
            let (kind, region) = match indoor.get(&(cell.x, cell.z)) {
                Some(&(mask, region)) if mask & cell.mask != 0 => (kinds.get(&region).copied().unwrap_or(ROOM_LIVING), region),
                _ => (ROOM_PORCH, 0),
            };
            floors.push(FloorBaked { level: level as u8, x: cell.x as u16, z: cell.z as u16, mask: cell.mask, kind, region: region as u16 });
        }
    }

    // Walls with the kind of room on each side (outdoors when no indoor floor is there).
    let floor_at: HashMap<(u8, u16, u16), (u8, u8)> = floors.iter().map(|f| ((f.level, f.x, f.z), (f.mask, f.kind))).collect();
    let has_floor = |level: u32, p: [f32; 2]| -> bool {
        p[0] >= 0.0 && p[1] >= 0.0 && floor_at.contains_key(&(level as u8, p[0].floor() as u16, p[1].floor() as u16))
    };
    let side_kind = |level: u32, p: [f32; 2]| -> u8 {
        let (x, z) = (p[0].floor(), p[1].floor());
        if x < 0.0 || z < 0.0 {
            return ROOM_OUTSIDE;
        }
        match floor_at.get(&(level as u8, x as u16, z as u16)) {
            Some(&(mask, kind)) if kind != ROOM_PORCH => {
                // Which of the tile's triangles holds the point.
                let (fx, fz) = (p[0] - x - 0.5, p[1] - z - 0.5);
                let t = if fz.abs() > fx.abs() { if fz < 0.0 { 0 } else { 2 } } else if fx > 0.0 { 1 } else { 3 };
                if mask & (1 << t) != 0 { kind } else { ROOM_OUTSIDE }
            }
            _ => ROOM_OUTSIDE,
        }
    };
    let mut walls = Vec::new();
    // The wall graph names the room on each side (0 = outdoors). Which side is "left" is
    // settled per lot by agreement with where the floors are.
    let normal_of = |a: [f32; 2], b: [f32; 2]| {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dz * dz).sqrt().max(1e-6);
        [-dz / len, dx / len]
    };
    let mut agree = 0i32;
    for (a, b, level, e) in data.walls.segments() {
        if (e.left == 0) == (e.right == 0) {
            continue;
        }
        let n = normal_of(a, b);
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let plus = has_floor(level, [mid[0] + n[0] * 0.3, mid[1] + n[1] * 0.3]);
        let minus = has_floor(level, [mid[0] - n[0] * 0.3, mid[1] - n[1] * 0.3]);
        if plus != minus {
            // Convention "left = +normal": the outdoor room is on the side without floor.
            agree += if (e.left == 0) == !plus { 1 } else { -1 };
        }
    }
    let left_is_plus = agree >= 0;
    for (a, b, level, e) in data.walls.segments() {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dz * dz).sqrt();
        if len < 1e-3 {
            continue;
        }
        let n = normal_of(a, b);
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let side = |sign: f32| [mid[0] + n[0] * 0.3 * sign, mid[1] + n[1] * 0.3 * sign];
        // Upper-storey "walls" with no floor on either side are beams over open structures.
        if level > 1 && !has_floor(level, side(1.0)) && !has_floor(level, side(-1.0)) {
            continue;
        }
        let (room_plus, room_minus) = if left_is_plus { (e.left, e.right) } else { (e.right, e.left) };
        let kind = |sign: f32, room: u32| {
            if room == 0 {
                ROOM_OUTSIDE
            } else {
                match side_kind(level, side(sign)) {
                    ROOM_OUTSIDE | ROOM_PORCH => ROOM_LIVING,
                    k => k,
                }
            }
        };
        walls.push(WallBaked { a, b, level: level as u8, left: kind(1.0, room_plus), right: kind(-1.0, room_minus) });
    }
    let foundation: Vec<([f32; 2], [f32; 2])> = data.rooms.segments().filter(|s| s.2 == 0).map(|s| (s.0, s.1)).collect();

    Some(LotBuildingBaked { lot: lot_index as u32, width: w, depth: d, levels, walls, floors, foundation, objects: objs })
}
