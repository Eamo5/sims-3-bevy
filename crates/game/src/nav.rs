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
        app.add_systems(Update, (rebuild_upper_floors, rebuild_grid, follow_paths).chain().run_if(in_state(PlayMode::Live)));
    }
}

pub const CELL: f32 = 0.5;

/// An object footprint that blocks walking: an oriented rectangle on the ground.
#[derive(Component, Clone, Copy)]
pub struct Obstacle {
    pub half: Vec2,
    pub center_offset: Vec2,
}

/// Which floor of the house something is on: 1 = the ground floor and the yard.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Floor(pub u8);

impl Default for Floor {
    fn default() -> Self {
        Self(1)
    }
}

/// Stairs or an elevator joining `level` to `upper`: walk to `bottom`, climb (or ride) to `top`.
#[derive(Clone, Copy, Debug)]
pub struct StairLink {
    pub level: u8,
    pub upper: u8,
    pub bottom: Vec2,
    pub top: Vec2,
    pub y0: f32,
    pub y1: f32,
}

/// Walk grids of the house's upper floors (the ground floor uses [`NavGrid`]).
#[derive(Resource, Default)]
pub struct UpperFloors {
    pub grids: HashMap<u8, NavGrid>,
    pub stairs: Vec<StairLink>,
}

/// A point along a route, on a floor; `climb` = reached by stairs from height y0 to y1.
#[derive(Clone, Copy, Debug)]
pub struct Waypoint {
    pub p: Vec2,
    pub level: u8,
    pub climb: Option<(f32, f32)>,
}

