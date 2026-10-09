//! Terrain paint, build mode's brush for the ground: any of the world's own paint layers (the
//! grass, earth, sand and flowers its terrain is painted with) brushed onto the household's
//! lot in three sizes, or the eraser taking it back to how the world had it. The paint is a
//! map over the whole world, a texel a metre, of which layer and how much (drawn over the
//! terrain by its shader); each stroke is kept in saves and laid again on loading.
//!
//! And the terrain tools: raise, lower, flatten (to the height where the brush went down) and
//! smooth the ground of the lot, clear of the house, the pool and everything standing on it
//! and short of the lot's edges. The heights sculpted are kept in saves.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use serde::{Deserialize, Serialize};

use crate::buy::BuyMode;
use crate::camera::SimsCamera;
use crate::hud::PointerOverUi;
use crate::{AppState, PlayMode};

pub struct TerrainPaintPlugin;

impl Plugin for TerrainPaintPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Strokes>()
            .init_resource::<Swatches>()
            .add_systems(OnEnter(AppState::Loading), |mut s: ResMut<Strokes>| *s = Strokes::default())
            .init_resource::<Sculpted>()
            .init_resource::<SculptAt>()
            .init_resource::<TerrainHistory>()
            .add_systems(OnEnter(AppState::Loading), |mut h: ResMut<TerrainHistory>| *h = TerrainHistory::default())
            .add_systems(OnEnter(AppState::Loading), |mut s: ResMut<Sculpted>| *s = Sculpted::default())
            .add_systems(Update, (paint_tool, flush, sculpt_tool, restore_heights).chain().after(crate::hud::pointer_over_ui).before(crate::buyhistory::update).run_if(in_state(PlayMode::Live)));
    }
}

/// The paint map: r the layer (index / 15), g how much.
#[derive(Resource)]
pub struct PaintMap {
    pub image: Handle<Image>,
    pub size: u32,
    pub world_size: f32,
}

/// A picture of each of the world's paint layers, for the palette.
#[derive(Resource, Default)]
pub struct Swatches(pub Vec<Handle<Image>>);

/// One dab of the brush: where, how wide, and which layer (or `ERASE`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub x: f32,
    pub z: f32,
    pub radius: f32,
    pub layer: u8,
}

/// A held brush is one history entry, regardless of how many dabs or height ticks it makes.
#[derive(Resource, Default)]
pub struct TerrainHistory {
    paint: Option<Vec<Stroke>>,
    heights: std::collections::HashMap<(u32, u32), (u16, Option<u16>)>,
}

#[derive(Clone)]
pub struct TerrainEdit {
    paint: Option<(Vec<Stroke>, Vec<Stroke>)>,
    heights: Vec<((u32, u32), u16, Option<u16>, u16)>,
}

impl TerrainHistory {
    pub fn take_edit(&mut self, strokes: &Strokes, sculpted: &Sculpted) -> Option<TerrainEdit> {
        let paint = self.paint.take().map(|before| (before, strokes.saved())).filter(|(before, after)| before != after);
        let heights = self.heights.drain().filter_map(|(at, (before, saved))| {
            let after = *sculpted.heights.get(&at)?;
            (before != after || saved != Some(after)).then_some((at, before, saved, after))
        }).collect::<Vec<_>>();
        (paint.is_some() || !heights.is_empty()).then_some(TerrainEdit { paint, heights })
    }
}

