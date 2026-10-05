//! The household's house, built from the lot's baked walls, floors and furniture: textured walls
//! with door and window openings, floors styled by room, the foundation, a cutaway view and
//! per-floor visibility. From afar the game's own pre-rendered lot imposter stands in.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use s3bake::*;
use s3formats::world::LotInfo;

use crate::camera::SimsCamera;
use crate::loading::{Catalog, WorldInfo};
use crate::nav::{Floor, Obstacle, StairLink};
use crate::objects::{AssetCtx, ImposterLayer, ObjectAssets, parts_bounds, spawn_parts};
use crate::world::LotImposter;
use crate::{AppState, PlayMode};

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyLots>().add_systems(
            Update,
            (follow_selected_floor, view_level_keys, building_visibility, stream_nearby_lots, lamps_at_night).chain().run_if(in_state(PlayMode::Live)),
        );
    }
}

/// Wall height of one storey, thickness, and the height of cut-away walls.
const WALL_H: f32 = s3bake::building::LEVEL_HEIGHT;
const WALL_T: f32 = 0.14;
const CUT_H: f32 = 0.45;
/// Beyond this camera distance the pre-rendered imposter replaces the detailed house.
const IMPOSTER_DISTANCE: f32 = 75.0;

/// The detailed house of the active lot.
#[derive(Resource)]
pub struct ActiveBuilding {
    pub lot: usize,
    pub corner: Vec3,
    pub rot: Quat,
    pub levels: Vec<f32>,
    pub top_level: u8,
    /// Floors above this one are hidden; walls on it are cut away.
    pub view_level: u8,
    pub center: Vec3,
    /// Half the size of the area the house covers (for the upper-floor walk grids).
    pub extent: f32,
    pub stairs: Vec<StairLink>,
    floor_cells: HashMap<(u8, i32, i32), u8>,
    /// Room kind (`ROOM_*`) of each floor tile.
    floor_kinds: HashMap<(u8, i32, i32), u8>,
    far: Option<bool>,
    /// The house as built (with any repainting), for redrawing parts of it.
    pub data: LotBuildingBaked,
}

impl ActiveBuilding {
    pub fn local(&self, p: Vec3) -> Vec2 {
        let l = self.rot.inverse() * (p - self.corner);
        Vec2::new(l.x, l.z)
    }

    pub fn world(&self, x: f32, z: f32, y: f32) -> Vec3 {
        let p = self.corner + self.rot * Vec3::new(x, 0.0, z);
        Vec3::new(p.x, y, p.z)
    }

    fn dir(&self, x: f32, z: f32) -> Vec3 {
        self.rot * Vec3::new(x, 0.0, z)
    }

    /// Whether there's floor at lot-local `p` on `level`.
    fn floor_cells_has(&self, level: u8, p: Vec2) -> bool {
        let w = self.world(p.x, p.y, 0.0);
        self.floor_y(level, w).is_some()
    }

    /// Floor height at `p` on `level`, if the house has floor there.
    pub fn floor_y(&self, level: u8, p: Vec3) -> Option<f32> {
        let l = self.local(p);
        let (x, z) = (l.x.floor(), l.y.floor());
        let mask = *self.floor_cells.get(&(level, x as i32, z as i32))?;
        let (fx, fz) = (l.x - x - 0.5, l.y - z - 0.5);
        let t = if fz.abs() > fx.abs() {
            if fz < 0.0 { 0 } else { 2 }
        } else if fx > 0.0 {
            1
        } else {
            3
        };
        (mask & (1 << t) != 0).then(|| self.levels[level as usize])
    }

    /// Room kind (`ROOM_*`) of the floor at `p` on `level`, if the house has floor there.
    pub fn room_at(&self, level: u8, p: Vec3) -> Option<u8> {
        self.floor_y(level, p)?;
        let l = self.local(p);
        self.floor_kinds.get(&(level, l.x.floor() as i32, l.y.floor() as i32)).copied()
    }
}

/// Height a Sim stands at: the house's ground floor when inside it, else the terrain.
pub fn walk_height(world: &WorldInfo, building: Option<&ActiveBuilding>, p: Vec3) -> f32 {
    building.and_then(|b| b.floor_y(1, p)).unwrap_or_else(|| world.heightmap.sample(p.x, p.z))
}

/// Part of the detailed house, shown when its level is in view.
#[derive(Component)]
pub struct BuildingPiece {
    pub level: u8,
}

/// A lamp of the active house; its light comes on after dark.
#[derive(Component)]
pub struct LotLamp;

