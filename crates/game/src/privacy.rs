//! Privacy, as the game's: while a Sim's using the toilet, the shower or the bath, the room is
//! theirs. Anyone else setting out to use something in it waits where they are (thinking of the
//! bathroom) until it's free, or gives up after half an hour.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase};

pub struct PrivacyPlugin;

impl Plugin for PrivacyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Privacy>().add_systems(Update, (private_rooms, waiting).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The objects in rooms someone's using privately, and who.
#[derive(Resource, Default)]
pub struct Privacy {
    pub objects: HashMap<Entity, Entity>,
}

/// Waiting for a bathroom to be free: since when (game minutes), and when the balloon's next
/// shown.
#[derive(Component)]
pub struct WaitingForPrivacy {
    since: f64,
    balloon: f64,
}

impl WaitingForPrivacy {
    pub fn new(now: f64) -> Self {
        Self { since: now, balloon: now }
    }
}

/// How long a Sim waits for a bathroom before giving up (game minutes), and how often they think
/// of it meanwhile.
const WAIT_MINUTES: f64 = 30.0;
const BALLOON_EVERY: f64 = 6.0;

/// Whether using a kind of object makes its room private.
fn private(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Toilet | ObjectKind::Shower | ObjectKind::Bathtub | ObjectKind::ShowerTub | ObjectKind::Urinal)
}

/// The rooms in private use just now, and the objects in them.
fn private_rooms(
    mut privacy: ResMut<Privacy>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    sims: Query<(Entity, &ActionQueue), Without<crate::visit::OnLot>>,
    objects: Query<(Entity, &GameObject, &Transform), Without<crate::visit::LotObject>>,
) {
    privacy.objects.clear();
    let Some(b) = building else { return };
    let room = |p: Vec3| {
        let level = b.level_at(p.y + 0.3);
        b.room_id(level, p).map(|r| (level, r))
    };
    let mut rooms: Vec<((u8, u16), Entity)> = Vec::new();
    for (e, q) in &sims {
        let Some(a) = q.0.front() else { continue };
        let (ActionKind::Object { target, .. }, Phase::Running(_)) = (&a.kind, a.phase) else { continue };
        let Ok((_, o, tf)) = objects.get(*target) else { continue };
        if private(o.kind)
            && let Some(r) = room(tf.translation)
        {
            rooms.push((r, e));
        }
    }
    if rooms.is_empty() {
        return;
    }
    for (oe, _, tf) in &objects {
        if let Some(r) = room(tf.translation)
            && let Some((_, who)) = rooms.iter().find(|(k, _)| *k == r)
        {
            privacy.objects.insert(oe, *who);
        }
    }
}

/// Those waiting for a bathroom think of it now and then, and give up in time; once their way's
/// clear (or they've something else to do) they're no longer waiting.
fn waiting(
    mut commands: Commands,
    clock: Res<GameClock>,
    privacy: Res<Privacy>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut sims: Query<(Entity, &crate::sim::Sim, &mut WaitingForPrivacy, &mut ActionQueue)>,
) {
    for (e, sim, mut w, mut queue) in &mut sims {
        let blocked = queue.0.front().is_some_and(|a| a.phase == Phase::Start && matches!(a.kind, ActionKind::Object { target, .. } if privacy.objects.get(&target).is_some_and(|u| *u != e)));
        if !blocked {
            commands.entity(e).remove::<WaitingForPrivacy>();
            continue;
        }
        if clock.minutes - w.since > WAIT_MINUTES {
            debug!("{} gave up waiting for the bathroom", sim.first);
            if let Some(a) = queue.0.front_mut() {
                a.cancel = true;
            }
            commands.entity(e).remove::<WaitingForPrivacy>();
            continue;
        }
        if clock.minutes >= w.balloon {
            w.balloon = clock.minutes + BALLOON_EVERY;
            if let Some(b) = ui.as_ref().and_then(|u| u.data.balloons.list("MotiveBladder")).and_then(|l| l.first()) {
                commands.entity(e).insert(crate::balloons::BalloonRequest { kind: crate::balloons::BalloonKind::Thought, icon: b.icon.clone(), axis: b.axis });
            }
        }
    }
}
