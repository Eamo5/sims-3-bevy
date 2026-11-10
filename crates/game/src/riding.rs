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
        app.add_systems(Update, (ride_test, dress).before(ride).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (ride, in_step).chain().before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)))
            .add_systems(PostUpdate, seat_riders.before(bevy::transform::TransformSystems::Propagate).run_if(in_state(PlayMode::Live)));
    }
}

/// The pack's tuning: liking to be offered a ride and to be let up; Riding to canter and gallop;
/// skill points a minute up there and more at a gallop (`kSimRidingSkillGainWhileGalloping`), the
/// horse's Racing at a gallop (`kHorseRacingSkillGainWhileGalloping`); every ten minutes
/// (`kBenefitTimeBetweenUpdates`) liking (`kBenefitLTRDelta`), the horse's exercise and company and
/// the rider's fun and company (`kBenefit…CommoditiesToUpdateDeltas`) and Riding
/// (`kBenefitSimSkillsToUpdateDeltas`), as `BeingRiddenPosture`'s tuning has them.
pub const MIN_LIKING_OFFERED: f32 = -100.0;
pub const MIN_LIKING_ACCEPTED: f32 = -10.0;
/// Below this Riding a rider sits a horse's gaits unsteadily (`kLevelForStableRider`, the pack's
/// `_unsteady` rider clips).
pub const STABLE_LEVEL: u32 = 5;
const CANTER_LEVEL: u32 = 1;
const GALLOP_LEVEL: u32 = 2;
const POINTS_A_MINUTE: f32 = 1.0;
const POINTS_AT_SPEED: f32 = 20.0;
const RACING_AT_SPEED: f32 = 20.0;
const BENEFIT_MINUTES: f64 = 10.0;
const BENEFIT_LIKING: f32 = 0.25;
const BENEFIT_EXERCISE: f32 = 1.0;
const BENEFIT_HORSE_SOCIAL: f32 = 2.0;
const BENEFIT_RIDER: f32 = 0.5;
const BENEFIT_RIDING: f32 = 3.0;
/// A horse throwing its rider (`BeingRiddenPosture`'s tuning): at this mood or worse, so likely
/// as its mood changes (an ornery one's sooner and likelier: `kOrneryMoodThresholdToThrowRider`);
/// a starving or exhausted one so many minutes after they're up (`kDistressThrowRiderDelay`); the
/// liking it costs (`ThrowRider.kLTRDelta`).
const THROW_MOOD: f32 = -30.0;
const THROW_CHANCE: f64 = 0.2;
const ORNERY_THROW_MOOD: f32 = 25.0;
const ORNERY_THROW_CHANCE: f64 = 0.4;
const DISTRESS_THROW: (f64, f64) = (15.0, 20.0);
const THROW_LIKING: f32 = -50.0;
/// Where the thrown rider's clips are, behind the horse (their first pose's seat over its saddle).
const THROWN_AT: Vec3 = Vec3::new(0.0, 0.0, -1.93);
/// How long up before a rider's saddle sore, untrained to master (`RidingSkill`'s
/// `kMinSaddleSoreTime`, `kMaxSaddleSoreTime`, minutes).
const SADDLE_SORE: (f64, f64) = (180.0, 1080.0);

/// When a rider of a Riding level gets saddle sore, so many minutes up.
pub fn saddle_sore_after(level: u32) -> f64 {
    SADDLE_SORE.0 + (SADDLE_SORE.1 - SADDLE_SORE.0) * level.min(10) as f64 / 10.0
}

/// Whether a horse in this mood, with these traits, throws its rider (`r`, 0 to 1, the roll):
/// never a gentle one, sooner a mean (ornery) one.
pub fn throws(mood: f32, traits: &[String], r: f64) -> bool {
    if crate::pettraits::has(traits, "GentlePet") {
        return false;
    }
    let (at, chance) = if crate::pettraits::has(traits, "MeanPet") { (ORNERY_THROW_MOOD, ORNERY_THROW_CHANCE) } else { (THROW_MOOD, THROW_CHANCE) };
    mood <= at && r < chance
}

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
            ActionKind::Ride { horse, unsaddle: false, .. } => Some(horse),
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

