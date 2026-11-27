//! The chin-up bar, Showtime's (`ChinUpBar`): teens and up "Do Chin Ups" on it (the Daredevil's
//! "Do Extreme Chin Ups", a Natural Born Performer's "Do Chin Ups with Flair"), a strength workout
//! as the workout bench's (Athletic, in workout clothes, shaping up): up onto it
//! (`a2o_chinBar_chinUp_start`) and at it as their Athletic has them (`GetSkillLevelParameterForJazzGraph`:
//! the low ones to level five, middling to seven, high to nine, one-armed after), and down. The
//! Fatigued can't ("Too tired to do chin ups!").

use bevy::prelude::*;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Skills};
use crate::life::Trait;

pub struct ChinUpPlugin;

impl Plugin for ChinUpPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, chin_ups.after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words.
pub const CHIN_UPS: &str = "Do Chin Ups";
pub const TOO_TIRED: &str = "Too tired to do chin ups!";

/// The Athletic levels for the middling, high and one-armed chin ups (the athletic objects'
/// `kAthleticLevelFor...Animations`).
const NORMAL: f32 = 5.0;
const SKILLED: f32 = 7.0;
const EXPERT: f32 = 9.0;

/// Its name for a Sim (`GetWorkoutStrengthName`).
pub fn label(traits: &[Trait]) -> &'static str {
    if traits.contains(&Trait::Daredevil) {
        "Do Extreme Chin Ups"
    } else if traits.contains(&Trait::NaturalBornPerformer) {
        "Do Chin Ups with Flair"
    } else {
        CHIN_UPS
    }
}

/// The chin ups for an Athletic level.
pub fn band(level: f32) -> &'static [&'static str] {
    if level >= EXPERT {
        &["a2o_chinBar_chinUp_oneArmed_loop"]
    } else if level >= SKILLED {
        &["a2o_chinBar_chinUp_high_loop"]
    } else if level >= NORMAL {
        &["a2o_chinBar_chinUp_medium_loop"]
    } else {
        &["a2o_chinBar_chinUp_low_loop"]
    }
}

/// At the bar: the chin ups they're at.
#[derive(Component)]
struct ChinningUp(&'static [&'static str]);

/// The chin ups their Athletic has them at (the one-armed ones in and out of their own).
fn chin_ups(mut commands: Commands, sims: Query<(Entity, &ActionQueue, &Skills, Option<&ChinningUp>)>, objects: Query<&GameObject>) {
    for (e, q, skills, at) in &sims {
        let on = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).is_some_and(|a| match a.kind {
            ActionKind::Object { target, .. } => objects.get(target).is_ok_and(|o| o.kind == ObjectKind::ChinUpBar),
            _ => false,
        });
        if !on {
            if at.is_some() {
                commands.entity(e).remove::<ChinningUp>();
            }
            continue;
        }
        let level = skills.0.get("Athletic").copied().unwrap_or(0.0);
        let b = band(level);
        if at.is_some_and(|a| a.0 == b) {
            continue;
        }
        let clip = if level >= EXPERT {
            ActionClip::steps("a2o_chinBar_chinUp_start_x", &["a2o_chinBar_chinUp_oneArmed_start_x"], b).ending(&["a2o_chinBar_chinUp_oneArmed_stop_x", "a2o_chinBar_chinUp_stop_x"], 1.5)
        } else {
            ActionClip::new(Some("a2o_chinBar_chinUp_start_x"), b).ending(&["a2o_chinBar_chinUp_stop_x"], 1.5)
        };
        commands.entity(e).insert((ChinningUp(b), clip));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chin_ups_as_the_game_has_them() {
        assert_eq!(band(2.0), &["a2o_chinBar_chinUp_low_loop"]);
        assert_eq!(band(5.0), &["a2o_chinBar_chinUp_medium_loop"]);
        assert_eq!(band(8.0), &["a2o_chinBar_chinUp_high_loop"]);
        assert_eq!(band(10.0), &["a2o_chinBar_chinUp_oneArmed_loop"]);
        assert_eq!(label(&[Trait::Daredevil]), "Do Extreme Chin Ups");
        assert_eq!(label(&[Trait::NaturalBornPerformer]), "Do Chin Ups with Flair");
        assert_eq!(label(&[]), CHIN_UPS);
    }
}
