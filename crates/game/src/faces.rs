//! Faces that show how a Sim feels: now and then one of the game's facial expressions (its
//! `FacialOverlays`: a smile, a frown, a sneer, wide eyes) plays over whatever the body is
//! doing, on the face alone. Which kind is the strongest thing they feel: disgust when smelly or
//! in a mess, fear when scared, sleepiness when tired, anger when annoyed, sadness when lonely,
//! heartbroken or low, happiness in a good mood; the Evil now and then grin as the Evil do.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::anim::{ClipLibrary, FaceOverlay};
use crate::clock::{GameClock, SPEED_RATES};
use crate::life::{Mood, MoodletKind as M, Moodlets, Trait};
use crate::sim::{Pose, Sim, SimAnim};

pub struct FacesPlugin;

impl Plugin for FacesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, faces.run_if(in_state(PlayMode::Live)));
    }
}

/// When a Sim's face next shows something (game minutes).
#[derive(Component)]
struct NextFace(f64);

/// The kind of expression for how a Sim feels just now, if any.
fn feeling(sim: &Sim, mood: f32, moodlets: Option<&Moodlets>, rng: &mut impl Rng) -> Option<&'static str> {
    let has = |k: &[M]| moodlets.is_some_and(|m| m.0.iter().any(|b| k.contains(&b.kind)));
    if has(&[M::Scared, M::Singed]) {
        Some("FacialFear")
    } else if has(&[M::Smelly, M::Nauseous, M::FilthySurroundings, M::DirtySurroundings]) {
        Some("FacialDisgust")
    } else if has(&[M::Annoyed, M::Fired, M::BadDate]) {
        Some("FacialAnger")
    } else if has(&[M::Exhausted, M::Tired, M::Fatigued]) {
        Some("FacialSleepy")
    } else if has(&[M::Heartbroken, M::Lonely]) || mood <= -40.0 {
        Some("FacialSad")
    } else if sim.traits.contains(&Trait::Evil) && rng.random_bool(0.3) {
        Some("FacialEvil")
    } else if mood >= 25.0 {
        Some("FacialHappy")
    } else {
        None
    }
}

#[allow(clippy::type_complexity)]
fn faces(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    data: Res<crate::baked::Baked>,
    mut lib: ResMut<ClipLibrary>,
    mut sims: Query<(Entity, &Sim, &SimAnim, &Mood, Option<&Moodlets>, Option<&mut FaceOverlay>, Option<&mut NextFace>, &InheritedVisibility)>,
) {
    let Some(ui) = ui else { return };
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed];
    let mut rng = rand::rng();
    for (e, sim, anim, mood, moodlets, overlay, next, shown) in &mut sims {
        // An expression under way, until its clip's done.
        if let Some(mut f) = overlay {
            f.time += dt;
            let length = lib.get(&data, &f.clip).map_or(0.0, |c| c.duration);
            if f.time >= length || matches!(anim.pose, Pose::Lie | Pose::Talk) {
                commands.entity(e).try_remove::<FaceOverlay>();
            }
            continue;
        }
        if sim.age.is_little() {
            continue;
        }
        let Some(mut next) = next else {
            commands.entity(e).try_insert(NextFace(clock.minutes + rng.random_range(3.0..10.0)));
            continue;
        };
        // (Not asleep, nor off stage, nor talking: a social's clips make its own faces.)
        if clock.minutes < next.0 || matches!(anim.pose, Pose::Lie | Pose::Talk) || !shown.get() {
            continue;
        }
        next.0 = clock.minutes + rng.random_range(6.0..16.0);
        let Some(kind) = feeling(sim, mood.0, moodlets, &mut rng) else { continue };
        let Some((_, clips)) = ui.data.idles.faces.iter().find(|(k, _)| k == kind) else { continue };
        let Ok((clip, _)) = clips.choose_weighted(&mut rng, |c| c.1.max(0.01)) else { continue };
        if lib.get(&data, clip).is_none() {
            continue;
        }
        commands.entity(e).try_insert(FaceOverlay { clip: clip.clone(), time: 0.0 });
    }
}
