//! Race training posts, the Pets pack's (`TrainingPosts`: the Dashing Training Posts). A horse
//! trains for racing at them, galloping through each post on the lot in turn (up to twelve,
//! `kMaxPostCount`), in at one end and out at the other (the posts' routing slots, 2.5 m further
//! out: `RoutingSlotPos`), swinging round to come back through where it would turn back on
//! itself; lap after lap until it needs a break from learning (`PetSkillFatigue`) or is called
//! away. A played horse is sent to ("Train For Racing ›", in a tone); a restless one goes of its
//! own accord. A rider up on a horse rides it ("Train <Horse> For Racing": Riding 2 for it,
//! `kLevelForTrainForRacing`), the tone picked from the queue's tones box. A post's taken while a
//! horse trains on it ("<Horse> is already training on this post.").
//!
//! The tones are the game's (`kJockeyToneTuning`...): Basic Training; Jockey Training (Racing 2,
//! ridden only: more Riding for the rider, less Racing for the horse); Endurance Training (Racing 4:
//! at a trot, half as tiring and thirsty, slower to learn); Hard Gallop (Racing 6: two and a quarter
//! times the Racing, twice as tiring and thirsty); Perfect Pace (Racing 8: 1.75 times the Racing,
//! a little more tiring). At a gallop the horse learns Racing as it does galloping anywhere
//! (`kHorseRacingSkillGainWhileGalloping`, unridden `...UnRidden`) and the training's own
//! (`SkillRacing`, the rider's `RacingSkillMultiplier` on it), exercised and thirstier for it
//! (`HorseExercise`, `HorseThirst` with Racing's `kThirstMultiplierPerLevel`); its rider has fun and
//! learns Riding. From Racing 5 a session leaves it a Pumped Pony (`kLevelForPumpedPony`).
//!
//! Sims about "Watch" (4 to 6 m off to the side, `kMinWatchDistance`), turned to the posts,
//! cheering as the horse comes through one (`ProdWatchers`: Cheer), having fun and company
//! (`Watch`: Fun 30, Social 50 an hour).

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::horsejumps::HorseSkills;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::pets::{Pet, PetAnim, PetBusy};
use crate::sim::{Pose, Sim};

pub struct TrainingPostsPlugin;

impl Plugin for TrainingPostsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PostPassed>().add_systems(
            Update,
            (training_test, ordered, ridden, alone, gains, watching, autonomy).chain().after(crate::riding::ride).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)),
        );
    }
}

/// The game's words (`Gameplay/Objects/HobbiesSkills/TrainingPosts`).
pub const TRAIN: &str = "Train For Racing";
pub const WATCH: &str = "Watch";
pub const NEEDS_RIDER: &str = "A rider is required for this training.";

pub fn rider_label(horse: &str) -> String {
    format!("Train {horse} For Racing")
}

pub fn not_enough_riding(first: &str, female: bool) -> String {
    format!("{first} needs to improve {} Riding Skill before doing race training.", if female { "her" } else { "his" })
}

pub fn in_use(trainee: &str) -> String {
    format!("{trainee} is already training on this post.")
}

pub fn tired(horse: &str) -> String {
    format!("{horse} needs a break from learning new skills!")
}

/// The game's tuning (its XML's): the posts used, the rider's Riding for it, where watchers
/// stand; what training's worth an hour (its ITUN: the horse's exercise, thirst and Racing, the
/// rider's fun and Riding, a watcher's fun and company); a gallop's Racing a minute, unridden and
/// ridden; the Racing for a Pumped Pony.
const MAX_POSTS: usize = 12;
pub const RIDER_LEVEL: u32 = 2;
const WATCH_FROM: (f32, f32) = (4.0, 6.0);
const HORSE_EXERCISE: f32 = 10.0;
const HORSE_THIRST: f32 = -50.0;
const HORSE_RACING: f32 = 5.0;
const RIDER_FUN: f32 = 40.0;
const RIDER_RIDING: f32 = 100.0;
const WATCH_FUN: f32 = 30.0;
const WATCH_SOCIAL: f32 = 50.0;
const GALLOP_ALONE: f32 = 10.0;
const GALLOP_RIDDEN: f32 = 20.0;
const PUMPED_LEVEL: u32 = 5;
/// How far out from a post its ends are (its routing slots and the 2.5 m beyond them), and how
/// far before an end a horse lines up to come in straight.
const GATE: f32 = 3.0;
const APPROACH: f32 = 1.5;
/// Within this of a post's middle, it's going through it.
const THROUGH: f32 = 0.8;

/// The training tones (the game's `TrainingType`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Tone {
    #[default]
    Basic,
    Jockey,
    Endurance,
    HardGallop,
    PerfectPace,
}

/// A tone's tuning: the horse's Racing it's offered from, its Racing, the rider's Riding, the
/// horse's fatigue and thirst (multipliers), at a trot, and only ridden.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ToneTuning {
    pub level: u32,
    pub racing: f32,
    pub riding: f32,
    pub fatigue: f32,
    pub thirst: f32,
    pub trot: bool,
    pub rider: bool,
}

impl Tone {
    pub const ALL: [Tone; 5] = [Tone::Basic, Tone::Jockey, Tone::Endurance, Tone::HardGallop, Tone::PerfectPace];

