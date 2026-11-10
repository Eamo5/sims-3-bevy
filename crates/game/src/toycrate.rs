//! The pet toy crate, the Pets pack's (`PetToyCrate`): bought, it comes with three of the cats'
//! toys (`kMedatorCatToyNames`: a catnip mouse, a paper, yarn, foil or plastic ball) and three of
//! the dogs' (`kMedatorDogToyNames`: a newspaper, a bone, a hot dog), and looks as full as it is
//! (its `empty`, `partiallyFull` and `full` geometry states, full from six). A bored cat or dog
//! goes to it and takes a toy of its kind out (`GetToy`: `ac2o_petToyCrate_pickUpToy`...),
//! and plays with it there, a cat batting it about, a dog chewing it; a played pet can be told
//! to ("Get Toy"). Toys left lying about are put back by a Sim ("Put Away Toys" on the crate:
//! each picked up off the floor and back in it), a Neat Sim of their own accord. What's in a crate
//! is kept in saves.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::SliceRandom;

use crate::PlayMode;
use crate::interact::{ActionKind, GameObject, ObjectKind};
use crate::petneeds::{FUN, PetDo, PetMotives, PetTask};
use crate::pets::{HomePet, Pet, PetAnim, PetBusy};

pub struct ToyCratePlugin;

impl Plugin for ToyCratePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (stock, looks, want_toys, fetch, tidy_up, toycrate_test).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The game's tuning: the toys a crate holds and comes with, and how many make it look full.
pub const CAT_TOYS: [&str; 5] = ["petToyMouse", "petToyBallPaper", "petToyBallYarn", "petToyBallFoil", "petToyBallPlastic"];
pub const DOG_TOYS: [&str; 3] = ["chewToyNewspaper", "chewToyBone", "chewToyHotDog"];
const STOCKED: usize = 3;
const FULL_AT: usize = 6;
pub const PUT_AWAY: &str = "Put Away Toys";
pub const GET_TOY: &str = "Get Toy";
/// The game's words with nothing of a pet's kind in it (`NoToys`).
pub const NO_TOYS: &str = "No appropriate toys are available.";
/// How long a Sim's pick-up takes (game minutes), and their clip.
pub const PICK_UP_MINUTES: f32 = 1.5;
pub const PICK_UP_CLIP: &str = "a_genericSwipe_pickUp_floor_x";
/// How bored a pet's to go for a toy, how often it thinks of it (game minutes).
const BORED: f32 = -20.0;
const THINK: f64 = 20.0;

/// What's in a crate (the toys' catalogue instance names).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ToyCrate(pub Vec<String>);

/// The geometry state a crate's drawn in (`empty`, `partiallyFull`, `full`).
#[derive(Component)]
struct Shown(&'static str);

/// A cat's toy or a dog's (by its instance name; none: neither).
pub fn toy_kind(instance: &str) -> Option<bool> {
    if CAT_TOYS.iter().any(|t| t.eq_ignore_ascii_case(instance)) {
        Some(true)
    } else if DOG_TOYS.iter().any(|t| t.eq_ignore_ascii_case(instance)) {
        Some(false)
    } else {
        None
    }
}

/// Whether a pet can have a toy from a crate: grown cats and dogs (the crate's clips' kinds).
pub fn takes_toys(kind: &str) -> Option<bool> {
    match kind {
        "ac" => Some(true),
        "ad" | "al" => Some(false),
        _ => None,
    }
}

/// A newly bought crate's toys: so many of each (as many as there are, if fewer).
pub fn stocked(rng: &mut impl Rng) -> Vec<String> {
    let mut cats: Vec<&str> = CAT_TOYS.to_vec();
    let mut dogs: Vec<&str> = DOG_TOYS.to_vec();
    cats.shuffle(rng);
    dogs.shuffle(rng);
    cats.into_iter().take(STOCKED).chain(dogs.into_iter().take(STOCKED)).map(String::from).collect()
}

/// How full a crate looks with so many toys in it.
pub fn state(n: usize) -> &'static str {
    match n {
        0 => "empty",
        n if n >= FULL_AT => "full",
        _ => "partiallyFull",
    }
}

/// Where a pet stands to take a toy out, and where the toy's put down (before the crate).
fn front(crate_tf: &Transform, by: f32) -> Vec2 {
    crate_tf.transform_point(Vec3::new(0.0, 0.0, by)).xz()
}

