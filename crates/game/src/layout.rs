//! The game's own interface layouts (baked by `s3bake::ui` from `UI.package`), put on screen
//! with Bevy UI: each window where its layout proc places it, drawing its images (stretched,
//! at their own size or nine-sliced; buttons by state: normal, highlighted, pressed, disabled,
//! selected), its caption in the game's fonts and text styles, its tooltip, and its children
//! (which the game lists front to back).
//! Game code finds windows by their control ids to fill them in and hear their clicks.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::LineHeight;
use bevy::ui::FocusPolicy;
use s3bake::ui::{ANCHOR_BOTTOM, ANCHOR_LEFT, ANCHOR_RIGHT, ANCHOR_TOP, TextStyle, UiBaked, UiDrawable, UiPlace, UiWindow, WIN_CLIP, WIN_ENABLED, WIN_IGNORE_MOUSE, WIN_VISIBLE};

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (sliders, combos))
            .add_systems(Startup, open_ui.after(crate::load_ui_font)).add_systems(Update, (open_ui_when_baked, set_icons, select_marked.before(button_states), button_states, fill_bars))
            .add_systems(Update, scrollbars.before(crate::buyhud::CatalogueControls));
    }
}

/// The baked layouts, text styles, and the pictures and fonts they use (decoded once).
#[derive(Resource)]
pub struct UiAssets {
    data: Arc<UiBaked>,
    /// The game's English text by key (`LocalizeString`).
    strings: Arc<HashMap<u64, String>>,
    by_id: HashMap<u64, usize>,
    styles: HashMap<u32, TextStyle>,
    pack: s3bake::PackReader,
    images: HashMap<u64, Option<(Handle<Image>, Vec2)>>,
    fonts: HashMap<u64, Handle<Font>>,
}

/// The default style's font (Helvetica Rounded) as the font of all text, as the game's.
pub(crate) fn open(fonts: &mut Assets<Font>) -> Option<UiAssets> {
    let root = s3bake::default_root();
    if !s3bake::ui_ready(&root) {
        return None;
    }
    let data = s3bake::load_ui(&root)?;
    let pack = s3bake::PackReader::open(&root.global_dir().join("ui.pack")).ok()?;
    let by_id = data.layouts.iter().enumerate().map(|(i, l)| (l.0, i)).collect();
    let styles: HashMap<u32, TextStyle> = data.styles.iter().cloned().collect();
    // (The game's text, for the words its code looks up by key.)
    let strings: HashMap<u64, String> = s3bake::read_value(&root.global_dir().join("strings.bin")).unwrap_or_default();
    let ui = UiAssets { data: Arc::new(data), by_id, styles, pack, images: HashMap::new(), fonts: HashMap::new(), strings: Arc::new(strings) };
    if let Some(bytes) = ui.styles.get(&0).and_then(|s| ui.pack.get::<Vec<u8>>(&(s3bake::ui::T_FONT, 0, s.font))) {
        let _ = fonts.insert(&Handle::<Font>::default(), Font::from_bytes(bytes));
    }
    Some(ui)
}

fn open_ui(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    if let Some(ui) = open(&mut fonts) {
        commands.insert_resource(ui);
    }
}

/// (On a first run the interface is converted while the town loads.)
fn open_ui_when_baked(mut commands: Commands, ui: Option<Res<UiAssets>>, mut fonts: ResMut<Assets<Font>>, state: Res<State<crate::AppState>>, mut tried: Local<bool>) {
    if ui.is_some() || *tried || *state.get() != crate::AppState::InGame {
        return;
    }
    *tried = true;
    if let Some(ui) = open(&mut fonts) {
        commands.insert_resource(ui);
    }
}

/// A window of a layout on screen, by its control id.
#[derive(Component)]
pub struct UiWin(pub u32);

/// A button drawn by state: its pictures (normal, disabled, highlighted, pressed, then the same
/// selected), the picture node, and an icon's tint per state.
#[derive(Component)]
pub struct UiButton {
    images: [Option<Handle<Image>>; 8],
    picture: Option<Entity>,
    icon: Option<(Entity, [Color; 8])>,
    /// Shown as selected (a toggled mode, the current speed).
    pub selected: bool,
    pub disabled: bool,
}

/// A window's (first) picture node, whose image game code can change (a portrait in the bust,
/// the household's faces on the skewer).
#[derive(Component, Clone, Copy)]
pub struct UiPicture(pub Entity);

/// A skinned scrollbar. Its controller supplies the total and visible rows (or columns),
/// and reads `value` after the arrows, track or thumb have been used.
#[derive(Component)]
pub struct UiScrollBar {
    pub value: usize,
    pub total: usize,
    pub visible: usize,
    vertical: bool,
    min_thumb: f32,
    thumb: Entity,
    up: Entity,
    down: Entity,
    grab: Option<f32>,
}

fn scroll_geometry(length: f32, arrow: f32, total: usize, visible: usize, minimum: f32, value: usize) -> (f32, f32, f32) {
    let track = (length - arrow * 2.0).max(0.0);
    let thumb = (track * visible as f32 / total.max(1) as f32).max(minimum).min(track);
    let travel = track - thumb;
    let start = arrow + travel * value.min(total.saturating_sub(visible)) as f32 / total.saturating_sub(visible).max(1) as f32;
    (start, thumb, travel)
}

