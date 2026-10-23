//! Horse riding, the Pets pack's (its `Mount`, `RidingPosture`, `BeingRiddenPosture`, `RideHere`
//! and `Dismount`). A Sim, a teen or older, gets up on one of the household's horses that likes
//! them well enough (the horse's menu's "Ride", offered from the pack's
//! `kMinLtrForMountAvailability` of liking, the horse letting them from `kMinLtrForMountAccept`):
//! round to its left side, where the game's mounting jig stands them (beside the saddle, facing
//! it), they climb up with the pack's clips, the horse's half and the rider's together, near
//! failing at it at first and with a swing once skilled (`kNearFailMountChance`,
//! `kSkilledMountChance`). Up there the rider sits the horse's every move in the pack's own rider
//! clips, in step with it: its idles (the game's `GetRiderAnimationName`: each horse idle's
//! rider's half; the mounted idles, a pat on the neck, the bridle adjusted), and "Ride Here" on
//! the ground takes them there at a walk, or for a way off at a canter (Riding 1: `kLevelForTrot`)
//! and a gallop (Riding 2: `kLevelForGallop`). Riding teaches Riding (a point a minute up there,
//! `kRiderRidingSkillPerMinute`, ten more at a canter or a gallop,
//! `kSimRidingSkillGainWhileGalloping`), the two grow fonder (`kBenefitLTRDelta` every five
//! minutes, `kBenefitTimeBetweenUpdates`) and the horse's fun and company are seen to
//! (`kBenefitHorseCommoditiesToUpdateDeltas`). "Dismount" (the horse's menu, the ride cancelled,
//! or anything else asked of them) gets them down beside it again.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::{ActionClip, ClipLibrary, ClipPlayer};
use crate::baked::Baked;
use crate::clock::{GameClock, SPEED_RATES};
use crate::interact::{ActionKind, ActionQueue, Notifications, Skills};
use crate::pets::{Pet, PetAnim, PetBonds};
use crate::sim::{Pose, Sim, SimAnim};

pub struct RidingPlugin;

impl Plugin for RidingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ride_test.before(ride).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (ride, in_step).chain().before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)))
            .add_systems(PostUpdate, seat_riders.before(bevy::transform::TransformSystems::Propagate).run_if(in_state(PlayMode::Live)));
    }
}

/// The pack's tuning: liking to be offered a ride and to be let up; Riding to canter and gallop;
/// skill points a minute up there and more at speed; liking and the horse's fun and company each
/// five minutes.
pub const MIN_LIKING_OFFERED: f32 = 5.0;
pub const MIN_LIKING_ACCEPTED: f32 = 10.0;
const CANTER_LEVEL: u32 = 1;
const GALLOP_LEVEL: u32 = 2;
const POINTS_A_MINUTE: f32 = 1.0;
const POINTS_AT_SPEED: f32 = 10.0;
const BENEFIT_MINUTES: f64 = 5.0;
const BENEFIT_LIKING: f32 = 0.1;
const BENEFIT_HORSE: f32 = 5.0;
/// How far off a canter's worth going to, and a gallop's (metres).
const CANTER_FROM: f32 = 12.0;
const GALLOP_FROM: f32 = 30.0;

/// Where the game's mounting jig stands the Sim to get on and off, in the horse's frame (its
/// clips' frame: by the left of the saddle, turned to face the horse); the rider's mounted
/// clips are in the horse's own.
const JIG_AT: Vec3 = Vec3::new(0.70, 0.0, -0.30);
const JIG_TURN: f32 = -std::f32::consts::FRAC_PI_2;
/// Where the Sim stands at the start of getting on, in the jig (the clips' first pose).
const STAND_AT: Vec3 = Vec3::new(-0.03, 0.0, 0.008);

/// The jig's place and turn by a horse standing so.
fn jig(horse: &Transform) -> Transform {
    Transform::from_translation(horse.translation + horse.rotation * JIG_AT).with_rotation(horse.rotation * Quat::from_rotation_y(JIG_TURN))
}

