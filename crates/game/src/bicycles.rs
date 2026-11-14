//! Bicycles, the game's (`Bicycle`, `AdultBicycle`, `ChildBicycle`): the NostalgiCycle for teens
//! and up, the Bi-Trike for Kids for children (too young or too old for one, it's greyed with the
//! game's words). A household Sim's bike offers the trips its car would (`CarOwnable`'s: to work,
//! to a community lot, to visit a family, shopping), and "Make Preferred Vehicle" (`SetOwner`);
//! going out, a Sim takes the vehicle they prefer, else the car, else a bike they can ride
//! (children, who can't drive, their bikes).
//!
//! Off on it, they go to the bike's side (its routing slot), get on and push off (the pack's
//! `bicycle_getOn` for both, the bike rolling forward), and ride off the lot to the street (the
//! seated pose over the bike, its wheels going round: `bicycle_ride`), Saving the Environment as
//! they go (`AddSavingTheEnvironmentBuff`); the bike's gone from the lot till they're back. Home
//! again, they ride in from the street, roll up to where it stands and get off (`getOff`).

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind};
use crate::objanim::PlayingClip;
use crate::rabbitholes::AtRabbitHole;
use crate::sim::{Sim, SimAnim};

pub struct BicyclesPlugin;

impl Plugin for BicyclesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (bike_test, depart, come_back, ride).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(Update, spin.after(crate::objanim::animate_objects).run_if(in_state(PlayMode::Live)));
    }
}

/// Off to ride a bike (the one walked to, to leave on).
#[derive(Component)]
pub struct BikeTrip(pub Entity);

/// The vehicle a Sim prefers to go out on (`SetPreferredVehicle`).
#[derive(Component, Clone, Copy)]
pub struct PreferredVehicle(pub Entity);

/// A bike out with its rider: where it stands at home.
#[derive(Component)]
pub struct BikeOut {
    pub rider: Entity,
    home: Transform,
}

/// Where a bike ridden home is parked: in the nearest bike rack with room (its slot: the rack's
/// own place and facing), the one it stood in first if it did; else where it stood.
pub fn rack_spot(racks: &[Transform], home: Transform, taken: impl Fn(Vec3) -> bool) -> Transform {
    if racks.iter().any(|r| r.translation.distance(home.translation) < 0.3) {
        return home;
    }
    racks.iter().filter(|r| !taken(r.translation)).min_by(|a, b| a.translation.distance(home.translation).total_cmp(&b.translation.distance(home.translation))).copied().unwrap_or(home)
}

/// A bike out of sight down the street with its rider.
#[derive(Component)]
pub struct BikeAway;

/// On a bike: getting on, riding off, riding back, getting off.
#[derive(Component)]
pub struct BikeRide {
    bike: Entity,
    stage: Stage,
    t: f32,
    path: Vec<Vec2>,
    /// Where the bike stands at home, and where it is now.
    home: Transform,
    at: Transform,
    child: bool,
    /// How far it's been ridden (metres), for its wheels and pedals.
    spun: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    On,
    Away,
    Back,
    Off,
}

/// The bike's routing slot the Sim gets on at (`5D2145C0`, half a metre to its side, facing it:
/// the clips' frame); the bike's forward is its +z.
fn slot() -> Transform {
    Transform::from_xyz(0.5, 0.0, 0.0).with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
}

/// A bike's clips (the Sim's and the bike's), and how far it rolls: on getting on (from the seat
/// at rest, `idle`, to riding, `ride_pose`), and in getting off (`getOff`'s start to the seat).
struct Clips {
    on: &'static str,
    ride: &'static str,
    off: &'static str,
    bike_on: &'static str,
    bike_off: &'static str,
    roll_on: f32,
    roll_off: f32,
}

const ADULT: Clips = Clips {
    on: "a2o_bicycle_getOn_x",
    ride: "a2o_bicycle_ride_pose_x",
    off: "a2o_bicycle_getOff_x",
    bike_on: "a2o_bicycle_getOn_bicycle",
    bike_off: "a2o_bicycle_getOff_bicycle",
    roll_on: 2.219,
    roll_off: 2.0,
};
const CHILD: Clips = Clips {
    on: "c2o_bicycle_getOn_x",
    ride: "c2o_bicycle_ride_pose_x",
    off: "c2o_bicycle_getOff_x",
    bike_on: "c2o_bicycle_getOn_bicycle",
    bike_off: "c2o_bicycle_getOff_bicycle",
    roll_on: 2.023,
    roll_off: 2.0,
};
/// How fast a bike's ridden (metres a second at normal speed: the pack's riding cycle's three
/// metres in its second and a third), and the clips' lengths where they're not to be had.
const SPEED: f32 = 2.25;
/// The getting on and off clips' roll of the bike along its length (their offset track), its
/// wheels (front and back) and pedals' crank (bones), the wheels' radius and a pedal stroke's
/// length (metres).
const ROLL_TRACK: u32 = 0x2AF0_A8AE;
const WHEELS: [u32; 2] = [0x748A_EB23, 0x11EF_7D56];
const CRANK: u32 = 0x1A30_F8F0;
const WHEEL_RADIUS: f32 = 0.34;
const STROKE: f32 = 2.3;
const ON_SECS: f32 = 1.97;
const OFF_SECS: f32 = 2.03;

