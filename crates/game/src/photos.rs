//! Photographs, as the game's: a Sim of child age and up takes out their phone and "Take Photo"
//! (`PhoneCell.TakePhoto`): the view goes to their eyes (`ActivatePhotographMode`, the Sim and
//! their plumbob hidden, the interface away), framed by the game's own screen grab
//! (`ScreenGrab`'s "Capture Image": the phone's frame, the world outside it dimmed, "Press C to
//! capture, and Z and X to zoom in and out."), turned about with the camera's keys or a drag
//! and zoomed in and out; C takes the picture (its click, `ui_camera_photo`), Escape puts the
//! phone away. The picture is what's inside the frame, kept beside the saves, and becomes a
//! photograph in their inventory ("Untitled", by them, worth nothing: a phone's camera earns
//! no Photography), its own picture on its tile, to give a title ("Name...", the game's
//! `NameImageObject`), sell, or hang on a wall in the game's little photo frame
//! (`PhotographFrameSmall` in its wall state), the picture in it.
//!
//! The same screen grab gives a painter at an easel their still life (`PaintStillLife`, Painting
//! five and up): "Capture Subject" in the game's gilt frames, small, medium or large (`I`), the
//! camera kept on them; the subject captured is painted in the style chosen (plain, or a genius's,
//! the insane's or the somber's as skill and traits allow) on the canvas its frame was for, and
//! shows on the canvas as they paint, in their inventory and hung on a wall.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::ui::FocusPolicy;
use s3bake::Key;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue, Phase};
use crate::inventory::{ItemKind, Stack};
use crate::sim::{Age, Selected, Sim};

pub struct PhotosPlugin;

impl Plugin for PhotosPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Grabbed>()
            .add_systems(Update, (sittings, start_grab, grab, aim, photo_taken, titled, photos_gone, frames, photo_test, photo_menu_test).chain().after(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words.
pub const TAKE_PHOTO: &str = "Take Photo";
pub const NAME: &str = "Name...";
pub const UNTITLED: &str = "Untitled";
const NAME_TITLE: &str = "Name Photograph";
/// A photograph's title at most.
pub const TITLE_LENGTH: usize = 40;

/// The screen grab's layout, and its exports (`ScreenGrabType`) and windows: each frame's
/// window (`0x100` on), its target (`1`), and the dimming outside it (its `FrameDrawable`'s
/// shade).
const LAYOUT: &str = "ScreenGrab";
const FRAME_BASE: u32 = 0x100;
const TARGET: u32 = 1;
const PHONE_SHADE: u32 = 0x9300_0000;
const PAINTING_SHADE: u32 = 0xBF00_0000;
/// How far a camera zooms (`PhotographZoom`: none, minimum, medium, maximum), as the narrowest
/// field of view it comes to (degrees; the camera's own is `camera::FOV`).
const ZOOMS: [f32; 4] = [crate::camera::FOV, 35.0, 22.0, 12.0];
/// A phone's zoom (`PhoneCell`'s `MaximumZoom`: minimum).
const PHONE_ZOOM: usize = 1;

/// The skill, and its tables (the `Photography` XML's `SkillProgression`, `Sizes`, `Styles`, and
/// its tuning): what each level unlocks, how likely a bad photo is and what photos are worth at
/// it; what each size and style costs a shot and does to the worth; and the base cost, worth and
/// skill a photo's taken with.
pub const SKILL: &str = "Photography";
const STYLE_UNLOCKS: [(Style, u32); 4] = [(Style::Plain, 0), (Style::Gray, 2), (Style::Sepia, 4), (Style::Vignette, 8)];
const SIZE_UNLOCKS: [(Size, u32); 4] = [(Size::Small, 0), (Size::Medium, 2), (Size::Large, 5), (Size::Panorama, 10)];
const ZOOM_UPGRADE_LEVEL: u32 = 6;
const BAD_PHOTO: [f32; 11] = [0.75, 0.33, 0.17, 0.1, 0.05, 0.05, 0.03, 0.01, 0.01, 0.01, 0.0];
const LEVEL_VALUE: [f32; 11] = [0.3, 0.3, 0.4, 0.5, 0.8, 1.0, 1.1, 1.25, 1.45, 1.5, 2.0];
/// Each size's shot cost and worth (Tiny, Small, Medium, Large, Panorama).
const SIZE_COST: [i64; 5] = [0, 6, 13, 20, 35];
const SIZE_VALUE: [f32; 5] = [0.2, 0.6, 1.0, 1.5, 2.3];
/// Each style's (Plain, Sepia, Gray, Vignette).
const STYLE_COST: [i64; 4] = [0, 5, 5, 7];
const STYLE_VALUE: [f32; 4] = [1.0, 1.25, 1.1, 1.4];
const BASE_COST: i64 = 10;
const BASE_VALUE: f32 = 5.0;
const BASE_POINTS: f32 = 1.0;
const BAD_VALUE: f32 = 0.1;
const EYE_VALUE: f32 = 1.25;

/// A camera (`CameraCheap`, `CameraMedium`, `CameraExpensive`'s `kCameraTuning`, the XML's): what a
/// shot costs more, the largest size it takes, how far it zooms (and with the zoom upgrade), what
/// its photos are worth more, and its shutter's sound.
#[derive(Debug, PartialEq)]
pub struct CameraModel {
    pub name: &'static str,
    cost: i64,
    max: Size,
    zoom: (usize, usize),
    value: f32,
    click: &'static str,
}

pub const CAMERAS: [CameraModel; 3] = [
    CameraModel { name: "CameraCheap", cost: 1, max: Size::Small, zoom: (0, 1), value: 0.7, click: "ui_photo_take_chp" },
    CameraModel { name: "CameraMedium", cost: 6, max: Size::Medium, zoom: (1, 2), value: 1.1, click: "ui_photo_take_med" },
    CameraModel { name: "CameraExpensive", cost: 12, max: Size::Panorama, zoom: (2, 3), value: 1.4, click: "ui_photo_take_exp" },
];

/// A photographer's cameras' settings (`DefaultSize`, `DefaultStyle`), by camera.
#[derive(Component, Default, Clone, Debug)]
pub struct CameraSettings(pub HashMap<&'static str, (Size, Style)>);

impl CameraSettings {
    /// As kept in saves: each camera's name, size and mode.
    pub fn to_saved(&self) -> Vec<(String, u8, u8)> {
        let mut v: Vec<(String, u8, u8)> = self.0.iter().map(|(c, (z, y))| (c.to_string(), *z as u8, *y as u8)).collect();
        v.sort();
        v
    }

    pub fn from_saved(saved: &[(String, u8, u8)]) -> Self {
        CameraSettings(saved.iter().filter_map(|(c, z, y)| Some((camera(c)?.name, (*Size::ALL.get(*z as usize)?, *Style::ALL.get(*y as usize)?)))).collect())
    }
}

/// A camera by its catalogue name.
pub fn camera(name: &str) -> Option<&'static CameraModel> {
    CAMERAS.iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

/// The styles and sizes a photographer has (`StyleUnlocked`, `SizeUnlocked`).
pub fn styles_at(level: u32) -> impl Iterator<Item = Style> {
    STYLE_UNLOCKS.into_iter().filter(move |(_, l)| level >= *l).map(|(s, _)| s)
}

pub fn sizes_at(level: u32, camera: &CameraModel) -> impl Iterator<Item = Size> + '_ {
    SIZE_UNLOCKS.into_iter().filter(move |(s, l)| level >= *l && *s as u8 <= camera.max as u8).map(|(s, _)| s)
}

/// What a shot costs (`GetCostToTakePhoto`).
pub fn shot_cost(camera: &CameraModel, size: Size, style: Style) -> i64 {
    BASE_COST + SIZE_COST[size as usize] + STYLE_COST[style as usize] + camera.cost
}

/// A photo's worth (`OnPhotoTakenPreInstantiate`): its base and subject's worth (and its
/// collection's multiplier), by the camera, the photographer's level, its size and style, a bad
/// photo's tenth, a Photographer's Eye's.
pub fn photo_value(camera: &CameraModel, level: u32, size: Size, style: Style, (subject, collection): (f32, f32), bad: bool, eye: bool) -> i64 {
    let mut m = camera.value * collection * LEVEL_VALUE[level.min(10) as usize] * SIZE_VALUE[size as usize] * STYLE_VALUE[style as usize];
    if bad {
        m *= BAD_VALUE;
    }
    if eye {
        m *= EYE_VALUE;
    }
    ((BASE_VALUE + subject) * m) as i64
}

/// How a bad photo's spoiled (`FlipCoinForPhotoFailure`, the styles' `UseForFailureChance`):
/// a gnome in it, a finger over the lens, blurred, or noisy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Spoilt {
    Gnome,
    Finger,
    Blurry,
    Noise,
}

impl Spoilt {
    const WEIGHTS: [(Spoilt, f32); 4] = [(Spoilt::Gnome, 1.0), (Spoilt::Finger, 6.0), (Spoilt::Blurry, 6.0), (Spoilt::Noise, 3.0)];

    /// Whether a photographer's shot's spoilt, and how (`roll`, `pick`: 0 to 1).
    pub fn flip(level: u32, roll: f32, pick: f32) -> Option<Spoilt> {
        if roll >= BAD_PHOTO[level.min(10) as usize] {
            return None;
        }
        let total: f32 = Self::WEIGHTS.iter().map(|w| w.1).sum();
        let mut at = pick * total;
        Self::WEIGHTS.iter().find(|(_, w)| {
            at -= w;
            at < 0.0
        }).map(|w| w.0).or(Some(Spoilt::Noise))
    }

    /// Spoils a picture (RGBA, `w` × `h`) so.
    pub fn spoil(self, px: &mut [u8], w: u32, h: u32, seed: u64) {
        let (wi, hi) = (w as i32, h as i32);
        match self {
            Spoilt::Blurry => {
                // (Smeared: a wide box blur, twice over.)
                let r = (wi.min(hi) / 25).max(2);
                for _ in 0..2 {
                    let src = px.to_vec();
                    for y in 0..hi {
                        for x in 0..wi {
                            let mut sum = [0u32; 3];
                            let mut n = 0;
                            for dy in (-r..=r).step_by(2) {
                                for dx in (-r..=r).step_by(2) {
                                    let i = (((y + dy).clamp(0, hi - 1) * wi + (x + dx).clamp(0, wi - 1)) * 4) as usize;
                                    for k in 0..3 {
                                        sum[k] += src[i + k] as u32;
                                    }
                                    n += 1;
                                }
                            }
                            let i = ((y * wi + x) * 4) as usize;
                            for k in 0..3 {
                                px[i + k] = (sum[k] / n) as u8;
                            }
                        }
                    }
                }
            }
            Spoilt::Noise => {
                // (Blocky and speckled.)
                let b = (wi.min(hi) / 30).max(2);
                let mut s = seed | 1;
                for y in 0..hi {
                    for x in 0..wi {
                        let (bx, by) = (x / b * b, y / b * b);
                        let src = ((by * wi + bx) * 4) as usize;
                        let i = ((y * wi + x) * 4) as usize;
                        s ^= s << 13;
                        s ^= s >> 7;
                        s ^= s << 17;
                        let n = (s % 61) as i32 - 30;
                        for k in 0..3 {
                            px[i + k] = (px[src + k] as i32 + n).clamp(0, 255) as u8;
                        }
                    }
                }
            }
            Spoilt::Finger | Spoilt::Gnome => {
                // (Over a corner of the lens: a fingertip's pink blur, or a gnome's red hat and
                // white beard peeking in.)
                let (cx, cy, r) = if self == Spoilt::Finger { (wi as f32 * 0.1, hi as f32 * 0.95, wi.min(hi) as f32 * 0.55) } else { (wi as f32 * 0.82, hi as f32 * 0.78, wi.min(hi) as f32 * 0.3) };
                for y in 0..hi {
                    for x in 0..wi {
                        let (dx, dy) = ((x as f32 - cx) / r, (y as f32 - cy) / (r * if self == Spoilt::Finger { 1.6 } else { 1.0 }));
                        let d = (dx * dx + dy * dy).sqrt();
                        if d > 1.0 {
                            continue;
                        }
                        let i = ((y * wi + x) * 4) as usize;
                        let colour = match self {
                            Spoilt::Finger => [196.0, 128.0, 112.0],
                            _ if dy < -0.15 => [200.0, 30.0, 30.0],
                            _ if dy < 0.25 => [232.0, 186.0, 150.0],
                            _ => [238.0, 238.0, 232.0],
                        };
                        let a = if self == Spoilt::Finger { (1.0 - d).min(0.35) / 0.35 } else if d > 0.9 { (1.0 - d) * 10.0 } else { 1.0 };
                        for k in 0..3 {
                            px[i + k] = (px[i + k] as f32 * (1.0 - a) + colour[k] * a) as u8;
                        }
                    }
                }
            }
        }
    }
}
/// The picture part of a photograph's frame (its stand-in texture, the picture drawn in its
/// place) and the frame's state on a wall.
const PLACEHOLDER: Key = (0x00B2_D882, 0, 0x410F_4E4F_6F98_E9DF);
const ON_WALL: &str = "frameWall";
/// The game's words when there's no more to choose in the photography grab, or no money for a
/// shot (`Ui/Tooltip/...Photography:Disabled...`).
const NO_FILTER: &str = "You don't have any photo filter options. Try gaining more photography skill.";
const NO_SIZE: &str = "You don't have any other options for photo size. Try gaining some more Photography skill.";
const NO_ZOOM: &str = "This camera doesn't have a zoom feature! Maybe it's time for an upgrade.";
const NO_MONEY: &str = "You don't have enough money to take a photo.";

/// A photo style's and size's names (the `Photography` XML's `Styles` and `Sizes`).
pub fn style_name(s: Style) -> &'static str {
    ["Color", "Sepiatone", "Black & White", "Classic Vignette"][s as usize]
}

pub fn size_name(s: Size) -> &'static str {
    ["Tiny", "Small Portrait", "Medium Landscape", "Large Portrait", "Panorama"][s as usize]
}

/// The phone's click as the picture's taken.
const CLICK: &str = "ui_camera_photo";

/// A photograph's size (`PhotographSize`), and the game's frame for it
/// (`GetInstanceNameForPhotographSize`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Tiny,
    Small,
    Medium,
    Large,
    Panorama,
}

impl Size {
    pub const ALL: [Size; 5] = [Size::Tiny, Size::Small, Size::Medium, Size::Large, Size::Panorama];