/// Where a Sim goes to get on a horse standing so.
pub fn mount_slot(horse: &Transform) -> Vec2 {
    jig(horse).transform_point(STAND_AT).xz()
}

/// Who can ride: teens and up.
pub fn can_ride(sim: &Sim) -> bool {
    !sim.age.is_little() && sim.age != crate::sim::Age::Child
}

/// The horse a Sim's up on (or getting on or off), by their queue: their ride under way.
pub fn riding_on(q: &ActionQueue) -> Option<Entity> {
    match q.0.front() {
        Some(a) if matches!(a.phase, crate::interact::Phase::Running(_)) => match a.kind {
            ActionKind::Ride { horse, .. } => Some(horse),
            _ => None,
        },
        _ => None,
    }
}

/// The ways of getting on and off: amateurish, near failing, skilled (horse's clip, rider's).
const MOUNTS: [(&str, &[&str]); 3] = [
    ("ah2a_mount_amateur_x", &["ah2a_mount_amateur_rider_y"]),
    ("ah2a_mount_nearFail_x", &["ah2a_mount_nearFail_rider_y"]),
    ("ah2a_mount_skilled_x", &["ah2a_mount_skilled_rider_y"]),
];
const DISMOUNTS: [(&str, &[&str]); 3] = [
    ("ah2a_dismount_amateur_x", &["ah2a_dismount_amateur_rider_y"]),
    ("ah2a_dismount_nearFail_x", &["ah2a_dismount_nearFail_rider_y"]),
    ("ah2a_dismount_skilled_x", &["ah2a_dismount_skilled_rider_y"]),
];

/// The gaits: walk, canter, gallop (the horse's clip, the rider's).
const GAITS: [(&str, &[&str]); 3] = [("ah_walk_x", &["a_rideHorse_walk_y"]), ("ah_canter_x", &["a_rideHorse_canter_y"]), ("ah_run_x", &["a_rideHorse_run_y"])];

/// The horse's idles with a rider up, each with the rider's half (`GetRiderAnimationName`), the
/// mounted ones too.
const IDLES: [(&str, &[&str]); 18] = [
    ("ah_idle_stand_breathe_x", &["a_idle_stand_breathe_rider_y"]),
    ("ah_idle_stand_breatheSwishTail_x", &["a_idle_stand_breatheSwishTail_rider_y"]),
    ("ah_idle_stand_earFlickL_x", &["a_idle_stand_earFlickL_rider_y"]),
    ("ah_idle_stand_earFlickR_x", &["a_idle_stand_earFlickR_rider_y"]),
    ("ah_idle_stand_lookAround_x", &["a_idle_stand_lookAround_rider_y"]),
    ("ah_idle_stand_lookLeft_x", &["a_idle_stand_lookLeft_rider_y"]),
    ("ah_idle_stand_lookRight_x", &["a_idle_stand_lookRight_rider_y"]),
    ("ah_idle_stand_pawAtFloor_x", &["a_idle_stand_pawAtFloor_rider_y"]),
    ("ah_idle_stand_perkUpHappy_x", &["a_idle_stand_perkUpHappy_rider_y"]),
    ("ah_idle_stand_shakeHead_x", &["a_idle_stand_shakeHead_rider_y"]),
    ("ah_idle_stand_sniffAround_x", &["a_idle_stand_sniffAround_rider_y"]),
    ("ah_idle_stand_snort_x", &["a_idle_stand_snort_rider_y"]),
    ("ah_idle_stand_yawn_x", &["a_idle_stand_yawn_rider_y"]),
    ("ah_idleMount_adjustBridle_x", &["a_idleMount_adjustBridle_rider_y"]),
    ("ah_idleMount_lookAround_x", &["a_idleMount_lookAround_rider_y"]),
    ("ah_idleMount_patBum_x", &["a_idleMount_patBum_rider_y"]),
    ("ah_idleMount_patNeck_x", &["a_idleMount_patNeck_rider_y"]),
    ("ah_idleMount_stretchShoulders_x", &["a_idleMount_stretchShoulders_rider_y"]),
];

