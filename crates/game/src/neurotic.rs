//! A Neurotic Sim's worries, as the game's `TraitFunctions.CheckForNeuroticAnxiety`: having used a
//! sink, the stove or a fireplace, one time in ten an hour later they're Feeling Anxious about it
//! (its tuning's `NeuroticTraitAnxiousChance` 0.1, `NeuroticTraitAnxiousDelay` an hour; one worry
//! at a time).
//! Only theirs to do, Check Sink, Check Stove and Check Fireplace (`Sink.Check`, `Stove_Check`,
//! `Fireplace.Check`) leave them Feeling Calm, and the worry about it gone; they check of their
//! own accord now and then too (the game's `TraitNeurotic` commodity), not while still calm.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Special};
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{Age, Sim};

pub struct NeuroticPlugin;

impl Plugin for NeuroticPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (fret, worry, checked).run_if(in_state(PlayMode::Live)));
    }
}

/// What they worry about: what they used.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum About {
    Sink,
    Stove,
    Fireplace,
}

impl About {
    pub fn of(kind: ObjectKind) -> Option<About> {
        match kind {
            ObjectKind::Sink => Some(About::Sink),
            ObjectKind::Stove => Some(About::Stove),
            ObjectKind::Fireplace => Some(About::Fireplace),
            _ => None,
        }
    }

    pub fn moodlet(self) -> MoodletKind {
        match self {
            About::Sink => MoodletKind::AnxiousSink,
            About::Stove => MoodletKind::AnxiousStove,
            About::Fireplace => MoodletKind::AnxiousFireplace,
        }
    }
}

const WORRIES: [MoodletKind; 3] = [MoodletKind::AnxiousSink, MoodletKind::AnxiousStove, MoodletKind::AnxiousFireplace];

/// `NeuroticTraitAnxiousDelay`: how long after (game minutes); and `NeuroticTraitAnxiousChance`.
const DELAY_MINUTES: f64 = 60.0;
const ANXIOUS_CHANCE: f64 = 0.1;

/// What they've used and been worried by, or not, already (the chance is had once a use).
#[derive(Component)]
pub struct Fretted(pub Entity);

/// How keen they are to check what they're anxious about, and to check now and then.
const ANXIOUS_PULL: f32 = 40.0;
const NOW_AND_THEN: f32 = 4.0;

/// A worry coming (the game's `NeuroticCheckHandle`): about what, and when.
#[derive(Component, Clone, Copy)]
pub struct WorryLater {
    pub about: About,
    pub at: f64,
}

/// Checked (its outcome to be had).
#[derive(Component)]
pub struct Checked(pub About);

/// Whether it's offered: the Neurotic's own, teens and up (the game's tuning's ages).
pub fn offers(special: Special, sim: &Sim) -> bool {
    special != Special::NeuroticCheck || (sim.traits.contains(&Trait::Neurotic) && (sim.age == Age::Teen || sim.age.is_grown()))
}

/// How much it draws them by free will: what they're anxious about, much; otherwise a check now
/// and then, unless still calm from the last.
pub fn drawn(kind: ObjectKind, moodlets: Option<&Moodlets>) -> Option<f32> {
    let about = About::of(kind)?;
    let has = |k: MoodletKind| moodlets.is_some_and(|m| m.has(k));
    if has(about.moodlet()) {
        Some(ANXIOUS_PULL)
    } else if has(MoodletKind::FeelingCalm) || WORRIES.iter().any(|w| has(*w)) {
        None
    } else {
        Some(NOW_AND_THEN)
    }
}

/// Using a sink, the stove or a fireplace (not checking it), a Neurotic's worry about it to come.
fn fret(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    sims: Query<(Entity, &Sim, &ActionQueue, Option<&Fretted>), Without<WorryLater>>,
    objects: Query<&GameObject>,
) {
    let mut rng = rand::rng();
    for (e, sim, q, fretted) in &sims {
        if !sim.traits.contains(&Trait::Neurotic) {
            continue;
        }
        let using = q.current().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(|a| match a.kind {
            ActionKind::Object { target, def } => Some((target, Some(def))),
            ActionKind::Repair { target } => Some((target, None)),
            _ => None,
        });
        let worry = using.and_then(|(target, def)| {
            let obj = objects.get(target).ok()?;
            let about = About::of(obj.kind)?;
            let check = def.is_some_and(|d| crate::interact::interactions_for(obj.kind).get(d).is_some_and(|d| d.special == Special::NeuroticCheck));
            (!check).then_some((target, about))
        });
        let Some((target, about)) = worry else {
            if fretted.is_some() {
                commands.entity(e).remove::<Fretted>();
            }
            continue;
        };
        if fretted.is_some_and(|f| f.0 == target) {
            continue;
        }
        commands.entity(e).insert(Fretted(target));
        if rng.random_bool(ANXIOUS_CHANCE) {
            commands.entity(e).insert(WorryLater { about, at: clock.minutes + DELAY_MINUTES });
        }
    }
}

/// The hour up: Feeling Anxious about it (the one worry: another's put aside).
fn worry(mut commands: Commands, clock: Res<crate::clock::GameClock>, mut sims: Query<(Entity, &Sim, &WorryLater, &mut Moodlets)>) {
    for (e, sim, w, mut m) in &mut sims {
        if clock.minutes < w.at {
            continue;
        }
        commands.entity(e).remove::<WorryLater>();
        for k in WORRIES {
            m.remove(k);
        }
        m.add(w.about.moodlet(), clock.minutes);
        info!("neurotic: {} is anxious about the {:?}", sim.first, w.about);
    }
}

/// Checked: Feeling Calm, and the worry about it gone.
fn checked(mut commands: Commands, clock: Res<crate::clock::GameClock>, mut sims: Query<(Entity, &Sim, &Checked, &mut Moodlets)>) {
    for (e, sim, c, mut m) in &mut sims {
        commands.entity(e).remove::<Checked>();
        m.add(MoodletKind::FeelingCalm, clock.minutes);
        m.remove(c.0.moodlet());
        info!("neurotic: {} checked the {:?}, and feels calm", sim.first, c.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anxious_ones_check_what_they_worry_about() {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "N", Some(true), Age::Adult);
        s.traits = vec![Trait::Neurotic];
        assert!(offers(Special::NeuroticCheck, &s));
        s.traits.clear();
        assert!(!offers(Special::NeuroticCheck, &s));
        let mut m = Moodlets::default();
        assert_eq!(drawn(ObjectKind::Sink, Some(&m)), Some(NOW_AND_THEN));
        m.add(MoodletKind::AnxiousStove, 0.0);
        assert_eq!(drawn(ObjectKind::Stove, Some(&m)), Some(ANXIOUS_PULL));
        assert_eq!(drawn(ObjectKind::Sink, Some(&m)), None);
        m.remove(MoodletKind::AnxiousStove);
        m.add(MoodletKind::FeelingCalm, 0.0);
        assert_eq!(drawn(ObjectKind::Fireplace, Some(&m)), None);
    }
}