#[allow(clippy::type_complexity)]
fn scrollbars(
    mut bars: Query<(Entity, &mut UiScrollBar, &ComputedNode, &bevy::ui::UiGlobalTransform, &InheritedVisibility)>,
    interactions: Query<&Interaction>,
    clicks: Query<&Interaction, Changed<Interaction>>,
    mut nodes: Query<&mut Node>,
    mut buttons: Query<&mut UiButton>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else { return };
    for (e, mut bar, computed, tf, visibility) in &mut bars {
        if !visibility.get() {
            bar.grab = None;
            continue;
        }
        let size = computed.size();
        let length = if bar.vertical { size.y } else { size.x };
        let arrow = if bar.vertical { size.x } else { size.y };
        let maximum = bar.total.saturating_sub(bar.visible);
        bar.value = bar.value.min(maximum);
        if clicks.get(bar.up).is_ok_and(|i| *i == Interaction::Pressed) {
            bar.value = bar.value.saturating_sub(1);
        }
        if clicks.get(bar.down).is_ok_and(|i| *i == Interaction::Pressed) {
            bar.value = (bar.value + 1).min(maximum);
        }
        let minimum = bar.min_thumb / computed.inverse_scale_factor();
        let (start, thumb, travel) = scroll_geometry(length, arrow, bar.total, bar.visible, minimum, bar.value);
        let point = window.physical_cursor_position().and_then(|p| tf.try_inverse().map(|i| i.transform_point2(p) + size * 0.5));
        if let Some(p) = point {
            let along = if bar.vertical { p.y } else { p.x };
            let over = interactions.get(e).is_ok_and(|i| *i != Interaction::None) || interactions.get(bar.thumb).is_ok_and(|i| *i != Interaction::None);
            if mouse.just_pressed(MouseButton::Left) && over && along >= arrow && along <= length - arrow {
                if (start..=start + thumb).contains(&along) {
                    bar.grab = Some(along - start);
                } else if along < start {
                    bar.value = bar.value.saturating_sub(bar.visible.max(1));
                } else {
                    bar.value = (bar.value + bar.visible.max(1)).min(maximum);
                }
            }
            if mouse.pressed(MouseButton::Left) && let Some(grab) = bar.grab && travel > 0.0 {
                bar.value = (((along - arrow - grab) / travel).clamp(0.0, 1.0) * maximum as f32).round() as usize;
            }
        }
        if !mouse.pressed(MouseButton::Left) {
            bar.grab = None;
        }
        let (start, thumb, _) = scroll_geometry(length, arrow, bar.total, bar.visible, minimum, bar.value);
        let scale = computed.inverse_scale_factor();
        if let Ok(mut n) = nodes.get_mut(bar.thumb) {
            if bar.vertical {
                n.top = Val::Px(start * scale);
                n.height = Val::Px(thumb * scale);
            } else {
                n.left = Val::Px(start * scale);
                n.width = Val::Px(thumb * scale);
            }
        }
        for (e, disabled) in [(bar.up, bar.value == 0), (bar.down, bar.value == maximum), (bar.thumb, maximum == 0)] {
            if let Ok(mut b) = buttons.get_mut(e) && b.disabled != disabled {
                b.disabled = disabled;
            }
        }
    }
}

/// A fill bar (`FillBarController`): game code sets `value` (0..1) and the bar's clip window
/// follows, from the start, the end or the middle (below it in its second colour).
#[derive(Component)]
pub struct UiFillBar {
    pub value: f32,
    direction: u8,
    colors: (Color, Color),
    size: Vec2,
    clip: Entity,
    fill: Option<Entity>,
}

const FILL_CLIP: u32 = 0x000f_bc02;
const FILL_FILL: u32 = 0x000f_bc01;

fn fill_bars(bars: Query<&UiFillBar, Changed<UiFillBar>>, mut nodes: Query<&mut Node>, pictures: Query<&UiPicture>, mut images: Query<&mut ImageNode>) {
    for b in &bars {
        let vertical = b.size.y > b.size.x;
        let full = if vertical { b.size.y } else { b.size.x };
        let v = b.value.clamp(0.0, 1.0) * full;
        let (start, len, below) = match b.direction {
            2 => (full - v, v, false),
            1 if v > full / 2.0 => (full / 2.0, v - full / 2.0, false),
            1 => (v, full / 2.0 - v, true),
            _ => (0.0, v, false),
        };
        if let Ok(mut n) = nodes.get_mut(b.clip) {
            if vertical {
                (n.top, n.height) = (Val::Px(start.round()), Val::Px(len.round()));
            } else {
                (n.left, n.width) = (Val::Px(start.round()), Val::Px(len.round()));
            }
        }
        if let Some(p) = b.fill.and_then(|f| pictures.get(f).ok())
            && let Ok(mut img) = images.get_mut(p.0)
        {
            img.color = if below { b.colors.1 } else { b.colors.0 };
        }
    }
}

/// A picture for an icon button's icon (set once the button is there).
#[derive(Component)]
pub struct SetIcon(pub Handle<Image>);

fn set_icons(mut commands: Commands, q: Query<(Entity, &UiButton, &SetIcon)>, mut pics: Query<&mut ImageNode>) {
    for (e, b, s) in &q {
        if let Some(i) = b.icon()
            && let Ok(mut img) = pics.get_mut(i)
        {
            img.image = s.0.clone();
        }
        commands.entity(e).remove::<SetIcon>();
    }
}

impl UiButton {
    /// The icon picture of an icon button (whose image game code sets: a tab's career icon).
    pub fn icon(&self) -> Option<Entity> {
        self.icon.map(|i| i.0)
    }
}

