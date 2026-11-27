//! Bathroom and kitchen grime, as the game's `CleanableComponent`: toilets, showers, tubs and
//! sinks get dirtier with every use, a stove with every meal cooked on it (`StoveCleanable`, its
//! tuned `DirtyIncPerUse` 9 to the bathroom's 5), Neat Sims leaving less and slobs more; they
//! look it (the model's own `Dirty` and `VeryDirty` materials; a clogged toilet its `Clogged`
//! one) and are offered to be cleaned once they need it ("Clean Stove", the game's sponge and
//! scrub). A Self-Cleaning one never needs it.

use bevy::prelude::*;

use crate::interact::{Broken, GameObject, ObjectKind};

pub struct GrimePlugin;

impl Plugin for GrimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (grime_objects, dirty_looks).run_if(in_state(crate::AppState::InGame)))
            .add_systems(Update, stove_test.run_if(in_state(crate::PlayMode::Live)));
    }
}

/// Burnt in a fire: charred, its `Burnt` look, until it's mended (the game's `Charred`).
#[derive(Component)]
pub struct Charred;

/// How dirty something is: 0 clean, 100 filthy (the game's dirty level, turned about).
#[derive(Component, Default)]
pub struct Grime(pub f32);

/// What a use leaves (the game's `DirtyIncPerUse`).
pub const PER_USE: f32 = 5.0;
/// From here it's offered to be cleaned (`DirtyThreshold`); it looks dirty from the next
/// (`ChangeToDirtyState`), and very dirty from the last (`ChangeToVeryDirtyState`).
pub const NEEDS_CLEANING: f32 = 25.0;
const LOOKS_DIRTY: f32 = 30.0;
const LOOKS_VERY_DIRTY: f32 = 60.0;

/// A stove's per meal (`StoveExpensive`'s tuned `DirtyIncPerUse`).
pub const STOVE_PER_USE: f32 = 9.0;

impl Grime {
    /// A use's dirt, as the user is: the game's `kDirtyIncNeatMultiplier` and slob's.
    pub fn used(&mut self, traits: &[crate::life::Trait]) {
        self.used_by(PER_USE, traits);
    }

    /// A use leaving so much dirt, as the user is.
    pub fn used_by(&mut self, per_use: f32, traits: &[crate::life::Trait]) {
        let k = if traits.contains(&crate::life::Trait::Neat) {
            0.8
        } else if traits.contains(&crate::life::Trait::Slob) {
            1.2
        } else {
            1.0
        };
        self.0 = (self.0 + per_use * k).min(100.0);
    }

    pub fn needs_cleaning(&self) -> bool {
        self.0 >= NEEDS_CLEANING
    }
}

/// The kinds that get dirty.
pub fn cleanable(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Toilet | ObjectKind::Shower | ObjectKind::Bathtub | ObjectKind::ShowerTub | ObjectKind::Sink | ObjectKind::LitterBox | ObjectKind::Terrarium | ObjectKind::BirdCage | ObjectKind::BoxStall | ObjectKind::Stove)
}

/// What a use of it leaves.
pub fn per_use(kind: ObjectKind) -> f32 {
    if kind == ObjectKind::Stove { STOVE_PER_USE } else { PER_USE }
}

/// Bathroom things start clean.
fn grime_objects(mut commands: Commands, objects: Query<(Entity, &GameObject), Without<Grime>>) {
    for (e, o) in &objects {
        if cleanable(o.kind) {
            commands.entity(e).insert(Grime::default());
        }
    }
}