/// A horse's gallop by its Racing, and its rider's: faster from 4 (`kLevelForGallopFasterSpeed`,
/// the pack's `ah_run_speed2`), fastest at 10 (`kLevelForGallopFastestSpeed`).
pub fn gallop(racing: u32) -> (&'static str, &'static [&'static str]) {
    match racing {
        10.. => ("ah_run_speed3_x", &["a_run_speed3_rider_y"]),
        4.. => ("ah_run_speed2_x", &["a_run_speed2_rider_y"]),
        _ => GAITS[2],
    }
}

/// A gait's rider's half for a rider of a Riding level: unsteady below `STABLE_LEVEL`.
pub fn seat(rider: &'static [&'static str], level: u32) -> &'static [&'static str] {
    if level >= STABLE_LEVEL {
        return rider;
    }
    match rider.first().copied() {
        Some("a_rideHorse_walk_y") => &["a_rideHorse_walk_unsteady_y"],
        Some("a_rideHorse_canter_y") => &["a_rideHorse_canter_unsteady_y"],
        Some("a_rideHorse_run_y") => &["a_rideHorse_run_unsteady_y"],
        Some("a_run_speed2_rider_y") => &["a_run_speed2_unsteady_rider_y"],
        Some("a_run_speed3_rider_y") => &["a_run_speed3_unsteady_rider_y"],
        _ => rider,
    }
}

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

/// How a Sim of a Riding level gets on or off: near failing (three times in ten untrained, never
/// at 10: `kNearFailMountChance`), with a swing (never untrained, always at 10:
/// `kSkilledMountChance`), else amateurishly.
pub fn way_on(level: u32, roll: f32) -> usize {
    let k = (level.min(10) as f32) / 10.0;
    let near = 0.3 * (1.0 - k);
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
    /// Saddling it first (or, `false`, taking its saddle off): how far into the swipe.
    Saddle(bool, f32),
    /// Getting on (the way: `MOUNTS`).
    On(usize),
    Up,
    /// Over a jump (`horsejumps`).
    Jump,
    /// Thrown off (`ThrowRider`).
    Thrown,
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
    /// Only come to take its saddle off.
    unsaddle: bool,
    /// On the way to a jump (`horsejumps`): which, its start and the way over it.
    jump: Option<(Entity, Vec2, Vec2)>,
    /// Off to the Equestrian Center (`equestrian`): the lot and what for, ridden off the lot's
    /// edge.
    compete: Option<(usize, crate::equestrian::CenterTask)>,
    /// When they'll be saddle sore, and when a starving or exhausted horse throws them (game
    /// minutes).
    sore_at: f64,
    throw_at: Option<f64>,
    /// To be tied up at a hitching post (`hitching`): which, at which place, the spot there.
    tie: Option<(Entity, usize, Vec2)>,
}

impl Mounted {
    pub fn new(horse: Entity, unsaddle: bool) -> Self {
        Self { horse, stage: Stage::On(0), path: Vec::new(), gait: 0, rider: IDLES[0].1, benefit_at: 0.0, started: false, unsaddle, jump: None, compete: None, sore_at: f64::INFINITY, throw_at: None, tie: None }
    }

    /// Playing in step with the horse (not saddling it).
    fn in_step(&self) -> bool {
        !matches!(self.stage, Stage::Saddle(..))
    }

    /// Up on it (not getting on or off).
    pub fn up(&self) -> bool {
        self.stage == Stage::Up
    }

    /// Up on it and standing about: going nowhere, over nothing.
    pub fn idle(&self) -> bool {
        self.stage == Stage::Up && self.path.is_empty() && self.jump.is_none()
    }

    /// Sat on it (up, or over a jump).
    fn seated(&self) -> bool {
        matches!(self.stage, Stage::Up | Stage::Jump)
    }
}

