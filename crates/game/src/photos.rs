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
            .add_systems(Update, (start_grab, grab, photo_taken, titled, frames, photo_test).chain().after(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
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
const CELL_PHONE: u32 = 0xF59E_A6BB;
const CELL_PHONE_COMMENT: &str = "Cellphone Photography Screen Grab";
const FRAME_BASE: u32 = 0x100;
const TARGET: u32 = 1;
const PHONE_SHADE: u32 = 0x9300_0000;
/// How far a phone zooms (its field of view, degrees).
const PHONE_ZOOM: (f32, f32) = (18.0, 60.0);
/// The picture part of a photograph's frame (its stand-in texture, the picture drawn in its
/// place) and the frame's state on a wall.
const PLACEHOLDER: Key = (0x00B2_D882, 0, 0x410F_4E4F_6F98_E9DF);
const ON_WALL: &str = "frameWall";
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

    /// Where its picture's kept.
    pub fn path(&self) -> PathBuf {
        dir().join(format!("{}.png", self.id))
    }
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
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GrabKind {
    /// A phone's photo (`CellPhonePhotography`).
    Phone,
}

/// A picture taken: by whom, for what, and the photograph.
#[derive(Message, Clone, Debug)]
pub struct Grabbed {
    pub sim: Entity,
    pub kind: GrabKind,
    pub photo: Photo,
}

/// A screen grab under way (`ScreenGrabController`): who for, what for, the frame and style,
/// how far along, the layout on screen and what's been hidden for it, and where the camera was.
#[derive(Resource)]
pub struct Grabbing {
    pub sim: Entity,
    pub kind: GrabKind,
    size: Size,
    style: Style,
    stage: Stage,
    root: Option<Entity>,
    target: Option<Entity>,
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
    pub fn new(kind: GrabKind, sim: Entity) -> Self {
        Grabbing {
            sim,
            kind,
            size: Size::Tiny,
            style: Style::Plain,
            stage: Stage::Open,
            root: None,
            target: None,
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

/// Whether the action's a photo being taken.
fn taking(a: &Action) -> bool {
    a.label == TAKE_PHOTO && matches!(a.kind, ActionKind::Outro { .. })
}

/// "Take Photo" from the phone: an action of its own, the grab for as long as it lasts.
pub fn take_photo(sim: Entity) -> ActionKind {
    ActionKind::Outro { clips: &[], then: None, secs: 1.0e9, stand_at: None, target: sim }
}

/// The selected Sim's photo begun: the grab.
fn start_grab(mut commands: Commands, grabbing: Option<Res<Grabbing>>, sims: Query<(Entity, &ActionQueue), With<Selected>>, time: Res<Time>) {
    if grabbing.is_some() {
        return;
    }
    for (e, q) in &sims {
        if q.0.front().is_some_and(|a| taking(a) && !a.cancel && matches!(a.phase, Phase::Running(_))) {
            let mut g = Grabbing::new(GrabKind::Phone, e);
            g.auto = std::env::var("PHOTO_CAPTURE_AT").ok().and_then(|v| v.parse().ok());
            g.began = time.elapsed_secs();
            commands.insert_resource(g);
            info!("photos: a screen grab for a phone photo");
        }
    }
}

/// How high a Sim's eyes are, by their age.
fn eye_height(age: Age) -> f32 {
    match age {
        Age::Child => 1.08,
        Age::Teen => 1.55,
        a if a.is_little() => 0.6,
        _ => 1.62,
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
) {
    let Some(mut g) = grabbing else { return };
    let g = &mut *g;
    let walls = &mut *walls;
    // (Put away from the queue, or the Sim gone: stopped.)
    let still = sims.get(g.sim).is_ok_and(|s| s.2.0.front().is_some_and(|a| taking(a) && !a.cancel));
    let mut done: Option<bool> = (!still).then_some(false);
    match g.stage {
        Stage::Open => {
            let Some(mut ui) = ui else { return };
            let Some(mut w) = ui.export(LAYOUT, CELL_PHONE).or_else(|| ui.export_commented(LAYOUT, CELL_PHONE_COMMENT)).cloned() else {
                warn!("photos: no screen grab layout");
                done = Some(false);
                return finish(&mut commands, g, done, &mut cams, &mut sims, &mut roots, &mut seen, walls);
            };
            localize(&ui, &mut w);
            w.flags |= s3bake::ui::WIN_VISIBLE;
            // (A phone's one frame: its small portrait.)
            let frame = FRAME_BASE;
            for c in w.children.iter_mut().filter(|c| c.id == frame) {
                c.flags |= s3bake::ui::WIN_VISIBLE;
                // (The viewfinder at its own size, round the target: the phone's frame isn't
                // stretched to its window.)
                if let Some(s3bake::ui::UiDrawable::Image { flags, .. }) = &mut c.drawable {
                    *flags = 0;
                }
            }
            let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
            let Some(root) = s.root else { return };
            commands.entity(root).insert((GlobalZIndex(60), Interaction::default(), FocusPolicy::Block, crate::hud::BlocksWorld, DespawnOnExit(crate::AppState::InGame)));
            g.target = s.id(frame).and_then(|f| s.within(f, TARGET));
            // (The world outside the target dimmed, as the base window's frame drawable has it:
            // under the frame's picture, out to the screen's edges.)
            // (Under the layout's windows.)
            g.shades = (0..4).map(|_| commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, BackgroundColor(crate::layout::color(PHONE_SHADE)), Pickable::IGNORE)).id()).collect();
            commands.entity(root).insert_children(0, &g.shades);
            g.root = Some(root);
            // The camera to their eyes, looking the way they face.
            if let (Ok(mut cam), Ok((tf, sim, ..))) = (cams.single_mut(), sims.get(g.sim)) {
                g.view = Some((cam.focus, cam.yaw, cam.pitch, cam.distance, cam.height_offset, cam.follow));
                let f = tf.rotation * Vec3::Z;
                let zoom = std::env::var("PHOTO_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(crate::camera::FOV);
                let turn = std::env::var("PHOTO_YAW").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                cam.photo = Some(crate::camera::PhotoView {
                    eye: tf.translation + Vec3::Y * eye_height(sim.age) + f * 0.12,
                    yaw: (-f.x).atan2(-f.z) + turn,
                    pitch: std::env::var("PHOTO_PITCH").ok().and_then(|v| v.parse().ok()).unwrap_or(-0.05),
                    fov: zoom.clamp(PHONE_ZOOM.0, PHONE_ZOOM.1).to_radians(),
                    fovs: (PHONE_ZOOM.0.to_radians(), PHONE_ZOOM.1.to_radians()),
                });
            }
            hide(g, &mut roots, &mut seen, &sims);
            g.walls = Some(*walls);
            *walls = crate::building::WallMode::Up;
            g.stage = Stage::Framing;
        }
        Stage::Framing => {
            // (The interface, the Sim and their plumbob kept out of the picture.)
            hide(g, &mut roots, &mut seen, &sims);
            // (Where the picture's to be cut from the window's: the target, in pixels.)
            if let (Some((c, at)), Some((screen, _))) = (g.target.and_then(|t| nodes.get(t).ok()), g.root.and_then(|r| nodes.get(r).ok())) {
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

            if keys.just_pressed(KeyCode::Escape) {
                keys.clear_just_pressed(KeyCode::Escape);
                done = Some(false);
            } else if (keys.just_pressed(KeyCode::KeyC) || auto) && g.rect.width() > 4 && g.rect.height() > 4 {
                keys.clear_just_pressed(KeyCode::KeyC);

                sounds.write(crate::sound::PlaySound { name: CLICK.into(), at: None, volume: 1.0, speed: 1.0 });
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
                filter(&mut pic, cw, ch, g.style);
                let photo = Photo { id: format!("{:016x}", rand::random::<u64>()), size: g.size, style: g.style };
                let path = photo.path();
                let _ = std::fs::create_dir_all(dir());
                match std::fs::write(&path, s3bake::gamedata::encode_rgba_png(cw, ch, &pic)) {
                    Ok(()) => {
                        info!("photos: took a {cw}x{ch} picture ({})", path.display());
                        grabbed.write(Grabbed { sim: g.sim, kind: g.kind, photo });
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
    let bob = sims.get(g.sim).ok().and_then(|s| s.3.map(|p| p.0));
    for e in std::iter::once(g.sim).chain(bob) {
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
    if let Ok((.., mut q, _)) = sims.get_mut(g.sim)
        && q.0.front().is_some_and(taking)
    {
        q.0.pop_front();
    }
    info!("photos: the grab's over ({})", if taken { "taken" } else { "put away" });
    commands.remove_resource::<Grabbing>();
}

/// A phone's photo into its taker's inventory: "Untitled", worth nothing.
fn photo_taken(mut commands: Commands, mut grabbed: MessageReader<Grabbed>) {
    for g in grabbed.read() {
        if g.kind == GrabKind::Phone {
            crate::inventory::give(&mut commands, g.sim, ItemKind::Photo, g.photo.key(), UNTITLED.into(), 0, 0, 1);
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
    let key: Key = (0x5048_4F54, 0, s3pkg::fnv64(&p.id));
    cache
        .0
        .entry(key)
        .or_insert_with(|| {
            let png = std::fs::read(p.path()).ok()?;
            let (w, h, px) = s3bake::gamedata::decode_icon(&png)?;
            let img = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
            Some((images.add(img), UVec2::new(w, h)))
        })
        .clone()
        .map(|(h, _)| h)
}

/// A photograph's frame showing its picture.
#[derive(Component)]
struct PhotoShown;

/// Photographs' frames drawn in their wall state, and hung photographs (and one held up in Buy
/// mode) with their pictures in them.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn frames(
    mut commands: Commands,
    hung: Query<(Entity, &crate::paintings::Hung, &crate::interact::GameObject), Without<PhotoShown>>,
    buy: Res<crate::buy::BuyMode>,
    kids: Query<&Children>,
    mut parts: Query<&mut MeshMaterial3d<StandardMaterial>>,
    data: Option<Res<crate::baked::Baked>>,
    (mut assets, mut cache): (ResMut<crate::objects::ObjectAssets>, ResMut<crate::paintings::PaintingImages>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut walls: Local<HashMap<Key, Handle<StandardMaterial>>>,
    mut pictures: Local<HashMap<String, Handle<StandardMaterial>>>,
) {
    let Some(data) = data else { return };
    // (Once: each size's frame in its wall state, and its picture's stand-in material.)
    if walls.is_empty() {
        for size in Size::ALL {
            let Some(objd) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(size.frame())).map(|c| c.objd) else { continue };
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            let state = assets.object_in_state(&mut ctx, objd, s3pkg::fnv32(ON_WALL));
            let state = if state.iter().any(|p| p.tex == Some(PLACEHOLDER)) { state } else { assets.object(&mut ctx, objd) };
            if let Some(p) = state.iter().find(|p| p.tex == Some(PLACEHOLDER)) {
                walls.insert(objd, p.material.clone());
            }
            assets.set_object(objd, state);
        }
        if walls.is_empty() {
            walls.insert((0, 0, 0), Handle::default());
        }
    }
    let ghost = buy.placing.as_ref().and_then(|p| p.item.as_ref().filter(|i| i.kind == ItemKind::Photo).map(|i| (p.ghost, p.objd, i.clone())));
    let mut todo: Vec<(Entity, Key, Stack, bool)> = hung.iter().filter(|(_, h, _)| h.0.kind == ItemKind::Photo).map(|(e, h, o)| (e, o.objd, h.0.clone(), true)).collect();
    todo.extend(ghost.map(|(e, o, s)| (e, o, s, false)));
    for (e, objd, s, mark) in todo {
        let Some(stand_in) = walls.get(&objd).cloned() else { continue };
        let Some(id) = Photo::from_key(&s.key).map(|p| p.id) else { continue };
        let picture = match pictures.get(&id) {
            Some(m) => m.clone(),
            None => {
                let Some(img) = image(&mut cache, &mut images, &s) else { continue };
                let m = mats.add(StandardMaterial { base_color_texture: Some(img), perceptual_roughness: 0.45, reflectance: 0.35, ..default() });
                pictures.insert(id, m.clone());
                m
            }
        };
        let Ok(children) = kids.get(e) else { continue };
        let mut swapped = false;
        for c in children.iter() {
            if let Ok(mut m) = parts.get_mut(c) {
                if m.0 == stand_in {
                    m.0 = picture.clone();
                    swapped = true;
                } else if m.0 == picture {
                    swapped = true;
                }
            }
        }
        if swapped && mark {
            commands.entity(e).insert(PhotoShown);
            debug!("photos: {} shows its picture", s.name);
        }
    }
}

/// `PHOTO_TEST=1` (tests): ten seconds in, the selected Sim takes a photo with their phone
/// (`PHOTO_CAPTURE_AT=<seconds>`: taken that long into the grab; `PHOTO_YAW`, `PHOTO_PITCH`,
/// `PHOTO_ZOOM`: which way and how far zoomed); `PHOTO_HANG=1`: taken, it's hung on the nearest
/// wall and the camera's on it; `PHOTO_PIE=1`: its inventory menu instead; `PHOTO_PHONE=1`: just
/// the phone's menu.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn photo_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(Entity, &Transform, &mut ActionQueue, Option<&crate::inventory::Inventory>), With<Selected>>,
    grabbing: Option<Res<Grabbing>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
    (world, mut wall_mode): (Res<crate::loading::CurrentWorld>, ResMut<crate::building::WallMode>),
) {
    if std::env::var_os("PHOTO_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let Ok((me, tf, mut q, inv)) = sims.single_mut() else { return };
    match stage.0 {
        0 if now > 10.0 => {
            *stage = (1, now);
            // (PHOTO_PHONE=1: the phone's menu instead.)
            if std::env::var_os("PHOTO_PHONE").is_some() {
                *stage = (9, now);
                commands.insert_resource(crate::hud::OpenPhone);
                return;
            }
            q.push_player(Action::new(TAKE_PHOTO, take_photo(me), false));
            info!("photo test: taking a photo");
        }
        1 if grabbing.is_none() && now - stage.1 > 2.0 => {
            let Some(s) = inv.and_then(|i| i.0.iter().find(|s| s.kind == ItemKind::Photo)).cloned() else { return };
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
            let (Some(b), Some(objd)) = (building, frame(&data.0, &s)) else { return };
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
            if let Some(o) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, objd, pos, rot, None) {
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
        assert!(p.path().ends_with("photos/00ab12cd34ef5678.png") || p.path().ends_with("photos\\00ab12cd34ef5678.png"));
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
