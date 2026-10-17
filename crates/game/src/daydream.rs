//! Daydreaming in bed, as the game's `DayDream`: offered while relaxing there (its
//! `RelaxingPosture`), the Sim lies on thinking of the household's Sims, one every six minutes
//! (`kFrequencyOfThoughtBalloon`), for its ten fun an hour till they've had their fill.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::interact::{ActionKind, ActionQueue, GameObject, Phase, Special, interactions_for};
use crate::sim::HouseholdMember;

pub struct DaydreamPlugin;

impl Plugin for DaydreamPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, daydreams.run_if(in_state(PlayMode::Live)));
    }
}

/// How the game names it.
pub const DAYDREAM: &str = "Daydream";
/// What it's offered from: relaxing in the bed.
pub const RELAX: &str = "Relax";
/// `kFrequencyOfThoughtBalloon` (game minutes).
const THOUGHT_MINUTES: f64 = 6.0;

/// Whether a Sim's relaxing in that bed (what's being done now, under way).
pub fn relaxing_in(queue: &ActionQueue, bed: Entity, kind: crate::interact::ObjectKind) -> bool {
    queue.0.front().is_some_and(|a| {
        !a.cancel
            && matches!(a.phase, Phase::Running(_))
            && matches!(a.kind, ActionKind::Object { target, def } if target == bed && interactions_for(kind).get(def).is_some_and(|d| d.name == RELAX))
    })
}

/// The relaxing turned to daydreaming, lying on as they are.
pub fn start(queue: &mut ActionQueue, daydream_def: usize) {
    if let Some(a) = queue.0.front_mut()
        && let ActionKind::Object { target, .. } = a.kind
    {
        a.kind = ActionKind::Object { target, def: daydream_def };
        a.label = DAYDREAM.to_string();
        a.autonomous = false;
    }
}

/// The daydreamers' thoughts: one of the household (themselves too) every few minutes.
fn daydreams(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut next: Local<HashMap<Entity, f64>>,
    sims: Query<(Entity, &ActionQueue)>,
    objects: Query<&GameObject>,
    household: Query<Entity, With<HouseholdMember>>,
) {
    let mut dreaming = Vec::new();
    for (me, q) in &sims {
        let Some(a) = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))) else { continue };
        let ActionKind::Object { target, def } = a.kind else { continue };
        if objects.get(target).is_ok_and(|o| interactions_for(o.kind).get(def).is_some_and(|d| d.special == Special::Daydream)) {
            dreaming.push(me);
        }
    }
    next.retain(|e, _| dreaming.contains(e));
    let family: Vec<Entity> = household.iter().collect();
    for me in dreaming {
        let due = next.entry(me).or_insert(clock.minutes);
        if clock.minutes < *due {
            continue;
        }
        *due = clock.minutes + THOUGHT_MINUTES;
        if let Some(&who) = family.choose(&mut rand::rng()) {
            commands.entity(me).insert(crate::balloons::BalloonRequest::thought(&crate::balloons::portrait(who)));
        }
    }
}
