//! Callers at the door: a Sim come to visit walks up to the house's front door and rings the
//! bell (the game's ring, and its chime), then stands back from the door to wait to be let in
//! (`Lot.MoveAwayFromDoor`). The player's household invites them inside ("Invite Inside": never
//! of their own accord, as the game's); a family visited lets them in itself. Left waiting a
//! couple of hours, they give up and go home.

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue, GoingHome, Notifications};
use crate::sim::Sim;

pub struct DoorbellPlugin;

impl Plugin for DoorbellPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, door_waits.run_if(in_state(PlayMode::Live)));
    }
}

/// A caller at the door: on their way to ring (no `since` yet), then waiting since then.
#[derive(Component)]
pub struct AtTheDoor {
    pub since: Option<f64>,
}

/// How long a caller waits to be let in (game minutes).
const WAIT_MINUTES: f64 = 120.0;

/// The ring at the door.
pub const RING: &[&str] = &["a2o_door_ringBell_x"];

/// How far back from the front door callers wait (metres: the game's 2 to 4).
pub const STAND_BACK: f32 = 2.5;

/// A caller come to the house: up to its front door (standing outside it, `door`), to ring, and
/// back from it a little (`back`) to wait.
pub fn come_to_door(commands: &mut Commands, e: Entity, queue: &mut ActionQueue, door: Vec2, back: Option<Vec2>, target: Entity) {
    info!("a caller comes to the front door at {:.1},{:.1}", door.x, door.y);
    queue.0.clear();
    queue.0.push_back(Action::new("Go to the Door", ActionKind::GoHere(door, 1), true));
    queue.0.push_back(Action::new("Ring the Doorbell", ActionKind::Outro { clips: RING, then: None, secs: 3.0, stand_at: None, target }, true));
    if let Some(back) = back {
        queue.0.push_back(Action::new("Wait Outside", ActionKind::GoHere(back, 1), true));
    }
    commands.entity(e).insert(AtTheDoor { since: None });
}

/// Callers ring and wait; left too long, they go home.
fn door_waits(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut callers: Query<(Entity, &Sim, &mut AtTheDoor, &mut ActionQueue, Has<crate::sim::HouseholdMember>)>,
    mut notes: ResMut<Notifications>,
) {
    for (e, sim, mut door, mut queue, ours) in &mut callers {
        match door.since {
            // (Rung: now waiting.)
            None if queue.0.is_empty() => {
                door.since = Some(clock.minutes);
                if !ours {
                    notes.push(format!("{} is at the door.", sim.full_name()));
                }
            }
            // (The household's own, calling on someone: nobody answered, and they're left outside.)
            Some(since) if ours && clock.minutes - since > WAIT_MINUTES => {
                notes.push(format!("Nobody answered the door for {}.", sim.first));
                commands.entity(e).remove::<AtTheDoor>();
            }
            Some(since) if clock.minutes - since > WAIT_MINUTES => {
                notes.push(format!("{} got tired of waiting at the door and went home.", sim.first));
                queue.0.clear();
                commands.entity(e).remove::<AtTheDoor>().insert(GoingHome);
            }
            _ => {}
        }
    }
}
