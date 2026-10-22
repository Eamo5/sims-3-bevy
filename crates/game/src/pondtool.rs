//! Ponds: Build mode's Water tool (the terrain panel's, and the diagram's Ponds), pouring water
//! into ground lowered on the household's lot. A click fills the hollow under the cursor a
//! quarter metre deeper (from its lowest point, or from the water already there), as far as the
//! ground under that level reaches; Ctrl+click drains it a quarter metre (dry ground once it's
//! below the bottom). Water won't spill off the lot or under the house. The water is the lot's
//! own as the towns' ponds are (`PondBaked`: a level at each lot grid point): drawn with the
//! ponds' water, kept off by Sims walking, built as build ops (undone, redone, kept in saves).

use std::collections::{BTreeMap, HashSet, VecDeque};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::build::BuildTool;
use crate::building::{ActiveBuilding, PaintOp};
use crate::buy::BuyMode;
use crate::camera::SimsCamera;
use crate::hud::PointerOverUi;
use crate::objects::{AssetCtx, ObjectAssets};

pub struct PondToolPlugin;

impl Plugin for PondToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (pond_tool, sync).chain().run_if(in_state(crate::PlayMode::Live)));
    }
}

/// A lot's pond water (its lot's index), as the terrain drew it at loading.
#[derive(Component)]
pub struct LotPond(pub u32);

/// The ponds' water material, and the terrain's heights it sees the shore by.
#[derive(Resource)]
pub struct PondMaterial(pub Handle<crate::water::WaterMaterial>, pub Handle<Image>);

/// The Water tool's place among the terrain tools (`terrain_paint::Sculpt::Water`).
pub const WATER: u8 = 5;

/// How much a click raises or lowers the water (metres).
pub const STEP: f32 = 0.25;

/// What a click of the Water tool does at a lot grid point: the points to wet (at the level) and
/// those to dry, or why it can't.
pub fn pour(
    ground: &dyn Fn(u16, u16) -> f32,
    (w, d): (u16, u16),
    pond: &BTreeMap<(u16, u16), f32>,
    built: &dyn Fn(u16, u16) -> bool,
    at: (u16, u16),
    drain: bool,
) -> Result<(f32, Vec<(u16, u16)>, Vec<(u16, u16)>), &'static str> {
    // The pond there now (its connected water), and its level.
    let here = pond.get(&at).copied();
    let old: Vec<(u16, u16)> = match here {
        Some(l) => flood(at, (w, d), &|x, z| pond.get(&(x, z)).is_some_and(|v| (*v - l).abs() < 1e-3)),
        None => Vec::new(),
    };
    let bottom = old.iter().map(|(x, z)| ground(*x, *z)).fold(ground(at.0, at.1), f32::min);
    let level = match (here, drain) {
        (Some(l), false) => l + STEP,
        (None, false) => bottom + STEP,
        (Some(l), true) => l - STEP,
        (None, true) => return Err("There's no water there to drain."),
    };
    let wet: Vec<(u16, u16)> = if level <= bottom + 0.01 { Vec::new() } else { flood(at, (w, d), &|x, z| ground(x, z) < level) };
    if wet.iter().any(|(x, z)| *x == 0 || *z == 0 || *x >= w || *z >= d) {
        return Err("The water would spill off the lot.");
    }
    if wet.iter().any(|(x, z)| built(*x, *z)) {
        return Err("The house is in the way of the water.");
    }
    let keep: HashSet<(u16, u16)> = wet.iter().copied().collect();
    let dry = old.into_iter().filter(|p| !keep.contains(p)).collect();
    Ok((level, wet, dry))
}

