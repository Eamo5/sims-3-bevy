//! Cats and little dogs sitting in a Sim's lap, as the Pets pack has them (`SitOnSim`,
//! `BeSatOnByPet`): a grown cat or little dog goes up to one of the family sitting on a sofa or
//! loveseat, jumps up into their lap and is petted there (`SitOnLap`: `ac2a_sofa_jumpUpLap`,
//! rounds of `LapIdle`, `jumpDownLap`; a child's `ac2c_`) for half an hour to an hour
//! (`kTimeRangeToSit`), liking them the better every quarter hour it stays (`kLTRIncrease`),
//! both having company and fun (Social 75 and Fun 25 an hour for the pet, Fun 25 and Social 50
//! for the Sim). A Sim who dislikes it (`LLTR` -10; 0, Mean-Spirited or Grumpy) won't have it
//! and it likes them the less (`kLTRDecrease`). Pets short of company do it of their own accord,
//! a lazy pet less often; a played pet's "Sit on <Sim>".

use bevy::prelude::*;
use rand::Rng;

use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::pets::{HomePet, Pet, PetAnim, PetBonds, PetBusy};
use crate::sim::{Age, Sim};

pub struct PetLapPlugin;

impl Plugin for PetLapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (want_laps, laps, lap_test).chain().after(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's tuning (`SitOnSim`).
const MINUTES: [f64; 2] = [30.0, 60.0];
const LIKING_EVERY: f64 = 15.0;
const LIKING: f32 = 2.0;
const REJECTED: f32 = -2.0;
/// The pet's and the Sim's gains an hour (`SitOnSim`'s and `BeSatOnByPet`'s tuning).
const PET_SOCIAL: f32 = 75.0;
const PET_FUN: f32 = 25.0;
const SIM_FUN: f32 = 25.0;
const SIM_SOCIAL: f32 = 50.0;
/// How far before the seated Sim the pet stands to jump up (the clips land it in their lap).
pub const BEFORE: f32 = 0.85;
/// How short of company a pet goes looking for a lap, and how often it thinks of it.
const LONELY: f32 = 0.0;
const THINK: f64 = 30.0;

/// The menu's words for a played pet (`Gameplay/Actors/Sim/SitOnSim:InteractionName`).
pub fn label(first: &str) -> String {
    format!("Sit on {first}")
}

/// Pets that sit in laps: grown cats and little dogs (the social's ages).
pub fn sits(kind: &str) -> bool {
    matches!(kind, "ac" | "al")
}

/// The sofa a Sim's sitting on, if they're sitting on one (a sofa or loveseat: the game's
/// `MultiSeatObject`), not napping.
pub fn seated_on_sofa(q: &ActionQueue, kind_of: impl Fn(Entity) -> Option<ObjectKind>) -> Option<Entity> {
    let a = q.0.front()?;
    if !matches!(a.phase, Phase::Running(_)) || a.cancel {
        return None;
    }
    let ActionKind::Object { target, def } = a.kind else { return None };
    let kind = kind_of(target)?;
    let d = crate::interact::interactions_for(kind).get(def)?;
    (kind == ObjectKind::Sofa && d.pose == crate::sim::Pose::Sit).then_some(target)
}

/// Whether a Sim turns a pet away (the social's rules: liking -10 or less; 0 or less and
/// Mean-Spirited or Grumpy).
pub fn turned_away(liking: f32, traits: &[crate::life::Trait]) -> bool {
    use crate::life::Trait;
    liking <= -10.0 || (liking <= 0.0 && traits.iter().any(|t| matches!(t, Trait::MeanSpirited | Trait::Grumpy)))
}

/// Where a pet stands to jump up into a seated Sim's lap, facing them.
pub fn spot(sim: &Transform) -> (Vec2, Quat) {
    let f = (sim.rotation * Vec3::Z).xz().normalize_or(Vec2::Y);
    (sim.translation.xz() + f * BEFORE, sim.rotation * Quat::from_rotation_y(std::f32::consts::PI))
}

/// The clips (`ac2a_sofa_jumpUpLap_x` the pet, `_y` the Sim; a child's `ac2c_`).
pub fn clip(kind: &str, age: Age, what: &str, side: char) -> String {
    let who = if age == Age::Child { 'c' } else { 'a' };
    format!("{kind}2{who}_sofa_{what}_{side}")
}

/// A Sim a pet's coming to the lap of, or is in it: they stay sitting till it's down.
#[derive(Component)]
pub struct LapTaken;

/// A pet on its way to a lap, or in it.
#[derive(Component)]
pub struct OnLap {
    pub sim: Entity,
    part: Part,
    time: f32,
    length: f32,
    since: f64,
    stay: f64,
    liked: f64,
    /// The Sim's own seated clips, for after.
    before: Option<ActionClip>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Going,
    Up,
    Idle,
    Down,
}

impl OnLap {
    pub fn new(sim: Entity) -> Self {
        Self { sim, part: Part::Going, time: 0.0, length: 0.0, since: 0.0, stay: 0.0, liked: 0.0, before: None }
    }
}

/// A pet sent off to a seated Sim's lap.
pub fn send(commands: &mut Commands, e: Entity, pet: &Pet, here: Vec2, sim: Entity, stf: &Transform, grid: &crate::nav::NavGrid, rng: &mut impl Rng) -> bool {
    let (to, _) = spot(stf);
    let Some(task) = crate::petneeds::plan(pet, here, crate::petneeds::PetDo::Go, Some(to), None, grid, None, rng) else { return false };
    commands.entity(e).insert((task, OnLap::new(sim))).remove::<(crate::pethunting::Hunt, crate::pethunting::GoHunt, crate::petcompany::Greeting)>();
    commands.entity(sim).insert(LapTaken);
    true
}

/// Household cats and little dogs short of company go to one of the family sitting on a sofa.
#[allow(clippy::type_complexity)]
fn want_laps(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    pets: Query<(Entity, &Pet, &crate::petneeds::PetMotives, &Transform, Option<&PetBonds>, Option<&crate::pets::PetTraits>), (With<HomePet>, Without<crate::petneeds::PetTask>, Without<PetBusy>, Without<OnLap>, Without<crate::sim::Selected>)>,
    sims: Query<(Entity, &Sim, &ActionQueue, &Transform), With<crate::sim::HouseholdMember>>,
    objects: Query<&GameObject>,
    taken: Query<&OnLap>,
    grid: Option<Res<crate::nav::NavGrid>>,
    mut next: Local<std::collections::HashMap<Entity, f64>>,
) {
    let Some(grid) = grid else { return };
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, pet, m, tf, bonds, traits) in &pets {
        if !sits(&pet.kind) || m.0[crate::petneeds::SOCIAL] >= LONELY {
            continue;
        }
        let at = next.entry(e).or_insert(now);
        if now < *at {
            continue;
        }
        *at = now + THINK;
        // (A lazy pet the less, an independent one too; a friendly one the more: the game's
        // scoring multipliers.)
        let keen: f64 = traits.map_or(1.0, |t| {
            use crate::pettraits::has;
            if has(&t.0, "LazyPet") {
                0.7
            } else if has(&t.0, "FriendlyPet") {
                1.3
            } else if has(&t.0, "IndependentPet") {
                0.75
            } else {
                1.0
            }
        });
        if !rng.random_bool((0.5 * keen).min(1.0)) {
            continue;
        }
        let here = tf.translation.xz();
        let lap = sims
            .iter()
            .filter(|(s, ..)| !taken.iter().any(|l| l.sim == *s))
            .filter(|(_, _, q, _)| seated_on_sofa(q, |o| objects.get(o).ok().map(|g| g.kind)).is_some())
            .filter(|(_, sim, ..)| bonds.map_or(crate::pets::FAMILY_LIKING, |b| b.liking(sim.id, true)) > -10.0)
            .min_by(|a, b| a.3.translation.xz().distance(here).total_cmp(&b.3.translation.xz().distance(here)));
        if let Some((s, sim, _, stf)) = lap
            && send(&mut commands, e, pet, here, s, stf, &grid, &mut rng)
        {
            info!("pet lap: {} goes to sit on {}", pet.name, sim.first);
        }
    }
}

