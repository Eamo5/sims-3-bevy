//! Hunting, the Pets pack's skill for cats and dogs (`CatHuntingSkill`, `DogHuntingSkill`).
//!
//! A cat goes hunting ("Locate Prey...", the game's `StalkForPrey`, from the ground's menu with
//! it played; or of its own accord now and then, a Hunter sooner): out a few metres
//! (`kMinMaxAutonomousDist`), it sniffs the air about (`ac_hunting_locatePrey_sniffAir`) for a
//! quarter of an hour or so (`kMinMaxWanderAndSniffTime`), wandering a little between sniffs,
//! then finds what's about: one of the pack's minor pets its Hunting reaches, weighted as the
//! table weighs them by its level (`MinHuntSkillLevel`... `MinHuntWeight`: rats and
//! chipmunks, snakes, lizards, little birds, turtles), more likely to be something the better it
//! is (`kMinMaxChanceFindPrey` 30% to 80%; else something it mistakes for prey). It stalks up
//! to it (`ac_startStalk`, `ac_stalk`, `ac_stopStalk`) and pounces (`ac2o_hunting_catch`), and
//! catches it as often as its Hunting says (`kMinMaxChanceCatchPrey` 30% to 80%): hungry
//! (`kHungerThreshold`) it eats it there and then (`stopSuccess_eat`, `kEatPreyHungerGain`), else
//! carries it off into its inventory (`stopSuccess_carry`); missing, it's scared off
//! (`stopFreakedOut`: a Skittish cat more often, an Adventurous never), the prey's gone
//! (`stopConfused`: a Clueless one more often) or all it's caught is a feather, a maple leaf, a
//! bit of snake skin or an empty chip bag (an Aggressive or Hunter one more often:
//! `kFailureObjectOutcomeOverrideTraits`); the game's words for each.
//!
//! A dog sniffs out collectables ("Sniff Out Collectable", the game's `SniffOut`): it wanders
//! about sniffing the ground (`ad_huntingSkill_sniffingGround`), and digs one up
//! (`ad_digStart`, `ad_digLoop`, `ad_digStop`, the pack's dug hole left behind a while): a
//! gem, some metal or a rock (`FoundGem`, `FoundMetal`, `FoundRock`), the rarer the better its
//! Hunting (`kMinMaxChanceFindHuntable`), into its inventory.
//!
//! Both learn Hunting as they go (the skill's own levels), shown on the skills panel with the
//! pet played; what they've caught or found is theirs to give to the family (their inventory's
//! "Give to"). Kept in saves.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{GameObject, Notifications};
use crate::petneeds::{FUN, HUNGER, PetDo, PetMotives, PetTask};
use crate::pets::{HomePet, Pet, PetAnim, PetBusy, PetTraits};

pub struct PetHuntingPlugin;

impl Plugin for PetHuntingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (hunting_test, autonomy, start, hunt, holes).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// A pet's Hunting: a cat's or a dog's, its level (the way to the next its fraction), what it's
/// caught or found.
#[derive(Component, Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Hunting {
    pub cat: bool,
    pub level: f32,
    #[serde(default)]
    pub found: u32,
}

impl Hunting {
    pub fn new(kind: &str) -> Self {
        Self { cat: kind.ends_with('c'), level: 0.0, found: 0 }
    }

    /// The skill's name in the game's tables.
    pub fn skill_name(&self) -> &'static str {
        if self.cat { "CatHunting" } else { "DogHunting" }
    }

    pub fn whole(&self) -> u32 {
        (self.level as u32).min(10)
    }

    /// Practice: points by the skill's own levels.
    pub fn add_points(&mut self, points: f32) {
        let table = crate::interact::skill_points(self.skill_name());
        self.level = crate::interact::points_to_level(table, crate::interact::level_to_points(table, self.level) + points.max(0.0)).min(10.0);
    }
}

/// Whether a pet hunts: grown or young cats and dogs.
pub fn hunts(kind: &str) -> bool {
    matches!(kind.chars().nth(1), Some('c' | 'd' | 'l'))
}

