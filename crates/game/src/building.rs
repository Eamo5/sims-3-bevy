//! The household's house, built from the lot's baked walls, floors and furniture: textured walls
//! with door and window openings, floors styled by room, the foundation, a cutaway view and
//! per-floor visibility. From afar the game's own pre-rendered lot imposter stands in.

use std::collections::{BTreeSet, HashMap, HashSet};

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
        app.init_resource::<NearbyLots>().init_resource::<WallMode>().add_message::<PoolChanged>().add_systems(
            Update,
            (follow_selected_floor, view_level_keys, building_visibility, bulldozed_lots, stream_nearby_lots, lamps_at_night, cut_openings)
                .chain()
                .run_if(in_state(PlayMode::Live)),
        )
        // (The lots shown went with the game: a game loaded after it, or a household changed to,
        // shows its own.)
        .add_systems(OnExit(crate::AppState::InGame), |mut nearby: ResMut<NearbyLots>| *nearby = NearbyLots::default());
    }
}

/// How the house's walls are shown, as the game's three wall buttons have it: up, cut away on
/// the camera's side, or down.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum WallMode {
    Up,
    #[default]
    Cutaway,
    Down,
}

impl WallMode {
    pub fn next(self) -> Self {
        match self {
            WallMode::Up => WallMode::Cutaway,
            WallMode::Cutaway => WallMode::Down,
            WallMode::Down => WallMode::Up,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WallMode::Up => "Walls Up",
            WallMode::Cutaway => "Cutaway",
            WallMode::Down => "Walls Down",
        }
    }

    /// Whether a wall of the viewed floor at `mid` is cut down, for a camera looking along
    /// `view` at a house centred on `center`.
    pub fn cuts(self, mid: Vec3, center: Vec3, view: Vec3) -> bool {
        match self {
            WallMode::Up => false,
            WallMode::Cutaway => (mid - center).dot(view) < 0.3,
            WallMode::Down => true,
        }
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
    /// Floor tiles' own heights, where not their storey's (a room on a foundation beside
    /// rooms without one).
    floor_heights: HashMap<(u8, i32, i32), f32>,
    /// Room kind (`ROOM_*`) of each floor tile.
    floor_kinds: HashMap<(u8, i32, i32), u8>,
    far: Option<bool>,
    /// The house as built (with any repainting), for redrawing parts of it.
    pub data: LotBuildingBaked,
    /// The exterior wall style and the wall caps' material (for building new walls).
    pub exterior: Key,
    pub cap_mat: Handle<StandardMaterial>,
    /// Doors and windows cut into the walls (with their furniture, when it's in play).
    holes: Vec<(Option<Entity>, Hole)>,
    /// Walls the household built: their sides face the rooms they enclose or the outdoors.
    pub built: HashSet<u32>,
    /// No imposter stands in for the house from afar (an empty lot built on, or a house rebuilt).
    pub always_detailed: bool,
    /// The house's own stairs and lifts, and the staircases the household built.
    base_stairs: Vec<StairLink>,
    pub built_stairs: Vec<BuiltStairs>,
    /// The lot came with a house (whose roof the game's imposter shows from afar).
    pub had_house: bool,
    /// What's spawned for each wall, the floors, the household's stairs and roofs (to redraw).
    wall_entities: HashMap<u32, Vec<Entity>>,
    floor_entities: Vec<Entity>,
    stair_entities: Vec<Entity>,
    fence_entities: Vec<Entity>,
    pool_entities: Vec<Entity>,
    roof_entity: Option<Entity>,
    /// The roof pattern on the household's rooms.
    pub roof_texture: Key,
    /// The stair style the household's staircases are built in (the house's own, or one of the
    /// town's): its pieces and railings.
    pub stair_style: Option<StairBaked>,
    /// The building of the community lot the household is out at (for its floors).
    pub away: Option<Box<ActiveBuilding>>,
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

    /// Whether a point on the ground floor is inside a room (not a porch or the yard).
    pub fn is_indoors(&self, p: Vec3) -> bool {
        let l = self.local(p);
        self.floor_kinds.get(&(1, l.x.floor() as i32, l.y.floor() as i32)).is_some_and(|k| *k != s3bake::ROOM_PORCH && *k != s3bake::ROOM_OUTSIDE)
    }

    fn dir(&self, x: f32, z: f32) -> Vec3 {
        self.rot * Vec3::new(x, 0.0, z)
    }

    /// The floor (1 = ground) a height is on.
    pub fn level_at(&self, y: f32) -> u8 {
        let mut level = 1;
        for (l, h) in self.levels.iter().enumerate().skip(1) {
            if y > h - 0.3 {
                level = l as u8;
            }
        }
        level
    }

    /// Refreshes the floor lookups after the floors changed.
    fn reindex(&mut self) {
        self.floor_cells = self.data.floors.iter().map(|f| ((f.level, f.x as i32, f.z as i32), f.mask)).collect();
        self.floor_heights = self.data.floors.iter().filter_map(|f| Some(((f.level, f.x as i32, f.z as i32), f.y?))).collect();
        self.floor_kinds = self.data.floors.iter().map(|f| ((f.level, f.x as i32, f.z as i32), f.kind)).collect();
    }

    /// Doors and windows set in the wall section from `p` to `q` on `level`.
    pub fn openings_on(&self, level: u8, p: Vec2, q: Vec2) -> Vec<Entity> {
        let w = WallBaked { a: p.into(), b: q.into(), level, left: ROOM_OUTSIDE, right: ROOM_OUTSIDE, cover: [NO_COVER; 2], y: None };
        self.holes.iter().filter(|(_, h)| h.cuts(&w)).filter_map(|(e, _)| *e).collect()
    }

    /// Whether a door or window is in the wall behind something hung on it, `tiles` wide (as
    /// `build::snap_to_wall` puts it: at `at`, facing out of the wall).
    pub fn opening_behind(&self, at: Vec3, rot: Quat, tiles: u32) -> bool {
        let fwd = self.local_dir(rot * Vec3::Z);
        let dir = fwd.perp();
        let n = tiles.max(1);
        let start = self.local(at) - fwd * 0.5 - dir * (n as f32 * 0.5);
        (0..n).any(|k| {
            let p = start + dir * k as f32;
            let w = WallBaked { a: p.into(), b: (p + dir).into(), level: self.view_level, left: ROOM_OUTSIDE, right: ROOM_OUTSIDE, cover: [NO_COVER; 2], y: None };
            self.holes.iter().any(|(_, h)| h.cuts(&w))
        })
    }

    /// Lot-local direction of a world rotation's axis.
    fn local_dir(&self, v: Vec3) -> Vec2 {
        let l = self.rot.inverse() * v;
        Vec2::new(l.x, l.z).normalize_or_zero()
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
        (mask & (1 << t) != 0).then(|| self.floor_heights.get(&(level, x as i32, z as i32)).copied().unwrap_or(self.levels[level as usize]))
    }

    /// Room kind (`ROOM_*`) of the floor at `p` on `level`, if the house has floor there.
    pub fn room_at(&self, level: u8, p: Vec3) -> Option<u8> {
        self.floor_y(level, p)?;
        let l = self.local(p);
        self.floor_kinds.get(&(level, l.x.floor() as i32, l.y.floor() as i32)).copied()
    }

    /// The front door for someone coming from `from` (world): of the ground floor's doors from
    /// a room to the outdoors, the nearest; where a caller stands outside it (world), and the
    /// door itself.
    pub fn front_door(&self, from: Vec2) -> Option<(Vec2, Option<Entity>)> {
        let indoors = |p: Vec2| self.room_at(1, self.world(p.x, p.y, 0.0)).is_some_and(|k| !matches!(k, ROOM_PORCH | ROOM_OUTSIDE));
        let from = self.local(Vec3::new(from.x, 0.0, from.y));
        self.holes
            .iter()
            .filter(|(_, h)| h.door && h.level.max(1) == 1)
            .filter_map(|(e, h)| {
                let (a, b) = (h.wall_point + h.fwd * 0.8, h.wall_point - h.fwd * 0.8);
                match (indoors(a), indoors(b)) {
                    (false, true) => Some((a, *e)),
                    (true, false) => Some((b, *e)),
                    _ => None,
                }
            })
            .min_by(|p, q| p.0.distance(from).total_cmp(&q.0.distance(from)))
            .map(|(p, e)| (self.world(p.x, p.y, 0.0).xz(), e))
    }
}

/// Height a Sim stands at: the house's ground floor when inside it, else the terrain.
pub fn walk_height(world: &WorldInfo, building: Option<&ActiveBuilding>, p: Vec3) -> f32 {
    building
        .and_then(|b| b.floor_y(1, p).or_else(|| b.away.as_ref().and_then(|a| a.floor_y(1, p))))
        .unwrap_or_else(|| world.heightmap.sample(p.x, p.z))
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

/// Whether an object hangs on a wall, by its model's bounds: the game's paintings, mirrors and
/// wall lamps hang behind their middle (on the wall at the back of their tile), off the floor.
pub fn hangs_on_wall((mn, mx): (Vec3, Vec3)) -> bool {
    mn.y > 0.3 && mx.z < 0.0 && mn.z < -0.3
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
#[derive(Clone)]
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

impl Hole {
    /// The opening a door (or window) makes: it stands half a tile in front of its wall, facing
    /// away from it; `mn`/`mx` are its model's bounds.
    fn new(local: Vec2, fwd: Vec2, right: Vec2, (mn, mx): (Vec3, Vec3), door: bool, level: u8) -> Self {
        Hole {
            level,
            wall_point: local - fwd * 0.5,
            fwd,
            right,
            x0: mn.x,
            x1: mx.x,
            y0: if door { 0.0 } else { mn.y.max(0.0) },
            y1: mx.y.min(WALL_H - 0.05),
            door,
        }
    }

    /// Whether the opening goes through (a section of) wall `w`.
    fn cuts(&self, w: &s3bake::types::WallBaked) -> bool {
        let (a, b) = (Vec2::from(w.a), Vec2::from(w.b));
        let len = (b - a).length();
        if len < 1e-3 || self.level.max(1) != w.level.max(1) || ((b - a) / len).dot(self.right).abs() < 0.9 {
            return false;
        }
        let d = (a + b) * 0.5 - self.wall_point;
        let t = d.dot(self.right);
        d.dot(self.fwd).abs() < 0.3 && t > self.x0 - 0.1 && t < self.x1 + 0.1
    }
}

/// The vertical spans the openings cut out of wall `w`, and whether one is a door.
fn openings(holes: &[(Option<Entity>, Hole)], w: &s3bake::types::WallBaked) -> (Vec<(f32, f32)>, bool) {
    let mut spans = Vec::new();
    let mut door = false;
    for (_, h) in holes {
        if h.cuts(w) {
            spans.push((h.y0, h.y1));
            door |= h.door;
        }
    }
    (spans, door)
}

/// Whether a catalogue script is a door or archway (`Some(true)`) or a window (`Some(false)`).
pub fn is_opening(script: &str) -> Option<bool> {
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

/// The walls of a community lot being visited, full height or cut down (shown as the camera
/// comes in close, as the game's cutaway does).
#[derive(Component)]
pub struct VisitWalls {
    pub cut: bool,
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
pub fn spawn_floors(commands: &mut Commands, assets: &mut ObjectAssets, ctx: &mut AssetCtx, b: &LotBuildingBaked, active: &ActiveBuilding, neighbor: Option<Entity>) -> Vec<Entity> {
    let level_y = |l: u8| b.levels.get(l as usize).copied().unwrap_or(b.levels[b.levels.len() - 1]);
    let cover_key = |i: u16| (i != s3bake::types::NO_COVER).then(|| b.covers.get(i as usize).copied()).flatten();
    let mut floor_bufs: HashMap<(u8, Key), MeshBuf> = HashMap::new();
    for f in &b.floors {
        // (Ground-level paving sits just above the terrain; a tile may stand at its own height.)
        let y = f.y.unwrap_or(level_y(f.level)) + if f.level == 0 { 0.03 } else { 0.012 };
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
            // (Paving on the ground follows it.)
            let at = |p: Vec2| match f.level {
                0 => b.ground_at(p.x, p.y).map_or(y, |g| g + 0.03),
                _ => y,
            };
            buf.tri(pts.map(|p| active.world(p.x, p.y, at(p))), pts.map(|p| [p.x, p.y]), Vec3::Y);
        }
    }
    let mut out = Vec::new();
    for ((level, style), buf) in floor_bufs {
        let mat = surface_material(assets, ctx, style);
        let e = commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat))).id();
        if neighbor.is_none() {
            commands.entity(e).insert(FloorMesh);
        }
        place(commands, e, neighbor, level);
        out.push(e);
    }
    out
}

/// One repainting of the active house: a wall side, or a floor tile, given a covering texture.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PaintOp {
    Wall { wall: u32, side: u8, texture: Key },
    Floor { level: u8, x: u16, z: u16, texture: Key },
    /// A wall section built (added to the end of the walls).
    AddWall { a: [f32; 2], b: [f32; 2], level: u8 },
    /// A wall knocked down (left in the list with no length, so the others keep their indices).
    RemoveWall { wall: u32 },
    /// A wall cut in two at `at`: it keeps the part from its start; the rest is added to the end.
    SplitWall { wall: u32, at: [f32; 2] },
    /// A floor tile laid; `region` groups a room's tiles for painting (0 = a tile on its own).
    AddFloor { level: u8, x: u16, z: u16, region: u16 },
    RemoveFloor { level: u8, x: u16, z: u16 },
    /// A staircase built (see [`BuiltStairs`]), or the one standing on tile (`x`, `z`) taken away.
    AddStairs { x: u16, z: u16, dir: u8, level: u8 },
    RemoveStairs { x: u16, z: u16, level: u8 },
    /// The roof pattern for the household's rooms.
    Roof { texture: Key },
    /// A fence run built along a grid edge (with posts at its ends, for fences that have them).
    AddFence { a: [f32; 2], b: [f32; 2], level: u8, model: Key, post: Option<Key> },
    /// The fence along a grid edge taken down (and posts left standing alone).
    RemoveFence { a: [f32; 2], b: [f32; 2], level: u8 },
    /// A pool tile dug (on the ground floor, out of doors), or filled in again.
    AddPool { x: u16, z: u16 },
    RemovePool { x: u16, z: u16 },
}

