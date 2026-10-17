//! Smashing the dollhouse, as the game's `DollHouse.Smash`: a Sim in a very bad mood (the game's
//! strongly negative mood, `kMoodStrongNegativeValue` -50) takes it out on the dollhouse, eight
//! minutes of it (a child's eighteen: `kAdultTimeToSmash`, `kChildTimeToSmash`); by free will
//! only the Evil, the Grumpy, the Inappropriate and the Hot-Headed. (Played with by children,
//! and by grown-ups only if Childish: the game's `Play` test.) It's left smashed (the
//! model's own `dollhouseSmashed` geometry state), no use to play with, till it's thrown away
//! (`DisposeDollHouse`). Kept in saves.

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::GameObject;
use crate::life::Trait;
use crate::sim::Sim;

pub struct DollhousePlugin;

impl Plugin for DollhousePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (smashed, show_smashed, thrown_away).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The model's geometry state when smashed.
pub const SMASHED_STATE: &str = "dollhouseSmashed";
/// `MoodManager.kMoodStrongNegativeValue`.
const STRONG_NEGATIVE_MOOD: f32 = -50.0;
/// The game's `kAdultTimeToSmash` and `kChildTimeToSmash` (game minutes).
pub const ADULT_MINUTES: f32 = 8.0;
pub const CHILD_MINUTES: f32 = 18.0;
/// The last blow and getting up (seconds: the smash's `smash` and `stop` clips, a grown-up's
/// and a child's).
pub const ADULT_END_SECS: f32 = 5.4;
pub const CHILD_END_SECS: f32 = 9.2;
/// How keen those who'd smash it are, by free will.
const SMASH_PULL: f32 = 30.0;

/// A smashed dollhouse.
#[derive(Component)]
pub struct Smashed;

/// Drawn smashed already.
#[derive(Component)]
struct ShownSmashed;

/// A Sim done smashing a dollhouse, or throwing one away (its outcome to be had).
#[derive(Component)]
pub enum DollhouseDone {
    Smashed(Entity),
    ThrownAway(Entity),
}

/// Whether they'd play with it: children, and grown-ups only if Childish.
pub fn may_play(sim: &Sim) -> bool {
    sim.age == crate::sim::Age::Child || sim.traits.contains(&Trait::Childish)
}

/// How long smashing it takes them (game minutes): a child longer.
pub fn smash_minutes(age: crate::sim::Age) -> f32 {
    if age == crate::sim::Age::Child { CHILD_MINUTES } else { ADULT_MINUTES }
}

/// Whether they're in the mood to smash it (strongly negative: the game's test).
pub fn may_smash(mood: f32) -> bool {
    mood <= STRONG_NEGATIVE_MOOD
}

/// How keen they are by free will: only the Evil, the Grumpy, the Inappropriate and the
/// Hot-Headed, in that mood.
pub fn drawn(sim: &Sim, mood: f32) -> Option<f32> {
    let by_nature = [Trait::Evil, Trait::Grumpy, Trait::Inappropriate, Trait::HotHeaded].iter().any(|t| sim.traits.contains(t));
    (by_nature && may_smash(mood)).then_some(SMASH_PULL)
}

fn smashed(mut commands: Commands, sims: Query<(Entity, &Sim, &DollhouseDone)>, houses: Query<&GameObject>, mut notes: ResMut<crate::interact::Notifications>) {
    for (e, sim, done) in &sims {
        let DollhouseDone::Smashed(house) = *done else { continue };
        commands.entity(e).remove::<DollhouseDone>();
        if houses.get(house).is_ok() {
            commands.entity(house).insert(Smashed);
            notes.push(format!("{} smashed the dollhouse to pieces!", sim.first));
        }
    }
}

/// A smashed dollhouse drawn so: its model's smashed geometry state.
#[allow(clippy::type_complexity)]
fn show_smashed(
    mut commands: Commands,
    houses: Query<(Entity, &GameObject, Option<&crate::objects::Design>), (With<Smashed>, Without<ShownSmashed>)>,
    data: Option<Res<crate::baked::Baked>>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(data) = data else { return };
    for (e, o, design) in &houses {
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let parts = assets.object_state_design(&mut ctx, o.objd, Some(s3pkg::fnv32(SMASHED_STATE)), design.map(|d| d.0));
        crate::objects::reshape_parts(&mut commands, e, &parts);
        commands.entity(e).insert(ShownSmashed);
    }
}

/// Thrown away: gone from the lot (the lot's own noted as gone, for the save).
#[allow(clippy::type_complexity)]
fn thrown_away(
    mut commands: Commands,
    sims: Query<(Entity, &Sim, &DollhouseDone)>,
    houses: Query<(&GameObject, &Transform, Has<crate::save::Bought>)>,
    mut removed: ResMut<crate::save::RemovedLotObjects>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    for (e, sim, done) in &sims {
        let DollhouseDone::ThrownAway(house) = *done else { continue };
        commands.entity(e).remove::<DollhouseDone>();
        let Ok((o, tf, bought)) = houses.get(house) else { continue };
        if !bought {
            crate::save::note_removed(&mut removed, o, tf);
        }
        commands.entity(house).try_despawn();
        notes.push(format!("{} threw the smashed dollhouse away.", sim.first));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_bad_tempered_smash_it_by_free_will_and_only_when_miserable() {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "D", Some(true), crate::sim::Age::Child);
        s.traits = vec![Trait::Grumpy];
        assert_eq!(drawn(&s, -60.0), Some(SMASH_PULL));
        assert_eq!(drawn(&s, -20.0), None);
        s.traits = vec![Trait::Friendly];
        assert_eq!(drawn(&s, -80.0), None);
        assert!(may_smash(-50.0) && !may_smash(-49.0));
    }

    #[test]
    fn played_with_by_children_and_the_childish_and_smashed_longer_by_children() {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "D", Some(true), crate::sim::Age::Adult);
        s.traits = vec![Trait::Grumpy];
        assert!(!may_play(&s));
        s.traits.push(Trait::Childish);
        assert!(may_play(&s));
        s.age = crate::sim::Age::Child;
        s.traits.clear();
        assert!(may_play(&s));
        assert_eq!(smash_minutes(crate::sim::Age::Child), 18.0);
        assert_eq!(smash_minutes(crate::sim::Age::Teen), 8.0);
    }
}