/// The game's tuning: how far off a cat goes to hunt, how long it sniffs about (game minutes),
/// the chances of finding prey and of catching it (at no Hunting and at ten), the hunger under
/// which it eats its catch and what that's worth; how long a dog sniffs, how long it digs
/// (seconds), its chance of finding something; the failure objects.
const OUT: std::ops::Range<f32> = 3.0..8.0;
const CAT_SNIFF: std::ops::Range<f64> = 10.0..15.0;
const FIND: (f32, f32) = (0.3, 0.8);
const CATCH: (f32, f32) = (0.3, 0.8);
const HUNGRY: f32 = -50.0;
const EAT_GAIN: f32 = 25.0;
const DOG_SNIFF: std::ops::Range<f64> = 30.0..60.0;
const DIG_SECS: f32 = 5.0;
pub const FAILURE_OBJECTS: [(&str, &str); 4] = [("catHuntColorfulFeather", "Colorful Feather"), ("catHuntSnakeSkinPiece", "Snake Skin Piece"), ("catHuntMapleLeaf", "Maple Leaf"), ("catHuntWorthlessChipBag", "Empty Chip Bag")];
/// How long between moves while sniffing about (game minutes), and how far a stalk is from.
const MOVE_EVERY: f64 = 3.0;
const STALK_FROM: f32 = 2.5;
const STALK_SPEED: f32 = 0.35;
/// How long a dug hole's left (game minutes).
const HOLE_MINUTES: f64 = 120.0;

/// A chance between its worst and best by a Hunting level.
pub fn chance(range: (f32, f32), level: f32) -> f32 {
    range.0 + (range.1 - range.0) * (level / 10.0).clamp(0.0, 1.0)
}

/// What a missed catch comes to (`kFailureOutcomeWeights`, overridden by traits): scared off,
/// the prey gone, or a failure object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Miss {
    Scared,
    Gone,
    Junk,
}

pub fn miss_weights(traits: &[String]) -> [(Miss, f32); 3] {
    let has = |t: &str| traits.iter().any(|x| x == t);
    let scared = if has("AdventurousPet") {
        0.0
    } else if has("SkittishPet") {
        3.0
    } else {
        1.0
    };
    let gone = if has("CluelessPet") { 3.0 } else { 1.0 };
    let junk = if has("AggressivePet") || has("HunterPet") { 3.0 } else { 1.0 };
    [(Miss::Scared, scared), (Miss::Gone, gone), (Miss::Junk, junk)]
}

/// The prey a cat of a Hunting level could find, weighted (`SpawnAndPlacePrey`: the weight
/// between its least and most by the level).
pub fn prey_weights(pets: &[s3bake::gamedata::MinorPetInfo], level: u32) -> Vec<(&s3bake::gamedata::MinorPetInfo, f32)> {
    pets.iter()
        .filter_map(|p| {
            let (lo, hi, wlo, whi) = p.hunt?;
            if level < lo as u32 || level > hi as u32 {
                return None;
            }
            let t = if hi > lo { (level - lo as u32) as f32 / (hi - lo) as f32 } else { 0.0 };
            Some((p, (wlo + (whi - wlo) * t).max(0.01)))
        })
        .collect()
}

/// Where a hunt is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Going,
    Sniffing,
    Stalking,
    Catching,
    Digging,
    Done,
}

/// A pet out hunting: where it's got to, since when (seconds into a step), till when it sniffs
/// (game minutes), its next wander, the prey (its object, catalogue key, name, worth), how the
/// catch went.
#[derive(Component)]
pub struct Hunt {
    stage: Stage,
    t: f32,
    until: f64,
    next_move: f64,
    prey: Option<(Option<Entity>, String, String, i64)>,
    caught: Option<bool>,
    miss: Miss,
    clip: u8,
}

/// Asked to hunt (the player's order, or of its own accord).
#[derive(Component)]
pub struct GoHunt;

/// A dog's dug hole: when it goes.
#[derive(Component)]
struct DugHole(f64);

