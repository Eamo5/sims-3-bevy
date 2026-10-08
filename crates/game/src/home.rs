//! Creating a household, choosing a lot in the world and moving in.

use bevy::prelude::*;
use rand::Rng;
use s3formats::world::LotInfo;
use s3bake::Key;

use crate::camera::SimsCamera;
use crate::baked::Baked;
use crate::interact::*;
use crate::loading::{Catalog, CurrentWorld};
use crate::menu::{BTN_NORMAL, PLUMBOB_GREEN, button_visuals, text};
use crate::nav::{NavGrid, Obstacle};
use crate::objects::{AssetCtx, ObjectAssets, parts_bounds, spawn_parts};
use crate::sim::*;
use crate::{AppState, PlayMode};

pub struct HomePlugin;

impl Plugin for HomePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(PlayMode::ChooseLot), spawn_lot_chooser)
            .add_systems(
                Update,
                (button_visuals, lot_buttons, draw_lots, auto_move_in, premade_move_in).run_if(in_state(PlayMode::ChooseLot)),
            )
            .add_systems(Update, (draw_home_lot, start_move).run_if(in_state(PlayMode::Live)));
    }
}

/// The household being created before the world loads.
#[derive(Resource, Clone)]
pub struct PendingHousehold {
    pub last_name: String,
    pub members: Vec<Sim>,
    /// A town family being played (they move straight into their own home).
    pub premade: Option<s3bake::HouseholdBaked>,
    /// What its Sims are to each other (by Sim id; pairs not listed are roommates).
    pub ties: Vec<(u64, u64, crate::family::Tie)>,
}

impl PendingHousehold {
    pub fn random() -> Self {
        let mut rng = rand::rng();
        let last = random_last_name(&mut rng);
        let a = random_sim(&mut rng, &last, Some(true), Age::YoungAdult);
        let b = random_sim(&mut rng, &last, Some(false), Age::YoungAdult);
        Self { last_name: last, members: vec![a, b], premade: None, ties: Vec::new() }
    }
}

pub use crate::cas::CasAction;

// ---------------------------------------------------------------------------------------------
// Choosing a lot


pub fn lot_center(lot: &LotInfo) -> Vec3 {
    let rot = Quat::from_rotation_y(lot.rotation);
    Vec3::from(lot.corner) + rot * Vec3::new(lot.width as f32 * 0.5, 0.0, lot.depth as f32 * 0.5)
}

#[derive(Component)]
pub struct LotButton(pub usize);
#[derive(Component)]
pub struct MoveInButton;
#[derive(Resource, Default)]
pub struct ChosenLot(pub Option<usize>);

