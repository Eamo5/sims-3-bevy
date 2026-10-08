//! Buy mode: browse the game's catalog, place, move and sell objects on the home lot.

use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use s3bake::Key;

use crate::camera::SimsCamera;
use crate::baked::Baked;
use crate::hud::{PointerOverUi, ground_hit};
use crate::interact::{GameObject, Household, Notifications, ObjectKind};
use crate::loading::{Catalog, CurrentWorld};
use crate::menu::{BTN_HOVER, BTN_NORMAL, BTN_PRESS, PLUMBOB_GREEN, text};
use crate::nav::NavGrid;
use crate::objects::{AssetCtx, ObjectAssets, spawn_parts};
use crate::{AppState, PlayMode};

pub struct BuyPlugin;

impl Plugin for BuyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuyMode>()
            .add_systems(OnEnter(PlayMode::Live), |mut b: ResMut<BuyMode>| *b = BuyMode::default())
            .add_systems(
                Update,
                (toggle_buy, buy_panel, buy_buttons, buy_visuals, buy_pick, placement, paint).chain().run_if(in_state(PlayMode::Live)),
            )
            .add_systems(Update, scripted_style.run_if(in_state(PlayMode::Live)));
    }
}

pub const CATEGORIES: [&str; 12] =
    ["Appliances", "Plumbing", "Beds", "Seating", "Surfaces", "Electronics", "Hobbies", "Kids", "Lighting", "Decor", "Outdoors", "Misc"];
/// Build-mode tabs after the buy categories: wallpaper, floors, the construction tools, doors
/// and windows.
pub const PAINT_TABS: [&str; 8] = ["Wallpaper", "Floors", "Walls & Floors", "Doors", "Windows", "Roofs", "Fences", "Terrain"];
const PAGE: usize = 24;
/// Objects per page (thumbnail tiles).
const OBJECT_PAGE: usize = 30;

pub struct Placing {
    pub objd: Key,
    pub ghost: Entity,
    /// Moving an object already owned (no charge).
    pub owned: bool,
    /// The design it's in (its texture; none: as the game ships it).
    pub design: Option<Key>,
    /// What it is as an inventory item (a painting), if it is one.
    pub item: Option<crate::inventory::Stack>,
    /// Where it was picked up from (one of the lot's objects), or the Sim whose inventory it
    /// came out of: where it goes back to if it isn't put down.
    pub origin: Option<Transform>,
    pub from: Option<Entity>,
}

impl Placing {
    /// Something from the catalogue (or the lot) in hand.
    pub fn new(objd: Key, ghost: Entity, owned: bool, design: Option<Key>) -> Self {
        Self { objd, ghost, owned, design, item: None, origin: None, from: None }
    }
}

/// Something from a Sim's inventory to hold up in Buy mode: a painting to hang.
#[derive(Resource)]
pub struct HoldRequest {
    pub objd: Key,
    pub design: Option<Key>,
    pub item: crate::inventory::Stack,
    pub from: Entity,
}

#[derive(Resource, Default)]
pub struct BuyMode {
    pub active: bool,
    pub category: usize,
    pub page: usize,
    pub placing: Option<Placing>,
    pub yaw: f32,
    dirty: bool,
    /// The wallpaper or floor being painted with (index into the game data's patterns).
    pub painting: Option<usize>,
    /// The construction tool in hand.
    pub tool: Option<crate::build::BuildTool>,
    /// A roof pattern just chosen (index into the game data's roofs), to put on the house.
    pub roof_pick: Option<usize>,
    /// The fence the fence tool puts up (index into the game data's fences).
    pub fence: Option<usize>,
    /// The terrain paint in the brush (a paint layer, or `terrain_paint::ERASE`), the terrain
    /// tool (index into `terrain_paint::Sculpt::ALL`), and the brush's radius (metres; 0 for
    /// the middle size).
    pub terrain: u8,
    pub sculpt: u8,
    pub brush: f32,
    /// Things being moved that were put down unplaced (Buy mode closed with them in hand), to
    /// go back where they came from.
    returning: Vec<Placing>,
    /// Create a Style open for the object in hand, and the style it's in (if one's been made).
    styling: bool,
    style: Option<crate::style::ObjectStyle>,
}

#[derive(Component)]
struct BuyPanel;

/// An existing object the player clicked in buy mode, to be picked up next frame.
#[derive(Resource)]
struct PickupRequest(Entity);