/// Off hunting: out to a spot on the lot.
#[allow(clippy::type_complexity)]
fn start(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    pets: Query<(Entity, &Pet, &Transform, Option<&Hunting>), (With<GoHunt>, With<HomePet>)>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut notes: ResMut<Notifications>,
) {
    let Some(grid) = grid else { return };
    let mut rng = rand::rng();
    for (e, pet, tf, hunting) in &pets {
        commands.entity(e).remove::<GoHunt>();
        if hunting.is_none() {
            commands.entity(e).insert(Hunting::new(&pet.kind));
        }
        let here = tf.translation.xz();
        let mut task = None;
        for _ in 0..16 {
            let a = rng.random_range(0.0..std::f32::consts::TAU);
            let to = here + Vec2::from_angle(a) * rng.random_range(OUT);
            if grid.cell_of(to).is_some()
                && open_ground(building.as_deref(), to)
                && let Some(t) = crate::petneeds::plan(pet, here, PetDo::Go, Some(to), None, &grid, building.as_deref(), &mut rng)
            {
                task = Some(t);
                break;
            }
        }
        let Some(task) = task else {
            notes.push(format!("{} can't get there.", pet.name));
            continue;
        };
        let cat = pet.kind.ends_with('c');
        let sniff = if cat { rng.random_range(CAT_SNIFF) } else { rng.random_range(DOG_SNIFF) };
        info!("pet hunting: {} goes hunting ({:.0} minutes' sniffing)", pet.name, sniff);
        commands
            .entity(e)
            .remove::<(crate::petcompany::Greeting, crate::petcompany::Greeted)>()
            .insert((task, crate::petlitters::Held, Hunt { stage: Stage::Going, t: 0.0, until: clock.minutes + sniff, next_move: 0.0, prey: None, caught: None, miss: Miss::Gone, clip: 0 }));
    }
}

/// Out on the open ground: no floor of the house there (a room, a porch, a deck), as the game's
/// hunting asks of the terrain (`kValidTerrainTypes`).
fn open_ground(building: Option<&crate::building::ActiveBuilding>, p: Vec2) -> bool {
    building.is_none_or(|b| b.room_at(1, Vec3::new(p.x, 0.0, p.y)).is_none())
}

/// The clips of a hunt for a kind (`cc` kittens as cats; little dogs as theirs).
fn sniff_clip(kind: &str) -> String {
    if kind.ends_with('c') { "ac_hunting_locatePrey_sniffAir_x".into() } else { format!("{}_huntingSkill_sniffingGround_x", if kind.ends_with('l') { "al" } else { "ad" }) }
}

fn dog(kind: &str) -> &'static str {
    if kind.ends_with('l') { "al" } else { "ad" }
}

