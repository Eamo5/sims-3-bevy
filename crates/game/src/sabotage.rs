//! A Technophobe's sabotage, as the game's `TV.SabotageTV`, `Computer.Sabotage` and
//! `Stereo.SabotageStereo`: "Sabotage" on a TV, computer or stereo that isn't broken or made
//! unbreakable, only when told (their tuning disallows it by free will), teens and up. Half an
//! hour tinkering at it (the repair animation: the game's "Upgrade" state), and then it's broken
//! nine times in ten for a TV (`AntiTVTraitChanceToSabotageTV`), one in four for a computer or a
//! stereo (`kChanceToSucceed`; a stereo turned off first): the game's words, and Fight the Power
//! (+15, two hours). Otherwise, the game's words for a sabotage foiled.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{Broken, GameObject, Notifications, ObjectKind};
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{Age, Sim};

pub struct SabotagePlugin;

impl Plugin for SabotagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sabotaged.run_if(in_state(PlayMode::Live)));
    }
}

/// The odds it's broken: a TV's, and the rest's.
const TV_CHANCE: f64 = 0.9;
const CHANCE: f64 = 0.25;

/// Whether a Sim may sabotage it: a Technophobe, teen or older, at a TV, computer or stereo
/// that isn't broken or unbreakable.
pub fn offers(sim: &Sim, kind: ObjectKind, broken: bool, unbreakable: bool) -> bool {
    sim.traits.contains(&Trait::Technophobe)
        && (sim.age == Age::Teen || (sim.age.is_grown() && sim.age != Age::Child))
        && matches!(kind, ObjectKind::Tv | ObjectKind::Computer | ObjectKind::Stereo)
        && !broken
        && !unbreakable
}

/// Done tinkering with it.
#[derive(Component)]
pub struct Sabotaged(pub Entity);

fn sabotaged(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &Sabotaged, &mut Moodlets)>,
    objects: Query<(&GameObject, Option<&crate::upgrades::Upgrades>), Without<Broken>>,
    ui: Option<Res<crate::layout::UiAssets>>,
    clock: Res<crate::clock::GameClock>,
    mut notes: ResMut<Notifications>,
) {
    for (e, sim, s, mut moodlets) in &mut sims {
        commands.entity(e).remove::<Sabotaged>();
        let Ok((obj, upgrades)) = objects.get(s.0) else { continue };
        if crate::upgrades::has(upgrades, crate::upgrades::Upgrade::Unbreakable) {
            continue;
        }
        let (key, chance, fail) = match obj.kind {
            ObjectKind::Tv => ("TV/SabotageTV", TV_CHANCE, "SabotageFail"),
            ObjectKind::Computer => ("Computer/Sabotage", CHANCE, "SabotageFailure"),
            _ => ("Stereo/Sabotage", CHANCE, "SabotageFail"),
        };
        let say = |k: &str, or: String| {
            ui.as_deref().and_then(|u| u.localize(&format!("Gameplay/Objects/Electronics/{key}:{k}"))).map_or(or, |t| t.replace("{0.SimFirstName}", &sim.first))
        };
        if rand::rng().random_bool(chance) {
            commands.entity(s.0).insert(Broken).remove::<crate::music::StereoOn>();
            info!("{} sabotaged the {}", sim.first, obj.name);
            notes.push(say("SabotageSuccess", "Rejoice! The evil forces of technology have been beaten back once again!".into()));
            moodlets.add(MoodletKind::FightThePower, clock.minutes);
        } else {
            notes.push(say(fail, format!("Drats! Foiled in sabotage plans once again. {} will just have to try again.", sim.first)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_technophobes_sabotage_what_still_works() {
        let mut sim = crate::sim::random_sim(&mut rand::rng(), "T", Some(true), Age::Adult);
        sim.traits.clear();
        assert!(!offers(&sim, ObjectKind::Tv, false, false));
        sim.traits.push(Trait::Technophobe);
        assert!(offers(&sim, ObjectKind::Tv, false, false));
        assert!(offers(&sim, ObjectKind::Stereo, false, false));
        assert!(!offers(&sim, ObjectKind::Tv, true, false), "not broken already");
        assert!(!offers(&sim, ObjectKind::Computer, false, true), "not unbreakable");
        assert!(!offers(&sim, ObjectKind::Toilet, false, false));
        sim.age = Age::Child;
        assert!(!offers(&sim, ObjectKind::Tv, false, false), "teens and up");
    }
}
