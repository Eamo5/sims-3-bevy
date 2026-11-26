//! A cat's catch, as the Pets pack has it (`CatHuntingComponent`). Prey a cat has caught and
//! carried off lies out cold in its inventory; from there it may be "Present To..." someone
//! (`PresentTo`: anyone toddler or older of the family about the lot, or known to it within ten
//! metres, `kRadiusForValidSims`; greyed "There are no valid Sims nearby." with none). The cat
//! takes it up in its mouth (`ac2o_prey_pickUp`, the catch's tail or feathers showing, the game's
//! `accessoryCatHunt...`), carries it over and puts it down a metre off at their feet
//! (`kDistanceFromSimToPresent`, `ac2o_prey_putDown`), out cold on the floor (`catHuntRodent`...).
//! Within ten metres (`kMaxDistanceForSimToReact`) they react: a Sim pets the cat for it half the
//! time (`kHumanBaseChanceToPetCat`: the Slob a fifth likelier, the Snob a fifth and the Neat
//! three-tenths less, three-tenths likelier where the cat likes them 30 or more), the cat liking
//! them 20 more for it (`kLtrGainForPositiveReactionFromHuman`); otherwise they retch at it
//! (`ReactToPresentedObject`'s `Retch`), and it likes them 10 less. Or it's to "Drop Somewhere"
//! (`DropSomewhere`): by its bed four times in ten (`kChanceToDropAtBed`), by a plant four in ten
//! (`kChanceToDropAtPlant`), else anywhere about. Of its own accord a cat with a catch now and then
//! presents it to one of the family, the one it likes best seven times in ten
//! (`kChanceOfAutonomyChooseHighestLTR`), or drops it somewhere. A catch on the floor is "Put In
//! Inventory" by a Sim (the game's `PutInInventory`, a swipe: to keep or sell), "Eat"en by a cat or
//! dog (`PetEatPrey`: ten minutes a cat's, five a dog's, `kSimMinutesForCatToEatPrey`; a bird
//! Tastes Like Chicken to a cat), a hungry one of its own accord, or "Put Away" by a cat, back into
//! its inventory (`PetPutInInventory`). Kept in saves.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::inventory::{Inventory, ItemKind, Stack};
use crate::petneeds::{HUNGER, PetDo, PetMotives, PetTask};
use crate::pets::{HomePet, Pet, PetAnim, PetBusy};
use crate::sim::{Age, Sim};

pub struct PetPresentPlugin;

impl Plugin for PetPresentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (autonomy, present, at_prey, present_test).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words.
pub const PRESENT_TO: &str = "Present To...";
pub const DROP_SOMEWHERE: &str = "Drop Somewhere";
pub const PUT_IN_INVENTORY: &str = "Put In Inventory";
pub const EAT: &str = "Eat";
pub const PUT_AWAY: &str = "Put Away";
pub const NO_SIMS: &str = "There are no valid Sims nearby.";

/// The game's tuning: how far off it's put down, how near one must be to react, how far off
/// those it knows may be presented to (and the family about the lot); the chance of a Sim
/// petting the cat for it, the traits that change it and by how much, the liking that does and
/// by how much; what it's worth to the cat's liking either way; how often of its own accord it
/// presents to the one it likes best.
const PRESENT_FROM: f32 = 1.0;
const REACT_WITHIN: f32 = 10.0;
const KNOWN_WITHIN: f32 = 10.0;
const ABOUT_THE_LOT: f32 = 40.0;
const PET_CHANCE: f64 = 0.5;
const TRAIT_CHANCES: [(crate::life::Trait, f64); 3] = [(crate::life::Trait::Slob, 0.2), (crate::life::Trait::Snob, -0.2), (crate::life::Trait::Neat, -0.3)];
const LIKED: f32 = 30.0;
const LIKED_CHANCE: f64 = 0.3;
const GAIN: f32 = 20.0;
const LOSS: f32 = 10.0;
const BEST_LIKED: f64 = 0.7;
/// Where it drops a catch somewhere: by its bed, by a plant (how likely each, how near).
const BED_CHANCE: f64 = 0.4;
const PLANT_CHANCE: f64 = 0.4;
const DROP_BY: f32 = 1.0;
/// How long a cat and a dog take over eating one (game minutes), and what it's worth to hunger
/// (as a catch eaten there and then, `pethunting`).
const CAT_EAT_MINUTES: f64 = 10.0;
const DOG_EAT_MINUTES: f64 = 5.0;
const EAT_GAIN: f32 = 25.0;
/// When the clips have it in the mouth or out of it (seconds: their `SwapModel` events).
const PICKED_UP_AT: f32 = 0.433;
const PUT_DOWN_AT: f32 = 0.467;
/// The clips' slot for the mouth (`b__Mouth_slot`, by its hash).
const MOUTH_SLOT: u32 = 387_372_671;
/// How often a cat with a catch thinks of presenting it (game minutes), and how likely it is to
/// then; how hungry a pet must be to eat one lying about.
const THINK: f64 = 60.0;
const THINK_CHANCE: f64 = 0.3;
const HUNGRY: f32 = -30.0;