/// Who lives where, as the lot chooser sees it: the town's families, those played before, and
/// (moving house) the household itself.
fn lots_lived_in(
    world: &CurrentWorld,
    town: Option<&crate::premade::TownPremades>,
    story: &crate::story::TownStory,
    dormant: &crate::household::Dormant,
    moving: Option<(&Moving, Option<&Household>)>,
) -> std::collections::HashMap<usize, String> {
    let active = moving.and_then(|(_, h)| h.map(|h| h.lot_index));
    crate::edittown::occupants(&world.data, town.map(|t| &*t.0), story, &dormant.0, active)
        .into_iter()
        .map(|(i, o)| {
            let name = match o {
                crate::edittown::Occupant::Active => moving.map_or(String::new(), |m| m.0 .0.household.clone()),
                crate::edittown::Occupant::Played(k) => dormant.0.get(k).map_or(String::new(), |d| d.household.clone()),
                crate::edittown::Occupant::Town(id) => town.and_then(|t| t.0.households.iter().find(|h| h.id == id)).map_or(String::new(), |h| h.name.clone()),
            };
            (i, name)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn spawn_lot_chooser(
    mut commands: Commands,
    world: Res<CurrentWorld>,
    mut cam: Query<&mut SimsCamera>,
    data: Res<Baked>,
    mut images: ResMut<Assets<Image>>,
    (town, story, dormant, moving, household): (
        Option<Res<crate::premade::TownPremades>>,
        Res<crate::story::TownStory>,
        Res<crate::household::Dormant>,
        Option<Res<Moving>>,
        Option<Res<Household>>,
    ),
) {
    // (Homes lived in are taken.)
    let lived_in = lots_lived_in(&world, town.as_deref(), &story, &dormant, moving.as_deref().map(|m| (m, household.as_deref())));
    commands.insert_resource(ChosenLot(None));
    if let Ok(mut c) = cam.single_mut() {
        c.distance = 420.0;
        c.pitch = 1.05;
    }
    let mut lots: Vec<(usize, String, &LotInfo)> = world
        .data
        .lots
        .iter()
        .enumerate()
        .filter(|(_, l)| l.is_residential())
        .map(|(i, l)| (i, world.data.lot_names.get(i).cloned().unwrap_or_else(|| l.internal_name.clone()), l))
        .collect();
    // Furnished houses first, then empty lots.
    lots.sort_by_key(|l| {
        let b = world.data.buildings.get(&l.0);
        (b.is_some_and(|b| b.is_penthouse()), !b.is_some_and(|b| b.is_furnished()), !b.is_some_and(|b| b.is_house()), l.1.clone())
    });
    commands
        .spawn((
            DespawnOnExit(PlayMode::ChooseLot),
            Node {
                border_radius: BorderRadius::all(Val::Px(12.0)),
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(380.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(crate::menu::PANEL_BG),
        ))
        .with_children(|p| {
            p.spawn(text(format!("{} — choose a home", world.name), 24.0, Color::WHITE));
            p.spawn(text("Pick a residential lot, then press Move In.", 14.0, Color::srgb(0.75, 0.85, 1.0)));
            p.spawn((
                Button,
                MoveInButton,
                Node {
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    height: Val::Px(44.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    margin: UiRect::vertical(Val::Px(6.0)),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(BTN_NORMAL),
            ))
            .with_children(|b| {
                b.spawn(text("Move In", 22.0, PLUMBOB_GREEN));
            });
            for (i, name, lot) in lots {
                let picture = crate::objects::cpu_texture(&data.0, s3bake::lot_thumbnail_key(lot.id)).map(|img| images.add(img));
                let kind = match world.data.buildings.get(&i).filter(|b| b.is_house()) {
                    Some(b) => {
                        let floors = b.floors.iter().filter(|f| f.level > 0).map(|f| f.level).collect::<std::collections::BTreeSet<_>>().len().max(1);
                        let what = if b.is_penthouse() {
                            "Penthouse (not yet playable)"
                        } else if b.is_furnished() {
                            "Furnished house"
                        } else {
                            "Unfurnished house"
                        };
                        format!("{what} · {floors} floor{}", if floors > 1 { "s" } else { "" })
                    }
                    None => "Empty lot".to_string(),
                };
                let kind = match lived_in.get(&i) {
                    Some(who) => format!("{kind} · home of the {who} household"),
                    None => kind,
                };
                p.spawn((
                    Button,
                    LotButton(i),
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    if let Some(img) = picture {
                        b.spawn((ImageNode::new(img), Node { width: Val::Px(72.0), height: Val::Px(72.0), flex_shrink: 0.0, ..default() }));
                    }
                    b.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                        t.spawn(text(name, 16.0, Color::WHITE));
                        t.spawn(text(format!("{}x{} lot · {kind}", lot.width, lot.depth), 12.0, Color::srgb(0.7, 0.8, 0.9)));
                    });
                });
            }
        });
}

/// Asked (by phone) to move house: the game is saved first.
#[derive(Resource)]
pub struct MoveRequested {
    pub at: f64,
}

/// Moving house: the game as saved, waiting for the new lot to be chosen.
#[derive(Resource)]
pub struct Moving(pub crate::save::SaveGame);

/// A move asked for: save, then choose the new home.
pub fn start_move(
    mut commands: Commands,
    req: Option<Res<MoveRequested>>,
    last: Option<Res<crate::save::LastSave>>,
    mut asked: Local<bool>,
    mut save: MessageWriter<crate::save::SaveRequest>,
    mut next: ResMut<NextState<PlayMode>>,
) {
    let Some(req) = req else {
        *asked = false;
        return;
    };
    if !*asked {
        *asked = true;
        save.write(crate::save::SaveRequest);
        return;
    }
    if let Some(l) = last.filter(|l| l.0.minutes >= req.at) {
        commands.remove_resource::<MoveRequested>();
        commands.insert_resource(Moving(l.0.clone()));
        *asked = false;
        next.set(PlayMode::ChooseLot);
    }
}

#[allow(clippy::too_many_arguments)]
fn lot_buttons(
    q: Query<(&Interaction, &LotButton), Changed<Interaction>>,
    move_in: Query<&Interaction, (Changed<Interaction>, With<MoveInButton>)>,
    mut chosen: ResMut<ChosenLot>,
    world: Res<CurrentWorld>,
    mut cam: Query<&mut SimsCamera>,
    mut next: ResMut<NextState<PlayMode>>,
    mut commands: Commands,
    (moving, worlds, catalog, mut app, slot): (Option<Res<Moving>>, Res<crate::data::WorldList>, Res<Catalog>, ResMut<NextState<AppState>>, Res<crate::save::SaveSlot>),
    (town, story, dormant, household, mut notes): (
        Option<Res<crate::premade::TownPremades>>,
        Res<crate::story::TownStory>,
        Res<crate::household::Dormant>,
        Option<Res<Household>>,
        ResMut<Notifications>,
    ),
) {
    for (i, b) in &q {
        if *i == Interaction::Pressed {
            chosen.0 = Some(b.0);
            let lot = &world.data.lots[b.0];
            if let Ok(mut c) = cam.single_mut() {
                c.look_at(lot_center(lot));
                c.distance = (lot.width.max(lot.depth) as f32 * 1.4).max(40.0);
                c.pitch = 0.8;
            }
        }
    }
    for i in &move_in {
        if *i == Interaction::Pressed
            && let Some(l) = chosen.0
        {
            // (Not into someone else's home: evict them in Edit Town first.)
            let lived_in = lots_lived_in(&world, town.as_deref(), &story, &dormant, moving.as_deref().map(|m| (m, household.as_deref())));
            if let Some(who) = lived_in.get(&l) {
                notes.push(format!("That's the {who} household's home. (In Edit Town they can be moved out.)"));
                continue;
            }
            // Moving house: the save, rewritten for the new home, is loaded there.
            if let Some(m) = &moving {
                let mut g = m.0.clone();
                g.lot_index = l;
                g.lot_name = world.data.lot_names.get(l).cloned().unwrap_or_default();
                // The old home's furniture is sold (for four-fifths of what it cost), and its
                // walls, garden and graves stay behind.
                let refund: i64 = g.bought.iter().filter_map(|o| catalog.by_key(&o.objd)).map(|e| e.price.max(0) as i64 * 4 / 5).sum();
                g.funds += refund;
                g.bought.clear();
                g.removed.clear();
                g.paint.clear();
                g.terrain.clear();
                g.heights.clear();
                g.plants.clear();
                g.graves.clear();
                let c = lot_center(&world.data.lots[l]);
                let y = world.data.heightmap.sample(c.x, c.z);
                for s in g.sims.iter_mut().filter(|s| s.member) {
                    s.position = [c.x, y, c.z];
                    s.floor = 1;
                    s.whereabouts = "home".into();
                }
                info!("moving the {} household to lot {l} ({}); furniture sold for §{refund}", g.household, g.lot_name);
                commands.remove_resource::<Moving>();
                // (Into the same file: it's the same game.)
                if crate::save::begin_load(&mut commands, &worlds, g, slot.0.clone()) {
                    app.set(AppState::Loading);
                }
                return;
            }
            commands.insert_resource(MoveInRequest(l));
            next.set(PlayMode::Live);
        }
    }
}

/// How far a sidewalk runs from the middle of its road.
const SIDEWALK_OFFSET: f32 = 5.0;

/// The sidewalk of the road nearest `near` (within 40 m), on `near`'s side of it: the road's
/// point closest by, its direction there, and `half_length` each way.
fn street_sidewalk(curves: &[[[f32; 2]; 4]], near: Vec2, half_length: f32) -> Option<crate::town::Sidewalk> {
    let at = |c: &[[f32; 2]; 4], t: f32| {
        let p = c.map(Vec2::from);
        let u = 1.0 - t;
        p[0] * u * u * u + p[1] * 3.0 * u * u * t + p[2] * 3.0 * u * t * t + p[3] * t * t * t
    };
    let (c, t, d) = curves
        .iter()
        .flat_map(|c| (0..=32).map(move |i| (c, i as f32 / 32.0)))
        .map(|(c, t)| (c, t, at(c, t).distance(near)))
        .min_by(|a, b| a.2.total_cmp(&b.2))?;
    if d > 40.0 {
        return None;
    }
    let p = at(c, t);
    let along = (at(c, (t + 0.02).min(1.0)) - at(c, (t - 0.02).max(0.0))).normalize_or_zero();
    if along == Vec2::ZERO {
        return None;
    }
    let side = if along.perp_dot(near - p) >= 0.0 { along.perp() } else { -along.perp() };
    Some(crate::town::Sidewalk { center: p + side * SIDEWALK_OFFSET, along, half_length })
}

/// `--lot <name>` / `--world` automation: pick a lot and move in immediately.
fn auto_move_in(
    args: Res<crate::autotest::AutoArgs>,
    world: Res<CurrentWorld>,
    pending: Option<Res<PendingHousehold>>,
    mut commands: Commands,
    mut next: ResMut<NextState<PlayMode>>,
    mut done: Local<bool>,
    loading_save: Option<Res<crate::save::PendingLoad>>,
) {
    if *done || args.world.is_none() || pending.is_some_and(|p| p.premade.is_some()) || loading_save.is_some() {
        return;
    }
    *done = true;
    let want = args.lot.clone().unwrap_or_else(|| "empty".into()).to_ascii_lowercase();
    let idx = if want == "house" {
        // The first residential lot with a pre-built house.
        world.data.lots.iter().enumerate().position(|(i, l)| l.is_residential() && world.data.buildings.get(&i).is_some_and(|b| b.is_furnished()))
    } else {
        world
            .data
            .lots
            .iter()
            .position(|l| l.internal_name.to_ascii_lowercase().contains(&want) || format!("{:016x}", l.id).contains(&want))
    }
    .or_else(|| world.data.lots.iter().position(|l| l.is_residential()));
    if let Some(i) = idx {
        info!("moving into {} ({})", world.data.lots[i].internal_name, world.data.lot_names[i]);
        commands.insert_resource(MoveInRequest(i));
        next.set(PlayMode::Live);
    }
}

/// A town family moves straight into their own home.
fn premade_move_in(
    pending: Option<Res<PendingHousehold>>,
    world: Res<CurrentWorld>,
    story: Res<crate::story::TownStory>,
    loading_save: Option<Res<crate::save::PendingLoad>>,
    mut commands: Commands,
    mut next: ResMut<NextState<PlayMode>>,
    moving: Option<Res<Moving>>,
) {
    let Some(h) = pending.as_ref().and_then(|p| p.premade.as_ref()) else { return };
    // (Not when they're choosing a new home.)
    if loading_save.is_some() || moving.is_some() {
        return;
    }
    // (Their home: the world's, or where they've moved since.)
    let home = story.home_of(h);
    if let Some(i) = world.data.lots.iter().position(|l| Some(l.id) == home) {
        commands.insert_resource(crate::premade::PremadeChoice(h.clone()));
        commands.insert_resource(MoveInRequest(i));
        next.set(PlayMode::Live);
    }
}

fn lot_corners(lot: &LotInfo) -> [Vec3; 4] {
    let rot = Quat::from_rotation_y(lot.rotation);
    let c = Vec3::from(lot.corner);
    let (w, d) = (lot.width as f32, lot.depth as f32);
    [c, c + rot * Vec3::new(w, 0.0, 0.0), c + rot * Vec3::new(w, 0.0, d), c + rot * Vec3::new(0.0, 0.0, d)]
}

fn draw_lots(mut gizmos: Gizmos, world: Res<CurrentWorld>, chosen: Res<ChosenLot>) {
    for (i, lot) in world.data.lots.iter().enumerate() {
        let corners = lot_corners(lot);
        let color = if chosen.0 == Some(i) {
            Color::srgb(1.0, 1.0, 0.2)
        } else if lot.is_residential() {
            Color::srgb(0.3, 1.0, 0.3)
        } else {
            Color::srgb(0.3, 0.6, 1.0)
        };
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            let a = a + Vec3::Y * 0.6;
            let b = b + Vec3::Y * 0.6;
            gizmos.line(a, b, color);
        }
    }
}

fn draw_home_lot(mut gizmos: Gizmos, world: Res<CurrentWorld>, household: Option<Res<Household>>) {
    let Some(h) = household else { return };
    let Some(lot) = world.data.lots.get(h.lot_index) else { return };
    let corners = lot_corners(lot);
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        let hm = &world.data.heightmap;
        let n = 24;
        for s in 0..n {
            let p0 = a.lerp(b, s as f32 / n as f32);
            let p1 = a.lerp(b, (s + 1) as f32 / n as f32);
            gizmos.line(
                Vec3::new(p0.x, hm.sample(p0.x, p0.z) + 0.05, p0.z),
                Vec3::new(p1.x, hm.sample(p1.x, p1.z) + 0.05, p1.z),
                Color::srgba(1.0, 1.0, 1.0, 0.35),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Moving in

#[derive(Resource)]
pub struct MoveInRequest(pub usize);

/// Starter furniture: (OBJD instance, local x, local z, yaw degrees). Local origin = lot centre.
/// The starter furniture an empty lot is moved into with (as `move_in` sets it out): each
/// piece's key, where it stands (on the ground; the computer on its desk) and its turn.
pub fn starter_furniture(lot: &LotInfo, hm: &s3formats::world::Heightmap) -> Vec<(Key, Vec3, Quat)> {
    let rot = Quat::from_rotation_y(lot.rotation);
    let center = lot_center(lot);
    // (On the deck.)
    let (top, _) = crate::building::deck_heights(lot, hm);
    STARTER
        .iter()
        .map(|&(inst, x, z, yaw)| {
            let p = center + rot * Vec3::new(x, 0.0, z);
            let desk = if inst == 0x369 { 0.75 } else { 0.0 };
            ((s3pkg::types::OBJD, 0, inst), Vec3::new(p.x, top + desk, p.z), Quat::from_rotation_y(lot.rotation + yaw.to_radians()))
        })
        .collect()
}

const STARTER: [(u64, f32, f32, f32); 22] = [
    (0x5BE, -6.6, -3.2, 0.0),   // fridge
    (0x919, -5.4, -3.2, 0.0),   // stove
    (0x358, -5.4, 0.2, 0.0),    // dining table
    (0x364, -5.4, -0.9, 0.0),   // dining chair
    (0x364, -5.4, 1.3, 180.0),  // dining chair
    (0x37E, 0.3, 2.6, 180.0),   // sofa
    (0x5CA, 0.3, -0.6, 0.0),    // TV
    (0x367, 2.6, -3.4, 0.0),    // bookshelf
    (0x49B, 1.3, -3.4, 0.0),    // stereo
    (0x5C2, 3.6, 1.6, 0.0),     // desk table
    (0x364, 3.6, 2.6, 180.0),   // desk chair
    (0x360, 6.2, -1.9, 0.0),    // double bed
    (0x3EB, 6.4, 2.6, 180.0),   // single bed
    (0x35A, -1.6, -5.6, 0.0),   // toilet
    (0x489, 0.4, -5.8, 0.0),    // shower
    (0x3F6, 2.2, -5.6, 0.0),    // sink
    (0x3E6, 3.6, -5.7, 0.0),    // mirror
    (0x624, -6.4, 4.6, 180.0),  // easel
    (0x5E4, -3.0, 5.4, 0.0),    // chess
    (0x626, 2.0, 5.4, 180.0),   // guitar
    (0x6B0, 5.4, 5.6, 180.0),   // treadmill
    (0x369, 3.6, 1.6, 180.0),   // computer (on the desk)
];

pub struct SpawnedObject {
    pub entity: Entity,
}

/// Spawns a catalog object with gameplay components at a world position.
#[allow(clippy::too_many_arguments)]
pub fn spawn_game_object(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    objd: Key,
    pos: Vec3,
    yaw: f32,
) -> Option<SpawnedObject> {
    spawn_game_object_rot(commands, assets, ctx, catalog, objd, pos, Quat::from_rotation_y(yaw))
}

/// [`spawn_game_object`] with a full rotation.
#[allow(clippy::too_many_arguments)]
pub fn spawn_game_object_rot(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    objd: Key,
    pos: Vec3,
    rotation: Quat,
) -> Option<SpawnedObject> {
    spawn_game_object_design(commands, assets, ctx, catalog, objd, pos, rotation, None)
}

/// [`spawn_game_object_rot`] in a design (`None`: as the game ships it).
#[allow(clippy::too_many_arguments)]
pub fn spawn_game_object_design(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    objd: Key,
    pos: Vec3,
    rotation: Quat,
    design: Option<Key>,
) -> Option<SpawnedObject> {
    let asked = design;
    let design = design.filter(|d| assets.design_applies(ctx, objd, *d));
    let parts = assets.object_design(ctx, objd, design);
    let entry = catalog.by_key(&objd);
    // (DESIGN_LOG=1: each object put down, the design asked for and the one drawn.)
    if std::env::var("DESIGN_LOG").is_ok() {
        info!("DESIGN_LOG {} {:?} asked {:?} designs-known {} design {:?}", entry.map_or("?", |e| e.name.as_str()), objd, asked, ctx.baked.designs.contains_key(&objd), design);
    }
    let (name, price, kind) = entry
        .map(|e| (e.name.clone(), e.price, e.kind))
        .unwrap_or_else(|| ("Object".into(), 0, ObjectKind::Other));
    // (A sprinkler's model holds the dome of its spray: it stands where its solid parts are.)
    let solid: Vec<_> = parts.iter().filter(|p| kind != ObjectKind::Sprinkler || p.mode == 0).cloned().collect();
    let (mn, mx) = parts_bounds(if solid.is_empty() { &parts } else { &solid })?;
    // (Rugs and other flat things lie just over the floor, which is drawn a whisker above its
    // level.)
    let lift = if mx.y - mn.y < 0.03 { 0.02 } else { 0.0 };
    let tf = Transform::from_translation(pos + Vec3::Y * lift).with_rotation(rotation);
    let e = spawn_parts(commands, &parts, tf);
    let center = Vec2::new((mn.x + mx.x) * 0.5, (mn.z + mx.z) * 0.5);
    let half = Vec2::new((mx.x - mn.x) * 0.5, (mx.z - mn.z) * 0.5);
    commands.entity(e).insert((
        GameObject { kind, name, objd, price, center, half, height: mx.y },
        UsedBy::default(),
        DespawnOnExit(AppState::InGame),
    ));
    if let Some(d) = design {
        commands.entity(e).insert(crate::objects::Design(d));
    }
    if entry.is_some_and(|c| c.shell) {
        commands.entity(e).insert(crate::building::Shell);
    }
    // Small decorations don't block walking.
    if half.x * half.y > 0.04 && mx.y > 0.25 && !matches!(kind, ObjectKind::Light) {
        commands.entity(e).insert(Obstacle { half, center_offset: center });
    }
    Some(SpawnedObject { entity: e })
}

#[allow(clippy::too_many_arguments)]
pub fn move_in(
    mut commands: Commands,
    request: Option<Res<MoveInRequest>>,
    pending: Option<Res<PendingHousehold>>,
    world: Res<CurrentWorld>,
    data: Res<Baked>,
    catalog: Res<Catalog>,
    mut assets: ResMut<ObjectAssets>,
    sim_assets: Res<SimAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut cam: Query<&mut SimsCamera>,
    mut notes: ResMut<Notifications>,
    imposters: Query<(Entity, &crate::world::LotImposter)>,
    (mut bindposes, mut prepared, mut skin_mats, mut sim_tex): (
        ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
        Option<ResMut<crate::simbody::PreparedSims>>,
        ResMut<Assets<crate::simbody::SimSkinMaterial>>,
        ResMut<crate::simbody::SimTextures>,
    ),
    (mut life, loading_save): (MessageWriter<crate::life::LifeEvent>, Option<Res<crate::save::PendingLoad>>),
) {
    let Some(req) = request else { return };
    let fresh = loading_save.is_none();
    let lot_index = req.0;
    let house = world.data.buildings.get(&lot_index).filter(|b| b.is_house());
    // A pre-built house keeps its imposter for the distant view; an empty lot loses it.
    if house.is_none() {
        for (e, imp) in &imposters {
            if imp.0 == lot_index {
                commands.entity(e).despawn();
            }
        }
    }
    commands.remove_resource::<MoveInRequest>();
    if fresh {
        commands.insert_resource(crate::building::LotPaint::default());
        commands.insert_resource(crate::gardening::Garden::default());
        commands.insert_resource(crate::gardening::StarterSeeds);
    }
    let lot = world.data.lots[lot_index].clone();
    let rot = Quat::from_rotation_y(lot.rotation);
    let center = lot_center(&lot);

    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let mut furniture_value = 0;
    // An empty lot gets an empty building to build on: the wooden deck they move in on.
    let empty = crate::building::deck_building(lot_index, &lot, &world.data.heightmap);
    let mut building = Some(crate::building::spawn_building(&mut commands, &mut assets, &mut ctx, &catalog, house.unwrap_or(&empty), &lot, None, false));
    // (Staircases built where the house has none of its own are in the town's usual style.)
    if let Some(b) = building.as_mut().filter(|b| b.stair_style.is_none()) {
        b.stair_style = crate::building::town_stair_style(&world.data);
    }
    let to_world = |x: f32, z: f32| {
        let p = center + rot * Vec3::new(x, 0.0, z);
        Vec3::new(p.x, crate::building::walk_height(&world.data, building.as_ref(), p), p.z)
    };
    if let Some(b) = house {
        for o in &b.objects {
            if let Some(e) = catalog.by_key(&o.objd) {
                furniture_value += e.price.max(0);
            }
        }
    } else {
        let mut desk_top = 0.0;
        for (inst, x, z, yaw) in STARTER {
            let key: Key = (s3pkg::types::OBJD, 0, inst);
            let mut pos = to_world(x, z);
            if inst == 0x369 {
                pos.y += desk_top;
            }
            if spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, pos, lot.rotation + yaw.to_radians()).is_some() {
                if let Some(e) = catalog.by_key(&key) {
                    furniture_value += e.price;
                }
                if inst == 0x5C2 {
                    desk_top = assets.object(&mut ctx, key).iter().map(|p| p.bounds.1.y).fold(0.0, f32::max);
                }
            }
        }
    }
    let dc = building.as_ref().map(|b| Vec3::new(b.center.x, b.levels[1], b.center.z)).unwrap_or_else(|| to_world(0.0, 0.0));
    // Sims arrive at the front of the lot (by the road) when there's a house in the way.
    let arrive_z = if house.is_some() { -(lot.depth as f32) * 0.5 + 2.0 } else { 0.9 };
    let pending = pending.map(|p| p.clone()).unwrap_or_else(PendingHousehold::random);
    let starting_funds = if house.is_some() { 20000 - furniture_value as i64 / 4 } else { 20000 - furniture_value as i64 / 2 };
    let starting_funds = pending.premade.as_ref().map_or(starting_funds.max(5000), |h| h.funds.max(0));
    commands.insert_resource(Household {
        name: pending.last_name.clone(),
        funds: starting_funds,
        lot_index,
        last_bill_day: 0,
        bills: Vec::new(),
    });
    let exit = to_world(0.0, -(lot.depth as f32) * 0.5 - 2.0);
    commands.insert_resource(LotExit(Vec2::new(exit.x, exit.z)));
    // The sidewalk along the street in front of the lot: beside the nearest road (or, with none
    // near, along the lot's front edge).
    let walk_center = to_world(0.0, -(lot.depth as f32) * 0.5 - 2.5);
    let along = (rot * Vec3::X).xz().normalize_or(Vec2::X);
    let sidewalk = street_sidewalk(&world.data.road_curves, exit.xz(), lot.width as f32 * 0.5 + 25.0)
        .unwrap_or(crate::town::Sidewalk { center: walk_center.xz(), along, half_length: lot.width as f32 * 0.5 + 25.0 });
    commands.insert_resource(sidewalk);
    commands.insert_resource(NavGrid::new(
        Vec2::new(center.x, center.z),
        lot.width.max(lot.depth) as f32 * 0.5 + 14.0,
    ));

    let mut rng = rand::rng();
    let mut first = None;
    let townies = prepared.as_mut().map(|p| std::mem::take(&mut p.townies)).unwrap_or_default();
    let (members, neighbors) = match prepared.as_mut() {
        Some(p) if !p.members.is_empty() => (std::mem::take(&mut p.members), std::mem::take(&mut p.neighbors)),
        _ => {
            let n: Vec<(Sim, Option<crate::simbody::SimModelCpu>)> = pending.members.iter().map(|s| (s.clone(), None)).collect();
            let nb = (0..2)
                .map(|k| {
                    let last = random_last_name(&mut rng);
                    (random_sim(&mut rng, &last, None, if k == 0 { Age::Adult } else { Age::YoungAdult }), None)
                })
                .collect();
            (n, nb)
        }
    };
    let mut sctx = SimSpawnCtx {
        assets: &sim_assets,
        render: crate::simbody::SimRenderCtx {
            meshes: &mut meshes,
            images: &mut images,
            mats: &mut mats,
            skin_mats: &mut skin_mats,
            bindposes: &mut bindposes,
            textures: &mut sim_tex,
        },
    };
    let n = members.len();
    for (i, (s, model)) in members.into_iter().enumerate() {
        let p = to_world(-1.0 + (i as f32 - n as f32 * 0.5) * 0.9, arrive_z);
        let e = spawn_sim_full(&mut commands, &mut sctx, s, p, model);
        commands.entity(e).insert((
            HouseholdMember,
            ActionQueue::default(),
            AutonomyTimer(rng.random_range(1.0..6.0)),
            Skills::default(),
            DespawnOnExit(AppState::InGame),
        ));
        if first.is_none() {
            first = Some(e);
            commands.entity(e).insert(Selected);
        }
        if fresh {
            life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::MovedIn));
        }
    }
    // A couple of neighbours drop by to say hello (at the front door, a house that has one).
    let front_door = building.as_ref().and_then(|b| b.front_door(exit.xz()));
    for (k, (s, model)) in neighbors.into_iter().enumerate() {
        let p = to_world(-3.0 + k as f32 * 6.0, -(lot.depth as f32) * 0.5 + 1.0);
        let name = s.full_name();
        let e = spawn_sim_full(&mut commands, &mut sctx, s, p, model);
        let mut queue = ActionQueue::default();
        if let (true, Some((door, door_e))) = (fresh, front_door) {
            // (Side by side on the step.)
            let at = door + Vec2::new(k as f32 * 0.8 - 0.4, 0.0);
            crate::doorbell::come_to_door(&mut commands, e, &mut queue, at, door_e.unwrap_or(e));
        }
        commands.entity(e).insert((
            queue,
            AutonomyTimer(rng.random_range(1.0..4.0)),
            Skills::default(),
            Visitor { leave_at: 18.0 * 60.0 + k as f64 * 40.0 },
            DespawnOnExit(AppState::InGame),
        ));
        if fresh {
            notes.push(format!("{name} from next door came over to welcome the {} family.", pending.last_name));
        }
    }
    // Townies, hidden until they stroll by.
    for (k, (s, model)) in townies.into_iter().enumerate() {
        let e = spawn_sim_full(&mut commands, &mut sctx, s, exit, model);
        commands.entity(e).insert((
            Visibility::Hidden,
            crate::town::Townie { next_walk: 8.0 * 60.0 + 5.0 + k as f64 * 37.0, walking: false },
            DespawnOnExit(AppState::InGame),
        ));
    }
    if let Ok(mut c) = cam.single_mut() {
        c.look_at(dc);
        c.distance = 26.0;
        c.pitch = 0.75;
        c.yaw = lot.rotation + 0.6;
    }
    notes.push(format!("Welcome home, {} family! You have §{starting_funds} to spend.", pending.last_name));
    match building {
        Some(b) => commands.insert_resource(b),
        None => commands.remove_resource::<crate::building::ActiveBuilding>(),
    }
}
