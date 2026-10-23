//! Grid-based pathfinding around the active lot.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::{GameClock, SPEED_RATES};
use crate::loading::CurrentWorld;
use crate::sim::{Pose, SimAnim};

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        // (After doors and gates open their ways through, and with them in place.)
        app.init_resource::<SpiralLinks>().add_systems(Update, (spiral_place_test, spiral_links, rebuild_upper_floors, rebuild_grid, settle_lot_exit, spiral_test, follow_paths, climb_spirals).chain().after(crate::building::cut_openings).run_if(in_state(PlayMode::Live)));
    }
}

pub const CELL: f32 = 0.5;

/// An object footprint that blocks walking: an oriented rectangle on the ground.
#[derive(Component, Clone, Copy)]
pub struct Obstacle {
    pub half: Vec2,
    pub center_offset: Vec2,
}

/// A thin obstacle (a wall, a fence run) that can't be walked across: besides the cells it
/// covers, every step and every straight stretch of a way is checked against its line, so it
/// holds however it lies on the grid (a wall on a cell boundary covers no cell's middle).
#[derive(Component, Clone, Copy)]
pub struct Barrier;

/// The obstacles a walk grid is made from.
pub type Obstacles<'w, 's> = Query<'w, 's, (&'static GlobalTransform, &'static Obstacle, Option<&'static Floor>, Has<Barrier>)>;

/// Which floor of the house something is on: 1 = the ground floor and the yard.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Floor(pub u8);

impl Default for Floor {
    fn default() -> Self {
        Self(1)
    }
}

/// Stairs or an elevator joining `level` to `upper`: walk to `bottom`, climb (or ride) to `top`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StairLink {
    pub level: u8,
    pub upper: u8,
    pub bottom: Vec2,
    pub top: Vec2,
    pub y0: f32,
    pub y1: f32,
    /// A spiral staircase's (where it stands, how it's turned): climbed with its own clips.
    pub spiral: Option<(Vec3, Quat)>,
}

/// A spiral staircase's routing slots, as the game's spiral staircases all have them (model
/// space, and the way a Sim faces there): getting on at the foot, at the top, getting off at the
/// foot, at the top (`RoutingSlot_0` to `_3`).
pub const SPIRAL_ON_FOOT: (Vec3, Vec3) = (Vec3::new(0.337, 0.0, -0.5), Vec3::NEG_X);
pub const SPIRAL_ON_TOP: (Vec3, Vec3) = (Vec3::new(0.5, 3.0, -0.195), Vec3::Z);
pub const SPIRAL_OFF_FOOT: (Vec3, Vec3) = (Vec3::new(0.337, 0.0, -0.5), Vec3::X);
pub const SPIRAL_OFF_TOP: (Vec3, Vec3) = (Vec3::new(0.5, 3.0, -0.195), Vec3::NEG_Z);

/// A spiral staircase's slot, in the world: where, and which way a Sim faces there.
pub fn spiral_slot(at: Vec3, rot: Quat, slot: (Vec3, Vec3)) -> (Vec3, Quat) {
    let f = rot * slot.1;
    (at + rot * slot.0, Quat::from_rotation_y(f.x.atan2(f.z)))
}

/// Walk grids of the house's upper floors (the ground floor uses [`NavGrid`]).
#[derive(Resource, Default)]
pub struct UpperFloors {
    pub grids: HashMap<u8, NavGrid>,
    pub stairs: Vec<StairLink>,
    /// Made at least once (the house's upper floors' grids are there).
    pub made: bool,
}

impl UpperFloors {
    pub fn ready(&self) -> bool {
        self.made
    }
}

/// A point along a route, on a floor; `climb` = reached by stairs from height y0 to y1.
#[derive(Clone, Copy, Debug)]
pub struct Waypoint {
    pub p: Vec2,
    pub level: u8,
    pub climb: Option<(f32, f32)>,
    /// Reached by a spiral staircase (where it stands, how it's turned, and whether up it).
    pub spiral: Option<(Vec3, Quat, bool)>,
}

/// Plans a walk from one floor to another, using stairs to change floors.
pub fn plan_route(ground: &NavGrid, upper: Option<&UpperFloors>, from: Vec2, from_level: u8, to: Vec2, to_level: u8) -> Option<Vec<Waypoint>> {
    // (Storey 0 is a basement's, where there's one; otherwise the ground's, the ground floor's.)
    let storey = |l: u8| if l == 0 && upper.is_some_and(|u| u.grids.contains_key(&0)) { 0 } else { l.max(1) };
    plan_route_depth(ground, upper, from, storey(from_level), to, storey(to_level), 0)
}

fn plan_route_depth(
    ground: &NavGrid,
    upper: Option<&UpperFloors>,
    from: Vec2,
    from_level: u8,
    to: Vec2,
    to_level: u8,
    depth: usize,
) -> Option<Vec<Waypoint>> {
    let grid_of = |l: u8| if l == 1 { Some(ground) } else { upper.and_then(|u| u.grids.get(&l)) };
    if from_level == to_level {
        let grid = grid_of(from_level)?;
        if let Some(pts) = grid.find_path(from, to).filter(|pts| pts.last().is_some_and(|q| q.distance(to) < 1.6)) {
            return Some(pts.into_iter().map(|p| Waypoint { p, level: from_level, climb: None, spiral: None }).collect());
        }
        // (Or up or down the steps between two parts of the storey: a platform raised on it.)
        for s in upper.into_iter().flat_map(|u| u.stairs.iter()).filter(|s| s.level == from_level && s.upper == from_level) {
            for (a, b, ys) in [(s.bottom, s.top, (s.y0, s.y1)), (s.top, s.bottom, (s.y1, s.y0))] {
                // (A step or so from where they are, its foot's a landing's width away: stepped
                // to (a staircase's top meeting a half-storey's steps, the grid split between).)
                let first = grid.find_path(from, a).filter(|p| p.last().is_some_and(|q| q.distance(a) < 0.8)).or_else(|| (from.distance(a) < 1.25).then(|| vec![a]));
                let (Some(first), Some(rest)) = (first, grid.find_path(b, to)) else { continue };
                if !first.last().is_some_and(|q| q.distance(a) < 0.8) || !rest.last().is_some_and(|q| q.distance(to) < 1.6) {
                    continue;
                }
                let mut w: Vec<Waypoint> = first.into_iter().map(|p| Waypoint { p, level: from_level, climb: None, spiral: None }).collect();
                w.push(Waypoint { p: b, level: from_level, climb: Some(ys), spiral: None });
                w.extend(rest.into_iter().map(|p| Waypoint { p, level: from_level, climb: None, spiral: None }));
                return Some(w);
            }
        }
        // (Or down a floor and up again by other stairs, or up and down: two parts of a storey
        // joined only by the floor below or above it.)
        if depth < 4
            && let Some(u) = upper
        {
            type Way = (Vec2, Vec2, u8, (f32, f32), Option<(Vec3, Quat, bool)>);
            let mut ways: Vec<Way> = u
                .stairs
                .iter()
                .filter_map(|s| {
                    if s.upper == from_level && s.level < from_level {
                        Some((s.top, s.bottom, s.level, (s.y1, s.y0), s.spiral.map(|(a, r)| (a, r, false))))
                    } else if s.level == from_level && s.upper > from_level {
                        Some((s.bottom, s.top, s.upper, (s.y0, s.y1), s.spiral.map(|(a, r)| (a, r, true))))
                    } else {
                        None
                    }
                })
                .collect();
            ways.sort_by(|a, b| a.0.distance(from).total_cmp(&b.0.distance(from)));
            for (entry, exit, level, ys, spiral) in ways {
                let Some(mut first) = grid.find_path(from, entry).filter(|p| p.last().is_some_and(|q| q.distance(entry) < 0.8 || spiral.is_some())) else { continue };
                // (Onto a spiral staircase's own step, from beside it.)
                if spiral.is_some() {
                    first.push(entry);
                }
                let Some(rest) = plan_route_depth(ground, upper, exit, level, to, to_level, depth + 2) else { continue };
                if !rest.last().is_some_and(|w| w.level == to_level && w.p.distance(to) < 1.6) {
                    continue;
                }
                let mut w: Vec<Waypoint> = first.into_iter().map(|p| Waypoint { p, level: from_level, climb: None, spiral: None }).collect();
                w.push(Waypoint { p: exit, level, climb: Some(ys), spiral });
                w.extend(rest);
                return Some(w);
            }
        }
        let pts = grid.find_path(from, to)?;
        return Some(pts.into_iter().map(|p| Waypoint { p, level: from_level, climb: None, spiral: None }).collect());
    }
    if depth > 6 {
        return None;
    }
    let up = to_level > from_level;
    // Links leaving this floor in the right direction, preferring those that don't overshoot.
    let mut links: Vec<StairLink> = upper?
        .stairs
        .iter()
        .copied()
        .filter(|s| if up { s.level == from_level && s.upper <= to_level } else { s.upper == from_level && s.level >= to_level })
        .collect();
    let entry = |s: &StairLink| if up { s.bottom } else { s.top };
    links.sort_by(|a, b| entry(a).distance(from).total_cmp(&entry(b).distance(from)));
    for s in links {
        let Some(mut first) = grid_of(from_level)?
            .find_path(from, entry(&s))
            .map(|v| v.into_iter().map(|p| Waypoint { p, level: from_level, climb: None, spiral: None }).collect::<Vec<_>>())
        else {
            continue;
        };
        // (Onto a spiral staircase's own step, from beside it.)
        if s.spiral.is_some() {
            first.push(Waypoint { p: entry(&s), level: from_level, climb: None, spiral: None });
        }
        let (exit, next_level, ys) = if up { (s.top, s.upper, (s.y0, s.y1)) } else { (s.bottom, s.level, (s.y1, s.y0)) };
        first.push(Waypoint { p: exit, level: next_level, climb: Some(ys), spiral: s.spiral.map(|(a, r)| (a, r, up)) });
        if let Some(rest) = plan_route_depth(ground, upper, exit, next_level, to, to_level, depth + 1) {
            first.extend(rest);
            return Some(first);
        }
    }
    None
}

