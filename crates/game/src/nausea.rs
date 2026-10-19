//! Throwing up, as the game's `BuffNauseous`: when a Sim's nausea runs its course (a pregnant
//! Sim's in the mornings), they hurry to the nearest toilet and are sick in it (`Toilet.Vomit`,
//! the toilet flushed and dirtied after), or, with no toilet to get to, throw up where they are
//! (`ThrowUpOutside`). Either way the nausea's over.

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Special, interactions_for};
use crate::life::{MoodletKind, Moodlets};
use crate::sim::Sim;

pub struct NauseaPlugin;

impl Plugin for NauseaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (be_sick, threw_up, nausea_test).before(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
    }
}

/// The words for it (the game's `ThrowUpOutside:InteractionName`).
pub const THROW_UP: &str = "Throw Up";

/// Thrown up in this toilet.
#[derive(Component)]
pub struct ThrewUp(pub Entity);

/// On their way to be sick (once for each bout).
#[derive(Component)]
struct GoingToBeSick;

/// How far a toilet can be for a Sim to make it there (about the lot).
const TOILET_REACH: f32 = 30.0;

/// Their nausea's run its course: to the nearest toilet that works, or sick where they stand.
#[allow(clippy::type_complexity)]
fn be_sick(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<
        (Entity, &Transform, &Moodlets, &mut ActionQueue, Has<GoingToBeSick>),
        (Without<crate::careers::AtWork>, Without<crate::rabbitholes::AtRabbitHole>),
    >,
    toilets: Query<(Entity, &GameObject, &Transform), (Without<Sim>, Without<crate::interact::Broken>)>,
) {
    for (me, tf, moodlets, mut queue, going) in &mut sims {
        let Some(m) = moodlets.0.iter().find(|m| m.kind == MoodletKind::Nauseous) else {
            if going {
                commands.entity(me).remove::<GoingToBeSick>();
            }
            continue;
        };
        // (When it's about to run out: the game's `OnTimeout`.)
        if going || m.until - clock.minutes > 1.0 {
            continue;
        }
        commands.entity(me).insert(GoingToBeSick);
        let toilet = toilets
            .iter()
            .filter(|t| t.1.kind == ObjectKind::Toilet && t.2.translation.distance(tf.translation) < TOILET_REACH)
            .min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
        let def = interactions_for(ObjectKind::Toilet).iter().position(|d| d.special == Special::Vomit);
        let action = match (toilet, def) {
            (Some((t, ..)), Some(def)) => Action::new(THROW_UP, ActionKind::Object { target: t, def }, false),
            // (No toilet: sick on the spot.)
            _ => Action::new(THROW_UP, ActionKind::Outro { clips: &["a_buff_throwUp_x"], then: None, secs: 4.0, stand_at: None, target: me }, false),
        };
        debug!("nausea: {me} goes to be sick ({})", if toilet.is_some() { "the toilet" } else { "where they are" });
        queue.0.push_front(action);
    }
}

/// Sick in the toilet: the nausea over, the toilet flushed and a little dirtier.
fn threw_up(
    mut commands: Commands,
    mut done: Query<(Entity, &ThrewUp, &Sim, &mut Moodlets)>,
    mut grime: Query<&mut crate::grime::Grime>,
) {
    for (e, t, sim, mut moodlets) in &mut done {
        commands.entity(e).remove::<ThrewUp>().insert(crate::toilets::Flushed(t.0));
        moodlets.remove(MoodletKind::Nauseous);
        if let Ok(mut g) = grime.get_mut(t.0) {
            g.used(&sim.traits);
        }
    }
}

/// NAUSEA_TEST=1: the selected Sim's queasy, their nausea about to run out.
fn nausea_test(mut done: Local<bool>, time: Res<Time>, clock: Res<crate::clock::GameClock>, mut sims: Query<&mut Moodlets, With<crate::sim::Selected>>) {
    // (A few seconds in: once a save's moodlets are back.)
    if *done || time.elapsed_secs() < 8.0 || std::env::var("NAUSEA_TEST").is_err() {
        return;
    }
    *done = true;
    if let Ok(mut m) = sims.single_mut() {
        m.add(MoodletKind::Nauseous, clock.minutes);
        if let Some(n) = m.0.iter_mut().find(|n| n.kind == MoodletKind::Nauseous) {
            n.until = clock.minutes + 3.0;
        }
        info!("nausea test: queasy for three minutes");
    }
}
