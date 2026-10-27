//! Pets seeking company, as the Pets pack's pets do of their own accord (its autonomous socials):
//! a household pet short of company or fun goes to one of the family who's standing free, or to
//! another of the household's pets, and greets them in the pack's own way:
//!
//! - a dog licks a Sim it likes (`Lick`), a cat rubs round their legs (`Mark Sim`), a horse
//!   nuzzles them (`Nuzzle`); one it doesn't know well sniffs them (`Sniff`); and one that dislikes
//!   them a dog pees on (`Pee On`), a cat scratches (`Attack Shred`), a horse bites at (`Bite At`);
//! - pets nuzzle and sniff each other, and cats and dogs short of fun play together (`PlayPetToPet`:
//!   its start, then rounds of play and swapping who chases, for a while).
//!
//! Each plays the pack's clip for its side, the Sim or pet greeted turned to it and playing theirs
//! (`<pet>2<them>_soc_neutral_<social>_x`, `_y`). Company and fun for the pet, company for the
//! Sim, and the pet's liking for them up a little, or down for a mean one. `PET_GREET=<pet>`
//! (tests): that pet seeks company four seconds in.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::clock::{GameClock, SPEED_RATES};
use crate::interact::{Action, ActionKind, ActionQueue};
use crate::petneeds::{FUN, PetMotives, SOCIAL};
use crate::pets::{HomePet, Pet, PetAnim, PetBonds, PetBusy};
use crate::sim::{Age, HouseholdMember, Sim};

pub struct PetCompanyPlugin;

impl Plugin for PetCompanyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (seek_company, greet).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// How a pet greets: the pack's social, and whether it's friendly (or mean).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Greet {
    Sniff,
    Nuzzle,
    Lick,
    MarkSim,
    PeeOn,
    AttackSim,
    BiteAt,
    Play,
}

impl Greet {
    /// The pack's clip stem (`<pet>2<them>_soc_neutral_<stem>_x`).
    fn stem(self) -> &'static str {
        match self {
            Greet::Sniff => "sniff_neutral_neutral",
            Greet::Nuzzle => "nuzzle_neutral_neutral",
            Greet::Lick => "lick_friendly_neutral",
            Greet::MarkSim => "markSimAccept_friendly_neutral",
            Greet::PeeOn => "peeOn_insulting_neutral",
            Greet::AttackSim => "attackSim_insulting_neutral",
            Greet::BiteAt => "biteAt_insulting_neutral",
            Greet::Play => "play",
        }
    }

    pub fn mean(self) -> bool {
        matches!(self, Greet::PeeOn | Greet::AttackSim | Greet::BiteAt)
    }

    /// What it does for the pet's company and fun, the other's company, and the liking between
    /// them (a mean one costs some).
    fn effects(self) -> (f32, f32, f32, f32) {
        match self {
            _ if self.mean() => (10.0, 10.0, -10.0, -5.0),
            Greet::Play => (25.0, 45.0, 20.0, 2.0),
            Greet::Sniff => (15.0, 5.0, 5.0, 1.0),
            _ => (30.0, 10.0, 15.0, 3.0),
        }
    }
}

/// The friendly greeting for a Sim a pet likes, by its species.
fn friendly(kind: &str) -> Greet {
    match kind.get(1..) {
        Some("d" | "l") => Greet::Lick,
        Some("c") => Greet::MarkSim,
        _ => Greet::Nuzzle,
    }
}

/// The mean one for a Sim it dislikes.
fn unfriendly(kind: &str) -> Greet {
    match kind.get(1..) {
        Some("d" | "l") => Greet::PeeOn,
        Some("c") => Greet::AttackSim,
        _ => Greet::BiteAt,
    }
}

/// How a pet would greet a Sim it likes this much (none of the mean ones from the young).
pub fn greeting_for_sim(kind: &str, liking: f32, roll: f32) -> Greet {
    if liking < DISLIKE && roll < 0.5 && kind.starts_with('a') {
        unfriendly(kind)
    } else if liking >= LIKE {
        friendly(kind)
    } else {
        Greet::Sniff
    }
}

