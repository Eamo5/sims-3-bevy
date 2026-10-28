//! Blueprint mode (the base game's, UI.dll's `BlueprintController`): ready-made rooms, the
//! catalogue's (`bp…` objects, each a `Blueprint` of walls, floors, furniture, doors and windows:
//! `gamedata::BlueprintInfo`), chosen from the Build panel's blueprint catalogue by room kind (the
//! game's `BlueprintCatalog` tabs) and put down whole on the household's lot. The room follows the
//! pointer on the floor in view, outlined (green where it fits and is affordable, red where not),
//! turned a quarter at a time with `,` and `.`; a click builds it: its walls (each side in its own
//! covering), its floors (in theirs, the room closed in), its doors and windows set in its walls
//! and its furniture placed (what goes on surfaces in their slots), all for the blueprint's price.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::build::BuildTool;
use crate::building::{ActiveBuilding, PaintOp};
use crate::buy::BuyMode;
use crate::camera::SimsCamera;
use crate::hud::PointerOverUi;
use crate::objects::{AssetCtx, ObjectAssets};

pub struct BlueprintsPlugin;

impl Plugin for BlueprintsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, place_blueprint.after(crate::hud::pointer_over_ui).before(crate::buyhistory::update).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// A point of a blueprint (about its middle) turned a quarter `turns` times, as objects are by
/// `Quat::from_rotation_y`.
pub fn turn(v: Vec2, turns: u8) -> Vec2 {
    match turns % 4 {
        0 => v,
        1 => Vec2::new(v.y, -v.x),
        2 => -v,
        _ => Vec2::new(-v.y, v.x),
    }
}

/// The lot tiles a blueprint covers put down at `origin` (lot-local, a grid point), turned.
pub fn tiles(bp: &s3bake::gamedata::BlueprintInfo, origin: Vec2, turns: u8) -> Vec<IVec2> {
    bp.floors.iter().map(|f| (origin + turn(Vec2::new(f.x as f32 + 0.5, f.z as f32 + 0.5), turns)).floor().as_ivec2()).collect()
}

/// Whether a blueprint goes there: all its tiles on the lot, none where there's a floor already
/// (on that storey), nor a wall across them.
pub fn fits(b: &s3bake::LotBuildingBaked, bp: &s3bake::gamedata::BlueprintInfo, origin: Vec2, turns: u8, level: u8) -> bool {
    let ts = tiles(bp, origin, turns);
    let (w, d) = (b.width as i32, b.depth as i32);
    let on_lot = ts.iter().all(|t| t.x >= 0 && t.y >= 0 && t.x < w && t.y < d);
    let floored = ts.iter().any(|t| b.floors.iter().any(|f| f.level == level && f.x as i32 == t.x && f.z as i32 == t.y && f.region > 0));
    on_lot && !floored && !ts.is_empty()
}

