//! Build mode's roofs, fences and half walls in the game's own panels (the `Build` layout's Roof
//! Catalog, Fence Catalog and Half Wall Catalog), each with the build catalogue's grid
//! (`BuildExpandableCatalogGrid`, two rows, scrolled by the wheel or its bar):
//! - Roofs: auto-roofing on or off, Remove All Roofs, the Roof Pitch slider (how steep the
//!   household's roofs are, shallow to steep), and the tabs for the roofs' shapes (hip, gable,
//!   shed, mansard) and their patterns (the catalogue's roof textures);
//! - Fences: the catalogue's fences, each with the game's picture of it (else a piece of it
//!   rendered), its name and price on hover; Gates opens the gates;
//! - Half walls: the catalogue's half walls likewise, the Walls and Wallpaper buttons back to
//!   those tools;
//! - Stairs: the catalogue's staircases (CSTR) with the game's pictures of them and their prices
//!   (a tile's), auto-railings on or off, and Railings: the catalogue's railings (CRAL), a
//!   staircase put in with the one chosen (else its own, by name);
//! - Blueprints: Blueprint mode's ready-made rooms (`blueprints`) by room kind (the game's
//!   `BlueprintCatalog` tabs, its pictures), each with the game's picture of it and its price;
//! - Foundations: the catalogue's foundation kinds (CFND: foundation, deck, platform), each its
//!   sides' covering, the household's foundation in the one chosen.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use s3bake::ui::UiPlace;

use crate::build::BuildTool;
use crate::building::{ROOF_PITCHES, RoofStyle};
use crate::buy::{BuyButton, BuyMode, FENCES_TAB, ROOFS_TAB, WALLPAPER_TAB};
use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar, UiSlider};
use crate::livehud::{pressed, set_visible};

pub struct BuildCatsPlugin;

impl Plugin for BuildCatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildCats>().add_systems(
            Update,
            (spawn, controls, draw).chain().after(crate::buildhud::show).before(crate::buyhistory::update).run_if(in_state(crate::PlayMode::Live)).run_if(resource_exists::<crate::buildhud::BuildHud>),
        );
    }
}

/// The catalogues: the roofs', the fences', the half walls'.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cat {
    Roofs,
    Fences,
    HalfWalls,
    Stairs,
    Railings,
    Foundations,
    Blueprints,
}

impl Cat {
    const ALL: [Cat; 7] = [Cat::Roofs, Cat::Fences, Cat::HalfWalls, Cat::Stairs, Cat::Railings, Cat::Foundations, Cat::Blueprints];