/// Who presents their catch: grown cats (the pack's `ac2o_prey` clips).
pub fn presents(kind: &str) -> bool {
    kind == "ac"
}

/// Whether an inventory stack is a catch (one of the pack's minor pets, not a failure object).
pub fn is_prey(st: &Stack, minor_pets: &[s3bake::gamedata::MinorPetInfo]) -> bool {
    st.kind == ItemKind::Object && minor_pets.iter().any(|p| p.species.eq_ignore_ascii_case(&st.key))
}

/// How a catch looks of its kind (the table's tab): out cold on the floor, and what shows of it
/// in a cat's mouth (`UpdateVisualState`'s world and pet-carry models).
pub fn looks(kind: &str) -> (&'static str, &'static str) {
    let k = kind.to_ascii_lowercase();
    if k.contains("bird") {
        ("catHuntBird", "accessoryCatHuntFeathers")
    } else if k.contains("lizard") {
        ("catHuntLizard", "accessoryCatHuntReptileTail")
    } else if k.contains("snake") {
        ("catHuntSnake", "accessoryCatHuntReptileTail")
    } else if k.contains("turtle") {
        ("catHuntTurtle", "accessoryCatHuntReptileTail")
    } else {
        ("catHuntRodent", "accessoryCatHuntRodentTail")
    }
}

/// Whether a catalogue object's a catch on the floor.
pub fn is_prey_model(instance: &str) -> bool {
    ["catHuntBird", "catHuntLizard", "catHuntSnake", "catHuntTurtle", "catHuntRodent"].iter().any(|m| m.eq_ignore_ascii_case(instance))
}

/// Who a cat may present to: toddlers and older, of the family about the lot, or known to it near
/// by.
pub fn can_present_to(age: Age, family: bool, known: bool, distance: f32) -> bool {
    age != Age::Baby && ((family && distance <= ABOUT_THE_LOT) || (known && distance <= KNOWN_WITHIN))
}

/// How likely a Sim's to pet the cat for its catch (`ShouldSimDoPositiveReaction`).
pub fn pet_chance(traits: &[crate::life::Trait], liking: f32) -> f64 {
    let mut p = PET_CHANCE;
    for (t, m) in TRAIT_CHANCES {
        if traits.contains(&t) {
            p += m;
        }
    }
    if liking >= LIKED {
        p += LIKED_CHANCE;
    }
    p.clamp(0.0, 1.0)
}

/// A catch on the floor: what it is (as it'll go into an inventory).
#[derive(Component, Clone, Debug)]
pub struct CaughtPrey(pub Stack);

/// A catch's model made one on the floor: its words and its menu's.
pub fn lay(commands: &mut Commands, e: Entity, item: Stack) {
    let name = item.name.clone();
    commands.entity(e).insert(CaughtPrey(item)).queue_silenced(move |mut w: EntityWorldMut| {
        if let Some(mut g) = w.get_mut::<GameObject>() {
            g.kind = ObjectKind::Prey;
            g.name = name;
        }
    });
}

/// A catch picked up off the floor by a Sim, into their inventory.
pub fn pocket(w: &mut World, sim: Entity, prey: Entity, _objd: s3bake::Key) {
    let Some(item) = w.get::<CaughtPrey>(prey).map(|p| p.0.clone()) else { return };
    if let Ok(e) = w.get_entity_mut(prey) {
        e.despawn();
    }
    let Ok(mut s) = w.get_entity_mut(sim) else { return };
    if s.get::<Inventory>().is_none() {
        s.insert(Inventory::default());
    }
    if let Some(mut inv) = s.get_mut::<Inventory>() {
        inv.put(item.clone());
    }
    let first = w.get::<Sim>(sim).map(|s| s.first.clone()).unwrap_or_default();
    info!("pet present: {first} put the {} in their inventory", item.name);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    PickUp,
    Going,
    PutDown,
}

