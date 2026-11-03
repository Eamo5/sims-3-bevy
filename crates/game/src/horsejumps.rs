//! Horse jumps, the Pets pack's (`HorseJump` and its kinds: the gate, wall and scary wall that
//! can be knocked down, the oxer and triple bars with their rails, the water jumps, the fire
//! jumps and the cavaletti trainer). A rider up on a horse picks "Jump" ("Practice Jumping" over
//! a cavaletti; greyed on foot, "You must be riding a horse to do this"): the horse is ridden round
//! to a run-up behind the nearer start (the jump's `StartSlots`; all but a cavaletti go both
//! ways, the game's two-way portal), gallops in and goes over with the pack's `HorseJumps` clips,
//! the horse's, the rider's and the obstacle's together, carried over by its clip's
//! `world_offset`. How it goes is the game's roll (`DoJump`): a chance of clearing it clean and of
//! scraping over by the horse's Jumping (the obstacle's `HighSuccessChances` and
//! `LowSuccessChances`), the roll lifted by its rider's Riding (`RideSkillModifier`) and the pair's
//! mood (`MoodModifier`), weighed down for a horse that hates jumping (`HatesJumpingModifier`);
//! else it refuses. A scrape knocks a gate's, wall's or scary wall's bars down (its half,
//! `knockDown`, left lying) or an oxer's or triple bars' rails (`o_jump_down_pose`), till a Sim
//! picks them up ("Pick Up", "Replace All Rails"); a knocked-down jump is ridden straight through
//! (`RideThrough`), and a water jump soaks its rider. Going over gives the horse Jumping
//! (`HighSuccessGain`, `LowSuccessGain`, more the better its rider rides: `JumpingSkillMultiplier`)
//! and the rider Riding (`RiderHighSuccessGain`, `RiderLowSuccessGain`); each jump tires a horse
//! for learning (`PetSkillFatigue`, as its level's `FatigueModifiers` have it; not after a hundred
//! jumps, Tireless Leaper) till it needs a break. Horses go over the household's jumps of their own
//! accord when bored or restless (`Jump`: Fun, `HorseExercise`); one that hates jumping kicks a
//! jump's bars down instead (`KickDown`).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use s3bake::gamedata::HorseJumpTuning;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Skills};
use crate::pets::{Pet, PetAnim, PetBusy};
use crate::petneeds::PetMotives;
use crate::sim::Sim;

pub struct HorseJumpsPlugin;

impl Plugin for HorseJumpsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JumpTunings>()
            .add_systems(Update, (load_tunings, set_up).chain().run_if(in_state(crate::AppState::InGame)))
            .add_systems(
                Update,
                (jump_test, practise, approach, start_jumps, jumping, pick_up, rest).chain().after(crate::riding::ride).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)),
            );
    }
}

/// How far behind its start a horse sets off on its run at a jump (the game's run-up, its
/// `RadialRangeDestination` 4.5 to 6.75 m out).
pub const RUN_UP: f32 = 5.5;
/// A hundred and fifty jumps and a horse never tires of them (Tireless Leaper:
/// `kTirelessLeaperRequirement`).
pub const TIRELESS: u32 = 150;
/// A jump's toll on a horse's freshness for learning (`DoJump`'s `PetSkillFatigue`, once), and
/// its rest from it (back over four hours, as a dog's tricks).
const FATIGUE: f32 = 5.0;
const REST_HOURS: f32 = 4.0;
/// What a jump does a horse's needs (`DoJump`'s `HorseExercise`, once; `Jump`'s Fun).
const EXERCISE: f32 = 50.0;
const FUN: f32 = 5.0;
/// The swipe that sets a jump's bars back up (`a2o_object_genericSwipe_x`).
const SWIPE: &[&str] = &["a2o_object_genericSwipe_x"];

/// What a jump's made of: what its scrape does to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Build {
    Plain,
    /// Bars knocked down (`DownableJump`: the gate, the wall, the scary wall).
    Downable,
    /// Rails knocked down (`MultiStateJump`: the oxer, the triple bars).
    Rails,
    Water,
    Cavaletti,
    /// Fire jumps: a scrape looks clean (`HighSuccessAnimAlways`).
    Fire,
}

/// A jump class's build.
pub fn build_of(class: &str) -> Build {
    match class {
        "GateJump" | "WallJump" | "ScaryJump" => Build::Downable,
        "OxerJump" | "TripleBarJump" => Build::Rails,
        "WaterJump" | "DuckyWaterJump" => Build::Water,
        "Cavaletti" => Build::Cavaletti,
        "FireRingJump" | "FireWallJump" => Build::Fire,
        _ => Build::Plain,
    }
}

/// The class whose tuning a jump class uses (the ducky water jump is a water jump).
pub fn tuning_class(class: &str) -> &str {
    if class == "DuckyWaterJump" { "WaterJump" } else { class }
}

/// The game's jump tunings, by class.
#[derive(Resource, Default)]
pub struct JumpTunings(pub HashMap<String, HorseJumpTuning>);

fn load_tunings(ui: Option<Res<crate::icons::GameUi>>, mut t: ResMut<JumpTunings>) {
    if t.0.is_empty()
        && let Some(ui) = ui
    {
        t.0 = ui.data.horse_jumps.iter().map(|j| (j.class.clone(), j.clone())).collect();
        info!("horse jumps: {} tunings", t.0.len());
    }
}

/// A horse jump as placed: its class, what it's made of, its lanes (a start and an end, in its
/// own frame, the way a horse goes over it), and whether its bars are down.
#[derive(Component, Clone, Debug)]
pub struct Obstacle {
    pub class: String,
    pub build: Build,
    pub lanes: Vec<(Vec2, Vec2)>,
    pub down: bool,
}

impl Obstacle {
    /// Whether a scrape over it knocks it down.
    pub fn downable(&self) -> bool {
        matches!(self.build, Build::Downable | Build::Rails)
    }

