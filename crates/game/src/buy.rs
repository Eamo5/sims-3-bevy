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
                (toggle_buy, buy_panel, buy_buttons, buy_visuals, buy_pick, placement).chain().run_if(in_state(PlayMode::Live)),
            );
    }
}

pub const CATEGORIES: [&str; 9] =
    ["Appliances", "Plumbing", "Beds", "Seating", "Surfaces", "Electronics", "Hobbies", "Lighting", "Decor"];
const PAGE: usize = 24;

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
    Prev,
    Next,
}

fn toggle_buy(keys: Res<ButtonInput<KeyCode>>, mut buy: ResMut<BuyMode>, mut commands: Commands, mut clock: ResMut<crate::clock::GameClock>) {
    if keys.just_pressed(KeyCode::KeyB) || keys.just_pressed(KeyCode::F2) {
        let on = !buy.active;
        set_active(&mut buy, on, &mut commands, &mut clock);
    }
}

fn set_active(buy: &mut BuyMode, on: bool, commands: &mut Commands, clock: &mut crate::clock::GameClock) {
    buy.active = on;
    buy.dirty = true;
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

fn buy_panel(
    mut commands: Commands,
    mut buy: ResMut<BuyMode>,
    catalog: Res<Catalog>,
    panel: Query<Entity, With<BuyPanel>>,
    mut spawned_toggle: Local<bool>,
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
            }
        });
        if !buy.active {
            return;
        }
        let items = catalog.in_category(CATEGORIES[buy.category]);
        let pages = items.len().div_ceil(PAGE).max(1);
        let page = buy.page.min(pages - 1);
        p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
            for item in items.iter().skip(page * PAGE).take(PAGE) {
                let mut name = item.name.clone();
                if name.chars().count() > 28 {
                    name = name.chars().take(26).collect::<String>() + "…";
                }
                button(grid, format!("{name}\n§{}", item.price), BuyButton::Item(item.key), Val::Px(176.0), 42.0, false);
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
            BuyButton::Item(key) => {
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
