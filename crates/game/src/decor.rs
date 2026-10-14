//! Room decor, as the game judges it (`SimUpdate.CalculateSimEnvironmentScore`): the things in
//! the room a household Sim is in at home (or near them outdoors) add up, the best first and the
//! rest for less the bigger the room, to Decorated, Nicely Decorated or Beautifully Decorated.
//! Each thing counts its class's `EnvironmentScore`; one dirty, broken or burnt counts nothing,
//! and a Snob takes it all twice as much to heart.

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{GameObject, ObjectRates};
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{HouseholdMember, Sim};

pub struct DecorPlugin;

impl Plugin for DecorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, decor.run_if(in_state(PlayMode::Live)));
    }
}

/// The game's `EnvironmentScore` tunables: how far outdoors things count (metres), the sizes
/// (square root of the tiles) of a small and a large room, how much the best thing counts in
/// each and outdoors, and after how many things the rest count for nothing.
const OUTDOOR_RANGE: f32 = 10.0;
const SMALL_ROOM: f32 = 5.0;
const LARGE_ROOM: f32 = 20.0;
const SHARE_SMALL: f32 = 1.0;
const SHARE_LARGE: f32 = 0.5;
const SHARE_OUTSIDE: f32 = 1.0;
const OBJECTS_SMALL: f32 = 5.0;
const OBJECTS_LARGE: f32 = 15.0;
const OBJECTS_OUTSIDE: f32 = 10.0;
/// `TraitTuning.kSnobTraitEnvironmentScoreMultiplier`.
const SNOB: f32 = 2.0;
/// The decor scores the moodlets start at (the game's `Decoration` motive's buff triggers).
const DECORATED: f32 = 16.0;
const NICELY: f32 = 41.0;
const BEAUTIFULLY: f32 = 99.0;

/// A room's (or the outdoors') decor score from its things' environment scores, as the game's
/// `CalculateContributingObjectScore`: the best counts most, each after it less, up to 100.
pub fn room_score(mut scores: Vec<f32>, tiles: f32, outside: bool) -> f32 {
    let (share, count) = if outside {
        (SHARE_OUTSIDE, OBJECTS_OUTSIDE)
    } else {
        let size = tiles.sqrt();
        if size <= SMALL_ROOM {
            (SHARE_SMALL, OBJECTS_SMALL)
        } else if size >= LARGE_ROOM {
            (SHARE_LARGE, OBJECTS_LARGE)
        } else {
            let f = (size - SMALL_ROOM) / (LARGE_ROOM - SMALL_ROOM);
            (SHARE_LARGE.min(SHARE_SMALL) + f * (SHARE_SMALL - SHARE_LARGE).abs(), (OBJECTS_SMALL + f * (OBJECTS_LARGE - OBJECTS_SMALL)).round())
        }
    };
    scores.retain(|s| *s > 0.0);
    scores.sort_by(|a, b| b.total_cmp(a));
    let mut total = 0.0;
    for (n, s) in scores.into_iter().enumerate() {
        let part = (-share * n as f32 / count + share).max(0.0);
        if part == 0.0 {
            break;
        }
        total += s * part;
    }
    total.clamp(0.0, 100.0)
}

/// Which decor moodlet a score earns.
pub fn decor_moodlet(score: f32) -> Option<MoodletKind> {
    match score {
        s if s >= BEAUTIFULLY => Some(MoodletKind::BeautifullyDecorated),
        s if s >= NICELY => Some(MoodletKind::NicelyDecorated),
        s if s >= DECORATED => Some(MoodletKind::Decorated),
        _ => None,
    }
}

#[allow(clippy::type_complexity)]
fn decor(
    time: Res<Time>,
    mut last: Local<f32>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    rates: Res<ObjectRates>,
    mut sims: Query<(&Sim, &Transform, &mut Moodlets, &Visibility), (With<HouseholdMember>, Without<crate::visit::OnLot>)>,
    objects: Query<
        (&GameObject, &Transform, Option<&crate::grime::Grime>, Has<crate::interact::Broken>, Has<crate::grime::Charred>),
        (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>),
    >,
) {
    let now = time.elapsed_secs();
    if now - *last < 2.0 {
        return;
    }
    *last = now;
    let Some(b) = building else { return };
    let where_is = |p: Vec3| {
        let level = b.level_at(p.y + 0.3);
        (level, b.room_id(level, p))
    };
    // Each thing's score, and where it is.
    let things: Vec<(Vec3, u8, Option<u16>, f32)> = objects
        .iter()
        .filter_map(|(o, tf, grime, broken, charred)| {
            let base = rates.0.get(&o.objd).map_or(0.0, |r| r.environment);
            // (Dirty, broken or burnt, it's no ornament.)
            let spoilt = broken || charred || grime.is_some_and(|g| g.0 >= crate::grime::NEEDS_CLEANING);
            let score = if spoilt { 0.0 } else { base };
            (score > 0.0).then(|| {
                let (level, room) = where_is(tf.translation);
                (tf.translation, level, room, score)
            })
        })
        .collect();
    for (sim, tf, mut moodlets, vis) in &mut sims {
        let score = if *vis == Visibility::Hidden {
            0.0
        } else {
            let (level, room) = where_is(tf.translation);
            let snob = if sim.traits.contains(&Trait::Snob) { SNOB } else { 1.0 };
            match room {
                Some(r) => {
                    let scores = things.iter().filter(|t| t.1 == level && t.2 == Some(r)).map(|t| t.3 * snob).collect();
                    room_score(scores, b.room_tiles(level, r) as f32, false)
                }
                None => {
                    let near = |t: &&(Vec3, u8, Option<u16>, f32)| t.2.is_none() && t.0.with_y(0.0).distance(tf.translation.with_y(0.0)) <= OUTDOOR_RANGE;
                    room_score(things.iter().filter(near).map(|t| t.3 * snob).collect(), 0.0, true)
                }
            }
        };
        let earned = decor_moodlet(score);
        if let Some(kind) = earned
            && !moodlets.has(kind)
        {
            debug!("{} finds the room {:?} (decor {score:.0})", sim.first, kind);
        }
        for kind in [MoodletKind::Decorated, MoodletKind::NicelyDecorated, MoodletKind::BeautifullyDecorated] {
            moodlets.set_while(kind, earned == Some(kind));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooms_are_judged_as_the_game_does() {
        // (A small room: the best thing in full, each after it a fifth less.)
        assert_eq!(room_score(vec![10.0, 10.0], 9.0, false), 18.0);
        assert_eq!(room_score(vec![10.0; 8], 9.0, false), 30.0, "nothing past the fifth");
        // (A large room: the best counts half.)
        assert_eq!(room_score(vec![10.0], 400.0, false), 5.0);
        assert_eq!(room_score(vec![-10.0, 4.0], 9.0, false), 4.0, "the unsightly don't count here");
        assert_eq!(room_score(vec![100.0, 100.0], 9.0, false), 100.0);
        assert_eq!(decor_moodlet(15.0), None);
        assert_eq!(decor_moodlet(18.0), Some(MoodletKind::Decorated));
        assert_eq!(decor_moodlet(41.0), Some(MoodletKind::NicelyDecorated));
        assert_eq!(decor_moodlet(100.0), Some(MoodletKind::BeautifullyDecorated));
    }
}
