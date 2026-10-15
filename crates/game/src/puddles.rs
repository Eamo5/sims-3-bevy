//! Puddles, as the game's `PuddleManager`: water on the floor, a tile each. A broken shower,
//! tub, sink or toilet spills a new one every quarter hour (four at most, its
//! `FlowingPuddleTuning`), and a Sim who couldn't make it to the bathroom leaves one. A puddle
//! where another already is, or under furniture, runs to the nearest free tile. They dry up in
//! time (two hours outdoors, eight on a floor) or are mopped up (the game's mop clips, five
//! minutes): Neat Sims see to it on their own.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rand::Rng;

use crate::clock::GameClock;
use crate::interact::{Broken, GameObject, ObjectKind, UsedBy};
use crate::nav::Floor;
use crate::{AppState, PlayMode};

pub struct PuddlesPlugin;

impl Plugin for PuddlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AddPuddle>()
            .init_resource::<PendingPuddles>()
            .add_systems(Update, (puddle_test, restore, spill, add_puddles, dry_up).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// PUDDLE_AT=<x>,<z>,<storey> (tests): a puddle spilled there (world metres) a few seconds in.
fn puddle_test(mut done: Local<bool>, time: Res<Time>, world: Res<crate::loading::CurrentWorld>, mut add: MessageWriter<AddPuddle>) {
    let Ok(v) = std::env::var("PUDDLE_AT") else { return };
    if *done || time.elapsed_secs() < 8.0 {
        return;
    }
    *done = true;
    let n: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let [x, z, level] = n[..] else { return };
    let y = world.data.heightmap.sample(x, z);
    info!("puddle test: one spilled at {x},{z} on floor {level}");
    add.write(AddPuddle { at: Vec3::new(x, y, z), level: level as u8, lasts: None });
}

/// Water on a floor tile, drying by `until` (game minutes).
#[derive(Component)]
pub struct Puddle {
    pub until: f64,
}

/// A puddle wanted where something spilled (it runs to a free tile nearby).
#[derive(Message, Clone, Copy)]
pub struct AddPuddle {
    pub at: Vec3,
    pub level: u8,
    /// How long it's left (game minutes; none: as long as a puddle there lasts).
    pub lasts: Option<f64>,
}

/// A broken fixture spilling water: when the next puddle comes, and those it's made.
#[derive(Component, Default)]
struct Spilling {
    next: f64,
    made: Vec<Entity>,
}

/// The game's tuning: a puddle every quarter hour, four at most; dry on the ground after two
/// hours, on a floor after eight; how far one runs looking for room.
const SPILL_MINUTES: f64 = 15.0;
const SPILL_MAX: usize = 4;
const GROUND_LIFETIME: f64 = 120.0;
const FLOOR_LIFETIME: f64 = 480.0;
const MAX_RADIUS: i32 = 15;

/// What spills when it's broken.
pub(crate) fn spills(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Shower | ObjectKind::Bathtub | ObjectKind::ShowerTub | ObjectKind::Sink | ObjectKind::Toilet | ObjectKind::Dishwasher)
}

/// Broken plumbing spills a puddle every quarter hour, while there are fewer than four of its own.
fn spill(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut fixtures: Query<(Entity, &GameObject, &Transform, Option<&Floor>, Has<Broken>, Option<&mut Spilling>)>,
    puddles: Query<(), With<Puddle>>,
    mut add: MessageWriter<AddPuddle>,
) {
    for (e, o, tf, floor, broken, spilling) in &mut fixtures {
        if !spills(o.kind) {
            continue;
        }
        match (broken, spilling) {
            (true, None) => {
                commands.entity(e).insert(Spilling { next: clock.minutes + 1.0, made: Vec::new() });
            }
            (true, Some(mut s)) => {
                s.made.retain(|p| puddles.contains(*p));
                if clock.minutes >= s.next {
                    s.next = clock.minutes + SPILL_MINUTES;
                    if s.made.len() < SPILL_MAX {
                        add.write(AddPuddle { at: o.world_center(tf), level: floor.map_or(1, |f| f.0), lasts: None });
                    }
                }
            }
            (false, Some(_)) => {
                commands.entity(e).remove::<Spilling>();
            }
            (false, None) => {}
        }
    }
}

/// The puddles' look: a pool of water (soft-edged, a few shapes), glossy over the floor.
#[derive(Default)]
struct Looks {
    quad: Option<Handle<Mesh>>,
    mats: Vec<Handle<StandardMaterial>>,
}