/// A layout put on screen: its root, and its windows (and their texts) by control id.
#[derive(Default, Clone)]
pub struct Spawned {
    pub root: Option<Entity>,
    ids: HashMap<u32, Entity>,
    texts: HashMap<u32, Entity>,
    comments: Vec<(String, Entity)>,
    /// Every window with an id, and its parent (ids repeat in item templates: each skewer slot
    /// has its thumbnail as 1 and its button as 3).
    all: Vec<(u32, Entity)>,
    parents: HashMap<Entity, Entity>,
    text_of: HashMap<Entity, Entity>,
}

impl Spawned {
    /// The window with a control id.
    pub fn id(&self, id: u32) -> Option<Entity> {
        self.ids.get(&id).copied()
    }
    /// The text of the window with a control id.
    pub fn text(&self, id: u32) -> Option<Entity> {
        self.texts.get(&id).copied()
    }
    /// The (first) window the designers commented so (for windows without an id).
    pub fn comment(&self, c: &str) -> Option<Entity> {
        self.comments.iter().find(|(n, _)| n == c).map(|(_, e)| *e)
    }
    /// The window with a control id under another (an item's own child).
    pub fn within(&self, ancestor: Entity, id: u32) -> Option<Entity> {
        self.all.iter().filter(|(i, _)| *i == id).map(|(_, e)| *e).find(|e| self.is_within(*e, ancestor))
    }
    /// A window's text (the window being a `Text` or a captioned button).
    pub fn text_of(&self, window: Entity) -> Option<Entity> {
        self.text_of.get(&window).copied()
    }
    /// Every window with a control id (ids repeat in a layout's copies of a piece).
    pub fn all_with(&self, id: u32) -> Vec<Entity> {
        self.all.iter().filter(|(i, _)| *i == id).map(|(_, e)| *e).collect()
    }
    /// The (first) window commented so under another.
    pub fn comment_within(&self, ancestor: Entity, c: &str) -> Option<Entity> {
        self.comments.iter().filter(|(n, _)| n == c).map(|(_, e)| *e).find(|e| self.is_within(*e, ancestor))
    }
    fn is_within(&self, e: Entity, ancestor: Entity) -> bool {
        let mut at = e;
        for _ in 0..32 {
            match self.parents.get(&at) {
                Some(&p) if p == ancestor => return true,
                Some(&p) => at = p,
                None => return false,
            }
        }
        false
    }
    /// The same windows under other ids (a layout with its own copy of another's controls:
    /// buy mode's puck is the HUD's, under ids of its own).
    pub fn renamed(&self, f: impl Fn(u32) -> u32) -> Spawned {
        let mut s = self.clone();
        s.ids = self.ids.iter().map(|(k, v)| (f(*k), *v)).collect();
        s.texts = self.texts.iter().map(|(k, v)| (f(*k), *v)).collect();
        s.all = self.all.iter().map(|(k, v)| (f(*k), *v)).collect();
        s
    }
}

/// Lights a button as soon as it's spawned (a chosen tab or cell).
#[derive(Component)]
pub struct Selected;

fn select_marked(mut q: Query<&mut UiButton, Added<Selected>>) {
    for mut b in &mut q {
        b.selected = true;
    }
}

/// The custom controls that load a layout of their own name into themselves (`PreInit`).
const EMBEDDED: [&str; 5] = ["BubbleMeter", "MoodBar", "RelationshipBar", "DifficultyMeter", "ChallengeProgressMeter"];

pub fn color(argb: u32) -> Color {
    Color::srgba_u8((argb >> 16) as u8, (argb >> 8) as u8, argb as u8, (argb >> 24) as u8)
}

impl UiAssets {
    /// A layout by its name (as the game's code loads it: `HUDSimDisplay`).
    pub fn layout(&self, name: &str) -> Option<&UiWindow> {
        self.by_id.get(&s3pkg::fnv64(name)).and_then(|&i| self.data.layouts[i].1.first()).map(|w| &w.1)
    }

    /// A window by control id anywhere in a layout (any of its exports).
    pub fn find(&self, name: &str, id: u32) -> Option<&UiWindow> {
        self.by_id.get(&s3pkg::fnv64(name)).and_then(|&i| self.data.layouts[i].1.iter().find_map(|w| w.1.find(id)))
    }

    /// The game's words for a text key (`Ui/Caption/...:Name`).
    pub fn localize(&self, key: &str) -> Option<String> {
        self.strings.get(&s3pkg::fnv64(key)).cloned()
    }

    /// The game's words for a text key's hash (a world file's name and description).
    pub fn localize_key(&self, key: u64) -> Option<String> {
        self.strings.get(&key).cloned()
    }

    /// Everything baked from the interface (buy mode's catalogue, each object's place in it).
    pub fn baked(&self) -> &std::sync::Arc<UiBaked> {
        &self.data
    }

    /// One of a layout's exported windows (`GetWindowByExportID`).
    pub fn export(&self, name: &str, id: u32) -> Option<&UiWindow> {
        self.by_id.get(&s3pkg::fnv64(name)).and_then(|&i| self.data.layouts[i].1.iter().find(|w| w.0 == id)).map(|w| &w.1)
    }

    /// An interface picture and its size.
    pub fn image(&mut self, images: &mut Assets<Image>, key: u64) -> Option<(Handle<Image>, Vec2)> {
        if key == 0 {
            return None;
        }
        if let Some(h) = self.images.get(&key) {
            return h.clone();
        }
        let img = self.pack.get::<Vec<u8>>(&(s3bake::ui::T_IMAGE, 0, key)).and_then(|png| s3bake::gamedata::decode_icon(&png)).map(|(w, h, px)| {
            let img = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
            (images.add(img), Vec2::new(w as f32, h as f32))
        });
        self.images.insert(key, img.clone());
        img
    }