/// Plans a walk from one floor to another, using stairs to change floors.
pub fn plan_route(ground: &NavGrid, upper: Option<&UpperFloors>, from: Vec2, from_level: u8, to: Vec2, to_level: u8) -> Option<Vec<Waypoint>> {
    plan_route_depth(ground, upper, from, from_level.max(1), to, to_level.max(1), 0)
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
    let grid_of = |l: u8| if l <= 1 { Some(ground) } else { upper.and_then(|u| u.grids.get(&l)) };
    if from_level == to_level {
        let pts = grid_of(from_level)?.find_path(from, to)?;
        return Some(pts.into_iter().map(|p| Waypoint { p, level: from_level, climb: None }).collect());
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
            .map(|v| v.into_iter().map(|p| Waypoint { p, level: from_level, climb: None }).collect::<Vec<_>>())
        else {
            continue;
        };
        let (exit, next_level, ys) = if up { (s.top, s.upper, (s.y0, s.y1)) } else { (s.bottom, s.level, (s.y1, s.y0)) };
        first.push(Waypoint { p: exit, level: next_level, climb: Some(ys) });
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
        let (cx, cz) = self.cell_of(p.clamp(self.origin, self.origin + Vec2::new(self.w as f32, self.h as f32) * CELL - 0.01))?;
        if !self.is_blocked(cx, cz) {
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
                    if !self.is_blocked(x, z) {
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
        (0..=steps).all(|i| self.walkable_pt(a + d * (i as f32 / steps as f32)))
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
pub fn fill_grid(grid: &mut NavGrid, world: &crate::loading::WorldInfo, obstacles: &Query<(&GlobalTransform, &Obstacle, Option<&Floor>)>) {
    let hm = &world.heightmap;
    let (w, h) = (grid.w, grid.h);
    for z in 0..h {
        for x in 0..w {
            let c = grid.center_of(x, z);
            let y = hm.sample(c.x, c.y);
            let slope = (hm.sample(c.x + 0.5, c.y) - hm.sample(c.x - 0.5, c.y))
                .abs()
                .max((hm.sample(c.x, c.y + 0.5) - hm.sample(c.x, c.y - 0.5)).abs());
            let pond = world.pond_at(c.x, c.y).is_some_and(|w| y < w + 0.1);
            grid.blocked[z * w + x] = y < world.sea_level + 0.2 || pond || slope > 0.9;
        }
    }
    for (gt, ob, floor) in obstacles {
        if floor.is_some_and(|f| f.0 > 1) {
            continue;
        }
        mark_obstacle(grid, gt, ob);
    }
}

/// Marks obstacles, steep slopes and water as unwalkable whenever the grid is dirty.
fn rebuild_grid(
    grid: Option<ResMut<NavGrid>>,
    world: Res<CurrentWorld>,
    obstacles: Query<(&GlobalTransform, &Obstacle, Option<&Floor>)>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut upper: Option<ResMut<UpperFloors>>,
) {
    let Some(mut grid) = grid else { return };
    if !grid.dirty {
        return;
    }
    grid.dirty = false;
    if let (Some(b), Some(u)) = (building.as_deref(), upper.as_deref_mut()) {
        rebuild_upper(b, u, &obstacles);
    }
    fill_grid(&mut grid, &world.data, &obstacles);
}

/// Marks the cells under an obstacle's footprint as blocked.
fn mark_obstacle(grid: &mut NavGrid, gt: &GlobalTransform, ob: &Obstacle) {
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
fn rebuild_upper(b: &crate::building::ActiveBuilding, upper: &mut UpperFloors, obstacles: &Query<(&GlobalTransform, &Obstacle, Option<&Floor>)>) {
    upper.grids.clear();
    upper.stairs = b.stairs.clone();
    for level in 2..=b.top_level {
        let mut g = NavGrid::new(b.center.xz(), b.extent + 2.0);
        g.dirty = false;
        for z in 0..g.h {
            for x in 0..g.w {
                let c = g.center_of(x, z);
                g.blocked[z * g.w + x] = b.floor_y(level, Vec3::new(c.x, 0.0, c.y)).is_none();
            }
        }
        for (gt, ob, floor) in obstacles {
            if floor.is_some_and(|f| f.0 == level) {
                mark_obstacle(&mut g, gt, ob);
            }
        }
        upper.grids.insert(level, g);
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
    pub speed: f32,
    pub done: bool,
    seg_start: Option<Vec2>,
    /// Whether a long way is run (as the game's Sims do), and whether it's being run now.
    pub run_far: bool,
    pub running: bool,
}

impl PathFollow {
    pub fn new(waypoints: Vec<Waypoint>) -> Self {
        Self { waypoints, speed: 1.45, done: false, seg_start: None, run_far: true, running: false }
    }
}

/// A walk longer than this (metres) is run, until it's nearly done; and how much faster.
const RUN_FROM: f32 = 22.0;
const RUN_UNTIL: f32 = 5.0;
const RUN_FACTOR: f32 = 2.1;

/// Standing height on a floor at a point: the house floor or the terrain.
pub fn floor_height(world: &crate::loading::WorldInfo, building: Option<&crate::building::ActiveBuilding>, level: u8, p: Vec3) -> f32 {
    match building {
        Some(b) if level > 1 => b.floor_y(level, p).unwrap_or_else(|| b.levels.get(level as usize).copied().unwrap_or(p.y)),
        _ => crate::building::walk_height(world, building, p),
    }
}

fn follow_paths(
    time: Res<Time>,
    clock: Res<GameClock>,
    world: Res<CurrentWorld>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut q: Query<(&mut Transform, &mut PathFollow, &mut SimAnim, &mut Floor, Option<&crate::sim::Sim>, Option<&crate::little::Pregnancy>), Without<crate::portraits::Staged>>,
) {
    let rate = SPEED_RATES[clock.speed];
    let dt = time.delta_secs().min(0.1) * rate;
    for (mut tf, mut pf, mut anim, mut floor, sim, pregnancy) in &mut q {
        if pf.done {
            continue;
        }
        if dt <= 0.0 {
            continue;
        }
        // A long way (on the level) is run by teens and grown-ups, until they're nearly there.
        let here = Vec2::new(tf.translation.x, tf.translation.z);
        let left: f32 = pf.waypoints.iter().scan(here, |at, w| Some(std::mem::replace(at, w.p).distance(w.p))).sum();
        // (Not heavily pregnant.)
        let can_run = pf.run_far
            && sim.is_some_and(|s| !s.age.is_little() && s.age != crate::sim::Age::Child)
            && !pregnancy.is_some_and(|p| p.stage >= 2)
            && pf.waypoints.first().is_some_and(|w| w.climb.is_none());
        pf.running = can_run && left > if pf.running { RUN_UNTIL } else { RUN_FROM };
        let mut budget = pf.speed * if pf.running { RUN_FACTOR } else { 1.0 } * dt;
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