/// The hunt under way.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn hunt(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib, ui): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>, Option<Res<crate::icons::GameUi>>),
    (catalog, mut assets): (Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut pets: Query<(Entity, &Pet, &mut Hunt, &mut Transform, &mut PetAnim, Has<PetTask>, &mut Hunting, Option<&mut PetMotives>, Option<&PetTraits>, Option<&mut crate::inventory::Inventory>)>,
    objects: Query<&Transform, (With<GameObject>, Without<Pet>)>,
    (grid, building, world): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>, Res<crate::loading::CurrentWorld>),
    mut notes: ResMut<Notifications>,
) {
    let (Some(grid), Some(ui)) = (grid, ui) else { return };
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let now = clock.minutes;
    let mut rng = rand::rng();
    let practice = delta.0 / 60.0 * crate::interact::practice_points("");
    let len = |lib: &mut crate::anim::ClipLibrary, n: &str, or: f32| lib.get(&data, n).map_or(or, |c| c.duration);
    for (e, pet, mut h, mut tf, mut anim, tasked, mut skill, motives, traits, inv) in &mut pets {
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        let cat = pet.kind.ends_with('c');
        let k = if cat { "ac".to_string() } else { dog(&pet.kind).to_string() };
        if h.stage != Stage::Done {
            skill.add_points(practice);
        }
        match h.stage {
            // Out to the spot.
            Stage::Going => {
                if !tasked {
                    h.stage = Stage::Sniffing;
                    h.next_move = now + MOVE_EVERY;
                    anim.play(sniff_clip(&pet.kind));
                }
            }
            // Sniffing about, a little wander between.
            Stage::Sniffing => {
                if tasked {
                    continue;
                }
                if anim.clip != sniff_clip(&pet.kind) {
                    anim.play(sniff_clip(&pet.kind));
                }
                if now < h.until {
                    if now >= h.next_move {
                        h.next_move = now + MOVE_EVERY;
                        let here = tf.translation.xz();
                        let to = here + Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU)) * rng.random_range(1.0..3.0);
                        if grid.cell_of(to).is_some()
                            && open_ground(building.as_deref(), to)
                            && let Some(t) = crate::petneeds::plan(pet, here, PetDo::Go, Some(to), None, &grid, building.as_deref(), &mut rng)
                        {
                            commands.entity(e).insert(t);
                        }
                    }
                    continue;
                }
                commands.entity(e).insert(PetBusy);
                let level = skill.level;
                if cat {
                    // Something found, a little way ahead: prey, or (unlucky) a failure object.
                    let found = rng.random::<f32>() < chance(FIND, level);
                    let fwd = (tf.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
                    let at = tf.translation + fwd * STALK_FROM;
                    let (key, name, worth) = if found {
                        let weighted = prey_weights(&ui.data.minor_pets, skill.whole());
                        match weighted.choose_weighted(&mut rng, |p| p.1) {
                            Ok((p, _)) => (p.species.clone(), p.name.clone(), rng.random_range(p.min_value.min(p.max_value)..=p.max_value.max(p.min_value)) as i64),
                            Err(_) => junk(&mut rng),
                        }
                    } else {
                        junk(&mut rng)
                    };
                    let objd = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&key)).map(|c| c.objd);
                    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                    let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, at);
                    let object = objd.and_then(|o| crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, o, at.with_y(y), rng.random_range(0.0..std::f32::consts::TAU))).map(|s| s.entity);
                    info!("pet hunting: {} spots a {name}", pet.name);
                    h.prey = Some((object, key, name, worth));
                    // (Prey caught or missed; a failure object, no prey at all.)
                    h.caught = found.then(|| rng.random::<f32>() < chance(CATCH, level));
                    h.miss = miss_weights(traits).choose_weighted(&mut rng, |m| m.1).map_or(Miss::Gone, |m| m.0);
                    (h.stage, h.t, h.clip) = (Stage::Stalking, 0.0, 0);
                    anim.replay("ac_startStalk_0_x".into());
                } else {
                    (h.stage, h.t, h.clip) = (Stage::Digging, 0.0, 0);
                    anim.replay(format!("{k}_digStart_x"));
                }
            }
            // Stalking up to it, slow and low.
            Stage::Stalking => {
                h.t += dt;
                let target = h.prey.as_ref().and_then(|p| p.0).and_then(|o| objects.get(o).ok()).map(|o| o.translation);
                let to = target.unwrap_or(tf.translation + tf.rotation * Vec3::Z);
                let d = (to - tf.translation).with_y(0.0);
                if h.clip == 0 && h.t >= len(&mut lib, "ac_startStalk_0_x", 1.0) {
                    h.clip = 1;
                    anim.play("ac_stalk_x".into());
                }
                if h.clip == 1 {
                    if d.length() > 0.55 {
                        let step = d.normalize() * STALK_SPEED * dt;
                        tf.translation += step;
                        tf.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
                    } else {
                        // The pounce.
                        tf.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
                        (h.stage, h.t, h.clip) = (Stage::Catching, 0.0, 0);
                        anim.replay("ac2o_hunting_catch_start_x".into());
                    }
                }
            }
            // The catch: in, a tussle, out as it went.
            Stage::Catching => {
                h.t += dt;
                let start = len(&mut lib, "ac2o_hunting_catch_start_x", 1.5);
                let loop_end = start + 3.0;
                if h.clip == 0 && h.t >= start {
                    h.clip = 1;
                    anim.play("ac2o_hunting_catch_loop_x".into());
                }
                if h.clip == 1 && h.t >= loop_end {
                    h.clip = 2;
                    let (_, _, name, worth) = h.prey.clone().unwrap_or_default();
                    let hungry = motives.as_ref().is_some_and(|m| m.0[HUNGER] < HUNGRY);
                    let stop = match h.caught {
                        Some(true) if hungry => {
                            notes.push(format!("{} just caught a {name} but {} was hungry so {} ate it.", pet.name, she(pet), she(pet)));
                            if let Some(mut m) = motives {
                                m.add(HUNGER, EAT_GAIN);
                            }
                            "ac2o_hunting_catch_stopSuccess_eat_x"
                        }
                        Some(true) => {
                            notes.push(format!("{} has caught a {name}! You can find it in {}'s inventory.", pet.name, pet.name));
                            give(&mut commands, e, inv, h.prey.as_ref().map(|p| p.1.clone()).unwrap_or_default(), name.clone(), worth);
                            skill.found += 1;
                            "ac2o_hunting_catch_stopSuccess_carry_x"
                        }
                        Some(false) if h.miss == Miss::Scared => {
                            notes.push(format!("{} attempted to catch a {name} but got scared. Poor {} truly is a scaredy cat!", pet.name, pet.name));
                            "ac2o_hunting_catch_stopFreakedOut_x"
                        }
                        Some(false) if h.miss == Miss::Gone => {
                            notes.push(format!("{} tried to catch a {name} but didn't succeed.", pet.name));
                            "ac2o_hunting_catch_stopConfused_x"
                        }
                        // (Missed, and something else caught instead; or no prey at all.)
                        caught => {
                            let (key, junk_name) = match caught {
                                None => h.prey.as_ref().map(|p| (p.1.clone(), p.2.clone())).unwrap_or_default(),
                                _ => {
                                    let (k, n, _) = junk(&mut rng);
                                    (k, n)
                                }
                            };
                            notes.push(format!("Unfortunately, {} didn't catch that tricky critter. All {} managed to catch was a {junk_name}.", pet.name, pet.name));
                            give(&mut commands, e, inv, key, junk_name, 0);
                            "ac2o_hunting_catch_stopSuccess_carry_x"
                        }
                    };
                    // (The prey off, caught or fled.)
                    if let Some(o) = h.prey.as_ref().and_then(|p| p.0) {
                        commands.entity(o).try_despawn();
                    }
                    add_fun(&mut commands, e);
                    anim.replay(stop.into());
                    h.t = 0.0;
                    h.stage = Stage::Done;
                }
            }
            // Digging it up (a dog).
            Stage::Digging => {
                h.t += dt;
                let start = len(&mut lib, &format!("{k}_digStart_x"), 1.0);
                if h.clip == 0 && h.t >= start {
                    h.clip = 1;
                    anim.play(format!("{k}_digLoop_x"));
                }
                if h.clip == 1 && h.t >= start + DIG_SECS {
                    h.clip = 2;
                    anim.replay(format!("{k}_digStop_x"));
                    // The hole it's left (the pack's, in its dug state: `SetHoleVisiable`'s `full`).
                    if let Some(o) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("dogDigHole")).map(|c| c.objd) {
                        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                        let parts = assets.object(&mut ctx, o);
                        info!("pet hunting: a hole dug ({} parts)", parts.len());
                        let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 0.2);
                        let hole = crate::objects::spawn_parts(&mut commands, &parts, Transform::from_translation(at).with_rotation(tf.rotation));
                        commands.entity(hole).insert((DugHole(now + HOLE_MINUTES), DespawnOnExit(crate::AppState::InGame)));
                    }
                    // What it's found: a gem, some metal or a rock, the rarer the better it is.
                    let found = rng.random::<f32>() < chance(FIND, skill.level);
                    let level = skill.whole() as u8;
                    let finds: Vec<&s3bake::gamedata::CollectibleInfo> = ui
                        .data
                        .collectibles
                        .iter()
                        .filter(|c| matches!(c.kind, s3bake::gamedata::CollectKind::Gem | s3bake::gamedata::CollectKind::Metal | s3bake::gamedata::CollectKind::SpaceRock) && c.level <= level)
                        .collect();
                    let weight = |c: &&s3bake::gamedata::CollectibleInfo| match c.rarity.as_str() {
                        "Common" => 10.0,
                        "Uncommon" => 3.0 + level as f32 * 0.3,
                        "Rare" => 1.0 + level as f32 * 0.3,
                        _ => 0.3 + level as f32 * 0.2,
                    };
                    match finds.choose_weighted(&mut rng, weight).ok().filter(|_| found) {
                        Some(c) => {
                            let words = match c.kind {
                                s3bake::gamedata::CollectKind::Gem => "found a gem!",
                                s3bake::gamedata::CollectKind::Metal => "found some metal!",
                                _ => "found a rock!",
                            };
                            notes.push(format!("{} {words}", pet.name));
                            let worth = rng.random_range(c.min_price.min(c.max_price)..=c.max_price.max(c.min_price)) as i64;
                            give_find(&mut commands, e, inv, c.key.clone(), c.name.clone(), worth);
                            skill.found += 1;
                        }
                        None => {
                            notes.push(format!("{} found something...interesting...", pet.name));
                            let (key, name, _) = junk(&mut rng);
                            give(&mut commands, e, inv, key, name, 0);
                        }
                    }
                    h.t = 0.0;
                    h.stage = Stage::Done;
                }
            }
            // Over: fun had, free again.
            Stage::Done => {
                h.t += dt;
                if h.t >= 2.5 {
                    commands.entity(e).remove::<(Hunt, PetBusy, crate::petlitters::Held)>().insert(crate::petneeds::PetThink::until(clock.minutes + 30.0));
                    anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
                    info!("pet hunting: {} is done hunting (Hunting {:.2})", pet.name, skill.level);
                }
            }
        }
    }
}