/// New crates (not the town's) stocked.
fn stock(mut commands: Commands, crates: Query<(Entity, &GameObject), (Without<ToyCrate>, Without<crate::visit::LotObject>)>) {
    let mut rng = rand::rng();
    for (e, o) in &crates {
        if o.kind == ObjectKind::PetToyCrate {
            commands.entity(e).insert(ToyCrate(stocked(&mut rng)));
        }
    }
}

/// A crate drawn as full as it is.
#[allow(clippy::type_complexity)]
fn looks(
    mut commands: Commands,
    crates: Query<(Entity, &GameObject, &ToyCrate, Option<&Shown>, Option<&crate::objects::Design>), Changed<ToyCrate>>,
    data: Option<Res<crate::baked::Baked>>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(data) = data else { return };
    for (e, o, c, shown, design) in &crates {
        let want = state(c.0.len());
        if shown.is_some_and(|s| s.0 == want) {
            continue;
        }
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let parts = assets.object_state_design(&mut ctx, o.objd, Some(s3pkg::fnv32(want)), design.map(|d| d.0));
        crate::objects::reshape_parts(&mut commands, e, &parts);
        commands.entity(e).insert(Shown(want));
    }
}

/// A pet off to a crate for a toy: how far along (on the way; taking it out, how long), and which
/// toy.
#[derive(Component)]
pub struct FetchingToy {
    pub crate_e: Entity,
    taking: Option<f32>,
}

impl FetchingToy {
    pub fn new(crate_e: Entity) -> Self {
        Self { crate_e, taking: None }
    }
}

/// A pet set off to a crate for a toy (its way there).
pub fn send(commands: &mut Commands, e: Entity, pet: &Pet, here: Vec2, crate_e: Entity, crate_tf: &Transform, grid: &crate::nav::NavGrid, building: Option<&crate::building::ActiveBuilding>, rng: &mut impl Rng) -> bool {
    let Some(task) = crate::petneeds::plan(pet, here, PetDo::Go, Some(front(crate_tf, 0.55)), None, grid, building, rng) else { return false };
    commands.entity(e).insert((task, FetchingToy::new(crate_e)));
    true
}

/// Bored cats and dogs with a crate of their kind's toys about go for one, now and then.
#[allow(clippy::type_complexity)]
fn want_toys(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    pets: Query<(Entity, &Pet, &PetMotives, &Transform), (With<HomePet>, Without<PetTask>, Without<PetBusy>, Without<FetchingToy>, Without<crate::petlitters::Held>, Without<crate::petcompany::Greeting>, Without<crate::sim::Selected>)>,
    crates: Query<(Entity, &ToyCrate, &Transform), Without<Pet>>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut next: Local<std::collections::HashMap<Entity, f64>>,
) {
    let Some(grid) = grid else { return };
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, pet, m, tf) in &pets {
        let Some(cat) = takes_toys(&pet.kind) else { continue };
        if m.0[FUN] >= BORED {
            continue;
        }
        let at = next.entry(e).or_insert(now);
        if now < *at {
            continue;
        }
        *at = now + THINK;
        let here = tf.translation.xz();
        let near = crates.iter().filter(|(_, c, ctf)| c.0.iter().any(|t| toy_kind(t) == Some(cat)) && ctf.translation.xz().distance(here) < 20.0).min_by(|a, b| a.2.translation.xz().distance(here).total_cmp(&b.2.translation.xz().distance(here)));
        if let Some((c, _, ctf)) = near
            && rng.random_bool(0.6)
            && send(&mut commands, e, pet, here, c, ctf, &grid, building.as_deref(), &mut rng)
        {
            info!("toy crate: {} goes for a toy", pet.name);
        }
    }
}

