//! Handiness upgrades, as the game's (`UpgradableComponent`, each named "Upgrade: ..." by its
//! component's word): a handy Sim can make an appliance "Unbreakable", better at what it does
//! ("Boost Channels" for the TV, "Improve Speakers" for the stereo, "Improved Graphics" for the
//! computer, "Improve Meal Quality" for the stove, "Silent Running" for the dishwasher, "Faster
//! Cooking" for the microwave, "Improved Crushing" for the trash compactor, a "Clean Polymer
//! Injection System" for the washing machine), "Self-Cleaning" (the bathroom's, the stove),
//! "Fireproof" (the stove, the fireplace), automatic ("Auto Light" for a fireplace after dark,
//! "Auto Water" for a sprinkler at five each morning), a fireplace's "Change Fire Color", or the
//! stereo's "Wire House With Speakers", each from its own Handiness level (the tunings'
//! `SkillThreshold`). Upgrading takes a while, with the game's repair animations, and builds
//! Handiness; electronics can shock the unskilled. Upgrades are kept in saves, found again by
//! the object and where it stands.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{GameObject, ObjectKind};

pub struct UpgradesPlugin;

impl Plugin for UpgradesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingUpgrades>().add_systems(Update, (restore_upgrades, auto_light, auto_water, upgrade_menu_test).run_if(in_state(PlayMode::Live)));
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
    /// It does what it does better: the TV's, stereo's and computer's fun half again, a stove's
    /// meals a fifth better, a dishwasher silent, a microwave quicker, a trash compactor holding
    /// twice as much, a washing machine's fresh clothes lasting half again as long.
    Improved,
    /// It never gets dirty.
    SelfCleaning,
    /// It never catches fire, nor sets anything else alight.
    Fireproof,
    /// It sees to itself: a fireplace lit after dark and out in the morning, a sprinkler on at
    /// five each morning for an hour.
    Automatic,
    /// A fireplace's fire can be turned blue, green or purple ("Change Fire Color": `fireplace`).
    ChangeColor,
    /// A stereo heard all over the house.
    WholeHouse,
}

impl Upgrade {
    pub const ALL: [Upgrade; 7] = [Upgrade::Unbreakable, Upgrade::Improved, Upgrade::SelfCleaning, Upgrade::Fireproof, Upgrade::Automatic, Upgrade::ChangeColor, Upgrade::WholeHouse];

    pub fn bit(self) -> u8 {
        match self {
            Upgrade::Unbreakable => 1,
            Upgrade::Improved => 2,
            Upgrade::SelfCleaning => 4,
            Upgrade::Fireproof => 8,
            Upgrade::Automatic => 16,
            Upgrade::ChangeColor => 32,
            Upgrade::WholeHouse => 64,
        }
    }

    pub fn from_bit(bit: u8) -> Option<Upgrade> {
        Self::ALL.into_iter().find(|u| u.bit() == bit)
    }

    /// What it's called on an object of this kind, as the game's menus have it ("Upgrade: " and
    /// its `Gameplay/ObjectComponents/Upgrade` word; none: not offered for it).
    pub fn name(self, kind: ObjectKind) -> Option<&'static str> {
        use ObjectKind as K;
        match (self, kind) {
            (Upgrade::Unbreakable, K::Shower | K::Bathtub | K::ShowerTub | K::Sink | K::Toilet | K::Computer | K::Dishwasher | K::Tv | K::Stereo | K::TrashCompactor | K::Washer) => Some("Unbreakable"),
            (Upgrade::Improved, K::Tv) => Some("Boost Channels"),
            (Upgrade::Improved, K::Stereo) => Some("Improve Speakers"),
            (Upgrade::Improved, K::Computer) => Some("Improved Graphics"),
            (Upgrade::Improved, K::Stove) => Some("Improve Meal Quality"),
            (Upgrade::Improved, K::Dishwasher) => Some("Silent Running"),
            (Upgrade::Improved, K::Microwave) => Some("Faster Cooking"),
            (Upgrade::Improved, K::TrashCompactor) => Some("Improved Crushing"),
            (Upgrade::Improved, K::Washer) => Some("Clean Polymer Injection System"),
            (Upgrade::SelfCleaning, K::Shower | K::Bathtub | K::ShowerTub | K::Sink | K::Toilet | K::Stove) => Some("Self-Cleaning"),
            (Upgrade::Fireproof, K::Stove | K::Fireplace) => Some("Fireproof"),
            (Upgrade::Automatic, K::Fireplace) => Some("Auto Light"),
            (Upgrade::Automatic, K::Sprinkler) => Some("Auto Water"),
            (Upgrade::ChangeColor, K::Fireplace) => Some("Change Fire Color"),
            (Upgrade::WholeHouse, K::Stereo) => Some("Wire House With Speakers"),
            _ => None,
        }
    }

    /// The Handiness it takes on an object of this kind (the tunings' `SkillThreshold`).
    pub fn level(self, kind: ObjectKind) -> u32 {
        use ObjectKind as K;
        match (self, kind) {
            (Upgrade::Unbreakable, _) => 6,
            (Upgrade::Improved, K::Stereo | K::Dishwasher) => 3,
            (Upgrade::Improved, K::TrashCompactor | K::Washer) => 6,
            (Upgrade::Improved, K::Tv | K::Stove | K::Microwave) => 7,
            (Upgrade::Improved, _) => 8,
            (Upgrade::SelfCleaning, _) => 3,
            (Upgrade::Fireproof, K::Stove) => 4,
            (Upgrade::Fireproof, _) => 6,
            (Upgrade::Automatic, K::Sprinkler) => 4,
            (Upgrade::Automatic, _) => 7,
            (Upgrade::ChangeColor, _) => 9,
            (Upgrade::WholeHouse, _) => 8,
        }
    }

    /// How long it takes the unskilled (game minutes).
    pub const MINUTES: f32 = 120.0;
}

