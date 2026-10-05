//! Town traffic: cars driving the world's roads around the camera, keeping to the right-hand lane
//! along the road graph's curves and crossing its intersections onto the next road.

use bevy::prelude::*;
use rand::Rng;

use crate::baked::Baked;
use crate::camera::SimsCamera;
use crate::clock::{GameClock, SPEED_RATES};
use crate::loading::CurrentWorld;
use crate::objects::{AssetCtx, ObjectAssets, spawn_parts};
use crate::{AppState, PlayMode};

pub struct TrafficPlugin;

impl Plugin for TrafficPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingRides>().add_systems(Update, (rides, traffic).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The everyday cars that drive about town (catalogue instance names).
const CARS: [&str; 10] = ["CarSedan", "CarHatchback", "CarPickup2door", "CarSports", "CarExpensive1", "CarExpensive2", "CarUsed1", "CarUsed2", "CarNormal1", "CarVan4door"];
/// How many cars are about at once, within what distance of the camera's focus.
const MAX_CARS: usize = 10;
const NEAR: f32 = 170.0;
/// Driving speed (m/s of game time at normal speed) and the lane's offset from the middle.
const SPEED: f32 = 9.0;
const LANE: f32 = 1.75;
/// How close a road's end must be to an intersection to join it, or to another road's end to
/// carry straight on.
const AT_JUNCTION: f32 = 22.0;
const JOINED: f32 = 1.0;

/// A road of the graph: a cubic bezier, with its length.
struct Road {
    p: [Vec2; 4],
    len: f32,
}

impl Road {
    fn at(&self, t: f32) -> Vec2 {
        let u = 1.0 - t;
        self.p[0] * (u * u * u) + self.p[1] * (3.0 * u * u * t) + self.p[2] * (3.0 * u * t * t) + self.p[3] * (t * t * t)
    }

    fn tangent(&self, t: f32) -> Vec2 {
        let u = 1.0 - t;
        let d = (self.p[1] - self.p[0]) * (3.0 * u * u) + (self.p[2] - self.p[1]) * (6.0 * u * t) + (self.p[3] - self.p[2]) * (3.0 * t * t);
        d.normalize_or(self.p[3] - self.p[0]).normalize_or(Vec2::X)
    }

    fn end(&self, forward: bool) -> Vec2 {
        if forward { self.p[3] } else { self.p[0] }
    }
}

/// The road graph, ready for driving.
#[derive(Default)]
struct Roads {
    roads: Vec<Road>,
    junctions: Vec<Vec2>,
}

impl Roads {
    fn build(world: &crate::loading::WorldInfo) -> Self {
        let roads = world
            .road_curves
            .iter()
            .map(|c| {
                let p = c.map(Vec2::from);
                let mut len = 0.0;
                let r = Road { p, len: 0.0 };
                let mut prev = r.at(0.0);
                for i in 1..=16 {
                    let q = r.at(i as f32 / 16.0);
                    len += prev.distance(q);
                    prev = q;
                }
                Road { p, len: len.max(0.5) }
            })
            .filter(|r| r.len > 2.0)
            .collect();
        Self { roads, junctions: world.road_intersections.iter().map(|i| Vec2::new(i[0], i[1])).collect() }
    }

    /// Where to go from the end `at` of road `from`: another road carrying straight on, or one
    /// leaving the same intersection (with which way along it).
    fn next(&self, from: usize, at: Vec2, rng: &mut impl Rng) -> Option<(usize, bool)> {
        let ends = |near: Vec2, within: f32| -> Vec<(usize, bool)> {
            self.roads
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != from)
                .flat_map(|(i, r)| [(i, true, r.p[0]), (i, false, r.p[3])])
                .filter(|(_, _, p)| p.distance(near) < within)
                .map(|(i, f, _)| (i, f))
                .collect()
        };
        let straight = ends(at, JOINED);
        if !straight.is_empty() {
            return Some(straight[rng.random_range(0..straight.len())]);
        }
        let junction = self.junctions.iter().copied().filter(|j| j.distance(at) < AT_JUNCTION).min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))?;
        let out = ends(junction, AT_JUNCTION);
        (!out.is_empty()).then(|| out[rng.random_range(0..out.len())])
    }
}