/// Pets at a lap: up into it, petted there a while, and down again; the Sim's clips with theirs.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn laps(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut pets: Query<(Entity, &Pet, &mut OnLap, &mut Transform, &mut PetAnim, &mut crate::petneeds::PetMotives, Option<&mut PetBonds>, Has<crate::petneeds::PetTask>), Without<Sim>>,
    mut sims: Query<(&Sim, &ActionQueue, &Transform, &mut crate::sim::Motives, Option<&ActionClip>), Without<Pet>>,
    objects: Query<&GameObject>,
    mut notes: ResMut<crate::interact::Notifications>,
    taken: Query<Entity, With<LapTaken>>,
) {
    // (A Sim kept sitting for a pet that's gone: free to get up.)
    for s in &taken {
        if !pets.iter().any(|p| p.2.sim == s) {
            commands.entity(s).remove::<LapTaken>();
        }
    }
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    let has = |n: &str| data.0.clip_names.iter().any(|c| c.eq_ignore_ascii_case(n));
    for (e, pet, mut lap, mut tf, mut anim, mut pm, bonds, walking) in &mut pets {
        let Ok((sim, q, stf, mut sm, clip_now)) = sims.get_mut(lap.sim) else {
            commands.entity(e).remove::<(OnLap, PetBusy)>();
            continue;
        };
        let seated = seated_on_sofa(q, |o| objects.get(o).ok().map(|g| g.kind)).is_some();
        let free = |commands: &mut Commands, lap: &OnLap| {
            commands.entity(e).remove::<(OnLap, PetBusy)>();
            commands.entity(lap.sim).remove::<LapTaken>();
            if let Some(c) = lap.before.clone() {
                commands.entity(lap.sim).insert(c);
            }
        };
        match lap.part {
            Part::Going => {
                if walking {
                    continue;
                }
                let (at, face) = spot(stf);
                if !seated || tf.translation.xz().distance(at) > 0.6 {
                    debug!("pet lap: {} didn't get to the lap (seated {seated}, {:.1} m off)", pet.name, tf.translation.xz().distance(at));
                    commands.entity(e).remove::<OnLap>();
                    commands.entity(lap.sim).remove::<LapTaken>();
                    continue;
                }
                tf.translation = Vec3::new(at.x, tf.translation.y, at.y);
                tf.rotation = face;
                let liking = bonds.as_ref().map_or(crate::pets::FAMILY_LIKING, |b| b.liking(sim.id, true));
                if turned_away(liking, &sim.traits) {
                    if let Some(mut b) = bonds {
                        b.add(sim.id, true, REJECTED);
                    }
                    notes.push(format!("{} won't have {} in their lap.", sim.first, pet.name));
                    commands.entity(e).remove::<OnLap>();
                    commands.entity(lap.sim).remove::<LapTaken>();
                    continue;
                }
                let up = clip(&pet.kind, sim.age, "jumpUpLap", 'x');
                if !has(&up) {
                    commands.entity(e).remove::<OnLap>();
                    commands.entity(lap.sim).remove::<LapTaken>();
                    continue;
                }
                lap.before = clip_now.cloned();
                lap.since = now;
                lap.liked = now;
                lap.stay = rng.random_range(MINUTES[0]..MINUTES[1]);
                lap.part = Part::Up;
                // (Whatever else it had in mind, dropped.)
                commands.entity(e).insert(PetBusy).remove::<(crate::pethunting::Hunt, crate::pethunting::GoHunt, crate::petcompany::Greeting, crate::toycrate::FetchingToy)>();
                info!("pet lap: {} jumps up into {}'s lap", pet.name, sim.first);
                start(&mut commands, &mut lib, &data, &mut lap, &mut anim, &pet.kind, sim.age, "jumpUpLap");
            }
            Part::Up | Part::Idle | Part::Down => {
                // (Their lap gone, the Sim up: the pet back down at once.)
                if !seated && lap.part != Part::Down {
                    let (at, face) = spot(stf);
                    tf.translation = Vec3::new(at.x, tf.translation.y, at.y);
                    tf.rotation = face;
                    anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
                    lap.before = None;
                    free(&mut commands, &lap);
                    continue;
                }
                if lap.part == Part::Idle && hours > 0.0 {
                    pm.add(crate::petneeds::SOCIAL, PET_SOCIAL * hours);
                    pm.add(crate::petneeds::FUN, PET_FUN * hours);
                    sm.add(crate::sim::FUN, SIM_FUN * hours);
                    sm.add(crate::sim::SOCIAL, SIM_SOCIAL * hours);
                    if now - lap.liked >= LIKING_EVERY {
                        lap.liked = now;
                        if let Some(mut b) = bonds {
                            b.add(sim.id, true, LIKING);
                        }
                    }
                }
                lap.time += dt;
                if lap.time < lap.length {
                    continue;
                }
                match lap.part {
                    Part::Up | Part::Idle if now - lap.since < lap.stay => {
                        lap.part = Part::Idle;
                        let n = rng.random_range(1..=3);
                        start(&mut commands, &mut lib, &data, &mut lap, &mut anim, &pet.kind, sim.age, &format!("LapIdle{n}"));
                    }
                    Part::Up | Part::Idle => {
                        lap.part = Part::Down;
                        start(&mut commands, &mut lib, &data, &mut lap, &mut anim, &pet.kind, sim.age, "jumpDownLap");
                    }
                    _ => {
                        info!("pet lap: {} off {}'s lap ({:.0} minutes)", pet.name, sim.first, now - lap.since);
                        anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
                        free(&mut commands, &lap);
                    }
                }
            }
        }
    }
}

