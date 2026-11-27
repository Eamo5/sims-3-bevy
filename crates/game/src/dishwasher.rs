//! Dishwashers, as the game's `Dishwasher`: once the dishes are loaded ("Load Dishes") it runs
//! (`WashTime`: thirty minutes the cheap one, twenty the dear one), heard humming as it washes
//! (`dwasher_running_lp_chp`) and chiming when it's done (`dwasher_done_chp`); loaded again while
//! it runs, it starts over. Running, its noise (`kNormalDishwasherBroadcastParams`: three metres,
//! its room only) wakes a Sim asleep there (Rude Awakening, not a Heavy Sleeper: `disturb`) and
//! has those awake there pull a face at it the first time (`BadNoise`). Made silent ("Upgrade:
//! Make Silent", Handiness 3) it disturbs nobody and is heard with the quiet one's sounds
//! (`dwasher_running_lp_exp`, `dwasher_done_exp`); nor does it run noisily for a Heavy Sleeper
//! who loads it (`CreateDiswasherNoiseBroadcaster`). Broken as it's loaded, it doesn't run.

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::GameObject;

pub struct DishwasherPlugin;

impl Plugin for DishwasherPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (start, run, dish_test).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// Dishes just loaded, by whom.
#[derive(Component)]
pub struct Loaded(pub Entity);

/// Washing till then (game minutes); `Noisy` while it's heard about the room.
#[derive(Component)]
pub struct Washing {
    pub until: f64,
}

#[derive(Component)]
pub struct Noisy;

/// The noise's reach (`kNormalDishwasherBroadcastParams`' `PulseRadius`), in its room.
pub const NOISE_RADIUS: f32 = 3.0;

/// How long a load takes (the dishwashers' tuned `WashTime`).
pub fn wash_minutes(script: &str) -> f64 {
    if script.to_ascii_lowercase().contains("expensive") { 20.0 } else { 30.0 }
}

/// Its sounds, running and done (`WashSound`, `DoneSound`): the quiet one's once it's silent.
pub fn sounds(silent: bool) -> (&'static str, &'static str) {
    if silent { ("dwasher_running_lp_exp", "dwasher_done_exp") } else { ("dwasher_running_lp_chp", "dwasher_done_chp") }
}

/// Whether it's been made silent ("Make Silent": the dishwasher's improvement).
pub fn silent(upgrades: Option<&crate::upgrades::Upgrades>) -> bool {
    crate::upgrades::has(upgrades, crate::upgrades::Upgrade::Improved)
}

/// Loaded: running (again, from the start), unless it broke.
#[allow(clippy::type_complexity)]
fn start(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    data: Option<Res<crate::baked::Baked>>,
    loaded: Query<(Entity, &GameObject, &Loaded, Has<crate::interact::Broken>, Option<&crate::upgrades::Upgrades>)>,
    sims: Query<&crate::sim::Sim>,
) {
    for (e, o, by, broken, upgrades) in &loaded {
        commands.entity(e).remove::<Loaded>();
        if broken {
            commands.entity(e).remove::<(Washing, Noisy, crate::sound::AmbientSound)>();
            continue;
        }
        let script = data.as_ref().and_then(|d| d.0.catalog_entry(&o.objd)).map_or("", |c| c.script.as_str());
        let quiet = silent(upgrades);
        let heavy = sims.get(by.0).is_ok_and(|s| s.traits.contains(&crate::life::Trait::HeavySleeper));
        commands.entity(e).insert((Washing { until: clock.minutes + wash_minutes(script) }, crate::sound::AmbientSound(sounds(quiet).0.to_string())));
        if quiet || heavy {
            commands.entity(e).remove::<Noisy>();
        } else {
            commands.entity(e).insert(Noisy);
        }
        info!("dishwasher: the {} is running", o.name);
    }
}

/// Done washing: quiet, and its chime.
fn run(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    washing: Query<(Entity, &GameObject, &Washing, &GlobalTransform, Has<crate::interact::Broken>, Option<&crate::upgrades::Upgrades>)>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    for (e, o, w, tf, broken, upgrades) in &washing {
        if broken {
            commands.entity(e).remove::<(Washing, Noisy, crate::sound::AmbientSound)>();
        } else if clock.minutes >= w.until {
            commands.entity(e).remove::<(Washing, Noisy, crate::sound::AmbientSound)>();
            play.write(crate::sound::PlaySound { name: sounds(silent(upgrades)).1.into(), at: Some(tf.translation()), volume: 1.0, speed: 1.0 });
            info!("dishwasher: the {} is done", o.name);
        }
    }
}

/// `DISH_TEST=load|silent` (tests): ten seconds in, the selected Sim loads the lot's dishwasher
/// (made silent first), the view on it.
fn dish_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<&mut crate::interact::ActionQueue, With<crate::sim::Selected>>,
    dishwashers: Query<(Entity, &GameObject, &GlobalTransform)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut done: Local<bool>,
) {
    let Ok(what) = std::env::var("DISH_TEST") else { return };
    let t = time.elapsed_secs();
    let Some((d, _, at)) = dishwashers.iter().find(|(_, o, _)| o.kind == crate::interact::ObjectKind::Dishwasher) else { return };
    if t > 7.0
        && let Ok(mut cam) = cams.single_mut()
    {
        let front = at.rotation() * Vec3::Z;
        cam.look_at(at.translation() + Vec3::Y * 0.5);
        cam.distance = 7.0;
        cam.yaw = front.x.atan2(front.z) + 0.6;
    }
    if *done || t < 10.0 {
        return;
    }
    *done = true;
    let Ok(mut q) = sims.single_mut() else { return };
    if what == "silent" {
        commands.entity(d).insert(crate::upgrades::Upgrades(crate::upgrades::Upgrade::Improved.bit()));
    }
    let Some(def) = crate::interact::interactions_for(crate::interact::ObjectKind::Dishwasher).iter().position(|x| x.special == crate::interact::Special::WashDishes) else { return };
    q.push_player(crate::interact::Action::new("Load Dishes", crate::interact::ActionKind::Object { target: d, def }, false));
    info!("dish test: loading");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dishwashers_wash_as_tuned_and_hush_once_silent() {
        assert_eq!(wash_minutes("Sims3.Gameplay.Objects.Appliances.DishwasherCheap"), 30.0);
        assert_eq!(wash_minutes("Sims3.Gameplay.Objects.Appliances.DishwasherExpensive"), 20.0);
        let up = crate::upgrades::Upgrades(crate::upgrades::Upgrade::Improved.bit());
        assert!(silent(Some(&up)) && !silent(None));
        assert_eq!(sounds(true), ("dwasher_running_lp_exp", "dwasher_done_exp"));
        assert_eq!(sounds(false).0, "dwasher_running_lp_chp");
    }
}
