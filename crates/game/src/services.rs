//! Town services: the babysitter, who comes whenever the little ones would otherwise be home
//! alone, feeds them, changes them and plays with them, and is paid by the hour.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::careers::AtWork;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, Household, Notifications, Skills};
use crate::loading::CurrentWorld;
use crate::nav::{Floor, PathFollow};
use crate::sim::{Age, BLADDER, DecayScale, FUN, HUNGER, HouseholdMember, Motives, SOCIAL, Sim};
use crate::social::{SOCIALS, social_index};
use crate::{AppState, PlayMode};

pub struct ServicesPlugin;

impl Plugin for ServicesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sitter>().add_systems(Update, babysitting.run_if(in_state(PlayMode::Live)));
    }
}

// ---------------------------------------------------------------------------------------------
// The babysitter

/// What the babysitter is paid an hour.
pub const SITTER_WAGE: i64 = 10;

#[derive(Resource, Default)]
struct Sitter {
    /// When the little ones were first left alone.
    alone_since: Option<f64>,
    sitter: Option<Entity>,
}

#[derive(Component)]
pub struct Babysitter {
    arrived: f64,
    leaving: bool,
}

/// Whether a household member is out (at work or school, at a rabbit hole, out on the town).
type Out = (Has<AtWork>, Has<crate::rabbitholes::AtRabbitHole>, Has<crate::visit::Trip>, Has<crate::visit::OnLot>);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn babysitting(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut state: ResMut<Sitter>,
    mut household: Option<ResMut<Household>>,
    members: Query<(Entity, &Sim, &Motives, &Transform, Out), (With<HouseholdMember>, Without<Babysitter>)>,
    mut sitters: Query<(Entity, &mut Babysitter, &mut ActionQueue, &Transform, Option<&PathFollow>), Without<HouseholdMember>>,
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<CurrentWorld>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let Some(exit) = exit else { return };
    let out = |o: (bool, bool, bool, bool)| o.0 || o.1 || o.2 || o.3;
    let littles: Vec<(Entity, &Sim, &Motives, &Transform)> =
        members.iter().filter(|(_, s, _, _, o)| s.age.is_little() && !out(*o)).map(|(e, s, m, t, _)| (e, s, m, t)).collect();
    let carer_home = members.iter().any(|(_, s, _, _, o)| (s.age == Age::Teen || s.age.is_grown()) && !out(o));
    let needed = !littles.is_empty() && !carer_home;
    let mut rng = rand::rng();

    let Some(se) = state.sitter else {
        if !needed {
            state.alone_since = None;
            return;
        }
        let since = *state.alone_since.get_or_insert(clock.minutes);
        if clock.minutes - since < 15.0 {
            return;
        }
        // She arrives by car at the curb.
        let (female, age) = (rng.random_bool(0.8), if rng.random_bool(0.5) { Age::YoungAdult } else { Age::Adult });
        let base = crate::sim::random_sim(&mut rng, "Babysitter", Some(female), age);
        let first = base.first.clone();
        let p = exit.0;
        let y = world.data.heightmap.sample(p.x, p.y);
        let e = commands
            .spawn((
                Transform::from_xyz(p.x, y, p.y),
                Visibility::default(),
                base,
                Motives::default(),
                DecayScale::default(),
                crate::social::Relationships::default(),
                Skills::default(),
                crate::sim::SimAnim::default(),
                crate::anim::ClipPlayer::default(),
                crate::aging::NeedsNewBody,
                Floor(1),
                ActionQueue::default(),
                Babysitter { arrived: clock.minutes, leaving: false },
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                c.spawn((Transform::default(), Visibility::default()));
            })
            .id();
        state.sitter = Some(e);
        rides.0.push((p, "CarServiceSedan"));
        let names: Vec<&str> = littles.iter().map(|l| l.1.first.as_str()).collect();
        notes.push(format!("{first} the babysitter has come to look after {} (§{SITTER_WAGE} an hour).", names.join(" and ")));
        return;
    };
    let Ok((me, mut b, mut queue, tf, path)) = sitters.get_mut(se) else {
        state.sitter = None;
        return;
    };
    if b.leaving {
        let at_exit = tf.translation.xz().distance(exit.0) < 2.0;
        if at_exit {
            commands.entity(me).despawn();
            state.sitter = None;
            state.alone_since = None;
        } else if queue.0.is_empty() && path.is_none() {
            queue.0.push_back(Action::new("Go Home", ActionKind::GoHere(exit.0, 1), false));
        }
        return;
    }
    if !needed {
        // Someone's home: she's paid and goes.
        b.leaving = true;
        let hours = ((clock.minutes - b.arrived) / 60.0).ceil().max(1.0) as i64;
        let wage = hours * SITTER_WAGE;
        if let Some(h) = household.as_mut() {
            h.funds -= wage;
        }
        queue.0.clear();
        notes.push(format!("The babysitter went home. {hours} hour{} came to §{wage}.", if hours == 1 { "" } else { "s" }));
        return;
    }
    if !queue.0.is_empty() {
        return;
    }
    // The neediest little one: food, a change, a nap or some fun.
    let mut best: Option<(f32, Entity, &str)> = None;
    for (e, _, m, _) in &littles {
        let wants = [("Feed", m.0[HUNGER]), ("Change Diaper", m.0[BLADDER]), ("Play With", m.0[SOCIAL].min(m.0[FUN]))];
        for (what, v) in wants {
            if v < 40.0 && best.is_none_or(|b| v < b.0) {
                best = Some((v, *e, what));
            }
        }
    }
    match best.and_then(|(_, e, what)| social_index(what).map(|si| (e, si))) {
        Some((target, si)) => {
            debug!("the babysitter sees to {}: {}", littles.iter().find(|l| l.0 == target).map_or("?", |l| l.1.first.as_str()), SOCIALS[si].name);
            queue.0.push_back(Action::new(SOCIALS[si].name, ActionKind::Social { target, social: si }, false));
        }
        None => {
            // Keep an eye on them.
            if let Some((_, _, _, ltf)) = littles.choose(&mut rng)
                && ltf.translation.distance(tf.translation) > 4.0
            {
                let a = rng.random_range(0.0..std::f32::consts::TAU);
                let to = ltf.translation.xz() + Vec2::new(a.cos(), a.sin()) * 1.8;
                queue.0.push_back(Action::new("Keep an Eye on the Children", ActionKind::GoHere(to, 1), false));
            }
        }
    }
}