#[derive(Resource)]
pub struct NavGrid {
    pub origin: Vec2,
    pub w: usize,
    pub h: usize,
    pub blocked: Vec<bool>,
    pub dirty: bool,
    /// Marked dirty last frame: remade this one, once what changed then (spawned or despawned
    /// by commands) is in place.
    settling: bool,
    /// The barriers' lines, and each cell's barriers (those whose line may pass through it).
    barriers: Vec<[Vec2; 2]>,
    near: Vec<Vec<u32>>,
    /// The storey it's the walks of (doors locked against a Sim are shut on theirs).
    pub level: u8,
}

thread_local! {
    /// The doorways shut to the Sim a way's being planned for (their storey, and the line across
    /// them): see `with_locked`.
    static LOCKED: std::cell::RefCell<Vec<(u8, [Vec2; 2])>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A way planned with these doorways shut (the doors locked against whoever's going: see
/// `doorlocks`).
pub fn with_locked<R>(doors: &[(u8, [Vec2; 2])], f: impl FnOnce() -> R) -> R {
    if doors.is_empty() {
        return f();
    }
    LOCKED.with(|l| *l.borrow_mut() = doors.to_vec());
    let r = f();
    LOCKED.with(|l| l.borrow_mut().clear());
    r
}

impl NavGrid {
    pub fn new(center: Vec2, half_size: f32) -> Self {
        let n = (half_size * 2.0 / CELL) as usize;
        Self {
            origin: center - Vec2::splat(half_size),
            w: n,
            h: n,
            blocked: vec![false; n * n],
            dirty: true,
            settling: false,
            barriers: Vec::new(),
            near: vec![Vec::new(); n * n],
            level: 1,
        }
    }

    /// Whether it's made (not waiting to be remade after a change).
    pub fn ready(&self) -> bool {
        !self.dirty && !self.settling
    }

    /// (Debugging) the grid as it would be without its barriers.
    pub fn without_barriers(&self) -> NavGrid {
        NavGrid {
            origin: self.origin,
            w: self.w,
            h: self.h,
            blocked: self.blocked.clone(),
            dirty: false,
            settling: false,
            barriers: Vec::new(),
            near: vec![Vec::new(); self.w * self.h],
            level: self.level,
        }
    }

    /// (Debugging) the barriers the straight way from `a` to `b` crosses.
    pub fn barriers_crossed(&self, a: Vec2, b: Vec2) -> Vec<[Vec2; 2]> {
        let mut out: Vec<[Vec2; 2]> = Vec::new();
        self.cells_on(a, b, |i| {
            for &id in &self.near[i] {
                let [c, d] = self.barriers[id as usize];
                if segments_touch(a, b, c, d) && !out.contains(&[c, d]) {
                    out.push([c, d]);
                }
            }
            false
        });
        out
    }

    /// The cells that can be walked to from `p`.
    pub fn reachable_from(&self, p: Vec2) -> Vec<bool> {
        let mut seen = vec![false; self.w * self.h];
        let Some(start) = self.nearest_free(p) else { return seen };
        let mut todo = vec![start];
        seen[start.1 * self.w + start.0] = true;
        while let Some((x, z)) = todo.pop() {
            for (dx, dz) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (nx, nz) = (x as i64 + dx, z as i64 + dz);
                if nx < 0 || nz < 0 || nx >= self.w as i64 || nz >= self.h as i64 {
                    continue;
                }
                let (nx, nz) = (nx as usize, nz as usize);
                let i = nz * self.w + nx;
                if seen[i] || self.is_blocked(nx, nz) || (dx != 0 && dz != 0 && self.is_blocked(x, nz) && self.is_blocked(nx, z)) || self.crosses(self.center_of(x, z), self.center_of(nx, nz)) {
                    continue;
                }
                seen[i] = true;
                todo.push((nx, nz));
            }
        }
        seen
    }

    /// How many barriers it has.
    pub fn barrier_count(&self) -> usize {
        self.barriers.len()
    }

    /// No barriers (the grid being made afresh).
    fn clear_barriers(&mut self) {
        self.barriers.clear();
        for v in &mut self.near {
            v.clear();
        }
    }

    /// A barrier along `a`-`b`: listed in every cell its line's bounds touch.
    pub fn add_barrier(&mut self, a: Vec2, b: Vec2) {
        let id = self.barriers.len() as u32;
        self.barriers.push([a, b]);
        let (lo, hi) = (a.min(b) - Vec2::splat(0.02), a.max(b) + Vec2::splat(0.02));
        let cell = |p: Vec2| ((p - self.origin) / CELL).floor();
        let (c0, c1) = (cell(lo).max(Vec2::ZERO), cell(hi).min(Vec2::new(self.w as f32 - 1.0, self.h as f32 - 1.0)));
        if c0.x > c1.x || c0.y > c1.y {
            return;
        }
        for z in c0.y as usize..=c1.y as usize {
            for x in c0.x as usize..=c1.x as usize {
                self.near[z * self.w + x].push(id);
            }
        }
    }

    /// Whether the straight way from `a` to `b` crosses (or touches) a barrier: the barriers of
    /// every cell it passes through, checked against it.
    pub fn crosses(&self, a: Vec2, b: Vec2) -> bool {
        // (A doorway shut to whoever's way this is.)
        if LOCKED.with(|l| l.borrow().iter().any(|(level, [c, d])| *level == self.level && segments_touch(a, b, *c, *d))) {
            return true;
        }
        if self.barriers.is_empty() {
            return false;
        }
        let mut seen: Vec<u32> = Vec::new();
        let mut hit = false;
        self.cells_on(a, b, |i| {
            for &id in &self.near[i] {
                if !seen.contains(&id) {
                    seen.push(id);
                    let [c, d] = self.barriers[id as usize];
                    if segments_touch(a, b, c, d) {
                        hit = true;
                        return true;
                    }
                }
            }
            false
        });
        hit
    }

    /// Every cell the segment from `a` to `b` passes through, in order (a grid walk that misses
    /// none), until `f` says stop.
    fn cells_on(&self, a: Vec2, b: Vec2, mut f: impl FnMut(usize) -> bool) {
        let (pa, pb) = ((a - self.origin) / CELL, (b - self.origin) / CELL);
        let (mut x, mut z) = (pa.x.floor() as i64, pa.y.floor() as i64);
        let (ex, ez) = (pb.x.floor() as i64, pb.y.floor() as i64);
        let d = pb - pa;
        let (sx, sz) = (d.x.signum() as i64, d.y.signum() as i64);
        let delta = |v: f32| if v.abs() < 1e-9 { f32::INFINITY } else { 1.0 / v.abs() };
        let (tdx, tdz) = (delta(d.x), delta(d.y));
        let first = |p: f32, c: i64, v: f32| if v > 0.0 { (c as f32 + 1.0 - p) / v } else if v < 0.0 { (p - c as f32) / -v } else { f32::INFINITY };
        let (mut tx, mut tz) = (first(pa.x, x, d.x), first(pa.y, z, d.y));
        for _ in 0..(self.w + self.h + 4) {
            if x >= 0 && z >= 0 && (x as usize) < self.w && (z as usize) < self.h && f(z as usize * self.w + x as usize) {
                return;
            }
            if (x, z) == (ex, ez) {
                return;
            }
            if tx < tz {
                x += sx;
                tx += tdx;
            } else {
                z += sz;
                tz += tdz;
            }
        }
    }

    pub fn cell_of(&self, p: Vec2) -> Option<(usize, usize)> {
        let c = (p - self.origin) / CELL;
        if c.x < 0.0 || c.y < 0.0 || c.x >= self.w as f32 || c.y >= self.h as f32 {
            return None;
        }
        Some((c.x as usize, c.y as usize))
    }

    pub fn center_of(&self, x: usize, z: usize) -> Vec2 {
        self.origin + Vec2::new(x as f32 + 0.5, z as f32 + 0.5) * CELL
    }

    pub fn is_blocked(&self, x: usize, z: usize) -> bool {
        self.blocked[z * self.w + x]
    }

    pub fn contains(&self, p: Vec2) -> bool {
        self.cell_of(p).is_some()
    }

    fn walkable_pt(&self, p: Vec2) -> bool {
        self.cell_of(p).is_some_and(|(x, z)| !self.is_blocked(x, z))
    }

    /// Nearest unblocked cell to `p`, searching outward.
    pub fn nearest_free(&self, p: Vec2) -> Option<(usize, usize)> {
        self.nearest_where(p, |x, z| !self.is_blocked(x, z))
    }

    /// Nearest cell to `p` among those reached (by [`NavGrid::reachable_from`]): a pond's shore
    /// a Sim can walk to, not an island's.
    pub fn nearest_reached(&self, p: Vec2, reached: &[bool]) -> Option<(usize, usize)> {
        self.nearest_where(p, |x, z| reached.get(z * self.w + x).copied().unwrap_or(false))
    }

    fn nearest_where(&self, p: Vec2, ok: impl Fn(usize, usize) -> bool) -> Option<(usize, usize)> {
        let (cx, cz) = self.cell_of(p.clamp(self.origin, self.origin + Vec2::new(self.w as f32, self.h as f32) * CELL - 0.01))?;
        if ok(cx, cz) {
            return Some((cx, cz));
        }
        for r in 1..40i64 {
            let mut best: Option<((usize, usize), f32)> = None;
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dz.abs() != r {
                        continue;
                    }
                    let (x, z) = (cx as i64 + dx, cz as i64 + dz);
                    if x < 0 || z < 0 || x >= self.w as i64 || z >= self.h as i64 {
                        continue;
                    }
                    let (x, z) = (x as usize, z as usize);
                    if ok(x, z) {
                        let d = self.center_of(x, z).distance(p);
                        if best.is_none_or(|b| d < b.1) {
                            best = Some(((x, z), d));
                        }
                    }
                }
            }
            if let Some((c, _)) = best {
                return Some(c);
            }
        }
        None
    }

    fn line_clear(&self, a: Vec2, b: Vec2) -> bool {
        let d = b - a;
        let steps = (d.length() / (CELL * 0.35)).ceil().max(1.0) as usize;
        (0..=steps).all(|i| self.walkable_pt(a + d * (i as f32 / steps as f32))) && !self.crosses(a, b)
    }

    /// A* from `from` to `to`; returns smoothed waypoints (excluding the start).
    pub fn find_path(&self, from: Vec2, to: Vec2) -> Option<Vec<Vec2>> {
        let start = self.nearest_free(from)?;
        let goal = self.nearest_free(to)?;
        if start == goal {
            return Some(vec![self.center_of(goal.0, goal.1)]);
        }
        let idx = |x: usize, z: usize| z * self.w + x;
        let n = self.w * self.h;
        let mut g = vec![f32::INFINITY; n];
        let mut came = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        let hfn = |x: usize, z: usize| {
            let dx = (x as f32 - goal.0 as f32).abs();
            let dz = (z as f32 - goal.1 as f32).abs();
            dx.max(dz) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dz)
        };
        g[idx(start.0, start.1)] = 0.0;
        open.push(Node { f: hfn(start.0, start.1), i: idx(start.0, start.1) });
        let mut found = false;
        let mut expanded = 0;
        while let Some(Node { i, .. }) = open.pop() {
            let (x, z) = (i % self.w, i / self.w);
            if (x, z) == goal {
                found = true;
                break;
            }
            expanded += 1;
            if expanded > 200_000 {
                break;
            }
            for (dx, dz) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (nx, nz) = (x as i64 + dx, z as i64 + dz);
                if nx < 0 || nz < 0 || nx >= self.w as i64 || nz >= self.h as i64 {
                    continue;
                }
                let (nx, nz) = (nx as usize, nz as usize);
                if self.is_blocked(nx, nz) {
                    continue;
                }
                // (A diagonal step squeezes between two blocked cells only never: a wall drawn
                // diagonally across the grid, cells touching at their corners, can't be crossed,
                // but a corridor along it can be walked.)
                if dx != 0 && dz != 0 && self.is_blocked(x, nz) && self.is_blocked(nx, z) {
                    continue;
                }
                // (Nor across a wall or a fence.)
                if self.crosses(self.center_of(x, z), self.center_of(nx, nz)) {
                    continue;
                }
                let cost = if dx != 0 && dz != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
                let ni = idx(nx, nz);
                let ng = g[i] + cost;
                if ng < g[ni] {
                    g[ni] = ng;
                    came[ni] = i;
                    open.push(Node { f: ng + hfn(nx, nz), i: ni });
                }
            }
        }
        // (A goal shut in, say a use point in a corner of furniture set at an angle to the grid:
        // the nearest place close to it that could be reached instead.)
        let end = if found {
            idx(goal.0, goal.1)
        } else {
            (0..n)
                .filter(|&i| g[i].is_finite())
                .map(|i| (i, self.center_of(i % self.w, i / self.w).distance(to)))
                .filter(|(_, d)| *d < 1.5)
                .min_by(|a, b| a.1.total_cmp(&b.1))?
                .0
        };
        let mut cells = vec![end];
        let mut cur = end;
        while came[cur] != usize::MAX {
            cur = came[cur];
            cells.push(cur);
        }
        cells.reverse();
        let pts: Vec<Vec2> = cells.iter().map(|&i| self.center_of(i % self.w, i / self.w)).collect();
        // String-pulling: skip waypoints while the straight line stays clear.
        let mut out = Vec::new();
        let mut anchor = from;
        let mut k = 0;
        while k < pts.len() {
            let mut far = k;
            for j in (k..pts.len()).rev() {
                if self.line_clear(anchor, pts[j]) {
                    far = j;
                    break;
                }
            }
            out.push(pts[far]);
            anchor = pts[far];
            k = far + 1;
        }
        Some(out)
    }
}