/// A cat taking its catch to someone (or somewhere): the catch, how far along, what's in its
/// mouth, how many times it's set off after them again.
#[derive(Component, Debug)]
pub struct Presenting {
    to: Option<Entity>,
    item: Stack,
    stage: Stage,
    t: f32,
    prop: Option<Entity>,
    tries: u8,
    laid: bool,
    own_accord: bool,
}

impl Presenting {
    pub fn new(to: Option<Entity>, item: Stack, own_accord: bool) -> Self {
        Self { to, item, stage: Stage::PickUp, t: 0.0, prop: None, tries: 0, laid: false, own_accord }
    }
}

/// Where a cat stands to put it down for someone: a metre off them, on its side.
fn before(them: Vec2, here: Vec2) -> Vec2 {
    them + (here - them).normalize_or(Vec2::X) * PRESENT_FROM
}

/// Where a catch is dropped somewhere: by the cat's bed, by a plant, or a few metres off.
fn somewhere(here: Vec2, beds: &[Vec2], plants: &[Vec2], rng: &mut impl Rng) -> Vec2 {
    let near = |l: &[Vec2]| l.iter().copied().min_by(|a, b| a.distance(here).total_cmp(&b.distance(here)));
    if let Some(b) = near(beds).filter(|_| rng.random_bool(BED_CHANCE)) {
        return b + (here - b).normalize_or(Vec2::X) * DROP_BY;
    }
    if let Some(p) = plants.choose(rng).filter(|_| rng.random_bool(PLANT_CHANCE)) {
        return *p + (here - *p).normalize_or(Vec2::X) * DROP_BY;
    }
    here + Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU)) * rng.random_range(3.0..7.0)
}

/// The catch's kind (the table's tab) by its species key.
fn kind_of<'a>(key: &str, minor_pets: &'a [s3bake::gamedata::MinorPetInfo]) -> Option<&'a s3bake::gamedata::MinorPetInfo> {
    minor_pets.iter().find(|p| p.species.eq_ignore_ascii_case(key))
}

