//! Collecting: the gems, metals and space rocks the world's spawners leave lying about community
//! lots, and the fish in their ponds. Finds go into the household's collection (a journal of all
//! that's been found, and what's still held, worth money when sold).

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use rand::Rng;
use s3bake::gamedata::{CollectKind, CollectibleInfo};
use serde::{Deserialize, Serialize};

use crate::baked::Baked;
use crate::hud::BlocksWorld;
use crate::interact::{GameObject, Household, Notifications, ObjectKind, UsedBy};
use crate::loading::{Catalog, CurrentWorld};
use crate::menu::{BTN_NORMAL, text};
use crate::nav::{Floor, Obstacle};
use crate::objects::{AssetCtx, ObjectAssets};
use crate::sim::Sim;
use crate::visit::{LotObject, VisitedLot};
use crate::{AppState, PlayMode};

pub struct CollectingPlugin;

impl Plugin for CollectingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Collection>()
            .init_resource::<LotFinds>()
            .init_resource::<JournalPanel>()
            .add_systems(Update, (lot_finds, collect_requests, toggle_journal, journal_panel, sell_button).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(PlayMode::Live), |mut p: ResMut<JournalPanel>| *p = JournalPanel::default());
    }
}

/// Everything the household has found, and what they still hold (count and total worth).
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct Collection {
    pub found: BTreeSet<String>,
    pub held: BTreeMap<String, (u32, i64)>,
}

impl Collection {
    fn add(&mut self, key: &str, value: i64) -> bool {
        let new = self.found.insert(key.to_string());
        let h = self.held.entry(key.to_string()).or_default();
        h.0 += 1;
        h.1 += value;
        new
    }
}

/// A find lying on the ground, to collect.
#[derive(Component)]
pub struct Pickup {
    pub key: String,
}

/// Where fish bite on a lot (by a world builder's fishing spawner).
#[derive(Component)]
pub struct FishingSpot {
    pub class: String,
}

/// A Sim's finished collecting: a pick-up, or a spell of fishing at a spot.
#[derive(Component)]
pub enum CollectRequest {
    Pick(Entity),
    Fished { spot: Entity, minutes: f32 },
}

/// A rock spawner of the lot being visited, and what it's put out.
struct Spawner {
    class: String,
    at: Vec3,
    next: f64,
    out: Vec<Entity>,
}

/// The finds and fishing spots of the community lot being visited.
#[derive(Resource, Default)]
struct LotFinds {
    lot: Option<usize>,
    spawners: Vec<Spawner>,
    spots: Vec<Entity>,
}

/// How far from its spawner a find turns up (metres).
const SPREAD: f32 = 4.0;

fn info<'a>(ui: &'a crate::icons::GameUi, key: &str) -> Option<&'a CollectibleInfo> {
    ui.data.collectibles.iter().find(|c| c.key.eq_ignore_ascii_case(key))
}

fn pick_weighted<'a>(items: &'a [(String, f32)], ok: impl Fn(&str) -> bool) -> Option<&'a str> {
    let total: f32 = items.iter().filter(|(k, _)| ok(k)).map(|(_, w)| *w).sum();
    if total <= 0.0 {
        return None;
    }
    let mut r = rand::rng().random_range(0.0..total);
    for (k, w) in items.iter().filter(|(k, _)| ok(k)) {
        if r < *w {
            return Some(k);
        }
        r -= w;
    }
    None
}