/// Whether segments `a`-`b` and `c`-`d` cross or touch.
fn segments_touch(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    const E: f32 = 1e-5;
    let (d1, d2) = ((d - c).perp_dot(a - c), (d - c).perp_dot(b - c));
    let (d3, d4) = ((b - a).perp_dot(c - a), (b - a).perp_dot(d - a));
    if ((d1 > E && d2 < -E) || (d1 < -E && d2 > E)) && ((d3 > E && d4 < -E) || (d3 < -E && d4 > E)) {
        return true;
    }
    // (Touching: an end on the other segment.)
    let on = |p: Vec2, q: Vec2, r: Vec2, o: f32| o.abs() <= E && r.x >= p.x.min(q.x) - E && r.x <= p.x.max(q.x) + E && r.y >= p.y.min(q.y) - E && r.y <= p.y.max(q.y) + E;
    on(c, d, a, d1) || on(c, d, b, d2) || on(a, b, c, d3) || on(a, b, d, d4)
}

#[derive(Copy, Clone, PartialEq)]
struct Node {
    f: f32,
    i: usize,
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Marks steep slopes, water and the obstacles on the ground floor as unwalkable.
pub fn fill_grid(grid: &mut NavGrid, world: &crate::loading::WorldInfo, obstacles: &Obstacles, building: Option<&crate::building::ActiveBuilding>) {
    let hm = &world.heightmap;
    let (w, h) = (grid.w, grid.h);
    // (The paving on the ground's level: a houseboat's deck is, out over the water.)
    let paved: std::collections::HashSet<(i32, i32)> = building.map(|b| b.data.floors.iter().filter(|f| f.level == 0 && f.y.is_none()).map(|f| (f.x as i32, f.z as i32)).collect()).unwrap_or_default();
    // (A fountain's basin is water, not walked through.)
    let fountain: std::collections::HashSet<(i32, i32)> = building.map(|b| b.data.fountains.iter().flat_map(|f| f.cells.iter().map(|&(x, z)| (x as i32, z as i32))).collect()).unwrap_or_default();
    for z in 0..h {
        for x in 0..w {
            let c = grid.center_of(x, z);
            if let Some(b) = building.filter(|_| !fountain.is_empty()) {
                let l = b.local(Vec3::new(c.x, 0.0, c.y));
                if fountain.contains(&(l.x.floor() as i32, l.y.floor() as i32)) {
                    grid.blocked[z * w + x] = true;
                    continue;
                }
            }
            // (A ground floor's tile is walked on whatever ground lies under it: a hillside under
            // a house standing out over it, as Cormorant's.)
            if building.is_some_and(|b| b.floor_y(1, Vec3::new(c.x, 0.0, c.y)).is_some()) {
                grid.blocked[z * w + x] = false;
                continue;
            }
            let y = hm.sample(c.x, c.y);
            if y < world.sea_level + 0.2
                && let Some(b) = building
                && paved.contains(&{
                    let l = b.local(Vec3::new(c.x, 0.0, c.y));
                    (l.x.floor() as i32, l.y.floor() as i32)
                })
            {
                grid.blocked[z * w + x] = false;
                continue;
            }
            let slope = (hm.sample(c.x + 0.5, c.y) - hm.sample(c.x - 0.5, c.y))
                .abs()
                .max((hm.sample(c.x, c.y + 0.5) - hm.sample(c.x, c.y - 0.5)).abs());
            let pond = world.pond_at(c.x, c.y).is_some_and(|w| y < w + 0.1);
            grid.blocked[z * w + x] = y < world.sea_level + 0.2 || pond || slope > 0.9;
        }
    }
    grid.clear_barriers();
    for (gt, ob, floor, barrier) in obstacles {
        if floor.is_some_and(|f| f.0 != 1) {
            continue;
        }
        mark_obstacle(grid, gt, ob, barrier);
    }
}

/// Marks obstacles, steep slopes and water as unwalkable whenever the grid is dirty.
fn rebuild_grid(
    grid: Option<ResMut<NavGrid>>,
    world: Res<CurrentWorld>,
    obstacles: Obstacles,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut upper: Option<ResMut<UpperFloors>>,
    spirals: Res<SpiralLinks>,
) {
    let Some(mut grid) = grid else { return };
    // (A frame after it's marked dirty: a wall, fence or object put in or taken out then is
    // spawned or gone by now.)
    if grid.dirty {
        grid.dirty = false;
        grid.settling = true;
        return;
    }
    if !grid.settling {
        return;
    }
    grid.settling = false;
    if let (Some(b), Some(u)) = (building.as_deref(), upper.as_deref_mut()) {
        rebuild_upper(b, u, &obstacles, &spirals);
    }
    fill_grid(&mut grid, &world.data, &obstacles, building.as_deref());
}

/// Marks the cells under an obstacle's footprint as blocked (and a barrier's line as one).
fn mark_obstacle(grid: &mut NavGrid, gt: &GlobalTransform, ob: &Obstacle, barrier: bool) {
    let (w, h) = (grid.w, grid.h);
    {
        let tf = gt.compute_transform();
        let fwd = tf.rotation * Vec3::X;
        let ax = Vec2::new(fwd.x, fwd.z).normalize_or(Vec2::X);
        let az = Vec2::new(-ax.y, ax.x);
        // Bevy's +Z after rotation: perpendicular to X on the ground plane.
        let rz = tf.rotation * Vec3::Z;
        let az = if Vec2::new(rz.x, rz.z).dot(az) < 0.0 { -az } else { az };
        let center = Vec2::new(tf.translation.x, tf.translation.z) + ax * ob.center_offset.x + az * ob.center_offset.y;
        if barrier {
            grid.add_barrier(center - ax * ob.half.x, center + ax * ob.half.x);
        }
        let r = ob.half.length() + CELL;
        let (Some((x0, z0)), Some((x1, z1))) = (
            grid.cell_of((center - Vec2::splat(r)).max(grid.origin)),
            grid.cell_of((center + Vec2::splat(r)).min(grid.origin + Vec2::new(w as f32, h as f32) * CELL - 0.01)),
        ) else {
            return;
        };
        for z in z0..=z1 {
            for x in x0..=x1 {
                let d = grid.center_of(x, z) - center;
                let (lx, lz) = (d.dot(ax), d.dot(az));
                if lx.abs() < ob.half.x + 0.12 && lz.abs() < ob.half.y + 0.12 {
                    grid.blocked[z * w + x] = true;
                }
            }
        }
    }
}

/// Builds the walk grids of the upper floors: cells with floor, minus walls and furniture there.
pub fn rebuild_upper(b: &crate::building::ActiveBuilding, upper: &mut UpperFloors, obstacles: &Obstacles, spirals: &SpiralLinks) {
    upper.made = true;
    upper.grids.clear();
    upper.stairs = b.stairs.clone();
    upper.stairs.extend(spirals.0.iter().copied());
    // (The upper floors, and a basement.)
    for level in (b.data.basement.is_some().then_some(0).into_iter()).chain(2..=b.top_level) {
        let mut g = NavGrid::new(b.center.xz(), b.extent + 2.0);
        g.dirty = false;
        g.level = level;
        for z in 0..g.h {
            for x in 0..g.w {
                let c = g.center_of(x, z);
                g.blocked[z * g.w + x] = b.floor_y(level, Vec3::new(c.x, 0.0, c.y)).is_none();
            }
        }
        for (gt, ob, floor, barrier) in obstacles {
            if floor.is_some_and(|f| f.0 == level) {
                mark_obstacle(&mut g, gt, ob, barrier);
            }
        }
        upper.grids.insert(level, g);
    }
}

/// The way onto the lot, made sure of once its walk grid is made: the middle of its front edge
/// unless the house's doors can't be walked to from there (a lot on a platform walled round
/// but for its walkway, as Oasis Landing's are), when it's the point off the lot nearest that
/// a ground-floor door can be walked to from.
fn settle_lot_exit(
    grid: Option<Res<NavGrid>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    exit: Option<ResMut<crate::interact::LotExit>>,
    mut settled: Local<Option<usize>>,
) {
    let (Some(grid), Some(b), Some(mut exit)) = (grid, building, exit) else { return };
    if *settled == Some(b.lot) || !grid.ready() {
        return;
    }
    *settled = Some(b.lot);
    let doors: Vec<Vec2> = b
        .doorways(1)
        .into_iter()
        .flat_map(|(mid, fwd)| [mid + fwd * 0.6, mid - fwd * 0.6])
        .map(|p| b.world(p.x, p.y, 0.0).xz())
        .filter(|p| grid.cell_of(*p).is_some_and(|(x, z)| !grid.is_blocked(x, z)))
        .collect();
    if doors.is_empty() {
        return;
    }
    let from_exit = grid.reachable_from(exit.0);
    let reached = |r: &[bool], p: Vec2| grid.cell_of(p).is_some_and(|(x, z)| r[z * grid.w + x]);
    if doors.iter().any(|d| reached(&from_exit, *d)) {
        return;
    }
    let (w, d) = (b.data.width as f32, b.data.depth as f32);
    let off_lot = |p: Vec2| {
        let l = b.local(Vec3::new(p.x, 0.0, p.y));
        l.x < -0.5 || l.y < -0.5 || l.x > w + 0.5 || l.y > d + 0.5
    };
    let mut best: Option<(f32, Vec2)> = None;
    for door in &doors {
        let from_door = grid.reachable_from(*door);
        for z in 0..grid.h {
            for x in 0..grid.w {
                let c = grid.center_of(x, z);
                if from_door[z * grid.w + x] && off_lot(c) {
                    let dist = c.distance(exit.0);
                    if best.is_none_or(|(bd, _)| dist < bd) {
                        best = Some((dist, c));
                    }
                }
            }
        }
        if best.is_some() {
            break;
        }
    }
    if let Some((_, p)) = best {
        info!("the way onto the lot moved to {} (lot-local), where the house can be reached", b.local(Vec3::new(p.x, 0.0, p.y)));
        exit.0 = p;
    }
}

/// Creates the upper-floor grids when a house becomes active (and drops them when it goes).
fn rebuild_upper_floors(
    mut commands: Commands,
    building: Option<Res<crate::building::ActiveBuilding>>,
    upper: Option<Res<UpperFloors>>,
    grid: Option<ResMut<NavGrid>>,
) {
    match (building, upper) {
        (Some(_), None) => {
            commands.insert_resource(UpperFloors::default());
            if let Some(mut g) = grid {
                g.dirty = true;
            }
        }
        (None, Some(_)) => commands.remove_resource::<UpperFloors>(),
        _ => {}
    }
}

/// Walking along a computed path.
#[derive(Component, Default)]
pub struct PathFollow {
    pub waypoints: Vec<Waypoint>,
    /// Metres a second (none: the walk style's own pace).
    pub speed: f32,
    pub done: bool,
    seg_start: Option<Vec2>,
    /// How the way is gone (the game's walk style for it), and how it's being gone just now
    /// (stairs are walked) at what pace: what the Sim's legs keep up with.
    pub style: WalkStyle,
    pub now: WalkStyle,
    pub pace: f32,
}

impl PathFollow {
    pub fn new(waypoints: Vec<Waypoint>) -> Self {
        Self { waypoints, ..default() }
    }

