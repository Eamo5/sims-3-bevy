//! Potty training, as the game's `PottyChair`: a toddler not yet trained uses the potty chair
//! only when sat on it (never by free will: diapers till then); a grown-up of the family may
//! "Potty Train" them there, from the chair's menu ("Potty Train..."), the toddler sat on it a
//! quarter of an hour (`kPottyTimeLength`) with the grown-up beside them, encouraging them
//! (the game's `a2p_pottyChair_pottyTrain` clips), company for them (`kSocialChange`), if
//! they're in a good enough mood (`kPottyTrainToddlerMinimumMood`; otherwise they won't). Three
//! sessions train them, as their walking and talking are learned (a Family-Oriented teacher's
//! count half as much again, `kFamilyOrientedTraitPottyTrainMultiplier`); trained, they go by
//! themselves. Each use leaves the chair to be emptied ("Empty", after
//! `kVolumeForEmptyInteraction`), and past two uses (`kVolumeForPuddles`) it overflows.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, InteractionDef, Notifications, ObjectKind, Phase, Special, interactions_for};
use crate::sim::{Age, Sim};

pub struct PottyPlugin;

impl Plugin for PottyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Potties>().add_systems(Update, (potties.after(crate::interact::run_actions), potty_test).run_if(in_state(PlayMode::Live)));
    }
}

/// How the game names it, and its menu of toddlers.
pub const POTTY_TRAIN: &str = "Potty Train";
/// How far a session takes a toddler (three to be trained), and a Family-Oriented teacher's.
const LESSON: f32 = 0.34;
const FAMILY_ORIENTED: f32 = 1.5;
/// `kPottyTrainToddlerMinimumMood`.
const MINIMUM_MOOD: f32 = -20.0;
/// `kVolumeForEmptyInteraction`, `kVolumeForPuddles`.
const TO_EMPTY: u8 = 1;
const TO_OVERFLOW: u8 = 2;
/// How long a sitting must last to count (game minutes; the session's a quarter hour).
const A_SITTING: f32 = 12.0;

/// The grown-up's encouragement (and a refusal's), the toddler's sitting.
const ENCOURAGE: &str = "a2p_pottyChair_pottyTrain_accept_x";
const REFUSED: &str = "a2p_pottyChair_pottyTrain_reject_x";
const STANDING: &[&str] = &["a_idle_neutral_loop_"];

/// The potty chairs' state: how full each is; and which toddlers are trained.
#[derive(Resource, Default)]
pub struct Potties {
    pub volume: HashMap<Entity, u8>,
    pub trained: HashSet<Entity>,
}

impl Potties {
    pub fn needs_emptying(&self, chair: Entity) -> bool {
        self.volume.get(&chair).is_some_and(|v| *v >= TO_EMPTY)
    }
}

/// Who may do what at a potty chair: toddlers sit on it, teens and up empty it and train them.
pub fn suits(d: &InteractionDef, sim: &Sim) -> bool {
    match d.special {
        Special::PottyTrain | Special::EmptyPotty => !sim.age.is_little() && sim.age != Age::Child,
        _ => sim.age == Age::Toddler,
    }
}

/// The toddlers a Sim might potty train: their family's (household or kin), not yet trained.
pub fn trainable<'a>(potties: &Potties, toddlers: impl Iterator<Item = (Entity, &'a Sim, bool)>) -> Vec<(Entity, String)> {
    toddlers.filter(|(e, s, family)| s.age == Age::Toddler && *family && !potties.trained.contains(e)).map(|(e, s, _)| (e, s.first.clone())).collect()
}

/// A grown-up off to potty train a toddler (who, and whether they've been sat down yet).
#[derive(Component, Debug)]
pub struct PottyTraining {
    pub toddler: Entity,
    pub asked: bool,
}

