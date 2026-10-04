//! Creating a household, choosing a lot in the world and moving in.

use bevy::prelude::*;
use rand::Rng;
use s3formats::world::LotInfo;
use s3pkg::{ResourceKey, types};

use crate::camera::SimsCamera;
use crate::data::GameData;
use crate::interact::*;
use crate::loading::{Catalog, CurrentWorld, Strings};
use crate::menu::{BTN_NORMAL, PLUMBOB_GREEN, button_visuals, text};
use crate::nav::{NavGrid, Obstacle};
use crate::objects::{AssetCtx, ObjectAssets, parts_bounds, spawn_parts};
use crate::sim::*;
use crate::{AppState, PlayMode};

pub struct HomePlugin;

impl Plugin for HomePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::CreateHousehold), spawn_cas)
            .add_systems(Update, (button_visuals, cas_buttons).run_if(in_state(AppState::CreateHousehold)))
            .add_systems(OnEnter(PlayMode::ChooseLot), spawn_lot_chooser)
            .add_systems(
                Update,
                (button_visuals, lot_buttons, draw_lots, auto_move_in).run_if(in_state(PlayMode::ChooseLot)),
            )
            .add_systems(Update, draw_home_lot.run_if(in_state(PlayMode::Live)));
    }
}

/// The household being created before the world loads.
#[derive(Resource, Clone)]
pub struct PendingHousehold {
    pub last_name: String,
    pub members: Vec<Sim>,
}

impl PendingHousehold {
    pub fn random() -> Self {
        let mut rng = rand::rng();
        let last = random_last_name(&mut rng);
        let a = random_sim(&mut rng, &last, Some(true), Age::YoungAdult);
        let b = random_sim(&mut rng, &last, Some(false), Age::YoungAdult);
        Self { last_name: last, members: vec![a, b] }
    }
}

// ---------------------------------------------------------------------------------------------
// Create-a-household

#[derive(Component)]
enum CasAction {
    RandomizeAll,
    Randomize(usize),
    Gender(usize),
    Age(usize),
    Remove(usize),
    Add,
    Done,
}

#[derive(Component)]
struct CasRoot;

fn spawn_cas(mut commands: Commands, pending: Option<Res<PendingHousehold>>) {
    if pending.is_none() {
        commands.insert_resource(PendingHousehold::random());
    }
    commands.spawn((Camera2d, DespawnOnExit(AppState::CreateHousehold)));
    rebuild_cas(&mut commands, &pending.map(|p| p.clone()).unwrap_or_else(PendingHousehold::random));
}

fn rebuild_cas(commands: &mut Commands, h: &PendingHousehold) {
    commands.spawn((
        CasRoot,
        DespawnOnExit(AppState::CreateHousehold),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.05, 0.16, 0.30)),
        children![],
    ));
    let _ = h;
}

fn btn(p: &mut ChildSpawnerCommands, label: &str, action: CasAction, w: f32) {
    p.spawn((
        Button,
        action,
        Node {
            border_radius: BorderRadius::all(Val::Px(8.0)),
            width: Val::Px(w),
            height: Val::Px(36.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BTN_NORMAL),
    ))
    .with_children(|b| {
        b.spawn(text(label, 17.0, Color::WHITE));
    });
}

fn age_name(a: Age) -> &'static str {
    match a {
        Age::Child => "Child",
        Age::YoungAdult => "Young Adult",
        Age::Adult => "Adult",
        Age::Elder => "Elder",
    }
}