    pub fn with_style(mut self, style: WalkStyle) -> Self {
        self.style = style;
        self
    }

    /// The whole way's length from `from`.
    pub fn length_from(waypoints: &[Waypoint], from: Vec2) -> f32 {
        waypoints.iter().scan(from, |at, w| Some(std::mem::replace(at, w.p).distance(w.p))).sum()
    }
}

/// How a Sim goes along a route: the game's walk styles (`Sim.WalkStyle`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WalkStyle {
    #[default]
    Walk,
    FastWalk,
    FastJog,
    Run,
    /// (Running, for the athletic.)
    FastRun,
    Jog,
    OnFire,
}

impl WalkStyle {
    /// Ground speed, metres a second: its clip's own (the feet planted as they go, measured
    /// from the game's cycles).
    pub fn speed(self, child: bool, female: bool) -> f32 {
        use WalkStyle::*;
        match (self, child) {
            (Walk, true) => 1.25,
            (_, true) => 3.7,
            (Walk, false) if female => 1.7,
            (Walk, false) => 1.8,
            (FastWalk, _) => 2.55,
            (FastJog, _) => 3.3,
            (Run, _) => 5.7,
            (FastRun, _) => 7.3,
            (Jog, _) => 2.4,
            (OnFire, _) => 6.4,
        }
    }