    pub fn name(self) -> &'static str {
        match self {
            Tone::Basic => "Basic Training",
            Tone::Jockey => "Jockey Training",
            Tone::Endurance => "Endurance Training",
            Tone::HardGallop => "Hard Gallop",
            Tone::PerfectPace => "Perfect Pace",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Tone::Basic => "Horses will train at a normal pace.",
            Tone::Jockey => "This type of training will focus more on improving the Sim's skill.",
            Tone::Endurance => "Horses will be able to train for a longer period of time, but gain skill at a slower rate.",
            Tone::HardGallop => "This gallop will increase the rate of the Horse's skill gain, but also makes them tired and thirsty faster.",
            Tone::PerfectPace => "This training provides a moderately faster skill and fatigue gain for Horses.",
        }
    }

    pub fn tuning(self) -> ToneTuning {
        let t = |level, racing, riding, fatigue, thirst, trot, rider| ToneTuning { level, racing, riding, fatigue, thirst, trot, rider };
        match self {
            Tone::Basic => t(0, 1.0, 1.0, 1.0, 1.0, false, false),
            Tone::Jockey => t(2, 0.6, 1.5, 1.0, 1.0, false, true),
            Tone::Endurance => t(4, 0.6, 1.0, 0.5, 0.5, true, false),
            Tone::HardGallop => t(6, 2.25, 1.5, 2.0, 2.0, false, false),
            Tone::PerfectPace => t(8, 1.75, 1.25, 1.15, 1.0, false, false),
        }
    }

    /// The tones a horse of this Racing trains in, ridden or alone (the game's `Test`: its
    /// level; a rider's own tone only ridden).
    pub fn offered(racing: u32, ridden: bool) -> Vec<Tone> {
        Tone::ALL.into_iter().filter(|t| racing >= t.tuning().level && (ridden || !t.tuning().rider)).collect()
    }

    /// The gait it's run at: a canter for a trot, else a gallop (`riding`'s).
    pub fn gait(self) -> usize {
        if self.tuning().trot { 1 } else { 2 }
    }
}

/// A rider's Racing multiplier on their horse's training (`RacingSkillMultiplier`: 1 untrained,
/// 1.8 mastered).
pub fn rider_racing(level: u32) -> f32 {
    1.0 + 0.8 * level.min(10) as f32 / 10.0
}

/// How thirsty training makes a horse by its Racing (`kThirstMultiplierPerLevel`: 1, to a half
/// at 10).
pub fn thirst_rate(level: u32) -> f32 {
    1.0 - 0.5 * level.min(10) as f32 / 10.0
}

/// A horse's traits' say in its Racing (`kLazyPetRacingSkillMultiplier`,
/// `kFastPetRacingSkillMultiplier`).
pub fn trait_rate(traits: &[String]) -> f32 {
    let mut r = 1.0;
    if crate::pettraits::has(traits, "LazyHorse") {
        r *= 0.6;
    }
    if crate::pettraits::has(traits, "FastPet") {
        r *= 1.5;
    }
    r
}

/// A post's two ends, in front and behind.
pub fn gates(post: &Transform) -> (Vec2, Vec2) {
    (post.transform_point(Vec3::new(0.0, 0.0, GATE)).xz(), post.transform_point(Vec3::new(0.0, 0.0, -GATE)).xz())
}

/// A lap of the posts from where the horse is and the way it's going: each post in turn, the one
/// with the nearest end next, run through from that end to the other (lined up to come in
/// straight, by the way `route` finds there); where that end's the one it's just come out of, a
/// loop out and round to come back in (what's `free` to run on).
pub fn lap(posts: &[Transform], from: Vec2, heading: Vec2, free: impl Fn(Vec2) -> bool, route: impl Fn(Vec2, Vec2) -> Option<Vec<Vec2>>) -> Vec<Vec2> {
    let mut out = Vec::new();
    let mut left: Vec<&Transform> = posts.iter().take(MAX_POSTS).collect();
    let (mut cur, mut head) = (from, heading.normalize_or(Vec2::Y));
    while !left.is_empty() {
        let Some((i, entry, exit)) = left
            .iter()
            .enumerate()
            .flat_map(|(i, p)| {
                let (a, b) = gates(p);
                [(i, a, b), (i, b, a)]
            })
            .min_by(|x, y| x.1.distance(cur).total_cmp(&y.1.distance(cur)))
        else {
            break;
        };
        let post = left.remove(i);
        let dir = (exit - entry).normalize_or(Vec2::Y);
        if cur.distance(entry) < APPROACH {
            if head.dot(dir) < 0.0 {
                out.extend(turn_back(entry, dir, &free));
            }
        } else {
            let approach = entry - dir * APPROACH;
            out.extend(route(cur, approach).unwrap_or_else(|| vec![approach]));
        }
        out.extend([entry, post.translation.xz(), exit]);
        cur = exit;
        head = dir;
    }
    out
}

/// A loop out beyond a post's end and round, to come back in at it facing `dir` (the horse there
/// facing out): round a circle a little beyond it, to whichever side's free.
pub fn turn_back(at: Vec2, dir: Vec2, free: &impl Fn(Vec2) -> bool) -> Vec<Vec2> {
    for r in [2.5f32, 1.75] {
        let o = at - dir * (r + 1.0);
        for side in [1.0f32, -1.0] {
            let pts: Vec<Vec2> = [40.0f32, 90.0, 140.0, 180.0, 220.0, 270.0, 320.0]
                .iter()
                .map(|deg| {
                    let a = deg.to_radians();
                    o + dir * (r * a.cos()) + dir.perp() * (side * r * a.sin())
                })
                .collect();
            if pts.iter().all(|p| free(*p)) {
                return pts;
            }
        }
    }
    Vec::new()
}

/// Where a watcher stands: 4 to 6 m off to one side of the posts (the nearer to them), turned to
/// them; the first spot that's `free`.
pub fn watch_spot(post: &Transform, from: Vec2, free: impl Fn(Vec2) -> bool) -> Option<(Vec2, Vec2)> {
    let c = post.translation.xz();
    let side = (post.rotation * Vec3::X).xz().normalize_or(Vec2::X);
    let side = if (from - c).dot(side) < 0.0 { -side } else { side };
    let mid = (WATCH_FROM.0 + WATCH_FROM.1) / 2.0;
    for d in [mid, WATCH_FROM.0, WATCH_FROM.1] {
        for deg in [0.0f32, 25.0, -25.0, 50.0, -50.0] {
            for s in [side, -side] {
                let at = c + Vec2::from_angle(deg.to_radians()).rotate(s) * d;
                if free(at) {
                    return Some((at, (c - at).normalize_or(-s)));
                }
            }
        }
    }
    None
}

