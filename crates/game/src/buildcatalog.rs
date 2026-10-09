//! Floor and wallpaper catalogues in the original Build panels and expandable grid.
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use s3bake::ui::UiPlace;

use crate::buy::{BuyButton, BuyMode, FLOORS_TAB, WALLPAPER_TAB};
use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar};
use crate::livehud::{pressed, set_visible};

const PANELS: [u32; 2] = [0x320, 0x340];
const HOSTS: [u32; 2] = [0x323, 0x343];
const TABS: [u32; 2] = [0x322, 0x342];
const GRID: u32 = 0x0a5f_e43c;
const EXPAND: u32 = 0x0a65_20dd;
const BAR: u32 = 0x0600_0002;

#[derive(Resource)]
pub struct BuildCatalog {
    grids: [Spawned; 2],
    cells: [Option<Entity>; 2],
    category: usize,
    scroll: usize,
    columns: usize,
    rows: usize,
    count: usize,
    expanded: bool,
    shown: Option<(usize, usize, bool, Option<usize>, u32)>,
}

pub fn showing(buy: &BuyMode) -> bool {
    buy.active && buy.native_covers && matches!(buy.category, WALLPAPER_TAB | FLOORS_TAB)
}

pub fn spawn(mut commands: Commands, hud: Option<Res<crate::buildhud::BuildHud>>, ui: Option<ResMut<UiAssets>>,
    mut images: ResMut<Assets<Image>>, mut fonts: ResMut<Assets<Font>>, mut buy: ResMut<BuyMode>) {
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
        // Pattern category metadata is not baked yet; show the original All tab.
        if let Some(tabs) = ui.find("Build", TABS[k]) {
            for tab in &tabs.children {
                if let Some(e) = hud.puck.id(tab.id) {
                    commands.entity(e).insert(Visibility::Hidden);
                }
            }
        }
        if let (Some(parent), Some(mut tab)) = (hud.puck.id(TABS[k]), ui.layout("TabControl").cloned()) {
            tab.area = [1.0, 1.0, 45.0, 52.0];
            let spawned = ui.spawn_under(&mut commands, &mut images, &mut fonts, &tab, parent);
            if let Some(root) = spawned.root {
                commands.entity(root).insert((crate::layout::Selected, crate::icons::Tooltip("All".into())));
                if let Some((image, _)) = ui.image(&mut images, s3pkg::fnv64("glb_i_all_r2")) {
                    commands.entity(root).insert(crate::layout::SetIcon(image));
                }
            }
        }
    }
    for (id, fill) in [(0x324, false), (0x326, true), (0x344, false), (0x345, true)] {
        if let Some(e) = hud.puck.id(id) {
            commands.entity(e).insert((Fill(fill), crate::icons::Tooltip(if fill { "Fill room (Shift)" } else { "Single surface" }.into())));
        }
    }
    for id in [0x329, 0x348, 0x0b4e_f550] {
        if let Some(e) = hud.puck.id(id) { commands.entity(e).insert(Visibility::Hidden); }
    }
    buy.native_covers = true;
    commands.insert_resource(BuildCatalog { grids, cells: [None, None], category: usize::MAX, scroll: 0, columns: 1, rows: 2, count: 0, expanded: false, shown: None });
}

#[derive(Component)]
pub struct Fill(bool);

