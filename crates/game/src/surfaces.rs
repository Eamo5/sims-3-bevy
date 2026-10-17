//! Things put on others in Buy mode, as the game's containment slots have them: a lamp on an end
//! table, a vase on a counter, a computer on a desk, a sink set in a counter (the game data's
//! `placement` and `surfaces`, from the OBJDs' slot placement flags and their models' container
//! slots). Pointed at a surface with a free slot it fits, what's held goes in the one nearest the
//! pointer, facing as the slot does; what's on a surface moves with it.

use bevy::prelude::*;
use s3bake::gamedata::{GameDataBaked, SurfaceSlot, slot_fits};

use crate::interact::GameObject;

/// An object's slot placement flags, if it goes in slots at all.
pub fn placement(data: &GameDataBaked, objd: u64) -> Option<u32> {
    data.placement.binary_search_by_key(&objd, |p| p.0).ok().map(|i| data.placement[i].1)
}

/// A surface's container slots.
pub fn slots(data: &GameDataBaked, objd: u64) -> Option<&[SurfaceSlot]> {
    data.surfaces.binary_search_by_key(&objd, |s| s.0).ok().map(|i| data.surfaces[i].1.as_slice())
}

/// Where a slot of an object standing so is, and which way it faces.
pub fn slot_world(tf: &Transform, s: &SurfaceSlot) -> (Vec3, Quat) {
    (tf.transform_point(Vec3::from(s.pos)), tf.rotation * Quat::from_rotation_y(s.yaw))
}

/// The slot pointed at for something held (by its placement flags): of the surfaces the ray
/// meets, the nearest with a free slot it fits, and of those slots the one nearest where the ray
/// comes down to their height. The surface, and where in it the held thing goes.
pub fn pointed_slot<'a>(
    ray: Ray3d,
    fits: u32,
    data: &GameDataBaked,
    surfaces: impl Iterator<Item = (Entity, &'a GameObject, &'a Transform)>,
    taken: impl Fn(Vec3) -> bool,
) -> Option<(Entity, Vec3, Quat)> {
    let dir = Vec3::from(ray.direction);
    let mut best: Option<(f32, Entity, Vec3, Quat)> = None;
    for (e, o, tf) in surfaces {
        let Some(slots) = slots(data, o.objd.2) else { continue };
        let Some(t) = crate::hud::object_hit(ray, &GlobalTransform::from(*tf), o) else { continue };
        if best.is_some_and(|b| b.0 <= t) {
            continue;
        }
        let pick = slots
            .iter()
            .filter(|s| slot_fits(fits, s.flags))
            .map(|s| slot_world(tf, s))
            .filter(|(p, _)| !taken(*p))
            .map(|(p, r)| {
                // (Where the ray comes down to the slot's height.)
                let at = if dir.y.abs() > 1e-4 { ray.origin + dir * ((p.y - ray.origin.y) / dir.y) } else { ray.origin + dir * t };
                (at.xz().distance(p.xz()), p, r)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p, r)) = pick {
            best = Some((t, e, p, r));
        }
    }
    best.map(|(_, e, p, r)| (e, p, r))
}

/// What sits on a surface (within a few centimetres of a slot for things put on it, or a sink's),
/// to move along with it. (Not the chairs at a table or the stools at a bar: those stand on the
/// floor by it.)
pub fn on_surface<'a>(data: &GameDataBaked, objd: u64, tf: &Transform, others: impl Iterator<Item = (Entity, &'a Transform)>) -> Vec<Entity> {
    let Some(slots) = slots(data, objd) else { return Vec::new() };
    let at: Vec<Vec3> = slots.iter().filter(|s| s.flags & (s3bake::gamedata::SLOT_SIZES | 0x400) != 0).map(|s| slot_world(tf, s).0).collect();
    others.filter(|(_, t)| at.iter().any(|p| p.distance(t.translation) < 0.08)).map(|(e, _)| e).collect()
}

/// Where something on a surface is once the surface stands elsewhere.
pub fn carried(old: &Transform, new: &Transform, child: &Transform) -> Transform {
    Transform::from_matrix(new.to_matrix() * old.to_matrix().inverse() * child.to_matrix())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameDataBaked {
        GameDataBaked {
            placement: vec![(7, 0x2000_0008)],
            surfaces: vec![(9, vec![SurfaceSlot { pos: [0.3, 0.8, 0.0], yaw: 0.0, flags: 0x200C_0008 }, SurfaceSlot { pos: [-0.3, 0.8, 0.0], yaw: 0.0, flags: 0x200C_0008 }])],
            ..Default::default()
        }
    }

    fn table() -> GameObject {
        GameObject { kind: crate::interact::ObjectKind::Table, name: "End Table".into(), objd: (0, 0, 9), price: 50, center: Vec2::ZERO, half: Vec2::new(0.45, 0.45), height: 0.8, route: None }
    }

    #[test]
    fn what_is_held_goes_in_the_free_slot_nearest_the_pointer() {
        let d = data();
        let (t, tf) = (table(), Transform::from_xyz(10.0, 0.0, 5.0));
        let e = Entity::from_raw_u32(3).unwrap();
        // (Looking straight down just right of the middle: the right slot.)
        let ray = Ray3d::new(Vec3::new(10.2, 5.0, 5.0), Dir3::NEG_Y);
        let fits = placement(&d, 7).unwrap();
        let got = pointed_slot(ray, fits, &d, std::iter::once((e, &t, &tf)), |_| false).unwrap();
        assert!(got.1.distance(Vec3::new(10.3, 0.8, 5.0)) < 1e-4);
        // (That one taken: the other.)
        let got = pointed_slot(ray, fits, &d, std::iter::once((e, &t, &tf)), |p| p.x > 10.0).unwrap();
        assert!(got.1.distance(Vec3::new(9.7, 0.8, 5.0)) < 1e-4);
        // (A medium lamp fits neither small slot.)
        assert!(pointed_slot(ray, 0x2000_0010, &d, std::iter::once((e, &t, &tf)), |_| false).is_none());
    }

    #[test]
    fn what_is_on_a_surface_moves_with_it() {
        let d = data();
        let old = Transform::from_xyz(10.0, 0.0, 5.0);
        let lamp = Transform::from_xyz(10.3, 0.8, 5.0);
        let e = Entity::from_raw_u32(4).unwrap();
        assert_eq!(on_surface(&d, 9, &old, std::iter::once((e, &lamp))), vec![e]);
        let new = Transform::from_xyz(2.0, 3.0, 1.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let moved = carried(&old, &new, &lamp);
        assert!(moved.translation.distance(Vec3::new(2.0, 3.8, 0.7)) < 1e-4, "{:?}", moved.translation);
    }
}