/// A cat with its catch: taken up, carried to them (after them again if they've moved on, twice),
/// put down at their feet, and their reaction; or dropped where it was to be.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn present(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib, ui, catalog): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>, Option<Res<crate::icons::GameUi>>, Res<crate::loading::Catalog>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut pets: Query<(Entity, &Pet, &mut Presenting, &mut Transform, &mut PetAnim, Has<PetTask>, Option<&crate::pets::PetSkeleton>, Option<&mut crate::pets::PetBonds>, Has<HomePet>, Option<&mut Inventory>), Without<Sim>>,
    mut sims: Query<(&Sim, &Transform, &mut ActionQueue), Without<Pet>>,
    objects: Query<(&GameObject, &Transform), (Without<Pet>, Without<Sim>)>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let (Some(grid), Some(ui)) = (grid, ui) else { return };
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let now = clock.minutes;
    let mut rng = rand::rng();
    let len = |lib: &mut crate::anim::ClipLibrary, n: &str| lib.get(&data, n).map_or(1.0, |c| c.duration);
    for (e, pet, mut p, mut tf, mut anim, tasked, skel, bonds, home, inv) in &mut pets {
        let here = tf.translation.xz();
        let (world_model, carried) = looks(kind_of(&p.item.key, &ui.data.minor_pets).map_or("", |k| k.kind.as_str()));
        match p.stage {
            // Taken up in its mouth.
            Stage::PickUp => {
                if p.t == 0.0 {
                    anim.replay("ac2o_prey_pickUp_x".into());
                    commands.entity(e).remove::<PetTask>().insert((PetBusy, crate::petlitters::Held));
                    info!("pet present: {} takes up the {}", pet.name, p.item.name);
                }
                p.t += dt;
                if p.prop.is_none()
                    && p.t >= PICKED_UP_AT
                    && let Some(skel) = skel
                {
                    let held = carry_hold(&mut lib, &data);
                    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                    p.prop = in_mouth(&mut commands, &mut assets, &mut ctx, held, skel, carried);
                }
                if p.t < len(&mut lib, "ac2o_prey_pickUp_x") {
                    continue;
                }
                let to = match p.to {
                    Some(s) => sims.get(s).ok().map(|(_, stf, _)| before(stf.translation.xz(), here)),
                    None => {
                        let beds: Vec<Vec2> = objects.iter().filter(|(o, _)| matches!(o.kind, ObjectKind::PetBed | ObjectKind::PetHouse)).map(|(_, t)| t.translation.xz()).filter(|b| b.distance(here) < 30.0).collect();
                        let plants: Vec<Vec2> = objects.iter().filter(|(o, _)| matches!(o.kind, ObjectKind::Plant | ObjectKind::GardenPlant)).map(|(_, t)| t.translation.xz()).filter(|b| b.distance(here) < 20.0).collect();
                        Some(somewhere(here, &beds, &plants, &mut rng))
                    }
                };
                match to.and_then(|to| crate::petneeds::plan(pet, here, PetDo::Go, Some(to), None, &grid, building.as_deref(), &mut rng)) {
                    Some(task) => {
                        commands.entity(e).remove::<PetBusy>().insert(task);
                        (p.stage, p.t) = (Stage::Going, 0.0);
                    }
                    // (Nowhere to go: put down where it is.)
                    None => {
                        anim.replay("ac2o_prey_putDown_x".into());
                        (p.stage, p.t) = (Stage::PutDown, 0.0);
                    }
                }
            }
            // On its way.
            Stage::Going => {
                if tasked {
                    continue;
                }
                if let Some(s) = p.to
                    && let Ok((_, stf, _)) = sims.get(s)
                {
                    let them = stf.translation.xz();
                    // (Moved on: after them, twice at most.)
                    if them.distance(here) > PRESENT_FROM + 1.0
                        && p.tries < 2
                        && let Some(task) = crate::petneeds::plan(pet, here, PetDo::Go, Some(before(them, here)), None, &grid, building.as_deref(), &mut rng)
                    {
                        p.tries += 1;
                        commands.entity(e).insert(task);
                        continue;
                    }
                    let d = them - here;
                    tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
                }
                anim.replay("ac2o_prey_putDown_x".into());
                commands.entity(e).insert(PetBusy);
                (p.stage, p.t) = (Stage::PutDown, 0.0);
            }
            // Put down, out cold on the floor; and their reaction.
            Stage::PutDown => {
                p.t += dt;
                if p.t >= PUT_DOWN_AT && !p.laid {
                    p.laid = true;
                    if let Some(prop) = p.prop.take() {
                        commands.entity(prop).try_despawn();
                    }
                    {
                        let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 0.35);
                        let objd = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(world_model)).map(|c| c.objd);
                        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                        match objd.and_then(|o| crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, o, at, rng.random_range(0.0..std::f32::consts::TAU))) {
                            Some(o) => {
                                commands.entity(o.entity).insert(crate::save::Bought);
                                lay(&mut commands, o.entity, p.item.clone());
                            }
                            // (No model for it: back in its inventory.)
                            None => match inv {
                                Some(mut i) => i.put(p.item.clone()),
                                None => crate::inventory::give(&mut commands, e, p.item.kind, p.item.key.clone(), p.item.name.clone(), 0, p.item.worth, 1),
                            },
                        }
                    }
                }
                if p.t < len(&mut lib, "ac2o_prey_putDown_x") {
                    continue;
                }
                commands.entity(e).remove::<(Presenting, PetBusy, crate::petlitters::Held)>().insert(crate::petneeds::PetThink::until(now + 30.0));
                anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
                let Some(s) = p.to else {
                    info!("pet present: {} dropped the {} somewhere", pet.name, p.item.name);
                    continue;
                };
                let Ok((sim, stf, mut q)) = sims.get_mut(s) else { continue };
                if stf.translation.distance(tf.translation) > REACT_WITHIN {
                    continue;
                }
                let liking = bonds.as_ref().map_or(if home { crate::pets::FAMILY_LIKING } else { 0.0 }, |b| b.liking(sim.id, home));
                let good = rng.random_bool(pet_chance(&sim.traits, liking));
                if good && !sim.age.is_little() {
                    // (Petted for it: the pack's "Pet", as its `Pet On Floor Auto Accept`.)
                    if let Some(social) = crate::pets::PET_SOCIALS.iter().position(|x| x.name == "Pet") {
                        let a = Action::new("Pet", ActionKind::PetSocial { target: e, social, at: here, kind: "ac" }, p.own_accord);
                        let next = usize::from(q.0.front().is_some_and(|a| matches!(a.phase, Phase::Running(_)))).min(q.0.len());
                        q.0.insert(next, a);
                    }
                } else {
                    let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 0.35);
                    commands.entity(s).insert(crate::reactions::Pending::new(&["Retch"], Some(at), now + 10.0));
                }
                if let Some(mut b) = bonds {
                    b.add(sim.id, home, if good { GAIN } else { -LOSS });
                }
                info!("pet present: {} presented the {} to {}, who {}", pet.name, p.item.name, sim.first, if good { "petted them for it" } else { "retched" });
                if !p.own_accord && !good {
                    notes.push(format!("{} wasn't impressed by {}'s gift.", sim.first, pet.name));
                }
            }
        }
    }
}