fn clips(child: bool) -> &'static Clips {
    if child { &CHILD } else { &ADULT }
}

/// Where a Sim goes to get on a bike standing so.
pub fn mount_spot(bike: &Transform) -> Vec2 {
    (*bike * slot()).translation.xz()
}

/// Whether a Sim can ride (or drive) a vehicle: a car teens and up, a bike for their age.
pub fn can_use(sim: &Sim, kind: ObjectKind) -> bool {
    match kind {
        ObjectKind::Car => sim.age == crate::sim::Age::Teen || (sim.age.is_grown() && sim.age != crate::sim::Age::Child),
        ObjectKind::Bicycle | ObjectKind::ChildBicycle => kind.suits(sim),
        _ => false,
    }
}

/// The vehicle a Sim goes out on, and where they go to it: the one they prefer, else the car,
/// else a bike they can ride (`GetPreferredVehicle`, `GetReservedVehicle`).
pub fn vehicle<'a>(sim: &Sim, preferred: Option<Entity>, vehicles: impl Iterator<Item = (Entity, &'a GameObject, &'a Transform)>) -> Option<(Entity, Vec2, bool)> {
    let usable: Vec<(Entity, &GameObject, &Transform)> = vehicles.filter(|(_, o, _)| can_use(sim, o.kind)).collect();
    let pick = preferred.and_then(|p| usable.iter().find(|v| v.0 == p)).or_else(|| usable.iter().find(|v| v.1.kind == ObjectKind::Car)).or_else(|| usable.first())?;
    let bike = pick.1.kind != ObjectKind::Car;
    let at = if bike { mount_spot(pick.2) } else { pick.1.use_point(pick.2) };
    Some((pick.0, at, bike))
}

/// A bike's menu beyond its trips: "Make Preferred Vehicle", or, for one the wrong size, greyed
/// with the game's words.
pub fn options(sim: &Sim, bike: Entity, kind: ObjectKind, preferred: Option<Entity>, ui: Option<&crate::layout::UiAssets>) -> Vec<(String, ActionKind)> {
    let words = |k: &str, or: &str| ui.and_then(|u| u.localize(&format!("Gameplay/Objects/Vehicles/Bicycle{k}"))).filter(|s| !s.is_empty()).unwrap_or_else(|| or.to_string());
    if !kind.suits(sim) {
        let why = if kind == ObjectKind::Bicycle {
            words(":GreyedOutTooltipSimTooYoung", "{0.SimFirstName} is too young to ride adult-sized bikes")
        } else {
            words(":GreyedOutTooltipSimTooOld", "{0.SimFirstName} is too old to ride child-sized bikes")
        };
        return vec![("Ride".to_string(), ActionKind::Greyed(why.replace("{0.SimFirstName}", &sim.first)))];
    }
    if preferred == Some(bike) {
        return Vec::new();
    }
    vec![(words("/SetOwner:InteractionName", "Make Preferred Vehicle"), ActionKind::PreferVehicle { vehicle: bike })]
}

