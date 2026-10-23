//! Terrariums and their minor pets, the Pets pack's (`MinorPetTerrarium`, `MinorPet`): a turtle,
//! snake, lizard or rodent terrarium bought is stocked from its menu with one of the pack's
//! stockable species of its kind (`Stock`, a species at its most: the table's `MaxValue`, the
//! game's `StockCost`), the pet's own model in it. The pet hungers (`BehaviorAlarmCallbackHunger
//! Range`: a point and a quarter every five minutes) and eats from its food dish when hungry, a
//! serving at a time (`NumFoodServings` four, filled by `Fill Food`: §50 for a full dish, in
//! proportion for one part full, as `FillCost`); starved two days (`StarvationThreshold`) it
//! dies, and of old age after its lifetime (`BaseLifetimeRange` two to ten days, and one to three
//! more). The terrarium gets dirtier every twelve hours it's lived in (`IntervalToIncreaseDirtiness`)
//! and is cleaned from its menu ("Clean {pet}'s Terrarium"). "Watch {pet}" is fun (five to fifteen
//! minutes, the game's `kWatchLength`), "Show {pet} Info" tells what it is and how it's doing, and a
//! dead one is mourned and taken out. What lives where, its food and its days are kept in saves.
//!
//! A bird cage is the pack's terrarium for a bird (`BirdCage`): stocked with one of the stockable
//! small or large birds, fed, watched, cleaned with its own clips. A large bird is taught to talk
//! (`TeachToTalk`, ten minutes at a time: one of the ways it hasn't learned, Happy, Greetings,
//! Love, Angry, Scared or Songs, `kSpeechTypeIncrementPerTalk` a minute, four times as fast for
//! songs, a quarter more for each of the Sim's Charisma levels: `kCharismaModifierPerLevel`; not
//! in a bad mood, hungry: `kBadMoodThreshold`), and once it knows a way it's talked with
//! (`TalkWith`): each a Charisma point for the Sim (`kCharismaPointsGained`) and company, more the
//! more it can say (`kSocialDelta` by half a point for each way: `kSocialDeltaModifierPerSpeechType`).
//! What it's learned is kept in saves.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{GameObject, Notifications, ObjectKind};
use crate::sim::Sim;

pub struct TerrariumPlugin;

