//! Cats and little dogs picked up, carried about and put down, as the Pets pack has them
//! (`PickUpPet`, `CarryingPetPosture`, `PutDownPet`, `PetHeldPet`): a Sim (a child too) lifts a
//! cat, a kitten, a little dog or a puppy into their arms (`<sim>2<pet>_pickUp_start`), walks
//! about with it held (`_pickUp_pose`, the pet idling and now and then meowing or barking in
//! their arms), pets it there ("Pet", `_petHeld`: Loved), and puts it down again (`_stop`).
//! Anything else that needs their hands has them put it down first (the posture's standing
//! transition), and a pet that badly needs something is put down for it.
//!
//! The pack's clips are made with the pet standing before the Sim, facing them (the social jig's
//! two places), lifting it from there: the pet keeps that place before them as they go.

use std::f32::consts::PI;

use bevy::prelude::*;
use rand::Rng;

use crate::anim::{ActionClip, Carrying, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, Phase};
use crate::pets::{Pet, PetAnim, PetBonds, PetBusy};
use crate::sim::{Age, Sim};

pub struct PetCarryPlugin;

impl Plugin for PetCarryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, put_down_first.before(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, (carry_steps, held_idles, petcarry_test).chain().after(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(PostUpdate, follow.before(bevy::transform::TransformSystems::Propagate));
    }
}

/// The menus' words (the game's interactions' names).
pub const PICK_UP: &str = "Pick Up";
pub const PUT_DOWN: &str = "Put Down";
pub const PET_HELD: &str = "Pet";
/// Where the pet stands for the clips, before the Sim and a little to their left (where they lift it
/// from; held, the pack's clips have it in their arms).
pub const APART: f32 = 1.15;
const SIDE: f32 = 0.2;

/// The pet's place for the clips, and its turn (facing the Sim).
pub fn spot(sim: &Transform) -> (Vec3, Quat) {
    let f = sim.rotation * Vec3::Z;
    let side = sim.rotation * Vec3::X;
    let p = sim.translation + Vec3::new(f.x, 0.0, f.z) * APART + Vec3::new(side.x, 0.0, side.z) * SIDE;
    (p, sim.rotation * Quat::from_rotation_y(PI))
}
/// What petting a held pet's worth an hour (as "Pet"'s), and its liking for it.
const PET_FUN: f32 = 40.0;
const PET_SOCIAL: f32 = 70.0;
const PET_LIKING: f32 = 2.5;
/// How badly a held pet must need something to be put down for it.
const NEEDY: f32 = -50.0;

/// Pets that can be picked up: cats, kittens, little dogs and puppies (the game's `CustomTest`).
pub fn liftable(kind: &str) -> bool {
    matches!(kind, "ac" | "cc" | "al" | "cd" | "cl")
}

/// The clips' names for a Sim of an age with a pet of a kind (`a2ac_pickUp_start_x`; a child's
/// `c2…`, a puppy of either size the `cd` ones).
pub fn clip(age: Age, kind: &str, what: &str, side: char) -> String {
    let who = if age == Age::Child { 'c' } else { 'a' };
    let kind = if kind == "cl" { "cd" } else { kind };
    format!("{who}2{kind}_{what}_{side}")
}

/// Whether a pet's a dog (barks in the arms) or a cat (meows).
fn barks(kind: &str) -> bool {
    matches!(kind, "al" | "cd" | "cl")
}

/// A pet in a Sim's arms (or being lifted into them, or put down).
#[derive(Component)]
pub struct CarriedPet {
    pub by: Entity,
}

/// A Sim holding a pet.
#[derive(Component)]
pub struct HoldingPet(pub Entity);

