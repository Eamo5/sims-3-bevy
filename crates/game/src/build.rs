//! Build mode's construction tools: walls drawn along the lot's grid, a section at a time or a
//! whole room at once (closing a room lays its floor), floor tiles, and the sledgehammer. Each
//! change is a build op, logged with the repaintings and kept in saves.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use s3bake::LotBuildingBaked;

use crate::baked::Baked;
use crate::building::{ActiveBuilding, PaintOp};
use crate::buy::BuyMode;
use crate::camera::SimsCamera;
use crate::hud::PointerOverUi;
use crate::interact::{GameObject, Household, Notifications};
use crate::objects::{AssetCtx, ObjectAssets};
use crate::{AppState, PlayMode};

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, build_tool.after(crate::hud::pointer_over_ui).before(crate::buyhistory::update).run_if(in_state(PlayMode::Live)));
    }
}

/// What a wall section, a floor tile and a staircase cost (floors laid when a room is closed
/// come free).
pub const WALL_PRICE: i64 = 70;
pub const FLOOR_PRICE: i64 = 5;
pub const STAIRS_PRICE: i64 = 300;
/// What a pool tile costs to dig.
pub const POOL_PRICE: i64 = 40;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildTool {
    Wall,
    Room,
    Floor,
    Stairs,
    Sledgehammer,
    /// The fence picked on the Fences tab.
    Fence,
    /// Dig a pool out of doors on the ground floor.
    Pool,
    /// Paint the ground (the paint picked on the Terrain tab).
    Terrain,
    /// Raise, lower, flatten or smooth the ground (the tool picked on the Terrain tab).
    Sculpt,
}

impl BuildTool {
    pub const ALL: [BuildTool; 6] = [BuildTool::Wall, BuildTool::Room, BuildTool::Floor, BuildTool::Stairs, BuildTool::Pool, BuildTool::Sledgehammer];

    pub fn label(self) -> String {
        match self {
            BuildTool::Wall => format!("Wall Tool\n§{WALL_PRICE} a section"),
            BuildTool::Room => format!("Room Tool\n§{WALL_PRICE} a section"),
            BuildTool::Floor => format!("Floor Tiles\n§{FLOOR_PRICE} a tile"),
            BuildTool::Stairs => format!("Staircase\n§{STAIRS_PRICE}"),
            BuildTool::Sledgehammer => "Sledgehammer\nknock down walls".to_string(),
            BuildTool::Fence => "Fence Tool".to_string(),
            BuildTool::Pool => format!("Pool Tool\n§{POOL_PRICE} a tile"),
            BuildTool::Terrain => "Terrain Paint".to_string(),
            BuildTool::Sculpt => "Terrain Tools".to_string(),
        }
    }

    pub fn help(self) -> String {
        match self {
            BuildTool::Wall => "Drag along the grid to build a wall; Ctrl+drag knocks one down. Closing off a room lays its floor.",
            BuildTool::Room => "Drag out a rectangle to build a room's four walls; Ctrl+drag knocks them down.",
            BuildTool::Floor => "Drag out a rectangle of floor tiles; Ctrl+drag takes them up.",
            BuildTool::Stairs => "Click to put in a staircase up to the next floor (its landing gets a floor); , and . turn it; Ctrl+click takes one away.",
            BuildTool::Sledgehammer => "Drag along a wall to knock it down.",
            BuildTool::Fence => "Drag along the grid to put up the fence; Ctrl+drag takes fencing down.",
            BuildTool::Pool => "Drag out a pool on the ground out of doors; Ctrl+drag fills it in. Buy a pool ladder for Sims to swim.",
            BuildTool::Terrain => "Hold the mouse down to paint the ground on the lot.",
            BuildTool::Sculpt => "Hold the mouse down to work the ground on the lot (not under the house or things).",
        }
        .to_string()
            + " Page Up/Down change floors · Esc puts the tool down."
    }
}

/// Where the current drag started (a grid point, or a tile for floors), and which way a
/// staircase climbs.
#[derive(Default)]
struct Drag {
    start: Option<IVec2>,
    stair_dir: u8,
    context: Option<(BuildTool, u8)>,
}

impl Drag {
    /// A drag belongs to the tool and floor where it began, never a later selection.
    fn sync(&mut self, context: Option<(BuildTool, u8)>, cancel: bool) {
        if cancel || self.context != context { self.start = None; }
        self.context = context;
    }
}

#[cfg(test)]
mod drag_tests {
    use super::*;