    /// The font and size of a text style (`TextFont` of a window; 0 the default).
    pub fn text_font(&mut self, fonts: &mut Assets<Font>, style: u32) -> (TextFont, LineHeight) {
        let st = self.styles.get(&style).or_else(|| self.styles.get(&0)).cloned().unwrap_or_default();
        let handle = match self.fonts.get(&st.font) {
            Some(h) => Some(h.clone()),
            None => self.pack.get::<Vec<u8>>(&(s3bake::ui::T_FONT, 0, st.font)).map(|b| {
                let h = fonts.add(Font::from_bytes(b));
                self.fonts.insert(st.font, h.clone());
                h
            }),
        };
        let mut f = TextFont::from_font_size(st.size.max(6.0));
        if let Some(h) = handle {
            f.font = h.into();
        }
        (f, LineHeight::Px(st.line.max(st.size)))
    }

    /// Puts a layout on screen (its root at the top level, placed against the window).
    pub fn spawn(&mut self, commands: &mut Commands, images: &mut Assets<Image>, fonts: &mut Assets<Font>, name: &str) -> Option<Spawned> {
        let data = self.data.clone();
        let w = data.layouts.get(*self.by_id.get(&s3pkg::fnv64(name))?).and_then(|l| l.1.first()).map(|w| &w.1)?;
        let mut out = Spawned::default();
        let root = self.spawn_window(commands, images, fonts, w, None, &mut out);
        out.root = Some(root);
        Some(out)
    }

    /// Puts a (changed copy of a) layout's window on screen on its own.
    pub fn spawn_root(&mut self, commands: &mut Commands, images: &mut Assets<Image>, fonts: &mut Assets<Font>, w: &UiWindow) -> Spawned {
        let mut out = Spawned::default();
        let root = self.spawn_window(commands, images, fonts, w, None, &mut out);
        out.root = Some(root);
        out
    }

    /// Puts a window (and what's under it) under a parent: a layout's item template, as a
    /// moodlet's grid cell or a queued interaction.
    pub fn spawn_under(&mut self, commands: &mut Commands, images: &mut Assets<Image>, fonts: &mut Assets<Font>, w: &UiWindow, parent: Entity) -> Spawned {
        let mut out = Spawned::default();
        let root = self.spawn_window(commands, images, fonts, w, Some(parent), &mut out);
        out.root = Some(root);
        out
    }

    /// The data of a layout window, to spawn copies of (cloned out of the shared data).
    pub fn template(&self, layout: &str, comment: &str) -> Option<UiWindow> {
        self.layout(layout)?.find_comment(comment).cloned()
    }