/// How a Sim of a Riding level gets on or off: near failing (eight times in ten untrained, never
/// at 10), with a swing (never untrained, always at 10), else amateurishly.
pub fn way_on(level: u32, roll: f32) -> usize {
    let k = (level.min(10) as f32) / 10.0;
    let near = 0.8 * (1.0 - k);
    if roll < near {
        1
    } else if roll < near + k {
        2
    } else {
        0
    }
}

/// The gait for a ride so far by a Sim of a Riding level: a walk, a canter or a gallop.
pub fn gait_for(level: u32, distance: f32) -> usize {
    if level >= GALLOP_LEVEL && distance >= GALLOP_FROM {
        2
    } else if level >= CANTER_LEVEL && distance >= CANTER_FROM {
        1
    } else {
        0
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    /// Getting on (the way: `MOUNTS`).
    On(usize),
    Up,
    /// Getting off (`DISMOUNTS`).
    Off(usize),
}

/// A Sim on a horse (or getting on or off it): which, how far along, where it's going and at
/// what gait, the rider's clip, and when the ride's next five minutes are up.
#[derive(Component)]
pub struct Mounted {
    pub horse: Entity,
    stage: Stage,
    path: Vec<Vec2>,
    gait: usize,
    rider: &'static [&'static str],
    benefit_at: f64,
    started: bool,
}

impl Mounted {
    pub fn new(horse: Entity) -> Self {
        Self { horse, stage: Stage::On(0), path: Vec::new(), gait: 0, rider: IDLES[0].1, benefit_at: 0.0, started: false }
    }

    /// Up on it (not getting on or off).
    pub fn up(&self) -> bool {
        self.stage == Stage::Up
    }
}

/// A horse with a rider (or one getting on or off).
#[derive(Component)]
pub struct Ridden(pub Entity);

