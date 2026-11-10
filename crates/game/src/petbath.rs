//! Bathing pets, as the Pets pack's bathtubs have it (`Bathtub.BathePet`): a grown cat, dog or
//! little dog of the household bathed in a bathtub (or a shower over a tub) by a teen or older.
//! The Sim kneels at the tub's side (its `BathePetHumanSlots` for a dog, its enter slots for a
//! smaller pet, the far side mirrored), a dog coming up beside them (`BathePetDogSlots`) and
//! hopping in, a cat or little dog lifted in from their arms; then the pack's `BathtubPets`
//! clips, the Sim's, the pet's and the tub's together: in, washed (`Bathe` loops) until the pet's
//! clean, out and dried off with the hair dryer. The pet's hygiene fills as the game's
//! `kHygieneIncreaseSpeed` (200 an hour), the Sim's falls (`kHumanHygieneDecreaseSpeed`), at least
//! `kBatheMinTime` minutes. A Hyper or Aggressive dog makes a bad bath of it half the time
//! (`kTraitsDogBadBath`: its own loop, slower clean); a cat scrambles out half the time
//! (`kCatFailChance`), liking the Sim the less for it (`kCatFailLTRDecrease`). The household's
//! grown-ups bathe a dog that's grown grungy of their own accord.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::pets::{Pet, PetAnim, PetBonds, PetBusy};
use crate::sim::Sim;

pub struct PetBathPlugin;

impl Plugin for PetBathPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (plan.before(crate::interact::run_actions), bathe.after(crate::interact::run_actions), walk_back, chores, bath_test).run_if(in_state(PlayMode::Live)));
    }
}

/// A tub's routing slots (model space: the slot's number, place and facing), and whether it's a
/// shower over a tub.
#[derive(Component, Clone, Debug, Default)]
pub struct TubSlots(pub Vec<(u8, Vec2, Vec2)>, pub bool);

impl TubSlots {
    /// The slots bathing a pet takes, a side at a time (the enter slot a smaller pet's lifted in
    /// from, the Sim's slot for a dog and the dog's beside it): a bathtub's either side (its
    /// `RouteEnterSlots`, `BathePetHumanSlots`, `BathePetDogSlots`), a shower over a tub's and a
    /// corner tub's only side (`ShowerTub`'s and `CornerBathtub`'s overrides).
    fn sides(&self) -> Vec<(u8, u8, u8)> {
        let has = |n: u8| self.0.iter().any(|s| s.0 == n);
        if self.1 {
            vec![(0, 2, 3)]
        } else if has(7) {
            vec![(0, 4, 5), (1, 6, 7)]
        } else {
            vec![(0, 1, 2)]
        }
    }
}

/// Whether a pet of a kind is bathed (the game's grown cats and dogs: `AdultOrAbove`).
pub fn bathable(kind: &str) -> bool {
    matches!(kind, "ac" | "ad" | "al")
}

/// Whether a kind's lifted into the tub (`BatheSmallPetInTub`) rather than hopping in.
fn carried(kind: &str) -> bool {
    kind != "ad"
}

/// How fast a pet comes clean (an hour), and the Sim grows dirtier: `kHygieneIncreaseSpeed`, a
/// bad bath's `kDogBadBatheHygieneIncreaseSpeed`; `kHumanHygieneDecreaseSpeed`.
pub fn rates(bad: bool) -> (f32, f32) {
    (if bad { 100.0 } else { 200.0 }, -200.0)
}

/// The least time in the bath (game minutes: `kBatheMinTime`, a bad bath's `kDogBadBatheMinTime`).
pub fn least_minutes(bad: bool) -> f64 {
    if bad { 10.0 } else { 5.0 }
}

/// The chance a cat scrambles out (`kCatFailChance`), and how much less it likes the Sim for it.
pub const CAT_FAIL_CHANCE: f64 = 0.5;
pub const CAT_FAIL_LIKING: f32 = -20.0;
/// The chance a Hyper or Aggressive dog makes a bad bath of it (`kDogBadBatheChance`).
pub const BAD_BATH_CHANCE: f64 = 0.5;
/// A dog this grungy gets bathed by the household of their own accord.
const GRUNGY: f32 = -40.0;

