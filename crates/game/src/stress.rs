//! The game's stress exits (`Motives.StressInteractionTest`, `MoodManager.StressInteractionTest`,
//! `Motives.StressExitCheck`): chores, skills, work and schoolwork (the interactions its tuning
//! marks `stressExit`) aren't for a Sim in a foul mood, or with a need run down that the
//! interaction does nothing for. On the pie menu they're greyed, the reason their tooltip
//! (`hud`); free will leaves them be; and one under way stops, the Sim thinking of the need (the
//! game's hidden `MotiveFail`, with its balloon).

use bevy::prelude::*;

use crate::interact::{ActionKind, ActionQueue, GameObject, InteractionDef, Phase, Special};
use crate::sim::{BLADDER, ENERGY, FUN, HUNGER, Motives, SOCIAL, Sim};

pub struct StressPlugin;

impl Plugin for StressPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, stress_exit.run_if(in_state(crate::PlayMode::Live)));
    }
}

/// A need's stress exit: how low it may run (the game's `kStressExit...`, from the `Motives`
/// class's tuning; and its `TraitResistant` ones, a Neat Sim's while cleaning), the greyed
/// option's words, and the balloon of the Sim's `MotiveFail`.
pub struct Exit {
    pub need: usize,
    pub low: f32,
    pub resistant: f32,
    key: &'static str,
    words: &'static str,
    pub balloon: &'static str,
}

/// In the game's order: the bladder first.
pub const EXITS: [Exit; 5] = [
    Exit { need: BLADDER, low: -80.0, resistant: -101.0, key: "Gameplay/Actors/Sim:BladderFailureTooltip", words: "Your Sim's Bladder is too full to do that right now.", balloon: "balloon_moodlet_hasToPee" },
    Exit { need: ENERGY, low: -81.0, resistant: -90.0, key: "Gameplay/Actors/Sim:EnergyFailureTooltip", words: "Your Sim is too tired to do that right now.", balloon: "balloon_moodlet_tired" },
    Exit { need: FUN, low: -51.0, resistant: -101.0, key: "Gameplay/Actors/Sim:FunFailureTooltip", words: "Your Sim is too stressed to do that right now.", balloon: "balloon_moodlet_stressed" },
    Exit { need: HUNGER, low: -62.0, resistant: -82.0, key: "Gameplay/Actors/Sim:HungerFailureTooltip", words: "Your Sim is too hungry to do that right now.", balloon: "balloon_moodlet_veryHungry" },
    Exit { need: SOCIAL, low: -99.0, resistant: -101.0, key: "Gameplay/Actors/Sim:SocialFailureTooltip", words: "Your Sim is too lonely to do that right now.", balloon: "balloon_moodlet_desolate" },
];

/// A mood this low is one of the game's `MoodExitFlavors` (furious, depressed, miserable,
/// stressed: `kMoodStrongNegativeValue`).
const MOOD_LOW: f32 = -50.0;
const MOOD_KEY: &str = "Gameplay/Actors/Sim:StressFailure";
const MOOD_WORDS: &str = "Mood is too low!";

/// Our interactions the game's tuning marks stress exits (its `ITUN`s' `stressExit`): chores,
/// the garden and fishing, skills, and work and schoolwork. (Fun, and seeing to a need, aren't.)
const STRESS_EXITS: [&str; 41] = [
    "Clean",
    "Empty Trash",
    "Make Bed",
    "Do Laundry",
    "Dry Clothes",
    "Load the Dryer",
    "Empty Dryer",
    "Hang Clothes",
    "Take Down Clothes",
    "Rake Up",
    "Water",
    "Weed",
    "Harvest",
    "Fish",
    "Paint",
    "Play Guitar",
    "Play Piano",
    "Play Drums",
    "Play Keyboard",
    "Play Laser Harp",
    "Play Chess",
    "Practice Speech",
    "Practice Writing",
    "Write Novel",
    "Solve Hard Problems",
    "Stargaze",
    "Search Galaxy",
    "Work Out",
    "Cardio Exercise",
    "Strength Training",
    "Pump Iron",
    "Practice Horseshoes",
    "Ride Mechanical Bull",
    "Play with Peg Box",
    "Play Arcade Game",
    "Do Homework",
    "Work from Home",
    "Work on Job-Related Issues",
    "Write Report",
    "Hack",
    "Overclock",
];

/// Whether the game stops this one for a low need (and greys it).
pub fn is_stress_exit(d: &InteractionDef) -> bool {
    STRESS_EXITS.contains(&d.name) || matches!(d.special, Special::PlayForTips | Special::Sabotage)
}