    #[test]
    fn changing_floor_or_tool_abandons_the_previous_construction_drag() {
        let mut drag = Drag::default();
        drag.sync(Some((BuildTool::Room, 0)), false);
        drag.start = Some(IVec2::new(4, 5));
        drag.sync(Some((BuildTool::Room, 0)), false);
        assert_eq!(drag.start, Some(IVec2::new(4, 5)));
        drag.sync(Some((BuildTool::Room, 1)), false);
        assert!(drag.start.is_none());
        drag.start = Some(IVec2::new(2, 3));
        drag.sync(Some((BuildTool::Sledgehammer, 1)), false);
        assert!(drag.start.is_none());
    }

    #[test]
    fn interruption_cannot_resume_a_drag_on_mouse_release() {
        let context = Some((BuildTool::Pool, 0));
        let mut drag = Drag { start: Some(IVec2::ONE), context, stair_dir: 2 };
        drag.sync(context, true);
        drag.sync(context, false);
        assert!(drag.start.is_none());
        assert_eq!(drag.stair_dir, 2);
        drag.start = Some(IVec2::ZERO);
        drag.sync(None, false);
        assert!(drag.start.is_none());
    }
}

/// The price of what's being dragged out, next to the pointer.
#[derive(Component)]
struct CostLabel;

/// Grid edges from `a` towards `b`: straight along x or z, or on the diagonal, whichever ends
/// nearest `b`.
fn line(a: IVec2, b: IVec2) -> Vec<(IVec2, IVec2)> {
    let dv = b - a;
    let s = dv.x.abs().min(dv.y.abs());
    let ends = [IVec2::new(b.x, a.y), IVec2::new(a.x, b.y), a + dv.signum() * s];
    let end = ends.into_iter().min_by_key(|e| (*e - b).length_squared()).unwrap();
    let step = (end - a).signum();
    let mut out = Vec::new();
    let mut p = a;
    while p != end {
        out.push((p, p + step));
        p += step;
    }
    out
}

/// The edges around the rectangle with corners `a` and `b`.
fn room(a: IVec2, b: IVec2) -> Vec<(IVec2, IVec2)> {
    if a.x == b.x || a.y == b.y {
        return line(a, b);
    }
    let (c1, c3) = (IVec2::new(b.x, a.y), IVec2::new(a.x, b.y));
    [line(a, c1), line(c1, b), line(b, c3), line(c3, a)].concat()
}

/// The wall on `level` that runs along the whole of the edge from `p` to `q`, if any.
fn wall_along(b: &LotBuildingBaked, level: u8, p: Vec2, q: Vec2) -> Option<usize> {
    let on = |a: Vec2, c: Vec2, x: Vec2| {
        let d = c - a;
        let t = (x - a).dot(d) / d.length_squared();
        (-0.01..=1.01).contains(&t) && (a + d * t).distance(x) < 0.05
    };
    b.walls.iter().position(|w| {
        let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
        w.level.max(1) == level && a.distance(c) > 1e-3 && on(a, c, p) && on(a, c, q)
    })
}

/// Ops that cut the wall section from `p` to `q` free of a longer wall (applied to `sim` as
/// they're made).
fn isolate(sim: &mut LotBuildingBaked, level: u8, p: Vec2, q: Vec2, ops: &mut Vec<PaintOp>) {
    for at in [p, q] {
        let Some(i) = wall_along(sim, level, p, q) else { return };
        let w = sim.walls[i];
        if at.distance(Vec2::from(w.a)) > 0.05 && at.distance(Vec2::from(w.b)) > 0.05 {
            let op = PaintOp::SplitWall { wall: i as u32, at: at.into() };
            crate::building::apply_paint(sim, &op);
            ops.push(op);
        }
    }
}

/// Ops that take the wall section from `p` to `q` out of `sim`, applied to it as they're made.
fn knock_down(sim: &mut LotBuildingBaked, level: u8, p: Vec2, q: Vec2, ops: &mut Vec<PaintOp>) {
    isolate(sim, level, p, q, ops);
    if let Some(i) = wall_along(sim, level, p, q) {
        let op = PaintOp::RemoveWall { wall: i as u32 };
        crate::building::apply_paint(sim, &op);
        ops.push(op);
    }
}

