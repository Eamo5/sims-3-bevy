//! Going jogging (clicking the Sim themselves): out to the street and up and down its sidewalk
//! at a jog, the game's jogging clip, for an hour or until they're worn out, then home. It
//! trains Athletic, builds fitness, and counts towards the skill journal's cardio and
//! Marathon Runner (the game's six kilometres an hour of running).

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
        app.add_systems(Update, jog.run_if(in_state(PlayMode::Live)));
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

/// Who can go jogging: teens and grown-ups.
pub fn can_jog(sim: &Sim) -> bool {
    !sim.age.is_little() && sim.age != crate::sim::Age::Child
}

#[allow(clippy::type_complexity)]
fn jog(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    delta: Res<SimDelta>,
    sidewalk: Option<Res<crate::town::Sidewalk>>,
    exit: Option<Res<LotExit>>,
    mut sims: Query<(Entity, &Sim, &mut Jogging, &mut ActionQueue, Option<&PathFollow>, &mut Motives, &mut Skills, Option<&crate::journal::SkillJournal>, Option<&crate::wishes::Wishes>)>,
    mut did: MessageWriter<crate::journal::Did>,
    mut life: MessageWriter<LifeEvent>,
    mut notes: ResMut<Notifications>,
) {
    let dt = delta.0;
    for (e, sim, mut j, mut queue, path, mut motives, mut skills, journal, wishes) in &mut sims {
        // Off to something else (or told to stop): no longer jogging.
        if !matches!(queue.current().map(|a| &a.kind), Some(ActionKind::Jog { .. })) {
            commands.entity(e).remove::<(Jogging, PathFollow)>();
            continue;
        }
        let (Some(walk), Some(exit)) = (sidewalk.as_deref(), exit.as_deref()) else { continue };
        if dt > 0.0 {
            let h = dt / 60.0;
            // Tiring (not for a Fitness Nut) and sweaty, and good fun for the athletic.
            if !crate::journal::earned(journal, "Fitness Nut") {
                motives.add(ENERGY, -35.0 * h);
            }
            motives.add(HYGIENE, -30.0 * h);
            motives.add(FUN, crate::life::activity_affinity(&sim.traits, "Work Out") * 10.0 * h);
            let rate = crate::life::skill_rate(&sim.traits, "Athletic") * crate::wishes::reward_skill_rate(wishes);
            let v = skills.0.entry("Athletic").or_insert(0.0);
            let before = *v as u32;
            *v = (*v + h * 0.6 * rate / (1.0 + *v * 0.25)).min(10.0);
            if *v as u32 > before {
                notes.push(format!("{} reached level {} in Athletic!", sim.first, *v as u32));
                life.write(LifeEvent::new(e, LifeEventKind::SkillUp { skill: "Athletic", level: *v as u32 }));
            }
            let burn = if crate::wishes::has(wishes, "FastMetabolism") { 1.25 } else { 1.0 };
            commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| crate::aging::reshape(&mut w, -0.03 * h * burn, 0.05 * h));
            did.write(crate::journal::Did::count(e, crate::journal::Stat::KmJogged, KM_PER_HOUR * h as f64));
            did.write(crate::journal::Did::count(e, crate::journal::Stat::CardioHours, h as f64));
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
        let p = |q: Vec2| Waypoint { p: q, level: 1, climb: None };
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