    /// Its name for going over it: "Jump", a cavaletti's "Practice Jumping".
    pub fn verb(&self) -> &'static str {
        if self.build == Build::Cavaletti { "Practice Jumping" } else { "Jump" }
    }

    /// The pose it's left in knocked down: a downable jump's bars where its knock-down left them,
    /// rails lying (`o_jump_down_pose`).
    pub fn down_pose(&self) -> &'static str {
        if self.build == Build::Rails { "o_jump_down_pose_obstacle" } else { "ah2o_jump_knockDown_obstacle" }
    }

    /// Its lanes in the world, for a jump standing so: each start and end.
    pub fn world_lanes(&self, tf: &Transform) -> Vec<(Vec2, Vec2)> {
        let w = |p: Vec2| (tf.translation + tf.rotation * Vec3::new(p.x, 0.0, p.y)).xz();
        self.lanes.iter().map(|(a, b)| (w(*a), w(*b))).collect()
    }
}

/// A jump's lanes from its routing slots: the start (`routingSlot_0`) to the end (`routingSlot_1`)
/// and back; a cavaletti's one way each side (0 to 1, 3 to 2).
pub fn lanes(slots: &[(u8, [f32; 3], [f32; 3])], cavaletti: bool) -> Vec<(Vec2, Vec2)> {
    let at = |n: u8| slots.iter().find(|s| s.0 == n).map(|s| Vec2::new(s.1[0], s.1[2]));
    let pairs: &[(u8, u8)] = if cavaletti { &[(0, 1), (3, 2)] } else { &[(0, 1), (1, 0)] };
    pairs.iter().filter_map(|(a, b)| Some((at(*a)?, at(*b)?))).collect()
}

/// Jumps new in the world get their class and lanes (the game's data, once it's in).
fn set_up(
    mut commands: Commands,
    (data, ui): (Option<Res<Baked>>, Option<Res<crate::icons::GameUi>>),
    added: Query<(Entity, &GameObject), Added<GameObject>>,
    objects: Query<&GameObject>,
    mut pending: Local<Vec<Entity>>,
) {
    pending.extend(added.iter().filter(|(_, o)| o.kind == ObjectKind::HorseJump).map(|(e, _)| e));
    let (Some(data), Some(ui)) = (data, ui) else { return };
    for e in pending.drain(..) {
        let Ok(o) = objects.get(e) else { continue };
        let Some(script) = data.0.catalog.iter().find(|c| c.objd == o.objd).map(|c| c.script.as_str()) else { continue };
        let class = script.rsplit('.').next().unwrap_or(script).to_string();
        let build = build_of(&class);
        let slots = ui.data.route_slots.iter().find(|(k, _)| *k == o.objd).map(|(_, s)| s.as_slice()).unwrap_or(&[]);
        let lanes = lanes(slots, build == Build::Cavaletti);
        debug!("horse jump {} ({class}): {} lanes", o.name, lanes.len());
        commands.entity(e).insert(Obstacle { class, build, lanes, down: false });
    }
}

/// A horse's own skills: its Jumping and Racing (each the level, the way to the next its
/// fraction), the jumps it's tried and cleared, and how fresh it is for learning (100 rested,
/// -100 worn out).
#[derive(Component, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HorseSkills {
    pub jumping: f32,
    pub jumps: u32,
    pub clean: u32,
    pub fresh: f32,
    pub worn_out: bool,
    #[serde(default)]
    pub racing: f32,
}

impl Default for HorseSkills {
    fn default() -> Self {
        Self { jumping: 0.0, jumps: 0, clean: 0, fresh: 100.0, worn_out: false, racing: 0.0 }
    }
}

/// How much a gallop's learning tires a horse, by its Racing (`kFatigueDecayPerLevel`: all of it
/// untrained, a fifth at 10), and how fast (fresh to worn out over four hours, as a dog's tricks).
pub fn racing_fatigue(level: u32) -> f32 {
    1.0 - 0.8 * level.min(10) as f32 / 10.0
}
const FATIGUE_HOURS: f32 = 4.0;

impl HorseSkills {
    pub fn level(&self) -> u32 {
        (self.jumping as u32).min(10)
    }

    /// Jumping points; the level reached if it's a new one.
    pub fn add_points(&mut self, points: f32) -> Option<u32> {
        let table = crate::interact::skill_points("Jumping");
        let before = self.level();
        self.jumping = crate::interact::points_to_level(table, crate::interact::level_to_points(table, self.jumping) + points.max(0.0));
        (self.level() > before).then_some(self.level())
    }

    pub fn racing_level(&self) -> u32 {
        (self.racing as u32).min(10)
    }

    /// Racing points for galloping so many minutes, tiring it as it learns; the level reached if
    /// it's a new one.
    pub fn race(&mut self, points: f32, minutes: f32) -> Option<u32> {
        let table = crate::interact::skill_points("Racing");
        let before = self.racing_level();
        self.racing = crate::interact::points_to_level(table, crate::interact::level_to_points(table, self.racing) + points.max(0.0));
        self.fresh = (self.fresh - 200.0 / FATIGUE_HOURS * racing_fatigue(before) * minutes / 60.0).max(-100.0);
        if self.fresh <= -98.0 {
            self.worn_out = true;
        }
        (self.racing_level() > before).then_some(self.racing_level())
    }

    /// Racing points as they come (a competition's), the level reached if it's a new one.
    pub fn add_racing(&mut self, points: f32) -> Option<u32> {
        let table = crate::interact::skill_points("Racing");
        let before = self.racing_level();
        self.racing = crate::interact::points_to_level(table, crate::interact::level_to_points(table, self.racing) + points.max(0.0));
        (self.racing_level() > before).then_some(self.racing_level())
    }

