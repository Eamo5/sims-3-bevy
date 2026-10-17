//! Handiness upgrades, as the game's: a handy Sim can make an appliance unbreakable ("Prevent
//! Breakage"), better at what it does (a water heater for the shower and the bath, "Boost
//! Channels" for the TV, "Improve Speakers" for the stereo, "Enhance Graphics" for the computer,
//! "Improve Meal Quality" for the stove), self-cleaning (the bathroom's), fireproof (the stove, the
//! fireplace) or lighting itself after dark (a fireplace's "Auto-Light"), each from its own
//! Handiness level (the tunings' `SkillThreshold`). Upgrading takes a while, with the game's
//! repair animations, and builds Handiness; electronics can shock the unskilled. Upgrades are
//! kept in saves, found again by the object and where it stands.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{GameObject, ObjectKind};

pub struct UpgradesPlugin;

impl Plugin for UpgradesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingUpgrades>().add_systems(Update, (restore_upgrades, auto_light).run_if(in_state(PlayMode::Live)));
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
    /// It does what it does better (by half again; a stove's meals a fifth better).
    Improved,
    /// It never gets dirty.
    SelfCleaning,
    /// It never catches fire, nor sets anything else alight.
    Fireproof,
    /// A fireplace lights itself after dark, and goes out in the morning.
    AutoLight,
}

impl Upgrade {
    pub const ALL: [Upgrade; 5] = [Upgrade::Unbreakable, Upgrade::Improved, Upgrade::SelfCleaning, Upgrade::Fireproof, Upgrade::AutoLight];

    pub fn bit(self) -> u8 {
        match self {
            Upgrade::Unbreakable => 1,
            Upgrade::Improved => 2,
            Upgrade::SelfCleaning => 4,
            Upgrade::Fireproof => 8,
            Upgrade::AutoLight => 16,
        }
    }

    pub fn from_bit(bit: u8) -> Option<Upgrade> {
        Self::ALL.into_iter().find(|u| u.bit() == bit)
    }

    /// What it's called on an object of this kind, as the game's menus have it (none: not offered
    /// for it).
    pub fn name(self, kind: ObjectKind) -> Option<&'static str> {
        use ObjectKind as K;
        match (self, kind) {
            (Upgrade::Unbreakable, K::Shower | K::Bathtub | K::ShowerTub | K::Sink | K::Toilet | K::Computer) => Some("Prevent Breakage"),
            (Upgrade::Unbreakable, K::Tv | K::Stereo) => Some("Make Unbreakable"),
            (Upgrade::Improved, K::Shower | K::Bathtub | K::ShowerTub) => Some("Water Heater"),
            (Upgrade::Improved, K::Tv) => Some("Boost Channels"),
            (Upgrade::Improved, K::Stereo) => Some("Improve Speakers"),
            (Upgrade::Improved, K::Computer) => Some("Enhance Graphics"),
            (Upgrade::Improved, K::Stove) => Some("Improve Meal Quality"),
            (Upgrade::SelfCleaning, K::Shower | K::Bathtub | K::ShowerTub | K::Sink | K::Toilet) => Some("Make Self-Cleaning"),
            (Upgrade::Fireproof, K::Stove | K::Fireplace) => Some("Make Fireproof"),
            (Upgrade::AutoLight, K::Fireplace) => Some("Auto-Light"),
            _ => None,
        }
    }

    /// The Handiness it takes on an object of this kind (the tunings' `SkillThreshold`).
    pub fn level(self, kind: ObjectKind) -> u32 {
        use ObjectKind as K;
        match (self, kind) {
            (Upgrade::Unbreakable, _) => 6,
            (Upgrade::Improved, K::Bathtub | K::Stereo) => 3,
            (Upgrade::Improved, K::Shower | K::ShowerTub) => 6,
            (Upgrade::Improved, K::Tv | K::Stove) => 7,
            (Upgrade::Improved, _) => 8,
            (Upgrade::SelfCleaning, _) => 3,
            (Upgrade::Fireproof, K::Stove) => 4,
            (Upgrade::Fireproof, _) => 6,
            (Upgrade::AutoLight, _) => 7,
        }
    }

    /// How long it takes the unskilled (game minutes).
    pub const MINUTES: f32 = 120.0;
}

/// How much more an improved object gives of a motive.
pub fn boost(kind: ObjectKind, upgrades: Option<&Upgrades>, motive: usize, gain: f32) -> f32 {
    let improved = upgrades.is_some_and(|u| u.has(Upgrade::Improved));
    let boosted = match kind {
        ObjectKind::Shower | ObjectKind::Bathtub | ObjectKind::ShowerTub => motive == crate::sim::HYGIENE,
        ObjectKind::Tv | ObjectKind::Stereo | ObjectKind::Computer => motive == crate::sim::FUN,
        _ => false,
    };
    if improved && boosted && gain > 0.0 { gain * 1.5 } else { gain }
}

/// Whether an object has an upgrade.
pub fn has(upgrades: Option<&Upgrades>, u: Upgrade) -> bool {
    upgrades.is_some_and(|x| x.has(u))
}

/// "Improve Meal Quality": what a stove's meals' bonus points are multiplied by (the game's
/// `CookingQualityMultiplier`).
pub const IMPROVED_MEAL: f32 = 1.2;

/// Fireplaces with "Auto-Light": lit as it gets dark, put out once it's light (the game's
/// fireplace switched as its lights are).
fn auto_light(
    night: Res<crate::clock::Night>,
    fireplaces: Query<(Entity, &GameObject, &Upgrades, Has<crate::fireplace::Lit>)>,
    mut requests: MessageWriter<crate::fireplace::FireplaceRequest>,
    mut was_dark: Local<Option<bool>>,
) {
    let dark = night.0 > 0.5;
    if *was_dark == Some(dark) {
        return;
    }
    *was_dark = Some(dark);
    for (e, o, u, lit) in &fireplaces {
        if o.kind != ObjectKind::Fireplace || !u.has(Upgrade::AutoLight) {
            continue;
        }
        if dark && !lit {
            requests.write(crate::fireplace::FireplaceRequest::Light(e));
        } else if !dark && lit {
            requests.write(crate::fireplace::FireplaceRequest::PutOut(e));
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_are_offered_as_the_games_with_its_levels() {
        use ObjectKind as K;
        assert_eq!(Upgrade::SelfCleaning.name(K::Toilet), Some("Make Self-Cleaning"));
        assert_eq!(Upgrade::SelfCleaning.name(K::Tv), None);
        assert_eq!(Upgrade::Fireproof.name(K::Stove), Some("Make Fireproof"));
        assert_eq!(Upgrade::AutoLight.name(K::Fireplace), Some("Auto-Light"));
        assert_eq!(Upgrade::Improved.name(K::Stove), Some("Improve Meal Quality"));
        assert_eq!((Upgrade::Improved.level(K::Computer), Upgrade::Improved.level(K::Bathtub), Upgrade::Fireproof.level(K::Stove)), (8, 3, 4));
        // (Each its own bit, saved as they were.)
        for u in Upgrade::ALL {
            assert_eq!(Upgrade::from_bit(u.bit()), Some(u));
        }
    }
}
