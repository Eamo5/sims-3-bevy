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
            .init_resource::<crate::buyhistory::BuyHistory>()
            .add_systems(OnEnter(PlayMode::Live), reset_buy_mode)
            .add_systems(PreUpdate, toggle_buy.after(bevy::input::InputSystems).run_if(in_state(PlayMode::Live)))
            .add_systems(
                Update,
                (buy_panel, buy_buttons, buy_visuals, buy_pick, eyedrop, placement, paint, |mut buy: ResMut<BuyMode>| {
                    // (The eyedropper's click is spent by the end of its frame.)
                    if buy.eyedropped {
                        buy.eyedropped = false;
                    }
                })
                    .chain()
                    .before(crate::buyhistory::update)
                    .after(crate::hud::pointer_over_ui)
                    .run_if(in_state(PlayMode::Live)),
            )
            .add_systems(Update, lot_grid.run_if(in_state(PlayMode::Live)))
            .add_systems(Update, crate::buyhistory::update.run_if(in_state(PlayMode::Live)))
            .add_systems(Update, crate::buyhistory::settle_purchases.after(crate::buyhistory::update).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (scripted_style, scripted_cover, scripted_eyedrop).run_if(in_state(PlayMode::Live)));
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

pub(crate) fn reset_buy_mode(mut buy: ResMut<BuyMode>) {
    *buy = BuyMode::default();
}

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
    /// A piece of the lot's furniture stays alive while it is moved. Keeping its entity
    /// preserves upgrades, breakage, contents, animated parts and Sims' references to it.
    source: Option<(Entity, Option<crate::nav::Obstacle>)>,
    /// The design tool previews at the original location and commits without a placement click.
    pub edit_in_place: bool,
}

impl Placing {
    /// Something from the catalogue (or the lot) in hand.
    pub fn new(objd: Key, ghost: Entity, owned: bool, design: Option<Key>) -> Self {
        Self { objd, ghost, owned, design, item: None, origin: None, from: None, source: None, edit_in_place: false }
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
    /// The game's own buy catalogue is showing (`buyhud`): this panel keeps only the object
    /// in hand's designs.
    pub game_look: bool,
    pub build_look: bool,
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
    pending_style: Option<crate::style::ObjectStyle>,
    /// What the wallpaper or floor being painted with is painted in: one of its swatches, or a
    /// style made for it (none: its first swatch).
    cover: Option<Key>,
    cover_style: Option<crate::style::CoverStyle>,
    pending_cover: Option<crate::style::CoverStyle>,
    /// The eyedropper in hand: the next object, wall or floor clicked is taken up again (a new
    /// one of the object in its design, or the covering to paint with).
    pub eyedropper: bool,
    /// The click that took something up with the eyedropper (not to put it down too).
    eyedropped: bool,
    /// The sledgehammer in hand: what's clicked is sold.
    pub selling: bool,
    /// The live-mode speed to return to, including a game already paused by the player.
    resume_speed: Option<usize>,
    /// A lighting preview only: the simulation's date and time stay where they are.
    pub preview_hour: Option<f32>,
    /// The grid is shown in Buy/Build mode until the player hides it.
    pub hide_grid: bool,
    /// Waiting for an existing object to edit with Create a Style.
    pub design_tool: bool,
    apply_design: bool,
}

#[derive(Component)]
struct BuyPanel;

/// Furniture hidden while its preview is in hand. Building visibility leaves it hidden.
#[derive(Component)]
pub struct HeldObject;

fn hold_source(commands: &mut Commands, source: Entity) {
    crate::buyhistory::remember_pickup(commands, source);
    commands.entity(source).insert((HeldObject, Visibility::Hidden)).remove::<crate::nav::Obstacle>();
}

/// Resumes the same object, with every gameplay component and child still attached.
fn restore_source(commands: &mut Commands, p: &Placing) -> Option<Entity> {
    let (e, obstacle) = p.source?;
    let mut entity = commands.entity(e);
    entity.remove::<HeldObject>().insert(Visibility::Inherited);
    if let Some(obstacle) = obstacle {
        entity.insert(obstacle);
    }
    Some(e)
}

/// An existing object the player clicked in buy mode, to be picked up next frame.
#[derive(Resource)]
pub(crate) struct PickupRequest(pub Entity);

/// An object clicked with the eyedropper: a new one of it, in its design, taken up next frame.
#[derive(Resource)]
struct EyedropRequest(Entity);

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
        if buy.eyedropper {
            commands.insert_resource(EyedropRequest(root));
        } else {
            commands.insert_resource(PickupRequest(root));
        }
    } else if buy.eyedropper {
        commands.insert_resource(EyedropCover(ray));
    }
}

/// A wall or floor clicked with the eyedropper (the ray through the pointer), for its covering.
#[derive(Resource)]
struct EyedropCover(Ray3d);

/// The eyedropper's catch: a new one of the object clicked, in its design, in hand (bought when
/// it's put down); or the covering of the wall side or floor clicked, to paint with.
#[allow(clippy::too_many_arguments)]
fn eyedrop(
    mut commands: Commands,
    mut buy: ResMut<BuyMode>,
    (object, cover): (Option<Res<EyedropRequest>>, Option<Res<EyedropCover>>),
    objects: Query<(&GameObject, Option<&crate::objects::Design>)>,
    (data, catalog, ui, world, building): (Res<Baked>, Res<Catalog>, Option<Res<crate::icons::GameUi>>, Res<CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>),
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut notes, mut play): (ResMut<Notifications>, MessageWriter<crate::sound::PlaySound>),
) {
    if let Some(req) = object {
        commands.remove_resource::<EyedropRequest>();
        let Ok((obj, design)) = objects.get(req.0) else { return };
        if catalog.by_key(&obj.objd).is_none_or(|e| e.price <= 0) {
            notes.push(format!("The {} isn't for sale.", obj.name));
            return;
        }
        let design = design.map(|d| d.0);
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let parts = assets.object_design(&mut ctx, obj.objd, design);
        let ghost = spawn_parts(&mut commands, &parts, Transform::from_xyz(0.0, -1000.0, 0.0));
        commands.entity(ghost).insert(DespawnOnExit(AppState::InGame));
        buy.eyedropper = false;
        buy.eyedropped = true;
        buy.placing = Some(Placing::new(obj.objd, ghost, false, design));
        buy.dirty = true;
        play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
        return;
    }
    let Some(req) = cover else { return };
    commands.remove_resource::<EyedropCover>();
    let (Some(b), Some(ui)) = (building.as_deref(), ui) else { return };
    let ray = req.0;
    let cover_key = |i: u16| (i != s3bake::types::NO_COVER).then(|| b.data.covers.get(i as usize).copied()).flatten();
    // (A wall side under the pointer, else the floor.)
    let found = pick_wall(ray, b)
        .and_then(|(w, side)| b.data.walls.get(w as usize).and_then(|w| cover_key(w.cover[side as usize])).map(|k| (k, false)))
        .or_else(|| {
            let (p, level) = crate::hud::floor_hit(ray, &world, Some(b))?;
            let l = b.local(p);
            let tile = b.data.floors.iter().find(|f| f.level == level && f.x as f32 == l.x.floor() && f.z as f32 == l.y.floor())?;
            cover_key(tile.cover[0]).map(|k| (k, true))
        });
    let Some((key, floor)) = found else {
        notes.push("Nothing to take up there.");
        return;
    };
    // (Painted at the price of the catalogue pattern it is, or else the cheapest of its kind.)
    let Some(i) = ui
        .data
        .patterns
        .iter()
        .position(|p| p.floor == floor && (p.texture == key || p.swatches.contains(&key)))
        .or_else(|| ui.data.patterns.iter().enumerate().filter(|(_, p)| p.floor == floor).min_by_key(|(_, p)| p.price).map(|(i, _)| i))
    else {
        return;
    };
    buy.eyedropper = false;
    buy.eyedropped = true;
    buy.painting = Some(i);
    buy.cover = Some(key);
    buy.category = if floor { FLOORS_TAB } else { WALLPAPER_TAB };
    buy.dirty = true;
    play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
}
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub enum BuyButton {
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
    ApplyDesign,
    CancelDesign,
    StyleColour(u8, u8),
    /// A swatch of the wallpaper or floor in hand.
    CoverSwatch(u8),
    /// The eyedropper.
    Eyedropper,
    /// A terrain paint (or the eraser), or a brush size (index into `BRUSHES`).
    Terrain(u8),
    Brush(usize),
    Sculpt(u8),
    Prev,
    Next,
}