pub fn restore_history(
    In((edit, undo)): In<(TerrainEdit, bool)>, mut strokes: ResMut<Strokes>, mut sculpted: ResMut<Sculpted>,
    map: Option<Res<PaintMap>>, mut images: ResMut<Assets<Image>>, mut world: ResMut<crate::loading::CurrentWorld>,
    chunks: Query<(&crate::terrain::TerrainChunk, &Mesh3d)>, mut meshes: ResMut<Assets<Mesh>>,
    holes: Option<Res<crate::terrain::TerrainHoles>>, mut grid: Option<ResMut<crate::nav::NavGrid>>,
) {
    if let Some((before, after)) = edit.paint {
        let selected = if undo { before } else { after };
        strokes.restore(&selected);
        strokes.last = None;
        if let Some(map) = map && let Some(mut image) = images.get_mut(&map.image) && let Some(pixels) = image.data.as_mut() {
            pixels.fill(0);
            for stroke in &selected { apply(pixels, map.size, map.world_size, stroke); }
            strokes.all = selected;
            strokes.pending.clear();
        }
    }
    if edit.heights.is_empty() { return; }
    let hm = &mut std::sync::Arc::make_mut(&mut world.data).heightmap;
    let mut cells = Vec::new();
    for ((x, z), before, saved, after) in edit.heights {
        hm.data[z as usize * hm.width + x as usize] = if undo { before } else { after };
        if undo && saved.is_none() { sculpted.heights.remove(&(x, z)); }
        else { sculpted.heights.insert((x, z), if undo { saved.unwrap() } else { after }); }
        let (x, z) = (x as i64, z as i64);
        cells.extend([(x - 1, z - 1), (x, z - 1), (x - 1, z), (x, z)]);
    }
    sculpted.flatten_to = None;
    let empty = std::collections::HashSet::new();
    crate::terrain::rebuild_chunks(&cells, hm, &chunks, &mut meshes, holes.as_ref().map_or(&empty, |h| &h.0));
    if let Some(grid) = grid.as_mut() { grid.dirty = true; }
}

/// The eraser.
pub const ERASE: u8 = 255;

/// The brush sizes (radius in metres).
pub const BRUSHES: [(f32, &str); 3] = [(1.0, "Small"), (2.0, "Medium"), (3.5, "Large")];

/// Every stroke on the household's lot, and those not yet on the map.
#[derive(Resource, Default)]
pub struct Strokes {
    pub all: Vec<Stroke>,
    pub pending: Vec<Stroke>,
    /// Where the brush last dabbed (while held).
    last: Option<Vec2>,
    flushed: f32,
}

impl Strokes {
    /// For a save: everything painted.
    pub fn saved(&self) -> Vec<Stroke> {
        self.all.iter().chain(&self.pending).copied().collect()
    }

    /// A save's paint, laid again.
    pub fn restore(&mut self, strokes: &[Stroke]) {
        self.all.clear();
        self.pending = strokes.to_vec();
    }
}

pub fn blank(size: u32) -> Image {
    // (Kept on the CPU too, to paint into.)
    Image::new(Extent3d { width: size.max(1), height: size.max(1), depth_or_array_layers: 1 }, TextureDimension::D2, vec![0; (size.max(1) * size.max(1) * 2) as usize], TextureFormat::Rg8Unorm, RenderAssetUsages::default())
}