/// Fun from a hunt.
fn add_fun(commands: &mut Commands, e: Entity) {
    commands.entity(e).queue_silenced(|mut w: EntityWorldMut| {
        if let Some(mut m) = w.get_mut::<PetMotives>() {
            m.add(FUN, 30.0);
        }
    });
}

fn she(pet: &Pet) -> &'static str {
    if pet.female { "she" } else { "he" }
}

/// One of the failure objects, at random: its catalogue key, its name, what it's worth.
fn junk(rng: &mut impl Rng) -> (String, String, i64) {
    let (k, n) = FAILURE_OBJECTS[rng.random_range(0..FAILURE_OBJECTS.len())];
    (k.into(), n.into(), 0)
}

/// A catch into the pet's inventory (a catalogue object: the minor pet, or a failure object).
fn give(commands: &mut Commands, e: Entity, inv: Option<Mut<crate::inventory::Inventory>>, key: String, name: String, worth: i64) {
    match inv {
        Some(mut i) => i.add(crate::inventory::ItemKind::Object, &key, &name, 0, worth),
        None => crate::inventory::give(commands, e, crate::inventory::ItemKind::Object, key, name, 0, worth, 1),
    }
}

/// A find into the pet's inventory (a gem, metal or rock, as a Sim's).
fn give_find(commands: &mut Commands, e: Entity, inv: Option<Mut<crate::inventory::Inventory>>, key: String, name: String, worth: i64) {
    match inv {
        Some(mut i) => i.add(crate::inventory::ItemKind::Find, &key, &name, 0, worth),
        None => crate::inventory::give(commands, e, crate::inventory::ItemKind::Find, key, name, 0, worth, 1),
    }
}

