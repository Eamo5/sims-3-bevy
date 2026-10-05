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
        app.add_systems(Update, build_tool.run_if(in_state(PlayMode::Live)));
    }
}

/// What a wall section and a floor tile cost (floors laid when a room is closed come free).
pub const WALL_PRICE: i64 = 70;
pub const FLOOR_PRICE: i64 = 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildTool {
    Wall,
    Room,
    Floor,
    Sledgehammer,
}

impl BuildTool {
    pub const ALL: [BuildTool; 4] = [BuildTool::Wall, BuildTool::Room, BuildTool::Floor, BuildTool::Sledgehammer];

    pub fn label(self) -> String {
        match self {
            BuildTool::Wall => format!("Wall Tool\n§{WALL_PRICE} a section"),
            BuildTool::Room => format!("Room Tool\n§{WALL_PRICE} a section"),
            BuildTool::Floor => format!("Floor Tiles\n§{FLOOR_PRICE} a tile"),
            BuildTool::Sledgehammer => "Sledgehammer\nknock down walls".to_string(),
        }
    }

    pub fn help(self) -> String {
        match self {
            BuildTool::Wall => "Drag along the grid to build a wall; Ctrl+drag knocks one down. Closing off a room lays its floor.",
            BuildTool::Room => "Drag out a rectangle to build a room's four walls; Ctrl+drag knocks them down.",
            BuildTool::Floor => "Drag out a rectangle of floor tiles; Ctrl+drag takes them up.",
            BuildTool::Sledgehammer => "Drag along a wall to knock it down.",
        }
        .to_string()
            + " Page Up/Down change floors · Esc puts the tool down."
    }
}

/// Where the current drag started (a grid point, or a tile for floors).
#[derive(Default)]
struct Drag {
    start: Option<IVec2>,
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
) {
    let mut gone: Vec<Entity> = edges.iter().flat_map(|(p, q)| b.openings_on(level, p.as_vec2(), q.as_vec2())).collect();
    gone.sort();
    gone.dedup();
    for e in gone {
        let Ok((obj, tf, bought)) = objects.get(e) else { continue };
        if !bought {
            crate::save::note_removed(removed, obj, tf);
        }
        if let Some(h) = household.as_deref_mut() {
            h.funds += obj.price as i64;
        }
        notes.push(format!("{} was sold for §{}.", obj.name, obj.price));
        commands.entity(e).despawn();
    }
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
    (keys, mouse, over_ui): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, Res<PointerOverUi>),
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    mut building: Option<ResMut<ActiveBuilding>>,
    (data, mut assets): (Res<Baked>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut faces, floor_meshes, pieces): (
        Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
        Query<Entity, With<crate::building::FloorMesh>>,
        Query<(Entity, &crate::building::WallPiece)>,
    ),
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
    (objects, mut removed): (Query<(&GameObject, &Transform, Has<crate::save::Bought>)>, ResMut<crate::save::RemovedLotObjects>),
) {
    let tool = buy.tool.filter(|_| buy.active);
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    let (Some(tool), Some(b), Some(cursor)) = (tool, building.as_deref_mut(), cursor) else {
        drag.start = None;
        for (e, ..) in &label {
            commands.entity(e).despawn();
        }
        return;
    };
    if keys.just_pressed(KeyCode::Escape) {
        buy.drop_tools(&mut commands);
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
    let tiles = tool == BuildTool::Floor;
    let cur = if tiles {
        IVec2::new((local.x.floor() as i32).clamp(0, w - 1), (local.y.floor() as i32).clamp(0, d - 1))
    } else {
        IVec2::new((local.x.round() as i32).clamp(0, w), (local.y.round() as i32).clamp(0, d))
    };
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let removing = ctrl || tool == BuildTool::Sledgehammer;
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
    let (ops, cost) = plan(b, tool, removing, level, start, cur);
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
            let h = if removing { 0.3 } else { wall_h };
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
                BackgroundColor(Color::srgba(0.05, 0.15, 0.30, 0.85)),
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
    if let Some(h) = household.as_mut() {
        h.funds -= cost;
    }
    if removing && !tiles {
        sell_openings(&mut commands, b, level, &edges(tool, start, cur), &objects, &mut removed, household.as_deref_mut(), &mut notes);
    }
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces, &floor_meshes, &pieces);
    match log.as_mut() {
        Some(l) => l.0.extend(ops),
        None => commands.insert_resource(crate::building::LotPaint(ops)),
    }
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
    play.write(crate::sound::PlaySound::ui(match (tiles, removing) {
        (true, _) => "ui_build_flooring_section",
        (false, true) => "ui_build_walldelete_section",
        (false, false) => "ui_build_wall_mup",
    }));
}