/// How a pet would greet another pet: cats and dogs short of fun play; otherwise a nuzzle, now
/// and then a sniff.
pub fn greeting_for_pet(kind: &str, other: &str, bored: bool, roll: f32) -> Greet {
    let playful = |k: &str| matches!(k.get(1..), Some("c" | "d" | "l"));
    if bored && playful(kind) && playful(other) {
        Greet::Play
    } else if roll < 0.6 {
        Greet::Nuzzle
    } else {
        Greet::Sniff
    }
}

/// Liking at which a pet greets a Sim warmly, and under which it may be mean.
const LIKE: f32 = 20.0;
const DISLIKE: f32 = -30.0;
/// A pet seeks company once its company's under this (or its fun's under `BORED`), now and then.
const LONELY: f32 = 40.0;
const BORED: f32 = 25.0;
/// How often a pet thinks of it (game minutes), and how far it'll go (metres).
const EVERY: std::ops::Range<f64> = 20.0..45.0;
const REACH: f32 = 20.0;
/// How long a game of play goes on (game minutes), and how long a pet gives a greeting before it
/// gives up getting there.
const PLAY_MINUTES: f64 = 10.0;
const GIVE_UP: f64 = 60.0;

/// What the other of a greeting is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Other {
    Sim(Entity),
    Pet(Entity),
}

impl Other {
    fn entity(self) -> Entity {
        match self {
            Other::Sim(e) | Other::Pet(e) => e,
        }
    }
}

/// A pet greeting someone: who and how, its way there, when it started (none: on its way), and
/// when it's over. The pet greeted waits under a `Greeted`.
#[derive(Component, Clone, Debug)]
pub struct Greeting {
    pub other: Other,
    pub how: Greet,
    path: Vec<Vec2>,
    set_off: f64,
    since: Option<f64>,
    until: f64,
    /// The clips being played (the pet's, the other's), for a game of play its rounds.
    round: u32,
}

/// A pet being greeted by another: it stays where it is.
#[derive(Component)]
pub struct Greeted(pub Entity);

/// The code a Sim goes by in the pack's clips: `a` grown, `c` a child (none for the little ones).
fn sim_code(sim: &Sim) -> Option<&'static str> {
    match sim.age {
        Age::Baby | Age::Toddler => None,
        Age::Child => Some("c"),
        _ => Some("a"),
    }
}

/// The clips for a greeting, the pet's and the other's, if the game has them.
fn clips(data: &Baked, kind: &str, them: &str, how: Greet, part: &str) -> Option<(&'static str, &'static str)> {
    let has = |n: &str| data.0.clip_names.iter().any(|c| c.eq_ignore_ascii_case(n));
    let stem = if how == Greet::Play { format!("play_{part}") } else { how.stem().to_string() };
    let x = format!("{kind}2{them}_soc_neutral_{stem}_x");
    let y = format!("{kind}2{them}_soc_neutral_{stem}_y");
    (has(&x) && has(&y)).then(|| (crate::pets::intern(x), crate::pets::intern(y)))
}

/// Where a pet stands to greet someone at `at` coming from `from`: facing them, as far off as
/// it stands for a Sim's socials.
fn spot(at: Vec2, from: Vec2, kind: &str) -> Vec2 {
    at + (from - at).normalize_or(Vec2::Y) * crate::pets::social_distance(kind)
}