/// A horse with a rider (or one getting on or off, or seeing to its saddle).
#[derive(Component)]
pub struct Ridden(pub Entity);

/// A horse's riding tack on it (its meshes): put on to ride (the game's `Saddle`, a swipe of the
/// hand), left on after, taken off from its menu (`Unsaddle`).
#[derive(Component)]
pub struct Tack(pub Vec<Entity>);

/// A horse to have its tack on (left saddled when the game was saved).
#[derive(Component)]
pub struct WantsTack;

/// Horses left saddled saddled again, once their skeletons are up.
#[allow(clippy::type_complexity)]
fn dress(
    mut commands: Commands,
    q: Query<(Entity, &Pet, &crate::pets::PetSkeleton), With<WantsTack>>,
    (pet_data, data, mut assets): (Res<crate::pets::PetData>, Res<Baked>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(d) = pet_data.0.as_deref() else { return };
    for (e, pet, skel) in &q {
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        let parts = crate::pets::put_on_tack(&mut commands, d, &pet.kind, skel, e, &mut ctx, &mut assets);
        commands.entity(e).remove::<WantsTack>().insert(Tack(parts));
    }
}

/// The swipe that saddles a horse, or unsaddles it, its length, and when the tack's on or off.
const SWIPE_ON: &[&str] = &["a_saddle_swipeSaddle_x"];
const SWIPE_OFF: &[&str] = &["a_saddle_swipeUnsaddle_x"];
const SWIPE_LEN: f32 = 1.7;
const SWIPE_AT: f32 = 0.67;

/// The horse and the rider play a pair of clips together.
fn play(anim: &mut PetAnim, m: &mut Mounted, (horse, rider): (&'static str, &'static [&'static str])) {
    anim.replay(horse.to_string());
    m.rider = rider;
}

/// Up onto the horse, as well as their Riding has them.
fn mount(anim: &mut PetAnim, m: &mut Mounted, level: u32, rng: &mut impl Rng) {
    let way = way_on(level, rng.random());
    m.stage = Stage::On(way);
    play(anim, m, MOUNTS[way]);
}

/// An idle pair for the horse and its rider: mostly just breathing.
fn idle(rng: &mut impl Rng) -> (&'static str, &'static [&'static str]) {
    if rng.random_bool(0.5) { IDLES[0] } else { IDLES[rng.random_range(1..IDLES.len())] }
}

/// Riding: getting on, sitting the horse's idles and gaits where it's ridden to, the ride's
/// benefits, getting off when the ride's over or something else is asked of the rider.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn ride(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    (grid, world, building, exit): (Option<Res<crate::nav::NavGrid>>, Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>, Option<Res<crate::interact::LotExit>>),
    mut sims: Query<(Entity, &Sim, &mut Mounted, &mut ActionQueue, &mut SimAnim, &mut Skills, (Option<&crate::life::Mood>, Option<&mut crate::sim::Motives>, Option<&mut crate::life::Moodlets>)), Without<crate::rabbitholes::AtRabbitHole>>,
    mut horses: Query<(&Pet, &mut Transform, &mut PetAnim, Option<&mut crate::petneeds::PetMotives>, Option<&mut PetBonds>, (&crate::pets::PetSkeleton, Option<&Tack>, Option<&mut crate::horsejumps::HorseSkills>, Option<&crate::pets::PetTraits>, Option<&crate::life::Mood>)), Without<Sim>>,
    (mut notes, mut life): (ResMut<Notifications>, MessageWriter<crate::life::LifeEvent>),
    (pet_data, mut assets, mut meshes, mut images, mut materials): (Res<crate::pets::PetData>, ResMut<crate::objects::ObjectAssets>, ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (obstacles, jumps, (posts, tied)): (
        Query<(&crate::horsejumps::Obstacle, &Transform), (Without<Pet>, Without<Sim>)>,
        Query<(Has<crate::horsejumps::HorseJumping>, Has<crate::horsejumps::JumpRequest>)>,
        (Query<&Transform, (With<crate::interact::GameObject>, Without<Pet>, Without<Sim>)>, Query<&crate::hitching::Tied>),
    ),
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    for (e, sim, mut m, mut queue, mut anim, mut skills, (mood, mut sim_motives, mut moodlets)) in &mut sims {
        let Ok((pet, mut htf, mut hanim, mut motives, mut bonds, (skel, tack, mut hskills, htraits, hmood))) = horses.get_mut(m.horse) else {
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
            if !m.unsaddle && liking < MIN_LIKING_ACCEPTED {
                notes.push(format!("{} won't let {} ride.", pet.name, sim.first));
                queue.0.retain(|a| !matches!(a.kind, ActionKind::Ride { .. }));
                commands.entity(e).remove::<Mounted>();
                continue;
            }
            m.benefit_at = clock.minutes + BENEFIT_MINUTES;
            anim.pose = Pose::Use;
            commands.entity(e).remove::<crate::nav::PathFollow>();
            commands.entity(m.horse).insert((Ridden(e), crate::pets::PetBusy)).remove::<crate::petneeds::PetTask>();
            // (Saddled first, if it isn't; or come only to take its saddle off.)
            if m.unsaddle || tack.is_none() {
                m.stage = Stage::Saddle(!m.unsaddle, 0.0);
                m.rider = if m.unsaddle { SWIPE_OFF } else { SWIPE_ON };
                hanim.replay(IDLES[0].0.to_string());
            } else {
                mount(&mut hanim, &mut m, level, &mut rng);
                info!("riding: {} gets on {}", sim.first, pet.name);
            }
        }
        let done = |lib: &mut ClipLibrary, a: &PetAnim| lib.get(&data, &a.clip).is_none_or(|c| a.time() >= c.duration);
        match m.stage {
            Stage::Saddle(on, t) => {
                let t = t + dt;
                m.stage = Stage::Saddle(on, t);
                if t >= SWIPE_AT && on && tack.is_none() {
                    let Some(d) = pet_data.0.as_deref() else { continue };
                    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                    let parts = crate::pets::put_on_tack(&mut commands, d, &pet.kind, skel, m.horse, &mut ctx, &mut assets);
                    commands.entity(m.horse).insert(Tack(parts));
                }
                if t >= SWIPE_AT && !on && let Some(Tack(parts)) = tack {
                    for p in parts {
                        commands.entity(*p).try_despawn();
                    }
                    commands.entity(m.horse).remove::<Tack>();
                }
                if t >= SWIPE_LEN {
                    if on {
                        mount(&mut hanim, &mut m, level, &mut rng);
                        info!("riding: {} saddles {} and gets on", sim.first, pet.name);
                    } else {
                        queue.0.retain(|a| !matches!(a.kind, ActionKind::Ride { .. }));
                        commands.entity(e).remove::<(Mounted, ActionClip)>();
                        commands.entity(m.horse).remove::<Ridden>();
                        anim.pose = Pose::Stand;
                        info!("riding: {} unsaddles {}", sim.first, pet.name);
                    }
                }
            }
            Stage::On(_) => {
                if done(&mut lib, &hanim) {
                    m.stage = Stage::Up;
                    play(&mut hanim, &mut m, IDLES[0]);
                    m.sore_at = clock.minutes + saddle_sore_after(level);
                    // (A starving or exhausted horse: warned, and thrown off before long.)
                    if let Some(mo) = motives.as_deref()
                        && (mo.0[crate::petneeds::HUNGER] < -80.0 || mo.0[crate::petneeds::ENERGY] < -99.0)
                    {
                        let key = if mo.0[crate::petneeds::HUNGER] < -80.0 { "Starving" } else { "Exhausted" };
                        notes.push(format!("{} is {}! {} should get off soon before {} bucks them off!", pet.name, key.to_lowercase(), sim.full_name(), pet.name));
                        m.throw_at = Some(clock.minutes + rng.random_range(DISTRESS_THROW.0..DISTRESS_THROW.1));
                    }
                }
            }
            // (Thrown: the two's clips, the rider ending up on the ground behind it.)
            Stage::Thrown => {
                if done(&mut lib, &hanim) {
                    queue.0.retain(|a| !matches!(a.kind, ActionKind::Ride { .. }));
                    commands.entity(e).remove::<(Mounted, ActionClip)>();
                    commands.entity(m.horse).remove::<Ridden>();
                    hanim.replay(format!("{}_idle_stand_breathe_x", pet.kind));
                    anim.pose = Pose::Stand;
                    if let Some(b) = bonds.as_deref_mut() {
                        b.add(sim.id, true, THROW_LIKING);
                    }
                    if let Some(ml) = moodlets.as_deref_mut() {
                        ml.add(crate::life::MoodletKind::ForciblyDismounted, clock.minutes);
                    }
                    info!("riding: {} threw {}", pet.name, sim.first);
                }
            }
            // (Over a jump: `horsejumps` plays it, and it's over when it's done with it.)
            Stage::Jump => {
                if jumps.get(m.horse).is_ok_and(|(over, asked)| !over && !asked) {
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
                    // (Got off at a hitching post to tie it up: tied there.)
                    if let Some((post, part, _)) = m.tie.take()
                        && let Ok(ptf) = posts.get(post)
                    {
                        let (at, face) = crate::hitching::horse_spot(ptf, part);
                        htf.translation = Vec3::new(at.x, htf.translation.y, at.y);
                        htf.rotation = face;
                        crate::hitching::tie(&mut commands, m.horse, post, part);
                        info!("riding: {} tied {} up", sim.first, pet.name);
                    }
                    anim.pose = Pose::Stand;
                    info!("riding: {} is off {}", sim.first, pet.name);
                }
            }
            Stage::Up => {
                // "Ride Here": the last place asked for, from where the horse stands.
                let mut off = false;
                let mut to: Option<Vec2> = None;
                let mut jump_to: Option<Entity> = None;
                let mut compete: Option<(usize, crate::equestrian::CenterTask)> = None;
                let mut i = 1;
                while i < queue.0.len() {
                    match queue.0[i].kind {
                        ActionKind::GoHere(p, _) if p.is_finite() => {
                            to = Some(p);
                            jump_to = None;
                            queue.0.remove(i);
                        }
                        // ("Jump": ridden at it.)
                        ActionKind::HorseJump { obstacle } => {
                            jump_to = Some(obstacle);
                            to = None;
                            queue.0.remove(i);
                        }
                        // ("Tie Up": ridden to a free place at the post, to get off there.)
                        ActionKind::TieUp { post } => {
                            let taken: Vec<usize> = tied.iter().filter(|t| t.post == post).map(|t| t.part).collect();
                            match (crate::hitching::free_part(&taken), posts.get(post)) {
                                (Some(part), Ok(ptf)) => {
                                    let spot = crate::hitching::horse_spot(ptf, part).0;
                                    m.tie = Some((post, part, spot));
                                    // (Up to it from behind, walked in straight at the post.)
                                    let away = (spot - ptf.translation.xz()).normalize_or(Vec2::Y);
                                    (to, jump_to) = (Some(spot + away * 1.6), None);
                                }
                                _ => notes.push(crate::hitching::POST_FULL.to_string()),
                            }
                            queue.0.remove(i);
                        }
                        ActionKind::Dismount => {
                            off = true;
                            queue.0.remove(i);
                        }
                        // (Off to the Equestrian Center: ridden off to it.)
                        ActionKind::Equestrian { lot, task } => {
                            compete = Some((lot, task));
                            (to, jump_to) = (None, None);
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
                // (Thrown off: a horse in a foul mood as its mood turns, a starving or exhausted
                // one when its patience is out; not over a jump or on the way to one.)
                let distressed = m.throw_at.is_some_and(|t| clock.minutes >= t);
                let foul = clock.minutes >= m.benefit_at && motives.as_deref().is_some_and(|mo| throws(hmood.map_or_else(|| crate::petneeds::mood(mo), |x| x.0), htraits.map_or(&[][..], |t| &t.0[..]), rng.random()));
                let forced = std::env::var_os("RIDE_THROW").is_some();
                if !off && m.jump.is_none() && (distressed || foul || forced) && !(forced && m.throw_at == Some(f64::NEG_INFINITY)) {
                    let teen = sim.age == crate::sim::Age::Teen;
                    m.stage = Stage::Thrown;
                    m.path.clear();
                    m.compete = None;
                    m.throw_at = Some(f64::NEG_INFINITY);
                    play(&mut hanim, &mut m, if teen { ("ah2t_riding_throwRider_x", &["ah2t_riding_throwRider_y"]) } else { ("ah2a_riding_throwRider_x", &["ah2a_riding_throwRider_y"]) });
                    continue;
                }
                // (Saddle sore after so long up, and as long as they stay up.)
                if clock.minutes >= m.sore_at
                    && let Some(ml) = moodlets.as_deref_mut()
                    && !ml.has(crate::life::MoodletKind::SaddleSore)
                {
                    ml.add(crate::life::MoodletKind::SaddleSore, clock.minutes);
                }
                // (At the hitching post's place: off, to tie it up; not getting there, not.)
                if let Some((_, _, spot)) = m.tie
                    && m.path.is_empty()
                    && to.is_none()
                {
                    if htf.translation.xz().distance(spot) < 0.9 {
                        off = true;
                    } else {
                        m.tie = None;
                    }
                }
                if off {
                    let way = way_on(level, rng.random());
                    m.stage = Stage::Off(way);
                    m.path.clear();
                    play(&mut hanim, &mut m, DISMOUNTS[way]);
                    continue;
                }
                let here = htf.translation.xz();
                // (Off to the Equestrian Center: off the lot's edge, then away.)
                if let Some(c) = compete {
                    m.jump = None;
                    m.compete = Some(c);
                    let path = exit.as_deref().and_then(|x| grid.as_deref().and_then(|g| g.nearest_free(x.0).map(|(x, z)| g.center_of(x, z)).and_then(|p| g.find_path(here, p))));
                    m.path = path.unwrap_or_default();
                    let far = m.path.iter().fold((here, 0.0), |(a, d), b| (*b, d + a.distance(*b))).1;
                    m.gait = gait_for(level, far);
                }
                if let (Some(o), Some(g)) = (jump_to, grid.as_deref()) {
                    let tired = hskills.as_ref().is_some_and(|s| s.worn_out);
                    match obstacles.get(o).ok().filter(|_| !tired).and_then(|(obs, otf)| crate::horsejumps::plan(g, here, &obs.world_lanes(otf))) {
                        Some((p, start, face)) => {
                            let far = p.iter().fold((here, 0.0), |(a, d), b| (*b, d + a.distance(*b))).1;
                            m.gait = gait_for(level, far);
                            m.path = p;
                            m.jump = Some((o, start, face));
                        }
                        None if tired => notes.push(format!("{} needs a break from learning new skills!", pet.name)),
                        None => notes.push(format!("{} can't ride {} at that jump.", sim.first, pet.name)),
                    }
                }
                if let (Some(to), Some(g)) = (to, grid.as_deref()) {
                    m.jump = None;
                    m.compete = None;
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
                // (The last steps in to the hitching post's place.)
                if to.is_some()
                    && let Some((_, _, spot)) = m.tie
                    && !m.path.is_empty()
                {
                    m.path.push(spot);
                }
                // Along the way at its gait, in step with its clip's own pace (the run-in at a
                // jump at a gallop).
                if let Some(&next) = m.path.first() {
                    let gait = if m.jump.is_some() && m.path.len() == 1 { 2 } else { m.gait };
                    let (clip, rider) = if gait == 2 { gallop(hskills.as_ref().map_or(0, |s| s.racing_level())) } else { GAITS[gait] };
                    let rider = seat(rider, level);
                    if hanim.clip != clip {
                        play(&mut hanim, &mut m, (clip, rider));
                    }
                    let pace = lib.get(&data, clip).map_or(1.25, |c| crate::pets::clip_pace(&c));
                    let d = next - here;
                    let step = pace * dt;
                    if d.length() <= step.max(0.05) {
                        m.path.remove(0);
                        if m.path.is_empty() {
                            match m.jump.take() {
                                // (At its start, facing over it: over it.)
                                Some((o, start, face)) => {
                                    let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(start.x, htf.translation.y, start.y));
                                    htf.translation = Vec3::new(start.x, y, start.y);
                                    htf.rotation = Quat::from_rotation_y(face.x.atan2(face.y));
                                    commands.entity(m.horse).insert(crate::horsejumps::JumpRequest { obstacle: o, rider: Some(e), kick: false });
                                    m.stage = Stage::Jump;
                                    continue;
                                }
                                None => play(&mut hanim, &mut m, IDLES[0]),
                            }
                            if let Some((lot, task)) = m.compete.take() {
                                commands.entity(e).insert(crate::equestrian::ToCenter { lot, task, horse: m.horse });
                            }
                        }
                    } else {
                        let p = here + d.normalize() * step;
                        let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(p.x, htf.translation.y, p.y));
                        htf.translation = Vec3::new(p.x, y, p.y);
                        htf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
                    }
                } else if let Some((lot, task)) = m.compete.take() {
                    // (Nowhere to ride to: off from where they are.)
                    commands.entity(e).insert(crate::equestrian::ToCenter { lot, task, horse: m.horse });
                } else if done(&mut lib, &hanim) {
                    play(&mut hanim, &mut m, idle(&mut rng));
                }
                // What a ride does them both.
                let minutes = delta.0;
                let mut riding_points = 0.0;
                if minutes > 0.0 {
                    // (At a gallop, the walk styles the game counts as galloping.)
                    let galloping = !m.path.is_empty() && (m.gait == 2 || (m.jump.is_some() && m.path.len() == 1));
                    riding_points = (POINTS_A_MINUTE + if galloping { POINTS_AT_SPEED } else { 0.0 }) * minutes;
                    // (The horse's Racing at a gallop, while it's fresh for learning.)
                    if galloping {
                        match hskills.as_deref_mut() {
                            Some(s) if !s.worn_out => {
                                if let Some(l) = s.race(RACING_AT_SPEED * minutes, minutes) {
                                    notes.push(format!("{} reached level {l} in Racing!", pet.name));
                                }
                            }
                            Some(_) => {}
                            None => {
                                let mut s = crate::horsejumps::HorseSkills::default();
                                s.race(RACING_AT_SPEED * minutes, minutes);
                                commands.entity(m.horse).insert(s);
                            }
                        }
                    }
                }
                if clock.minutes >= m.benefit_at {
                    m.benefit_at = clock.minutes + BENEFIT_MINUTES;
                    if let Some(b) = bonds.as_deref_mut() {
                        b.add(sim.id, true, BENEFIT_LIKING);
                    }
                    if let Some(mo) = motives.as_deref_mut() {
                        mo.add(crate::petneeds::URGE, BENEFIT_EXERCISE);
                        mo.add(crate::petneeds::SOCIAL, BENEFIT_HORSE_SOCIAL);
                    }
                    if let Some(sm) = sim_motives.as_deref_mut() {
                        sm.add(crate::sim::FUN, BENEFIT_RIDER);
                        sm.add(crate::sim::SOCIAL, BENEFIT_RIDER);
                    }
                    riding_points += BENEFIT_RIDING;
                }
                if riding_points > 0.0 {
                    let rate = crate::life::skill_rate(&sim.traits, "Riding") * mood.map_or(1.0, |m| m.skill_rate());
                    if let Some(l) = skills.add_points("Riding", riding_points * rate) {
                        notes.push(format!("{} reached level {l} in Riding!", sim.first));
                        life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: "Riding", level: l }));
                    }
                }
            }
        }
    }
}

/// `RIDE_TEST=<horse>[:<dx>,<dz>[,<seconds>]]` (tests): eight seconds in, the selected Sim rides the
/// household's horse of that name, the camera on it (`RIDE_DIST` away, 6 m); up on it three seconds,
/// ridden that far off (metres east and south); and after so many seconds up, off again.
/// `RIDE_SKILL=<level>`: the rider that good; `HORSE_RACING=<level>` the horse at Racing.
fn ride_test(
    mut commands: Commands,
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
            if let Some(l) = std::env::var("HORSE_RACING").ok().and_then(|l| l.parse::<f32>().ok()) {
                commands.entity(h).insert(crate::horsejumps::HorseSkills { racing: l, ..default() });
            }
            info!("ride test: riding {name}");
            q.push_player(crate::interact::Action::new("Ride", ActionKind::Ride { horse: h, slot: mount_slot(htf), unsaddle: false }, false));
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
fn in_step(
    mut commands: Commands,
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Mounted, Option<&ActionClip>, &mut ClipPlayer)>,
    horses: Query<(&PetAnim, Option<&crate::horsejumps::HorseJumping>)>,
) {
    for (e, m, clip, mut player) in &mut sims {
        // (Over a jump, the rider's half of the horse's jump.)
        let rider = match horses.get(m.horse) {
            Ok((_, Some(j))) if m.stage == Stage::Jump => j.rider_clips(),
            _ => m.rider,
        };
        let want = ActionClip::new(None, rider);
        if clip != Some(&want) {
            commands.entity(e).insert(want);
        }
        let Some((h, jumping)) = horses.get(m.horse).ok().filter(|_| m.in_step()) else { continue };
        let Some(dur) = player.clip.as_ref().map(|c| c.duration).filter(|d| *d > 0.0) else { continue };
        // (Getting on and off, and a jump, play once, held at their end; the rest go round, the
        // gallop in too till the horse is over the jump.)
        let round = m.up() || (m.stage == Stage::Jump && jumping.is_none());
        // (Round in step with the horse's own cycle: a faster gallop's shorter.)
        let cycle = lib.get(&data, &h.clip).map_or(dur, |c| c.duration).max(1e-3);
        player.time = if round { (h.time() % cycle) / cycle * dur } else { h.time().min(dur - 1e-3) };
    }
}

/// The rider where the horse is: up on it in its own frame, getting on and off in the jig beside
/// it.
fn seat_riders(mut sims: Query<(&Mounted, &mut Transform), With<Sim>>, horses: Query<&Transform, (With<Pet>, Without<Sim>)>) {
    for (m, mut tf) in &mut sims {
        let Ok(h) = horses.get(m.horse) else { continue };
        let at = if m.stage == Stage::Thrown {
            Transform::from_translation(h.translation + h.rotation * THROWN_AT).with_rotation(h.rotation)
        } else if m.started && m.seated() {
            *h
        } else {
            jig(h)
        };
        if *tf != at {
            *tf = at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foul_tempered_horses_throw_their_riders() {
        let none: Vec<String> = Vec::new();
        assert!(throws(-40.0, &none, 0.1));
        assert!(!throws(-40.0, &none, 0.3), "one time in five");
        assert!(!throws(-10.0, &none, 0.0), "not in a fair mood");
        let mean = vec!["MeanPet".to_string()];
        assert!(throws(20.0, &mean, 0.35), "an ornery one sooner and likelier");
        assert!(!throws(-90.0, &["GentlePet".to_string()], 0.0), "never a gentle one");
        assert_eq!((saddle_sore_after(0), saddle_sore_after(10), saddle_sore_after(5)), (180.0, 1080.0, 630.0));
    }

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
        // (Untrained: near failing three times in ten.)
        assert_eq!(way_on(0, 0.25), 1);
        assert_eq!(way_on(0, 0.5), 0);
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
