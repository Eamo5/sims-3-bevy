//! Fireplaces: Sims light a fire (it burns a few hours, flames dancing in the hearth and a warm
//! flickering light on the room), warm their hands by it, and put it out. Now and then a spark
//! catches the floor in front. Candles likewise: one small flame, a soft light, no sparks.

use bevy::prelude::*;
use rand::Rng;

use crate::clock::GameClock;
use crate::interact::GameObject;
use crate::nav::Floor;
use crate::{AppState, PlayMode};

pub struct FireplacePlugin;

impl Plugin for FireplacePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FireplaceRequest>().add_systems(Update, (requests, burn, glow, cozy).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// A fire in the hearth, burning until then.
#[derive(Component)]
pub struct Lit {
    until: f64,
    flames: Entity,
    next_spark: f64,
}

/// Light or put out a fireplace's fire.
#[derive(Message)]
pub enum FireplaceRequest {
    Light(Entity),
    PutOut(Entity),
}

/// A fire poked up to raging (the game's `PokeFireSuccess`), till then.
#[derive(Component)]
pub struct Raging(pub f64);

/// The game's `PokeFireTime`: how long a poked fire rages (game minutes).
pub const RAGING_MINUTES: f64 = 20.0;
/// The game's `CozyFireBuffNormal`, `CozyFireBuffRaging` and `CozyFireBuffRange` (metres).
const COZY: i32 = 10;
const COZY_RAGING: i32 = 20;
const COZY_RANGE: f32 = 10.0;
/// How long Cozy lasts, kept up while near (the Flame Fruit's leaves it at an hour to go).
const COZY_MINUTES: f64 = 120.0;

/// How long a fire burns (game minutes), and a candle.
const BURN_MINUTES: f64 = 300.0;
const CANDLE_MINUTES: f64 = 240.0;

/// The hearth's flames and light.
#[derive(Component)]
struct HearthLight;

/// A candle's light.
#[derive(Component)]
struct CandleLight;

#[derive(Default)]
struct Looks {
    quad: Option<Handle<Mesh>>,
    mat: Option<Handle<StandardMaterial>>,
}

fn requests(
    mut commands: Commands,
    mut reqs: MessageReader<FireplaceRequest>,
    clock: Res<GameClock>,
    places: Query<(&GameObject, Option<&Lit>)>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut looks: Local<Looks>,
    ui: Option<Res<crate::icons::GameUi>>,
) {
    for r in reqs.read() {
        match *r {
            FireplaceRequest::Light(e) => {
                let Ok((o, lit)) = places.get(e) else { continue };
                if lit.is_some() {
                    continue;
                }
                let quad = looks.quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
                let mat = looks
                    .mat
                    .get_or_insert_with(|| {
                        mats.add(StandardMaterial {
                            base_color: Color::LinearRgba(LinearRgba::new(2.4, 1.6, 1.0, 1.0)),
                            base_color_texture: Some(images.add(crate::fire::flame_image())),
                            alpha_mode: AlphaMode::Blend,
                            unlit: true,
                            double_sided: true,
                            cull_mode: None,
                            ..default()
                        })
                    })
                    .clone();
                // The hearth: the fireplace's effect slot, else low down in the middle of the
                // firebox, a little in from the front. (A candle's wick: its slot, else its top.)
                let candle = o.kind == crate::interact::ObjectKind::Candle;
                // (A fire pit's fire in its bowl.)
                let pit = o.kind == crate::interact::ObjectKind::FirePit;
                let slot = ui.as_ref().and_then(|ui| ui.data.fx_slots.iter().find(|(k, _)| *k == o.objd)).and_then(|(_, s)| s.first().copied());
                let at = match (candle, slot) {
                    (true, Some(s)) => Vec3::from(s),
                    (true, None) => Vec3::new(o.center.x, o.height + 0.02, o.center.y),
                    // (Its effect slot can be down at its foot, under the logs: up on them then.)
                    (false, s) if pit => s.map(Vec3::from).filter(|s| s.y > 0.15).unwrap_or(Vec3::new(o.center.x, o.height * 0.55, o.center.y)),
                    (false, s) => s.map_or(Vec3::new(o.center.x, 0.08, o.center.y + o.half.y * 0.15), |s| Vec3::from(s) - Vec3::Y * 0.12),
                };
                let mut rng = rand::rng();
                if candle {
                    let flames = commands
                        .spawn((Transform::from_translation(at), Visibility::default(), DespawnOnExit(AppState::InGame), ChildOf(e)))
                        .with_children(|f| {
                            let size = Vec2::new(0.035, 0.07);
                            let offset = Vec3::Y * 0.03;
                            f.spawn((Mesh3d(quad.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(offset), crate::fire::Flame::new(size, rng.random_range(0.0..6.0), offset)));
                            f.spawn((
                                PointLight { color: Color::srgb(1.0, 0.65, 0.32), intensity: 2_500.0, range: 3.5, shadow_maps_enabled: false, ..default() },
                                Transform::from_xyz(0.0, 0.1, 0.0),
                                CandleLight,
                            ));
                        })
                        .id();
                    // (No sparks from a candle.)
                    commands.entity(e).insert(Lit { until: clock.minutes + CANDLE_MINUTES, flames, next_spark: f64::MAX });
                    continue;
                }
                let flames = commands
                    .spawn((Transform::from_translation(at), Visibility::default(), DespawnOnExit(AppState::InGame), ChildOf(e)))
                    .with_children(|f| {
                        for k in 0..5 {
                            let size = Vec2::new(rng.random_range(0.16..0.26), rng.random_range(0.28..0.5));
                            let offset = Vec3::new(rng.random_range(-0.22..0.22), 0.0, rng.random_range(-0.05..0.05));
                            f.spawn((Mesh3d(quad.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(offset), crate::fire::Flame::new(size, k as f32 * 1.9 + rng.random_range(0.0..3.0), offset)));
                        }
                        f.spawn((
                            PointLight { color: Color::srgb(1.0, 0.58, 0.25), intensity: 30_000.0, range: 7.0, shadow_maps_enabled: false, ..default() },
                            Transform::from_xyz(0.0, 0.4, if pit { 0.0 } else { 0.35 }),
                            HearthLight,
                        ));
                    })
                    .id();
                commands.entity(e).insert(Lit { until: clock.minutes + BURN_MINUTES, flames, next_spark: clock.minutes + 60.0 });
                debug!("the {} ({e:?}) is lit, flames at {at:?} (half {:?})", o.name, o.half);
            }
            FireplaceRequest::PutOut(e) => {
                if let Ok((_, Some(l))) = places.get(e) {
                    commands.entity(l.flames).try_despawn();
                    commands.entity(e).remove::<Lit>();
                }
            }
        }
    }
}

/// Fires burn down in time; once an hour a spark may catch the floor in front.
fn burn(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut lit: Query<(Entity, &mut Lit, &GameObject, &Transform, Option<&Floor>)>,
    mut fire: MessageWriter<crate::fire::StartFire>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let mut rng = rand::rng();
    for (e, mut l, o, tf, floor) in &mut lit {
        if clock.minutes >= l.until {
            commands.entity(l.flames).try_despawn();
            commands.entity(e).remove::<Lit>();
            continue;
        }
        if clock.minutes >= l.next_spark {
            l.next_spark = clock.minutes + 60.0;
            if rng.random_bool(0.015) {
                let at = tf.translation + tf.rotation * Vec3::new(o.center.x, 0.0, o.center.y + o.half.y + 0.9);
                fire.write(crate::fire::StartFire { at, level: floor.map_or(1, |f| f.0) });
                notes.push(format!("A spark from the {} caught the floor!", o.name));
            }
        }
    }
}

/// The hearth's light flickers.
fn glow(
    time: Res<Time>,
    mut lights: Query<(&mut PointLight, &ChildOf), With<HearthLight>>,
    mut candles: Query<&mut PointLight, (With<CandleLight>, Without<HearthLight>)>,
    raging: Query<&Lit, With<Raging>>,
) {
    let t = time.elapsed_secs();
    // (A poked fire roars brighter.)
    let roaring: Vec<Entity> = raging.iter().map(|l| l.flames).collect();
    for (mut l, parent) in &mut lights {
        let up = if roaring.contains(&parent.parent()) { 1.6 } else { 1.0 };
        l.intensity = up * (26_000.0 + 9_000.0 * ((t * 11.0).sin() * 0.5 + (t * 27.0).sin() * 0.5));
    }
    for mut l in &mut candles {
        l.intensity = 2_300.0 + 400.0 * ((t * 13.0).sin() * 0.5 + (t * 31.0).sin() * 0.5);
    }
}

/// A fire's warmth: those near it Cozy (the game's `BuffCozyFire` pulse: stronger while it
/// rages), and a poked fire back to burning once its time's up.
fn cozy(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    mut last: Local<f32>,
    fires: Query<(Entity, &GameObject, &GlobalTransform, Option<&Raging>), With<Lit>>,
    mut sims: Query<(&GlobalTransform, &mut crate::life::Moodlets), With<crate::sim::Sim>>,
) {
    let now = time.elapsed_secs();
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    for (e, o, tf, raging) in &fires {
        if o.kind == crate::interact::ObjectKind::Candle {
            continue;
        }
        if raging.is_some_and(|r| clock.minutes >= r.0) {
            commands.entity(e).remove::<Raging>();
        }
        let strength = if raging.is_some_and(|r| clock.minutes < r.0) { COZY_RAGING } else { COZY };
        let at = tf.translation();
        for (stf, mut m) in &mut sims {
            let p = stf.translation();
            if (p.y - at.y).abs() > 1.8 || p.xz().distance(at.xz()) > COZY_RANGE {
                continue;
            }
            m.add_valued(crate::life::MoodletKind::CozyFire, clock.minutes, strength, COZY_MINUTES);
        }
    }
}
