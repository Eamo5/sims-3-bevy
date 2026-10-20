//! Create a Sim's picker for a colour of the Sim's own (UI.dll's `CASSingleColorPickerDialog`
//! with its `ColorPicker`): the colour wheel (hue round it, saturation out from its middle) with
//! its lightness slider and the colour's numbers, or the swatches; the Sim takes the colour as
//! it's let go of; Accept keeps it, Cancel puts back what was. Asked for by the Color buttons
//! of the eyes (as the game's `CASEyes`), the eyebrows (`CASEyebrows`) and the hair (the game's
//! is its advanced dialog's four colours, roots to tips: here, one).

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::AppState;
use crate::cas::CasAction;
use crate::layout::{Spawned, UiAssets, UiButton, UiSlider, edit_windows};
use crate::livehud::pressed;

pub struct CasColourPlugin;

impl Plugin for CasColourPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (open_picker, picker).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<ColourDialog>();
            UP.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the picker's up.
pub static UP: AtomicBool = AtomicBool::new(false);

/// What a colour's being picked for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PickFor {
    Eyes,
    Brows,
    Hair,
}

impl PickFor {
    /// The dialog's title (the game's key, and its words).
    fn title(self) -> (&'static str, &'static str) {
        match self {
            PickFor::Eyes => ("Ui/Caption/CAS/Physical:EyeColor", "Eye Color"),
            PickFor::Brows => ("Ui/Caption/CAS/Physical:EyebrowColor", "Eyebrow Color"),
            PickFor::Hair => ("Ui/Caption/CAS/Hair:HairColor", "Hair Color"),
        }
    }

    /// The swatches offered (the game's `dm_swatch_basics_eyes` and `_natural`: here, the eyes'
    /// presets and the hair's).
    fn swatches(self) -> Vec<[f32; 3]> {
        match self {
            PickFor::Eyes => crate::sim::EYES.iter().map(|c| [c.0, c.1, c.2]).collect(),
            PickFor::Brows | PickFor::Hair => crate::sim::HAIRS.iter().map(|c| [c.0, c.1, c.2]).collect(),
        }
    }

    /// The colour given the Sim.
    fn action(self, rgb: [f32; 3]) -> CasAction {
        match self {
            PickFor::Eyes => CasAction::EyeRgb(rgb),
            PickFor::Brows => CasAction::BrowRgb(rgb),
            PickFor::Hair => CasAction::HairRgb(rgb),
        }
    }
}

/// Asked for: the picker, for this, from this colour.
#[derive(Resource)]
pub struct PickColour(pub PickFor, pub [f32; 3]);

// `CASSingleColorPickerDialog`'s windows.
const TITLE: u32 = 0x25;
const TAB_SWATCHES: u32 = 0x22;
const TAB_WHEEL: u32 = 0x23;
const OKAY: u32 = 0x31;
const CANCEL: u32 = 0x30;
const SAVE: u32 = 0x32;
// Its `ColorPicker`'s.
const WHEEL_PANEL: u32 = 0x72c6_2d10;
const PALETTE_PANEL: u32 = 0x72c6_2d20;
const PALETTE_GRID: u32 = 0x72c6_2d21;
const WHEEL: u32 = 0x72c6_2d18;
const SELECTOR: u32 = 0x72c6_2d15;
const LIGHTNESS: u32 = 0x72c6_2d14;
const LIGHTNESS_RAMP: u32 = 0x72c6_2d19;
const RGB: [u32; 3] = [0x72c6_2d11, 0x72c6_2d12, 0x72c6_2d13];
const CELL_THUMBNAIL: u32 = 0x20;
/// The dialog's size (its layout's).
const SIZE: Vec2 = Vec2::new(348.0, 398.0);

/// The picker on screen: what for, the colour it opened with, the colour on the wheel (hue,
/// saturation) and its lightness, the page (the wheel or the swatches), and the swatches' cells.
#[derive(Resource)]
struct ColourDialog {
    s: Spawned,
    backdrop: Entity,
    what: PickFor,
    was: [f32; 3],
    hue_sat: (f32, f32),
    lightness: f32,
    wheel: bool,
    grabbed: bool,
    white: Handle<Image>,
    started: bool,
    shown: Option<(bool, [u8; 3])>,
}

/// A swatch's cell.
#[derive(Component)]
struct Swatch([f32; 3]);