    /// Never tired by jumping (Tireless Leaper).
    pub fn tireless(&self) -> bool {
        self.jumps >= TIRELESS
    }

    /// A jump's toll at its level's fatigue (none for a tireless leaper).
    pub fn tire(&mut self, modifier: f32) {
        if !self.tireless() {
            self.fresh = (self.fresh - FATIGUE * modifier).max(-100.0);
            if self.fresh <= -98.0 {
                self.worn_out = true;
            }
        }
    }
}

/// A horse's skills for its tooltip.
pub fn summary(s: &HorseSkills) -> String {
    let mut out = String::new();
    if s.racing > 0.0 {
        out += &format!("\nRacing level {}", s.racing_level());
    }
    if s.jumping > 0.0 || s.jumps > 0 {
        out += &format!("\nJumping level {}", s.level());
    }
    if s.worn_out && !out.is_empty() {
        out += " (needs a break)";
    }
    out
}

/// How a jump goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// Over it clean (`HighSuccess`).
    Clean,
    /// Scraped over (`LowSuccess`): bars knocked down, a rider soaked.
    Scrape,
    /// Refused (`Fail`).
    Refuse,
    /// Knocked down already: ridden straight through (`RideThrough`).
    Through,
    /// A horse that hates jumping kicking its bars down (`KickDown`).
    Kick,
}

/// The game's roll (`DoJump.Run`): the roll, plus a hater's weight, less its rider's Riding lift
/// and the pair's mood, under the horse's level's chance of a clean jump, else of a scrape.
pub fn roll(t: &HorseJumpTuning, level: u32, rider_level: Option<u32>, mood: f32, hates: bool, r: f32) -> Outcome {
    let at = |v: &[f32], i: u32| v.get((i as usize).min(v.len().saturating_sub(1))).copied().unwrap_or(0.0);
    let mut x = r;
    if hates {
        x += t.hates_jumping;
    }
    x -= at(&t.ride_skill, rider_level.unwrap_or(0));
    x -= mood * t.mood;
    let high = at(&t.high, level);
    if x < high {
        Outcome::Clean
    } else if x < high + at(&t.low, level) {
        Outcome::Scrape
    } else {
        Outcome::Refuse
    }
}

/// What a jump's worth: the horse's Jumping points (more the better its rider rides, the game's
/// `JumpingSkillMultiplier`, 1 untrained to 1.75 at Riding 10) and the rider's Riding points.
pub fn gains(t: &HorseJumpTuning, outcome: Outcome, rider_level: Option<u32>) -> (f32, f32) {
    let mult = rider_level.map_or(1.0, |l| 1.0 + 0.75 * l.min(10) as f32 / 10.0);
    let rider = |g: f32| if rider_level.is_some() { g } else { 0.0 };
    match outcome {
        Outcome::Clean => (t.high_gain * mult, rider(t.rider_high_gain)),
        Outcome::Scrape => (t.low_gain * mult, rider(t.rider_low_gain)),
        _ => (0.0, 0.0),
    }
}

/// The clips for a jump: the horse's, its rider's, the obstacle's half.
pub fn clips(outcome: Outcome, build: Build, scrape_looks_clean: bool) -> (&'static str, &'static [&'static str], Option<&'static str>) {
    const CLEAN: (&str, &[&str], Option<&str>) = ("ah2o_jump_highSuccess_x", &["a2o_jump_highSuccess_rider_y"], None);
    match (outcome, build) {
        (Outcome::Clean | Outcome::Scrape, Build::Cavaletti) => ("ah2o_jump_cavaletti_x", &["a2o_jump_cavaletti_rider_y"], None),
        (Outcome::Clean, _) => CLEAN,
        (Outcome::Scrape, _) if scrape_looks_clean => CLEAN,
        (Outcome::Scrape, Build::Water) => ("ah2o_jump_lowSuccess_waterObstacle_x", &["a2o_jump_lowSuccess_waterObstacle_rider_y"], Some("ah2o_jump_lowSuccess_waterObstacle_obstacle")),
        (Outcome::Scrape, Build::Downable | Build::Rails) => ("ah2o_jump_lowSuccess_x", &["a2o_jump_lowSuccess_rider_y"], Some("ah2o_jump_knockDown_obstacle")),
        (Outcome::Scrape, _) => ("ah2o_jump_lowSuccess_x", &["a2o_jump_lowSuccess_rider_y"], None),
        (Outcome::Refuse, _) => ("ah2o_jump_fail_x", &["a2o_jump_fail_rider_y"], None),
        (Outcome::Through, _) => ("ah2o_jump_downObstacle_x", &["a2o_jump_downObstacle_rider_y"], None),
        (Outcome::Kick, _) => ("ah2o_trait_hatesJumping_kickObject_x", &[], Some("ah2o_trait_hatesJumping_kickObject_obstacle")),
    }
}

/// A way at a jump from where a horse stands: its run-up behind the start (the way there, on the
/// lot's walk grid), the start and the way it faces there. The nearer way, of the lanes there's a
/// way to.
pub fn plan(grid: &crate::nav::NavGrid, here: Vec2, lanes: &[(Vec2, Vec2)]) -> Option<(Vec<Vec2>, Vec2, Vec2)> {
    lanes
        .iter()
        .filter_map(|(start, end)| {
            let face = (*end - *start).normalize_or(Vec2::Y);
            let run_up = *start - face * RUN_UP;
            // (Somewhere to land.)
            if grid.cell_of(*end).is_none_or(|(x, z)| grid.is_blocked(x, z)) {
                return None;
            }
            let mut path = if here.distance(run_up) < 0.5 { Vec::new() } else { grid.find_path(here, run_up)? };
            path.push(run_up);
            let far = path.iter().fold((here, 0.0), |(a, d), b| (*b, d + a.distance(*b))).1;
            Some((far, path, *start, face))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, mut path, start, face)| {
            path.push(start);
            (path, start, face)
        })
}

/// A horse at a jump's start, to go over it (ridden: `riding`'s; or of its own accord).
#[derive(Component)]
pub struct JumpRequest {
    pub obstacle: Entity,
    pub rider: Option<Entity>,
    /// Kicking its bars down instead.
    pub kick: bool,
}

/// A horse going over a jump: the obstacle, how it's going, its clips, where it set off from, and
/// its rider.
#[derive(Component)]
pub struct HorseJumping {
    pub obstacle: Entity,
    pub outcome: Outcome,
    horse: &'static str,
    rider_clips: &'static [&'static str],
    half: Option<&'static str>,
    base: Option<(Vec3, Quat)>,
    pub rider: Option<Entity>,
}

