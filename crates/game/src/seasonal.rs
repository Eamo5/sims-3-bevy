//! Seasons' outdoor fun, with the pack's own objects and animations: in the snow, Sims build a
//! snowman (rolling and patting its three balls, then giving it its face: it grows as they go)
//! and lie down to make snow angels, both melting away with the snow; out in snow or rain they
//! catch flakes or drops on their tongues; and through fall leaf piles gather on the home lot
//! (every ten to twelve hours, `kHoursLeavesGatherOnGround`) to play in, jump into and rake up.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::clock::GameClock;
use crate::interact::GameObject;
use crate::weather::{Season, Weather, WeatherKind};

pub struct SeasonalPlugin;

impl Plugin for SeasonalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (outdoor_events, grow, melt, leaf_piles).run_if(in_state(PlayMode::Live)));
    }
}

/// Something done out of doors, at a spot on the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outdoor {
    Snowman,
    SnowAngel,
    CatchSnow,
    CatchRain,
}

/// Snow deep enough to build in (`kSnowLevelCutOffs`: low snow).
pub const SNOW_TO_PLAY: f32 = 15.0;

impl Outdoor {
    pub fn label(self) -> &'static str {
        match self {
            Outdoor::Snowman => "Build Snowman",
            Outdoor::SnowAngel => "Make Snow Angel",
            Outdoor::CatchSnow => "Catch Snowflakes",
            Outdoor::CatchRain => "Catch Raindrops",
        }
    }

    /// How long it takes (game minutes): a snowman its whole animation, rolled and patted ball
    /// by ball.
    pub fn minutes(self) -> f32 {
        match self {
            Outdoor::Snowman => 54.0,
            Outdoor::SnowAngel => 16.0,
            Outdoor::CatchSnow | Outdoor::CatchRain => 12.0,
        }
    }

    /// Fun (an hour).
    pub fn fun(self) -> f32 {
        match self {
            Outdoor::Snowman => 70.0,
            Outdoor::SnowAngel => 90.0,
            Outdoor::CatchSnow | Outdoor::CatchRain => 80.0,
        }
    }

    /// Where the Sim stands for it: beside the snowman they're building, on the spot otherwise.
    pub fn stand(self, at: Vec2) -> Vec2 {
        match self {
            Outdoor::Snowman => at + Vec2::new(0.0, 0.9),
            _ => at,
        }
    }

    pub fn clip(self) -> ActionClip {
        match self {
            Outdoor::Snowman => ActionClip::steps(
                "a2o_snowman_build_start_x",
                &[
                    "a2o_snowman_build_loopFormBottom_x",
                    "a2o_snowman_build_loopPatBottom_x",
                    "a2o_snowman_build_loopFormMiddle_x",
                    "a2o_snowman_build_loopPatMiddle_x",
                    "a2o_snowman_build_loopFormTop_x",
                    "a2o_snowman_build_loopPatTop_x",
                ],
                &["a2o_snowman_build_placeAccessories_x"],
            )
            .ending(&["a2o_snowman_build_stop_x"], 0.8),
            Outdoor::SnowAngel => ActionClip::new(Some("a2o_snowAngel_makeBack_start_x"), &["a2o_snowAngel_makeBack_loop_x"]).ending(&["a2o_snowAngel_makeBack_stop_x"], 4.6),
            Outdoor::CatchSnow => ActionClip::new(None, &["a_idle_catchSnowTongue_x", "a_idle_catchSnowHand_x"]),
            Outdoor::CatchRain => ActionClip::new(None, &["a_idle_catchRainTongue_x", "a_idle_catchRainHand_x"]),
        }
    }

    /// What can be done out here now: in the snow, a snowman and a snow angel; in snow or rain,
    /// catching it.
    pub fn options(w: &Weather, minutes: f64) -> Vec<Outdoor> {
        let mut v = Vec::new();
        if w.snow >= SNOW_TO_PLAY {
            v.extend([Outdoor::Snowman, Outdoor::SnowAngel]);
        }
        if w.falling(minutes) > 0.1 {
            v.push(if w.kind == WeatherKind::Snow { Outdoor::CatchSnow } else { Outdoor::CatchRain });
        }
        v
    }
}

/// What a Sim began or finished out of doors (from the actions), for the snowman or snow angel.
#[derive(Component, Clone, Copy)]
pub enum OutdoorEvent {
    Begun { what: Outdoor, at: Vec3, minutes: f32 },
    Done { what: Outdoor, at: Vec3, facing: Quat, child: bool },
}

/// Made of snow: melts away with it.
#[derive(Component)]
pub struct SnowMade;

/// A snowman still being built: grows to full size by then.
#[derive(Component)]
struct Growing {
    since: f64,
    minutes: f32,
}