/// A horse training: at which posts (all it runs, the one it was sent to first), in which tone,
/// ridden by whom, its way round when alone, the post it's going through, since when, and whether
/// it's going (and how) now.
#[derive(Component, Debug)]
pub struct Training {
    pub posts: Vec<Entity>,
    pub tone: Tone,
    pub rider: Option<Entity>,
    path: Vec<Vec2>,
    through: Option<Entity>,
    since: f64,
    moving: bool,
    /// Gone to of its own accord (over once it's exercised enough).
    own_accord: bool,
}

/// A horse told to train (by the player, or of its own accord), not yet at it.
#[derive(Component, Clone, Copy, Debug)]
pub struct TrainOrder {
    pub posts: Entity,
    pub tone: Tone,
    pub own_accord: bool,
}

/// A horse coming through a post (the game's `OnPortalStart`): its watchers cheer.
#[derive(Message, Clone, Copy, Debug)]
pub struct PostPassed {
    pub post: Entity,
}

/// The posts a horse training at one runs: the lot's (the ones on the same lot as it, free of
/// other trainees), it first.
fn acquire(first: Entity, posts: &[(Entity, Vec2, bool)], taken: &HashSet<Entity>) -> Vec<Entity> {
    let Some(&(_, at, town)) = posts.iter().find(|p| p.0 == first) else { return Vec::new() };
    let mut v: Vec<(Entity, f32)> = posts.iter().filter(|p| p.0 != first && p.2 == town && !taken.contains(&p.0)).map(|p| (p.0, p.1.distance(at))).collect();
    v.sort_by(|a, b| a.1.total_cmp(&b.1));
    std::iter::once(first).chain(v.into_iter().map(|x| x.0)).take(MAX_POSTS).collect()
}

/// Who trains on each post now.
fn trainees(training: impl Iterator<Item = (Entity, Vec<Entity>)>) -> HashMap<Entity, Entity> {
    let mut m = HashMap::new();
    for (horse, posts) in training {
        for p in posts {
            m.insert(p, horse);
        }
    }
    m
}

/// A session's end: a horse of Racing 5 or more a Pumped Pony for it (`AddTrainForRacingTime`).
fn session_over(commands: &mut Commands, horse: Entity, racing: u32, now: f64) {
    if racing >= PUMPED_LEVEL {
        crate::petmoods::give(commands, horse, crate::life::MoodletKind::PumpedPony, now);
    }
}

/// The posts' places for laps: by entity.
fn post_tfs(posts: &[Entity], q: &Query<(Entity, &GameObject, &Transform, Has<crate::visit::LotObject>), (Without<Pet>, Without<Sim>)>) -> Vec<Transform> {
    posts.iter().filter_map(|p| q.get(*p).ok().map(|x| *x.2)).collect()
}

/// A Sim's posts menu: up on a horse, "Train <Horse> For Racing" (greyed for too little Riding,
/// a horse in need of a break, or the posts taken by another horse); on foot, "Watch" a horse
/// training there.
#[allow(clippy::too_many_arguments)]
pub fn sim_options(
    posts: Entity,
    ptf: &Transform,
    sim: &Sim,
    riding_level: u32,
    riding: Option<(Entity, &Pet, Option<&HorseSkills>)>,
    trainee: Option<(Entity, &str)>,
    here: Vec2,
    free: impl Fn(Vec2) -> bool,
) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    match riding {
        Some((h, horse, skills)) => {
            let label = rider_label(&horse.name);
            let why = if riding_level < RIDER_LEVEL {
                Some(not_enough_riding(&sim.first, sim.female))
            } else if skills.is_some_and(|s| s.worn_out) {
                Some(tired(&horse.name))
            } else {
                trainee.filter(|t| t.0 != h).map(|t| in_use(t.1))
            };
            out.push((label, why.map_or(ActionKind::TrainForRacing { posts, horse: h, tone: Tone::Basic }, ActionKind::Greyed)));
        }
        None => {
            if trainee.is_some()
                && let Some((at, face)) = watch_spot(ptf, here, free)
            {
                out.push((WATCH.to_string(), ActionKind::WatchTraining { posts, at, face }));
            }
        }
    }
    out
}

/// A played horse's posts menu: "Train For Racing ›" the tones it can (Jockey Training greyed:
/// it needs a rider), greyed in need of a break or with the posts taken.
pub fn horse_options(posts: Entity, horse: Entity, pet: &Pet, skills: Option<&HorseSkills>, trainee: Option<(Entity, &str)>, pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>, submenu: impl Fn(usize) -> ActionKind) -> Vec<(String, ActionKind)> {
    if pet.kind != "ah" {
        return Vec::new();
    }
    if skills.is_some_and(|s| s.worn_out) {
        return vec![(TRAIN.to_string(), ActionKind::Greyed(tired(&pet.name)))];
    }
    if let Some(t) = trainee.filter(|t| t.0 != horse) {
        return vec![(TRAIN.to_string(), ActionKind::Greyed(in_use(t.1)))];
    }
    let racing = skills.map_or(0, |s| s.racing_level());
    let list: Vec<(String, ActionKind)> = Tone::offered(racing, true)
        .into_iter()
        .map(|t| {
            let kind = if t.tuning().rider { ActionKind::Greyed(NEEDS_RIDER.into()) } else { ActionKind::PetCommand(crate::petcontrol::PetCommand::Train(posts, t)) };
            (t.name().to_string(), kind)
        })
        .collect();
    let out = vec![(format!("{TRAIN} ›"), submenu(pie_submenus.len()))];
    pie_submenus.push((TRAIN.to_string(), list));
    out
}

/// The tones a queued action can be in (the queue's tones box): a ridden horse's training, the
/// ones its Racing's up to, and the one it's in.
pub fn queue_tones(kind: &ActionKind, racing: impl Fn(Entity) -> u32) -> Option<(Vec<Tone>, Tone)> {
    match kind {
        ActionKind::TrainForRacing { horse, tone, .. } => Some((Tone::offered(racing(*horse), true), *tone)),
        _ => None,
    }
}