#[allow(clippy::too_many_arguments)]
pub fn controls(mut catalog: ResMut<BuildCatalog>, mut buy: ResMut<BuyMode>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>, fill: Query<(&Interaction, &Fill), Changed<Interaction>>,
    bars: Query<&UiScrollBar>, windows: Query<&Window, With<PrimaryWindow>>,
    areas: Query<(&ComputedNode, &UiGlobalTransform, &InheritedVisibility)>, mut wheel: MessageReader<MouseWheel>) {
    let delta: f32 = wheel.read().map(|w| w.y).sum();
    if !showing(&buy) { return; }
    let c = &mut *catalog;
    let mut reset = false;
    if buy.painting.is_some() && c.shown.is_some_and(|s| s.3 != buy.painting) { c.expanded = false; }
    if c.category != buy.category {
        c.category = buy.category;
        c.scroll = 0;
        c.expanded = false;
        reset = true;
    }
    for (i, fill) in &fill { if *i == Interaction::Pressed { buy.cover_fill = fill.0; } }
    let k = (buy.category == FLOORS_TAB) as usize;
    let grid = &c.grids[k];
    if pressed(&clicks, grid.id(EXPAND)) { c.expanded = !c.expanded; c.scroll = 0; reset = true; }
    if !reset && let Some(bar) = grid.id(BAR).and_then(|e| bars.get(e).ok()) { c.scroll = bar.value; }
    let over = windows.single().ok().is_some_and(|w| crate::buyhud::over_window(grid.id(GRID), &areas, w));
    if over && delta != 0.0 {
        c.scroll = if delta > 0.0 { c.scroll.saturating_sub(1) } else { (c.scroll + 1).min(c.count.div_ceil(c.columns.max(1)).saturating_sub(c.rows)) };
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(mut commands: Commands, mut catalog: ResMut<BuildCatalog>, hud: Res<crate::buildhud::BuildHud>, buy: Res<BuyMode>,
    ui: Option<ResMut<UiAssets>>, game_ui: Option<Res<crate::icons::GameUi>>, data: Res<crate::baked::Baked>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats, mut fonts): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>, ResMut<Assets<Font>>),
    windows: Query<&Window, With<PrimaryWindow>>, mut nodes: Query<&mut Node>, mut visibility: Query<&mut Visibility>,
    mut buttons: Query<&mut UiButton>, mut bars: Query<&mut UiScrollBar>) {
    if !showing(&buy) { return; }
    let (Some(mut ui), Some(game_ui), Ok(window)) = (ui, game_ui, windows.single()) else { return };
    for (id, fill) in [(0x324, false), (0x326, true), (0x344, false), (0x345, true)] {
        if let Some(e) = hud.puck.id(id) && let Ok(mut b) = buttons.get_mut(e) { b.selected = buy.cover_fill == fill; }
    }
    let c = &mut *catalog;
    let signature = (buy.category, c.scroll, c.expanded, buy.painting, window.width() as u32);
    if c.shown == Some(signature) { return; }
    let k = (buy.category == FLOORS_TAB) as usize;
    let items: Vec<_> = game_ui.data.patterns.iter().enumerate().filter(|(_, p)| p.floor == (k == 1)).collect();
    let Some(grid_window) = ui.find("BuildExpandableCatalogGrid", GRID).cloned() else { return };
    let g = grid_window.grid.unwrap_or_default();
    let step = Vec2::new(g.cell[0] + g.cell_padding[2] - g.cell_padding[0], g.cell[1] + g.cell_padding[3] - g.cell_padding[1]);
    let width = (window.width() - 328.0).max(320.0);
    let left = if k == 0 { 158.0 } else { 110.0 };
    c.columns = ((width - left - 80.0) / step.x).floor().max(1.0) as usize;
    c.count = items.len();
    let total = c.count.div_ceil(c.columns);
    c.rows = if c.expanded { total.clamp(2, 6) } else { 2 };
    c.scroll = c.scroll.min(total.saturating_sub(c.rows));
    let lift = (c.rows - 2) as f32 * step.y;
    for id in [0x06e7_70f8, PANELS[k]] {
        if let Some(e) = hud.puck.id(id) && let Ok(mut node) = nodes.get_mut(e) { node.width = Val::Px(width); node.overflow = Overflow::visible(); }
    }
    if let Some(e) = hud.puck.id(HOSTS[k]) && let Ok(mut n) = nodes.get_mut(e) { n.overflow = Overflow::visible(); }
    set_visible(&mut visibility, hud.puck.id(TABS[k]), true);
    if let Some(e) = hud.puck.id(TABS[k]) && let Ok(mut n) = nodes.get_mut(e) { n.top = Val::Px(-32.0 - lift); n.overflow = Overflow::visible(); }
    let grid = &c.grids[k];
    if let Some(root) = grid.root && let Ok(mut n) = nodes.get_mut(root) { n.top = Val::Px(-lift); }
    for (id, on) in [(0x301, c.rows == 2), (0x302, c.rows == 2), (0x201, c.rows > 2), (0x202, c.rows > 2), (BAR, total > c.rows)] {
        set_visible(&mut visibility, grid.id(id), on);
    }
    if let Some(e) = grid.id(BAR) {
        if let Ok(mut bar) = bars.get_mut(e) { bar.value = c.scroll; bar.total = total; bar.visible = c.rows; }
        if let Ok(mut n) = nodes.get_mut(e) { n.top = Val::Px(1.0); n.bottom = Val::Px(2.0); n.height = Val::Auto; }
    }
    if let Some(e) = grid.id(EXPAND) && let Ok(mut b) = buttons.get_mut(e) { b.selected = c.expanded; b.disabled = total <= 2; }
    let Some(parent) = grid.id(GRID) else { return };
    if let Ok(mut n) = nodes.get_mut(parent) { n.overflow = Overflow::clip(); }
    let holder = crate::buyhud::holder(&mut commands, parent, &mut c.cells[k], &visibility);
    let Some(template) = ui.layout("BuyCatalogItem").cloned() else { return };
    for (n, (index, pattern)) in items.iter().skip(c.scroll * c.columns).take(c.rows * c.columns).enumerate() {
        let (x, y) = (g.padding[0] + (n % c.columns) as f32 * step.x, g.padding[1] + (n / c.columns) as f32 * step.y);
        let mut w = template.clone();
        w.cls = "Button".into();
        w.area = [x, y, x + step.x, y + g.cell[1]];
        let cell = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        if let Some(root) = cell.root {
            commands.entity(root).insert((BuyButton::Pattern(*index), crate::icons::Tooltip(format!("{}\n§{}", pattern.name, pattern.price))));
            if buy.painting == Some(*index) { commands.entity(root).insert(crate::layout::Selected); }
        }
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        if let Some(texture) = assets.texture(&mut ctx, pattern.texture) && let Some(thumb) = cell.id(0x05ac_1600) {
            crate::hudpanels::picture(&mut commands, thumb, texture, Color::WHITE);
        }
    }
    c.shown = Some((buy.category, c.scroll, c.expanded, buy.painting, window.width() as u32));
}