/// Where a pool ladder goes on the pool edge nearest `p` (a point on the lot, within a tile and a
/// half of the edge): standing on the pool's floor in the middle of the edge's pool tile, facing
/// into the water (the game's ladders are modelled from the pool's floor up, their rails
/// curving out over the coping behind them).
pub fn snap_to_pool(b: &ActiveBuilding, p: Vec3) -> Option<(Vec3, Quat)> {
    let tiles: std::collections::HashSet<(i32, i32)> = b.data.pool.iter().map(|f| (f.x as i32, f.z as i32)).collect();
    let lp = b.local(p);
    let mut best: Option<(f32, (i32, i32), (i32, i32))> = None;
    for &(x, z) in &tiles {
        for d in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            if tiles.contains(&(x + d.0, z + d.1)) {
                continue;
            }
            let mid = Vec2::new(x as f32 + 0.5 + d.0 as f32 * 0.5, z as f32 + 0.5 + d.1 as f32 * 0.5);
            let dist = mid.distance(lp);
            if dist < 1.5 && best.is_none_or(|bb| dist < bb.0) {
                best = Some((dist, (x, z), d));
            }
        }
    }
    let (_, (x, z), d) = best?;
    let ground = *b.data.levels.first()?;
    let pos = b.world(x as f32 + 0.5, z as f32 + 0.5, ground + b.data.pool_depth);
    // Its front (+z) away from the edge, into the pool.
    let yaw = (-d.0 as f32).atan2(-d.1 as f32);
    Some((pos, b.rot * Quat::from_rotation_y(yaw)))
}

/// Where a door or window `tiles` wide goes in the wall under the pointer (on the floor in
/// view): half a tile out from the wall on the pointer's side, facing away from it; with the
/// ops that give it wall sections of its own.
pub fn snap_to_wall(b: &ActiveBuilding, ray: Ray3d, tiles: u32) -> Option<(Vec3, Quat, Vec<PaintOp>)> {
    let level = b.view_level;
    let y = *b.levels.get(level as usize)?;
    if ray.direction.y.abs() < 1e-4 {
        return None;
    }
    let t = (y - ray.origin.y) / ray.direction.y;
    if t <= 0.0 {
        return None;
    }
    let lp = b.local(ray.origin + *ray.direction * t);
    // The nearest straight wall along the grid.
    let mut best: Option<(f32, Vec2, Vec2)> = None;
    for w in b.data.walls.iter().filter(|w| w.level.max(1) == level) {
        let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
        let len = a.distance(c);
        if len < 0.5 {
            continue;
        }
        let along = (c - a) / len;
        if along.x.abs() > 0.01 && along.y.abs() > 0.01 {
            continue;
        }
        let s = (lp - a).dot(along);
        if s < -0.2 || s > len + 0.2 {
            continue;
        }
        let dist = (lp - a - along * s).length();
        if dist < 1.2 && best.is_none_or(|bb| dist < bb.0) {
            best = Some((dist, a, along));
        }
    }
    let (_, a, along) = best?;
    let axis = if along.x.abs() > 0.5 { 0 } else { 1 };
    let dir = if axis == 0 { Vec2::X } else { Vec2::Y };
    // Its middle on a tile's middle (odd widths) or a grid point (even), on the wall's line.
    let mut mid = a + along * (lp - a).dot(along);
    mid[axis] = if tiles % 2 == 1 { mid[axis].floor() + 0.5 } else { mid[axis].round() };
    mid[1 - axis] = mid[1 - axis].round();
    let mut sim = b.data.clone();
    let mut ops = Vec::new();
    for k in 0..tiles {
        let p = mid - dir * (tiles as f32 * 0.5) + dir * k as f32;
        let q = p + dir;
        wall_along(&sim, level, p, q)?;
        isolate(&mut sim, level, p, q, &mut ops);
    }
    let n = dir.perp();
    let fwd = if (lp - mid).dot(n) >= 0.0 { n } else { -n };
    let at = mid + fwd * 0.5;
    let f3 = b.rot * Vec3::new(fwd.x, 0.0, fwd.y);
    Some((b.world(at.x, at.y, y), Quat::from_rotation_y(f3.x.atan2(f3.z)), ops))
}

/// The wall sections a drag with the wall, room or sledgehammer tool goes along.
pub fn edges(tool: BuildTool, start: IVec2, cur: IVec2) -> Vec<(IVec2, IVec2)> {
    if tool == BuildTool::Room { room(start, cur) } else { line(start, cur) }
}

