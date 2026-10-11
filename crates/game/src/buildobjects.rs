//! Build mode's object catalogues in the game's own look (UI.dll's `BuildController`): the
//! Construction catalogue (doors, windows, arches, columns and fireplaces, its tabs along the
//! top) and the Gardening catalogue (flowers, shrubs, trees and rocks), each the game's expanding
//! grid of catalogue cells, opened from the house diagram's buttons. An object picked is in hand
//! as in Buy mode (doors, windows and arches set into a wall).

use std::collections::HashMap;

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use s3bake::Key;
use s3bake::ui::UiPlace;

use crate::buy::{BuyButton, BuyMode, CONSTRUCTION_TAB, GARDENING_TAB};
use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar};
use crate::livehud::{pressed, set_visible};
use crate::loading::{Catalog, CatalogEntry};

pub struct BuildObjectsPlugin;

impl Plugin for BuildObjectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(crate::PlayMode::Live), spawn.after(crate::buildcatalog::spawn).after(crate::buy::reset_buy_mode))
            .add_systems(
                Update,
                (controls, draw)
                    .chain()
                    .after(crate::buildcatalog::draw)
                    .before(crate::buyhistory::update)
                    .run_if(in_state(crate::PlayMode::Live))
                    .run_if(resource_exists::<BuildObjects>),
            )
            .add_systems(Update, scripted.before(controls).before(crate::buy::buy_buttons).run_if(in_state(crate::PlayMode::Live)).run_if(resource_exists::<BuildObjects>));
    }
}

/// BUILD_OBJECTS=1 (tests): Build mode (ten seconds in), the construction catalogue's columns,
/// the gardening catalogue's trees, and the first tree taken in hand, a step every two seconds,
/// with screenshots into BUILD_OBJECTS_SHOTS.
#[allow(clippy::too_many_arguments)]
fn scripted(
    mut commands: Commands,
    time: Res<Time<crate::autotest::InputTimeline>>,
    mut buy: ResMut<BuyMode>,
    mut clock: ResMut<crate::clock::GameClock>,
    mut objects: ResMut<BuildObjects>,
    mut cells: Query<(&BuyButton, &mut Interaction, &InheritedVisibility)>,
    mut step: Local<u8>,
) {
    if std::env::var("BUILD_OBJECTS").is_err() {
        return;
    }
    let shot = |commands: &mut Commands, name: &str| {
        let dir = std::env::var("BUILD_OBJECTS_SHOTS").unwrap_or_else(|_| ".".into());
        let _ = std::fs::create_dir_all(&dir);
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(format!("{dir}/{name}.png")));
    };
    let t = time.elapsed_secs();
    match *step {
        0 if t > 10.0 => crate::buy::enter_mode(&mut buy, Some(true), &mut commands, &mut clock),
        1 if t > 12.0 => {
            objects.choose(ObjTab::Columns);
            buy.show(CONSTRUCTION_TAB);
        }
        2 if t > 14.0 => {
            info!("build objects test: {} columns", objects.count);
            shot(&mut commands, "bo1_columns");
        }
        3 if t > 15.0 => {
            objects.choose(ObjTab::Arches);
        }
        4 if t > 17.0 => {
            info!("build objects test: {} arches", objects.count);
            shot(&mut commands, "bo1b_arches");
        }
        5 if t > 18.0 => {
            objects.choose(ObjTab::Trees);
            buy.show(GARDENING_TAB);
        }
        6 if t > 20.0 => {
            info!("build objects test: {} trees", objects.count);
            shot(&mut commands, "bo2_trees");
            if let Some((_, mut i, _)) = cells.iter_mut().find(|(b, _, v)| matches!(b, BuyButton::Item(_)) && v.get()) {
                *i = Interaction::Pressed;
            }
        }
        7 if t > 22.0 => {
            info!("build objects test: in hand {:?}", buy.placing.as_ref().map(|p| p.objd));
            shot(&mut commands, "bo3_in_hand");
        }
        _ => return,
    }
    *step += 1;
}

/// A tab of the two catalogues.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjTab {
    Doors,
    Windows,
    Fireplaces,
    Arches,
    Columns,
    Flowers,
    Shrubs,
    Trees,
    Rocks,
}

