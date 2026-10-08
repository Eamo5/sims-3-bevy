//! Buy mode in the game's own look: the `Buy` layout (UI.package) with its puck and tools, and
//! the catalogue as UI.dll's `BuyController` drives it. By function: the categories' buttons,
//! the chosen category's subcategory tabs and its "All" tab, its objects in the game's cells, two
//! rows widening across the screen to fit them (and expanding upwards). By room: a tab per room,
//! the room's picture of buttons, the objects of the one clicked. Over it, the preview panel for
//! the object in hand (`BBCatalogPreviewPanelController`: its picture, name, price, description
//! and its designs to choose from).

use std::collections::HashMap;

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use s3bake::Key;
use s3bake::ui::{ObjBuy, UiPlace};

use crate::buy::{BuyButton, BuyMode, WALLPAPER_TAB};
use crate::layout::{Spawned, UiAssets, UiButton};
use crate::livehud::{LiveHud, pressed, set_text, set_visible};
use crate::loading::{Catalog, CatalogEntry};
use crate::{AppState, PlayMode};

pub struct BuyHudPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CatalogueControls;

impl Plugin for BuyHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(PlayMode::Live), spawn_buy_hud.after(crate::buy::reset_buy_mode))
            .add_systems(Update, (show, catalogue_controls.in_set(CatalogueControls), tools, fill_catalogue, preview_panel).chain().after(crate::hud::pointer_over_ui).run_if(in_state(PlayMode::Live)).run_if(resource_exists::<BuyHud>));
    }
}

// The windows' control ids (`BuyController.ControlID`).
/// The layout's own copy of the HUD puck, under ids 0x00100000 below the HUD's.
const PUCK_BASE: u32 = 0x8fdf_fa00;
const PUCK_OFFSET: u32 = 0x0010_0000;
const UNDO: u32 = 0x05ac_150d;
const REDO: u32 = 0x05ac_150e;
const TOOL_HAND: u32 = 0x05b7_0640;
const TOOL_SELL: u32 = 0x05b7_0728;
const TOOL_CLONE: u32 = 0x05b7_06b0;
const TOOL_DESIGN: u32 = 0x05b7_0760;
const DAY_NIGHT: u32 = 0x05b7_06c0;
const INDOOR_GRID: u32 = 0x05b7_06d0;
const BLUEPRINT: u32 = 0x06f9_c80a;
const BY_ROOM: u32 = 0x05b1_7300;
const BY_FUNCTION: u32 = 0x05b1_7301;
const FAMILY_INVENTORY: u32 = 0x05b1_7302;
const COLLECTIONS: u32 = 0x05b7_06d1;
const CATEGORY_PANEL: u32 = 0x06e7_40b8;
/// The categories' buttons, in the by-function catalogue's order.
const FIRST_CATEGORY: u32 = 0x06e8_e6d0;
/// The categories' two pages (with more than one expansion's category): the toggle.
const TOGGLE_PAGE: u32 = 0x0c6d_e960;
/// The expansions' categories on the first page (pets, stage props) and on the second.
const EXTRA_CATEGORIES: [(u32, u32); 2] = [(0x06e8_e6db, 0x06e8_e6df), (0x06e8_e6dc, 0x06e8_e6e0)];
const PAGE_ONE_END: u32 = 0x06e8_e6db;
const WIN_BY_ROOM: u32 = 0x2000;
const WIN_BY_FUNCTION: u32 = 0x2001;
const WIN_ROOM_EMPTY: u32 = 0x2003;
const ROOM_EMPTY_WIDTH: u32 = 0x2400;
const GRID_BY_ROOM: u32 = 0x2101;
const GRID_BY_FUNCTION: u32 = 0x2102;
const FILTERS: [u32; 2] = [0x2201, 0x2202];
const TABS_BY_ROOM: u32 = 0x2301;
const TABS_BY_FUNCTION: u32 = 0x2302;
const SHOP_MODE: [u32; 4] = [0x0db8_fb90, 0x0db8_fb91, 0x0dd3_3980, 0x0dd3_3a70];
const HOUSE_COST: u32 = 0x091b_e560;
/// The preview panel and its pieces.
const PREVIEW: u32 = 0x06e2_3360;
const PREVIEW_SCENE: u32 = 0x06e2_3361;
const PREVIEW_TITLE: u32 = 0x06e2_3362;
const PREVIEW_PRICE: u32 = 0x06e2_3363;
const PREVIEW_DESC: u32 = 0x06e2_3364;
const PREVIEW_GRID_HOLDER: u32 = 0x06e2_3365;
const PREVIEW_PRESETS: u32 = 0x06e2_3366;
const PREVIEW_BUTTONS: [u32; 5] = [0x06e2_3367, 0x06e2_3368, 0x06e2_3369, 0x06e2_3374, 0x0daf_a230];
const PREVIEW_MOODLET: u32 = 0x06e2_3370;
const PREVIEW_DESC_BACK: u32 = 0x06e2_3371;
const PREVIEW_THUMB: u32 = 0x06e2_3372;
const PREVIEW_PATTERNS: u32 = 0x06e2_3375;
const PREVIEW_BUFF: u32 = 0x06e2_336d;
/// The catalogue grid's own layout (`BuyExpandableCatalogGrid`): its grid, expand button,
/// and the backgrounds shown closed (0x30x) and open (0x20x).
const ITEM_GRID: u32 = 0x0a5f_e43c;
const EXPAND: u32 = 0x0a65_20dd;
const SCROLL_UP: u32 = 0x0600_0000;
const SCROLL_DOWN: u32 = 0x0600_0001;
const SCROLLBAR: u32 = 0x0600_0002;
const GRID_CLOSED: [u32; 2] = [0x301, 0x302];
const GRID_OPEN: [u32; 2] = [0x201, 0x202];
/// A catalogue cell's thumbnail (`BuyCatalogItem`, `CatalogPreviewPresetItem`).
const CELL_THUMB: u32 = 0x05ac_1600;