/// How much more an improved object gives of a motive.
pub fn boost(kind: ObjectKind, upgrades: Option<&Upgrades>, motive: usize, gain: f32) -> f32 {
    let improved = upgrades.is_some_and(|u| u.has(Upgrade::Improved));
    let boosted = matches!(kind, ObjectKind::Tv | ObjectKind::Stereo | ObjectKind::Computer) && motive == crate::sim::FUN;
    if improved && boosted && gain > 0.0 { gain * 1.5 } else { gain }
}

/// "Faster Cooking": a microwave's time, as its cook speed's 8 to the usual 6
/// (`CookSpeedMultiplier`, `kNormalCookTimeSpeedMultiplier`).
pub const FASTER_COOKING: f32 = 6.0 / 8.0;
/// "Improved Crushing": how much more a trash compactor holds (`kTrashUnitCapacityImprovedCrushing`
/// 100 to 50).
pub const IMPROVED_CRUSHING: u8 = 2;
/// "Clean Polymer Injection System": how much longer clean clothes stay fresh
/// (`kFreshClothingBuffPolymerSystemUpgradeTimeoutMultiplier`).
pub const POLYMER_FRESH: f64 = 1.5;
/// "Auto Water": when a sprinkler turns itself on and off (`kAutoWaterTimeOfDay`,
/// `kAutoWaterOffTimeOfDay`).
pub const AUTO_WATER_HOURS: (f64, f64) = (5.0, 6.0);

/// Whether an object has an upgrade.
pub fn has(upgrades: Option<&Upgrades>, u: Upgrade) -> bool {
    upgrades.is_some_and(|x| x.has(u))
}

/// "Improve Meal Quality": what a stove's meals' bonus points are multiplied by (the game's
/// `CookingQualityMultiplier`).
pub const IMPROVED_MEAL: f32 = 1.2;

/// `UPGRADE_MENU=<kind>` (tests): ten seconds in, the selected Sim, Handiness 10, at the lot's
/// first object of that kind (`Stereo`, `Microwave`...), its menu up, the view on it.
fn upgrade_menu_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<&mut crate::interact::Skills, With<crate::sim::Selected>>,
    objects: Query<(Entity, &GameObject, &GlobalTransform)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut done: Local<bool>,
) {
    let Ok(kind) = std::env::var("UPGRADE_MENU") else { return };
    let t = time.elapsed_secs();
    let Some((e, _, at)) = objects.iter().find(|(_, o, _)| format!("{:?}", o.kind) == kind) else { return };
    if t > 7.0
        && let Ok(mut cam) = cams.single_mut()
    {
        let front = at.rotation() * Vec3::Z;
        cam.look_at(at.translation() + Vec3::Y * 0.5);
        cam.distance = 9.0;
        cam.yaw = front.x.atan2(front.z) + 0.5;
    }
    if *done || t < 10.0 {
        return;
    }
    *done = true;
    if let Ok(mut s) = sims.single_mut() {
        s.0.insert("Handiness", 10.0);
    }
    commands.insert_resource(crate::hud::PieFor(e));
}