/// The grid points joined to `from` (four ways) where `inside` holds, the lot's edge included.
fn flood(from: (u16, u16), (w, d): (u16, u16), inside: &dyn Fn(u16, u16) -> bool) -> Vec<(u16, u16)> {
    if !inside(from.0, from.1) {
        return Vec::new();
    }
    let mut seen = HashSet::from([from]);
    let mut todo = VecDeque::from([from]);
    let mut out = Vec::new();
    while let Some((x, z)) = todo.pop_front() {
        out.push((x, z));
        // (Big enough: a pond of more than this has run off somewhere.)
        if out.len() > 4096 {
            break;
        }
        let next = [(x.wrapping_sub(1), z), (x + 1, z), (x, z.wrapping_sub(1)), (x, z + 1)];
        for (nx, nz) in next {
            if nx > w || nz > d || !seen.insert((nx, nz)) || !inside(nx, nz) {
                continue;
            }
            todo.push_back((nx, nz));
        }
    }
    out
}

/// The Water tool in hand: the grid point under the cursor marked; a click pours (Ctrl: drains).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn pond_tool(
    mut commands: Commands,
    buy: Res<BuyMode>,
    (mouse, keys, over_ui): (Res<ButtonInput<MouseButton>>, Res<ButtonInput<KeyCode>>, Res<PointerOverUi>),
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    mut building: Option<ResMut<ActiveBuilding>>,
    world: Res<crate::loading::CurrentWorld>,
    (data, mut assets, mut meshes, mut images, mut mats): (Res<crate::baked::Baked>, ResMut<ObjectAssets>, ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut faces: Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
    (mut log, removed, mut notes, mut play): (Option<ResMut<crate::building::LotPaint>>, Res<crate::save::RemovedLotObjects>, ResMut<crate::interact::Notifications>, MessageWriter<crate::sound::PlaySound>),
    mut gizmos: Gizmos,
    (mut test, time, sculpt_at): (Local<bool>, Res<Time>, Res<crate::terrain_paint::SculptAt>),
) {
    // (POND_TEST=<x>,<z>[,<clicks>] (tests): water poured at that lot point, so many times.)
    let test_at = std::env::var("POND_TEST").ok().filter(|_| !*test && time.elapsed_secs() > 10.0 && sculpt_at.0.is_none_or(|s| s.1 == 0)).and_then(|v| {
        let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (n.len() >= 2).then(|| (n[0], n[1], n.get(2).copied().unwrap_or(1.0) as u32))
    });
    let Some(b) = building.as_deref_mut() else { return };
    let holding = buy.active && buy.tool == Some(BuildTool::Sculpt) && buy.sculpt == WATER;
    if !holding && test_at.is_none() {
        return;
    }
    let (w, d) = (b.data.width as u16, b.data.depth as u16);
    let point = match test_at {
        Some((x, z, _)) => Some((x.round() as u16, z.round() as u16)),
        None => {
            let cursor = windows.single().ok().and_then(|w| w.cursor_position());
            let (Some(cursor), Ok((camera, cam_tf))) = (cursor, cams.single()) else { return };
            let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
            let Some(p) = crate::hud::ground_hit(ray, &world) else { return };
            let l = b.local(p);
            (l.x >= 0.0 && l.y >= 0.0 && l.x <= w as f32 && l.y <= d as f32).then(|| (l.x.round() as u16, l.y.round() as u16))
        }
    };
    let Some(at) = point else { return };
    // (The ground at each grid point of the lot.)
    let heights: Vec<f32> = (0..=w)
        .flat_map(|x| (0..=d).map(move |z| (x, z)))
        .map(|(x, z)| {
            let p = b.world(x as f32, z as f32, 0.0);
            world.data.heightmap.sample(p.x, p.z)
        })
        .collect();
    let ground = |x: u16, z: u16| heights.get(x as usize * (d as usize + 1) + z as usize).copied().unwrap_or(f32::MAX);
    // The point marked: a ring at the water (or the ground) there.
    let y = b.pond.get(&at).copied().unwrap_or_else(|| ground(at.0, at.1)) + 0.05;
    let mark = b.world(at.0 as f32, at.1 as f32, y);
    gizmos.circle(Isometry3d::new(mark, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), 0.4, Color::srgb(0.45, 0.75, 1.0));
    let clicks = match test_at {
        Some((.., n)) => {
            *test = true;
            info!("pond test: lot {w}x{d}, ground there {:.2}, about {:.2}", ground(at.0, at.1), ground(at.0.saturating_sub(3), at.1));
            n
        }
        None if mouse.just_pressed(MouseButton::Left) && !over_ui.0 => 1,
        None => 0,
    };
    let drain = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    // (Under the house: by the ground floor's tiles about the point.)
    let floors: HashSet<(i32, i32)> = b.data.floors.iter().filter(|f| f.level <= 1).map(|f| (f.x as i32, f.z as i32)).chain(b.data.pool.iter().map(|p| (p.x as i32, p.z as i32))).collect();
    let built = |x: u16, z: u16| [(0, 0), (-1, 0), (0, -1), (-1, -1)].iter().any(|(dx, dz)| floors.contains(&(x as i32 + dx, z as i32 + dz)));
    // (POND_MAP=1 (tests): the lot's floors as a map, to find open ground.)
    if clicks > 0 && std::env::var_os("POND_MAP").is_some() {
        for z in (0..=d).step_by(2) {
            info!("pond map {z:2}: {}", (0..=w).map(|x| if built(x, z) { '#' } else { '.' }).collect::<String>());
        }
    }
    for _ in 0..clicks {
        let pond = b.pond.clone();
        let (level, wet, dry) = match pour(&ground, (w, d), &pond, &built, at, drain) {
            Ok(r) => r,
            Err(why) => {
                notes.push(why);
                return;
            }
        };
        let ops: Vec<PaintOp> = wet
            .iter()
            .map(|(x, z)| PaintOp::PondWater { x: *x, z: *z, level: Some(level) })
            .chain(dry.iter().map(|(x, z)| PaintOp::PondWater { x: *x, z: *z, level: None }))
            .collect();
        if ops.is_empty() {
            return;
        }
        info!("pond: {} points at {level:.2} m, {} drained", wet.len(), dry.len());
        let before = crate::building::BuildingSnapshot::capture(b, log.as_deref());
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
        crate::buyhistory::record_construction(&mut commands, before, b, &ops, 0, removed.0.clone(), Vec::new());
        match log.as_mut() {
            Some(l) => l.0.extend(ops),
            None => commands.insert_resource(crate::building::LotPaint(ops)),
        }
        play.write(crate::sound::PlaySound::ui("ui_build_pool_mup"));
    }
}

