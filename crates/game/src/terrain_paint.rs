//! Terrain paint, build mode's brush for the ground: any of the world's own paint layers (the
//! grass, earth, sand and flowers its terrain is painted with) brushed onto the household's
//! lot in three sizes, or the eraser taking it back to how the world had it. The paint is a
//! map over the whole world, a texel a metre, of which layer and how much (drawn over the
//! terrain by its shader); each stroke is kept in saves and laid again on loading.

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
            .add_systems(Update, (paint_tool, flush).chain().run_if(in_state(PlayMode::Live)));
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
    (mouse, over_ui): (Res<ButtonInput<MouseButton>>, Res<PointerOverUi>),
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    building: Option<Res<crate::building::ActiveBuilding>>,
    world: Res<crate::loading::CurrentWorld>,
    mut strokes: ResMut<Strokes>,
    mut gizmos: Gizmos,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let painting = buy.active && buy.tool == Some(crate::build::BuildTool::Terrain);
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

#[cfg(test)]
mod tests {
    use super::*;

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