/// A car on the road: which road, which way, how far along (0..1), and when crossing an
/// intersection, the straight line it's on.
#[derive(Component)]
struct Car {
    road: usize,
    forward: bool,
    t: f32,
    speed: f32,
    crossing: Option<(Vec2, Vec2, f32)>,
    /// Seconds to stand at the curb first (picking someone up or dropping them off).
    hold: f32,
}

/// A carpool, school bus or taxi come for someone (not counted with the passing traffic).
#[derive(Component)]
struct Ride;

/// Rides to send: where (the curb nearest), and which vehicle (catalogue instance name).
#[derive(Resource, Default)]
pub struct PendingRides(pub Vec<(Vec2, &'static str)>);

impl Roads {
    /// The point of a road nearest `p`: which road and how far along.
    fn nearest(&self, p: Vec2) -> Option<(usize, f32, f32)> {
        let mut best: Option<(usize, f32, f32)> = None;
        for (i, r) in self.roads.iter().enumerate() {
            for k in 0..=32 {
                let t = k as f32 / 32.0;
                let d = r.at(t).distance(p);
                if best.is_none_or(|b| d < b.2) {
                    best = Some((i, t, d));
                }
            }
        }
        best
    }
}

/// Someone leaving (for work in the carpool, for school on the bus, or by taxi) or coming back
/// gets a ride at the curb nearest them.
#[allow(clippy::type_complexity)]
fn rides(
    mut queue: ResMut<PendingRides>,
    departed: Query<(&Transform, Option<&crate::rabbitholes::AtRabbitHole>, Has<crate::interact::AtWork>), Or<(Added<crate::interact::AtWork>, Added<crate::rabbitholes::AtRabbitHole>)>>,
    (mut back_work, mut back_trip): (RemovedComponents<crate::interact::AtWork>, RemovedComponents<crate::rabbitholes::AtRabbitHole>),
    members: Query<&Transform, With<crate::sim::HouseholdMember>>,
) {
    for (tf, trip, work) in &departed {
        let kind = match trip {
            Some(t) if std::ptr::eq(t.activity, &crate::rabbitholes::SCHOOL) => "CarBusSchool",
            _ if work => "CarServiceSedan",
            _ => "CarTaxi",
        };
        queue.0.push((tf.translation.xz(), kind));
    }
    for e in back_work.read().chain(back_trip.read()) {
        if let Ok(tf) = members.get(e) {
            queue.0.push((tf.translation.xz(), "CarTaxi"));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn traffic(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    world: Res<CurrentWorld>,
    data: Res<Baked>,
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    cams: Query<&SimsCamera>,
    mut cars: Query<(Entity, &mut Car, &mut Transform, Has<Ride>)>,
    mut roads: Local<Option<Roads>>,
    mut pending: ResMut<PendingRides>,
) {
    let roads = roads.get_or_insert_with(|| Roads::build(&world.data));
    if roads.roads.is_empty() {
        return;
    }
    let Ok(cam) = cams.single() else { return };
    let focus = cam.focus.xz();
    let mut rng = rand::rng();
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed].min(4.0);
    let hm = &world.data.heightmap;
    let mut count = 0;
    // Rides pull up at the curb.
    for (at, model) in pending.0.drain(..) {
        let Some((road, t, _)) = roads.nearest(at) else { continue };
        let Some(objd) = data.0.catalog.iter().find(|c| c.instance_name == model).map(|c| c.objd) else { continue };
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let parts = assets.object(&mut ctx, objd);
        if parts.is_empty() {
            continue;
        }
        let p = roads.roads[road].at(t);
        let e = spawn_parts(&mut commands, &parts, Transform::from_xyz(p.x, hm.sample(p.x, p.y), p.y));
        commands.entity(e).insert((
            Car { road, forward: rng.random_bool(0.5), t, speed: SPEED * 0.8, crossing: None, hold: 2.5 },
            Ride,
            DespawnOnExit(AppState::InGame),
        ));
    }
    for (e, mut car, mut tf, ride) in &mut cars {
        // Out of sight (or zoomed far out): gone.
        if tf.translation.xz().distance(focus) > NEAR * 1.3 || cam.distance > 240.0 {
            commands.entity(e).despawn();
            continue;
        }
        count += usize::from(!ride);
        let step = if car.hold > 0.0 {
            car.hold -= time.delta_secs();
            0.0
        } else {
            car.speed * dt
        };
        let (pos, dir) = match car.crossing {
            Some((a, b, s)) => {
                let len = a.distance(b).max(0.1);
                let s = s + step / len;
                if s >= 1.0 {
                    car.crossing = None;
                } else {
                    car.crossing = Some((a, b, s));
                }
                (a.lerp(b, s.min(1.0)), (b - a).normalize_or(Vec2::X))
            }
            None => {
                let r = &roads.roads[car.road];
                let dt_param = step / r.len;
                car.t += if car.forward { dt_param } else { -dt_param };
                if !(0.0..=1.0).contains(&car.t) {
                    // The end of the road: on to the next, across the junction.
                    let at = r.end(car.forward);
                    match roads.next(car.road, at, &mut rng) {
                        Some((next, forward)) => {
                            let start = roads.roads[next].end(!forward);
                            car.road = next;
                            car.forward = forward;
                            car.t = if forward { 0.0 } else { 1.0 };
                            if start.distance(at) > 0.5 {
                                car.crossing = Some((at, start, 0.0));
                            }
                        }
                        None => {
                            // A dead end: turn round.
                            car.forward = !car.forward;
                            car.t = car.t.clamp(0.0, 1.0);
                        }
                    }
                }
                let r = &roads.roads[car.road];
                let t = car.t.clamp(0.0, 1.0);
                let d = r.tangent(t) * if car.forward { 1.0 } else { -1.0 };
                (r.at(t), d)
            }
        };
        // Keep right.
        let right = Vec2::new(-dir.y, dir.x);
        let p = pos + right * LANE;
        let y = hm.sample(p.x, p.y) + 0.02;
        let ahead = p + dir * 1.5;
        let rise = hm.sample(ahead.x, ahead.y) - hm.sample(p.x - dir.x * 1.5, p.y - dir.y * 1.5);
        let fwd = Vec3::new(dir.x, rise / 3.0, dir.y).normalize_or(Vec3::Z);
        tf.translation = Vec3::new(p.x, y, p.y);
        tf.rotation = Transform::IDENTITY.looking_to(-fwd, Vec3::Y).rotation;
    }
    // New cars come along nearby, while the town's in view.
    if count >= MAX_CARS || cam.distance > 240.0 || !rng.random_bool(0.05) {
        return;
    }
    let near: Vec<usize> = (0..roads.roads.len()).filter(|&i| roads.roads[i].at(0.5).distance(focus) < NEAR).collect();
    if near.is_empty() {
        return;
    }
    let road = near[rng.random_range(0..near.len())];
    let names: Vec<_> = CARS.iter().filter_map(|n| data.0.catalog.iter().find(|c| c.instance_name == *n).map(|c| c.objd)).collect();
    if names.is_empty() {
        return;
    }
    let objd = names[rng.random_range(0..names.len())];
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let parts = assets.object(&mut ctx, objd);
    if parts.is_empty() {
        return;
    }
    let p = roads.roads[road].at(0.5);
    let e = spawn_parts(&mut commands, &parts, Transform::from_xyz(p.x, hm.sample(p.x, p.y), p.y));
    commands.entity(e).insert((
        Car { road, forward: rng.random_bool(0.5), t: rng.random_range(0.1..0.9), speed: SPEED * rng.random_range(0.85..1.15), crossing: None, hold: 0.0 },
        DespawnOnExit(AppState::InGame),
    ));
}