/// A ripple on the water where the fish are, to click.
fn ripple_mesh() -> Mesh {
    Annulus::new(0.35, 0.6).mesh().resolution(24).build().rotated_by(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
}

/// Opens the visited lot's finds: fishing spots at its fishing spawners, and the rock spawners'
/// gems, metals and space rocks (some already lying about, more over time); clears them when
/// the lot closes.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn lot_finds(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    world: Res<CurrentWorld>,
    (data, catalog, mut assets, ui): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>, Option<Res<crate::icons::GameUi>>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    visited: Option<Res<VisitedLot>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut finds: ResMut<LotFinds>,
    alive: Query<(), With<Pickup>>,
) {
    let lot = visited.as_ref().filter(|v| !v.grid.dirty).map(|v| v.lot);
    if finds.lot != lot {
        for s in finds.spawners.drain(..) {
            for e in s.out {
                commands.entity(e).try_despawn();
            }
        }
        for e in finds.spots.drain(..) {
            commands.entity(e).try_despawn();
        }
        finds.lot = lot;
        let (Some(lot), Some(v), Some(ui)) = (lot, visited.as_ref(), ui.as_ref()) else { return };
        let Some(b) = world.data.buildings.get(&lot) else { return };
        let mut rng = rand::rng();
        let ripple = meshes.add(ripple_mesh());
        let water = mats.add(StandardMaterial {
            base_color: Color::srgba(0.85, 0.95, 1.0, 0.55),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        for o in &b.objects {
            let class = o.script.rsplit('.').next().unwrap_or("").to_string();
            let at = Vec3::from(o.position);
            if class.starts_with("FishingSpawner") {
                // The spot faces the shore (its use point): where a Sim can stand nearest.
                let Some((x, z)) = v.grid.nearest_free(at.xz()) else { continue };
                let shore = v.grid.center_of(x, z);
                let to_shore = (shore - at.xz()).normalize_or(Vec2::Y);
                let y = world.data.sea_level.max(world.data.heightmap.sample(at.x, at.z)) + 0.05;
                let e = commands
                    .spawn((
                        Mesh3d(ripple.clone()),
                        MeshMaterial3d(water.clone()),
                        Transform::from_xyz(at.x, y, at.z).with_rotation(Quat::from_rotation_y(to_shore.x.atan2(to_shore.y))),
                        GameObject {
                            kind: ObjectKind::FishingSpot,
                            name: "Fishing Spot".into(),
                            objd: (0, 0, 0),
                            price: 0,
                            center: Vec2::ZERO,
                            half: Vec2::splat(0.3),
                            height: 0.1,
                        },
                        UsedBy::default(),
                        FishingSpot { class },
                        LotObject(lot),
                        Floor(1),
                        DespawnOnExit(AppState::InGame),
                    ))
                    .id();
                finds.spots.push(e);
            } else if class.starts_with("RockGemMetalSpawner") && ui.data.spawners.iter().any(|s| s.class == class) {
                let mut s = Spawner { class, at, next: clock.minutes + rng.random_range(60.0..600.0), out: Vec::new() };
                // Something already lying about, most of the time.
                if rng.random_bool(0.7) {
                    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                    if let Some(e) = spawn_find(&mut commands, &mut assets, &mut ctx, &catalog, &data, ui, &world.data, building.as_deref(), v, &s) {
                        s.out.push(e);
                    }
                }
                finds.spawners.push(s);
            }
        }
        return;
    }
    // More turn up over time.
    let (Some(v), Some(ui)) = (visited.as_ref(), ui.as_ref()) else { return };
    let mut rng = rand::rng();

    for i in 0..finds.spawners.len() {
        finds.spawners[i].out.retain(|e| alive.contains(*e));
        let s = &finds.spawners[i];
        if clock.minutes < s.next {
            continue;
        }
        let Some(info) = ui.data.spawners.iter().find(|x| x.class == s.class) else { continue };
        let (lo, hi) = (info.hours.0.max(1.0), info.hours.1.max(info.hours.0.max(1.0) + 1.0));
        let next = clock.minutes + rng.random_range(lo..hi) as f64 * 60.0;
        let room = s.out.len() < info.capacity as usize;
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let made = if room { spawn_find(&mut commands, &mut assets, &mut ctx, &catalog, &data, ui, &world.data, building.as_deref(), v, s) } else { None };
        let s = &mut finds.spawners[i];
        s.next = next;
        s.out.extend(made);
    }
}

/// Puts out one of a spawner's finds near it, on open ground.
#[allow(clippy::too_many_arguments)]
fn spawn_find(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    data: &Baked,
    ui: &crate::icons::GameUi,
    world: &crate::loading::WorldInfo,
    building: Option<&crate::building::ActiveBuilding>,
    v: &VisitedLot,
    s: &Spawner,
) -> Option<Entity> {
    let spawner = ui.data.spawners.iter().find(|x| x.class == s.class)?;
    let key = pick_weighted(&spawner.items, |k| k != "None" && info(ui, k).is_some())?.to_string();
    let c = info(ui, &key)?;
    let objd = data.0.catalog.iter().find(|o| o.instance_name == c.model).map(|o| o.objd)?;
    let mut rng = rand::rng();
    let spot = (0..12).find_map(|_| {
        let p = s.at.xz() + Vec2::new(rng.random_range(-SPREAD..SPREAD), rng.random_range(-SPREAD..SPREAD));
        v.grid.cell_of(p).filter(|(x, z)| !v.grid.is_blocked(*x, *z)).map(|_| p)
    })?;
    let y = crate::building::walk_height(world, building, Vec3::new(spot.x, 0.0, spot.y));
    let o = crate::home::spawn_game_object(commands, assets, ctx, catalog, objd, Vec3::new(spot.x, y, spot.y), rng.random_range(0.0..std::f32::consts::TAU))?;
    let name = match c.kind {
        CollectKind::Gem => "Gem",
        CollectKind::Metal => "Metal",
        _ => "Space Rock",
    };
    let lot = v.lot;
    commands.entity(o.entity).remove::<Obstacle>().insert((Pickup { key }, LotObject(lot), Floor(1)));
    // (A collectible, whatever its catalogue script.)
    commands.entity(o.entity).queue_silenced(move |mut e: EntityWorldMut| {
        if let Some(mut g) = e.get_mut::<GameObject>() {
            g.kind = ObjectKind::Collectible;
            g.name = name.to_string();
        }
    });
    Some(o.entity)
}

/// What a Sim brings back: the find picked up, or the catch from a spell of fishing.
#[allow(clippy::too_many_arguments)]
fn collect_requests(
    mut commands: Commands,
    q: Query<(Entity, &CollectRequest, &Sim, &crate::interact::Skills)>,
    pickups: Query<&Pickup>,
    spots: Query<&FishingSpot>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut collection: ResMut<Collection>,
    mut notes: ResMut<Notifications>,
    mut life: MessageWriter<crate::life::LifeEvent>,
) {
    let Some(ui) = ui else { return };
    let mut rng = rand::rng();
    for (e, req, sim, skills) in &q {
        commands.entity(e).remove::<CollectRequest>();
        match req {
            CollectRequest::Pick(target) => {
                let Ok(p) = pickups.get(*target) else { continue };
                let Some(c) = info(&ui, &p.key) else { continue };
                let value = rng.random_range(c.min_price..=c.max_price.max(c.min_price)) as i64;
                let new = collection.add(&c.key, value);
                commands.entity(*target).try_despawn();
                // ("found Silver", "found a Ruby".)
                let a = match c.kind {
                    CollectKind::Metal => "",
                    _ if c.name.starts_with(['A', 'E', 'I', 'O', 'U']) => "an ",
                    _ => "a ",
                };
                notes.push(format!(
                    "{} found {a}{} (worth §{value}){}",
                    sim.first,
                    c.name,
                    if new { ". A new find for the collection!" } else { "." }
                ));
                life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::Finished { activity: "Collect", completed: true }));
            }
            CollectRequest::Fished { spot, minutes } => {
                let Ok(s) = spots.get(*spot) else { continue };
                let Some(spawner) = ui.data.spawners.iter().find(|x| x.class == s.class) else { continue };
                let level = skills.level("Fishing") as u8 + if sim.traits.contains(&crate::life::Trait::Angler) { 2 } else { 0 };
                let tries = (minutes / 20.0).floor().max(1.0) as u32;
                let mut caught: Vec<(String, i64)> = Vec::new();
                for _ in 0..tries {
                    if !rng.random_bool((0.3 + level as f64 * 0.05).min(0.85)) {
                        continue;
                    }
                    let ok = |k: &str| info(&ui, k).is_some_and(|c| c.kind == CollectKind::Fish && c.level <= level + 1);
                    let Some(key) = pick_weighted(&spawner.items, ok) else { continue };
                    let Some(c) = info(&ui, key) else { continue };
                    // Bigger fish for better anglers.
                    let t = (level as f32 / 10.0 + rng.random_range(-0.2..0.3)).clamp(0.0, 1.0);
                    let value = (c.min_price as f32 + (c.max_price - c.min_price) as f32 * t).round() as i64;
                    collection.add(&c.key, value);
                    caught.push((c.name.clone(), value));
                }
                if caught.is_empty() {
                    notes.push(format!("{} didn't catch anything.", sim.first));
                } else {
                    let worth: i64 = caught.iter().map(|c| c.1).sum();
                    let best = caught.iter().max_by_key(|c| c.1).map(|c| c.0.clone()).unwrap_or_default();
                    notes.push(format!("{} caught {} fish (the best: {best}), worth §{worth} in all.", sim.first, caught.len()));
                }
            }
        }
    }
}