/// The catalogue windows' widths and columns as designed (by function, by room), the rows shown
/// closed and most rows open (`ExpandableCatalogGrid`), where the catalogue window starts on
/// screen (the puck's place plus the mid-panel's), a tab's width (`TabControl.ButtonArea`), the
/// preview panel's width and its place over the catalogue.
const DEFAULT_WIDTH: [f32; 2] = [688.0, 700.0];
const DEFAULT_COLUMNS: [i32; 2] = [6, 4];
const CLOSED_ROWS: usize = 2;
const MAX_ROWS: usize = 6;
const CATALOGUE_X: f32 = 316.0;
const TAB_STEP: f32 = 43.0;
const PREVIEW_WIDTH: f32 = 562.0;
const PREVIEW_X: f32 = 102.0;

/// The `Buy` layout on screen, and what the catalogue is showing.
#[derive(Resource)]
pub struct BuyHud {
    s: Spawned,
    /// Its puck under the HUD puck's ids (the puck system works both).
    pub(crate) puck: Spawned,
    /// The catalogue grids' own layouts (by function, by room).
    grids: [Spawned; 2],
    flags: HashMap<Key, ObjBuy>,
    descriptions: HashMap<Key, String>,
    by_room: bool,
    /// The category (index into the by-function list), its subcategory tab (none: All; not
    /// chosen yet: the first), and the tab last chosen in each (`SetLastSubCategoryFilter`).
    category: usize,
    sub: Option<Option<u8>>,
    last_sub: HashMap<usize, Option<u8>>,
    /// The room (index into the rooms), the room picture's button clicked.
    room: usize,
    room_button: Option<usize>,
    page_two: bool,
    scroll: usize,
    expanded: bool,
    dirty: bool,
    /// What's on the grid: its cells' holder, the tabs' holders, and the window width it was
    /// laid out for.
    cells: Option<Entity>,
    tabs: [Option<Entity>; 2],
    width: f32,
    rows: usize,
    columns: usize,
    count: usize,
    /// The preview panel's: the object it shows, its designs' cells and their first shown.
    previewing: Option<(Key, Option<Key>)>,
    presets: Option<Entity>,
    preset_scroll: usize,
}

/// A subcategory's (or a room's) tab.
#[derive(Component, Clone, Copy, PartialEq)]
enum Tab {
    Sub(Option<u8>),
    Room(usize),
}

/// One of a room picture's buttons.
#[derive(Component, Clone, Copy)]
struct RoomPick(usize);

/// A design in the preview panel's grid.
#[derive(Component)]
struct PresetCell;