fn fill_cas(commands: &mut Commands, root: Entity, h: &PendingHousehold) {
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        p.spawn(text("Create a Household", 48.0, Color::WHITE));
        p.spawn(text(format!("The {} Household", h.last_name), 28.0, PLUMBOB_GREEN));
        for (i, s) in h.members.iter().enumerate() {
            p.spawn((
                Node {
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
            ))
            .with_children(|row| {
                row.spawn((Node { width: Val::Px(26.0), height: Val::Px(26.0), border_radius: BorderRadius::all(Val::Px(13.0)), ..default() }, BackgroundColor(s.skin)));
                row.spawn((Node { width: Val::Px(18.0), height: Val::Px(26.0), ..default() }, BackgroundColor(s.hair)));
                row.spawn((Node { width: Val::Px(18.0), height: Val::Px(26.0), ..default() }, BackgroundColor(s.top)));
                row.spawn((
                    text(
                        format!("{} {} · {} {}", s.first, s.last, if s.female { "Female" } else { "Male" }, age_name(s.age)),
                        20.0,
                        Color::WHITE,
                    ),
                    Node { width: Val::Px(380.0), ..default() },
                ));
                btn(row, "Randomize", CasAction::Randomize(i), 120.0);
                btn(row, "Gender", CasAction::Gender(i), 90.0);
                btn(row, "Age", CasAction::Age(i), 70.0);
                btn(row, "Remove", CasAction::Remove(i), 90.0);
            });
        }
        p.spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(12.0)), ..default() }).with_children(|row| {
            btn(row, "Add Sim", CasAction::Add, 140.0);
            btn(row, "New Family", CasAction::RandomizeAll, 160.0);
            btn(row, "Done", CasAction::Done, 140.0);
        });
    });
}

fn cas_buttons(
    mut commands: Commands,
    q: Query<(&Interaction, &CasAction), Changed<Interaction>>,
    root: Query<Entity, With<CasRoot>>,
    mut pending: ResMut<PendingHousehold>,
    mut next: ResMut<NextState<AppState>>,
    mut first: Local<bool>,
) {
    let Ok(root) = root.single() else { return };
    if !*first {
        *first = true;
        fill_cas(&mut commands, root, &pending);
    }
    let mut rng = rand::rng();
    let mut changed = false;
    for (i, a) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        changed = true;
        let last = pending.last_name.clone();
        match a {
            CasAction::RandomizeAll => *pending = PendingHousehold::random(),
            CasAction::Randomize(k) => {
                let s = &pending.members[*k];
                let (f, age) = (s.female, s.age);
                pending.members[*k] = random_sim(&mut rng, &last, Some(f), age);
            }
            CasAction::Gender(k) => {
                let s = &pending.members[*k];
                let age = s.age;
                let f = !s.female;
                pending.members[*k] = random_sim(&mut rng, &last, Some(f), age);
            }
            CasAction::Age(k) => {
                let s = &mut pending.members[*k];
                s.age = match s.age {
                    Age::YoungAdult => Age::Adult,
                    Age::Adult => Age::Elder,
                    Age::Elder => Age::Child,
                    Age::Child => Age::YoungAdult,
                };
            }
            CasAction::Remove(k) => {
                if pending.members.len() > 1 {
                    pending.members.remove(*k);
                }
            }
            CasAction::Add => {
                if pending.members.len() < 6 {
                    pending.members.push(random_sim(&mut rng, &last, None, Age::YoungAdult));
                }
            }
            CasAction::Done => {
                next.set(AppState::Loading);
                *first = false;
                return;
            }
        }
    }
    if changed {
        fill_cas(&mut commands, root, &pending);
    }
}

// ---------------------------------------------------------------------------------------------
// Choosing a lot

pub fn lot_display_name(lot: &LotInfo, strings: &Strings) -> String {
    let lookup = |k: &str| strings.0.get(&s3pkg::fnv64(k)).cloned();
    let name = lot.name_key().and_then(lookup);
    let addr = lot.address_key().and_then(lookup);
    match (name, addr) {
        (Some(n), Some(a)) if n != a => format!("{n} — {a}"),
        (Some(n), _) => n,
        (None, Some(a)) => a,
        _ => lot.internal_name.clone(),
    }
}

pub fn lot_center(lot: &LotInfo) -> Vec3 {
    let rot = Quat::from_rotation_y(lot.rotation);
    Vec3::from(lot.corner) + rot * Vec3::new(lot.width as f32 * 0.5, 0.0, lot.depth as f32 * 0.5)
}

