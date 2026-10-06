//! Household helpers from the base game's catalogue: the grill (the game's grilled recipes:
//! hot dogs, burgers, salmon and tri-tip, served like any group meal), the hot-beverage maker
//! (a coffee for energy), and the alarm clock — set, it wakes the household's workers an hour
//! before their shift and the schoolchildren an hour before the bus.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{ActionQueue, Notifications, Phase};
use crate::sim::{Age, HouseholdMember, Sim};

pub struct AppliancesPlugin;

impl Plugin for AppliancesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Alarm>().add_systems(Update, wake_up.run_if(in_state(PlayMode::Live)));
    }
}

/// The recipes a grill cooks (the base game's grilling menu), by recipe key.
pub const GRILL_RECIPES: [&str; 8] = ["HotDog", "TofuDog", "Hamburger", "Veggieburger", "GrilledSalmon", "VegetarianGrilledSalmon", "TriTipSteak", "TriTipTofuSteak"];

/// Whether the household's alarm clock is set.
#[derive(Resource, Default, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Alarm {
    pub on: bool,
}

/// The hour the bus comes for schoolchildren.
const SCHOOL_BUS: f32 = 7.25;

fn wake_up(
    clock: Res<GameClock>,
    alarm: Res<Alarm>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, Option<&crate::careers::Job>), With<HouseholdMember>>,
    mut woken: Local<HashMap<Entity, u32>>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    if !alarm.on {
        return;
    }
    let (h, day) = (clock.hour_f(), clock.day());
    let mut rang = false;
    for (e, sim, mut queue, job) in &mut sims {
        // When they need to be up: an hour before work or the bus.
        let due = match (job, sim.age) {
            (Some(j), _) if j.works_on(clock.weekday()) => Some(j.info().start - 1.0),
            (_, Age::Child | Age::Teen) if clock.weekday() < 5 => Some(SCHOOL_BUS - 1.0),
            _ => None,
        };
        if !due.is_some_and(|d| (d..d + 0.5).contains(&h)) || woken.get(&e) == Some(&day) {
            continue;
        }
        let Some(front) = queue.0.front_mut() else { continue };
        if !(matches!(front.phase, Phase::Running(_)) && (front.label.contains("Sleep") || front.label.contains("Nap"))) {
            continue;
        }
        woken.insert(e, day);
        front.cancel = true;
        rang = true;
        notes.push(format!("{} woke up to the alarm.", sim.first));
    }
    if rang {
        // (The alarm clock's ring, unnamed in the sound tables.)
        play.write(crate::sound::PlaySound::ui("4DE8AFC3B236AF6D"));
    }
}