/// How the carry clip holds a catch at the mouth's slot (its `transformBone` at its start).
fn carry_hold(lib: &mut crate::anim::ClipLibrary, data: &crate::baked::Baked) -> (Vec3, Quat) {
    lib.get(data, "ac2o_prey_carry_prey")
        .and_then(|c| c.tracks.get(&s3pkg::fnv32("transformBone")).map(|t| (crate::anim::sample_track_vec(&t.translation, 0.0).unwrap_or_default(), crate::anim::sample_track_quat(&t.rotation, 0.0).unwrap_or_default())))
        .unwrap_or((Vec3::ZERO, Quat::IDENTITY))
}

/// What shows of a catch in the cat's mouth, at its mouth's slot as the carry clip holds it.
fn in_mouth(commands: &mut Commands, assets: &mut crate::objects::ObjectAssets, ctx: &mut crate::objects::AssetCtx, (at, turn): (Vec3, Quat), skel: &crate::pets::PetSkeleton, model: &str) -> Option<Entity> {
    let joint = skel.slot_joint(MOUTH_SLOT)?;
    let objd = ctx.baked.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(model))?.objd;
    let parts = assets.object(ctx, objd);
    if parts.is_empty() {
        return None;
    }
    let m = crate::objects::spawn_parts(commands, &parts, Transform::IDENTITY);
    let e = commands.spawn((Transform::from_translation(at).with_rotation(turn), Visibility::default())).add_child(m).id();
    commands.entity(joint).add_child(e);
    Some(e)
}

/// A pet off to a catch on the floor, to eat it or put it away: how far along, and till when it
/// eats.
#[derive(Component, Debug)]
pub struct AtPrey {
    prey: Entity,
    eat: bool,
    stage: u8,
    t: f32,
    until: f64,
}

/// A pet sent to a catch on the floor (its way there): false where it can't get there.
#[allow(clippy::too_many_arguments)]
pub fn send(commands: &mut Commands, e: Entity, pet: &Pet, here: Vec2, prey: Entity, at: Vec2, eat: bool, grid: &crate::nav::NavGrid, building: Option<&crate::building::ActiveBuilding>, rng: &mut impl Rng) -> bool {
    let little = pet.kind.ends_with('l');
    let off = if little { 0.2 } else { 0.55 };
    let Some(task) = crate::petneeds::plan(pet, here, PetDo::Go, Some(at + (here - at).normalize_or(Vec2::X) * off), None, grid, building, rng) else { return false };
    commands.entity(e).insert((task, crate::petlitters::Held, AtPrey { prey, eat, stage: 0, t: 0.0, until: 0.0 }));
    true
}