/// Cleaning, for a Neat Sim's resistance (the game's `ICleaningInteractionDefinition`s).
fn cleaning(d: &InteractionDef) -> bool {
    d.special == Special::Scrub
}

/// What an action does to the needs an hour, if it's a stress exit: its interaction's (a
/// repair, an upgrade and jogging do nothing for any); and whether it's cleaning.
pub fn exit_of(kind: &ActionKind, objects: &Query<&GameObject>) -> Option<([f32; 6], bool)> {
    match kind {
        ActionKind::Object { target, def } => {
            let obj = objects.get(*target).ok()?;
            let d = crate::interact::interactions_for(obj.kind).get(*def)?;
            is_stress_exit(d).then_some((d.per_hour, cleaning(d)))
        }
        ActionKind::Repair { .. } | ActionKind::Upgrade { .. } | ActionKind::Jog { .. } => Some(([0.0; 6], false)),
        _ => None,
    }
}

/// The need run too low for a stress exit doing this much for the needs, if one is (the game's
/// `StressInteractionTestPrivate`: a need it fills doesn't count).
pub fn low_need(motives: &Motives, per_hour: &[f32; 6], resistant: bool) -> Option<&'static Exit> {
    EXITS.iter().find(|e| motives.0[e.need] <= if resistant { e.resistant } else { e.low } && per_hour[e.need] <= 0.0)
}

/// Why a stress exit can't be done now, in the game's words, if it can't: their mood, or a need.
/// (Only while they've nothing else to do first, or another stress exit: what's queued behind
/// a meal may wait for it, as the game's test has it; a Neat Sim cleans regardless.)
pub fn why_not(ui: Option<&crate::layout::UiAssets>, sim: &Sim, motives: &Motives, mood: f32, queue: Option<&ActionQueue>, front_exit: bool, (per_hour, cleaning): ([f32; 6], bool)) -> Option<String> {
    if queue.is_some_and(|q| !q.0.is_empty()) && !front_exit {
        return None;
    }
    if cleaning && sim.traits.contains(&crate::life::Trait::Neat) {
        return None;
    }
    let word = |key: &str, plain: &str| ui.and_then(|u| u.localize(key)).unwrap_or_else(|| plain.to_string());
    if mood < MOOD_LOW {
        return Some(word(MOOD_KEY, MOOD_WORDS));
    }
    low_need(motives, &per_hour, false).map(|e| word(e.key, e.words))
}

/// A stress exit under way stops once a need it does nothing for runs too low (a Neat Sim
/// cleaning holds out longer), the Sim thinking of it: the game's `DoStressExit`.
fn stress_exit(mut commands: Commands, mut sims: Query<(Entity, &Motives, &Sim, &mut ActionQueue)>, objects: Query<&GameObject>) {
    for (me, motives, sim, mut queue) in &mut sims {
        let Some(a) = queue.0.front_mut() else { continue };
        if a.cancel || !matches!(a.phase, Phase::Running(_)) {
            continue;
        }
        let Some((per_hour, cleaning)) = exit_of(&a.kind, &objects) else { continue };
        let resistant = cleaning && sim.traits.contains(&crate::life::Trait::Neat);
        if let Some(e) = low_need(motives, &per_hour, resistant) {
            info!("stress exit: {} stops {} (need {} at {:.0})", sim.full_name(), a.label, e.need, motives.0[e.need]);
            a.cancel = true;
            commands.entity(me).insert(crate::balloons::BalloonRequest::thought(e.balloon));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_need_it_fills_doesnt_stop_it() {
        let mut m = Motives([50.0; 6]);
        assert!(low_need(&m, &[0.0; 6], false).is_none());
        m.0[HUNGER] = -70.0;
        assert_eq!(low_need(&m, &[0.0; 6], false).map(|e| e.need), Some(HUNGER));
        // (A Neat Sim cleaning holds out longer.)
        assert!(low_need(&m, &[0.0; 6], true).is_none());
        // (Fun low: not for what's fun.)
        m.0[HUNGER] = 0.0;
        m.0[FUN] = -60.0;
        let mut fun = [0.0; 6];
        fun[FUN] = 30.0;
        assert!(low_need(&m, &fun, false).is_none());
        assert_eq!(low_need(&m, &[0.0; 6], false).map(|e| e.need), Some(FUN));
        // (The bladder before anything else.)
        m.0[BLADDER] = -90.0;
        assert_eq!(low_need(&m, &[0.0; 6], false).map(|e| e.need), Some(BLADDER));
    }
}