/// Whether a dog makes a bad bath of it (its traits, and a roll).
pub fn bad_bath(kind: &str, traits: &[String], roll: f64) -> bool {
    matches!(kind, "ad" | "al") && traits.iter().any(|t| t == "HyperPet" || t == "AggressivePet") && roll < BAD_BATH_CHANCE
}

/// Where a bath's Sim and pet go, in the world: the Sim's slot and the way they face; where the
/// pet's clips start from (its slot beside them, or their arms) and its frame there (the tub's
/// own: the clips take it in along the tub's axes); and whether it's the far side (mirrored).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Places {
    pub sim: Vec2,
    pub face: Vec2,
    pub pet: Vec2,
    pub pet_turn: Quat,
    pub mirror: bool,
}

/// The places for bathing a pet of a kind at a tub, the side nearer `from` (as `way` measures the
/// way there: none where the Sim can't stand, a tub against a wall having the one).
pub fn places(slots: &TubSlots, tub: &Transform, kind: &str, from: Vec2, way: impl Fn(Vec2, Vec2) -> Option<f32>) -> Option<Places> {
    let slot = |n: u8| slots.0.iter().find(|s| s.0 == n).map(|s| (s.1, s.2));
    // (A dog at the human slot and the dog's beside it; a smaller pet from the enter slot.)
    let sides: Vec<(u8, u8)> = slots.sides().into_iter().map(|(enter, human, dog)| if carried(kind) { (enter, enter) } else { (human, dog) }).collect();
    let world = |p: Vec2| (tub.translation + tub.rotation * Vec3::new(p.x, 0.0, p.y)).xz();
    let dir = |d: Vec2| (tub.rotation * Vec3::new(d.x, 0.0, d.y)).xz().normalize_or(Vec2::Y);
    let mut best: Option<(f32, Places)> = None;
    for (i, (s, p)) in sides.into_iter().enumerate() {
        let (Some((sp, sf)), Some((pp, _))) = (slot(s), slot(p)) else { continue };
        let at = world(sp);
        // (A side the Sim can't stand at last, if none can.)
        let d = way(at, dir(sf)).unwrap_or(1000.0 + at.distance(from));
        if best.is_none_or(|b| d < b.0) {
            best = Some((d, Places { sim: at, face: dir(sf), pet: world(pp), pet_turn: tub.rotation, mirror: i == 1 }));
        }
    }
    best.map(|b| b.1)
}

/// The parts of a bath.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    /// The pet coming up to the Sim (or the tub).
    Fetch,
    Start,
    Loop,
    Stop,
    /// A cat scrambling out.
    Fail,
}

/// A part's clip for a side (`x` the Sim, `y` the pet): the pack's `<who>2<pet>_tub_bathe_<part>`.
pub fn clip_name(kind: &str, part: Part, round: u32, bad: bool, mirror: bool, side: char) -> String {
    let p = match part {
        Part::Start | Part::Fetch => "start".to_string(),
        Part::Loop if bad => "loop1_bad".to_string(),
        Part::Loop => format!("loop{}", 1 + round % 2),
        Part::Stop => "stop".to_string(),
        Part::Fail => "fail".to_string(),
    };
    let m = if mirror { "_mirror" } else { "" };
    format!("a2{kind}_tub_bathe_{p}{m}_{side}")
}

/// A Sim bathing a pet: which, at which tub, the part and how far into it (seconds), when the
/// washing began (game minutes), and how it's going.
#[derive(Component)]
pub struct Bathing {
    pet: Entity,
    tub: Entity,
    places: Places,
    part: Part,
    time: f32,
    length: f32,
    round: u32,
    since: f64,
    bad: bool,
    /// The pet's way to the tub (up the stairs to one on an upper floor), the tub's floor, and
    /// where it came from.
    route: Vec<crate::nav::Waypoint>,
    level: u8,
    home: Vec2,
    base: (Vec3, Quat),
    pet_clip: String,
}

/// A pet being bathed (and seen back down after): the bath's to see to.
#[derive(Component)]
pub struct InBath;

