//! Sims keep out of each other: the game's routing has Sims as obstacles to one another (each
//! route planned round the others, and one in the way stepping aside: `RouteAway`, `BePushed`).
//! Here, two who come too close ease apart: one walking sidesteps to pass on their right, one
//! standing about steps aside, and one busy at something (sat, using a thing, in the middle of a
//! talk or a hug) holds their ground while the other gives way. Never onto ground that can't be
//! walked on.

use bevy::prelude::*;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::ActionQueue;
use crate::nav::{Floor, NavGrid, PathFollow, UpperFloors};
use crate::sim::{Pose, Sim, SimAnim};

pub struct CrowdingPlugin;

impl Plugin for CrowdingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, keep_apart.after(crate::nav::follow_paths).run_if(in_state(PlayMode::Live)));
    }
}

/// How close two Sims' middles come before they ease apart (metres: about a body's width), and
/// how fast they give way (metres a second of game time).
const ROOM: f32 = 0.45;
const GIVE: f32 = 1.2;

/// How a Sim takes being too close to another.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gives {
    /// Walking: sidesteps, to their right.
    Walking(Vec2),
    /// Standing about: steps aside.
    Standing,
    /// Busy at something: holds their ground.
    Holds,
}

/// How far each of two moves apart (in the direction from the other to them) to get room
/// between them, as each gives way: halves when both do, all of it by one when the other holds.
pub fn shares(a: Gives, b: Gives) -> (f32, f32) {
    match (a == Gives::Holds, b == Gives::Holds) {
        (true, true) => (0.0, 0.0),
        (false, true) => (1.0, 0.0),
        (true, false) => (0.0, 1.0),
        (false, false) => (0.5, 0.5),
    }
}

/// Which way a Sim gives way from another (`away`: from the other to them): a walker, round to
/// their right of where they're headed so two meeting head on pass each other.
pub fn give_way(how: Gives, away: Vec2) -> Vec2 {
    match how {
        Gives::Walking(heading) => {
            let right = Vec2::new(-heading.y, heading.x);
            // (Head on: not straight back, but round.)
            let side = if right.dot(away) >= 0.0 { right } else { right * 0.5 };
            (away + side).normalize_or(away)
        }
        _ => away,
    }
}

#[allow(clippy::type_complexity)]
fn keep_apart(
    time: Res<Time>,
    clock: Res<crate::clock::GameClock>,
    grid: Option<Res<NavGrid>>,
    upper: Option<Res<UpperFloors>>,
    mut sims: Query<
        (Entity, &mut Transform, &Floor, &SimAnim, Option<&PathFollow>, Option<&ActionClip>, &ActionQueue, &InheritedVisibility),
        (With<Sim>, Without<crate::little::Carried>, Without<crate::carry::BeingMoved>, Without<crate::little::InCrib>),
    >,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    if dt <= 0.0 {
        return;
    }
    let people: Vec<(Entity, Vec2, u8, f32, Gives)> = sims
        .iter()
        .filter(|s| s.7.get())
        .map(|(e, tf, floor, anim, path, clip, queue, _)| {
            let walking = path.filter(|p| !p.done && p.waypoints.first().is_none_or(|w| w.climb.is_none()));
            let how = match walking {
                Some(p) => {
                    let to = p.waypoints.first().map_or(Vec2::ZERO, |w| w.p - tf.translation.xz());
                    Gives::Walking(to.normalize_or(Vec2::Y))
                }
                None if anim.pose == Pose::Stand && clip.is_none() && queue.0.is_empty() => Gives::Standing,
                // (Climbing stairs, or at something: not moved off it.)
                None => Gives::Holds,
            };
            (e, tf.translation.xz(), floor.0, tf.translation.y, how)
        })
        .collect();
    let mut moves: Vec<(Entity, Vec2)> = Vec::new();
    for (i, a) in people.iter().enumerate() {
        for b in people.iter().skip(i + 1) {
            if a.2 != b.2 || (a.3 - b.3).abs() > 1.0 {
                continue;
            }
            let d = a.1.distance(b.1);
            if d >= ROOM {
                continue;
            }
            let (sa, sb) = shares(a.4, b.4);
            if sa == 0.0 && sb == 0.0 {
                continue;
            }
            // (Right on top of each other: apart along some way.)
            let away = if d > 1e-3 { (a.1 - b.1) / d } else { Vec2::new((i as f32).cos(), (i as f32).sin()) };
            let overlap = ROOM - d;
            let step = (overlap).min(GIVE * dt);
            if sa > 0.0 {
                moves.push((a.0, give_way(a.4, away) * step * sa));
            }
            if sb > 0.0 {
                moves.push((b.0, give_way(b.4, -away) * step * sb));
            }
        }
    }
    let walkable = |level: u8, p: Vec2| {
        let g = if level <= 1 { grid.as_deref() } else { upper.as_deref().and_then(|u| u.grids.get(&level)) };
        // (Off the grids, out about town: anywhere.)
        g.is_none_or(|g| g.cell_of(p).is_none_or(|(x, z)| !g.is_blocked(x, z)))
    };
    for (e, by) in moves {
        let Ok((_, mut tf, floor, ..)) = sims.get_mut(e) else { continue };
        let to = tf.translation.xz() + by;
        if walkable(floor.0, to) {
            tf.translation.x = to.x;
            tf.translation.z = to.y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_gives_way() {
        assert_eq!(shares(Gives::Standing, Gives::Holds), (1.0, 0.0));
        assert_eq!(shares(Gives::Holds, Gives::Walking(Vec2::Y)), (0.0, 1.0));
        assert_eq!(shares(Gives::Standing, Gives::Standing), (0.5, 0.5));
        assert_eq!(shares(Gives::Holds, Gives::Holds), (0.0, 0.0));
        // (A walker meeting someone head on goes round, not back.)
        let way = give_way(Gives::Walking(Vec2::Y), -Vec2::Y);
        assert!(way.x.abs() > 0.3, "{way}");
        // (One standing is moved straight off.)
        assert_eq!(give_way(Gives::Standing, Vec2::X), Vec2::X);
    }
}
