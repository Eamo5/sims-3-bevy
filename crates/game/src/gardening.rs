//! Gardening, from the game's `plants` and `ingredients` tables: seeds (a starter packet, more
//! from the grocery store), planted anywhere outdoors on the home lot in the game's garden soil;
//! the plant (the game's bush, vine or tree) grows over a couple of days while it's kept watered
//! and weeded, then bears produce to harvest, sold at the table's prices. Planting, watering,
//! weeding and harvesting use the game's gardening animations and teach Gardening.

use std::collections::BTreeMap;

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::baked::Baked;
use crate::interact::{GameObject, Notifications, ObjectKind, Skills};
use crate::loading::Catalog;
use crate::objects::{AssetCtx, ObjectAssets};
use crate::rabbitholes::Activity;
use crate::sim::Sim;

pub struct GardeningPlugin;

impl Plugin for GardeningPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Garden>()
            .add_systems(Update, (starter_packet, restore_plants, garden_requests, grow, sprinkle).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The household's seeds, by plant (index into the game data's plants), and the produce it's
/// grown to perfection.
#[derive(Resource, Default, Clone)]
pub struct Garden {
    pub seeds: BTreeMap<usize, u32>,
    pub perfect: Vec<String>,
}

/// The game's produce qualities, worst to best, with what each sells for (its harvest
/// tuning's `kCostMultiplierForPlantables`).
pub const QUALITIES: [(&str, f32); 10] = [
    ("Horrifying", 0.7),
    ("Putrid", 0.8),
    ("Bad", 0.9),
    ("Normal", 1.0),
    ("Nice", 1.1),
    ("Very Nice", 1.5),
    ("Great", 2.0),
    ("Excellent", 2.5),
    ("Outstanding", 3.0),
    ("Perfect", 4.0),
];

/// A plant's quality (0..1) as one of the game's qualities.
pub fn quality_tier(q: f32) -> usize {
    ((q * 10.0).ceil() as usize).clamp(1, 10) - 1
}

/// What a newly planted seed starts at (a Normal plant).
fn seed_quality() -> f32 {
    0.35
}

/// A planted garden plant.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct GrowingPlant {
    pub plant: usize,
    /// 0 (just planted) .. 1 (grown).
    pub growth: f32,
    /// 0 (parched) .. 100 (just watered).
    pub water: f32,
    pub weedy: bool,
    /// Produce ready to pick, harvests left in its life, and when the next crop is ready.
    pub ready: u32,
    pub harvests_left: u32,
    pub next_ready: f64,
    /// 0 (Horrifying) .. 1 (Perfect): better with care and a skilled gardener, worse neglected.
    #[serde(default = "seed_quality")]
    pub quality: f32,
    /// The skill (0..1) of whoever last tended it.
    #[serde(default)]
    pub care: f32,
}

impl GrowingPlant {
    /// Commit a crop only once. Queued interactions can finish after another Sim has
    /// already harvested this plant, so the menu's readiness check is not sufficient.
    fn harvest(&mut self, now: f64) -> Option<u32> {
        if self.ready == 0 || self.harvests_left == 0 {
            return None;
        }
        let picked = self.ready;
        self.ready = 0;
        self.next_ready = now + CROP_MINUTES;
        self.harvests_left -= 1;
        Some(picked)
    }
}
#[cfg(test)]
mod harvest_tests {
    use super::*;

    fn plant() -> GrowingPlant {
        GrowingPlant { plant: 0, growth: 1.0, water: 70.0, weedy: false,
            ready: 3, harvests_left: 2, next_ready: 0.0, quality: 0.55, care: 0.5 }
    }