/// At a catch on the floor: eaten (the pack's eating clips, ten minutes a cat, five a dog), or
/// taken up and put away.
#[allow(clippy::type_complexity)]
fn at_prey(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib, ui): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>, Option<Res<crate::icons::GameUi>>),
    mut pets: Query<(Entity, &Pet, &mut AtPrey, &mut Transform, &mut PetAnim, Has<PetTask>, Option<&mut PetMotives>, Option<&mut Inventory>), Without<CaughtPrey>>,
    prey_q: Query<(&Transform, &CaughtPrey), Without<Pet>>,
) {
    let Some(ui) = ui else { return };
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, pet, mut a, mut tf, mut anim, tasked, motives, inv) in &mut pets {
        if tasked {
            continue;
        }
        let done = |commands: &mut Commands, anim: &mut PetAnim, rng: &mut rand::rngs::ThreadRng| {
            commands.entity(e).remove::<(AtPrey, PetBusy, crate::petlitters::Held)>().insert(crate::petneeds::PetThink::until(now + 30.0));
            anim.play(crate::pets::idle_clip(&pet.kind, false, rng));
        };
        let Ok((ptf, prey)) = prey_q.get(a.prey) else {
            if a.stage < 3 {
                done(&mut commands, &mut anim, &mut rng);
            }
            continue;
        };
        let k = &pet.kind[..2.min(pet.kind.len())];
        let k = if k.starts_with('a') { k.to_string() } else { format!("a{}", &k[1..]) };
        if a.stage == 0 {
            let d = ptf.translation.xz() - tf.translation.xz();
            if d.length() > 1.5 {
                done(&mut commands, &mut anim, &mut rng);
                continue;
            }
            tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
            commands.entity(e).insert(PetBusy);
            a.stage = 1;
            a.t = 0.0;
            if a.eat {
                anim.replay(format!("{k}2o_eatfish_start_x"));
                a.until = now + if pet.kind.ends_with('c') { CAT_EAT_MINUTES } else { DOG_EAT_MINUTES };
                info!("pet present: {} eats the {}", pet.name, prey.0.name);
            } else {
                anim.replay("ac2o_prey_pickUp_x".into());
            }
            continue;
        }
        a.t += dt;
        if !a.eat {
            // (Taken up, and into its inventory.)
            if a.t >= PICKED_UP_AT && a.stage == 1 {
                a.stage = 2;
                let item = prey.0.clone();
                commands.entity(a.prey).try_despawn();
                info!("pet present: {} puts the {} away", pet.name, item.name);
                match inv {
                    Some(mut i) => i.put(item),
                    None => crate::inventory::give(&mut commands, e, item.kind, item.key.clone(), item.name.clone(), 0, item.worth, 1),
                }
                done(&mut commands, &mut anim, &mut rng);
            }
            continue;
        }
        match a.stage {
            1 if a.t >= lib.get(&data, &format!("{k}2o_eatfish_start_x")).map_or(1.0, |c| c.duration) => {
                a.stage = 2;
                anim.play(format!("{k}2o_eatfish_loop{}_x", rng.random_range(1..=2)));
            }
            2 if now >= a.until => {
                a.stage = 3;
                a.t = 0.0;
                anim.replay(format!("{k}2o_eatfish_stop_x"));
                if let Some(mut m) = motives {
                    m.add(HUNGER, EAT_GAIN);
                }
                // (A bird: Tastes Like Chicken, to a cat.)
                if pet.kind.ends_with('c') && kind_of(&prey.0.key, &ui.data.minor_pets).is_some_and(|p| p.kind.contains("Bird")) {
                    crate::petmoods::give(&mut commands, e, crate::life::MoodletKind::TastesLikeChicken, now);
                }
            }
            3 if a.t >= lib.get(&data, &format!("{k}2o_eatfish_stop_x")).map_or(1.0, |c| c.duration) => {
                commands.entity(a.prey).try_despawn();
                done(&mut commands, &mut anim, &mut rng);
            }
            _ => {}
        }
    }
}

/// Cats with a catch present it now and then of their own accord (to the one of the family they
/// like best, seven times in ten) or drop it somewhere; hungry cats and dogs eat one lying about.
#[allow(clippy::type_complexity)]
fn autonomy(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut pets: Query<
        (Entity, &Pet, &Transform, Option<&mut Inventory>, Option<&PetMotives>, Option<&crate::pets::PetBonds>),
        (With<HomePet>, Without<PetTask>, Without<PetBusy>, Without<crate::petlitters::Held>, Without<Presenting>, Without<AtPrey>, Without<crate::sim::Selected>),
    >,
    family: Query<(Entity, &Sim, &Transform, &InheritedVisibility), (With<crate::sim::HouseholdMember>, Without<Pet>)>,
    prey_q: Query<(Entity, &Transform), (With<CaughtPrey>, Without<Pet>, Without<Sim>)>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut next: Local<std::collections::HashMap<Entity, f64>>,
) {
    let (Some(ui), Some(grid)) = (ui, grid) else { return };
    if std::env::var_os("PRESENT_TEST").is_some() {
        return;
    }
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, pet, tf, inv, motives, bonds) in &mut pets {
        let at = next.entry(e).or_insert(now + THINK);
        if now < *at {
            continue;
        }
        *at = now + THINK;
        let here = tf.translation.xz();
        // (Hungry: a catch lying about eaten.)
        if motives.is_some_and(|m| m.0[HUNGER] < HUNGRY)
            && let Some((p, ptf)) = prey_q.iter().filter(|(_, t)| t.translation.xz().distance(here) < 15.0).min_by(|a, b| a.1.translation.xz().distance(here).total_cmp(&b.1.translation.xz().distance(here)))
            && send(&mut commands, e, pet, here, p, ptf.translation.xz(), true, &grid, building.as_deref(), &mut rng)
        {
            info!("pet present: {} goes to eat a catch lying about", pet.name);
            continue;
        }
        if !presents(&pet.kind) || !rng.random_bool(THINK_CHANCE) {
            continue;
        }
        let Some(mut inv) = inv else { continue };
        let Some(k) = inv.0.iter().position(|s| is_prey(s, &ui.data.minor_pets)) else { continue };
        let stack = inv.0[k].clone();
        let about: Vec<(Entity, f32)> = family
            .iter()
            .filter(|(_, s, t, shown)| shown.get() && can_present_to(s.age, true, false, t.translation.xz().distance(here)))
            .map(|(o, s, ..)| (o, bonds.map_or(crate::pets::FAMILY_LIKING, |b| b.liking(s.id, true))))
            .collect();
        let to = if about.is_empty() || rng.random_bool(1.0 / 3.0) {
            None
        } else {
            let best = about.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|b| b.0);
            let others: Vec<Entity> = about.iter().map(|a| a.0).filter(|o| Some(*o) != best).collect();
            if others.is_empty() || rng.random_bool(BEST_LIKED) { best } else { others.choose(&mut rng).copied() }
        };
        let Some(worth) = inv.take_one(k) else { continue };
        info!("pet present: {} means to {} of its own accord", pet.name, if to.is_some() { "present its catch" } else { "drop its catch somewhere" });
        commands.entity(e).insert(Presenting::new(to, Stack { count: 1, worth, ..stack }, true));
    }
}