/// A queued action put in another tone.
pub fn set_tone(kind: &mut ActionKind, to: Tone) -> bool {
    match kind {
        ActionKind::TrainForRacing { tone, .. } if *tone != to => {
            *tone = to;
            true
        }
        _ => false,
    }
}

/// Played and self-sent horses set off training (a tone it's not up to, Basic).
#[allow(clippy::type_complexity)]
fn ordered(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    horses: Query<(Entity, &Pet, &TrainOrder, Option<&HorseSkills>, Has<crate::riding::Ridden>), Without<Training>>,
    training: Query<&Training>,
    posts_q: Query<(Entity, &GameObject, &Transform, Has<crate::visit::LotObject>), (Without<Pet>, Without<Sim>)>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    for (e, pet, order, skills, ridden) in &horses {
        commands.entity(e).remove::<TrainOrder>();
        if ridden {
            continue;
        }
        if skills.is_some_and(|s| s.worn_out) {
            if !order.own_accord {
                notes.push(tired(&pet.name));
            }
            continue;
        }
        let taken: HashSet<Entity> = training.iter().flat_map(|t| t.posts.iter().copied()).collect();
        if taken.contains(&order.posts) {
            continue;
        }
        let all: Vec<(Entity, Vec2, bool)> = posts_q.iter().filter(|p| p.1.kind == ObjectKind::TrainingPosts).map(|p| (p.0, p.2.translation.xz(), p.3)).collect();
        let posts = acquire(order.posts, &all, &taken);
        if posts.is_empty() {
            continue;
        }
        let racing = skills.map_or(0, |s| s.racing_level());
        let tone = if Tone::offered(racing, false).contains(&order.tone) { order.tone } else { Tone::Basic };
        info!("training posts: {} trains for racing ({} posts, {})", pet.name, posts.len(), tone.name());
        commands
            .entity(e)
            .insert((Training { posts, tone, rider: None, path: Vec::new(), through: None, since: clock.minutes, moving: false, own_accord: order.own_accord }, PetBusy))
            .remove::<(crate::petneeds::PetTask, crate::petcompany::Greeting, crate::pethunting::Hunt)>();
    }
}

/// Riders training their horses: started once they're up, its laps ridden one after another
/// (the next before the last's run out), the tone kept to; over when the training's dropped from
/// their queue (cancelled, or anything else asked of them) or the horse needs a break.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn ridden(
    mut commands: Commands,
    (clock, delta): (Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    mut riders: Query<(Entity, &Sim, &mut ActionQueue, &mut crate::riding::Mounted, &mut crate::interact::Skills, &mut crate::sim::Motives, Option<&crate::life::Mood>)>,
    mut horses: Query<(&Pet, &Transform, Option<&mut Training>, Option<&HorseSkills>), Without<Sim>>,
    posts_q: Query<(Entity, &GameObject, &Transform, Has<crate::visit::LotObject>), (Without<Pet>, Without<Sim>)>,
    grid: Option<Res<crate::nav::NavGrid>>,
    (mut notes, mut life): (ResMut<crate::interact::Notifications>, MessageWriter<crate::life::LifeEvent>),
) {
    let hours = delta.0 / 60.0;
    let taken: HashSet<Entity> = horses.iter().filter_map(|h| h.2).flat_map(|t| t.posts.iter().copied()).collect();
    for (e, sim, mut queue, mut m, mut skills, mut motives, mood) in &mut riders {
        // (Cancelled from the queue: gone from it, behind the ride as it is.)
        queue.0.retain(|a| !(a.cancel && matches!(a.kind, ActionKind::TrainForRacing { .. })));
        let want = queue.0.iter().find_map(|a| match a.kind {
            ActionKind::TrainForRacing { posts, horse, tone } => Some((posts, horse, tone)),
            _ => None,
        });
        let horse = m.horse;
        let Ok((pet, htf, training, hskills)) = horses.get_mut(horse) else { continue };
        let racing = hskills.map_or(0, |s| s.racing_level());
        let worn = hskills.is_some_and(|s| s.worn_out);
        match (want, training) {
            (Some((posts, h, tone)), None) if h == horse && m.up() => {
                if worn {
                    notes.push(tired(&pet.name));
                    queue.0.retain(|a| !matches!(a.kind, ActionKind::TrainForRacing { .. }));
                    continue;
                }
                let all: Vec<(Entity, Vec2, bool)> = posts_q.iter().filter(|p| p.1.kind == ObjectKind::TrainingPosts).map(|p| (p.0, p.2.translation.xz(), p.3)).collect();
                let posts = acquire(posts, &all, &taken);
                if posts.is_empty() {
                    queue.0.retain(|a| !matches!(a.kind, ActionKind::TrainForRacing { .. }));
                    continue;
                }
                info!("training posts: {} rides {} round {} posts ({})", sim.first, pet.name, posts.len(), tone.name());
                m.training = Some(tone);
                m.halt();
                commands.entity(horse).insert(Training { posts, tone, rider: Some(e), path: Vec::new(), through: None, since: clock.minutes, moving: false, own_accord: false });
            }
            (Some((_, h, tone)), Some(mut t)) if h == horse && t.rider == Some(e) => {
                // (Worn out: the training's over.)
                if worn {
                    notes.push(tired(&pet.name));
                    queue.0.retain(|a| !matches!(a.kind, ActionKind::TrainForRacing { .. }));
                    continue;
                }
                if t.tone != tone {
                    t.tone = tone;
                    m.extend_route(Vec::new(), tone.gait());
                    info!("training posts: {} trains {} in {}", sim.first, pet.name, tone.name());
                }
                m.training = Some(tone);
                // (The next lap on before this one's run out: from its last stretch on.)
                if m.up() && m.route().len() <= 1 {
                    let tfs = post_tfs(&t.posts, &posts_q);
                    let r = m.route();
                    let (from, heading) = match r {
                        [] => (htf.translation.xz(), (htf.rotation * Vec3::Z).xz()),
                        [last] => (*last, *last - htf.translation.xz()),
                        _ => unreachable!(),
                    };
                    let g = grid.as_deref();
                    let free = |p: Vec2| g.is_none_or(|g| g.cell_of(p).is_some_and(|(x, z)| !g.is_blocked(x, z)));
                    let more = lap(&tfs, from, heading, free, |a, b| g.and_then(|g| g.find_path(a, b)));
                    m.extend_route(more, tone.gait());
                }
                t.moving = m.up() && !m.route().is_empty();
                // (The rider's fun and Riding from it: `RiderTrainForRacing`'s.)
                if hours > 0.0 && t.moving {
                    motives.add(crate::sim::FUN, RIDER_FUN * hours);
                    let rate = crate::life::skill_rate(&sim.traits, "Riding") * mood.map_or(1.0, |m| m.skill_rate()) * tone.tuning().riding;
                    if let Some(l) = skills.add_points("Riding", RIDER_RIDING * hours * rate) {
                        notes.push(format!("{} reached level {l} in Riding!", sim.first));
                        life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: "Riding", level: l }));
                    }
                }
            }
            (None, Some(t)) if t.rider == Some(e) => {
                // Over: pulled up where it is.
                m.training = None;
                m.halt();
                commands.entity(horse).remove::<Training>();
                session_over(&mut commands, horse, racing, clock.minutes);
                info!("training posts: {} stops training {} ({:.0} minutes)", sim.first, pet.name, clock.minutes - t.since);
            }
            _ => {
                if want.is_none() {
                    m.training = None;
                }
            }
        }
    }
}