/// Household pets short of company or fun go and find it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn seek_company(
    mut commands: Commands,
    clock: Res<GameClock>,
    time: Res<Time>,
    data: Res<Baked>,
    grid: Option<Res<crate::nav::NavGrid>>,
    pets: Query<(Entity, &Pet, &PetMotives, &Transform, Option<&PetBonds>, Option<&PetAnim>), (With<HomePet>, Without<crate::petneeds::PetTask>, Without<PetBusy>, Without<Greeting>, Without<Greeted>)>,
    sims: Query<(Entity, &Sim, &Transform, &ActionQueue, Option<&crate::nav::PathFollow>, Option<&crate::interact::OffLot>, &InheritedVisibility), With<HouseholdMember>>,
    mut next: Local<HashMap<Entity, f64>>,
    mut tested: Local<bool>,
) {
    let Some(grid) = grid else { return };
    let now = clock.minutes;
    let night = !(6.0..21.0).contains(&clock.hour_f());
    let mut rng = rand::rng();
    // (PET_GREET=<pet> (tests): that pet, from four seconds in till it's found someone, whatever
    // its needs.)
    let test = (!*tested && time.elapsed_secs() > 4.0).then(|| std::env::var("PET_GREET").ok()).flatten();
    let free_sim = |q: &ActionQueue, p: Option<&crate::nav::PathFollow>, off: bool, vis: &InheritedVisibility| q.0.is_empty() && p.is_none_or(|p| p.done) && !off && vis.get();
    let mut claimed: Vec<Entity> = Vec::new();
    for (e, pet, m, tf, bonds, anim) in &pets {
        // ("<pet>:<whom>": that one, by name.)
        let (who, whom) = test.as_deref().map_or(("", None), |t| t.split_once(':').map_or((t, None), |(a, b)| (a, Some(b))));
        let forced = pet.name.eq_ignore_ascii_case(who);
        let named = |n: &str| !forced || whom.is_none_or(|w| w.eq_ignore_ascii_case(n));
        if !forced {
            if claimed.contains(&e) || night || anim.is_some_and(|a| a.clip.contains("sleep")) {
                continue;
            }
            let at = *next.entry(e).or_insert_with(|| now + rng.random_range(EVERY));
            if now < at {
                continue;
            }
            next.insert(e, now + rng.random_range(EVERY));
            let (lonely, bored) = (m.0[SOCIAL] < LONELY, m.0[FUN] < BORED);
            if !(lonely || bored) || !rng.random_bool(0.6) {
                continue;
            }
        }
        let here = tf.translation.xz();
        let bored = m.0[FUN] < BORED || forced;
        // Whoever's nearest and free: one of the family, or another pet.
        let mut best: Option<(f32, Other, Greet, Vec2)> = None;
        for (s, sim, stf, q, path, off, vis) in &sims {
            let Some(code) = sim_code(sim) else { continue };
            if !free_sim(q, path, off.is_some(), vis) || claimed.contains(&s) || !named(&sim.first) {
                continue;
            }
            let d = stf.translation.xz().distance(here);
            if d > REACH || best.as_ref().is_some_and(|b| b.0 <= d) {
                continue;
            }
            let liking = bonds.map_or(crate::pets::FAMILY_LIKING, |b| b.liking(sim.id, true));
            let mut how = greeting_for_sim(&pet.kind, liking, rng.random());
            if clips(&data, &pet.kind, code, how, "").is_none() {
                how = Greet::Sniff;
            }
            if clips(&data, &pet.kind, code, how, "").is_some() {
                best = Some((d, Other::Sim(s), how, stf.translation.xz()));
            }
        }
        for (o, other, _, otf, ..) in &pets {
            if o == e || claimed.contains(&o) || !named(&other.name) {
                continue;
            }
            // (Company of their own kind counts for a little less than the family's.)
            let d = otf.translation.xz().distance(here) * 1.3;
            if d > REACH || best.as_ref().is_some_and(|b| b.0 <= d) {
                continue;
            }
            let mut how = greeting_for_pet(&pet.kind, &other.kind, bored, rng.random());
            let part = if how == Greet::Play { "start" } else { "" };
            if clips(&data, &pet.kind, &other.kind, how, part).is_none() {
                how = Greet::Sniff;
            }
            let part = if how == Greet::Play { "start" } else { "" };
            if clips(&data, &pet.kind, &other.kind, how, part).is_some() {
                best = Some((d, Other::Pet(o), how, otf.translation.xz()));
            }
        }
        let Some((_, other, how, at)) = best else { continue };
        let to = spot(at, here, &pet.kind);
        let path = grid.find_path(here, to).or_else(|| grid.nearest_free(to).and_then(|(x, z)| grid.find_path(here, grid.center_of(x, z))));
        let Some(mut path) = path else { continue };
        if path.last().is_none_or(|l| l.distance(to) > 0.05) {
            path.push(to);
        }
        info!("pet {} goes to greet {:?} ({how:?})", pet.name, other);
        *tested |= forced;
        claimed.extend([e, other.entity()]);
        if let Other::Pet(o) = other {
            commands.entity(o).insert(Greeted(e));
        }
        commands.entity(e).insert(Greeting { other, how, path, set_off: now, since: None, until: 0.0, round: 0 });
    }
}

