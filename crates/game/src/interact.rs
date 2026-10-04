//! Interactions: object kinds, the per-sim action queue, social interactions, autonomy,
//! careers and household money.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use rand::Rng;
use s3bake::Key;

use crate::PlayMode;
use crate::clock::{GameClock, SimDelta};
use crate::loading::CurrentWorld;
use crate::nav::{NavGrid, PathFollow};
use crate::sim::*;

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Notifications>().add_systems(
            Update,
            (work_schedule, autonomy, run_actions, motive_warnings, pay_bills)
                .chain()
                .run_if(in_state(PlayMode::Live)),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Objects

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ObjectKind {
    Fridge,
    Stove,
    Microwave,
    BedDouble,
    BedSingle,
    Toilet,
    Shower,
    Bathtub,
    Sink,
    Sofa,
    Chair,
    Tv,
    Computer,
    Stereo,
    Bookshelf,
    Mirror,
    Easel,
    Guitar,
    Treadmill,
    Chess,
    Table,
    Light,
    Plant,
    Decoration,
    Other,
}

impl ObjectKind {
    pub fn from_script(script: &str, name: &str) -> Self {
        let s = script.to_ascii_lowercase();
        let n = name.to_ascii_lowercase();
        let has = |k: &str| s.contains(k);
        if has("fridge") {
            Self::Fridge
        } else if has("microwave") {
            Self::Microwave
        } else if has("stove") {
            Self::Stove
        } else if has("beds.") && (has("double") || has("bedcircle") || has("heart")) {
            Self::BedDouble
        } else if has("beds.") {
            Self::BedSingle
        } else if has("toilet") {
            Self::Toilet
        } else if has("bathtubshower") || has("shower") {
            Self::Shower
        } else if has("bathtub") || has("hottub") {
            Self::Bathtub
        } else if has("sink") {
            Self::Sink
        } else if has("sofa") || has("loveseat") {
            Self::Sofa
        } else if has("seating") || has("chair") || has("stool") || has("bench") {
            Self::Chair
        } else if has("electronics.tv") {
            Self::Tv
        } else if has("computer") {
            Self::Computer
        } else if has("stereo") {
            Self::Stereo
        } else if has("bookshelf") {
            Self::Bookshelf
        } else if has("mirror") {
            Self::Mirror
        } else if has("easel") && !has("canvas") {
            Self::Easel
        } else if has("guitar") && !has("decorations") {
            Self::Guitar
        } else if has("treadmill") || has("workout") {
            Self::Treadmill
        } else if has("chess") {
            Self::Chess
        } else if has("tables.") || has("counters.") {
            Self::Table
        } else if has("lighting") {
            Self::Light
        } else if has("flora") || n.contains("plant") || has("tree") {
            Self::Plant
        } else if has("decorations") {
            Self::Decoration
        } else {
            Self::Other
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Fridge | Self::Stove | Self::Microwave => "Appliances",
            Self::BedDouble | Self::BedSingle => "Beds",
            Self::Toilet | Self::Shower | Self::Bathtub | Self::Sink => "Plumbing",
            Self::Sofa | Self::Chair => "Seating",
            Self::Tv | Self::Computer | Self::Stereo => "Electronics",
            Self::Bookshelf | Self::Mirror | Self::Easel | Self::Guitar | Self::Treadmill | Self::Chess => "Hobbies",
            Self::Table => "Surfaces",
            Self::Light => "Lighting",
            Self::Plant | Self::Decoration => "Decor",
            Self::Other => "Misc",
        }
    }
}

/// A placed, interactive catalog object.
#[derive(Component, Clone)]
pub struct GameObject {
    pub kind: ObjectKind,
    pub name: String,
    pub objd: Key,
    pub price: i32,
    /// Local XZ bounds centre and half extents.
    pub center: Vec2,
    pub half: Vec2,
    pub height: f32,
}

impl GameObject {
    /// Where a sim stands to use the object (in front, along local +Z).
    pub fn use_point(&self, tf: &Transform) -> Vec2 {
        let local = Vec3::new(self.center.x, 0.0, self.center.y + self.half.y + 0.45);
        let p = tf.transform_point(local);
        Vec2::new(p.x, p.z)
    }

    pub fn world_center(&self, tf: &Transform) -> Vec3 {
        tf.transform_point(Vec3::new(self.center.x, 0.0, self.center.y))
    }

    /// Height of the seat / mattress surface.
    pub fn seat_height(&self) -> f32 {
        match self.kind {
            ObjectKind::BedDouble | ObjectKind::BedSingle => self.height.min(0.75).max(0.45),
            ObjectKind::Bathtub => 0.25,
            _ => 0.45,
        }
    }
}

#[derive(Component, Default)]
pub struct UsedBy(pub Option<Entity>);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    None,
    FindJob,
    QuitJob,
    SellPainting,
    Cook,
}