/// `STOVE_TEST=menu|clean` (tests): ten seconds in, the lot's stove filthy, the view on it, and
/// its menu up or the selected Sim cleaning it.
fn stove_test(
    mut commands: Commands,
    time: Res<Time>,
    mut stoves: Query<(Entity, &GameObject, &GlobalTransform, Option<&mut Grime>)>,
    mut sims: Query<&mut crate::interact::ActionQueue, With<crate::sim::Selected>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut done: Local<bool>,
) {
    let Ok(what) = std::env::var("STOVE_TEST") else { return };
    let t = time.elapsed_secs();
    let Some((e, _, at, grime)) = stoves.iter_mut().find(|(_, o, ..)| o.kind == ObjectKind::Stove) else { return };
    if t > 7.0
        && let Ok(mut cam) = cams.single_mut()
    {
        let front = at.rotation() * Vec3::Z;
        cam.look_at(at.translation() + Vec3::Y * 0.5);
        cam.distance = 6.0;
        cam.yaw = front.x.atan2(front.z) + 0.5;
    }
    if t > 8.0
        && let Some(mut g) = grime
        && !*done
    {
        g.0 = 70.0;
    }
    if *done || t < 10.0 {
        return;
    }
    *done = true;
    if what == "menu" {
        commands.insert_resource(crate::hud::PieFor(e));
        return;
    }
    let Some(def) = crate::interact::interactions_for(ObjectKind::Stove).iter().position(|d| d.special == crate::interact::Special::Scrub) else { return };
    if let Ok(mut q) = sims.single_mut() {
        q.push_player(crate::interact::Action::new("Clean Stove", crate::interact::ActionKind::Object { target: e, def }, false));
    }
}

/// A part's other looks (the model's material states), and its own while it wears another.
#[derive(Component)]
pub struct MaterialStates {
    pub states: Vec<(u32, Handle<StandardMaterial>)>,
    pub own: Option<Handle<StandardMaterial>>,
    pub shown: u32,
}

/// Dirty objects' parts in their dirty looks, and back when they're clean; a clogged toilet in
/// its clogged one.
fn dirty_looks(
    mut parts: Query<(&mut MaterialStates, &mut MeshMaterial3d<StandardMaterial>, &ChildOf)>,
    up: Query<&ChildOf>,
    objects: Query<(&GameObject, Option<&Grime>, Has<Broken>, Has<Charred>)>,
) {
    use s3formats::model::{STATE_BURNT, STATE_CLOGGED, STATE_DIRTY, STATE_VERY_DIRTY};
    for (mut states, mut mat, parent) in &mut parts {
        // (The object: the part's model's parent, or the model itself.)
        let object = [parent.parent()].into_iter().chain(up.get(parent.parent()).ok().map(|p| p.parent())).find(|e| objects.contains(*e));
        let Some((o, grime, broken, charred)) = object.and_then(|e| objects.get(e).ok()) else { continue };
        let g = grime.map_or(0.0, |g| g.0);
        let want = if charred {
            STATE_BURNT
        } else if broken && o.kind == ObjectKind::Toilet {
            STATE_CLOGGED
        } else if g >= LOOKS_VERY_DIRTY {
            STATE_VERY_DIRTY
        } else if g >= LOOKS_DIRTY {
            STATE_DIRTY
        } else {
            0
        };
        // (A state it hasn't got: the next best, or its own.)
        let want = if want == STATE_VERY_DIRTY && !states.states.iter().any(|s| s.0 == want) { STATE_DIRTY } else { want };
        let found = states.states.iter().find(|s| s.0 == want).map(|s| s.1.clone());
        let want = if found.is_some() { want } else { 0 };
        if want == states.shown {
            continue;
        }
        if states.shown == 0 {
            states.own = Some(mat.0.clone());
        }
        match found {
            Some(h) => mat.0 = h,
            None => {
                if let Some(own) = states.own.take() {
                    mat.0 = own;
                }
            }
        }
        states.shown = want;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stove_gets_dirtier_than_a_sink_with_each_use() {
        assert!(cleanable(ObjectKind::Stove) && cleanable(ObjectKind::Sink));
        let (mut stove, mut sink) = (Grime::default(), Grime::default());
        stove.used_by(per_use(ObjectKind::Stove), &[]);
        sink.used_by(per_use(ObjectKind::Sink), &[]);
        assert_eq!((stove.0, sink.0), (9.0, 5.0));
        // (Three meals cooked and it wants cleaning; a Neat cook's leave less.)
        stove.used_by(per_use(ObjectKind::Stove), &[]);
        stove.used_by(per_use(ObjectKind::Stove), &[]);
        assert!(stove.needs_cleaning());
        let mut neat = Grime::default();
        neat.used_by(STOVE_PER_USE, &[crate::life::Trait::Neat]);
        assert!((neat.0 - 7.2).abs() < 1e-4);
    }
}