#[allow(clippy::too_many_arguments)]
fn spawn_buy_hud(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), old: Option<Res<BuyHud>>, mut buy: ResMut<BuyMode>) {
    let Some(mut ui) = ui else { return };
    if !crate::livehud::active(Some(&ui)) || ui.layout("Buy").is_none() {
        return;
    }
    if let Some(r) = old.and_then(|o| o.s.root) {
        commands.entity(r).try_despawn();
    }
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "Buy") else { return };
    let Some(root) = s.root else { return };
    commands.entity(root).insert((DespawnOnExit(AppState::InGame), GlobalZIndex(5), Visibility::Hidden));
    // The catalogue grids bring in their own layout (`LayoutName`), filling them.
    let mut grids = [Spawned::default(), Spawned::default()];
    for (k, id) in [GRID_BY_FUNCTION, GRID_BY_ROOM].into_iter().enumerate() {
        if let (Some(e), Some(mut w)) = (s.id(id), ui.layout("BuyExpandableCatalogGrid").cloned()) {
            w.area = [0.0; 4];
            w.place = UiPlace::Simple(15);
            grids[k] = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, e);
            if let Some(g) = grids[k].id(ITEM_GRID) {
                commands.entity(g).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
            }
        }
    }
    let baked = ui.baked();
    let flags = baked.buy_flags.iter().copied().collect();
    let descriptions = baked.descriptions.iter().cloned().collect();
    // The category buttons' and room tabs' tooltips.
    for (i, name) in baked.buy.by_category.iter().enumerate() {
        if let (Some(e), Some(c)) = (s.id(FIRST_CATEGORY + i as u32), baked.buy.categories.iter().find(|c| &c.name == name)) {
            commands.entity(e).insert(crate::icons::Tooltip(c.label.clone()));
        }
    }
    // The rooms' buttons.
    for room in &baked.buy.rooms {
        let Some(panel) = s.id(room.bit as u32) else { continue };
        for (i, (_, label, id, _)) in room.buttons.iter().enumerate() {
            if let Some(b) = s.within(panel, *id) {
                commands.entity(b).insert((RoomPick(i), crate::icons::Tooltip(label.clone())));
            }
        }
    }
    // The clone tool is the eyedropper, the design tool Create a Style (`buy`'s own buttons).
    if let Some(e) = s.id(TOOL_CLONE) {
        commands.entity(e).insert((BuyButton::Eyedropper, crate::icons::Tooltip("Eyedropper".into())));
    }
    if let Some(e) = s.id(TOOL_DESIGN) {
        commands.entity(e).insert((BuyButton::Styling, crate::icons::Tooltip("Design Tool".into())));
    }
    for (id, tip) in [(TOOL_HAND, "Hand Tool"), (TOOL_SELL, "Sledgehammer")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    // (No store, collections, filters, undo, blueprints or lot value here; the panels for
    // things other than the catalogue stay away.)
    for id in SHOP_MODE.into_iter().chain([HOUSE_COST, PREVIEW_SCENE, PREVIEW_MOODLET, PREVIEW_BUFF]).chain(PREVIEW_BUTTONS) {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    if let Some(e) = s.id(PREVIEW_THUMB) {
        commands.entity(e).insert(Visibility::Inherited);
    }
    let puck = s.renamed(|id| if (PUCK_BASE..PUCK_BASE + 0x300).contains(&id) { id + PUCK_OFFSET } else { id });
    buy.game_look = true;
    commands.insert_resource(BuyHud {
        s,
        puck,
        grids,
        flags,
        descriptions,
        by_room: false,
        category: 0,
        sub: None,
        last_sub: HashMap::new(),
        room: 0,
        room_button: None,
        page_two: false,
        scroll: 0,
        expanded: false,
        dirty: true,
        cells: None,
        tabs: [None, None],
        width: 0.0,
        rows: CLOSED_ROWS,
        columns: 0,
        count: 0,
        previewing: None,
        presets: None,
        preset_scroll: 0,
    });
}

/// Whether the catalogue's up: buy mode (not build).
fn showing(buy: &BuyMode) -> bool {
    buy.active && buy.category < WALLPAPER_TAB
}

/// In buy mode the layout comes up in place of the HUD's puck.
fn show(hud: Res<BuyHud>, live: Option<Res<LiveHud>>, buy: Res<BuyMode>, mut vis: Query<&mut Visibility>, mut was: Local<Option<(Option<Entity>, bool)>>) {
    let on = showing(&buy);
    if *was == Some((hud.s.root, on)) {
        return;
    }
    *was = Some((hud.s.root, on));
    set_visible(&mut vis, hud.s.root, on);
    if let Some(l) = live {
        set_visible(&mut vis, l.puck.root, !on);
    }
}

/// The catalogue's buttons: by room or function, the categories (and their pages), the tabs, the
/// room picture's buttons, expanding the grid, and the wheel over it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn catalogue_controls(
    mut hud: ResMut<BuyHud>,
    buy: Res<BuyMode>,
    ui: Option<Res<UiAssets>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    (tabs, picks): (Query<(&Interaction, &Tab), Changed<Interaction>>, Query<(&Interaction, &RoomPick), Changed<Interaction>>),
    areas: Query<(&ComputedNode, &bevy::ui::UiGlobalTransform, &InheritedVisibility)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    scrollbars: Query<&crate::layout::UiScrollBar>,
    buttons: Query<&UiButton>,
    mut wheel: MessageReader<MouseWheel>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let scrolled: f32 = wheel.read().map(|w| if w.unit == bevy::input::mouse::MouseScrollUnit::Line { w.y } else { w.y / 40.0 }).sum();
    if !showing(&buy) {
        return;
    }
    let Some(ui) = ui else { return };
    let cat = &ui.baked().buy;
    let h = &mut *hud;
    if let Some(bar) = h.grids[h.by_room as usize].id(SCROLLBAR).and_then(|e| scrollbars.get(e).ok())
        && h.scroll != bar.value
    {
        h.scroll = bar.value;
        h.dirty = true;
    }
    let mut click = |h: &mut BuyHud| {
        h.scroll = 0;
        h.dirty = true;
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
    };
    if pressed(&clicks, h.s.id(BY_FUNCTION)) && h.by_room {
        h.by_room = false;
        click(h);
    }
    if pressed(&clicks, h.s.id(BY_ROOM)) && !h.by_room {
        h.by_room = true;
        click(h);
    }
    if pressed(&clicks, h.s.id(TOGGLE_PAGE)) {
        h.page_two = !h.page_two;
        click(h);
    }
    for i in 0..cat.by_category.len() {
        if pressed(&clicks, h.s.id(FIRST_CATEGORY + i as u32)) && h.category != i {
            h.category = i;
            h.sub = h.last_sub.get(&i).copied();
            h.expanded = false;
            click(h);
        }
    }
    for (i, t) in &tabs {
        if *i != Interaction::Pressed {
            continue;
        }
        match *t {
            Tab::Sub(s) => {
                h.sub = Some(s);
                h.last_sub.insert(h.category, s);
            }
            Tab::Room(r) => {
                h.room = r;
                h.room_button = None;
            }
        }
        h.expanded = false;
        click(h);
    }
    for (i, p) in &picks {
        if *i == Interaction::Pressed {
            h.room_button = Some(p.0);
            click(h);
        }
    }
    let k = h.by_room as usize;
    if pressed(&clicks, h.grids[k].id(EXPAND)) && h.grids[k].id(EXPAND).and_then(|e| buttons.get(e).ok()).is_some_and(|b| !b.disabled) {
        h.expanded = !h.expanded;
        h.scroll = 0;
        h.dirty = true;
    }
    // Hit the whole grid, including its thumbnails: a child under the pointer can swallow
    // the grid window's Interaction. Both its arrows and the wheel move one row at a time.
    let grid = h.grids[k].id(ITEM_GRID);
    let over = windows.single().ok().is_some_and(|w| over_window(grid, &areas, w));
    let up = pressed(&clicks, grid.and_then(|g| h.grids[k].within(g, SCROLL_UP)));
    let down = pressed(&clicks, grid.and_then(|g| h.grids[k].within(g, SCROLL_DOWN)));
    if up || down || over && scrolled != 0.0 {
        let rows = h.count.div_ceil(h.columns.max(1));
        let most = rows.saturating_sub(h.rows);
        let to = if up || over && scrolled > 0.0 { h.scroll.saturating_sub(1) } else { (h.scroll + 1).min(most) };
        if to != h.scroll {
            h.scroll = to;
            h.dirty = true;
        }
    }
}