    /// The game's clip for it (none: the Sim's own walk).
    pub fn clip(self, child: bool, female: bool) -> Option<&'static [&'static str]> {
        use WalkStyle::*;
        Some(match (self, child) {
            (Walk, _) => return None,
            (_, true) => &["c_run"],
            (FastWalk, _) => &["a_male_walk_fast"],
            (FastJog, _) => &["a_male_fastjog"],
            (Run, _) if female => &["a_female_run"],
            (Run, _) => &["a_male_run"],
            (FastRun, _) => &["a_male_run_fast"],
            (Jog, _) => &["a_male_jog"],
            (OnFire, _) => &["a_male_run_onFire"],
        })
    }
}

/// The ground speed of a walk cycle (metres a second, feet planted), for playing it in step with
/// the Sim's pace.
pub fn cycle_speed(clip: &str) -> Option<f32> {
    const CYCLES: [(&str, f32); 12] = [
        ("a_male_walk", 1.82),
        ("a_female_walk", 1.7),
        ("a_female_walk_pregnant", 1.73),
        ("a_male_walk_fast", 2.56),
        ("a_male_fastjog", 3.3),
        ("a_male_jog", 2.4),
        ("a_male_run", 5.7),
        ("a_female_run", 5.7),
        ("a_male_run_fast", 7.3),
        ("a_male_run_onFire", 6.4),
        ("c_walk", 1.25),
        ("c_run", 3.7),
    ];
    CYCLES.iter().find(|(n, _)| n.eq_ignore_ascii_case(clip)).map(|c| c.1)
}