/// The horse and the rider play a pair of clips together.
fn play(anim: &mut PetAnim, m: &mut Mounted, (horse, rider): (&'static str, &'static [&'static str])) {
    anim.replay(horse.to_string());
    m.rider = rider;
}

/// An idle pair for the horse and its rider: mostly just breathing.
fn idle(rng: &mut impl Rng) -> (&'static str, &'static [&'static str]) {
    if rng.random_bool(0.5) { IDLES[0] } else { IDLES[rng.random_range(1..IDLES.len())] }
}

/// Riding: getting on, sitting the horse's idles and gaits where it's ridden to, the ride's
/// benefits, getting off when the ride's over or something else is asked of the rider.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn ride(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    (grid, world, building): (Option<Res<crate::nav::NavGrid>>, Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>),
    mut sims: Query<(Entity, &Sim, &mut Mounted, &mut ActionQueue, &mut SimAnim, &mut Skills, Option<&crate::life::Mood>)>,
    mut horses: Query<(&Pet, &mut Transform, &mut PetAnim, Option<&mut crate::petneeds::PetMotives>, Option<&mut PetBonds>), Without<Sim>>,
    (mut notes, mut life): (ResMut<Notifications>, MessageWriter<crate::life::LifeEvent>),
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    for (e, sim, mut m, mut queue, mut anim, mut skills, mood) in &mut sims {
        let Ok((pet, mut htf, mut hanim, motives, mut bonds)) = horses.get_mut(m.horse) else {
            // (The horse gone: down where they are.)
            queue.0.retain(|a| !matches!(a.kind, ActionKind::Ride { .. }));
            commands.entity(e).remove::<(Mounted, ActionClip)>();
            anim.pose = Pose::Stand;
            continue;
        };
        let level = skills.level("Riding");
        if !m.started {
            m.started = true;
            // (Only one that likes them enough lets them up.)
            let liking = bonds.as_deref().map_or(crate::pets::FAMILY_LIKING, |b| b.liking(sim.id, true));
            if liking < MIN_LIKING_ACCEPTED {
                notes.push(format!("{} won't let {} ride.", pet.name, sim.first));
                queue.0.retain(|a| !matches!(a.kind, ActionKind::Ride { .. }));
                commands.entity(e).remove::<Mounted>();
                continue;
            }
            let way = way_on(level, rng.random());
            m.stage = Stage::On(way);
            m.benefit_at = clock.minutes + BENEFIT_MINUTES;
            play(&mut hanim, &mut m, MOUNTS[way]);
            anim.pose = Pose::Use;
            commands.entity(e).remove::<crate::nav::PathFollow>();
            commands.entity(m.horse).insert((Ridden(e), crate::pets::PetBusy)).remove::<crate::petneeds::PetTask>();
            info!("riding: {} gets on {} ({})", sim.first, pet.name, MOUNTS[way].0);
        }
        let done = |lib: &mut ClipLibrary, a: &PetAnim| lib.get(&data, &a.clip).is_none_or(|c| a.time() >= c.duration);
        match m.stage {
            Stage::On(_) => {
                if done(&mut lib, &hanim) {
                    m.stage = Stage::Up;
                    play(&mut hanim, &mut m, IDLES[0]);
                }
            }
            Stage::Off(_) => {
                if done(&mut lib, &hanim) {
                    // Down again, beside it, where they got on.
                    queue.0.retain(|a| !matches!(a.kind, ActionKind::Ride { .. }));
                    commands.entity(e).remove::<(Mounted, ActionClip)>();
                    commands.entity(m.horse).remove::<Ridden>();
                    hanim.replay(format!("{}_idle_stand_breathe_x", pet.kind));
                    anim.pose = Pose::Stand;
                    info!("riding: {} is off {}", sim.first, pet.name);
                }
            }
            Stage::Up => {
                // "Ride Here": the last place asked for, from where the horse stands.
                let mut off = false;
                let mut to: Option<Vec2> = None;
                let mut i = 1;
                while i < queue.0.len() {
                    match queue.0[i].kind {
                        ActionKind::GoHere(p, _) if p.is_finite() => {
                            to = Some(p);
                            queue.0.remove(i);
                        }
                        ActionKind::Dismount => {
                            off = true;
                            queue.0.remove(i);
                        }
                        _ => {
                            // (Anything else asked of them: off first.)
                            off = true;
                            i += 1;
                        }
                    }
                }
                if !matches!(queue.0.front(), Some(a) if matches!(a.kind, ActionKind::Ride { .. }) && !a.cancel) {
                    off = true;
                }
                if off {
                    let way = way_on(level, rng.random());
                    m.stage = Stage::Off(way);
                    m.path.clear();
                    play(&mut hanim, &mut m, DISMOUNTS[way]);
                    continue;
                }
                let here = htf.translation.xz();
                if let (Some(to), Some(g)) = (to, grid.as_deref()) {
                    let indoors = |p: Vec2| building.as_deref().is_some_and(|b| b.is_indoors(Vec3::new(p.x, 0.0, p.y)));
                    let path = g.nearest_free(to).map(|(x, z)| g.center_of(x, z)).filter(|p| !indoors(*p)).and_then(|p| g.find_path(here, p)).filter(|p| !p.iter().any(|q| indoors(*q)));
                    match path {
                        Some(p) => {
                            let far = p.iter().fold((here, 0.0), |(a, d), b| (*b, d + a.distance(*b))).1;
                            m.gait = gait_for(level, far);
                            m.path = p;
                        }
                        None => notes.push(format!("{} can't ride {} there.", sim.first, pet.name)),
                    }
                }
                // Along the way at its gait, in step with its clip's own pace.
                if let Some(&next) = m.path.first() {
                    let (clip, rider) = GAITS[m.gait];
                    if hanim.clip != clip {
                        play(&mut hanim, &mut m, (clip, rider));
                    }
                    let pace = lib.get(&data, clip).map_or(1.25, |c| crate::pets::clip_pace(&c));
                    let d = next - here;
                    let step = pace * dt;
                    if d.length() <= step.max(0.05) {
                        m.path.remove(0);
                        if m.path.is_empty() {
                            play(&mut hanim, &mut m, IDLES[0]);
                        }
                    } else {
                        let p = here + d.normalize() * step;
                        let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(p.x, htf.translation.y, p.y));
                        htf.translation = Vec3::new(p.x, y, p.y);
                        htf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
                    }
                } else if done(&mut lib, &hanim) {
                    play(&mut hanim, &mut m, idle(&mut rng));
                }
                // What a ride does them both.
                let minutes = delta.0;
                if minutes > 0.0 {
                    let fast = !m.path.is_empty() && m.gait > 0;
                    let rate = crate::life::skill_rate(&sim.traits, "Riding") * mood.map_or(1.0, |m| m.skill_rate());
                    let points = (POINTS_A_MINUTE + if fast { POINTS_AT_SPEED } else { 0.0 }) * minutes * rate;
                    if let Some(l) = skills.add_points("Riding", points) {
                        notes.push(format!("{} reached level {l} in Riding!", sim.first));
                        life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: "Riding", level: l }));
                    }
                }
                if clock.minutes >= m.benefit_at {
                    m.benefit_at = clock.minutes + BENEFIT_MINUTES;
                    if let Some(b) = bonds.as_deref_mut() {
                        b.add(sim.id, true, BENEFIT_LIKING);
                    }
                    if let Some(mut mo) = motives {
                        mo.add(crate::petneeds::FUN, BENEFIT_HORSE);
                        mo.add(crate::petneeds::SOCIAL, BENEFIT_HORSE);
                    }
                }
            }
        }
    }
}

