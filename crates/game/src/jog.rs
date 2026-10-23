//! Going jogging (clicking the Sim themselves): out to the street and up and down its sidewalk
//! at a jog, the game's jogging clip, for an hour or until they're worn out, then home. It
//! trains Athletic, builds fitness, and counts towards the skill journal's cardio and
//! Marathon Runner (the game's six kilometres an hour of running). "Jog Here" (the game's
//! `JogHere`) does the same on the way to a place clicked, twenty metres off or more, for those
//! with a level of Athletic (the fast jog from level five).

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::SimDelta;
use crate::interact::{Action, ActionKind, ActionQueue, LotExit, Notifications, Skills};
use crate::life::{LifeEvent, LifeEventKind};
use crate::nav::{PathFollow, Waypoint};
use crate::sim::{Motives, Sim, ENERGY, FUN, HYGIENE};

pub struct JogPlugin;

impl Plugin for JogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (jog, jog_here).run_if(in_state(PlayMode::Live)));
    }
}

/// A jog under way: from where, since when, which leg of the street it's on.
#[derive(Component)]
pub struct Jogging {
    pub home: Vec2,
    pub since: f64,
    leg: u32,
    returning: bool,
}

impl Jogging {
    pub fn new(home: Vec2, since: f64) -> Self {
        Self { home, since, leg: 0, returning: false }
    }
}

/// How long a jog lasts (game minutes; at the game's jogging pace).
const JOG_MINUTES: f64 = 60.0;
/// The game's kilometres of running an hour (`kDistanceJoggedPerHourOfRunning`).
pub const KM_PER_HOUR: f64 = 6.0;
/// Athletic skill points an hour of it (the game's `JogHere_Terrain`).
const JOG_POINTS: f32 = 600.0;

/// Who can go jogging: teens and grown-ups.
pub fn can_jog(sim: &Sim) -> bool {
    !sim.age.is_little() && sim.age != crate::sim::Age::Child
}

/// The game's `JogHere` tuning: the Athletic level to jog somewhere, to do it at the fast jog, and
/// how far off it must be (metres).
pub const JOG_HERE_LEVEL: u32 = 1;
pub const FAST_JOG_LEVEL: u32 = 5;
pub const JOG_HERE_DISTANCE: f32 = 20.0;

/// Whether a Sim can "Jog Here" to a place `from` where they are.
pub fn can_jog_here(sim: &Sim, athletic: u32, from: Vec3, to: Vec3) -> bool {
    can_jog(sim) && athletic >= JOG_HERE_LEVEL && from.xz().distance(to.xz()) >= JOG_HERE_DISTANCE
}

/// An hour's (or so) exercise at a jog: tiring (not for a Fitness Nut) and sweaty, good fun for
/// the athletic, Athletic learned at the game's rate, fitness, and the journal's running.
#[allow(clippy::too_many_arguments)]
fn exercise(
    commands: &mut Commands,
    e: Entity,
    sim: &Sim,
    h: f32,
    motives: &mut Motives,
    skills: &mut Skills,
    (journal, wishes, mood): (Option<&crate::journal::SkillJournal>, Option<&crate::wishes::Wishes>, Option<&crate::life::Mood>),
    did: &mut MessageWriter<crate::journal::Did>,
    life: &mut MessageWriter<LifeEvent>,
    notes: &mut Notifications,
) {
    if !crate::journal::earned(journal, "Fitness Nut") {
        motives.add(ENERGY, -35.0 * h);
    }
    motives.add(HYGIENE, -30.0 * h);
    motives.add(FUN, crate::life::activity_affinity(&sim.traits, "Work Out") * 10.0 * h);
    let rate = crate::life::skill_rate(&sim.traits, "Athletic") * crate::wishes::reward_skill_rate(wishes) * mood.map_or(1.0, |m| m.skill_rate());
    // (The game's JogHere_Terrain: 600 points an hour.)
    if let Some(level) = skills.add_points("Athletic", h * JOG_POINTS * rate) {
        notes.push(format!("{} reached level {level} in Athletic!", sim.first));
        life.write(LifeEvent::new(e, LifeEventKind::SkillUp { skill: "Athletic", level }));
    }
    let burn = if crate::wishes::has(wishes, "FastMetabolism") { 1.25 } else { 1.0 };
    commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| crate::aging::reshape(&mut w, -0.03 * h * burn, 0.05 * h));
    did.write(crate::journal::Did::count(e, crate::journal::Stat::KmJogged, KM_PER_HOUR * h as f64));
    did.write(crate::journal::Did::count(e, crate::journal::Stat::CardioHours, h as f64));
}

