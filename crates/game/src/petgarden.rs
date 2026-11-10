//! Pets in the garden, as the Pets pack has them (`HarvestPlant.EatFromHarvestPlantPet`): a hungry
//! horse or dog eats produce ready on a plant it can reach (a low or medium one), where the
//! ingredient's `PetSpeciesThatCanEatOffPlant` names its kind (horses most vegetables and fruit,
//! dogs tomatoes and grapes), and a horse eats a young plant whole, leaving its shreds
//! (`HarvestableShreds`, gone in two days, `kDecayTime`, or cleaned up sooner). It takes ten
//! minutes (`kEatTime`: the pack's `harvestable_vine_eat` clips) and fills its hunger
//! (`Hunger=100`); a Piggy pet does it the more, a Neat one never of its own accord.

use bevy::prelude::*;
use rand::Rng;

use crate::baked::Baked;
use crate::gardening::GrowingPlant;
use crate::interact::{GameObject, ObjectKind};
use crate::pets::{HomePet, Pet, PetAnim, PetBusy};

pub struct PetGardenPlugin;

impl Plugin for PetGardenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (want_plants, graze, shreds_decay, petgarden_test).chain().run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's tuning.
const EAT_MINUTES: f64 = 10.0;
const HUNGER: f32 = 100.0;
const SHRED_MINUTES: f64 = 48.0 * 60.0;
/// How hungry a pet is to go for the garden, and how often it thinks of it.
const HUNGRY: f32 = -20.0;
const THINK: f64 = 60.0;
/// How far from a plant a pet stands to eat it (`PetRouteToPlantAndCheckInUse`).
pub fn reach(kind: &str) -> f32 {
    if kind.ends_with('h') { 1.5 } else { 0.75 }
}

/// What a pet eats at a plant: the produce on it, or the plant itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Meal {
    Produce,
    Plant,
}

/// The game's tag for a pet's kind in the ingredients' lists.
fn tag(kind: &str) -> Option<&'static str> {
    match kind {
        "ah" => Some("Horses"),
        "ad" | "al" => Some("AllDogs"),
        _ => None,
    }
}

/// Whether a kind of pet eats an ingredient off its plant.
pub fn eats_off_plant(data: &s3bake::gamedata::GameDataBaked, key: &str, kind: &str) -> bool {
    let Some(t) = tag(kind) else { return false };
    data.ingredients.iter().find(|i| i.key.eq_ignore_ascii_case(key)).is_some_and(|i| i.pets_graze.iter().any(|s| s == t || s == "AllAvailable"))
}

/// What a pet of a kind would eat at a plant (none: nothing): produce it eats ready on a low or
/// medium plant; for a horse, a young plant whole (grown past a sprout, not bearing, not wilted).
pub fn meal(p: &GrowingPlant, info: &s3bake::gamedata::PlantInfo, data: &s3bake::gamedata::GameDataBaked, kind: &str) -> Option<Meal> {
    if p.dead || !matches!(info.height.as_str(), "Low" | "Medium") {
        return None;
    }
    if p.ready > 0 {
        return eats_off_plant(data, &info.produce, kind).then_some(Meal::Produce);
    }
    (kind == "ah" && p.growth >= 0.35 && p.water > 15.0).then_some(Meal::Plant)
}

/// A pet off to a plant, or eating at it.
#[derive(Component)]
pub struct Grazing {
    plant: Entity,
    meal: Meal,
    eating: Option<f64>,
    clip_end: f32,
    stopping: bool,
}

/// A pet sent to eat at a plant.
pub fn send(commands: &mut Commands, e: Entity, pet: &Pet, here: Vec2, plant: Entity, at: Vec2, meal: Meal, grid: &crate::nav::NavGrid, rng: &mut impl Rng) -> bool {
    let to = at + (here - at).normalize_or(Vec2::Y) * reach(&pet.kind);
    let Some(task) = crate::petneeds::plan(pet, here, crate::petneeds::PetDo::Go, Some(to), None, grid, None, rng) else { return false };
    commands.entity(e).insert((task, Grazing { plant, meal, eating: None, clip_end: 0.0, stopping: false }));
    true
}