impl Plugin for TerrariumPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingTerrariums>().add_systems(Update, (kinds, restore, terrarium_test, requests, live, dirt, scurry).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The game's tuning.
pub const FILL_FOOD_COST: i64 = 50;
pub const FOOD_SERVINGS: u8 = 4;
const DIRT_HOURS: f64 = 12.0;
/// Hunger lost an hour (a point and a quarter each five minutes), and when it eats.
const HUNGER_PER_HOUR: f32 = 15.0;
const EATS_BELOW: f32 = 40.0;
const STARVES_DAYS: f64 = 2.0;
/// How long a Sim watches (minutes), and the fun of it an hour.
pub const WATCH_MINUTES: [f32; 2] = [5.0, 15.0];
pub const WATCH_FUN: f32 = 90.0;
/// How long a talk with a bird, or a lesson, takes (minutes: `kDurationSimMinutes`).
const TALK_MINUTES: f32 = 10.0;

/// How far up its terrarium a pet sits (its tank's floor, over the stand), as a part of it.
pub const FLOOR_AT: f32 = 0.55;

/// The menu's words, the game's.
pub const STOCK: &str = "Stock";
pub const TEACH_TO_TALK: &str = "Teach To Talk";

/// The ways a large bird can be taught to talk (the pack's `SpeechType`s), and how much a minute's
/// teaching gets it towards one (`kSpeechTypeIncrementPerTalk`: talking, songs), of a hundred.
pub const SPEECH: [&str; 6] = ["Happy", "Greetings", "Love", "Angry", "Scared", "Songs"];
const SPEECH_PER_MINUTE: [f32; 2] = [0.5, 2.0];
const CHARISMA_PER_LEVEL: f32 = 0.25;
const BAD_MOOD: f32 = 10.0;
const SOCIAL_PER_WAY: f32 = 5.0 * 0.5;

/// A bird's teaching so far: a minute's worth at a way of talking by a Sim of so much Charisma.
pub fn taught(progress: f32, way: &str, minutes: f32, charisma: u32) -> f32 {
    let rate = SPEECH_PER_MINUTE[(way == "Songs") as usize] * (1.0 + CHARISMA_PER_LEVEL * charisma as f32);
    (progress + rate * minutes).min(100.0)
}

/// A terrarium's kind of pet (its script's: `TerrariumTurtle`…, kept as it's set out), as the
/// table's tab names it.
#[derive(Component, Clone, Copy)]
pub struct TerrariumKind(pub &'static str);

pub fn kind_of_script(script: &str) -> Option<&'static str> {
    let script = script.to_ascii_lowercase();
    [("terrariumturtle", "Turtles"), ("terrariumsnake", "Snakes"), ("terrariumlizard", "Lizards"), ("terrariumrodent", "Rodents"), ("birdcage", "Birds")].into_iter().find(|(s, _)| script.ends_with(s)).map(|x| x.1)
}

/// Terrariums set out learn their kind.
fn kinds(mut commands: Commands, q: Query<(Entity, &GameObject), Without<TerrariumKind>>, data: Option<Res<crate::baked::Baked>>) {
    let Some(data) = data else { return };
    for (e, o) in &q {
        if !matches!(o.kind, ObjectKind::Terrarium | ObjectKind::BirdCage) {
            continue;
        }
        let kind = if o.kind == ObjectKind::BirdCage { "Birds" } else { data.0.catalog.iter().find(|c| c.objd == o.objd).and_then(|c| kind_of_script(&c.script)).unwrap_or("Turtles") };
        commands.entity(e).insert(TerrariumKind(kind));
    }
}

/// The clips' word for a kind (`o_minorPet_…_turtle`).
fn clip_word(kind: &str) -> &'static str {
    match kind {
        "Snakes" => "snake",
        "Lizards" => "lizard",
        "Rodents" => "rodent",
        _ => "turtle",
    }
}

/// The species a terrarium of a kind can be stocked with, cheapest first.
pub fn stockable<'a>(ui: &'a crate::icons::GameUi, kind: &str) -> Vec<&'a s3bake::gamedata::MinorPetInfo> {
    // (A bird cage takes the small birds and the large.)
    let fits = |k: &str| k == kind || (kind == "Birds" && k.ends_with("Birds"));
    let mut v: Vec<_> = ui.data.minor_pets.iter().filter(|p| fits(&p.kind) && p.stockable && !p.name.is_empty()).collect();
    v.sort_by_key(|p| (p.max_value, p.name.clone()));
    v
}

/// A species as "Stock" offers it: "Red-Eared Slider Turtle - §8".
pub fn stock_label(p: &s3bake::gamedata::MinorPetInfo) -> String {
    format!("{} - §{}", p.name, p.max_value)
}

/// The species a "Stock" choice names, by its label.
pub fn stock_choice(ui: &crate::icons::GameUi, label: &str) -> Option<String> {
    ui.data.minor_pets.iter().find(|p| p.stockable && stock_label(p) == label).map(|p| p.species.clone())
}

/// What filling the dish costs, as full as it is (`FillCost`).
pub fn fill_cost(food: u8) -> i64 {
    (FILL_FOOD_COST as f32 * (1.0 - food as f32 / FOOD_SERVINGS as f32)).round() as i64
}

/// A terrarium's pet: its species, what it's worth, its hunger (100 full), the servings in its
/// dish, when it last ate and when it dies of old age (game minutes), and whether it's died.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct TerrariumPet {
    pub species: String,
    pub worth: i32,
    pub hunger: f32,
    pub food: u8,
    pub ate_at: f64,
    pub dies_at: f64,
    pub dirt_at: f64,
    #[serde(default)]
    pub dead: bool,
    /// A large bird's ways of talking, as far as it's been taught each (of a hundred).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speech: Vec<(String, f32)>,
    #[serde(skip)]
    model: Option<Entity>,
}

