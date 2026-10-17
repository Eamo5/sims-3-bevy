//! Bed ownership, as the game's `BedData.Owner`: a Sim sent to bed by the player claims the side
//! they sleep on (and gives up any other bed's), and "Set Bed Ownership" gives a side to one of
//! the household (the game's `SetBedOwnership`). By free will a Sim only goes to sleep in a bed
//! with a side free or theirs (or their partner's): the game's `BedSleep` test, its
//! `kAutonomyScoreMultiplierForBedOwner` 1 to the owner.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::sim::Sim;

pub struct BedOwnersPlugin;

impl Plugin for BedOwnersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, claim.run_if(in_state(PlayMode::Live)));
    }
}

/// The game's tuned `kAutonomyScoreMultiplierForBedAwayFromOwnedBed`: another bed, for one who
/// has their own.
pub const AWAY_FROM_OWN_BED: f32 = 0.01;

/// A bed's owners, side by side (a single bed's one; Sims by id).
#[derive(Component, Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct BedOwners(pub [Option<u64>; 2]);

impl BedOwners {
    /// Whether a Sim may sleep in it of their own accord: a side free, theirs or their partner's.
    pub fn lets(&self, kind: ObjectKind, me: u64, partner: Option<u64>) -> bool {
        self.sides(kind).iter().any(|o| o.is_none_or(|o| o == me || Some(o) == partner))
    }

    fn sides(&self, kind: ObjectKind) -> &[Option<u64>] {
        if kind == ObjectKind::BedDouble { &self.0 } else { &self.0[..1] }
    }

    /// A side given to someone (the first free one, else the first), taken from anyone else.
    pub fn give(&mut self, kind: ObjectKind, who: u64) {
        if self.sides(kind).contains(&Some(who)) {
            return;
        }
        let n = if kind == ObjectKind::BedDouble { 2 } else { 1 };
        let side = (0..n).find(|i| self.0[*i].is_none()).unwrap_or(0);
        self.0[side] = Some(who);
    }

    pub fn owns(&self, who: u64) -> bool {
        self.0.contains(&Some(who))
    }

    pub fn release(&mut self, who: u64) {
        for s in self.0.iter_mut().filter(|s| **s == Some(who)) {
            *s = None;
        }
    }
}

/// Whether an object's a bed with sides to own.
pub fn is_bed(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::BedSingle | ObjectKind::BedDouble)
}

/// A side given to someone, there and then: theirs, and no longer any other bed of theirs.
pub fn assign(world: &mut World, bed: Entity, who: u64) {
    let mut beds = world.query::<(Entity, &GameObject, Option<&mut BedOwners>)>();
    let mut fresh: Option<ObjectKind> = None;
    for (e, o, owners) in beds.iter_mut(world) {
        if !is_bed(o.kind) {
            continue;
        }
        match owners {
            Some(mut ow) if e == bed => ow.give(o.kind, who),
            Some(mut ow) => ow.release(who),
            None if e == bed => fresh = Some(o.kind),
            None => {}
        }
    }
    if let Some(kind) = fresh {
        let mut ow = BedOwners::default();
        ow.give(kind, who);
        world.entity_mut(bed).insert(ow);
    }
}

/// Sent to bed by the player: the side they sleep on is theirs (the game's `ClaimOwnership`).
fn claim(mut commands: Commands, sims: Query<(&Sim, &ActionQueue)>, beds: Query<(&GameObject, Option<&BedOwners>)>) {
    for (sim, q) in &sims {
        let Some(a) = q.current() else { continue };
        let ActionKind::Object { target, def } = a.kind else { continue };
        if a.autonomous || !matches!(a.phase, Phase::Running(_)) {
            continue;
        }
        let Ok((o, owners)) = beds.get(target) else { continue };
        if !is_bed(o.kind) || crate::interact::interactions_for(o.kind).get(def).is_none_or(|d| d.name != "Sleep") || owners.is_some_and(|ow| ow.owns(sim.id)) {
            continue;
        }
        let id = sim.id;
        commands.queue(move |w: &mut World| assign(w, target, id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bed_with_no_side_free_is_the_owners() {
        let mut b = BedOwners::default();
        b.give(ObjectKind::BedDouble, 1);
        assert!(b.lets(ObjectKind::BedDouble, 2, None));
        b.give(ObjectKind::BedDouble, 3);
        assert!(!b.lets(ObjectKind::BedDouble, 2, None) && b.lets(ObjectKind::BedDouble, 2, Some(3)) && b.lets(ObjectKind::BedDouble, 1, None));
        let mut s = BedOwners::default();
        s.give(ObjectKind::BedSingle, 1);
        assert!(!s.lets(ObjectKind::BedSingle, 2, None));
        s.release(1);
        assert!(s.lets(ObjectKind::BedSingle, 2, None));
    }
}