/// Hungry household horses and dogs go for the garden, now and then (a Piggy one the more, a Neat
/// one never).
#[allow(clippy::type_complexity)]
fn want_plants(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    pets: Query<(Entity, &Pet, &crate::petneeds::PetMotives, &Transform, Option<&crate::pets::PetTraits>), (With<HomePet>, Without<crate::petneeds::PetTask>, Without<PetBusy>, Without<Grazing>, Without<crate::petlitters::Held>, Without<crate::petlap::OnLap>, Without<crate::sim::Selected>, Without<crate::riding::Ridden>)>,
    plants: Query<(Entity, &GrowingPlant, &Transform), Without<Pet>>,
    taken: Query<&Grazing>,
    (ui, grid): (Option<Res<crate::icons::GameUi>>, Option<Res<crate::nav::NavGrid>>),
    mut next: Local<std::collections::HashMap<Entity, f64>>,
) {
    let (Some(ui), Some(grid)) = (ui, grid) else { return };
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, pet, m, tf, traits) in &pets {
        if tag(&pet.kind).is_none() || m.0[crate::petneeds::HUNGER] >= HUNGRY {
            continue;
        }
        let at = next.entry(e).or_insert(now);
        if now < *at {
            continue;
        }
        *at = now + THINK;
        let has = |n: &str| traits.is_some_and(|t| crate::pettraits::has(&t.0, n));
        if has("NeatPet") || !rng.random_bool(if has("PiggyPet") { 0.6 } else { 0.3 }) {
            continue;
        }
        let here = tf.translation.xz();
        let near = plants
            .iter()
            .filter(|(p, ..)| !taken.iter().any(|g| g.plant == *p))
            .filter_map(|(p, gp, ptf)| ui.data.plants.get(gp.plant).and_then(|info| meal(gp, info, &ui.data, &pet.kind)).map(|m| (p, m, ptf.translation.xz())))
            .filter(|(.., at)| at.distance(here) < 30.0)
            .min_by(|a, b| a.2.distance(here).total_cmp(&b.2.distance(here)));
        if let Some((p, what, at)) = near
            && send(&mut commands, e, pet, here, p, at, what, &grid, &mut rng)
        {
            info!("pet garden: {} goes to eat at the garden ({what:?})", pet.name);
        }
    }
}

/// At the plant: the pack's eating clips for ten minutes, then the produce or the plant gone.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn graze(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    (data, mut lib, catalog): (Res<Baked>, ResMut<crate::anim::ClipLibrary>, Res<crate::loading::Catalog>),
    mut pets: Query<(Entity, &Pet, &mut Grazing, &mut Transform, &mut PetAnim, &mut crate::petneeds::PetMotives, Has<crate::petneeds::PetTask>)>,
    mut plants: Query<(&mut GrowingPlant, &Transform, Option<&crate::gardening::PlantSoil>), Without<Pet>>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    ui: Option<Res<crate::icons::GameUi>>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, pet, mut g, mut tf, mut anim, mut pm, walking) in &mut pets {
        let Ok((mut p, ptf, soil)) = plants.get_mut(g.plant) else {
            commands.entity(e).remove::<(Grazing, PetBusy)>();
            continue;
        };
        let k = pet.kind.as_str();
        let clip = |part: &str| format!("{k}2o_harvestable_vine_eat_{part}_x");
        let Some(since) = g.eating else {
            if walking {
                continue;
            }
            let at = ptf.translation.xz();
            let still = ui.as_ref().and_then(|ui| ui.data.plants.get(p.plant).and_then(|info| meal(&p, info, &ui.data, k)));
            if tf.translation.xz().distance(at) > reach(k) + 0.8 || still != Some(g.meal) {
                commands.entity(e).remove::<Grazing>();
                continue;
            }
            let to = at - tf.translation.xz();
            debug!("pet garden: {} at the plant, {:.1} m off", pet.name, to.length());
            tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
            commands.entity(e).insert(PetBusy).remove::<(crate::pethunting::Hunt, crate::petcompany::Greeting)>();
            g.eating = Some(now);
            let start = clip("start");
            g.clip_end = lib.get(&data, &start).map_or(2.0, |c| c.duration) - 0.05;
            anim.replay(start);
            continue;
        };
        if anim.time() < g.clip_end {
            continue;
        }
        if g.stopping {
            // Done: the produce gone (one of it), or the plant, its shreds left.
            pm.add(crate::petneeds::HUNGER, HUNGER);
            match g.meal {
                Meal::Produce => {
                    p.ready = p.ready.saturating_sub(1);
                    info!("pet garden: {} ate produce off the plant", pet.name);
                }
                Meal::Plant => {
                    let at = ptf.translation;
                    if let Some(s) = soil {
                        commands.entity(s.0).try_despawn();
                    }
                    commands.entity(g.plant).try_despawn();
                    if let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("HarvestableShreds")).map(|c| c.objd) {
                        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                        if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, at, rng.random_range(0.0..std::f32::consts::TAU)) {
                            commands.entity(o.entity).insert((crate::save::Bought, Shreds(now + SHRED_MINUTES)));
                        }
                    }
                    info!("pet garden: {} ate the plant", pet.name);
                }
            }
            commands.entity(e).remove::<(Grazing, PetBusy)>();
            anim.play(crate::pets::idle_clip(k, false, &mut rng));
            continue;
        }
        let rounds: Vec<String> = (1..=3).map(|i| clip(&format!("loop{i}"))).filter(|c| data.0.clip_names.iter().any(|n| n.eq_ignore_ascii_case(c))).collect();
        let next = if now - since >= EAT_MINUTES || rounds.is_empty() {
            g.stopping = true;
            clip("stop")
        } else {
            rounds[rng.random_range(0..rounds.len())].clone()
        };
        g.clip_end = lib.get(&data, &next).map_or(2.0, |c| c.duration) - 0.05;
        anim.replay(next);
    }
}