    fn spawn_window(&mut self, commands: &mut Commands, images: &mut Assets<Image>, fonts: &mut Assets<Font>, w: &UiWindow, parent: Option<Entity>, out: &mut Spawned) -> Entity {
        let mut node = place(w);
        if w.flags & WIN_CLIP != 0 {
            node.overflow = Overflow::clip();
        }
        let vis = if w.visible() { Visibility::Inherited } else { Visibility::Hidden };
        let e = commands.spawn((node, vis, UiWin(w.id))).id();
        if let Some(p) = parent {
            commands.entity(e).insert(ChildOf(p));
        }
        if !w.comment.is_empty() {
            commands.entity(e).insert(Name::new(w.comment.clone()));
            out.comments.push((w.comment.clone(), e));
        }
        if w.id != 0 {
            out.ids.insert(w.id, e);
            out.all.push((w.id, e));
        }
        if let Some(p) = parent {
            out.parents.insert(e, p);
        }
        let size = Vec2::new(w.area[2] - w.area[0], w.area[3] - w.area[1]);
        let shade = color(w.shade);
        let is_button = w.cls.contains("Button") || w.combo.is_some();
        let mut button: Option<UiButton> = is_button.then(|| UiButton { images: Default::default(), picture: None, icon: None, selected: false, disabled: w.flags & WIN_ENABLED == 0 });
        match &w.drawable {
            // (A slider's thumb (the first: the game's plumbob) where its value puts it over its
            // track (the second): see `sliders`.)
            Some(UiDrawable::Multi(list)) if w.slider.is_some() && list.len() >= 2 => {
                self.drawable(commands, images, &list[1], e, size, shade, None);
                if let UiDrawable::Std { images: keys, .. } = &list[0]
                    && let Some((h, isize)) = self.image(images, keys[0])
                {
                    let [min, max, value, orientation] = w.slider.unwrap_or_default();
                    let thumb = commands.spawn((Node { position_type: PositionType::Absolute, width: Val::Px(isize.x), height: Val::Px(isize.y), ..default() }, ImageNode::new(h), Pickable::IGNORE, ChildOf(e))).id();
                    commands.entity(e).insert((UiSlider { min, max, value, thumb, thumb_size: isize, vertical: orientation == 0.0, grab: false }, Interaction::default(), FocusPolicy::Block, crate::hud::BlocksWorld));
                }
            }
            Some(d) => {
                self.drawable(commands, images, d, e, size, shade, button.as_mut());
                // (A check box's box at its left, the middle of its height.)
                if w.align == 1
                    && let (Some(pic), UiDrawable::Std { scale: 1, images: keys, .. }) = (button.as_ref().and_then(|b| b.picture), d)
                    && let Some((_, isize)) = self.image(images, keys[0])
                {
                    commands.entity(pic).insert(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(((size.y - isize.y) * 0.5).round()), width: Val::Px(isize.x), height: Val::Px(isize.y), ..default() });
                }
            }
            // (A plain fill, modulated by the shade as the game's.)
            None if w.fill >> 24 != 0 && w.shade >> 24 != 0 && !is_button && w.cls != "Text" => {
                let (f, sh) = (color(w.fill).to_srgba(), shade.to_srgba());
                commands.entity(e).insert(BackgroundColor(Color::srgba(f.red * sh.red, f.green * sh.green, f.blue * sh.blue, f.alpha * sh.alpha)));
            }
            None => {}
        }
        // (Clicks: buttons take them; panels that draw something keep them from the world.)
        let ignore = w.flags & WIN_IGNORE_MOUSE != 0;
        if let Some(b) = button {
            commands.entity(e).insert((Button, b, FocusPolicy::Block));
        } else if w.drawable.is_some() && !ignore && w.visible() {
            commands.entity(e).insert((Interaction::default(), FocusPolicy::Block, crate::hud::BlocksWorld));
        } else {
            commands.entity(e).insert(Pickable::IGNORE);
        }
        if is_button {
            commands.entity(e).insert(crate::hud::BlocksWorld);
        }
        if !w.tooltip.is_empty() && !w.tooltip.contains('/') && !w.tooltip.contains(':') {
            if !is_button && w.drawable.is_none() {
                commands.entity(e).insert(Interaction::default());
            }
            commands.entity(e).insert(crate::icons::Tooltip(w.tooltip.clone()));
        }
        if w.cls == "Text" || w.cls == "TextEdit" || (is_button && !w.caption.is_empty()) {
            let caption = if w.caption.contains('/') && w.caption.contains(':') { String::new() } else { w.caption.clone() };
            let t = self.spawn_text(commands, fonts, w, &caption, e);
            out.texts.insert(w.id, t);
            out.text_of.insert(e, t);
        }
        // (The game's windows list their children front to back: the first is drawn on top.)
        for c in w.children.iter().rev() {
            self.spawn_window(commands, images, fonts, c, Some(e), out);
        }
        if let Some((vertical, minimum, parts)) = &w.scrollbar {
            let width = size.x.max(1.0);
            let height = size.y.max(1.0);
            let arrow = if *vertical { width } else { height };
            let child = |id: u32, drawable: Option<UiDrawable>, area: [f32; 4], place: UiPlace, button: bool| UiWindow {
                id, cls: if button { "Button" } else { "Window" }.into(), flags: WIN_VISIBLE | WIN_ENABLED, shade: w.shade, drawable, area, place, ..default()
            };
            let skin = |i: usize| parts.get(i).cloned().flatten();
            let track = child(0, skin(4), if *vertical { [0.0, arrow, 0.0, -arrow] } else { [arrow, 0.0, -arrow, 0.0] }, UiPlace::Simple(15), false);
            self.spawn_window(commands, images, fonts, &track, Some(e), out);
            let thumb = child(0, skin(3), if *vertical { [0.0, arrow, width, arrow + minimum] } else { [arrow, 0.0, arrow + minimum, height] }, UiPlace::Fixed, true);
            let thumb = self.spawn_window(commands, images, fonts, &thumb, Some(e), out);
            let up = child(0x0600_0000, skin(1), [0.0, 0.0, if *vertical { width } else { arrow }, if *vertical { arrow } else { height }], UiPlace::Fixed, true);
            let up = self.spawn_window(commands, images, fonts, &up, Some(e), out);
            let down = child(0x0600_0001, skin(6), if *vertical { [0.0, -arrow, width, 0.0] } else { [-arrow, 0.0, 0.0, height] }, UiPlace::Simple(if *vertical { 6 } else { 9 }), true);
            let down = self.spawn_window(commands, images, fonts, &down, Some(e), out);
            commands.entity(e).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld, UiScrollBar { value: 0, total: 0, visible: 1, vertical: *vertical, min_thumb: *minimum, thumb, up, down, grab: None }));
        }
        if let (Some((direction, main, second)), Some(clip)) = (w.fill_bar, out.within(e, FILL_CLIP)) {
            commands.entity(e).insert(UiFillBar { value: 0.0, direction, colors: (color(main), color(second)), size, clip, fill: out.within(e, FILL_FILL) });
        }
        // (Custom controls bring their own layout in as their child: a bubble meter's bubbles.)
        // (A combo box shows its choice in its own words: see `combos`.)
        if let Some(c) = &w.combo {
            let (font, line) = self.text_font(fonts, w.font);
            let col = color(c.colors.first().copied().unwrap_or(0xff00_2d74));
            let holder = commands
                .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(8.0), right: Val::Px(24.0), top: Val::Px(0.0), bottom: Val::Px(0.0), align_items: AlignItems::Center, ..default() }, Pickable::IGNORE, ChildOf(e)))
                .id();
            let text = commands.spawn((Text::new(""), font, line, TextColor(col), TextLayout::new(Justify::Left, LineBreak::NoWrap), Pickable::IGNORE, ChildOf(holder))).id();
            commands.entity(e).insert(UiCombo { items: Vec::new(), selected: 0, open: false, text, list: None, combo: c.clone(), font: w.font, shown: None });
        }
        if EMBEDDED.contains(&w.cls.as_str()) {
            let data = self.data.clone();
            if let Some(inner) = self.by_id.get(&s3pkg::fnv64(&w.cls)).and_then(|&i| data.layouts[i].1.first()) {
                self.spawn_window(commands, images, fonts, &inner.1, Some(e), out);
            }
        }
        e
    }

    /// A check box's box's width (its picture's).
    fn check_width(&mut self, w: &UiWindow) -> f32 {
        match &w.drawable {
            // (Its picture's already loaded: the window's drawn before its words.)
            Some(UiDrawable::Std { images: keys, .. }) => self.images.get(&keys[0]).and_then(|i| i.as_ref()).map_or(0.0, |(_, s)| s.x),
            _ => 0.0,
        }
    }

    /// A text filling its window, aligned as the window says.
    fn spawn_text(&mut self, commands: &mut Commands, fonts: &mut Assets<Font>, w: &UiWindow, caption: &str, parent: Entity) -> Entity {
        let (font, line) = self.text_font(fonts, w.font);
        // (A text's alignment: across 0 left, 1 centre, 2 right, 4 justified (paragraphs);
        // down 0 top, 1 middle, 2 bottom, 3 middle. Buttons' captions sit in the middle.)
        let button = w.cls.contains("Button");
        // (A check box's words start after its box, at its left.)
        let check = button && w.align == 1 && w.halign == 0 && matches!(&w.drawable, Some(UiDrawable::Std { scale: 1, .. }));
        let (justify, align_x) = match w.halign {
            _ if check => (Justify::Left, JustifyContent::FlexStart),
            _ if button => (Justify::Center, JustifyContent::Center),
            1 => (Justify::Center, JustifyContent::Center),
            2 => (Justify::Right, JustifyContent::FlexEnd),
            _ => (Justify::Left, JustifyContent::FlexStart),
        };
        let align_y = match w.valign {
            _ if button => AlignItems::Center,
            // (A text box's words start at its top.)
            _ if w.cls == "TextEdit" => AlignItems::FlexStart,
            1 | 3 => AlignItems::Center,
            2 => AlignItems::FlexEnd,
            _ => AlignItems::FlexStart,
        };
        let col = w.colors.first().copied().map_or(Color::BLACK, color);
        let holder = commands
            .spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(if check { self.check_width(w) + 6.0 } else { 0.0 }), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), justify_content: align_x, align_items: align_y, ..default() },
                Pickable::IGNORE,
                ChildOf(parent),
            ))
            .id();
        let wrap = if w.wrap == 0 { LineBreak::NoWrap } else { LineBreak::WordBoundary };
        commands.spawn((Text::new(caption), font, line, TextColor(col), TextLayout::new(justify, wrap), Pickable::IGNORE, ChildOf(holder))).id()
    }

    /// A drawable as picture nodes under its window.
    #[allow(clippy::too_many_arguments)]
    fn drawable(&mut self, commands: &mut Commands, images: &mut Assets<Image>, d: &UiDrawable, parent: Entity, size: Vec2, shade: Color, button: Option<&mut UiButton>) {
        match d {
            UiDrawable::Std { images: keys, scale, borders, .. } => {
                let pics: Vec<Option<(Handle<Image>, Vec2)>> = keys.iter().map(|k| self.image(images, *k)).collect();
                let Some((first, isize)) = pics.iter().flatten().next().cloned() else { return };
                let mode = match scale {
                    2 => NodeImageMode::Sliced(TextureSlicer {
                        border: BorderRect { min_inset: Vec2::new(borders[0] * isize.x, borders[1] * isize.y), max_inset: Vec2::new(borders[2] * isize.x, borders[3] * isize.y) },
                        center_scale_mode: SliceScaleMode::Stretch,
                        sides_scale_mode: SliceScaleMode::Stretch,
                        max_corner_scale: 1.0,
                    }),
                    _ => NodeImageMode::Stretch,
                };
                let node = if *scale == 1 { centred(isize, size) } else { fill() };
                let pic = commands.spawn((node, ImageNode { image: first, color: shade, image_mode: mode, ..default() }, Pickable::IGNORE, ChildOf(parent))).id();
                commands.entity(parent).insert_if_new(UiPicture(pic));
                if let Some(b) = button {
                    for (i, p) in pics.into_iter().enumerate() {
                        b.images[i] = p.map(|p| p.0);
                    }
                    b.picture = Some(pic);
                }
            }
            UiDrawable::Image { image, flags, halign, valign, scale, colors, .. } => {
                let Some((h, isize)) = self.image(images, *image) else { return };
                // (Fitted to the window keeping its shape, or at its own size; aligned.)
                let s = if flags & 1 != 0 { (size.x / isize.x).min(size.y / isize.y) * scale } else { *scale };
                let dim = isize * s;
                let x = match halign {
                    1 => 0.0,
                    2 => size.x - dim.x,
                    3 => (size.x - dim.x) * 0.5,
                    _ => 0.0,
                };
                let y = match valign {
                    1 => 0.0,
                    2 => size.y - dim.y,
                    3 => (size.y - dim.y) * 0.5,
                    _ => 0.0,
                };
                let node = if *halign == 0 && *valign == 0 && *flags == 0 {
                    fill()
                } else {
                    Node { position_type: PositionType::Absolute, left: Val::Px(x), top: Val::Px(y), width: Val::Px(dim.x), height: Val::Px(dim.y), ..default() }
                };
                let tint = colors.first().copied().map_or(shade, color);
                let pic = commands.spawn((node, ImageNode { image: h, color: tint, image_mode: NodeImageMode::Stretch, ..default() }, Pickable::IGNORE, ChildOf(parent))).id();
                commands.entity(parent).insert_if_new(UiPicture(pic));
                if let Some(b) = button
                    && colors.len() >= 8
                {
                    let mut c = [Color::WHITE; 8];
                    for (i, v) in colors.iter().take(8).enumerate() {
                        c[i] = color(*v);
                    }
                    b.icon = Some((pic, c));
                }
            }
            UiDrawable::Multi(list) => {
                let mut button = button;
                for d in list {
                    self.drawable(commands, images, d, parent, size, shade, button.as_deref_mut());
                }
            }
        }
    }
}