/// Doors and windows in wall sections being knocked down go back to the household (sold).
#[allow(clippy::too_many_arguments)]
pub fn sell_openings(
    commands: &mut Commands,
    b: &ActiveBuilding,
    level: u8,
    edges: &[(IVec2, IVec2)],
    objects: &Query<(&GameObject, &Transform, Has<crate::save::Bought>)>,
    removed: &mut crate::save::RemovedLotObjects,
    mut household: Option<&mut Household>,
    notes: &mut Notifications,
) -> Vec<Entity> {
    let mut gone: Vec<Entity> = edges.iter().flat_map(|(p, q)| b.openings_on(level, p.as_vec2(), q.as_vec2())).collect();
    gone.sort();
    gone.dedup();
    let mut sold = Vec::new();
    for e in gone {
        let Ok((obj, tf, bought)) = objects.get(e) else { continue };
        if !bought {
            crate::save::note_removed(removed, obj, tf);
        }
        if let Some(h) = household.as_deref_mut() {
            h.funds += obj.price as i64;
        }
        notes.push(format!("{} was sold for §{}.", obj.name, obj.price));
        crate::buyhistory::remember_pickup(commands, e);
        crate::buyhistory::park(commands, e);
        sold.push(e);
    }
    sold
}

/// A staircase on `level` from tile `at` climbing along `dir`: its ops (opening the stairwell
/// above and flooring the landing), or why it can't go there. Taking one away instead when
/// `removing`.
pub fn plan_stairs(b: &ActiveBuilding, level: u8, at: IVec2, dir: u8, removing: bool) -> Result<Vec<PaintOp>, &'static str> {
    if removing {
        return match b.built_stairs.iter().find(|s| s.level == level && s.tiles().contains(&at)) {
            Some(_) => Ok(vec![PaintOp::RemoveStairs { x: at.x as u16, z: at.y as u16, level }]),
            None => Err("There's no staircase you built there."),
        };
    }
    let s = crate::building::BuiltStairs { x: at.x as u16, z: at.y as u16, dir, level };
    let (w, d) = (b.data.width as i32, b.data.depth as i32);
    let inside = |t: IVec2| t.x >= 0 && t.y >= 0 && t.x < w && t.y < d;
    let tiles = s.tiles();
    let landing = s.landing();
    if !tiles.iter().all(|t| inside(*t)) || !inside(landing) {
        return Err("The staircase doesn't fit on the lot there.");
    }
    let has_floor = |l: u8, t: IVec2| b.data.floors.iter().any(|f| f.level == l && f.x as i32 == t.x && f.z as i32 == t.y);
    if level > 1 && !tiles.iter().all(|t| has_floor(level, *t)) {
        return Err("Stairs upstairs need floor under them.");
    }
    if b.built_stairs.iter().any(|o| o.level == level && o.tiles().iter().any(|t| tiles.contains(t))) {
        return Err("There's a staircase there already.");
    }
    // Nothing walled across the way up (or onto the landing).
    let step = crate::building::BuiltStairs::step(dir);
    let across = |l: u8, from: IVec2| {
        // The grid edge between tile `from` and the next one up the stairs.
        let c = from.as_vec2() + Vec2::splat(0.5) + step.as_vec2() * 0.5;
        let half = step.as_vec2().perp() * 0.5;
        wall_along(&b.data, l, c - half, c + half).is_some()
    };
    if tiles[..tiles.len() - 1].iter().any(|t| across(level, *t)) || across(level + 1, tiles[tiles.len() - 1]) {
        return Err("A wall is in the way of the staircase.");
    }
    let mut ops = vec![PaintOp::AddStairs { x: s.x, z: s.z, dir, level }];
    // The stairwell: no floor above the steps; a landing at the top.
    for t in &tiles {
        if has_floor(level + 1, *t) {
            ops.push(PaintOp::RemoveFloor { level: level + 1, x: t.x as u16, z: t.y as u16 });
        }
    }
    if !has_floor(level + 1, landing) {
        ops.push(PaintOp::AddFloor { level: level + 1, x: landing.x as u16, z: landing.y as u16, region: 0 });
    }
    Ok(ops)
}

/// A pool dug over the tiles from `start` to `cur` (or filled in), on the ground out of doors:
/// its ops and what it costs.
pub fn plan_pool(b: &ActiveBuilding, removing: bool, level: u8, start: IVec2, cur: IVec2) -> (Vec<PaintOp>, i64) {
    if level != 1 {
        return (Vec::new(), 0);
    }
    let (lo, hi) = (start.min(cur), start.max(cur));
    let mut ops = Vec::new();
    for z in lo.y..=hi.y {
        for x in lo.x..=hi.x {
            let (x, z) = (x as u16, z as u16);
            let pool = b.data.pool.iter().any(|f| f.x == x && f.z == z);
            if removing {
                if pool {
                    ops.push(PaintOp::RemovePool { x, z });
                }
                continue;
            }
            // (Not under a floor at any level, a basement's or a garage's too, nor a staircase.)
            let floored = b.data.floors.iter().any(|f| f.x == x && f.z == z);
            let stairs = b.built_stairs.iter().any(|s| s.tiles().contains(&IVec2::new(x as i32, z as i32)));
            if !pool && !floored && !stairs {
                ops.push(PaintOp::AddPool { x, z });
            }
        }
    }
    let cost = if removing { 0 } else { ops.len() as i64 * POOL_PRICE };
    (ops, cost)
}