    pub fn frame(self) -> &'static str {
        ["PhotographFrameSmall", "PhotographFrameB", "PhotographFrameMedium", "PhotographFrameLarge", "PhotographFramePanorama"][self as usize]
    }
}

/// A picture's look (`PaintingStyle`'s photo styles: `PhotoPlain`, `PhotoSepia`, `PhotoGray`,
/// `PhotoVignette`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Plain,
    Sepia,
    Gray,
    Vignette,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Plain, Style::Sepia, Style::Gray, Style::Vignette];
}

/// A photograph: its picture (by id, beside the saves), size and style.
#[derive(Clone, Debug, PartialEq)]
pub struct Photo {
    pub id: String,
    pub size: Size,
    pub style: Style,
}

impl Photo {
    /// Its inventory stack's key (`photo:<size>:<style>:<id>`).
    pub fn key(&self) -> String {
        format!("photo:{}:{}:{}", self.size as u8, self.style as u8, self.id)
    }

    pub fn from_key(key: &str) -> Option<Photo> {
        let mut p = key.strip_prefix("photo:")?.splitn(3, ':');
        let size = *Size::ALL.get(p.next()?.parse::<usize>().ok()?)?;
        let style = *Style::ALL.get(p.next()?.parse::<usize>().ok()?)?;
        let id = p.next().filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()))?.to_string();
        Some(Photo { id, size, style })
    }
}

/// A still life's style (`PaintingStyle`'s paint styles: `PaintBasic`, and the stylized
/// `PaintGenius`, `PaintInsane` and `PaintSomber`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaintStyle {
    Basic,
    Genius,
    Insane,
    Somber,
}

impl PaintStyle {
    pub const ALL: [PaintStyle; 4] = [PaintStyle::Basic, PaintStyle::Genius, PaintStyle::Insane, PaintStyle::Somber];
    pub const STYLIZED: [PaintStyle; 3] = [PaintStyle::Genius, PaintStyle::Insane, PaintStyle::Somber];

    /// The game's word for it (`Easel:PaintGenius`...).
    pub fn name(self) -> &'static str {
        ["Basic", "Genius", "Insane", "Somber"][self as usize]
    }
}

/// The game's words for painting a still life, the Painting it takes (`PaintStillLife`'s
/// `SkillThreshold`), and when the stylized ones come (`kPaintingStyleUnlockLevel` with the right
/// traits, `kSpecialPaintingStyleUnlockLevels` for the somber one, or a masterpiece for genius).
pub const STILL_LIFE: &str = "Paint Still-Life";
pub const STYLIZED: &str = "Paint Stylized Still-Life...";
pub const STILL_LIFE_LEVEL: u32 = 5;
const STYLE_LEVEL: u32 = 6;
const SOMBER_LEVEL: u32 = 9;

/// Whether a painter can paint in a style (`SkillOwnerCanPaint`, the XML's traits).
pub fn can_paint(style: PaintStyle, level: u32, traits: &[crate::life::Trait], masterpieces: f64) -> bool {
    use crate::life::Trait;
    let has = |t: &[Trait]| t.iter().any(|t| traits.contains(t));
    match style {
        PaintStyle::Basic => level >= STILL_LIFE_LEVEL,
        PaintStyle::Genius => masterpieces > 0.0 || (level >= STYLE_LEVEL && has(&[Trait::Artistic, Trait::Genius])),
        PaintStyle::Insane => level >= STYLE_LEVEL && has(&[Trait::Artistic, Trait::Insane]),
        PaintStyle::Somber => level >= SOMBER_LEVEL || (level >= STYLE_LEVEL && has(&[Trait::Artistic, Trait::Grumpy])),
    }
}

/// The game's words for a portrait (`PaintPortrait`: "Paint Portrait of..." and its Sim picker,
/// "Paint Portrait" on the queue, "Pose for Portrait" for whoever sits for it), and for a scene
/// memorized with the phone and painted later (`MemorizeScene`, `PaintFromMemory`); the Painting
/// they take (their tunings' `SkillThreshold`).
pub const PORTRAIT_MENU: &str = "Paint Portrait of...";
pub const PORTRAIT: &str = "Paint Portrait";
pub const POSE: &str = "Pose for Portrait";
pub const MEMORIZE: &str = "Memorize Scene";
pub const FROM_MEMORY: &str = "Paint from Memory";
pub const PORTRAIT_LEVEL: u32 = 7;
pub const MEMORY_LEVEL: u32 = 8;

/// A scene memorized with the phone (`MemorizedScene`): its picture, and the frame it was taken
/// in (the canvas it's for).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Memorized {
    pub id: String,
    pub frame: u8,
}

/// Sitting for a portrait (`PoseForPortrait`): for whom, at which easel, where to stand, and
/// whether they're there, posing.
#[derive(Component, Clone, Copy, Debug)]
pub struct Posing {
    pub painter: Entity,
    pub easel: Entity,
    pub spot: Vec2,
    pub posed: bool,
    /// When they began posing (seconds), turned to the painter a moment after.
    pub since: f32,
}

/// Whether an easel's action waits on its subject before the painting's begun (a still life's or
/// portrait's grab, a memorized scene).
pub fn needs_subject(label: &str) -> bool {
    still_style(label).is_some() || label == PORTRAIT || label == FROM_MEMORY
}

/// Where someone sits for a portrait: before the easel, the painter's side of it opposite
/// (`RouteToObjectRadius` two metres off).
pub fn pose_spot(easel_at: Vec3, painter_at: Vec2) -> Vec2 {
    let e = easel_at.xz();
    e + (e - painter_at).normalize_or(Vec2::Y) * 1.6
}

/// A portrait begun: the painter to the easel ("Paint Portrait"), and whoever's painted to stand
/// before it and pose ("Pose for Portrait").
pub fn sit_for_portrait(w: &mut World, painter: Entity, paintee: Entity, easel: Entity) {
    let Some(def) = w.get::<crate::interact::GameObject>(easel).and_then(|o| crate::interact::interactions_for(o.kind).iter().position(|d| d.special == crate::interact::Special::SellPainting)) else { return };
    let Some((o, tf)) = w.get::<crate::interact::GameObject>(easel).cloned().zip(w.get::<Transform>(easel).copied()) else { return };
    let level = w.get::<crate::nav::Floor>(easel).map_or(1, |f| f.0);
    let spot = pose_spot(tf.translation, o.use_point(&tf));
    if let Some(mut q) = w.get_mut::<ActionQueue>(painter) {
        q.push_player(Action::new(PORTRAIT, ActionKind::Object { target: easel, def }, false));
    }
    if let Some(mut q) = w.get_mut::<ActionQueue>(paintee) {
        q.push_player(Action::new(POSE, ActionKind::GoHere(spot, level), false));
    }
    w.entity_mut(paintee).insert(Posing { painter, easel, spot, posed: false, since: 0.0 });
    info!("photos: a portrait at the {}", o.name);
}

/// Those sitting for portraits: once at their spot, posing (an idle, turned to the painter) for
/// as long as the painter's at it; let go when the painter's done or has stopped.
#[allow(clippy::type_complexity)]
fn sittings(mut commands: Commands, time: Res<Time>, mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Option<&mut Posing>)>) {
    let sitters: Vec<(Entity, Posing)> = sims.iter().filter_map(|(e, _, _, _, p)| p.map(|p| (e, *p))).collect();
    for (e, p) in sitters {
        let painting = sims.get(p.painter).is_ok_and(|(_, _, _, q, _)| q.0.iter().any(|a| a.label == PORTRAIT && !a.cancel));
        let painter_at = sims.get(p.painter).map(|s| s.2.translation).ok();
        let Ok((_, sim, tf, mut q, posing)) = sims.get_mut(e) else { continue };
        let Some(mut posing) = posing else { continue };
        if !painting {
            for a in q.0.iter_mut().filter(|a| a.label == POSE) {
                a.cancel = true;
            }
            commands.entity(e).remove::<Posing>();
            continue;
        }
        // (On the way still, or something before it to finish first.)
        let walking = q.0.iter().any(|a| a.label == POSE && !a.cancel && matches!(a.kind, ActionKind::GoHere(..)));
        if posing.posed || walking {
            continue;
        }
        // (Not there after all (the way there failed): close enough, as the game's radius.)
        if tf.translation.xz().distance(posing.spot) > 3.0 {
            debug!("photos: {} posing short of the spot", sim.first);
        }
        // (There: posing, an idle, turned to the painter, till the portrait's captured.)
        posing.posed = true;
        posing.since = time.elapsed_secs();
        q.0.push_front(Action::new(POSE, ActionKind::Outro { clips: &[], then: None, secs: 600.0, stand_at: None, target: p.easel }, false));
        let idle: &'static [&'static str] = if sim.age == Age::Child { &["c_idle_neutral_loop_"] } else { &["a_idle_neutral_loop_"] };
        commands.entity(e).insert(crate::anim::ActionClip::new(None, idle));
        if let Some(at) = painter_at {
            let d = at - tf.translation;
            commands.entity(e).insert(crate::reactions::Facing(Quat::from_rotation_y(d.x.atan2(d.z))));
        }
        info!("photos: {} poses for a portrait", sim.first);
    }
}

/// A portrait sitter let go: posing no more.
fn release(commands: &mut Commands, sims: &mut Query<(&Transform, &Sim, &mut ActionQueue, Option<&crate::sim::PlumbobRef>)>, paintee: Entity) {
    commands.entity(paintee).remove::<Posing>();
    if let Ok((.., mut q, _)) = sims.get_mut(paintee) {
        for a in q.0.iter_mut().filter(|a| a.label == POSE) {
            a.cancel = true;
        }
    }
}

/// The still life an easel's action is painting, by its name on the menu.
pub fn still_style(label: &str) -> Option<PaintStyle> {
    if label == STILL_LIFE {
        return Some(PaintStyle::Basic);
    }
    PaintStyle::STYLIZED.into_iter().find(|s| s.name() == label)
}

/// A still life painting's key (`painting:<canvas>:still-<style>-<picture>#<n>`): its canvas,
/// style and picture.
pub fn still_key(size: u8, style: PaintStyle, id: &str, n: u32) -> String {
    format!("painting:{size}:still-{}-{id}#{n}", style as u8)
}

/// What a still life's key holds.
pub fn still(key: &str) -> Option<(u8, PaintStyle, String)> {
    let (size, rest) = key.strip_prefix("painting:")?.split_once(':')?;
    let mut p = rest.split('#').next()?.strip_prefix("still-")?.splitn(2, '-');
    let style = *PaintStyle::ALL.get(p.next()?.parse::<usize>().ok()?)?;
    let id = p.next().filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()))?.to_string();
    Some((size.parse::<u8>().ok()?.min(2), style, id))
}

/// A picture painted (RGBA, `w` × `h`): its colours gathered into strokes (the quietest of the
/// four corners about each pixel: a Kuwahara filter), then the style's palette: plain, a
/// genius's few bold tones, the insane's turned colours, or the somber's grey-blue gloom.
pub fn paint_filter(px: &mut [u8], w: u32, h: u32, style: PaintStyle) {
    let (w, h) = (w as i32, h as i32);
    let r = (w.min(h) / 60).clamp(2, 5);
    let src = px.to_vec();
    let at = |x: i32, y: i32| {
        let i = ((y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) * 4) as usize;
        [src[i] as f32, src[i + 1] as f32, src[i + 2] as f32]
    };
    for y in 0..h {
        for x in 0..w {
            let mut best = (f32::MAX, [0.0f32; 3]);
            for (dx, dy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (mut sum, mut sq, mut n) = ([0.0f32; 3], 0.0f32, 0.0f32);
                for j in 0..=r {
                    for i in 0..=r {
                        let c = at(x + i * dx, y + j * dy);
                        for k in 0..3 {
                            sum[k] += c[k];
                        }
                        let l = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
                        sq += l * l;
                        n += 1.0;
                    }
                }
                let mean = sum.map(|s| s / n);
                let l = 0.299 * mean[0] + 0.587 * mean[1] + 0.114 * mean[2];
                let var = sq / n - l * l;
                if var < best.0 {
                    best = (var, mean);
                }
            }
            let [mut r0, mut g0, mut b0] = best.1;
            let l = 0.299 * r0 + 0.587 * g0 + 0.114 * b0;
            match style {
                PaintStyle::Basic => {
                    // (A touch richer.)
                    [r0, g0, b0] = [r0, g0, b0].map(|c| l + (c - l) * 1.15);
                }
                PaintStyle::Genius => {
                    // (Bold: fewer tones, more contrast.)
                    [r0, g0, b0] = [r0, g0, b0].map(|c| (((l + (c - l) * 1.4 - 128.0) * 1.25 + 128.0) / 51.0).round() * 51.0);
                }
                PaintStyle::Insane => {
                    // (Its colours turned about, louder.)
                    [r0, g0, b0] = [b0, r0, g0].map(|c| l + (c - l) * 1.7);
                }
                PaintStyle::Somber => {
                    let g = l * 0.8 + 0.2 * (r0 + g0 + b0) / 3.0;
                    [r0, g0, b0] = [g * 0.72, g * 0.78, g * 0.92];
                }
            }
            let i = ((y * w + x) * 4) as usize;
            px[i] = r0.round().clamp(0.0, 255.0) as u8;
            px[i + 1] = g0.round().clamp(0.0, 255.0) as u8;
            px[i + 2] = b0.round().clamp(0.0, 255.0) as u8;
            px[i + 3] = 255;
        }
    }
}

/// A picture's file, by its id.
fn picture_path(id: &str) -> PathBuf {
    dir().join(format!("{id}.png"))
}

/// Where photographs' pictures are kept: beside the saves.
fn dir() -> PathBuf {
    crate::save::saves_dir().join("photos")
}