impl HorseJumping {
    /// The rider's half of it.
    pub fn rider_clips(&self) -> &'static [&'static str] {
        self.rider_clips
    }
}

/// A horse at a jump or on its way to one, of its own accord: free of all else meanwhile.
#[derive(Component)]
pub struct AtJump;

/// A horse on its way to a jump of its own accord: the way, the start and the way it faces.
#[derive(Component)]
pub struct ToJump {
    obstacle: Entity,
    path: Vec<Vec2>,
    start: Vec2,
    face: Vec2,
    kick: bool,
}

/// How bored or restless a horse goes over a jump of its own accord (fun or exercise this low),
/// and how often it thinks of it.
const BORED: f32 = 0.0;
const AGILE_BORED: f32 = 30.0;
const THINK_MINUTES: f64 = 30.0;

/// The household's horses, bored or restless, set off for a jump of their own accord (a horse that
/// hates jumping to kick its bars down, if it can).
#[allow(clippy::type_complexity)]
fn practise(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    grid: Option<Res<crate::nav::NavGrid>>,
    horses: Query<
        (Entity, &Pet, &Transform, &PetMotives, Option<&crate::pets::PetTraits>, Option<&HorseSkills>),
        (With<crate::pets::HomePet>, Without<crate::petneeds::PetTask>, Without<PetBusy>, Without<crate::riding::Ridden>, Without<AtJump>),
    >,
    obstacles: Query<(Entity, &Obstacle, &Transform), (Without<crate::visit::LotObject>, Without<Pet>)>,
    taken: Query<&ToJump>,
    mut next: Local<f64>,
) {
    let Some(grid) = grid else { return };
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + THINK_MINUTES;
    let mut rng = rand::rng();
    let busy: Vec<Entity> = taken.iter().map(|t| t.obstacle).collect();
    for (e, pet, tf, m, traits, skills) in &horses {
        if pet.kind != "ah" || skills.is_some_and(|s| s.worn_out) {
            continue;
        }
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        let hates = crate::pettraits::has(traits, "HatesJumpingPet");
        let bored_at = if crate::pettraits::has(traits, "AgilePet") { AGILE_BORED } else { BORED };
        if (m.0[crate::petneeds::FUN] > bored_at && m.0[crate::petneeds::URGE] > bored_at) || !rng.random_bool(0.5) {
            continue;
        }
        let here = tf.translation.xz();
        let mut best: Option<(f32, Entity, Vec<Vec2>, Vec2, Vec2, bool)> = None;
        for (o, obs, otf) in &obstacles {
            if busy.contains(&o) {
                continue;
            }
            // (A hater to the kicking spot beside the bars, `routingSlot_2`, facing along it.)
            let found = if hates {
                if !obs.downable() || obs.down {
                    continue;
                }
                let at = (otf.translation + otf.rotation * Vec3::new(0.5, 0.0, -2.272)).xz();
                let face = (otf.rotation * Vec3::Z).xz();
                grid.find_path(here, at).map(|mut p| {
                    p.push(at);
                    (p, at, face, true)
                })
            } else {
                plan(&grid, here, &obs.world_lanes(otf)).map(|(p, s, f)| (p, s, f, false))
            };
            let Some((path, start, face, kick)) = found else { continue };
            let d = here.distance(start);
            if best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, o, path, start, face, kick));
            }
        }
        let Some((_, obstacle, path, start, face, kick)) = best else { continue };
        info!("horse jumps: {} sets off to {} a jump", pet.name, if kick { "kick down" } else { "go over" });
        commands.entity(e).insert((ToJump { obstacle, path, start, face, kick }, PetBusy, AtJump)).remove::<crate::petneeds::PetTask>();
    }
}

/// Horses on their way to a jump of their own accord: at a canter, galloping the run-in; there,
/// over it.
fn approach(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    (world, building): (Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>),
    mut horses: Query<(Entity, &mut ToJump, &mut Transform, &mut PetAnim), With<Pet>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    for (e, mut tj, mut tf, mut anim) in &mut horses {
        let Some(&next) = tj.path.first() else {
            tf.rotation = Quat::from_rotation_y(tj.face.x.atan2(tj.face.y));
            let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(tj.start.x, tf.translation.y, tj.start.y));
            tf.translation = Vec3::new(tj.start.x, y, tj.start.y);
            commands.entity(e).remove::<ToJump>().insert(JumpRequest { obstacle: tj.obstacle, rider: None, kick: tj.kick });
            continue;
        };
        // (The run-in at a gallop; the rest at a canter, a hater at a walk.)
        let clip = if tj.kick { "ah_walk_x" } else if tj.path.len() == 1 { "ah_run_x" } else { "ah_canter_x" };
        anim.play(clip.to_string());
        let pace = lib.get(&data, clip).map_or(3.0, |c| crate::pets::clip_pace(&c));
        let here = tf.translation.xz();
        let d = next - here;
        let step = pace * dt;
        if d.length() <= step.max(0.05) {
            tj.path.remove(0);
        } else {
            let p = here + d.normalize() * step;
            let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(p.x, tf.translation.y, p.y));
            tf.translation = Vec3::new(p.x, y, p.y);
            tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
        }
    }
}