    /// Its panel, and the window its grid goes in.
    fn ids(self) -> (u32, u32) {
        match self {
            Cat::Roofs => (0x3b0, 0x3b3),
            Cat::Fences => (0x3c0, 0x3c3),
            Cat::HalfWalls => (0x0a49_b993, 0x0a49_ba87),
            Cat::Stairs => (0x3e0, 0x3e2),
            Cat::Railings => (0x3e1, 0x3e3),
            Cat::Foundations => (0x000a_a300, 0x000a_a302),
            Cat::Blueprints => (0x350, 0x353),
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

// The roof panel's controls.
const AUTO_ROOF_ON: u32 = 0x3b4;
const AUTO_ROOF_OFF: u32 = 0x3b5;
const REMOVE_ROOFS: u32 = 0x3b6;
const PITCH_TOOL: u32 = 0x3b7;
const PITCH: u32 = 0x3b9;
const PAGODA: u32 = 0x3ba;
const ROOF_TABS: u32 = 0x3b2;
/// Its tabs: the roofs' shapes, their patterns, diagonal roofs.
const ROOF_TYPES: u32 = 0x3b02;
const ROOF_TEXTURES: u32 = 0x3b01;
const ROOF_DIAGONAL: u32 = 0x3b03;
// The fence panel's.
const FENCES_BUTTON: u32 = 0x3c6;
const GATES_BUTTON: u32 = 0x3c7;
const FENCE_SINGLE: u32 = 0x3c4;
const FENCE_ROOM: u32 = 0x3c5;
// The half walls'.
const HALF_SINGLE: u32 = 0x0a49_be81;
const HALF_ROOM: u32 = 0x0a49_be95;
const HALF_DIAGONAL: u32 = 0x0a49_bea4;
const HALF_WALL: u32 = 0x0a49_beb4;
const HALF_WALLS_BUTTON: u32 = 0x0a4a_9429;
const HALF_WALLPAPER_BUTTON: u32 = 0x0a4a_9443;
// The stairs' and railings' panels'.
const STAIR_TABS: u32 = 0x3f2;
const STAIR_TAB: u32 = 0x3f21;
const SPIRAL_TAB: u32 = 0x3f22;
const STAIRS_BUTTONS: [u32; 2] = [0x3e4, 0x3e5];
const RAILINGS_BUTTONS: [u32; 2] = [0x3e6, 0x3e7];
const ELEVATORS_BUTTONS: [u32; 2] = [0x0a14_e850, 0x0a15_0aa0];
const AUTO_RAILS_ON: u32 = 0x3e9;
const AUTO_RAILS_OFF: u32 = 0x3ea;
/// The stair width slider and its words (the household's staircases are a tile wide).
const STAIR_WIDTH: [u32; 3] = [0x3e8, 0x3f1, 0x3f3];
/// Their filters (not here).
const FILTERS: [u32; 5] = [0x3b8, 0x3cc, 0x0a49_ba90, 0x3ef, 0x3f0];
/// The grid's (`BuildExpandableCatalogGrid`).
const GRID: u32 = 0x0a5f_e43c;
const EXPAND: u32 = 0x0a65_20dd;
const BAR: u32 = 0x0600_0002;
const THUMB: u32 = 0x05ac_1600;

#[derive(Resource, Default)]
pub struct BuildCats {
    spawned_for: Option<Entity>,
    grids: Vec<Spawned>,
    cells: [Option<Entity>; 7],
    tabs: [Option<Entity>; 7],
    scroll: usize,
    columns: usize,
    count: usize,
    /// The roofs' patterns shown (else their shapes).
    textures: bool,
    shown: Option<(usize, usize, usize, bool, Option<usize>, u8, u32)>,
    roof_icons: Vec<Handle<Image>>,
    /// The pitch slider's value as last set.
    pitch: f32,
    /// Blueprints' plans drawn.
    plans: std::collections::HashMap<usize, Handle<Image>>,
}

/// The catalogue the game's panel is up for, if any.
fn current(buy: &BuyMode) -> Option<Cat> {
    if !(buy.active && buy.build_look) {
        return None;
    }
    match buy.category {
        ROOFS_TAB => Some(Cat::Roofs),
        FENCES_TAB if buy.half_walls => Some(Cat::HalfWalls),
        FENCES_TAB => Some(Cat::Fences),
        crate::buy::BUILD_TAB if matches!(buy.tool, Some(BuildTool::Foundation | BuildTool::DiagonalFoundation)) => Some(Cat::Foundations),
        crate::buy::BUILD_TAB if buy.tool == Some(BuildTool::Blueprint) => Some(Cat::Blueprints),
        crate::buy::BUILD_TAB if buy.tool == Some(BuildTool::Stairs) => Some(if buy.railings_view { Cat::Railings } else { Cat::Stairs }),
        _ => None,
    }
}

/// Whether one of these panels is up (the plain one stands aside).
pub fn showing(buy: &BuyMode) -> bool {
    current(buy).is_some()
}

/// A tab made from the game's template, with the designer's tab's icon and words.
#[derive(Component)]
struct RoofTab(bool);

/// The stairs panel's spiral staircases tab.
#[derive(Component)]
struct SpiralTab;

/// A blueprint room kind's tab (its flag bit).
#[derive(Component)]
struct RoomTab(u8);

/// Blueprint mode's room kinds, as the game's `BlueprintCatalog` has them: their flag bits,
/// names and pictures.
const ROOM_KINDS: [(u8, &str, &str); 7] = [
    (1, "Living Room", "buy_room_livingroom_r2"),
    (2, "Dining Room", "buy_room_dining_r2"),
    (3, "Kitchen", "buy_room_kitchen_r2"),
    (4, "Kids' Room", "buy_room_kids_r2"),
    (5, "Bathroom", "buy_room_bathroom_r2"),
    (6, "Bedroom", "buy_room_bedroom_r2"),
    (7, "Study", "buy_room_study_r2"),
];

/// A roof shape's picture: its outline from the front, as the build catalogue's icons are drawn.
fn roof_icon(style: RoofStyle) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const S: usize = 64;
    // (Front elevations in a 0..1 square, y down: the roof, then the wall under it.)
    let roof: Vec<(f32, f32)> = match style {
        RoofStyle::Hip => vec![(0.06, 0.62), (0.3, 0.3), (0.7, 0.3), (0.94, 0.62)],
        RoofStyle::Gable => vec![(0.06, 0.62), (0.5, 0.2), (0.94, 0.62)],
        RoofStyle::Shed => vec![(0.06, 0.62), (0.06, 0.5), (0.94, 0.26), (0.94, 0.62)],
        RoofStyle::Mansard => vec![(0.06, 0.62), (0.16, 0.36), (0.3, 0.3), (0.7, 0.3), (0.84, 0.36), (0.94, 0.62)],
    };
    let wall = [(0.14, 0.62), (0.86, 0.62), (0.86, 0.88), (0.14, 0.88)];
    let inside = |poly: &[(f32, f32)], x: f32, y: f32| {
        let mut c = false;
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + n - 1) % n]);
            if (a.1 > y) != (b.1 > y) && x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0 {
                c = !c;
            }
        }
        c
    };
    let mut px = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            // (Four samples a pixel, for smooth edges.)
            let (mut r, mut w) = (0.0, 0.0);
            for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let (u, v) = ((x as f32 + dx) / S as f32, (y as f32 + dy) / S as f32);
                if inside(&roof, u, v) {
                    r += 0.25;
                } else if inside(&wall, u, v) {
                    w += 0.25;
                }
            }
            let (roof_c, wall_c) = ([42.0, 72.0, 140.0], [226.0, 232.0, 244.0]);
            let a = r + w;
            if a > 0.0 {
                let i = (y * S + x) * 4;
                for k in 0..3 {
                    px[i + k] = ((roof_c[k] * r + wall_c[k] * w) / a) as u8;
                }
                px[i + 3] = (a * 255.0) as u8;
            }
        }
    }
    Image::new(Extent3d { width: S as u32, height: S as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

fn spawn(mut commands: Commands, hud: Res<crate::buildhud::BuildHud>, mut c: ResMut<BuildCats>, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>)) {
    let Some(mut ui) = ui else { return };
    if hud.puck.root.is_none() || c.spawned_for == hud.puck.root {
        return;
    }
    let Some(template) = ui.layout("BuildExpandableCatalogGrid").cloned() else { return };
    *c = BuildCats { spawned_for: hud.puck.root, ..default() };
    for cat in Cat::ALL {
        let (_, host) = cat.ids();
        let Some(host) = hud.puck.id(host) else { return };
        let mut w = template.clone();
        w.area = [0.0; 4];
        w.place = UiPlace::Simple(15);
        let g = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, host);
        if let Some(grid) = g.id(GRID) {
            commands.entity(grid).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
        }
        if let Some(e) = g.id(EXPAND) {
            commands.entity(e).insert(Visibility::Hidden);
        }
        c.grids.push(g);
    }
    let s = &hud.puck;
    for id in FILTERS.into_iter().chain([PITCH_TOOL, PAGODA, STAIR_TAB, SPIRAL_TAB, 0x356, 0x3501, 0x3503, 0x3504, 0x3505]).chain(STAIR_WIDTH) {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for name in ["Pagoda Short", "Pagoda Long"] {
        if let Some(e) = s.comment(name) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    // (The roof tabs are made at run time from the game's template: the designer's put away.)
    for id in [ROOF_TYPES, ROOF_TEXTURES, ROOF_DIAGONAL] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    let tips = [
        (AUTO_ROOF_ON, "Auto-Roofing On"),
        (AUTO_ROOF_OFF, "Auto-Roofing Off"),
        (REMOVE_ROOFS, "Remove All Roofs"),
        (PITCH, "Roof Pitch"),
        (FENCES_BUTTON, "Fences"),
        (GATES_BUTTON, "Gates"),
        (FENCE_SINGLE, "Fence Tool"),
        (FENCE_ROOM, "Fence Room Tool"),
        (HALF_SINGLE, "Wall Tool"),
        (HALF_ROOM, "Room Tool"),
        (HALF_DIAGONAL, "Diagonal Room Tool"),
        (HALF_WALL, "Half Wall Tool"),
        (HALF_WALLS_BUTTON, "Walls"),
        (HALF_WALLPAPER_BUTTON, "Wallpaper"),
        (AUTO_RAILS_ON, "Auto-Railings On"),
        (AUTO_RAILS_OFF, "Auto-Railings Off"),
    ];
    for (id, tip) in tips {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    for (ids, tip) in [(STAIRS_BUTTONS, "Stairs"), (RAILINGS_BUTTONS, "Railings"), (ELEVATORS_BUTTONS, "Elevators")] {
        for id in ids {
            for e in s.all_with(id) {
                commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
            }
        }
    }
    c.roof_icons = RoofStyle::ALL.iter().map(|r| images.add(roof_icon(*r))).collect();
    c.pitch = -1.0;
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn controls(
    mut commands: Commands,
    mut c: ResMut<BuildCats>,
    hud: Res<crate::buildhud::BuildHud>,
    mut buy: ResMut<BuyMode>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    (clicks, tabs, room_tabs, spiral_tab): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &RoofTab), Changed<Interaction>>, Query<(&Interaction, &RoomTab), Changed<Interaction>>, Query<&Interaction, (With<SpiralTab>, Changed<Interaction>)>),
    (bars, mut sliders, mouse): (Query<&UiScrollBar>, Query<&mut UiSlider>, Res<ButtonInput<MouseButton>>),
    (windows, areas, mut wheel): (Query<&Window, With<PrimaryWindow>>, Query<(&ComputedNode, &UiGlobalTransform, &InheritedVisibility)>, MessageReader<MouseWheel>),
    mut objects: Option<ResMut<crate::buildobjects::BuildObjects>>,
) {
    let delta: f32 = wheel.read().map(|w| w.y).sum();
    let Some(cat) = current(&buy) else {
        c.scroll = 0;
        return;
    };
    let s = &hud.puck;
    let hit = |id: u32| pressed(&clicks, s.id(id));
    match cat {
        Cat::Roofs => {
            for (i, t) in &tabs {
                if *i == Interaction::Pressed && c.textures != t.0 {
                    c.textures = t.0;
                    c.scroll = 0;
                }
            }
            let roofs_on = building.as_deref().is_none_or(|b| b.roofs_on);
            if hit(AUTO_ROOF_ON) && !roofs_on {
                buy.auto_roof_pick = Some(true);
            }
            if (hit(AUTO_ROOF_OFF) || hit(REMOVE_ROOFS)) && roofs_on {
                buy.auto_roof_pick = Some(false);
            }
            // The pitch slider: the roofs made as steep as it's put, once it's let go.
            if let Some(mut sl) = s.id(PITCH).and_then(|e| sliders.get_mut(e).ok()) {
                let most = sl.max.max(1.0);
                let now = building.as_deref().map_or(crate::building::ROOF_PITCH, |b| b.roof_pitch);
                let v = ((now - ROOF_PITCHES.0) / (ROOF_PITCHES.1 - ROOF_PITCHES.0) * most).round();
                if c.pitch < 0.0 || (sl.value == c.pitch && v != c.pitch) {
                    sl.value = v;
                    c.pitch = v;
                } else if sl.value != c.pitch && !mouse.pressed(MouseButton::Left) {
                    c.pitch = sl.value;
                    buy.roof_pitch_pick = Some(ROOF_PITCHES.0 + (ROOF_PITCHES.1 - ROOF_PITCHES.0) * sl.value / most);
                }
            }
        }
        Cat::Fences => {
            if hit(GATES_BUTTON) {
                buy.drop_tools(&mut commands);
                if let Some(o) = objects.as_deref_mut() {
                    o.choose(crate::buildobjects::ObjTab::Gates);
                    buy.show(crate::buildobjects::ObjTab::Gates.catalogue());
                }
                return;
            }
        }
        Cat::Foundations => {}
        Cat::Blueprints => {
            for (i, t) in &room_tabs {
                if *i == Interaction::Pressed && buy.blueprint_room != t.0 {
                    buy.blueprint_room = t.0;
                    c.scroll = 0;
                }
            }
        }
        Cat::Stairs | Cat::Railings => {
            // (The spiral staircases: the catalogue's grid of them.)
            if spiral_tab.iter().any(|i| *i == Interaction::Pressed)
                && let Some(o) = objects.as_deref_mut()
            {
                buy.drop_tools(&mut commands);
                o.choose(crate::buildobjects::ObjTab::SpiralStairs);
                buy.show(crate::buildobjects::ObjTab::SpiralStairs.catalogue());
                return;
            }
            let any = |ids: [u32; 2]| ids.iter().any(|id| s.all_with(*id).into_iter().any(|e| pressed(&clicks, Some(e))));
            // (The elevators: the catalogue's, put in a floor at a time as objects are.)
            if any(ELEVATORS_BUTTONS)
                && let Some(o) = objects.as_deref_mut()
            {
                buy.drop_tools(&mut commands);
                o.choose(crate::buildobjects::ObjTab::Elevators);
                buy.show(crate::buildobjects::ObjTab::Elevators.catalogue());
                return;
            }
            if any(RAILINGS_BUTTONS) && !buy.railings_view {
                buy.railings_view = true;
                c.scroll = 0;
            }
            if any(STAIRS_BUTTONS) && buy.railings_view {
                buy.railings_view = false;
                c.scroll = 0;
            }
            if hit(AUTO_RAILS_ON) {
                buy.no_rails = false;
            }
            if hit(AUTO_RAILS_OFF) {
                buy.no_rails = true;
            }
        }
        Cat::HalfWalls => {
            if hit(HALF_WALLS_BUTTON) || hit(HALF_SINGLE) || hit(HALF_ROOM) || hit(HALF_DIAGONAL) {
                let tool = if hit(HALF_ROOM) {
                    BuildTool::Room
                } else if hit(HALF_DIAGONAL) {
                    BuildTool::DiagonalRoom
                } else {
                    BuildTool::Wall
                };
                buy.drop_tools(&mut commands);
                buy.show(crate::buy::BUILD_TAB);
                buy.tool = Some(tool);
                return;
            }
            if hit(HALF_WALLPAPER_BUTTON) {
                buy.drop_tools(&mut commands);
                buy.show(WALLPAPER_TAB);
                return;
            }
        }
    }
    let g = c.grids[cat.index()].clone();
    let rows = 2;
    let total = c.count.div_ceil(c.columns.max(1));
    if let Some(bar) = g.id(BAR).and_then(|e| bars.get(e).ok())
        && bar.value != c.scroll
    {
        c.scroll = bar.value.min(total.saturating_sub(rows));
    }
    let over = windows.single().ok().is_some_and(|w| crate::buyhud::over_window(g.id(GRID), &areas, w));
    if over && delta != 0.0 {
        c.scroll = if delta > 0.0 { c.scroll.saturating_sub(1) } else { (c.scroll + 1).min(total.saturating_sub(rows)) };
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    mut c: ResMut<BuildCats>,
    hud: Res<crate::buildhud::BuildHud>,
    buy: Res<BuyMode>,
    (building, household): (Option<Res<crate::building::ActiveBuilding>>, Option<Res<crate::interact::Household>>),
    ui: Option<ResMut<UiAssets>>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
    (data, mut assets, mut thumbs): (Res<crate::baked::Baked>, ResMut<crate::objects::ObjectAssets>, ResMut<crate::thumbs::ModelThumbs>),
    (mut meshes, mut images, mut mats, mut fonts): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>, ResMut<Assets<Font>>),
    windows: Query<&Window, With<PrimaryWindow>>,
    (mut nodes, mut visibility, mut buttons, mut bars): (Query<&mut Node>, Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>),
) {
    let s = &hud.puck;
    let cat = current(&buy);
    for k in Cat::ALL {
        set_visible(&mut visibility, s.id(k.ids().0), cat == Some(k));
    }
    let Some(cat) = cat else {
        c.shown = None;
        return;
    };
    let (Some(mut ui), Some(gu), Ok(window)) = (ui, gu.as_deref_mut(), windows.single()) else { return };
    let roofs_on = building.as_deref().is_none_or(|b| b.roofs_on);
    let style = building.as_deref().map(|b| b.roof_style);
    let roof_tex = building.as_deref().map(|b| b.roof_texture);
    let mut lit = |id: u32, on: bool, off: bool| {
        if let Some(mut b) = s.id(id).and_then(|e| buttons.get_mut(e).ok())
            && (b.selected, b.disabled) != (on, off)
        {
            b.selected = on;
            b.disabled = off;
        }
    };
    lit(AUTO_ROOF_ON, roofs_on, false);
    lit(AUTO_ROOF_OFF, !roofs_on, false);
    lit(REMOVE_ROOFS, false, !roofs_on);
    lit(FENCES_BUTTON, true, false);
    lit(FENCE_SINGLE, buy.tool == Some(BuildTool::Fence), false);
    lit(FENCE_ROOM, false, true);
    lit(HALF_WALL, buy.tool == Some(BuildTool::Fence), false);
    lit(AUTO_RAILS_ON, !buy.no_rails, false);
    lit(AUTO_RAILS_OFF, buy.no_rails, false);
    for (k, id) in STAIRS_BUTTONS.iter().enumerate() {
        lit(*id, k == 0, false);
        lit(RAILINGS_BUTTONS[k], k == 1, false);
        lit(ELEVATORS_BUTTONS[k], false, false);
    }
    // The panel as wide as the screen allows, the grid's columns to fill it.
    let (panel, _) = cat.ids();
    let width = (window.width() - 328.0).max(533.0);
    for id in [0x06e7_70f8, panel] {
        if let Some(mut n) = s.id(id).and_then(|e| nodes.get_mut(e).ok())
            && n.width != Val::Px(width)
        {
            n.width = Val::Px(width);
            n.overflow = Overflow::visible();
        }
    }
    let Some(gw) = ui.find("BuildExpandableCatalogGrid", GRID).and_then(|w| w.grid) else { return };
    let step = Vec2::new(gw.cell[0] + gw.cell_padding[2] - gw.cell_padding[0], gw.cell[1] + gw.cell_padding[3] - gw.cell_padding[1]);
    let left = match cat {
        Cat::Roofs => 218.0,
        Cat::Fences => 158.0,
        Cat::HalfWalls => 208.0,
        Cat::Stairs => 264.0,
        Cat::Railings => 95.0,
        Cat::Foundations => 110.0,
        Cat::Blueprints => 48.0,
    };
    c.columns = ((width - left - 57.0 - 40.0) / step.x).floor().max(1.0) as usize;
    // What's in it: the roofs' shapes or patterns, the fences or the half walls.
    let half = cat == Cat::HalfWalls;
    let items: Vec<(BuyButton, String, bool)> = match cat {
        Cat::Roofs if !c.textures => RoofStyle::ALL.iter().map(|r| (BuyButton::RoofStyle(*r), r.label().to_string(), style == Some(*r))).collect(),
        Cat::Roofs => gu.data.roofs.iter().enumerate().map(|(i, r)| (BuyButton::Roof(i), r.name.clone(), roof_tex == Some(r.tile))).collect(),
        _ => gu
            .data
            .fences
            .iter()
            .enumerate()
            .filter(|(_, f)| f.half_wall == half)
            .map(|(i, f)| (BuyButton::Fence(i), format!("{}\n§{}", f.name, f.price), buy.tool == Some(BuildTool::Fence) && buy.fence == Some(i)))
            .collect(),
    };
    let items: Vec<(BuyButton, String, bool)> = match cat {
        Cat::Stairs => gu.data.stair_styles.iter().enumerate().map(|(i, st)| (BuyButton::Stair(i), format!("{}\n§{} a step", st.name, st.price), buy.stair_style == Some(i))).collect(),
        Cat::Railings => gu.data.rail_styles.iter().enumerate().map(|(i, r)| (BuyButton::Rail(i), r.name.clone(), buy.rail_style == Some(i))).collect(),
        Cat::Blueprints => {
            let room = if buy.blueprint_room == 0 { 1 } else { buy.blueprint_room };
            let funds = household.as_ref().map_or(i64::MAX, |h| h.funds);
            gu.data.blueprints.iter().enumerate().filter(|(_, bp)| bp.room == room).map(|(i, bp)| (BuyButton::Blueprint(i), format!("{}\n§{}{}", bp.name, bp.price, if funds < bp.price as i64 { " (can't afford)" } else { "" }), buy.blueprint == Some(i))).collect()
        }
        Cat::Foundations => {
            let now = building.as_deref().and_then(|b| b.foundation_cover);
            gu.data.foundation_styles.iter().enumerate().map(|(i, f)| (BuyButton::FoundationStyle(i), format!("{}\n§{} a tile", f.name, f.price), now == Some(f.texture))).collect()
        }
        _ => items,
    };
    let rows = 2;
    c.count = items.len();
    let total = items.len().div_ceil(c.columns);
    c.scroll = c.scroll.min(total.saturating_sub(rows));
    let chosen = items.iter().position(|i| i.2);
    let signature = (cat.index(), c.scroll, c.columns, c.textures, chosen, roofs_on as u8 | (buy.no_rails as u8) << 1 | buy.blueprint_room << 2, window.width() as u32);
    if c.shown == Some(signature) {
        return;
    }
    c.shown = Some(signature);
    // The roof tabs: shapes and patterns (diagonal roofs aren't here).
    if cat == Cat::Roofs
        && let (Some(parent), Some(template)) = (s.id(ROOF_TABS), ui.layout("TabControl").cloned())
    {
        let holder = crate::buyhud::holder(&mut commands, parent, &mut c.tabs[0], &visibility);
        for (n, (id, textures)) in [(ROOF_TYPES, false), (ROOF_TEXTURES, true)].into_iter().enumerate() {
            let Some(source) = ui.find("Build", id).cloned() else { continue };
            let mut tab = template.clone();
            let x = n as f32 * 36.0;
            tab.area = [x + 1.0, 1.0, x + 44.0, 52.0];
            let t = ui.spawn_under(&mut commands, &mut images, &mut fonts, &tab, holder);
            if let Some(root) = t.root {
                commands.entity(root).insert((RoofTab(textures), crate::icons::Tooltip(if textures { "Roof Patterns" } else { "Roof Types" }.into())));
                if textures == c.textures {
                    commands.entity(root).insert((crate::layout::Selected, ZIndex(1)));
                }
                if let Some((icon, _)) = ui.image(&mut images, source.icon) {
                    commands.entity(root).insert(crate::layout::SetIcon(icon));
                }
            }
        }
        commands.entity(parent).insert(Visibility::Inherited).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::visible());
    }
    // Blueprint mode's room kinds, a tab each.
    if cat == Cat::Blueprints
        && let (Some(parent), Some(template)) = (s.id(0x352), ui.layout("TabControl").cloned())
    {
        let holder = crate::buyhud::holder(&mut commands, parent, &mut c.tabs[6], &visibility);
        let shown = if buy.blueprint_room == 0 { 1 } else { buy.blueprint_room };
        for (n, (bit, name, pic)) in ROOM_KINDS.iter().enumerate() {
            let mut tab = template.clone();
            let x = n as f32 * 36.0;
            tab.area = [x + 1.0, 1.0, x + 44.0, 52.0];
            let t = ui.spawn_under(&mut commands, &mut images, &mut fonts, &tab, holder);
            if let Some(root) = t.root {
                commands.entity(root).insert((RoomTab(*bit), crate::icons::Tooltip((*name).into())));
                if *bit == shown {
                    commands.entity(root).insert((crate::layout::Selected, ZIndex(1)));
                }
                if let Some((icon, _)) = ui.image(&mut images, s3pkg::fnv64(pic)) {
                    commands.entity(root).insert(crate::layout::SetIcon(icon));
                }
            }
        }
        commands.entity(parent).insert(Visibility::Inherited).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::visible());
    }
    // The stairs' tabs: the staircases (this page) and the spiral staircases (the catalogue's,
    // put in as objects are).
    if cat == Cat::Stairs
        && let (Some(parent), Some(template)) = (s.id(STAIR_TABS), ui.layout("TabControl").cloned())
    {
        let holder = crate::buyhud::holder(&mut commands, parent, &mut c.tabs[3], &visibility);
        for (n, (id, tip)) in [(STAIR_TAB, "Stairs"), (SPIRAL_TAB, "Spiral Staircases")].into_iter().enumerate() {
            let Some(source) = ui.find("Build", id).cloned() else { continue };
            let mut tab = template.clone();
            let x = n as f32 * 36.0;
            tab.area = [x + 1.0, 1.0, x + 44.0, 52.0];
            let t = ui.spawn_under(&mut commands, &mut images, &mut fonts, &tab, holder);
            if let Some(root) = t.root {
                commands.entity(root).insert(crate::icons::Tooltip(tip.into()));
                if n == 0 {
                    commands.entity(root).insert((crate::layout::Selected, ZIndex(1)));
                } else {
                    commands.entity(root).insert(SpiralTab);
                }
                if let Some((icon, _)) = ui.image(&mut images, source.icon) {
                    commands.entity(root).insert(crate::layout::SetIcon(icon));
                }
            }
        }
        commands.entity(parent).insert(Visibility::Inherited).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::visible());
    }
    let k = cat.index();
    let g = c.grids[k].clone();
    for (id, show) in [(0x301, true), (0x302, true), (0x201, false), (0x202, false), (BAR, total > rows)] {
        set_visible(&mut visibility, g.id(id), show);
    }
    if let Some(e) = g.id(BAR) {
        if let Ok(mut bar) = bars.get_mut(e) {
            (bar.value, bar.total, bar.visible) = (c.scroll, total, rows);
        }
        if let Ok(mut n) = nodes.get_mut(e) {
            n.top = Val::Px(1.0);
            n.bottom = Val::Px(2.0);
            n.height = Val::Auto;
        }
    }
    let Some(parent) = g.id(GRID) else { return };
    if let Ok(mut n) = nodes.get_mut(parent) {
        n.overflow = Overflow::clip();
    }
    let holder = crate::buyhud::holder(&mut commands, parent, &mut c.cells[k], &visibility);
    let Some(template) = ui.layout("BuyCatalogItem").cloned() else { return };
    for (n, (button, tip, picked)) in items.iter().skip(c.scroll * c.columns).take(rows * c.columns).enumerate() {
        let (x, y) = (gw.padding[0] + (n % c.columns) as f32 * step.x, gw.padding[1] + (n / c.columns) as f32 * step.y);
        let mut w = template.clone();
        w.cls = "Button".into();
        w.area = [x, y, x + step.x, y + gw.cell[1]];
        let cell = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        let Some(root) = cell.root else { continue };
        commands.entity(root).insert((*button, crate::icons::Tooltip(tip.clone())));
        if *picked {
            commands.entity(root).insert(crate::layout::Selected);
        }
        let pic = match *button {
            BuyButton::RoofStyle(r) => RoofStyle::ALL.iter().position(|x| *x == r).and_then(|i| c.roof_icons.get(i).cloned()),
            BuyButton::Roof(i) => gu.data.roofs.get(i).map(|r| r.tile).and_then(|t| {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                assets.texture(&mut ctx, t)
            }),
            // (Else a flight of it, rendered.)
            BuyButton::Stair(i) => gu.data.stair_styles.get(i).cloned().and_then(|st| st.thumbs.iter().find_map(|k| gu.icon(&mut images, &s3bake::gamedata::thumb_name(*k))).or_else(|| st.flight.map(|m| thumbs.get(&mut images, m)))),
            BuyButton::Rail(i) => gu.data.rail_styles.get(i).map(|r| r.thumbs.clone()).and_then(|t| t.iter().find_map(|k| gu.icon(&mut images, &s3bake::gamedata::thumb_name(*k)))),
            BuyButton::Blueprint(i) => gu.data.blueprints.get(i).map(|bp| c.plans.entry(i).or_insert_with(|| images.add(crate::blueprints::plan_picture(bp))).clone()),
            BuyButton::FoundationStyle(i) => gu.data.foundation_styles.get(i).map(|f| f.texture).and_then(|t| {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                assets.texture(&mut ctx, t)
            }),
            BuyButton::Fence(i) => gu.data.fences.get(i).cloned().and_then(|f| gu.icon(&mut images, &s3bake::gamedata::thumb_name(f.key.2)).or_else(|| f.straight.map(|m| thumbs.get(&mut images, m)))),
            _ => None,
        };
        if let (Some(t), Some(h)) = (cell.id(THUMB), pic) {
            crate::hudpanels::picture(&mut commands, t, h, Color::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_panel() {
        let mut buy = BuyMode::default();
        buy.active = true;
        buy.build_look = true;
        buy.show(ROOFS_TAB);
        assert_eq!(current(&buy), Some(Cat::Roofs));
        buy.show(FENCES_TAB);
        assert_eq!(current(&buy), Some(Cat::Fences));
        buy.half_walls = true;
        assert_eq!(current(&buy), Some(Cat::HalfWalls));
        buy.build_look = false;
        assert!(!showing(&buy));
    }

    #[test]
    fn roof_icons_are_drawn() {
        for r in RoofStyle::ALL {
            let img = roof_icon(r);
            let px = img.data.unwrap();
            assert!(px.chunks_exact(4).filter(|p| p[3] > 0).count() > 800, "{r:?} has a shape");
        }
    }
}