fn fill() -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }
}

/// A picture at its own size, centred in the window.
fn centred(isize: Vec2, size: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(((size.x - isize.x) * 0.5).round()),
        top: Val::Px(((size.y - isize.y) * 0.5).round()),
        width: Val::Px(isize.x),
        height: Val::Px(isize.y),
        ..default()
    }
}

/// Where a window goes in its parent, by its layout proc (see [`UiPlace`]).
fn place(w: &UiWindow) -> Node {
    let [x1, y1, x2, y2] = w.area;
    let mut n = Node { position_type: PositionType::Absolute, ..default() };
    let (wd, ht) = (Val::Px(x2 - x1), Val::Px(y2 - y1));
    match w.place {
        UiPlace::Fixed => {
            (n.left, n.top, n.width, n.height) = (Val::Px(x1), Val::Px(y1), wd, ht);
        }
        UiPlace::Simple(a) => {
            match (a & ANCHOR_LEFT != 0, a & ANCHOR_RIGHT != 0) {
                (true, true) => (n.left, n.right) = (Val::Px(x1), Val::Px(-x2)),
                (false, true) => (n.right, n.width) = (Val::Px(-x2), wd),
                _ => (n.left, n.width) = (Val::Px(x1), wd),
            }
            match (a & ANCHOR_TOP != 0, a & ANCHOR_BOTTOM != 0) {
                (true, true) => (n.top, n.bottom) = (Val::Px(y1), Val::Px(-y2)),
                (false, true) => (n.bottom, n.height) = (Val::Px(-y2), ht),
                _ => (n.top, n.height) = (Val::Px(y1), ht),
            }
        }
        UiPlace::Hud(a, [dw, dh]) => {
            match (a & ANCHOR_LEFT != 0, a & ANCHOR_RIGHT != 0) {
                (true, true) => (n.left, n.right) = (Val::Px(x1), Val::Px(dw - x2)),
                (false, true) => (n.right, n.width) = (Val::Px(dw - x2), wd),
                _ => (n.left, n.width) = (Val::Px(x1), wd),
            }
            match (a & ANCHOR_TOP != 0, a & ANCHOR_BOTTOM != 0) {
                (true, true) => (n.top, n.bottom) = (Val::Px(y1), Val::Px(dh - y2)),
                (false, true) => (n.bottom, n.height) = (Val::Px(dh - y2), ht),
                _ => (n.top, n.height) = (Val::Px(y1), ht),
            }
        }
        UiPlace::Center(v) => {
            n.left = Val::Percent(50.0);
            n.margin.left = Val::Px(-(x2 - x1) * 0.5);
            n.top = Val::Percent(v * 100.0);
            n.margin.top = Val::Px(-(y2 - y1) * v);
            (n.width, n.height) = (wd, ht);
        }
    }
    n
}

