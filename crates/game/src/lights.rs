//! Lamps switched by hand, as the game's `LightGameObject`: "Turn On..." or "Turn Off..." this
//! light, the room's or all the lot's (`SwitchLight`: on or off whatever the hour, its
//! `UserControlled` set), "Auto-Lights..." to leave them to the dark again (`SetAutoLights`), and
//! "Set Intensity..." (`SetLightIntensity`: Dim, Normal, Bright, the game's `kIntensityDim` 0.6,
//! `kIntensityNormal` 1 and `kIntensityBright` 2). All there and then; kept in saves.

use bevy::prelude::*;

use crate::building::ActiveBuilding;
use crate::visit::LotObject;

pub struct LightsPlugin;

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, switch_lights.before(crate::building::lamps_at_night));
    }
}

/// How the game names its menus and whose lights they're for.
pub const TURN_ON: &str = "Turn On";
pub const TURN_OFF: &str = "Turn Off";
pub const AUTO: &str = "Auto-Lights";
pub const SET_INTENSITY: &str = "Set Intensity";
pub const SCOPES: [(&str, Scope); 3] = [("This Light", Scope::Light), ("This Room", Scope::Room), ("All Lights", Scope::Lot)];
pub const INTENSITIES: [(&str, f32); 3] = [("Dim", 0.6), ("Normal", 1.0), ("Bright", 2.0)];

/// A lamp: hung from the ceiling (its room's the floor's under it), and whether it's lit now.
#[derive(Component, Default, Debug)]
pub struct Lamp {
    pub ceiling: bool,
    pub lit: bool,
}

/// A lamp switched on or off by hand, whatever the hour (the game's `UserControlled`); without,
/// it's on after dark.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Switched(pub bool);

/// A lamp's brightness other than Normal (the game's `Intensity`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Dimmer(pub f32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    On,
    Off,
    Auto,
    Intensity(f32),
}

/// The game's `LightsToChange`: this light, the lights of its room (outdoors, the lot's outdoor
/// ones), or all the lot's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    Light,
    Room,
    Lot,
}

/// What's been asked of a lamp from its menu, for `switch_lights`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Asked(pub Op, pub Scope);

/// What a choice from a lamp's menus asks: the submenu's title (one of the intensities under
/// "Set Intensity...") and the scope picked in it.
pub fn chosen(title: &str, label: &str) -> Option<Asked> {
    let scope = SCOPES.iter().find(|s| s.0 == label)?.1;
    let op = match title {
        TURN_ON => Op::On,
        TURN_OFF => Op::Off,
        AUTO => Op::Auto,
        _ => Op::Intensity(INTENSITIES.iter().find(|i| i.0 == title)?.1),
    };
    Some(Asked(op, scope))
}

/// The room a lamp lights (its floor and region), or `None` outdoors.
fn room(b: &ActiveBuilding, lamp: &Lamp, at: Vec3) -> Option<(u8, u16)> {
    let p = if lamp.ceiling { at - Vec3::Y } else { at + Vec3::Y * 0.1 };
    let level = b.level_at_point(p);
    b.room_id(level, p).map(|r| (level, r))
}

fn switch_lights(
    mut commands: Commands,
    asked: Query<(Entity, &Asked)>,
    lamps: Query<(Entity, &Lamp, &GlobalTransform, Option<&LotObject>)>,
    building: Option<Res<ActiveBuilding>>,
) {
    for (e, a) in &asked {
        commands.entity(e).remove::<Asked>();
        // Where each is: its lot (none: home), and its room in the house there.
        let place = |(_, lamp, tf, lot): (Entity, &Lamp, &GlobalTransform, Option<&LotObject>)| {
            let lot = lot.map(|l| l.0);
            let b = building.as_deref().and_then(|b| match lot {
                None => Some(b),
                Some(l) => b.away.as_deref().filter(|a| a.lot == l),
            });
            (lot, b.and_then(|b| room(b, lamp, tf.translation())))
        };
        let Ok(this) = lamps.get(e) else { continue };
        let here = place(this);
        let mut n = 0;
        for l in &lamps {
            let ok = match a.1 {
                Scope::Light => l.0 == e,
                Scope::Room => place(l) == here,
                Scope::Lot => place(l).0 == here.0,
            };
            if !ok {
                continue;
            }
            n += 1;
            let mut c = commands.entity(l.0);
            match a.0 {
                Op::On => c.insert(Switched(true)),
                Op::Off => c.insert(Switched(false)),
                Op::Auto => c.remove::<Switched>(),
                Op::Intensity(v) if v == 1.0 => c.remove::<Dimmer>(),
                Op::Intensity(v) => c.insert(Dimmer(v)),
            };
        }
        info!("lights: {:?} {:?} ({n} lamps)", a.0, a.1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_from_the_menus_is_what_it_asks() {
        assert_eq!(chosen(TURN_ON, "This Room"), Some(Asked(Op::On, Scope::Room)));
        assert_eq!(chosen(AUTO, "All Lights"), Some(Asked(Op::Auto, Scope::Lot)));
        assert_eq!(chosen("Dim", "This Light"), Some(Asked(Op::Intensity(0.6), Scope::Light)));
        assert_eq!(chosen("Lock", "This Light"), None);
        assert_eq!(chosen(TURN_OFF, "Everybody"), None);
    }

    #[test]
    fn switched_lamps_change_and_the_rest_are_left() {
        let mut app = App::new();
        app.add_systems(Update, switch_lights);
        let a = app.world_mut().spawn((Lamp::default(), GlobalTransform::default())).id();
        let b = app.world_mut().spawn((Lamp::default(), GlobalTransform::default(), Switched(true))).id();
        let away = app.world_mut().spawn((Lamp::default(), GlobalTransform::default(), LotObject(3))).id();
        app.world_mut().entity_mut(a).insert(Asked(Op::Off, Scope::Light));
        app.update();
        assert_eq!(app.world().get::<Switched>(a), Some(&Switched(false)));
        assert_eq!(app.world().get::<Switched>(b), Some(&Switched(true)));
        assert!(app.world().get::<Asked>(a).is_none());
        app.world_mut().entity_mut(a).insert(Asked(Op::Auto, Scope::Lot));
        app.update();
        assert!(app.world().get::<Switched>(a).is_none() && app.world().get::<Switched>(b).is_none());
        app.world_mut().entity_mut(b).insert(Asked(Op::Intensity(2.0), Scope::Lot));
        app.update();
        assert_eq!(app.world().get::<Dimmer>(a), Some(&Dimmer(2.0)));
        assert!(app.world().get::<Dimmer>(away).is_none());
    }
}