/// A plant's shreds, left by a pet that ate it: gone by then (game minutes).
#[derive(Component)]
pub struct Shreds(pub f64);

fn shreds_decay(mut commands: Commands, clock: Res<crate::clock::GameClock>, shreds: Query<(Entity, &Shreds)>) {
    for (e, s) in &shreds {
        if clock.minutes >= s.0 {
            commands.entity(e).try_despawn();
        }
    }
}

/// `GARDEN_TEST=<pet>` (tests): ten seconds in, a tomato plant ready to pick (and a young lettuce
/// for a horse) put down near that pet, and the pet sent to eat; the camera on it.
#[allow(clippy::type_complexity)]
fn petgarden_test(
    mut commands: Commands,
    time: Res<Time>,
    pets: Query<(Entity, &Pet, &Transform), With<HomePet>>,
    plants: Query<(Entity, &GrowingPlant, &Transform), Without<Pet>>,
    (ui, grid): (Option<Res<crate::icons::GameUi>>, Option<Res<crate::nav::NavGrid>>),
    mut garden: MessageWriter<crate::gardening::PlantNow>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("GARDEN_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let (Some(ui), Some(grid)) = (ui, grid) else { return };
    let Some((e, pet, tf)) = pets.iter().find(|p| p.1.name.eq_ignore_ascii_case(&name)) else { return };
    if let Ok(mut c) = cams.single_mut() {
        let plant = plants.iter().map(|p| p.2.translation).min_by(|a, b| a.distance(tf.translation).total_cmp(&b.distance(tf.translation)));
        c.look_at(plant.unwrap_or(tf.translation));
        c.distance = 8.0;
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let whole = std::env::var_os("GARDEN_WHOLE").is_some();
            let want = if whole { "Lettuce" } else { "Tomato" };
            if let Some(i) = ui.data.plants.iter().position(|p| p.produce.eq_ignore_ascii_case(want)) {
                let at = tf.translation + Vec3::new(3.0, 0.0, 0.0);
                garden.write(crate::gardening::PlantNow { plant: i, at, growth: if whole { 0.6 } else { 1.0 }, ready: if whole { 0 } else { 3 } });
                info!("garden test: a {want} plant at {at:.1}");
            }
        }
        1 if now - stage.1 > 2.0 => {
            *stage = (2, now);
            let here = tf.translation.xz();
            let near = plants.iter().filter_map(|(p, gp, ptf)| ui.data.plants.get(gp.plant).and_then(|info| meal(gp, info, &ui.data, &pet.kind)).map(|m| (p, m, ptf.translation.xz()))).min_by(|a, b| a.2.distance(here).total_cmp(&b.2.distance(here)));
            if let Some((p, m, at)) = near
                && send(&mut commands, e, pet, here, p, at, m, &grid, &mut rand::rng())
            {
                info!("garden test: {} sent ({m:?}) to the plant {p:?} at {at:.1}, of {} plants", pet.name, plants.iter().count());
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horses_and_dogs_eat_what_the_game_lets_them() {
        let ing = |key: &str, graze: &[&str]| s3bake::gamedata::IngredientInfo { key: key.into(), name: key.into(), pets_graze: graze.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        let data = s3bake::gamedata::GameDataBaked { ingredients: vec![ing("Tomato", &["AllDogs", "Horses", "AB"]), ing("Lettuce", &["Horses", "AB"])], ..Default::default() };
        let info = |produce: &str, height: &str| s3bake::gamedata::PlantInfo { produce: produce.into(), height: height.into(), ..Default::default() };
        let ripe = GrowingPlant { plant: 0, growth: 1.0, water: 70.0, weedy: false, ready: 2, harvests_left: 3, next_ready: 0.0, quality: 0.5, care: 0.0, vigor: 0.0, dead: false, fertilizer: None };
        assert_eq!(meal(&ripe, &info("Tomato", "Low"), &data, "ad"), Some(Meal::Produce));
        assert_eq!(meal(&ripe, &info("Lettuce", "Low"), &data, "ad"), None, "dogs don't eat lettuce");
        assert_eq!(meal(&ripe, &info("Tomato", "Tall"), &data, "ah"), None, "out of reach");
        let young = GrowingPlant { growth: 0.6, ready: 0, ..ripe.clone() };
        assert_eq!(meal(&young, &info("Lettuce", "Medium"), &data, "ah"), Some(Meal::Plant));
        assert_eq!(meal(&young, &info("Lettuce", "Medium"), &data, "ad"), None, "only horses eat plants whole");
        assert_eq!(meal(&GrowingPlant { growth: 0.1, ..young.clone() }, &info("Lettuce", "Medium"), &data, "ah"), None, "not a sprout");
    }
}