/// Horses training alone: round the posts lap after lap at a gallop (a canter in the Endurance
/// tone), till worn out, called away or (gone of their own accord) exercised enough.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn alone(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut horses: Query<(Entity, &Pet, &mut Training, &mut Transform, &mut PetAnim, Option<&HorseSkills>, Option<&crate::petneeds::PetMotives>, (Has<crate::petcontrol::PetOrder>, Has<crate::riding::Ridden>))>,
    posts_q: Query<(Entity, &GameObject, &Transform, Has<crate::visit::LotObject>), (Without<Pet>, Without<Sim>)>,
    (grid, world, building): (Option<Res<crate::nav::NavGrid>>, Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>),
    mut notes: ResMut<crate::interact::Notifications>,
    mut passed: MessageWriter<PostPassed>,
    riders: Query<(), With<crate::riding::Mounted>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    for (e, pet, mut t, mut tf, mut anim, skills, motives, (told, ridden)) in &mut horses {
        // (Through a post: its watchers cheer.)
        let here = tf.translation.xz();
        let near = t.posts.iter().copied().find(|p| posts_q.get(*p).is_ok_and(|x| x.2.translation.xz().distance(here) < THROUGH));
        match near {
            Some(p) if t.through != Some(p) => {
                t.through = Some(p);
                passed.write(PostPassed { post: p });
            }
            Some(_) => {}
            None => {
                if t.posts.iter().all(|p| posts_q.get(*p).is_ok_and(|x| x.2.translation.xz().distance(here) > 2.0)) {
                    t.through = None;
                }
            }
        }
        if let Some(r) = t.rider {
            // (Ridden: `ridden` and `riding` see to it; its rider gone, it's over.)
            if !riders.contains(r) {
                commands.entity(e).remove::<Training>();
            }
            continue;
        }
        let racing = skills.map_or(0, |s| s.racing_level());
        let worn = skills.is_some_and(|s| s.worn_out);
        let exercised = t.own_accord && motives.is_some_and(|m| m.0[crate::petneeds::URGE] >= 95.0);
        let gone = t.posts.iter().any(|p| !posts_q.contains(*p));
        if worn || told || ridden || exercised || gone {
            if worn && !t.own_accord {
                notes.push(tired(&pet.name));
            }
            // (Got up on: the ride's busy with it now.)
            if ridden {
                commands.entity(e).remove::<Training>();
            } else {
                commands.entity(e).remove::<(Training, PetBusy)>();
                anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
            }
            session_over(&mut commands, e, racing, clock.minutes);
            info!("training posts: {} stops training ({:.0} minutes)", pet.name, clock.minutes - t.since);
            continue;
        }
        // (The next lap on before this one's run out.)
        if t.path.len() <= 1 {
            let tfs = post_tfs(&t.posts, &posts_q);
            let (from, heading) = match t.path.as_slice() {
                [] => (here, (tf.rotation * Vec3::Z).xz()),
                [last] => (*last, *last - here),
                _ => unreachable!(),
            };
            let g = grid.as_deref();
            let free = |p: Vec2| g.is_none_or(|g| g.cell_of(p).is_some_and(|(x, z)| !g.is_blocked(x, z)));
            let more = lap(&tfs, from, heading, free, |a, b| g.and_then(|g| g.find_path(a, b)));
            t.path.extend(more);
        }
        let Some(&next) = t.path.first() else {
            t.moving = false;
            continue;
        };
        let clip = if t.tone.tuning().trot { "ah_canter_x" } else { crate::riding::gallop(racing).0 };
        if anim.clip != clip {
            anim.play(clip.to_string());
        }
        let pace = lib.get(&data, clip).map_or(4.0, |c| crate::pets::clip_pace(&c));
        let step = pace * dt;
        let d = next - here;
        if d.length() <= step.max(0.05) {
            t.path.remove(0);
        } else {
            let p = here + d.normalize() * step;
            let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(p.x, tf.translation.y, p.y));
            tf.translation = Vec3::new(p.x, y, p.y);
            tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
        }
        t.moving = true;
    }
}