/// Off out on a bike: once the trip's begun (they're at its side), on it and away.
#[allow(clippy::type_complexity)]
fn depart(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &BikeTrip, &mut Transform, &mut SimAnim, Option<&mut crate::life::Moodlets>), Or<(Added<AtRabbitHole>, Added<crate::interact::AtWork>)>>,
    stale: Query<(Entity, &ActionQueue), (With<BikeTrip>, Without<AtRabbitHole>, Without<crate::interact::AtWork>)>,
    bikes: Query<(&GameObject, &Transform), Without<Sim>>,
) {
    for (e, q) in &stale {
        let going = q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::GoToWork | ActionKind::Visit { .. } | ActionKind::GoToLot { .. }));
        if !going {
            commands.entity(e).remove::<BikeTrip>();
        }
    }
    for (e, sim, trip, mut tf, mut anim, moodlets) in &mut sims {
        commands.entity(e).remove::<BikeTrip>();
        let Ok((o, btf)) = bikes.get(trip.0) else { continue };
        if tf.translation.xz().distance(mount_spot(btf)) > 1.5 {
            continue;
        }
        let child = o.kind == ObjectKind::ChildBicycle;
        let c = clips(child);
        *tf = *btf * slot();
        anim.pose = crate::sim::Pose::Use;
        commands.entity(e).insert((Visibility::Inherited, crate::anim::ActionClip::new(Some(c.on), std::slice::from_ref(&c.ride)), BikeRide { bike: trip.0, stage: Stage::On, t: 0.0, path: Vec::new(), home: *btf, at: *btf, child, spun: 0.0 }));
        commands.entity(trip.0).insert((BikeOut { rider: e, home: *btf }, PlayingClip { name: c.bike_on, time: 0.0, looping: false }));
        if let Some(mut m) = moodlets {
            m.add(crate::life::MoodletKind::SavingTheEnvironment, clock.minutes);
        }
        info!("bicycles: {} rides off", sim.first);
    }
}

/// Home again with a bike out: on it at the street, riding in.
#[allow(clippy::type_complexity)]
fn come_back(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &mut Transform, &mut SimAnim), (Without<AtRabbitHole>, Without<crate::interact::AtWork>, Without<crate::visit::OnLot>, Without<BikeRide>, Without<GameObject>)>,
    mut bikes: Query<(Entity, &GameObject, &BikeOut, &mut Transform), Without<Sim>>,
    (grid, exit): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::interact::LotExit>>),
    (racks, parked): (Query<(&GameObject, &Transform), (Without<Sim>, Without<BikeOut>, Without<crate::visit::LotObject>)>, Query<(&GameObject, &Transform), (Without<Sim>, Without<BikeOut>, Without<BikeAway>)>),
) {
    let (Some(grid), Some(exit)) = (grid, exit) else { return };
    let mut taken: Vec<Vec3> = Vec::new();
    for (b, o, out, mut btf) in &mut bikes {
        let Ok((e, sim, mut tf, mut anim)) = sims.get_mut(out.rider) else { continue };
        let child = o.kind == ObjectKind::ChildBicycle;
        let c = clips(child);
        // (Parked in the lot's bike rack, where there's one with room: the game's
        // `TryParkBikeInRack`; else back where it stood.)
        let racks_here: Vec<Transform> = racks.iter().filter(|(r, _)| r.kind == ObjectKind::BikeRack).map(|(_, t)| *t).collect();
        let home = rack_spot(&racks_here, out.home, |p| taken.iter().any(|t| t.distance(p) < 0.3) || parked.iter().any(|(g, t)| matches!(g.kind, ObjectKind::Bicycle | ObjectKind::ChildBicycle) && t.translation.distance(p) < 0.3));
        if home.translation.distance(out.home.translation) > 0.01 {
            info!("bicycles: {}'s bike to be parked in the bike rack at {:.1}", sim.first, home.translation);
        }
        taken.push(home.translation);
        let out = BikeOut { rider: out.rider, home };
        // (In from the street to just behind where it stands, the last of it straight on.)
        let fwd = (out.home.rotation * Vec3::Z).xz().normalize_or(Vec2::Y);
        let stop = out.home.translation.xz() - fwd * c.roll_off;
        let lead = stop - fwd * 1.5;
        let start = exit.0;
        let mut path = grid.find_path(start, lead).unwrap_or_else(|| vec![lead]);
        path.push(stop);
        let heading = (path[0] - start).normalize_or(fwd);
        let y = btf.translation.y;
        *btf = Transform::from_xyz(start.x, y, start.y).with_rotation(Quat::from_rotation_y(heading.x.atan2(heading.y)));
        *tf = *btf * slot() * Transform::from_xyz(-c.roll_on, 0.0, 0.0);
        anim.pose = crate::sim::Pose::Use;
        commands.entity(b).remove::<BikeAway>().insert(Visibility::Inherited);
        commands.entity(e).insert((Visibility::Inherited, crate::anim::ActionClip::new(None, std::slice::from_ref(&c.ride)), BikeRide { bike: b, stage: Stage::Back, t: 0.0, path, home: out.home, at: *btf, child, spun: 0.0 }));
        info!("bicycles: {} rides home", sim.first);
    }
}