/// BUILD_CATALOG_TEST=1 exercises the actual expand button, scrollbar and pattern cells.
pub fn probe(time: Res<Time>, catalog: Res<BuildCatalog>, buy: Res<BuyMode>,
    hud: Res<crate::buildhud::BuildHud>,
    live: Option<Res<crate::livehud::LiveHud>>, visibility: Query<&InheritedVisibility>,
    mut interactions: Query<&mut Interaction>, mut bars: Query<&mut UiScrollBar>,
    cells: Query<(Entity, &BuyButton, &InheritedVisibility)>, mut stage: Local<u8>, mut selected: Local<Option<usize>>) {
    if std::env::var_os("BUILD_CATALOG_TEST").is_none() || *stage >= 6 || time.elapsed_secs() < 7.0 + *stage as f32 * 0.5 { return; }
    assert!(showing(&buy));
    if let Some(live) = live {
        for root in [live.motives.root, live.skills.root, live.simology.root, live.career.root, live.inventory.root].into_iter().flatten() {
            assert!(!visibility.get(root).unwrap().get(), "Live panels must stay hidden while building");
        }
    }
    let k = (buy.category == FLOORS_TAB) as usize;
    let grid = &catalog.grids[k];
    match *stage {
        0 => {
            assert_eq!(catalog.rows, 2);
            assert!(catalog.count > catalog.columns * 2);
            *interactions.get_mut(grid.id(EXPAND).unwrap()).unwrap() = Interaction::Pressed;
            *interactions.get_mut(hud.puck.id(if k == 0 { 0x326 } else { 0x345 }).unwrap()).unwrap() = Interaction::Pressed;
        }
        1 => {
            assert!(catalog.rows > 2);
            assert!(buy.cover_fill);
            if catalog.count.div_ceil(catalog.columns) <= catalog.rows {
                *interactions.get_mut(grid.id(EXPAND).unwrap()).unwrap() = Interaction::Pressed;
            }
        }
        2 => {
            bars.get_mut(grid.id(BAR).unwrap()).unwrap().value = 1;
        }
        3 => {
            assert_eq!(catalog.scroll, 1);
            let (e, index) = cells.iter().filter_map(|(e, b, v)| if let BuyButton::Pattern(index) = b { v.get().then_some((e, *index)) } else { None }).min_by_key(|(_, index)| *index).expect("visible native catalogue cells");
            *selected = Some(index);
            *interactions.get_mut(e).unwrap() = Interaction::Pressed;
        }
        4 => {
            assert_eq!(buy.painting, *selected);
            assert!(buy.cover_fill, "changing patterns retains the selected fill tool");
            *interactions.get_mut(hud.puck.id(if k == 0 { 0x324 } else { 0x344 }).unwrap()).unwrap() = Interaction::Pressed;
        }
        5 => {
            assert!(!buy.cover_fill);
            info!("autotest: native Build catalogue PASS — expandable rows, skinned scrollbar, pattern selection and surface/room tools");
        }
        _ => unreachable!(),
    }
    *stage += 1;
}