/// What training does a horse: exercise and thirst, and Racing (a gallop's and the training's
/// own, the tone's and its traits' say in it), tiring it at the tone's rate as it goes.
#[allow(clippy::type_complexity)]
fn gains(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    mut horses: Query<(Entity, &Pet, &Training, Option<&mut HorseSkills>, Option<&mut crate::petneeds::PetMotives>, Option<&crate::pets::PetTraits>)>,
    riders: Query<&crate::interact::Skills>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let minutes = delta.0;
    if minutes <= 0.0 {
        return;
    }
    let hours = minutes / 60.0;
    for (e, pet, t, skills, motives, traits) in &mut horses {
        if !t.moving {
            continue;
        }
        let tune = t.tone.tuning();
        let racing = skills.as_deref().map_or(0, |s| s.racing_level());
        if let Some(mut m) = motives {
            m.add(crate::petneeds::URGE, HORSE_EXERCISE * hours);
            m.add(crate::petneeds::THIRST, HORSE_THIRST * hours * thirst_rate(racing) * tune.thirst);
        }
        let rider_mul = t.rider.and_then(|r| riders.get(r).ok()).map_or(1.0, |s| rider_racing(s.level("Riding")));
        let gallop = if tune.trot { 0.0 } else if t.rider.is_some() { GALLOP_RIDDEN } else { GALLOP_ALONE };
        let points = (gallop * minutes + HORSE_RACING * hours * rider_mul) * tune.racing * trait_rate(traits.map_or(&[][..], |x| &x.0[..]));
        match skills {
            Some(mut s) => {
                if let Some(l) = s.train(points, minutes, tune.fatigue) {
                    notes.push(format!("{} reached level {l} in Racing!", pet.name));
                }
            }
            None => {
                let mut s = HorseSkills::default();
                s.train(points, minutes, tune.fatigue);
                commands.entity(e).insert(s);
            }
        }
    }
}

/// A watcher cheering (seconds left of it).
#[derive(Component)]
struct Cheering(f32);

/// Watchers: fun and company while a horse trains at their posts, a cheer as it comes through
/// one; done when it's done.
#[allow(clippy::type_complexity)]
fn watching(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib, ui): (Res<Baked>, ResMut<ClipLibrary>, Option<Res<crate::icons::GameUi>>),
    mut watchers: Query<(Entity, &Sim, &mut ActionQueue, &mut crate::sim::Motives, Option<&mut Cheering>, Option<&ActionClip>)>,
    training: Query<&Training>,
    mut passed: MessageReader<PostPassed>,
    mut known: Local<Option<HashSet<String>>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let hours = delta.0 / 60.0;
    let through: Vec<Entity> = passed.read().map(|p| p.post).collect();
    let mut rng = rand::rng();
    for (e, sim, mut q, mut m, cheering, clip) in &mut watchers {
        let Some(posts) = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).and_then(|a| match a.kind {
            ActionKind::WatchTraining { posts, .. } => Some(posts),
            _ => None,
        }) else {
            if cheering.is_some() {
                commands.entity(e).remove::<(Cheering, ActionClip)>();
            }
            continue;
        };
        // (Nobody training there any more: done watching.)
        let Some(t) = training.iter().find(|t| t.posts.contains(&posts)) else {
            q.0.pop_front();
            commands.entity(e).remove::<(Cheering, ActionClip)>();
            continue;
        };
        if hours > 0.0 {
            m.add(crate::sim::FUN, WATCH_FUN * hours);
            m.add(crate::sim::SOCIAL, WATCH_SOCIAL * hours);
        }
        if let Some(mut c) = cheering {
            c.0 -= dt;
            if c.0 <= 0.0 {
                commands.entity(e).remove::<(Cheering, ActionClip)>();
            }
            continue;
        }
        if clip.is_some() || !through.iter().any(|p| t.posts.contains(p)) {
            continue;
        }
        let Some(ui) = ui.as_deref() else { continue };
        let known = known.get_or_insert_with(|| data.0.clip_names.iter().map(|n| n.to_ascii_lowercase()).collect());
        let Some(clips) = crate::reactions::clips_for(&ui.data.idles.reactions, "Cheer", sim, false, known, &mut rng) else { continue };
        let secs: f32 = clips.iter().filter_map(|c| lib.get(&data, c)).map(|c| c.duration).sum();
        let leaked: Vec<&'static str> = clips.into_iter().map(crate::pets::intern).collect();
        let leaked: &'static [&'static str] = Box::leak(leaked.into_boxed_slice());
        commands.entity(e).insert((ActionClip::steps(leaked[0], &leaked[1..], &[]), Cheering(secs)));
        debug!("training posts: {} cheers", sim.first);
    }
}

/// Restless household horses go and train at the lot's posts of their own accord, now and then
/// (`HorseTrainForRacing`'s advertising: Exercise), when they're fresh enough to learn.
#[allow(clippy::type_complexity)]
fn autonomy(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    horses: Query<(Entity, &Pet, &crate::petneeds::PetMotives, &Transform, Option<&HorseSkills>), (With<crate::pets::HomePet>, Without<PetBusy>, Without<crate::petneeds::PetTask>, Without<Training>, Without<TrainOrder>, Without<crate::sim::Selected>, Without<crate::hitching::Tied>, Without<crate::riding::Ridden>)>,
    posts_q: Query<(Entity, &GameObject, &Transform), (Without<Pet>, Without<crate::visit::LotObject>)>,
    training: Query<&Training>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 60.0;
    if std::env::var_os("TRAINING_TEST").is_some() {
        return;
    }
    let taken: HashSet<Entity> = training.iter().flat_map(|t| t.posts.iter().copied()).collect();
    let mut rng = rand::rng();
    for (e, pet, m, tf, skills) in &horses {
        if pet.kind != "ah" || m.0[crate::petneeds::URGE] > -20.0 || skills.is_some_and(|s| s.worn_out || s.fresh < 0.0) || !rng.random_bool(0.5) {
            continue;
        }
        let near = posts_q.iter().filter(|p| p.1.kind == ObjectKind::TrainingPosts && !taken.contains(&p.0)).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
        if let Some((p, ..)) = near {
            commands.entity(e).insert(TrainOrder { posts: p, tone: Tone::Basic, own_accord: true });
            info!("training posts: {} goes to train of its own accord", pet.name);
            break;
        }
    }
}