/// A step under way: lifting, putting down, petting in the arms; how far in, how long it takes.
#[derive(Component)]
struct Step {
    pet: Entity,
    what: What,
    time: f32,
    length: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum What {
    PickUp,
    PutDown,
    Pet,
}

/// A held pet's menu: pet it, put it down.
pub fn held_options(pet: Entity) -> Vec<(String, ActionKind)> {
    vec![(PET_HELD.to_string(), ActionKind::PetHeldPet { pet }), (PUT_DOWN.to_string(), ActionKind::PutDownPet { pet })]
}

/// Whether a Sim can go on with what's next with a pet in their arms: walking about, and what's
/// done with it held.
fn with_pet(kind: &ActionKind, pet: Entity) -> bool {
    match kind {
        ActionKind::GoHere(..) | ActionKind::Outro { .. } => true,
        ActionKind::PutDownPet { pet: p } | ActionKind::PetHeldPet { pet: p } => *p == pet,
        _ => false,
    }
}

/// Before anything that needs their hands, the pet's put down (the game's standing transition);
/// and a pet in need of something is put down for it.
#[allow(clippy::type_complexity)]
fn put_down_first(
    mut commands: Commands,
    mut carriers: Query<(Entity, &HoldingPet, &mut ActionQueue), (With<Sim>, Without<Step>)>,
    pets: Query<&crate::petneeds::PetMotives>,
) {
    for (me, holding, mut queue) in &mut carriers {
        let pet = holding.0;
        let needy = pets.get(pet).is_ok_and(|m| [crate::petneeds::HUNGER, crate::petneeds::BLADDER, crate::petneeds::ENERGY].iter().any(|i| m.0[*i] < NEEDY));
        let front = queue.0.front();
        let busy = front.is_some_and(|a| a.cancel || !matches!(a.phase, Phase::Start | Phase::Routing) || with_pet(&a.kind, pet));
        if front.is_some() && busy {
            continue;
        }
        if front.is_none() && !needy {
            continue;
        }
        if front.is_some_and(|a| matches!(a.phase, Phase::Routing)) {
            commands.entity(me).remove::<crate::nav::PathFollow>();
            if let Some(a) = queue.0.front_mut() {
                a.phase = Phase::Start;
            }
        }
        queue.0.push_front(Action::new(PUT_DOWN, ActionKind::PutDownPet { pet }, true));
    }
}

/// Lifting, putting down and petting in the arms: both sides' clips, and what each comes to.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn carry_steps(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &Transform, &mut crate::sim::Motives, Option<&mut Step>, Option<&HoldingPet>), Without<Pet>>,
    mut pets: Query<(&Pet, &mut Transform, &mut PetAnim, Option<&mut PetBonds>, Has<CarriedPet>), Without<Sim>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    let has = |n: &str| data.0.clip_names.iter().any(|c| c.eq_ignore_ascii_case(n));
    for (e, sim, mut queue, stf, mut motives, step, holding) in &mut sims {
        let running = queue.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).and_then(|a| match a.kind {
            ActionKind::PickUpPet { pet, .. } => Some((pet, What::PickUp)),
            ActionKind::PutDownPet { pet } => Some((pet, What::PutDown)),
            ActionKind::PetHeldPet { pet } => Some((pet, What::Pet)),
            _ => None,
        });
        let Some(mut s) = step else {
            let Some((pet, what)) = running else { continue };
            let Ok((p, mut ptf, mut anim, _, carried)) = pets.get_mut(pet) else {
                queue.0.pop_front();
                continue;
            };
            // (Picked up by someone else meanwhile, or not held by them to put down or pet.)
            let theirs = holding.is_some_and(|h| h.0 == pet);
            if (what == What::PickUp && (carried || holding.is_some())) || (what != What::PickUp && !theirs) {
                queue.0.pop_front();
                continue;
            }
            let (x, y) = match what {
                What::PickUp => (clip(sim.age, &p.kind, "pickUp_start", 'x'), clip(sim.age, &p.kind, "pickUp_start", 'y')),
                What::PutDown => (clip(sim.age, &p.kind, "pickUp_stop", 'x'), clip(sim.age, &p.kind, "pickUp_stop", 'y')),
                What::Pet => (clip(sim.age, &p.kind, "petHeld", 'x'), clip(sim.age, &p.kind, "petHeld", 'y')),
            };
            if !has(&x) {
                warn!("pet carry: no {x}");
                queue.0.pop_front();
                continue;
            }
            if what == What::PickUp {
                // (Before them, facing them, where the clips lift it from.)
                (ptf.translation, ptf.rotation) = spot(stf);
                commands.entity(pet).insert((CarriedPet { by: e }, PetBusy)).remove::<crate::petneeds::PetTask>();
                info!("pet carry: {} picks up {}", sim.first, p.name);
            }
            if what == What::PutDown {
                commands.entity(e).remove::<Carrying>();
            }
            // (Done a moment before the clips are, the pet's not looping back to its start.)
            let length = lib.get(&data, &x).map_or(2.0, |c| c.duration).min(lib.get(&data, &y).map_or(f32::MAX, |c| c.duration)).max(0.3) - 0.05;
            let x: &'static str = crate::pets::intern(x);
            let clips: &'static [&'static str] = Box::leak(Box::new([x]));
            commands.entity(e).insert((ActionClip::new(None, clips), Step { pet, what, time: 0.0, length }));
            anim.replay(y);
            continue;
        };
        let Ok((p, mut ptf, mut anim, bonds, _)) = pets.get_mut(s.pet) else {
            commands.entity(e).remove::<(Step, ActionClip, HoldingPet, Carrying)>();
            continue;
        };
        // (Off to something else midway: a lift undone, the pet set down where it was.)
        if running.is_none_or(|r| r.0 != s.pet || r.1 != s.what) {
            commands.entity(e).remove::<(Step, ActionClip)>();
            if s.what == What::PickUp || s.what == What::PutDown {
                release(&mut commands, e, s.pet, stf, &mut ptf);
                anim.play(crate::pets::idle_clip(&p.kind, false, &mut rng));
            }
            continue;
        }
        s.time += dt;
        if s.what == What::Pet && hours > 0.0 {
            motives.add(crate::sim::FUN, PET_FUN * hours);
            motives.add(crate::sim::SOCIAL, PET_SOCIAL * hours);
        }
        if s.time < s.length {
            continue;
        }
        queue.0.pop_front();
        commands.entity(e).remove::<(Step, ActionClip)>();
        match s.what {
            What::PickUp => {
                let pose: &'static str = crate::pets::intern(clip(sim.age, &p.kind, "pickUp_pose", 'x'));
                commands.entity(e).insert((HoldingPet(s.pet), Carrying(pose)));
                anim.replay(clip(sim.age, &p.kind, "pickedUp_idle", 'y'));
            }
            What::PutDown => {
                release(&mut commands, e, s.pet, stf, &mut ptf);
                anim.play(crate::pets::idle_clip(&p.kind, false, &mut rng));
                info!("pet carry: {} puts {} down", sim.first, p.name);
            }
            What::Pet => {
                crate::petmoods::give(&mut commands, s.pet, crate::life::MoodletKind::LovedPet, now);
                if let Some(mut b) = bonds {
                    b.add(sim.id, true, PET_LIKING);
                }
                anim.replay(clip(sim.age, &p.kind, "pickedUp_idle", 'y'));
            }
        }
    }
}