/// A picture's pixels (RGBA, `w` × `h`) in a style: as it is, sepia-toned, grey, or darkened
/// towards its edges.
pub fn filter(px: &mut [u8], w: u32, h: u32, style: Style) {
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let far = (cx * cx + cy * cy).sqrt().max(1.0);
    for (i, p) in px.chunks_exact_mut(4).enumerate() {
        let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let (r, g, b) = match style {
            Style::Plain => (r, g, b),
            Style::Gray => {
                let l = 0.299 * r + 0.587 * g + 0.114 * b;
                (l, l, l)
            }
            Style::Sepia => (0.393 * r + 0.769 * g + 0.189 * b, 0.349 * r + 0.686 * g + 0.168 * b, 0.272 * r + 0.534 * g + 0.131 * b),
            Style::Vignette => {
                let (x, y) = ((i as u32 % w) as f32 + 0.5 - cx, (i as u32 / w) as f32 + 0.5 - cy);
                let d = ((x * x + y * y).sqrt() / far - 0.45).max(0.0) / 0.55;
                let k = 1.0 - 0.85 * d * d * (3.0 - 2.0 * d);
                (r * k, g * k, b * k)
            }
        };
        p[0] = r.round().clamp(0.0, 255.0) as u8;
        p[1] = g.round().clamp(0.0, 255.0) as u8;
        p[2] = b.round().clamp(0.0, 255.0) as u8;
        p[3] = 255;
    }
}

/// The part of a picture (RGBA, `w` wide) inside a rectangle (pixels), clipped to it.
pub fn crop(px: &[u8], w: u32, h: u32, rect: URect) -> (u32, u32, Vec<u8>) {
    let (x0, y0) = (rect.min.x.min(w), rect.min.y.min(h));
    let (x1, y1) = (rect.max.x.min(w).max(x0), rect.max.y.min(h).max(y0));
    let mut out = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
    for y in y0..y1 {
        let row = ((y * w + x0) * 4) as usize;
        out.extend_from_slice(&px[row..row + ((x1 - x0) * 4) as usize]);
    }
    (x1 - x0, y1 - y0, out)
}

/// What's grabbed for (`ScreenGrabType`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GrabKind {
    /// A phone's photo (`CellPhonePhotography`).
    Phone,
    /// A still life's subject, to paint in a style (`Painting`).
    Painting(PaintStyle),
    /// A camera's photo, for the Photography skill (`Photography`).
    Camera(&'static CameraModel),
}

/// The photography grab's frames: two for each size (as laid out for 4:3, and widescreen), the
/// small, medium, large and panorama, then a smart phone's.
const PHOTO_FRAMES: [u32; 12] = [0x100, 0x101, 0x102, 0x103, 0x104, 0x105, 0x106, 0x107, 0x108, 0x109, 0x10A, 0x10B];

impl GrabKind {
    /// The layout's export for it (by its designers' comment), its frames, and its shade.
    fn layout(self) -> (&'static str, &'static [u32], u32) {
        match self {
            GrabKind::Phone => ("Cellphone Photography Screen Grab", &[FRAME_BASE], PHONE_SHADE),
            GrabKind::Painting(_) => ("Painting Screen Grab", &[FRAME_BASE, FRAME_BASE + 1, FRAME_BASE + 2], PAINTING_SHADE),
            GrabKind::Camera(_) => ("Photography Screen Grab", &PHOTO_FRAMES, PHONE_SHADE),
        }
    }
}

/// The photography grab's windows: the shot's style, size and price, the shots left, the
/// subject, and the keys' words (dimmed when there's nothing more to choose).
const INFO: u32 = 0x0880_27A0;
const SHOTS_LEFT: u32 = 0x0880_27A1;
const SUBJECT: u32 = 0x0880_27A4;
const KEY_TAKE: u32 = 0x0880_27A7;
const KEY_FILTER: u32 = 0x0880_27A8;
const KEY_SIZE: u32 = 0x0880_27A9;
const KEY_ZOOM_IN: u32 = 0x0880_27AA;
const KEY_ZOOM_OUT: u32 = 0x0880_27AB;
/// (`kDisabledShadeColor`, and the most shots told: `kNumMaxShotsRemainingToShow`.)
const DISABLED: u32 = 0x998D_8D8D;
const MOST_SHOTS: i64 = 100;

/// What a painter's grab is for: a still life, a portrait of someone posing at the easel
/// (`PaintPortrait`), or a scene memorized with the phone to paint later (`MemorizeScene`).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Purpose {
    #[default]
    StillLife,
    Portrait {
        paintee: Entity,
        easel: Entity,
    },
    Memorize,
}

/// A picture taken: by whom, for what, its picture (by id) and the frame it was taken in.
#[derive(Message, Clone, Debug)]
pub struct Grabbed {
    pub sim: Entity,
    pub kind: GrabKind,
    pub purpose: Purpose,
    pub id: String,
    pub frame: usize,
    /// A camera's photo: its size and style, and whether it came out spoilt.
    pub size: Size,
    pub style: Style,
    pub spoilt: Option<Spoilt>,
    /// What it's of (`photosubjects`): the subject, and its key objects' names.
    pub subject: Option<(String, Vec<String>)>,
}

/// A screen grab under way (`ScreenGrabController`): who for, what for, the frame and style,
/// how far along, the layout on screen and what's been hidden for it, and where the camera was.
#[derive(Resource)]
pub struct Grabbing {
    pub sim: Entity,
    pub kind: GrabKind,
    pub purpose: Purpose,
    /// The action it's for (its name on the queue).
    label: String,
    style: Style,
    stage: Stage,
    root: Option<Entity>,
    /// Its frames' windows and their targets, and the one chosen (`I` for the next).
    frames: Vec<(Entity, Option<Entity>)>,
    frame: usize,
    /// A camera's: the size chosen, the photographer's level, the frames for a widescreen, the
    /// layout's windows, and a word to the player on show (as the game's greyed-out tooltips) and
    /// when it goes.
    size: Size,
    level: u32,
    wide: bool,
    ui: Option<crate::layout::Spawned>,
    tip: Option<(Entity, f32)>,
    /// What the camera's aimed at (`photosubjects`: the subject, its key objects' names), and when
    /// it was last looked at.
    subject: Option<(String, Vec<String>)>,
    aimed: f32,
    /// The world outside the frame dimmed (the base window's frame drawable): four pieces turning
    /// about it.
    shades: Vec<Entity>,
    hidden: Vec<(Entity, Visibility)>,
    view: Option<(Vec3, f32, f32, f32, f32, Option<Entity>)>,
    /// How the walls were shown (they're up while looking out: `SetWallsViewMode`).
    walls: Option<crate::building::WallMode>,
    rect: URect,
    shot: Arc<Mutex<Option<(u32, u32, Vec<u8>)>>>,
    /// (Tests: captured on its own after so many seconds.)
    auto: Option<f32>,
    began: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Open,
    Framing,
    Hiding(u8),
    Shooting,
}

impl Grabbing {
    pub fn new(kind: GrabKind, sim: Entity, label: &str) -> Self {
        Grabbing {
            sim,
            kind,
            purpose: Purpose::default(),
            label: label.to_string(),
            style: Style::Plain,
            stage: Stage::Open,
            root: None,
            frames: Vec::new(),
            frame: 0,
            size: Size::Small,
            level: 0,
            wide: false,
            ui: None,
            tip: None,
            subject: None,
            aimed: f32::MIN,
            shades: Vec::new(),
            hidden: Vec::new(),
            view: None,
            walls: None,
            rect: URect::default(),
            shot: Arc::new(Mutex::new(None)),
            auto: None,
            began: 0.0,
        }
    }
}

/// "Take Photo" from the phone: an action of its own, the grab for as long as it lasts.
pub fn take_photo(_sim: Entity) -> ActionKind {
    ActionKind::TakePhoto { camera: None }
}

/// "Take Photo" with a camera from the inventory.
pub fn take_photo_with(camera: &'static CameraModel) -> ActionKind {
    ActionKind::TakePhoto { camera: Some(camera.name) }
}

/// Whether the action's at an easel with its subject still to come (a still life's, a portrait's
/// or a memory's, not yet begun on): the easel, and its style (a portrait's and a memory's
/// plain).
fn at_easel(a: &Action, objects: &Query<&crate::interact::GameObject>, plan: Option<&crate::paintings::PaintPlan>) -> Option<(Entity, PaintStyle)> {
    let ActionKind::Object { target, def } = a.kind else { return None };
    let o = objects.get(target).ok()?;
    if crate::interact::interactions_for(o.kind).get(def).is_none_or(|d| d.special != crate::interact::Special::SellPainting) || !needs_subject(&a.label) {
        return None;
    }
    let style = still_style(&a.label).unwrap_or(PaintStyle::Basic);
    (!plan.is_some_and(|p| p.painted.as_ref().is_some_and(|x| still(&x.key).is_some()))).then_some((target, style))
}

/// The selected Sim's photo begun, or their still life, portrait or scene to memorize: the grab
/// (a portrait's once the sitter's posing). A memory painted: its picture, straight on.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn start_grab(
    mut commands: Commands,
    grabbing: Option<Res<Grabbing>>,
    mut sims: Query<(Entity, &mut ActionQueue, Option<&crate::paintings::PaintPlan>, &crate::interact::Skills, Option<&CameraSettings>, Option<&Memorized>, &Sim), With<Selected>>,
    sitters: Query<(Entity, &Posing)>,
    objects: Query<&crate::interact::GameObject>,
    (time, baked): (Res<Time>, Option<Res<crate::baked::Baked>>),
    (wishes_q, journals_q): (Query<&crate::wishes::Wishes>, Query<&crate::journal::SkillJournal>),
) {
    if grabbing.is_some() {
        return;
    }
    for (e, mut q, plan, skills, settings, memorized, sim) in &mut sims {
        let Some(a) = q.0.front_mut().filter(|a| !a.cancel && matches!(a.phase, Phase::Running(_))) else { continue };
        let mut purpose = Purpose::StillLife;
        let kind = if let ActionKind::TakePhoto { camera: model } = a.kind {
            match model.and_then(camera) {
                _ if a.label == MEMORIZE => {
                    purpose = Purpose::Memorize;
                    GrabKind::Painting(PaintStyle::Basic)
                }
                Some(c) => GrabKind::Camera(c),
                None => GrabKind::Phone,
            }
        } else if let Some((easel, style)) = at_easel(a, &objects, plan) {
            if a.label == FROM_MEMORY {
                // (A memory: painted straight from its picture, on the canvas it was framed for;
                // none, nothing to paint.)
                let Some(m) = memorized else {
                    a.cancel = true;
                    continue;
                };
                let mut rng = rand::rng();
                let size = m.frame.min(2);
                let mut p = crate::paintings::paint(
                    baked.as_ref().map(|b| &b.0.paintings),
                    size,
                    skills.level("Painting"),
                    &sim.traits,
                    sim.age == Age::Child,
                    crate::wishes::has(wishes_q.get(e).ok(), "ExtraCreative"),
                    crate::journal::earned(journals_q.get(e).ok(), "Proficient Painter"),
                    &mut rng,
                );
                p.key = still_key(size, PaintStyle::Basic, &m.id, rand::Rng::random(&mut rng));
                commands.entity(e).insert(crate::paintings::PaintPlan { size, painted: Some(p) });
                info!("photos: {} paints from memory", sim.first);
                continue;
            }
            if a.label == PORTRAIT {
                // (Not till the sitter's posing; nobody sitting, nothing to paint.)
                match sitters.iter().find(|(_, p)| p.painter == e) {
                    Some((paintee, p)) if p.posed && time.elapsed_secs() - p.since > 1.5 => purpose = Purpose::Portrait { paintee, easel },
                    Some(_) => continue,
                    None => {
                        a.cancel = true;
                        continue;
                    }
                }
            }
            GrabKind::Painting(style)
        } else {
            continue;
        };
        let mut g = Grabbing::new(kind, e, &a.label);
        g.purpose = purpose;
        g.auto = std::env::var("PHOTO_CAPTURE_AT").ok().and_then(|v| v.parse().ok());
        g.frame = std::env::var("PHOTO_FRAME").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        // (A camera's: its default size and mode, as far as the photographer's skill goes.)
        if let GrabKind::Camera(c) = kind {
            g.level = skills.level(SKILL);
            let (size, style) = settings.and_then(|s| s.0.get(c.name)).copied().unwrap_or((Size::Small, Style::Plain));
            g.size = if sizes_at(g.level, c).any(|s| s == size) { size } else { Size::Small };
            g.style = if styles_at(g.level).any(|s| s == style) { style } else { Style::Plain };
        }
        g.began = time.elapsed_secs();
        commands.insert_resource(g);
        info!("photos: a screen grab ({kind:?})");
    }
}

/// A portrait's camera: at the sitter's face (`kPortraitOffsetAdult`, `...Teen`, `...Child`,
/// `...Toddler`), so far before it.
fn portrait_height(age: Age) -> f32 {
    match age {
        Age::Child => 1.174,
        Age::Teen => 1.601,
        a if a.is_little() => 0.36,
        _ => 1.722,
    }
}
const PORTRAIT_DISTANCE: f32 = 1.6;

/// How high a Sim's eyes are, by their age.
fn eye_height(age: Age) -> f32 {
    match age {
        Age::Child => 1.08,
        Age::Teen => 1.55,
        a if a.is_little() => 0.6,
        _ => 1.62,
    }
}

/// A camera's frame for a size (`UIFrame`, two to each: 4:3's, then widescreen's).
fn frame_for(size: Size, wide: bool) -> usize {
    (size as usize).saturating_sub(1) * 2 + usize::from(wide)
}

/// A frame laid out for a screen not 4:3 (`AdjustWindowForNewRes`): scaled by the screen's
/// height over the layout's (768), its middle where it fell across the layout's 1024 × 768, its
/// target scaled with it.
fn fit_frame(c: &mut s3bake::ui::UiWindow, screen: Vec2) {
    let k = screen.y / 768.0;
    let [x0, y0, x1, y1] = c.area;
    let (w, h) = ((x1 - x0) * k, (y1 - y0) * k);
    let (cx, cy) = ((x0 + x1) * 0.5 / 1024.0 * screen.x, (y0 + y1) * 0.5 / 768.0 * screen.y);
    c.area = [cx - w * 0.5, cy - h * 0.5, cx + w * 0.5, cy + h * 0.5];
    c.place = s3bake::ui::UiPlace::Fixed;
    for t in &mut c.children {
        t.area = t.area.map(|v| v * k);
        t.place = s3bake::ui::UiPlace::Fixed;
    }
}

/// The game's text keys in a layout's captions, put in its words.
fn localize(ui: &crate::layout::UiAssets, w: &mut s3bake::ui::UiWindow) {
    if w.caption.contains('/') && w.caption.contains(':') {
        w.caption = ui.localize(&w.caption).unwrap_or_default();
    }
    for c in &mut w.children {
        localize(ui, c);
    }
}