#[derive(Component)]
struct LotButton(usize);
#[derive(Component)]
struct MoveInButton;
#[derive(Resource, Default)]
struct ChosenLot(Option<usize>);

fn spawn_lot_chooser(
    mut commands: Commands,
    world: Res<CurrentWorld>,
    strings: Res<Strings>,
    mut cam: Query<&mut SimsCamera>,
) {
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
        .map(|(i, l)| (i, lot_display_name(l, &strings), l))
        .collect();
    lots.sort_by_key(|l| (!l.2.internal_name.to_ascii_lowercase().contains("empty"), l.1.clone()));
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
            BackgroundColor(Color::srgba(0.05, 0.15, 0.30, 0.92)),
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
                p.spawn((
                    Button,
                    LotButton(i),
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text(name, 16.0, Color::WHITE));
                    b.spawn(text(
                        format!("{}x{} lot · {}", lot.width, lot.depth, if lot.internal_name.to_ascii_lowercase().contains("empty") { "Empty lot" } else { "Residential" }),
                        12.0,
                        Color::srgb(0.7, 0.8, 0.9),
                    ));
                });
            }
        });
}

fn lot_buttons(
    q: Query<(&Interaction, &LotButton), Changed<Interaction>>,
    move_in: Query<&Interaction, (Changed<Interaction>, With<MoveInButton>)>,
    mut chosen: ResMut<ChosenLot>,
    world: Res<CurrentWorld>,
    mut cam: Query<&mut SimsCamera>,
    mut next: ResMut<NextState<PlayMode>>,
    mut commands: Commands,
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
            commands.insert_resource(MoveInRequest(l));
            next.set(PlayMode::Live);
        }
    }
}

