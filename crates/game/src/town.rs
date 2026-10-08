//! Town life: townies strolling along the sidewalk past the household's lot during the day, and
//! now and then (mornings and evenings most) jogging past in their athletic wear.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::nav::{Floor, PathFollow, Waypoint};
use crate::sim::{Pose, SimAnim};

pub struct TownPlugin;

impl Plugin for TownPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, stroll.run_if(in_state(PlayMode::Live)));
    }
}

/// The sidewalk in front of the home lot: a line along the street.
#[derive(Resource, Clone, Copy)]
pub struct Sidewalk {
    pub center: Vec2,
    pub along: Vec2,
    pub half_length: f32,
}

/// A passer-by, hidden between walks.
#[derive(Component)]
pub struct Townie {
    /// Game minute of the next walk.
    pub next_walk: f64,
    pub walking: bool,
}

fn stroll(
    mut commands: Commands,
    clock: Res<GameClock>,
    sidewalk: Option<Res<Sidewalk>>,
    mut townies: Query<(Entity, &mut Townie, &mut Transform, &mut Visibility, &mut Floor, &mut SimAnim, Option<&PathFollow>, &crate::sim::Sim, Option<&crate::simbody::Wearing>)>,
    weather: Res<crate::weather::Weather>,
) {
    let Some(walk) = sidewalk else { return };
    let mut rng = rand::rng();
    let hour = clock.hour_f();
    let daytime = (7.5..21.0).contains(&hour);
    for (e, mut t, mut tf, mut vis, mut floor, mut anim, path, sim, wearing) in &mut townies {
        if t.walking {
            if path.is_none_or(|p| p.done) {
                t.walking = false;
                t.next_walk = clock.minutes + rng.random_range(40.0..160.0);
                *vis = Visibility::Hidden;
                anim.pose = Pose::Stand;
                commands.entity(e).remove::<(PathFollow, crate::jog::Jogging)>();
            }
            continue;
        }
        if !daytime || clock.minutes < t.next_walk {
            continue;
        }
        // Start at one end of the sidewalk, a little to the side, and walk to the other.
        let dir = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
        let side = Vec2::new(-walk.along.y, walk.along.x) * rng.random_range(-0.6..0.6);
        let start = walk.center + walk.along * walk.half_length * -dir + side;
        let end = walk.center + walk.along * walk.half_length * dir + side;
        tf.translation = Vec3::new(start.x, tf.translation.y, start.y);
        tf.rotation = Quat::from_rotation_y((end - start).x.atan2((end - start).y));
        floor.0 = 1;
        *vis = Visibility::Inherited;
        t.walking = true;
        let mut pf = PathFollow::new(vec![Waypoint { p: end, level: 1, climb: None }]);
        pf.speed = rng.random_range(1.4..1.9);
        // (Some jog past instead, in their athletic wear: more of them mornings and evenings.)
        let jog_hours = (7.0..10.0).contains(&hour) || (17.0..20.0).contains(&hour);
        let jogging = crate::jog::can_jog(sim) && rng.random_bool(if jog_hours { 0.35 } else { 0.1 });
        let athletic = wearing.is_some_and(|w| w.0 == crate::simbody::OutfitKind::Athletic);
        // (Wrapped up against the cold.)
        let coat = weather.temperature < 50.0 && !jogging;
        let wearing_coat = wearing.is_some_and(|w| w.0 == crate::simbody::OutfitKind::Outerwear);
        debug!("{} {} past", sim.full_name(), if jogging { "jogs" } else { "walks" });
        if jogging {
            pf.style = crate::nav::WalkStyle::Jog;
            pf.speed = rng.random_range(2.2..2.7);
            commands.entity(e).insert(crate::jog::Jogging::new(start, clock.minutes));
            if !athletic {
                commands.entity(e).insert((crate::simbody::Wearing(crate::simbody::OutfitKind::Athletic), crate::aging::NeedsNewBody));
            }
        } else if coat && !wearing_coat {
            commands.entity(e).insert((crate::simbody::Wearing(crate::simbody::OutfitKind::Outerwear), crate::aging::NeedsNewBody));
        } else if (athletic || wearing_coat) && !coat {
            commands.entity(e).remove::<crate::simbody::Wearing>().insert(crate::aging::NeedsNewBody);
        }
        commands.entity(e).insert(pf);
    }
}