/// A pet bathed upstairs, on its way back down to where it was (free again once it's there).
#[derive(Component)]
pub struct WalkBack(Vec<crate::nav::Waypoint>);

/// A step along a pet's route, over floors and up and down straight stairs (a spiral's steps
/// crossed at once); whether it's there.
pub fn walk_route(route: &mut Vec<crate::nav::Waypoint>, tf: &mut Transform, step: f32, height: impl Fn(u8, Vec3) -> f32) -> bool {
    let Some(&w) = route.first() else { return true };
    let here = tf.translation.xz();
    let d = w.p - here;
    let pace = if w.climb.is_some() { 0.6 } else { 1.0 };
    if d.length() <= (step * pace).max(0.05) || w.spiral.is_some() {
        tf.translation = Vec3::new(w.p.x, w.climb.map_or_else(|| height(w.level, Vec3::new(w.p.x, tf.translation.y, w.p.y)), |c| c.1), w.p.y);
        route.remove(0);
        return route.is_empty();
    }
    let p = here + d.normalize() * step * pace;
    // (On stairs, as high as it's come along them.)
    // (On stairs, the height left to go in step with the way left.)
    let y = match w.climb {
        Some((_, y1)) => {
            let moved = step * pace;
            let left = (w.p - p).length();
            tf.translation.y + (y1 - tf.translation.y) * (moved / (left + moved)).clamp(0.0, 1.0)
        }
        None => height(w.level, Vec3::new(p.x, tf.translation.y, p.y)),
    };
    tf.translation = Vec3::new(p.x, y, p.y);
    tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
    false
}

/// Bath actions just asked for get their places (the tub's slots, the side nearer the Sim; a tub
/// without them, beside it, turned to it).
fn plan(
    mut sims: Query<(&mut ActionQueue, &Transform), With<Sim>>,
    tubs: Query<(&Transform, Option<&TubSlots>, Option<&crate::nav::Floor>), Without<Sim>>,
    (grid, upper): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::nav::UpperFloors>>),
) {
    for (mut q, tf) in &mut sims {
        for a in q.0.iter_mut() {
            if let ActionKind::BathePet { tub, kind, at, face, level, planned, .. } = &mut a.kind
                && !*planned
            {
                *planned = true;
                let Ok((ttf, slots, floor)) = tubs.get(*tub) else { continue };
                let here = tf.translation.xz();
                // (On the tub's floor: upstairs, up there.)
                *level = floor.map_or(1, |f| f.0);
                let l = *level;
                let on = if l <= 1 { grid.as_deref() } else { upper.as_deref().and_then(|u| u.grids.get(&l)) };
                // (Room for them there: the spot and a Sim's breadth round it, as the slot's
                // footprint has it: no wall that near, nothing standing there).
                let open = |p: Vec2| on.is_none_or(|g| g.cell_of(p).is_some_and(|(x, z)| !g.is_blocked(x, z)));
                let free = |p: Vec2| [Vec2::ZERO, Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y].iter().all(|d| open(p + *d * 0.3) && on.is_none_or(|g| !g.crosses(p, p + *d * 0.4)));
                // (The way there on the ground floor's grid, as the game's routes to its slots go.)
                // (Not outside the room: the tub's to be reached from there, no wall between.)
                let way = |p: Vec2, face: Vec2| {
                    if !free(p) || on.is_some_and(|g| g.crosses(p, p + face * 0.8)) {
                        return None;
                    }
                    let walked = (l <= 1).then(|| grid.as_deref().and_then(|g| g.find_path(here, p))).flatten();
                    let d = walked.map_or(here.distance(p), |w| w.iter().fold((here, 0.0), |(a, n), b| (*b, n + a.distance(*b))).1);

                    Some(d)
                };
                match slots.and_then(|s| places(s, ttf, kind, here, way)) {
                    Some(p) => (*at, *face) = (p.sim, p.face),
                    None => {
                        let c = ttf.translation.xz();
                        *face = (c - here).normalize_or(Vec2::Y);
                        *at = c - *face;
                    }
                }
            }
        }
    }
}