/// How deep a pool dug in build mode is (a storey down, as the game's).
pub const POOL_DEPTH: f32 = -2.6;

/// The active lot's pool changed: the ground over it opens (or closes) to match.
#[derive(Message, Clone, Copy)]
pub struct PoolChanged;

/// The active house's repaintings since it was built (kept in saves).
#[derive(Resource, Default, Clone)]
pub struct LotPaint(pub Vec<PaintOp>);

/// Repaints and rebuilds the active house: applies the operations to its data and redraws the
/// walls painted, built or knocked down and, if any floor changed, the floors.
#[allow(clippy::too_many_arguments)]
pub fn repaint(
    commands: &mut Commands,
    b: &mut ActiveBuilding,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    ops: &[PaintOp],
    faces: &mut Query<(&WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let mut mats: HashMap<Key, Handle<StandardMaterial>> = HashMap::new();
    let mut floors_changed = false;
    let mut walls_changed: BTreeSet<u32> = BTreeSet::new();
    let mut structure = false;
    let mut stairs_changed = false;
    let mut roof_changed = false;
    let mut fences_changed = false;
    let mut pool_changed = false;
    for op in ops {
        apply_paint(&mut b.data, op);
        let last = b.data.walls.len().saturating_sub(1) as u32;
        match *op {
            PaintOp::Wall { wall, side, texture } => {
                let m = mats.entry(texture).or_insert_with(|| surface_material(assets, ctx, texture)).clone();
                for (f, mut mat) in faces.iter_mut() {
                    if f.wall == wall && f.side == side {
                        mat.0 = m.clone();
                    }
                }
                walls_changed.extend(b.built.get(&wall));
            }
            PaintOp::Floor { .. } | PaintOp::AddFloor { .. } | PaintOp::RemoveFloor { .. } => floors_changed = true,
            PaintOp::AddWall { .. } => {
                b.built.insert(last);
                walls_changed.insert(last);
                structure = true;
            }
            PaintOp::RemoveWall { wall } => {
                b.built.remove(&wall);
                walls_changed.insert(wall);
                structure = true;
            }
            PaintOp::SplitWall { wall, .. } => {
                if b.built.contains(&wall) {
                    b.built.insert(last);
                }
                walls_changed.extend([wall, last]);
                structure = true;
            }
            PaintOp::AddStairs { x, z, dir, level } => {
                b.built_stairs.push(BuiltStairs { x, z, dir, level });
                stairs_changed = true;
            }
            PaintOp::RemoveStairs { x, z, level } => {
                let at = IVec2::new(x as i32, z as i32);
                b.built_stairs.retain(|s| !(s.level == level && s.tiles().contains(&at)));
                stairs_changed = true;
            }
            PaintOp::Roof { texture } => {
                b.roof_texture = texture;
                roof_changed = true;
            }
            PaintOp::AddFence { .. } | PaintOp::RemoveFence { .. } => fences_changed = true,
            PaintOp::AddPool { .. } | PaintOp::RemovePool { .. } => pool_changed = true,
        }
    }
    if pool_changed {
        for e in b.pool_entities.drain(..) {
            commands.entity(e).try_despawn();
        }
        let data = b.data.clone();
        b.pool_entities = spawn_pool(commands, assets, ctx, &data, b, None);
        commands.queue(|w: &mut World| {
            w.write_message(PoolChanged);
        });
    }
    if fences_changed {
        for e in b.fence_entities.drain(..) {
            commands.entity(e).try_despawn();
        }
        let data = b.data.clone();
        b.fence_entities = spawn_fences(commands, assets, ctx, &data, b, None);
    }
    if b.data.levels.len() > b.levels.len() {
        b.levels = b.data.levels.clone();
    }
    // Floors with something on them are in view (and walkable).
    let highest = b.data.walls.iter().map(|w| w.level).chain(b.data.floors.iter().map(|f| f.level)).max().unwrap_or(1);
    b.top_level = b.top_level.max(highest.min(b.levels.len().saturating_sub(1) as u8));
    if stairs_changed {
        respawn_stairs(commands, b, assets, ctx);
        b.always_detailed = true;
    }
    if floors_changed {
        b.reindex();
    }
    if structure {
        walls_changed.extend(restyle(b));
        b.always_detailed = true;
        // Cutaway works around the middle of the walls.
        let mids: Vec<Vec2> = b.data.walls.iter().filter(|w| Vec2::from(w.a) != Vec2::from(w.b)).map(|w| (Vec2::from(w.a) + Vec2::from(w.b)) * 0.5).collect();
        if !mids.is_empty() {
            let c = mids.iter().copied().sum::<Vec2>() / mids.len() as f32;
            b.center = b.world(c.x, c.y, b.corner.y);
        }
    }
    respawn_walls(commands, b, assets, ctx, &walls_changed);
    if structure || floors_changed || roof_changed {
        respawn_roofs(commands, b, assets, ctx);
    }
    if floors_changed {
        for e in b.floor_entities.drain(..) {
            commands.entity(e).try_despawn();
        }
        let data = b.data.clone();
        let floors = spawn_floors(commands, assets, ctx, &data, b, None);
        b.floor_entities = floors;
    }
}

/// Redraws walls of the active house from its data (with the openings cut into them).
fn respawn_walls(commands: &mut Commands, b: &mut ActiveBuilding, assets: &mut ObjectAssets, ctx: &mut AssetCtx, walls: &BTreeSet<u32>) {
    let mut mats: HashMap<Key, Handle<StandardMaterial>> = HashMap::new();
    let mut material = |assets: &mut ObjectAssets, ctx: &mut AssetCtx, key: Key| mats.entry(key).or_insert_with(|| surface_material(assets, ctx, key)).clone();
    for &i in walls {
        for e in b.wall_entities.remove(&i).unwrap_or_default() {
            commands.entity(e).try_despawn();
        }
        let Some(w) = b.data.walls.get(i as usize).copied() else { continue };
        let (spans, door) = openings(&b.holes, &w);
        let ents = spawn_wall(commands, assets, ctx, b, i as usize, &w, &spans, door, b.exterior, &b.cap_mat, &mut material);
        b.wall_entities.insert(i, ents);
    }
}

/// Every tile of `level` with the room it's in (row by row along z; 0 = outdoors). Rooms are
/// what the walls along the lot's grid lines close off from the lot's edge.
pub fn rooms(b: &LotBuildingBaked, level: u8) -> Vec<u16> {
    let (w, d) = (b.width as usize, b.depth as usize);
    // Walled grid edges: along x on line z (`hz`), and along z on line x (`vt`).
    let mut hz = vec![false; w * (d + 1)];
    let mut vt = vec![false; (w + 1) * d];
    for wall in b.walls.iter().filter(|wl| wl.level.max(1) == level) {
        let (a, c) = (Vec2::from(wall.a), Vec2::from(wall.b));
        if a.distance(c) < 0.5 {
            continue;
        }
        let on_line = |v: f32| (v - v.round()).abs() < 0.05;
        if (a.y - c.y).abs() < 0.05 && on_line(a.y) {
            let z = a.y.round() as i32;
            if (0..=d as i32).contains(&z) {
                for x in (a.x.min(c.x).round() as i32).max(0)..(a.x.max(c.x).round() as i32).min(w as i32) {
                    hz[z as usize * w + x as usize] = true;
                }
            }
        } else if (a.x - c.x).abs() < 0.05 && on_line(a.x) {
            let x = a.x.round() as i32;
            if (0..=w as i32).contains(&x) {
                for z in (a.y.min(c.y).round() as i32).max(0)..(a.y.max(c.y).round() as i32).min(d as i32) {
                    vt[z as usize * (w + 1) + x as usize] = true;
                }
            }
        }
    }
    // Flood the grid, with a ring of outdoor cells around it (padded coordinates).
    let (pw, pd) = (w as i32 + 2, d as i32 + 2);
    let mut label = vec![u16::MAX; (pw * pd) as usize];
    let blocked = |x: i32, z: i32, dx: i32, dz: i32| -> bool {
        // Crossing from padded cell (x, z) to its neighbour (x + dx, z + dz).
        if dx != 0 {
            let line = x.max(x + dx) - 1; // the grid line x between the cells (lot coordinates)
            let rz = z - 1;
            rz >= 0 && rz < d as i32 && line <= w as i32 && line >= 0 && vt[rz as usize * (w + 1) + line as usize]
        } else {
            let line = z.max(z + dz) - 1;
            let rx = x - 1;
            rx >= 0 && rx < w as i32 && line <= d as i32 && line >= 0 && hz[line as usize * w + rx as usize]
        }
    };
    let mut next_room = 0u16;
    for start in 0..label.len() {
        if label[start] != u16::MAX {
            continue;
        }
        let id = next_room;
        next_room += 1;
        let mut stack = vec![start as i32];
        label[start] = id;
        while let Some(c) = stack.pop() {
            let (x, z) = (c % pw, c / pw);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx < 0 || nz < 0 || nx >= pw || nz >= pd || blocked(x, z, dx, dz) {
                    continue;
                }
                let n = (nz * pw + nx) as usize;
                if label[n] == u16::MAX {
                    label[n] = id;
                    stack.push(n as i32);
                }
            }
        }
    }
    // The first region flooded holds the padding: the outdoors.
    let mut out = vec![0u16; w * d];
    for z in 0..d {
        for x in 0..w {
            out[z * w + x] = label[(z as i32 + 1) as usize * pw as usize + x + 1];
        }
    }
    out
}