/// A pet let go of: on the ground before the Sim, facing them, free again.
fn release(commands: &mut Commands, sim: Entity, pet: Entity, stf: &Transform, ptf: &mut Transform) {
    (ptf.translation, ptf.rotation) = spot(stf);
    commands.entity(pet).remove::<(CarriedPet, PetBusy)>();
    commands.entity(sim).remove::<(HoldingPet, Carrying)>();
}

/// A held pet idles in the arms, now and then meowing or barking.
#[allow(clippy::type_complexity)]
fn held_idles(
    clock: Res<crate::clock::GameClock>,
    mut pets: Query<(&Pet, &CarriedPet, &mut PetAnim)>,
    sims: Query<(&Sim, Option<&HoldingPet>, Has<Step>)>,
    data: Res<Baked>,
    mut next: Local<f64>,
) {
    let mut rng = rand::rng();
    let call = clock.minutes >= *next;
    if call {
        *next = clock.minutes + rng.random_range(10.0..30.0);
    }
    for (p, c, mut anim) in &mut pets {
        let Ok((sim, holding, stepping)) = sims.get(c.by) else { continue };
        if stepping || holding.is_none() {
            continue;
        }
        let idle = clip(sim.age, &p.kind, "pickedUp_idle", 'y');
        let noise = clip(sim.age, &p.kind, if barks(&p.kind) { "pickedUp_bark" } else { "pickedUp_meow" }, 'y');
        if call && rng.random_bool(0.5) && data.0.clip_names.iter().any(|n| n.eq_ignore_ascii_case(&noise)) {
            anim.replay(noise);
        } else if anim.clip != noise || anim.time() > 2.5 {
            anim.play(idle);
        }
    }
}