/// Horses at a jump's start go over it: the game's roll for how it goes, by the horse's Jumping,
/// its rider's Riding and the two's mood.
#[allow(clippy::type_complexity)]
fn start_jumps(
    mut commands: Commands,
    tunings: Res<JumpTunings>,
    horses: Query<(Entity, &Pet, &JumpRequest, Option<&HorseSkills>, Option<&PetMotives>, Option<&crate::pets::PetTraits>)>,
    obstacles: Query<&Obstacle>,
    riders: Query<(&Skills, Option<&crate::life::Mood>)>,
) {
    let mut rng = rand::rng();
    for (e, pet, req, skills, motives, traits) in &horses {
        commands.entity(e).remove::<JumpRequest>();
        let Ok(obs) = obstacles.get(req.obstacle) else { continue };
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        let level = skills.map_or(0, |s| s.level());
        let rider = req.rider.and_then(|r| riders.get(r).ok());
        let rider_level = rider.map(|(s, _)| s.level("Riding"));
        // (The pair's mood: the horse's, and its rider's, halved between them.)
        let mut mood = motives.map_or(0.0, crate::petneeds::mood);
        if let Some((_, m)) = rider {
            mood = (mood + m.map_or(0.0, |m| m.0)) / 2.0;
        }
        let tuning = tunings.0.get(tuning_class(&obs.class));
        // (JUMP_FORCE=scrape|refuse|clean, tests: that, unless it's down.)
        let forced = std::env::var("JUMP_FORCE").ok().and_then(|f| match f.as_str() {
            "scrape" => Some(Outcome::Scrape),
            "refuse" => Some(Outcome::Refuse),
            "clean" => Some(Outcome::Clean),
            _ => None,
        });
        let outcome = if req.kick {
            Outcome::Kick
        } else if obs.down {
            Outcome::Through
        } else if let Some(f) = forced {
            f
        } else {
            tuning.map_or(Outcome::Clean, |t| roll(t, level, rider_level, mood, crate::pettraits::has(traits, "HatesJumpingPet"), rng.random()))
        };
        let (horse, rider_clips, half) = clips(outcome, obs.build, tuning.is_some_and(|t| t.high_anim_always));
        info!("horse jumps: {} at the {} (Jumping {level}, rider's Riding {rider_level:?}, mood {mood:.0}): {outcome:?}", pet.name, obs.class);
        commands.entity(e).insert((HorseJumping { obstacle: req.obstacle, outcome, horse, rider_clips, half, base: None, rider: req.rider }, PetBusy, AtJump));
    }
}

/// Horses going over: moved by their clip from where they set off, the obstacle playing its half;
/// over, what it's worth to the horse and its rider, the bars left down after a scrape.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn jumping(
    mut commands: Commands,
    (data, mut lib, tunings, clock): (Res<Baked>, ResMut<ClipLibrary>, Res<JumpTunings>, Res<crate::clock::GameClock>),
    (world, building): (Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>),
    mut horses: Query<(Entity, &Pet, &mut HorseJumping, &mut Transform, &mut PetAnim, Option<&HorseSkills>, Option<&mut PetMotives>), Without<Obstacle>>,
    mut obstacles: Query<(&mut Obstacle, &Transform), Without<Pet>>,
    (rigs, mut bones): (Query<&crate::objanim::ObjectRig>, Query<&mut Transform, (Without<Pet>, Without<Obstacle>)>),
    mut riders: Query<(&Sim, &mut Skills, Option<&mut crate::life::Moodlets>)>,
    (mut notes, mut life): (ResMut<crate::interact::Notifications>, MessageWriter<crate::life::LifeEvent>),
) {
    let mut rng = rand::rng();
    for (e, pet, mut hj, mut tf, mut anim, skills, motives) in &mut horses {
        let Some(clip) = lib.get(&data, hj.horse) else {
            commands.entity(e).remove::<(HorseJumping, AtJump)>();
            continue;
        };
        let (p, r) = match hj.base {
            Some(b) => b,
            None => {
                anim.replay(hj.horse.to_string());
                hj.base = Some((tf.translation, tf.rotation));
                (tf.translation, tf.rotation)
            }
        };
        let t = anim.time().clamp(0.0, clip.duration);

        if let Some((d, turn)) = crate::pets::root_motion(&clip, t) {
            let to = p + r * d;
            let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, to);
            tf.translation = Vec3::new(to.x, y, to.z);
            tf.rotation = r * turn;
        }
        // (The obstacle's half, as far in.)
        if let (Some(h), Ok(rig)) = (hj.half, rigs.get(hj.obstacle))
            && let Some(hc) = lib.get(&data, h)
        {
            commands.entity(hj.obstacle).remove::<crate::objanim::HeldPose>();
            for (b, pose) in rig.clip_pose(&hc, t.min(hc.duration), 0) {
                if let Ok(mut btf) = bones.get_mut(b) {
                    *btf = pose;
                }
            }
        }
        if anim.time() < clip.duration {
            continue;
        }
        // Over: what it was worth.
        let Ok((mut obs, _)) = obstacles.get_mut(hj.obstacle) else {
            commands.entity(e).remove::<(HorseJumping, AtJump)>();
            continue;
        };
        let tuning = tunings.0.get(tuning_class(&obs.class));
        let rider_level = hj.rider.and_then(|r| riders.get(r).ok()).map(|(_, s, _)| s.level("Riding"));
        let mut s = skills.cloned().unwrap_or_default();
        if let Some(t) = tuning
            && hj.outcome != Outcome::Kick
        {
            let (horse_points, rider_points) = gains(t, hj.outcome, rider_level);
            if let Some(l) = s.add_points(horse_points) {
                notes.push(format!("{} reached level {l} in Jumping!", pet.name));
            }
            if hj.outcome != Outcome::Through {
                let at = (s.level() as usize).min(t.fatigue.len().saturating_sub(1));
                s.tire(t.fatigue.get(at).copied().unwrap_or(1.0));
            }
            if matches!(hj.outcome, Outcome::Clean | Outcome::Scrape | Outcome::Refuse) && obs.build != Build::Cavaletti {
                s.jumps += 1;
                s.clean += u32::from(hj.outcome == Outcome::Clean);
            }
            if let Some(r) = hj.rider
                && let Ok((sim, mut rs, moodlets)) = riders.get_mut(r)
            {
                let rate = crate::life::skill_rate(&sim.traits, "Riding");
                if let Some(l) = rs.add_points("Riding", rider_points * rate) {
                    notes.push(format!("{} reached level {l} in Riding!", sim.first));
                    life.write(crate::life::LifeEvent::new(r, crate::life::LifeEventKind::SkillUp { skill: "Riding", level: l }));
                }
                // (Soaked by a water jump scraped over.)
                if hj.outcome == Outcome::Scrape
                    && obs.build == Build::Water
                    && let Some(mut m) = moodlets
                {
                    m.add(crate::life::MoodletKind::Soaked, clock.minutes);
                }
            }
        }
        commands.entity(e).insert(s);
        if let Some(mut m) = motives {
            m.add(crate::petneeds::URGE, EXERCISE);
            m.add(crate::petneeds::FUN, FUN);
        }
        // (Knocked down: left lying till it's picked up.)
        if matches!(hj.outcome, Outcome::Scrape | Outcome::Kick) && obs.downable() {
            obs.down = true;
            let pose = if hj.outcome == Outcome::Kick && obs.build == Build::Downable { "ah2o_trait_hatesJumping_kickObject_obstacle" } else { obs.down_pose() };
            commands.entity(hj.obstacle).insert(crate::objanim::HeldPose(pose));
        }
        commands.entity(e).remove::<(HorseJumping, AtJump)>();
        if hj.rider.is_none() {
            commands.entity(e).remove::<PetBusy>();
            anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
        }
    }
}