/// `TRAINING_TEST=<horse>` (tests): ten seconds in, training posts put down near the horse
/// (`TRAINING_POSTS=<n>` of them, one), and the horse sent to train at them; with
/// `TRAINING_RIDE=1` the selected Sim rides it there (Riding 3 for it) and trains it;
/// `TRAINING_TONE=<tone>` in that tone (`HORSE_RACING=<level>` the horse that good,
/// `TRAINING_FRESH=<freshness>` that fresh for learning);
/// `TRAINING_WATCH=<first name>` that Sim of the household watching. The camera on the posts.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn training_test(
    mut commands: Commands,
    time: Res<Time>,
    horses: Query<(Entity, &Pet, &Transform), With<crate::pets::HomePet>>,
    mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, &mut crate::interact::Skills, Option<&crate::riding::Mounted>, Has<crate::sim::Selected>), Without<Pet>>,
    posts_q: Query<(Entity, &GameObject, &Transform), (Without<Pet>, Without<Sim>)>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("TRAINING_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let Some((horse, pet, htf)) = horses.iter().find(|h| h.1.name.eq_ignore_ascii_case(&name)) else { return };
    let mine: Vec<(Entity, Transform)> = posts_q.iter().filter(|p| p.1.kind == ObjectKind::TrainingPosts).map(|p| (p.0, *p.2)).collect();
    if let (Ok(mut c), Some((_, ptf))) = (cams.single_mut(), mine.first()) {
        c.look_at(ptf.translation);
        c.distance = std::env::var("TRAINING_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(16.0);
        c.pitch = 0.7;
    }
    let tone = std::env::var("TRAINING_TONE").ok().and_then(|t| Tone::ALL.into_iter().find(|x| format!("{x:?}").eq_ignore_ascii_case(&t) || x.name().eq_ignore_ascii_case(&t))).unwrap_or_default();
    let ride = std::env::var_os("TRAINING_RIDE").is_some();
    match stage.0 {
        0 => {
            *stage = (1, now);
            // (HORSE_RACING=<level>, TRAINING_FRESH=<freshness>: the horse that good, that tired.)
            let racing = std::env::var("HORSE_RACING").ok().and_then(|l| l.parse::<f32>().ok());
            let fresh = std::env::var("TRAINING_FRESH").ok().and_then(|l| l.parse::<f32>().ok());
            if racing.is_some() || fresh.is_some() {
                commands.entity(horse).insert(HorseSkills { racing: racing.unwrap_or(0.0), fresh: fresh.unwrap_or(100.0), ..default() });
            }
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("trainingPostsClassic")).map(|c| c.objd) else { return };
            let n: usize = std::env::var("TRAINING_POSTS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
            // (On open ground near the horse: each post, its ends and round them free.)
            let g = grid.as_deref();
            let free = |p: Vec2| g.is_none_or(|g| g.cell_of(p).is_some_and(|(x, z)| !g.is_blocked(x, z)));
            let clear = |c: Vec2| (-4..=4).all(|i| (-2..=2).all(|j| free(c + Vec2::new(j as f32 * 1.0, i as f32 * 1.2))));
            let here = htf.translation.xz();
            let mut placed: Vec<Vec2> = Vec::new();
            for k in 0..n {
                let spot = (3..30).flat_map(|r| (0..16).map(move |a| here + Vec2::from_angle(a as f32 * std::f32::consts::TAU / 16.0) * r as f32)).find(|c| clear(*c) && placed.iter().all(|p| p.distance(*c) > 9.0));
                let Some(spot) = spot else { break };
                placed.push(spot);
                let p = Vec3::new(spot.x, htf.translation.y, spot.y);
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, k as f32 * 0.7) {
                    commands.entity(o.entity).insert(crate::save::Bought);
                    info!("training test: posts at {p:.1}");
                }
            }
        }
        1 if now - stage.1 > 2.0 => {
            let Some((posts, ..)) = mine.first().copied() else { return };
            *stage = (2, now);
            if ride {
                if let Some((_, sim, _, mut q, mut skills, ..)) = sims.iter_mut().find(|s| s.6) {
                    if skills.level("Riding") < RIDER_LEVEL {
                        skills.0.insert("Riding", 3.0);
                    }
                    q.push_player(Action::new("Ride", ActionKind::Ride { horse, slot: crate::riding::mount_slot(htf), unsaddle: false }, false));
                    info!("training test: {} gets on {}", sim.first, pet.name);
                }
            } else {
                commands.entity(horse).insert(TrainOrder { posts, tone, own_accord: false });
            }
        }
        2 if ride => {
            let Some((posts, ..)) = mine.first().copied() else { return };
            if let Some((_, sim, _, mut q, _, Some(m), _)) = sims.iter_mut().find(|s| s.6)
                && m.up()
            {
                *stage = (3, now);
                q.push_player(Action::new(rider_label(&pet.name), ActionKind::TrainForRacing { posts, horse, tone }, false));
                info!("training test: {} trains {} ({})", sim.first, pet.name, tone.name());
            }
        }
        2 | 3 if now - stage.1 > 4.0 => {
            *stage = (4, now);
            let Some(&(posts, ptf)) = mine.first() else { return };
            let Ok(who) = std::env::var("TRAINING_WATCH") else { return };
            let g = grid.as_deref();
            let free = |p: Vec2| g.is_none_or(|g| g.cell_of(p).is_some_and(|(x, z)| !g.is_blocked(x, z)));
            if let Some((_, sim, stf, mut q, ..)) = sims.iter_mut().find(|s| s.1.first.eq_ignore_ascii_case(&who))
                && let Some((at, face)) = watch_spot(&ptf, stf.translation.xz(), free)
            {
                q.push_player(Action::new(WATCH, ActionKind::WatchTraining { posts, at, face }, false));
                info!("training test: {} watches", sim.first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tones_are_the_games() {
        assert_eq!(Tone::offered(0, true), [Tone::Basic]);
        assert_eq!(Tone::offered(4, false), [Tone::Basic, Tone::Endurance], "Jockey Training's a rider's");
        assert_eq!(Tone::offered(8, true), Tone::ALL);
        assert_eq!(Tone::Endurance.gait(), 1, "a trot");
        assert_eq!(Tone::HardGallop.tuning().racing, 2.25);
        assert!((rider_racing(10) - 1.8).abs() < 1e-6);
        assert!((thirst_rate(10) - 0.5).abs() < 1e-6);
        assert!((trait_rate(&["FastPet".to_string()]) - 1.5).abs() < 1e-6);
        let mut k = ActionKind::TrainForRacing { posts: Entity::from_raw_u32(1).unwrap(), horse: Entity::from_raw_u32(2).unwrap(), tone: Tone::Basic };
        assert!(set_tone(&mut k, Tone::Jockey));
        assert_eq!(queue_tones(&k, |_| 2), Some((vec![Tone::Basic, Tone::Jockey], Tone::Jockey)));
    }

    #[test]
    fn a_lap_runs_each_post_end_to_end() {
        let a = Transform::IDENTITY;
        let (front, back) = gates(&a);
        assert!((front - Vec2::new(0.0, 3.0)).length() < 1e-5 && (back - Vec2::new(0.0, -3.0)).length() < 1e-5);
        // (From in front: through to behind.)
        let p = lap(&[a], Vec2::new(0.0, 8.0), Vec2::NEG_Y, |_| true, |_, b| Some(vec![b]));
        assert_eq!(p.len(), 4);
        assert!((p[1] - front).length() < 1e-5 && (p[3] - back).length() < 1e-5);
        // (Out behind, facing on: round and back through the other way.)
        let q = lap(&[a], back, Vec2::NEG_Y, |_| true, |_, b| Some(vec![b]));
        assert_eq!(q.len(), 7 + 3, "a loop, then through");
        assert!(q[..7].iter().all(|x| x.y < -3.0), "the loop's beyond the end");
        assert!((q[9] - front).length() < 1e-5);
        // (Two posts: the nearer first.)
        let b = Transform::from_xyz(10.0, 0.0, 0.0);
        let r = lap(&[a, b], Vec2::new(10.0, 8.0), Vec2::NEG_Y, |_| true, |_, b| Some(vec![b]));
        assert!((r[2] - Vec2::new(10.0, 0.0)).length() < 1e-5);
        assert!(r.iter().any(|x| (*x - Vec2::ZERO).length() < 1e-5));
    }

    #[test]
    fn the_menus_are_greyed_as_the_games() {
        let (posts, horse, other) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let horse_of = |kind: &str| Pet { kind: kind.into(), breed: 0, name: "Freebird".into(), female: false, look: Default::default(), elder: false };
        let pet = horse_of("ah");
        let mut sim = crate::sim::random_sim(&mut rand::rng(), "Johnny", Some(false), crate::sim::Age::Adult);
        sim.first = "Johnny".into();
        let tf = Transform::IDENTITY;
        let label = |o: &[(String, ActionKind)]| o.first().map(|x| x.0.clone());
        // (Up on it: trained, unless they can't ride well enough, it's tired or the posts taken.)
        let ok = sim_options(posts, &tf, &sim, 2, Some((horse, &pet, None)), None, Vec2::ZERO, |_| true);
        assert_eq!(label(&ok).as_deref(), Some("Train Freebird For Racing"));
        assert!(matches!(ok[0].1, ActionKind::TrainForRacing { tone: Tone::Basic, .. }));
        let green = sim_options(posts, &tf, &sim, 1, Some((horse, &pet, None)), None, Vec2::ZERO, |_| true);
        assert!(matches!(&green[0].1, ActionKind::Greyed(w) if w == "Johnny needs to improve his Riding Skill before doing race training."));
        let worn = HorseSkills { worn_out: true, ..Default::default() };
        assert!(matches!(&sim_options(posts, &tf, &sim, 5, Some((horse, &pet, Some(&worn))), None, Vec2::ZERO, |_| true)[0].1, ActionKind::Greyed(w) if w.contains("needs a break")));
        assert!(matches!(&sim_options(posts, &tf, &sim, 5, Some((horse, &pet, None)), Some((other, "Shadow")), Vec2::ZERO, |_| true)[0].1, ActionKind::Greyed(w) if w == "Shadow is already training on this post."));
        // (On foot: watched while a horse trains there.)
        assert!(sim_options(posts, &tf, &sim, 0, None, None, Vec2::ZERO, |_| true).is_empty());
        assert_eq!(label(&sim_options(posts, &tf, &sim, 0, None, Some((other, "Shadow")), Vec2::new(-9.0, 0.0), |_| true)).as_deref(), Some(WATCH));
        // (A played horse: its tones, a rider's greyed.)
        let mut subs = Vec::new();
        let racing = HorseSkills { racing: 2.0, ..Default::default() };
        let opts = horse_options(posts, horse, &pet, Some(&racing), None, &mut subs, |_| ActionKind::Dismount);
        assert_eq!(opts[0].0, "Train For Racing ›");
        assert_eq!(subs[0].1.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["Basic Training", "Jockey Training"]);
        assert!(matches!(&subs[0].1[1].1, ActionKind::Greyed(w) if w == NEEDS_RIDER));
        assert!(horse_options(posts, horse, &horse_of("ad"), None, None, &mut subs, |_| ActionKind::Dismount).is_empty());
    }

    #[test]
    fn watchers_stand_off_to_the_side() {
        let post = Transform::IDENTITY;
        let (at, face) = watch_spot(&post, Vec2::new(-10.0, 1.0), |_| true).unwrap();
        assert!((at - Vec2::new(-5.0, 0.0)).length() < 1e-4, "the nearer side, 5 m off");
        assert!((face - Vec2::X).length() < 1e-4, "turned to the posts");
        assert!(watch_spot(&post, Vec2::ZERO, |_| false).is_none());
    }
}