/// Pets on their way to greet someone, then at it: the pet's side and the other's together, and
/// what comes of it once it's over.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn greet(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    data: Res<Baked>,
    mut lib: ResMut<ClipLibrary>,
    world: Res<crate::loading::CurrentWorld>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    grid: Option<Res<crate::nav::NavGrid>>,
    mut pets: Query<(Entity, &Pet, &mut Transform, &mut PetAnim, Option<&mut Greeting>, Option<&mut PetMotives>, Option<&mut PetBonds>), Without<Sim>>,
    mut sims: Query<(&Sim, &Transform, &mut ActionQueue, Option<&crate::nav::PathFollow>, &mut crate::sim::Motives), (With<HouseholdMember>, Without<Pet>)>,
    greeted: Query<(Entity, &Greeted)>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed].min(3.0);
    let now = clock.minutes;
    let mut rng = rand::rng();
    let leaders: Vec<Entity> = pets.iter().filter(|p| p.4.is_some()).map(|p| p.0).collect();
    // (A pet waiting for one that's no longer coming is free again.)
    for (e, g) in &greeted {
        if !pets.get(g.0).is_ok_and(|p| p.4.is_some_and(|x| x.other == Other::Pet(e))) {
            commands.entity(e).remove::<Greeted>();
        }
    }
    for e in leaders {
        let Ok((_, pet, tf, _, Some(g), ..)) = pets.get(e) else { continue };
        let (kind, name, walk, here, g) = (pet.kind.clone(), pet.name.clone(), pet.walk_speed(), tf.translation.xz(), g.clone());
        let other = g.other.entity();
        // Where the other is, and what they go by in the clips.
        let (them, there, code) = match g.other {
            Other::Sim(s) => match sims.get(s) {
                Ok((sim, stf, ..)) => (stf.translation, stf.translation.xz(), sim_code(sim).unwrap_or("a").to_string()),
                Err(_) => {
                    end(&mut commands, e, None, &mut pets, &mut rng);
                    continue;
                }
            },
            Other::Pet(p) => match pets.get(p) {
                Ok((_, op, otf, ..)) => (otf.translation, otf.translation.xz(), op.kind.clone()),
                Err(_) => {
                    end(&mut commands, e, None, &mut pets, &mut rng);
                    continue;
                }
            },
        };
        let _ = them;
        // On the way.
        if g.since.is_none() {
            // (Too long getting there: given up.)
            if now - g.set_off > GIVE_UP {
                debug!("pet {name} gives up greeting");
                end(&mut commands, e, Some(other), &mut pets, &mut rng);
                continue;
            }
            if let Some(&next) = g.path.first() {
                let d = next - here;
                let step = walk * 1.6 * dt;
                let Ok((_, _, mut tf, mut anim, Some(mut gm), ..)) = pets.get_mut(e) else { continue };
                if d.length() <= step.max(0.05) {
                    gm.path.remove(0);
                } else {
                    let p = here + d.normalize() * step;
                    let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(p.x, tf.translation.y, p.y));
                    tf.translation = Vec3::new(p.x, y, p.y);
                    tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
                    anim.play(format!("{kind}_trot_x"));
                }
                continue;
            }
            // (They've moved on from where it was going: after them again, if near; a Sim busy
            // at something, or walking, waited on.)
            if there.distance(here) > crate::pets::social_distance(&kind) + 0.8 {
                let to = spot(there, here, &kind);
                match grid.as_deref().and_then(|grid| grid.find_path(here, to)).filter(|_| there.distance(here) < REACH) {
                    Some(mut p) => {
                        p.push(to);
                        if let Ok((.., Some(mut gm), _, _)) = pets.get_mut(e) {
                            gm.path = p;
                        }
                    }
                    None => end(&mut commands, e, Some(other), &mut pets, &mut rng),
                }
                continue;
            }
            let waiting = matches!(g.other, Other::Sim(s) if sims.get(s).is_ok_and(|(_, _, q, p, _)| {
                p.is_some_and(|p| !p.done) || q.0.front().is_some_and(|a| !a.autonomous || matches!(a.phase, crate::interact::Phase::Running(_)) || matches!(a.kind, ActionKind::Outro { .. }))
            }));
            if waiting {
                if let Ok((_, pet, _, mut anim, ..)) = pets.get_mut(e) {
                    let k = &pet.kind;
                    anim.play(if k.ends_with('h') { format!("{k}_idle_stand_breathe_x") } else { format!("{k}_idle_sit_breathe_x") });
                }
                continue;
            }
            // There: the two face each other and play their sides.
            let part = if g.how == Greet::Play { "start" } else { "" };
            let Some((x, y)) = clips(&data, &kind, &code, g.how, part) else {
                end(&mut commands, e, Some(other), &mut pets, &mut rng);
                continue;
            };
            let secs = lib.get(&data, x).map_or(4.0, |c| c.duration);
            let toward = there - here;
            let until = now + if g.how == Greet::Play { PLAY_MINUTES } else { secs as f64 };
            if let Ok((_, _, mut tf, mut anim, Some(mut gm), ..)) = pets.get_mut(e) {
                if toward.length() > 0.05 {
                    tf.rotation = Quat::from_rotation_y(toward.x.atan2(toward.y));
                }
                anim.replay(x.to_string());
                gm.since = Some(now);
                gm.until = until;
            }
            match g.other {
                Other::Sim(s) => {
                    if let Ok((sim, _, mut q, ..)) = sims.get_mut(s) {
                        q.0.push_front(Action::new("Greeted", ActionKind::Outro { clips: Box::leak(vec![y].into_boxed_slice()), then: None, secs, stand_at: None, target: e }, true));
                        commands.entity(s).try_remove::<crate::anim::FaceOverlay>().try_insert((ActionClip::steps(y, &[], &[]), crate::reactions::Facing(Quat::from_rotation_y((-toward).x.atan2((-toward).y)))));
                        info!("pet {name} greets {} ({:?})", sim.first, g.how);
                    }
                }
                Other::Pet(p) => {
                    if let Ok((_, _, mut otf, mut oanim, ..)) = pets.get_mut(p) {
                        otf.rotation = Quat::from_rotation_y((-toward).x.atan2((-toward).y));
                        oanim.replay(y.to_string());
                    }
                }
            }
            continue;
        }
        // At it: a game of play goes round in rounds (play, and swap who chases).
        if g.how == Greet::Play && now < g.until {
            let at = lib.get(&data, pets.get(e).map(|p| p.3.clip.clone()).unwrap_or_default().as_str()).map_or(3.0, |c| c.duration);
            let played = pets.get(e).map_or(0.0, |p| p.3.time());
            if played >= at {
                const ROUNDS: [&str; 3] = ["loop1", "loop2", "swap"];
                let part = ROUNDS[rng.random_range(0..ROUNDS.len())];
                if let Some((x, y)) = clips(&data, &kind, &code, Greet::Play, part) {
                    if let Ok((_, _, _, mut anim, Some(mut gm), ..)) = pets.get_mut(e) {
                        anim.replay(x.to_string());
                        gm.round += 1;
                    }
                    if let Ok((_, _, _, mut oanim, ..)) = pets.get_mut(other) {
                        oanim.replay(y.to_string());
                    }
                }
            }
            continue;
        }
        if now < g.until {
            continue;
        }
        // Over: what came of it.
        let (pet_social, pet_fun, their_social, liking) = g.how.effects();
        let sim_id = match g.other {
            Other::Sim(s) => sims.get(s).ok().map(|(sim, ..)| sim.id),
            Other::Pet(_) => None,
        };
        if let Ok((_, _, _, _, _, m, bonds)) = pets.get_mut(e) {
            if let Some(mut m) = m {
                m.add(SOCIAL, pet_social);
                m.add(FUN, pet_fun);
            }
            if let (Some(mut b), Some(id)) = (bonds, sim_id) {
                b.add(id, true, liking);
            }
        }
        match g.other {
            Other::Sim(s) => {
                if let Ok((.., mut motives)) = sims.get_mut(s) {
                    motives.add(crate::sim::SOCIAL, their_social);
                }
            }
            Other::Pet(p) => {
                if let Ok((.., Some(mut m), _)) = pets.get_mut(p) {
                    m.add(SOCIAL, their_social);
                    if g.how == Greet::Play {
                        m.add(FUN, pet_fun);
                    }
                }
            }
        }
        info!("pet {name} done greeting ({:?})", g.how);
        end(&mut commands, e, Some(other), &mut pets, &mut rng);
    }
}