impl BuyMode {
    /// Time used by the sky and lights, without changing appointments, seasons or needs.
    pub fn lighting_hour(&self, hour: f32) -> f32 {
        if self.active { self.preview_hour.unwrap_or(hour) } else { hour }
    }

    pub fn toggle_lighting(&mut self, hour: f32) {
        self.preview_hour = Some(if (6.0..20.0).contains(&self.lighting_hour(hour)) { 0.0 } else { 12.0 });
    }

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

/// COVER_STYLE=1 (tests): the wallpaper tab opened (10 seconds in), a pattern with colour presets
/// picked, its second swatch, Create a Style, and its first channel made blue, a step every two
/// seconds.
/// EYEDROP=object|floor (tests): 10 seconds in, Buy mode open and the eyedropper used on the
/// nearest object for sale to the selected Sim, or on the floor in the middle of the house; what
/// it took up is logged two seconds later.
fn scripted_eyedrop(
    mut commands: Commands,
    time: Res<Time>,
    mut buy: ResMut<BuyMode>,
    sel: Query<&Transform, With<crate::sim::Selected>>,
    objects: Query<(Entity, &GameObject, &Transform)>,
    catalog: Res<Catalog>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut step: Local<u8>,
) {
    let Ok(want) = std::env::var("EYEDROP") else { return };
    let t = time.elapsed_secs();
    match *step {
        0 if t > 10.0 => {
            let Ok(me) = sel.single() else { return };
            buy.show(0);
            buy.eyedropper = true;
            if want == "floor" {
                let at = building.as_ref().map_or(me.translation, |b| b.center);
                let ray = Ray3d::new(at + Vec3::Y * 2.5, Dir3::NEG_Y);
                commands.insert_resource(EyedropCover(ray));
            } else if let Some((e, o, _)) = objects
                .iter()
                .filter(|(_, o, _)| catalog.by_key(&o.objd).is_some_and(|c| c.price > 0))
                .min_by(|a, b| a.2.translation.distance(me.translation).total_cmp(&b.2.translation.distance(me.translation)))
            {
                info!("eyedrop test: taking up the {}", o.name);
                commands.insert_resource(EyedropRequest(e));
            }
            *step = 1;
        }
        1 if t > 12.0 => {
            info!(
                "eyedrop test: in hand {:?} (design {:?}), painting {:?} in {:?}, eyedropper {}",
                buy.placing.as_ref().map(|p| p.objd),
                buy.placing.as_ref().and_then(|p| p.design),
                buy.painting,
                buy.cover,
                buy.eyedropper
            );
            *step = 2;
        }
        _ => {}
    }
}

fn scripted_cover(time: Res<Time>, mut buy: ResMut<BuyMode>, ui: Option<Res<crate::icons::GameUi>>, mut buttons: Query<(&BuyButton, &mut Interaction)>, mut step: Local<u8>) {
    if std::env::var("COVER_STYLE").is_err() {
        return;
    }
    let t = time.elapsed_secs();
    if t < 10.0 + *step as f32 * 2.0 {
        return;
    }
    if *step == 0 {
        buy.show(WALLPAPER_TAB);
        *step = 1;
        return;
    }
    let Some(ui) = ui else { return };
    let want = match *step {
        1 => buttons.iter().find_map(|(b, _)| match b {
            BuyButton::Pattern(i) if ui.data.patterns.get(*i).is_some_and(|p| p.swatches.len() > 2 && p.channels.get(1).is_some_and(|c| !c.is_empty())) => Some(*b),
            _ => None,
        }),
        2 => Some(BuyButton::CoverSwatch(1)),
        3 => Some(BuyButton::Styling),
        4 => Some(BuyButton::StyleColour(buy.painting.and_then(|i| ui.data.patterns.get(i)).and_then(|p| p.channels.get(1)?.first().map(|c| c.0)).unwrap_or(0), 12)),
        _ => None,
    };
    let Some(want) = want else { return };
    if let Some((_, mut i)) = buttons.iter_mut().find(|(b, _)| **b == want) {
        *i = Interaction::Pressed;
        info!("cover style test: pressed {want:?}");
    }
    *step += 1;
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
    /// Whether Create a Style is open.
    pub fn styling(&self) -> bool {
        self.styling || self.design_tool
    }

    fn select_design_tool(&mut self, commands: &mut Commands) {
        if self.placing.is_some() || self.painting.is_some() {
            self.styling = self.placing.as_ref().is_some_and(|p| p.edit_in_place) || !self.styling;
        } else {
            self.drop_tools(commands);
            self.design_tool = true;
        }
        self.dirty = true;
    }

    /// Puts down the tool, pattern or object in hand.
    pub fn drop_tools(&mut self, commands: &mut Commands) {
        self.painting = None;
        self.eyedropper = false;
        self.selling = false;
        self.styling = false;
        self.style = None;
        self.pending_style = None;
        self.pending_cover = None;
        self.design_tool = false;
        self.apply_design = false;
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

pub(crate) fn toggle_buy(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut buy: ResMut<BuyMode>,
    mut commands: Commands,
    mut clock: ResMut<crate::clock::GameClock>,
    hold: Option<Res<HoldRequest>>,
    menu: Res<crate::options::GameMenu>,
    modal: Query<(), With<crate::dialog::Modal>>,
) {
    if menu.is_open() || !modal.is_empty() {
        return;
    }
    if keys.just_pressed(KeyCode::F1) {
        enter_mode(&mut buy, None, &mut commands, &mut clock);
    } else if keys.just_pressed(KeyCode::F2) {
        enter_mode(&mut buy, Some(false), &mut commands, &mut clock);
    } else if keys.just_pressed(KeyCode::F3) {
        enter_mode(&mut buy, Some(true), &mut commands, &mut clock);
    } else if keys.just_pressed(KeyCode::KeyB) {
        let on = !buy.active;
        set_active(&mut buy, on, &mut commands, &mut clock);
    }
    if buy.active {
        if keys.just_pressed(KeyCode::KeyH) {
            buy.drop_tools(&mut commands);
        } else if keys.just_pressed(KeyCode::KeyE) {
            buy.drop_tools(&mut commands);
            buy.eyedropper = true;
        } else if keys.just_pressed(KeyCode::KeyK) {
            buy.drop_tools(&mut commands);
            buy.selling = true;
        } else if keys.just_pressed(KeyCode::KeyR) {
            buy.select_design_tool(&mut commands);
        }
    }
    if buy.active && (keys.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right)) {
        let holding = buy.placing.is_some() || buy.painting.is_some() || buy.tool.is_some() || buy.eyedropper || buy.selling || buy.design_tool;
        if holding {
            buy.drop_tools(&mut commands);
        } else if keys.just_pressed(KeyCode::Escape) {
            enter_mode(&mut buy, None, &mut commands, &mut clock);
        }
        // Escape's job is done: don't also open the game menu or dismiss another window.
        keys.clear_just_pressed(KeyCode::Escape);
    }
    // Something from an inventory to put down: Buy mode, on the decorations.
    if hold.is_some() && !buy.active {
        set_active(&mut buy, true, &mut commands, &mut clock);
        buy.show(9);
    }
}

/// Puts something being moved back: in the inventory it came out of, or where it stood.
fn put_back(commands: &mut Commands, assets: &mut ObjectAssets, ctx: &mut AssetCtx, catalog: &Catalog, p: &Placing) {
    if restore_source(commands, p).is_some() {
        return;
    }
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

/// The puck's mode buttons: live (`None`), buy (`Some(false)`) or build (`Some(true)`) mode.
pub fn enter_mode(buy: &mut BuyMode, mode: Option<bool>, commands: &mut Commands, clock: &mut crate::clock::GameClock) {
    match mode {
        None => {
            if buy.active {
                set_active(buy, false, commands, clock);
            }
        }
        Some(build) => {
            // Selecting the current mode is idempotent: keep its tool, category and
            // furniture in hand, just as clicking the already-selected puck button does.
            if buy.active && (buy.category >= WALLPAPER_TAB) == build {
                return;
            }
            if !buy.active {
                set_active(buy, true, commands, clock);
            }
            buy.drop_tools(commands);
            // (Build mode opens on the walls and floors tools, as the game's.)
            buy.show(if build { WALLPAPER_TAB + 2 } else { 0 });
        }
    }
}

fn set_active(buy: &mut BuyMode, on: bool, commands: &mut Commands, clock: &mut crate::clock::GameClock) {
    if on == buy.active {
        return;
    }
    buy.active = on;
    buy.drop_tools(commands);
    // Time stops while shopping, like the original.
    if on {
        buy.resume_speed = Some(clock.speed);
        clock.set_speed(0);
    } else {
        clock.set_speed(buy.resume_speed.take().unwrap_or(0));
        buy.preview_hour = None;
    }
}

/// The lot's tile grid follows its rotation, its sculpted ground and the floor being viewed.
fn lot_grid(buy: Res<BuyMode>, building: Option<Res<crate::building::ActiveBuilding>>, world: Res<CurrentWorld>, mut gizmos: Gizmos) {
    if !buy.active || buy.hide_grid {
        return;
    }
    let Some(b) = building else { return };
    let level = b.view_level;
    let point = |x: f32, z: f32| {
        let p = b.world(x, z, 0.0);
        let y = b.floor_y(level, p).or_else(|| (level == 1).then(|| crate::building::walk_height(&world.data, Some(&b), p)))?;
        Some(p.with_y(y + 0.045))
    };
    let color = Color::srgba(1.0, 1.0, 1.0, 0.22);
    for x in 0..=b.data.width {
        for z in 0..b.data.depth {
            if let (Some(a), Some(c)) = (point(x as f32 + 0.001, z as f32 + 0.001), point(x as f32 + 0.001, z as f32 + 0.999)) {
                gizmos.line(a, c, color);
            }
        }
    }
    for z in 0..=b.data.depth {
        for x in 0..b.data.width {
            if let (Some(a), Some(c)) = (point(x as f32 + 0.001, z as f32 + 0.001), point(x as f32 + 0.999, z as f32 + 0.001)) {
                gizmos.line(a, c, color);
            }
        }
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
    // (The game's own catalogue is up, with the object in hand's designs: only Create a Style
    // here, above it.)
    let editing_style = buy.styling && buy.placing.is_some();
    let build_look = buy.build_look && buy.active && buy.category >= WALLPAPER_TAB;
    let game_look = buy.game_look && buy.active && (buy.category < WALLPAPER_TAB || editing_style);
    if crate::buildhud::native_panel(&buy) { return; }
    if game_look && !(buy.styling && buy.placing.is_some()) {
        return;
    }
    let root = commands
        .spawn((
            BuyPanel,
            GlobalZIndex(6),
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
    if buy.active {
        commands.entity(root).insert(crate::hud::BlocksWorld);
    }
    if game_look || build_look {
        commands.entity(root).insert(Node {
            border_radius: BorderRadius::all(Val::Px(12.0)),
            position_type: PositionType::Absolute,
            left: Val::Px(330.0),
            right: Val::Px(20.0),
            bottom: Val::Px(176.0),
            padding: UiRect::all(Val::Px(10.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        });
    }
    commands.entity(root).with_children(|p| {
        p.spawn(Node { column_gap: Val::Px(6.0), flex_wrap: FlexWrap::Wrap, row_gap: Val::Px(6.0), display: if game_look || build_look { Display::None } else { Display::Flex }, ..default() }).with_children(|row| {
            button(row, if buy.active { "Exit Buy Mode (B)".into() } else { "Buy Mode (B)".into() }, BuyButton::Toggle, Val::Px(150.0), 32.0, buy.active);
            if buy.active {
                button(row, "Eyedropper".into(), BuyButton::Eyedropper, Val::Auto, 32.0, buy.eyedropper);
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
        if buy.category == ROOFS_TAB && !editing_style {
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
        if buy.category == TERRAIN_TAB && !editing_style {
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
        if buy.category == FENCES_TAB && !editing_style {
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
        if buy.category == BUILD_TAB && !editing_style {
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
        if (buy.category == WALLPAPER_TAB || buy.category == FLOORS_TAB) && !editing_style {
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
                let how = if floor { "click a tile · Shift: fill room" } else { "click a wall side · Shift: fill room" };
                row.spawn(text(format!("Page {} / {} · {} patterns · {how} · right-click to stop", page + 1, pages, items.len()), 13.0, Color::WHITE));
                button(row, "Next >".into(), BuyButton::Next, Val::Px(80.0), 28.0, false);
            });
            // The pattern in hand: its swatches (the game's colour presets), and Create a Style.
            let Some(pat) = buy.painting.and_then(|i| ui.data.patterns.get(i)) else { return };
            let current = buy.cover.unwrap_or(pat.texture);
            let swatch = buy.cover_style.as_ref().filter(|s| s.cwal == pat.cwal).map(|s| s.swatch as usize).or_else(|| pat.swatches.iter().position(|k| *k == current)).unwrap_or(0);
            let channels = pat.channels.get(swatch).cloned().unwrap_or_default();
            if pat.swatches.len() < 2 && channels.is_empty() {
                return;
            }
            p.spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn(text(format!("{} · Colors", pat.name), 13.0, Color::WHITE));
                if !channels.is_empty() {
                    button(row, if buy.styling { "Close Create a Style".into() } else { "Create a Style".into() }, BuyButton::Styling, Val::Px(150.0), 28.0, buy.styling);
                }
                for (s, key) in pat.swatches.iter().enumerate() {
                    let tex = assets.texture(&mut ctx, *key);
                    let chosen = *key == current;
                    row.spawn((
                        Button,
                        BuyButton::CoverSwatch(s as u8),
                        Node {
                            width: Val::Px(if floor { 36.0 } else { 22.0 }),
                            height: Val::Px(36.0),
                            border: UiRect::all(Val::Px(if chosen { 3.0 } else { 1.0 })),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BorderColor::all(if chosen { PLUMBOB_GREEN } else { Color::WHITE }),
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|b| {
                        if let Some(t) = tex {
                            b.spawn((ImageNode::new(t), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                        }
                    });
                }
                // (The style made, shown as what's painted with now.)
                if buy.cover_style.as_ref().is_some_and(|s| s.cwal == pat.cwal)
                    && let Some(t) = assets.texture(&mut ctx, current)
                {
                    row.spawn((
                        Node { width: Val::Px(if floor { 36.0 } else { 22.0 }), height: Val::Px(36.0), border: UiRect::all(Val::Px(3.0)), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() },
                        BorderColor::all(PLUMBOB_GREEN),
                    ))
                    .with_children(|b| {
                        b.spawn((ImageNode::new(t), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                    });
                }
            });
            if buy.styling && !channels.is_empty() {
                if renders.busy() {
                    p.spawn(text("Restyling…", 13.0, Color::srgb(1.0, 0.9, 0.5)));
                }
                for (ch, own) in channels {
                    let now = buy.cover_style.as_ref().filter(|s| s.cwal == pat.cwal && s.swatch as usize == swatch).and_then(|s| s.colours.iter().find(|c| c.0 == ch)).map_or(own, |c| c.1);
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
            return;
        }
        // The object in hand's designs (the game's colour and pattern presets for it).
        if let Some(pl) = &buy.placing {
            if pl.edit_in_place {
                let name = catalog.by_key(&pl.objd).map_or("Object", |entry| entry.name.as_str());
                p.spawn(Node { column_gap: Val::Px(8.0), row_gap: Val::Px(4.0), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    row.spawn(text(format!("Create a Style · {name}"), 14.0, Color::WHITE));
                    button(row, "Apply (Enter)".into(), BuyButton::ApplyDesign, Val::Px(120.0), 28.0, true);
                    button(row, "Cancel (Esc)".into(), BuyButton::CancelDesign, Val::Px(120.0), 28.0, false);
                });
            }
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            let n = ObjectAssets::design_count(&ctx, pl.objd);
            // (A row for its designs, or for Create a Style when it has colours to change.)
            let restylable = data.0.designs.get(&pl.objd).is_some_and(|d| d.channels.iter().any(|c| !c.is_empty()));
            if n > 1 || restylable {
                let name = catalog.by_key(&pl.objd).map(|e| e.name.clone()).unwrap_or_default();
                p.spawn(Node { column_gap: Val::Px(6.0), row_gap: Val::Px(4.0), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    row.spawn(text(format!("{name} · Design"), 14.0, Color::WHITE));
                    // (Create a Style, where its design has colour channels to change.)
                    let design = buy.style.as_ref().filter(|s| s.objd == pl.objd).map(|s| s.design).or_else(|| pl.design.filter(|k| k.0 == s3bake::gamedata::T_DESIGN).map(|k| k.1 as u8)).unwrap_or(0);
                    if !pl.edit_in_place && data.0.designs.get(&pl.objd).is_some_and(|d| d.channels.get(design as usize).is_some_and(|c| !c.is_empty())) {
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
        if game_look {
            return;
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
    (ui, mut covered): (Option<Res<crate::icons::GameUi>>, MessageReader<crate::style::CoverStyleReady>),
    mut notes: ResMut<Notifications>,
) {
    // A wall or floor style rendered: painted with from now on.
    for r in covered.read() {
        if buy.pending_cover.as_ref() != Some(&r.0) { continue; }
        buy.pending_cover = None;
        buy.cover = Some(r.0.texture());
        buy.cover_style = Some(r.0.clone());
        buy.dirty = true;
    }
    // A style made in Create a Style, rendered: the object in hand in it.
    for r in styled.read() {
        if buy.pending_style.as_ref() != Some(&r.0) { continue; }
        buy.pending_style = None;
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
    if !renders.busy() && (buy.pending_style.is_some() || buy.pending_cover.is_some()) {
        buy.pending_style = None;
        buy.pending_cover = None;
        buy.dirty = true;
        notes.push("That style couldn't be rendered. The previous design is still selected.");
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
                buy.select_design_tool(&mut commands);
            }
            BuyButton::ApplyDesign => {
                if !renders.busy() && buy.pending_style.is_none() { buy.apply_design = true; }
            }
            BuyButton::CancelDesign => {
                buy.drop_tools(&mut commands);
            }
            BuyButton::Eyedropper => {
                let on = !buy.eyedropper;
                buy.drop_tools(&mut commands);
                buy.eyedropper = on;
                buy.dirty = true;
                play.write(crate::sound::PlaySound::ui("ui_build_design_tool_open"));
            }
            BuyButton::CoverSwatch(s) => {
                let Some(pat) = buy.painting.and_then(|i| ui.as_ref()?.data.patterns.get(i)) else { continue };
                buy.cover = pat.swatches.get(*s as usize).copied();
                buy.cover_style = None;
                buy.pending_cover = None;
                buy.dirty = true;
            }
            // (Create a Style for the wallpaper or floor in hand.)
            BuyButton::StyleColour(ch, idx) if buy.painting.is_some() => {
                let (Some(pat), Some(&colour), Some(path)) = (buy.painting.and_then(|i| ui.as_ref()?.data.patterns.get(i)), crate::style::PALETTE.get(*idx as usize), install.as_ref()) else { continue };
                let current = buy.pending_cover.as_ref().or(buy.cover_style.as_ref());
                let swatch = current.filter(|s| s.cwal == pat.cwal).map(|s| s.swatch).or_else(|| pat.swatches.iter().position(|k| Some(*k) == buy.cover).map(|i| i as u8)).unwrap_or(0);
                let mut colours = current.filter(|s| s.cwal == pat.cwal && s.swatch == swatch).map(|s| s.colours.clone()).unwrap_or_default();
                colours.retain(|(c, _)| c != ch);
                colours.push((*ch, colour));
                colours.sort_by_key(|c| c.0);
                let style = crate::style::CoverStyle { cwal: pat.cwal, swatch, floor: pat.floor, colours };
                renders.request_cover(style.clone(), path.0.clone());
                buy.pending_cover = Some(style);
                buy.dirty = true;
            }
            BuyButton::StyleColour(ch, idx) => {
                let (Some(p), Some(&colour), Some(path)) = (buy.placing.as_ref(), crate::style::PALETTE.get(*idx as usize), install.as_ref()) else { continue };
                let Some(info) = data.0.designs.get(&p.objd) else { continue };
                let current = buy.pending_style.as_ref().or(buy.style.as_ref());
                let design = current.filter(|s| s.objd == p.objd).map(|s| s.design).or_else(|| p.design.filter(|k| k.0 == s3bake::gamedata::T_DESIGN).map(|k| k.1 as u8)).unwrap_or(0);
                // (On top of the style already made from this design, if any.)
                let mut colours = current.filter(|s| s.objd == p.objd && s.design == design).map(|s| s.colours.clone()).unwrap_or_default();
                colours.retain(|(c, _)| c != ch);
                colours.push((*ch, colour));
                colours.sort_by_key(|c| c.0);
                let style = crate::style::ObjectStyle { objd: p.objd, design, colours };
                renders.request_object(style.clone(), info.size, path.0.clone());
                buy.pending_style = Some(style);
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
                buy.pending_style = None;
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
                buy.cover = None;
                buy.cover_style = None;
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
    (keys, mouse, over_ui, menu, modal): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, Res<PointerOverUi>, Res<crate::options::GameMenu>, Query<(), With<crate::dialog::Modal>>),
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
    removed: Res<crate::save::RemovedLotObjects>,
) {
    if menu.is_open() || !modal.is_empty() { return; }
    let Some(i) = buy.painting.filter(|_| buy.active) else { return };
    if mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) {
        buy.painting = None;
        buy.dirty = true;
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) || over_ui.0 || std::mem::take(&mut buy.eyedropped) {
        return;
    }
    let (Some(ui), Some(b)) = (ui, building.as_deref_mut()) else { return };
    let Some(mut pat) = ui.data.patterns.get(i).cloned() else { return };
    // (In the swatch or style chosen for it.)
    if let Some(k) = buy.cover {
        pat.texture = k;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    let fill = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ops = if pat.floor {
        // Normal click covers one tile; Shift follows the room's current wall boundaries.
        let Some((p, level)) = crate::hud::floor_hit(ray, &world, Some(b)) else { return };
        let l = b.local(p);
        if std::env::var_os("BUILD_HISTORY_TEST").is_some() { info!("autotest: floor covering at {l:?}, level {level}, room fill {fill}"); }
        let ground = b.view_level <= 1 && !b.data.floors.iter().any(|f| f.level == 1 && f.x as f32 == l.x.floor() && f.z as f32 == l.y.floor());
        if ground {
            let at = l.floor();
            let heights = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y].map(|offset| {
                let p = b.world(at.x + offset.x, at.y + offset.y, 0.0);
                world.data.heightmap.sample(p.x, p.z)
            });
            crate::covering::paving(&b.data, l, pat.texture, heights)
        } else {
            crate::covering::floors(&b.data, level, l, pat.texture, fill)
        }
    } else {
        let Some((wall, side)) = pick_wall(ray, b) else { return };
        crate::covering::walls(&b.data, wall, side, pat.texture, fill)
    };
    if ops.is_empty() {
        return;
    }
    let cost = crate::covering::cost(&b.data, &ops, pat.price.max(0) as u32);
    if household.as_ref().is_some_and(|h| h.funds < cost) {
        notes.push("You can't afford that.");
        return;
    }
    let before = crate::building::BuildingSnapshot::capture(b, log.as_deref());
    if let Some(h) = household.as_mut() {
        h.funds -= cost;
    }
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
    crate::buyhistory::record_construction(&mut commands, before, b, &ops, -cost, removed.0.clone(), Vec::new());
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
    selected: Query<Entity, With<crate::sim::Selected>>,
    mut grid: Option<ResMut<NavGrid>>,
    (pickup, hold): (Option<Res<PickupRequest>>, Option<Res<HoldRequest>>),
    objects: Query<(&GameObject, &Transform, Option<&crate::objects::Design>, Option<&crate::paintings::Hung>, Option<&crate::interact::UsedBy>, Option<&crate::nav::Obstacle>)>,
    (bought_q, mut removed): (Query<(), With<crate::save::Bought>>, ResMut<crate::save::RemovedLotObjects>),
    mut tfs: Query<&mut Transform, Without<GameObject>>,
    (mut faces, mut gizmos, renders, menu, modal): (Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>, Gizmos, Res<crate::style::StyleRenders>, Res<crate::options::GameMenu>, Query<(), With<crate::dialog::Modal>>),
) {
    // What was being moved when Buy mode closed goes back.
    for p in std::mem::take(&mut buy.returning) {
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        put_back(&mut commands, &mut assets, &mut ctx, &catalog, &p);
        if let Some(g) = grid.as_mut() {
            g.dirty = true;
        }
    }
    if !buy.active || menu.is_open() || !modal.is_empty() {
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
            if let Ok((obj, tf, design, hung, used, obstacle)) = objects.get(req.0) {
                if used.is_some_and(|u| u.0.is_some()) {
                    notes.push("That object is being used.");
                    return;
                }
                let design = design.map(|d| d.0);
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let edit_in_place = buy.design_tool;
                if edit_in_place && ObjectAssets::design_count(&ctx, obj.objd) < 2
                    && !data.0.designs.get(&obj.objd).is_some_and(|d| d.channels.iter().any(|c| !c.is_empty()))
                {
                    notes.push("That object has no editable designs.");
                    return;
                }
                let parts = assets.object_design(&mut ctx, obj.objd, design);
                let ghost = spawn_parts(&mut commands, &parts, *tf);
                commands.entity(ghost).insert(DespawnOnExit(AppState::InGame));
                buy.yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
                buy.placing = Some(Placing { item: hung.map(|h| h.0.clone()), origin: Some(*tf), source: Some((req.0, obstacle.copied())), edit_in_place, ..Placing::new(obj.objd, ghost, true, design) });
                buy.design_tool = false;
                buy.styling = edit_in_place;
                buy.dirty = true;
                hold_source(&mut commands, req.0);
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
    let design = placing.design;
    if placing.edit_in_place {
        if !renders.busy() && buy.pending_style.is_none() && (buy.apply_design || keys.just_pressed(KeyCode::Enter)) {
            if let Some((source, _)) = placing.source {
                let removed_before = removed.0.clone();
                restore_source(&mut commands, placing);
                if let Ok((obj, tf, original_design, ..)) = objects.get(source)
                    && original_design.map(|d| d.0) != design
                {
                    if !bought_q.contains(source) {
                        crate::save::note_removed(&mut removed, obj, tf);
                    }
                    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                    let parts = assets.object_design(&mut ctx, objd, design);
                    crate::objects::restyle_parts(&mut commands, source, &parts);
                    commands.entity(source).insert(crate::save::Bought);
                    if let Some(design) = design {
                        commands.entity(source).insert(crate::objects::Design(design));
                    } else {
                        commands.entity(source).remove::<crate::objects::Design>();
                    }
                    crate::buyhistory::record(&mut commands, source, true, 0, removed_before);
                }
            }
            commands.entity(ghost).despawn();
            buy.placing = None;
            buy.styling = false;
            buy.apply_design = false;
            buy.dirty = true;
            if let Some(g) = grid.as_mut() { g.dirty = true; }
        }
        return;
    }
    let price = placing.item.as_ref().map_or_else(|| catalog.by_key(&objd).map_or(0, |e| e.price.max(0) as i64), |i| i.worth);
    // Selling and cancelling do not need a cursor in the window.
    if (keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) || buy.selling) && owned {
        let removed_before = removed.0.clone();
        let source = placing.source.map(|(e, _)| e);
        if let Some((source, _)) = placing.source {
            if !bought_q.contains(source) && let Ok((obj, tf, ..)) = objects.get(source) {
                crate::save::note_removed(&mut removed, obj, tf);
            }
            crate::buyhistory::park(&mut commands, source);
        }
        commands.entity(ghost).despawn();
        buy.placing = None;
        buy.dirty = true;
        if let Some(h) = household.as_mut() {
            h.funds += price;
        }
        if let Some(g) = grid.as_mut() {
            g.dirty = true;
        }
        notes.push(format!("Sold for §{price}."));
        if let Some(source) = source {
            crate::buyhistory::record(&mut commands, source, true, price, removed_before);
        } else {
            crate::buyhistory::discard(&mut commands);
        }
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };

    let ground = ground_hit(ray, &world);
    let floor = crate::hud::floor_hit(ray, &world, building.as_deref());
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
        } else if let Some((p, level)) = floor {
            // Quarter tiles in the lot's coordinate system, including rotated lots.
            let p = match building.as_deref() {
                Some(b) if !keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) => {
                    let l = snap_local(b.rot, b.corner, p);
                    b.world(l.x, l.y, 0.0)
                }
                _ => p,
            };
            tf.translation = p.with_y(crate::nav::floor_height(&world.data, building.as_deref(), level, p));
            tf.rotation = Quat::from_rotation_y(buy.yaw);
        }
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
    let problem = tfs.get(ghost).ok().and_then(|tf| {
        let b = building.as_deref()?;
        let bounds = bounds?;
        placement_problem(b, tf, bounds, floor.map_or(b.view_level, |(_, level)| level), ladder, opening.is_some() || wall_hung)
    });
    if !over_ui.0 && let (Ok(tf), Some(bounds)) = (tfs.get(ghost), bounds) {
        let afford = owned || household.as_ref().is_some_and(|h| h.funds >= price);
        let valid = afford && problem.is_none() && (!wall_hung && opening.is_none() || in_wall.is_some()) && (!ladder || on_edge.is_some());
        let color = if valid { Color::srgb(0.35, 1.0, 0.15) } else { Color::srgb(1.0, 0.15, 0.1) };
        let corners = footprint(tf, bounds);
        for i in 0..4 {
            gizmos.line(corners[i].with_y(tf.translation.y + 0.07), corners[(i + 1) % 4].with_y(tf.translation.y + 0.07), color);
        }
        let center = tf.translation + tf.rotation * Vec3::new((bounds.0.x + bounds.1.x) * 0.5, 0.07, bounds.1.z);
        let tip = center + tf.rotation * Vec3::Z * 0.4;
        gizmos.line(center, tip, color);
        gizmos.line(tip, tip + tf.rotation * Vec3::new(-0.15, 0.0, -0.15), color);
        gizmos.line(tip, tip + tf.rotation * Vec3::new(0.15, 0.0, -0.15), color);
    }
    if mouse.just_pressed(MouseButton::Left) && !over_ui.0 && floor.is_some() && !std::mem::take(&mut buy.eyedropped) {
        let funds = household.as_ref().map(|h| h.funds).unwrap_or(0);
        if !owned && funds < price {
            notes.push("You can't afford that.");
            return;
        }
        if let Some(problem) = problem {
            notes.push(problem);
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
        let mut building_before = None;
        if let (Some((_, _, ops)), Some(b)) = (in_wall, building.as_deref_mut())
            && !ops.is_empty()
        {
            building_before = Some(crate::building::BuildingSnapshot::capture(b, log.as_deref()));
            crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
            match log.as_mut() {
                Some(l) => l.0.extend(ops),
                None => commands.insert_resource(crate::building::LotPaint(ops)),
            }
        }
        if opening.is_some() {
            play.write(crate::sound::PlaySound::ui("ui_build_door_plop"));
        }
        let source = buy.placing.as_ref().and_then(|p| p.source.map(|s| s.0));
        let removed_before = removed.0.clone();
        let level = building.as_ref().map_or(1, |b| b.level_at(pos.y));
        let placed = if let Some(source) = source {
            if !bought_q.contains(source) && let Ok((obj, tf, ..)) = objects.get(source) {
                crate::save::note_removed(&mut removed, obj, tf);
            }
            restore_source(&mut commands, buy.placing.as_ref().unwrap());
            // Moving a placed object must also keep any authored model scale.
            let scale = objects.get(source).map_or(Vec3::ONE, |(_, tf, ..)| tf.scale);
            commands.entity(source).insert(Transform::from_translation(pos).with_rotation(rot).with_scale(scale));
            let parts = assets.object_design(&mut ctx, objd, design);
            crate::objects::restyle_parts(&mut commands, source, &parts);
            match design {
                Some(key) => { commands.entity(source).insert(crate::objects::Design(key)); }
                None => { commands.entity(source).remove::<crate::objects::Design>(); }
            }
            Some(source)
        } else {
            crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, objd, pos, rot, design).map(|o| o.entity)
        };
        if let Some(entity) = placed {
            commands.entity(entity).insert((crate::nav::Floor(level), crate::building::BuildingPiece { level }));
            commands.entity(entity).insert(crate::save::Bought);
            if let Some(item) = buy.placing.as_ref().and_then(|p| p.item.clone()) {
                commands.entity(entity).insert(crate::paintings::Hung(item));
            }
            if !owned && let Some(h) = household.as_mut() {
                h.funds -= price;
                if let Ok(s) = selected.single() {
                    commands.entity(entity).insert(crate::buyhistory::PendingPurchase { sim: s, price: price as i32 });
                }
            }
            if let Some(g) = grid.as_mut() {
                g.dirty = true;
            }
            if owned {
                if source.is_some() {
                    crate::buyhistory::record_with_building(&mut commands, entity, true, 0, removed_before, building_before);
                } else {
                    crate::buyhistory::discard(&mut commands);
                }
                commands.entity(ghost).despawn();
                buy.placing = None;
                buy.dirty = true;
            } else {
                crate::buyhistory::record_with_building(&mut commands, entity, false, -price, removed_before, building_before);
            }
        }
    }
    let _ = ObjectKind::Other;
}

fn snap_local(rot: Quat, corner: Vec3, p: Vec3) -> Vec2 {
    let l = rot.inverse() * (p - corner);
    Vec2::new((l.x * 4.0).round() / 4.0, (l.z * 4.0).round() / 4.0)
}

fn footprint(tf: &Transform, (min, max): (Vec3, Vec3)) -> [Vec3; 4] {
    [(min.x, min.z), (max.x, min.z), (max.x, max.z), (min.x, max.z)].map(|(x, z)| tf.transform_point(Vec3::new(x, 0.0, z)))
}

fn placement_problem(b: &crate::building::ActiveBuilding, tf: &Transform, bounds: (Vec3, Vec3), level: u8, ladder: bool, wall: bool) -> Option<&'static str> {
    let corners = footprint(tf, bounds);
    let size = Vec2::new(b.data.width as f32, b.data.depth as f32);
    if corners.iter().any(|p| { let l = b.local(*p); l.cmplt(Vec2::splat(-0.01)).any() || l.cmpgt(size + 0.01).any() }) {
        return Some("Place objects inside your home lot.");
    }
    if !wall && !ladder {
        let local = corners.map(|p| b.local(p));
        // Flat floor decorations may lie beneath a wall; furniture may not straddle it.
        if bounds.1.y - bounds.0.y > 0.08 && b.data.walls.iter().any(|w| {
            w.level.max(1) == level && wall_overlaps_footprint(&local, Vec2::from(w.a), Vec2::from(w.b))
        }) {
            return Some("There's a wall in the way.");
        }
        if level > 1 && !footprint_supported(&local, |p| b.floor_y(level, b.world(p.x, p.y, 0.0)).is_some()) {
            return Some("The whole object needs a floor underneath it.");
        }
        if level == 1 && b.data.pool.iter().any(|f| {
            let p = Vec2::new(f.x as f32, f.z as f32);
            convex_overlap(&local, &[p, p + Vec2::X, p + Vec2::ONE, p + Vec2::Y])
        }) {
            return Some("That object can't be placed in a swimming pool.");
        }
    }
    None
}

fn wall_overlaps_footprint(corners: &[Vec2; 4], a: Vec2, b: Vec2) -> bool {
    let along = b - a;
    if along.length_squared() < 0.0001 {
        return false;
    }
    // Match a narrow solid wall, rather than its infinitely thin centre line.
    let side = along.normalize().perp() * 0.03;
    convex_overlap(corners, &[a - side, b - side, b + side, a + side])
}

/// Positive-area overlap; touching a tile edge is legal. Axes from both polygons
/// are needed for furniture rotated relative to the lot and triangular floor tiles.
fn convex_overlap(a: &[Vec2], b: &[Vec2]) -> bool {
    for polygon in [a, b] {
        for i in 0..polygon.len() {
            let axis = (polygon[(i + 1) % polygon.len()] - polygon[i]).perp().normalize_or_zero();
            if axis == Vec2::ZERO {
                continue;
            }
            let interval = |points: &[Vec2]| points.iter().map(|p| p.dot(axis)).fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x)));
            let (al, ah) = interval(a);
            let (bl, bh) = interval(b);
            if ah.min(bh) - al.max(bl) <= 0.0001 {
                return false;
            }
        }
    }
    true
}

/// Check every floor triangle touched by the footprint, including holes between
/// its corners. Floor masks divide each tile into four triangles meeting at its centre.
fn footprint_supported(corners: &[Vec2; 4], has_floor: impl Fn(Vec2) -> bool) -> bool {
    let min = corners.iter().copied().fold(Vec2::splat(f32::INFINITY), Vec2::min);
    let max = corners.iter().copied().fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
    for x in min.x.floor() as i32..max.x.ceil() as i32 {
        for z in min.y.floor() as i32..max.y.ceil() as i32 {
            let p = Vec2::new(x as f32, z as f32);
            let edges = [p, p + Vec2::X, p + Vec2::ONE, p + Vec2::Y];
            let centre = p + Vec2::splat(0.5);
            for i in 0..4 {
                let triangle = [edges[i], edges[(i + 1) % 4], centre];
                if convex_overlap(corners, &triangle) && !has_floor((triangle[0] + triangle[1] + triangle[2]) / 3.0) {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn furniture_cannot_cross_straight_or_diagonal_walls() {
        let footprint = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y];
        assert!(wall_overlaps_footprint(&footprint, Vec2::new(-1.0, 0.5), Vec2::new(2.0, 0.5)));
        assert!(wall_overlaps_footprint(&footprint, Vec2::splat(-1.0), Vec2::splat(2.0)));
        assert!(!wall_overlaps_footprint(&footprint, Vec2::new(-1.0, 1.1), Vec2::new(2.0, 1.1)));
        assert!(!wall_overlaps_footprint(&footprint, Vec2::ZERO, Vec2::ZERO));
        // The infinite extension of a wall is not an obstacle past its endpoint.
        assert!(!wall_overlaps_footprint(&footprint, Vec2::new(2.0, 0.5), Vec2::new(3.0, 0.5)));
    }

    #[test]
    fn selecting_current_mode_keeps_tools_and_category() {
        let mut app = controls();
        key(&mut app, KeyCode::F2);
        app.world_mut().resource_mut::<BuyMode>().category = 4;
        key(&mut app, KeyCode::KeyK);
        key(&mut app, KeyCode::F2);
        let buy = app.world().resource::<BuyMode>();
        assert!(buy.selling);
        assert_eq!(buy.category, 4);
        key(&mut app, KeyCode::F3);
        assert!(!app.world().resource::<BuyMode>().selling);
        app.world_mut().resource_mut::<BuyMode>().category = FLOORS_TAB;
        key(&mut app, KeyCode::F3);
        assert_eq!(app.world().resource::<BuyMode>().category, FLOORS_TAB);
    }

    #[test]
    fn tool_shortcuts_are_exclusive_and_only_work_while_shopping() {
        let mut app = controls();
        key(&mut app, KeyCode::KeyE);
        assert!(!app.world().resource::<BuyMode>().eyedropper);
        key(&mut app, KeyCode::F2);
        key(&mut app, KeyCode::KeyE);
        assert!(app.world().resource::<BuyMode>().eyedropper);
        key(&mut app, KeyCode::KeyK);
        let buy = app.world().resource::<BuyMode>();
        assert!(buy.selling);
        assert!(!buy.eyedropper);
        key(&mut app, KeyCode::KeyH);
        let buy = app.world().resource::<BuyMode>();
        assert!(!buy.selling && !buy.eyedropper);
        assert!(buy.active);
    }

    #[test]
    fn design_tool_selection_cancels_without_leaving_buy_mode() {
        let mut app = controls();
        key(&mut app, KeyCode::F2);
        key(&mut app, KeyCode::KeyR);
        assert!(app.world().resource::<BuyMode>().design_tool);
        assert!(app.world().resource::<BuyMode>().styling());
        key(&mut app, KeyCode::F2);
        assert!(app.world().resource::<BuyMode>().design_tool);
        key(&mut app, KeyCode::Escape);
        let buy = app.world().resource::<BuyMode>();
        assert!(buy.active);
        assert!(!buy.design_tool);
    }

    #[test]
    fn cancelling_style_edit_discards_in_flight_render_requests() {
        let mut app = controls();
        key(&mut app, KeyCode::F2);
        let ghost = app.world_mut().spawn_empty().id();
        let mut buy = app.world_mut().resource_mut::<BuyMode>();
        buy.placing = Some(Placing::new((1, 2, 3), ghost, false, None));
        buy.pending_style = Some(crate::style::ObjectStyle { objd: (1, 2, 3), design: 0, colours: vec![(0, [1.0, 0.0, 0.0])] });
        buy.pending_cover = Some(crate::style::CoverStyle { cwal: 1, swatch: 0, floor: false, colours: Vec::new() });
        key(&mut app, KeyCode::Escape);
        let buy = app.world().resource::<BuyMode>();
        assert!(buy.pending_style.is_none() && buy.pending_cover.is_none());
        assert!(buy.placing.is_none());
        assert!(app.world().get_entity(ghost).is_err());
    }

    #[test]
    fn furniture_cannot_bridge_an_interior_floor_hole() {
        let corners = [Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::splat(4.0), Vec2::new(0.0, 4.0)];
        assert!(!footprint_supported(&corners, |p| !(p.x.floor() == 1.0 && p.y.floor() == 2.0)));
        assert!(footprint_supported(&corners, |p| p.x >= 0.0 && p.x < 4.0 && p.y >= 0.0 && p.y < 4.0));
    }

    #[test]
    fn footprint_respects_diagonal_floor_masks_and_edge_contact() {
        let supported = [Vec2::new(0.35, 0.05), Vec2::new(0.65, 0.05), Vec2::new(0.65, 0.2), Vec2::new(0.35, 0.2)];
        let north = |p: Vec2| p.y < p.x && p.y < 1.0 - p.x;
        assert!(footprint_supported(&supported, north));
        let crossing = supported.map(|p| p + Vec2::new(0.0, 0.4));
        assert!(!footprint_supported(&crossing, north));
        let tile = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y];
        assert!(!convex_overlap(&tile, &tile.map(|p| p + Vec2::X)));
        assert!(convex_overlap(&tile, &tile.map(|p| p + Vec2::new(0.9, 0.0))));
        let rotated = tile.map(|p| Vec2::new(p.x - p.y, p.x + p.y) * 0.7);
        assert!(convex_overlap(&tile, &rotated));
        // A long object can cross a pool tile without any of its corners being in it.
        let bridge = [Vec2::new(-2.0, 0.2), Vec2::new(3.0, 0.2), Vec2::new(3.0, 0.4), Vec2::new(-2.0, 0.4)];
        assert!(convex_overlap(&tile, &bridge));
    }

    fn controls() -> App {
        let mut app = App::new();
        app.init_resource::<BuyMode>()
            .init_resource::<crate::clock::GameClock>()
            .init_resource::<crate::options::GameMenu>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_systems(Update, toggle_buy);
        app
    }

    fn key(app: &mut App, code: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(code);
        app.update();
    }

    #[test]
    fn mode_keys_select_and_restore_every_live_speed() {
        for speed in 0..4 {
            let mut app = controls();
            app.world_mut().resource_mut::<crate::clock::GameClock>().set_speed(speed);
            key(&mut app, KeyCode::F2);
            assert!(app.world().resource::<BuyMode>().active);
            assert_eq!(app.world().resource::<crate::clock::GameClock>().speed, 0);
            key(&mut app, KeyCode::F2);
            assert!(app.world().resource::<BuyMode>().active, "F2 must select, not toggle");
            key(&mut app, KeyCode::F3);
            assert_eq!(app.world().resource::<BuyMode>().category, BUILD_TAB);
            key(&mut app, KeyCode::F1);
            assert!(!app.world().resource::<BuyMode>().active);
            assert_eq!(app.world().resource::<crate::clock::GameClock>().speed, speed);
        }
    }

    #[test]
    fn escape_cancels_the_tool_then_leaves_buy_mode() {
        let mut app = controls();
        key(&mut app, KeyCode::F2);
        app.world_mut().resource_mut::<BuyMode>().selling = true;
        key(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<BuyMode>().active);
        assert!(!app.world().resource::<BuyMode>().selling);
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::Escape));
        key(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<BuyMode>().active);
    }

    #[test]
    fn lighting_preview_is_reset_on_return_to_live_mode() {
        let mut app = controls();
        let minutes = app.world().resource::<crate::clock::GameClock>().minutes;
        key(&mut app, KeyCode::F2);
        let mut buy = app.world_mut().resource_mut::<BuyMode>();
        buy.toggle_lighting(8.0);
        assert_eq!(buy.lighting_hour(8.0), 0.0);
        buy.toggle_lighting(8.0);
        assert_eq!(buy.lighting_hour(8.0), 12.0);
        key(&mut app, KeyCode::F1);
        assert_eq!(app.world().resource::<BuyMode>().lighting_hour(8.0), 8.0);
        assert_eq!(app.world().resource::<crate::clock::GameClock>().minutes, minutes);
    }

    #[test]
    fn cancelling_a_move_preserves_the_original_entity_and_its_gameplay_state() {
        let mut world = World::new();
        let obstacle = crate::nav::Obstacle { half: Vec2::ONE, center_offset: Vec2::ZERO };
        let source = world.spawn((Transform::from_xyz(10.0, 2.0, 5.0), Visibility::Inherited, obstacle, crate::upgrades::Upgrades(3), crate::interact::Broken, crate::surroundings::TrashFill(4))).id();
        let child = world.spawn(ChildOf(source)).id();
        let ghost = world.spawn_empty().id();
        let p = Placing { source: Some((source, Some(obstacle))), ..Placing::new((0, 0, 1), ghost, true, None) };
        let mut queue = bevy::ecs::world::CommandQueue::default();
        hold_source(&mut Commands::new(&mut queue, &world), source);
        queue.apply(&mut world);
        assert!(world.get::<HeldObject>(source).is_some());
        assert!(world.get::<crate::nav::Obstacle>(source).is_none());
        assert_eq!(*world.get::<Visibility>(source).unwrap(), Visibility::Hidden);
        assert_eq!(restore_source(&mut Commands::new(&mut queue, &world), &p), Some(source));
        queue.apply(&mut world);
        assert!(world.get::<HeldObject>(source).is_none());
        assert_eq!(world.get::<crate::nav::Obstacle>(source).unwrap().half, Vec2::ONE);
        assert_eq!(world.get::<crate::upgrades::Upgrades>(source).unwrap().0, 3);
        assert!(world.get::<crate::interact::Broken>(source).is_some());
        assert_eq!(world.get::<crate::surroundings::TrashFill>(source).unwrap().0, 4);
        assert_eq!(world.get::<ChildOf>(child).unwrap().parent(), source);
        assert_eq!(world.get::<Transform>(source).unwrap().translation, Vec3::new(10.0, 2.0, 5.0));
    }

    #[test]
    fn placement_snaps_to_quarter_tiles_on_a_rotated_lot() {
        let corner = Vec3::new(50.0, 3.0, 70.0);
        let rot = Quat::from_rotation_y(0.7);
        let p = corner + rot * Vec3::new(3.14, 0.0, 4.61);
        assert!(snap_local(rot, corner, p).distance(Vec2::new(3.25, 4.5)) < 0.001);
        let tf = Transform::from_translation(corner).with_rotation(rot);
        let points = footprint(&tf, (Vec3::new(-1.0, 0.0, -2.0), Vec3::new(1.0, 2.0, 2.0)));
        for p in points {
            let l = rot.inverse() * (p - corner);
            assert!((l.x.abs() - 1.0).abs() < 0.001);
            assert!((l.z.abs() - 2.0).abs() < 0.001);
        }
    }
}