fn to_u8(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn open_picker(mut commands: Commands, want: Option<Res<PickColour>>, open: Option<Res<ColourDialog>>, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>)) {
    let Some(want) = want else { return };
    commands.remove_resource::<PickColour>();
    let Some(mut ui) = ui else { return };
    if open.is_some() {
        return;
    }
    let Some(mut layout) = ui.layout("CASSingleColorPickerDialog").cloned() else { return };
    let (key, plain) = want.0.title();
    let title = ui.localize(key).unwrap_or_else(|| plain.to_string());
    edit_windows(&mut layout, TITLE, &mut |t| t.caption = title.clone());
    // (Accept in the panel's corner, Cancel by it, as the game's dialogs have them.)
    for (id, area) in [(OKAY, [258.0, 321.0, 314.0, 377.0]), (CANCEL, [224.0, 340.0, 249.0, 365.0])] {
        edit_windows(&mut layout, id, &mut |w| {
            w.area = area;
            w.place = s3bake::ui::UiPlace::Fixed;
        });
    }
    // (Modal: what's under it can't be clicked.)
    let backdrop = commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
            Interaction::default(),
            crate::hud::BlocksWorld,
            GlobalZIndex(29),
            DespawnOnExit(AppState::CreateHousehold),
        ))
        .id();
    let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &layout);
    let Some(root) = s.root else {
        commands.entity(backdrop).despawn();
        return;
    };
    commands.entity(root).insert((GlobalZIndex(30), crate::hud::BlocksWorld, DespawnOnExit(AppState::CreateHousehold)));
    // (In the middle of the screen, as a modal dialog with no place of its own is.)
    commands.entity(root).entry::<Node>().and_modify(|mut n| {
        n.left = Val::Percent(50.0);
        n.top = Val::Percent(50.0);
        n.margin = UiRect { left: Val::Px(-SIZE.x * 0.5), top: Val::Px(-SIZE.y * 0.5), ..default() };
    });
    for e in s.all_with(SAVE) {
        commands.entity(e).insert(Visibility::Hidden);
    }
    for (id, tip) in [(TAB_SWATCHES, "Swatches"), (TAB_WHEEL, "Color Wheel"), (OKAY, "Accept"), (CANCEL, "Cancel")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    if let Some(e) = s.id(WHEEL) {
        commands.entity(e).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
    }
    let white = images.add(Image::new_fill(
        bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        &[255, 255, 255, 255],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    ));
    let [r, g, b] = want.1;
    let hsv = Hsva::from(Color::srgb(r, g, b));
    UP.store(true, Ordering::Relaxed);
    commands.insert_resource(ColourDialog {
        s,
        backdrop,
        what: want.0,
        was: want.1,
        hue_sat: (hsv.hue, hsv.saturation),
        lightness: hsv.value,
        wheel: true,
        grabbed: false,
        white,
        started: false,
        shown: None,
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn picker(
    mut commands: Commands,
    dialog: Option<ResMut<ColourDialog>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    swatches: Query<(&Interaction, &Swatch), Changed<Interaction>>,
    wheel_q: Query<(&Interaction, &ComputedNode, &bevy::ui::UiGlobalTransform)>,
    mut sliders: Query<&mut UiSlider>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    (mut vis, mut texts, mut nodes, mut buttons): (Query<&mut Visibility>, Query<&mut Text>, Query<&mut Node>, Query<&mut UiButton>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
) {
    let (Some(mut d), Some(mut ui)) = (dialog, ui) else { return };
    let d = &mut *d;
    let colour = |d: &ColourDialog| {
        let c = Color::hsv(d.hue_sat.0, d.hue_sat.1, d.lightness).to_srgba();
        [c.red, c.green, c.blue]
    };
    let set_lightness = |d: &ColourDialog, sliders: &mut Query<&mut UiSlider>| {
        if let Some(mut sl) = d.s.id(LIGHTNESS).and_then(|e| sliders.get_mut(e).ok()) {
            sl.value = (d.lightness * sl.max).round();
        }
    };
    // (Once up: the slider where the colour is, the swatches in their grid.)
    if !d.started {
        d.started = true;
        set_lightness(d, &mut sliders);
        if let (Some(grid), Some(g), Some(template)) = (d.s.id(PALETTE_GRID), ui.find("ColorPicker", PALETTE_GRID).and_then(|w| w.grid), ui.layout("ColorsGridItem").cloned()) {
            let cols = g.columns.max(1) as usize;
            let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
            for (n, c) in d.what.swatches().into_iter().enumerate() {
                let (x, y) = (g.padding[0] + (n % cols) as f32 * step.x + g.cell_padding[0], g.padding[1] + (n / cols) as f32 * step.y + g.cell_padding[1]);
                let mut w = template.clone();
                w.area = [x, y, x + g.cell[0], y + g.cell[1]];
                w.cls = "Button".into();
                edit_windows(&mut w, CELL_THUMBNAIL, &mut |t| t.area = [3.0, 3.0, g.cell[0] - 3.0, g.cell[1] - 3.0]);
                let cs = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, grid);
                let Some(r) = cs.root else { continue };
                commands.entity(r).insert(Swatch(c));
                if let Some(win) = cs.id(CELL_THUMBNAIL) {
                    crate::hudpanels::picture(&mut commands, win, d.white.clone(), Color::srgb(c[0], c[1], c[2]));
                }
            }
        }
    }
    // Accept: kept; Cancel: as it was.
    let okay = pressed(&clicks, d.s.id(OKAY));
    let cancel = pressed(&clicks, d.s.id(CANCEL));
    if okay || cancel {
        info!("colour picker: {}", if okay { "accepted" } else { "cancelled" });
        if cancel {
            actions.write(crate::cas::CasActionRequest(d.what.action(d.was)));
        }
        if let Some(r) = d.s.root {
            commands.entity(r).try_despawn();
        }
        commands.entity(d.backdrop).try_despawn();
        commands.remove_resource::<ColourDialog>();
        UP.store(false, Ordering::Relaxed);
        return;
    }
    // The pages.
    if pressed(&clicks, d.s.id(TAB_WHEEL)) {
        d.wheel = true;
    }
    if pressed(&clicks, d.s.id(TAB_SWATCHES)) {
        d.wheel = false;
    }
    // A swatch: the colour.
    let mut give = None;
    for (i, sw) in &swatches {
        if *i == Interaction::Pressed {
            let hsv = Hsva::from(Color::srgb(sw.0[0], sw.0[1], sw.0[2]));
            d.hue_sat = (hsv.hue, hsv.saturation);
            d.lightness = hsv.value;
            set_lightness(d, &mut sliders);
            give = Some(sw.0);
        }
    }
    // The wheel dragged over: the colour follows; let go, the Sim takes it.
    if let Some((i, node, tf)) = d.s.id(WHEEL).and_then(|e| wheel_q.get(e).ok())
        && let Some(p) = windows.single().ok().and_then(|w| w.physical_cursor_position())
        && let Some(local) = tf.try_inverse().map(|inv| inv.transform_point2(p))
    {
        if *i == Interaction::Pressed && mouse.just_pressed(MouseButton::Left) {
            d.grabbed = true;
        }
        if d.grabbed && mouse.pressed(MouseButton::Left) {
            let r = node.size().x.min(node.size().y) * 0.5;
            let v = local / r.max(1.0);
            // (The wheel's hues go round anticlockwise, red on the right: the screen's down is
            // the wheel's minus.)
            d.hue_sat = (((-v.y).atan2(v.x).to_degrees() + 360.0) % 360.0, v.length().min(1.0));
        }
    }
    if d.grabbed && !mouse.pressed(MouseButton::Left) {
        d.grabbed = false;
        give = Some(colour(d));
    }
    // The lightness slider: let go, the Sim takes it.
    if let Some(sl) = d.s.id(LIGHTNESS).and_then(|e| sliders.get(e).ok()) {
        let l = if sl.max > 0.0 { sl.value / sl.max } else { 1.0 };
        if (l - d.lightness).abs() > 1e-3 {
            d.lightness = l;
            if !sl.is_grabbed() {
                give = Some(colour(d));
            }
        }
    }
    if let Some(c) = give {
        actions.write(crate::cas::CasActionRequest(d.what.action(c)));
    }
    // The wheel's selector where the colour is; the lightness ramp in its hue; the numbers.
    let (hue, sat) = d.hue_sat;
    if let (Some(sel), Some((_, node, _))) = (d.s.id(SELECTOR), d.s.id(WHEEL).and_then(|e| wheel_q.get(e).ok())) {
        let size = node.size() * node.inverse_scale_factor();
        let r = size.x.min(size.y) * 0.5;
        let a = hue.to_radians();
        // (The selector's window is 29 across; the wheel's picture sits 4 in from the wheel.)
        let at = Vec2::new(4.0 + r + a.cos() * sat * r - 14.5, 4.0 + r - a.sin() * sat * r - 15.0);
        if let Ok(mut n) = nodes.get_mut(sel) {
            let want = (Val::Px(at.x), Val::Px(at.y));
            if (n.left, n.top) != want {
                (n.left, n.top) = want;
            }
        }
    }
    let now = to_u8(colour(d));
    if d.shown == Some((d.wheel, now)) {
        return;
    }
    d.shown = Some((d.wheel, now));
    for (n, id) in RGB.iter().enumerate() {
        crate::livehud::set_text(&mut texts, d.s.text(*id), &format!("{}", now[n]));
    }
    let full = Color::hsv(hue, sat.max(0.0), 1.0);
    if let Some(ramp) = d.s.id(LIGHTNESS_RAMP) {
        // (The ramp's picture is the window's, or its child's.)
        commands.entity(ramp).queue(move |mut e: EntityWorldMut| {
            if let Some(mut img) = e.get_mut::<ImageNode>() {
                img.color = full;
            }
            let kids: Vec<Entity> = e.get::<Children>().map(|c| c.to_vec()).unwrap_or_default();
            e.world_scope(|w| {
                for k in kids {
                    if let Some(mut img) = w.get_mut::<ImageNode>(k) {
                        img.color = full;
                    }
                }
            });
        });
    }
    crate::livehud::set_visible(&mut vis, d.s.id(WHEEL_PANEL), d.wheel);
    crate::livehud::set_visible(&mut vis, d.s.id(PALETTE_PANEL), !d.wheel);
    for (id, lit) in [(TAB_WHEEL, d.wheel), (TAB_SWATCHES, !d.wheel)] {
        if let Some(mut b) = d.s.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != lit
        {
            b.selected = lit;
        }
    }
}