fn lamps_at_night(night: Res<crate::clock::Night>, mut lamps: Query<(&mut PointLight, &mut Visibility), With<LotLamp>>) {
    if !night.is_changed() {
        return;
    }
    for (mut light, mut vis) in &mut lamps {
        light.intensity = 90_000.0 * night.0;
        vis.set_if_neq(if night.0 > 0.02 { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// A door or window: hidden along with the wall it sits in when that wall is cut away.
#[derive(Component)]
pub struct WallObject {
    mid: Vec3,
}

/// One face of a wall segment with its full-height and cut-away meshes.
#[derive(Component)]
pub struct WallFace {
    /// Which wall (index into the building's walls) and side (0 = left, 1 = right; caps 255).
    pub wall: u32,
    pub side: u8,
    full: Handle<Mesh>,
    /// None when nothing of the face remains below the cut height (door openings).
    cut: Option<Handle<Mesh>>,
    mid: Vec3,
    level: u8,
    is_cut: bool,
}

#[derive(Default)]
struct MeshBuf {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl MeshBuf {
    /// A quad facing `n` (winding fixed up to match).
    fn quad(&mut self, p: [Vec3; 4], uv: [[f32; 2]; 4], n: Vec3) {
        let base = self.pos.len() as u32;
        for k in 0..4 {
            self.pos.push(p[k].into());
            self.nrm.push(n.into());
            self.uv.push(uv[k]);
        }
        let flip = (p[1] - p[0]).cross(p[2] - p[0]).dot(n) < 0.0;
        if flip {
            self.idx.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        } else {
            self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    fn tri(&mut self, p: [Vec3; 3], uv: [[f32; 2]; 3], n: Vec3) {
        let base = self.pos.len() as u32;
        for k in 0..3 {
            self.pos.push(p[k].into());
            self.nrm.push(n.into());
            self.uv.push(uv[k]);
        }
        if (p[1] - p[0]).cross(p[2] - p[0]).dot(n) < 0.0 {
            self.idx.extend_from_slice(&[base, base + 2, base + 1]);
        } else {
            self.idx.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }

    fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    fn mesh(self) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        m.insert_indices(Indices::U32(self.idx));
        m
    }
}

fn floor_style(kind: u8) -> Key {
    match kind {
        ROOM_BATH => STYLE_FLOOR_BATH,
        ROOM_KITCHEN => STYLE_FLOOR_TERRACOTTA,
        ROOM_BED => STYLE_FLOOR_CARPET,
        ROOM_PORCH => STYLE_FLOOR_DECK,
        _ => STYLE_FLOOR_WOOD,
    }
}

fn wall_style(kind: u8, exterior: Key) -> Key {
    match kind {
        ROOM_OUTSIDE | ROOM_PORCH => exterior,
        ROOM_BED | ROOM_BATH => STYLE_WALL_MOULDING,
        _ => STYLE_WALL_DADO,
    }
}

/// Vertical spans of a wall left over around the openings `holes` (each `(y0, y1)`).
fn wall_spans(holes: &[(f32, f32)], top: f32) -> Vec<(f32, f32)> {
    let mut spans = vec![(0.0, top)];
    for &(h0, h1) in holes {
        let mut next = Vec::new();
        for (s0, s1) in spans {
            if h1 <= s0 || h0 >= s1 {
                next.push((s0, s1));
                continue;
            }
            if h0 > s0 + 0.02 {
                next.push((s0, h0));
            }
            if h1 < s1 - 0.02 {
                next.push((h1, s1));
            }
        }
        spans = next;
    }
    spans
}

/// A door or window cut into the wall behind it (lot-local coordinates, heights above the floor).
struct Hole {
    level: u8,
    wall_point: Vec2,
    fwd: Vec2,
    right: Vec2,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    door: bool,
}

fn is_opening(script: &str) -> Option<bool> {
    let s = script.to_ascii_lowercase();
    if s.contains(".door.") || s.contains("archway") {
        Some(true)
    } else if s.contains(".window.") {
        Some(false)
    } else {
        None
    }
}

/// Puts a spawned piece either into the active house (per-floor visibility) or under a
/// neighbouring lot's root entity.
fn place(commands: &mut Commands, e: Entity, root: Option<Entity>, level: u8) {
    match root {
        Some(r) => {
            commands.entity(e).insert(ChildOf(r));
        }
        None => {
            commands.entity(e).insert((BuildingPiece { level }, DespawnOnExit(AppState::InGame)));
        }
    }
}

/// A floor mesh of the active house (redrawn when the floors are repainted).
#[derive(Component)]
pub struct FloorMesh;

/// A covering's material: its texture on a matte surface.
pub fn surface_material(assets: &mut ObjectAssets, ctx: &mut AssetCtx, key: Key) -> Handle<StandardMaterial> {
    let tex = assets.texture(ctx, key);
    ctx.materials.add(StandardMaterial {
        base_color: if tex.is_some() { Color::WHITE } else { Color::srgb(0.8, 0.78, 0.72) },
        base_color_texture: tex,
        perceptual_roughness: 0.85,
        reflectance: 0.2,
        ..default()
    })
}

/// The house's floors: one mesh per level and covering.
pub fn spawn_floors(commands: &mut Commands, assets: &mut ObjectAssets, ctx: &mut AssetCtx, b: &LotBuildingBaked, active: &ActiveBuilding, neighbor: Option<Entity>) {
    let level_y = |l: u8| b.levels.get(l as usize).copied().unwrap_or(b.levels[b.levels.len() - 1]);
    let cover_key = |i: u16| (i != s3bake::types::NO_COVER).then(|| b.covers.get(i as usize).copied()).flatten();
    let mut floor_bufs: HashMap<(u8, Key), MeshBuf> = HashMap::new();
    for f in &b.floors {
        // (Ground-level paving sits just above the terrain.)
        let y = level_y(f.level) + if f.level == 0 { 0.03 } else { 0.012 };
        let (x, z) = (f.x as f32, f.z as f32);
        let c = Vec2::new(x + 0.5, z + 0.5);
        let corners = [Vec2::new(x, z), Vec2::new(x + 1.0, z), Vec2::new(x + 1.0, z + 1.0), Vec2::new(x, z + 1.0)];
        for t in 0..4 {
            if f.mask & (1 << t) == 0 {
                continue;
            }
            let buf = floor_bufs.entry((f.level, cover_key(f.cover[t]).unwrap_or_else(|| floor_style(f.kind)))).or_default();
            let (p1, p2) = (corners[t], corners[(t + 1) % 4]);
            let pts = [c, p1, p2];
            buf.tri(pts.map(|p| active.world(p.x, p.y, y)), pts.map(|p| [p.x, p.y]), Vec3::Y);
        }
    }
    for ((level, style), buf) in floor_bufs {
        let mat = surface_material(assets, ctx, style);
        let e = commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat))).id();
        if neighbor.is_none() {
            commands.entity(e).insert(FloorMesh);
        }
        place(commands, e, neighbor, level);
    }
}

/// One repainting of the active house: a wall side, or a floor tile, given a covering texture.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PaintOp {
    Wall { wall: u32, side: u8, texture: Key },
    Floor { level: u8, x: u16, z: u16, texture: Key },
}

/// The active house's repaintings since it was built (kept in saves).
#[derive(Resource, Default, Clone)]
pub struct LotPaint(pub Vec<PaintOp>);

/// Repaints the active house: applies the operations to its data and redraws the walls painted
/// and, if any floor changed, the floors.
pub fn repaint(
    commands: &mut Commands,
    b: &mut ActiveBuilding,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    ops: &[PaintOp],
    faces: &mut Query<(&WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
    floors: &Query<Entity, With<FloorMesh>>,
) {
    let mut mats: HashMap<Key, Handle<StandardMaterial>> = HashMap::new();
    let mut floors_changed = false;
    for op in ops {
        apply_paint(&mut b.data, op);
        match *op {
            PaintOp::Wall { wall, side, texture } => {
                let m = mats.entry(texture).or_insert_with(|| surface_material(assets, ctx, texture)).clone();
                for (f, mut mat) in faces.iter_mut() {
                    if f.wall == wall && f.side == side {
                        mat.0 = m.clone();
                    }
                }
            }
            PaintOp::Floor { .. } => floors_changed = true,
        }
    }
    if floors_changed {
        for e in floors {
            commands.entity(e).despawn();
        }
        let data = b.data.clone();
        spawn_floors(commands, assets, ctx, &data, b, None);
    }
}

/// Applies a repainting to a building's data.
pub fn apply_paint(b: &mut LotBuildingBaked, op: &PaintOp) {
    let index = |b: &mut LotBuildingBaked, k: Key| -> u16 {
        match b.covers.iter().position(|c| *c == k) {
            Some(i) => i as u16,
            None => {
                b.covers.push(k);
                (b.covers.len() - 1) as u16
            }
        }
    };
    match *op {
        PaintOp::Wall { wall, side, texture } => {
            let i = index(b, texture);
            if let Some(w) = b.walls.get_mut(wall as usize) {
                w.cover[(side as usize).min(1)] = i;
            }
        }
        PaintOp::Floor { level, x, z, texture } => {
            let i = index(b, texture);
            if let Some(f) = b.floors.iter_mut().find(|f| f.level == level && f.x == x && f.z == z) {
                f.cover = [i; 4];
            }
        }
    }
}

/// Spawns the detailed house of `lot` with all its furniture and returns its state. With a
/// `neighbor` root, it's built for show only (merged meshes, furniture without gameplay).
#[allow(clippy::too_many_arguments)]
pub fn spawn_building(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    b: &LotBuildingBaked,
    lot: &LotInfo,
    neighbor: Option<Entity>,
) -> ActiveBuilding {
    let corner = Vec3::from(lot.corner);
    let rot = Quat::from_rotation_y(lot.rotation);
    let top_level = b.levels.len().saturating_sub(2).max(1) as u8;
    let floor_cells = b.floors.iter().map(|f| ((f.level, f.x as i32, f.z as i32), f.mask)).collect();
    let floor_kinds = b.floors.iter().map(|f| ((f.level, f.x as i32, f.z as i32), f.kind)).collect();
    let mut active = ActiveBuilding {
        lot: b.lot as usize,
        corner,
        rot,
        levels: b.levels.clone(),
        top_level,
        view_level: 1,
        center: corner + rot * Vec3::new(b.width as f32 * 0.5, 0.0, b.depth as f32 * 0.5),
        extent: b.width.max(b.depth) as f32 * 0.5,
        stairs: Vec::new(),
        floor_cells,
        floor_kinds,
        far: None,
        data: b.clone(),
    };
    let level_y = |l: u8| b.levels.get(l as usize).copied().unwrap_or(b.levels[b.levels.len() - 1]);
    let exterior = STYLE_EXTERIOR[(lot.id % STYLE_EXTERIOR.len() as u64) as usize];
    let mut materials: HashMap<Key, Handle<StandardMaterial>> = HashMap::new();
    let mut material = |assets: &mut ObjectAssets, ctx: &mut AssetCtx, key: Key| -> Handle<StandardMaterial> {
        materials
            .entry(key)
            .or_insert_with(|| {
                let tex = assets.texture(ctx, key);
                ctx.materials.add(StandardMaterial {
                    base_color: if tex.is_some() { Color::WHITE } else { Color::srgb(0.8, 0.78, 0.72) },
                    base_color_texture: tex,
                    perceptual_roughness: 0.85,
                    reflectance: 0.2,
                    double_sided: key == STYLE_FOUNDATION,
                    cull_mode: if key == STYLE_FOUNDATION { None } else { Some(bevy::render::render_resource::Face::Back) },
                    ..default()
                })
            })
            .clone()
    };
    // A wall side's or floor triangle's covering texture, when the lot has one there.
    let cover_key = |i: u16| (i != s3bake::types::NO_COVER).then(|| b.covers.get(i as usize).copied()).flatten();
    let cap_mat = ctx.materials.add(StandardMaterial { base_color: Color::srgb(0.93, 0.91, 0.86), perceptual_roughness: 0.9, ..default() });

    // Furniture, doors and windows.
    let mut holes: Vec<Hole> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut stairs: Vec<(LotObjectBaked, Quat)> = Vec::new();
    for o in &b.objects {
        let q = Quat::from_xyzw(o.rotation[0], o.rotation[1], o.rotation[2], o.rotation[3]);
        let q = if q.length_squared() < 1e-6 { Quat::IDENTITY } else { q.normalize() };
        if o.script.contains("Stairs") {
            stairs.push((o.clone(), q));
            continue;
        }
        let opening = is_opening(&o.script);
        let entity = if let Some(root) = neighbor {
            let parts = assets.object(ctx, o.objd);
            if parts.is_empty() {
                continue;
            }
            let e = spawn_parts(commands, &parts, Transform::from_translation(Vec3::from(o.position)).with_rotation(q));
            commands.entity(e).insert(ChildOf(root));
            e
        } else {
            let Some(spawned) = crate::home::spawn_game_object_rot(commands, assets, ctx, catalog, o.objd, Vec3::from(o.position), q) else {
                missing.push(o.script.rsplit('.').next().unwrap_or("").to_string());
                continue;
            };
            commands.entity(spawned.entity).insert((BuildingPiece { level: o.level }, Floor(o.level.max(1))));
            let lower = o.script.to_ascii_lowercase();
            if lower.contains(".lighting.") || lower.contains("lightfloorlamp") || lower.contains("lightwalllamp") || lower.contains("lighttablelamp") {
                // Ceiling lights shine from just under the ceiling; lamps from their shade.
                let h = parts_bounds(&assets.object(ctx, o.objd)).map_or(1.5, |(mn, mx)| if lower.contains("ceiling") { mn.y.max(-1.2) } else { mx.y * 0.8 });
                let lamp = commands
                    .spawn((
                        LotLamp,
                        PointLight { intensity: 0.0, range: 9.0, radius: 0.1, color: Color::srgb(1.0, 0.85, 0.62), shadow_maps_enabled: false, ..default() },
                        Transform::from_xyz(0.0, h, 0.0),
                        Visibility::Hidden,
                    ))
                    .id();
                commands.entity(spawned.entity).add_child(lamp);
            }
            if opening.is_some() || o.script.contains("Stairs") || o.script.contains("Column") {
                commands.entity(spawned.entity).remove::<Obstacle>();
            }
            spawned.entity
        };
        if let (Some(door), Some((mn, mx))) = (opening, parts_bounds(&assets.object(ctx, o.objd))) {
            let to_local = |v: Vec3| {
                let l = rot.inverse() * v;
                Vec2::new(l.x, l.z).normalize_or_zero()
            };
            let fwd = to_local(q * Vec3::Z);
            let wp = Vec2::from(o.local) - fwd * 0.5;
            if neighbor.is_none() {
                commands.entity(entity).insert(WallObject { mid: active.world(wp.x, wp.y, o.position[1]) });
            }
            holes.push(Hole {
                level: o.level,
                wall_point: Vec2::from(o.local) - fwd * 0.5,
                fwd,
                right: to_local(q * Vec3::X),
                x0: mn.x,
                x1: mx.x,
                y0: if door { 0.0 } else { mn.y.max(0.0) },
                y1: mx.y.min(WALL_H - 0.05),
                door,
            });
        }
    }

    // Stairs: the game generates their steps, so build them here and link the floors.
    let stair_mat = material(assets, ctx, STYLE_FLOOR_DECK);
    for (o, q) in &stairs {
        let fl = {
            let v = rot.inverse() * (*q * Vec3::Z);
            Vec2::new(v.x, v.z).normalize_or_zero()
        };
        let rl = Vec2::new(-fl.y, fl.x);
        let p = Vec2::from(o.local);
        let has = |level: u8, at: Vec2| active.floor_cells_has(level, at);
        let storey = o.level >= 1 && (o.level as usize + 1) < b.levels.len();
        let (run, lower, upper_level) = if storey { (4.0f32, o.level, o.level + 1) } else { (1.0f32, 0u8, 1u8) };
        let dirs = [fl, -fl, rl, -rl];
        let pick = dirs.iter().copied().find(|&d| {
            let bottom = p - d * 0.5;
            let top = bottom + d * run;
            if storey {
                has(upper_level, top + d * 0.5) && has(lower, bottom - d * 0.5)
            } else {
                has(1, top + d * 0.4) && !has(1, bottom - d * 0.4)
            }
        });
        let Some(d) = pick else { continue };
        let (y0, y1) = (level_y(lower), level_y(upper_level));
        let bottom = p - d * 0.5;
        let top = bottom + d * run;
        let side = Vec2::new(-d.y, d.x) * 0.5;
        let steps = ((y1 - y0) / 0.25).round().max(1.0) as usize;
        let mut buf = MeshBuf::default();
        for i in 0..steps {
            let (t0, t1) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
            let a = bottom + d * (run * t0);
            let c = bottom + d * (run * t1);
            let h = y0 + (y1 - y0) * t1;
            let w = |q: Vec2, y: f32| active.world(q.x, q.y, y);
            let up3 = Vec3::Y;
            // Tread.
            buf.quad([w(a - side, h), w(a + side, h), w(c + side, h), w(c - side, h)], [[0.0, t0 * run], [1.0, t0 * run], [1.0, t1 * run], [0.0, t1 * run]], up3);
            // Riser.
            let back = active.dir(-d.x, -d.y);
            let hb = y0 + (y1 - y0) * t0;
            buf.quad([w(a - side, hb), w(a + side, hb), w(a + side, h), w(a - side, h)], [[0.0, 0.0], [1.0, 0.0], [1.0, 0.25], [0.0, 0.25]], back);
            // Sides down to the floor.
            for sgn in [-1.0f32, 1.0] {
                let n = active.dir(side.x * sgn * 2.0, side.y * sgn * 2.0);
                let e = side * sgn;
                buf.quad([w(a + e, y0), w(c + e, y0), w(c + e, h), w(a + e, h)], [[t0 * run, 1.0], [t1 * run, 1.0], [t1 * run, 0.0], [t0 * run, 0.0]], n);
            }
        }
        let e = commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(stair_mat.clone()))).id();
        place(commands, e, neighbor, if storey { lower } else { 0 });
        if storey && neighbor.is_none() {
            let bw = |q: Vec2| active.world(q.x, q.y, 0.0).xz();
            active.stairs.push(StairLink { level: lower, upper: upper_level, bottom: bw(bottom - d * 0.45), top: bw(top + d * 0.45), y0, y1 });
        }
    }

    // Elevators: one per floor, stacked; Sims ride between the floors they stop at.
    if neighbor.is_none() {
        let mut shafts: HashMap<(i32, i32), Vec<(u8, Vec2)>> = HashMap::new();
        for o in b.objects.iter().filter(|o| o.script.to_ascii_lowercase().contains("elevator")) {
            let q = Quat::from_xyzw(o.rotation[0], o.rotation[1], o.rotation[2], o.rotation[3]).normalize();
            let f = rot.inverse() * (q * Vec3::Z);
            let door = Vec2::from(o.local) + Vec2::new(f.x, f.z).normalize_or_zero() * 0.9;
            let key = ((o.local[0] * 2.0).round() as i32, (o.local[1] * 2.0).round() as i32);
            shafts.entry(key).or_default().push((o.level.max(1), door));
        }
        for mut stops in shafts.into_values() {
            stops.sort_by_key(|s| s.0);
            stops.dedup_by_key(|s| s.0);
            for w in stops.windows(2) {
                let ((l0, p0), (l1, p1)) = (w[0], w[1]);
                let bw = |q: Vec2| active.world(q.x, q.y, 0.0).xz();
                active.stairs.push(StairLink { level: l0, upper: l1, bottom: bw(p0), top: bw(p1), y0: level_y(l0), y1: level_y(l1) });
            }
        }
    }

    // Walls: one entity per segment with a face per side, full and cut-away versions
    // (neighbours: everything merged into a few meshes).
    let mut merged: HashMap<Key, MeshBuf> = HashMap::new();
    let mut merged_caps = MeshBuf::default();
    for (wall_index, w) in b.walls.iter().enumerate() {
        let level = w.level.max(1);
        let y0 = level_y(level);
        let (a, bb) = (Vec2::from(w.a), Vec2::from(w.b));
        let seg = bb - a;
        let len = seg.length();
        if len < 1e-3 {
            continue;
        }
        let along = seg / len;
        let n_local = Vec2::new(-along.y, along.x);
        let mid_local = (a + bb) * 0.5;
        let mut seg_holes = Vec::new();
        let mut has_door = false;
        for h in &holes {
            if h.level != w.level || along.dot(h.right).abs() < 0.9 {
                continue;
            }
            let d = mid_local - h.wall_point;
            let t = d.dot(h.right);
            if d.dot(h.fwd).abs() < 0.3 && t > h.x0 - 0.1 && t < h.x1 + 0.1 {
                seg_holes.push((h.y0, h.y1));
                has_door |= h.door;
            }
        }
        let ext = along * (WALL_T * 0.5);
        let (pa, pb) = (a - ext, bb + ext);
        let to3 = |p: Vec2, y: f32| active.world(p.x, p.y, y0 + y);
        let n3 = active.dir(n_local.x, n_local.y);
        let half = n3 * (WALL_T * 0.5);
        let ulen = len + WALL_T;
        let mid = to3(mid_local, 0.0);
        if neighbor.is_some() {
            for (side, kind, cover) in [(1.0f32, w.left, w.cover[0]), (-1.0, w.right, w.cover[1])] {
                let buf = merged.entry(cover_key(cover).unwrap_or_else(|| wall_style(kind, exterior))).or_default();
                for (s0, s1) in wall_spans(&seg_holes, WALL_H) {
                    let off = half * side;
                    let p = [to3(pa, s0) + off, to3(pb, s0) + off, to3(pb, s1) + off, to3(pa, s1) + off];
                    let (v0, v1) = (1.0 - s0 / WALL_H, 1.0 - s1 / WALL_H);
                    let (u0, u1) = if side > 0.0 { (0.0, ulen) } else { (ulen, 0.0) };
                    buf.quad(p, [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], n3 * side);
                }
            }
            for (_, s1) in wall_spans(&seg_holes, WALL_H) {
                let p = [to3(pa, s1) + half, to3(pb, s1) + half, to3(pb, s1) - half, to3(pa, s1) - half];
                merged_caps.quad(p, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], Vec3::Y);
            }
            continue;
        }
        let parent = commands
            .spawn((Transform::IDENTITY, Visibility::default(), BuildingPiece { level }, DespawnOnExit(AppState::InGame)))
            .id();
        for (side_index, (side, kind, cover)) in [(1.0f32, w.left, w.cover[0]), (-1.0, w.right, w.cover[1])].into_iter().enumerate() {
            let mut meshes = [MeshBuf::default(), MeshBuf::default()];
            for (mi, top) in [(0usize, WALL_H), (1, CUT_H)] {
                for (s0, s1) in wall_spans(&seg_holes, top) {
                    let off = half * side;
                    let p = [to3(pa, s0) + off, to3(pb, s0) + off, to3(pb, s1) + off, to3(pa, s1) + off];
                    let (v0, v1) = (1.0 - s0 / WALL_H, 1.0 - s1 / WALL_H);
                    let (u0, u1) = if side > 0.0 { (0.0, ulen) } else { (ulen, 0.0) };
                    meshes[mi].quad(p, [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], n3 * side);
                }
            }
            let [full, cut] = meshes;
            if full.is_empty() {
                continue;
            }
            let mat = material(assets, ctx, cover_key(cover).unwrap_or_else(|| wall_style(kind, exterior)));
            let full = ctx.meshes.add(full.mesh());
            let cut = (!cut.is_empty()).then(|| ctx.meshes.add(cut.mesh()));
            let face = commands
                .spawn((
                    Mesh3d(full.clone()),
                    MeshMaterial3d(mat),
                    WallFace { wall: wall_index as u32, side: side_index as u8, full, cut, mid, level, is_cut: false },
                ))
                .id();
            commands.entity(parent).add_child(face);
        }
        // Wall tops: caps over every span, for both heights.
        let mut caps = [MeshBuf::default(), MeshBuf::default()];
        for (mi, top) in [(0usize, WALL_H), (1, CUT_H)] {
            for (_, s1) in wall_spans(&seg_holes, top) {
                let p = [to3(pa, s1) + half, to3(pb, s1) + half, to3(pb, s1) - half, to3(pa, s1) - half];
                caps[mi].quad(p, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], Vec3::Y);
            }
        }
        let [full, cut] = caps;
        if !full.is_empty() {
            let full = ctx.meshes.add(full.mesh());
            let cut = (!cut.is_empty()).then(|| ctx.meshes.add(cut.mesh()));
            let cap = commands
                .spawn((
                    Mesh3d(full.clone()),
                    MeshMaterial3d(cap_mat.clone()),
                    WallFace { wall: wall_index as u32, side: 255, full, cut, mid, level, is_cut: false },
                ))
                .id();
            commands.entity(parent).add_child(cap);
        }
        // Walls block walking on their floor, except where a door is.
        if !has_door {
            let dir = active.dir(along.x, along.y);
            commands.spawn((
                Transform::from_translation(mid).with_rotation(Quat::from_rotation_y((-dir.z).atan2(dir.x))),
                Obstacle { half: Vec2::new(len * 0.5 + 0.02, 0.08), center_offset: Vec2::ZERO },
                Floor(level),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }

    for (style, buf) in merged {
        let mat = material(assets, ctx, style);
        let e = commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat))).id();
        place(commands, e, neighbor, 1);
    }
    if !merged_caps.is_empty() {
        let e = commands.spawn((Mesh3d(ctx.meshes.add(merged_caps.mesh())), MeshMaterial3d(cap_mat.clone()))).id();
        place(commands, e, neighbor, 1);
    }

    // Floors, one mesh per level and covering.
    spawn_floors(commands, assets, ctx, b, &active, neighbor);

    // Foundation sides from slightly below the ground up to the ground floor.
    if !b.foundation.is_empty() {
        let mut buf = MeshBuf::default();
        let (g, top) = (level_y(0) - 0.6, level_y(1));
        for (a, bb) in &b.foundation {
            let (a, bb) = (Vec2::from(*a), Vec2::from(*bb));
            let len = (bb - a).length();
            if len < 1e-3 {
                continue;
            }
            let n = (bb - a) / len;
            let n3 = active.dir(-n.y, n.x);
            let p = [active.world(a.x, a.y, g), active.world(bb.x, bb.y, g), active.world(bb.x, bb.y, top), active.world(a.x, a.y, top)];
            let v = (top - g) / WALL_H;
            buf.quad(p, [[0.0, v], [len, v], [len, 0.0], [0.0, 0.0]], n3);
        }
        if !buf.is_empty() {
            let mat = material(assets, ctx, STYLE_FOUNDATION);
            let e = commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat))).id();
            place(commands, e, neighbor, 0);
        }
    }
    // Cutaway and imposter swaps work around the middle of the walls, not of the lot.
    let mids: Vec<Vec2> = b.walls.iter().map(|w| (Vec2::from(w.a) + Vec2::from(w.b)) * 0.5).collect();
    if !mids.is_empty() {
        let c = mids.iter().copied().sum::<Vec2>() / mids.len() as f32;
        active.center = active.world(c.x, c.y, corner.y);
    }
    if neighbor.is_none() {
        info!("house: {} floors, {} objects ({} without a model)", top_level, b.objects.len(), missing.len());
    }
    active.view_level = 1;
    active
}