/// The screen grab: its layout up, the camera at the Sim's eyes and the rest hidden; C takes the
/// picture (the layout away a moment, the window's picture cut to the frame), Escape stops.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn grab(
    mut commands: Commands,
    grabbing: Option<ResMut<Grabbing>>,
    ui: Option<ResMut<crate::layout::UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut sims: Query<(&Transform, &Sim, &mut ActionQueue, Option<&crate::sim::PlumbobRef>)>,
    mut roots: Query<(Entity, &mut Visibility), (With<Node>, Without<ChildOf>)>,
    mut seen: Query<&mut Visibility, Without<Node>>,
    nodes: Query<(&ComputedNode, &bevy::ui::UiGlobalTransform)>,
    mut places: Query<&mut Node>,
    (mut sounds, mut grabbed, time, mut walls): (MessageWriter<crate::sound::PlaySound>, MessageWriter<Grabbed>, Res<Time>, ResMut<crate::building::WallMode>),
    (windows, mut texts, mut colours, mut household): (Query<&Window, With<bevy::window::PrimaryWindow>>, Query<&mut Text>, Query<&mut TextColor>, Option<ResMut<crate::interact::Household>>),
) {
    let Some(mut g) = grabbing else { return };
    let g = &mut *g;
    let walls = &mut *walls;
    // (Put away from the queue, or the Sim gone: stopped.)
    let still = sims.get(g.sim).is_ok_and(|s| s.2.0.front().is_some_and(|a| a.label == g.label && !a.cancel));
    let mut done: Option<bool> = (!still).then_some(false);
    // (A photo's from the Sim's eyes; a still life's from where the player looks.)
    let phone = !matches!(g.kind, GrabKind::Painting(_));
    // (A still life's painting not begun till its subject's captured.)
    if !phone
        && let Ok((.., mut q, _)) = sims.get_mut(g.sim)
        && let Some(a) = q.0.front_mut()
    {
        a.phase = Phase::Running(0.0);
    }
    match g.stage {
        Stage::Open => {
            let Some(mut ui) = ui else { return };
            let (comment, frames, shade) = g.kind.layout();
            let Some(mut w) = ui.export_commented(LAYOUT, comment).cloned() else {
                warn!("photos: no screen grab layout");
                done = Some(false);
                return finish(&mut commands, g, done, &mut cams, &mut sims, &mut roots, &mut seen, walls);
            };
            localize(&ui, &mut w);
            w.flags |= s3bake::ui::WIN_VISIBLE;
            let camera = matches!(g.kind, GrabKind::Camera(_));
            let screen = windows.single().map_or(Vec2::new(1024.0, 768.0), |w| Vec2::new(w.width(), w.height()));
            if camera {
                // (A camera's frames for the size, laid out for the screen's shape.)
                g.wide = (screen.y / screen.x - 0.75).abs() > 0.01;
                g.frame = frame_for(g.size, g.wide);
            }
            g.frame = g.frame.min(frames.len() - 1);
            for c in w.children.iter_mut().filter(|c| frames.contains(&c.id)) {
                if c.id == frames[g.frame] {
                    c.flags |= s3bake::ui::WIN_VISIBLE;
                }
                if camera {
                    // (A widescreen frame scaled to the screen's height and kept where it falls
                    // across it, its target with it: `AdjustWindowForNewRes`; its picture
                    // stretched to it, to frame the target.)
                    if g.wide && c.id % 2 == 1 {
                        fit_frame(c, screen);
                    }
                    if let Some(s3bake::ui::UiDrawable::Image { flags, .. }) = &mut c.drawable {
                        *flags |= 1;
                    }
                } else if let Some(s3bake::ui::UiDrawable::Image { flags, .. }) = &mut c.drawable {
                    // (The frame's picture at its own size, round the target: a phone's
                    // viewfinder isn't stretched to its window.)
                    *flags = 0;
                }
            }
            let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
            let Some(root) = s.root else { return };
            commands.entity(root).insert((GlobalZIndex(60), Interaction::default(), FocusPolicy::Block, crate::hud::BlocksWorld, DespawnOnExit(crate::AppState::InGame)));
            g.frames = frames.iter().filter_map(|f| s.id(*f).map(|e| (e, s.within(e, TARGET)))).collect();
            g.ui = Some(s.clone());
            // (The world outside the target dimmed, as the base window's frame drawable has it:
            // under the layout's windows, out to the screen's edges.)
            g.shades = (0..4).map(|_| commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, BackgroundColor(crate::layout::color(shade)), Pickable::IGNORE)).id()).collect();
            commands.entity(root).insert_children(0, &g.shades);
            g.root = Some(root);
            if let Ok(cam) = cams.single() {
                g.view = Some((cam.focus, cam.yaw, cam.pitch, cam.distance, cam.height_offset, cam.follow));
            }
            let painter_at = sims.get(g.sim).map(|s| s.0.translation).ok();
            let portrait = match g.purpose {
                Purpose::Portrait { paintee, .. } => sims.get(paintee).ok().map(|(tf, sim, ..)| (*tf, sim.age)),
                _ => None,
            };
            if let (Ok(mut cam), Some((tf, age))) = (cams.single_mut(), portrait) {
                // (A portrait's: the camera locked on the sitter's face, before them, from the
                // painter's side (`GrabPortrait`'s `LockOnObject`, at the age's
                // `kPortraitOffset`), the painter and the easel faded out of the way.)
                let f = painter_at.map(|p| (p - tf.translation).with_y(0.0).normalize_or_zero()).filter(|f| *f != Vec3::ZERO).unwrap_or(tf.rotation * Vec3::Z);
                let head = tf.translation + Vec3::Y * portrait_height(age);
                let eye = head + f * PORTRAIT_DISTANCE + Vec3::Y * 0.1;
                cam.photo = Some(crate::camera::PhotoView {
                    eye,
                    yaw: f.x.atan2(f.z),
                    pitch: -0.05,
                    fov: crate::camera::FOV.to_radians(),
                    fovs: ((crate::camera::FOV * 0.5).to_radians(), crate::camera::FOV.to_radians()),
                });
            } else if let (Ok(mut cam), Ok((tf, sim, ..))) = (cams.single_mut(), sims.get(g.sim)) {
                if phone {
                    // The camera to their eyes, looking the way they face.
                    let f = tf.rotation * Vec3::Z;
                    // (As far as the phone or camera zooms: a camera further at level six,
                    // `ZoomUpgrade`.)
                    let narrowest = ZOOMS[match g.kind {
                        GrabKind::Camera(c) if g.level >= ZOOM_UPGRADE_LEVEL => c.zoom.1,
                        GrabKind::Camera(c) => c.zoom.0,
                        _ => PHONE_ZOOM,
                    }];
                    let zoom = std::env::var("PHOTO_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(crate::camera::FOV);
                    let turn = std::env::var("PHOTO_YAW").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                    cam.photo = Some(crate::camera::PhotoView {
                        eye: tf.translation + Vec3::Y * eye_height(sim.age) + f * 0.12,
                        yaw: (-f.x).atan2(-f.z) + turn,
                        pitch: std::env::var("PHOTO_PITCH").ok().and_then(|v| v.parse().ok()).unwrap_or(-0.05),
                        fov: zoom.clamp(narrowest, crate::camera::FOV).to_radians(),
                        fovs: (narrowest.to_radians(), crate::camera::FOV.to_radians()),
                    });
                } else {
                    // (A still life's subject: the camera kept on the painter, `tetherCamera`.)
                    cam.look_at(tf.translation);
                    cam.follow(g.sim);
                    if let Some(d) = std::env::var("PHOTO_DIST").ok().and_then(|v| v.parse::<f32>().ok()) {
                        cam.distance = d;
                    }
                    if let Some(y) = std::env::var("PHOTO_YAW").ok().and_then(|v| v.parse::<f32>().ok()) {
                        let f = tf.rotation * Vec3::Z;
                        cam.yaw = f.x.atan2(f.z) + y;
                    }
                }
            }
            hide(g, &mut roots, &mut seen, &sims);
            // (The walls up for a phone's photo, from the Sim's eyes, and a portrait's:
            // `SetWallsViewMode`.)
            if phone || matches!(g.purpose, Purpose::Portrait { .. }) {
                g.walls = Some(*walls);
                *walls = crate::building::WallMode::Up;
            }
            g.stage = Stage::Framing;
        }
        Stage::Framing => {
            // (The interface, the Sim and their plumbob kept out of the picture.)
            hide(g, &mut roots, &mut seen, &sims);
            // (Where the picture's to be cut from the window's: the target, in pixels.)
            let target = g.frames.get(g.frame).and_then(|f| f.1);
            if let (Some((c, at)), Some((screen, _))) = (target.and_then(|t| nodes.get(t).ok()), g.root.and_then(|r| nodes.get(r).ok())) {
                let half = c.size() * 0.5;
                let (min, max) = (at.translation - half, at.translation + half);
                g.rect = URect::new(min.x.max(0.0) as u32, min.y.max(0.0) as u32, max.x.max(0.0) as u32, max.y.max(0.0) as u32);
                // The world outside it dimmed: left and above, above and right, right and below,
                // below and left.
                let k = c.inverse_scale_factor();
                let (l, t, r, b) = (min.x * k, min.y * k, max.x * k, max.y * k);
                let (w, h) = (screen.size().x * k, screen.size().y * k);
                let abs = |x0: f32, y0: f32, x1: f32, y1: f32| Node { position_type: PositionType::Absolute, left: Val::Px(x0), top: Val::Px(y0), width: Val::Px((x1 - x0).max(0.0)), height: Val::Px((y1 - y0).max(0.0)), ..default() };
                let boxes = [abs(0.0, 0.0, l, b), abs(l, 0.0, w, t), abs(r, t, w, h), abs(0.0, b, r, h)];
                for (e, n) in g.shades.iter().zip(boxes) {
                    if let Ok(mut was) = places.get_mut(*e)
                        && *was != n
                    {
                        *was = n;
                    }
                }
            }
            let auto = g.auto.is_some_and(|s| time.elapsed_secs() - g.began > s);
            let now = time.elapsed_secs();
            // (A word to the player gone after a while.)
            if let Some((e, until)) = g.tip
                && now > until
            {
                commands.entity(e).try_despawn();
                g.tip = None;
            }
            let funds = household.as_ref().map_or(0, |h| h.funds);
            let mut tip: Option<&str> = None;
            let mut frame_to = None;
            match g.kind {
                // A camera's: its next mode (F) and size (I) as the photographer's skill and the
                // camera allow (`GetNextPhotographFilterType`, `GetNextCameraSize`), zoom (Z, X)
                // as it can, and what a shot costs and how many there are money for.
                GrabKind::Camera(c) => {
                    let styles: Vec<Style> = styles_at(g.level).collect();
                    let sizes: Vec<Size> = sizes_at(g.level, c).collect();
                    let zooms = ZOOMS[if g.level >= ZOOM_UPGRADE_LEVEL { c.zoom.1 } else { c.zoom.0 }] < crate::camera::FOV;
                    if keys.just_pressed(KeyCode::KeyF) {
                        match styles.iter().position(|s| *s == g.style) {
                            Some(i) if styles.len() > 1 => {
                                g.style = styles[(i + 1) % styles.len()];
                                sounds.write(crate::sound::PlaySound::ui("ui_photo_type"));
                            }
                            _ => tip = Some(NO_FILTER),
                        }
                    }
                    if keys.just_pressed(KeyCode::KeyI) {
                        match sizes.iter().position(|s| *s == g.size) {
                            Some(i) if sizes.len() > 1 => {
                                g.size = sizes[(i + 1) % sizes.len()];
                                frame_to = Some(frame_for(g.size, g.wide));
                                sounds.write(crate::sound::PlaySound::ui("ui_photo_size"));
                            }
                            _ => tip = Some(NO_SIZE),
                        }
                    }
                    if !zooms && keys.any_just_pressed([KeyCode::KeyZ, KeyCode::KeyX]) {
                        tip = Some(NO_ZOOM);
                    }
                    if let Ok(mut cam) = cams.single_mut()
                        && let Some(v) = cam.photo.as_mut()
                        && !zooms
                    {
                        v.fov = crate::camera::FOV.to_radians();
                    }
                    let price = shot_cost(c, g.size, g.style);
                    let shots = funds / price.max(1);
                    if let Some(ui) = &g.ui {
                        let set = |texts: &mut Query<&mut Text>, id: u32, words: String| {
                            if let Some(mut t) = ui.text(id).and_then(|e| texts.get_mut(e).ok())
                                && t.0 != words
                            {
                                t.0 = words;
                            }
                        };
                        set(&mut texts, INFO, format!("{} / {} / §{price} per shot", style_name(g.style), size_name(g.size)));
                        set(&mut texts, SHOTS_LEFT, if shots > MOST_SHOTS { format!("Shots remaining: {MOST_SHOTS}+") } else { format!("Shots remaining: {shots}") });
                        for (id, lit) in [(KEY_FILTER, styles.len() > 1), (KEY_SIZE, sizes.len() > 1), (KEY_ZOOM_IN, zooms), (KEY_ZOOM_OUT, zooms), (KEY_TAKE, shots > 0)] {
                            let colour = crate::layout::color(if lit { 0xFFFF_FFFF } else { DISABLED });
                            if let Some(mut c) = ui.text(id).and_then(|e| colours.get_mut(e).ok())
                                && c.0 != colour
                            {
                                c.0 = colour;
                            }
                        }
                    }
                }
                // (A painter's frame, the next size of canvas: `ChangeSize`.)
                _ if keys.just_pressed(KeyCode::KeyI) && g.frames.len() > 1 => frame_to = Some((g.frame + 1) % g.frames.len()),
                _ => {}
            }
            if let Some(to) = frame_to.filter(|to| *to != g.frame && *to < g.frames.len()) {
                if let Some((f, _)) = g.frames.get(g.frame) {
                    commands.entity(*f).insert(Visibility::Hidden);
                }
                g.frame = to;
                if let Some((f, _)) = g.frames.get(g.frame) {
                    commands.entity(*f).insert(Visibility::Inherited);
                }
            }
            let price = if let GrabKind::Camera(c) = g.kind { shot_cost(c, g.size, g.style) } else { 0 };
            let capture = keys.just_pressed(KeyCode::KeyC) || auto;
            if capture && funds < price {
                tip = Some(NO_MONEY);
                keys.clear_just_pressed(KeyCode::KeyC);
            }
            if let (Some(words), Some(root)) = (tip, g.root) {
                if let Some((e, _)) = g.tip.take() {
                    commands.entity(e).try_despawn();
                }
                let e = commands
                    .spawn((
                        Node { position_type: PositionType::Absolute, left: Val::Percent(30.0), right: Val::Percent(30.0), top: Val::Percent(48.0), justify_content: JustifyContent::Center, padding: UiRect::all(Val::Px(8.0)), border_radius: BorderRadius::all(Val::Px(6.0)), ..default() },
                        BackgroundColor(Color::srgba(0.95, 0.95, 0.85, 0.95)),
                        ChildOf(root),
                    ))
                    .with_child((Text::new(words), TextFont::from_font_size(14.0), TextColor(Color::srgb(0.1, 0.1, 0.1)), TextLayout::justify(Justify::Center)))
                    .id();
                g.tip = Some((e, now + 3.0));
            }
            if keys.just_pressed(KeyCode::Escape) {
                keys.clear_just_pressed(KeyCode::Escape);
                done = Some(false);
            } else if capture && funds >= price && g.rect.width() > 4 && g.rect.height() > 4 {
                keys.clear_just_pressed(KeyCode::KeyC);
                // (A phone's click, a camera's shutter as it's cheap or dear; the painter's
                // capture is quiet.)
                match g.kind {
                    GrabKind::Phone => {
                        sounds.write(crate::sound::PlaySound::ui(CLICK));
                    }
                    GrabKind::Camera(c) => {
                        sounds.write(crate::sound::PlaySound::ui(c.click));
                        // (The shot's paid for.)
                        if let Some(h) = household.as_mut() {
                            h.funds -= price;
                        }
                    }
                    GrabKind::Painting(_) => {}
                }
                for e in g.root.iter().chain(&g.shades) {
                    commands.entity(*e).insert(Visibility::Hidden);
                }
                g.stage = Stage::Hiding(0);
            }
        }
        // (The frame off the screen a frame or two before the window's picture's taken.)
        Stage::Hiding(n) => {
            hide(g, &mut roots, &mut seen, &sims);
            if n < 2 {
                g.stage = Stage::Hiding(n + 1);
            } else {
                let shot = g.shot.clone();
                commands.spawn(Screenshot::primary_window()).observe(move |s: On<ScreenshotCaptured>| {
                    if let Ok(d) = s.image.clone().try_into_dynamic() {
                        let rgba = d.to_rgba8();
                        let (w, h) = (rgba.width(), rgba.height());
                        if let Ok(mut slot) = shot.lock() {
                            *slot = Some((w, h, rgba.into_raw()));
                        }
                    }
                });
                g.stage = Stage::Shooting;
            }
        }
        Stage::Shooting => {
            hide(g, &mut roots, &mut seen, &sims);
            let taken = g.shot.lock().ok().and_then(|mut s| s.take());
            if let Some((w, h, px)) = taken {
                let (cw, ch, mut pic) = crop(&px, w, h, g.rect);
                // (A photo in its style; a still life painted in its; a camera's shot in its
                // mode, and spoilt now and then as the photographer's green
                // (`FlipCoinForPhotoFailure`).)
                let mut spoilt = None;
                match g.kind {
                    GrabKind::Phone => filter(&mut pic, cw, ch, g.style),
                    GrabKind::Painting(style) => paint_filter(&mut pic, cw, ch, style),
                    GrabKind::Camera(_) => {
                        filter(&mut pic, cw, ch, g.style);
                        let mut rng = rand::rng();
                        spoilt = match std::env::var("PHOTO_SPOIL").ok().as_deref() {
                            Some("gnome") => Some(Spoilt::Gnome),
                            Some("finger") => Some(Spoilt::Finger),
                            Some("blurry") => Some(Spoilt::Blurry),
                            Some("noise") => Some(Spoilt::Noise),
                            Some(_) => None,
                            None => Spoilt::flip(g.level, rand::Rng::random(&mut rng), rand::Rng::random(&mut rng)),
                        };
                        if let Some(s) = spoilt {
                            s.spoil(&mut pic, cw, ch, rand::Rng::random(&mut rng));
                        }
                    }
                }
                let id = format!("{:016x}", rand::random::<u64>());
                let path = picture_path(&id);
                let _ = std::fs::create_dir_all(dir());
                match std::fs::write(&path, s3bake::gamedata::encode_rgba_png(cw, ch, &pic)) {
                    Ok(()) => {
                        info!("photos: took a {cw}x{ch} picture ({})", path.display());
                        grabbed.write(Grabbed { sim: g.sim, kind: g.kind, purpose: g.purpose, id, frame: g.frame, size: g.size, style: g.style, spoilt, subject: g.subject.clone() });
                        done = Some(true);
                    }
                    Err(e) => {
                        warn!("photos: couldn't keep the picture: {e}");
                        done = Some(false);
                    }
                }
            }
        }
    }
    finish(&mut commands, g, done, &mut cams, &mut sims, &mut roots, &mut seen, walls);
}