/// The objects the catalogue lists for a filter: sold, not doors or windows (build mode's), one
/// of each name, cheapest first.
fn listed<'a>(catalog: &'a Catalog, flags: &HashMap<Key, ObjBuy>, keep: impl Fn(&ObjBuy) -> bool) -> Vec<&'a CatalogEntry> {
    let mut v: Vec<&CatalogEntry> = catalog.entries.iter().filter(|e| e.price > 0 && e.opening.is_none() && !e.shell && !e.name.is_empty() && flags.get(&e.key).is_some_and(&keep)).collect();
    v.sort_by(|a, b| a.price.cmp(&b.price).then(a.name.cmp(&b.name)));
    let mut names = std::collections::HashSet::new();
    v.retain(|e| names.insert(e.name.as_str()));
    v
}

/// The tools: hand, sledgehammer (sells what's clicked), eyedropper and design tool (`buy`'s), lit
/// for the one in hand; those without a use here greyed.
fn tools(hud: Res<BuyHud>, mut commands: Commands, mut buy: ResMut<BuyMode>, clock: Res<crate::clock::GameClock>, clicks: Query<(Entity, &Interaction), Changed<Interaction>>, mut buttons: Query<&mut UiButton>) {
    if !showing(&buy) {
        return;
    }
    let s = &hud.s;
    if pressed(&clicks, s.id(TOOL_HAND)) {
        buy.drop_tools(&mut commands);
    }
    if pressed(&clicks, s.id(TOOL_SELL)) {
        let on = !buy.selling;
        buy.drop_tools(&mut commands);
        buy.selling = on;
    }
    if pressed(&clicks, s.id(DAY_NIGHT)) {
        buy.toggle_lighting(clock.hour_f());
    }
    if pressed(&clicks, s.id(INDOOR_GRID)) {
        buy.hide_grid = !buy.hide_grid;
    }
    let styling = buy.styling();
    let lit = [
        (TOOL_HAND, !buy.selling && !buy.eyedropper && !styling, false),
        (TOOL_SELL, buy.selling, false),
        (TOOL_CLONE, buy.eyedropper, false),
        (TOOL_DESIGN, styling, buy.placing.is_none()),
        (UNDO, false, true),
        (REDO, false, true),
        (DAY_NIGHT, !(6.0..20.0).contains(&buy.lighting_hour(clock.hour_f())), false),
        (INDOOR_GRID, !buy.hide_grid, false),
        (BLUEPRINT, false, true),
        (FAMILY_INVENTORY, false, true),
        (COLLECTIONS, false, true),
        (FILTERS[0], false, true),
        (FILTERS[1], false, true),
        (BY_ROOM, hud.by_room, false),
        (BY_FUNCTION, !hud.by_room, false),
        (TOGGLE_PAGE, hud.page_two, false),
    ];
    for (id, on, off) in lit {
        if let Some(e) = s.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
            && (b.selected != on || b.disabled != off)
        {
            b.selected = on;
            b.disabled = off;
        }
    }
}

