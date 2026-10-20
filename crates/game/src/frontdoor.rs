//! The front door and its bell, as the game's `Door.SetFrontDoor`, `UnsetFrontDoor` and
//! `ChangeDoorbellSound`. Of a home's doors to the outdoors the front door is the one nearest
//! the street, where callers ring and wait, unless the household picks another ("Set as Front
//! Door"; "Unset Front Door" puts it back). A handy Sim (Handiness 3) can change the front
//! door's bell, an upgrade of 160 minutes with the door's own tinkering: Default, Jingle,
//! Haunted, Posh or Jokester, rung once it's done. Both are kept in saves.

use bevy::prelude::*;

use crate::AppState;

pub struct FrontDoorPlugin;

impl Plugin for FrontDoorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Doorbell>()
            .init_resource::<Front>()
            .add_systems(OnEnter(AppState::InGame), |mut commands: Commands| commands.insert_resource(Doorbell::default()))
            .add_systems(Update, (find_front, front_test).chain().run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The door the household set as its front door.
#[derive(Component)]
pub struct FrontDoor;

/// The household's doorbell (of [`BELLS`]).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Doorbell(pub u8);

/// The home's front door now (set, or the nearest the street).
#[derive(Resource, Default)]
pub struct Front(pub Option<Entity>);

/// The words (the game's `Gameplay/Abstracts/Door`).
pub const SET: &str = "Set as Front Door";
pub const UNSET: &str = "Unset Front Door";
pub const CHANGE: &str = "Change Doorbell...";

/// The bells, by name, and their sounds (the game's `kDoorbellSoundEffects`).
pub const BELLS: [(&str, &str); 5] = [("Default", "doorbell_ring"), ("Jingle", "doorbell_ringa"), ("Haunted", "doorbell_ringb"), ("Posh", "doorbell_ringc"), ("Jokester", "doorbell_ringd")];

/// The Handiness it takes, and how long it takes (game minutes: `kUpgradeDoorbellSoundTuning`).
pub const LEVEL: u32 = 3;
pub const MINUTES: f32 = 160.0;

/// A doorbell change, as an upgrade's bit: the bell's number with the top bit set.
const BELL_BIT: u8 = 0x80;

pub fn bell_bit(bell: u8) -> u8 {
    BELL_BIT | bell
}

/// The bell an upgrade's bit is for, if it's a doorbell change.
pub fn bell_of(bit: u8) -> Option<u8> {
    (bit & BELL_BIT != 0).then_some(bit & !BELL_BIT).filter(|b| (*b as usize) < BELLS.len())
}

impl Doorbell {
    /// The sound it rings with.
    pub fn sound(self) -> &'static str {
        BELLS.get(self.0 as usize).map_or(BELLS[0].1, |b| b.1)
    }
}

/// Keeps track of the home's front door: the one set, or the one nearest the street.
fn find_front(
    mut front: ResMut<Front>,
    building: Option<ResMut<crate::building::ActiveBuilding>>,
    set: Query<Entity, (With<FrontDoor>, Without<crate::visit::LotObject>)>,
    exit: Option<Res<crate::interact::LotExit>>,
    mut next: Local<f32>,
    time: Res<Time>,
) {
    let Some(mut building) = building else {
        front.0 = None;
        return;
    };
    let chosen = set.iter().next();
    if building.front != chosen {
        building.front = chosen;
        *next = 0.0;
    }
    // (Worked out now and then: the doors don't move often.)
    if time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 2.0;
    let door = exit.and_then(|x| building.front_door(x.0)).and_then(|(_, e)| e);
    if front.0 != door {
        front.0 = door;
    }
}

/// The household's door set as the front door, or (`set` false) put back to the nearest.
pub fn set_front(w: &mut World, door: Entity, set: bool) {
    let mut q = w.query_filtered::<Entity, With<FrontDoor>>();
    let had: Vec<Entity> = q.iter(w).collect();
    for e in had {
        w.entity_mut(e).remove::<FrontDoor>();
    }
    if set && let Ok(mut e) = w.get_entity_mut(door) {
        e.insert(FrontDoor);
    }
    let at = w.get::<Transform>(door).map(|t| t.translation);
    info!("front door {}: {door} at {at:.1?}", if set { "set" } else { "unset" });
}

/// Whether a door can be set as the front door: one of the home's to the outdoors, on the
/// ground floor, not already it.
pub fn settable(building: Option<&crate::building::ActiveBuilding>, door: Entity, is_front: bool) -> bool {
    !is_front && building.is_some_and(|b| b.outside_door(door))
}

/// A doorbell changed: the household's now, and rung.
pub fn bell_changed(w: &mut World, door: Entity, bell: u8) {
    w.insert_resource(Doorbell(bell));
    let at = w.get::<Transform>(door).map(|t| t.translation);
    let sound = Doorbell(bell).sound().to_string();
    w.write_message(crate::sound::PlaySound { name: sound, at, volume: 1.0, speed: 1.0 });
    info!("doorbell changed to {}", BELLS[bell as usize].0);
}

/// FRONT_DOOR=set|bell:<n> (tests): the home's door farthest from the street set as the front
/// door; or, the selected Sim's Handiness raised to the bell's level, the bell changed.
fn front_test(
    mut commands: Commands,
    mut done: Local<bool>,
    time: Res<Time>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    exit: Option<Res<crate::interact::LotExit>>,
    doors: Query<(Entity, &crate::interact::GameObject, &Transform), Without<crate::visit::LotObject>>,
    front: Res<Front>,
    mut sel: Query<(Entity, &mut crate::interact::Skills, &mut crate::interact::ActionQueue), With<crate::sim::Selected>>,
) {
    let Ok(want) = std::env::var("FRONT_DOOR") else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    let (Some(b), Some(exit)) = (building, exit) else { return };
    if want == "set" {
        let far = doors
            .iter()
            .filter(|(e, o, _)| o.kind == crate::interact::ObjectKind::Door && b.outside_door(*e) && front.0 != Some(*e))
            .max_by(|a, c| a.2.translation.xz().distance(exit.0).total_cmp(&c.2.translation.xz().distance(exit.0)));
        match far {
            Some((e, ..)) => {
                commands.queue(move |w: &mut World| set_front(w, e, true));
            }
            None => info!("front door test: no other door to the outdoors"),
        }
        *done = true;
    } else if let Some(n) = want.strip_prefix("bell:").and_then(|n| n.parse::<u8>().ok()) {
        let (Some(door), Ok((_, mut skills, mut queue))) = (front.0, sel.single_mut()) else { return };
        for _ in 0..100 {
            if skills.level("Handiness") >= LEVEL {
                break;
            }
            skills.add_points("Handiness", 500.0);
        }
        queue.push_player(crate::interact::Action::new(format!("{CHANGE} {}", BELLS[n as usize % BELLS.len()].0), crate::interact::ActionKind::Upgrade { target: door, bit: bell_bit(n) }, false));
        info!("front door test: changing the bell to {}", BELLS[n as usize % BELLS.len()].0);
        *done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bells_as_upgrade_bits() {
        assert_eq!(bell_of(bell_bit(3)), Some(3));
        assert_eq!(bell_of(bell_bit(0)), Some(0));
        // (The handiness upgrades' own bits aren't bells.)
        for u in crate::upgrades::Upgrade::ALL {
            assert_eq!(bell_of(u.bit()), None);
        }
        assert_eq!(bell_of(bell_bit(9)), None);
        assert_eq!(Doorbell(2).sound(), "doorbell_ringb");
        assert_eq!(Doorbell::default().sound(), "doorbell_ring");
    }
}