impl TerrariumPet {
    /// What it's called: its species' name.
    pub fn called(&self, ui: &crate::icons::GameUi) -> String {
        ui.data.minor_pets.iter().find(|p| p.species == self.species).map_or_else(|| self.species.clone(), |p| p.name.clone())
    }

    /// A large bird (that can be taught to talk).
    pub fn talker(&self, ui: &crate::icons::GameUi) -> bool {
        ui.data.minor_pets.iter().any(|p| p.species == self.species && p.kind == "LargeBirds")
    }

    /// How far it's been taught a way of talking.
    pub fn progress(&self, way: &str) -> f32 {
        self.speech.iter().find(|s| s.0 == way).map_or(0.0, |s| s.1)
    }

    /// The ways it can talk (learned through).
    pub fn known(&self) -> Vec<&'static str> {
        SPEECH.into_iter().filter(|w| self.progress(w) >= 100.0).collect()
    }

    /// Those it could still be taught.
    pub fn unknown(&self) -> Vec<&'static str> {
        SPEECH.into_iter().filter(|w| self.progress(w) < 100.0).collect()
    }
}

/// An empty terrarium's dish (filled before there's a pet: the game's `FillFood` "no pet").
#[derive(Component, Clone, Copy, Default)]
pub struct TerrariumFood(pub u8);

/// A Sim done at a terrarium.
#[derive(Component, Clone)]
pub enum TerrariumRequest {
    Stock(Entity, String),
    Fill(Entity),
    TakeOut(Entity),
    /// A large bird talked with, or (a way of talking) taught.
    Talk(Entity, Option<String>),
}

/// A terrarium's pet as saved, by where the terrarium stands.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedTerrarium {
    pub at: [f32; 3],
    pub pet: TerrariumPet,
}

#[derive(Resource, Default)]
pub struct PendingTerrariums(pub Vec<SavedTerrarium>);

/// The pet's model, in its terrarium.
#[derive(Component)]
struct InTerrarium {
    kind: &'static str,
}

fn spawn_model(commands: &mut Commands, terrarium: Entity, obj: &GameObject, (pet, name, ui): (&TerrariumPet, &str, &crate::icons::GameUi), data: &crate::baked::Baked, assets: &mut crate::objects::ObjectAssets, ctx: &mut crate::objects::AssetCtx) -> Option<Entity> {
    // (The species' own object: by its name, or its instance's.)
    let objd = data
        .0
        .catalog
        .iter()
        .filter(|c| c.script.to_ascii_lowercase().contains("objects.minorpets.minorpet"))
        .find(|c| c.name.eq_ignore_ascii_case(name) || c.instance_name.eq_ignore_ascii_case(&pet.species))?
        .objd;
    let parts = assets.object(ctx, objd);
    // (In the terrarium's own slot for it, where it has one; else on its tank's floor.)
    let slot = crate::surfaces::slots(&ui.data, obj.objd.2).and_then(|s| s.first());
    let at = slot.map_or(Vec3::new(obj.center.x, obj.height * FLOOR_AT, obj.center.y), |s| Vec3::from(s.pos));
    info!("terrarium: {} in its terrarium at {at:?} (slot {})", pet.species, slot.is_some());
    let tf = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(slot.map_or(0.0, |s| s.yaw)));
    let e = crate::objects::spawn_parts(commands, &parts, tf);
    let kind = data.0.catalog.iter().find(|c| c.objd == objd).map(|c| c.script.to_ascii_lowercase()).map_or("Turtles", |s| {
        if s.contains("minorpetbird") {
            "Birds"
        } else if s.ends_with("snake") {
            "Snakes"
        } else if s.ends_with("lizard") {
            "Lizards"
        } else if s.ends_with("rodent") {
            "Rodents"
        } else {
            "Turtles"
        }
    });
    commands.entity(e).insert((ChildOf(terrarium), InTerrarium { kind }));
    if pet.dead {
        belly_up(commands, e);
    }
    Some(e)
}