/// `RIDE_TEST=<horse>[:<dx>,<dz>[,<seconds>]]` (tests): eight seconds in, the selected Sim rides the
/// household's horse of that name, the camera on it (`RIDE_DIST` away, 6 m); up on it three seconds,
/// ridden that far off (metres east and south); and after so many seconds up, off again.
/// `RIDE_SKILL=<level>`: the rider that good.
fn ride_test(
    time: Res<Time>,
    mut stage: Local<(u8, f32)>,
    mut sel: Query<(&mut ActionQueue, Option<&Mounted>, &mut Skills), With<crate::sim::Selected>>,
    horses: Query<(Entity, &Pet, &Transform), With<crate::pets::HomePet>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
) {
    let Ok(v) = std::env::var("RIDE_TEST") else { return };
    let (name, rest) = v.split_once(':').unwrap_or((v.as_str(), ""));
    let n: Vec<f32> = rest.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    let now = time.elapsed_secs();
    let (Ok((mut q, m, mut skills)), Some((h, _, htf))) = (sel.single_mut(), horses.iter().find(|(_, p, _)| p.name.eq_ignore_ascii_case(name))) else { return };
    if let Ok(mut c) = cams.single_mut() {
        c.look_at(htf.translation);
        c.distance = std::env::var("RIDE_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(6.0);
    }
    match stage.0 {
        0 if now > 8.0 => {
            // (RIDE_SKILL=<level>: that good a rider.)
            if let Some(l) = std::env::var("RIDE_SKILL").ok().and_then(|l| l.parse::<f32>().ok()) {
                skills.0.insert("Riding", l);
            }
            info!("ride test: riding {name}");
            q.push_player(crate::interact::Action::new("Ride", ActionKind::Ride { horse: h, slot: mount_slot(htf) }, false));
            *stage = (1, now);
        }
        1 if m.is_some_and(|m| m.up()) => *stage = (2, now),
        2 if now - stage.1 > 3.0 && n.len() >= 2 => {
            info!("ride test: riding {} m off", Vec2::new(n[0], n[1]).length());
            q.push_player(crate::interact::Action::new("Ride Here", ActionKind::GoHere(htf.translation.xz() + Vec2::new(n[0], n[1]), 1), false));
            *stage = (3, stage.1);
        }
        3 if n.len() >= 3 && now - stage.1 > n[2] => {
            info!("ride test: dismounting");
            q.push_player(crate::interact::Action::new("Dismount", ActionKind::Dismount, false));
            *stage = (4, now);
        }
        _ => {}
    }
}

/// The rider's clip played with the horse's, at the same time through it.
fn in_step(mut commands: Commands, mut sims: Query<(Entity, &Mounted, Option<&ActionClip>, &mut ClipPlayer)>, horses: Query<&PetAnim>) {
    for (e, m, clip, mut player) in &mut sims {
        let want = ActionClip::new(None, m.rider);
        if clip != Some(&want) {
            commands.entity(e).insert(want);
        }
        let Ok(h) = horses.get(m.horse) else { continue };
        let Some(dur) = player.clip.as_ref().map(|c| c.duration).filter(|d| *d > 0.0) else { continue };
        // (Getting on and off play once, held at their end; the rest go round.)
        player.time = if m.up() { h.time() % dur } else { h.time().min(dur - 1e-3) };
    }
}

/// The rider where the horse is: up on it in its own frame, getting on and off in the jig beside
/// it.
fn seat_riders(mut sims: Query<(&Mounted, &mut Transform), With<Sim>>, horses: Query<&Transform, (With<Pet>, Without<Sim>)>) {
    for (m, mut tf) in &mut sims {
        let Ok(h) = horses.get(m.horse) else { continue };
        let at = if m.started && m.up() { *h } else { jig(h) };
        if *tf != at {
            *tf = at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jig_is_by_the_horses_left_facing_it() {
        let horse = Transform::from_xyz(10.0, 0.0, 5.0);
        let j = jig(&horse);
        // (Its left: +X with it facing +Z; the Sim facing back towards it, -X.)
        assert!((j.translation - Vec3::new(10.7, 0.0, 4.7)).length() < 1e-5);
        let fwd = j.rotation * Vec3::Z;
        assert!((fwd - Vec3::NEG_X).length() < 1e-5);
        let slot = mount_slot(&horse);
        assert!((slot - Vec2::new(10.692, 4.67)).length() < 0.01, "{slot}");
        // (Turned with the horse.)
        let turned = Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        assert!((jig(&turned).translation - Vec3::new(-0.30, 0.0, -0.70)).length() < 1e-5);
    }

    #[test]
    fn skill_brings_better_mounts_and_faster_gaits() {
        // (Untrained: near failing four times in five.)
        assert_eq!(way_on(0, 0.5), 1);
        assert_eq!(way_on(0, 0.9), 0);
        // (Master riders always with a swing.)
        assert_eq!(way_on(10, 0.0), 2);
        assert_eq!(way_on(10, 0.99), 2);
        assert_eq!(gait_for(0, 100.0), 0);
        assert_eq!(gait_for(1, 100.0), 1);
        assert_eq!(gait_for(2, 100.0), 2);
        assert_eq!(gait_for(5, 5.0), 0);
        assert_eq!(gait_for(5, 20.0), 1);
    }

    #[test]
    fn rider_clips_are_seated_on_the_horse() {
        assert_eq!(crate::anim::rider_clip("a_rideHorse_canter_y"), Some(Some("ah_canter_x")));
        assert_eq!(crate::anim::rider_clip("a_idle_stand_breathe_rider_y"), Some(None));
        assert_eq!(crate::anim::rider_clip("ah2a_mount_amateur_rider_y"), Some(None));
        assert_eq!(crate::anim::rider_clip("a_idle_stand_breathe_x"), None);
        for (h, r) in IDLES.iter().chain(&GAITS).chain(&MOUNTS).chain(&DISMOUNTS) {
            assert!(h.starts_with("ah"));
            assert!(crate::anim::rider_clip(r[0]).is_some(), "{}", r[0]);
        }
    }
}
