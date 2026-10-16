//! Ambrosia, as the game's: the food of the gods, made by a Sim who's mastered Cooking,
//! Gardening and Fishing (`RecipeMakeTest.AmbrosiaMakeTest`), from a Lifefruit and a Deathfish
//! (neither to be bought). It's set out a single serving (the game's `ServingContainerSingle`),
//! for whoever's to eat it. Eaten, it's a Divine Meal (+75 for a week, `BuffDivineMeal`) and the
//! eater's age winds back to the start of their stage, a tenth of it kept
//! (`kAmbrosiaAgingRewindPercentage`). Left out at night, it draws a ghost of the household
//! (`kAmbrosiaGhostEatScore`): the ghost eats, and is alive again (`Urnstone`'s
//! "ResurrectionAmbrosia"), back in the household.

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{ActionQueue, Notifications};
use crate::life::{MoodletKind, Moodlets};
use crate::sim::{HouseholdMember, Sim};

pub struct AmbrosiaPlugin;

impl Plugin for AmbrosiaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (learn_ambrosia, ate_ambrosia, ghost_feast, grave_test).run_if(in_state(PlayMode::Live)));
    }
}

/// The recipe's key.
pub const AMBROSIA: &str = "Ambrosia";
/// The skills it takes, and how far (`kCookingSkillForAmbrosia`...).
const MASTERED: [&str; 3] = ["Cooking", "Gardening", "Fishing"];
const MASTERY: u32 = 10;
/// What's kept of their stage's days.
const REWIND_KEEP: f32 = 0.1;
/// The Divine Meal: its mood, a week.
const DIVINE_VALUE: i32 = 75;
const DIVINE_MINUTES: f64 = 7.0 * 1440.0;
/// How near a ghost must be to eat it.
const REACH: f32 = 1.2;

/// Whether a Sim can make it.
pub fn can_make(skills: &crate::interact::Skills) -> bool {
    MASTERED.iter().all(|s| skills.level(s) >= MASTERY)
}

/// Mastering the three, a Sim knows how to make it.
fn learn_ambrosia(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &crate::interact::Skills, Option<&mut crate::meals::KnownRecipes>), With<HouseholdMember>>,
    mut notes: ResMut<Notifications>,
) {
    for (e, sim, skills, known) in &mut sims {
        if !can_make(skills) || known.as_ref().is_some_and(|k| k.0.iter().any(|r| r == AMBROSIA)) {
            continue;
        }
        match known {
            Some(mut k) => k.0.push(AMBROSIA.to_string()),
            None => {
                commands.entity(e).insert(crate::meals::KnownRecipes(vec![AMBROSIA.to_string()]));
            }
        }
        notes.push(format!("{} has mastered Cooking, Gardening and Fishing, and knows how to make Ambrosia!", sim.first));
    }
}

/// Ambrosia eaten (a living Sim's plateful).
#[derive(Component)]
pub struct AteAmbrosia;

fn ate_ambrosia(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, Option<&mut crate::aging::Aging>, &mut Moodlets), With<AteAmbrosia>>,
    mut notes: ResMut<Notifications>,
) {
    for (e, sim, aging, mut m) in &mut sims {
        commands.entity(e).remove::<AteAmbrosia>();
        m.add_valued(MoodletKind::DivineMeal, clock.minutes, DIVINE_VALUE, DIVINE_MINUTES);
        if let Some(mut a) = aging {
            a.days *= REWIND_KEEP;
        }
        notes.push(format!("{} ate Ambrosia: divine! The years fall away, back to the start of this stage of {} life.", sim.first, if sim.female { "her" } else { "his" }));
    }
}

/// GRAVE=1 (tests): a grave by the selected Sim a few seconds in (a stranger of the household's
/// past, dead of old age), for its ghost to rise from.
#[allow(clippy::too_many_arguments)]
fn grave_test(
    mut commands: Commands,
    mut done: Local<bool>,
    time: Res<Time>,
    sel: Query<&Transform, With<crate::sim::Selected>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    if *done || std::env::var("GRAVE").is_err() || time.elapsed_secs() < 8.0 {
        return;
    }
    let Ok(tf) = sel.single() else { return };
    *done = true;
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    let Some(entry) = data.0.catalog.iter().find(|c| c.instance_name == "UrnstoneHuman") else { return };
    let at = tf.translation + tf.rotation * Vec3::new(1.5, 0.0, 1.5);
    let Some(stone) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, entry.objd, at, tf.rotation) else { return };
    let sim = crate::sim::random_sim(&mut rand::rng(), "Ghostly", Some(false), crate::sim::Age::Elder);
    info!("grave test: {} lies at {at}", sim.full_name());
    commands.entity(stone.entity).insert(crate::ghosts::Grave::new(sim, "of old age"));
}