/// A toddler sat on a potty chair: which, how long (game minutes), and whether for a lesson.
#[derive(Component, Debug)]
struct Sitting {
    chair: Entity,
    minutes: f32,
    lesson: bool,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn potties(
    mut commands: Commands,
    mut potties: ResMut<Potties>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, Option<&crate::little::ToddlerSkills>, Option<&crate::life::Mood>, Option<&Sitting>, Option<&mut PottyTraining>, &mut Transform, Option<&crate::nav::Floor>), Without<GameObject>>,
    chairs: Query<(&GameObject, &Transform), Without<Sim>>,
    mut notes: ResMut<Notifications>,
    mut puddles: MessageWriter<crate::puddles::AddPuddle>,
) {
    potties.trained = sims.iter().filter(|s| s.3.is_some_and(|t| t.potty_trained())).map(|s| s.0).collect();
    let def_of = |special: Special| interactions_for(ObjectKind::PottyChair).iter().position(|d| d.special == special);
    // What everyone's doing at a potty chair.
    let mut trainers: HashMap<Entity, Entity> = HashMap::new();
    let mut lessons: Vec<(Entity, Entity)> = Vec::new();
    let mut finished: Vec<(Entity, Entity, bool, bool)> = Vec::new();
    for (me, _, queue, _, _, sitting, training, ..) in &sims {
        let at = queue.0.front().filter(|a| !a.cancel).and_then(|a| match (a.kind.clone(), a.phase) {
            (ActionKind::Object { target, def }, Phase::Running(t)) => chairs.get(target).ok().filter(|c| c.0.kind == ObjectKind::PottyChair).and_then(|_| interactions_for(ObjectKind::PottyChair).get(def)).map(|d| (target, d.special, t)),
            _ => None,
        });
        match at {
            Some((chair, Special::PottyTrain, _)) => {
                trainers.insert(chair, me);
                if let Some(t) = training.filter(|t| !t.asked) {
                    lessons.push((me, t.toddler));
                }
            }
            Some((chair, Special::EmptyPotty, _)) => {
                potties.volume.insert(chair, 0);
            }
            Some((chair, special, t)) => {
                let lesson = special == Special::PottyLesson;
                commands.entity(me).insert(Sitting { chair, minutes: t, lesson });
            }
            None => {
                if let Some(s) = sitting {
                    finished.push((me, s.chair, s.lesson, s.minutes >= A_SITTING));
                    commands.entity(me).remove::<Sitting>();
                }
            }
        }
    }
    // The grown-up's there: the toddler sat down for it, or not, if they're in no mood.
    for (trainer, toddler) in lessons {
        let Ok((_, t_sim, _, _, mood, ..)) = sims.get(toddler) else { continue };
        let (name, willing) = (t_sim.first.clone(), mood.is_none_or(|m| m.0 >= MINIMUM_MOOD));
        let Some(chair) = trainers.iter().find(|(_, t)| **t == trainer).map(|(c, _)| *c) else { continue };
        if let Ok((.., Some(mut t), _, _)) = sims.get_mut(trainer) {
            t.asked = true;
        }
        if !willing {
            // (Turned down, as the game's `Reject`: the grown-up's done once it's played.)
            notes.push(format!("{name} isn't in the mood to be potty trained."));
            commands.entity(trainer).insert(ActionClip::new(Some(REFUSED), STANDING));
            if let Ok((_, _, mut q, ..)) = sims.get_mut(trainer)
                && let Some(a) = q.0.front_mut()
                && let (ActionKind::Object { def, .. }, Phase::Running(t)) = (a.kind.clone(), a.phase)
                && let Some(d) = interactions_for(ObjectKind::PottyChair).get(def)
            {
                a.phase = Phase::Running(t.max(d.minutes - 4.0));
            }
            commands.entity(trainer).remove::<PottyTraining>();
            continue;
        }
        if let (Ok((_, _, mut q, ..)), Some(def)) = (sims.get_mut(toddler), def_of(Special::PottyLesson)) {
            q.0.clear();
            q.push_player(Action::new("Use Potty Chair", ActionKind::Object { target: chair, def }, false));
        }
    }
    // (Encouraged while they sit, the grown-up turned to them wherever they could stand.)
    for (chair, trainer) in &trainers {
        if let (Ok((_, ctf)), Ok((.., mut tf, _))) = (chairs.get(*chair), sims.get_mut(*trainer)) {
            let to = (ctf.translation - tf.translation).with_y(0.0);
            if to.length_squared() > 1e-4 {
                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.z));
            }
        }
        let sat = sims.iter().any(|s| s.5.is_some_and(|x| x.chair == *chair && x.lesson));
        if sat && let Ok((.., Some(t), _, _)) = sims.get(*trainer) && t.asked {
            commands.entity(*trainer).insert(ActionClip::new(Some(ENCOURAGE), STANDING));
        }
    }
    // A sitting over: the chair the fuller, and a lesson learned if it was one.
    for (toddler, chair, lesson, sat) in finished {
        let v = potties.volume.entry(chair).or_insert(0);
        *v = v.saturating_add(1);
        if *v > TO_OVERFLOW
            && let Ok((_, ctf)) = chairs.get(chair)
        {
            puddles.write(crate::puddles::AddPuddle { at: ctf.translation, level: 1, lasts: None });
        }
        let trainer = trainers.get(&chair).copied();
        // (The grown-up's done once the sitting is.)
        if let Some(t) = trainer
            && let Ok((_, _, mut q, ..)) = sims.get_mut(t)
            && let Some(a) = q.0.front_mut()
        {
            a.cancel = true;
            commands.entity(t).remove::<PottyTraining>();
        }
        if !(lesson && sat) {
            continue;
        }
        let keen = trainer.and_then(|t| sims.get(t).ok()).is_some_and(|t| t.1.traits.contains(&crate::life::Trait::FamilyOriented));
        let Ok((_, sim, _, skills, ..)) = sims.get(toddler) else { continue };
        let mut s = skills.copied().unwrap_or_default();
        let before = s.potty;
        s.potty = (s.potty + LESSON * if keen { FAMILY_ORIENTED } else { 1.0 }).min(1.0);
        info!("{} had a potty lesson: {:.0}%", sim.first, s.potty * 100.0);
        if before < 1.0 && s.potty >= 1.0 {
            notes.push(format!("{} is potty trained!", sim.first));
        }
        commands.entity(toddler).insert(s);
    }
}