/// A camera's aim, a few times a second (`UpdateCameraParameters`): the subject it's on, named
/// under the frame ("A Couch (Still Life)").
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn aim(
    grabbing: Option<ResMut<Grabbing>>,
    mut scenes: crate::photosubjects::SceneParams,
    cams: Query<(&Camera, &GlobalTransform), With<crate::camera::SimsCamera>>,
    photographers: Query<(&crate::interact::Skills, Option<&crate::photosubjects::PhotoRecords>)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    (mut texts, mut places, time): (Query<&mut Text>, Query<&mut Node>, Res<Time>),
) {
    let Some(mut g) = grabbing else { return };
    if !matches!(g.kind, GrabKind::Camera(_)) || g.stage != Stage::Framing {
        return;
    }
    let now = time.elapsed_secs();
    if now - g.aimed < 0.3 {
        return;
    }
    g.aimed = now;
    let (Ok((camera, at)), Ok(window)) = (cams.single(), windows.single()) else { return };
    let k = window.scale_factor();
    let frame = Rect::new(g.rect.min.x as f32 / k, g.rect.min.y as f32 / k, g.rect.max.x as f32 / k, g.rect.max.y as f32 / k);
    if frame.width() < 4.0 {
        return;
    }
    let (skills, records) = photographers.get(g.sim).map_or((None, None), |(s, r)| (Some(s), r));
    let level = skills.map_or(0, |s| s.level(SKILL));
    let aimed = scenes.aim(camera, at, frame, g.sim, records, level);
    let line = aimed.as_ref().map(|a| a.line.clone()).unwrap_or_default();
    g.subject = aimed.map(|a| (a.key, a.names));
    // (Its words, just under the frame.)
    if let Some(ui) = &g.ui
        && let Some(e) = ui.id(SUBJECT)
    {
        if let Some(mut t) = ui.text(SUBJECT).and_then(|t| texts.get_mut(t).ok())
            && t.0 != line
        {
            t.0 = line;
        }
        if let Ok(mut n) = places.get_mut(e) {
            let top = Val::Px(frame.max.y + 10.0);
            if n.top != top {
                n.top = top;
            }
        }
    }
}

/// The interface's windows, the Sim and their plumbob hidden for the grab (what was showing kept
/// to show again).
#[allow(clippy::type_complexity)]
fn hide(
    g: &mut Grabbing,
    roots: &mut Query<(Entity, &mut Visibility), (With<Node>, Without<ChildOf>)>,
    seen: &mut Query<&mut Visibility, Without<Node>>,
    sims: &Query<(&Transform, &Sim, &mut ActionQueue, Option<&crate::sim::PlumbobRef>)>,
) {
    for (e, mut v) in roots.iter_mut() {
        if Some(e) != g.root && !g.shades.contains(&e) && *v != Visibility::Hidden {
            g.hidden.push((e, *v));
            *v = Visibility::Hidden;
        }
    }
    // (A painter stays at the easel, in sight; only their plumbob's put away. Painting a portrait,
    // they and the easel are faded out of the way, as the game's `FadeOut`.)
    let bob = sims.get(g.sim).ok().and_then(|s| s.3.map(|p| p.0));
    let easel = match g.purpose {
        Purpose::Portrait { easel, .. } => Some(easel),
        _ => None,
    };
    let sim = (!matches!(g.kind, GrabKind::Painting(_)) || easel.is_some() || g.purpose == Purpose::Memorize).then_some(g.sim);
    for e in sim.into_iter().chain(bob).chain(easel) {
        if let Ok(mut v) = seen.get_mut(e)
            && *v != Visibility::Hidden
        {
            g.hidden.push((e, *v));
            *v = Visibility::Hidden;
        }
    }
}

/// The grab over (the picture taken or not): everything as it was, and the photo's action done.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn finish(
    commands: &mut Commands,
    g: &mut Grabbing,
    done: Option<bool>,
    cams: &mut Query<&mut crate::camera::SimsCamera>,
    sims: &mut Query<(&Transform, &Sim, &mut ActionQueue, Option<&crate::sim::PlumbobRef>)>,
    roots: &mut Query<(Entity, &mut Visibility), (With<Node>, Without<ChildOf>)>,
    seen: &mut Query<&mut Visibility, Without<Node>>,
    walls: &mut crate::building::WallMode,
) {
    let Some(taken) = done else { return };
    if let Some(w) = g.walls.take() {
        *walls = w;
    }
    for e in g.root.take().into_iter().chain(g.shades.drain(..)) {
        commands.entity(e).try_despawn();
    }
    // (First hidden first: what was showing before it was hidden again.)
    for (e, v) in g.hidden.drain(..).rev() {
        if let Ok((_, mut was)) = roots.get_mut(e) {
            *was = v;
        } else if let Ok(mut was) = seen.get_mut(e) {
            *was = v;
        }
    }
    if let Ok(mut cam) = cams.single_mut() {
        cam.photo = None;
        if let Some((focus, yaw, pitch, distance, height, follow)) = g.view.take() {
            (cam.focus, cam.yaw, cam.pitch, cam.distance, cam.height_offset, cam.follow) = (focus, yaw, pitch, distance, height, follow);
        }
    }
    // (A phone's photo done, a scene memorized; a still life's or portrait's painting begun, or,
    // its subject not captured, not; a portrait's sitter let go either way.)
    if let Ok((.., mut q, _)) = sims.get_mut(g.sim)
        && let Some(a) = q.0.front_mut()
        && a.label == g.label
    {
        match g.kind {
            GrabKind::Phone | GrabKind::Camera(_) => {
                q.0.pop_front();
            }
            GrabKind::Painting(_) if g.purpose == Purpose::Memorize => {
                q.0.pop_front();
            }
            GrabKind::Painting(_) if !taken => a.cancel = true,
            GrabKind::Painting(_) => {}
        }
    }
    if let Purpose::Portrait { paintee, .. } = g.purpose {
        release(commands, sims, paintee);
    }
    info!("photos: the grab's over ({})", if taken { "taken" } else { "put away" });
    commands.remove_resource::<Grabbing>();
}

/// A phone's photo into its taker's inventory: "Untitled", worth nothing. A still life's subject
/// to paint: the painting decided (its worth and quality as any of theirs, on the canvas its
/// frame was for, `CanvasSizeForScreenGrab`), its picture the subject in its style.
#[allow(clippy::type_complexity)]
fn photo_taken(
    mut commands: Commands,
    mut grabbed: MessageReader<Grabbed>,
    mut sims: Query<(&Sim, &mut crate::interact::Skills, Option<&crate::wishes::Wishes>, Option<&crate::journal::SkillJournal>, Option<&mut crate::photosubjects::PhotoRecords>)>,
    (baked, ui, clock): (Option<Res<crate::baked::Baked>>, Option<Res<crate::icons::GameUi>>, Res<crate::clock::GameClock>),
    (mut notes, mut life, mut sounds): (ResMut<crate::interact::Notifications>, MessageWriter<crate::life::LifeEvent>, MessageWriter<crate::sound::PlaySound>),
) {
    for g in grabbed.read() {
        match g.kind {
            GrabKind::Phone => {
                let photo = Photo { id: g.id.clone(), size: Size::Tiny, style: Style::Plain };
                crate::inventory::give(&mut commands, g.sim, ItemKind::Photo, photo.key(), UNTITLED.into(), 0, 0, 1);
            }
            // A camera's photo: its subject captured (`photosubjects`: new, or new today, worth
            // its skill points and simoleons, its collection more), worth what the camera, the
            // photographer, its size and mode make it (a spoilt one a tenth), titled for its
            // subject, and Photography for the taking.
            GrabKind::Camera(c) => {
                let Ok((sim, mut skills, _, _, records)) = sims.get_mut(g.sim) else { continue };
                let level = skills.level(SKILL);
                let eye = sim.traits.contains(&crate::life::Trait::PhotographersEye);
                let mut fresh = crate::photosubjects::PhotoRecords::default();
                let had = records.is_some();
                let rec: &mut crate::photosubjects::PhotoRecords = match records {
                    Some(r) => r.into_inner(),
                    None => &mut fresh,
                };
                let cap = ui.as_ref().map(|u| {
                    let subjects = crate::photosubjects::Subjects::new(&u.data.photo_subjects, &u.data.photo_collections);
                    let (key, names) = g.subject.clone().unwrap_or_else(|| ("Nothing".into(), Vec::new()));
                    crate::photosubjects::capture(&subjects, rec, &key, &names, (clock.minutes / 1440.0) as u32, rand::random())
                });
                let (value, multiplier, bonus) = cap.as_ref().map_or((0.0, 1.0, 0.0), |c| (c.value, c.multiplier, c.points));
                let worth = photo_value(c, level, g.size, g.style, (value, multiplier), g.spoilt.is_some(), eye);
                let title = cap.as_ref().map(|c| c.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| UNTITLED.into());
                let photo = Photo { id: g.id.clone(), size: g.size, style: g.style };
                crate::inventory::give(&mut commands, g.sim, ItemKind::Photo, photo.key(), title.clone(), 0, worth, 1);
                if !had {
                    commands.entity(g.sim).insert(fresh);
                }
                // (A spoilt one's sting: `PlayFailSting`; a collection complete's.)
                if g.spoilt.is_some() {
                    sounds.write(crate::sound::PlaySound::ui("ui_photo_sting_bad"));
                }
                if let Some(words) = cap.as_ref().and_then(|c| crate::photosubjects::note_words(c, worth)) {
                    if matches!(cap.as_ref().and_then(|c| c.note.as_ref()), Some(crate::photosubjects::Note::New { remaining: 0, .. })) {
                        sounds.write(crate::sound::PlaySound::ui("sting_obj_collect_piece_final"));
                    }
                    notes.push(words);
                }
                let points = (BASE_POINTS + bonus) * crate::life::skill_rate(&sim.traits, SKILL);
                if let Some(l) = skills.add_points(SKILL, points) {
                    notes.push(format!("{} reached level {l} in {SKILL}!", sim.first));
                    life.write(crate::life::LifeEvent::new(g.sim, crate::life::LifeEventKind::SkillUp { skill: SKILL, level: l }));
                }
                info!("photos: {} took a {} {} photo, \"{title}\" ({:?}), worth §{worth}, {points} points{}", sim.first, style_name(g.style), size_name(g.size), g.subject.as_ref().map(|s| &s.0), if g.spoilt.is_some() { " (spoilt)" } else { "" });
            }
            // (A scene memorized: kept in mind to paint from later, the last one forgotten.)
            GrabKind::Painting(_) if g.purpose == Purpose::Memorize => {
                commands.entity(g.sim).insert(Memorized { id: g.id.clone(), frame: g.frame.min(2) as u8 });
                info!("photos: a scene memorized ({})", g.id);
            }
            GrabKind::Painting(style) => {
                let Ok((sim, skills, wishes, journal, _)) = sims.get(g.sim) else { continue };
                let size = g.frame.min(2) as u8;
                let mut rng = rand::rng();
                let mut p = crate::paintings::paint(
                    baked.as_ref().map(|b| &b.0.paintings),
                    size,
                    skills.level("Painting"),
                    &sim.traits,
                    sim.age == Age::Child,
                    crate::wishes::has(wishes, "ExtraCreative"),
                    crate::journal::earned(journal, "Proficient Painter"),
                    &mut rng,
                );
                p.key = still_key(size, style, &g.id, rand::Rng::random(&mut rng));
                info!("photos: {} paints {} ({}, {})", sim.first, if matches!(g.purpose, Purpose::Portrait { .. }) { "a portrait" } else { "a still life" }, style.name(), p.key);
                commands.entity(g.sim).insert(crate::paintings::PaintPlan { size, painted: Some(p) });
            }
        }
    }
}