/// A Sim setting a knocked-down jump's bars back up ("Pick Up"; an oxer's or triple bars' "Replace
/// All Rails"): before it, a swipe of the hand.
#[derive(Component)]
pub struct Swiping(f32);

#[allow(clippy::type_complexity)]
fn pick_up(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &mut ActionQueue, Option<&mut Swiping>), With<Sim>>,
    mut obstacles: Query<&mut Obstacle>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    for (e, mut queue, swiping) in &mut sims {
        let running = queue.0.front().and_then(|a| match a.kind {
            ActionKind::PickUpJump { obstacle, .. } if matches!(a.phase, Phase::Running(_)) && !a.cancel => Some(obstacle),
            _ => None,
        });
        match (running, swiping) {
            (Some(_), None) => {
                commands.entity(e).insert((Swiping(0.0), ActionClip::new(None, SWIPE)));
            }
            (Some(o), Some(mut s)) => {
                s.0 += dt;
                let len = lib.get(&data, SWIPE[0]).map_or(1.5, |c| c.duration);
                if s.0 >= len {
                    if let Ok(mut obs) = obstacles.get_mut(o) {
                        obs.down = false;
                    }
                    commands.entity(o).remove::<crate::objanim::HeldPose>();
                    queue.0.pop_front();
                    commands.entity(e).remove::<(Swiping, ActionClip)>();
                }
            }
            (None, Some(_)) => {
                commands.entity(e).remove::<(Swiping, ActionClip)>();
            }
            (None, None) => {}
        }
    }
}

/// Horses rest from jumping: fresh again over four hours.
fn rest(delta: Res<crate::clock::SimDelta>, mut horses: Query<&mut HorseSkills, Without<HorseJumping>>) {
    let hours = delta.0 / 60.0;
    if hours <= 0.0 {
        return;
    }
    for mut s in &mut horses {
        if s.fresh < 100.0 {
            s.fresh = (s.fresh + 200.0 / REST_HOURS * hours).min(100.0);
            if s.worn_out && s.fresh >= 98.0 {
                s.worn_out = false;
            }
        }
    }
}

/// A jump's menu for a Sim: "Jump" (a cavaletti's "Practice Jumping") up on a horse, greyed on
/// foot or with the horse in need of a break; a knocked-down jump's "Pick Up" ("Replace All
/// Rails") for a teen or older on foot.
pub fn options(target: Entity, obs: &Obstacle, otf: &Transform, obj: &GameObject, sim: &Sim, riding: Option<(&Pet, Option<&HorseSkills>)>) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    match riding {
        Some((horse, skills)) if skills.is_some_and(|s| s.worn_out) => {
            out.push((obs.verb().to_string(), ActionKind::Greyed(format!("{} needs a break from learning new skills!", horse.name))));
        }
        Some(_) => out.push((obs.verb().to_string(), ActionKind::HorseJump { obstacle: target })),
        None => out.push((obs.verb().to_string(), ActionKind::Greyed("You must be riding a horse to do this".into()))),
    }
    if obs.down && riding.is_none() && crate::riding::can_ride(sim) {
        // (Behind the bars, clear of where they fell: they're knocked over the way it's ridden from
        // its first start, `routingSlot_0`.)
        let at = otf.transform_point(Vec3::new(obj.center.x, 0.0, obj.center.y - obj.half.y - 0.8)).xz();
        let face = (otf.rotation * Vec3::Z).xz().normalize_or(Vec2::Y);
        let label = if obs.build == Build::Rails { "Replace All Rails" } else { "Pick Up" };
        out.push((label.to_string(), ActionKind::PickUpJump { obstacle: target, at, face }));
    }
    out
}