/// Lays the catalogue out for what's chosen: the windows for by room or by function, the
/// category buttons and their page, the tabs, and the grid's cells (as many columns as the
/// screen has room for, at least as designed, no more than two rows' worth).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn fill_catalogue(
    mut commands: Commands,
    mut hud: ResMut<BuyHud>,
    buy: Res<BuyMode>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (catalog, mut game_ui): (Res<Catalog>, Option<ResMut<crate::icons::GameUi>>),
    windows: Query<&Window, With<PrimaryWindow>>,
    (mut vis, mut nodes, mut buttons): (Query<&mut Visibility>, Query<&mut Node>, Query<&mut UiButton>),
    mut scrollbars: Query<&mut crate::layout::UiScrollBar>,
    mut held: Local<Option<Key>>,
) {
    if !showing(&buy) {
        return;
    }
    let (Some(mut ui), Ok(window)) = (ui, windows.single()) else { return };
    // (Laid out again for another screen width, and lit for the object in hand.)
    let screen = window.width();
    let in_hand = buy.placing.as_ref().map(|p| p.objd);
    if *held != in_hand {
        *held = in_hand;
        hud.dirty = true;
    }
    if !hud.dirty && hud.width == screen {
        return;
    }
    hud.dirty = false;
    hud.width = screen;
    let data = ui.baked().clone();
    let cat = &data.buy;
    let h = &mut *hud;
    let k = h.by_room as usize;
    // By function or by room.
    for (id, on) in [
        (WIN_BY_FUNCTION, !h.by_room),
        (CATEGORY_PANEL, !h.by_room),
        (TABS_BY_FUNCTION, !h.by_room),
        (TABS_BY_ROOM, h.by_room),
        (WIN_BY_ROOM, h.by_room && h.room_button.is_some()),
        (WIN_ROOM_EMPTY, h.by_room && h.room_button.is_none()),
    ] {
        set_visible(&mut vis, h.s.id(id), on);
    }
    for (i, room) in cat.rooms.iter().enumerate() {
        set_visible(&mut vis, h.s.id(room.bit as u32), h.by_room && i == h.room);
    }
    if let (Some(e), Some(w)) = (h.s.id(WIN_ROOM_EMPTY), ui.find("Buy", ROOM_EMPTY_WIDTH))
        && let Ok(mut n) = nodes.get_mut(e)
    {
        n.width = Val::Px(w.area[2] - w.area[0]);
    }
    // The categories' buttons: the expansions' that have objects, on a second page when there's
    // more than one.
    let has = |name: &str| cat.categories.iter().find(|c| c.name == name).is_some_and(|c| !listed(&catalog, &h.flags, |f| f.in_category(c.bit)).is_empty());
    let extras: Vec<(u32, u32)> = EXTRA_CATEGORIES.iter().copied().filter(|(id, _)| cat.by_category.get((id - FIRST_CATEGORY) as usize).is_some_and(|n| has(n))).collect();
    let paged = extras.len() > 1;
    for i in 0..cat.by_category.len() as u32 {
        let id = FIRST_CATEGORY + i;
        let main = id < PAGE_ONE_END;
        let on = if main {
            !paged || !h.page_two
        } else if let Some((_, p2)) = extras.iter().find(|(p1, _)| *p1 == id) {
            !paged && *p2 != id
        } else {
            paged && h.page_two && extras.iter().any(|(_, p2)| *p2 == id)
        };
        set_visible(&mut vis, h.s.id(id), on);
        if let Some(e) = h.s.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
            && b.selected != (i as usize == h.category)
        {
            b.selected = i as usize == h.category;
        }
    }
    set_visible(&mut vis, h.s.id(TOGGLE_PAGE), paged && !h.by_room);
    // (The page-two buttons go along the top row, as the game's.)
    if paged {
        let first = ui.find("Buy", EXTRA_CATEGORIES[0].1).map(|w| w.area).unwrap_or_default();
        for (n, (_, p2)) in extras.iter().enumerate() {
            if let Some(e) = h.s.id(*p2)
                && let Ok(mut node) = nodes.get_mut(e)
            {
                node.left = Val::Px(first[0] + n as f32 * 51.0);
                node.top = Val::Px(first[1]);
            }
        }
    }
    // The tabs, and the objects.
    let (tab_list, items): (Vec<(Tab, String, u64)>, Vec<&CatalogEntry>) = if h.by_room {
        let tabs = cat.rooms.iter().enumerate().map(|(i, r)| (Tab::Room(i), r.label.clone(), r.image)).collect();
        let room = cat.rooms.get(h.room);
        // (The room picture's buttons with nothing to show are greyed, sets and collections
        // hidden; the first with something is chosen to start with.)
        let mut firsts = None;
        if let Some(r) = room {
            let panel = h.s.id(r.bit as u32);
            for (i, (_, _, id, bits)) in r.buttons.iter().enumerate() {
                let n = listed(&catalog, &h.flags, |f| f.in_room(r.bit) && bits.iter().any(|b| *b < 64 && f.room_sub & (1u64 << b) != 0)).len();
                let e = panel.and_then(|p| h.s.within(p, *id));
                if bits.contains(&60) {
                    set_visible(&mut vis, e, false);
                }
                if let Some(e) = e
                    && let Ok(mut b) = buttons.get_mut(e)
                {
                    let (sel, off) = (h.room_button == Some(i), n == 0);
                    if b.selected != sel || b.disabled != off {
                        b.selected = sel;
                        b.disabled = off;
                    }
                }
                if n > 0 && firsts.is_none() {
                    firsts = Some(i);
                }
            }
        }
        if h.room_button.is_none() {
            h.room_button = firsts;
            set_visible(&mut vis, h.s.id(WIN_BY_ROOM), h.room_button.is_some());
            set_visible(&mut vis, h.s.id(WIN_ROOM_EMPTY), h.room_button.is_none());
        }
        let items = match (room, h.room_button.and_then(|b| room?.buttons.get(b))) {
            (Some(r), Some((_, _, _, bits))) => listed(&catalog, &h.flags, |f| f.in_room(r.bit) && bits.iter().any(|b| *b < 64 && f.room_sub & (1u64 << b) != 0)),
            _ => Vec::new(),
        };
        (tabs, items)
    } else {
        let c = cat.by_category.get(h.category).and_then(|n| cat.categories.iter().find(|c| &c.name == n));
        let mut tabs = Vec::new();
        let mut items = Vec::new();
        if let Some(c) = c {
            for s in &c.subs {
                if !listed(&catalog, &h.flags, |f| f.in_category(c.bit) && f.in_sub(s.bit)).is_empty() {
                    tabs.push((Tab::Sub(Some(s.bit)), s.label.clone(), s.image));
                }
            }
            // ("All" last; the first is chosen to start with.)
            tabs.push((Tab::Sub(None), "All".to_string(), s3pkg::fnv64("glb_i_all_r2")));
            let sub = *h.sub.get_or_insert(match tabs[0].0 {
                Tab::Sub(s) => s,
                Tab::Room(_) => None,
            });
            items = listed(&catalog, &h.flags, |f| f.in_category(c.bit) && sub.is_none_or(|s| f.in_sub(s)));
        }
        (tabs, items)
    };
    // The tabs, left to right (squeezed to fit), each over the next, the chosen on top.
    let container = h.s.id(if h.by_room { TABS_BY_ROOM } else { TABS_BY_FUNCTION });
    let tab_area = ui.find("Buy", if h.by_room { TABS_BY_ROOM } else { TABS_BY_FUNCTION }).map(|w| w.area).unwrap_or_default();
    let tab_width = tab_area[2] - tab_area[0];
    if let (Some(c), Some(template)) = (container, ui.layout("TabControl").cloned()) {
        let holder = holder(&mut commands, c, &mut h.tabs[k], &vis);
        let n = tab_list.len();
        let total = n as f32 * TAB_STEP;
        let squeeze = if total > tab_width && n > 1 { (total - tab_width) / (n - 1) as f32 } else { 0.0 };
        for (i, (tab, label, image)) in tab_list.iter().enumerate().rev() {
            let chosen = match tab {
                Tab::Sub(s) => !h.by_room && Some(*s) == h.sub,
                Tab::Room(r) => *r == h.room,
            };
            let mut w = template.clone();
            let x = (i as f32 * (TAB_STEP - squeeze)).round();
            w.area = [x + 1.0, 1.0, x + 44.0, 52.0];
            let t = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
            let Some(r) = t.root else { continue };
            commands.entity(r).insert((*tab, crate::icons::Tooltip(label.clone())));
            if chosen {
                commands.entity(r).insert((ZIndex(1), crate::layout::Selected));
            }
            if let Some((icon, _)) = ui.image(&mut images, *image) {
                commands.entity(r).insert(crate::layout::SetIcon(icon));
            }
        }
    }
    // The catalogue's width: as designed, plus a column for every two objects more than it
    // shows (fewer for fewer), as far as the screen goes, and wide enough for the tabs.
    let count = items.len();
    let grid_win = ui.find("BuyExpandableCatalogGrid", ITEM_GRID).cloned();
    let g = grid_win.as_ref().and_then(|w| w.grid).unwrap_or_default();
    let step = Vec2::new(g.cell[0] + g.cell_padding[2] - g.cell_padding[0], g.cell[1] + g.cell_padding[3] - g.cell_padding[1]).max(Vec2::ONE);
    let (w0, c0) = (DEFAULT_WIDTH[k], DEFAULT_COLUMNS[k]);
    let fit = ((screen - (CATALOGUE_X + w0)) / step.x).floor() as i32;
    let mut extra = fit.min((count.max(4) as i32 + 1) / 2 - c0);
    let tabs_need = (30.0 + tab_area[0] + tab_list.len() as f32 * TAB_STEP).min(30.0 + tab_area[2]);
    while w0 + extra as f32 * step.x < tabs_need {
        extra += 1;
    }
    let width = w0 + extra as f32 * step.x;
    let columns = (c0 + extra).max(1) as usize;
    let win = h.s.id(if h.by_room { WIN_BY_ROOM } else { WIN_BY_FUNCTION });
    if let Some(e) = win
        && let Ok(mut n) = nodes.get_mut(e)
    {
        n.width = Val::Px(width);
        n.overflow = Overflow::visible();
    }
    // Expanded: up to six rows, growing upwards.
    let item_rows = count.div_ceil(columns);
    let rows = if h.expanded { item_rows.clamp(CLOSED_ROWS, MAX_ROWS) } else { CLOSED_ROWS };
    h.rows = rows;
    h.columns = columns;
    h.count = count;
    h.scroll = h.scroll.min(item_rows.saturating_sub(rows));
    let gs = &h.grids[k];
    // The custom grid's host is clipped in its source layout. Expanded rows deliberately
    // extend above it; only the actual item grid clips thumbnails to its visible rows.
    if let Some(e) = h.s.id(if h.by_room { GRID_BY_ROOM } else { GRID_BY_FUNCTION })
        && let Ok(mut node) = nodes.get_mut(e)
    {
        node.overflow = Overflow::visible();
    }
    if !h.by_room && let Some(e) = h.s.id(TABS_BY_FUNCTION) && let Ok(mut node) = nodes.get_mut(e) {
        node.top = Val::Px(tab_area[1] - (rows - CLOSED_ROWS) as f32 * step.y);
    }
    let scrollbar = gs.id(SCROLLBAR);
    set_visible(&mut vis, scrollbar, item_rows > rows);
    if let Some(e) = scrollbar {
        if let Ok(mut bar) = scrollbars.get_mut(e) {
            bar.value = h.scroll;
            bar.total = item_rows;
            bar.visible = rows;
        }
        if let Ok(mut node) = nodes.get_mut(e) {
            node.top = Val::Px(1.0);
            node.bottom = Val::Px(2.0);
            node.height = Val::Auto;
        }
    }
    if let Some(root) = gs.root
        && let Ok(mut n) = nodes.get_mut(root)
    {
        n.top = Val::Px(-((rows - CLOSED_ROWS) as f32 * step.y));
    }
    for id in GRID_CLOSED {
        set_visible(&mut vis, gs.id(id), rows == CLOSED_ROWS);
    }
    for id in GRID_OPEN {
        set_visible(&mut vis, gs.id(id), rows > CLOSED_ROWS);
    }
    if let Some(e) = gs.id(EXPAND)
        && let Ok(mut b) = buttons.get_mut(e)
    {
        let (sel, off) = (h.expanded, !h.expanded && (columns < c0 as usize || item_rows <= CLOSED_ROWS));
        if b.selected != sel || b.disabled != off {
            b.selected = sel;
            b.disabled = off;
        }
    }
    // The cells.
    let Some(grid) = gs.id(ITEM_GRID) else { return };
    if let Ok(mut n) = nodes.get_mut(grid) {
        n.overflow = Overflow::clip();
    }
    for (id, disabled) in [(SCROLL_UP, h.scroll == 0), (SCROLL_DOWN, h.scroll >= item_rows.saturating_sub(rows))] {
        if let Some(e) = gs.within(grid, id)
            && let Ok(mut b) = buttons.get_mut(e)
            && b.disabled != disabled
        {
            b.disabled = disabled;
        }
    }
    let holder = holder(&mut commands, grid, &mut h.cells, &vis);
    let Some(template) = ui.layout("BuyCatalogItem").cloned() else { return };
    for (i, item) in items.iter().enumerate().skip(h.scroll * columns).take(rows * columns) {
        let n = i - h.scroll * columns;
        let (col, row) = ((n % columns) as f32, (n / columns) as f32);
        let (x, y) = (g.padding[0] + col * step.x, g.padding[1] + row * step.y);
        let mut w = template.clone();
        w.cls = "Button".to_string();
        w.area = [x, y, x + step.x, y + g.cell[1]];
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        let Some(r) = c.root else { continue };
        commands.entity(r).insert((BuyButton::Item(item.key), crate::icons::Tooltip(format!("{}\n§{}", item.name, crate::lifetime::group(item.price as i64)))));
        if in_hand == Some(item.key) {
            commands.entity(r).insert(crate::layout::Selected);
        }
        let thumb = game_ui.as_deref_mut().and_then(|gu| gu.icon(&mut images, &s3bake::gamedata::thumb_name(item.key.2)));
        if let (Some(t), Some(win)) = (thumb, c.id(CELL_THUMB)) {
            crate::hudpanels::picture(&mut commands, win, t, Color::WHITE);
        }
    }
}