/// Floor for the rooms on `level` that have none yet (as the game lays when walls close a room),
/// and the outdoor floor they close in (a deck's) made theirs.
pub fn room_floors(b: &LotBuildingBaked, level: u8) -> Vec<PaintOp> {
    let rooms = rooms(b, level);
    let w = b.width as usize;
    let have: HashSet<(u16, u16)> = b.floors.iter().filter(|f| f.level == level && f.region > 0).map(|f| (f.x, f.z)).collect();
    let base = b.floors.iter().map(|f| f.region).max().unwrap_or(0);
    let mut ops = Vec::new();
    for (i, &room) in rooms.iter().enumerate() {
        let (x, z) = ((i % w) as u16, (i / w) as u16);
        if room > 0 && !have.contains(&(x, z)) {
            ops.push(PaintOp::AddFloor { level, x, z, region: base + room });
        }
    }
    ops
}

/// Faces the sides of the household's own walls into the rooms they close off (or the
/// outdoors); returns the walls whose sides changed.
fn restyle(b: &mut ActiveBuilding) -> Vec<u32> {
    let levels: BTreeSet<u8> = b.built.iter().filter_map(|&i| b.data.walls.get(i as usize)).map(|w| w.level.max(1)).collect();
    let rooms: HashMap<u8, Vec<u16>> = levels.into_iter().map(|l| (l, rooms(&b.data, l))).collect();
    let (w, d) = (b.data.width as i32, b.data.depth as i32);
    let mut changed = Vec::new();
    let mut built: Vec<u32> = b.built.iter().copied().collect();
    built.sort();
    for i in built {
        let Some(wall) = b.data.walls.get(i as usize).copied() else { continue };
        let (a, c) = (Vec2::from(wall.a), Vec2::from(wall.b));
        let len = a.distance(c);
        if len < 1e-3 {
            continue;
        }
        let level = wall.level.max(1);
        let n = Vec2::new(-(c.y - a.y), c.x - a.x) / len;
        let mid = (a + c) * 0.5;
        let kind_at = |p: Vec2| {
            let (x, z) = (p.x.floor() as i32, p.y.floor() as i32);
            if x < 0 || z < 0 || x >= w || z >= d || rooms[&level][(z * w + x) as usize] == 0 {
                return ROOM_OUTSIDE;
            }
            // Indoors: the room's own kind where the house has one.
            match b.floor_kinds.get(&(level, x, z)) {
                Some(&k) if k != ROOM_OUTSIDE && k != ROOM_PORCH => k,
                _ => ROOM_LIVING,
            }
        };
        let (l, r) = (kind_at(mid + n * 0.5), kind_at(mid - n * 0.5));
        if (l, r) != (wall.left, wall.right) {
            let wl = &mut b.data.walls[i as usize];
            wl.left = l;
            wl.right = r;
            changed.push(i);
        }
    }
    changed
}

/// Doors and windows placed in the house cut their openings into the walls behind them (and
/// close them up again when they're moved or sold).
#[allow(clippy::too_many_arguments)]
fn cut_openings(
    mut commands: Commands,
    building: Option<ResMut<ActiveBuilding>>,
    (catalog, data): (Res<Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    added: Query<(Entity, &crate::interact::GameObject, &Transform), Added<crate::interact::GameObject>>,
    mut removed: RemovedComponents<crate::interact::GameObject>,
    mut grid: Option<ResMut<crate::nav::NavGrid>>,
) {
    let gone: Vec<Entity> = removed.read().collect();
    let Some(mut b) = building else { return };
    let mut changed: BTreeSet<u32> = BTreeSet::new();
    let walls_cut = |b: &ActiveBuilding, h: &Hole| -> Vec<u32> { b.data.walls.iter().enumerate().filter(|(_, w)| h.cuts(w)).map(|(i, _)| i as u32).collect() };
    if !gone.is_empty() {
        let closed: Vec<Hole> = b.holes.iter().filter(|(e, _)| e.is_some_and(|e| gone.contains(&e))).map(|(_, h)| h.clone()).collect();
        for h in &closed {
            changed.extend(walls_cut(&b, h));
        }
        b.holes.retain(|(e, _)| !e.is_some_and(|e| gone.contains(&e)));
    }
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    for (e, obj, tf) in &added {
        if b.holes.iter().any(|(he, _)| *he == Some(e)) {
            continue;
        }
        let Some(bounds) = parts_bounds(&assets.object(&mut ctx, obj.objd)) else { continue };
        let level = b.level_at(tf.translation.y);
        let Some(door) = catalog.by_key(&obj.objd).and_then(|c| c.opening) else {
            // Paintings, mirrors and wall lamps hang on the wall behind them: cut away with it.
            if hangs_on_wall(bounds) {
                let mid = tf.translation - tf.rotation * Vec3::Z * 0.5;
                commands.entity(e).insert((WallObject { mid }, BuildingPiece { level }, Floor(level)));
            }
            continue;
        };
        let hole = Hole::new(b.local(tf.translation), b.local_dir(tf.rotation * Vec3::Z), b.local_dir(tf.rotation * Vec3::X), bounds, door, level);
        changed.extend(walls_cut(&b, &hole));
        let mid = b.world(hole.wall_point.x, hole.wall_point.y, tf.translation.y);
        commands.entity(e).remove::<Obstacle>().insert((WallObject { mid }, BuildingPiece { level }, Floor(level)));
        b.holes.push((Some(e), hole));
    }
    if changed.is_empty() {
        return;
    }
    respawn_walls(&mut commands, &mut b, &mut assets, &mut ctx, &changed);
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
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
        PaintOp::AddWall { a, b: end, level } => {
            while b.levels.len() <= level as usize {
                let top = b.levels[b.levels.len() - 1];
                b.levels.push(top + s3bake::building::LEVEL_HEIGHT);
            }
            b.walls.push(WallBaked { a, b: end, level, left: ROOM_OUTSIDE, right: ROOM_OUTSIDE, cover: [NO_COVER; 2], y: None });
        }
        PaintOp::RemoveWall { wall } => {
            if let Some(w) = b.walls.get_mut(wall as usize) {
                w.b = w.a;
            }
        }
        PaintOp::SplitWall { wall, at } => {
            if let Some(w) = b.walls.get(wall as usize).copied() {
                let p = Vec2::from(at);
                if p.distance(Vec2::from(w.a)) > 0.05 && p.distance(Vec2::from(w.b)) > 0.05 {
                    b.walls[wall as usize].b = at;
                    b.walls.push(WallBaked { a: at, ..w });
                }
            }
        }
        PaintOp::AddFloor { level, x, z, region } => match b.floors.iter_mut().find(|f| f.level == level && f.x == x && f.z == z) {
            // (An outdoor floor walls now close in, a deck's say: the room's.)
            Some(f) => {
                f.mask = 0xF;
                if region > 0 && f.region == 0 {
                    f.region = region;
                    f.kind = ROOM_LIVING;
                }
            }
            None => b.floors.push(FloorBaked {
                level,
                x,
                z,
                mask: 0xF,
                kind: if region > 0 { ROOM_LIVING } else { ROOM_OUTSIDE },
                region,
                cover: [NO_COVER; 4],
                y: None,
            }),
        },
        PaintOp::RemoveFloor { level, x, z } => b.floors.retain(|f| !(f.level == level && f.x == x && f.z == z)),
        PaintOp::AddStairs { level, .. } => {
            while b.levels.len() <= level as usize + 1 {
                let top = b.levels[b.levels.len() - 1];
                b.levels.push(top + s3bake::building::LEVEL_HEIGHT);
            }
        }
        PaintOp::RemoveStairs { .. } | PaintOp::Roof { .. } => {}
        PaintOp::AddFence { a, b: end, level, model, post } => {
            let same = |f: &s3bake::FenceBaked| f.level == level && ((f.a == a && f.b == end) || (f.a == end && f.b == a));
            if !b.fences.iter().any(same) {
                b.fences.push(s3bake::FenceBaked { a, b: end, level, model });
            }
            if let Some(post) = post {
                for p in [a, end] {
                    if !b.fences.iter().any(|f| f.level == level && f.a == p && f.b == p) {
                        b.fences.push(s3bake::FenceBaked { a: p, b: p, level, model: post });
                    }
                }
            }
        }
        PaintOp::AddPool { x, z } => {
            if !b.pool.iter().any(|f| f.x == x && f.z == z) {
                b.pool.push(FloorBaked { level: 0, x, z, mask: 0xF, kind: ROOM_OUTSIDE, region: 0, cover: [NO_COVER; 4], y: None });
            }
            if b.pool_depth == 0.0 {
                b.pool_depth = POOL_DEPTH;
            }
        }
        PaintOp::RemovePool { x, z } => b.pool.retain(|f| !(f.x == x && f.z == z)),
        PaintOp::RemoveFence { a, b: end, level } => {
            let near = |p: [f32; 2], q: [f32; 2]| Vec2::from(p).distance(Vec2::from(q)) < 0.05;
            b.fences.retain(|f| !(f.level == level && f.a != f.b && ((near(f.a, a) && near(f.b, end)) || (near(f.a, end) && near(f.b, a)))));
            // (Posts with no run left at them go too.)
            let runs: Vec<([f32; 2], [f32; 2])> = b.fences.iter().filter(|f| f.level == level && f.a != f.b).map(|f| (f.a, f.b)).collect();
            b.fences.retain(|f| f.level != level || f.a != f.b || !(near(f.a, a) || near(f.a, end)) || runs.iter().any(|r| near(r.0, f.a) || near(r.1, f.a)));
        }
    }
}