#[allow(clippy::too_many_arguments)]
fn buy_pick(
    mut commands: Commands,
    buy: Res<BuyMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    over_ui: Res<PointerOverUi>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<SimsCamera>>,
    mut ray_cast: MeshRayCast,
    parents: Query<&ChildOf>,
    objects: Query<(), (With<GameObject>, Without<crate::visit::LotObject>)>,
) {
    if !buy.active || buy.placing.is_some() || buy.painting.is_some() || buy.tool.is_some() || !mouse.just_pressed(MouseButton::Left) || over_ui.0 {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    let root_of = |e: Entity| {
        let mut cur = e;
        for _ in 0..6 {
            if objects.contains(cur) {
                return Some(cur);
            }
            cur = parents.get(cur).ok()?.parent();
        }
        None
    };
    let filter = |e: Entity| root_of(e).is_some();
    let hits = ray_cast.cast_ray(ray, &MeshRayCastSettings::default().with_filter(&filter));
    if let Some(root) = hits.first().and_then(|(e, _)| root_of(*e)) {
        commands.insert_resource(PickupRequest(root));
    }
}
#[derive(Component, Clone, Copy, PartialEq, Debug)]
enum BuyButton {
    Toggle,
    Category(usize),
    Item(Key),
    Pattern(usize),
    Tool(crate::build::BuildTool),
    Roof(usize),
    Fence(usize),
    /// A design for the object in hand.
    Design(u8),
    /// Create a Style for the object in hand: open or close it, and a channel's colour (of the
    /// palette).
    Styling,
    StyleColour(u8, u8),
    /// A terrain paint (or the eraser), or a brush size (index into `BRUSHES`).
    Terrain(u8),
    Brush(usize),
    Sculpt(u8),
    Prev,
    Next,
}

impl BuyMode {
    /// Opens buy/build mode on a category (the paint tabs follow the buy categories).
    pub fn show(&mut self, category: usize) {
        self.active = true;
        self.category = category;
        self.page = 0;
        self.dirty = true;
    }
}

/// OBJ_STYLE=1 (tests): with an object in hand, Create a Style opened (from 12 seconds in) and
/// its first channel made red (two seconds later).
fn scripted_style(time: Res<Time>, buy: Res<BuyMode>, mut buttons: Query<(&BuyButton, &mut Interaction)>, mut step: Local<u8>) {
    if std::env::var("OBJ_STYLE").is_err() || buy.placing.is_none() {
        return;
    }
    let want = match (*step, time.elapsed_secs()) {
        (0, t) if t > 12.0 => BuyButton::Styling,
        (1, t) if t > 14.0 => BuyButton::StyleColour(0, 6),
        (2, t) if t > 20.0 => BuyButton::Styling,
        _ => return,
    };
    if let Some((_, mut i)) = buttons.iter_mut().find(|(b, _)| **b == want) {
        *i = Interaction::Pressed;
        *step += 1;
        info!("object style test: pressed {want:?}");
    }
}

/// Index of the wallpaper tab (the floors, construction, doors and windows tabs follow).
pub const WALLPAPER_TAB: usize = CATEGORIES.len();
pub const FLOORS_TAB: usize = WALLPAPER_TAB + 1;
pub const BUILD_TAB: usize = WALLPAPER_TAB + 2;
pub const DOORS_TAB: usize = WALLPAPER_TAB + 3;
pub const WINDOWS_TAB: usize = WALLPAPER_TAB + 4;
pub const ROOFS_TAB: usize = WALLPAPER_TAB + 5;
pub const FENCES_TAB: usize = WALLPAPER_TAB + 6;
pub const TERRAIN_TAB: usize = WALLPAPER_TAB + 7;

impl BuyMode {
    /// Puts down the tool, pattern or object in hand.
    pub fn drop_tools(&mut self, commands: &mut Commands) {
        self.painting = None;
        self.tool = None;
        if let Some(p) = self.placing.take() {
            commands.entity(p.ghost).despawn();
            if p.owned {
                self.returning.push(p);
            }
        }
        self.dirty = true;
    }
}

fn toggle_buy(
    keys: Res<ButtonInput<KeyCode>>,
    mut buy: ResMut<BuyMode>,
    mut commands: Commands,
    mut clock: ResMut<crate::clock::GameClock>,
    hold: Option<Res<HoldRequest>>,
) {
    if keys.just_pressed(KeyCode::KeyB) || keys.just_pressed(KeyCode::F2) {
        let on = !buy.active;
        set_active(&mut buy, on, &mut commands, &mut clock);
    }
    // Something from an inventory to put down: Buy mode, on the decorations.
    if hold.is_some() && !buy.active {
        set_active(&mut buy, true, &mut commands, &mut clock);
        buy.show(9);
    }
}

/// Puts something being moved back: in the inventory it came out of, or where it stood.
fn put_back(commands: &mut Commands, assets: &mut ObjectAssets, ctx: &mut AssetCtx, catalog: &Catalog, p: &Placing) {
    if let (Some(sim), Some(item)) = (p.from, &p.item) {
        crate::inventory::give(commands, sim, item.kind, item.key.clone(), item.name.clone(), item.quality, item.each(), item.count);
        return;
    }
    let Some(tf) = p.origin else { return };
    if let Some(o) = crate::home::spawn_game_object_design(commands, assets, ctx, catalog, p.objd, tf.translation, tf.rotation, p.design) {
        commands.entity(o.entity).insert(crate::save::Bought);
        if let Some(item) = &p.item {
            commands.entity(o.entity).insert(crate::paintings::Hung(item.clone()));
        }
    }
}

fn set_active(buy: &mut BuyMode, on: bool, commands: &mut Commands, clock: &mut crate::clock::GameClock) {
    buy.active = on;
    buy.drop_tools(commands);
    // Time stops while shopping, like the original.
    if on {
        clock.set_speed(0);
    } else if clock.speed == 0 {
        clock.set_speed(1);
    }
}

fn button(p: &mut ChildSpawnerCommands, label: String, b: BuyButton, w: Val, h: f32, highlight: bool) {
    p.spawn((
        Button,
        b,
        Node {
            border_radius: BorderRadius::all(Val::Px(8.0)),
            width: w,
            height: Val::Px(h),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(Val::Px(6.0)),
            border: UiRect::all(Val::Px(if highlight { 2.0 } else { 0.0 })),
            ..default()
        },
        BorderColor::all(PLUMBOB_GREEN),
        BackgroundColor(BTN_NORMAL),
    ))
    .with_children(|c| {
        c.spawn(text(label, 13.0, Color::WHITE));
    });
}

#[allow(clippy::too_many_arguments)]
fn buy_panel(
    mut commands: Commands,
    mut buy: ResMut<BuyMode>,
    catalog: Res<Catalog>,
    panel: Query<Entity, With<BuyPanel>>,
    mut spawned_toggle: Local<bool>,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    (data, mut assets, mut thumbs): (Res<Baked>, ResMut<ObjectAssets>, ResMut<crate::thumbs::ModelThumbs>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    swatches: Res<crate::terrain_paint::Swatches>,
    renders: Res<crate::style::StyleRenders>,
) {
    if !*spawned_toggle {
        *spawned_toggle = true;
        buy.dirty = true;
    }
    if !buy.dirty {
        return;
    }
    buy.dirty = false;
    for p in &panel {
        commands.entity(p).despawn();
    }
    let root = commands
        .spawn((
            BuyPanel,
            DespawnOnExit(AppState::InGame),
            Node {
                border_radius: BorderRadius::all(Val::Px(12.0)),
                position_type: PositionType::Absolute,
                left: Val::Px(420.0),
                right: Val::Px(180.0),
                bottom: Val::Px(100.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(if buy.active { crate::menu::PANEL_BG } else { Color::NONE }),
            Interaction::default(),
        ))
        .id();
    commands.entity(root).with_children(|p| {
        p.spawn(Node { column_gap: Val::Px(6.0), flex_wrap: FlexWrap::Wrap, row_gap: Val::Px(6.0), ..default() }).with_children(|row| {
            button(row, if buy.active { "Exit Buy Mode (B)".into() } else { "Buy Mode (B)".into() }, BuyButton::Toggle, Val::Px(150.0), 32.0, buy.active);
            if buy.active {
                for (i, c) in CATEGORIES.iter().enumerate() {
                    button(row, c.to_string(), BuyButton::Category(i), Val::Auto, 32.0, i == buy.category);
                }
                if ui.is_some() {
                    for (i, c) in PAINT_TABS.iter().enumerate() {
                        let k = CATEGORIES.len() + i;
                        button(row, c.to_string(), BuyButton::Category(k), Val::Auto, 32.0, k == buy.category);
                    }
                }
            }
        });
        if !buy.active {
            return;
        }
        // Roof patterns: one click puts it on the household's roofs.
        if buy.category == ROOFS_TAB {
            let Some(ui) = ui.as_deref() else { return };
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
                for (i, r) in ui.data.roofs.iter().enumerate() {
                    let tex = assets.texture(&mut ctx, r.tile);
                    grid.spawn((
                        Button,
                        BuyButton::Roof(i),
                        Node {
                            width: Val::Px(176.0),
                            height: Val::Px(42.0),
                            column_gap: Val::Px(6.0),
                            align_items: AlignItems::Center,
                            padding: UiRect::horizontal(Val::Px(4.0)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(BTN_NORMAL),
                        crate::icons::Tooltip(r.name.clone()),
                    ))
                    .with_children(|b| {
                        if let Some(t) = tex {
                            b.spawn((ImageNode::new(t), Node { width: Val::Px(34.0), height: Val::Px(34.0), ..default() }, Pickable::IGNORE));
                        }
                        let mut name = r.name.trim_end_matches(" Roof").to_string();
                        if name.chars().count() > 22 {
                            name = name.chars().take(20).collect::<String>() + "…";
                        }
                        b.spawn((text(name, 12.0, Color::WHITE), Pickable::IGNORE));
                    });
                }
            });
            p.spawn(text("Roofs go on the rooms you build; they show when the camera pulls back.", 13.0, Color::WHITE));
            return;
        }
        // Terrain paints: the world's own, then the eraser, and the brush's size.
        if buy.category == TERRAIN_TAB {
            let brush = if buy.brush > 0.0 { buy.brush } else { crate::terrain_paint::BRUSHES[1].0 };
            let painting = buy.tool == Some(crate::build::BuildTool::Terrain);
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
                for (i, s) in swatches.0.iter().enumerate() {
                    let picked = painting && buy.terrain == i as u8;
                    grid.spawn((
                        Button,
                        BuyButton::Terrain(i as u8),
                        Node {
                            width: Val::Px(56.0),
                            height: Val::Px(56.0),
                            border: UiRect::all(Val::Px(if picked { 3.0 } else { 1.0 })),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BorderColor::all(if picked { crate::menu::PLUMBOB_GREEN } else { Color::srgba(1.0, 1.0, 1.0, 0.3) }),
                        BackgroundColor(BTN_NORMAL),
                        crate::icons::Tooltip(format!("Terrain paint {}", i + 1)),
                    ))
                    .with_children(|b| {
                        b.spawn((ImageNode::new(s.clone()), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                    });
                }
                let erasing = painting && buy.terrain == crate::terrain_paint::ERASE;
                grid.spawn((
                    Button,
                    BuyButton::Terrain(crate::terrain_paint::ERASE),
                    Node {
                        height: Val::Px(56.0),
                        padding: UiRect::horizontal(Val::Px(10.0)),
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(if erasing { 3.0 } else { 1.0 })),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BorderColor::all(if erasing { crate::menu::PLUMBOB_GREEN } else { Color::srgba(1.0, 1.0, 1.0, 0.3) }),
                    BackgroundColor(BTN_NORMAL),
                    crate::icons::Tooltip("Takes the ground back to how it was".to_string()),
                ))
                .with_children(|b| {
                    b.spawn((text("Eraser", 13.0, Color::WHITE), Pickable::IGNORE));
                });
            });
            p.spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn(text("Terrain tools:", 13.0, Color::WHITE));
                for (i, s) in crate::terrain_paint::Sculpt::ALL.iter().enumerate() {
                    let picked = buy.tool == Some(crate::build::BuildTool::Sculpt) && buy.sculpt == i as u8;
                    row.spawn((
                        Button,
                        BuyButton::Sculpt(i as u8),
                        Node {
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            border: UiRect::all(Val::Px(if picked { 2.0 } else { 0.0 })),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BorderColor::all(crate::menu::PLUMBOB_GREEN),
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|b| {
                        b.spawn((text(s.label(), 13.0, Color::WHITE), Pickable::IGNORE));
                    });
                }
                row.spawn(text("   Brush:", 13.0, Color::WHITE));
                for (i, (r, name)) in crate::terrain_paint::BRUSHES.iter().enumerate() {
                    let picked = (brush - r).abs() < 0.01;
                    row.spawn((
                        Button,
                        BuyButton::Brush(i),
                        Node {
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            border: UiRect::all(Val::Px(if picked { 2.0 } else { 0.0 })),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BorderColor::all(crate::menu::PLUMBOB_GREEN),
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|b| {
                        b.spawn((text(name.to_string(), 13.0, Color::WHITE), Pickable::IGNORE));
                    });
                }
            });
            p.spawn(text("Pick a paint or a terrain tool, then hold the mouse down over the lot.", 13.0, Color::WHITE));
            return;
        }
        // Fences: pick one, then drag it out along the grid.
        if buy.category == FENCES_TAB {
            let Some(ui) = ui.as_deref_mut() else { return };
            let fences = ui.data.fences.clone();
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
                for (i, f) in fences.iter().enumerate() {
                    // (The game keeps no pictures of its fences: a piece of each, rendered.)
                    let thumb = ui.icon(&mut images, &s3bake::gamedata::thumb_name(f.key.2)).or_else(|| f.straight.map(|m| thumbs.get(&mut images, m)));
                    let picked = buy.tool == Some(crate::build::BuildTool::Fence) && buy.fence == Some(i);
                    grid.spawn((
                        Button,
                        BuyButton::Fence(i),
                        Node {
                            width: Val::Px(176.0),
                            height: Val::Px(48.0),
                            column_gap: Val::Px(6.0),
                            align_items: AlignItems::Center,
                            padding: UiRect::horizontal(Val::Px(4.0)),
                            border: UiRect::all(Val::Px(if picked { 2.0 } else { 0.0 })),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BorderColor::all(crate::menu::PLUMBOB_GREEN),
                        BackgroundColor(BTN_NORMAL),
                        crate::icons::Tooltip(format!("{} — §{} a section", f.name, f.price)),
                    ))
                    .with_children(|b| {
                        if let Some(t) = thumb {
                            b.spawn((ImageNode::new(t), Node { width: Val::Px(40.0), height: Val::Px(40.0), ..default() }, Pickable::IGNORE));
                        }
                        let mut name = f.name.clone();
                        if name.chars().count() > 18 {
                            name = name.chars().take(16).collect::<String>() + "…";
                        }
                        b.spawn((text(format!("{name}\n§{}", f.price), 12.0, Color::WHITE), Pickable::IGNORE));
                    });
                }
            });
            p.spawn(text(crate::build::BuildTool::Fence.help(), 13.0, Color::WHITE));
            return;
        }
        // The construction tools.
        if buy.category == BUILD_TAB {
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
                for t in crate::build::BuildTool::ALL {
                    button(grid, t.label(), BuyButton::Tool(t), Val::Px(190.0), 42.0, buy.tool == Some(t));
                }
            });
            let help = match buy.tool {
                Some(t) => t.help(),
                None => "Pick a tool. Ctrl+drag with the wall, room or floor tool takes away instead.".to_string(),
            };
            p.spawn(text(help, 13.0, Color::WHITE));
            return;
        }
        // Wallpaper and floors: swatches of the catalogue's patterns.
        if buy.category == WALLPAPER_TAB || buy.category == FLOORS_TAB {
            let floor = buy.category == FLOORS_TAB;
            let Some(ui) = ui.as_deref() else { return };
            let items: Vec<(usize, &s3bake::gamedata::PatternInfo)> = ui.data.patterns.iter().enumerate().filter(|(_, p)| p.floor == floor).collect();
            let pages = items.len().div_ceil(PAGE).max(1);
            let page = buy.page.min(pages - 1);
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
                for (i, pat) in items.iter().skip(page * PAGE).take(PAGE) {
                    let tex = assets.texture(&mut ctx, pat.texture);
                    let chosen = buy.painting == Some(*i);
                    grid.spawn((
                        Button,
                        BuyButton::Pattern(*i),
                        Node {
                            width: Val::Px(176.0),
                            height: Val::Px(42.0),
                            column_gap: Val::Px(6.0),
                            align_items: AlignItems::Center,
                            padding: UiRect::horizontal(Val::Px(4.0)),
                            border: UiRect::all(Val::Px(if chosen { 2.0 } else { 0.0 })),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BorderColor::all(PLUMBOB_GREEN),
                        BackgroundColor(BTN_NORMAL),
                        crate::icons::Tooltip(format!("{} — §{} per {}", pat.name, pat.price, if floor { "tile" } else { "wall" })),
                    ))
                    .with_children(|b| {
                        if let Some(t) = tex {
                            let h = if floor { 34.0 } else { 36.0 };
                            b.spawn((ImageNode::new(t), Node { width: Val::Px(if floor { 34.0 } else { 18.0 }), height: Val::Px(h), ..default() }, Pickable::IGNORE));
                        }
                        let mut name = pat.name.clone();
                        if name.chars().count() > 22 {
                            name = name.chars().take(20).collect::<String>() + "…";
                        }
                        b.spawn((text(format!("{name}\n§{}", pat.price), 12.0, Color::WHITE), Pickable::IGNORE));
                    });
                }
            });
            p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                button(row, "< Prev".into(), BuyButton::Prev, Val::Px(80.0), 28.0, false);
                let how = if floor { "click a floor to cover the room" } else { "click a wall to paper that side" };
                row.spawn(text(format!("Page {} / {} · {} patterns · {how} · right-click to stop", page + 1, pages, items.len()), 13.0, Color::WHITE));
                button(row, "Next >".into(), BuyButton::Next, Val::Px(80.0), 28.0, false);
            });
            return;
        }
        // The object in hand's designs (the game's colour and pattern presets for it).
        if let Some(pl) = &buy.placing {
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            let n = ObjectAssets::design_count(&ctx, pl.objd);
            // (A row for its designs, or for Create a Style when it has colours to change.)
            let restylable = data.0.designs.get(&pl.objd).is_some_and(|d| d.channels.iter().any(|c| !c.is_empty()));
            if n > 1 || restylable {
                let name = catalog.by_key(&pl.objd).map(|e| e.name.clone()).unwrap_or_default();
                p.spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    row.spawn(text(format!("{name} · Design"), 14.0, Color::WHITE));
                    // (Create a Style, where its design has colour channels to change.)
                    let design = buy.style.as_ref().filter(|s| s.objd == pl.objd).map(|s| s.design).or_else(|| pl.design.filter(|k| k.0 == s3bake::gamedata::T_DESIGN).map(|k| k.1 as u8)).unwrap_or(0);
                    if data.0.designs.get(&pl.objd).is_some_and(|d| d.channels.get(design as usize).is_some_and(|c| !c.is_empty())) {
                        button(row, if buy.styling { "Close Create a Style".into() } else { "Create a Style".into() }, BuyButton::Styling, Val::Px(150.0), 28.0, buy.styling);
                    }
                    for d in 0..n {
                        let tex = assets.texture(&mut ctx, crate::objects::design_texture(pl.objd, d));
                        let chosen = pl.design == Some(crate::objects::design_texture(pl.objd, d));
                        row.spawn((
                            Button,
                            BuyButton::Design(d),
                            Node {
                                width: Val::Px(40.0),
                                height: Val::Px(40.0),
                                border: UiRect::all(Val::Px(if chosen { 3.0 } else { 1.0 })),
                                border_radius: BorderRadius::all(Val::Px(6.0)),
                                ..default()
                            },
                            BorderColor::all(if chosen { PLUMBOB_GREEN } else { Color::WHITE }),
                            BackgroundColor(BTN_NORMAL),
                            crate::icons::Tooltip(format!("Design {}", d + 1)),
                        ))
                        .with_children(|b| {
                            if let Some(t) = tex {
                                b.spawn((ImageNode::new(t), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                            }
                        });
                    }
                });
            }
            // Create a Style: each of the design's colour channels, any colour of the palette.
            let design = buy.style.as_ref().filter(|s| s.objd == pl.objd).map(|s| s.design).or_else(|| pl.design.filter(|k| k.0 == s3bake::gamedata::T_DESIGN).map(|k| k.1 as u8)).unwrap_or(0);
            let channels = data.0.designs.get(&pl.objd).and_then(|d| d.channels.get(design as usize).cloned()).unwrap_or_default();
            if buy.styling && !channels.is_empty() {
                if renders.busy() {
                    p.spawn(text("Restyling…", 13.0, Color::srgb(1.0, 0.9, 0.5)));
                }
                for (ch, own) in channels {
                    let now = buy.style.as_ref().filter(|s| s.objd == pl.objd && s.design == design).and_then(|s| s.colours.iter().find(|c| c.0 == ch)).map_or(own, |c| c.1);
                    p.spawn(Node { column_gap: Val::Px(3.0), align_items: AlignItems::Center, flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|row| {
                        row.spawn((text(format!("Color {}", (b'A' + ch) as char), 13.0, Color::WHITE), Node { width: Val::Px(58.0), ..default() }));
                        row.spawn((
                            Node { width: Val::Px(24.0), height: Val::Px(24.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(4.0)), margin: UiRect::right(Val::Px(8.0)), ..default() },
                            BorderColor::all(Color::WHITE),
                            BackgroundColor(Color::srgb(now[0], now[1], now[2])),
                        ));
                        for (i, [r, g, b]) in crate::style::PALETTE.iter().enumerate() {
                            row.spawn((
                                Button,
                                BuyButton::StyleColour(ch, i as u8),
                                Node { width: Val::Px(20.0), height: Val::Px(20.0), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() },
                                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.5)),
                                BackgroundColor(Color::srgb(*r, *g, *b)),
                            ));
                        }
                    });
                }
            }
        }
        let items = match buy.category {
            DOORS_TAB => catalog.openings(true),
            WINDOWS_TAB => catalog.openings(false),
            c => catalog.in_category(CATEGORIES[c.min(CATEGORIES.len() - 1)]),
        };
        let pages = items.len().div_ceil(OBJECT_PAGE).max(1);
        let page = buy.page.min(pages - 1);
        p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
            for item in items.iter().skip(page * OBJECT_PAGE).take(OBJECT_PAGE) {
                // The game's catalogue picture, with the price under it (the name on hover).
                let thumb = ui.as_deref_mut().and_then(|ui| ui.icon(&mut images, &s3bake::gamedata::thumb_name(item.key.2)));
                let Some(thumb) = thumb else {
                    let mut name = item.name.clone();
                    if name.chars().count() > 12 {
                        name = name.chars().take(11).collect::<String>() + "…";
                    }
                    button(grid, format!("{name}\n§{}", item.price), BuyButton::Item(item.key), Val::Px(84.0), 96.0, false);
                    continue;
                };
                grid.spawn((
                    Button,
                    BuyButton::Item(item.key),
                    Node {
                        width: Val::Px(84.0),
                        height: Val::Px(96.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                    crate::icons::Tooltip(format!("{} — §{}", item.name, item.price)),
                ))
                .with_children(|b| {
                    b.spawn((ImageNode::new(thumb), Node { width: Val::Px(72.0), height: Val::Px(72.0), ..default() }, Pickable::IGNORE));
                    b.spawn((text(format!("§{}", item.price), 13.0, Color::WHITE), Pickable::IGNORE));
                });
            }
        });
        p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
            button(row, "< Prev".into(), BuyButton::Prev, Val::Px(80.0), 28.0, false);
            row.spawn(text(format!("Page {} / {} · {} items · , . rotate · right-click cancel · Del sells held object", page + 1, pages, items.len()), 13.0, Color::WHITE));
            button(row, "Next >".into(), BuyButton::Next, Val::Px(80.0), 28.0, false);
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn buy_buttons(
    mut commands: Commands,
    q: Query<(&Interaction, &BuyButton), Changed<Interaction>>,
    mut buy: ResMut<BuyMode>,
    mut clock: ResMut<crate::clock::GameClock>,
    data: Res<Baked>,
    mut assets: ResMut<ObjectAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    ghost_tf: Query<&Transform>,
    (mut renders, install, mut styled): (ResMut<crate::style::StyleRenders>, Option<Res<crate::data::InstallPath>>, MessageReader<crate::style::ObjectStyleReady>),
) {
    // A style made in Create a Style, rendered: the object in hand in it.
    for r in styled.read() {
        let Some(p) = buy.placing.as_mut().filter(|p| p.objd == r.0.objd) else { continue };
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let design = r.0.texture();
        let parts = assets.object_design(&mut ctx, p.objd, Some(design));
        let at = ghost_tf.get(p.ghost).copied().unwrap_or_default();
        commands.entity(p.ghost).despawn();
        p.ghost = spawn_parts(&mut commands, &parts, at);
        commands.entity(p.ghost).insert(DespawnOnExit(AppState::InGame));
        p.design = Some(design);
        buy.style = Some(r.0.clone());
        buy.dirty = true;
    }
    for (i, b) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        match b {
            BuyButton::Toggle => {
                let on = !buy.active;
                set_active(&mut buy, on, &mut commands, &mut clock);
            }
            BuyButton::Category(c) => {
                buy.drop_tools(&mut commands);
                buy.category = *c;
                buy.page = 0;
            }
            BuyButton::Roof(i) => {
                buy.roof_pick = Some(*i);
            }
            BuyButton::Styling => {
                buy.styling = !buy.styling;
                buy.dirty = true;
            }
            BuyButton::StyleColour(ch, idx) => {
                let (Some(p), Some(&colour), Some(path)) = (buy.placing.as_ref(), crate::style::PALETTE.get(*idx as usize), install.as_ref()) else { continue };
                let Some(info) = data.0.designs.get(&p.objd) else { continue };
                let design = buy.style.as_ref().filter(|s| s.objd == p.objd).map(|s| s.design).or_else(|| p.design.filter(|k| k.0 == s3bake::gamedata::T_DESIGN).map(|k| k.1 as u8)).unwrap_or(0);
                // (On top of the style already made from this design, if any.)
                let mut colours = buy.style.as_ref().filter(|s| s.objd == p.objd && s.design == design).map(|s| s.colours.clone()).unwrap_or_default();
                colours.retain(|(c, _)| c != ch);
                colours.push((*ch, colour));
                colours.sort_by_key(|c| c.0);
                renders.request_object(crate::style::ObjectStyle { objd: p.objd, design, colours }, info.size, path.0.clone());
                buy.dirty = true;
            }
            BuyButton::Design(d) => {
                // The object in hand, in that design.
                let Some(p) = buy.placing.as_mut() else { continue };
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let design = crate::objects::design_texture(p.objd, *d);
                let parts = assets.object_design(&mut ctx, p.objd, Some(design));
                let at = ghost_tf.get(p.ghost).copied().unwrap_or_default();
                commands.entity(p.ghost).despawn();
                p.ghost = spawn_parts(&mut commands, &parts, at);
                commands.entity(p.ghost).insert(DespawnOnExit(AppState::InGame));
                p.design = Some(design);
                buy.style = None;
                buy.dirty = true;
                play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
            }
            BuyButton::Fence(i) => {
                buy.drop_tools(&mut commands);
                buy.tool = Some(crate::build::BuildTool::Fence);
                buy.fence = Some(*i);
                buy.dirty = true;
                play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
            }
            BuyButton::Terrain(l) => {
                buy.drop_tools(&mut commands);
                buy.tool = Some(crate::build::BuildTool::Terrain);
                buy.terrain = *l;
                buy.dirty = true;
                play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
            }
            BuyButton::Brush(i) => {
                buy.brush = crate::terrain_paint::BRUSHES[*i].0;
                buy.dirty = true;
            }
            BuyButton::Sculpt(i) => {
                buy.drop_tools(&mut commands);
                buy.tool = Some(crate::build::BuildTool::Sculpt);
                buy.sculpt = *i;
                buy.dirty = true;
                play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
            }
            BuyButton::Tool(t) => {
                buy.drop_tools(&mut commands);
                buy.tool = Some(*t);
                play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
            }
            BuyButton::Prev => {
                buy.page = buy.page.saturating_sub(1);
                buy.dirty = true;
            }
            BuyButton::Next => {
                buy.page += 1;
                buy.dirty = true;
            }
            BuyButton::Pattern(i) => {
                buy.drop_tools(&mut commands);
                buy.painting = Some(*i);
            }
            BuyButton::Item(key) => {
                buy.drop_tools(&mut commands);
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let parts = assets.object(&mut ctx, *key);
                if parts.is_empty() {
                    continue;
                }
                let ghost = spawn_parts(&mut commands, &parts, Transform::from_xyz(0.0, -1000.0, 0.0));
                commands.entity(ghost).insert(DespawnOnExit(AppState::InGame));
                buy.placing = Some(Placing::new(*key, ghost, false, None));
                buy.dirty = true;
            }
        }
    }
}

/// Painting: a click on a wall papers that side; a click on a floor covers its room.
#[allow(clippy::too_many_arguments)]
fn paint(
    mut commands: Commands,
    mut buy: ResMut<BuyMode>,
    (keys, mouse, over_ui): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, Res<PointerOverUi>),
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    (world, data, ui): (Res<CurrentWorld>, Res<Baked>, Option<Res<crate::icons::GameUi>>),
    mut building: Option<ResMut<crate::building::ActiveBuilding>>,
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut faces: Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
    (mut household, mut notes, mut log, mut play): (
        Option<ResMut<Household>>,
        ResMut<Notifications>,
        Option<ResMut<crate::building::LotPaint>>,
        MessageWriter<crate::sound::PlaySound>,
    ),
) {
    let Some(i) = buy.painting.filter(|_| buy.active) else { return };
    if mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) {
        buy.painting = None;
        buy.dirty = true;
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) || over_ui.0 {
        return;
    }
    let (Some(ui), Some(b)) = (ui, building.as_deref_mut()) else { return };
    let Some(pat) = ui.data.patterns.get(i).cloned() else { return };
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    let mut ops = Vec::new();
    if pat.floor {
        // The room under the pointer (a single tile outdoors).
        let Some((p, level)) = crate::hud::floor_hit(ray, &world, Some(b)) else { return };
        let l = b.local(p);
        let (x, z) = (l.x.floor(), l.y.floor());
        if x < 0.0 || z < 0.0 {
            return;
        }
        let Some(tile) = b.data.floors.iter().find(|f| f.level == level && f.x == x as u16 && f.z == z as u16).copied() else { return };
        for f in b.data.floors.iter().filter(|f| f.level == tile.level && if tile.region == 0 { f.x == tile.x && f.z == tile.z } else { f.region == tile.region }) {
            ops.push(crate::building::PaintOp::Floor { level: f.level, x: f.x, z: f.z, texture: pat.texture });
        }
    } else {
        let Some((wall, side)) = pick_wall(ray, b) else { return };
        ops.push(crate::building::PaintOp::Wall { wall, side, texture: pat.texture });
    }
    if ops.is_empty() {
        return;
    }
    let cost = pat.price as i64 * ops.len() as i64;
    if household.as_ref().is_some_and(|h| h.funds < cost) {
        notes.push("You can't afford that.");
        return;
    }
    if let Some(h) = household.as_mut() {
        h.funds -= cost;
    }
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
    match log.as_mut() {
        Some(l) => l.0.extend(ops),
        None => commands.insert_resource(crate::building::LotPaint(ops)),
    }
    play.write(crate::sound::PlaySound::ui(if pat.floor { "ui_build_flooring_plop" } else { "ui_build_wallcovering_plop" }));
}