const CONSTRUCTION: [ObjTab; 5] = [ObjTab::Doors, ObjTab::Windows, ObjTab::Fireplaces, ObjTab::Arches, ObjTab::Columns];
const GARDENING: [ObjTab; 4] = [ObjTab::Flowers, ObjTab::Shrubs, ObjTab::Trees, ObjTab::Rocks];

// The OBJDs' build category bits.
const ARCH: u32 = 0x100;
const COLUMN: u32 = 0x10;
const FLOWER: u32 = 0x200;
const SHRUB: u32 = 0x400;
const TREE: u32 = 0x800;
const ROCK: u32 = 0x2000;

impl ObjTab {
    /// The catalogue it's in (a Build-mode tab).
    pub fn catalogue(self) -> usize {
        if GARDENING.contains(&self) { GARDENING_TAB } else { CONSTRUCTION_TAB }
    }

    /// Its tab in the `Build` layout (the picture and tooltip come from it).
    fn window(self) -> u32 {
        match self {
            ObjTab::Doors => 0x3902,
            ObjTab::Windows => 0x3901,
            ObjTab::Columns => 0x3903,
            ObjTab::Arches => 0x3904,
            ObjTab::Fireplaces => 0x3905,
            ObjTab::Flowers => 0x3a01,
            ObjTab::Shrubs => 0x3a02,
            ObjTab::Trees => 0x3a03,
            ObjTab::Rocks => 0x3a04,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ObjTab::Doors => "Doors",
            ObjTab::Windows => "Windows",
            ObjTab::Fireplaces => "Fireplaces",
            ObjTab::Arches => "Arches",
            ObjTab::Columns => "Columns",
            ObjTab::Flowers => "Flowers",
            ObjTab::Shrubs => "Shrubs",
            ObjTab::Trees => "Trees",
            ObjTab::Rocks => "Rocks",
        }
    }

    /// Whether a catalogue object belongs on this tab (by its build categories).
    fn keeps(self, e: &CatalogEntry, build: u32) -> bool {
        match self {
            ObjTab::Doors => e.opening == Some(true) && build & ARCH == 0,
            ObjTab::Windows => e.opening == Some(false),
            ObjTab::Arches => e.opening == Some(true) && build & ARCH != 0,
            ObjTab::Columns => build & COLUMN != 0,
            ObjTab::Fireplaces => e.kind == crate::interact::ObjectKind::Fireplace,
            ObjTab::Flowers => build & FLOWER != 0,
            ObjTab::Shrubs => build & SHRUB != 0 && build & TREE == 0,
            ObjTab::Trees => build & TREE != 0,
            ObjTab::Rocks => build & ROCK != 0,
        }
    }
}

// The catalogues' windows: their panels, the grids' hosts, the tabs' containers, and the
// designer's own buttons not used here (the column height slider, the filters).
const PANELS: [u32; 2] = [0x390, 0x3a0];
const HOSTS: [u32; 2] = [0x393, 0x3a3];
const TABS: [u32; 2] = [0x392, 0x3a2];
const NOT_HERE: [u32; 3] = [0x39a0, 0x397, 0x3a6];
const GRID: u32 = 0x0a5f_e43c;
const EXPAND: u32 = 0x0a65_20dd;
const BAR: u32 = 0x0600_0002;
const CELL_THUMB: u32 = 0x05ac_1600;
/// Where the grid starts in its panel.
const GRID_LEFT: f32 = 50.0;
const TAB_STEP: f32 = 43.0;

/// The two catalogues while in Build mode: their grids, cells and tabs, the tab chosen, and how
/// the grid is scrolled and laid out.
#[derive(Resource)]
pub struct BuildObjects {
    grids: [Spawned; 2],
    cells: [Option<Entity>; 2],
    tabs: [Option<Entity>; 2],
    tab: ObjTab,
    scroll: usize,
    columns: usize,
    rows: usize,
    count: usize,
    expanded: bool,
    flags: HashMap<Key, u32>,
    shown: Option<(ObjTab, usize, bool, u32, Option<Key>)>,
}