/// A pool let into the ground: its tiled floor and sides, a stone coping round the edge, and
/// the water (given its material by the water module).
fn spawn_pool(commands: &mut Commands, assets: &mut ObjectAssets, ctx: &mut AssetCtx, b: &LotBuildingBaked, active: &ActiveBuilding, neighbor: Option<Entity>) -> Vec<Entity> {
    let mut out = Vec::new();
    if b.pool.is_empty() {
        return out;
    }
    let level_y = |l: u8| b.levels.get(l as usize).copied().unwrap_or(b.levels[b.levels.len() - 1]);
    let tiles: HashSet<(i32, i32)> = b.pool.iter().map(|f| (f.x as i32, f.z as i32)).collect();
    let cover_key = |i: u16| (i != s3bake::types::NO_COVER).then(|| b.covers.get(i as usize).copied()).flatten();
    let ground_y = |x: f32, z: f32| b.ground_at(x, z).unwrap_or(level_y(0));
    let floor_y = level_y(0) + b.pool_depth;
    let water_y = level_y(0) - 0.22;
    let mut bufs: HashMap<Key, MeshBuf> = HashMap::new();
    let mut coping = MeshBuf::default();
    let mut water = MeshBuf::default();
    for f in &b.pool {
        let (x, z) = (f.x as f32, f.z as f32);
        let key = f.cover.iter().find_map(|&c| cover_key(c)).unwrap_or(STYLE_FLOOR_TERRACOTTA);
        let buf = bufs.entry(key).or_default();
        buf.quad(
            [active.world(x, z, floor_y), active.world(x + 1.0, z, floor_y), active.world(x + 1.0, z + 1.0, floor_y), active.world(x, z + 1.0, floor_y)],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            Vec3::Y,
        );
        water.quad(
            [active.world(x, z, water_y), active.world(x + 1.0, z, water_y), active.world(x + 1.0, z + 1.0, water_y), active.world(x, z + 1.0, water_y)],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            Vec3::Y,
        );
        // Sides where the pool ends: tiled down to the floor, the coping along the top.
        for (dx, dz, a, c) in [(-1, 0, (x, z + 1.0), (x, z)), (1, 0, (x + 1.0, z), (x + 1.0, z + 1.0)), (0, -1, (x, z), (x + 1.0, z)), (0, 1, (x + 1.0, z + 1.0), (x, z + 1.0))] {
            if tiles.contains(&(f.x as i32 + dx, f.z as i32 + dz)) {
                continue;
            }
            let inward = active.dir(-dx as f32, -dz as f32);
            let (ya, yc) = (ground_y(a.0, a.1), ground_y(c.0, c.1));
            buf.quad(
                [active.world(a.0, a.1, floor_y), active.world(c.0, c.1, floor_y), active.world(c.0, c.1, yc), active.world(a.0, a.1, ya)],
                [[0.0, 0.0], [1.0, 0.0], [1.0, ya - floor_y], [0.0, yc - floor_y]],
                inward,
            );
            let out = Vec2::new(dx as f32, dz as f32) * 0.3;
            coping.quad(
                [active.world(a.0, a.1, ya + 0.04), active.world(c.0, c.1, yc + 0.04), active.world(c.0 + out.x, c.1 + out.y, yc + 0.04), active.world(a.0 + out.x, a.1 + out.y, ya + 0.04)],
                [[0.0, 0.0], [1.0, 0.0], [1.0, 0.3], [0.0, 0.3]],
                Vec3::Y,
            );
        }
        // The coping round the pool's outer corners.
        for (dx, dz) in [(-1, -1), (1, -1), (1, 1), (-1, 1)] {
            let (tx, tz) = (f.x as i32, f.z as i32);
            if tiles.contains(&(tx + dx, tz)) || tiles.contains(&(tx, tz + dz)) {
                continue;
            }
            let (cx, cz) = (x + (dx + 1) as f32 * 0.5, z + (dz + 1) as f32 * 0.5);
            let y = ground_y(cx, cz) + 0.04;
            let (ox, oz) = (dx as f32 * 0.3, dz as f32 * 0.3);
            let q = [active.world(cx, cz, y), active.world(cx + ox, cz, y), active.world(cx + ox, cz + oz, y), active.world(cx, cz + oz, y)];
            coping.quad(q, [[0.0, 0.0], [0.3, 0.0], [0.3, 0.3], [0.0, 0.3]], Vec3::Y);
        }
    }
    for (key, buf) in bufs {
        let mat = surface_material(assets, ctx, key);
        let e = commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat))).id();
        place(commands, e, neighbor, 1);
        out.push(e);
    }
    let stone = ctx.materials.add(StandardMaterial { base_color: Color::srgb(0.82, 0.8, 0.74), perceptual_roughness: 0.8, ..default() });
    let e = commands.spawn((Mesh3d(ctx.meshes.add(coping.mesh())), MeshMaterial3d(stone))).id();
    place(commands, e, neighbor, 1);
    out.push(e);
    let e = commands.spawn((Mesh3d(ctx.meshes.add(water.mesh())), Transform::default(), Visibility::default(), crate::water::PoolWater)).id();
    place(commands, e, neighbor, 1);
    out.push(e);
    out
}

/// Fences and railings: each run (and post) is its fence's piece, turned along the run.
fn spawn_fences(commands: &mut Commands, assets: &mut ObjectAssets, ctx: &mut AssetCtx, b: &LotBuildingBaked, active: &ActiveBuilding, neighbor: Option<Entity>) -> Vec<Entity> {
    let level_y = |l: u8| b.levels.get(l as usize).copied().unwrap_or(b.levels[b.levels.len() - 1]);
    let mut out = Vec::new();
    for f in &b.fences {
        let parts = assets.model(ctx, f.model);
        if parts.is_empty() {
            continue;
        }
        let (a, c) = (Vec2::from(f.a), Vec2::from(f.b));
        let y = if f.level == 0 { b.ground_at(a.x, a.y).unwrap_or(level_y(0)) } else { level_y(f.level) };
        let d = c - a;
        let turn = if d.length_squared() > 1e-6 { Quat::from_rotation_y((-d.y).atan2(d.x)) } else { Quat::IDENTITY };
        let tf = Transform::from_translation(active.world(a.x, a.y, y)).with_rotation(active.rot * turn);
        let e = crate::objects::spawn_parts(commands, &parts, tf);
        place(commands, e, neighbor, f.level.max(1));
        // (A run is a barrier along its length, on its own floor.)
        if d.length_squared() > 1e-6 {
            let len = d.length();
            commands.entity(e).insert((crate::nav::Obstacle { half: Vec2::new(len * 0.5, 0.06), center_offset: Vec2::new(len * 0.5, 0.0) }, crate::nav::Floor(f.level.max(1))));
        }
        out.push(e);
    }
    out
}

/// The building data of a lot with no house yet: nothing on it, its ground floor at `ground`.
/// The wooden deck a family moving onto an empty lot starts out on: the catalogue's Rustic
/// Wooden Planks in their weathered brown (its second swatch, as Build mode lays it).
pub const DECK_COVER: Key = (s3bake::types::T_COVER, 4 | 1 << 8, 0xEAEE_86E8_4CBD_73D4);

/// The starter deck's tiles on a lot (lot-local: first x, z and one past the last), under the
/// starter furniture's open-plan home in the middle of the lot.
pub fn deck_tiles(lot: &LotInfo) -> (u32, u32, u32, u32) {
    let (w, d) = (18.min(lot.width), 14.min(lot.depth));
    let (x0, z0) = ((lot.width - w) / 2, (lot.depth - d) / 2);
    (x0, z0, x0 + w, z0 + d)
}

/// How high the starter deck stands (a little over the highest ground under it, so the ground
/// never comes through), and the lowest ground under it (its foundation goes down to that).
pub fn deck_heights(lot: &LotInfo, hm: &s3formats::world::Heightmap) -> (f32, f32) {
    let (x0, z0, x1, z1) = deck_tiles(lot);
    let rot = Quat::from_rotation_y(lot.rotation);
    let corner = Vec3::from(lot.corner);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for x in x0..=x1 {
        for z in z0..=z1 {
            let p = corner + rot * Vec3::new(x as f32, 0.0, z as f32);
            let h = hm.sample(p.x, p.z);
            lo = lo.min(h);
            hi = hi.max(h);
        }
    }
    (hi + 0.12, lo)
}

/// An empty lot with the starter deck laid: its wooden floor tiles (outdoors: painted a tile at a
/// time, as the game's decks are), on a foundation down to the ground.
pub fn deck_building(lot_index: usize, lot: &LotInfo, hm: &s3formats::world::Heightmap) -> LotBuildingBaked {
    let (top, low) = deck_heights(lot, hm);
    let mut b = empty_building(lot_index, lot, top);
    b.levels = vec![low, top];
    b.covers = vec![DECK_COVER];
    let (x0, z0, x1, z1) = deck_tiles(lot);
    for x in x0..x1 {
        for z in z0..z1 {
            b.floors.push(s3bake::types::FloorBaked { level: 1, x: x as u16, z: z as u16, mask: 0xF, kind: s3bake::ROOM_PORCH, region: 0, cover: [0; 4], y: None });
        }
    }
    // (Its sides, facing out.)
    let c = |x: u32, z: u32| [x as f32, z as f32];
    b.foundation = vec![(c(x0, z0), c(x0, z1)), (c(x0, z1), c(x1, z1)), (c(x1, z1), c(x1, z0)), (c(x1, z0), c(x0, z0))];
    b
}

/// A position saved before the starter deck was raised (on the ground under it, or on
/// something standing there), lifted onto the deck: by how far the deck stands over the ground
/// there. Elsewhere, or on a house, as it was.
pub fn onto_deck(b: &ActiveBuilding, hm: &s3formats::world::Heightmap, p: Vec3) -> Vec3 {
    if b.data.is_house() {
        return p;
    }
    match b.floor_y(1, p) {
        Some(top) => p + Vec3::Y * (top - hm.sample(p.x, p.z)).max(0.0),
        None => p,
    }
}

pub fn empty_building(lot_index: usize, lot: &LotInfo, ground: f32) -> LotBuildingBaked {
    LotBuildingBaked {
        lot: lot_index as u32,
        width: lot.width,
        depth: lot.depth,
        levels: vec![ground, ground],
        walls: Vec::new(),
        floors: Vec::new(),
        foundation: Vec::new(),
        foundation_top: Vec::new(),
        objects: Vec::new(),
        covers: Vec::new(),
        ground: Vec::new(),
        pool: Vec::new(),
        pool_depth: 0.0,
        fences: Vec::new(),
        stairs: Vec::new(),
    }
}

