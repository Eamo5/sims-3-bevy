//! Build mode's terrain tools in the game's own panels (UI.dll's `BuildController` terrain
//! states, the `Build` layout's): Terrain Paint (its Paint and Erase buttons, the brush's six
//! shapes and sizes and its softness slider, and the world's own paints in the build catalogue's
//! grid beside them, two rows scrolled by the wheel or its bar) and Terrain Sculpt (Raise,
//! Lower, Level, Soften and Flatten Lot, with the same brushes; the Water tool isn't here yet).

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use s3bake::ui::UiPlace;

use crate::build::BuildTool;
use crate::buy::{BuyMode, TERRAIN_TAB};
use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar, UiSlider};
use crate::livehud::{pressed, set_visible};

pub struct BuildTerrainPlugin;

impl Plugin for BuildTerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildTerrain>().add_systems(
            Update,
            (spawn, controls, draw).chain().after(crate::buildhud::show).before(crate::buyhistory::update).run_if(in_state(crate::PlayMode::Live)).run_if(resource_exists::<crate::buildhud::BuildHud>),
        );
    }
}

// The `Build` layout's terrain panels.
pub const SCULPT_PANEL: u32 = 0x370;
pub const PAINT_PANEL: u32 = 0x380;
const SCULPT_TOOLS: [(u32, u8); 5] = [(0x261, 0), (0x262, 1), (0x263, 2), (0x27d, 3), (0x27e, 4)];
const WATER_TOOL: u32 = 0x264;
const NO_TOOLS: u32 = 0x37b;
const TOOL_SHAPES: u32 = 0x37a;
const PAINT: u32 = 0x251;
const ERASE: u32 = 0x252;
const ERASE_BACK: u32 = 0x386;
const PAINTS: u32 = 0x384;
const PAINT_GRID_PLACEHOLDER: u32 = 0x383;
const PAINT_TABS: u32 = 0x387;
const PAINT_TAB_ALL: u32 = 0x38f;
const PAINT_FILTER: u32 = 0x385;
/// The brush's shapes and sizes: small, medium and large circles, then squares (the shape's
/// frame and its picture; the sculpt panel's six, then the paint panel's).
const SHAPE_FRAMES: [u32; 2] = [0xb370, 0xb376];
const SHAPE_ICONS: [u32; 2] = [0xa370, 0xa376];
const SOFTNESS: [u32; 2] = [0xc370, 0xc371];
/// The build catalogue grid's (`BuildExpandableCatalogGrid`) item grid, expand button and bar.
const GRID: u32 = 0x0a5f_e43c;
const EXPAND: u32 = 0x0a65_20dd;
const BAR: u32 = 0x0600_0002;
const THUMB: u32 = 0x05ac_1600;

/// A shape's brush: its radius (as the plain panel's three sizes) and whether it's square.
fn shape(i: usize) -> (f32, bool) {
    (crate::terrain_paint::BRUSHES[i % 3].0, i >= 3)
}

/// The panels' state: the catalogue grid spawned under the paints' window, its cells, and what
/// it shows.
#[derive(Resource, Default)]
pub struct BuildTerrain {
    host: Option<Entity>,
    grid: Spawned,
    cells: Option<Entity>,
    scroll: usize,
    columns: usize,
    shown: Option<(u8, usize, usize, u32, bool)>,
    softness: [f32; 2],
}

/// Whether the game's terrain panels are up (the plain one stands aside).
pub fn showing(buy: &BuyMode) -> bool {
    buy.active && buy.build_look && buy.category == TERRAIN_TAB
}

/// Whether it's the sculpt panel (else the paint one).
pub fn sculpting(buy: &BuyMode) -> bool {
    buy.tool == Some(BuildTool::Sculpt)
}

/// A paint's cell.
#[derive(Component)]
struct PaintCell(u8);