    #[test]
    fn competing_harvests_consume_one_crop_and_one_lifetime_use() {
        let mut p = plant();
        assert!(offers(&p, crate::interact::Special::Harvest));
        assert_eq!(p.harvest(100.0), Some(3));
        assert_eq!(p.harvests_left, 1);
        assert_eq!(p.next_ready, 100.0 + CROP_MINUTES);
        assert!(!offers(&p, crate::interact::Special::Harvest));
        let saved = serde_json::to_value(&p).unwrap();
        assert_eq!(p.harvest(200.0), None);
        assert_eq!(serde_json::to_value(&p).unwrap(), saved, "a stale request must not delay the next crop or age the plant");
        p.ready = 4;
        assert_eq!(p.harvest(3000.0), Some(4));
        assert_eq!(p.harvests_left, 0);
    }

    #[test]
    fn exhausted_plants_cannot_yield_even_with_stale_ready_produce() {
        let mut p = plant();
        p.harvests_left = 0;
        let saved = serde_json::to_value(&p).unwrap();
        assert!(!offers(&p, crate::interact::Special::Harvest));
        assert_eq!(p.harvest(100.0), None);
        assert_eq!(serde_json::to_value(&p).unwrap(), saved);
    }

    #[test]
    fn fast_learner_gardening_crosses_levels_and_still_caps_at_ten() {
        let mut world = World::new();
        world.init_resource::<Messages<crate::life::LifeEvent>>();
        let me = world.spawn_empty().id();
        let mut state = bevy::ecs::system::SystemState::<MessageWriter<crate::life::LifeEvent>>::new(&mut world);
        let mut writer = state.get_mut(&mut world).unwrap();
        let mut sim = crate::sim::random_sim(&mut rand::rng(), "Test", Some(false), crate::sim::Age::Adult);
        sim.traits.clear();
        let wishes = crate::wishes::Wishes::restored(0, vec!["FastLearner".into()], 0.0);
        let mut notes = Notifications::default();
        let mut ordinary = Skills::default();
        let mut rewarded = Skills::default();
        ordinary.0.insert("Gardening", 0.4);
        rewarded.0.insert("Gardening", 0.4);
        learn(&sim, me, &mut ordinary, &mut notes, &mut writer, 325.0, None);
        assert_eq!(ordinary.level("Gardening"), 0);
        assert!(notes.0.is_empty());
        learn(&sim, me, &mut rewarded, &mut notes, &mut writer, 325.0, Some(&wishes));
        assert_eq!(rewarded.level("Gardening"), 1);
        assert_eq!(notes.0.len(), 1, "reward-driven level crossing must notify");
        learn(&sim, me, &mut rewarded, &mut notes, &mut writer, 65000.0, Some(&wishes));
        assert_eq!(rewarded.0["Gardening"], 10.0);
    }
}

/// A plant in a saved game (where it stands, and how it's doing).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedPlant {
    pub name: String,
    pub position: [f32; 3],
    pub yaw: f32,
    pub state: GrowingPlant,
}

/// Plants (and seeds, by plant name) from a saved game, waiting for the game data.
#[derive(Resource)]
pub struct PendingPlants(pub Vec<SavedPlant>, pub Vec<(String, u32)>, pub Vec<String>);

/// What a Sim has just done in the garden (set by the interactions, handled here).
#[derive(Component, Clone, Copy, Debug)]
pub enum GardenRequest {
    Plant { at: Vec3, plant: usize },
    Water(Entity),
    Weed(Entity),
    Harvest(Entity),
    BoughtSeeds,
}

/// Minutes from planting to grown, when kept watered.
const GROW_MINUTES: f32 = 2.0 * 1440.0;
/// Between crops.
const CROP_MINUTES: f64 = 1.5 * 1440.0;
/// The grocery's seed packets.
pub const SEED_PRICE: i64 = 30;

/// Buying seeds at the grocery store.
pub static BUY_SEEDS: Activity = Activity {
    name: "Buy Seeds",
    minutes: 30.0,
    cost: SEED_PRICE,
    per_hour: [-4.0, -4.0, -2.0, 10.0, 0.0, 10.0],
    skill: None,
    open: 8.0,
    close: 22.0,
};
/// The Visit activity number for buying seeds.
pub const SEEDS_TASK: usize = 999;