/// A camera on the lot put in a Sim's inventory ("Put In Inventory", the game's
/// `PutInInventory`), worth what it's worth now.
pub fn pocket(w: &mut World, sim: Entity, camera: Entity) {
    let Some(o) = w.get::<crate::interact::GameObject>(camera).cloned() else { return };
    let lost = w.get::<crate::interact::Depreciated>(camera);
    let worth = crate::interact::object_value(&o, lost, false, None);
    let Some(key) = w.resource::<crate::baked::Baked>().0.catalog.iter().find(|c| c.objd == o.objd).map(|c| c.instance_name.clone()) else { return };
    w.entity_mut(camera).despawn();
    let Ok(mut s) = w.get_entity_mut(sim) else { return };
    if s.get::<crate::inventory::Inventory>().is_none() {
        s.insert(crate::inventory::Inventory::default());
    }
    if let Some(mut inv) = s.get_mut::<crate::inventory::Inventory>() {
        inv.put(Stack { kind: ItemKind::Object, key, name: o.name.clone(), quality: 0, count: 1, worth });
    }
    info!("photos: {} put in an inventory", o.name);
}

/// The game's words for a camera's menu in the inventory (`Camera:...`).
const SETTINGS: &str = "Settings...";
const SIZE_MENU: &str = "Set Default Size...";
const STYLE_MENU: &str = "Set Default Mode...";
const TOO_LOW: &str = "Your Photography Skill level is too low to take photos of this size.";
const CURRENT: &str = "This is the current setting.";

/// What a camera in the inventory offers (`Camera`'s inventory interactions): "Take Photo",
/// its "Settings..." (the default size and mode, the ones out of the photographer's reach or
/// already chosen greyed as the game greys them), and selling or erasing the photos it's taken.
/// Its entries, and its submenus (name, entries) by their place after `first`.
pub fn camera_menu(
    me: Entity,
    st: &Stack,
    level: u32,
    settings: Option<&CameraSettings>,
    photos: &[Stack],
    first: usize,
) -> Option<(Vec<(String, ActionKind)>, Vec<(String, Vec<(String, ActionKind)>)>)> {
    use crate::inventory::ItemButton;
    let c = camera(&st.key)?;
    let (size, style) = settings.and_then(|s| s.0.get(c.name)).copied().unwrap_or((Size::Small, Style::Plain));
    let item = |b| ActionKind::InventoryItem(b, me, st.clone());
    let sizes: Vec<(String, ActionKind)> = SIZE_UNLOCKS
        .iter()
        .filter(|(s, _)| *s as u8 <= c.max as u8)
        .map(|(s, l)| {
            let kind = if level < *l { ActionKind::Greyed(TOO_LOW.into()) } else if *s == size { ActionKind::Greyed(CURRENT.into()) } else { item(ItemButton::CameraSize(*s as u8)) };
            (size_name(*s).to_string(), kind)
        })
        .collect();
    let styles: Vec<(String, ActionKind)> = STYLE_UNLOCKS
        .iter()
        .filter(|(_, l)| level >= *l)
        .map(|(s, _)| (style_name(*s).to_string(), if *s == style { ActionKind::Greyed(CURRENT.into()) } else { item(ItemButton::CameraStyle(*s as u8)) }))
        .collect();
    let settings = vec![(format!("{SIZE_MENU} ›"), crate::hud::submenu_kind(first + 1)), (format!("{STYLE_MENU} ›"), crate::hud::submenu_kind(first + 2))];
    let mut options = vec![(TAKE_PHOTO.to_string(), take_photo_with(c)), (format!("{SETTINGS} ›"), crate::hud::submenu_kind(first))];
    let (n, worth) = camera_photos(photos);
    if n > 0 {
        options.push((format!("Sell All Photos (§{worth})..."), item(ItemButton::SellPhotos)));
        options.push(("Erase All Photos...".to_string(), item(ItemButton::ErasePhotos)));
    }
    Some((options, vec![(SETTINGS.trim_end_matches('.').to_string(), settings), (SIZE_MENU.trim_end_matches('.').to_string(), sizes), (STYLE_MENU.trim_end_matches('.').to_string(), styles)]))
}

/// The photos a camera's taken (theirs not a phone's): how many, and what they're worth.
pub fn camera_photos(stacks: &[Stack]) -> (u32, i64) {
    stacks.iter().filter(|s| is_camera_photo(s)).fold((0, 0), |(n, w), s| (n + s.count, w + s.worth))
}

fn is_camera_photo(s: &Stack) -> bool {
    s.kind == ItemKind::Photo && Photo::from_key(&s.key).is_some_and(|p| p.size != Size::Tiny)
}

/// A camera's default size or mode set (`DefaultSize`, `DefaultStyle`).
pub fn set_default(w: &mut World, sim: Entity, camera_key: &str, size: Option<u8>, style: Option<u8>) {
    let Some(c) = camera(camera_key) else { return };
    let Ok(mut e) = w.get_entity_mut(sim) else { return };
    if e.get::<CameraSettings>().is_none() {
        e.insert(CameraSettings::default());
    }
    let Some(mut s) = e.get_mut::<CameraSettings>() else { return };
    let now = s.0.entry(c.name).or_insert((Size::Small, Style::Plain));
    if let Some(z) = size.and_then(|z| Size::ALL.get(z as usize)) {
        now.0 = *z;
    }
    if let Some(y) = style.and_then(|y| Style::ALL.get(y as usize)) {
        now.1 = *y;
    }
    info!("photos: {} set to {} {}", c.name, size_name(now.0), style_name(now.1));
}

/// "Sell All Photos" or "Erase All Photos": the game's question first ("Are you sure you want
/// to sell 3 photos for §45?").
pub fn ask_photos(commands: &mut Commands, sim: Entity, photos: &[Stack], sell: bool) {
    let (n, worth) = camera_photos(photos);
    let text = if sell { format!("Are you sure you want to sell {n} photos for §{worth}?") } else { format!("Are you sure you want to erase {n} photos?  They will be gone forever.") };
    commands.queue(move |w: &mut World| {
        let answer = |l: &str| crate::dialog::Answer { label: l.into(), detail: String::new(), icon: String::new() };
        w.resource_mut::<crate::dialog::Questions>().ask(crate::dialog::Ask {
            about: crate::dialog::Question::CameraPhotos { sim, sell },
            icon: String::new(),
            heading: String::new(),
            title: String::new(),
            text,
            answers: vec![answer("Yes"), answer("No")],
        });
    });
}

/// A camera's photos sold for what they're worth, or erased: told as the game tells it ("3 photos
/// sold for §45.").
fn photos_gone(
    mut answers: MessageReader<crate::dialog::Answered>,
    mut invs: Query<&mut crate::inventory::Inventory>,
    mut household: Option<ResMut<crate::interact::Household>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    for a in answers.read() {
        let crate::dialog::Question::CameraPhotos { sim, sell } = a.about else { continue };
        if a.answer != 0 {
            continue;
        }
        let Ok(mut inv) = invs.get_mut(sim) else { continue };
        let (n, worth) = camera_photos(&inv.0);
        inv.0.retain(|s| !is_camera_photo(s));
        if sell {
            if let Some(h) = household.as_mut() {
                h.funds += worth;
            }
            notes.push(format!("{n} photos sold for §{worth}."));
        } else {
            notes.push(format!("{n} photos erased."));
        }
    }
}

/// "Name...": the game's box to give a photograph a title.
pub fn ask_title(commands: &mut Commands, sim: Entity, s: &Stack) {
    let (key, suggested) = (s.key.clone(), s.name.clone());
    commands.queue(move |w: &mut World| {
        w.resource_mut::<crate::dialog::Questions>().ask(crate::dialog::Ask {
            about: crate::dialog::Question::NamePhoto { sim, key, suggested },
            icon: String::new(),
            heading: String::new(),
            title: NAME_TITLE.into(),
            text: String::new(),
            answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
        });
    });
}

/// A photograph titled: its stack in their inventory renamed.
fn titled(mut answers: MessageReader<crate::dialog::Answered>, typed: Res<crate::dialog::Typed>, mut invs: Query<&mut crate::inventory::Inventory>) {
    for a in answers.read() {
        let crate::dialog::Question::NamePhoto { sim, key, .. } = &a.about else { continue };
        let t: String = typed.0.trim().chars().take(TITLE_LENGTH).collect();
        if a.answer != 0 || t.is_empty() {
            continue;
        }
        let Ok(mut inv) = invs.get_mut(*sim) else { continue };
        for s in inv.0.iter_mut().filter(|s| s.kind == ItemKind::Photo && s.key == *key) {
            s.name = t.clone();
        }
        info!("photos: titled {t}");
    }
}

/// A photograph's frame (its catalogue object), for its size.
pub fn frame(baked: &crate::baked::BakedData, s: &Stack) -> Option<Key> {
    let p = Photo::from_key(&s.key)?;
    baked.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(p.size.frame())).map(|c| c.objd)
}

/// A photograph's picture (loaded once), for its tile and its frame.
pub fn image(cache: &mut crate::paintings::PaintingImages, images: &mut Assets<Image>, s: &Stack) -> Option<Handle<Image>> {
    let p = Photo::from_key(&s.key)?;
    picture(cache, images, &p.id).map(|(h, _)| h)
}

/// A picture taken (a photo's, a still life's), by its id: loaded once, and its size.
pub fn picture(cache: &mut crate::paintings::PaintingImages, images: &mut Assets<Image>, id: &str) -> Option<(Handle<Image>, UVec2)> {
    let key: Key = (0x5048_4F54, 0, s3pkg::fnv64(id));
    cache
        .0
        .entry(key)
        .or_insert_with(|| {
            let png = std::fs::read(picture_path(&id)).ok()?;
            let (w, h, px) = s3bake::gamedata::decode_icon(&png)?;
            let img = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
            Some((images.add(img), UVec2::new(w, h)))
        })
        .clone()
}

/// A still life's canvas on an easel, showing its picture.
#[derive(Component)]
pub struct StillCanvas(pub String);