/// How often a pet thinks of hunting of its own accord (game minutes), and the fun under which
/// it might (a Hunter any time, now and then).
const THINK_EVERY: std::ops::Range<f64> = 60.0..120.0;
const BORED: f32 = 30.0;

/// Cats and dogs of the household go hunting of their own accord now and then: bored, or a
/// Hunter; by day.
#[allow(clippy::type_complexity)]
fn autonomy(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    pets: Query<(Entity, &Pet, &PetMotives, Option<&PetTraits>), (With<HomePet>, Without<PetTask>, Without<PetBusy>, Without<crate::petlitters::Held>, Without<Hunt>, Without<crate::petcompany::Greeting>, Without<crate::petcompany::Greeted>, Without<crate::sim::Selected>)>,
    mut next: Local<std::collections::HashMap<Entity, f64>>,
) {
    if std::env::var_os("PET_HUNT_TEST").is_some() || !(7.0..20.0).contains(&clock.hour_f()) {
        return;
    }
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, pet, m, traits) in &pets {
        if !hunts(&pet.kind) || !pet.kind.starts_with('a') {
            continue;
        }
        let at = *next.entry(e).or_insert_with(|| now + rng.random_range(THINK_EVERY));
        if now < at {
            continue;
        }
        next.insert(e, now + rng.random_range(THINK_EVERY));
        let hunter = traits.is_some_and(|t| t.0.iter().any(|x| x == "HunterPet"));
        if (m.0[FUN] < BORED && rng.random_bool(0.5)) || (hunter && rng.random_bool(0.4)) {
            commands.entity(e).insert(GoHunt);
        }
    }
}