/// A puddle's shape: a blob with a wandering edge (the alpha), seeded.
fn puddle_image(seed: u32) -> Image {
    let n = 64u32;
    let hash = |x: i32, y: i32| {
        let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        (h ^ (h >> 16)) as f32 / u32::MAX as f32
    };
    // (Smooth noise: a few octaves of interpolated lattice values.)
    let noise = |u: f32, v: f32| {
        let mut sum = 0.0;
        let mut amp = 0.5;
        let mut f = 3.0;
        for _ in 0..3 {
            let (x, y) = (u * f, v * f);
            let (xi, yi) = (x.floor() as i32, y.floor() as i32);
            let (tx, ty) = (x - x.floor(), y - y.floor());
            let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
            let a = hash(xi, yi) + (hash(xi + 1, yi) - hash(xi, yi)) * sx;
            let b = hash(xi, yi + 1) + (hash(xi + 1, yi + 1) - hash(xi, yi + 1)) * sx;
            sum += (a + (b - a) * sy) * amp;
            amp *= 0.5;
            f *= 2.0;
        }
        sum / 0.875
    };
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = ((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32);
            let r = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
            // (Wetter in the middle, the edge wandering in and out.)
            let edge = 0.62 + 0.38 * noise(u, v);
            let a = ((edge - r) / 0.12).clamp(0.0, 1.0);
            // (A brighter rim where the water's edge catches the light.)
            let rim = (1.0 - ((edge - r) / 0.16).clamp(0.0, 1.0)) * a;
            let depth = ((0.55 + 0.15 * noise(v + 3.1, u + 1.7)) * a + 0.35 * rim).min(1.0);
            let c = (225.0 + 30.0 * rim) as u8;
            data.extend_from_slice(&[c, c.saturating_add(10), 255, (depth * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// Where a puddle can lie: the floor's free there (nothing standing on it), on that storey, and
/// open to the rest of it (not a pocket behind the tub, where whoever came to mop it would be
/// stranded).
pub(crate) fn free_at(grid: Option<&crate::nav::NavGrid>, upper: Option<&crate::nav::UpperFloors>, level: u8, p: Vec2) -> bool {
    let g = if level == 1 { grid } else { upper.and_then(|u| u.grids.get(&level)) };
    g.is_some_and(|g| {
        g.cell_of(p).is_some_and(|(x, z)| !g.is_blocked(x, z) && g.reachable_from(p).iter().filter(|r| **r).count() >= OPEN_CELLS)
    })
}

/// How many walkable cells a puddle's spot must be joined to (a room's worth, not a pocket).
const OPEN_CELLS: usize = 40;

/// Puts each spilled puddle on its tile, or the nearest free one about it.
#[allow(clippy::too_many_arguments)]
fn add_puddles(
    mut commands: Commands,
    mut asked: MessageReader<AddPuddle>,
    clock: Res<GameClock>,
    world: Res<crate::loading::CurrentWorld>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    (grid, upper): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::nav::UpperFloors>>),
    (mut meshes, mut mats, mut images): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>, ResMut<Assets<Image>>),
    mut looks: Local<Looks>,
    puddles: Query<(&Transform, &Floor), With<Puddle>>,
) {
    let mut placed: Vec<(Vec2, u8)> = puddles.iter().map(|(tf, f)| (tf.translation.xz(), f.0)).collect();
    let mut rng = rand::rng();
    for ask in asked.read() {
        // (Tiles are the lot's: whole metres from its corner, turned with it.)
        let (to_local, to_world): (Box<dyn Fn(Vec3) -> Vec2>, Box<dyn Fn(Vec2) -> Vec2>) = match building.as_deref() {
            Some(b) => (Box::new(move |p| b.local(p)), Box::new(move |l| b.world(l.x, l.y, 0.0).xz())),
            None => (Box::new(|p: Vec3| p.xz()), Box::new(|l| l)),
        };
        let l = to_local(ask.at);
        let (tx, tz) = (l.x.floor() as i32, l.y.floor() as i32);
        let tile = |dx: i32, dz: i32| to_world(Vec2::new((tx + dx) as f32 + 0.5, (tz + dz) as f32 + 0.5));
        let taken = |p: Vec2| placed.iter().any(|(q, lv)| *lv == ask.level && q.distance(p) < 0.5);
        // (Where the house can be walked to from its front door: not a pocket of an upper
        // storey that's open floor enough but joined to no stairs, where no one could mop it.)
        let entry = building.as_deref().and_then(|b| b.front_door(ask.at.xz())).map(|d| d.0);
        let walked_to = |p: Vec2| entry.is_none_or(|e| grid.as_deref().is_some_and(|g| crate::nav::plan_route(g, upper.as_deref(), e, 1, p, ask.level).is_some()));
        // (Ring by ring outwards: the tile itself first.)
        let Some(at) = (0..=MAX_RADIUS)
            .flat_map(|r| (-r..=r).flat_map(move |dx| (-r..=r).filter(move |dz| dx.abs().max(dz.abs()) == r).map(move |dz| (dx, dz))))
            .map(|(dx, dz)| tile(dx, dz))
            .find(|p| !taken(*p) && free_at(grid.as_deref(), upper.as_deref(), ask.level, *p) && walked_to(*p))
        else {
            debug!("no room for a puddle about {:?} on floor {}", ask.at, ask.level);
            continue;
        };
        debug!("a puddle at {at:?} on floor {} (spilled at {:?})", ask.level, ask.at);
        let y = crate::nav::floor_height(&world.data, building.as_deref(), ask.level, Vec3::new(at.x, ask.at.y, at.y));
        let indoors = ask.level > 1 || building.as_deref().is_some_and(|b| b.floor_y(ask.level, Vec3::new(at.x, y, at.y)).is_some());
        let lasts = ask.lasts.unwrap_or(if indoors { FLOOR_LIFETIME } else { GROUND_LIFETIME });
        let quad = looks.quad.get_or_insert_with(|| meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(0.5)))).clone();
        if looks.mats.is_empty() {
            for seed in 0..4 {
                let img = images.add(puddle_image(seed * 7919 + 13));
                looks.mats.push(mats.add(StandardMaterial {
                    base_color: Color::srgba(0.8, 0.86, 0.9, 0.75),
                    base_color_texture: Some(img),
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 0.04,
                    reflectance: 1.0,
                    ..default()
                }));
            }
        }
        let mat = looks.mats[rng.random_range(0..looks.mats.len())].clone();
        let turn = building.as_deref().map_or(Quat::IDENTITY, |b| b.rot) * Quat::from_rotation_y(rng.random_range(0..4) as f32 * std::f32::consts::FRAC_PI_2);
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(mat),
            Transform::from_xyz(at.x, y + 0.012, at.y).with_rotation(turn).with_scale(Vec3::splat(rng.random_range(0.85..1.05))),
            GameObject {
                kind: ObjectKind::Puddle,
                name: "Puddle".into(),
                objd: (0, 0, 0),
                price: 0,
                center: Vec2::ZERO,
                half: Vec2::splat(0.4),
                height: 0.02,
                // (Mopped standing on it.)
                route: Some((Vec2::ZERO, Vec2::Y)),
            },
            UsedBy::default(),
            Floor(ask.level),
            Puddle { until: clock.minutes + lasts },
            NotShadowCaster,
            DespawnOnExit(AppState::InGame),
        ));
        placed.push((at, ask.level));
    }
}