/// `--lot <name>` / `--world` automation: pick a lot and move in immediately.
fn auto_move_in(
    args: Res<crate::autotest::AutoArgs>,
    world: Res<CurrentWorld>,
    mut commands: Commands,
    mut next: ResMut<NextState<PlayMode>>,
    mut done: Local<bool>,
) {
    if *done || args.world.is_none() {
        return;
    }
    *done = true;
    let want = args.lot.clone().unwrap_or_else(|| "empty".into()).to_ascii_lowercase();
    let idx = world
        .data
        .lots
        .iter()
        .position(|l| l.internal_name.to_ascii_lowercase().contains(&want))
        .or_else(|| world.data.lots.iter().position(|l| l.is_residential()));
    if let Some(i) = idx {
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
    objd: ResourceKey,
    pos: Vec3,
    yaw: f32,
) -> Option<SpawnedObject> {
    let parts = assets.object(ctx, objd);
    let (mn, mx) = parts_bounds(&parts)?;
    let entry = catalog.by_key(&objd);
    let (name, price, kind) = entry
        .map(|e| (e.name.clone(), e.price, e.kind))
        .unwrap_or_else(|| ("Object".into(), 0, ObjectKind::Other));
    let tf = Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw));
    let e = spawn_parts(commands, &parts, tf);
    let center = Vec2::new((mn.x + mx.x) * 0.5, (mn.z + mx.z) * 0.5);
    let half = Vec2::new((mx.x - mn.x) * 0.5, (mx.z - mn.z) * 0.5);
    commands.entity(e).insert((
        GameObject { kind, name, objd, price, center, half, height: mx.y },
        UsedBy::default(),
        DespawnOnExit(AppState::InGame),
    ));
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
    data: Res<GameData>,
    catalog: Res<Catalog>,
    mut assets: ResMut<ObjectAssets>,
    sim_assets: Res<SimAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut cam: Query<&mut SimsCamera>,
    mut notes: ResMut<Notifications>,
    imposters: Query<(Entity, &crate::world::LotImposter)>,
) {
    let Some(req) = request else { return };
    let lot_index = req.0;
    for (e, imp) in &imposters {
        if imp.0 == lot_index {
            commands.entity(e).despawn();
        }
    }
    commands.remove_resource::<MoveInRequest>();
    let lot = world.data.lots[lot_index].clone();
    let hm = &world.data.heightmap;
    let rot = Quat::from_rotation_y(lot.rotation);
    let center = lot_center(&lot);
    let to_world = |x: f32, z: f32| {
        let p = center + rot * Vec3::new(x, 0.0, z);
        Vec3::new(p.x, hm.sample(p.x, p.z), p.z)
    };

    let mut ctx = AssetCtx { pkgs: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let mut furniture_value = 0;
    let mut desk_top = 0.0;
    for (inst, x, z, yaw) in STARTER {
        let key = ResourceKey::new(types::OBJD, 0, inst);
        let mut pos = to_world(x, z);
        if inst == 0x369 {
            pos.y += desk_top;
        }
        if let Some(o) = spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, pos, lot.rotation + yaw.to_radians()) {
            let _ = o;
            if let Some(e) = catalog.by_key(&key) {
                furniture_value += e.price;
            }
            if inst == 0x5C2 {
                desk_top = assets.object(&mut ctx, key).iter().map(|p| p.bounds.1.y).fold(0.0, f32::max);
            }
        }
    }
    // A wooden deck under the open-plan home.
    let deck = meshes.add(Cuboid::new(17.0, 0.12, 14.0));
    let deck_mat = mats.add(StandardMaterial { base_color: Color::srgb(0.55, 0.40, 0.26), perceptual_roughness: 0.8, ..default() });
    let dc = to_world(0.0, 0.0);
    commands.spawn((
        Mesh3d(deck),
        MeshMaterial3d(deck_mat),
        Transform::from_translation(dc - Vec3::Y * 0.055).with_rotation(rot),
        DespawnOnExit(AppState::InGame),
    ));

    let pending = pending.map(|p| p.clone()).unwrap_or_else(PendingHousehold::random);
    let starting_funds = 20000 - furniture_value as i64 / 2;
    commands.insert_resource(Household {
        name: pending.last_name.clone(),
        funds: starting_funds.max(5000),
        lot_index,
        last_bill_day: 0,
    });
    let exit = to_world(0.0, -(lot.depth as f32) * 0.5 - 2.0);
    commands.insert_resource(LotExit(Vec2::new(exit.x, exit.z)));
    commands.insert_resource(NavGrid::new(
        Vec2::new(center.x, center.z),
        lot.width.max(lot.depth) as f32 * 0.5 + 14.0,
    ));

    let mut rng = rand::rng();
    let mut first = None;
    let n = pending.members.len();
    for (i, s) in pending.members.iter().enumerate() {
        let p = to_world(-1.0 + (i as f32 - n as f32 * 0.5) * 0.9, 0.9);
        let e = spawn_sim(&mut commands, &sim_assets, &mut mats, s.clone(), p);
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
    }
    // A couple of neighbours drop by to say hello.
    for k in 0..2 {
        let last = random_last_name(&mut rng);
        let s = random_sim(&mut rng, &last, None, if k == 0 { Age::Adult } else { Age::YoungAdult });
        let p = to_world(-3.0 + k as f32 * 6.0, -(lot.depth as f32) * 0.5 + 1.0);
        let e = spawn_sim(&mut commands, &sim_assets, &mut mats, s.clone(), p);
        commands.entity(e).insert((
            ActionQueue::default(),
            AutonomyTimer(rng.random_range(1.0..4.0)),
            Skills::default(),
            DespawnOnExit(AppState::InGame),
        ));
        notes.push(format!("{} from next door came over to welcome the {}s.", s.full_name(), pending.last_name));
    }
    if let Ok(mut c) = cam.single_mut() {
        c.look_at(dc);
        c.distance = 26.0;
        c.pitch = 0.75;
        c.yaw = lot.rotation + 0.6;
    }
    notes.push(format!("Welcome home, {} family! You have §{} to spend.", pending.last_name, starting_funds.max(5000)));
}