/// `JUMP_TEST=<horse>[:<jumps>]` (tests): eight seconds in, a jump (`JUMP_OBSTACLE=<catalogue name>`,
/// the gate jump; `JUMP_AT=<dx>,<dz>` off the horse, `JUMP_YAW` turned) put down by the household's
/// horse of that name, which the selected Sim rides over it so many times (3), the camera on the
/// horse (`JUMP_DIST` away, 14 m; `JUMP_FIXED=<yaw>` on the jump). `JUMP_SKILL=<level>`: the horse that good; `RIDE_SKILL=<level>`
/// the rider; `JUMP_AUTO=1`: no rider, the horse bored, to go over it of its own accord.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn jump_test(
    mut commands: Commands,
    time: Res<Time>,
    mut stage: Local<(u8, u32, f32, Option<Entity>)>,
    mut focus: Local<Option<(Vec3, f32)>>,
    mut sel: Query<(&mut ActionQueue, Option<&crate::riding::Mounted>, &mut Skills, &Sim), (With<crate::sim::Selected>, Without<Pet>)>,
    mut horses: Query<(Entity, &Pet, &Transform, &mut PetMotives, Has<HorseJumping>), (With<crate::pets::HomePet>, Without<Obstacle>)>,
    obstacles: Query<(&Obstacle, &Transform, &GameObject), (Without<Pet>, Without<Sim>)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Ok(v) = std::env::var("JUMP_TEST") else { return };
    let (name, rest) = v.split_once(':').unwrap_or((v.as_str(), ""));
    let count: u32 = rest.parse().unwrap_or(3);
    let now = time.elapsed_secs();
    let auto = std::env::var_os("JUMP_AUTO").is_some();
    let Some((h, _, htf, mut m, jumping)) = horses.iter_mut().find(|(_, p, ..)| p.name.eq_ignore_ascii_case(name)) else { return };
    // (JUMP_FIXED=<yaw>: on the jump instead, from that way round its side.)
    if let Ok(mut c) = cams.single_mut() {
        let side = std::env::var("JUMP_FIXED").ok().and_then(|y| y.parse::<f32>().ok());
        match (side, *focus) {
            (Some(y), Some((at, yaw))) => {
                c.look_at(at);
                c.yaw = yaw + y;
                c.pitch = 0.35;
            }
            _ => c.look_at(htf.translation),
        }
        c.distance = std::env::var("JUMP_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(14.0);
    }
    let Ok((mut q, mounted, mut skills, sim)) = sel.single_mut() else { return };
    match stage.0 {
        0 if now > 8.0 => {
            let num = |k: &str| std::env::var(k).ok().map(|v| v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>()).unwrap_or_default();
            let off = num("JUMP_AT");
            let off = if off.len() == 2 { Vec3::new(off[0], 0.0, off[1]) } else { Vec3::new(0.0, 0.0, 10.0) };
            let yaw = num("JUMP_YAW").first().copied().unwrap_or(0.0);
            let what = std::env::var("JUMP_OBSTACLE").unwrap_or_else(|_| "horsesJumpObstacleGateClassic".into());
            if let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&what)).map(|c| c.objd) {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                let at = htf.translation + off;
                stage.3 = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, at, yaw).map(|s| s.entity);
                *focus = Some((at, yaw));
                info!("jump test: {what} put down at {at}");
            }
            // (JUMP_HATES=1: a horse that hates jumping.)
            if std::env::var_os("JUMP_HATES").is_some() {
                commands.entity(h).insert(crate::pets::PetTraits(vec!["HatesJumpingPet".to_string()]));
            }
            if let Some(l) = std::env::var("JUMP_SKILL").ok().and_then(|l| l.parse::<f32>().ok()) {
                commands.entity(h).insert(HorseSkills { jumping: l, ..default() });
            }
            if let Some(l) = std::env::var("RIDE_SKILL").ok().and_then(|l| l.parse::<f32>().ok()) {
                skills.0.insert("Riding", l);
            }
            if auto {
                m.0[crate::petneeds::FUN] = -60.0;
            } else if !std::env::var("JUMP_PIE").is_ok_and(|p| p == "foot") {
                q.push_player(crate::interact::Action::new("Ride", ActionKind::Ride { horse: h, slot: crate::riding::mount_slot(htf), unsaddle: false }, false));
            }
            *stage = (1, 0, now, stage.3);
        }
        // (JUMP_PIE=riding|foot: the jump's menu opened instead, up on the horse or before getting
        // on; the pointer's to be put somewhere with CURSOR_AT.)
        1 if let Ok(pie) = std::env::var("JUMP_PIE")
            && now - stage.2 > 3.0
            && (pie == "foot" || mounted.is_some_and(|m| m.idle())) =>
        {
            if let Some(o) = stage.3 {
                info!("jump test: the jump's menu");
                commands.insert_resource(crate::hud::PieFor(o));
            }
            *stage = (9, 0, now, stage.3);
        }
        // (Up, settled and not over a jump: at it again, so many times.)
        1 if !auto && stage.1 < count && now - stage.2 > 3.0 && mounted.is_some_and(|m| m.idle()) && !jumping && q.0.len() == 1 => {
            let Some(o) = stage.3 else { return };
            info!("jump test: jump {}", stage.1 + 1);
            q.push_player(crate::interact::Action::new("Jump", ActionKind::HorseJump { obstacle: o }, false));
            *stage = (1, stage.1 + 1, now, stage.3);
        }
        // (JUMP_PICKUP=1: done, off, and its bars picked up if they're down.)
        1 if !auto && stage.1 >= count && std::env::var_os("JUMP_PICKUP").is_some() && now - stage.2 > 3.0 && mounted.is_some_and(|m| m.idle()) && !jumping => {
            q.push_player(crate::interact::Action::new("Dismount", ActionKind::Dismount, false));
            *stage = (2, stage.1, now, stage.3);
        }
        2 if mounted.is_none() && now - stage.2 > 2.0 => {
            if let Some((obs, otf, o)) = stage.3.and_then(|e| obstacles.get(e).ok())
                && let Some((label, kind)) = options(stage.3.unwrap(), obs, otf, o, sim, None).into_iter().find(|(_, k)| matches!(k, ActionKind::PickUpJump { .. }))
            {
                info!("jump test: {label}");
                q.push_player(crate::interact::Action::new(label, kind, false));
            }
            *stage = (3, stage.1, now, stage.3);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> HorseJumpTuning {
        HorseJumpTuning {
            class: "GateJump".into(),
            high: vec![0.0, 0.3, 0.4, 0.6, 0.8, 0.9, 0.9, 0.9, 0.9, 0.9, 0.9],
            low: vec![0.0, 0.2, 0.2, 0.1, 0.05, 0.05, 0.05, 0.05, 0.05, 0.05, 0.05],
            ride_skill: vec![0.0, 0.0, 0.05, 0.05, 0.1, 0.1, 0.15, 0.15, 0.2, 0.2, 0.25],
            mood: 0.002,
            hates_jumping: 0.2,
            high_gain: 75.0,
            low_gain: 75.0,
            rider_high_gain: 20.0,
            rider_low_gain: 20.0,
            fatigue: vec![1.0; 11],
            high_anim_always: false,
            fire_on_low: false,
        }
    }

    #[test]
    fn a_jump_goes_as_the_games_roll_has_it() {
        let t = gate();
        // (An untrained horse never clears the gate; at level 1, three times in ten.)
        assert_eq!(roll(&t, 0, None, 0.0, false, 0.0), Outcome::Refuse);
        assert_eq!(roll(&t, 1, None, 0.0, false, 0.29), Outcome::Clean);
        assert_eq!(roll(&t, 1, None, 0.0, false, 0.45), Outcome::Scrape);
        assert_eq!(roll(&t, 1, None, 0.0, false, 0.55), Outcome::Refuse);
        // (A skilled rider and a good mood lift it; hating jumping weighs it down.)
        assert_eq!(roll(&t, 1, Some(10), 0.0, false, 0.54), Outcome::Clean);
        assert_eq!(roll(&t, 1, None, 100.0, false, 0.55), Outcome::Scrape);
        assert_eq!(roll(&t, 1, None, 0.0, true, 0.29), Outcome::Scrape);
    }

    #[test]
    fn what_a_jump_is_worth() {
        let t = gate();
        assert_eq!(gains(&t, Outcome::Clean, None), (75.0, 0.0));
        let (h, r) = gains(&t, Outcome::Scrape, Some(10));
        assert!((h - 131.25).abs() < 1e-3 && r == 20.0, "a Riding 10 rider: 1.75 times");
        assert_eq!(gains(&t, Outcome::Refuse, Some(5)), (0.0, 0.0));
        let mut s = HorseSkills::default();
        s.tire(1.0);
        assert_eq!(s.fresh, 95.0);
        s.jumps = TIRELESS;
        s.tire(1.0);
        assert_eq!(s.fresh, 95.0, "a tireless leaper");
    }

    #[test]
    fn galloping_teaches_racing_and_tires() {
        let mut s = HorseSkills::default();
        // (An hour's gallop: tired by a quarter of the way to worn out, untrained.)
        s.race(20.0 * 60.0, 60.0);
        assert!(s.racing > 0.5 && s.racing < 2.0, "{}", s.racing);
        assert!((s.fresh - 50.0).abs() < 1e-3, "{}", s.fresh);
        assert!((racing_fatigue(10) - 0.2).abs() < 1e-6);
        assert_eq!(crate::riding::gallop(4).0, "ah_run_speed2_x");
        assert_eq!(crate::riding::gallop(10).1, &["a_run_speed3_rider_y"]);
        assert_eq!(crate::riding::gallop(3).0, "ah_run_x");
    }

    #[test]
    fn each_kind_of_jump_has_its_clips() {
        assert_eq!(clips(Outcome::Clean, Build::Cavaletti, false).0, "ah2o_jump_cavaletti_x");
        assert_eq!(clips(Outcome::Scrape, Build::Downable, false).2, Some("ah2o_jump_knockDown_obstacle"));
        assert_eq!(clips(Outcome::Scrape, Build::Water, false).0, "ah2o_jump_lowSuccess_waterObstacle_x");
        assert_eq!(clips(Outcome::Scrape, Build::Fire, true).0, "ah2o_jump_highSuccess_x", "a fire jump's scrape looks clean");
        assert_eq!(clips(Outcome::Through, Build::Downable, false).1, &["a2o_jump_downObstacle_rider_y"]);
        assert_eq!((build_of("ScaryJump"), build_of("TripleBarJump"), tuning_class("DuckyWaterJump")), (Build::Downable, Build::Rails, "WaterJump"));
    }

    #[test]
    fn a_jump_is_gone_over_from_its_start_slots() {
        let s = |n: u8, z: f32| (n, [0.0, 0.0, z], [0.0, 0.0, 1.0]);
        // (A gate: either way; a cavaletti one way each side.)
        let l = lanes(&[s(0, -5.75), s(1, 6.25)], false);
        assert_eq!(l, vec![(Vec2::new(0.0, -5.75), Vec2::new(0.0, 6.25)), (Vec2::new(0.0, 6.25), Vec2::new(0.0, -5.75))]);
        let l = lanes(&[s(0, 3.0), s(1, -3.0), s(2, 3.0), s(3, -3.0)], true);
        assert_eq!(l, vec![(Vec2::new(0.0, 3.0), Vec2::new(0.0, -3.0)), (Vec2::new(0.0, -3.0), Vec2::new(0.0, 3.0))]);
    }
}