/// A greeting over: both free, back to standing about.
#[allow(clippy::type_complexity)]
fn end(
    commands: &mut Commands,
    e: Entity,
    other: Option<Entity>,
    pets: &mut Query<(Entity, &Pet, &mut Transform, &mut PetAnim, Option<&mut Greeting>, Option<&mut PetMotives>, Option<&mut PetBonds>), Without<Sim>>,
    rng: &mut impl Rng,
) {
    commands.entity(e).remove::<Greeting>();
    for p in std::iter::once(e).chain(other) {
        if let Ok((_, pet, _, mut anim, ..)) = pets.get_mut(p) {
            commands.entity(p).try_remove::<Greeted>();
            anim.play(crate::pets::idle_clip(&pet.kind, false, rng));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pets_greet_those_they_like_warmly_and_those_they_dont_meanly() {
        assert_eq!(greeting_for_sim("ad", 50.0, 0.9), Greet::Lick);
        assert_eq!(greeting_for_sim("ac", 50.0, 0.9), Greet::MarkSim);
        assert_eq!(greeting_for_sim("ah", 50.0, 0.9), Greet::Nuzzle);
        assert_eq!(greeting_for_sim("ad", 0.0, 0.9), Greet::Sniff);
        assert_eq!(greeting_for_sim("ad", -60.0, 0.1), Greet::PeeOn);
        assert_eq!(greeting_for_sim("ac", -60.0, 0.1), Greet::AttackSim);
        assert_eq!(greeting_for_sim("cd", -60.0, 0.1), Greet::Sniff, "the young aren't mean");
        assert!(Greet::BiteAt.mean() && !Greet::Lick.mean());
        // (Cats and dogs short of fun play; horses don't.)
        assert_eq!(greeting_for_pet("ad", "ac", true, 0.9), Greet::Play);
        assert_eq!(greeting_for_pet("ah", "ad", true, 0.1), Greet::Nuzzle);
        assert_eq!(greeting_for_pet("ad", "ac", false, 0.9), Greet::Sniff);
    }
}