/// The nearest wall the ray meets on the floors in view, and which side faces the ray (0 = the
/// wall's left / +normal side).
fn pick_wall(ray: Ray3d, b: &crate::building::ActiveBuilding) -> Option<(u32, u8)> {
    let mut best: Option<(f32, u32, u8)> = None;
    for (i, w) in b.data.walls.iter().enumerate() {
        let level = w.level.max(1);
        if level > b.view_level {
            continue;
        }
        let y0 = b.levels.get(level as usize).copied().unwrap_or(0.0);
        let (la, lb) = (Vec2::from(w.a), Vec2::from(w.b));
        let len = (lb - la).length();
        if len < 1e-3 {
            continue;
        }
        let along = (lb - la) / len;
        let n = b.rot * Vec3::new(-along.y, 0.0, along.x);
        let a = b.world(la.x, la.y, y0);
        let c = b.world(lb.x, lb.y, y0);
        let denom = ray.direction.dot(n);
        if denom.abs() < 1e-4 {
            continue;
        }
        let t = (a - ray.origin).dot(n) / denom;
        if t <= 0.0 || best.is_some_and(|b| t >= b.0) {
            continue;
        }
        let p = ray.origin + *ray.direction * t;
        let dir = (c - a).with_y(0.0);
        let s = (p - a).with_y(0.0).dot(dir) / dir.length_squared();
        if !(0.0..=1.0).contains(&s) || p.y < y0 || p.y > y0 + 3.0 {
            continue;
        }
        let side = if (ray.origin - a).dot(n) > 0.0 { 0 } else { 1 };
        best = Some((t, i as u32, side));
    }
    best.map(|(_, w, s)| (w, s))
}