/// A blueprint's picture for the catalogue (the game draws its own of the room; the catalogue
/// keeps only a placeholder): its plan from above, the floor pale, the furniture in it and the
/// walls dark round it, doors and windows marked in them.
pub fn plan_picture(bp: &s3bake::gamedata::BlueprintInfo) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const S: usize = 96;
    let pts = bp.walls.iter().flat_map(|w| [Vec2::from(w.a), Vec2::from(w.b)]).chain(bp.floors.iter().flat_map(|f| [Vec2::new(f.x as f32, f.z as f32), Vec2::new(f.x as f32 + 1.0, f.z as f32 + 1.0)]));
    let (lo, hi) = pts.fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(a, b), p| (a.min(p), b.max(p)));
    let size = (hi - lo).max_element().max(1.0);
    let scale = (S as f32 - 12.0) / size;
    let off = Vec2::splat(S as f32 * 0.5) - (lo + hi) * 0.5 * scale;
    let to_px = |p: Vec2| p * scale + off;
    let mut px = vec![0u8; S * S * 4];
    let mut put = |x: i32, y: i32, c: [u8; 4]| {
        if (0..S as i32).contains(&x) && (0..S as i32).contains(&y) {
            let i = (y as usize * S + x as usize) * 4;
            px[i..i + 4].copy_from_slice(&c);
        }
    };
    // The floor.
    for f in &bp.floors {
        let (a, b) = (to_px(Vec2::new(f.x as f32, f.z as f32)), to_px(Vec2::new(f.x as f32 + 1.0, f.z as f32 + 1.0)));
        for y in a.y as i32..b.y.ceil() as i32 {
            for x in a.x as i32..b.x.ceil() as i32 {
                put(x, y, [232, 222, 200, 255]);
            }
        }
    }
    // The furniture: a block for each, about where it stands.
    for o in bp.objects.iter().filter(|o| !o.in_wall) {
        let c = to_px(Vec2::new(o.at[0], o.at[2]));
        let r = (scale * 0.3) as i32;
        for y in -r..=r {
            for x in -r..=r {
                put(c.x as i32 + x, c.y as i32 + y, [120, 140, 175, 255]);
            }
        }
    }
    // The walls, and their doors and windows.
    let mut line = |a: Vec2, b: Vec2, c: [u8; 4], w: i32| {
        let n = ((b - a).length().ceil() as i32).max(1);
        for k in 0..=n {
            let p = a.lerp(b, k as f32 / n as f32);
            for dy in -w..=w {
                for dx in -w..=w {
                    put(p.x as i32 + dx, p.y as i32 + dy, c);
                }
            }
        }
    };
    for w in &bp.walls {
        line(to_px(Vec2::from(w.a)), to_px(Vec2::from(w.b)), [40, 52, 80, 255], 1);
    }
    for o in bp.objects.iter().filter(|o| o.in_wall) {
        let fwd = Vec2::new(o.facing.sin(), o.facing.cos());
        let at = Vec2::new(o.at[0], o.at[2]) - fwd * 0.5;
        let side = fwd.perp() * 0.35;
        line(to_px(at - side), to_px(at + side), [250, 250, 250, 255], 1);
    }
    Image::new(Extent3d { width: S as u32, height: S as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// The ops building a blueprint's walls, floors and coverings, as a construction (the walls
/// first, then the room's floors as closing it lays them, the rest of its tiles, and the
/// coverings), simulated on a copy of the lot as they go.
pub fn plan(b: &s3bake::LotBuildingBaked, bp: &s3bake::gamedata::BlueprintInfo, origin: Vec2, turns: u8, level: u8) -> Vec<PaintOp> {
    let mut sim = b.clone();
    let mut ops = Vec::new();
    let at = |v: [f32; 2]| (origin + turn(Vec2::from(v), turns)).round();
    let mut push = |sim: &mut s3bake::LotBuildingBaked, ops: &mut Vec<PaintOp>, op: PaintOp| {
        crate::building::apply_paint(sim, &op);
        ops.push(op);
    };
    for w in &bp.walls {
        let (p, q) = (at(w.a), at(w.b));
        if crate::build::wall_along(&sim, level, p, q).is_none() {
            push(&mut sim, &mut ops, PaintOp::AddWall { a: p.into(), b: q.into(), level });
        }
    }
    for op in crate::building::room_floors(&sim, level) {
        push(&mut sim, &mut ops, op);
    }
    for (f, t) in bp.floors.iter().zip(tiles(bp, origin, turns)) {
        let (x, z) = (t.x as u16, t.y as u16);
        if !sim.floors.iter().any(|g| g.level == level && g.x == x && g.z == z) {
            push(&mut sim, &mut ops, PaintOp::AddFloor { level, x, z, region: 0, mask: 0xF });
        }
        if let Some(texture) = f.cover {
            push(&mut sim, &mut ops, PaintOp::Floor { level, x, z, texture });
        }
    }
    // (Each side's covering on the side the blueprint has it, however the wall runs.)
    for w in &bp.walls {
        let (p, q) = (at(w.a), at(w.b));
        let Some(i) = crate::build::wall_along(&sim, level, p, q) else { continue };
        let wall = sim.walls[i];
        let n_bp = (q - p).perp();
        let n_wall = (Vec2::from(wall.b) - Vec2::from(wall.a)).perp();
        let same = n_bp.dot(n_wall) > 0.0;
        for (k, cover) in w.cover.iter().enumerate() {
            let Some(texture) = cover else { continue };
            let side = if same { k } else { 1 - k } as u8;
            push(&mut sim, &mut ops, PaintOp::Wall { wall: i as u32, side, texture: *texture });
        }
    }
    ops
}

/// The blueprint in hand: outlined where it would go, turned by `,` and `.`, built by a click.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn place_blueprint(
    mut commands: Commands,
    mut buy: ResMut<BuyMode>,
    (keys, mouse, over_ui): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, Res<PointerOverUi>),
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    mut building: Option<ResMut<ActiveBuilding>>,
    (gu, catalog, data): (Option<Res<crate::icons::GameUi>>, Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    (mut assets, mut meshes, mut images, mut mats): (ResMut<ObjectAssets>, ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut faces: Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
    (mut log, removed, mut household, mut notes, mut play): (
        Option<ResMut<crate::building::LotPaint>>,
        Res<crate::save::RemovedLotObjects>,
        Option<ResMut<crate::interact::Household>>,
        ResMut<crate::interact::Notifications>,
        MessageWriter<crate::sound::PlaySound>,
    ),
    mut grid: Option<ResMut<crate::nav::NavGrid>>,
    mut gizmos: Gizmos,
    mut test: Local<bool>,
) {
    // (BLUEPRINT_TEST=<n>,<x>,<z>[,<turns>] (tests): the nth blueprint built there, ten seconds in.)
    let test_at = std::env::var("BLUEPRINT_TEST").ok().filter(|_| !*test).and_then(|v| {
        let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (n.len() >= 3).then(|| (n[0] as usize, Vec2::new(n[1], n[2]), n.get(3).copied().unwrap_or(0.0) as u8))
    });
    let (Some(gu), Some(b)) = (gu, building.as_deref_mut()) else { return };
    let held = buy.active && buy.tool == Some(BuildTool::Blueprint);
    let (i, origin, turns, click) = match test_at {
        Some((i, at, t)) => {
            if catalog.entries.is_empty() {
                return;
            }
            *test = true;
            (i, at, t, true)
        }
        None => {
            if !held {
                return;
            }
            let Some(i) = buy.blueprint else { return };
            if keys.just_pressed(KeyCode::Comma) {
                buy.blueprint_turns = (buy.blueprint_turns + 1) % 4;
            }
            if keys.just_pressed(KeyCode::Period) {
                buy.blueprint_turns = (buy.blueprint_turns + 3) % 4;
            }
            // Where the pointer meets the floor in view.
            let cursor = windows.single().ok().and_then(|w| w.cursor_position());
            let (Some(cursor), Ok((camera, cam_tf))) = (cursor, cams.single()) else { return };
            let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
            let y = b.levels.get(b.view_level as usize).copied().unwrap_or(0.0);
            let Some(t) = ray.intersect_plane(Vec3::new(0.0, y, 0.0), InfinitePlane3d::new(Vec3::Y)) else { return };
            let local = b.local(ray.get_point(t));
            (i, local.round(), buy.blueprint_turns, mouse.just_pressed(MouseButton::Left) && !over_ui.0)
        }
    };
    let Some(bp) = gu.data.blueprints.get(i).cloned() else { return };
    let level = b.view_level.max(1);
    let y = b.levels.get(level as usize).copied().unwrap_or(0.0) + 0.05;
    let funds = household.as_ref().map_or(i64::MAX, |h| h.funds);
    let ok = fits(&b.data, &bp, origin, turns, level) && funds >= bp.price as i64;
    // Its outline: its walls, and its floor's edge.
    let colour = if ok { crate::menu::PLUMBOB_GREEN } else { Color::srgb(0.9, 0.3, 0.25) };
    for w in &bp.walls {
        let (p, q) = (origin + turn(Vec2::from(w.a), turns), origin + turn(Vec2::from(w.b), turns));
        for h in [0.0, 1.2, 2.4] {
            gizmos.line(b.world(p.x, p.y, y + h), b.world(q.x, q.y, y + h), colour);
        }
    }
    for t in tiles(&bp, origin, turns) {
        let c = t.as_vec2() + Vec2::splat(0.5);
        gizmos.line(b.world(c.x - 0.1, c.y, y), b.world(c.x + 0.1, c.y, y), colour.with_alpha(0.5));
    }
    if !click {
        return;
    }
    if !ok {
        notes.push(if funds < bp.price as i64 { "You can't afford that." } else { "The room doesn't fit there." });
        return;
    }
    // The room built.
    let ops = plan(&b.data, &bp, origin, turns, level);
    let before = crate::building::BuildingSnapshot::capture(b, log.as_deref());
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
    if let Some(h) = household.as_deref_mut() {
        h.funds -= bp.price as i64;
    }
    crate::buyhistory::record_construction(&mut commands, before, b, &ops, -(bp.price as i64), removed.0.clone(), Vec::new());
    match log.as_mut() {
        Some(l) => l.0.extend(ops),
        None => commands.insert_resource(crate::building::LotPaint(ops)),
    }
    // Its doors and windows set in its walls, its furniture placed (and what goes in its slots).
    let floor_y = b.levels.get(level as usize).copied().unwrap_or(0.0);
    let key_of = |k: s3bake::Key| data.0.catalog.iter().find(|c| c.objd == k).or_else(|| data.0.catalog.iter().find(|c| c.objd.2 == k.2 && c.objd.0 == k.0)).map(|c| c.objd);
    let mut placed = 0;
    for o in &bp.objects {
        let Some(key) = key_of(o.objd) else { continue };
        let fwd = Vec2::new(o.facing.sin(), o.facing.cos());
        let rot = b.rot * Quat::from_rotation_y(turns as f32 * std::f32::consts::FRAC_PI_2 + o.facing);
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        if o.in_wall {
            // (Its wall half a tile behind where it stands, a ray down just outside it.)
            let p = origin + turn(Vec2::new(o.at[0], o.at[2]) - fwd * 0.2, turns);
            let ray = Ray3d::new(b.world(p.x, p.y, floor_y + 40.0), Dir3::NEG_Y);
            let width = crate::objects::parts_bounds(&assets.object(&mut ctx, key)).map_or(1, |(mn, mx)| ((mx.x - mn.x).round() as u32).max(1));
            let Some((pos, wrot, cut)) = crate::build::snap_to_wall(b, ray, width) else { continue };
            if !cut.is_empty() {
                crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &cut, &mut faces);
                if let Some(l) = log.as_mut() {
                    l.0.extend(cut);
                }
            }
            if let Some(s) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, key, pos, wrot) {
                commands.entity(s.entity).insert((crate::save::Bought, crate::nav::Floor(level), crate::building::BuildingPiece { level }));
                placed += 1;
            }
            continue;
        }
        let p = origin + turn(Vec2::new(o.at[0], o.at[2]), turns);
        let pos = b.world(p.x, p.y, floor_y + o.at[1]);
        let design = crate::objects::first_design(&data.0, key);
        let Some(s) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, key, pos, rot, design) else { continue };
        commands.entity(s.entity).insert((crate::save::Bought, crate::nav::Floor(level), crate::building::BuildingPiece { level }));
        placed += 1;
        // (What goes on it, in its slots.)
        let tf = Transform::from_translation(pos).with_rotation(rot);
        let slots = crate::surfaces::slots(&gu.data, key.2).map(|s| s.to_vec()).unwrap_or_default();
        for (n, (child, slot, facing)) in o.slotted.iter().enumerate() {
            let (Some(ck), Some(s)) = (key_of(*child), slots.get(*slot as usize).or(slots.get(n))) else { continue };
            let (cpos, crot) = crate::surfaces::slot_world(&tf, s);
            let crot = crot * Quat::from_rotation_y(*facing - s.yaw);
            let cd = crate::objects::first_design(&data.0, ck);
            if let Some(c) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, ck, cpos, crot, cd) {
                commands.entity(c.entity).insert((crate::save::Bought, crate::nav::Floor(level), crate::building::BuildingPiece { level }));
                placed += 1;
            }
        }
    }
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
    info!("blueprint: {} built at {origin} (a quarter turn {turns} times), {placed} objects", bp.name);
    notes.push(format!("{} built for §{}.", bp.name, bp.price));
    play.write(crate::sound::PlaySound::ui("ui_build_wall_mup"));
    buy.blueprint = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use s3bake::gamedata::{BlueprintFloor, BlueprintInfo, BlueprintWall};

    fn room() -> BlueprintInfo {
        // (A 2x2 room about its middle: walls round it, four tiles.)
        let corners = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let walls = (0..4).map(|k| BlueprintWall { a: corners[k], b: corners[(k + 1) % 4], cover: [None, None] }).collect();
        let floors = [(-1, -1), (0, -1), (-1, 0), (0, 0)].into_iter().map(|(x, z)| BlueprintFloor { x, z, quarters: 15, cover: None }).collect();
        BlueprintInfo { name: "Test".into(), room: 1, price: 100, walls, floors, ..Default::default() }
    }

    #[test]
    fn turning_is_a_quarter_at_a_time() {
        let v = Vec2::new(1.0, 2.0);
        assert_eq!(turn(v, 0), v);
        assert_eq!(turn(v, 4), v);
        assert_eq!(turn(turn(v, 1), 3), v);
        // (As objects turn: a quarter turn about Y takes +Z to +X.)
        let q = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Vec3::new(1.0, 0.0, 2.0);
        assert!((Vec2::new(q.x, q.z) - turn(v, 1)).length() < 1e-5);
    }

    #[test]
    fn a_room_fits_on_the_lot_and_builds_its_walls_and_floor() {
        let b = s3bake::LotBuildingBaked { lot: 0, width: 10, depth: 10, levels: vec![-3.0, 0.0, 3.0], walls: Vec::new(), floors: Vec::new(),
            foundation: Vec::new(), foundation_top: Vec::new(), objects: Vec::new(), covers: Vec::new(), ground: Vec::new(), pool: Vec::new(),
            pool_depth: 0.0, fences: Vec::new(), stairs: Vec::new(), basement: None, fountains: Vec::new() };
        let bp = room();
        assert!(fits(&b, &bp, Vec2::new(5.0, 5.0), 0, 1));
        assert!(!fits(&b, &bp, Vec2::new(0.0, 5.0), 0, 1), "off the lot's edge");
        let ops = plan(&b, &bp, Vec2::new(5.0, 5.0), 1, 1);
        assert_eq!(ops.iter().filter(|o| matches!(o, PaintOp::AddWall { .. })).count(), 4);
        let mut sim = b.clone();
        for op in &ops {
            crate::building::apply_paint(&mut sim, op);
        }
        assert_eq!(sim.floors.iter().filter(|f| f.level == 1).count(), 4, "the room's floor laid");
        assert!(!fits(&sim, &bp, Vec2::new(5.0, 5.0), 0, 1), "not over the room now there");
    }
}