pub struct InteractionDef {
    pub name: &'static str,
    pub minutes: f32,
    pub per_hour: [f32; 6],
    pub until_full: Option<usize>,
    pub pose: Pose,
    pub on_object: bool,
    pub autonomous: bool,
    pub decay: [f32; 6],
    pub skill: Option<&'static str>,
    pub special: Special,
}

const N: [f32; 6] = [0.0; 6];
const D1: [f32; 6] = [1.0; 6];
const SLEEP_DECAY: [f32; 6] = [0.35, 0.35, 0.0, 0.3, 0.4, 0.2];

const fn def(name: &'static str, minutes: f32, per_hour: [f32; 6], pose: Pose) -> InteractionDef {
    InteractionDef {
        name,
        minutes,
        per_hour,
        until_full: None,
        pose,
        on_object: false,
        autonomous: true,
        decay: D1,
        skill: None,
        special: Special::None,
    }
}

static FRIDGE: [InteractionDef; 2] = [
    InteractionDef { special: Special::Cook, ..def("Have Quick Meal", 30.0, [130.0, -6.0, 0.0, 0.0, -4.0, 4.0], Pose::Use) },
    def("Grab a Snack", 12.0, [140.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Use),
];
static STOVE: [InteractionDef; 1] = [InteractionDef {
    skill: Some("Cooking"),
    special: Special::Cook,
    ..def("Cook Dinner", 60.0, [100.0, 0.0, 0.0, 0.0, -5.0, 10.0], Pose::Use)
}];
static MICROWAVE: [InteractionDef; 1] = [def("Microwave Dinner", 20.0, [150.0, 0.0, 0.0, 0.0, 0.0, 0.0], Pose::Use)];
static BED: [InteractionDef; 3] = [
    InteractionDef {
        until_full: Some(ENERGY),
        on_object: true,
        decay: SLEEP_DECAY,
        ..def("Sleep", 600.0, [0.0, 0.0, 24.0, 0.0, 0.0, 0.0], Pose::Lie)
    },
    InteractionDef { on_object: true, decay: SLEEP_DECAY, ..def("Nap", 90.0, [0.0, 0.0, 26.0, 0.0, 0.0, 0.0], Pose::Lie) },
    InteractionDef { on_object: true, ..def("Relax", 45.0, [0.0, 0.0, 6.0, 0.0, 0.0, 14.0], Pose::Lie) },
];
static TOILET: [InteractionDef; 1] =
    [InteractionDef { until_full: Some(BLADDER), on_object: true, ..def("Use Toilet", 15.0, [0.0, 900.0, 0.0, 0.0, -10.0, 0.0], Pose::Sit) }];
static SHOWER: [InteractionDef; 1] =
    [InteractionDef { until_full: Some(HYGIENE), ..def("Take Shower", 30.0, [0.0, 0.0, 0.0, 0.0, 400.0, 10.0], Pose::Use) }];
static BATHTUB: [InteractionDef; 1] = [InteractionDef {
    until_full: Some(HYGIENE),
    on_object: true,
    ..def("Take Bath", 50.0, [0.0, 0.0, 6.0, 0.0, 260.0, 40.0], Pose::Lie)
}];
static SINK: [InteractionDef; 1] = [def("Wash Hands", 6.0, [0.0, 0.0, 0.0, 0.0, 160.0, 0.0], Pose::Use)];
static SOFA: [InteractionDef; 2] = [
    InteractionDef { on_object: true, ..def("Sit", 40.0, [0.0, 0.0, 5.0, 0.0, 0.0, 10.0], Pose::Sit) },
    InteractionDef { on_object: true, decay: SLEEP_DECAY, ..def("Nap", 60.0, [0.0, 0.0, 18.0, 0.0, 0.0, 0.0], Pose::Lie) },
];
static CHAIR: [InteractionDef; 1] =
    [InteractionDef { on_object: true, autonomous: false, ..def("Sit", 30.0, [0.0, 0.0, 4.0, 0.0, 0.0, 4.0], Pose::Sit) }];
static TV: [InteractionDef; 2] = [
    def("Watch TV", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 55.0], Pose::Stand),
    def("Watch Cooking Channel", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 35.0], Pose::Stand),
];
static COMPUTER: [InteractionDef; 4] = [
    def("Play Computer Games", 60.0, [0.0, 0.0, -3.0, 0.0, 0.0, 60.0], Pose::Use),
    InteractionDef { autonomous: false, skill: Some("Writing"), ..def("Write Novel", 90.0, [0.0, 0.0, -4.0, 0.0, 0.0, 10.0], Pose::Use) },
    InteractionDef { autonomous: false, special: Special::FindJob, ..def("Find a Job", 20.0, N, Pose::Use) },
    InteractionDef { autonomous: false, special: Special::QuitJob, ..def("Quit Job", 5.0, N, Pose::Use) },
];
static STEREO: [InteractionDef; 1] = [def("Dance", 45.0, [0.0, 0.0, -6.0, 0.0, -6.0, 70.0], Pose::Dance)];
static BOOKSHELF: [InteractionDef; 1] =
    [InteractionDef { skill: Some("Logic"), ..def("Read a Book", 60.0, [0.0, 0.0, 0.0, 0.0, 0.0, 30.0], Pose::Stand) }];