/// Sprinklers with "Auto Water": on at five each morning till six, not with snow on the ground
/// (the game's `SetUpAutoSprinklerAlarms`).
fn auto_water(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    weather: Res<crate::weather::Weather>,
    sprinklers: Query<(Entity, &GameObject, &Upgrades), Without<crate::gardening::Sprinkling>>,
    mut last_day: Local<Option<u32>>,
) {
    let day = (clock.minutes / 1440.0) as u32;
    let hour = clock.minutes / 60.0 % 24.0;
    let (on, off) = AUTO_WATER_HOURS;
    if !(on..off).contains(&hour) || *last_day == Some(day) {
        return;
    }
    *last_day = Some(day);
    if weather.snow > 0.0 {
        return;
    }
    for (e, o, u) in &sprinklers {
        if o.kind == ObjectKind::Sprinkler && u.has(Upgrade::Automatic) {
            commands.entity(e).insert(crate::gardening::Sprinkling { until: day as f64 * 1440.0 + off * 60.0 });
            info!("upgrades: the {} waters by itself", o.name);
        }
    }
}

/// Fireplaces with "Auto Light": lit as it gets dark, put out once it's light (the game's
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
        if o.kind != ObjectKind::Fireplace || !u.has(Upgrade::Automatic) {
            continue;
        }
        if dark && !lit {
            requests.write(crate::fireplace::FireplaceRequest::Light(e));
        } else if !dark && lit {
            requests.write(crate::fireplace::FireplaceRequest::PutOut(e));
        }
    }
}

/// An object's upgrades as saved: the object, where it stands, the upgrade bits, and a
/// fireplace's fire colour (`fireplace::FireColor`'s number).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedUpgrades {
    pub objd: (u32, u32, u64),
    pub position: [f32; 3],
    pub bits: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub color: u8,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

pub fn saved(objects: &Query<(&GameObject, &Transform, &Upgrades, Option<&crate::fireplace::FireColor>)>) -> Vec<SavedUpgrades> {
    objects
        .iter()
        .filter(|(_, _, u, _)| u.0 != 0)
        .map(|(o, tf, u, c)| SavedUpgrades { objd: o.objd, position: tf.translation.to_array(), bits: u.0, color: c.map_or(0, |c| *c as u8) })
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
                if let Some(c) = crate::fireplace::FireColor::ALL.get(s.color as usize).filter(|_| s.color != 0) {
                    commands.entity(e).insert(*c);
                }
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
        assert_eq!(Upgrade::SelfCleaning.name(K::Toilet), Some("Self-Cleaning"));
        assert_eq!(Upgrade::SelfCleaning.name(K::Stove), Some("Self-Cleaning"));
        assert_eq!(Upgrade::SelfCleaning.name(K::Tv), None);
        assert_eq!(Upgrade::Fireproof.name(K::Stove), Some("Fireproof"));
        assert_eq!(Upgrade::Automatic.name(K::Fireplace), Some("Auto Light"));
        assert_eq!((Upgrade::Automatic.name(K::Sprinkler), Upgrade::Automatic.level(K::Sprinkler)), (Some("Auto Water"), 4));
        assert_eq!((Upgrade::ChangeColor.name(K::Fireplace), Upgrade::ChangeColor.name(K::Stove), Upgrade::ChangeColor.level(K::Fireplace)), (Some("Change Fire Color"), None, 9));
        assert_eq!(Upgrade::Improved.name(K::Stove), Some("Improve Meal Quality"));
        assert_eq!((Upgrade::Improved.name(K::Microwave), Upgrade::Improved.level(K::Microwave)), (Some("Faster Cooking"), 7));
        assert_eq!((Upgrade::Improved.name(K::TrashCompactor), Upgrade::Unbreakable.name(K::TrashCompactor)), (Some("Improved Crushing"), Some("Unbreakable")));
        assert_eq!((Upgrade::WholeHouse.name(K::Stereo), Upgrade::WholeHouse.level(K::Stereo)), (Some("Wire House With Speakers"), 8));
        // (Improved Crushing: a compactor holds twice as much; a can's no different.)
        let crushing = Upgrades(Upgrade::Improved.bit());
        assert_eq!(crate::surroundings::trash_capacity(K::TrashCompactor, Some(&crushing)), 2 * crate::surroundings::trash_capacity(K::TrashCompactor, None));
        assert_eq!(crate::surroundings::trash_capacity(K::TrashCan, Some(&crushing)), crate::surroundings::TRASH_CAPACITY);
        assert!((FASTER_COOKING - 0.75).abs() < 1e-6);
        // (No water heater: the game hasn't one.)
        assert_eq!(Upgrade::Improved.name(K::Shower), None);
        assert_eq!((Upgrade::Improved.level(K::Computer), Upgrade::Improved.level(K::Dishwasher), Upgrade::Fireproof.level(K::Stove)), (8, 3, 4));
        // (Each its own bit, clear of a doorbell's, saved as they were.)
        for u in Upgrade::ALL {
            assert_eq!(Upgrade::from_bit(u.bit()), Some(u));
            assert!(crate::frontdoor::bell_of(u.bit()).is_none());
        }
    }
}
