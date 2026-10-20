//! Create a Style in the game's own panel (UI.dll's `CASCompositorController` over its
//! `CASCompositor` layout): the materials bin down the right, the item worn's colourways in its
//! materials grid (the game's picture of each), the item's colour channels in the colour row (the
//! one being coloured lit), and the colour picker below with its pages: the wheel (hue round
//! it, saturation out from its middle, the lightness slider under it, the colour's red, green
//! and blue), and the palette's swatches. A colour chosen is the channel's (rendered afresh from
//! the item's pattern, as `cas::CasAction::StyleRgb` asks); OK keeps the style, Cancel puts the
//! item back as it was.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::AppState;
use crate::cas::{CasAction, CasTab};
use crate::home::PendingHousehold;
use crate::layout::{Spawned, UiAssets, UiButton, UiSlider, edit_windows};

pub struct CasStylePlugin;

impl Plugin for CasStylePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_style, style_panel).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<CasStyle>();
            STYLE_UP.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the game's Create a Style is up (see `caslook::game_panel`).
pub static STYLE_UP: AtomicBool = AtomicBool::new(false);

// `CASCompositor`'s windows.
const PRESETS_WINDOW: u32 = 0x0001_5000;
const MATERIALS_SKEWER: u32 = 0x0001_3000;
const TRANSFER: u32 = 0x0001_4000;
const MATERIALS_BIN: u32 = 0x0001_2000;
const MATERIAL_GRID: u32 = 0x0001_2001;
const COLOUR_NUMBERS: u32 = 0x0001_2005;
const PAGE_PALETTE: u32 = 0x0001_2006;
const PAGE_WHEEL: u32 = 0x0001_2007;
const PAGE_NUMBER: u32 = 0x0001_200f;
const OKAY: u32 = 0x0001_200a;
const CANCEL: u32 = 0x0001_2015;
/// The channels' buttons and their colour windows, and the colour summary's four.
const CHANNELS: [u32; 4] = [0x0001_202a, 0x0001_202b, 0x0001_202c, 0x0001_202d];
const CHANNEL_COLOURS: [u32; 4] = [0x0001_203a, 0x0001_203b, 0x0001_203c, 0x0001_203d];
const SUMMARY: [u32; 4] = [0x0001_400a, 0x0001_400b, 0x0001_400c, 0x0001_400d];
/// What this game hasn't: the materials' list, filter, favourites, deleting, saving and sharing,
/// undo and redo, the materials' empty word, the colour summary's points.
const NOT_HERE: [u32; 15] = [0x0001_2004, 0x0001_2009, 0x0012_0010, 0x0001_2002, 0x0001_2014, 0x0001_2003, 0x0001_200c, 0x0001_200d, 0x0001_2008, 0x0001_204a, 0x0001_204b, 0x0001_204c, 0x0001_204d, 0x0001_6000, PAGE_NUMBER];
// The colour picker's (layout `ColorPicker`).
const WHEEL_PANEL: u32 = 0x72c6_2d10;
const PALETTE_PANEL: u32 = 0x72c6_2d20;
const PALETTE_GRID: u32 = 0x72c6_2d21;
const WHEEL: u32 = 0x72c6_2d18;
const SELECTOR: u32 = 0x72c6_2d15;
const LIGHTNESS: u32 = 0x72c6_2d14;
const LIGHTNESS_RAMP: u32 = 0x72c6_2d19;
const RGB: [u32; 3] = [0x72c6_2d11, 0x72c6_2d12, 0x72c6_2d13];
const CELL_THUMBNAIL: u32 = 0x20;

/// The panel on screen: the channel being coloured, the picker's page (the wheel or the
/// palette), the colour on the wheel (hue, saturation) and the cells shown.
#[derive(Resource)]
pub struct CasStyle {
    s: Spawned,
    channel: usize,
    wheel: bool,
    hue_sat: Option<(f32, f32)>,
    lightness: f32,
    grabbed: bool,
    white: Handle<Image>,
    materials: Option<Entity>,
    swatches: Option<Entity>,
    shown: Option<Shown>,
}

#[derive(PartialEq, Clone)]
struct Shown {
    key: Option<s3bake::Key>,
    colours: Vec<(u8, [u8; 3])>,
    channel: usize,
    wheel: bool,
    design: u8,
}

/// A colourway's cell in the materials grid, and a swatch of the palette.
#[derive(Component)]
struct MaterialCell(u8);
#[derive(Component)]
struct SwatchCell(usize);