/// Puddles dry up in time (not while they're being mopped).
fn dry_up(mut commands: Commands, clock: Res<GameClock>, puddles: Query<(Entity, &Puddle, &UsedBy)>) {
    for (e, p, used) in &puddles {
        if clock.minutes >= p.until && used.0.is_none() {
            commands.entity(e).try_despawn();
        }
    }
}

/// A saved puddle: where, on which storey, and how long it has left.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SavedPuddle {
    pub at: [f32; 3],
    pub level: u8,
    pub left: f64,
}

/// A loaded game's puddles, put back once the floors' walk grids are made.
#[derive(Resource, Default)]
pub struct PendingPuddles(pub Vec<SavedPuddle>);

fn restore(mut pending: ResMut<PendingPuddles>, grid: Option<Res<crate::nav::NavGrid>>, upper: Option<Res<crate::nav::UpperFloors>>, mut add: MessageWriter<AddPuddle>) {
    if pending.0.is_empty() || grid.is_none() || upper.as_deref().is_none_or(|u| !u.ready()) {
        return;
    }
    for p in pending.0.drain(..) {
        add.write(AddPuddle { at: Vec3::from(p.at), level: p.level, lasts: Some(p.left) });
    }
}

pub fn saved(clock: &GameClock, puddles: &Query<(&Transform, &Floor, &Puddle)>) -> Vec<SavedPuddle> {
    puddles.iter().map(|(tf, f, p)| SavedPuddle { at: tf.translation.to_array(), level: f.0, left: (p.until - clock.minutes).max(1.0) }).collect()
}