/// A wall segment of the active house: a face per side (full and cut-away versions), its top
/// caps, and (without a door) the obstacle that keeps Sims from walking through it.
#[allow(clippy::too_many_arguments)]
fn spawn_wall(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    active: &ActiveBuilding,
    wall_index: usize,
    w: &s3bake::types::WallBaked,
    seg_holes: &[(f32, f32)],
    has_door: bool,
    exterior: Key,
    cap_mat: &Handle<StandardMaterial>,
    material: &mut dyn FnMut(&mut ObjectAssets, &mut AssetCtx, Key) -> Handle<StandardMaterial>,
) -> Vec<Entity> {
    let b = &active.data;
    let level_y = |l: u8| b.levels.get(l as usize).copied().unwrap_or(b.levels[b.levels.len() - 1]);
    let cover_key = |i: u16| (i != s3bake::types::NO_COVER).then(|| b.covers.get(i as usize).copied()).flatten();
    let level = w.level.max(1);
    let y0 = w.y.unwrap_or(level_y(level));
    let (a, bb) = (Vec2::from(w.a), Vec2::from(w.b));
    let seg = bb - a;
    let len = seg.length();
    if len < 1e-3 {
        return Vec::new();
    }
    let along = seg / len;
    let n_local = Vec2::new(-along.y, along.x);
    let mid_local = (a + bb) * 0.5;
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
    let mut out = vec![parent];
    if !has_door {
        let dir = active.dir(along.x, along.y);
        out.push(
            commands
                .spawn((
                    Transform::from_translation(mid).with_rotation(Quat::from_rotation_y((-dir.z).atan2(dir.x))),
                    Obstacle { half: Vec2::new(len * 0.5 + 0.02, 0.08), center_offset: Vec2::ZERO },
                    Floor(level),
                    DespawnOnExit(AppState::InGame),
                ))
                .id(),
        );
    }
    out
}

/// The steps of a straight staircase from `bottom` running `run` along `d` (lot-local), from
/// floor height `y0` up to `y1`.
fn stair_mesh(active: &ActiveBuilding, bottom: Vec2, d: Vec2, run: f32, y0: f32, y1: f32) -> Mesh {
    let side = Vec2::new(-d.y, d.x) * 0.5;
    let steps = ((y1 - y0) / 0.25).round().max(1.0) as usize;
    let mut buf = MeshBuf::default();
    let w = |q: Vec2, y: f32| active.world(q.x, q.y, y);
    for i in 0..steps {
        let (t0, t1) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
        let a = bottom + d * (run * t0);
        let c = bottom + d * (run * t1);
        let h = y0 + (y1 - y0) * t1;
        // Tread.
        buf.quad([w(a - side, h), w(a + side, h), w(c + side, h), w(c - side, h)], [[0.0, t0 * run], [1.0, t0 * run], [1.0, t1 * run], [0.0, t1 * run]], Vec3::Y);
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
    buf.mesh()
}

/// A staircase the household built: four tiles from tile (`x`, `z`) on `level` up along `dir`
/// (0 = +x, 1 = +z, 2 = -x, 3 = -z) to the floor above.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BuiltStairs {
    pub x: u16,
    pub z: u16,
    pub dir: u8,
    pub level: u8,
}

/// How far a built staircase runs (tiles).
pub const STAIR_RUN: i32 = 4;

impl BuiltStairs {
    pub fn step(dir: u8) -> IVec2 {
        [IVec2::X, IVec2::Y, IVec2::NEG_X, IVec2::NEG_Y][(dir & 3) as usize]
    }

    /// The tiles it stands on, bottom first.
    pub fn tiles(&self) -> Vec<IVec2> {
        let s = Self::step(self.dir);
        (0..STAIR_RUN).map(|k| IVec2::new(self.x as i32, self.z as i32) + s * k).collect()
    }

    /// The tile it arrives at upstairs.
    pub fn landing(&self) -> IVec2 {
        IVec2::new(self.x as i32, self.z as i32) + Self::step(self.dir) * STAIR_RUN
    }

    /// Where its steps start (lot-local) and which way they climb.
    fn bottom(&self) -> (Vec2, Vec2) {
        let d = Self::step(self.dir).as_vec2();
        (Vec2::new(self.x as f32 + 0.5, self.z as f32 + 0.5) - d * 0.5, d)
    }
}

/// The roof the household's rooms get until another is chosen ("Asphalt Shingle Roof", as the
/// plain field of tiles baked from its atlas).
pub const ROOF_DEFAULT: Key = (T_COVER, 6, 0x70C833585F305186);
/// How far roofs reach past the walls, and how steep they are (rise per run).
const ROOF_OVERHANG: f32 = 0.35;
const ROOF_PITCH: f32 = 0.6;

/// A roof over the household's rooms (shown in the outside view, as the game's are).
#[derive(Component)]
pub struct BuiltRoof;

/// Splits the marked cells (`w` wide, row by row) into rectangles `(x0, z0, x1, z1)` (exclusive
/// ends), taking the widest run of each row and as many rows of it as fit.
fn rectangles(cells: &mut [bool], w: usize, d: usize) -> Vec<(usize, usize, usize, usize)> {
    let mut out = Vec::new();
    for z in 0..d {
        let mut x = 0;
        while x < w {
            if !cells[z * w + x] {
                x += 1;
                continue;
            }
            let mut x1 = x;
            while x1 < w && cells[z * w + x1] {
                x1 += 1;
            }
            let mut z1 = z + 1;
            while z1 < d && (x..x1).all(|i| cells[z1 * w + i]) {
                z1 += 1;
            }
            for zz in z..z1 {
                for xx in x..x1 {
                    cells[zz * w + xx] = false;
                }
            }
            out.push((x, z, x1, z1));
            x = x1;
        }
    }
    out
}

/// A hip roof over the lot-local rectangle (`x0`, `z0`)-(`x1`, `z1`) with its eaves at height `y`.
fn hip_roof(buf: &mut MeshBuf, b: &ActiveBuilding, (x0, z0, x1, z1): (f32, f32, f32, f32), y: f32) {
    let o = ROOF_OVERHANG;
    let (x0, z0, x1, z1) = (x0 - o, z0 - o, x1 + o, z1 + o);
    let y = y - o * ROOF_PITCH;
    // Roofed along x when it's the longer way (else along z: swap the axes).
    let along_x = x1 - x0 >= z1 - z0;
    let (a0, a1, c0, c1) = if along_x { (x0, x1, z0, z1) } else { (z0, z1, x0, x1) };
    let half = (c1 - c0) * 0.5;
    let top = y + half * ROOF_PITCH;
    let cm = c0 + half;
    let (r0, r1) = (a0 + half, a1 - half);
    let pt = |a: f32, c: f32, h: f32| if along_x { b.world(a, c, h) } else { b.world(c, a, h) };
    let slope = (1.0 + ROOF_PITCH * ROOF_PITCH).sqrt();
    let mut face = |pts: Vec<(f32, f32, f32)>, eave: ((f32, f32), (f32, f32))| {
        let p: Vec<Vec3> = pts.iter().map(|&(a, c, h)| pt(a, c, h)).collect();
        let mut n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n.y < 0.0 {
            n = -n;
        }
        // Texture along the eave and up the slope.
        let (e0, e1) = (pt(eave.0.0, eave.0.1, y), pt(eave.1.0, eave.1.1, y));
        let ed = (e1 - e0).with_y(0.0).normalize_or_zero();
        let up = Vec3::new(n.x, 0.0, n.z).normalize_or_zero() * -1.0;
        let m = s3bake::gamedata::ROOF_ATLAS_METRES;
        let uv = |q: Vec3| [(q - e0).dot(ed) / m, (q - e0).with_y(0.0).dot(up) * slope / m];
        if p.len() == 4 {
            buf.quad([p[0], p[1], p[2], p[3]], [uv(p[0]), uv(p[1]), uv(p[2]), uv(p[3])], n);
        } else {
            buf.tri([p[0], p[1], p[2]], [uv(p[0]), uv(p[1]), uv(p[2])], n);
        }
    };
    // The two long slopes, and the hipped ends.
    face(vec![(a0, c0, y), (a1, c0, y), (r1, cm, top), (r0, cm, top)], ((a0, c0), (a1, c0)));
    face(vec![(a1, c1, y), (a0, c1, y), (r0, cm, top), (r1, cm, top)], ((a1, c1), (a0, c1)));
    face(vec![(a0, c1, y), (a0, c0, y), (r0, cm, top)], ((a0, c1), (a0, c0)));
    face(vec![(a1, c0, y), (a1, c1, y), (r1, cm, top)], ((a1, c0), (a1, c1)));
}

/// Roofs over the household's rooms: each room's top floor (no floor above it) gets hip roofs
/// over rectangles of its tiles. On a lot that came with a house only rooms the household
/// walled get one (the game's imposter shows the house's own roof).
fn respawn_roofs(commands: &mut Commands, b: &mut ActiveBuilding, assets: &mut ObjectAssets, ctx: &mut AssetCtx) {
    if let Some(e) = b.roof_entity.take() {
        commands.entity(e).try_despawn();
    }
    let (w, d) = (b.data.width as usize, b.data.depth as usize);
    let mut buf = MeshBuf::default();
    let has_floor: HashSet<(u8, u16, u16)> = b.data.floors.iter().map(|f| (f.level, f.x, f.z)).collect();
    for level in 1..b.levels.len() as u8 {
        let rooms = rooms(&b.data, level);
        if rooms.iter().all(|r| *r == 0) {
            continue;
        }
        let roofed: HashSet<u16> = if b.had_house {
            // The rooms on either side of the household's walls.
            let mut s = HashSet::new();
            for &i in &b.built {
                let Some(wl) = b.data.walls.get(i as usize).filter(|wl| wl.level.max(1) == level) else { continue };
                let (a, c) = (Vec2::from(wl.a), Vec2::from(wl.b));
                if a.distance(c) < 1e-3 {
                    continue;
                }
                let n = (c - a).perp().normalize();
                for side in [n, -n] {
                    let p = (a + c) * 0.5 + side * 0.5;
                    let (x, z) = (p.x.floor() as i64, p.y.floor() as i64);
                    if x >= 0 && z >= 0 && (x as usize) < w && (z as usize) < d {
                        s.insert(rooms[z as usize * w + x as usize]);
                    }
                }
            }
            s.remove(&0);
            s
        } else {
            rooms.iter().copied().filter(|r| *r > 0).collect()
        };
        let mut cells: Vec<bool> = (0..w * d).map(|i| roofed.contains(&rooms[i]) && !has_floor.contains(&(level + 1, (i % w) as u16, (i / w) as u16))).collect();
        let y = b.levels[level as usize] + WALL_H;
        for (x0, z0, x1, z1) in rectangles(&mut cells, w, d) {
            hip_roof(&mut buf, b, (x0 as f32, z0 as f32, x1 as f32, z1 as f32), y);
        }
    }
    if buf.is_empty() {
        return;
    }
    let tex = assets.texture(ctx, b.roof_texture);
    let mat = ctx.materials.add(StandardMaterial {
        base_color: if tex.is_some() { Color::WHITE } else { Color::srgb(0.35, 0.33, 0.32) },
        base_color_texture: tex,
        perceptual_roughness: 0.9,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    b.roof_entity = Some(commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat), BuiltRoof, Visibility::Hidden, DespawnOnExit(AppState::InGame))).id());
}

