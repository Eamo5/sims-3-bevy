//! Door locks, as the game's `CommonDoor.LockDoor`: a door of the household's own home can be
//! locked ("Lock..."), for "Everybody", "Everyone but Myself" or "Everyone but My Household", or
//! for all but the Sims picked ("Specify Sims for this Door"), and unlocked again. A Sim it's
//! locked against can't get through it: their ways are planned with its doorway shut (see
//! `nav::with_locked`). Locks are kept in saves, with the door.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::sim::Sim;

pub struct DoorLocksPlugin;

impl Plugin for DoorLocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, lock_test.run_if(in_state(crate::PlayMode::Live)));
    }
}

/// DOOR_LOCK=<lock> (tests: "Everybody", "Everyone but Myself", "Everyone but My Household"):
/// every door of the home locked so, for the selected Sim.
fn lock_test(
    mut commands: Commands,
    mut done: Local<bool>,
    time: Res<Time>,
    doors: Query<(Entity, &crate::interact::GameObject), Without<crate::visit::LotObject>>,
    sel: Query<&Sim, With<crate::sim::Selected>>,
) {
    let Ok(want) = std::env::var("DOOR_LOCK") else { return };
    if *done || time.elapsed_secs() < 8.0 {
        return;
    }
    let Ok(me) = sel.single() else { return };
    let Some(Some(lock)) = DoorLock::chosen(&want, me, None, None) else { return };
    let mut n = 0;
    for (e, o) in &doors {
        if o.kind == crate::interact::ObjectKind::Door {
            commands.entity(e).insert(lock.clone());
            n += 1;
        }
    }
    info!("door lock test: {n} doors locked: {want}");
    *done = true;
}

/// How the game names them (`Gameplay/Abstracts/Doors/Locking`).
pub const LOCK: &str = "Lock";
pub const UNLOCK: &str = "Unlock";
pub const EVERYBODY: &str = "Everybody";
pub const BUT_ME: &str = "Everyone but Myself";
pub const BUT_HOUSEHOLD: &str = "Everyone but My Household";
pub const SPECIFY: &str = "Specify Sims for this Door";

/// Who a door's locked against.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Lock {
    /// Everybody (the game's `Nobody` may open it).
    Everybody,
    /// All but those named (`SelectedActor`, `OwnerList`).
    AllBut,
    /// All but the household (`SelectedHousehold`).
    Household,
}

/// A locked door: against whom, and (locked against all but some) who may open it, by their ids.
#[derive(Component, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DoorLock {
    pub lock: Lock,
    #[serde(default)]
    pub who: Vec<u64>,
}

impl DoorLock {
    /// Whether this Sim may go through.
    pub fn lets(&self, sim: &Sim, household: bool) -> bool {
        match self.lock {
            Lock::Everybody => false,
            Lock::AllBut => self.who.contains(&sim.id),
            Lock::Household => household,
        }
    }

    /// The lock chosen from the menu (`label`), for the Sim choosing it; or none, to unlock. A
    /// Sim picked from "Specify Sims..." is let through, or no longer, as they were before.
    pub fn chosen(label: &str, actor: &Sim, was: Option<&DoorLock>, picked: Option<&Sim>) -> Option<Option<DoorLock>> {
        Some(match label {
            UNLOCK => None,
            EVERYBODY => Some(DoorLock { lock: Lock::Everybody, who: Vec::new() }),
            BUT_ME => Some(DoorLock { lock: Lock::AllBut, who: vec![actor.id] }),
            BUT_HOUSEHOLD => Some(DoorLock { lock: Lock::Household, who: Vec::new() }),
            _ => {
                let p = picked?;
                let mut who = was.filter(|w| w.lock == Lock::AllBut).map(|w| w.who.clone()).unwrap_or_default();
                match who.iter().position(|w| *w == p.id) {
                    Some(i) => {
                        who.remove(i);
                    }
                    None => who.push(p.id),
                }
                Some(DoorLock { lock: Lock::AllBut, who })
            }
        })
    }

    /// Which of the menu's locks it is now (not offered again).
    pub fn is(&self, label: &str, actor: &Sim) -> bool {
        match label {
            EVERYBODY => self.lock == Lock::Everybody,
            BUT_ME => self.lock == Lock::AllBut && self.who == [actor.id],
            BUT_HOUSEHOLD => self.lock == Lock::Household,
            _ => false,
        }
    }
}

/// A Sim picked to be let through ("Specify Sims..."), as the menu shows them: ticked if they are.
pub fn picker_label(sim: &Sim, lock: Option<&DoorLock>) -> String {
    let lets = lock.is_some_and(|l| l.lock == Lock::AllBut && l.who.contains(&sim.id));
    format!("{}{}", sim.full_name(), if lets { " ✓" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_let_through_only_whom_they_should() {
        let mut rng = rand::rng();
        let me = crate::sim::random_sim(&mut rng, "Test", Some(true), crate::sim::Age::Adult);
        let you = crate::sim::random_sim(&mut rng, "Test", Some(false), crate::sim::Age::Adult);
        let all = DoorLock::chosen(EVERYBODY, &me, None, None).unwrap().unwrap();
        assert!(!all.lets(&me, true));
        let mine = DoorLock::chosen(BUT_ME, &me, None, None).unwrap().unwrap();
        assert!(mine.lets(&me, true) && !mine.lets(&you, true) && mine.is(BUT_ME, &me));
        let family = DoorLock::chosen(BUT_HOUSEHOLD, &me, None, None).unwrap().unwrap();
        assert!(family.lets(&you, true) && !family.lets(&you, false));
        // (Picking another Sim lets them through as well; picking them again, no longer.)
        let both = DoorLock::chosen(&picker_label(&you, Some(&mine)), &me, Some(&mine), Some(&you)).unwrap().unwrap();
        assert!(both.lets(&you, false) && both.lets(&me, false));
        assert!(picker_label(&you, Some(&both)).ends_with('✓'));
        let back = DoorLock::chosen("", &me, Some(&both), Some(&you)).unwrap().unwrap();
        assert!(!back.lets(&you, false));
        assert_eq!(DoorLock::chosen(UNLOCK, &me, Some(&both), None), Some(None));
    }
}