/// The bath itself: the pet fetched, then each part played on all three, the pet moved by its
/// clips, needs changing as it's washed; over, the pet free again.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn bathe(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Transform, &mut crate::sim::Motives, Option<&mut Bathing>), Without<Pet>>,
    mut pets: Query<(&Pet, &mut Transform, &mut PetAnim, &mut crate::petneeds::PetMotives, Option<&mut PetBonds>, Option<&crate::pets::PetTraits>), Without<Sim>>,
    tubs: Query<(&Transform, &TubSlots, Option<&crate::nav::Floor>), (Without<Sim>, Without<Pet>)>,
    (world, building, grid, upper): (Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>, Option<Res<crate::nav::NavGrid>>, Option<Res<crate::nav::UpperFloors>>),
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    let has = |n: &str| data.0.clip_names.iter().any(|c| c.eq_ignore_ascii_case(n));
    for (e, sim, mut queue, mut stf, mut motives, session) in &mut sims {
        let running = queue.0.front().and_then(|a| match a.kind {
            ActionKind::BathePet { pet, tub, .. } if matches!(a.phase, Phase::Running(_)) && !a.cancel => Some((pet, tub)),
            _ => None,
        });
        let Some(mut s) = session else {
            // (There: the pet's called over, out of whatever it was doing.)
            if let Some((pet, tub)) = running {
                let Ok((p, ptf, ..)) = pets.get(pet) else {
                    queue.0.pop_front();
                    continue;
                };
                // (Where the Sim knelt: the side planned.)
                let Some((places, level)) = tubs.get(tub).ok().and_then(|(ttf, slots, f)| Some((places(slots, ttf, &p.kind, stf.translation.xz(), |_, _| Some(0.0))?, f.map_or(1, |f| f.0)))) else {
                    queue.0.pop_front();
                    continue;
                };
                // (Beside the Sim, a smaller pet; at its slot, a dog; up the stairs to it, if it's
                // upstairs.)
                let to = if carried(&p.kind) { places.sim + (places.pet - places.sim).normalize_or(Vec2::X).perp() * 0.5 } else { places.pet };
                let here = ptf.translation.xz();
                let mut route = grid.as_deref().and_then(|g| crate::nav::plan_route(g, upper.as_deref(), here, 1, to, level)).unwrap_or_default();
                route.push(crate::nav::Waypoint { p: to, level, climb: None, spiral: None });
                debug!("pet bath: the pet's route {:?}", route.iter().map(|w| (w.p, w.level, w.climb)).collect::<Vec<_>>());
                info!("pet bath: {} calls {} over to the tub ({} steps, floor {level}; at {:.1},{:.1} facing {:.2},{:.2})", sim.first, p.name, route.len(), places.sim.x, places.sim.y, places.face.x, places.face.y);
                commands.entity(e).insert(Bathing { pet, tub, places, part: Part::Fetch, time: 0.0, length: 0.0, round: 0, since: now, bad: false, route, level, home: here, base: (Vec3::ZERO, Quat::IDENTITY), pet_clip: String::new() });
                commands.entity(pet).insert((PetBusy, InBath)).remove::<crate::petneeds::PetTask>();
                stf.rotation = Quat::from_rotation_y(places.face.x.atan2(places.face.y));
            }
            continue;
        };
        let Ok((pet, mut ptf, mut anim, mut pm, mut bonds, traits)) = pets.get_mut(s.pet) else {
            commands.entity(e).remove::<Bathing>();
            continue;
        };
        let kind = pet.kind.clone();
        let pet_e = s.pet;
        // (Free again; from upstairs, down to where it was first.)
        let back = (s.level != 1).then(|| grid.as_deref().and_then(|g| crate::nav::plan_route(g, upper.as_deref(), ptf.translation.xz(), s.level, s.home, 1))).flatten();
        let release = |commands: &mut Commands, anim: &mut PetAnim, rng: &mut rand::rngs::ThreadRng| {
            commands.entity(e).remove::<(Bathing, ActionClip)>();
            match back.clone() {
                Some(route) => {
                    commands.entity(pet_e).insert(WalkBack(route));
                }
                None => {
                    commands.entity(pet_e).remove::<(PetBusy, InBath)>();
                }
            }
            anim.play(crate::pets::idle_clip(&kind, false, rng));
        };
        // (Off to something else, or told to stop: over, the pet out of the tub and free.)
        if running.is_none_or(|r| r.0 != s.pet) {
            if matches!(s.part, Part::Start | Part::Loop | Part::Stop) {
                let y = crate::nav::floor_height(&world.data, building.as_deref(), s.level, Vec3::new(s.places.pet.x, ptf.translation.y, s.places.pet.y));
                ptf.translation = Vec3::new(s.places.pet.x, y, s.places.pet.y);
            }
            release(&mut commands, &mut anim, &mut rng);
            continue;
        }
        stf.rotation = Quat::from_rotation_y(s.places.face.x.atan2(s.places.face.y));
        if s.part == Part::Fetch {
            // Up to its place.
            if !s.route.is_empty() {
                let step = pet.walk_speed() * 1.6 * dt;

                walk_route(&mut s.route, &mut ptf, step, |l, p| crate::nav::floor_height(&world.data, building.as_deref(), l, p));
                anim.play(crate::pets::hurry_clip(&kind));
                continue;
            }
            // (Lifted in from the Sim's arms, or hopping in from beside them; in the tub's frame.)
            let from = if carried(&kind) { s.places.sim } else { s.places.pet };
            let y = crate::nav::floor_height(&world.data, building.as_deref(), s.level, Vec3::new(from.x, ptf.translation.y, from.y));
            // (The clips' anchor: the Sim's slot, facing the tub, its arms the cat's; or the dog's
            // own slot beside them.)
            s.base = (Vec3::new(from.x, y, from.y), Quat::from_rotation_y(s.places.face.x.atan2(s.places.face.y)));
            let traits = traits.map_or(&[][..], |t| &t.0[..]);
            s.bad = bad_bath(&kind, traits, rng.random());
            let fail = kind == "ac" && rng.random_bool(CAT_FAIL_CHANCE);
            s.part = if fail { Part::Fail } else { Part::Start };
        } else {
            s.time += dt;
            // (The pet moved by its clip, from where the part began.)
            if let Some(c) = lib.get(&data, &s.pet_clip)
                && let Some((d, turn)) = crate::pets::root_motion(&c, s.time.clamp(0.0, c.duration))
            {
                let (p, r) = s.base;
                let to = p + r * d;
                ptf.translation = Vec3::new(to.x, p.y, to.z);
                ptf.rotation = r * turn;
            }
            // What the washing's worth.
            if s.part == Part::Loop && hours > 0.0 {
                let (pet_rate, sim_rate) = rates(s.bad);
                pm.add(crate::petneeds::HYGIENE, pet_rate * hours);
                motives.add(crate::sim::HYGIENE, sim_rate * hours);
            }
            if s.time < s.length {
                continue;
            }
            // The next part, once this one's played.
            let clean = pm.0[crate::petneeds::HYGIENE] >= 99.0 && now - s.since >= least_minutes(s.bad);
            let next = match s.part {
                Part::Start => Part::Loop,
                Part::Loop if clean => Part::Stop,
                Part::Loop => Part::Loop,
                Part::Stop | Part::Fail | Part::Fetch => {
                    if s.part == Part::Fail {
                        if let Some(b) = bonds.as_deref_mut() {
                            b.add(sim.id, true, CAT_FAIL_LIKING);
                        }
                        notes.push(format!("{} scrambled out of the bath!", pet.name));
                    }
                    info!("pet bath: {} bathed by {} ({:?}), hygiene {:.0}", pet.name, sim.first, s.part, pm.0[crate::petneeds::HYGIENE]);
                    queue.0.pop_front();
                    release(&mut commands, &mut anim, &mut rng);
                    continue;
                }
            };
            if next == Part::Loop {
                s.round += 1;
            }
            s.part = next;
        }
        let (x, y) = (clip_name(&kind, s.part, s.round, s.bad, s.places.mirror, 'x'), clip_name(&kind, s.part, s.round, s.bad, s.places.mirror, 'y'));
        if !has(&x) {
            warn!("pet bath: no {x}");
            queue.0.pop_front();
            release(&mut commands, &mut anim, &mut rng);
            continue;
        }
        debug!("pet bath: {} {:?}: {x}", pet.name, s.part);
        s.time = 0.0;
        // (Done a moment before the clips are, the pet's not looping back to its start.)
        s.length = lib.get(&data, &x).map_or(2.0, |c| c.duration).min(lib.get(&data, &y).map_or(f32::MAX, |c| c.duration)).max(0.3) - 0.05;
        let x: &'static str = crate::pets::intern(x);
        let clips: &'static [&'static str] = Box::leak(Box::new([x]));
        commands.entity(e).insert(ActionClip::new(None, clips));
        anim.replay(y.clone());
        s.pet_clip = y;
        if s.part == Part::Start {
            s.since = now;
        }
    }
}

