//! Grid-based pathfinding around the active lot.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::{GameClock, SPEED_RATES};
use crate::loading::CurrentWorld;
use crate::sim::{Pose, SimAnim};

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (rebuild_grid, follow_paths).chain().run_if(in_state(PlayMode::Live)));
    }
}

pub const CELL: f32 = 0.5;

/// An object footprint that blocks walking: an oriented rectangle on the ground.
#[derive(Component, Clone, Copy)]
pub struct Obstacle {
    pub half: Vec2,
    pub center_offset: Vec2,
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
                if dx != 0 && dz != 0 && (self.is_blocked(x, nz) || self.is_blocked(nx, z)) {
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
        if !found {
            return None;
        }
        let mut cells = vec![idx(goal.0, goal.1)];
        let mut cur = idx(goal.0, goal.1);
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

/// Marks obstacles, steep slopes and water as unwalkable whenever the grid is dirty.
fn rebuild_grid(
    grid: Option<ResMut<NavGrid>>,
    world: Res<CurrentWorld>,
    obstacles: Query<(&GlobalTransform, &Obstacle)>,
) {
    let Some(mut grid) = grid else { return };
    if !grid.dirty {
        return;
    }
    grid.dirty = false;
    let hm = &world.data.heightmap;
    let (w, h) = (grid.w, grid.h);
    for z in 0..h {
        for x in 0..w {
            let c = grid.center_of(x, z);
            let y = hm.sample(c.x, c.y);
            let slope = (hm.sample(c.x + 0.5, c.y) - hm.sample(c.x - 0.5, c.y))
                .abs()
                .max((hm.sample(c.x, c.y + 0.5) - hm.sample(c.x, c.y - 0.5)).abs());
            grid.blocked[z * w + x] = y < world.data.sea_level + 0.2 || slope > 0.9;
        }
    }
    for (gt, ob) in &obstacles {
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
            continue;
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

/// Walking along a computed path.
#[derive(Component, Default)]
pub struct PathFollow {
    pub waypoints: Vec<Vec2>,
    pub speed: f32,
    pub done: bool,
}

impl PathFollow {
    pub fn new(waypoints: Vec<Vec2>) -> Self {
        Self { waypoints, speed: 1.45, done: false }
    }
}

fn follow_paths(
    time: Res<Time>,
    clock: Res<GameClock>,
    world: Res<CurrentWorld>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut q: Query<(&mut Transform, &mut PathFollow, &mut SimAnim)>,
) {
    let rate = SPEED_RATES[clock.speed];
    let dt = time.delta_secs().min(0.1) * rate;
    for (mut tf, mut pf, mut anim) in &mut q {
        if pf.done {
            continue;
        }
        if dt <= 0.0 {
            continue;
        }
        let mut budget = pf.speed * dt;
        while budget > 0.0 {
            let Some(&target) = pf.waypoints.first() else {
                pf.done = true;
                break;
            };
            let pos = Vec2::new(tf.translation.x, tf.translation.z);
            let to = target - pos;
            let d = to.length();
            if d <= budget {
                tf.translation.x = target.x;
                tf.translation.z = target.y;
                budget -= d;
                pf.waypoints.remove(0);
            } else {
                let step = to / d * budget;
                tf.translation.x += step.x;
                tf.translation.z += step.y;
                budget = 0.0;
                let yaw = to.x.atan2(to.y);
                let target_rot = Quat::from_rotation_y(yaw);
                tf.rotation = tf.rotation.slerp(target_rot, (dt * 8.0).min(1.0));
            }
        }
        tf.translation.y = crate::building::walk_height(&world.data, building.as_deref(), tf.translation);
        anim.pose = if pf.done { Pose::Stand } else { Pose::Walk };
    }
}