/// A ghost on its way to the Ambrosia (not drifting about).
#[derive(Component)]
pub struct Feasting;

/// A ghost out at night drifts to the Ambrosia set out, eats it and lives again.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn ghost_feast(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    grid: Option<Res<crate::nav::NavGrid>>,
    ghosts: Query<(Entity, &crate::ghosts::Ghost, &Transform, &Sim, Option<&crate::nav::PathFollow>), Without<crate::ghosts::Resting>>,
    graves: Query<&crate::ghosts::Grave>,
    mut meals: Query<(Entity, &mut crate::meals::Meal, &crate::meals::Dish, &Transform)>,
    mut notes: ResMut<Notifications>,
    mut family: ResMut<crate::family::Genealogy>,
) {
    let (Some(ui), Some(grid)) = (ui, grid) else { return };
    let Some(ambrosia) = ui.data.recipes.iter().position(|r| r.key == AMBROSIA) else { return };
    let mut feasts: Vec<(Entity, Vec3)> = meals.iter().filter(|m| m.2.0 == ambrosia && m.1.servings > 0).map(|m| (m.0, m.3.translation)).collect();
    if feasts.is_empty() {
        return;
    }
    for (g, ghost, tf, sim, path) in &ghosts {
        let Some(i) = feasts.iter().enumerate().min_by(|a, b| a.1.1.distance(tf.translation).total_cmp(&b.1.1.distance(tf.translation))).map(|(i, _)| i) else { break };
        let (meal, at) = feasts[i];
        // Within reach: eaten, and alive again.
        if at.xz().distance(tf.translation.xz()) < REACH {
            let Ok(mut m) = meals.get_mut(meal) else { continue };
            m.1.servings = 0;
            feasts.remove(i);
            // (Eaten, dish and all.)
            commands.entity(meal).try_despawn();
            let Ok(grave) = graves.get(ghost.grave) else { continue };
            commands.entity(g).try_despawn();
            commands.entity(ghost.grave).try_despawn();
            let back = Sim { ..grave.sim.clone() };
            family.note(back.id, &back.full_name(), back.female);
            commands.spawn((
                Transform::from_translation(tf.translation.with_y(at.y.min(tf.translation.y))),
                Visibility::default(),
                back,
                crate::sim::Motives::default(),
                crate::sim::DecayScale::default(),
                crate::social::Relationships::default(),
                crate::interact::Skills::default(),
                crate::sim::SimAnim::default(),
                crate::anim::ClipPlayer::default(),
                crate::aging::NeedsNewBody,
                crate::nav::Floor(1),
                ActionQueue::default(),
                HouseholdMember,
                crate::aging::Aging::default(),
                DespawnOnExit(crate::AppState::InGame),
            ))
            .with_children(|c| {
                c.spawn((Transform::default(), Visibility::default()));
            });
            notes.push(format!("The ghost of {} ate the Ambrosia and has come back to life!", sim.full_name()));
            continue;
        }
        // (On the way already.)
        if path.is_some_and(|p| !p.done) {
            continue;
        }
        if let Some(wp) = crate::nav::plan_route(&grid, None, tf.translation.xz(), 1, at.xz(), 1) {
            commands.entity(g).remove::<crate::anim::ActionClip>().insert((crate::nav::PathFollow::new(wp), Feasting));
            debug!("the ghost of {} drifts to the Ambrosia at {:?} ({:.0})", sim.first, at, clock.minutes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambrosia_takes_mastery_of_all_three() {
        let mut skills = crate::interact::Skills::default();
        for s in ["Cooking", "Gardening"] {
            skills.0.insert(crate::save::SKILLS.iter().find(|n| **n == s).unwrap(), 10.0);
        }
        assert!(!can_make(&skills));
        skills.0.insert(crate::save::SKILLS.iter().find(|n| **n == "Fishing").unwrap(), 10.0);
        assert!(can_make(&skills));
    }
}