/// What a route's walk style turns on.
pub struct RouteSense {
    pub distance: f32,
    /// Gone autonomously (not at the player's word).
    pub autonomous: bool,
    /// To an object (not a spot on the ground).
    pub to_object: bool,
    /// Both ends on the same lot, and both indoors.
    pub same_lot: bool,
    pub indoors: bool,
    pub age: crate::sim::Age,
    pub athletic: u32,
    pub fatigued: bool,
    /// Big with child: they walk.
    pub pregnant: bool,
}

/// The game's distances (metres) beyond which a route is fast-walked, fast-jogged and run: a
/// child's, an elder's and everyone else's (SimWalkStyleRules' distance thresholds).
const CHILD_THRESHOLDS: [f32; 3] = [5.0, 10.0, 20.0];
const ELDER_THRESHOLDS: [f32; 3] = [15.0, 25.0, 45.0];
const THRESHOLDS: [f32; 3] = [10.0, 15.0, 30.0];
/// How likely a Sim going somewhere off the lot of their own accord walks, fast-walks,
/// fast-jogs or runs, up to what the distance allows (`kAutonomousWalkStyleWeights`).
const AUTONOMOUS_WEIGHTS: [f32; 4] = [0.3, 0.25, 0.2, 0.15];
/// Athletic skill from which a run is the fast run (`Athletic.kAthleticSkillForFastRun`).
const ATHLETIC_FOR_FAST_RUN: u32 = 5;

/// A route's walk style, by the game's rules (`SimWalkStyleRules`): on their own lot Sims
/// going about their business walk, and at the player's word hurry (fast-walk) the long ways;
/// elsewhere, the further the faster, up to a run (going of their own accord, by chance up to
/// that). Children walk or run; the fatigued don't jog; the athletic run fast. `roll` is a
/// chance, 0 to 1.
pub fn walk_style(r: &RouteSense, roll: f32) -> WalkStyle {
    use WalkStyle::*;
    const LADDER: [WalkStyle; 4] = [Walk, FastWalk, FastJog, Run];
    if r.pregnant || r.age.is_little() {
        return Walk;
    }
    let thresholds = match r.age {
        crate::sim::Age::Child => CHILD_THRESHOLDS,
        crate::sim::Age::Elder => ELDER_THRESHOLDS,
        _ => THRESHOLDS,
    };
    let far = thresholds.iter().take_while(|t| r.distance > **t).count();
    let step = if r.autonomous {
        if r.same_lot {
            0
        } else {
            let weights = &AUTONOMOUS_WEIGHTS[..=far];
            let mut left = roll * weights.iter().sum::<f32>();
            weights
                .iter()
                .position(|w| {
                    left -= w;
                    left <= 0.0
                })
                .unwrap_or(far)
        }
    } else if r.same_lot && (r.to_object || r.indoors) {
        far.min(1)
    } else {
        far
    };
    let mut style = LADDER[step];
    if r.age == crate::sim::Age::Child && style != Walk {
        style = Run;
    }
    if r.fatigued && matches!(style, FastJog | Jog) {
        style = FastWalk;
    }
    if style == Run && r.athletic >= ATHLETIC_FOR_FAST_RUN {
        style = FastRun;
    }
    style
}

/// Standing height on a floor at a point: the house floor or the terrain.
pub fn floor_height(world: &crate::loading::WorldInfo, building: Option<&crate::building::ActiveBuilding>, level: u8, p: Vec3) -> f32 {
    match building {
        // (The house's floor, or the floor of the lot being visited.)
        Some(b) if level > 1 || (level == 0 && b.data.basement.is_some()) => b
            .floor_y(level, p)
            .or_else(|| b.away.as_deref().and_then(|a| a.floor_y(level, p)))
            .unwrap_or_else(|| match b.away.as_deref().filter(|a| a.local(p).cmpge(Vec2::ZERO).all() && a.local(p).cmple(Vec2::new(a.data.width as f32, a.data.depth as f32)).all()) {
                Some(a) => a.levels.get(level as usize).copied().unwrap_or(p.y),
                None => b.levels.get(level as usize).copied().unwrap_or(p.y),
            }),
        _ => crate::building::walk_height(world, building, p),
    }
}

pub(crate) fn follow_paths(
    time: Res<Time>,
    clock: Res<GameClock>,
    world: Res<CurrentWorld>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut commands: Commands,
    mut q: Query<
        (Entity, &mut Transform, &mut PathFollow, &mut SimAnim, &mut Floor, Option<&crate::sim::Sim>, Option<&crate::little::Pregnancy>),
        (Without<crate::portraits::Staged>, Without<crate::dressing::ClothesSpin>, Without<SpiralClimb>),
    >,
) {
    let rate = SPEED_RATES[clock.speed];
    let dt = time.delta_secs().min(0.1) * rate;
    for (e, mut tf, mut pf, mut anim, mut floor, sim, pregnancy) in &mut q {
        if pf.done {
            continue;
        }
        // (At a spiral staircase's step: up or down it by its own clips, `SpiralClimb`.)
        if let Some(&Waypoint { p, level, climb: Some((_, y1)), spiral: Some((at, rot, up)) }) = pf.waypoints.first() {
            let _ = p;
            commands.entity(e).insert(SpiralClimb { at, rot, up, part: 0, time: 0.0, length: -1.0, level, y: y1 });
            continue;
        }
        if dt <= 0.0 {
            continue;
        }
        // In the route's walk style, at its pace (stairs walked; heavily pregnant, a walk).
        let stepping = pf.waypoints.first().is_some_and(|w| w.climb.is_some()) || pregnancy.is_some_and(|p| p.stage >= 2);
        pf.now = if stepping { WalkStyle::Walk } else { pf.style };
        let (child, female) = sim.map_or((false, false), |s| (s.age == crate::sim::Age::Child, s.female));
        pf.pace = if pf.speed > 0.0 && pf.now == pf.style { pf.speed } else { pf.now.speed(child, female) };
        let mut budget = pf.pace * dt;
        let mut climbing = None;
        while budget > 0.0 {
            let Some(&target) = pf.waypoints.first() else {
                pf.done = true;
                break;
            };
            let pos = Vec2::new(tf.translation.x, tf.translation.z);
            let start = *pf.seg_start.get_or_insert(pos);
            let to = target.p - pos;
            let d = to.length();
            // Stairs are climbed at a slower pace.
            let pace = if target.climb.is_some() { 0.6 } else { 1.0 };
            if d <= budget * pace {
                tf.translation.x = target.p.x;
                tf.translation.z = target.p.y;
                budget -= d / pace;
                floor.0 = target.level;
                pf.waypoints.remove(0);
                pf.seg_start = Some(target.p);
            } else {
                let step = to / d * (budget * pace);
                tf.translation.x += step.x;
                tf.translation.z += step.y;
                budget = 0.0;
                let yaw = to.x.atan2(to.y);
                let target_rot = Quat::from_rotation_y(yaw);
                tf.rotation = tf.rotation.slerp(target_rot, (dt * 8.0).min(1.0));
                if let Some((y0, y1)) = target.climb {
                    let total = (target.p - start).length().max(1e-3);
                    let done = 1.0 - (target.p - Vec2::new(tf.translation.x, tf.translation.z)).length() / total;
                    climbing = Some(y0 + (y1 - y0) * done.clamp(0.0, 1.0));
                }
            }
        }
        tf.translation.y = climbing.unwrap_or_else(|| floor_height(&world.data, building.as_deref(), floor.0, tf.translation));
        anim.pose = if pf.done { Pose::Stand } else { Pose::Walk };
    }
}

/// The spiral staircases standing (the lot's own, those bought, a visited lot's): each joins its
/// floor to the one over it, by its own steps.
#[derive(Resource, Default)]
pub struct SpiralLinks(pub Vec<StairLink>);