/// At the crate: a toy taken out (the pack's clip), put down before it, and played with.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn fetch(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib, catalog): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>, Res<crate::loading::Catalog>),
    mut pets: Query<(Entity, &Pet, &mut FetchingToy, &mut Transform, &mut PetAnim), Without<PetTask>>,
    mut crates: Query<(&mut ToyCrate, &Transform), Without<Pet>>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let Some(grid) = grid else { return };
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    for (e, pet, mut f, mut tf, mut anim) in &mut pets {
        let Some(cat) = takes_toys(&pet.kind) else {
            commands.entity(e).remove::<FetchingToy>();
            continue;
        };
        let Ok((mut c, ctf)) = crates.get_mut(f.crate_e) else {
            commands.entity(e).remove::<(FetchingToy, PetBusy)>();
            continue;
        };
        let clip = format!("{}2o_petToyCrate_pickUpToy_petToyCrate_x", pet.kind);
        match f.taking {
            None => {
                let i = c.0.iter().position(|t| toy_kind(t) == Some(cat));
                if tf.translation.xz().distance(front(ctf, 0.55)) > 0.8 || i.is_none() {
                    if i.is_none() && std::env::var_os("TOYCRATE_TEST").is_some() {
                        notes.push(NO_TOYS.to_string());
                    }
                    commands.entity(e).remove::<FetchingToy>();
                    continue;
                }
                let to = ctf.translation.xz() - tf.translation.xz();
                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                anim.replay(clip);
                f.taking = Some(0.0);
                commands.entity(e).insert(PetBusy);
            }
            Some(t) => {
                let t = t + dt;
                f.taking = Some(t);
                if t < lib.get(&data, &clip).map_or(2.0, |c| c.duration) {
                    continue;
                }
                commands.entity(e).remove::<(FetchingToy, PetBusy)>();
                let Some(i) = c.0.iter().position(|t| toy_kind(t) == Some(cat)) else { continue };
                let name = c.0.remove(i);
                let Some(key) = data.0.catalog.iter().find(|x| x.instance_name.eq_ignore_ascii_case(&name)).map(|x| x.objd) else { continue };
                // (Put down before the crate, between it and the pet.)
                let at2 = front(ctf, 0.3).lerp(tf.translation.xz(), 0.35);
                let at = Vec3::new(at2.x, tf.translation.y, at2.y);
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let Some(toy) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, at, rng.random_range(0.0..std::f32::consts::TAU)) else { continue };
                commands.entity(toy.entity).insert(crate::save::Bought);
                let here = tf.translation.xz();
                let otf = Transform::from_translation(at);
                let what = if cat { PetDo::Play(toy.entity, crate::pettoys::Toy::CatToy) } else { PetDo::Chew(toy.entity) };
                let to = (!cat).then(|| at2 + (here - at2).normalize_or(Vec2::Y) * 0.6);
                if let Some(task) = crate::petneeds::plan(pet, here, what, to, Some(otf), &grid, building.as_deref(), &mut rng) {
                    commands.entity(e).insert(task);
                }
                info!("toy crate: {} took out the {name}", pet.name);
            }
        }
    }
}

/// A crate's menu for a Sim: "Put Away Toys" with any lying about.
pub fn crate_options(crate_e: Entity, loose: &[Entity]) -> Vec<(String, ActionKind)> {
    match loose.first() {
        Some(t) => vec![(PUT_AWAY.to_string(), ActionKind::PutAwayToy { toy: *t, crate_e })],
        None => Vec::new(),
    }
}

/// The toys lying about (out of their crates): the crates' toys that are objects on the lot.
pub fn loose_toys<'a>(objects: impl Iterator<Item = (Entity, &'a GameObject)>, data: &crate::baked::Baked) -> Vec<Entity> {
    objects.filter(|(_, o)| data.0.catalog_entry(&o.objd).is_some_and(|c| toy_kind(&c.instance_name).is_some())).map(|(e, _)| e).collect()
}

/// A toy picked up off the floor (`PutAwayToys`): back in its crate, and the next one lying about
/// to be picked up after it.
pub fn put_away(commands: &mut Commands, sim: Entity, toy: Entity, crate_e: Entity) {
    commands.queue(move |w: &mut World| {
        let name = w.get::<GameObject>(toy).and_then(|o| w.resource::<crate::baked::Baked>().0.catalog_entry(&o.objd).map(|c| c.instance_name.clone()));
        let Some(name) = name else { return };
        if let Ok(e) = w.get_entity_mut(toy) {
            e.despawn();
        }
        if let Some(mut c) = w.get_mut::<ToyCrate>(crate_e) {
            c.0.push(name);
        }
        // (The next one, if there's another.)
        let next = {
            let data = w.resource::<crate::baked::Baked>().clone();
            let mut q = w.query::<(Entity, &GameObject)>();
            loose_toys(q.iter(w), &data).into_iter().next()
        };
        if let (Some(t), Some(mut queue)) = (next, w.get_mut::<crate::interact::ActionQueue>(sim)) {
            queue.0.push_back(crate::interact::Action::new(PUT_AWAY, ActionKind::PutAwayToy { toy: t, crate_e }, false));
        }
    });
}