/// `PRESENT_TEST=<pet>` (tests): ten seconds in, the household's cat of that name given a catch
/// (`PRESENT_PREY=<species>`, a brown rat by default) and, three seconds on, told to present it to
/// `PRESENT_TO=<first name>` (`drop`: drop it somewhere; `pie`: its inventory's menu opened, the
/// panel shown); `PRESENT_THEN=eat|away|pocket:<first name>`: once it's down, the cat eats it, puts
/// it away, or that Sim puts it in their inventory.
#[allow(clippy::type_complexity)]
fn present_test(
    mut commands: Commands,
    time: Res<Time>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut pets: Query<(Entity, &Pet, &GlobalTransform, Option<&mut Inventory>, Has<Presenting>, Has<AtPrey>), With<HomePet>>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue)>,
    prey_q: Query<(Entity, &Transform), With<CaughtPrey>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("PRESENT_TEST") else { return };
    let Some(ui) = ui else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let Some((e, _, ptf, inv, presenting, at_prey)) = pets.iter_mut().find(|p| p.1.name.eq_ignore_ascii_case(&name)) else { return };
    if let Ok(mut c) = cams.single_mut() {
        let f = |k: &str, d: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(ptf.translation());
        c.distance = f("PRESENT_DIST", 6.0);
        c.pitch = f("PRESENT_PITCH", 0.5);
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let species = env("PRESENT_PREY").unwrap_or_else(|| "RodentBrownRat".into());
            let p = ui.data.minor_pets.iter().find(|p| p.species.eq_ignore_ascii_case(&species)).or_else(|| ui.data.minor_pets.iter().find(|p| p.kind.contains("Rodent")));
            let Some(p) = p else { return };
            let item = (ItemKind::Object, p.species.clone(), p.name.clone(), p.min_value as i64);
            match inv {
                Some(mut i) => i.add(item.0, &item.1, &item.2, 0, item.3),
                None => crate::inventory::give(&mut commands, e, item.0, item.1, item.2, 0, item.3, 1),
            }
            info!("present test: {name} has a {}", p.name);
        }
        1 if now - stage.1 > 3.0 => {
            *stage = (2, now);
            let to = env("PRESENT_TO").unwrap_or_default();
            // (Its inventory's menu for the catch: the cat played, the panel open.)
            if to == "pie" {
                for (s, ..) in sims.iter() {
                    commands.entity(s).remove::<crate::sim::Selected>();
                }
                commands.entity(e).insert(crate::sim::Selected);
                commands.insert_resource(crate::livehud::InfoPanel::Inventory);
                let k = inv.as_ref().and_then(|i| i.0.iter().position(|s| is_prey(s, &ui.data.minor_pets))).unwrap_or(0);
                commands.insert_resource(crate::hudpanels::StackPie(k));
                info!("present test: {name}'s catch's menu");
                return;
            }
            let Some(mut inv) = inv else { return };
            let Some(k) = inv.0.iter().position(|s| is_prey(s, &ui.data.minor_pets)) else { return };
            let stack = inv.0[k].clone();
            let Some(worth) = inv.take_one(k) else { return };
            let who = sims.iter().find(|s| s.1.first.eq_ignore_ascii_case(&to)).map(|s| s.0);
            // (PRESENT_NEAR=1: they're brought out a few metres from the cat first.)
            if let (Some(w), true) = (who, env("PRESENT_NEAR").is_some()) {
                commands.queue(move |world: &mut World| {
                    let Some(at) = world.get::<Transform>(e).map(|t| t.translation) else { return };
                    if let Some(mut q) = world.get_mut::<ActionQueue>(w) {
                        q.0.clear();
                    }
                    // (And kept there a while, their free will held back.)
                    if let Some(mut t) = world.get_mut::<crate::interact::AutonomyTimer>(w) {
                        t.0 = 600.0;
                    }
                    world.entity_mut(w).remove::<crate::nav::PathFollow>();
                    if let Some(mut t) = world.get_mut::<Transform>(w) {
                        t.translation = at + Vec3::new(3.5, 0.0, 0.0);
                    }
                    if let Some(mut f) = world.get_mut::<crate::nav::Floor>(w) {
                        f.0 = 1;
                    }
                });
            }
            info!("present test: {name} to present it to {to}");
            commands.entity(e).insert(Presenting::new(who, Stack { count: 1, worth, ..stack }, false));
        }
        2 if !presenting && now - stage.1 > 2.0 => {
            let Some((p, pt)) = prey_q.iter().next() else { return };
            *stage = (3, now);
            match env("PRESENT_THEN").as_deref() {
                Some("eat") | Some("away") => {
                    let eat = env("PRESENT_THEN").as_deref() == Some("eat");
                    commands.entity(e).insert(crate::petcontrol::PetOrder(crate::petcontrol::PetCommand::Prey(p, eat)));
                    info!("present test: {name} to {}", if eat { "eat it" } else { "put it away" });
                }
                Some(s) if s.starts_with("pocket:") => {
                    let who = &s[7..];
                    if let Some((_, sim, mut q)) = sims.iter_mut().find(|x| x.1.first.eq_ignore_ascii_case(who)) {
                        q.push_player(Action::new(PUT_IN_INVENTORY, ActionKind::Object { target: p, def: 0 }, false));
                        info!("present test: {} picks it up ({:.1})", sim.first, pt.translation);
                    }
                }
                _ => {}
            }
            let _ = at_prey;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::Trait;

    #[test]
    fn a_sim_pets_the_cat_for_it_as_the_game_has_it() {
        assert_eq!(pet_chance(&[], 0.0), 0.5);
        assert!((pet_chance(&[Trait::Slob], 0.0) - 0.7).abs() < 1e-9);
        assert!((pet_chance(&[Trait::Neat, Trait::Snob], 0.0) - 0.0).abs() < 1e-9);
        assert!((pet_chance(&[Trait::Neat], 30.0) - 0.5).abs() < 1e-9, "liked enough makes up for being neat");
        assert_eq!(pet_chance(&[Trait::Slob], 50.0), 1.0);
    }

    #[test]
    fn who_and_what_a_cat_presents() {
        assert!(presents("ac") && !presents("cc") && !presents("ad"));
        assert!(can_present_to(Age::Toddler, true, false, 30.0));
        assert!(!can_present_to(Age::Baby, true, true, 1.0));
        assert!(!can_present_to(Age::Adult, false, true, 12.0), "a stranger too far off");
        assert!(can_present_to(Age::Adult, false, true, 8.0));
        assert!(!can_present_to(Age::Adult, false, false, 2.0), "nobody it knows");
        let rat = s3bake::gamedata::MinorPetInfo { kind: "Rodents".into(), species: "RodentBrownRat".into(), ..Default::default() };
        let st = |k: ItemKind, key: &str| Stack { kind: k, key: key.into(), name: String::new(), quality: 0, count: 1, worth: 0 };
        assert!(is_prey(&st(ItemKind::Object, "rodentbrownrat"), std::slice::from_ref(&rat)));
        assert!(!is_prey(&st(ItemKind::Object, "catHuntMapleLeaf"), std::slice::from_ref(&rat)), "a failure object's no catch");
        assert_eq!(looks("SmallBirds"), ("catHuntBird", "accessoryCatHuntFeathers"));
        assert_eq!(looks("Snakes").1, "accessoryCatHuntReptileTail");
        assert_eq!(looks("Rodents").0, "catHuntRodent");
        assert!(is_prey_model("catHuntTurtle") && !is_prey_model("catHuntWorthlessChipBag"));
    }

    #[test]
    fn put_down_a_metre_off_on_its_side() {
        let at = before(Vec2::new(10.0, 0.0), Vec2::new(14.0, 0.0));
        assert!((at - Vec2::new(11.0, 0.0)).length() < 1e-5);
        assert_eq!(s3pkg::fnv32("b__Mouth_slot"), MOUTH_SLOT);
    }
}
