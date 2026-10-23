//! Bathroom grime, as the game's `CleanableComponent`: toilets, showers, tubs and sinks get
//! dirtier with every use (Neat Sims leave less, slobs more), look it (the model's own `Dirty`
//! and `VeryDirty` materials; a clogged toilet its `Clogged` one) and are offered to be cleaned
//! once they need it.

use bevy::prelude::*;

use crate::interact::{Broken, GameObject, ObjectKind};

pub struct GrimePlugin;

impl Plugin for GrimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (grime_objects, dirty_looks).run_if(in_state(crate::AppState::InGame)));
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

impl Grime {
    /// A use's dirt, as the user is: the game's `kDirtyIncNeatMultiplier` and slob's.
    pub fn used(&mut self, traits: &[crate::life::Trait]) {
        let k = if traits.contains(&crate::life::Trait::Neat) {
            0.8
        } else if traits.contains(&crate::life::Trait::Slob) {
            1.2
        } else {
            1.0
        };
        self.0 = (self.0 + PER_USE * k).min(100.0);
    }

    pub fn needs_cleaning(&self) -> bool {
        self.0 >= NEEDS_CLEANING
    }
}

/// The kinds that get dirty.
pub fn cleanable(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Toilet | ObjectKind::Shower | ObjectKind::Bathtub | ObjectKind::ShowerTub | ObjectKind::Sink | ObjectKind::LitterBox | ObjectKind::Terrarium | ObjectKind::BirdCage)
}

/// Bathroom things start clean.
fn grime_objects(mut commands: Commands, objects: Query<(Entity, &GameObject), Without<Grime>>) {
    for (e, o) in &objects {
        if cleanable(o.kind) {
            commands.entity(e).insert(Grime::default());
        }
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