/// The ride: on and off at the bike's side as the clips have it, along the way at a bike's pace
/// between, the rider in the saddle.
#[allow(clippy::type_complexity)]
fn ride(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    (grid, exit, world): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::interact::LotExit>>, Res<crate::loading::CurrentWorld>),
    mut riders: Query<(Entity, &Sim, &mut BikeRide, &mut Transform, &mut SimAnim)>,
    mut bikes: Query<(&mut Transform, Option<&mut PlayingClip>), Without<Sim>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    for (e, sim, mut r, mut tf, mut anim) in &mut riders {
        let c = clips(r.child);
        r.t += dt;
        let Ok((mut btf, playing)) = bikes.get_mut(r.bike) else {
            commands.entity(e).remove::<(BikeRide, crate::anim::ActionClip)>();
            anim.pose = crate::sim::Pose::Stand;
            continue;
        };
        if let Some(mut p) = playing {
            p.time += dt;
        }
        // (The bike rolled along as the clips roll it, getting on and off.)
        let roll = |lib: &mut crate::anim::ClipLibrary, clip: &str, t: f32| lib.get(&data, clip).and_then(|k| k.tracks.get(&ROLL_TRACK).and_then(|tr| crate::anim::sample_track_vec(&tr.translation, t))).map_or(0.0, |v| v.z);
        match r.stage {
            Stage::On => {
                *tf = r.home * slot();
                let z = roll(&mut lib, c.bike_on, r.t);
                *btf = r.home * Transform::from_xyz(0.0, 0.0, z);
                if r.t >= lib.get(&data, c.on).map_or(ON_SECS, |k| k.duration) {
                    // (Rolled forward with the push off: away to the street.)
                    let fwd = r.home.rotation * Vec3::Z;
                    r.at = Transform::from_translation(r.home.translation + fwd * c.roll_on).with_rotation(r.home.rotation);
                    *btf = r.at;
                    let from = r.at.translation.xz();
                    r.path = match (grid.as_deref(), exit.as_deref()) {
                        (Some(g), Some(x)) => g.find_path(from, x.0).unwrap_or_else(|| vec![x.0]),
                        (None, Some(x)) => vec![x.0],
                        _ => Vec::new(),
                    };
                    r.stage = Stage::Away;
                    r.t = 0.0;
                    commands.entity(r.bike).remove::<PlayingClip>();
                }
            }
            Stage::Away | Stage::Back => {
                let mut step = SPEED * dt;
                while step > 0.0 {
                    let Some(&next) = r.path.first() else { break };
                    let here = r.at.translation.xz();
                    let d = next - here;
                    let len = d.length();
                    let p = if len <= step {
                        r.path.remove(0);
                        step -= len;
                        next
                    } else {
                        let p = here + d / len * step;
                        step = 0.0;
                        p
                    };
                    if len > 1e-3 {
                        r.at.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
                    }
                    r.spun += (p - here).length();
                    r.at.translation = Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y);
                }
                *btf = r.at;
                *tf = r.at * slot() * Transform::from_xyz(-c.roll_on, 0.0, 0.0);
                if r.path.is_empty() {
                    if r.stage == Stage::Away {
                        // (Off the lot: gone till they're back.)
                        commands.entity(e).remove::<(BikeRide, crate::anim::ActionClip)>().insert(Visibility::Hidden);
                        commands.entity(r.bike).insert((Visibility::Hidden, BikeAway));
                        anim.pose = crate::sim::Pose::Stand;
                        info!("bicycles: {} is off down the street", sim.first);
                    } else {
                        // (Up to where it stands: off.)
                        r.stage = Stage::Off;
                        r.t = 0.0;
                        *btf = r.home;
                        *tf = r.home * slot();
                        commands.entity(e).insert(crate::anim::ActionClip::new(Some(c.off), &["a2o_bicycle_idle_x"]));
                        commands.entity(r.bike).insert(PlayingClip { name: c.bike_off, time: 0.0, looping: false });
                    }
                }
            }
            Stage::Off => {
                *tf = r.home * slot();
                let z = roll(&mut lib, c.bike_off, r.t);
                *btf = r.home * Transform::from_xyz(0.0, 0.0, z);
                if r.t >= lib.get(&data, c.off).map_or(OFF_SECS, |k| k.duration) {
                    commands.entity(e).remove::<(BikeRide, crate::anim::ActionClip)>();
                    commands.entity(r.bike).remove::<(PlayingClip, BikeOut)>();
                    *btf = r.home;
                    anim.pose = crate::sim::Pose::Stand;
                    info!("bicycles: {} is home and off the bike", sim.first);
                }
            }
        }
    }
}