fn spawn(mut commands: Commands, hud: Res<crate::buildhud::BuildHud>, mut t: ResMut<BuildTerrain>, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>)) {
    let Some(mut ui) = ui else { return };
    let host = hud.puck.id(PAINTS);
    if host.is_none() || t.host == host {
        return;
    }
    let Some(mut w) = ui.layout("BuildExpandableCatalogGrid").cloned() else { return };
    *t = BuildTerrain { host, ..default() };
    w.area = [0.0; 4];
    w.place = UiPlace::Simple(15);
    t.grid = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, host.unwrap());
    if let Some(grid) = t.grid.id(GRID) {
        commands.entity(grid).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
    }
    // (The paints aren't sorted into the game's kinds here: all of them under All.)
    if let Some(tabs) = ui.find("Build", PAINT_TABS) {
        for tab in tabs.children.iter().filter(|c| c.id != PAINT_TAB_ALL) {
            if let Some(e) = hud.puck.id(tab.id) {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
    }
    for id in [PAINT_GRID_PLACEHOLDER, PAINT_FILTER, EXPAND] {
        for e in hud.puck.all_with(id).into_iter().chain(t.grid.id(id)) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    // (The tab controls are made at run time from the game's template, as the catalogues' are:
    // the designer's placeholders put away, All in their place.)
    if let Some(e) = hud.puck.id(PAINT_TAB_ALL) {
        commands.entity(e).insert(Visibility::Hidden);
    }
    if let (Some(parent), Some(mut tab), Some(source)) = (hud.puck.id(PAINT_TABS), ui.layout("TabControl").cloned(), ui.find("Build", PAINT_TAB_ALL).cloned()) {
        tab.area = [1.0, 1.0, 44.0, 52.0];
        let spawned = ui.spawn_under(&mut commands, &mut images, &mut fonts, &tab, parent);
        if let Some(root) = spawned.root {
            commands.entity(root).insert((crate::layout::Selected, crate::icons::Tooltip(if source.tooltip.is_empty() { "All".into() } else { source.tooltip.clone() })));
            if let Some((icon, _)) = ui.image(&mut images, source.icon) {
                commands.entity(root).insert(crate::layout::SetIcon(icon));
            }
        }
        commands.entity(parent).insert(Visibility::Inherited).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::visible());
    }
    let tips = [(PAINT, "Paint"), (ERASE, "Erase"), (0x261, "Raise"), (0x262, "Lower"), (0x263, "Level"), (0x27d, "Soften"), (0x27e, "Flatten Lot"), (WATER_TOOL, "Water")];
    for (id, tip) in tips {
        if let Some(e) = hud.puck.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    let names = ["Small Circle", "Medium Circle", "Large Circle", "Small Square", "Medium Square", "Large Square"];
    for k in 0..2 {
        for (i, name) in names.iter().enumerate() {
            for id in [SHAPE_FRAMES[k], SHAPE_ICONS[k]] {
                if let Some(e) = hud.puck.id(id + i as u32) {
                    commands.entity(e).insert(crate::icons::Tooltip((*name).into()));
                }
            }
        }
    }
    // (Ponds aren't made here yet.)
    if let Some(e) = hud.puck.id(WATER_TOOL) {
        commands.entity(e).insert(crate::icons::Tooltip("Water (not available)".into()));
    }
    t.softness = [-1.0; 2];
}

#[allow(clippy::too_many_arguments)]
fn controls(
    mut commands: Commands,
    mut t: ResMut<BuildTerrain>,
    hud: Res<crate::buildhud::BuildHud>,
    mut buy: ResMut<BuyMode>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    cells: Query<(&Interaction, &PaintCell), Changed<Interaction>>,
    (bars, mut sliders): (Query<&UiScrollBar>, Query<&mut UiSlider>),
    (windows, areas, mut wheel): (Query<&Window, With<PrimaryWindow>>, Query<(&ComputedNode, &UiGlobalTransform, &InheritedVisibility)>, MessageReader<MouseWheel>),
    swatches: Res<crate::terrain_paint::Swatches>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let delta: f32 = wheel.read().map(|w| w.y).sum();
    if !showing(&buy) {
        return;
    }
    let s = &hud.puck;
    let hit = |id: u32| pressed(&clicks, s.id(id));
    let mut pick = |buy: &mut BuyMode, tool: BuildTool| {
        buy.drop_tools(&mut commands);
        buy.tool = Some(tool);
        play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
    };
    for (id, n) in SCULPT_TOOLS {
        if hit(id) {
            pick(&mut buy, BuildTool::Sculpt);
            buy.sculpt = n;
        }
    }
    if hit(ERASE) {
        pick(&mut buy, BuildTool::Terrain);
        buy.terrain = crate::terrain_paint::ERASE;
    }
    if hit(PAINT) {
        pick(&mut buy, BuildTool::Terrain);
        if buy.terrain == crate::terrain_paint::ERASE {
            buy.terrain = 0;
        }
    }
    for (i, c) in &cells {
        if *i == Interaction::Pressed {
            pick(&mut buy, BuildTool::Terrain);
            buy.terrain = c.0;
        }
    }
    let panel = sculpting(&buy) as usize;
    for (k, (frames, icons)) in SHAPE_FRAMES.iter().zip(SHAPE_ICONS).enumerate() {
        for i in 0..6u32 {
            if hit(frames + i) || hit(icons + i) {
                (buy.brush, buy.brush_square) = shape(i as usize);
            }
        }
        // The softness slider (its value, 0 to its most: hard to soft).
        if let Some(mut sl) = s.id(SOFTNESS[k]).and_then(|e| sliders.get_mut(e).ok()) {
            let most = sl.max.max(1.0);
            if t.softness[k] < 0.0 || k != panel {
                let v = (buy.softness() * most).round();
                if sl.value != v {
                    sl.value = v;
                }
                t.softness[k] = v;
            } else if sl.value != t.softness[k] {
                t.softness[k] = sl.value;
                buy.brush_softness = Some(sl.value / most);
            }
        }
    }
    // The paints scrolled by the wheel over them, or their bar.
    let rows = 2;
    let total = swatches.0.len().div_ceil(t.columns.max(1));
    if let Some(bar) = t.grid.id(BAR).and_then(|e| bars.get(e).ok())
        && bar.value != t.scroll
    {
        t.scroll = bar.value.min(total.saturating_sub(rows));
    }
    let over = windows.single().ok().is_some_and(|w| crate::buyhud::over_window(t.grid.id(GRID), &areas, w));
    if over && delta != 0.0 {
        t.scroll = if delta > 0.0 { t.scroll.saturating_sub(1) } else { (t.scroll + 1).min(total.saturating_sub(rows)) };
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    mut t: ResMut<BuildTerrain>,
    hud: Res<crate::buildhud::BuildHud>,
    buy: Res<BuyMode>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    swatches: Res<crate::terrain_paint::Swatches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    (mut nodes, mut visibility, mut buttons, mut bars): (Query<&mut Node>, Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>),
) {
    let s = &hud.puck;
    let on = showing(&buy);
    let sculpt = sculpting(&buy);
    set_visible(&mut visibility, s.id(SCULPT_PANEL), on && sculpt);
    set_visible(&mut visibility, s.id(PAINT_PANEL), on && !sculpt);
    if !on {
        t.shown = None;
        return;
    }
    let (Some(mut ui), Ok(window)) = (ui, windows.single()) else { return };
    let erasing = buy.terrain == crate::terrain_paint::ERASE;
    let painting = buy.tool == Some(BuildTool::Terrain);
    // The panel's buttons lit as the tool in hand.
    let mut lit = |id: u32, on: bool, off: bool| {
        if let Some(mut b) = s.id(id).and_then(|e| buttons.get_mut(e).ok())
            && (b.selected, b.disabled) != (on, off)
        {
            b.selected = on;
            b.disabled = off;
        }
    };
    for (id, n) in SCULPT_TOOLS {
        lit(id, sculpt && buy.sculpt == n, false);
    }
    lit(WATER_TOOL, false, true);
    lit(PAINT, painting && !erasing, false);
    lit(ERASE, painting && erasing, false);
    let brush = if buy.brush > 0.0 { buy.brush } else { crate::terrain_paint::BRUSHES[1].0 };
    let chosen = (0..6).find(|i| {
        let (r, sq) = shape(*i);
        (r - brush).abs() < 0.01 && sq == buy.brush_square
    });
    for k in 0..2 {
        for i in 0..6 {
            lit(SHAPE_FRAMES[k] + i as u32, chosen == Some(i), false);
        }
    }
    set_visible(&mut visibility, s.id(NO_TOOLS), false);
    set_visible(&mut visibility, s.id(TOOL_SHAPES), sculpt);
    set_visible(&mut visibility, s.id(ERASE_BACK), erasing);
    set_visible(&mut visibility, s.id(PAINTS), !erasing);
    // The paints: as many columns as the width allows, two rows.
    let width = (window.width() - 328.0).max(560.0);
    for id in [0x06e7_70f8, PAINT_PANEL] {
        if let Some(mut n) = s.id(id).and_then(|e| nodes.get_mut(e).ok())
            && n.width != Val::Px(width)
        {
            n.width = Val::Px(width);
            n.overflow = Overflow::visible();
        }
    }
    let Some(g) = ui.find("BuildExpandableCatalogGrid", GRID).and_then(|w| w.grid) else { return };
    let step = Vec2::new(g.cell[0] + g.cell_padding[2] - g.cell_padding[0], g.cell[1] + g.cell_padding[3] - g.cell_padding[1]);
    t.columns = ((width - 393.0 - 97.0 - 40.0) / step.x).floor().max(1.0) as usize;
    let rows = 2;
    let total = swatches.0.len().div_ceil(t.columns);
    t.scroll = t.scroll.min(total.saturating_sub(rows));
    let signature = (if erasing { 255 } else { buy.terrain }, t.scroll, t.columns, swatches.0.len() as u32, painting);
    if t.shown == Some(signature) {
        return;
    }
    t.shown = Some(signature);
    for (id, show) in [(0x301, true), (0x302, true), (0x201, false), (0x202, false), (BAR, total > rows)] {
        set_visible(&mut visibility, t.grid.id(id), show);
    }
    if let Some(e) = t.grid.id(BAR) {
        if let Ok(mut bar) = bars.get_mut(e) {
            (bar.value, bar.total, bar.visible) = (t.scroll, total, rows);
        }
        if let Ok(mut n) = nodes.get_mut(e) {
            n.top = Val::Px(1.0);
            n.bottom = Val::Px(2.0);
            n.height = Val::Auto;
        }
    }
    let Some(parent) = t.grid.id(GRID) else { return };
    if let Ok(mut n) = nodes.get_mut(parent) {
        n.overflow = Overflow::clip();
    }
    let holder = crate::buyhud::holder(&mut commands, parent, &mut t.cells, &visibility);
    let Some(template) = ui.layout("BuyCatalogItem").cloned() else { return };
    for (n, (layer, swatch)) in swatches.0.iter().enumerate().skip(t.scroll * t.columns).take(rows * t.columns).enumerate() {
        let (x, y) = (g.padding[0] + (n % t.columns) as f32 * step.x, g.padding[1] + (n / t.columns) as f32 * step.y);
        let mut w = template.clone();
        w.cls = "Button".into();
        w.area = [x, y, x + step.x, y + g.cell[1]];
        let cell = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        let Some(root) = cell.root else { continue };
        commands.entity(root).insert((PaintCell(layer as u8), crate::icons::Tooltip(format!("Terrain Paint {}", layer + 1))));
        if painting && !erasing && buy.terrain == layer as u8 {
            commands.entity(root).insert(crate::layout::Selected);
        }
        if let Some(thumb) = cell.id(THUMB) {
            crate::hudpanels::picture(&mut commands, thumb, swatch.clone(), Color::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brush_shapes() {
        assert_eq!(shape(0), (1.0, false));
        assert_eq!(shape(2), (3.5, false));
        assert_eq!(shape(4), (2.0, true));
        let mut buy = BuyMode::default();
        buy.active = true;
        buy.build_look = true;
        buy.show(TERRAIN_TAB);
        assert!(showing(&buy) && !sculpting(&buy));
        buy.tool = Some(BuildTool::Sculpt);
        assert!(sculpting(&buy));
    }
}