/// Pets bathed upstairs walk back down, and are free again once there.
fn walk_back(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    mut pets: Query<(Entity, &Pet, &mut WalkBack, &mut Transform, &mut PetAnim)>,
    (world, building): (Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    for (e, pet, mut back, mut tf, mut anim) in &mut pets {
        let step = pet.walk_speed() * 1.6 * dt;
        if walk_route(&mut back.0, &mut tf, step, |l, p| crate::nav::floor_height(&world.data, building.as_deref(), l, p)) {
            commands.entity(e).remove::<(WalkBack, PetBusy, InBath)>();
            anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
        } else {
            anim.play(crate::pets::hurry_clip(&pet.kind));
        }
    }
}

/// The household's grown-ups bathe a dog grown grungy (where there's a tub), of their own accord.
#[allow(clippy::type_complexity)]
fn chores(
    clock: Res<crate::clock::GameClock>,
    pets: Query<(Entity, &Pet, &crate::petneeds::PetMotives), (With<crate::pets::HomePet>, Without<PetBusy>)>,
    tubs: Query<(Entity, &GameObject, &Transform), (With<TubSlots>, Without<crate::visit::LotObject>)>,
    mut sims: Query<(&Sim, &mut ActionQueue, &Transform, Option<&crate::visit::OnLot>), With<crate::sim::HouseholdMember>>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 60.0;
    if sims.iter().any(|(_, q, ..)| q.0.iter().any(|a| matches!(a.kind, ActionKind::BathePet { .. }))) {
        return;
    }
    let Some((pet, p, _)) = pets.iter().find(|(_, p, m)| matches!(p.kind.as_str(), "ad" | "al") && m.0[crate::petneeds::HYGIENE] < GRUNGY) else { return };
    let Some((sim, mut q, tf, _)) = sims.iter_mut().find(|(s, q, _, lot)| s.age.is_grown() && lot.is_none() && q.0.is_empty()) else { return };
    let Some((tub, ..)) = tubs.iter().filter(|(_, o, _)| matches!(o.kind, ObjectKind::Bathtub | ObjectKind::ShowerTub)).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation))) else { return };
    info!("{} bathes {} of their own accord", sim.first, p.name);
    q.0.push_back(bath_action(pet, &p.kind, tub, true));
}