/// A holder for fresh children under a window (cleared).
fn holder(commands: &mut Commands, parent: Entity, slot: &mut Option<Entity>, vis: &Query<&mut Visibility>) -> Entity {
    match slot.filter(|h| vis.contains(*h)) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => {
            let h = commands
                .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Visibility::Inherited, Pickable::IGNORE, ChildOf(parent)))
                .id();
            *slot = Some(h);
            h
        }
    }
}

/// Sets a window's place and size in its parent (a window placed by its own area).
fn set_area(nodes: &mut Query<&mut Node>, e: Option<Entity>, [x1, y1, x2, y2]: [f32; 4]) {
    if let Some(e) = e
        && let Ok(mut n) = nodes.get_mut(e)
    {
        n.left = Val::Px(x1);
        n.top = Val::Px(y1);
        n.width = Val::Px(x2 - x1);
        n.height = Val::Px(y2 - y1);
    }
}

/// About how many lines words take in a width (characters of about a size).
fn lines(s: &str, width: f32, char_width: f32) -> f32 {
    let per = (width / char_width).max(1.0) as usize;
    s.split('\n').map(|p| p.chars().count().div_ceil(per).max(1)).sum::<usize>() as f32
}

/// Tests the pointer against a visible window in physical UI coordinates. This works when
/// its children own hover, and when the OS scales the window for a high-DPI display.
fn over_window(e: Option<Entity>, areas: &Query<(&ComputedNode, &bevy::ui::UiGlobalTransform, &InheritedVisibility)>, window: &Window) -> bool {
    let Some((node, tf, vis)) = e.and_then(|e| areas.get(e).ok()) else { return false };
    if !vis.get() {
        return false;
    }
    let Some(p) = window.physical_cursor_position().and_then(|p| tf.try_inverse().map(|i| i.transform_point2(p))) else { return false };
    p.abs().cmple(node.size() * 0.5).all()
}