/// The grocery's garden aisle, on lots with a grocery store.
pub fn lot_options(world: &crate::loading::WorldInfo, lot: usize) -> Vec<(String, crate::interact::ActionKind)> {
    if crate::opportunities::lot_types(world, lot).contains(&"Grocery") {
        vec![(format!("Buy Seeds (§{SEED_PRICE})"), crate::interact::ActionKind::Visit { lot, activity: SEEDS_TASK })]
    } else {
        Vec::new()
    }
}

/// Which interactions a plant offers right now.
pub fn offers(p: &GrowingPlant, special: crate::interact::Special) -> bool {
    use crate::interact::Special as S;
    match special {
        S::Water => p.water < 85.0,
        S::Weed => p.weedy,
        S::Harvest => p.ready > 0 && p.harvests_left > 0,
        _ => true,
    }
}

/// Seed options for planting at a spot on the home lot.
pub fn plant_options(garden: &Garden, data: &s3bake::GameDataBaked, at: Vec2, level: u8) -> Vec<(String, crate::interact::ActionKind)> {
    garden
        .seeds
        .iter()
        .filter(|(_, n)| **n > 0)
        .filter_map(|(i, n)| {
            let p = data.plants.get(*i)?;
            Some((format!("Plant {} Seed ({n})", p.produce), crate::interact::ActionKind::PlantSeed { at, level, plant: *i }))
        })
        .collect()
}

/// Spawns one of the game's objects by internal name, as an object of `kind`.
#[allow(clippy::too_many_arguments)]
fn spawn_named(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    name: &str,
    kind: ObjectKind,
    label: String,
    at: Vec3,
    yaw: f32,
) -> Option<Entity> {
    let objd = ctx.baked.catalog.iter().find(|c| c.instance_name == name)?.objd;
    let e = crate::home::spawn_game_object_rot(commands, assets, ctx, catalog, objd, at, Quat::from_rotation_y(yaw))?.entity;
    commands.entity(e).remove::<crate::nav::Obstacle>().queue_silenced(move |mut w: EntityWorldMut| {
        if let Some(mut g) = w.get_mut::<GameObject>() {
            g.kind = kind;
            g.name = label;
        }
    });
    Some(e)
}

/// Plants the plant (in its soil) and returns its entity.
#[allow(clippy::too_many_arguments)]
fn plant_at(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    info: &s3bake::gamedata::PlantInfo,
    state: GrowingPlant,
    at: Vec3,
    yaw: f32,
) -> Option<Entity> {
    let e = spawn_named(commands, assets, ctx, catalog, &info.model, ObjectKind::GardenPlant, info.name.clone(), at, yaw)?;
    let soil = spawn_named(commands, assets, ctx, catalog, "GardenSoil", ObjectKind::Decoration, "Garden Soil".into(), at, yaw);
    commands.entity(e).insert((state.clone(), Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale(state.growth)))));
    if let Some(s) = soil {
        commands.entity(e).insert(PlantSoil(s));
    }
    Some(e)
}

/// The soil under a plant (cleared away with it).
#[derive(Component)]
struct PlantSoil(Entity);

fn scale(growth: f32) -> f32 {
    0.25 + 0.75 * growth.clamp(0.0, 1.0)
}