/// Buttons show their state's picture (and tint their icon), as the game's.
fn button_states(mut buttons: Query<(&Interaction, &UiButton), Or<(Changed<Interaction>, Changed<UiButton>)>>, mut pics: Query<&mut ImageNode>) {
    for (i, b) in &mut buttons {
        let base = if b.disabled {
            1
        } else {
            match i {
                Interaction::Pressed => 3,
                Interaction::Hovered => 2,
                Interaction::None => 0,
            }
        };
        let state = base + if b.selected { 4 } else { 0 };
        if let Some(p) = b.picture
            && let Ok(mut img) = pics.get_mut(p)
        {
            let pick = b.images[state].clone().or_else(|| b.images[base].clone()).or_else(|| b.images[0].clone());
            if let Some(h) = pick
                && img.image != h
            {
                img.image = h;
            }
        }
        if let Some((icon, colors)) = b.icon
            && let Ok(mut img) = pics.get_mut(icon)
        {
            img.color = colors[state];
        }
    }
}

/// A slider (`Slider`): its range and value (whole steps), its thumb. Dragged or clicked
/// along, it takes the value under the pointer; game code reads and sets `value`.
#[derive(Component)]
pub struct UiSlider {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    thumb: Entity,
    thumb_size: Vec2,
    vertical: bool,
    grab: bool,
}

fn sliders(
    mut q: Query<(&mut UiSlider, Option<&Interaction>, &ComputedNode, &bevy::ui::UiGlobalTransform, &InheritedVisibility)>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut nodes: Query<&mut Node>,
) {
    let Ok(window) = windows.single() else { return };
    for (mut s, i, computed, tf, vis) in &mut q {
        if !vis.get() {
            continue;
        }
        let scale = computed.inverse_scale_factor();
        let size = computed.size() * scale;
        let (length, thumb) = if s.vertical { (size.y, s.thumb_size.y) } else { (size.x, s.thumb_size.x) };
        let travel = (length - thumb).max(1.0);
        if i == Some(&Interaction::Pressed) && mouse.just_pressed(MouseButton::Left) {
            s.grab = true;
        }
        if !mouse.pressed(MouseButton::Left) {
            s.grab = false;
        }
        if s.grab
            && let Some(p) = window.physical_cursor_position().and_then(|p| tf.try_inverse().map(|t| (t.transform_point2(p) + computed.size() * 0.5) * scale))
        {
            let along = if s.vertical { length - p.y } else { p.x };
            let f = ((along - thumb * 0.5) / travel).clamp(0.0, 1.0);
            let v = (s.min + f * (s.max - s.min)).round();
            if v != s.value {
                s.value = v;
            }
        }
        let f = if s.max > s.min { ((s.value - s.min) / (s.max - s.min)).clamp(0.0, 1.0) } else { 0.0 };
        if let Ok(mut n) = nodes.get_mut(s.thumb) {
            let (left, top) = if s.vertical { ((size.x - s.thumb_size.x) * 0.5, (1.0 - f) * travel) } else { (f * travel, (size.y - s.thumb_size.y) * 0.5) };
            let want = (Val::Px(left.round()), Val::Px(top.round()));
            if (n.left, n.top) != want {
                (n.left, n.top) = want;
            }
        }
    }
}