/// The preview panel over the catalogue for the object in hand: its picture, name, price (red
/// when it can't be afforded), description, and its designs (the one it's in lit), sized to
/// what it shows as `SetWorkingProduct` sizes it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn preview_panel(
    mut commands: Commands,
    mut hud: ResMut<BuyHud>,
    buy: Res<BuyMode>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (catalog, household, mut game_ui): (Res<Catalog>, Option<Res<crate::interact::Household>>, Option<ResMut<crate::icons::GameUi>>),
    (data, mut assets, mut meshes, mut mats): (Res<crate::baked::Baked>, ResMut<crate::objects::ObjectAssets>, ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    (mut vis, mut nodes, mut texts, mut colors): (Query<&mut Visibility>, Query<&mut Node>, Query<&mut Text>, Query<&mut TextColor>),
    areas: Query<(&ComputedNode, &bevy::ui::UiGlobalTransform, &InheritedVisibility)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut buttons: Query<&mut UiButton>,
    mut wheel: MessageReader<MouseWheel>,
    over_ui: Res<crate::hud::PointerOverUi>,
    mut last_funds: Local<Option<i64>>,
) {
    let scrolled: f32 = wheel.read().map(|w| w.y.signum()).sum();
    let what = buy.placing.as_ref().filter(|_| showing(&buy)).map(|p| (p.objd, p.design));
    let h = &mut *hud;
    // (Away while the pointer's out in the world putting it down, back when it's over the
    // panels again, as the game's.)
    let panel = h.s.id(PREVIEW);
    set_visible(&mut vis, panel, what.is_some() && over_ui.0);
    let grid = h.s.id(PREVIEW_PRESETS);
    let presets_over = windows.single().ok().is_some_and(|w| over_window(grid, &areas, w));
    let up = pressed(&clicks, grid.and_then(|g| h.s.within(g, SCROLL_UP)));
    let down = pressed(&clicks, grid.and_then(|g| h.s.within(g, SCROLL_DOWN)));
    let mut rescroll = false;
    if up || down || presets_over && scrolled != 0.0 {
        let n = what.map_or(0, |(k, _)| {
            let ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            crate::objects::ObjectAssets::design_count(&ctx, k) as usize
        });
        let columns = ui.as_ref().and_then(|u| u.find("Buy", PREVIEW_PRESETS)?.grid).map_or(4, |g| g.columns.max(1) as usize);
        let to = if up || presets_over && scrolled > 0.0 { h.preset_scroll.saturating_sub(1) } else { (h.preset_scroll + 1).min(n.saturating_sub(columns)) };
        rescroll = to != h.preset_scroll;
        h.preset_scroll = to;
    }
    let funds = household.as_ref().map(|h| h.funds);
    let funds_changed = *last_funds != funds;
    *last_funds = funds;
    if h.previewing == what && !rescroll && !funds_changed {
        return;
    }
    if h.previewing.map(|p| p.0) != what.map(|p| p.0) {
        h.preset_scroll = 0;
    }
    h.previewing = what;
    let (Some((objd, design)), Some(mut ui), Some(entry)) = (what, ui, what.and_then(|w| catalog.by_key(&w.0))) else {
        set_visible(&mut vis, panel, false);
        return;
    };
    set_visible(&mut vis, panel, over_ui.0);
    let price = entry.price.max(0) as i64;
    let afford = household.as_ref().is_none_or(|hh| hh.funds >= price) || buy.placing.as_ref().is_some_and(|p| p.owned);
    let desc = h.descriptions.get(&objd).cloned().unwrap_or_default();
    // The words, one under another.
    let title_h = lines(&entry.name, 270.0, 9.0) * 22.0;
    set_text(&mut texts, h.s.text(PREVIEW_TITLE), &entry.name);
    set_area(&mut nodes, h.s.id(PREVIEW_TITLE), [185.0, 19.0, 455.0, 19.0 + title_h]);
    let price_y = 19.0 + title_h + 10.0;
    set_text(&mut texts, h.s.text(PREVIEW_PRICE), &format!("§{}", crate::lifetime::group(price)));
    set_area(&mut nodes, h.s.id(PREVIEW_PRICE), [185.0, price_y, 495.0, price_y + 19.0]);
    let red = Color::srgb(0.85, 0.1, 0.1);
    for (id, base) in [(PREVIEW_TITLE, ui.find("Buy", PREVIEW_TITLE).and_then(|w| w.colors.first().copied())), (PREVIEW_PRICE, ui.find("Buy", PREVIEW_PRICE).and_then(|w| w.colors.first().copied()))] {
        if let Some(t) = h.s.text(id)
            && let Ok(mut c) = colors.get_mut(t)
        {
            c.0 = if afford { base.map_or(Color::BLACK, crate::layout::color) } else { red };
        }
    }
    let desc_y = price_y + 19.0 + 10.0;
    set_text(&mut texts, h.s.text(PREVIEW_DESC), &desc);
    let desc_h = lines(&desc, 346.0, 6.0) * 14.0;
    set_area(&mut nodes, h.s.id(PREVIEW_DESC), [4.0, 4.0, 350.0, 4.0 + desc_h]);
    set_visible(&mut vis, h.s.id(PREVIEW_DESC_BACK), !desc.is_empty());
    let mut desc_bottom = (desc_y + desc_h + 10.0).max(93.0);
    // The picture.
    let thumb = game_ui.as_deref_mut().and_then(|gu| gu.icon(&mut images, &s3bake::gamedata::thumb_name(objd.2)));
    if let (Some(t), Some(win)) = (thumb, h.s.id(PREVIEW_THUMB).and_then(|p| h.s.comment_within(p, "Thumbnail"))) {
        crate::hudpanels::picture(&mut commands, win, t, Color::WHITE);
    }
    // Its designs.
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let n = crate::objects::ObjectAssets::design_count(&ctx, objd);
    let grid = h.s.id(PREVIEW_PRESETS);
    let height = if n > 1 {
        let holder_y = desc_bottom + 10.0;
        let gh = ui.find("Buy", PREVIEW_GRID_HOLDER).map_or(67.0, |w| w.area[3] - w.area[1]);
        set_area(&mut nodes, h.s.id(PREVIEW_GRID_HOLDER), [186.0, holder_y, 402.0, holder_y + gh]);
        set_visible(&mut vis, h.s.id(PREVIEW_GRID_HOLDER), true);
        set_visible(&mut vis, h.s.id(PREVIEW_PATTERNS), false);
        if let (Some(gr), Some(g), Some(template)) = (grid, ui.find("Buy", PREVIEW_PRESETS).and_then(|w| w.grid), ui.layout("CatalogPreviewPresetItem").cloned()) {
            let columns = g.columns.max(1) as usize;
            h.preset_scroll = h.preset_scroll.min((n as usize).saturating_sub(columns));
            for (id, disabled) in [(SCROLL_UP, h.preset_scroll == 0), (SCROLL_DOWN, h.preset_scroll + columns >= n as usize)] {
                if let Some(e) = h.s.within(gr, id)
                    && let Ok(mut b) = buttons.get_mut(e)
                    && b.disabled != disabled
                {
                    b.disabled = disabled;
                }
            }
            if let Ok(mut node) = nodes.get_mut(gr) {
                node.overflow = Overflow::clip();
            }
            commands.entity(gr).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
            let holder = holder(&mut commands, gr, &mut h.presets, &vis);
            let step = Vec2::new(g.cell[0] + g.cell_padding[2] - g.cell_padding[0], g.cell[1] + g.cell_padding[3] - g.cell_padding[1]);
            for (col, d) in (h.preset_scroll as u8..n).take(g.columns.max(1) as usize).enumerate() {
                let x = g.padding[0] + col as f32 * step.x;
                let mut w = template.clone();
                w.cls = "Button".to_string();
                w.area = [x, g.padding[1], x + step.x, g.padding[1] + step.y];
                let c = ui.spawn_under(&mut commands, &mut ctx.images, &mut fonts, &w, holder);
                let Some(r) = c.root else { continue };
                commands.entity(r).insert((BuyButton::Design(d), PresetCell, crate::icons::Tooltip(format!("Design {}", d + 1))));
                if design == Some(crate::objects::design_texture(objd, d)) || (design.is_none() && d == 0) {
                    commands.entity(r).insert(crate::layout::Selected);
                }
                if let (Some(t), Some(win)) = (assets.texture(&mut ctx, crate::objects::design_texture(objd, d)), c.id(CELL_THUMB)) {
                    crate::hudpanels::picture(&mut commands, win, t, Color::WHITE);
                }
            }
        }
        holder_y + gh + 25.0
    } else {
        set_visible(&mut vis, h.s.id(PREVIEW_GRID_HOLDER), false);
        desc_bottom = desc_bottom.max(166.0);
        desc_bottom + 25.0
    };
    set_area(&mut nodes, h.s.id(PREVIEW_DESC_BACK), [181.0, desc_y, 542.0, desc_bottom]);
    // Over the catalogue's left, its bottom on the catalogue's top.
    let x = PREVIEW_X.min(h.width.min(DEFAULT_WIDTH[0] + 4000.0) - PREVIEW_WIDTH + 150.0).max(0.0);
    set_area(&mut nodes, panel, [x, -height, x + PREVIEW_WIDTH, 0.0]);
}