/// A Neat Sim of the household puts toys away of their own accord, now and then.
#[allow(clippy::type_complexity)]
fn tidy_up(
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(&crate::sim::Sim, &mut crate::interact::ActionQueue), With<crate::sim::HouseholdMember>>,
    objects: Query<(Entity, &GameObject), Without<crate::visit::LotObject>>,
    crates: Query<Entity, With<ToyCrate>>,
    data: Res<crate::baked::Baked>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 60.0;
    let Some(crate_e) = crates.iter().next() else { return };
    let Some(toy) = loose_toys(objects.iter(), &data).into_iter().next() else { return };
    if let Some((_, mut q)) = sims.iter_mut().find(|(s, q)| q.0.is_empty() && !s.age.is_little() && s.age != crate::sim::Age::Child && s.traits.contains(&crate::life::Trait::Neat)) {
        q.0.push_back(crate::interact::Action::new(PUT_AWAY, ActionKind::PutAwayToy { toy, crate_e }, true));
    }
}

/// `TOYCRATE_TEST=<pet>` (tests): ten seconds in, a toy crate put down by that pet (if the lot's
/// none), and the pet sent to it; `TOYCRATE_TIDY=1`, the selected Sim then puts the toys away.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn toycrate_test(
    mut commands: Commands,
    time: Res<Time>,
    pets: Query<(Entity, &Pet, &Transform), With<HomePet>>,
    crates: Query<(Entity, &Transform), (With<ToyCrate>, Without<Pet>)>,
    objects: Query<(Entity, &GameObject), Without<crate::visit::LotObject>>,
    mut sel: Query<&mut crate::interact::ActionQueue, With<crate::sim::Selected>>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("TOYCRATE_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let Some((e, pet, tf)) = pets.iter().find(|p| p.1.name.eq_ignore_ascii_case(&name)) else { return };
    if let Ok(mut c) = cams.single_mut()
        && stage.0 < 3
    {
        let near = crates.iter().min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation)));
        c.look_at(near.map_or(tf.translation, |c| c.1.translation));
        c.distance = std::env::var("TOYCRATE_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(4.0);
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            if let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("petToyCrate")).map(|c| c.objd)
            {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let p = tf.translation + Vec3::new(2.0, 0.0, 0.0);
                crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0);
                info!("toy crate test: a crate at {p}");
            }
        }
        1 if now - stage.1 > 1.0 => {
            let (Some(grid), Some((c, ctf))) = (grid, crates.iter().min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation)))) else { return };
            *stage = (2, now);
            send(&mut commands, e, pet, tf.translation.xz(), c, ctf, &grid, building.as_deref(), &mut rand::rng());
            info!("toy crate test: {} sent", pet.name);
        }
        2 if std::env::var_os("TOYCRATE_TIDY").is_some() && now - stage.1 > 10.0 && !loose_toys(objects.iter(), &data).is_empty() && pets.get(e).is_ok() => {
            let Some((c, _)) = crates.iter().min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation))) else { return };
            *stage = (3, now);
            if let (Ok(mut q), Some(t)) = (sel.single_mut(), loose_toys(objects.iter(), &data).into_iter().next()) {
                q.push_player(crate::interact::Action::new(PUT_AWAY, ActionKind::PutAwayToy { toy: t, crate_e: c }, false));
                info!("toy crate test: tidying up");
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crate_comes_stocked_and_looks_as_full_as_it_is() {
        let toys = stocked(&mut rand::rng());
        assert_eq!(toys.iter().filter(|t| toy_kind(t) == Some(true)).count(), 3);
        assert_eq!(toys.iter().filter(|t| toy_kind(t) == Some(false)).count(), 3);
        assert_eq!((state(0), state(3), state(6)), ("empty", "partiallyFull", "full"));
        assert_eq!(toy_kind("chewToyBone"), Some(false));
        assert_eq!(toy_kind("dogBall"), None, "a bought toy isn't the crate's");
        assert_eq!((takes_toys("ac"), takes_toys("al"), takes_toys("cc"), takes_toys("ah")), (Some(true), Some(false), None, None));
        let t = Transform::from_xyz(1.0, 0.0, 1.0);
        assert!((front(&t, 0.5) - Vec2::new(1.0, 1.5)).length() < 1e-5);
    }
}