/// A canvas's face for a still life: the canvas's own colour, the picture where the game's
/// pictures go on it (its `uv`, fractions of the face).
pub fn canvas_face(pic: &[u8], w: u32, h: u32, uv: [f32; 4]) -> (u32, Vec<u8>) {
    const SIZE: u32 = 512;
    let mut out: Vec<u8> = [232u8, 226, 212, 255].repeat((SIZE * SIZE) as usize);
    let (x0, y0, x1, y1) = ((uv[0] * SIZE as f32) as u32, (uv[1] * SIZE as f32) as u32, (uv[2] * SIZE as f32).ceil() as u32, (uv[3] * SIZE as f32).ceil() as u32);
    let (x1, y1) = (x1.min(SIZE).max(x0 + 1), y1.min(SIZE).max(y0 + 1));
    // (Smoothly: each pixel blended from the four of the picture about it.)
    let at = |x: u32, y: u32, k: usize| pic[((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize + k] as f32;
    for y in y0..y1 {
        for x in x0..x1 {
            let fx = (((x - x0) as f32 + 0.5) / (x1 - x0) as f32 * w as f32 - 0.5).max(0.0);
            let fy = (((y - y0) as f32 + 0.5) / (y1 - y0) as f32 * h as f32 - 0.5).max(0.0);
            let (ix, iy, tx, ty) = (fx as u32, fy as u32, fx.fract(), fy.fract());
            let d = ((y * SIZE + x) * 4) as usize;
            for k in 0..4 {
                let top = at(ix, iy, k) * (1.0 - tx) + at(ix + 1, iy, k) * tx;
                let bottom = at(ix, iy + 1, k) * (1.0 - tx) + at(ix + 1, iy + 1, k) * tx;
                out[d + k] = (top * (1.0 - ty) + bottom * ty).round() as u8;
            }
        }
    }
    (SIZE, out)
}

/// A photograph's frame (or a still life's canvas) showing its picture.
#[derive(Component)]
struct PhotoShown;

/// Photographs' frames drawn in their wall state; hung photographs and still lifes, one held up
/// in Buy mode, and a still life's canvas on the easel, with their pictures in them.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn frames(
    mut commands: Commands,
    hung: Query<(Entity, &crate::paintings::Hung, &crate::interact::GameObject), Without<PhotoShown>>,
    easels: Query<(Entity, &StillCanvas), Without<PhotoShown>>,
    buy: Res<crate::buy::BuyMode>,
    kids: Query<&Children>,
    mut parts: Query<&mut MeshMaterial3d<StandardMaterial>>,
    data: Option<Res<crate::baked::Baked>>,
    (mut assets, mut cache): (ResMut<crate::objects::ObjectAssets>, ResMut<crate::paintings::PaintingImages>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut stand_ins: Local<HashMap<Key, Vec<Handle<StandardMaterial>>>>,
    mut pictures: Local<HashMap<String, Handle<StandardMaterial>>>,
) {
    let Some(data) = data else { return };
    // (Once: each size's photograph frame in its wall state, and the stand-ins for the pictures
    // in the frames and on the canvases.)
    if stand_ins.is_empty() {
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        for size in Size::ALL {
            let Some(objd) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(size.frame())).map(|c| c.objd) else { continue };
            let state = assets.object_in_state(&mut ctx, objd, s3pkg::fnv32(ON_WALL));
            let state = if state.iter().any(|p| p.tex == Some(PLACEHOLDER)) { state } else { assets.object(&mut ctx, objd) };
            stand_ins.insert(objd, state.iter().filter(|p| p.tex == Some(PLACEHOLDER)).map(|p| p.material.clone()).collect());
            assets.set_object(objd, state);
        }
        let face = data.0.paintings.face;
        for c in &data.0.paintings.canvases {
            let faces = assets.object(&mut ctx, c.objd).iter().filter(|p| p.tex.is_some_and(|k| k.0 == face.0 && k.2 == face.2)).map(|p| p.material.clone()).collect();
            stand_ins.insert(c.objd, faces);
        }
        stand_ins.entry((0, 0, 0)).or_default();
    }
    // (What to show where: the entity, its object, the picture (a photo's or a still life's), and
    // whether it's for good.)
    let canvas = |size: u8| data.0.paintings.canvases.get(size as usize).map(|c| (c.objd, c.uv));
    let mut todo: Vec<(Entity, Key, Stack, bool)> = hung.iter().filter(|(_, h, _)| shows_own(&h.0)).map(|(e, h, o)| (e, o.objd, h.0.clone(), true)).collect();
    if let Some(p) = buy.placing.as_ref()
        && let Some(i) = p.item.as_ref().filter(|i| shows_own(i))
    {
        todo.push((p.ghost, p.objd, i.clone(), false));
    }
    for (e, c) in &easels {
        let Some((objd, _)) = still(&c.0).and_then(|(size, ..)| canvas(size)) else { continue };
        todo.push((e, objd, Stack { kind: ItemKind::Painting, key: c.0.clone(), name: String::new(), quality: 0, count: 1, worth: 0 }, true));
    }
    for (e, objd, s, mark) in todo {
        let Some(stand_in) = stand_ins.get(&objd).cloned().filter(|v| !v.is_empty()) else { continue };
        // (Its material, made once.)
        let (name, id, uv) = match (Photo::from_key(&s.key), still(&s.key)) {
            (Some(p), _) => (format!("photo:{}", p.id), p.id, None),
            (None, Some((size, _, id))) => (format!("still:{size}:{id}"), id, canvas(size).map(|c| c.1)),
            _ => continue,
        };
        let picture = match pictures.get(&name) {
            Some(m) => m.clone(),
            None => {
                let img = match uv {
                    // (A still life: on a canvas's face, where the game's pictures go.)
                    Some(uv) => {
                        let Some((w, h, px)) = std::fs::read(picture_path(&id)).ok().and_then(|png| s3bake::gamedata::decode_icon(&png)) else { continue };
                        let (size, face) = canvas_face(&px, w, h, uv);
                        images.add(Image::new(Extent3d { width: size, height: size, depth_or_array_layers: 1 }, TextureDimension::D2, face, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD))
                    }
                    None => {
                        let Some((h, _)) = picture(&mut cache, &mut images, &id) else { continue };
                        h
                    }
                };
                let m = mats.add(StandardMaterial { base_color_texture: Some(img), perceptual_roughness: if uv.is_some() { 0.8 } else { 0.45 }, reflectance: 0.35, ..default() });
                pictures.insert(name, m.clone());
                m
            }
        };
        let Ok(children) = kids.get(e) else { continue };
        let mut swapped = false;
        for c in children.iter() {
            if let Ok(mut m) = parts.get_mut(c) {
                if stand_in.contains(&m.0) {
                    m.0 = picture.clone();
                    swapped = true;
                } else if m.0 == picture {
                    swapped = true;
                }
            }
        }
        if swapped && mark {
            commands.entity(e).insert(PhotoShown);
            debug!("photos: {} shows its picture", s.key);
        }
    }
}

/// Whether an item hangs with a picture of its own (a photograph, a still life).
fn shows_own(s: &Stack) -> bool {
    (s.kind == ItemKind::Photo && Photo::from_key(&s.key).is_some()) || (s.kind == ItemKind::Painting && still(&s.key).is_some())
}

/// `PHOTO_TEST=1` (tests): ten seconds in, the selected Sim takes a photo with their phone
/// (`PHOTO_CAPTURE_AT=<seconds>`: taken that long into the grab; `PHOTO_YAW`, `PHOTO_PITCH`,
/// `PHOTO_ZOOM`: which way and how far zoomed); `PHOTO_HANG=1`: taken, it's hung on the nearest
/// wall and the camera's on it; `PHOTO_PIE=1`: its inventory menu instead; `PHOTO_PHONE=1`: just
/// the phone's menu; `PHOTO_STILL=basic|genius|insane|somber`: a still life at the lot's easel
/// instead (`PHOTO_LEVEL`, `PHOTO_FRAME=<0-2>`, `PHOTO_DIST`; `PHOTO_FAST=1`: its painting
/// hurried), hung likewise once done.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn photo_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(Entity, &Transform, &mut ActionQueue, Option<&crate::inventory::Inventory>, &mut crate::interact::Skills, Option<&crate::paintings::PaintPlan>), With<Selected>>,
    objects: Query<(Entity, &crate::interact::GameObject, &Transform), Without<Selected>>,
    grabbing: Option<Res<Grabbing>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
    (world, mut wall_mode, grid, mut put_down): (Res<crate::loading::CurrentWorld>, ResMut<crate::building::WallMode>, Option<Res<crate::nav::NavGrid>>, Local<bool>),
    (others, memorized): (Query<Entity, (With<Sim>, With<crate::sim::HouseholdMember>, Without<Selected>)>, Query<(), With<Memorized>>),
) {
    if std::env::var_os("PHOTO_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let Ok((me, tf, mut q, inv, mut skills, plan)) = sims.single_mut() else { return };
    let still_life = std::env::var("PHOTO_STILL").ok();
    match stage.0 {
        0 if now > 10.0 => {
            *stage = (1, now);
            // (PHOTO_PHONE=1: the phone's menu instead.)
            if std::env::var_os("PHOTO_PHONE").is_some() {
                *stage = (9, now);
                commands.insert_resource(crate::hud::OpenPhone);
                return;
            }
            // (PHOTO_CAMERA=<camera>: one in their inventory, at Photography `PHOTO_LEVEL`, its
            // default size and mode `PHOTO_SIZE`, `PHOTO_STYLE` (numbers); a photo taken with
            // it, or with PHOTO_PIE=1 its menu, or with PHOTO_PLACE=1 it's put on the lot and
            // picked up.)
            if let Some(c) = std::env::var("PHOTO_CAMERA").ok().and_then(|n| camera(&n)) {
                let level = std::env::var("PHOTO_LEVEL").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                skills.0.insert(SKILL, level);
                let size = std::env::var("PHOTO_SIZE").ok().and_then(|v| v.parse::<u8>().ok());
                let style = std::env::var("PHOTO_STYLE").ok().and_then(|v| v.parse::<u8>().ok());
                let name = data.0.catalog.iter().find(|x| x.instance_name.eq_ignore_ascii_case(c.name)).map(|x| (x.objd, x.name.clone(), x.price));
                let Some((objd, name, price)) = name else { return };
                if std::env::var_os("PHOTO_PLACE").is_some() {
                    let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 1.0);
                    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                    if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, objd, at, 0.0) {
                        commands.entity(o.entity).insert(crate::save::Bought);
                        if let Some(def) = crate::interact::interactions_for(crate::interact::ObjectKind::Camera).iter().position(|d| d.special == crate::interact::Special::PocketCamera) {
                            q.push_player(Action::new(crate::petpresent::PUT_IN_INVENTORY, ActionKind::Object { target: o.entity, def }, false));
                        }
                    }
                    *stage = (9, now);
                    return;
                }
                crate::inventory::give(&mut commands, me, ItemKind::Object, c.name.to_string(), name, 0, price as i64, 1);
                let key = c.name;
                commands.queue(move |w: &mut World| set_default(w, me, key, size, style));
                if std::env::var_os("PHOTO_PIE").is_some() {
                    *stage = (8, now);
                    return;
                }
                q.push_player(Action::new(TAKE_PHOTO, take_photo_with(c), false));
                info!("photo test: taking a photo with the {}", c.name);
                return;
            }
            // (PHOTO_STILL=<style>: a still life at the lot's easel instead, at Painting
            // `PHOTO_LEVEL`.)
            if let Some(style) = still_life.as_deref() {
                let easel = objects.iter().filter(|(_, o, _)| o.kind == crate::interact::ObjectKind::Easel).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation))).map(|e| e.0);
                let Some(easel) = easel else {
                    // (None on the lot: one put down beside them, a moment to settle.)
                    *stage = (0, now);
                    if !*put_down {
                        *put_down = true;
                        let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("Easel")).map(|c| c.objd) else { return };
                        let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
                        let here = tf.translation.xz();
                        let spot = (3..20).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32 * 0.5)).find(|c| (-4..=4).all(|i| (-4..=4).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.4)))).unwrap_or(here + Vec2::X * 2.0);
                        let y = crate::building::walk_height(&world.data, building.as_deref(), Vec3::new(spot.x, tf.translation.y, spot.y));
                        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                        if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, Vec3::new(spot.x, y, spot.y), 0.0) {
                            commands.entity(o.entity).insert(crate::save::Bought);
                            info!("photo test: an easel put down");
                        }
                    }
                    return;
                };
                let level = std::env::var("PHOTO_LEVEL").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(6.0);
                skills.0.insert("Painting", level);
                // (PHOTO_STILL=menu: the easel's menu.)
                if style == "menu" {
                    commands.insert_resource(crate::hud::PieFor(easel));
                    *stage = (9, now);
                    return;
                }
                let def = crate::interact::interactions_for(crate::interact::ObjectKind::Easel).iter().position(|d| d.special == crate::interact::Special::SellPainting).unwrap_or(0);
                // (PHOTO_STILL=portrait: one of the household sitting for a portrait; =memory: a
                // scene memorized with the phone, then painted from memory.)
                if style == "portrait" {
                    let Some(paintee) = others.iter().next() else { return };
                    commands.queue(move |w: &mut World| sit_for_portrait(w, me, paintee, easel));
                    info!("photo test: painting a portrait");
                    return;
                }
                if style == "memory" {
                    q.push_player(Action::new(MEMORIZE, take_photo(me), false));
                    *stage = (7, now);
                    info!("photo test: memorizing a scene");
                    return;
                }
                let label = PaintStyle::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(style)).filter(|s| *s != PaintStyle::Basic).map_or(STILL_LIFE, |s| s.name());
                q.push_player(Action::new(label, ActionKind::Object { target: easel, def }, false));
                info!("photo test: painting a still life ({label})");
                return;
            }
            q.push_player(Action::new(TAKE_PHOTO, take_photo(me), false));
            info!("photo test: taking a photo");
        }
        // (A scene memorized: painted from memory at the easel.)
        7 if grabbing.is_none() && memorized.contains(me) && now - stage.1 > 2.0 => {
            let easel = objects.iter().find(|(_, o, _)| o.kind == crate::interact::ObjectKind::Easel).map(|e| e.0);
            let def = crate::interact::interactions_for(crate::interact::ObjectKind::Easel).iter().position(|d| d.special == crate::interact::Special::SellPainting).unwrap_or(0);
            if let Some(easel) = easel {
                q.push_player(Action::new(FROM_MEMORY, ActionKind::Object { target: easel, def }, false));
                info!("photo test: painting from memory");
            }
            *stage = (1, now);
        }
        // (The camera's menu in the inventory.)
        8 if now - stage.1 > 1.0 => {
            let Some(i) = inv.and_then(|i| i.0.iter().position(|s| camera(&s.key).is_some())) else { return };
            *stage = (9, now);
            commands.insert_resource(crate::hudpanels::StackPie(i));
            info!("photo test: the camera's menu");
        }
        // (A still life's painting hurried along, once it's begun.)
        1 if still_life.is_some()
            && grabbing.is_none()
            && let Some(p) = plan.filter(|p| p.painted.as_ref().is_some_and(|x| still(&x.key).is_some()))
            && now - stage.1 > 2.0 =>
        {
            let size = p.size;
            if let Some(a) = q.0.front_mut()
                && let Phase::Running(t) = a.phase
                && t < 1.0
                && std::env::var_os("PHOTO_FAST").is_some()
            {
                a.phase = Phase::Running(crate::paintings::CANVAS_MINUTES[size as usize] * 0.9);
                *stage = (3, now);
                info!("photo test: hurried along");
            }
        }
        1 | 3 if grabbing.is_none() && now - stage.1 > 2.0 => {
            let want = |s: &&Stack| if still_life.is_some() { s.kind == ItemKind::Painting && still(&s.key).is_some() } else { s.kind == ItemKind::Photo };
            let Some(s) = inv.and_then(|i| i.0.iter().find(want)).cloned() else { return };
            *stage = (2, now);
            info!("photo test: took {} ({})", s.name, s.key);
            if std::env::var_os("PHOTO_PIE").is_some() {
                let i = inv.and_then(|i| i.0.iter().position(|x| x.key == s.key)).unwrap_or(0);
                commands.insert_resource(crate::hudpanels::StackPie(i));
                return;
            }
            if std::env::var_os("PHOTO_HANG").is_none() {
                return;
            }
            let object = if s.kind == ItemKind::Photo { frame(&data.0, &s).map(|o| (o, None)) } else { crate::paintings::object(&data.0.paintings, &s).map(|(o, d)| (o, Some(d))) };
            let (Some(b), Some((objd, design))) = (building, object) else { return };
            // (On the nearest straight wall with nothing behind.)
            let here = b.local(tf.translation);
            let y = b.levels.get(1).copied().unwrap_or(0.0);
            let mut walls: Vec<&s3bake::types::WallBaked> = b.data.walls.iter().filter(|w| w.level.max(1) == 1 && ((w.a[0] - w.b[0]).abs() < 0.01 || (w.a[1] - w.b[1]).abs() < 0.01)).collect();
            let dist = |w: &&s3bake::types::WallBaked| {
                let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
                here.distance((a + c) / 2.0)
            };
            walls.sort_by(|x, z| dist(x).total_cmp(&dist(z)));
            let spot = walls.iter().find_map(|w| {
                let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
                let n = (c - a).normalize().perp();
                [1.0, -1.0].into_iter().find_map(|side| {
                    let lp = (a + c) / 2.0 + n * 0.4 * side;
                    let ray = Ray3d::new(b.world(lp.x, lp.y, y) + Vec3::Y * 5.0, Dir3::NEG_Y);
                    crate::build::snap_to_wall(&b, ray, 1).filter(|(pos, rot, _)| !b.opening_behind(*pos, *rot, 1)).map(|(p, r, _)| (p, r))
                })
            });
            let Some((pos, rot)) = spot else { return };
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, objd, pos, rot, design) {
                commands.entity(o.entity).insert((crate::save::Bought, crate::paintings::Hung(s.clone())));
                info!("photo test: hung at {pos:.1?}");
            }
            // (The camera before it, at its height, the walls up.)
            *wall_mode = crate::building::WallMode::Up;
            if let Ok(mut c) = cams.single_mut() {
                let fwd = rot * Vec3::Z;
                c.look_at(pos - fwd * 0.35);
                c.yaw = fwd.x.atan2(fwd.z);
                c.pitch = 0.08;
                c.distance = 1.6;
                c.height_offset = pos.y - world.data.heightmap.sample(pos.x, pos.z) + 1.62;
            }
        }
        _ => {}
    }
}