/// A fence along the grid from `start` to `cur` (or taken down): its ops and what it costs.
/// Out of doors on the ground floor it stands on the ground.
pub fn plan_fence(b: &ActiveBuilding, style: &s3bake::gamedata::FenceStyle, removing: bool, level: u8, start: IVec2, cur: IVec2) -> (Vec<PaintOp>, i64) {
    let mut ops = Vec::new();
    for (p, q) in line(start, cur) {
        let (pv, qv) = (p.as_vec2(), q.as_vec2());
        let mid = (pv + qv) * 0.5;
        let on_floor = |l: u8| {
            let n = (qv - pv).perp().normalize_or_zero() * 0.5;
            [mid + n, mid - n].iter().any(|c| b.data.floors.iter().any(|f| f.level == l && f.x as f32 == c.x.floor() && f.z as f32 == c.y.floor()))
        };
        let at = if level == 1 && !on_floor(1) { 0 } else { level };
        if removing {
            if b.data.fences.iter().any(|f| f.level == at && f.a != f.b && {
                let (a, c) = (Vec2::from(f.a), Vec2::from(f.b));
                (a.distance(pv) < 0.05 && c.distance(qv) < 0.05) || (a.distance(qv) < 0.05 && c.distance(pv) < 0.05)
            }) {
                ops.push(PaintOp::RemoveFence { a: pv.into(), b: qv.into(), level: at });
            }
            continue;
        }
        // (Not through a wall.)
        if wall_along(&b.data, level.max(1), pv, qv).is_some() {
            continue;
        }
        let diagonal = p.x != q.x && p.y != q.y;
        let Some(model) = (if diagonal { style.diagonal } else { style.straight }) else { continue };
        ops.push(PaintOp::AddFence { a: pv.into(), b: qv.into(), level: at, model, post: style.post });
    }
    let cost = if removing { 0 } else { ops.len() as i64 * style.price.max(1) as i64 };
    (ops, cost)
}