/// A bath of a pet at a tub, to be planned.
pub fn bath_action(pet: Entity, kind: &str, tub: Entity, autonomous: bool) -> crate::interact::Action {
    let kind: &'static str = ["ac", "ad", "al"].into_iter().find(|k| *k == kind).unwrap_or("ad");
    crate::interact::Action::new("Bathe", ActionKind::BathePet { pet, tub, kind, at: Vec2::ZERO, face: Vec2::Y, level: 1, planned: false }, autonomous)
}

/// `PET_BATH=<pet>` (tests): eight seconds in, the selected Sim bathes the household's pet of that
/// name in the lot's nearest tub, the camera on the tub.
#[allow(clippy::type_complexity)]
fn bath_test(
    time: Res<Time>,
    mut done: Local<bool>,
    mut sel: Query<&mut ActionQueue, With<crate::sim::Selected>>,
    pets: Query<(Entity, &Pet, &Transform), With<crate::pets::HomePet>>,
    tubs: Query<(Entity, &GameObject, &Transform), (With<TubSlots>, Without<crate::visit::LotObject>)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
) {
    if *done || time.elapsed_secs() < 8.0 {
        return;
    }
    let Ok(name) = std::env::var("PET_BATH") else { return };
    let Some((pet, p, ptf)) = pets.iter().find(|(_, p, _)| p.name.eq_ignore_ascii_case(&name)) else { return };
    let Some((tub, _, ttf)) = tubs.iter().min_by(|a, b| a.2.translation.distance(ptf.translation).total_cmp(&b.2.translation.distance(ptf.translation))) else { return };
    let Ok(mut q) = sel.single_mut() else { return };
    *done = true;
    if let Ok(mut c) = cams.single_mut() {
        c.look_at(ttf.translation);
    }
    info!("pet bath test: {} to the {} at {}", p.name, tubs.get(tub).map_or(String::new(), |t| t.1.name.clone()), ttf.translation);
    q.push_player(bath_action(pet, &p.kind, tub, false));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tub() -> TubSlots {
        // (The Bathe Today tub's: enter slots 0 and 1, human and dog slots 4 to 7.)
        let s = |n: u8, x: f32, z: f32| (n, Vec2::new(x, z), Vec2::new(-x.signum(), 0.0));
        TubSlots(vec![s(0, -1.0, 0.15), s(1, 1.0, 0.15), s(4, -1.007, 0.106), s(5, -0.929, 0.562), s(6, 1.007, 0.106), s(7, 0.929, 0.562)], false)
    }

    #[test]
    fn a_pet_is_bathed_from_the_nearer_side() {
        let at = Transform::from_xyz(10.0, 0.0, 10.0);
        // (A dog: the Sim at the human slot, facing the tub, the dog at its slot beside them.)
        let near = |from: Vec2| move |p: Vec2, _: Vec2| Some(p.distance(from));
        let p = places(&tub(), &at, "ad", Vec2::new(5.0, 10.0), near(Vec2::new(5.0, 10.0))).unwrap();
        assert!(!p.mirror && (p.sim - Vec2::new(8.993, 10.106)).length() < 1e-3 && (p.pet - Vec2::new(9.071, 10.562)).length() < 1e-3);
        assert!((p.face - Vec2::X).length() < 1e-4);
        // (A cat from the far side: the enter slot, mirrored.)
        let p = places(&tub(), &at, "ac", Vec2::new(15.0, 10.0), near(Vec2::new(15.0, 10.0))).unwrap();
        assert!(p.mirror && (p.sim - Vec2::new(11.0, 10.15)).length() < 1e-3);
        // (That side against a wall: the other.)
        let p = places(&tub(), &at, "ac", Vec2::new(15.0, 10.0), |p, _| (p.x < 10.0).then_some(9.0)).unwrap();
        assert!(!p.mirror);
        assert_eq!(clip_name("ac", Part::Start, 0, false, true, 'y'), "a2ac_tub_bathe_start_mirror_y");
        assert_eq!(clip_name("ad", Part::Loop, 3, true, false, 'x'), "a2ad_tub_bathe_loop1_bad_x");
        assert_eq!(clip_name("al", Part::Loop, 1, false, false, 'x'), "a2al_tub_bathe_loop2_x");
        // (A shower over a tub: its one side, slots 2 and 3; a corner tub's 1 and 2.)
        assert_eq!(TubSlots(tub().0, true).sides(), vec![(0, 2, 3)]);
        assert_eq!(TubSlots(tub().0.into_iter().filter(|s| s.0 < 4).collect(), false).sides(), vec![(0, 1, 2)]);
    }

    #[test]
    fn baths_go_as_the_games_tuning_has_them() {
        assert!(bathable("ac") && bathable("al") && !bathable("cd") && !bathable("ah"));
        assert_eq!(rates(false), (200.0, -200.0));
        assert_eq!(rates(true).0, 100.0);
        // (Only a Hyper or Aggressive dog, half the time.)
        let hyper = vec!["HyperPet".to_string()];
        assert!(bad_bath("ad", &hyper, 0.2) && !bad_bath("ad", &hyper, 0.7) && !bad_bath("ac", &hyper, 0.2) && !bad_bath("ad", &[], 0.0));
        assert_eq!((least_minutes(false), least_minutes(true)), (5.0, 10.0));
    }
}