/// `PHOTO_MENU=<label>[><label>...]` (tests): once a menu's up, the submenus starting so opened
/// one after another, a second apart (into a camera's settings, say).
fn photo_menu_test(mut commands: Commands, time: Res<Time>, mut pie: ResMut<crate::hud::PieMenu>, mut step: Local<(usize, f32)>) {
    let Ok(path) = std::env::var("PHOTO_MENU") else { return };
    let now = time.elapsed_secs();
    let Some(want) = path.split('>').nth(step.0) else { return };
    if pie.root.is_none() || now - step.1 < 1.0 {
        return;
    }
    let Some(k) = pie.options.iter().position(|(l, _)| l.starts_with(want)) else { return };
    let label = pie.options[k].0.clone();
    if crate::hud::choose_submenu(&mut commands, &mut pie, k) {
        info!("photo test: chose {label}");
        *step = (step.0 + 1, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_photo_keeps_its_picture_size_and_style() {
        let p = Photo { id: "00ab12cd34ef5678".into(), size: Size::Tiny, style: Style::Sepia };
        assert_eq!(p.key(), "photo:0:1:00ab12cd34ef5678");
        assert_eq!(Photo::from_key(&p.key()), Some(p.clone()));
        assert_eq!(Photo::from_key("photo:9:0:abc"), None);
        assert_eq!(Photo::from_key("photo:0:0:../x"), None, "no way out of the photos' folder");
        assert_eq!(Size::Tiny.frame(), "PhotographFrameSmall");
        assert_eq!(Size::Small.frame(), "PhotographFrameB");
        assert!(picture_path(&p.id).ends_with("photos/00ab12cd34ef5678.png") || picture_path(&p.id).ends_with("photos\\00ab12cd34ef5678.png"));
    }

    #[test]
    fn a_cameras_shots_cost_and_are_worth_as_the_games() {
        let [cheap, medium, dear] = &CAMERAS;
        assert_eq!(camera("cameraexpensive"), Some(dear));
        assert_eq!(camera("CameraUnderwater"), None);
        // (`kBaseCostToTakeSkillPhoto`, the size's and style's `SkillCost`, the camera's
        // `AdditionalCostToTakePhoto`.)
        assert_eq!(shot_cost(cheap, Size::Small, Style::Plain), 10 + 6 + 1);
        assert_eq!(shot_cost(dear, Size::Panorama, Style::Vignette), 10 + 35 + 7 + 12);
        // (Five simoleons' base, by the camera, the level, size and style; a spoilt one a tenth;
        // a Photographer's Eye's a quarter more.)
        assert_eq!(photo_value(dear, 10, Size::Panorama, Style::Vignette, (0.0, 1.0), false, false), 45);
        assert_eq!(photo_value(medium, 5, Size::Medium, Style::Plain, (20.0, 1.0), false, false), 27);
        assert_eq!(photo_value(medium, 5, Size::Medium, Style::Plain, (20.0, 1.0), true, false), 2);
        assert_eq!(photo_value(medium, 5, Size::Medium, Style::Plain, (20.0, 1.0), false, true), 34);
        // (What the skill and the camera allow.)
        assert_eq!(sizes_at(0, dear).collect::<Vec<_>>(), vec![Size::Small]);
        assert_eq!(sizes_at(5, dear).collect::<Vec<_>>(), vec![Size::Small, Size::Medium, Size::Large]);
        assert_eq!(sizes_at(10, cheap).collect::<Vec<_>>(), vec![Size::Small]);
        assert_eq!(sizes_at(10, medium).collect::<Vec<_>>(), vec![Size::Small, Size::Medium]);
        assert_eq!(styles_at(4).collect::<Vec<_>>(), vec![Style::Plain, Style::Gray, Style::Sepia]);
        assert_eq!(styles_at(8).count(), 4);
        // (Each size's frame, for 4:3 and widescreen.)
        assert_eq!((frame_for(Size::Small, false), frame_for(Size::Small, true), frame_for(Size::Panorama, true)), (0, 1, 7));
    }

    #[test]
    fn green_photographers_spoil_shots() {
        // (Three in four at level nought, never at ten; most often a finger or a blur.)
        assert!(Spoilt::flip(0, 0.7, 0.5).is_some());
        assert_eq!(Spoilt::flip(0, 0.8, 0.5), None);
        assert_eq!(Spoilt::flip(10, 0.0, 0.5), None);
        assert_eq!(Spoilt::flip(3, 0.0, 0.0), Some(Spoilt::Gnome));
        assert_eq!(Spoilt::flip(3, 0.0, 0.3), Some(Spoilt::Finger));
        assert_eq!(Spoilt::flip(3, 0.0, 0.7), Some(Spoilt::Blurry));
        assert_eq!(Spoilt::flip(3, 0.0, 0.99), Some(Spoilt::Noise));
        // (A finger over the corner of the lens.)
        let mut px = vec![0u8; 20 * 20 * 4];
        Spoilt::Finger.spoil(&mut px, 20, 20, 7);
        assert!(px[(19 * 20 + 1) * 4] > 100, "the corner's pink");
        assert_eq!(px[(1 * 20 + 18) * 4], 0, "the far corner's clear");
    }

    #[test]
    fn a_cameras_menu_and_settings() {
        let me = Entity::from_bits(7);
        let cam = Stack { kind: ItemKind::Object, key: "CameraMedium".into(), name: "ChannonTec Outlaw SE".into(), quality: 0, count: 1, worth: 850 };
        let photo = Stack { kind: ItemKind::Photo, key: Photo { id: "ab".into(), size: Size::Medium, style: Style::Plain }.key(), name: UNTITLED.into(), quality: 0, count: 2, worth: 30 };
        let phone = Stack { kind: ItemKind::Photo, key: Photo { id: "cd".into(), size: Size::Tiny, style: Style::Plain }.key(), name: UNTITLED.into(), quality: 0, count: 1, worth: 0 };
        let mut settings = CameraSettings::default();
        settings.0.insert("CameraMedium", (Size::Small, Style::Plain));
        let (options, menus) = camera_menu(me, &cam, 1, Some(&settings), &[cam.clone(), photo, phone], 3).unwrap();
        let labels: Vec<&str> = options.iter().map(|o| o.0.as_str()).collect();
        assert_eq!(labels, vec![TAKE_PHOTO, "Settings... ›", "Sell All Photos (§30)...", "Erase All Photos..."]);
        assert!(matches!(options[0].1, ActionKind::TakePhoto { camera: Some("CameraMedium") }));
        // (Sizes as far as the camera goes, the one set and those out of reach greyed.)
        let sizes = &menus[1].1;
        assert_eq!(sizes.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), vec!["Small Portrait", "Medium Landscape"]);
        assert!(matches!(&sizes[0].1, ActionKind::Greyed(w) if w == CURRENT));
        assert!(matches!(&sizes[1].1, ActionKind::Greyed(w) if w == TOO_LOW));
        assert_eq!(menus[2].1.len(), 1, "just colour at level one");
        assert!(camera_menu(me, &photo_stack(), 1, None, &[], 0).is_none());
        // (Kept in saves.)
        let saved = settings.to_saved();
        assert_eq!(saved, vec![("CameraMedium".to_string(), 1, 0)]);
        assert_eq!(CameraSettings::from_saved(&saved).0, settings.0);
    }

    fn photo_stack() -> Stack {
        Stack { kind: ItemKind::Photo, key: "photo:1:0:ab".into(), name: UNTITLED.into(), quality: 0, count: 1, worth: 0 }
    }

    #[test]
    fn portraits_and_memories_wait_on_their_subjects() {
        assert!(needs_subject(PORTRAIT) && needs_subject(FROM_MEMORY) && needs_subject(STILL_LIFE) && needs_subject("Genius"));
        assert!(!needs_subject("Paint: Small") && !needs_subject(POSE));
        // (The sitter before the easel, across it from the painter.)
        let spot = pose_spot(Vec3::new(5.0, 0.0, 5.0), Vec2::new(5.0, 4.0));
        assert!((spot - Vec2::new(5.0, 6.6)).length() < 1e-4);
        assert_eq!((PORTRAIT_LEVEL, MEMORY_LEVEL), (7, 8));
        assert!(portrait_height(Age::Adult) > portrait_height(Age::Child));
    }

    #[test]
    fn still_lifes_come_with_skill_and_traits() {
        use crate::life::Trait;
        let key = still_key(2, PaintStyle::Somber, "00aa11bb22cc33dd", 7);
        assert_eq!(key, "painting:2:still-3-00aa11bb22cc33dd#7");
        assert_eq!(still(&key), Some((2, PaintStyle::Somber, "00aa11bb22cc33dd".into())));
        assert_eq!(still("painting:1:SomePicture#3"), None, "the game's own pictures aren't still lifes");
        assert_eq!(still_style(STILL_LIFE), Some(PaintStyle::Basic));
        assert_eq!(still_style("Insane"), Some(PaintStyle::Insane));
        assert_eq!(still_style("Paint"), None);
        // (Level five for a still life; six and the traits for the stylized; nine for somber
        // regardless, or a masterpiece for genius.)
        assert!(!can_paint(PaintStyle::Basic, 4, &[], 0.0));
        assert!(can_paint(PaintStyle::Basic, 5, &[], 0.0));
        assert!(!can_paint(PaintStyle::Insane, 6, &[Trait::Genius], 0.0));
        assert!(can_paint(PaintStyle::Insane, 6, &[Trait::Artistic], 0.0));
        assert!(can_paint(PaintStyle::Genius, 1, &[], 1.0));
        assert!(!can_paint(PaintStyle::Somber, 8, &[], 0.0));
        assert!(can_paint(PaintStyle::Somber, 9, &[], 0.0));
        assert!(can_paint(PaintStyle::Somber, 6, &[Trait::Grumpy], 0.0));
    }

    #[test]
    fn a_still_life_is_painted_in_its_style() {
        // (Half red, half blue: strokes keep the halves; a somber one's grey-blue, a genius's
        // tones few.)
        let (w, h) = (12u32, 12u32);
        let px: Vec<u8> = (0..w * h).flat_map(|i| if i % w < 6 { [220, 30, 30, 255] } else { [30, 30, 220, 255] }).collect();
        let mut basic = px.clone();
        paint_filter(&mut basic, w, h, PaintStyle::Basic);
        assert!(basic[0] > 200 && basic[2] < 60, "{:?}", &basic[..4]);
        let mut somber = px.clone();
        paint_filter(&mut somber, w, h, PaintStyle::Somber);
        assert!(somber[2] > somber[0], "blue-grey: {:?}", &somber[..4]);
        let mut genius = px;
        paint_filter(&mut genius, w, h, PaintStyle::Genius);
        assert!(genius.chunks(4).all(|p| p[..3].iter().all(|c| c % 51 == 0)), "posterized");
        // (Its face: the canvas's colour round the picture.)
        let (size, face) = canvas_face(&[255, 0, 0, 255], 1, 1, [0.25, 0.25, 0.75, 0.75]);
        assert_eq!(face[..4], [232, 226, 212, 255]);
        let mid = ((size / 2 * size + size / 2) * 4) as usize;
        assert_eq!(face[mid..mid + 4], [255, 0, 0, 255]);
    }

    #[test]
    fn pictures_are_cut_to_the_frame_and_toned() {
        // (A 4x2 picture, each pixel its own shade.)
        let px: Vec<u8> = (0..8u8).flat_map(|i| [i * 10, i * 10, i * 10, 255]).collect();
        let (w, h, out) = crop(&px, 4, 2, URect::new(1, 0, 3, 2));
        assert_eq!((w, h), (2, 2));
        assert_eq!(out.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), vec![10, 20, 50, 60]);
        // (Clipped to the picture.)
        assert_eq!(crop(&px, 4, 2, URect::new(3, 1, 9, 9)).0, 1);
        let mut red = vec![200, 40, 40, 255];
        filter(&mut red, 1, 1, Style::Gray);
        assert_eq!(red[0], red[1]);
        let mut red = vec![200, 40, 40, 255];
        filter(&mut red, 1, 1, Style::Sepia);
        assert!(red[0] > red[1] && red[1] > red[2], "warm browns");
        // (A vignette's corners darker than its middle.)
        let mut white = vec![255u8; 9 * 9 * 4];
        filter(&mut white, 9, 9, Style::Vignette);
        assert_eq!(white[(4 * 9 + 4) * 4], 255);
        assert!(white[0] < 100);
    }
}