/// Its wheels going round as it's ridden, and its pedals.
fn spin(riders: Query<&BikeRide>, rigs: Query<&crate::objanim::ObjectRig>, mut bones: Query<&mut Transform, Without<Sim>>) {
    for r in &riders {
        if !matches!(r.stage, Stage::Away | Stage::Back) {
            continue;
        }
        let Ok(rig) = rigs.get(r.bike) else { continue };
        for (hash, angle) in WHEELS.iter().map(|h| (*h, r.spun / WHEEL_RADIUS)).chain(std::iter::once((CRANK, r.spun / STROKE * std::f32::consts::TAU))) {
            if let Some((bone, rest)) = rig.bone(hash)
                && let Ok(mut b) = bones.get_mut(bone)
            {
                b.rotation = rest.rotation * Quat::from_rotation_x(angle);
            }
        }
    }
}

/// `BIKE_TEST=<n>` (tests): ten seconds in, a NostalgiCycle (a Bi-Trike for a child) put down a
/// few metres from the selected Sim, who rides it to the library and back (an hour's visit:
/// `n` seconds at the clock's speed, else straight away by the clock); the camera on them.
#[allow(clippy::type_complexity)]
fn bike_test(
    mut commands: Commands,
    time: Res<Time>,
    mut stage: Local<(u8, Option<Entity>)>,
    mut sel: Query<(Entity, &Sim, &Transform, &mut ActionQueue), With<crate::sim::Selected>>,
    bikes: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    (catalog, data, world): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Res<crate::loading::CurrentWorld>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    if std::env::var_os("BIKE_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let Ok((me, sim, tf, mut q)) = sel.single_mut() else { return };
    match stage.0 {
        0 if now > 10.0 => {
            let name = if sim.age == crate::sim::Age::Child { "BicycleChildStreet" } else { "BicycleAdultStreet" };
            if let Some(key) = data.0.catalog.iter().find(|c| c.instance_name == name).map(|c| c.objd) {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                let p = tf.translation + Vec3::new(2.0, 0.0, 1.0);
                crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0);
                info!("bike test: a bike at {p}");
                // (BIKE_RACK=1: a bike rack beside it, to be parked in coming home.)
                if std::env::var_os("BIKE_RACK").is_some()
                    && let Some(rack) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("BicycleRack")).map(|c| c.objd)
                {
                    let r = p + Vec3::new(2.5, 0.0, 0.0);
                    crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, rack, r, 0.0);
                    info!("bike test: a bike rack at {r}");
                }
            }
            stage.0 = 1;
        }
        1 => {
            let Some((b, ..)) = bikes.iter().find(|(_, o, _)| matches!(o.kind, ObjectKind::Bicycle | ObjectKind::ChildBicycle)) else { return };
            let lot = (0..world.data.lots.len()).find(|l| world.data.lots[*l].internal_name.to_ascii_lowercase().contains("library")).unwrap_or(0);
            let activity = 0;
            commands.entity(me).insert(PreferredVehicle(b));
            q.push_player(crate::interact::Action::new("Visit", ActionKind::Visit { lot, activity }, false));
            info!("bike test: off to {}", world.data.lots[lot].internal_name);
            *stage = (2, Some(b));
        }
        _ => {}
    }
    if let (Some(b), Ok(mut c)) = (stage.1, cams.single_mut()) {
        let at = bikes.get(b).map(|x| x.2.translation).unwrap_or(tf.translation);
        c.look_at(at);
        c.distance = std::env::var("BIKE_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(6.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bike_is_got_on_from_its_side_facing_it() {
        let bike = Transform::from_xyz(10.0, 0.0, 5.0);
        let at = mount_spot(&bike);
        assert!((at - Vec2::new(10.5, 5.0)).length() < 1e-4);
        let f = bike * slot();
        let facing = (f.rotation * Vec3::Z).xz();
        assert!((facing - Vec2::new(-1.0, 0.0)).length() < 1e-4, "facing the bike");
        let along = (f.rotation * Vec3::X).xz();
        assert!((along - Vec2::new(0.0, 1.0)).length() < 1e-4, "its forward the clips' +x");
    }

    #[test]
    fn a_bike_home_goes_in_the_nearest_rack_with_room() {
        let home = Transform::from_xyz(0.0, 0.0, 0.0);
        let racks = [Transform::from_xyz(5.0, 0.0, 0.0), Transform::from_xyz(2.0, 0.0, 0.0)];
        assert_eq!(rack_spot(&racks, home, |_| false).translation, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(rack_spot(&racks, home, |p| p.x == 2.0).translation, Vec3::new(5.0, 0.0, 0.0), "that one's taken");
        assert_eq!(rack_spot(&racks, home, |_| true).translation, Vec3::ZERO, "no room: where it stood");
        assert_eq!(rack_spot(&[], home, |_| false).translation, Vec3::ZERO);
    }
}