/// Skill points (the table's) as Gardening levels.
fn skill_gain(points: f32) -> f32 {
    points / 650.0
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn garden_requests(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &GardenRequest, &mut Skills, Option<&crate::wishes::Wishes>)>,
    mut plants: Query<(&mut GrowingPlant, Option<&PlantSoil>)>,
    mut garden: ResMut<Garden>,
    ui: Option<Res<crate::icons::GameUi>>,
    clock: Res<crate::clock::GameClock>,
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut notes: ResMut<Notifications>,
    mut life: MessageWriter<crate::life::LifeEvent>,
    mut did: MessageWriter<crate::journal::Did>,
) {
    let Some(ui) = ui else { return };
    let mut rng = rand::rng();
    for (me, sim, req, mut skills, wishes) in &mut sims {
        commands.entity(me).remove::<GardenRequest>();
        let level = skills.level("Gardening") as f32;
        match *req {
            GardenRequest::Plant { at, plant } => {
                let Some(info) = ui.data.plants.get(plant) else { continue };
                let Some(n) = garden.seeds.get_mut(&plant).filter(|n| **n > 0) else { continue };
                // (Better gardeners plant better seeds.)
                let state = GrowingPlant {
                    plant,
                    growth: 0.0,
                    water: 70.0,
                    weedy: false,
                    ready: 0,
                    harvests_left: info.lifetime.max(1),
                    next_ready: 0.0,
                    quality: seed_quality() + level * 0.02,
                    care: level / 10.0,
                };
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                if plant_at(&mut commands, &mut assets, &mut ctx, &catalog, info, state, at, rng.random_range(0.0..6.28)).is_some() {
                    *n -= 1;
                    notes.push(format!("{} planted a {}.", sim.first, info.name.to_lowercase()));
                    did.write(crate::journal::Did::kind(me, crate::journal::Kinds::PlantTypes, info.name.clone()));
                    learn(sim, me, &mut skills, &mut notes, &mut life, info.skill_plant, wishes);
                }
            }
            GardenRequest::Water(e) => {
                if let Ok((mut p, _)) = plants.get_mut(e) {
                    p.water = 100.0;
                    p.care = level / 10.0;
                    learn(sim, me, &mut skills, &mut notes, &mut life, 25.0, wishes);
                }
            }
            GardenRequest::Weed(e) => {
                if let Ok((mut p, _)) = plants.get_mut(e) {
                    if !p.weedy { continue; }
                    p.weedy = false;
                    p.care = level / 10.0;
                    learn(sim, me, &mut skills, &mut notes, &mut life, 40.0, wishes);
                }
            }
            GardenRequest::Harvest(e) => {
                let Ok((mut p, soil)) = plants.get_mut(e) else { continue };
                let Some(info) = ui.data.plants.get(p.plant) else { continue };
                let Some(picked) = p.harvest(clock.minutes) else { continue };
                // The plant's quality, give or take (a Super Green Thumb's a step finer).
                let q = p.quality + rng.random_range(-0.075..0.075) + if crate::wishes::has(wishes, "SuperGreenThumb") { 0.1 } else { 0.0 };
                let tier = quality_tier(q);
                let (word, multiplier) = QUALITIES[tier];
                // Into their inventory, to sell or eat.
                let each = (info.price as f32 * multiplier).round() as i64;
                crate::inventory::give(&mut commands, me, crate::inventory::ItemKind::Produce, info.produce.clone(), format!("{word} {}", info.produce), tier as u8, each, picked);
                learn(sim, me, &mut skills, &mut notes, &mut life, info.skill_harvest, wishes);
                notes.push(format!("{} harvested {picked} {word} {} (worth §{}).", sim.first, plural(&info.produce, picked), each * picked as i64));
                did.write(crate::journal::Did::count(me, crate::journal::Stat::Harvested, picked as f64));
                if word == "Perfect" {
                    did.write(crate::journal::Did::count(me, crate::journal::Stat::PerfectHarvested, picked as f64));
                }
                if word == "Perfect" && !garden.perfect.contains(&info.produce) {
                    garden.perfect.push(info.produce.clone());
                    notes.push(format!("{} grew perfect {} for the first time!", sim.first, plural(&info.produce, 2)));
                }
                if p.harvests_left == 0 {
                    notes.push(format!("The {} has borne its last crop.", info.name.to_lowercase()));
                    if let Some(s) = soil {
                        commands.entity(s.0).try_despawn();
                    }
                    commands.entity(e).despawn();
                }
            }
            GardenRequest::BoughtSeeds => {
                // A packet of four: rarer seeds for better gardeners.
                let mut got: BTreeMap<String, u32> = BTreeMap::new();
                for _ in 0..4 {
                    let roll: f32 = rng.random_range(0.0..1.0);
                    let rarity = if roll < 0.06 * level { "Rare" } else if roll < 0.15 + 0.08 * level { "Uncommon" } else { "Common" };
                    let pool: Vec<usize> = ui.data.plants.iter().enumerate().filter(|(_, p)| p.rarity == rarity).map(|(i, _)| i).collect();
                    if pool.is_empty() {
                        continue;
                    }
                    let i = pool[rng.random_range(0..pool.len())];
                    *garden.seeds.entry(i).or_insert(0) += 1;
                    *got.entry(ui.data.plants[i].produce.clone()).or_insert(0) += 1;
                }
                let list: Vec<String> = got.iter().map(|(k, n)| format!("{n} {k}")).collect();
                notes.push(format!("{} bought seeds: {}.", sim.first, list.join(", ")));
            }
        }
    }
}