/// Held pets go where the Sim does, before them, facing them (as the clips have it).
fn follow(mut pets: Query<(&CarriedPet, &mut Transform, Option<&mut crate::nav::Floor>), Without<Sim>>, sims: Query<(&Transform, Option<&crate::nav::Floor>), With<Sim>>) {
    for (c, mut tf, floor) in &mut pets {
        let Ok((stf, sfloor)) = sims.get(c.by) else { continue };
        (tf.translation, tf.rotation) = spot(stf);
        if let (Some(mut fl), Some(sf)) = (floor, sfloor) {
            fl.0 = sf.0;
        }
    }
}

/// `PETCARRY_TEST=<pet>` (tests): eight seconds in, the selected Sim picks that pet up, walks a few
/// metres with it, pets it and puts it down.
#[allow(clippy::type_complexity)]
fn petcarry_test(
    time: Res<Time>,
    pets: Query<(Entity, &Pet, &Transform), Without<Sim>>,
    mut sel: Query<(&Transform, &mut ActionQueue, Option<&HoldingPet>), (With<crate::sim::Selected>, Without<Pet>)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("PETCARRY_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 8.0 {
        return;
    }
    let Some((pet, p, ptf)) = pets.iter().find(|x| x.1.name.eq_ignore_ascii_case(&name)) else { return };
    let Ok((stf, mut q, holding)) = sel.single_mut() else { return };
    if let Ok(mut c) = cams.single_mut() {
        // (From before them, a little to the side.)
        let f = Quat::from_rotation_y(1.4) * (stf.rotation * Vec3::Z);
        c.look_at(stf.translation + Vec3::Y * 0.8);
        c.yaw = f.x.atan2(f.z);
        c.pitch = 0.4;
        c.distance = 3.5;
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            q.push_player(Action::new(PICK_UP, ActionKind::PickUpPet { pet, at: ptf.translation.xz() }, false));
            info!("pet carry test: picking up {}", p.name);
        }
        1 if holding.is_some() && q.0.is_empty() => {
            *stage = (2, now);
            let to = stf.translation.xz() + (stf.rotation * Vec3::X).xz() * 3.0;
            q.push_player(Action::new("Go Here", ActionKind::GoHere(to, 1), false));
        }
        2 if q.0.is_empty() && now - stage.1 > 1.0 => {
            *stage = (3, now);
            q.push_player(Action::new(PET_HELD, ActionKind::PetHeldPet { pet }, false));
            q.push_player(Action::new(PUT_DOWN, ActionKind::PutDownPet { pet }, false));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cats_and_little_dogs_are_lifted_with_the_packs_clips() {
        assert!(liftable("ac") && liftable("cd") && liftable("al"));
        assert!(!liftable("ad") && !liftable("ah"), "not a big dog nor a horse");
        assert_eq!(clip(Age::Adult, "ac", "pickUp_start", 'x'), "a2ac_pickUp_start_x");
        assert_eq!(clip(Age::Teen, "al", "petHeld", 'y'), "a2al_petHeld_y");
        assert_eq!(clip(Age::Child, "cl", "pickUp_pose", 'x'), "c2cd_pickUp_pose_x");
        let g = Entity::from_raw_u32(3).unwrap();
        assert!(with_pet(&ActionKind::PutDownPet { pet: g }, g));
        assert!(!with_pet(&ActionKind::Dismount, g), "anything else: put down first");
    }
}