/// A spiral staircase (an object of the game's `StaircaseSpiral` classes).
#[derive(Component)]
pub struct SpiralStair;

/// Spiral staircases found among the objects as they're set out, and their links kept up as they're
/// put in, moved or taken away (the walk grids rebuilt with them): up to the floor over the one they
/// stand on, where there's floor at the top step (the household's house; a visited lot's as built).
#[allow(clippy::type_complexity)]
fn spiral_links(
    mut commands: Commands,
    catalog: Option<Res<crate::loading::Catalog>>,
    new: Query<(Entity, &crate::interact::GameObject), Added<crate::interact::GameObject>>,
    spirals: Query<(&GlobalTransform, Option<&Floor>, Has<crate::visit::LotObject>), With<SpiralStair>>,
    changed: Query<(), (With<SpiralStair>, Or<(Changed<GlobalTransform>, Changed<Floor>)>)>,
    mut gone: RemovedComponents<SpiralStair>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut links: ResMut<SpiralLinks>,
    mut grid: Option<ResMut<NavGrid>>,
) {
    if let Some(cat) = catalog.as_deref() {
        for (e, o) in &new {
            if cat.by_key(&o.objd).is_some_and(|c| c.spiral) {
                commands.entity(e).insert(SpiralStair);
            }
        }
    }
    if changed.is_empty() && gone.read().count() == 0 && !building.as_ref().is_some_and(|b| b.is_changed()) {
        return;
    }
    let now: Vec<StairLink> = spirals
        .iter()
        .filter_map(|(gt, floor, visiting)| {
            let (_, rot, pos) = gt.to_scale_rotation_translation();
            let s = floor.map_or(1, |f| f.0);
            let (foot, _) = spiral_slot(pos, rot, SPIRAL_ON_FOOT);
            let (head, _) = spiral_slot(pos, rot, SPIRAL_ON_TOP);
            let floored = visiting || building.as_deref().is_none_or(|b| b.floor_y(s + 1, head).is_some());
            floored.then_some(StairLink { level: s, upper: s + 1, bottom: foot.xz(), top: head.xz(), y0: foot.y, y1: head.y, spiral: Some((pos, rot)) })
        })
        .collect();
    // (The walk grids rebuilt only for a change.)
    if now != links.0 {
        links.0 = now;
        if let Some(g) = grid.as_mut() {
            g.dirty = true;
        }
    }
}

/// A Sim on a spiral staircase, played up or down it with the game's clips (its `SpiralStaircase`:
/// on at the foot facing in, `walkUp_start`, `_climb`, `_stop`, off at the top; or on at the top,
/// `walkDown_start`, then from the foot `_descend` and `_stop`): which part, how far into it.
#[derive(Component)]
pub struct SpiralClimb {
    at: Vec3,
    rot: Quat,
    up: bool,
    part: u8,
    time: f32,
    length: f32,
    level: u8,
    y: f32,
}

/// `SPIRAL_TEST=once|both` (tests): eight seconds in, the selected Sim sent by the household's nearest
/// spiral staircase to its other floor (a little past its end), and (`both`) back again.
fn spiral_test(time: Res<Time>, mut stage: Local<u8>, upper: Option<Res<UpperFloors>>, mut sel: Query<(&Transform, &Floor, &mut crate::interact::ActionQueue), (With<crate::sim::Selected>, Without<SpiralClimb>)>) {
    let Ok(v) = std::env::var("SPIRAL_TEST") else { return };
    let (Some(u), Ok((tf, floor, mut q))) = (upper, sel.single_mut()) else { return };
    let going = q.0.iter().any(|a| matches!(a.kind, crate::interact::ActionKind::GoHere(..)));
    if time.elapsed_secs() < 8.0 || (*stage > 0 && going) {
        return;
    }
    debug!("spiral test: {} links, {} spiral", u.stairs.len(), u.stairs.iter().filter(|s| s.spiral.is_some()).count());
    let Some(s) = u.stairs.iter().filter(|s| s.spiral.is_some()).min_by(|a, b| a.bottom.distance(tf.translation.xz()).total_cmp(&b.bottom.distance(tf.translation.xz()))) else { return };
    let (at, rot) = s.spiral.unwrap_or_default();
    let off = |slot: (Vec3, Vec3)| { let (p, turn) = spiral_slot(at, rot, slot); p.xz() + (turn * Vec3::Z).xz() * 1.4 };
    // (To the other floor, and (`both`) back.)
    let other = |f: u8| if f == s.upper { (off(SPIRAL_OFF_FOOT), s.level) } else { (off(SPIRAL_OFF_TOP), s.upper) };
    let go = match (*stage, v.as_str()) {
        (0, _) | (1, "both") => Some(other(floor.0)),
        _ => None,
    };
    if let Some((p, level)) = go {
        info!("spiral test: to {p} on floor {level}");
        q.0.clear();
        q.push_player(crate::interact::Action::new("Go Here", crate::interact::ActionKind::GoHere(p, level), false));
        *stage += 1;
    }
}

/// `SPIRAL_PLACE=1` (tests): six seconds in, the catalogue's first spiral staircase bought and set
/// down in the household's house where the ground floor and the one over it are open floor (no wall
/// across), the camera on it.
#[allow(clippy::too_many_arguments)]
fn spiral_place_test(
    mut commands: Commands,
    time: Res<Time>,
    mut done: Local<bool>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    catalog: Res<crate::loading::Catalog>,
    data: Res<crate::baked::Baked>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
) {
    if *done || std::env::var_os("SPIRAL_PLACE").is_none() || time.elapsed_secs() < 6.0 {
        return;
    }
    let (Some(b), Some(e)) = (building, catalog.entries.iter().find(|e| e.spiral)) else { return };
    *done = true;
    let floored = |level: u8, x: i32, z: i32| b.data.floors.iter().any(|f| f.level == level && f.x as i32 == x && f.z as i32 == z);
    let walled = |x: i32, z: i32| {
        let (lo, hi) = (Vec2::new(x as f32 - 1.2, z as f32 - 1.2), Vec2::new(x as f32 + 1.2, z as f32 + 1.2));
        b.data.walls.iter().filter(|w| w.storey() <= 2).any(|w| {
            let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
            (0..=8).any(|i| { let p = a.lerp(c, i as f32 / 8.0); p.cmpge(lo).all() && p.cmple(hi).all() })
        })
    };
    let spot = (2..b.data.depth as i32 - 2).flat_map(|z| (2..b.data.width as i32 - 2).map(move |x| (x, z))).find(|&(x, z)| {
        [(-1, -1), (0, -1), (-1, 0), (0, 0)].iter().all(|(dx, dz)| floored(1, x + dx, z + dz) && floored(2, x + dx, z + dz)) && !walled(x, z)
    });
    let Some((x, z)) = spot else {
        warn!("spiral place test: nowhere open on two floors");
        return;
    };
    let pos = b.world(x as f32, z as f32, b.levels.get(1).copied().unwrap_or(0.0));
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    if let Some(o) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, e.key, pos, b.rot, None) {
        commands.entity(o.entity).insert((crate::save::Bought, crate::buyhistory::NewPurchase, Floor(1), crate::building::BuildingPiece { level: 1 }));
        info!("spiral place test: {} at lot ({x}, {z}), {pos}", e.name);
    }
    if let Ok(mut c) = cams.single_mut() {
        c.look_at(pos + Vec3::Y * 3.0);
        c.distance = 9.0;
    }
}