/// Dug holes go in a while.
fn holes(mut commands: Commands, clock: Res<crate::clock::GameClock>, holes: Query<(Entity, &DugHole)>) {
    for (e, h) in &holes {
        if clock.minutes >= h.0 {
            commands.entity(e).try_despawn();
        }
    }
}

/// `PET_HUNT_TEST=<name>[:<level>]` (tests): ten seconds in, the household's pet of that name
/// off hunting (at that Hunting level), the camera on it, its sniffing cut short.
fn hunting_test(mut commands: Commands, time: Res<Time>, clock: Res<crate::clock::GameClock>, mut pets: Query<(Entity, &Pet, &GlobalTransform, Option<&mut Hunt>), With<HomePet>>, mut cams: Query<&mut crate::camera::SimsCamera>, mut done: Local<bool>) {
    let Ok(v) = std::env::var("PET_HUNT_TEST") else { return };
    if time.elapsed_secs() < 10.0 {
        return;
    }
    let (name, level) = v.split_once(':').map_or((v.as_str(), None), |(a, b)| (a, b.parse::<f32>().ok()));
    let Some((e, pet, tf, h)) = pets.iter_mut().find(|p| p.1.name.eq_ignore_ascii_case(name)) else { return };
    if let Ok(mut c) = cams.single_mut() {
        c.look_at(tf.translation());
        c.distance = std::env::var("HUNT_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(5.0);
    }
    if let Some(mut h) = h
        && h.until > clock.minutes + 1.0
    {
        h.until = clock.minutes + 1.0;
    }
    if *done {
        return;
    }
    *done = true;
    let mut hunting = Hunting::new(&pet.kind);
    hunting.level = level.unwrap_or(5.0);
    info!("hunting test: {} hunts at {}", pet.name, hunting.level);
    commands.entity(e).insert((hunting, GoHunt));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunting_goes_as_the_game_tunes_it() {
        assert!((chance(FIND, 0.0) - 0.3).abs() < 1e-6 && (chance(CATCH, 10.0) - 0.8).abs() < 1e-6);
        assert!((chance(FIND, 5.0) - 0.55).abs() < 1e-6);
        let none: Vec<String> = Vec::new();
        assert_eq!(miss_weights(&none).map(|m| m.1), [1.0, 1.0, 1.0]);
        let skittish = vec!["SkittishPet".to_string()];
        assert_eq!(miss_weights(&skittish)[0].1, 3.0);
        let brave = vec!["AdventurousPet".to_string(), "HunterPet".to_string()];
        assert_eq!(miss_weights(&brave).map(|m| m.1), [0.0, 1.0, 3.0]);
        assert!(hunts("cc") && hunts("al") && !hunts("ah"));
        let mut h = Hunting::new("ac");
        assert_eq!(h.skill_name(), "CatHunting");
        h.add_points(1e9);
        assert_eq!(h.whole(), 10);
    }

    #[test]
    fn prey_is_weighted_by_level() {
        let p = |species: &str, hunt| s3bake::gamedata::MinorPetInfo { species: species.into(), hunt, ..Default::default() };
        let pets = vec![p("RodentRat", Some((1, 10, 10.0, 2.0))), p("RodentShrew", Some((5, 10, 2.0, 4.0))), p("BirdMacaw", None)];
        let at1 = prey_weights(&pets, 1);
        assert_eq!(at1.len(), 1, "a shrew's beyond a beginner, a macaw's no prey");
        assert_eq!(at1[0].1, 10.0);
        let at10 = prey_weights(&pets, 10);
        assert_eq!(at10.iter().map(|w| w.1).collect::<Vec<_>>(), vec![2.0, 4.0]);
    }
}
