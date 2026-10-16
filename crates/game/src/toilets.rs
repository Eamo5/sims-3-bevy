//! Toilet etiquette, as the game's `Toilet.UseToilet`: each use leaves the toilet needing a
//! flush (its `ToiletVolume`) unless they flush after, nine times in ten (a Neat Sim always,
//! a Slob never: `kFlushChanceNormal`, `kFlushChanceNeat`, `kFlushChanceSlob`), and then they
//! wash their hands at the nearest sink as often (`kWashHandsChance...`). A toilet left
//! unflushed can be flushed (`Toilet.Flush`: not by a Slob of their own accord), and until then
//! it's a mess about the place to all but the Slobs (`kNeedsToBeFlushedEnvironmentScorePenalty`).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Special, interactions_for};
use crate::life::Trait;
use crate::sim::Sim;

pub struct ToiletsPlugin;

impl Plugin for ToiletsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Unflushed>().add_systems(Update, (used, flushed).run_if(in_state(PlayMode::Live)));
    }
}

/// The toilets needing a flush, and how many uses since.
#[derive(Resource, Default)]
pub struct Unflushed(pub HashMap<Entity, u32>);

/// A toilet used (its outcome to be had): which, and whether by free will.
#[derive(Component)]
pub struct UsedToilet {
    pub toilet: Entity,
    pub autonomous: bool,
}

/// A toilet flushed.
#[derive(Component)]
pub struct Flushed(pub Entity);

/// The game's chances (in a hundred): flushing, and washing hands after.
const FLUSH_CHANCE: f64 = 0.9;
const WASH_CHANCE: f64 = 0.9;
/// How keen they are to flush one left unflushed: the Neat, and anyone else.
const NEAT_FLUSH: f32 = 26.0;
const FLUSH: f32 = 6.0;

fn has(sim: &Sim, t: Trait) -> bool {
    sim.traits.contains(&t)
}

/// Whether they flush after (`ShouldFlush`).
pub fn should_flush(sim: &Sim, rng: &mut impl Rng) -> bool {
    if has(sim, Trait::Neat) {
        true
    } else if has(sim, Trait::Slob) {
        false
    } else {
        rng.random_bool(FLUSH_CHANCE)
    }
}

/// Whether they wash their hands after (`ShouldWashHands`).
pub fn should_wash(sim: &Sim, rng: &mut impl Rng) -> bool {
    if has(sim, Trait::Neat) {
        true
    } else if has(sim, Trait::Slob) {
        false
    } else {
        rng.random_bool(WASH_CHANCE)
    }
}

/// How much one left unflushed draws them to flush it by free will (not the Slobs at all).
pub fn drawn(sim: &Sim) -> Option<f32> {
    if has(sim, Trait::Slob) {
        None
    } else if has(sim, Trait::Neat) {
        Some(NEAT_FLUSH)
    } else {
        Some(FLUSH)
    }
}

/// Whether an unflushed toilet's a mess to them (`GetEnvironmentScore`: not to a Slob).
pub fn bothers(sim: &Sim) -> bool {
    !has(sim, Trait::Slob)
}

/// Done at the toilet: it needs a flush, then (likely) flushed, and hands washed after.
#[allow(clippy::type_complexity)]
fn used(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &UsedToilet, &mut ActionQueue, Option<&crate::visit::OnLot>)>,
    objects: Query<(Entity, &GameObject, &Transform, Option<&crate::visit::LotObject>), Without<crate::interact::Broken>>,
    mut unflushed: ResMut<Unflushed>,
) {
    let mut rng = rand::rng();
    for (me, sim, u, mut queue, lot) in &mut sims {
        commands.entity(me).remove::<UsedToilet>();
        *unflushed.0.entry(u.toilet).or_insert(0) += 1;
        let Ok((_, _, ttf, _)) = objects.get(u.toilet) else { continue };
        let def_of = |kind: ObjectKind, f: &dyn Fn(&crate::interact::InteractionDef) -> bool| interactions_for(kind).iter().position(f);
        // (Hands washed at the nearest sink on that floor: the game's `FindClosestSink`.)
        if should_wash(sim, &mut rng)
            && let Some(sink) = objects
                .iter()
                .filter(|(_, o, t, l)| o.kind == ObjectKind::Sink && l.map(|l| l.0) == lot.map(|l| l.0) && (t.translation.y - ttf.translation.y).abs() < 1.5)
                .min_by(|a, b| a.2.translation.distance(ttf.translation).total_cmp(&b.2.translation.distance(ttf.translation)))
            && let Some(def) = def_of(ObjectKind::Sink, &|d| d.name == "Wash Hands")
        {
            queue.0.push_front(Action::new("Wash Hands", ActionKind::Object { target: sink.0, def }, u.autonomous));
        }
        if should_flush(sim, &mut rng)
            && let Some(def) = def_of(ObjectKind::Toilet, &|d| d.special == Special::Flush)
        {
            queue.0.push_front(Action::new("Flush", ActionKind::Object { target: u.toilet, def }, u.autonomous));
        } else {
            info!("toilets: {} left the toilet unflushed", sim.first);
        }
    }
}

fn flushed(mut commands: Commands, done: Query<(Entity, &Flushed)>, mut unflushed: ResMut<Unflushed>) {
    for (e, f) in &done {
        commands.entity(e).remove::<Flushed>();
        unflushed.0.remove(&f.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slobs_never_flush_and_the_neat_always_do() {
        let mut rng = rand::rng();
        let mut s = crate::sim::random_sim(&mut rng, "T", Some(true), crate::sim::Age::Adult);
        s.traits = vec![Trait::Slob];
        assert!((0..50).all(|_| !should_flush(&s, &mut rng) && !should_wash(&s, &mut rng)));
        assert!(drawn(&s).is_none() && !bothers(&s));
        s.traits = vec![Trait::Neat];
        assert!((0..50).all(|_| should_flush(&s, &mut rng) && should_wash(&s, &mut rng)));
        assert_eq!(drawn(&s), Some(NEAT_FLUSH));
    }
}
