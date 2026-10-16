//! Prank calls, as the game's phone `CallPrank`: someone the Sim knows called up and pranked.
//! Unless they're Childish, Friendly or a Loner (who take it well, Pranked and amused; the game's
//! list has Carefree too, not a base game trait), their talk with the prankster turns Insulting
//! (`kSTCInsultingIncrease`, 50) and they're Pranked and put out, and the prankster has a
//! Successful Prank.

use bevy::prelude::*;

use crate::PlayMode;
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::Sim;

pub struct PranksPlugin;

impl Plugin for PranksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, pranked.run_if(in_state(PlayMode::Live)));
    }
}

/// The game's `kPositiveReactionTraits`: those who take a prank well.
const TAKE_IT_WELL: [Trait; 3] = [Trait::Childish, Trait::Friendly, Trait::Loner];
/// How much more Insulting their talk gets (`kSTCInsultingIncrease`).
const INSULTING: f32 = 50.0;
/// How long a prank call takes (game minutes).
pub const PRANK_MINUTES: f32 = 10.0;

/// A prank call made, to whom: its outcome to be had.
#[derive(Component)]
pub struct PrankMade(pub Entity);

/// Whether the one pranked takes it well.
pub fn takes_it_well(sim: &Sim) -> bool {
    TAKE_IT_WELL.iter().any(|t| sim.traits.contains(t))
}

fn pranked(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    made: Query<(Entity, &Sim, &PrankMade)>,
    sims: Query<&Sim>,
    mut rels: Query<&mut crate::social::Relationships>,
    mut moods: Query<&mut Moodlets>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    for (me, sim, p) in &made {
        commands.entity(me).remove::<PrankMade>();
        let Ok(them) = sims.get(p.0) else { continue };
        let well = takes_it_well(them);
        if well {
            notes.push(format!("{} laughed along with {}'s prank call.", them.first, sim.first));
        } else {
            // (Their talk turned Insulting, both ways.)
            for (a, b) in [(me, p.0), (p.0, me)] {
                if let Ok(mut r) = rels.get_mut(a) {
                    let x = r.entry(b);
                    x.stc = crate::conversation::Stc { commodity: crate::conversation::Commodity::Insulting, value: INSULTING };
                }
            }
            if let Ok(mut m) = moods.get_mut(me) {
                m.add(MoodletKind::SuccessfulPrank, clock.minutes);
            }
            notes.push(format!("{} prank called {}, who wasn't amused.", sim.first, them.first));
        }
        if let Ok(mut m) = moods.get_mut(p.0) {
            m.add(if well { MoodletKind::PrankedPositive } else { MoodletKind::PrankedNegative }, clock.minutes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loner_takes_a_prank_well() {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "P", Some(true), crate::sim::Age::Adult);
        s.traits = vec![Trait::Loner];
        assert!(takes_it_well(&s));
        s.traits = vec![Trait::Grumpy];
        assert!(!takes_it_well(&s));
    }
}