/// A leaf pile of the fall.
#[derive(Component)]
pub struct LeafPile;

/// A catalogue object by the game's name for it.
fn catalog_key(data: &crate::baked::Baked, name: &str) -> Option<s3bake::Key> {
    data.0.catalog.iter().find(|c| c.instance_name == name).map(|c| c.objd)
}

#[allow(clippy::too_many_arguments)]
fn outdoor_events(
    mut commands: Commands,
    clock: Res<GameClock>,
    sims: Query<(Entity, &OutdoorEvent)>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    for (sim, ev) in &sims {
        commands.entity(sim).remove::<OutdoorEvent>();
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        match *ev {
            OutdoorEvent::Begun { what: Outdoor::Snowman, at, minutes } => {
                let Some(key) = catalog_key(&data, "snowMan") else { continue };
                if let Some(s) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, at, std::f32::consts::PI) {
                    commands.entity(s.entity).insert((SnowMade, Growing { since: clock.minutes, minutes }));
                }
            }
            OutdoorEvent::Done { what: Outdoor::SnowAngel, at, facing, child } => {
                let Some(key) = catalog_key(&data, if child { "snowAngelChild" } else { "snowAngelAdult" }) else { continue };
                let yaw = facing.to_euler(EulerRot::YXZ).0;
                if let Some(s) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, at, yaw) {
                    commands.entity(s.entity).insert(SnowMade);
                }
            }
            _ => {}
        }
    }
}

/// A snowman grows as it's built (its balls rolled one by one).
fn grow(mut commands: Commands, clock: Res<GameClock>, mut q: Query<(Entity, &Growing, &mut Transform)>) {
    for (e, g, mut tf) in &mut q {
        let p = ((clock.minutes - g.since) as f32 / g.minutes.max(1.0)).clamp(0.0, 1.0);
        let s = 0.25 + 0.75 * (p / 0.85).min(1.0).powf(0.8);
        tf.scale = Vec3::splat(s);
        if p >= 1.0 {
            tf.scale = Vec3::ONE;
            commands.entity(e).remove::<Growing>();
        }
    }
}

/// Snowmen and snow angels melt away when the snow does.
fn melt(mut commands: Commands, w: Res<Weather>, q: Query<Entity, With<SnowMade>>) {
    if w.snow >= 5.0 {
        return;
    }
    for e in &q {
        commands.entity(e).try_despawn();
    }
}

/// Leaves gather in piles on the home lot through fall (and are gone come winter).
#[allow(clippy::too_many_arguments)]
fn leaf_piles(
    mut commands: Commands,
    clock: Res<GameClock>,
    w: Res<Weather>,
    household: Option<Res<crate::interact::Household>>,
    world: Res<crate::loading::CurrentWorld>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    piles: Query<Entity, With<LeafPile>>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut next: Local<f64>,
) {
    if w.season(clock.day()) != Season::Fall || w.temperature.is_nan() {
        for e in &piles {
            commands.entity(e).try_despawn();
        }
        return;
    }
    let mut rng = rand::rng();
    if *next == 0.0 {
        // (LEAVES=1, for tests: one straight away.)
        *next = clock.minutes + if std::env::var("LEAVES").is_ok() { 0.0 } else { rng.random_range(1.0..4.0) * 60.0 };
    }
    if clock.minutes < *next || piles.iter().count() >= 4 {
        return;
    }
    *next = clock.minutes + rng.random_range(10.0..12.0) * 60.0;
    let Some(lot) = household.as_ref().and_then(|h| world.data.lots.get(h.lot_index)) else { return };
    let Some(key) = catalog_key(&data, "pileLeaves") else { return };
    // (Somewhere out in the yard.)
    for _ in 0..12 {
        let (x, z) = (rng.random_range(1.0..(lot.width as f32 - 1.0).max(1.5)), rng.random_range(1.0..(lot.depth as f32 - 1.0).max(1.5)));
        let (s, c) = lot.rotation.sin_cos();
        let p = Vec3::from(lot.corner) + Vec3::new(x * c + z * s, 0.0, -x * s + z * c);
        let p = Vec3::new(p.x, world.data.heightmap.sample(p.x, p.z), p.z);
        if building.as_deref().is_some_and(|b| b.floor_y(1, p).is_some()) {
            continue;
        }
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        if let Some(s) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, rng.random_range(0.0..6.28)) {
            commands.entity(s.entity).insert(LeafPile);
        }
        break;
    }
}

/// Whether this is something made of snow or leaves (no buying or selling it).
pub fn seasonal_object(o: &GameObject) -> bool {
    matches!(o.kind, crate::interact::ObjectKind::Snowman | crate::interact::ObjectKind::SnowAngel | crate::interact::ObjectKind::LeafPile)
}