fn buy_visuals(mut q: Query<(&Interaction, &mut BackgroundColor, &BuyButton)>) {
    for (i, mut bg, b) in &mut q {
        // (The palette's swatches keep their colours.)
        if matches!(b, BuyButton::StyleColour(..)) {
            continue;
        }
        bg.0 = match i {
            Interaction::Pressed => BTN_PRESS,
            Interaction::Hovered => BTN_HOVER,
            Interaction::None => BTN_NORMAL,
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn placement(
    mut commands: Commands,
    mut buy: ResMut<BuyMode>,
    (keys, mouse): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    over_ui: Res<PointerOverUi>,
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<SimsCamera>>),
    (world, data, catalog, mut building): (Res<CurrentWorld>, Res<Baked>, Res<Catalog>, Option<ResMut<crate::building::ActiveBuilding>>),
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut household, mut notes, mut log, mut play): (
        Option<ResMut<Household>>,
        ResMut<Notifications>,
        Option<ResMut<crate::building::LotPaint>>,
        MessageWriter<crate::sound::PlaySound>,
    ),
    (selected, mut life): (Query<Entity, With<crate::sim::Selected>>, MessageWriter<crate::life::LifeEvent>),
    mut grid: Option<ResMut<NavGrid>>,
    (pickup, hold): (Option<Res<PickupRequest>>, Option<Res<HoldRequest>>),
    objects: Query<(&GameObject, &Transform, Option<&crate::objects::Design>, Option<&crate::paintings::Hung>)>,
    (bought_q, mut removed): (Query<(), With<crate::save::Bought>>, ResMut<crate::save::RemovedLotObjects>),
    mut tfs: Query<&mut Transform, Without<GameObject>>,
    mut faces: Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    // What was being moved when Buy mode closed goes back.
    for p in std::mem::take(&mut buy.returning) {
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        put_back(&mut commands, &mut assets, &mut ctx, &catalog, &p);
        if let Some(g) = grid.as_mut() {
            g.dirty = true;
        }
    }
    if !buy.active {
        return;
    }
    // (Something else in hand when a painting's to be hung: it's put down first.)
    if hold.is_some() && buy.placing.is_some() {
        buy.drop_tools(&mut commands);
        return;
    }
    if keys.just_pressed(KeyCode::Comma) {
        buy.yaw += std::f32::consts::FRAC_PI_4;
    }
    if keys.just_pressed(KeyCode::Period) {
        buy.yaw -= std::f32::consts::FRAC_PI_4;
    }
    if buy.placing.is_none() {
        // A painting from an inventory, held up to the walls.
        if let Some(h) = hold {
            commands.remove_resource::<HoldRequest>();
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            let parts = assets.object_design(&mut ctx, h.objd, h.design);
            let ghost = spawn_parts(&mut commands, &parts, Transform::from_xyz(0.0, -1000.0, 0.0));
            commands.entity(ghost).insert(DespawnOnExit(AppState::InGame));
            buy.placing = Some(Placing { item: Some(h.item.clone()), from: Some(h.from), ..Placing::new(h.objd, ghost, true, h.design) });
            buy.dirty = true;
            return;
        }
        if let Some(req) = pickup {
            commands.remove_resource::<PickupRequest>();
            if let Ok((obj, tf, design, hung)) = objects.get(req.0) {
                if !bought_q.contains(req.0) {
                    crate::save::note_removed(&mut removed, obj, tf);
                }
                let design = design.map(|d| d.0);
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let parts = assets.object_design(&mut ctx, obj.objd, design);
                let ghost = spawn_parts(&mut commands, &parts, *tf);
                commands.entity(ghost).insert(DespawnOnExit(AppState::InGame));
                buy.yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
                buy.placing = Some(Placing { item: hung.map(|h| h.0.clone()), origin: Some(*tf), ..Placing::new(obj.objd, ghost, true, design) });
                buy.dirty = true;
                commands.entity(req.0).despawn();
                if let Some(g) = grid.as_mut() {
                    g.dirty = true;
                }
            }
        }
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };

    let placing = buy.placing.as_ref().unwrap();
    let ghost = placing.ghost;
    let objd = placing.objd;
    let owned = placing.owned;
    let design = placing.design;
    let ground = ground_hit(ray, &world);
    // Doors and windows go into the wall under the pointer; paintings, mirrors and wall lamps
    // hang on it (their models hang behind their middle, on the tile's wall, off the floor).
    let opening = catalog.by_key(&objd).and_then(|e| e.opening);
    let bounds = {
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        crate::objects::parts_bounds(&assets.object(&mut ctx, objd))
    };
    let wall_hung = opening.is_none() && bounds.is_some_and(crate::building::hangs_on_wall);
    let in_wall = match (opening.is_some() || wall_hung, building.as_deref()) {
        (true, Some(b)) => {
            let tiles = bounds.map_or(1, |(mn, mx)| ((mx.x - mn.x).round() as u32).max(1));
            crate::build::snap_to_wall(b, ray, tiles).map(|(p, r, ops)| (p, r, if wall_hung { Vec::new() } else { ops }))
        }
        _ => None,
    };
    // Pool ladders go on a pool's edge.
    let ladder = catalog.by_key(&objd).is_some_and(|e| e.kind == ObjectKind::PoolLadder);
    let on_edge = match (ladder, building.as_deref(), ground) {
        (true, Some(b), Some(p)) => crate::build::snap_to_pool(b, p),
        _ => None,
    };
    if let Ok(mut tf) = tfs.get_mut(ghost) {
        if let Some((pos, rot, _)) = &in_wall {
            tf.translation = *pos;
            tf.rotation = *rot;
        } else if let Some((pos, rot)) = on_edge {
            tf.translation = pos;
            tf.rotation = rot;
        } else if let Some(p) = ground {
            let snap = |v: f32| (v * 4.0).round() / 4.0;
            let (x, z) = (snap(p.x), snap(p.z));
            tf.translation = Vec3::new(x, crate::building::walk_height(&world.data, building.as_deref(), Vec3::new(x, 0.0, z)), z);
            tf.rotation = Quat::from_rotation_y(buy.yaw);
        }
    }
    let price = catalog.by_key(&objd).map(|e| e.price).unwrap_or(0) as i64;
    // (A painting sells for what it's worth.)
    let price = placing.item.as_ref().map_or(price, |i| i.worth);
    if (keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace)) && owned {
        commands.entity(ghost).despawn();
        buy.placing = None;
        buy.dirty = true;
        if let Some(h) = household.as_mut() {
            h.funds += price;
        }
        notes.push(format!("Sold for §{price}."));
        return;
    }
    if mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) {
        // Put it back where it was (or in the inventory it came from).
        if let Some(p) = buy.placing.as_ref().filter(|p| p.owned) {
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            put_back(&mut commands, &mut assets, &mut ctx, &catalog, p);
        }
        commands.entity(ghost).despawn();
        buy.placing = None;
        buy.dirty = true;
        if let Some(g) = grid.as_mut() {
            g.dirty = true;
        }
        return;
    }
    if mouse.just_pressed(MouseButton::Left) && !over_ui.0 && ground.is_some() {
        let funds = household.as_ref().map(|h| h.funds).unwrap_or(0);
        if !owned && funds < price {
            notes.push("You can't afford that.");
            return;
        }
        if opening.is_some() && in_wall.is_none() {
            notes.push("Doors and windows go into a straight wall, on the floor in view.");
            return;
        }
        if wall_hung && in_wall.is_none() {
            notes.push("Paintings, mirrors and wall lamps go on a straight wall, on the floor in view.");
            return;
        }
        // (Not over a window or a door.)
        if wall_hung
            && let (Some((p, r, _)), Some(b)) = (&in_wall, building.as_deref())
            && b.opening_behind(*p, *r, bounds.map_or(1, |(mn, mx)| ((mx.x - mn.x).round() as u32).max(1)))
        {
            notes.push("There's a window or door in the way.");
            return;
        }
        if ladder && on_edge.is_none() {
            notes.push("Pool ladders go on the edge of a pool.");
            return;
        }
        let Ok(tf) = tfs.get(ghost) else { return };
        let (pos, rot) = (tf.translation, tf.rotation);
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        // A door needs its own wall sections: cut them free of any longer wall first.
        if let (Some((_, _, ops)), Some(b)) = (in_wall, building.as_deref_mut())
            && !ops.is_empty()
        {
            crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
            match log.as_mut() {
                Some(l) => l.0.extend(ops),
                None => commands.insert_resource(crate::building::LotPaint(ops)),
            }
        }
        if opening.is_some() {
            play.write(crate::sound::PlaySound::ui("ui_build_door_plop"));
        }
        if let Some(o) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, objd, pos, rot, design) {
            commands.entity(o.entity).insert(crate::save::Bought);
            if let Some(item) = buy.placing.as_ref().and_then(|p| p.item.clone()) {
                commands.entity(o.entity).insert(crate::paintings::Hung(item));
            }
            if !owned && let Some(h) = household.as_mut() {
                h.funds -= price;
                if let Ok(s) = selected.single() {
                    life.write(crate::life::LifeEvent::new(s, crate::life::LifeEventKind::Bought { price: price as i32 }));
                }
            }
            if let Some(g) = grid.as_mut() {
                g.dirty = true;
            }
            if owned {
                commands.entity(ghost).despawn();
                buy.placing = None;
                buy.dirty = true;
            }
        }
    }
    let _ = ObjectKind::Other;
}