static MIRROR: [InteractionDef; 1] = [InteractionDef {
    autonomous: false,
    skill: Some("Charisma"),
    ..def("Practice Speech", 40.0, [0.0, 0.0, -2.0, 6.0, 0.0, 10.0], Pose::Talk)
}];
static EASEL: [InteractionDef; 1] = [InteractionDef {
    skill: Some("Painting"),
    special: Special::SellPainting,
    ..def("Paint", 90.0, [0.0, 0.0, -3.0, 0.0, 0.0, 40.0], Pose::Use)
}];
static GUITAR: [InteractionDef; 1] =
    [InteractionDef { skill: Some("Guitar"), ..def("Play Guitar", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 45.0], Pose::Use) }];
static TREADMILL: [InteractionDef; 1] = [InteractionDef {
    autonomous: false,
    skill: Some("Athletic"),
    ..def("Work Out", 60.0, [-10.0, 0.0, -25.0, 0.0, -40.0, 5.0], Pose::Exercise)
}];
static CHESS: [InteractionDef; 1] =
    [InteractionDef { skill: Some("Logic"), ..def("Play Chess", 60.0, [0.0, 0.0, -2.0, 0.0, 0.0, 40.0], Pose::Use) }];

/// The animation clip played while performing an interaction.
pub fn interaction_clip(name: &str) -> Option<&'static str> {
    Some(match name {
        "Have Quick Meal" | "Grab a Snack" | "Microwave Dinner" => "a2o_fridge_openDoor_x",
        "Cook Dinner" => "a2o_stove_clean_loop_x",
        "Use Toilet" | "Sit" => "a2o_sitTemplate_sit_loopBreathe",
        "Take Shower" => "a2o_shower_takeShower_loop1_x",
        "Wash Hands" => "a2o_sink_brushTeeth_Loop1_x",
        "Watch TV" | "Watch Cooking Channel" => "a_idle_neutral_loop_3",
        "Play Computer Games" => "a2o_computer_game_loop1_counter_x",
        "Write Novel" | "Find a Job" | "Quit Job" => "a2o_computer_chess_type_loop_counter_x",
        "Dance" => "a_dance_beg_posAHeadBob_x",
        "Read a Book" => "a2o_book_readBook_standing_loopRead_x",
        "Practice Speech" => "a2o_mirror_full_checkSelfOut_loop1_x",
        "Paint" => "a2o_holographicEasel_loopMed_1_x",
        "Play Guitar" => "a2o_guitar_play_high_loop1_x",
        "Work Out" => "a2o_treadmill_jog_loop_x",
        "Play Chess" => "a2o_chessTable_loop1_x",
        "Sleep" | "Nap" | "Relax" | "Take Bath" => "a2o_bed_sleep_back_loop_x",
        _ => return None,
    })
}

pub fn interactions_for(kind: ObjectKind) -> &'static [InteractionDef] {
    match kind {
        ObjectKind::Fridge => &FRIDGE,
        ObjectKind::Stove => &STOVE,
        ObjectKind::Microwave => &MICROWAVE,
        ObjectKind::BedDouble | ObjectKind::BedSingle => &BED,
        ObjectKind::Toilet => &TOILET,
        ObjectKind::Shower => &SHOWER,
        ObjectKind::Bathtub => &BATHTUB,
        ObjectKind::Sink => &SINK,
        ObjectKind::Sofa => &SOFA,
        ObjectKind::Chair => &CHAIR,
        ObjectKind::Tv => &TV,
        ObjectKind::Computer => &COMPUTER,
        ObjectKind::Stereo => &STEREO,
        ObjectKind::Bookshelf => &BOOKSHELF,
        ObjectKind::Mirror => &MIRROR,
        ObjectKind::Easel => &EASEL,
        ObjectKind::Guitar => &GUITAR,
        ObjectKind::Treadmill => &TREADMILL,
        ObjectKind::Chess => &CHESS,
        _ => &[],
    }
}