/// The household's water drawn, and the lot's pond as the world knows it (for walking, fishing),
/// as it changes: the town's own pond there, if it had one, with what's been poured over it.
#[allow(clippy::too_many_arguments)]
fn sync(
    mut commands: Commands,
    building: Option<Res<ActiveBuilding>>,
    mut world: ResMut<crate::loading::CurrentWorld>,
    mat: Option<Res<PondMaterial>>,
    (mut meshes, mut images): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>),
    ponds: Query<(Entity, &LotPond)>,
    mut grid: Option<ResMut<crate::nav::NavGrid>>,
    mut last: Local<Option<(usize, u32, usize)>>,
    sculpted: Res<crate::terrain_paint::Sculpted>,
    mut original: Local<Option<(usize, Option<s3bake::PondBaked>)>>,
) {
    let (Some(b), Some(mat)) = (building, mat) else { return };
    // (Again as the ground under it's sculpted, or put back from a save.)
    let now = (b.lot, b.pond_gen, if b.pond.is_empty() { 0 } else { sculpted.heights.len() });
    if *last == Some(now) {
        return;
    }
    let first = last.is_none_or(|l| l.0 != b.lot);
    *last = Some(now);
    let lot = b.lot as u32;
    if original.as_ref().is_none_or(|o| o.0 != b.lot) {
        *original = Some((b.lot, world.data.ponds.iter().find(|p| p.lot == lot).cloned()));
    }
    // (Nothing poured, on first sight: the town's pond stands as it was drawn.)
    if first && b.pond.is_empty() {
        return;
    }
    let Some(info) = world.data.lots.get(b.lot).cloned() else { return };
    let (nx, nz) = (b.data.width + 1, b.data.depth + 1);
    let mut water = match original.as_ref().and_then(|o| o.1.as_ref()).filter(|p| p.nx == nx && p.nz == nz) {
        Some(p) => p.water.clone(),
        None => vec![f32::NAN; (nx * nz) as usize],
    };
    for (&(x, z), &level) in &b.pond {
        if (x as u32) < nx && (z as u32) < nz {
            water[x as usize * nz as usize + z as usize] = level;
        }
    }
    let pond = s3bake::PondBaked { lot, nx, nz, water };
    // (The water sees its shore by the terrain's heights: as they are now, the hollow dug.)
    images.insert(&mat.1, crate::water::height_image(&world.data.heightmap)).ok();
    let data = std::sync::Arc::make_mut(&mut world.data);
    data.ponds.retain(|p| p.lot != lot);
    let has_water = pond.water.iter().any(|v| !v.is_nan());
    let mesh = has_water.then(|| crate::terrain::pond_mesh(&pond, &info));
    if has_water {
        data.ponds.push(pond);
    }
    for (e, p) in &ponds {
        if p.0 == lot {
            commands.entity(e).try_despawn();
        }
    }
    if let Some(m) = mesh {
        let at = b.pond.keys().next().map(|(x, z)| b.world(*x as f32, *z as f32, 0.0));
        info!("pond: the lot's water drawn ({} points poured, about {at:?}; {} heights sculpted)", b.pond.len(), sculpted.heights.len());
        let e = crate::water::spawn_water(&mut commands, meshes.add(m), mat.0.clone(), Transform::default());
        commands.entity(e).insert(LotPond(lot));
    }
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lot 8 by 8 with a hollow 2 m deep in its middle (points 3..=5 each way), and rising
    /// ground towards one edge.
    fn ground(x: u16, z: u16) -> f32 {
        if (3..=5).contains(&x) && (3..=5).contains(&z) { -2.0 } else { 0.0 }
    }

    #[test]
    fn water_fills_the_hollow_and_rises_and_drains() {
        let none = |_: u16, _: u16| false;
        let mut pond = BTreeMap::new();
        let (level, wet, dry) = pour(&ground, (8, 8), &pond, &none, (4, 4), false).unwrap();
        assert_eq!(level, -2.0 + STEP);
        assert_eq!(wet.len(), 9, "the hollow");
        assert!(dry.is_empty());
        for p in &wet {
            pond.insert(*p, level);
        }
        // (Again: a quarter metre higher, still in the hollow.)
        let (level2, wet2, _) = pour(&ground, (8, 8), &pond, &none, (4, 4), false).unwrap();
        assert_eq!(level2, level + STEP);
        assert_eq!(wet2.len(), 9);
        // (Drained back to nothing.)
        let (_, wet3, dry3) = pour(&ground, (8, 8), &pond, &none, (4, 4), true).unwrap();
        assert!(wet3.is_empty() && dry3.len() == 9);
        assert!(pour(&ground, (8, 8), &BTreeMap::new(), &none, (4, 4), true).is_err());
    }

    #[test]
    fn water_stays_on_the_lot_and_off_the_house() {
        let none = |_: u16, _: u16| false;
        // (A hollow reaching the lot's edge spills.)
        let edge = |x: u16, _z: u16| if x <= 2 { -1.0 } else { 0.0 };
        assert!(pour(&edge, (8, 8), &BTreeMap::new(), &none, (1, 4), false).is_err());
        let house = |x: u16, _z: u16| x == 4;
        assert!(pour(&ground, (8, 8), &BTreeMap::new(), &house, (4, 4), false).is_err());
        // (Flat ground: a quarter metre of water spreads to the edge, so not there either.)
        let flat = |_: u16, _: u16| 0.0;
        assert!(pour(&flat, (8, 8), &BTreeMap::new(), &none, (4, 4), false).is_err());
    }
}