fn spawn_style(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<crate::caslook::CasFrame>>, style: Option<Res<CasStyle>>) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if style.is_some() || ui.layout("CASCompositor").is_none() || ui.layout("ColorPicker").is_none() || ui.layout("MaterialsGridItem").is_none() || ui.layout("ColorsGridItem").is_none() {
        return;
    }
    let Some(mut w) = ui.layout("CASCompositor").cloned() else { return };
    // (The materials bin up, the presets', skewer's and transfer's windows not.)
    edit_windows(&mut w, MATERIALS_BIN, &mut |b| {
        b.flags |= s3bake::ui::WIN_VISIBLE;
        // (Its holder down the screen's middle: its layout has it mostly above the bin, for the
        // game's code to slide in.)
        for h in b.children.iter_mut().filter(|h| h.comment == "Holder window for layout.") {
            let height = h.area[3] - h.area[1];
            let top = ((b.area[3] - b.area[1]) - height) * 0.5;
            (h.area[1], h.area[3]) = (top, top + height);
        }
    });
    let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
    let Some(root) = s.root else { return };
    commands.entity(root).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(6), Visibility::Hidden, Pickable::IGNORE));
    for id in NOT_HERE.into_iter().chain([PRESETS_WINDOW, MATERIALS_SKEWER, TRANSFER, COLOUR_NUMBERS]) {
        for e in s.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for (id, tip) in [(PAGE_PALETTE, "Palette"), (PAGE_WHEEL, "Color Wheel"), (OKAY, "Accept"), (CANCEL, "Cancel")] {
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
    STYLE_UP.store(true, Ordering::Relaxed);
    commands.insert_resource(CasStyle { s, channel: 0, wheel: true, hue_sat: None, lightness: 1.0, grabbed: false, white, materials: None, swatches: None, shown: None });
}

/// The item worn's colour channels for its colourway, with their colours now (the style made,
/// else the colourway's own).
fn channels(scene: &crate::cas::CasScene, sim: &crate::sim::Sim, key: s3bake::Key) -> (u8, Vec<(u8, [f32; 3])>) {
    let design = sim.outfit.designs.iter().find(|(p, _)| *p == key).map_or(0, |d| d.1);
    let mut list: Vec<(u8, [f32; 3])> = scene.cas.colourways.get(&key).and_then(|w| w.channels.get(design as usize).cloned()).unwrap_or_default();
    if let Some(style) = sim.outfit.styles.iter().find(|s| s.part == key && s.preset == design) {
        for (c, v) in &style.colours {
            if let Some(x) = list.iter_mut().find(|x| x.0 == *c) {
                x.1 = *v;
            }
        }
    }
    list.truncate(4);
    (design, list)
}

fn to_u8(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn style_panel(
    mut commands: Commands,
    style: Option<ResMut<CasStyle>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    scene: Option<Res<crate::cas::CasScene>>,
    (mut vis, mut buttons, mut sliders, mut nodes, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiSlider>, Query<&mut Node>, Query<&mut Text>),
    (clicks, materials, swatches, wheel_q): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &MaterialCell), Changed<Interaction>>, Query<(&Interaction, &SwatchCell), Changed<Interaction>>, Query<(&Interaction, &ComputedNode, &bevy::ui::UiGlobalTransform)>),
    (mouse, windows): (Res<ButtonInput<MouseButton>>, Query<&Window, With<bevy::window::PrimaryWindow>>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
) {
    let (Some(mut st), Some(mut ui), Some(scene)) = (style, ui, scene) else { return };
    let st = &mut *st;
    let tab = selected.1;
    let on = crate::cas::STYLING.load(Ordering::Relaxed) && matches!(tab, CasTab::Tops | CasTab::Bottoms | CasTab::Outfits | CasTab::Shoes) && !crate::caslook::modal_up();
    crate::livehud::set_visible(&mut vis, st.s.root, on);
    if !on {
        st.shown = None;
        st.hue_sat = None;
        return;
    }
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let (Some(sim), Some(t)) = (pending.members.get(k), tab.clothing_type()) else { return };
    let key = crate::cas::worn(&scene, sim, t);
    let (design, list) = key.map_or((0, Vec::new()), |key| channels(&scene, sim, key));
    st.channel = st.channel.min(list.len().saturating_sub(1));
    let s = st.s.clone();
    let pressed = |id: u32| crate::livehud::pressed(&clicks, s.id(id));
    // OK keeps the style made; Cancel puts the item back as it was.
    if pressed(OKAY) {
        actions.write(crate::cas::CasActionRequest(CasAction::Styling));
        return;
    }
    if pressed(CANCEL) {
        actions.write(crate::cas::CasActionRequest(CasAction::StyleCancel));
        return;
    }
    for (n, id) in CHANNELS.iter().enumerate() {
        if pressed(*id) && n < list.len() && st.channel != n {
            st.channel = n;
            st.hue_sat = None;
        }
    }
    if pressed(PAGE_PALETTE) {
        st.wheel = false;
    }
    if pressed(PAGE_WHEEL) {
        st.wheel = true;
    }
    for (i, m) in &materials {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::Colourway(m.0)));
            st.hue_sat = None;
        }
    }
    let channel = list.get(st.channel).map(|c| c.0);
    for (i, w) in &swatches {
        if *i == Interaction::Pressed
            && let (Some(ch), Some(c)) = (channel, crate::style::PALETTE.get(w.0))
        {
            actions.write(crate::cas::CasActionRequest(CasAction::StyleRgb(ch, *c)));
            st.hue_sat = None;
        }
    }
    // The wheel: dragged over, the colour follows (hue round it, saturation out from the
    // middle); let go, the channel takes it.
    let current = list.get(st.channel).map_or([1.0; 3], |c| c.1);
    if st.hue_sat.is_none() {
        let hsv = Hsva::from(Color::srgb(current[0], current[1], current[2]));
        st.hue_sat = Some((hsv.hue, hsv.saturation));
        st.lightness = hsv.value;
        if let Some(mut sl) = s.id(LIGHTNESS).and_then(|e| sliders.get_mut(e).ok()) {
            sl.value = (hsv.value * sl.max).round();
        }
    }
    let mut changed = false;
    if let Some((i, node, tf)) = s.id(WHEEL).and_then(|e| wheel_q.get(e).ok())
        && let Some(p) = windows.single().ok().and_then(|w| w.physical_cursor_position())
        && let Some(local) = tf.try_inverse().map(|inv| inv.transform_point2(p))
    {
        if *i == Interaction::Pressed && mouse.just_pressed(MouseButton::Left) {
            st.grabbed = true;
        }
        if st.grabbed && mouse.pressed(MouseButton::Left) {
            let r = node.size().x.min(node.size().y) * 0.5;
            let d = local / r.max(1.0);
            // (The wheel's hues go round anticlockwise, red on the right: the screen's down is
            // the wheel's minus.)
            let hue = ((-d.y).atan2(d.x).to_degrees() + 360.0) % 360.0;
            st.hue_sat = Some((hue, d.length().min(1.0)));
            changed = true;
        }
    }
    if let Some(sl) = s.id(LIGHTNESS).and_then(|e| sliders.get(e).ok()) {
        let l = if sl.max > 0.0 { sl.value / sl.max } else { 1.0 };
        if (l - st.lightness).abs() > 1e-3 {
            st.lightness = l;
            changed = true;
        }
    }
    let (hue, sat) = st.hue_sat.unwrap_or((0.0, 0.0));
    let picked = Color::hsv(hue, sat, st.lightness).to_srgba();
    let picked = [picked.red, picked.green, picked.blue];
    let released = st.grabbed && !mouse.pressed(MouseButton::Left);
    if released {
        st.grabbed = false;
    }
    let sliding = s.id(LIGHTNESS).and_then(|e| sliders.get(e).ok()).is_some_and(|sl| sl.is_grabbed());
    if (released || (changed && !st.grabbed && !sliding)) && let Some(ch) = channel {
        actions.write(crate::cas::CasActionRequest(CasAction::StyleRgb(ch, picked)));
    }
    // The wheel's selector where the colour is; the lightness ramp in its hue; the numbers.
    if let (Some(sel), Some((_, node, _))) = (s.id(SELECTOR), s.id(WHEEL).and_then(|e| wheel_q.get(e).ok())) {
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
    let shown_colour = if st.grabbed || sliding { picked } else { current };
    for (n, id) in RGB.iter().enumerate() {
        crate::livehud::set_text(&mut texts, s.text(*id), &format!("{}", to_u8(shown_colour)[n]));
    }
    let want = Shown { key, colours: list.iter().map(|(c, v)| (*c, to_u8(*v))).collect(), channel: st.channel, wheel: st.wheel, design };
    if st.shown.as_ref() == Some(&want) {
        return;
    }
    st.shown = Some(want);
    let full = Color::hsv(hue, sat.max(0.0), 1.0);
    if let Some(ramp) = s.id(LIGHTNESS_RAMP) {
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
    // The pages: the wheel or the palette.
    crate::livehud::set_visible(&mut vis, s.id(WHEEL_PANEL), st.wheel);
    crate::livehud::set_visible(&mut vis, s.id(PALETTE_PANEL), !st.wheel);
    for (id, lit) in [(PAGE_WHEEL, st.wheel), (PAGE_PALETTE, !st.wheel)] {
        if let Some(mut b) = s.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != lit
        {
            b.selected = lit;
        }
    }
    // The channels: as many as the item has, each in its colour, the one being coloured lit;
    // the summary's four in them too.
    for n in 0..4 {
        let c = list.get(n).map(|c| c.1);
        crate::livehud::set_visible(&mut vis, s.id(CHANNELS[n]), c.is_some());
        if let Some(mut b) = s.id(CHANNELS[n]).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != (n == st.channel)
        {
            b.selected = n == st.channel;
        }
        if let (Some(win), Some(c)) = (s.id(CHANNEL_COLOURS[n]), c) {
            crate::hudpanels::picture(&mut commands, win, st.white.clone(), Color::srgb(c[0], c[1], c[2]));
        }
        if let Some(win) = s.id(SUMMARY[n]) {
            let colour = c.map_or(Color::NONE, |c| Color::srgb(c[0], c[1], c[2]));
            commands.entity(win).queue(move |mut e: EntityWorldMut| {
                if let Some(mut img) = e.get_mut::<ImageNode>() {
                    img.color = colour;
                }
            });
            crate::livehud::set_visible(&mut vis, Some(win), c.is_some());
        }
    }
    // The materials: the item's colourways, each the game's picture of it; the palette's
    // swatches.
    for old in [st.materials.take(), st.swatches.take()].into_iter().flatten() {
        commands.entity(old).try_despawn();
    }
    let holder = |commands: &mut Commands, grid: Entity| commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
    if let (Some(grid), Some(g), Some(template), Some(key)) = (s.id(MATERIAL_GRID), ui.find("CASCompositor", MATERIAL_GRID).and_then(|w| w.grid), ui.layout("MaterialsGridItem").cloned(), key) {
        let gw = ui.find("CASCompositor", MATERIAL_GRID).map_or(294.0, |w| w.area[2] - w.area[0]);
        let cols = g.columns.max(1) as usize;
        // (The game's grid stretches its cells to fill it.)
        let cell = ((gw - g.padding[0] - g.padding[2]) / cols as f32).min(70.0);
        let count = scene.cas.colourways.get(&key).map_or(1, |w| w.swatches.len().clamp(1, 8));
        let cells = holder(&mut commands, grid);
        st.materials = Some(cells);
        for n in 0..count {
            let (x, y) = (g.padding[0] + (n % cols) as f32 * cell, g.padding[1] + (n / cols) as f32 * cell);
            let mut w = template.clone();
            w.area = [x, y, x + cell, y + cell];
            w.cls = "Button".into();
            edit_windows(&mut w, CELL_THUMBNAIL, &mut |t| t.area = [3.0, 3.0, cell - 3.0, cell - 3.0]);
            let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, cells);
            let Some(r) = c.root else { continue };
            commands.entity(r).insert((MaterialCell(n as u8), crate::icons::Tooltip(format!("Colors {}", n + 1))));
            if n as u8 == design {
                commands.entity(r).queue(|mut e: EntityWorldMut| {
                    if let Some(mut b) = e.get_mut::<UiButton>() {
                        b.selected = true;
                    }
                });
            }
            if let (Some(win), Some((h, _))) = (c.id(CELL_THUMBNAIL), ui.image(&mut images, s3bake::ui::cas_preset_thumb(key.2, n))) {
                crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
            }
        }
    }
    if let (Some(grid), Some(g), Some(template)) = (s.id(PALETTE_GRID), ui.find("ColorPicker", PALETTE_GRID).and_then(|w| w.grid), ui.layout("ColorsGridItem").cloned()) {
        let cols = g.columns.max(1) as usize;
        let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
        let cells = holder(&mut commands, grid);
        st.swatches = Some(cells);
        for (n, c) in crate::style::PALETTE.iter().enumerate() {
            let (x, y) = (g.padding[0] + (n % cols) as f32 * step.x + g.cell_padding[0], g.padding[1] + (n / cols) as f32 * step.y + g.cell_padding[1]);
            let mut w = template.clone();
            w.area = [x, y, x + g.cell[0], y + g.cell[1]];
            w.cls = "Button".into();
            edit_windows(&mut w, CELL_THUMBNAIL, &mut |t| t.area = [3.0, 3.0, g.cell[0] - 3.0, g.cell[1] - 3.0]);
            let cs = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, cells);
            let Some(r) = cs.root else { continue };
            commands.entity(r).insert(SwatchCell(n));
            if let Some(win) = cs.id(CELL_THUMBNAIL) {
                crate::hudpanels::picture(&mut commands, win, st.white.clone(), Color::srgb(c[0], c[1], c[2]));
            }
        }
    }
}
