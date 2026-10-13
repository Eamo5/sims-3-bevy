//! A change of clothes as the game shows one: the Sim spins round (`a_clothesChange`, a child's
//! `c_clothesChange`) in a swirl of sparkles (its `clothesChangeAdult` effect) with a whoosh
//! (`change_clothes_spin`, the clip's own sound), and is in the new outfit by the end of the
//! turn (the clip's `change_clothes_here`, the game's script event 747).

use bevy::prelude::*;

use crate::PlayMode;
use crate::anim::{ActionClip, ClipPlayer};
use crate::clock::{GameClock, SPEED_RATES};
use crate::sim::{Age, Pose, Sim, SimAnim};
use crate::simbody::{ChangedInto, OutfitKind, Wearing};

pub struct DressingPlugin;

impl Plugin for DressingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spin.run_if(in_state(PlayMode::Live)));
    }
}

/// A spin into other clothes, under way (a walk waits for it).
#[derive(Component)]
pub struct ClothesSpin {
    /// What they're spinning into (none: their everyday clothes), and whether it was chosen
    /// (kept on until it's time for something else).
    to: Option<OutfitKind>,
    chosen: bool,
    t: f32,
    sparkled: bool,
    swapped: bool,
}

impl ClothesSpin {
    /// How far into the spin (seconds).
    pub fn at(&self) -> f32 {
        self.t
    }
}

/// The clip's moments (seconds): the sparkles (its effect event), and the clothes changed (its
/// `change_clothes_here`).
const SPARKLE_AT: f32 = 0.33;
const SWAP_AT: f32 = 0.43;
/// The longest a spin's held for, whatever its clip.
const MAX_SECS: f32 = 4.0;

/// Whether a Sim changes with a spin (babies and toddlers are changed as they are).
pub fn spins(sim: &Sim) -> bool {
    !sim.age.is_little()
}

/// Starts a spin into `to` (none: everyday clothes).
pub fn start(commands: &mut Commands, sim: Entity, to: Option<OutfitKind>, chosen: bool) {
    commands.entity(sim).insert((ClothesSpin { to, chosen, t: 0.0, sparkled: false, swapped: false }, ActionClip::new(None, &["a_clothesChange"])));
}

/// Puts an outfit on there and then (none: everyday clothes).
pub fn put_on(e: &mut EntityCommands, to: Option<OutfitKind>, chosen: bool) {
    match to {
        Some(k) if chosen => {
            e.insert((Wearing(k), ChangedInto));
        }
        Some(k) => {
            e.remove::<ChangedInto>().insert(Wearing(k));
        }
        None => {
            e.remove::<(Wearing, ChangedInto)>();
        }
    }
    e.insert(crate::aging::NeedsNewBody);
}

/// Turns the spin: sparkles, the clothes changed mid-turn, and done when its clip is.
fn spin(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    mut sims: Query<(Entity, &Sim, &mut ClothesSpin, &mut SimAnim, Option<&ClipPlayer>)>,
    mut sparkles: MessageWriter<crate::effects::SpinSparkle>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed];
    for (e, sim, mut s, mut anim, player) in &mut sims {
        // (Stood still for it.)
        if anim.pose == Pose::Walk {
            anim.pose = Pose::Stand;
        }
        if s.t == 0.0 {
            info!("{} spins into {:?}", sim.first, s.to);
        }
        s.t += dt;
        if !s.sparkled && s.t >= SPARKLE_AT {
            s.sparkled = true;
            sparkles.write(crate::effects::SpinSparkle { sim: e, height: if sim.age == Age::Child { 1.15 } else { 1.75 } });
        }
        if !s.swapped && s.t >= SWAP_AT {
            s.swapped = true;
            put_on(&mut commands.entity(e), s.to, s.chosen);
        }
        let len = player.filter(|p| p.name.contains("clothesChange")).and_then(|p| p.clip.as_ref()).map_or(1.5, |c| c.duration);
        if s.t >= len.max(SWAP_AT + 0.05).min(MAX_SECS) {
            commands.entity(e).remove::<(ClothesSpin, ActionClip)>();
        }
    }
}