const SPIRAL_UP: [&str; 3] = ["a2o_spiralStaircase_walkUp_start_x", "a2o_spiralStaircase_walkUp_climb_x", "a2o_spiralStaircase_walkUp_stop_x"];
const SPIRAL_DOWN: [&str; 3] = ["a2o_spiralStaircase_walkDown_start_x", "a2o_spiralStaircase_walkDown_descend_x", "a2o_spiralStaircase_walkDown_stop_x"];

/// Up or down a spiral staircase: each part played where the game's jig has it (going up all in
/// the foot's step; going down, the first at the top, the rest from the foot), then off at the
/// other end, on its floor, the route carrying on.
#[allow(clippy::type_complexity)]
pub(crate) fn climb_spirals(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    data: Res<crate::baked::Baked>,
    mut lib: ResMut<crate::anim::ClipLibrary>,
    mut q: Query<(Entity, &mut SpiralClimb, &mut Transform, &mut PathFollow, &mut SimAnim, &mut Floor, Option<&crate::sim::Sim>)>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed];
    for (e, mut s, mut tf, mut pf, mut anim, mut floor, sim) in &mut q {
        let child = sim.is_some_and(|s| s.age == crate::sim::Age::Child);
        s.time += dt;
        if s.time < s.length {
            continue;
        }
        let parts: &'static [&'static str; 3] = if s.up { &SPIRAL_UP } else { &SPIRAL_DOWN };
        if s.length >= 0.0 {
            s.part += 1;
        }
        let Some(name) = parts.get(s.part as usize) else {
            // Off at the other end, facing off it.
            let (at, turn) = spiral_slot(s.at, s.rot, if s.up { SPIRAL_OFF_TOP } else { SPIRAL_OFF_FOOT });
            tf.translation = Vec3::new(at.x, s.y, at.z);
            tf.rotation = turn;
            floor.0 = s.level;
            if !pf.waypoints.is_empty() {
                pf.waypoints.remove(0);
            }
            pf.seg_start = Some(at.xz());
            commands.entity(e).remove::<(SpiralClimb, crate::anim::ActionClip)>();
            anim.pose = Pose::Walk;
            continue;
        };
        // (Where this part's played from: the step it starts on, as the jig puts the Sim.)
        let slot = match (s.up, s.part) {
            (true, _) => SPIRAL_ON_FOOT,
            (false, 0) => SPIRAL_ON_TOP,
            (false, _) => SPIRAL_OFF_FOOT,
        };
        let (at, turn) = spiral_slot(s.at, s.rot, slot);
        tf.translation = at;
        tf.rotation = turn;
        let clip = lib.variants(&data, name, Some('x'), child).first().cloned().unwrap_or_else(|| name.to_string());
        s.length = lib.get(&data, &clip).map_or(1.0, |c| c.duration.max(0.1));
        debug!("spiral staircase: {} ({:.1} s)", clip, s.length);
        s.time = 0.0;
        anim.pose = Pose::Use;
        commands.entity(e).insert(crate::anim::ActionClip::new(None, std::slice::from_ref(name)));
    }
}

#[cfg(test)]
mod walk_style_tests {
    use super::*;

    #[test]
    fn a_spiral_staircases_steps_turn_with_it() {
        // (On at the foot facing in, -X; off at the top facing back, -Z: its routing slots.)
        let (p, q) = spiral_slot(Vec3::new(10.0, 2.0, 5.0), Quat::IDENTITY, SPIRAL_ON_FOOT);
        assert!((p - Vec3::new(10.337, 2.0, 4.5)).length() < 1e-4);
        assert!((q * Vec3::Z - Vec3::NEG_X).length() < 1e-4);
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let (p, q) = spiral_slot(Vec3::ZERO, turned, SPIRAL_OFF_TOP);
        assert!((p - turned * Vec3::new(0.5, 3.0, -0.195)).length() < 1e-4);
        assert!((q * Vec3::Z - turned * Vec3::NEG_Z).length() < 1e-4);
    }
    use crate::sim::Age;

    fn sense(distance: f32, autonomous: bool, same_lot: bool) -> RouteSense {
        RouteSense { distance, autonomous, to_object: true, same_lot, indoors: false, age: Age::YoungAdult, athletic: 0, fatigued: false, pregnant: false }
    }

    #[test]
    fn the_games_rules() {
        use WalkStyle::*;
        // On their own lot, going about their business: a walk, however far.
        assert_eq!(walk_style(&sense(40.0, true, true), 0.99), Walk);
        // At the player's word on the lot: a hurry, no more.
        assert_eq!(walk_style(&sense(8.0, false, true), 0.5), Walk);
        assert_eq!(walk_style(&sense(40.0, false, true), 0.5), FastWalk);
        // Off the lot: the further the faster.
        assert_eq!(walk_style(&sense(12.0, false, false), 0.5), FastWalk);
        assert_eq!(walk_style(&sense(20.0, false, false), 0.5), FastJog);
        assert_eq!(walk_style(&sense(40.0, false, false), 0.5), Run);
        // (Of their own accord, by chance up to that.)
        assert_eq!(walk_style(&sense(40.0, true, false), 0.0), Walk);
        assert_eq!(walk_style(&sense(40.0, true, false), 1.0), Run);
        assert_eq!(walk_style(&sense(5.0, true, false), 1.0), Walk);
        // Children walk or run; the athletic run fast; the fatigued don't jog.
        assert_eq!(walk_style(&RouteSense { age: Age::Child, ..sense(12.0, false, false) }, 0.5), Run);
        assert_eq!(walk_style(&RouteSense { athletic: 5, ..sense(40.0, false, false) }, 0.5), FastRun);
        assert_eq!(walk_style(&RouteSense { fatigued: true, ..sense(20.0, false, false) }, 0.5), FastWalk);
        assert_eq!(walk_style(&RouteSense { pregnant: true, ..sense(40.0, false, false) }, 0.5), Walk);
        // Elders take further to hurry.
        assert_eq!(walk_style(&RouteSense { age: Age::Elder, ..sense(20.0, false, false) }, 0.5), FastWalk);
    }
}

#[cfg(test)]
mod barrier_tests {
    use super::*;

    fn length(from: Vec2, path: &[Vec2]) -> f32 {
        std::iter::once(from).chain(path.iter().copied()).collect::<Vec<_>>().windows(2).map(|w| w[0].distance(w[1])).sum()
    }

    #[test]
    fn walls_on_cell_boundaries_are_walked_round() {
        // (A wall along x = 0 from z = -3 to 3: on the boundary between cells, covering no
        // cell's middle.)
        let mut g = NavGrid::new(Vec2::ZERO, 8.0);
        g.add_barrier(Vec2::new(0.0, -3.0), Vec2::new(0.0, 3.0));
        let (from, to) = (Vec2::new(-1.0, 0.1), Vec2::new(1.0, 0.1));
        assert!(g.crosses(from, to));
        let path = g.find_path(from, to).unwrap();
        assert!(length(from, &path) > 6.0, "round the wall's end, not through it: {path:?}");
        // A gap in it (a doorway) is walked through.
        let mut g = NavGrid::new(Vec2::ZERO, 8.0);
        g.add_barrier(Vec2::new(0.0, -3.0), Vec2::new(0.0, -0.5));
        g.add_barrier(Vec2::new(0.0, 0.5), Vec2::new(0.0, 3.0));
        let path = g.find_path(from, to).unwrap();
        assert!(length(from, &path) < 2.5, "{path:?}");
        // Two walls meeting at a corner leave no way through it diagonally.
        let mut g = NavGrid::new(Vec2::ZERO, 8.0);
        g.add_barrier(Vec2::new(0.0, 0.0), Vec2::new(0.0, 3.0));
        g.add_barrier(Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0));
        assert!(g.crosses(Vec2::new(-0.25, -0.25), Vec2::new(0.25, 0.25)));
    }
}