impl BuildObjects {
    /// A tab chosen (from the house diagram, or the tabs): its objects from the start.
    pub fn choose(&mut self, tab: ObjTab) {
        if self.tab != tab {
            self.scroll = 0;
            self.expanded = false;
        }
        self.tab = tab;
        self.shown = None;
    }
}

/// Whether one of the catalogues is up.
pub fn showing(buy: &BuyMode) -> bool {
    buy.active && buy.build_look && matches!(buy.category, CONSTRUCTION_TAB | GARDENING_TAB)
}

fn spawn(mut commands: Commands, hud: Option<Res<crate::buildhud::BuildHud>>, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>)) {
    let (Some(hud), Some(mut ui)) = (hud, ui) else { return };
    let Some(template) = ui.layout("BuildExpandableCatalogGrid").cloned() else { return };
    let mut grids = [Spawned::default(), Spawned::default()];
    for k in 0..2 {
        let Some(host) = hud.puck.id(HOSTS[k]) else { return };
        let mut w = template.clone();
        w.area = [0.0; 4];
        w.place = UiPlace::Simple(15);
        grids[k] = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, host);
        if let Some(grid) = grids[k].id(GRID) {
            commands.entity(grid).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
        }
        // (The tabs are made afresh from `TabControl`, as the game's; the designer's stand-ins go.)
        if let Some(tabs) = ui.find("Build", TABS[k]) {
            for tab in &tabs.children {
                if let Some(e) = hud.puck.id(tab.id) {
                    commands.entity(e).insert(Visibility::Hidden);
                }
            }
        }
    }
    for id in NOT_HERE {
        if let Some(e) = hud.puck.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for id in PANELS {
        if let Some(e) = hud.puck.id(id) {
            commands.entity(e).insert((Interaction::default(), crate::hud::BlocksWorld));
        }
    }
    let flags = ui.baked().buy_flags.iter().map(|(k, f)| (*k, f.build)).collect();
    commands.insert_resource(BuildObjects { grids, cells: [None, None], tabs: [None, None], tab: ObjTab::Doors, scroll: 0, columns: 1, rows: 2, count: 0, expanded: false, flags, shown: None });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn controls(
    mut objects: ResMut<BuildObjects>,
    buy: Res<BuyMode>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    tabs: Query<(&Interaction, &ObjTab), Changed<Interaction>>,
    bars: Query<&UiScrollBar>,
    windows: Query<&Window, With<PrimaryWindow>>,
    areas: Query<(&ComputedNode, &bevy::ui::UiGlobalTransform, &InheritedVisibility)>,
    mut wheel: MessageReader<MouseWheel>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let delta: f32 = wheel.read().map(|w| w.y).sum();
    if !showing(&buy) {
        return;
    }
    let o = &mut *objects;
    if let Some((_, t)) = tabs.iter().find(|(i, _)| **i == Interaction::Pressed) {
        o.choose(*t);
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
    }
    let k = (buy.category == GARDENING_TAB) as usize;
    let grid = &o.grids[k];
    if pressed(&clicks, grid.id(EXPAND)) {
        o.expanded = !o.expanded;
        o.scroll = 0;
        o.shown = None;
    }
    if let Some(bar) = grid.id(BAR).and_then(|e| bars.get(e).ok())
        && bar.value != o.scroll
        && o.shown.is_some()
    {
        o.scroll = bar.value;
    }
    let over = windows.single().ok().is_some_and(|w| crate::buyhud::over_window(grid.id(GRID), &areas, w));
    if over && delta != 0.0 {
        let most = o.count.div_ceil(o.columns.max(1)).saturating_sub(o.rows);
        o.scroll = if delta > 0.0 { o.scroll.saturating_sub(1) } else { (o.scroll + 1).min(most) };
    }
}

/// The objects on a tab: in the catalogue, one of each name, cheapest first.
fn listed<'a>(catalog: &'a Catalog, flags: &HashMap<Key, u32>, tab: ObjTab) -> Vec<&'a CatalogEntry> {
    let mut v: Vec<&CatalogEntry> = catalog.entries.iter().filter(|e| e.price > 0 && !e.name.is_empty() && !e.shell && tab.keeps(e, flags.get(&e.key).copied().unwrap_or(0))).collect();
    v.sort_by(|a, b| a.price.cmp(&b.price).then(a.name.cmp(&b.name)));
    v.dedup_by(|a, b| a.name == b.name);
    v
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    mut objects: ResMut<BuildObjects>,
    hud: Res<crate::buildhud::BuildHud>,
    buy: Res<BuyMode>,
    ui: Option<ResMut<UiAssets>>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    catalog: Res<Catalog>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    windows: Query<&Window, With<PrimaryWindow>>,
    (mut nodes, mut visibility, mut buttons, mut bars): (Query<&mut Node>, Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>),
) {
    if !showing(&buy) {
        return;
    }
    let (Some(mut ui), Ok(window)) = (ui, windows.single()) else { return };
    let o = &mut *objects;
    let k = (buy.category == GARDENING_TAB) as usize;
    // (The tab chosen must be one of this catalogue's.)
    let tabs: &[ObjTab] = if k == 1 { &GARDENING } else { &CONSTRUCTION };
    if !tabs.contains(&o.tab) {
        o.choose(tabs[0]);
    }
    let in_hand = buy.placing.as_ref().map(|p| p.objd);
    let signature = (o.tab, o.scroll, o.expanded, window.width() as u32, in_hand);
    if o.shown == Some(signature) {
        return;
    }
    let items = listed(&catalog, &o.flags, o.tab);
    let Some(grid_window) = ui.find("BuildExpandableCatalogGrid", GRID).cloned() else { return };
    let g = grid_window.grid.unwrap_or_default();
    let step = Vec2::new(g.cell[0] + g.cell_padding[2] - g.cell_padding[0], g.cell[1] + g.cell_padding[3] - g.cell_padding[1]).max(Vec2::ONE);
    // As wide as the screen has room for (as the wallpaper and floor catalogues).
    let width = (window.width() - 328.0).max(320.0);
    o.columns = ((width - GRID_LEFT - 80.0) / step.x).floor().max(1.0) as usize;
    o.count = items.len();
    let total = o.count.div_ceil(o.columns);
    o.rows = if o.expanded { total.clamp(2, 6) } else { 2 };
    o.scroll = o.scroll.min(total.saturating_sub(o.rows));
    let lift = (o.rows - 2) as f32 * step.y;
    for id in [0x06e7_70f8, PANELS[k]] {
        if let Some(e) = hud.puck.id(id)
            && let Ok(mut n) = nodes.get_mut(e)
        {
            n.width = Val::Px(width);
            n.overflow = Overflow::visible();
        }
    }
    if let Some(e) = hud.puck.id(HOSTS[k])
        && let Ok(mut n) = nodes.get_mut(e)
    {
        n.overflow = Overflow::visible();
    }
    // The tabs, left to right, the chosen on top.
    set_visible(&mut visibility, hud.puck.id(TABS[k]), true);
    if let Some(e) = hud.puck.id(TABS[k])
        && let Ok(mut n) = nodes.get_mut(e)
    {
        n.top = Val::Px(-32.0 - lift);
        n.overflow = Overflow::visible();
    }
    if let (Some(parent), Some(template)) = (hud.puck.id(TABS[k]), ui.layout("TabControl").cloned()) {
        let holder = crate::buyhud::holder(&mut commands, parent, &mut o.tabs[k], &visibility);
        for (i, tab) in tabs.iter().enumerate().rev() {
            let mut w = template.clone();
            let x = i as f32 * TAB_STEP;
            w.area = [x + 1.0, 1.0, x + 44.0, 52.0];
            let spawned = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
            let Some(root) = spawned.root else { continue };
            let source = ui.find("Build", tab.window()).cloned();
            let tip = source.as_ref().map(|s| s.tooltip.clone()).filter(|t| !t.is_empty() && !t.contains(':')).unwrap_or_else(|| tab.label().to_string());
            commands.entity(root).insert((*tab, crate::icons::Tooltip(tip)));
            if *tab == o.tab {
                commands.entity(root).insert((crate::layout::Selected, ZIndex(1)));
            }
            if let Some((icon, _)) = source.and_then(|s| ui.image(&mut images, s.icon)) {
                commands.entity(root).insert(crate::layout::SetIcon(icon));
            }
        }
    }
    // The grid: two rows, or up to six expanded (growing upwards).
    let grid = &o.grids[k];
    if let Some(root) = grid.root
        && let Ok(mut n) = nodes.get_mut(root)
    {
        n.top = Val::Px(-lift);
    }
    for (id, on) in [(0x301, o.rows == 2), (0x302, o.rows == 2), (0x201, o.rows > 2), (0x202, o.rows > 2), (BAR, total > o.rows)] {
        set_visible(&mut visibility, grid.id(id), on);
    }
    if let Some(e) = grid.id(BAR) {
        if let Ok(mut bar) = bars.get_mut(e) {
            bar.value = o.scroll;
            bar.total = total;
            bar.visible = o.rows;
        }
        if let Ok(mut n) = nodes.get_mut(e) {
            n.top = Val::Px(1.0);
            n.bottom = Val::Px(2.0);
            n.height = Val::Auto;
        }
    }
    if let Some(e) = grid.id(EXPAND)
        && let Ok(mut b) = buttons.get_mut(e)
    {
        b.selected = o.expanded;
        b.disabled = total <= 2;
    }
    let Some(parent) = grid.id(GRID) else { return };
    if let Ok(mut n) = nodes.get_mut(parent) {
        n.overflow = Overflow::clip();
    }
    let holder = crate::buyhud::holder(&mut commands, parent, &mut o.cells[k], &visibility);
    let Some(template) = ui.layout("BuyCatalogItem").cloned() else { return };
    for (n, item) in items.iter().skip(o.scroll * o.columns).take(o.rows * o.columns).enumerate() {
        let (x, y) = (g.padding[0] + (n % o.columns) as f32 * step.x, g.padding[1] + (n / o.columns) as f32 * step.y);
        let mut w = template.clone();
        w.cls = "Button".into();
        w.area = [x, y, x + step.x, y + g.cell[1]];
        let cell = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        let Some(root) = cell.root else { continue };
        commands.entity(root).insert((BuyButton::Item(item.key), crate::icons::Tooltip(format!("{}\n§{}", item.name, crate::lifetime::group(item.price as i64)))));
        if in_hand == Some(item.key) {
            commands.entity(root).insert(crate::layout::Selected);
        }
        let thumb = game_ui.as_deref_mut().and_then(|gu| gu.icon(&mut images, &s3bake::gamedata::thumb_name(item.key.2)));
        if let (Some(t), Some(win)) = (thumb, cell.id(CELL_THUMB)) {
            crate::hudpanels::picture(&mut commands, win, t, Color::WHITE);
        }
    }
    o.shown = Some(signature);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(opening: Option<bool>, kind: crate::interact::ObjectKind) -> CatalogEntry {
        CatalogEntry { key: (1, 0, 1), name: "x".into(), price: 10, kind, opening, category: "Misc", shell: false }
    }

    #[test]
    fn tabs_sort_by_build_categories() {
        use crate::interact::ObjectKind as K;
        let door = entry(Some(true), K::Other);
        assert!(ObjTab::Doors.keeps(&door, 0x2));
        assert!(!ObjTab::Arches.keeps(&door, 0x2));
        assert!(ObjTab::Arches.keeps(&door, 0x2 | ARCH));
        assert!(!ObjTab::Doors.keeps(&door, 0x2 | ARCH));
        assert!(ObjTab::Windows.keeps(&entry(Some(false), K::Other), 0x4));
        let plant = entry(None, K::Other);
        assert!(ObjTab::Trees.keeps(&plant, TREE | SHRUB));
        assert!(!ObjTab::Shrubs.keeps(&plant, TREE | SHRUB), "a tree that's also a shrub is a tree");
        assert!(ObjTab::Rocks.keeps(&plant, ROCK));
        assert!(ObjTab::Fireplaces.keeps(&entry(None, K::Fireplace), 0));
        assert_eq!(ObjTab::Rocks.catalogue(), GARDENING_TAB);
        assert_eq!(ObjTab::Columns.catalogue(), CONSTRUCTION_TAB);
    }
}
