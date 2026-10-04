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
use crate::objects::{AssetCtx, ObjectAssets, parts_bounds};
use crate::world::LotImposter;
use crate::{AppState, PlayMode};

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (follow_selected_floor, view_level_keys, building_visibility).chain().run_if(in_state(PlayMode::Live)));
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
    far: Option<bool>,
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

/// A door or window: hidden along with the wall it sits in when that wall is cut away.
#[derive(Component)]
pub struct WallObject {
    mid: Vec3,
}

/// One face of a wall segment with its full-height and cut-away meshes.
#[derive(Component)]
pub struct WallFace {
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

/// Spawns the detailed house of `lot` with all its furniture and returns its state.
#[allow(clippy::too_many_arguments)]
pub fn spawn_building(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    b: &LotBuildingBaked,
    lot: &LotInfo,
) -> ActiveBuilding {
    let corner = Vec3::from(lot.corner);
    let rot = Quat::from_rotation_y(lot.rotation);
    let top_level = b.levels.len().saturating_sub(2).max(1) as u8;
    let floor_cells = b.floors.iter().map(|f| ((f.level, f.x as i32, f.z as i32), f.mask)).collect();
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
        far: None,
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
    let cap_mat = ctx.materials.add(StandardMaterial { base_color: Color::srgb(0.93, 0.91, 0.86), perceptual_roughness: 0.9, ..default() });

    // Furniture, doors and windows.
    let mut holes: Vec<Hole> = Vec::new();
    let mut stairs: Vec<(LotObjectBaked, Quat)> = Vec::new();
    for o in &b.objects {
        let q = Quat::from_xyzw(o.rotation[0], o.rotation[1], o.rotation[2], o.rotation[3]);
        let q = if q.length_squared() < 1e-6 { Quat::IDENTITY } else { q.normalize() };
        if o.script.contains("Stairs") {
            stairs.push((o.clone(), q));
            continue;
        }
        let Some(spawned) = crate::home::spawn_game_object_rot(commands, assets, ctx, catalog, o.objd, Vec3::from(o.position), q) else {
            continue;
        };
        commands.entity(spawned.entity).insert((BuildingPiece { level: o.level }, Floor(o.level.max(1))));
        let opening = is_opening(&o.script);
        if opening.is_some() || o.script.contains("Stairs") || o.script.contains("Column") {
            commands.entity(spawned.entity).remove::<Obstacle>();
        }
        if let (Some(door), Some((mn, mx))) = (opening, parts_bounds(&assets.object(ctx, o.objd))) {
            let to_local = |v: Vec3| {
                let l = rot.inverse() * v;
                Vec2::new(l.x, l.z).normalize_or_zero()
            };
            let fwd = to_local(q * Vec3::Z);
            let wp = Vec2::from(o.local) - fwd * 0.5;
            commands.entity(spawned.entity).insert(WallObject { mid: active.world(wp.x, wp.y, o.position[1]) });
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
        commands.spawn((
            Mesh3d(ctx.meshes.add(buf.mesh())),
            MeshMaterial3d(stair_mat.clone()),
            BuildingPiece { level: if storey { lower } else { 0 } },
            DespawnOnExit(AppState::InGame),
        ));
        if storey {
            let bw = |q: Vec2| active.world(q.x, q.y, 0.0).xz();
            active.stairs.push(StairLink { level: lower, bottom: bw(bottom - d * 0.45), top: bw(top + d * 0.45), y0, y1 });
        }
    }

    // Walls: one entity per segment with a face per side, full and cut-away versions.
    for w in &b.walls {
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
        let parent = commands
            .spawn((Transform::IDENTITY, Visibility::default(), BuildingPiece { level }, DespawnOnExit(AppState::InGame)))
            .id();
        for (side, kind) in [(1.0f32, w.left), (-1.0, w.right)] {
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
            let mat = material(assets, ctx, wall_style(kind, exterior));
            let full = ctx.meshes.add(full.mesh());
            let cut = (!cut.is_empty()).then(|| ctx.meshes.add(cut.mesh()));
            let face = commands
                .spawn((Mesh3d(full.clone()), MeshMaterial3d(mat), WallFace { full, cut, mid, level, is_cut: false }))
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
                .spawn((Mesh3d(full.clone()), MeshMaterial3d(cap_mat.clone()), WallFace { full, cut, mid, level, is_cut: false }))
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

    // Floors, one mesh per level and style.
    let mut floor_bufs: HashMap<(u8, Key), MeshBuf> = HashMap::new();
    for f in &b.floors {
        let y = level_y(f.level) + 0.012;
        let (x, z) = (f.x as f32, f.z as f32);
        let c = Vec2::new(x + 0.5, z + 0.5);
        let corners = [Vec2::new(x, z), Vec2::new(x + 1.0, z), Vec2::new(x + 1.0, z + 1.0), Vec2::new(x, z + 1.0)];
        let buf = floor_bufs.entry((f.level, floor_style(f.kind))).or_default();
        for t in 0..4 {
            if f.mask & (1 << t) == 0 {
                continue;
            }
            let (p1, p2) = (corners[t], corners[(t + 1) % 4]);
            let pts = [c, p1, p2];
            buf.tri(pts.map(|p| active.world(p.x, p.y, y)), pts.map(|p| [p.x, p.y]), Vec3::Y);
        }
    }
    for ((level, style), buf) in floor_bufs {
        let mat = material(assets, ctx, style);
        commands.spawn((
            Mesh3d(ctx.meshes.add(buf.mesh())),
            MeshMaterial3d(mat),
            BuildingPiece { level },
            DespawnOnExit(AppState::InGame),
        ));
    }

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
            commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat), BuildingPiece { level: 0 }, DespawnOnExit(AppState::InGame)));
        }
    }
    // Cutaway and imposter swaps work around the middle of the walls, not of the lot.
    let mids: Vec<Vec2> = b.walls.iter().map(|w| (Vec2::from(w.a) + Vec2::from(w.b)) * 0.5).collect();
    if !mids.is_empty() {
        let c = mids.iter().copied().sum::<Vec2>() / mids.len() as f32;
        active.center = active.world(c.x, c.y, corner.y);
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
        (With<crate::sim::Sim>, Without<crate::interact::AtWork>, Without<BuildingPiece>, Without<LotImposter>, Without<crate::world::Tree>, Without<WallFace>),
    >,
) {
    let Some(mut b) = building else { return };
    let Ok((cam, cam_tf)) = cams.single() else { return };
    let far = cam.distance > IMPOSTER_DISTANCE || cam.focus.distance(b.center) > IMPOSTER_DISTANCE * 1.4;
    if b.far != Some(far) {
        b.far = Some(far);
        for (imp, mut vis) in &mut imposters {
            if imp.0 == b.lot {
                *vis = if far { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
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
    let is_cut = |mid: Vec3, level: u8| level == b.view_level && (mid - b.center).dot(view) < 0.3;
    for (floor, mut vis) in &mut sims {
        vis.set_if_neq(if floor.0 <= b.view_level { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (piece, wall_obj, mut vis) in &mut pieces {
        let show = !far && piece.level <= b.view_level && !wall_obj.is_some_and(|w| is_cut(w.mid, piece.level));
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
