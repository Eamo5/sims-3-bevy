//! Curfews, as the game's: children are home by ten at night and teens by eleven, till six
//! (`kChildStartCurfewHour`, `kTeenStartCurfewHour`, `kChildEndCurfewHour`, `kTeenEndCurfewHour`),
//! with a word an hour before ("Kim's curfew is in one hour..."). A child out after it without a
//! grown-up of the household near them (`kChildCurfewDistanceFromAdult`) goes home. A teen out
//! unsupervised is Out After Curfew (but not a Good one), and from half past eleven
//! (`kTeenBreakCurfewHour`) every half hour (`kTimeBetweenCurfewChecks`) may be caught by the
//! police (`kChanceOfPoliceSpawnAfterCurfew`, a Rebellious teen less often): brought home, and
//! Caught After Curfew.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{Age, HouseholdMember, Sim};

pub struct CurfewPlugin;

impl Plugin for CurfewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, curfews.run_if(in_state(PlayMode::Live)));
    }
}

const CHILD_START: f32 = 22.0;
const TEEN_START: f32 = 23.0;
const TEEN_BREAK: f32 = 23.5;
const END: f32 = 6.0;
/// `kChildCurfewDistanceFromAdult`.
const NEAR_ADULT: f32 = 5.0;
/// `kTimeBetweenCurfewChecks`, `kChanceOfPoliceSpawnAfterCurfew`,
/// `kChanceOfPoliceSpawnAfterCurfewRebellious`.
const CHECK_MINUTES: f64 = 30.0;
const CAUGHT: f64 = 0.3;
const CAUGHT_REBELLIOUS: f64 = 0.1;
/// The Caught After Curfew moodlet's three hours.
const CAUGHT_MINUTES: f64 = 180.0;

/// Whether it's within a curfew from `start` (till six in the morning).
pub fn in_curfew(hour: f32, start: f32) -> bool {
    hour >= start || hour < END
}

/// The game's warning, an hour before ("{0.SimFirstName}'s curfew is in one hour...").
fn warning(sim: &Sim) -> String {
    let he = if sim.female { "she" } else { "he" };
    match sim.age {
        Age::Child => format!("{}'s curfew is in effect in an hour. Maybe {he} should head home or find an adult.", sim.full_name()),
        _ => format!("{}'s curfew is in one hour. If {he} is out past curfew, {he} may get caught by the police.", sim.first),
    }
}

#[derive(Default)]
struct Watch {
    /// The day each warning was last given (children's, teens').
    warned: [Option<u32>; 2],
    /// When the police last looked.
    checked: f64,
}

#[allow(clippy::type_complexity)]
fn curfews(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut watch: Local<Watch>,
    sims: Query<(Entity, &Sim, &Transform, Option<&crate::visit::OnLot>), (With<HouseholdMember>, Without<crate::rabbitholes::AtRabbitHole>, Without<crate::interact::AtWork>)>,
    mut moodlets: Query<&mut Moodlets>,
    world: Res<crate::loading::CurrentWorld>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let hour = clock.hour_f();
    let day = clock.day();
    let now = clock.minutes;
    let away = |lot: Option<&crate::visit::OnLot>| lot.map(|l| l.0);
    // The warnings, an hour before (for those out).
    for (i, (age, start)) in [(Age::Child, CHILD_START), (Age::Teen, TEEN_START)].into_iter().enumerate() {
        if (start - 1.0..start).contains(&hour)
            && watch.warned[i] != Some(day)
            && let Some((_, sim, ..)) = sims.iter().find(|s| s.1.age == age && away(s.3).is_some())
        {
            watch.warned[i] = Some(day);
            notes.push(warning(sim));
        }
    }
    // (A grown-up of the household: young adult or older.)
    let grown: Vec<(Option<usize>, Vec3)> = sims
        .iter()
        .filter(|s| matches!(s.1.age, Age::YoungAdult | Age::Adult | Age::Elder))
        .map(|s| (away(s.3), s.2.translation))
        .collect();
    let supervised = |lot: Option<usize>, at: Vec3| grown.iter().any(|(l, p)| *l == lot && p.distance(at) < NEAR_ADULT);
    let check = now - watch.checked >= CHECK_MINUTES && in_curfew(hour, TEEN_BREAK);
    if check {
        watch.checked = now;
    }
    let mut rng = rand::rng();
    for (e, sim, tf, on_lot) in &sims {
        let Some(lot) = away(on_lot) else {
            // (Home again: no longer out after curfew.)
            if let Ok(mut m) = moodlets.get_mut(e)
                && m.has(MoodletKind::OutAfterCurfew)
            {
                m.remove(MoodletKind::OutAfterCurfew);
            }
            continue;
        };
        let place = crate::visit::place_name(&world.data, lot);
        match sim.age {
            Age::Child if in_curfew(hour, CHILD_START) && !supervised(Some(lot), tf.translation) => {
                notes.push(format!("{} has to be home by curfew, and goes home.", sim.first));
                crate::visit::drive_home(&mut commands, &clock, e, lot, place);
            }
            Age::Teen if in_curfew(hour, TEEN_START) && !supervised(Some(lot), tf.translation) => {
                let Ok(mut m) = moodlets.get_mut(e) else { continue };
                if !m.has(MoodletKind::OutAfterCurfew) && !sim.traits.contains(&Trait::Good) {
                    m.add(MoodletKind::OutAfterCurfew, now);
                }
                let chance = if sim.traits.contains(&Trait::Rebellious) { CAUGHT_REBELLIOUS } else { CAUGHT };
                if check && rng.random_bool(chance) {
                    // (The officer's words, and home in the back of the car.)
                    notes.push(format!("{} was caught out after curfew: \u{201c}You're out past your curfew! Come with me, right now.\u{201d}", sim.first));
                    m.remove(MoodletKind::OutAfterCurfew);
                    m.add_valued(MoodletKind::CaughtAfterCurfew, now, MoodletKind::CaughtAfterCurfew.def().value, CAUGHT_MINUTES);
                    crate::visit::drive_home(&mut commands, &clock, e, lot, place);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curfews_run_overnight() {
        assert!(in_curfew(22.5, CHILD_START) && in_curfew(3.0, CHILD_START) && !in_curfew(21.9, CHILD_START) && !in_curfew(6.0, CHILD_START));
        assert!(!in_curfew(22.5, TEEN_START) && in_curfew(23.0, TEEN_START));
    }
}