// ---------------------------------------------------------------------------------------------
// Socials

pub struct SocialDef {
    pub name: &'static str,
    pub minutes: f32,
    pub social_per_hour: f32,
    pub fun_per_hour: f32,
    pub relationship: f32,
    pub min_rel: f32,
    pub autonomous: bool,
}

pub static SOCIALS: [SocialDef; 7] = [
    SocialDef { name: "Chat", minutes: 25.0, social_per_hour: 110.0, fun_per_hour: 10.0, relationship: 8.0, min_rel: -100.0, autonomous: true },
    SocialDef { name: "Tell Joke", minutes: 12.0, social_per_hour: 90.0, fun_per_hour: 80.0, relationship: 6.0, min_rel: -30.0, autonomous: true },
    SocialDef { name: "Compliment", minutes: 8.0, social_per_hour: 80.0, fun_per_hour: 0.0, relationship: 7.0, min_rel: -100.0, autonomous: false },
    SocialDef { name: "Hug", minutes: 6.0, social_per_hour: 140.0, fun_per_hour: 20.0, relationship: 8.0, min_rel: 30.0, autonomous: false },
    SocialDef { name: "Dance Together", minutes: 30.0, social_per_hour: 80.0, fun_per_hour: 80.0, relationship: 10.0, min_rel: 20.0, autonomous: false },
    SocialDef { name: "Kiss", minutes: 6.0, social_per_hour: 160.0, fun_per_hour: 40.0, relationship: 12.0, min_rel: 60.0, autonomous: false },
    SocialDef { name: "Argue", minutes: 15.0, social_per_hour: 60.0, fun_per_hour: -60.0, relationship: -15.0, min_rel: -100.0, autonomous: false },
];

// ---------------------------------------------------------------------------------------------
// Actions

#[derive(Clone, Debug)]
pub enum ActionKind {
    Object { target: Entity, def: usize },
    Social { target: Entity, social: usize },
    GoHere(Vec2),
    GoToWork,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Start,
    Routing,
    Running(f32),
}

#[derive(Clone, Debug)]
pub struct Action {
    pub label: String,
    pub kind: ActionKind,
    pub phase: Phase,
    pub autonomous: bool,
    pub cancel: bool,
}

impl Action {
    pub fn new(label: impl Into<String>, kind: ActionKind, autonomous: bool) -> Self {
        Self { label: label.into(), kind, phase: Phase::Start, autonomous, cancel: false }
    }
}

#[derive(Component, Default)]
pub struct ActionQueue(pub VecDeque<Action>);

impl ActionQueue {
    /// Queues a player-chosen action, dropping autonomous ones first like the original.
    pub fn push_player(&mut self, a: Action) {
        for q in self.0.iter_mut() {
            if q.autonomous {
                q.cancel = true;
            }
        }
        if self.0.len() < 8 {
            self.0.push_back(a);
        }
    }
}

#[derive(Component)]
pub struct AutonomyTimer(pub f32);