/// When the selected Sim changes floor, the view follows.
fn follow_selected_floor(
    building: Option<ResMut<ActiveBuilding>>,
    sims: Query<&Floor, (With<crate::sim::Selected>, Or<(Changed<Floor>, Added<crate::sim::Selected>)>)>,
) {
    let Some(mut b) = building else { return };
    if let Some(f) = sims.iter().next() {
        let l = f.0.clamp(1, b.top_level);
        if b.view_level != l {
            b.view_level = l;
        }
    }
}

/// PageUp / PageDown move the floor being viewed.
fn view_level_keys(keys: Res<ButtonInput<KeyCode>>, building: Option<ResMut<ActiveBuilding>>) {
    let Some(mut b) = building else { return };
    if keys.just_pressed(KeyCode::PageUp) && b.view_level < b.top_level {
        b.view_level += 1;
    }
    if keys.just_pressed(KeyCode::PageDown) && b.view_level > 1 {
        b.view_level -= 1;
    }
}

/// Shows the detailed house near the camera (with upper floors hidden and the front walls cut
/// away) and the game's imposter from afar.
#[allow(clippy::type_complexity)]
fn building_visibility(
    building: Option<ResMut<ActiveBuilding>>,
    cams: Query<(&SimsCamera, &GlobalTransform)>,
    mut pieces: Query<(&BuildingPiece, Option<&WallObject>, &mut Visibility), (Without<LotImposter>, Without<crate::world::Tree>)>,
    mut imposters: Query<(&LotImposter, &mut Visibility), (Without<BuildingPiece>, Without<crate::world::Tree>)>,
    mut faces: Query<(&mut WallFace, &mut Mesh3d, &mut Visibility), (Without<BuildingPiece>, Without<LotImposter>, Without<crate::world::Tree>)>,
    mut trees: Query<(&GlobalTransform, &mut Visibility), (With<crate::world::Tree>, Without<BuildingPiece>, Without<LotImposter>)>,
    mut sims: Query<
        (&Floor, &mut Visibility),
        (
            With<crate::sim::Sim>,
            Without<crate::interact::AtWork>,
            Without<crate::rabbitholes::AtRabbitHole>,
            Without<crate::interact::OffLot>,
            Without<crate::town::Townie>,
            Without<BuildingPiece>,
            Without<LotImposter>,
            Without<crate::world::Tree>,
            Without<WallFace>,
        ),
    >,
) {
    let Some(mut b) = building else { return };
    let Ok((cam, cam_tf)) = cams.single() else { return };
    let far = cam.distance > IMPOSTER_DISTANCE || cam.focus.distance(b.center) > IMPOSTER_DISTANCE * 1.4;
    if b.far != Some(far) {
        b.far = Some(far);
        let _ = &mut imposters;
        // World trees right around the house would hide it in close-up.
        for (tf, mut vis) in &mut trees {
            let near = tf.translation().xz().distance(b.center.xz()) < 22.0;
            if near {
                *vis = if far { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
    // Cutaway: walls of the viewed floor in the half of the house nearer the camera are cut down.
    let view = cam_tf.forward().as_vec3();
    let view = Vec3::new(view.x, 0.0, view.z).normalize_or_zero();
    // Zoomed out, the house is seen from outside: every floor, walls up, roof on.
    let exterior = cam.distance > ROOF_DISTANCE;
    let view_level = if exterior { b.top_level } else { b.view_level };
    let is_cut = |mid: Vec3, level: u8| !exterior && level == view_level && (mid - b.center).dot(view) < 0.3;
    for (floor, mut vis) in &mut sims {
        vis.set_if_neq(if floor.0 <= view_level { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (piece, wall_obj, mut vis) in &mut pieces {
        let show = !far && piece.level <= view_level && !wall_obj.is_some_and(|w| is_cut(w.mid, piece.level));
        vis.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
    }
    if far {
        return;
    }
    for (mut face, mut mesh, mut vis) in &mut faces {
        let cut = is_cut(face.mid, face.level);
        if cut != face.is_cut {
            face.is_cut = cut;
            match (&face.cut, cut) {
                (Some(h), true) => mesh.0 = h.clone(),
                (None, true) => *vis = Visibility::Hidden,
                _ => {
                    mesh.0 = face.full.clone();
                    *vis = Visibility::Inherited;
                }
            }
        }
    }
}

/// Lots near the camera shown with their real walls and furniture (the imposter keeps only its
/// painted ground and its roofs), as the game does around the camera.
#[derive(Resource, Default)]
pub struct NearbyLots {
    spawned: HashMap<usize, Entity>,
}

const NEARBY_IN: f32 = 70.0;
const NEARBY_OUT: f32 = 95.0;
/// Below this camera distance the active house's roof is taken off.
const ROOF_DISTANCE: f32 = 42.0;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn stream_nearby_lots(
    mut commands: Commands,
    mut nearby: ResMut<NearbyLots>,
    world: Res<crate::loading::CurrentWorld>,
    data: Res<crate::baked::Baked>,
    catalog: Res<Catalog>,
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    cams: Query<&SimsCamera>,
    active: Option<Res<ActiveBuilding>>,
    imposters: Query<(Entity, &LotImposter)>,
    mut layers: Query<(&ImposterLayer, &ChildOf, &mut Visibility)>,
) {
    let Ok(cam) = cams.single() else { return };
    let active_lot = active.as_ref().map(|a| a.lot);
    let zoomed_in = cam.distance < 110.0;
    let lot_center = |i: usize| crate::home::lot_center(&world.data.lots[i]);
    // Drop lots that went out of range.
    let gone: Vec<usize> = nearby
        .spawned
        .keys()
        .copied()
        .filter(|&i| !zoomed_in || Some(i) == active_lot || lot_center(i).xz().distance(cam.focus.xz()) > NEARBY_OUT)
        .collect();
    for i in gone {
        if let Some(e) = nearby.spawned.remove(&i) {
            commands.entity(e).despawn();
        }
    }
    // Bring in the nearest lot not yet shown (one per frame).
    if zoomed_in {
        let next = world
            .data
            .buildings
            .keys()
            .copied()
            .filter(|i| Some(*i) != active_lot && !nearby.spawned.contains_key(i))
            .map(|i| (i, lot_center(i).xz().distance(cam.focus.xz())))
            .filter(|(_, d)| *d < NEARBY_IN)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = next {
            let root = commands.spawn((Transform::IDENTITY, Visibility::default(), DespawnOnExit(AppState::InGame))).id();
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            spawn_building(&mut commands, &mut assets, &mut ctx, &catalog, &world.data.buildings[&i], &world.data.lots[i], Some(root));
            nearby.spawned.insert(i, root);
        }
    }
    // Imposter layers: detailed lots keep their ground and roofs; the active house loses its
    // roof when the camera comes in close.
    let lot_of: HashMap<Entity, usize> = imposters.iter().map(|(e, l)| (e, l.0)).collect();
    let active_far = active.as_ref().is_none_or(|a| a.far != Some(false));
    for (layer, parent, mut vis) in &mut layers {
        let Some(&lot) = lot_of.get(&parent.parent()) else { continue };
        let detailed = nearby.spawned.contains_key(&lot) || (Some(lot) == active_lot && !active_far);
        let show = !detailed
            || layer.0 == LAYER_GROUND
            || (layer.0 == LAYER_ROOF && (Some(lot) != active_lot || cam.distance > ROOF_DISTANCE));
        vis.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
    }
}