/// The collection journal: every kind of find, those found so far named (with how many are
/// held and their worth), the rest still a mystery; and selling what's held.
#[derive(Resource, Default)]
pub struct JournalPanel {
    pub open: bool,
    root: Option<Entity>,
    shown: Option<(usize, usize)>,
}

/// The bottom bar's button that opens the journal.
#[derive(Component)]
pub struct JournalButton;

#[derive(Component)]
struct SellButton;

fn toggle_journal(
    mut commands: Commands,
    buttons: Query<&Interaction, (Changed<Interaction>, With<JournalButton>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<JournalPanel>,
) {
    if buttons.iter().any(|i| *i == Interaction::Pressed) || keys.just_pressed(KeyCode::KeyJ) {
        panel.open = !panel.open;
        panel.shown = None;
        if !panel.open
            && let Some(r) = panel.root.take()
        {
            commands.entity(r).despawn();
        }
    }
}

fn journal_panel(mut commands: Commands, mut panel: ResMut<JournalPanel>, collection: Res<Collection>, ui: Option<Res<crate::icons::GameUi>>) {
    if !panel.open {
        return;
    }
    let Some(ui) = ui else { return };
    let held: usize = collection.held.values().map(|h| h.0 as usize).sum();
    let key = (collection.found.len(), held);
    if panel.root.is_some() && panel.shown == Some(key) {
        return;
    }
    panel.shown = Some(key);
    if let Some(r) = panel.root.take() {
        commands.entity(r).despawn();
    }
    let worth: i64 = collection.held.values().map(|h| h.1).sum();
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(520.0),
                bottom: Val::Px(180.0),
                width: Val::Px(620.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(12.0)),
                border_radius: BorderRadius::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.15, 0.30, 0.94)),
            Interaction::default(),
            BlocksWorld,
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|p| {
            p.spawn(text(format!("Collection Journal — {} of {} found", collection.found.len(), ui.data.collectibles.len()), 18.0, Color::WHITE));
            for (kind, title) in [
                (CollectKind::Gem, "Gems"),
                (CollectKind::Metal, "Metals"),
                (CollectKind::SpaceRock, "Space Rocks"),
                (CollectKind::Fish, "Fish"),
                (CollectKind::Butterfly, "Butterflies"),
                (CollectKind::Beetle, "Beetles"),
            ] {
                let items: Vec<&CollectibleInfo> = ui.data.collectibles.iter().filter(|c| c.kind == kind).collect();
                let found = items.iter().filter(|c| collection.found.contains(&c.key)).count();
                p.spawn(text(format!("{title} ({found}/{})", items.len()), 15.0, Color::srgb(0.75, 0.9, 1.0)));
                p.spawn(Node { width: Val::Percent(100.0), flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(10.0), row_gap: Val::Px(2.0), ..default() }).with_children(|row| {
                    for c in items {
                        let label = match (collection.found.contains(&c.key), collection.held.get(&c.key)) {
                            (true, Some((n, _))) if *n > 0 => format!("{} ×{n}", c.name),
                            (true, _) => c.name.clone(),
                            (false, _) => "???".to_string(),
                        };
                        let color = if collection.found.contains(&c.key) { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.4) };
                        row.spawn(text(label, 13.0, color));
                    }
                });
            }
            p.spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn(text(format!("Held: {held} {} worth §{worth}", if held == 1 { "find" } else { "finds" }), 14.0, Color::WHITE));
                if held > 0 {
                    row.spawn((
                        Button,
                        SellButton,
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|b| {
                        b.spawn((text(format!("Sell all (§{worth})"), 14.0, Color::WHITE), Pickable::IGNORE));
                    });
                }
            });
        })
        .id();
    panel.root = Some(root);
}

fn sell_button(
    buttons: Query<&Interaction, (Changed<Interaction>, With<SellButton>)>,
    mut collection: ResMut<Collection>,
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    if !buttons.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    let worth: i64 = collection.held.values().map(|h| h.1).sum();
    collection.held.clear();
    if let Some(h) = household.as_mut() {
        h.funds += worth;
    }
    notes.push(format!("Sold the household's finds for §{worth}."));
    play.write(crate::sound::PlaySound::ui("ui_object_sell"));
}