/// POTTY=1 (tests): the selected Sim potty trains the household's toddler, if there's one and a
/// potty chair.
fn potty_test(
    mut done: Local<bool>,
    time: Res<Time>,
    mut sel: Query<(Entity, &mut ActionQueue), With<crate::sim::Selected>>,
    toddlers: Query<(Entity, &Sim), With<crate::sim::HouseholdMember>>,
    chairs: Query<(Entity, &GameObject)>,
    mut commands: Commands,
) {
    if *done || std::env::var("POTTY").is_err() || time.elapsed_secs() < 12.0 {
        return;
    }
    let Some((toddler, _)) = toddlers.iter().find(|t| t.1.age == Age::Toddler) else { return };
    let Some((chair, _)) = chairs.iter().find(|c| c.1.kind == ObjectKind::PottyChair) else { return };
    let Ok((me, mut q)) = sel.single_mut() else { return };
    let Some(def) = interactions_for(ObjectKind::PottyChair).iter().position(|d| d.special == Special::PottyTrain) else { return };
    q.0.clear();
    q.push_player(Action::new(POTTY_TRAIN, ActionKind::Object { target: chair, def }, false));
    commands.entity(me).insert(PottyTraining { toddler, asked: false });
    info!("potty test: potty training");
    *done = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toddlers_sit_and_grown_ups_teach() {
        let d = |s| InteractionDef { special: s, ..interactions_for(ObjectKind::PottyChair)[0] };
        let mut sim = crate::sim::random_sim(&mut rand::rng(), "Test", Some(true), Age::Toddler);
        assert!(suits(&d(Special::None), &sim) && !suits(&d(Special::PottyTrain), &sim));
        sim.age = Age::Adult;
        assert!(!suits(&d(Special::None), &sim) && suits(&d(Special::PottyTrain), &sim) && suits(&d(Special::EmptyPotty), &sim));
    }
}