/// On the way somewhere at a jog ("Jog Here"): the exercise of it, while they go.
#[allow(clippy::type_complexity)]
fn jog_here(
    mut commands: Commands,
    delta: Res<SimDelta>,
    mut sims: Query<(Entity, &Sim, &ActionQueue, &PathFollow, &mut Motives, &mut Skills, Option<&crate::journal::SkillJournal>, Option<&crate::wishes::Wishes>, Option<&crate::life::Mood>)>,
    mut did: MessageWriter<crate::journal::Did>,
    mut life: MessageWriter<LifeEvent>,
    mut notes: ResMut<Notifications>,
) {
    let h = delta.0 / 60.0;
    if h <= 0.0 {
        return;
    }
    for (e, sim, queue, path, mut motives, mut skills, journal, wishes, mood) in &mut sims {
        if path.done || !queue.0.front().is_some_and(|a| a.jog && matches!(a.phase, crate::interact::Phase::Routing)) {
            continue;
        }
        exercise(&mut commands, e, sim, h, &mut motives, &mut skills, (journal, wishes, mood), &mut did, &mut life, &mut notes);
    }
}

#[allow(clippy::type_complexity)]
fn jog(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    delta: Res<SimDelta>,
    sidewalk: Option<Res<crate::town::Sidewalk>>,
    exit: Option<Res<LotExit>>,
    mut sims: Query<(Entity, &Sim, &mut Jogging, &mut ActionQueue, Option<&PathFollow>, &mut Motives, &mut Skills, Option<&crate::journal::SkillJournal>, Option<&crate::wishes::Wishes>, Option<&crate::life::Mood>)>,
    mut did: MessageWriter<crate::journal::Did>,
    mut life: MessageWriter<LifeEvent>,
    mut notes: ResMut<Notifications>,
) {
    let dt = delta.0;
    for (e, sim, mut j, mut queue, path, mut motives, mut skills, journal, wishes, mood) in &mut sims {
        // Off to something else (or told to stop): no longer jogging.
        if !matches!(queue.current().map(|a| &a.kind), Some(ActionKind::Jog { .. })) {
            commands.entity(e).remove::<(Jogging, PathFollow)>();
            continue;
        }
        let (Some(walk), Some(exit)) = (sidewalk.as_deref(), exit.as_deref()) else { continue };
        if dt > 0.0 {
            exercise(&mut commands, e, sim, dt / 60.0, &mut motives, &mut skills, (journal, wishes, mood), &mut did, &mut life, &mut notes);
        }
        if path.is_some_and(|p| !p.done) {
            continue;
        }
        if j.returning {
            // Back at the lot's edge: the jog's done, and they walk back in.
            queue.0.pop_front();
            commands.entity(e).remove::<(Jogging, PathFollow)>();
            life.write(LifeEvent::new(e, LifeEventKind::Finished { activity: "Go Jogging", completed: true }));
            queue.0.push_front(Action::new("Go Here", ActionKind::GoHere(j.home, 1), true));
            continue;
        }
        let tired = motives.0[ENERGY] < -60.0 || motives.0[HYGIENE] < -70.0;
        let p = |q: Vec2| Waypoint { p: q, level: 1, climb: None, spiral: None };
        let ends = [walk.center - walk.along * walk.half_length * 0.85, walk.center + walk.along * walk.half_length * 0.85];
        let waypoints = if clock.minutes - j.since >= JOG_MINUTES || tired {
            j.returning = true;
            vec![p(exit.0)]
        } else {
            j.leg += 1;
            // (Onto the sidewalk first, then up and down it.)
            let mut w = Vec::new();
            if j.leg == 1 {
                let onto = walk.center + walk.along * (exit.0 - walk.center).dot(walk.along);
                w.push(p(onto));
            }
            w.push(p(ends[(j.leg % 2) as usize]));
            w
        };
        commands.entity(e).insert(PathFollow::new(waypoints).with_style(crate::nav::WalkStyle::Jog));
    }
}
