//! Fireplaces: Sims light a fire (it burns a few hours, flames dancing in the hearth and a warm
//! flickering light on the room), warm their hands by it, and put it out. Now and then a spark
//! catches the floor in front.

use bevy::prelude::*;
use rand::Rng;

use crate::clock::GameClock;
use crate::interact::GameObject;
use crate::nav::Floor;
use crate::{AppState, PlayMode};

pub struct FireplacePlugin;

impl Plugin for FireplacePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FireplaceRequest>().add_systems(Update, (requests, burn, glow).chain().run_if(in_state(PlayMode::Live)));
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

/// How long a fire burns (game minutes).
const BURN_MINUTES: f64 = 300.0;

/// The hearth's flames and light.
#[derive(Component)]
struct HearthLight;

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
                // The hearth: low down in the middle of the firebox, a little in from the front.
                let at = Vec3::new(o.center.x, 0.08, o.center.y + o.half.y * 0.15);
                let mut rng = rand::rng();
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
                            Transform::from_xyz(0.0, 0.4, 0.35),
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
fn glow(time: Res<Time>, mut lights: Query<&mut PointLight, With<HearthLight>>) {
    let t = time.elapsed_secs();
    for mut l in &mut lights {
        l.intensity = 26_000.0 + 9_000.0 * ((t * 11.0).sin() * 0.5 + (t * 27.0).sin() * 0.5);
    }
}