/// The stair style most of the town's houses have (for staircases built on a lot whose house
/// has none of its own).
pub fn town_stair_style(world: &crate::loading::WorldInfo) -> Option<StairBaked> {
    let mut count: HashMap<Key, (usize, usize)> = HashMap::new();
    let mut lots: Vec<&usize> = world.buildings.keys().collect();
    lots.sort();
    let all: Vec<&StairBaked> = lots.into_iter().flat_map(|l| world.buildings[l].stairs.iter()).filter(|s| s.flight.is_some()).collect();
    for (i, s) in all.iter().enumerate() {
        count.entry(s.style).or_insert((0, i)).0 += 1;
    }
    let (style, _) = count.into_iter().max_by_key(|(k, (n, first))| (*n, std::cmp::Reverse(*first), std::cmp::Reverse(*k)))?;
    // (One with its railings, if any have them.)
    all.into_iter().filter(|s| s.style == style).max_by_key(|s| s.rails.iter().filter(|r| r.rail.is_some()).count()).cloned()
}

/// A staircase's pieces as the game puts them together, in `style`: a flight per tile in each
/// lane (each a tile's rise further up), a first step, the side panels, and the railings up the
/// sides given (-1 or 1 across the climb) a tile's sloped rail at a time with a post at the
/// foot. Each with where it goes (lot-local), its height and whether it's stretched to the
/// staircase's rise (the posts aren't).
struct StairPieces {
    pieces: Vec<(Key, Vec2, f32, bool)>,
    rotation: Quat,
    scale: Vec3,
}

#[allow(clippy::too_many_arguments)]
fn stair_pieces(style: &StairBaked, rails: &[(f32, &StairRail)], bottom: Vec2, d: Vec2, run: u16, width: u16, y0: f32, y1: f32, lot_rot: Quat) -> StairPieces {
    let across = Vec2::new(-d.y, d.x);
    let rise = (y1 - y0) / run.max(1) as f32;
    let up = |k: u16| (d * k as f32, y0 + rise * k as f32);
    let mut pieces = Vec::new();
    for lane in 0..width {
        let off = across * (lane as f32 + 0.5 - width as f32 * 0.5);
        pieces.extend((0..run).filter_map(|k| style.flight.map(|f| (f, bottom + up(k).0 + off, up(k).1, true))));
        pieces.extend(style.start.map(|p| (p, bottom + off, y0, true)));
    }
    for sgn in [-1.0f32, 1.0] {
        let off = across * (sgn * width as f32 * 0.5);
        pieces.extend((0..run).filter_map(|k| style.side.map(|p| (p, bottom + up(k).0 + off, up(k).1, true))));
    }
    for &(sgn, r) in rails {
        let off = across * (sgn * width as f32 * 0.5);
        pieces.extend((0..run).filter_map(|k| r.rail.map(|p| (p, bottom + up(k).0 + off, up(k).1, true))));
        pieces.extend(r.start.map(|p| (p, bottom + off, y0, true)));
        pieces.extend(r.post.map(|p| (p, bottom + off, y0, false)));
    }
    StairPieces { pieces, rotation: lot_rot * Quat::from_rotation_y((-d.y).atan2(d.x)), scale: Vec3::new(1.0, rise / 0.75, 1.0) }
}

/// Spawns the household's staircases and links the floors they join.
fn respawn_stairs(commands: &mut Commands, b: &mut ActiveBuilding, assets: &mut ObjectAssets, ctx: &mut AssetCtx) {
    for e in b.stair_entities.drain(..) {
        commands.entity(e).try_despawn();
    }
    let mat = surface_material(assets, ctx, STYLE_FLOOR_DECK);
    let mut links = b.base_stairs.clone();
    for s in b.built_stairs.clone() {
        let (bottom, d) = s.bottom();
        let run = STAIR_RUN as f32;
        let (Some(&y0), Some(&y1)) = (b.levels.get(s.level as usize), b.levels.get(s.level as usize + 1)) else { continue };
        // In the stair style's pieces (with its railing up both sides); steps made here with no
        // style to hand.
        let built = b.stair_style.as_ref().map(|style| {
            let rails: Vec<(f32, &StairRail)> = style.rails.first().map(|r| vec![(-1.0, r), (1.0, r)]).unwrap_or_default();
            stair_pieces(style, &rails, bottom, d, STAIR_RUN as u16, 1, y0, y1, b.rot)
        });
        let mut spawned = Vec::new();
        if let Some(p) = built {
            for (key, at, y, stretched) in p.pieces {
                let parts = assets.model(ctx, key);
                if !parts.is_empty() {
                    let scale = if stretched { p.scale } else { Vec3::ONE };
                    spawned.push(spawn_parts(commands, &parts, Transform { translation: b.world(at.x, at.y, y), rotation: p.rotation, scale }));
                }
            }
        }
        if spawned.is_empty() {
            spawned.push(commands.spawn((Mesh3d(ctx.meshes.add(stair_mesh(b, bottom, d, run, y0, y1))), MeshMaterial3d(mat.clone()))).id());
        }
        for e in spawned {
            commands.entity(e).insert((BuildingPiece { level: s.level }, DespawnOnExit(AppState::InGame)));
            b.stair_entities.push(e);
        }
        let bw = |q: Vec2| b.world(q.x, q.y, 0.0).xz();
        links.push(StairLink { level: s.level, upper: s.level + 1, bottom: bw(bottom - d * 0.45), top: bw(bottom + d * (run + 0.45)), y0, y1 });
    }
    b.stairs = links;
}

