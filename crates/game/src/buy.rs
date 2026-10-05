//! Buy mode: browse the game's catalog, place, move and sell objects on the home lot.

use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use s3bake::Key;

use crate::camera::SimsCamera;
use crate::baked::Baked;
use crate::home::spawn_game_object;
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
            );
    }
}

pub const CATEGORIES: [&str; 10] =
    ["Appliances", "Plumbing", "Beds", "Seating", "Surfaces", "Electronics", "Hobbies", "Kids", "Lighting", "Decor"];
/// Build-mode tabs after the buy categories: wallpaper and floors.
const PAINT_TABS: [&str; 2] = ["Wallpaper", "Floors"];
const PAGE: usize = 24;
/// Objects per page (thumbnail tiles).
const OBJECT_PAGE: usize = 30;

pub struct Placing {
    pub objd: Key,
    pub ghost: Entity,
    /// Moving an object already owned (no charge).
    pub owned: bool,
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
    objects: Query<(), With<GameObject>>,
) {
    if !buy.active || buy.placing.is_some() || !mouse.just_pressed(MouseButton::Left) || over_ui.0 {
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
#[derive(Component)]
enum BuyButton {
    Toggle,
    Category(usize),
    Item(Key),
    Pattern(usize),
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

/// Index of the wallpaper tab.
pub const WALLPAPER_TAB: usize = CATEGORIES.len();

fn toggle_buy(keys: Res<ButtonInput<KeyCode>>, mut buy: ResMut<BuyMode>, mut commands: Commands, mut clock: ResMut<crate::clock::GameClock>) {
    if keys.just_pressed(KeyCode::KeyB) || keys.just_pressed(KeyCode::F2) {
        let on = !buy.active;
        set_active(&mut buy, on, &mut commands, &mut clock);
    }
}

fn set_active(buy: &mut BuyMode, on: bool, commands: &mut Commands, clock: &mut crate::clock::GameClock) {
    buy.active = on;
    buy.dirty = true;
    buy.painting = None;
    if let Some(p) = buy.placing.take() {
        commands.entity(p.ghost).despawn();
    }
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
    (data, mut assets): (Res<Baked>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
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
            BackgroundColor(if buy.active { Color::srgba(0.05, 0.15, 0.30, 0.92) } else { Color::NONE }),
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
        // Wallpaper and floors: swatches of the catalogue's patterns.
        if buy.category >= CATEGORIES.len() {
            let floor = buy.category == CATEGORIES.len() + 1;
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
        let items = catalog.in_category(CATEGORIES[buy.category]);
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
) {
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
                buy.category = *c;
                buy.page = 0;
                buy.painting = None;
                buy.dirty = true;
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
                if let Some(p) = buy.placing.take() {
                    commands.entity(p.ghost).despawn();
                }
                buy.painting = Some(*i);
                buy.dirty = true;
            }
            BuyButton::Item(key) => {
                buy.painting = None;
                if let Some(p) = buy.placing.take() {
                    commands.entity(p.ghost).despawn();
                }
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let parts = assets.object(&mut ctx, *key);
                if parts.is_empty() {
                    continue;
                }
                let ghost = spawn_parts(&mut commands, &parts, Transform::from_xyz(0.0, -1000.0, 0.0));
                commands.entity(ghost).insert(DespawnOnExit(AppState::InGame));
                buy.placing = Some(Placing { objd: *key, ghost, owned: false });
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
    (mut faces, floor_meshes): (
        Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
        Query<Entity, With<crate::building::FloorMesh>>,
    ),
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
    crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces, &floor_meshes);
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

fn buy_visuals(mut q: Query<(&Interaction, &mut BackgroundColor), With<BuyButton>>) {
    for (i, mut bg) in &mut q {
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
    (world, data, catalog, building): (Res<CurrentWorld>, Res<Baked>, Res<Catalog>, Option<Res<crate::building::ActiveBuilding>>),
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    (selected, mut life): (Query<Entity, With<crate::sim::Selected>>, MessageWriter<crate::life::LifeEvent>),
    mut grid: Option<ResMut<NavGrid>>,
    pickup: Option<Res<PickupRequest>>,
    objects: Query<(&GameObject, &Transform)>,
    (bought_q, mut removed): (Query<(), With<crate::save::Bought>>, ResMut<crate::save::RemovedLotObjects>),
    mut tfs: Query<&mut Transform, Without<GameObject>>,
) {
    if !buy.active {
        return;
    }
    if keys.just_pressed(KeyCode::Comma) {
        buy.yaw += std::f32::consts::FRAC_PI_4;
    }
    if keys.just_pressed(KeyCode::Period) {
        buy.yaw -= std::f32::consts::FRAC_PI_4;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };

    if buy.placing.is_none() {
        if let Some(req) = pickup {
            commands.remove_resource::<PickupRequest>();
            if let Ok((obj, tf)) = objects.get(req.0) {
                if !bought_q.contains(req.0) {
                    crate::save::note_removed(&mut removed, obj, tf);
                }
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let parts = assets.object(&mut ctx, obj.objd);
                let ghost = spawn_parts(&mut commands, &parts, *tf);
                commands.entity(ghost).insert(DespawnOnExit(AppState::InGame));
                buy.yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
                buy.placing = Some(Placing { objd: obj.objd, ghost, owned: true });
                commands.entity(req.0).despawn();
                if let Some(g) = grid.as_mut() {
                    g.dirty = true;
                }
            }
        }
        return;
    }

    let placing = buy.placing.as_ref().unwrap();
    let ghost = placing.ghost;
    let objd = placing.objd;
    let owned = placing.owned;
    let ground = ground_hit(ray, &world);
    if let (Some(p), Ok(mut tf)) = (ground, tfs.get_mut(ghost)) {
        let snap = |v: f32| (v * 4.0).round() / 4.0;
        let (x, z) = (snap(p.x), snap(p.z));
        tf.translation = Vec3::new(x, crate::building::walk_height(&world.data, building.as_deref(), Vec3::new(x, 0.0, z)), z);
        tf.rotation = Quat::from_rotation_y(buy.yaw);
    }
    let price = catalog.by_key(&objd).map(|e| e.price).unwrap_or(0) as i64;
    if (keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace)) && owned {
        commands.entity(ghost).despawn();
        buy.placing = None;
        if let Some(h) = household.as_mut() {
            h.funds += price;
        }
        notes.push(format!("Sold for §{price}."));
        return;
    }
    if mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) {
        if owned {
            // Put it back where the ghost is.
            if let Ok(tf) = tfs.get(ghost) {
                let (pos, yaw) = (tf.translation, buy.yaw);
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                if let Some(o) = spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, objd, pos, yaw) {
                    commands.entity(o.entity).insert(crate::save::Bought);
                }
            }
        }
        commands.entity(ghost).despawn();
        buy.placing = None;
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
        let Ok(tf) = tfs.get(ghost) else { return };
        let (pos, yaw) = (tf.translation, buy.yaw);
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        if let Some(o) = spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, objd, pos, yaw) {
            commands.entity(o.entity).insert(crate::save::Bought);
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
            }
        }
    }
    let _ = ObjectKind::Other;
}