/// A dead pet: still, on its back.
fn belly_up(commands: &mut Commands, model: Entity) {
    commands.entity(model).remove::<InTerrarium>().queue_silenced(|mut e: EntityWorldMut| {
        if let Some(mut tf) = e.get_mut::<Transform>() {
            tf.rotation = Quat::from_rotation_z(std::f32::consts::PI);
        }
    });
}

/// The pet moving about its terrarium now and then (turning to look about).
fn scurry(time: Res<Time>, clock: Res<crate::clock::GameClock>, mut q: Query<(&InTerrarium, &mut Transform)>) {
    let night = !(6.0..21.0).contains(&clock.hour_f());
    for (p, mut tf) in &mut q {
        if night {
            continue;
        }
        let speed = if p.kind == "Turtles" { 0.15 } else { 0.5 };
        let t = time.elapsed_secs() * speed;
        tf.rotation = Quat::from_rotation_y((t.sin() * 1.3) + (t * 0.37).cos());
    }
}

/// The species a terrarium's stocked with comes to live there, paid for; its dish filled; its
/// pet taken out once it's died.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn requests(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &TerrariumRequest, Option<&mut crate::interact::Skills>, Option<&mut crate::sim::Motives>)>,
    mut terrariums: Query<(&GameObject, Option<&mut TerrariumPet>, Option<&mut TerrariumFood>)>,
    (data, ui, mut assets): (Option<Res<crate::baked::Baked>>, Option<Res<crate::icons::GameUi>>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut notes: ResMut<Notifications>,
    clock: Res<crate::clock::GameClock>,
    mut household: Option<ResMut<crate::interact::Household>>,
) {
    let (Some(data), Some(ui)) = (data, ui) else { return };
    let mut rng = rand::rng();
    for (e, sim, req, skills, motives) in &mut sims {
        commands.entity(e).remove::<TerrariumRequest>();
        match req {
            TerrariumRequest::Stock(t, species) => {
                let Ok((obj, None, food)) = terrariums.get_mut(*t) else { continue };
                let Some(info) = ui.data.minor_pets.iter().find(|p| &p.species == species) else { continue };
                let price = info.max_value as i64;
                let Some(h) = household.as_deref_mut().filter(|h| h.funds >= price) else {
                    notes.push("You can't afford that.");
                    continue;
                };
                h.funds -= price;
                let days = rng.random_range(2..=10) + rng.random_range(1..=3);
                let mut pet = TerrariumPet {
                    species: species.clone(),
                    worth: rng.random_range(info.min_value..=info.max_value.max(info.min_value)),
                    hunger: 100.0,
                    food: food.map_or(0, |f| f.0),
                    ate_at: clock.minutes,
                    dies_at: clock.minutes + days as f64 * 1440.0,
                    dirt_at: clock.minutes + DIRT_HOURS * 60.0,
                    dead: false,
                    speech: Vec::new(),
                    model: None,
                };
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                pet.model = spawn_model(&mut commands, *t, obj, (&pet, &info.name, &ui), &data, &mut assets, &mut ctx);
                let home = if info.kind.ends_with("Birds") { "bird cage" } else { "terrarium" };
                notes.push(format!("{} stocked the {home} with {} {}.", sim.first, if info.name.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" }, info.name));
                commands.entity(*t).remove::<TerrariumFood>().insert(pet);
            }
            TerrariumRequest::Fill(t) => {
                let Ok((_, pet, food)) = terrariums.get_mut(*t) else { continue };
                let now = pet.as_ref().map(|p| p.food).or(food.as_ref().map(|f| f.0)).unwrap_or(0);
                let cost = fill_cost(now);
                let Some(h) = household.as_deref_mut().filter(|h| h.funds >= cost) else {
                    notes.push("Not enough funds.");
                    continue;
                };
                h.funds -= cost;
                match (pet, food) {
                    (Some(mut p), _) => p.food = FOOD_SERVINGS,
                    (None, Some(mut f)) => f.0 = FOOD_SERVINGS,
                    (None, None) => {
                        commands.entity(*t).insert(TerrariumFood(FOOD_SERVINGS));
                    }
                }
            }
            TerrariumRequest::Talk(t, way) => {
                let Ok((_, Some(mut p), _)) = terrariums.get_mut(*t) else { continue };
                if p.dead || !p.talker(&ui) {
                    continue;
                }
                let name = p.called(&ui);
                // (Hungry, it's in no mood: it won't.)
                if p.hunger < BAD_MOOD {
                    notes.push(format!("The {name} is in a bad mood and won't talk."));
                    continue;
                }
                let charisma = skills.as_deref().map_or(0, |s| s.level("Charisma"));
                if let Some(w) = way.as_deref().and_then(|w| SPEECH.into_iter().find(|s| *s == w)) {
                    let before = p.progress(w);
                    let now = taught(before, w, TALK_MINUTES, charisma);
                    match p.speech.iter_mut().find(|s| s.0 == w) {
                        Some(s) => s.1 = now,
                        None => p.speech.push((w.to_string(), now)),
                    }
                    if before < 100.0 && now >= 100.0 {
                        notes.push(format!("{} taught the {name} to talk: {w}!", sim.first));
                    }
                }
                if let Some(mut s) = skills {
                    s.add_points("Charisma", 1.0);
                }
                if let Some(mut m) = motives {
                    m.add(crate::sim::SOCIAL, SOCIAL_PER_WAY * p.known().len() as f32);
                }
            }
            TerrariumRequest::TakeOut(t) => {
                let Ok((_, Some(p), _)) = terrariums.get(*t) else { continue };
                if !p.dead {
                    continue;
                }
                if let Some(m) = p.model {
                    commands.entity(m).try_despawn();
                }
                notes.push(format!("{} laid the {} to rest.", sim.first, p.called(&ui)));
                commands.entity(*t).remove::<TerrariumPet>();
            }
        }
    }
}

/// `TERRARIUM_TEST=1` (tests): the first terrarium out stocked with the cheapest species of its
/// kind by the selected Sim (`TERRARIUM_SPECIES=<species>`: that one), and its dish filled, the
/// camera on it. `TERRARIUM_TEACH=<way>`: a bird taught that way of talking, then talked with.
#[allow(clippy::too_many_arguments)]
fn terrarium_test(
    mut commands: Commands,
    time: Res<Time>,
    (mut done, mut taught): (Local<bool>, Local<u8>),
    mut sel: Query<(Entity, &mut crate::interact::ActionQueue), With<crate::sim::Selected>>,
    q: Query<(Entity, &TerrariumKind, &GlobalTransform), Without<TerrariumPet>>,
    stocked: Query<(Entity, &GameObject), With<TerrariumPet>>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
) {
    if std::env::var_os("TERRARIUM_TEST").is_none() || time.elapsed_secs() < 8.0 {
        return;
    }
    if *done {
        // (Taught, once the bird's in; then talked with.)
        if let (Ok(way), Ok((_, mut queue)), Some((t, o))) = (std::env::var("TERRARIUM_TEACH"), sel.single_mut(), stocked.iter().next())
            && *taught < 2
            && (*taught == 0 || queue.0.is_empty())
        {
            queue.0.clear();
            let (label, def) = if *taught == 0 { (TEACH_TO_TALK, 4) } else { ("Talk With", 3) };
            info!("terrarium test: {label} ({way})");
            let mut a = crate::interact::Action::new(label, crate::interact::ActionKind::Object { target: t, def }, false);
            a.fish_choice = Some((way, 0));
            let _ = o;
            queue.push_player(a);
            *taught += 1;
        }
        return;
    }
    let (Ok((s, _)), Some((t, kind, tf)), Some(ui)) = (sel.single(), q.iter().next(), ui) else { return };
    let want = std::env::var("TERRARIUM_SPECIES").ok();
    let Some(p) = stockable(&ui, kind.0).into_iter().find(|p| want.as_ref().is_none_or(|w| p.species.eq_ignore_ascii_case(w))).map(|p| p.species.clone()) else { return };
    *done = true;
    info!("terrarium test: stocking with {p}");
    commands.entity(s).insert(TerrariumRequest::Stock(t, p));
    if let Ok(mut c) = cams.single_mut() {
        c.look_at(tf.translation());
        c.distance = std::env::var("TERRARIUM_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(2.2);
        c.pitch = std::env::var("TERRARIUM_PITCH").ok().and_then(|v| v.parse().ok()).unwrap_or(0.35);
    }
}

/// The pets hunger, eat from their dishes, and die starved or old.
fn live(mut commands: Commands, time: Res<Time>, clock: Res<crate::clock::GameClock>, mut pets: Query<&mut TerrariumPet>, mut notes: ResMut<Notifications>, ui: Option<Res<crate::icons::GameUi>>) {
    let hours = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed] / 60.0;
    for mut p in &mut pets {
        if p.dead {
            continue;
        }
        p.hunger = (p.hunger - HUNGER_PER_HOUR * hours).max(0.0);
        if p.hunger < EATS_BELOW && p.food > 0 {
            p.food -= 1;
            p.hunger = 100.0;
            p.ate_at = clock.minutes;
        }
        let starved = clock.minutes - p.ate_at > STARVES_DAYS * 1440.0;
        if starved || clock.minutes > p.dies_at {
            p.dead = true;
            let name = ui.as_deref().map_or_else(|| p.species.clone(), |u| p.called(u));
            let home = if p.species.starts_with("Bird") { "cage" } else { "terrarium" };
            notes.push(if starved { format!("The {name} in the {home} has died: it wasn't fed.") } else { format!("The {name} in the {home} has died of old age.") });
            if let Some(m) = p.model {
                belly_up(&mut commands, m);
            }
        }
    }
}

/// A lived-in terrarium gets dirtier every twelve hours.
fn dirt(clock: Res<crate::clock::GameClock>, mut q: Query<(&mut TerrariumPet, Option<&mut crate::grime::Grime>)>) {
    for (mut p, grime) in &mut q {
        if p.dead || clock.minutes < p.dirt_at {
            continue;
        }
        p.dirt_at = clock.minutes + DIRT_HOURS * 60.0;
        if let Some(mut g) = grime {
            g.0 = (g.0 + crate::grime::NEEDS_CLEANING).min(100.0);
        }
    }
}

/// Terrariums' pets from a save, back once the terrariums are out.
#[allow(clippy::too_many_arguments)]
fn restore(
    mut commands: Commands,
    mut pending: ResMut<PendingTerrariums>,
    terrariums: Query<(Entity, &GameObject, &Transform), Without<TerrariumPet>>,
    (data, ui, mut assets): (Option<Res<crate::baked::Baked>>, Option<Res<crate::icons::GameUi>>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    if pending.0.is_empty() {
        return;
    }
    let (Some(data), Some(ui)) = (data, ui) else { return };
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    pending.0.retain(|s| {
        let at = Vec3::from(s.at);
        let Some((e, obj, _)) = terrariums.iter().filter(|(_, o, _)| matches!(o.kind, ObjectKind::Terrarium | ObjectKind::BirdCage)).find(|(_, _, tf)| tf.translation.distance(at) < 0.3) else {
            return true;
        };
        let mut pet = s.pet.clone();
        let name = pet.called(&ui);
        pet.model = spawn_model(&mut commands, e, obj, (&pet, &name, &ui), &data, &mut assets, &mut ctx);
        commands.entity(e).insert(pet);
        false
    });
}

/// The terrariums' pets, for saving.
pub fn saved(q: &Query<(&TerrariumPet, &Transform), (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>)>) -> Vec<SavedTerrarium> {
    q.iter().map(|(p, tf)| SavedTerrarium { at: tf.translation.to_array(), pet: p.clone() }).collect()
}

/// "Show {pet} Info": what it is, what it's worth and how it's doing.
pub fn info(p: &TerrariumPet, ui: &crate::icons::GameUi, now: f64) -> String {
    let i = ui.data.minor_pets.iter().find(|x| x.species == p.species);
    let name = p.called(ui);
    let how = if p.dead {
        "It has died.".to_string()
    } else {
        let hunger = if p.hunger < EATS_BELOW { "hungry" } else { "well fed" };
        let days = ((p.dies_at - now) / 1440.0).max(0.0);
        let talks = if p.talker(ui) {
            let known = p.known();
            if known.is_empty() { " It hasn't learned to talk.".to_string() } else { format!(" It can talk: {}.", known.join(", ")) }
        } else {
            String::new()
        };
        format!("It is {hunger}; its dish has {} of {FOOD_SERVINGS} servings.{talks} {}", p.food, if days < 1.0 { "It is very old." } else { "" })
    };
    format!("{name} ({}, worth §{})\n{}\n{how}", i.map_or("", |i| i.rarity.as_str()), p.worth, i.map_or("", |i| i.desc.as_str()))
}

/// The Sim's side of feeding (the pack's own clip), and its kind's watching clip.
pub fn watch_clip(kind: &str) -> String {
    format!("a2o_terrarium_watch_comeCloser_{}", clip_word(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dish_costs_what_it_lacks() {
        assert_eq!(fill_cost(0), 50);
        assert_eq!(fill_cost(2), 25);
        assert_eq!(fill_cost(FOOD_SERVINGS), 0);
    }

    #[test]
    fn stocking_offers_the_kinds_stockable_species_at_their_most() {
        let pet = |kind: &str, species: &str, name: &str, max: i32, stockable: bool| s3bake::gamedata::MinorPetInfo { kind: kind.into(), species: species.into(), name: name.into(), max_value: max, stockable, ..Default::default() };
        let data = s3bake::GameDataBaked { minor_pets: vec![pet("Turtles", "A", "Box Turtle", 30, true), pet("Turtles", "B", "Slider", 8, true), pet("Turtles", "C", "Rare", 150, false), pet("Snakes", "D", "Garter", 10, true)], ..Default::default() };
        let ui = crate::icons::GameUi::for_tests(data);
        let list: Vec<String> = stockable(&ui, "Turtles").iter().map(|p| stock_label(p)).collect();
        assert_eq!(list, vec!["Slider - §8", "Box Turtle - §30"]);
        assert_eq!(stock_choice(&ui, "Box Turtle - §30").as_deref(), Some("A"));
        assert!(stock_choice(&ui, "Rare - §150").is_none());
    }

    #[test]
    fn a_bird_learns_to_talk_by_the_packs_tuning() {
        // (Half a point a minute talking, two singing; a quarter more a Charisma level.)
        assert_eq!(taught(0.0, "Happy", 10.0, 0), 5.0);
        assert_eq!(taught(0.0, "Songs", 10.0, 0), 20.0);
        assert_eq!(taught(0.0, "Songs", 10.0, 4), 40.0);
        assert_eq!(taught(95.0, "Love", 10.0, 10), 100.0);
        let mut p = TerrariumPet { species: "BirdCockatoo".into(), worth: 22, hunger: 100.0, food: 4, ate_at: 0.0, dies_at: 0.0, dirt_at: 0.0, dead: false, speech: vec![("Songs".into(), 100.0), ("Love".into(), 40.0)], model: None };
        assert_eq!(p.known(), vec!["Songs"]);
        assert_eq!(p.unknown().len(), 5);
        p.speech.clear();
        assert!(p.known().is_empty());
    }
}
