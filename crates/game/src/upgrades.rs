//! Handiness upgrades: a handy Sim can make an appliance unbreakable, or better at what it
//! does (a water heater for the shower and the bath, improved channels for the TV, better
//! speakers for the stereo, a faster processor for the computer). Upgrading takes a while,
//! with the game's repair animations, and builds Handiness; electronics can shock the unskilled.
//! Upgrades are kept in saves, found again by the object and where it stands.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{GameObject, ObjectKind};

pub struct UpgradesPlugin;

impl Plugin for UpgradesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingUpgrades>().add_systems(Update, restore_upgrades.run_if(in_state(PlayMode::Live)));
    }
}

/// The upgrades an object has had (bits of [`Upgrade`]).
#[derive(Component, Clone, Copy, Default, Debug)]
pub struct Upgrades(pub u8);

impl Upgrades {
    pub fn has(&self, u: Upgrade) -> bool {
        self.0 & u.bit() != 0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Upgrade {
    /// It never breaks again.
    Unbreakable,
    /// It does what it does better (by half again).
    Improved,
}

impl Upgrade {
    pub const ALL: [Upgrade; 2] = [Upgrade::Unbreakable, Upgrade::Improved];

    pub fn bit(self) -> u8 {
        match self {
            Upgrade::Unbreakable => 1,
            Upgrade::Improved => 2,
        }
    }

    pub fn from_bit(bit: u8) -> Option<Upgrade> {
        Self::ALL.into_iter().find(|u| u.bit() == bit)
    }

    /// What it's called on an object of this kind (none: not offered for it).
    pub fn name(self, kind: ObjectKind) -> Option<&'static str> {
        use ObjectKind as K;
        match (self, kind) {
            (Upgrade::Unbreakable, K::Shower | K::Bathtub | K::Sink | K::Toilet | K::Tv | K::Computer | K::Stereo) => Some("Unbreakable"),
            (Upgrade::Improved, K::Shower | K::Bathtub) => Some("Water Heater"),
            (Upgrade::Improved, K::Tv) => Some("Improved Channels"),
            (Upgrade::Improved, K::Stereo) => Some("Better Speakers"),
            (Upgrade::Improved, K::Computer) => Some("Faster Processor"),
            _ => None,
        }
    }

    /// The Handiness it takes.
    pub fn level(self) -> u32 {
        match self {
            Upgrade::Improved => 3,
            Upgrade::Unbreakable => 5,
        }
    }

    /// How long it takes the unskilled (game minutes).
    pub const MINUTES: f32 = 120.0;
}

/// How much more an improved object gives of a motive.
pub fn boost(kind: ObjectKind, upgrades: Option<&Upgrades>, motive: usize, gain: f32) -> f32 {
    let improved = upgrades.is_some_and(|u| u.has(Upgrade::Improved));
    let boosted = match kind {
        ObjectKind::Shower | ObjectKind::Bathtub => motive == crate::sim::HYGIENE,
        ObjectKind::Tv | ObjectKind::Stereo | ObjectKind::Computer => motive == crate::sim::FUN,
        _ => false,
    };
    if improved && boosted && gain > 0.0 { gain * 1.5 } else { gain }
}

/// An object's upgrades as saved: the object, where it stands, and the upgrade bits.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedUpgrades {
    pub objd: (u32, u32, u64),
    pub position: [f32; 3],
    pub bits: u8,
}

pub fn saved(objects: &Query<(&GameObject, &Transform, &Upgrades)>) -> Vec<SavedUpgrades> {
    objects
        .iter()
        .filter(|(_, _, u)| u.0 != 0)
        .map(|(o, tf, u)| SavedUpgrades { objd: o.objd, position: tf.translation.to_array(), bits: u.0 })
        .collect()
}

/// Upgrades from a save, waiting for their objects (the bought ones are placed a moment after
/// the game loads).
#[derive(Resource, Default)]
pub struct PendingUpgrades(pub Vec<SavedUpgrades>, pub f32);

fn restore_upgrades(mut commands: Commands, mut pending: ResMut<PendingUpgrades>, objects: Query<(Entity, &GameObject, &Transform)>, time: Res<Time>) {
    if pending.0.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    if pending.1 == 0.0 {
        pending.1 = now;
    }
    pending.0.retain(|s| {
        let at = Vec3::from(s.position);
        match objects.iter().find(|(_, o, tf)| o.objd == s.objd && tf.translation.distance(at) < 0.2) {
            Some((e, ..)) => {
                commands.entity(e).insert(Upgrades(s.bits));
                false
            }
            None => true,
        }
    });
    // (Whatever hasn't turned up after a while is gone.)
    if now - pending.1 > 10.0 {
        pending.0.clear();
    }
}