/// Gardening skill from the table's points.
fn learn(sim: &Sim, me: Entity, skills: &mut Skills, notes: &mut Notifications, life: &mut MessageWriter<crate::life::LifeEvent>, points: f32, wishes: Option<&crate::wishes::Wishes>) {
    let v = skills.0.entry("Gardening").or_insert(0.0);
    let before = *v as u32;
    *v = (*v + skill_gain(points) * crate::life::skill_rate(&sim.traits, "Gardening") * crate::wishes::reward_skill_rate(wishes)).min(10.0);
    if *v as u32 > before {
        notes.push(format!("{} reached level {} in Gardening!", sim.first, *v as u32));
        life.write(crate::life::LifeEvent::new(me, crate::life::LifeEventKind::SkillUp { skill: "Gardening", level: *v as u32 }));
    }
}

fn plural(produce: &str, n: u32) -> String {
    if n == 1 || produce.ends_with('s') {
        produce.to_lowercase()
    } else if produce.ends_with("to") {
        format!("{}es", produce.to_lowercase())
    } else {
        format!("{}s", produce.to_lowercase())
    }
}

/// Plants dry out, get weedy, grow and bear produce.
/// A garden sprinkler turned on: until when it runs.
#[derive(Component)]
pub struct Sprinkling {
    pub until: f64,
}

/// How far a sprinkler waters (metres).
const SPRINKLER_REACH: f32 = 3.5;

/// A sprinkler running waters the plants in its reach, and turns itself off after a while.
fn sprinkle(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    sprinklers: Query<(Entity, &Sprinkling, &Transform)>,
    mut plants: Query<(&mut GrowingPlant, &Transform), Without<Sprinkling>>,
) {
    for (e, s, tf) in &sprinklers {
        if clock.minutes >= s.until {
            commands.entity(e).remove::<Sprinkling>();
            continue;
        }
        for (mut p, ptf) in &mut plants {
            if ptf.translation.xz().distance(tf.translation.xz()) <= SPRINKLER_REACH && p.water < 100.0 {
                p.water = 100.0;
            }
        }
    }
}