/// A combo box (`Sims3ComboBox`): its choices (set by game code), the one chosen, and its
/// drop-down list (the game's pull-down picture, a row a choice, lit under the pointer).
#[derive(Component)]
pub struct UiCombo {
    pub items: Vec<String>,
    pub selected: usize,
    pub open: bool,
    text: Entity,
    list: Option<Entity>,
    combo: s3bake::ui::UiCombo,
    font: u32,
    shown: Option<(Vec<String>, usize, bool)>,
}

/// One of a combo box's choices in its list.
#[derive(Component)]
struct ComboRow(Entity, usize);

/// A combo box's rows' height.
const COMBO_ROW: f32 = 22.0;

#[allow(clippy::type_complexity)]
fn combos(
    mut commands: Commands,
    clicks: Query<(Entity, &Interaction), (Changed<Interaction>, With<UiCombo>)>,
    mut all: Query<(Entity, &mut UiCombo)>,
    rows: Query<(&Interaction, &ComboRow), Changed<Interaction>>,
    mut hover: Query<(&Interaction, &ComboRow, &mut BackgroundColor)>,
    mut texts: Query<&mut Text>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    // A click on the box opens or closes its list.
    for (e, i) in &clicks {
        if *i == Interaction::Pressed
            && let Ok((_, mut c)) = all.get_mut(e)
            && !c.items.is_empty()
        {
            c.open = !c.open;
            play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
        }
    }
    // A choice taken from the list.
    for (i, r) in &rows {
        if *i == Interaction::Pressed
            && let Ok((_, mut c)) = all.get_mut(r.0)
        {
            c.selected = r.1;
            c.open = false;
        }
    }
    for (i, r, mut bg) in &mut hover {
        let lit = *i != Interaction::None;
        if let Ok((_, c)) = all.get(r.0) {
            let want = if lit { color(c.combo.colors.get(3).copied().unwrap_or(0xbf52_77b5)) } else { Color::NONE };
            if bg.0 != want {
                bg.0 = want;
            }
        }
    }
    let Some(mut ui) = ui else { return };
    for (e, mut c) in &mut all {
        let want = (c.items.clone(), c.selected, c.open);
        if c.shown.as_ref() == Some(&want) {
            continue;
        }
        c.shown = Some(want);
        if let Ok(mut t) = texts.get_mut(c.text) {
            let s = c.items.get(c.selected).cloned().unwrap_or_default();
            if t.0 != s {
                t.0 = s;
            }
        }
        if let Some(l) = c.list.take() {
            commands.entity(l).try_despawn();
        }
        if !c.open {
            continue;
        }
        // The list, under the box.
        let h = c.items.len() as f32 * COMBO_ROW + 8.0;
        let list = commands
            .spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Percent(100.0), margin: UiRect::top(Val::Px(c.combo.offset)), height: Val::Px(h), padding: UiRect::all(Val::Px(4.0)), flex_direction: FlexDirection::Column, ..default() },
                GlobalZIndex(60),
                Interaction::default(),
                FocusPolicy::Block,
                crate::hud::BlocksWorld,
                ChildOf(e),
            ))
            .id();
        if let Some(d) = c.combo.pulldown.clone() {
            ui.drawable(&mut commands, &mut images, &d, list, Vec2::new(0.0, h), Color::WHITE, None);
        }
        let (font, line) = ui.text_font(&mut fonts, c.font);
        let colors = c.combo.colors.clone();
        for (k, item) in c.items.iter().enumerate() {
            let row = commands
                .spawn((Node { height: Val::Px(COMBO_ROW), padding: UiRect::left(Val::Px(6.0)), align_items: AlignItems::Center, ..default() }, Button, BackgroundColor(Color::NONE), ComboRow(e, k), ChildOf(list)))
                .id();
            let col = color(if k == c.selected { colors.get(2).copied().unwrap_or(0xffff_ffff) } else { colors.first().copied().unwrap_or(0xff00_2d74) });
            commands.spawn((Text::new(item.clone()), font.clone(), line, TextColor(col), Pickable::IGNORE, ChildOf(row)));
        }
        c.list = Some(list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_thumb_tracks_rows_and_stays_in_the_track() {
        let (top, size, travel) = scroll_geometry(200.0, 16.0, 20, 4, 30.0, 0);
        assert_eq!(top, 16.0);
        assert!((size - 33.6).abs() < 0.001);
        let (bottom, same_size, same_travel) = scroll_geometry(200.0, 16.0, 20, 4, 30.0, 16);
        assert_eq!(size, same_size);
        assert_eq!(travel, same_travel);
        assert!((bottom + size - 184.0).abs() < 0.001);
        assert_eq!(scroll_geometry(200.0, 16.0, 20, 4, 30.0, 100), (bottom, size, travel));
        assert_eq!(scroll_geometry(20.0, 16.0, 0, 4, 30.0, 0), (16.0, 0.0, 0.0));
    }
}