pub fn swatch_image(rgba: Vec<u8>) -> Image {
    let s = crate::terrain::SWATCH;
    Image::new(Extent3d { width: s, height: s, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// A stroke laid onto the map's texels: soft at the edge, firm in the middle.
pub fn apply(data: &mut [u8], size: u32, world_size: f32, s: &Stroke) {
    let n = size as i64;
    let cell = world_size / size as f32;
    let r = s.radius.max(cell);
    let range = |c: f32| (((c - r) / cell).floor().max(0.0) as i64, (((c + r) / cell).ceil() as i64).min(n - 1));
    let ((x0, x1), (z0, z1)) = (range(s.x), range(s.z));
    let code = (s.layer.min(15) as f32 / 15.0 * 255.0).round() as u8;
    for tz in z0..=z1 {
        for tx in x0..=x1 {
            let d = Vec2::new((tx as f32 + 0.5) * cell - s.x, (tz as f32 + 0.5) * cell - s.z).length();
            if d >= r {
                continue;
            }
            let a = ((1.0 - (d / r).powi(2)) * 1.6).clamp(0.0, 1.0);
            let i = ((tz * n + tx) * 2) as usize;
            let Some(px) = data.get_mut(i..i + 2) else { continue };
            let old = px[1] as f32 / 255.0;
            let g = if s.layer == ERASE {
                old * (1.0 - a)
            } else if px[0] == code || old < 0.01 {
                px[0] = code;
                old + (1.0 - old) * a
            } else if a >= old {
                // (Over another paint: the new one takes the texel where it's the stronger.)
                px[0] = code;
                a
            } else {
                old
            };
            px[1] = (g * 255.0).round() as u8;
        }
    }
}

/// The brush: held down over the household's lot, it dabs the chosen paint as it goes.
#[allow(clippy::too_many_arguments)]
fn paint_tool(
    buy: Res<BuyMode>,
    (mouse, over_ui, menu, modal): (Res<ButtonInput<MouseButton>>, Res<PointerOverUi>, Res<crate::options::GameMenu>, Query<(), With<crate::dialog::Modal>>),
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    building: Option<Res<crate::building::ActiveBuilding>>,
    world: Res<crate::loading::CurrentWorld>,
    mut strokes: ResMut<Strokes>,
    mut gizmos: Gizmos,
    mut play: MessageWriter<crate::sound::PlaySound>,
    mut history: ResMut<TerrainHistory>,
) {
    let painting = buy.active && buy.tool == Some(crate::build::BuildTool::Terrain) && !menu.is_open() && modal.is_empty();
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    let (true, Some(b), Some(cursor), Ok((camera, cam_tf))) = (painting, building, cursor, cams.single()) else {
        strokes.last = None;
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    let Some(p) = crate::hud::ground_hit(ray, &world) else { return };
    let r = if buy.brush > 0.0 { buy.brush } else { BRUSHES[1].0 };
    // Only the household's own lot.
    let l = b.local(p);
    let on_lot = l.x >= 0.0 && l.y >= 0.0 && l.x <= b.data.width as f32 && l.y <= b.data.depth as f32;
    let colour = if !on_lot {
        Color::srgb(0.9, 0.3, 0.25)
    } else if buy.terrain == ERASE {
        Color::srgb(1.0, 1.0, 1.0)
    } else {
        crate::menu::PLUMBOB_GREEN
    };
    gizmos.circle(Isometry3d::new(p + Vec3::Y * 0.06, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), r, colour);
    if !mouse.pressed(MouseButton::Left) || over_ui.0 || !on_lot {
        strokes.last = None;
        return;
    }
    let at = p.xz();
    if strokes.last.is_some_and(|q| q.distance(at) < r * 0.35) {
        return;
    }
    if strokes.last.is_none() {
        play.write(crate::sound::PlaySound::ui("ui_build_flooring_section"));
    }
    strokes.last = Some(at);
    let layer = buy.terrain;
    history.paint.get_or_insert_with(|| strokes.saved());
    strokes.pending.push(Stroke { x: at.x, z: at.y, radius: r, layer });
}

/// Strokes go onto the map a few times a second (each change sends the whole map to the GPU).
fn flush(mut strokes: ResMut<Strokes>, map: Option<Res<PaintMap>>, mut images: ResMut<Assets<Image>>, time: Res<Time>) {
    let Some(map) = map else { return };
    let now = time.elapsed_secs();
    if strokes.pending.is_empty() || now - strokes.flushed < 0.12 {
        return;
    }
    strokes.flushed = now;
    let Some(mut img) = images.get_mut(&map.image) else { return };
    let Some(data) = img.data.as_mut() else { return };
    let pending = std::mem::take(&mut strokes.pending);
    for s in &pending {
        apply(data, map.size, map.world_size, s);
    }
    strokes.all.extend(pending);
}

/// The terrain tools.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sculpt {
    Raise,
    Lower,
    Flatten,
    Smooth,
}

impl Sculpt {
    pub const ALL: [Sculpt; 4] = [Sculpt::Raise, Sculpt::Lower, Sculpt::Flatten, Sculpt::Smooth];
    pub fn label(self) -> &'static str {
        match self {
            Sculpt::Raise => "Raise",
            Sculpt::Lower => "Lower",
            Sculpt::Flatten => "Flatten",
            Sculpt::Smooth => "Smooth",
        }
    }
}

/// A heightmap point's sculpted height (as the heightmap stores it).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SavedHeight {
    pub x: u32,
    pub z: u32,
    pub h: u16,
}

/// The heights sculpted on the lot (kept in saves), and the brush's state.
#[derive(Resource, Default)]
pub struct Sculpted {
    pub heights: std::collections::HashMap<(u32, u32), u16>,
    /// Heights from a save, to put back once the terrain is up.
    pub pending: Vec<SavedHeight>,
    flatten_to: Option<f32>,
    last: f32,
}

impl Sculpted {
    pub fn saved(&self) -> Vec<SavedHeight> {
        let mut v: Vec<SavedHeight> = self.heights.iter().map(|(&(x, z), &h)| SavedHeight { x, z, h }).collect();
        v.extend(&self.pending);
        v.sort_by_key(|s| (s.z, s.x));
        v
    }
}

/// (Tests: the brush held down at this point for so many more steps, mouse or no mouse.)
#[derive(Resource, Default)]
pub struct SculptAt(pub Option<(Vec3, u32)>);

/// How far the ground may go below and above the lot's own (metres).
const SCULPT_RANGE: (f32, f32) = (-3.0, 4.0);

/// The terrain tools: held down over the lot, they work the ground under the brush a little at
/// a time.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sculpt_tool(
    buy: Res<BuyMode>,
    (mouse, over_ui, time, menu, modal): (Res<ButtonInput<MouseButton>>, Res<PointerOverUi>, Res<Time>, Res<crate::options::GameMenu>, Query<(), With<crate::dialog::Modal>>),
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut world: ResMut<crate::loading::CurrentWorld>,
    mut sculpted: ResMut<Sculpted>,
    (chunks, mut meshes, holes, mut grid): (
        Query<(&crate::terrain::TerrainChunk, &Mesh3d)>,
        ResMut<Assets<Mesh>>,
        Option<Res<crate::terrain::TerrainHoles>>,
        Option<ResMut<crate::nav::NavGrid>>,
    ),
    objects: Query<(&crate::interact::GameObject, &Transform), Without<crate::visit::LotObject>>,
    mut gizmos: Gizmos,
    mut test: ResMut<SculptAt>,
    mut history: ResMut<TerrainHistory>,
) {
    if menu.is_open() || !modal.is_empty() {
        sculpted.flatten_to = None;
        return;
    }
    let held_at = test.0.as_mut().filter(|t| t.1 > 0).map(|t| {
        t.1 -= 1;
        t.0
    });
    let sculpting = held_at.is_some() || buy.active && buy.tool == Some(crate::build::BuildTool::Sculpt);
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    let (true, Some(b), Ok((camera, cam_tf))) = (sculpting, building, cams.single()) else {
        sculpted.flatten_to = None;
        return;
    };
    let p = match (held_at, cursor) {
        (Some(p), _) => p,
        (None, Some(cursor)) => {
            let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
            let Some(p) = crate::hud::ground_hit(ray, &world) else { return };
            p
        }
        _ => return,
    };
    let r = if buy.brush > 0.0 { buy.brush } else { BRUSHES[1].0 } + 0.5;
    let l = b.local(p);
    let on_lot = l.x >= 1.0 && l.y >= 1.0 && l.x <= b.data.width as f32 - 1.0 && l.y <= b.data.depth as f32 - 1.0;
    gizmos.circle(
        Isometry3d::new(p + Vec3::Y * 0.06, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        r,
        if on_lot { Color::srgb(0.45, 0.75, 1.0) } else { Color::srgb(0.9, 0.3, 0.25) },
    );
    if held_at.is_none() && (!mouse.pressed(MouseButton::Left) || over_ui.0) || !on_lot {
        sculpted.flatten_to = None;
        return;
    }
    let now = time.elapsed_secs();
    if held_at.is_none() && now - sculpted.last < 0.08 {
        return;
    }
    sculpted.last = now;
    let tool = Sculpt::ALL[(buy.sculpt as usize).min(3)];
    let target = *sculpted.flatten_to.get_or_insert(world.data.heightmap.sample(p.x, p.z));
    // What the ground under the house, the pool and the things on the lot must keep.
    let built: std::collections::HashSet<(i32, i32)> = b.data.floors.iter().chain(&b.data.pool).map(|f| (f.x as i32, f.z as i32)).collect();
    let things: Vec<(Vec2, f32)> = objects.iter().map(|(o, tf)| (o.world_center(tf).xz(), o.half.max_element() + 0.6)).filter(|(c, _)| c.distance(p.xz()) < r + 6.0).collect();
    let ground = b.data.levels[0];
    let (w, d) = (b.data.width as f32, b.data.depth as f32);
    let keep = |x: f32, z: f32| {
        let l = b.local(Vec3::new(x, 0.0, z));
        if l.x < 1.0 || l.y < 1.0 || l.x > w - 1.0 || l.y > d - 1.0 {
            return true;
        }
        let near_built = (-1..=1).any(|dz| (-1..=1).any(|dx| built.contains(&((l.x + dx as f32 * 0.8).floor() as i32, (l.y + dz as f32 * 0.8).floor() as i32))));
        near_built || things.iter().any(|(c, rad)| c.distance(Vec2::new(x, z)) < *rad)
    };
    let data = std::sync::Arc::make_mut(&mut world.data);
    let hm = &mut data.heightmap;
    let (x0, x1) = ((p.x - r).floor().max(0.0) as i64, ((p.x + r).ceil() as i64).min(hm.width as i64 - 1));
    let (z0, z1) = ((p.z - r).floor().max(0.0) as i64, ((p.z + r).ceil() as i64).min(hm.height as i64 - 1));
    let mut changes = Vec::new();
    for z in z0..=z1 {
        for x in x0..=x1 {
            let dist = Vec2::new(x as f32 - p.x, z as f32 - p.z).length();
            if dist >= r || keep(x as f32, z as f32) {
                continue;
            }
            let f = 1.0 - (dist / r).powi(2);
            let h = hm.at(x, z);
            let nh = match tool {
                Sculpt::Raise => h + 0.06 * f,
                Sculpt::Lower => h - 0.06 * f,
                Sculpt::Flatten => h + (target - h) * 0.4 * f,
                Sculpt::Smooth => {
                    let avg = (hm.at(x - 1, z) + hm.at(x + 1, z) + hm.at(x, z - 1) + hm.at(x, z + 1)) * 0.25;
                    h + (avg - h) * 0.5 * f
                }
            }
            .clamp(ground + SCULPT_RANGE.0, ground + SCULPT_RANGE.1);
            changes.push((x, z, (nh / hm.scale).round().clamp(0.0, 65535.0) as u16));
        }
    }
    let mut cells = Vec::new();
    for (x, z, v) in changes {
        if hm.data[z as usize * hm.width + x as usize] == v { continue; }
        if buy.active {
            history.heights.entry((x as u32, z as u32)).or_insert_with(||
                (hm.data[z as usize * hm.width + x as usize], sculpted.heights.get(&(x as u32, z as u32)).copied()));
        }
        hm.data[z as usize * hm.width + x as usize] = v;
        sculpted.heights.insert((x as u32, z as u32), v);
        cells.extend([(x - 1, z - 1), (x, z - 1), (x - 1, z), (x, z)]);
    }
    if cells.is_empty() {
        return;
    }
    let empty = std::collections::HashSet::new();
    crate::terrain::rebuild_chunks(&cells, &data.heightmap, &chunks, &mut meshes, holes.as_ref().map_or(&empty, |h| &h.0));
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
}

/// A save's sculpted heights, put back once the terrain is up.
fn restore_heights(
    mut sculpted: ResMut<Sculpted>,
    mut world: ResMut<crate::loading::CurrentWorld>,
    (chunks, mut meshes, holes, mut grid): (
        Query<(&crate::terrain::TerrainChunk, &Mesh3d)>,
        ResMut<Assets<Mesh>>,
        Option<Res<crate::terrain::TerrainHoles>>,
        Option<ResMut<crate::nav::NavGrid>>,
    ),
) {
    if sculpted.pending.is_empty() || chunks.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut sculpted.pending);
    let data = std::sync::Arc::make_mut(&mut world.data);
    let hm = &mut data.heightmap;
    let mut cells = Vec::new();
    for s in pending {
        if (s.x as usize) < hm.width && (s.z as usize) < hm.height {
            hm.data[s.z as usize * hm.width + s.x as usize] = s.h;
            sculpted.heights.insert((s.x, s.z), s.h);
            let (x, z) = (s.x as i64, s.z as i64);
            cells.extend([(x - 1, z - 1), (x, z - 1), (x - 1, z), (x, z)]);
        }
    }
    let empty = std::collections::HashSet::new();
    crate::terrain::rebuild_chunks(&cells, &data.heightmap, &chunks, &mut meshes, holes.as_ref().map_or(&empty, |h| &h.0));
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_gesture_groups_dabs_and_retains_original_height_and_save_state() {
        let original = Stroke { x: 4.0, z: 5.0, radius: 2.0, layer: 1 };
        let dab = Stroke { x: 6.0, ..original };
        let strokes = Strokes { all: vec![original, dab], pending: vec![Stroke { x: 7.0, ..dab }], ..default() };
        let mut sculpted = Sculpted::default();
        sculpted.heights.insert((4, 5), 140);
        let mut history = TerrainHistory { paint: Some(vec![original]), ..default() };
        history.heights.insert((4, 5), (100, None));
        history.heights.entry((4, 5)).or_insert((120, Some(120)));
        let edit = history.take_edit(&strokes, &sculpted).unwrap();
        assert_eq!(edit.paint.unwrap(), (vec![original], strokes.saved()));
        assert_eq!(edit.heights, vec![((4, 5), 100, None, 140)]);
        assert!(history.take_edit(&strokes, &sculpted).is_none());
    }

    #[test]
    fn unchanged_terrain_gesture_does_not_create_history() {
        let mut history = TerrainHistory { paint: Some(Vec::new()), ..default() };
        history.heights.insert((2, 3), (100, Some(100)));
        let mut sculpted = Sculpted::default();
        sculpted.heights.insert((2, 3), 100);
        assert!(history.take_edit(&Strokes::default(), &sculpted).is_none());
    }

    #[test]
    fn returning_to_the_original_height_still_restores_its_unsaved_state() {
        let mut history = TerrainHistory::default();
        history.heights.insert((2, 3), (100, None));
        let mut sculpted = Sculpted::default();
        sculpted.heights.insert((2, 3), 100);
        let edit = history.take_edit(&Strokes::default(), &sculpted).unwrap();
        assert_eq!(edit.heights, vec![((2, 3), 100, None, 100)]);
    }

    #[test]
    fn paint_and_erase() {
        let size = 64;
        let mut data = vec![0u8; size * size * 2];
        apply(&mut data, size as u32, 64.0, &Stroke { x: 32.0, z: 32.0, radius: 3.0, layer: 5 });
        let at = |x: usize, z: usize| (data[(z * size + x) * 2], data[(z * size + x) * 2 + 1]);
        assert_eq!(at(32, 32).0, 85);
        assert!(at(32, 32).1 > 200);
        assert_eq!(at(40, 32).1, 0);
        apply(&mut data, size as u32, 64.0, &Stroke { x: 32.0, z: 32.0, radius: 3.0, layer: ERASE });
        assert!(data[(32 * size + 32) * 2 + 1] < 30);
    }
}