#[derive(Component, Default)]
pub struct Skills(pub HashMap<&'static str, f32>);

impl Skills {
    pub fn level(&self, s: &str) -> u32 {
        (self.0.get(s).copied().unwrap_or(0.0) as u32).min(10)
    }
}

#[derive(Clone)]
pub struct Career {
    pub track: &'static str,
    pub title: &'static str,
    pub hourly: i64,
    pub start: f32,
    pub end: f32,
}

const CAREERS: [Career; 6] = [
    Career { track: "Culinary", title: "Dishwasher", hourly: 18, start: 15.0, end: 21.0 },
    Career { track: "Business", title: "Office Assistant", hourly: 23, start: 9.0, end: 16.0 },
    Career { track: "Law Enforcement", title: "Desk Jockey", hourly: 21, start: 9.0, end: 15.0 },
    Career { track: "Science", title: "Test Subject", hourly: 22, start: 9.0, end: 15.0 },
    Career { track: "Medical", title: "Orderly", hourly: 24, start: 8.0, end: 15.0 },
    Career { track: "Music", title: "Roadie", hourly: 20, start: 11.0, end: 18.0 },
];

#[derive(Component, Clone)]
pub struct Job(pub Career);

#[derive(Component)]
pub struct AtWork {
    pub until: f64,
}

#[derive(Resource)]
pub struct Household {
    pub name: String,
    pub funds: i64,
    pub lot_index: usize,
    pub last_bill_day: u32,
}

#[derive(Resource, Default)]
pub struct Notifications(pub Vec<(String, f32)>);

impl Notifications {
    pub fn push(&mut self, s: impl Into<String>) {
        let s = s.into();
        info!("TNS: {s}");
        self.0.push((s, 9.0));
        if self.0.len() > 5 {
            self.0.remove(0);
        }
    }
}

/// The home lot's walk-off point where sims leave for work and carpools arrive.
#[derive(Resource, Clone, Copy)]
pub struct LotExit(pub Vec2);

// ---------------------------------------------------------------------------------------------
// Systems

#[allow(clippy::type_complexity)]
fn run_actions(
    mut commands: Commands,
    delta: Res<SimDelta>,
    clock: Res<GameClock>,
    grid: Option<Res<NavGrid>>,
    world: Res<CurrentWorld>,
    exit: Option<Res<LotExit>>,
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    mut sims: Query<
        (
            Entity,
            &Sim,
            &mut ActionQueue,
            &mut Transform,
            &mut Motives,
            &mut DecayScale,
            &mut SimAnim,
            &mut Skills,
            &mut Relationships,
            Option<&mut PathFollow>,
            Option<&Job>,
        ),
        (Without<GameObject>, Without<AtWork>),
    >,
    mut objects: Query<(&GameObject, &Transform, &mut UsedBy), Without<Sim>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
) {
    let Some(grid) = grid else { return };
    let dt = delta.0;
    let ground = |x: f32, z: f32| crate::building::walk_height(&world.data, building.as_deref(), Vec3::new(x, 0.0, z));
    // Social effects to apply to partners after the main pass: (target, actor, social, fun, rel, pose talk)
    let mut social_fx: Vec<(Entity, Entity, f32, f32, f32)> = Vec::new();
    let positions: HashMap<Entity, Vec3> = sims.iter().map(|s| (s.0, s.3.translation)).collect();

    for (me, sim, mut queue, mut tf, mut motives, mut decay, mut anim, mut skills, mut rels, path, job) in &mut sims {
        let Some(action) = queue.0.front_mut() else {
            if anim.pose != Pose::Walk && anim.pose != Pose::Stand && path.is_none() {
                anim.pose = Pose::Stand;
            }
            continue;
        };
        let mut finished = false;
        let mut stand_up_at: Option<Vec2> = None;

        // Cancellation
        if action.cancel {
            if let ActionKind::Object { target, .. } = action.kind
                && let Ok((obj, otf, mut used)) = objects.get_mut(target)
            {
                if used.0 == Some(me) {
                    used.0 = None;
                }
                if matches!(action.phase, Phase::Running(_)) {
                    stand_up_at = Some(obj.use_point(otf));
                }
            }
            finished = true;
        }

        if !finished {
            match action.phase {
                Phase::Start => {
                    let dest: Option<Vec2> = match &action.kind {
                        ActionKind::Object { target, def } => match objects.get(*target) {
                            Ok((obj, otf, used)) => {
                                if used.0.is_some_and(|u| u != me) {
                                    if !action.autonomous {
                                        notes.push(format!("{} can't use the {}: it's in use.", sim.first, obj.name));
                                    }
                                    None
                                } else {
                                    let _ = def;
                                    Some(obj.use_point(otf))
                                }
                            }
                            Err(_) => None,
                        },
                        ActionKind::Social { target, .. } => positions.get(target).map(|p| {
                            let mine = Vec2::new(tf.translation.x, tf.translation.z);
                            let theirs = Vec2::new(p.x, p.z);
                            theirs + (mine - theirs).normalize_or(Vec2::X) * 0.9
                        }),
                        ActionKind::GoHere(p) => Some(*p),
                        ActionKind::GoToWork => exit.as_ref().map(|e| e.0),
                    };
                    match dest.and_then(|d| grid.find_path(Vec2::new(tf.translation.x, tf.translation.z), d)) {
                        Some(wp) => {
                            commands.entity(me).insert(PathFollow::new(wp));
                            action.phase = Phase::Routing;
                            if let ActionKind::Object { target, .. } = action.kind
                                && let Ok((_, _, mut used)) = objects.get_mut(target)
                            {
                                used.0 = Some(me);
                            }
                        }
                        None => {
                            if dest.is_some() && !action.autonomous {
                                notes.push(format!("{} can't find a way to get there.", sim.first));
                            }
                            finished = true;
                        }
                    }
                }
                Phase::Routing => {
                    let arrived = path.as_ref().is_some_and(|p| p.done);
                    if arrived {
                        commands.entity(me).remove::<PathFollow>();
                        action.phase = Phase::Running(0.0);
                        match &action.kind {
                            ActionKind::Object { target, def } => {
                                if let Ok((obj, otf, _)) = objects.get(*target) {
                                    let d = &interactions_for(obj.kind)[*def];
                                    anim.pose = d.pose;
                                    *decay = DecayScale(d.decay);
                                    if let Some(c) = interaction_clip(d.name) {
                                        commands.entity(me).insert(crate::anim::ActionClip(c));
                                    }
                                    let face = otf.rotation * Quat::from_rotation_y(std::f32::consts::PI);
                                    if d.on_object {
                                        let c = obj.world_center(otf);
                                        tf.translation = Vec3::new(c.x, otf.translation.y, c.z);
                                        tf.rotation = otf.rotation;
                                        anim.seat_height = obj.seat_height();
                                    } else {
                                        tf.rotation = face;
                                        anim.seat_height = 0.0;
                                    }
                                }
                            }
                            ActionKind::Social { target, .. } => {
                                if let Some(p) = positions.get(target) {
                                    let to = Vec2::new(p.x - tf.translation.x, p.z - tf.translation.z);
                                    tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                                }
                                anim.pose = Pose::Talk;
                            }
                            ActionKind::GoHere(_) => finished = true,
                            ActionKind::GoToWork => {
                                if let Some(j) = job {
                                    let day_start = (clock.minutes / 1440.0).floor() * 1440.0;
                                    let until = day_start + j.0.end as f64 * 60.0;
                                    commands.entity(me).insert((AtWork { until }, Visibility::Hidden));
                                    notes.push(format!("{} left for work as a {}.", sim.first, j.0.title));
                                }
                                finished = true;
                            }
                        }
                    } else if path.is_none() {
                        action.phase = Phase::Start;
                    }
                }
                Phase::Running(elapsed) => {
                    let elapsed = elapsed + dt;
                    action.phase = Phase::Running(elapsed);
                    match &action.kind {
                        ActionKind::Object { target, def } => {
                            if let Ok((obj, otf, mut used)) = objects.get_mut(*target) {
                                let d = &interactions_for(obj.kind)[*def];
                                for i in 0..6 {
                                    motives.add(i, d.per_hour[i] * dt / 60.0);
                                }
                                if let Some(sk) = d.skill {
                                    let e = skills.0.entry(sk).or_insert(0.0);
                                    let before = *e as u32;
                                    *e = (*e + dt / 60.0 * 0.6 / (1.0 + *e * 0.25)).min(10.0);
                                    if *e as u32 > before {
                                        notes.push(format!("{} reached level {} in {}!", sim.first, *e as u32, sk));
                                    }
                                }
                                let full = d.until_full.is_some_and(|m| motives.0[m] >= 98.0);
                                if elapsed >= d.minutes || full {
                                    finished = true;
                                    used.0 = None;
                                    if d.on_object {
                                        stand_up_at = Some(obj.use_point(otf));
                                    }
                                    match d.special {
                                        Special::FindJob => {
                                            let c = CAREERS[rand::rng().random_range(0..CAREERS.len())].clone();
                                            notes.push(format!(
                                                "{} joined the {} career as a {} (§{}/hr, {}–{}).",
                                                sim.first,
                                                c.track,
                                                c.title,
                                                c.hourly,
                                                hour_label(c.start),
                                                hour_label(c.end)
                                            ));
                                            commands.entity(me).insert(Job(c));
                                        }
                                        Special::QuitJob => {
                                            if job.is_some() {
                                                commands.entity(me).remove::<Job>();
                                                notes.push(format!("{} quit their job.", sim.first));
                                            }
                                        }
                                        Special::SellPainting => {
                                            let lvl = skills.level("Painting") as i64;
                                            let value = 15 + lvl * lvl * 12 + rand::rng().random_range(0..20);
                                            if let Some(h) = household.as_mut() {
                                                h.funds += value;
                                            }
                                            notes.push(format!("{} finished a painting and sold it for §{value}.", sim.first));
                                        }
                                        Special::Cook | Special::None => {}
                                    }
                                }
                            } else {
                                finished = true;
                            }
                        }
                        ActionKind::Social { target, social } => {
                            let s = &SOCIALS[*social];
                            let close = positions.get(target).is_some_and(|p| p.distance(tf.translation) < 2.5);
                            if !close {
                                action.phase = Phase::Start;
                            } else {
                                motives.add(SOCIAL, s.social_per_hour * dt / 60.0);
                                motives.add(FUN, s.fun_per_hour * dt / 60.0);
                                let rel = s.relationship * dt / s.minutes;
                                rels.add(*target, rel);
                                social_fx.push((*target, me, s.social_per_hour * dt / 60.0, s.fun_per_hour * dt / 60.0, rel));
                                anim.pose = if s.name.contains("Dance") { Pose::Dance } else { Pose::Talk };
                                if elapsed >= s.minutes {
                                    finished = true;
                                }
                            }
                        }
                        _ => finished = true,
                    }
                }
            }
        }

        if finished {
            commands.entity(me).remove::<crate::anim::ActionClip>();
            queue.0.pop_front();
            *decay = DecayScale::default();
            anim.pose = Pose::Stand;
            anim.seat_height = 0.0;
            commands.entity(me).remove::<PathFollow>();
            if let Some(p) = stand_up_at {
                tf.translation = Vec3::new(p.x, ground(p.x, p.y), p.y);
            }
        }
    }

    for (target, actor, social, fun, rel) in social_fx {
        if let Ok((_, _, queue, mut tf, mut motives, _, mut anim, _, mut rels, path, _)) = sims.get_mut(target) {
            motives.add(SOCIAL, social);
            motives.add(FUN, fun);
            rels.add(actor, rel);
            if queue.0.is_empty() && path.is_none() {
                anim.pose = Pose::Talk;
                if let Some(p) = positions.get(&actor) {
                    let to = Vec2::new(p.x - tf.translation.x, p.z - tf.translation.z);
                    tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                }
            }
        }
    }
}

pub fn hour_label(h: f32) -> String {
    let h = h as u32;
    match h {
        0 => "12 AM".into(),
        1..=11 => format!("{h} AM"),
        12 => "12 PM".into(),
        _ => format!("{} PM", h - 12),
    }
}

/// Sims with free will pick something to satisfy their lowest needs when idle.
#[allow(clippy::type_complexity)]
fn autonomy(
    delta: Res<SimDelta>,
    clock: Res<GameClock>,
    mut sims: Query<
        (Entity, &Transform, &Motives, &mut ActionQueue, &mut AutonomyTimer, &Relationships, Option<&Job>),
        Without<AtWork>,
    >,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
) {
    if delta.0 <= 0.0 {
        return;
    }
    let others: Vec<(Entity, Vec3)> = sims.iter().map(|s| (s.0, s.1.translation)).collect();
    let mut rng = rand::rng();
    for (me, tf, motives, mut queue, mut timer, rels, job) in &mut sims {
        timer.0 -= delta.0;
        if timer.0 > 0.0 || !queue.0.is_empty() {
            continue;
        }
        timer.0 = rng.random_range(4.0..10.0);
        // Don't start long activities right before work.
        if let Some(j) = job {
            let h = clock.hour_f();
            if clock.is_workday() && h > j.0.start - 1.2 && h < j.0.start {
                continue;
            }
        }
        let urgency = |i: usize| {
            let v = motives.0[i];
            let u = ((100.0 - v) / 200.0).clamp(0.0, 1.0);
            u * u * u * 4.0 + u * 0.3
        };
        let mut best: Option<(f32, Action)> = None;
        for (oe, obj, otf, used) in &objects {
            if used.0.is_some_and(|u| u != me) {
                continue;
            }
            let dist = obj.world_center(otf).distance(tf.translation);
            for (di, d) in interactions_for(obj.kind).iter().enumerate() {
                if !d.autonomous {
                    continue;
                }
                let mut score = 0.0;
                for i in 0..6 {
                    let gain = d.per_hour[i] * d.minutes / 60.0;
                    let room = (100.0 - motives.0[i]).max(0.0);
                    score += gain.min(room).max(-50.0) * urgency(i);
                }
                // Only sleep when actually tired.
                if d.until_full == Some(ENERGY) && motives.0[ENERGY] > -10.0 {
                    score *= 0.1;
                }
                score /= 1.0 + dist / 25.0;
                score *= rng.random_range(0.85..1.15);
                if best.as_ref().is_none_or(|b| score > b.0) {
                    best = Some((score, Action::new(d.name, ActionKind::Object { target: oe, def: di }, true)));
                }
            }
        }
        if motives.0[SOCIAL] < 30.0 {
            for &(other, _) in &others {
                if other == me {
                    continue;
                }
                let rel = rels.get(other);
                let si = if rel > 10.0 && rng.random_bool(0.4) { 1 } else { 0 };
                let s = &SOCIALS[si];
                let score = (s.social_per_hour * s.minutes / 60.0) * urgency(SOCIAL) * rng.random_range(0.8..1.2);
                if best.as_ref().is_none_or(|b| score > b.0) {
                    best = Some((score, Action::new(s.name, ActionKind::Social { target: other, social: si }, true)));
                }
            }
        }
        if let Some((score, action)) = best
            && score > 2.0
        {
            queue.0.push_back(action);
        }
    }
}

/// Careers: head to work an hour before the shift, come home with pay.
fn work_schedule(
    mut commands: Commands,
    clock: Res<GameClock>,
    exit: Option<Res<LotExit>>,
    world: Res<CurrentWorld>,
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    mut workers: Query<(Entity, &Sim, &Job, &mut ActionQueue, Option<&AtWork>, &mut Transform, &mut Motives)>,
) {
    let h = clock.hour_f();
    for (e, sim, job, mut queue, at_work, mut tf, mut motives) in &mut workers {
        if let Some(w) = at_work {
            // Being at work is tiring but social.
            if clock.minutes >= w.until {
                let hours = (job.0.end - job.0.start) as i64;
                let pay = job.0.hourly * hours;
                if let Some(hh) = household.as_mut() {
                    hh.funds += pay;
                }
                notes.push(format!("{} is home from work and earned §{pay}.", sim.first));
                motives.0[HUNGER] = (motives.0[HUNGER] - 35.0).max(-90.0);
                motives.0[ENERGY] = (motives.0[ENERGY] - 30.0).max(-80.0);
                motives.0[FUN] = (motives.0[FUN] - 25.0).max(-80.0);
                motives.0[SOCIAL] = (motives.0[SOCIAL] + 40.0).min(100.0);
                motives.0[HYGIENE] = (motives.0[HYGIENE] - 20.0).max(-80.0);
                if let Some(x) = &exit {
                    tf.translation = Vec3::new(x.0.x, world.data.heightmap.sample(x.0.x, x.0.y), x.0.y);
                }
                commands.entity(e).remove::<AtWork>().insert(Visibility::Inherited);
            }
            continue;
        }
        let going = queue.0.iter().any(|a| matches!(a.kind, ActionKind::GoToWork));
        if clock.is_workday() && h >= job.0.start - 0.75 && h < job.0.start + 1.0 && !going {
            // Only once per day: skip if the shift already started more than an hour ago.
            for a in queue.0.iter_mut() {
                a.cancel = true;
            }
            queue.0.push_back(Action::new("Go to Work", ActionKind::GoToWork, false));
        }
    }
}

fn motive_warnings(
    clock: Res<GameClock>,
    mut last_check: Local<f64>,
    mut notes: ResMut<Notifications>,
    sims: Query<(&Sim, &Motives), With<HouseholdMember>>,
    mut warned: Local<HashMap<(String, usize), f64>>,
) {
    if clock.minutes - *last_check < 15.0 {
        return;
    }
    *last_check = clock.minutes;
    let msgs = [
        "is starving!",
        "really needs to use the bathroom!",
        "is exhausted!",
        "is lonely!",
        "is filthy!",
        "is bored out of their mind!",
    ];
    for (sim, m) in &sims {
        for i in 0..6 {
            if m.0[i] < -75.0 {
                let key = (sim.full_name(), i);
                let last = warned.get(&key).copied().unwrap_or(-1e9);
                if clock.minutes - last > 180.0 {
                    warned.insert(key, clock.minutes);
                    notes.push(format!("{} {}", sim.first, msgs[i]));
                }
            }
        }
    }
}

fn pay_bills(
    clock: Res<GameClock>,
    household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    objects: Query<&GameObject>,
) {
    let Some(mut h) = household else { return };
    let day = clock.day();
    // Bills arrive Monday and Thursday mornings.
    if day != h.last_bill_day && (day % 7 == 0 || day % 7 == 3) && clock.hour_f() >= 9.0 {
        h.last_bill_day = day;
        if day == 0 {
            return;
        }
        let value: i64 = objects.iter().map(|o| o.price as i64).sum();
        let bill = 60 + value / 60;
        h.funds -= bill;
        notes.push(format!("The bills arrived: §{bill} was paid automatically."));
    }
}
