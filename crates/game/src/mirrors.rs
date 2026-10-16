//! At a mirror, as the game's `Mirror` has it besides Practice Speech and Change Appearance:
//! Admire Self (the Snobs', `AdmireSelf`: I am Beautiful after), Gussy Up (teens and up,
//! `GussyUp`: Attractive after), Play with Mirror (children's, and the Childish of any age,
//! `PlayWithMirror`: fun) and Check Self Out (`CheckSelfOutInMirror`). Each draws those its
//! tuning advertises to: the Snobs and the Flirty to their reflection, the Childish to making
//! faces in it.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::interact::Special;
use crate::life::{MoodletKind, Trait};
use crate::sim::{Age, Sim};

pub struct MirrorsPlugin;

impl Plugin for MirrorsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastLooked>();
    }
}

/// When each Sim last looked themselves over in a mirror (game minutes): the trait's pull is met
/// by it, building again over the next hours (the game's trait commodities, satisfied and decaying).
#[derive(Resource, Default)]
pub struct LastLooked(pub HashMap<Entity, f64>);

/// How long (game minutes) before the vain feel drawn to their reflection again.
pub const AGAIN_MINUTES: f64 = 8.0 * 60.0;

impl LastLooked {
    /// Whether their trait would draw them to it again by now.
    pub fn due(&self, sim: Entity, now: f64) -> bool {
        self.0.get(&sim).is_none_or(|t| now - t >= AGAIN_MINUTES)
    }
}

/// Whether it's offered to a Sim: the game's ages for it (teens and up; Play with Mirror
/// children too) and its tests (Admire Self the Snobs' alone, its `RequiredTrait`; Play with
/// Mirror past childhood only for the Childish).
pub fn offers(special: Special, sim: &Sim) -> bool {
    let teen_up = sim.age == Age::Teen || sim.age.is_grown();
    let has = |t: Trait| sim.traits.contains(&t);
    match special {
        Special::AdmireSelf => teen_up && has(Trait::Snob),
        Special::GussyUp | Special::CheckSelfOut => teen_up,
        Special::PlayWithMirror => sim.age == Age::Child || (teen_up && has(Trait::Childish)),
        _ => true,
    }
}

/// How much a Sim's traits draw them to it of their own accord (the game's trait commodities
/// in its tuning: the Snobs' 200 to Admire Self and Check Self Out, the Flirty's 20 to those and
/// Gussy Up, the Childish's 35 to Play with Mirror), as an autonomy score; none for the rest.
pub fn drawn(special: Special, sim: &Sim) -> Option<f32> {
    let has = |t: Trait| sim.traits.contains(&t);
    let score = match special {
        Special::AdmireSelf => 20.0,
        Special::CheckSelfOut if has(Trait::Snob) => 16.0,
        Special::CheckSelfOut | Special::GussyUp if has(Trait::Flirty) => 6.0,
        Special::GussyUp if has(Trait::Snob) => 6.0,
        Special::PlayWithMirror if has(Trait::Childish) => 12.0,
        _ => return None,
    };
    Some(score)
}

/// What it leaves them with: I am Beautiful for admiring themselves, Attractive for gussying up.
pub fn moodlet(special: Special) -> Option<MoodletKind> {
    match special {
        Special::AdmireSelf => Some(MoodletKind::IAmBeautiful),
        Special::GussyUp => Some(MoodletKind::Attractive),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(age: Age, traits: &[Trait]) -> Sim {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "M", Some(true), age);
        s.traits = traits.to_vec();
        s
    }

    #[test]
    fn admiring_is_the_snobs_and_playing_the_childrens() {
        assert!(offers(Special::AdmireSelf, &sim(Age::Adult, &[Trait::Snob])));
        assert!(!offers(Special::AdmireSelf, &sim(Age::Adult, &[])));
        assert!(!offers(Special::AdmireSelf, &sim(Age::Child, &[Trait::Snob])));
        assert!(offers(Special::PlayWithMirror, &sim(Age::Child, &[])));
        assert!(!offers(Special::PlayWithMirror, &sim(Age::Adult, &[])));
        assert!(offers(Special::PlayWithMirror, &sim(Age::Elder, &[Trait::Childish])));
        assert!(offers(Special::GussyUp, &sim(Age::Teen, &[])) && !offers(Special::GussyUp, &sim(Age::Child, &[])));
    }

    #[test]
    fn looked_in_the_mirror_they_leave_it_a_while() {
        let mut l = LastLooked::default();
        let e = Entity::from_raw_u32(3).unwrap();
        assert!(l.due(e, 0.0));
        l.0.insert(e, 100.0);
        assert!(!l.due(e, 200.0) && l.due(e, 100.0 + AGAIN_MINUTES));
    }

    #[test]
    fn the_vain_are_drawn_to_their_reflection() {
        assert!(drawn(Special::CheckSelfOut, &sim(Age::Adult, &[Trait::Snob])).is_some());
        assert!(drawn(Special::CheckSelfOut, &sim(Age::Adult, &[])).is_none());
        assert!(drawn(Special::GussyUp, &sim(Age::Adult, &[Trait::Flirty])).is_some());
        assert_eq!(moodlet(Special::GussyUp), Some(MoodletKind::Attractive));
    }
}