fn grow(
    delta: Res<crate::clock::SimDelta>,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut plants: Query<(&mut GrowingPlant, &mut Transform)>,
) {
    let Some(ui) = ui else { return };
    let dt = delta.0;
    if dt <= 0.0 {
        return;
    }
    let mut rng = rand::rng();
    for (mut p, mut tf) in &mut plants {
        let Some(info) = ui.data.plants.get(p.plant) else { continue };
        p.water = (p.water - info.water_decay * dt / 60.0).max(0.0);
        if !p.weedy && rng.random_bool((info.weeds as f64 * 0.04 * dt as f64 / 60.0).clamp(0.0, 1.0)) {
            p.weedy = true;
        }
        // Watered and weeded, a plant improves (from the harvest tuning's base improvement);
        // parched or weedy, it suffers.
        let tended = p.water > 30.0 && !p.weedy;
        let change = if tended { 0.091 * (0.4 + p.care) } else if p.water <= 0.0 || p.weedy { -0.091 * 0.5 } else { 0.0 };
        p.quality = (p.quality + change * dt / 1440.0).clamp(0.0, 1.0);
        if p.water > 0.0 && p.growth < 1.0 {
            let rate = (0.5 + p.water / 200.0) * if p.weedy { 0.5 } else { 1.0 };
            p.growth = (p.growth + dt / GROW_MINUTES * rate).min(1.0);
            let s = scale(p.growth);
            if (tf.scale.x - s).abs() > 0.005 {
                tf.scale = Vec3::splat(s);
            }
            if p.growth >= 1.0 && p.next_ready == 0.0 {
                p.next_ready = clock.minutes;
            }
        }
        if p.growth >= 1.0 && p.ready == 0 && p.harvests_left > 0 && clock.minutes >= p.next_ready && p.water > 0.0 {
            p.ready = rng.random_range(info.harvest_min.max(1)..=info.harvest_max.max(1));
        }
    }
}

/// Plants from a saved game go back in the ground.
#[allow(clippy::too_many_arguments)]
fn restore_plants(
    mut commands: Commands,
    pending: Option<Res<PendingPlants>>,
    ui: Option<Res<crate::icons::GameUi>>,
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let (Some(pending), Some(ui)) = (pending, ui) else { return };
    commands.remove_resource::<PendingPlants>();
    let seeds = pending.1.iter().filter_map(|(name, n)| Some((ui.data.plants.iter().position(|p| &p.name == name)?, *n))).collect();
    commands.insert_resource(Garden { seeds, perfect: pending.2.clone() });
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    for s in &pending.0 {
        let Some(i) = ui.data.plants.iter().position(|p| p.name == s.name) else { continue };
        let state = GrowingPlant { plant: i, ..s.state.clone() };
        plant_at(&mut commands, &mut assets, &mut ctx, &catalog, &ui.data.plants[i], state, Vec3::from(s.position), s.yaw);
    }
}

/// A new household gets a starter packet once the game data is in.
#[derive(Resource)]
pub struct StarterSeeds;

fn starter_packet(mut commands: Commands, starter: Option<Res<StarterSeeds>>, ui: Option<Res<crate::icons::GameUi>>) {
    let (Some(_), Some(ui)) = (starter, ui) else { return };
    commands.remove_resource::<StarterSeeds>();
    commands.insert_resource(starter_seeds(&ui.data));
}

/// The household's seeds, for saving.
pub fn saved_seeds(garden: &Garden, data: &s3bake::GameDataBaked) -> Vec<(String, u32)> {
    garden.seeds.iter().filter_map(|(i, n)| Some((data.plants.get(*i)?.name.clone(), *n))).collect()
}

/// A new household's starter packet: a few everyday seeds.
pub fn starter_seeds(data: &s3bake::GameDataBaked) -> Garden {
    let mut seeds = BTreeMap::new();
    for want in ["Tomato", "Lettuce", "Apple"] {
        if let Some(i) = data.plants.iter().position(|p| p.produce == want) {
            seeds.insert(i, 1);
        }
    }
    Garden { seeds, ..default() }
}

/// The plants on the lot, for saving.
pub fn saved_plants(plants: &Query<(&GrowingPlant, &Transform)>, data: &s3bake::GameDataBaked) -> Vec<SavedPlant> {
    plants
        .iter()
        .filter_map(|(p, tf)| {
            Some(SavedPlant { name: data.plants.get(p.plant)?.name.clone(), position: tf.translation.to_array(), yaw: tf.rotation.to_euler(EulerRot::YXZ).0, state: p.clone() })
        })
        .collect()
}