/// The change a drag makes: its ops and what they cost.
pub fn plan(b: &ActiveBuilding, tool: BuildTool, removing: bool, level: u8, start: IVec2, cur: IVec2) -> (Vec<PaintOp>, i64) {
    let mut sim = b.data.clone();
    let mut ops = Vec::new();
    if tool == BuildTool::Floor {
        let (lo, hi) = (start.min(cur), start.max(cur));
        let mut cost = 0;
        for z in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let (x, z) = (x as u16, z as u16);
                let tile = sim.floors.iter().find(|f| f.level == level && f.x == x && f.z == z);
                // (Not over a stairwell.)
                let well = b.built_stairs.iter().any(|s| s.level + 1 == level && s.tiles().contains(&IVec2::new(x as i32, z as i32)));
                if well && !removing {
                    continue;
                }
                if removing && tile.is_some() {
                    ops.push(PaintOp::RemoveFloor { level, x, z });
                } else if !removing && tile.is_none_or(|t| t.mask != 0xF) {
                    ops.push(PaintOp::AddFloor { level, x, z, region: 0 });
                    cost += FLOOR_PRICE;
                }
            }
        }
        return (ops, cost);
    }
    let edges = edges(tool, start, cur);
    if removing {
        for (p, q) in edges {
            knock_down(&mut sim, level, p.as_vec2(), q.as_vec2(), &mut ops);
        }
        return (ops, 0);
    }
    for (p, q) in edges {
        let (p, q) = (p.as_vec2(), q.as_vec2());
        if wall_along(&sim, level, p, q).is_some() {
            continue;
        }
        // Upstairs, walls stand on floor.
        if level > 1 {
            let mid = (p + q) * 0.5;
            let n = (q - p).perp();
            let has = |c: Vec2| sim.floors.iter().any(|f| f.level == level && f.x as f32 == c.x.floor() && f.z as f32 == c.y.floor());
            if !has(mid + n * 0.5) && !has(mid - n * 0.5) {
                continue;
            }
        }
        let op = PaintOp::AddWall { a: p.into(), b: q.into(), level };
        crate::building::apply_paint(&mut sim, &op);
        ops.push(op);
    }
    let cost = ops.len() as i64 * WALL_PRICE;
    if !ops.is_empty() {
        ops.extend(crate::building::room_floors(&sim, level));
    }
    (ops, cost)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn build_tool(
    mut commands: Commands,
    mut buy: ResMut<BuyMode>,
    (keys, mouse, over_ui, menu, modal): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, Res<PointerOverUi>, Res<crate::options::GameMenu>, Query<(), With<crate::dialog::Modal>>),
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    mut building: Option<ResMut<ActiveBuilding>>,
    (data, mut assets): (Res<Baked>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut faces: Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
    (mut household, mut notes, mut log, mut play): (
        Option<ResMut<Household>>,
        ResMut<Notifications>,
        Option<ResMut<crate::building::LotPaint>>,
        MessageWriter<crate::sound::PlaySound>,
    ),
    mut grid: Option<ResMut<crate::nav::NavGrid>>,
    mut gizmos: Gizmos,
    mut drag: Local<Drag>,
    mut label: Query<(Entity, &mut Node, &mut Text), With<CostLabel>>,
    (objects, mut removed, ui): (
        Query<(&GameObject, &Transform, Has<crate::save::Bought>)>,
        ResMut<crate::save::RemovedLotObjects>,
        Option<Res<crate::icons::GameUi>>,
    ),
) {
    let blocked = menu.is_open() || !modal.is_empty();
    let cancel = blocked || keys.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right)
        || over_ui.0 && mouse.just_released(MouseButton::Left);
    let context = buy.tool.filter(|_| buy.active).zip(building.as_ref().map(|b| b.view_level));
    let interrupted = cancel || drag.context != context;
    if std::env::var_os("BUILD_HISTORY_TEST").is_some() && (mouse.just_pressed(MouseButton::Left) || mouse.just_released(MouseButton::Left)) {
        info!("autotest: build input context {context:?}, start {:?}, UI {}, interrupted {interrupted}, pressed {}, released {}", drag.start, over_ui.0, mouse.just_pressed(MouseButton::Left), mouse.just_released(MouseButton::Left));
    }
    drag.sync(context, cancel);
    if interrupted {
        for (e, ..) in &label { commands.entity(e).despawn(); }
    }
    if blocked || !buy.active {
        buy.roof_pick = None;
        return;
    }
    if interrupted { return; }
    // A roof pattern chosen on the Roofs tab.
    if let Some(i) = buy.roof_pick.take()
        && let (Some(b), Some(r)) = (building.as_deref_mut(), ui.as_ref().and_then(|u| u.data.roofs.get(i)))
    {
        let ops = vec![PaintOp::Roof { texture: r.tile }];
        let before = crate::building::BuildingSnapshot::capture(b, log.as_deref());
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
        crate::buyhistory::record_construction(&mut commands, before, b, &ops, 0, removed.0.clone(), Vec::new());
        match log.as_mut() {
            Some(l) => l.0.extend(ops),
            None => commands.insert_resource(crate::building::LotPaint(ops)),
        }
        play.write(crate::sound::PlaySound::ui("ui_build_roof_mup"));
        notes.push(format!("The roof is now {}.", r.name));
    }
    let tool = buy.tool.filter(|_| buy.active);
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    let (Some(tool), Some(b), Some(cursor)) = (tool, building.as_deref_mut(), cursor) else {
        drag.start = None;
        for (e, ..) in &label {
            commands.entity(e).despawn();
        }
        return;
    };
    // (The terrain brush is the terrain paint module's.)
    if matches!(tool, BuildTool::Terrain | BuildTool::Sculpt) {
        drag.start = None;
        return;
    }
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    let level = b.view_level;
    let y = b.levels.get(level as usize).copied().unwrap_or(b.levels[b.levels.len() - 1]);
    if ray.direction.y.abs() < 1e-4 {
        return;
    }
    let t = (y - ray.origin.y) / ray.direction.y;
    if t <= 0.0 {
        return;
    }
    let local = b.local(ray.origin + *ray.direction * t);
    let (w, d) = (b.data.width as i32, b.data.depth as i32);
    let tiles = matches!(tool, BuildTool::Floor | BuildTool::Stairs | BuildTool::Pool);
    let cur = if tiles {
        IVec2::new((local.x.floor() as i32).clamp(0, w - 1), (local.y.floor() as i32).clamp(0, d - 1))
    } else {
        IVec2::new((local.x.round() as i32).clamp(0, w), (local.y.round() as i32).clamp(0, d))
    };
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let removing = ctrl || tool == BuildTool::Sledgehammer;
    if tool == BuildTool::Stairs {
        if keys.just_pressed(KeyCode::Comma) {
            drag.stair_dir = (drag.stair_dir + 1) % 4;
        }
        if keys.just_pressed(KeyCode::Period) {
            drag.stair_dir = (drag.stair_dir + 3) % 4;
        }
        let plan = plan_stairs(b, level, cur, drag.stair_dir, removing);
        // The staircase's footprint and its landing, green where it fits.
        let color = match (&plan, removing) {
            (Err(_), _) => Color::srgb(0.9, 0.2, 0.2),
            (Ok(_), true) => Color::srgb(1.0, 0.35, 0.25),
            (Ok(_), false) => Color::srgb(0.35, 1.0, 0.45),
        };
        let at = |p: Vec2, h: f32| b.world(p.x, p.y, y + h);
        let s = crate::building::BuiltStairs { x: cur.x as u16, z: cur.y as u16, dir: drag.stair_dir, level };
        let step = crate::building::BuiltStairs::step(drag.stair_dir).as_vec2();
        let side = step.perp() * 0.5;
        let bottom = cur.as_vec2() + Vec2::splat(0.5) - step * 0.5;
        let top = bottom + step * crate::building::STAIR_RUN as f32;
        let rise = s3bake::building::LEVEL_HEIGHT;
        if !removing {
            gizmos.line(at(bottom - side, 0.03), at(top - side, rise), color);
            gizmos.line(at(bottom + side, 0.03), at(top + side, rise), color);
            gizmos.line(at(bottom - side, 0.03), at(bottom + side, 0.03), color);
            gizmos.line(at(top - side, rise), at(top + side, rise), color);
            let l = s.landing().as_vec2();
            for (a, q) in [(l, l + Vec2::X), (l + Vec2::X, l + Vec2::ONE), (l + Vec2::ONE, l + Vec2::Y), (l + Vec2::Y, l)] {
                gizmos.line(at(a, rise + 0.03), at(q, rise + 0.03), color.with_alpha(0.6));
            }
        } else {
            let p = cur.as_vec2();
            for (a, q) in [(p, p + Vec2::X), (p + Vec2::X, p + Vec2::ONE), (p + Vec2::ONE, p + Vec2::Y), (p + Vec2::Y, p)] {
                gizmos.line(at(a, 0.03), at(q, 0.03), color);
            }
        }
        for (e, ..) in &label {
            commands.entity(e).despawn();
        }
        drag.start = None;
        if !mouse.just_pressed(MouseButton::Left) || over_ui.0 {
            return;
        }
        let ops = match plan {
            Ok(ops) => ops,
            Err(why) => {
                notes.push(why);
                return;
            }
        };
        let cost = if removing { 0 } else { STAIRS_PRICE };
        if household.as_ref().is_some_and(|h| h.funds < cost) {
            notes.push("You can't afford that.");
            return;
        }
        let before = crate::building::BuildingSnapshot::capture(b, log.as_deref());
        if let Some(h) = household.as_mut() {
            h.funds -= cost;
        }
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
        crate::buyhistory::record_construction(&mut commands, before, b, &ops, -cost, removed.0.clone(), Vec::new());
        match log.as_mut() {
            Some(l) => l.0.extend(ops),
            None => commands.insert_resource(crate::building::LotPaint(ops)),
        }
        if let Some(g) = grid.as_mut() {
            g.dirty = true;
        }
        play.write(crate::sound::PlaySound::ui(if removing { "ui_build_walldelete_section" } else { "ui_build_stair_plop" }));
        return;
    }
    if mouse.just_pressed(MouseButton::Left) && !over_ui.0 {
        drag.start = Some(cur);
        play.write(crate::sound::PlaySound::ui(match (tiles, removing) {
            (true, _) => "ui_build_flooring_mdown",
            (false, true) => "ui_build_delete_tool_mdown",
            (false, false) => "ui_build_wall_mdown",
        }));
    }
    let at = |p: Vec2, h: f32| b.world(p.x, p.y, y + h);
    let wall_h = s3bake::building::LEVEL_HEIGHT;
    let Some(start) = drag.start else {
        // The grid point (or tile) under the pointer.
        let c = Color::srgba(1.0, 1.0, 1.0, 0.8);
        if tiles {
            let p = cur.as_vec2();
            for (a, q) in [(p, p + Vec2::X), (p + Vec2::X, p + Vec2::ONE), (p + Vec2::ONE, p + Vec2::Y), (p + Vec2::Y, p)] {
                gizmos.line(at(a, 0.03), at(q, 0.03), c);
            }
        } else {
            let p = cur.as_vec2();
            gizmos.line(at(p, 0.0), at(p, wall_h), c);
            gizmos.circle(Isometry3d::new(at(p, 0.02), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), 0.2, c);
        }
        for (e, ..) in &label {
            commands.entity(e).despawn();
        }
        return;
    };
    let fence = buy.fence.and_then(|i| ui.as_ref()?.data.fences.get(i).cloned());
    let (ops, cost) = match (&fence, tool) {
        (Some(style), BuildTool::Fence) => plan_fence(b, style, removing, level, start, cur),
        (None, BuildTool::Fence) => (Vec::new(), 0),
        (_, BuildTool::Pool) => plan_pool(b, removing, level, start, cur),
        _ => plan(b, tool, removing, level, start, cur),
    };
    let funds = household.as_ref().map_or(i64::MAX, |h| h.funds);
    let color = if removing {
        Color::srgb(1.0, 0.35, 0.25)
    } else if cost > funds {
        Color::srgb(0.9, 0.2, 0.2)
    } else {
        Color::srgb(0.35, 1.0, 0.45)
    };
    // The outline of what the drag would build or take away.
    if tiles {
        let (lo, hi) = (start.min(cur).as_vec2(), (start.max(cur) + IVec2::ONE).as_vec2());
        let corners = [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)];
        for k in 0..4 {
            gizmos.line(at(corners[k], 0.03), at(corners[(k + 1) % 4], 0.03), color);
        }
        for z in lo.y as i32..hi.y as i32 {
            for x in lo.x as i32..hi.x as i32 {
                let p = Vec2::new(x as f32, z as f32);
                gizmos.line(at(p, 0.03), at(p + Vec2::ONE, 0.03), color.with_alpha(0.5));
            }
        }
    } else {
        for (p, q) in edges(tool, start, cur) {
            let (p, q) = (p.as_vec2(), q.as_vec2());
            let h = if removing { 0.3 } else if tool == BuildTool::Fence { 1.0 } else { wall_h };
            gizmos.line(at(p, 0.02), at(q, 0.02), color);
            gizmos.line(at(p, h), at(q, h), color);
            gizmos.line(at(p, 0.0), at(p, h), color);
            gizmos.line(at(q, 0.0), at(q, h), color);
        }
    }
    // Its price by the pointer.
    let price = if removing { "Remove".to_string() } else { format!("§{cost}") };
    let (left, top) = (Val::Px(cursor.x + 18.0), Val::Px(cursor.y + 12.0));
    match label.iter_mut().next() {
        Some((_, mut node, mut text)) => {
            node.left = left;
            node.top = top;
            text.0 = price;
        }
        None => {
            commands.spawn((
                CostLabel,
                Text::new(price),
                TextFont::from_font_size(16.0),
                TextColor(Color::WHITE),
                Node { position_type: PositionType::Absolute, left, top, padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(6.0)), ..default() },
                BackgroundColor(crate::menu::PANEL_BG),
                Pickable::IGNORE,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    drag.start = None;
    if ops.is_empty() {
        return;
    }
    if cost > funds {
        notes.push("You can't afford that.");
        return;
    }
    let before = crate::building::BuildingSnapshot::capture(b, log.as_deref());
    let removed_before = removed.0.clone();
    let funds_before = household.as_ref().map_or(0, |h| h.funds);
    if let Some(h) = household.as_mut() {
        h.funds -= cost;
    }
    let sold = if removing && !tiles && tool != BuildTool::Fence {
        sell_openings(&mut commands, b, level, &edges(tool, start, cur), &objects, &mut removed, household.as_deref_mut(), &mut notes)
    } else { Vec::new() };
    let funds = household.as_ref().map_or(0, |h| h.funds - funds_before);
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
    crate::buyhistory::record_construction(&mut commands, before, b, &ops, funds, removed_before, sold);
    match log.as_mut() {
        Some(l) => l.0.extend(ops),
        None => commands.insert_resource(crate::building::LotPaint(ops)),
    }
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
    play.write(crate::sound::PlaySound::ui(match (tiles, removing) {
        (true, _) if tool == BuildTool::Pool => "ui_build_pool_mdown",
        (true, _) => "ui_build_flooring_section",
        (false, true) => "ui_build_walldelete_section",
        (false, false) if tool == BuildTool::Fence => "ui_build_rail_plop",
        (false, false) => "ui_build_wall_mup",
    }));
}