/// Spawns the detailed house of `lot` with all its furniture and returns its state. With a
/// `neighbor` root, it's built for show only (merged meshes, furniture without gameplay); for a
/// `visit` the walls also block walking and the furniture is left to the caller.
#[allow(clippy::too_many_arguments)]
pub fn spawn_building(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    b: &LotBuildingBaked,
    lot: &LotInfo,
    neighbor: Option<Entity>,
    visit: bool,
) -> ActiveBuilding {
    let corner = Vec3::from(lot.corner);
    let rot = Quat::from_rotation_y(lot.rotation);
    let top_level = b.levels.len().saturating_sub(2).max(1) as u8;
    let floor_cells = b.floors.iter().map(|f| ((f.level, f.x as i32, f.z as i32), f.mask)).collect();
    let floor_heights = b.floors.iter().filter_map(|f| Some(((f.level, f.x as i32, f.z as i32), f.y?))).collect();
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
        floor_heights,
        floor_kinds,
        far: None,
        data: b.clone(),
        exterior: STYLE_EXTERIOR[(lot.id % STYLE_EXTERIOR.len() as u64) as usize],
        cap_mat: Handle::default(),
        holes: Vec::new(),
        built: HashSet::new(),
        always_detailed: !b.is_house(),
        base_stairs: Vec::new(),
        built_stairs: Vec::new(),
        had_house: b.is_house(),
        roof_texture: ROOF_DEFAULT,
        stair_style: b.stairs.iter().find(|s| s.flight.is_some()).cloned(),
        away: None,
        wall_entities: HashMap::new(),
        floor_entities: Vec::new(),
        stair_entities: Vec::new(),
        fence_entities: Vec::new(),
        pool_entities: Vec::new(),
        roof_entity: None,
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
    active.cap_mat = cap_mat.clone();

    // Furniture, doors and windows.
    let mut holes: Vec<(Option<Entity>, Hole)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut stairs: Vec<(LotObjectBaked, Quat)> = Vec::new();
    for o in &b.objects {
        let q = Quat::from_xyzw(o.rotation[0], o.rotation[1], o.rotation[2], o.rotation[3]);
        let q = if q.length_squared() < 1e-6 { Quat::IDENTITY } else { q.normalize() };
        // Spawners (of fish, insects, rocks, seeds) are invisible markers in play.
        if o.script.contains("Spawner") {
            continue;
        }
        if o.script.contains("Stairs") {
            stairs.push((o.clone(), q));
            continue;
        }
        let opening = is_opening(&o.script);
        let entity = if visit {
            Entity::PLACEHOLDER
        } else if let Some(root) = neighbor {
            let parts = assets.object_design(ctx, o.objd, o.design);
            if parts.is_empty() {
                continue;
            }
            let e = spawn_parts(commands, &parts, Transform::from_translation(Vec3::from(o.position)).with_rotation(q));
            commands.entity(e).insert(ChildOf(root));
            e
        } else {
            let Some(spawned) = crate::home::spawn_game_object_design(commands, assets, ctx, catalog, o.objd, Vec3::from(o.position), q, o.design) else {
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
        if let (Some(door), Some(bounds)) = (opening, parts_bounds(&assets.object(ctx, o.objd))) {
            let hole = Hole::new(Vec2::from(o.local), active.local_dir(q * Vec3::Z), active.local_dir(q * Vec3::X), bounds, door, o.level);
            let wp = hole.wall_point;
            if neighbor.is_none() {
                commands.entity(entity).insert(WallObject { mid: active.world(wp.x, wp.y, o.position[1]) });
            }
            holes.push((neighbor.is_none().then_some(entity), hole));
        }
    }

    // Stairs, as the game builds them from their styles' pieces: a flight per tile, each a
    // tile's rise further up, a first step, and the side panels; and the floors they join
    // linked for walking. (Lots baked before staircases were: steps generated here.)
    let baked_stairs = b.stairs.iter().any(|s| s.flight.is_some());
    let mut links = Vec::new();
    for s in b.stairs.iter().filter(|s| s.flight.is_some()) {
        let (run, width) = s.run_width();
        let (y0, y1) = (level_y(s.bottom), level_y(s.top));
        if run == 0 || width == 0 || y1 - y0 < 0.1 {
            continue;
        }
        let d = Vec2::from(s.climb());
        let across = Vec2::new(-d.y, d.x);
        let middle = (Vec2::new(s.min[0] as f32, s.min[1] as f32) + Vec2::new(s.max[0] as f32, s.max[1] as f32)) * 0.5;
        let bottom = middle - d * (run as f32 * 0.5);
        // (Railings up the side they're on.)
        let rails: Vec<(f32, &StairRail)> = s.rails.iter().map(|r| (if (Vec2::from(r.at) - middle).dot(across) < 0.0 { -1.0 } else { 1.0 }, r)).collect();
        let p = stair_pieces(s, &rails, bottom, d, run, width, y0, y1, active.rot);
        for (key, at, y, stretched) in p.pieces {
            let parts = assets.model(ctx, key);
            if parts.is_empty() {
                continue;
            }
            let e = spawn_parts(commands, &parts, Transform { translation: active.world(at.x, at.y, y), rotation: p.rotation, scale: if stretched { p.scale } else { Vec3::ONE } });
            place(commands, e, neighbor, s.bottom);
        }
        if s.bottom >= 1 && neighbor.is_none() {
            let top = bottom + d * run as f32;
            links.push((s.bottom, s.top, bottom - d * 0.45, top + d * 0.45, y0, y1));
        }
    }
    for (level, upper, from, to, y0, y1) in links {
        let bw = |q: Vec2| active.world(q.x, q.y, 0.0).xz();
        active.stairs.push(StairLink { level, upper, bottom: bw(from), top: bw(to), y0, y1 });
    }
    let stair_mat = material(assets, ctx, STYLE_FLOOR_DECK);
    for (o, q) in stairs.iter().filter(|_| !baked_stairs) {
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
        let e = commands.spawn((Mesh3d(ctx.meshes.add(stair_mesh(&active, bottom, d, run, y0, y1))), MeshMaterial3d(stair_mat.clone()))).id();
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
    // (Indexed by height: full walls, and for a visit cut-down walls too.)
    let mut merged: [HashMap<Key, MeshBuf>; 2] = [HashMap::new(), HashMap::new()];
    let mut merged_caps = [MeshBuf::default(), MeshBuf::default()];
    let heights: &[(usize, f32)] = if visit { &[(0, WALL_H), (1, CUT_H)] } else { &[(0, WALL_H)] };
    for (wall_index, w) in b.walls.iter().enumerate() {
        let level = w.level.max(1);
        let y0 = w.y.unwrap_or(level_y(level));
        let (a, bb) = (Vec2::from(w.a), Vec2::from(w.b));
        let seg = bb - a;
        let len = seg.length();
        if len < 1e-3 {
            continue;
        }
        let along = seg / len;
        let n_local = Vec2::new(-along.y, along.x);
        let (seg_holes, has_door) = openings(&holes, w);
        let ext = along * (WALL_T * 0.5);
        let (pa, pb) = (a - ext, bb + ext);
        let to3 = |p: Vec2, y: f32| active.world(p.x, p.y, y0 + y);
        let n3 = active.dir(n_local.x, n_local.y);
        let half = n3 * (WALL_T * 0.5);
        let ulen = len + WALL_T;
        if neighbor.is_some() {
            for &(hi, top) in heights {
                for (side, kind, cover) in [(1.0f32, w.left, w.cover[0]), (-1.0, w.right, w.cover[1])] {
                    let buf = merged[hi].entry(cover_key(cover).unwrap_or_else(|| wall_style(kind, exterior))).or_default();
                    for (s0, s1) in wall_spans(&seg_holes, top) {
                        let off = half * side;
                        let p = [to3(pa, s0) + off, to3(pb, s0) + off, to3(pb, s1) + off, to3(pa, s1) + off];
                        let (v0, v1) = (1.0 - s0 / WALL_H, 1.0 - s1 / WALL_H);
                        let (u0, u1) = if side > 0.0 { (0.0, ulen) } else { (ulen, 0.0) };
                        buf.quad(p, [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], n3 * side);
                    }
                }
                for (_, s1) in wall_spans(&seg_holes, top) {
                    let p = [to3(pa, s1) + half, to3(pb, s1) + half, to3(pb, s1) - half, to3(pa, s1) - half];
                    merged_caps[hi].quad(p, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], Vec3::Y);
                }
            }
            if visit && !has_door && level == 1 {
                let dir = active.dir(along.x, along.y);
                let mid = to3((a + bb) * 0.5, 0.0);
                commands.spawn((
                    Transform::from_translation(mid).with_rotation(Quat::from_rotation_y((-dir.z).atan2(dir.x))),
                    Obstacle { half: Vec2::new(len * 0.5 + 0.02, 0.08), center_offset: Vec2::ZERO },
                    Floor(1),
                    ChildOf(neighbor.unwrap()),
                ));
            }
            continue;
        }
        let ents = spawn_wall(commands, assets, ctx, &active, wall_index, w, &seg_holes, has_door, exterior, &cap_mat, &mut material);
        active.wall_entities.insert(wall_index as u32, ents);
    }

    for (hi, (walls, caps)) in merged.into_iter().zip(merged_caps).enumerate() {
        let mut parts: Vec<Entity> = Vec::new();
        for (style, buf) in walls {
            let mat = material(assets, ctx, style);
            parts.push(commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat))).id());
        }
        if !caps.is_empty() {
            parts.push(commands.spawn((Mesh3d(ctx.meshes.add(caps.mesh())), MeshMaterial3d(cap_mat.clone()))).id());
        }
        for e in parts {
            place(commands, e, neighbor, 1);
            if visit {
                commands.entity(e).insert((VisitWalls { cut: hi == 1 }, if hi == 1 { Visibility::Hidden } else { Visibility::Inherited }));
            }
        }
    }

    // Floors, one mesh per level and covering.
    let floor_entities = spawn_floors(commands, assets, ctx, b, &active, neighbor);
    active.floor_entities = floor_entities;

    // Foundation sides from slightly below the ground up to the ground floor.
    if !b.foundation.is_empty() {
        let mut buf = MeshBuf::default();
        let (g, top) = (level_y(0) - 0.6, level_y(1));
        for (i, (a, bb)) in b.foundation.iter().enumerate() {
            let (a, bb) = (Vec2::from(*a), Vec2::from(*bb));
            // (Each up to the floor it holds up.)
            let top = b.foundation_top.get(i).copied().unwrap_or(top);
            let len = (bb - a).length();
            if len < 1e-3 {
                continue;
            }
            let n = (bb - a) / len;
            let n3 = active.dir(-n.y, n.x);
            // (Down to the ground where the lot slopes away under the house.)
            let ga = b.ground_at(a.x, a.y).map_or(g, |y| y.min(level_y(0)) - 0.6);
            let gb = b.ground_at(bb.x, bb.y).map_or(g, |y| y.min(level_y(0)) - 0.6);
            let p = [active.world(a.x, a.y, ga), active.world(bb.x, bb.y, gb), active.world(bb.x, bb.y, top), active.world(a.x, a.y, top)];
            let (va, vb) = ((top - ga) / WALL_H, (top - gb) / WALL_H);
            buf.quad(p, [[0.0, va], [len, vb], [len, 0.0], [0.0, 0.0]], n3);
        }
        if !buf.is_empty() {
            let mat = material(assets, ctx, STYLE_FOUNDATION);
            let e = commands.spawn((Mesh3d(ctx.meshes.add(buf.mesh())), MeshMaterial3d(mat))).id();
            place(commands, e, neighbor, 0);
        }
    }
    // Fences and railings.
    active.fence_entities = spawn_fences(commands, assets, ctx, b, &active, neighbor);

    // The pool.
    active.pool_entities = spawn_pool(commands, assets, ctx, b, &active, neighbor);
    active.holes = holes;
    active.base_stairs = active.stairs.clone();
    // Cutaway and imposter swaps work around the middle of the walls, not of the lot.
    let mids: Vec<Vec2> = b.walls.iter().map(|w| (Vec2::from(w.a) + Vec2::from(w.b)) * 0.5).collect();
    if !mids.is_empty() {
        let c = mids.iter().copied().sum::<Vec2>() / mids.len() as f32;
        active.center = active.world(c.x, c.y, corner.y);
    }
    if neighbor.is_none() {
        info!(
            "house: {} floors, {} objects ({} without a model); levels {:?}, {} foundation edges, ground grid {} ({}x{})",
            top_level,
            b.objects.len(),
            missing.len(),
            b.levels,
            b.foundation.len(),
            b.ground.len(),
            b.width,
            b.depth
        );
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
fn view_level_keys(keys: Res<ButtonInput<KeyCode>>, building: Option<ResMut<ActiveBuilding>>, buy: Option<Res<crate::buy::BuyMode>>) {
    let Some(mut b) = building else { return };
    // With a construction tool in hand, one floor above the top can be built on.
    let building_up = buy.is_some_and(|m| m.active && m.tool.is_some());
    let top = if building_up { (b.top_level + 1).min(6) } else { b.top_level };
    if keys.just_pressed(KeyCode::PageUp) && b.view_level < top {
        b.view_level += 1;
        while b.levels.len() <= b.view_level as usize {
            let h = b.levels[b.levels.len() - 1] + s3bake::building::LEVEL_HEIGHT;
            b.levels.push(h);
        }
    }
    if keys.just_pressed(KeyCode::PageDown) && b.view_level > 1 {
        b.view_level -= 1;
    }
}

/// Shows the detailed house near the camera (with upper floors hidden and the front walls cut
/// away) and the game's imposter from afar.
#[allow(clippy::type_complexity)]
fn building_visibility(
    (building, walls): (Option<ResMut<ActiveBuilding>>, Res<WallMode>),
    cams: Query<(&SimsCamera, &GlobalTransform)>,
    mut pieces: Query<(&BuildingPiece, Option<&WallObject>, &mut Visibility, Has<crate::traffic::CarOut>), (Without<LotImposter>, Without<crate::world::Tree>)>,
    mut imposters: Query<(&LotImposter, &mut Visibility), (Without<BuildingPiece>, Without<crate::world::Tree>)>,
    mut faces: Query<(&mut WallFace, &mut Mesh3d, &mut Visibility), (Without<BuildingPiece>, Without<LotImposter>, Without<crate::world::Tree>)>,
    mut trees: Query<(&GlobalTransform, &mut Visibility), (With<crate::world::Tree>, Without<BuildingPiece>, Without<LotImposter>)>,
    mut roofs: Query<
        &mut Visibility,
        (With<BuiltRoof>, Without<BuildingPiece>, Without<LotImposter>, Without<crate::world::Tree>, Without<WallFace>, Without<crate::sim::Sim>),
    >,
    mut sims: Query<
        (&Floor, &mut Visibility),
        (
            With<crate::sim::Sim>,
            Without<crate::interact::AtWork>,
            Without<crate::rabbitholes::AtRabbitHole>,
            Without<crate::interact::OffLot>,
            Without<crate::death::Dying>,
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
    let far = !b.always_detailed && (cam.distance > IMPOSTER_DISTANCE || cam.focus.distance(b.center) > IMPOSTER_DISTANCE * 1.4);
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
    for mut vis in &mut roofs {
        vis.set_if_neq(if exterior && !far { Visibility::Inherited } else { Visibility::Hidden });
    }
    let is_cut = |mid: Vec3, level: u8| !exterior && level == view_level && walls.cuts(mid, b.center, view);
    for (floor, mut vis) in &mut sims {
        vis.set_if_neq(if floor.0 <= view_level { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (piece, wall_obj, mut vis, car_out) in &mut pieces {
        // (The household's car is away while someone's out in it.)
        let show = !far && !car_out && piece.level <= view_level && !wall_obj.is_some_and(|w| is_cut(w.mid, piece.level));
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

/// A house of the town's put down on another lot of the same size (Edit Town's Place a House):
/// the same building, turned and raised to the new lot's place.
pub fn relocated(src: &LotBuildingBaked, from: &LotInfo, to: &LotInfo, to_index: usize) -> LotBuildingBaked {
    let dy = to.corner[1] - from.corner[1];
    let mut b = src.clone();
    b.lot = to_index as u32;
    for l in &mut b.levels {
        *l += dy;
    }
    for f in b.floors.iter_mut().chain(b.pool.iter_mut()) {
        if let Some(y) = &mut f.y {
            *y += dy;
        }
    }
    for w in &mut b.walls {
        if let Some(y) = &mut w.y {
            *y += dy;
        }
    }
    for t in b.foundation_top.iter_mut().chain(b.ground.iter_mut()) {
        *t += dy;
    }
    let (corner, rot) = (Vec3::from(to.corner), Quat::from_rotation_y(to.rotation));
    let turn = Quat::from_rotation_y(to.rotation - from.rotation);
    for o in &mut b.objects {
        let p = corner + rot * Vec3::new(o.local[0], 0.0, o.local[1]);
        o.position = [p.x, o.position[1] + dy, p.z];
        o.rotation = (turn * Quat::from_array(o.rotation)).normalize().to_array();
    }
    b
}

/// Edit Town's changes to the town (the town's story keeps them): houses put down on empty lots
/// stand there, and lots bulldozed stand empty. The world's data takes them (so everything
/// takes the lots as they are now, to move onto and build on), and the pictures and detailed
/// houses drawn for the lots as they were go.
pub fn bulldozed_lots(
    mut commands: Commands,
    story: Res<crate::story::TownStory>,
    mut world: ResMut<crate::loading::CurrentWorld>,
    mut nearby: ResMut<NearbyLots>,
    imposters: Query<(Entity, &crate::world::LotImposter)>,
    // (The world as this last left it, and each edited lot's state applied to it: a world loaded
    // afresh has none applied.)
    mut applied: Local<(usize, HashMap<u64, Option<u64>>, HashMap<u64, Option<String>>)>,
    (data, mut assets): (Res<crate::baked::Baked>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    if std::sync::Arc::as_ptr(&world.data) as usize != applied.0 {
        applied.1.clear();
        applied.2.clear();
    }
    // (Lots whose type was changed: renamed as what they are now.)
    let retyped: Vec<(u64, Option<String>)> = story.lot_types.iter().filter(|(l, k)| applied.2.get(*l) != Some(*k)).map(|(l, k)| (*l, k.clone())).collect();
    if !retyped.is_empty() {
        let mut w = (*world.data).clone();
        for (id, kind) in &retyped {
            if let Some(l) = w.lots.iter_mut().find(|l| l.id == *id) {
                crate::story::retype(l, kind.as_deref());
            }
            applied.2.insert(*id, kind.clone());
        }
        world.data = std::sync::Arc::new(w);
        applied.0 = std::sync::Arc::as_ptr(&world.data) as usize;
    }
    // Each lot edited: a copy of another lot's house on it, or else (bulldozed) nothing.
    let wanted: Vec<(u64, Option<u64>)> = story
        .placed
        .iter()
        .map(|(to, from)| (*to, Some(*from)))
        .chain(story.bulldozed.iter().filter(|l| !story.placed.contains_key(l)).map(|l| (*l, None)))
        .filter(|(l, w)| applied.1.get(l) != Some(w))
        .collect();
    if wanted.is_empty() {
        return;
    }
    let index = |id: u64| world.data.lots.iter().position(|l| l.id == id);
    // (Copies made from the town's houses as they stood before this round of changes.)
    let before = world.data.clone();
    let mut w = (*world.data).clone();
    let mut changed = Vec::new();
    let mut pictures = Vec::new();
    for (lot, from) in &wanted {
        let Some(to) = index(*lot) else { continue };
        match from.and_then(index) {
            Some(f) => {
                let Some(src) = before.buildings.get(&f).filter(|b| b.is_house()) else { continue };
                w.buildings.insert(to, relocated(src, &before.lots[f], &before.lots[to], to));
                pictures.push((to, f));
            }
            None => {
                w.buildings.remove(&to);
            }
        }
        changed.push(to);
        applied.1.insert(*lot, *from);
    }
    world.data = std::sync::Arc::new(w);
    applied.0 = std::sync::Arc::as_ptr(&world.data) as usize;
    // (The pictures and detailed houses drawn for the lots as they were go.)
    for (e, imp) in &imposters {
        if changed.contains(&imp.0) {
            commands.entity(e).despawn();
        }
    }
    for i in &changed {
        if let Some(e) = nearby.spawned.remove(i) {
            commands.entity(e).despawn();
        }
    }
    // (A house put down takes its picture with it: the roof the detailed house is drawn under,
    // and what's seen of it from afar.)
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    for (to, from) in pictures {
        let key = (s3pkg::types::MODL, 0x00B0_C507, before.lots[from].id);
        let parts = assets.model(&mut ctx, key);
        if parts.is_empty() {
            continue;
        }
        let l = &before.lots[to];
        let tf = Transform::from_translation(Vec3::from(l.corner)).with_rotation(Quat::from_rotation_y(l.rotation));
        let e = spawn_parts(&mut commands, &parts, tf);
        commands.entity(e).insert((crate::world::LotImposter(to), DespawnOnExit(crate::AppState::InGame)));
    }
}

const NEARBY_IN: f32 = 70.0;
const NEARBY_OUT: f32 = 95.0;
/// Below this camera distance the active house's roof is taken off.
const ROOF_DISTANCE: f32 = 42.0;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
/// A household played before's home as they left it: the lot's house (or, on a lot that had
/// none, nothing) with their building and painting done, the furniture they bought, and none
/// they sold. (`script` names a catalogue object's script, for doors and windows.)
pub fn dormant_building(world: &crate::loading::WorldInfo, lot: usize, game: &crate::save::SaveGame, script: impl Fn(Key) -> String) -> Option<LotBuildingBaked> {
    let l = world.lots.get(lot)?;
    let center = crate::home::lot_center(l);
    let _ = center;
    let mut b = world.buildings.get(&lot).filter(|b| b.is_house()).cloned().unwrap_or_else(|| deck_building(lot, l, &world.heightmap));
    for op in &game.paint {
        apply_paint(&mut b, op);
    }
    b.objects.retain(|o| !game.removed.iter().any(|r| r.objd == o.objd && Vec3::from(r.position).distance(Vec3::from(o.position)) < 0.1));
    // (Saved before the deck was raised: what was on it, lifted onto it.)
    let (deck_top, _) = deck_heights(l, &world.heightmap);
    let (dx0, dz0, dx1, dz1) = deck_tiles(l);
    let house = b.is_house();
    let lift = |p: [f32; 3]| -> [f32; 3] {
        if game.deck || house {
            return p;
        }
        let rot = Quat::from_rotation_y(l.rotation);
        let local = rot.inverse() * (Vec3::from(p) - Vec3::from(l.corner));
        let on = local.x >= dx0 as f32 && local.x <= dx1 as f32 && local.z >= dz0 as f32 && local.z <= dz1 as f32;
        if on { [p[0], p[1] + (deck_top - world.heightmap.sample(p[0], p[2])).max(0.0), p[2]] } else { p }
    };
    let (s, c) = l.rotation.sin_cos();
    // (On a lot the world left empty, the starter furniture they moved in with, less what they
    // sold.)
    if !world.buildings.get(&lot).is_some_and(|b| b.is_house()) {
        for (objd, p, q) in crate::home::starter_furniture(l, &world.heightmap) {
            if game.removed.iter().any(|r| r.objd == objd && Vec3::from(r.position).xz().distance(p.xz()) < 0.1) {
                continue;
            }
            let (dx, dz) = (p.x - l.corner[0], p.z - l.corner[2]);
            b.objects.push(s3bake::types::LotObjectBaked {
                objd,
                position: p.to_array(),
                rotation: q.to_array(),
                script: script(objd),
                level: 1,
                local: [dx * c - dz * s, dx * s + dz * c],
                design: None,
            });
        }
    }
    for o in &game.bought {
        let p = lift(o.position);
        let (dx, dz) = (p[0] - l.corner[0], p[2] - l.corner[2]);
        let level = b.levels.iter().enumerate().skip(1).filter(|(_, y)| p[1] >= **y - 0.3).map(|(i, _)| i as u8).last().unwrap_or(0);
        b.objects.push(s3bake::types::LotObjectBaked {
            objd: o.objd,
            position: p,
            rotation: o.rotation,
            script: script(o.objd),
            level,
            local: [dx * c - dz * s, dx * s + dz * c],
            design: o.design_texture(),
        });
    }
    Some(b)
}

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
    visited: Option<Res<crate::visit::VisitedLot>>,
    dormant: Res<crate::household::Dormant>,
) {
    let Ok(cam) = cams.single() else { return };
    let active_lot = active.as_ref().map(|a| a.lot);
    let visited_lot = visited.as_ref().map(|v| v.lot);
    let zoomed_in = cam.distance < 110.0;
    let lot_center = |i: usize| crate::home::lot_center(&world.data.lots[i]);
    // Drop lots that went out of range.
    let gone: Vec<usize> = nearby
        .spawned
        .keys()
        .copied()
        .filter(|&i| !zoomed_in || Some(i) == active_lot || Some(i) == visited_lot || lot_center(i).xz().distance(cam.focus.xz()) > NEARBY_OUT)
        .collect();
    for i in gone {
        if let Some(e) = nearby.spawned.remove(&i) {
            commands.entity(e).despawn();
        }
    }
    // Bring in the nearest lot not yet shown (one per frame).
    if zoomed_in {
        // (The households played before's homes too, as they left them: on an empty lot, what
        // they built.)
        let next = world
            .data
            .buildings
            .keys()
            .copied()
            .chain(dormant.0.iter().filter(|d| !d.homeless).map(|d| d.lot_index))
            .filter(|i| *i < world.data.lots.len() && Some(*i) != active_lot && Some(*i) != visited_lot && !nearby.spawned.contains_key(i))
            .map(|i| (i, lot_center(i).xz().distance(cam.focus.xz())))
            .filter(|(_, d)| *d < NEARBY_IN)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = next {
            let root = commands.spawn((Transform::IDENTITY, Visibility::default(), DespawnOnExit(AppState::InGame))).id();
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            let script = |k: Key| data.0.catalog.iter().find(|c| c.objd == k).map(|c| c.script.clone()).unwrap_or_default();
            let left = dormant.0.iter().find(|d| d.lot_index == i && !d.homeless).and_then(|d| dormant_building(&world.data, i, d, script));
            if let Some(b) = left.as_ref().or(world.data.buildings.get(&i)) {
                spawn_building(&mut commands, &mut assets, &mut ctx, &catalog, b, &world.data.lots[i], Some(root), false);
            }
            nearby.spawned.insert(i, root);
        }
    }
    // Imposter layers: detailed lots keep their roofs (the active house loses its roof when
    // the camera comes in close). The pre-lit ground picture is never drawn: the terrain's own
    // pre-composited colour already has the lots' paint, and the picture is far darker.
    let lot_of: HashMap<Entity, usize> = imposters.iter().map(|(e, l)| (e, l.0)).collect();
    let active_far = active.as_ref().is_none_or(|a| a.far != Some(false));
    for (layer, parent, mut vis) in &mut layers {
        let Some(&lot) = lot_of.get(&parent.parent()) else { continue };
        let detailed = nearby.spawned.contains_key(&lot) || (Some(lot) == active_lot && !active_far) || Some(lot) == visited_lot;
        let show = (!detailed && layer.0 != LAYER_GROUND)
            || (layer.0 == LAYER_ROOF && ((Some(lot) != active_lot && Some(lot) != visited_lot) || cam.distance > ROOF_DISTANCE));
        vis.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
    }
}
