//! Disturbances, as the game's `ReactToDisturbance` has them. A TV on or a stereo playing in the
//! room of a Sim asleep, or a baby or toddler crying there, wakes them with a Rude Awakening (not
//! a Heavy Sleeper): they get up, and turn the TV or stereo off. A baby or toddler of the
//! household crying at home gives those about Crying Baby while it cries (the game's buff
//! refreshes every few minutes it hears them).

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{HouseholdMember, Sim};

pub struct DisturbPlugin;

impl Plugin for DisturbPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, disturb.run_if(in_state(PlayMode::Live)));
    }
}

/// How near a crying little one has to be heard when they're not in a room (out of doors).
const CRY_RANGE: f32 = 12.0;

/// Whether a Sim's asleep (in bed, or napping; not already getting up).
pub(crate) fn asleep(q: &ActionQueue) -> bool {
    q.0.front().is_some_and(|a| matches!(a.label.as_str(), "Sleep" | "Nap") && matches!(a.phase, Phase::Running(_)) && !a.cancel)
}

#[allow(clippy::type_complexity)]
fn disturb(
    time: Res<Time>,
    mut last: Local<f32>,
    clock: Res<crate::clock::GameClock>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    noisy: Query<(Entity, &Transform, &GameObject), Or<(With<crate::effects::TvOn>, With<crate::music::StereoOn>)>>,
    littles: Query<(Entity, &Transform, &Sim, Option<&crate::anim::ActionClip>), With<HouseholdMember>>,
    mut sims: Query<(Entity, &Sim, &Transform, &mut Moodlets, &mut ActionQueue, &Visibility), (With<HouseholdMember>, Without<crate::visit::OnLot>)>,
) {
    let now = time.elapsed_secs();
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    let where_is = |p: Vec3| {
        building.as_deref().map(|b| {
            let level = b.level_at(p.y + 0.3);
            (level, b.room_id(level, p))
        })
    };
    let same_room = |a: Vec3, b: Vec3| match (where_is(a), where_is(b)) {
        (Some((la, Some(ra))), Some((lb, Some(rb)))) => la == lb && ra == rb,
        _ => a.distance(b) <= CRY_RANGE,
    };
    // The noise makers on, and the little ones crying (their crying clips).
    let noises: Vec<(Entity, Vec3, ObjectKind)> = noisy.iter().map(|(e, tf, o)| (e, tf.translation, o.kind)).collect();
    let crying: Vec<(Entity, Vec3)> = littles
        .iter()
        .filter(|(_, _, s, clip)| s.age.is_little() && clip.is_some_and(|c| c.loops.first().is_some_and(|l| l.contains("cry"))))
        .map(|(e, tf, ..)| (e, tf.translation))
        .collect();
    for (me, sim, tf, mut moodlets, mut queue, vis) in &mut sims {
        if sim.age.is_little() || *vis == Visibility::Hidden {
            continue;
        }
        let at = tf.translation;
        // Woken: a TV or stereo in the room, or a little one crying there.
        if asleep(&queue) && !sim.traits.contains(&Trait::HeavySleeper) {
            let noise = noises.iter().find(|n| same_room(at, n.1));
            let cry = crying.iter().find(|c| c.0 != me && same_room(at, c.1));
            if noise.is_some() || cry.is_some() {
                info!("{} is woken by {}", sim.first, if noise.is_some() { "the noise" } else { "a crying baby" });
                moodlets.add(MoodletKind::RudeAwakening, clock.minutes);
                if let Some(a) = queue.0.front_mut() {
                    a.cancel = true;
                }
                // (Then off with whatever woke them, as the game's reaction does.)
                if let Some(&(target, _, kind)) = noise
                    && let Some(def) = crate::interact::interactions_for(kind).iter().position(|d| d.name == "Turn Off")
                {
                    let at = queue.0.len().min(1);
                    queue.0.insert(at, Action::new("Turn Off", ActionKind::Object { target, def }, false));
                }
            }
        }
        // A little one of theirs crying at home.
        let hears = crying.iter().any(|c| same_room(at, c.1) || at.distance(c.1) <= CRY_RANGE);
        if hears && !moodlets.has(MoodletKind::CryingBaby) {
            debug!("{} hears a baby crying", sim.first);
        }
        // (Kept while they hear it, its few minutes topped up as they run out; gone when it's
        // quiet. Only touched when it changes, so the moodlets aren't redrawn every second.)
        let left = moodlets.0.iter().find(|m| m.kind == MoodletKind::CryingBaby).map(|m| m.until - clock.minutes);
        if hears && left.is_none_or(|l| l < 5.0) {
            moodlets.add(MoodletKind::CryingBaby, clock.minutes);
        } else if !hears && left.is_some() {
            moodlets.remove(MoodletKind::CryingBaby);
        }
    }
}