/// A part of the set begun: the pet's clip and the Sim's.
#[allow(clippy::too_many_arguments)]
fn start(commands: &mut Commands, lib: &mut ClipLibrary, data: &Baked, lap: &mut OnLap, anim: &mut PetAnim, kind: &str, age: Age, what: &str) {
    let (x, y) = (clip(kind, age, what, 'x'), clip(kind, age, what, 'y'));
    // (Done a moment before the clips are, the pet's not looping back to its start.)
    lap.length = lib.get(data, &x).map_or(2.0, |c| c.duration).min(lib.get(data, &y).map_or(f32::MAX, |c| c.duration)).max(0.3) - 0.05;
    lap.time = 0.0;
    anim.replay(x);
    let y: &'static str = crate::pets::intern(y);
    let clips: &'static [&'static str] = Box::leak(Box::new([y]));
    commands.entity(lap.sim).insert(ActionClip::new(None, clips));
}

/// `LAP_TEST=<pet>` (tests): eight seconds in, the selected Sim sits on the nearest sofa, and once
/// they're sitting, the pet's sent to their lap; the camera on them.
#[allow(clippy::type_complexity)]
fn lap_test(
    mut commands: Commands,
    time: Res<Time>,
    pets: Query<(Entity, &Pet, &Transform), (With<HomePet>, Without<Sim>)>,
    mut sel: Query<(Entity, &Transform, &mut ActionQueue), (With<crate::sim::Selected>, Without<Pet>)>,
    sofas: Query<(Entity, &GameObject, &Transform), (Without<Sim>, Without<Pet>, Without<crate::visit::LotObject>)>,
    objects: Query<&GameObject>,
    grid: Option<Res<crate::nav::NavGrid>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("LAP_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 8.0 {
        return;
    }
    let Some((pet_e, pet, ptf)) = pets.iter().find(|p| p.1.name.eq_ignore_ascii_case(&name)) else { return };
    let Ok((me, stf, mut q)) = sel.single_mut() else { return };
    if let Ok(mut c) = cams.single_mut() {
        let f = Quat::from_rotation_y(0.9) * (stf.rotation * Vec3::Z);
        c.look_at(stf.translation + Vec3::Y * 0.6);
        c.yaw = f.x.atan2(f.z);
        c.pitch = 0.4;
        c.distance = 3.5;
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let sofa = sofas.iter().filter(|s| s.1.kind == ObjectKind::Sofa).min_by(|a, b| a.2.translation.distance(stf.translation).total_cmp(&b.2.translation.distance(stf.translation)));
            if let Some((s, ..)) = sofa {
                q.push_player(crate::interact::Action::new("Sit", ActionKind::Object { target: s, def: 0 }, false));
                info!("pet lap test: sitting down");
            }
        }
        1 if seated_on_sofa(&q, |o| objects.get(o).ok().map(|g| g.kind)).is_some() && now - stage.1 > 2.0 => {
            *stage = (2, now);
            if let Some(grid) = grid.as_deref()
                && send(&mut commands, pet_e, pet, ptf.translation.xz(), me, stf, grid, &mut rand::rng())
            {
                info!("pet lap test: {} sent", pet.name);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lap_is_turned_down_as_the_game_has_it() {
        use crate::life::Trait;
        assert!(turned_away(-10.0, &[]));
        assert!(!turned_away(0.0, &[]));
        assert!(turned_away(0.0, &[Trait::Grumpy]));
        assert!(!turned_away(5.0, &[Trait::MeanSpirited]));
        assert!(sits("ac") && sits("al") && !sits("ad") && !sits("cc"));
        assert_eq!(clip("al", Age::Child, "LapIdle2", 'y'), "al2c_sofa_LapIdle2_y");
        assert_eq!(label("Hetty"), "Sit on Hetty");
        let s = Transform::from_xyz(1.0, 0.0, 1.0);
        assert!((spot(&s).0 - Vec2::new(1.0, 1.0 + BEFORE)).length() < 1e-5);
    }
}
