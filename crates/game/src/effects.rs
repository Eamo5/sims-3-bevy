//! Water effects at objects' effect slots (from their RSLT): fountains play all the time;
//! showers spray and sinks run while someone's using them. Droplets are small camera-facing
//! sprites thrown from the slot and falling under gravity.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rand::Rng;

use crate::camera::SimsCamera;
use crate::interact::{GameObject, ObjectKind, UsedBy};
use crate::{AppState, PlayMode};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach, fountain_water, emit, fall).chain().run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Spray {
    /// Up from the nozzle and back down into the basin.
    Fountain,
    /// Down from the shower head.
    Shower,
    /// A thin stream from the tap.
    Tap,
}

/// Where an object's water comes from (object space) and how.
#[derive(Component)]
struct Emitter {
    slot: Vec3,
    spray: Spray,
    owed: f32,
}

/// An object already looked at for effects.
#[derive(Component)]
struct FxChecked;

/// A fountain whose water surfaces (its untextured parts, drawn by the game's water shader)
/// are still to be made water.
#[derive(Component)]
struct FountainWater;

#[derive(Component)]
struct Droplet {
    vel: Vec3,
    age: f32,
    life: f32,
}

/// The droplets' looks, made once.
#[derive(Default)]
struct Looks {
    quad: Option<Handle<Mesh>>,
    mat: Option<Handle<StandardMaterial>>,
}

/// A soft round dot.
fn dot_image() -> Image {
    let n = 16u32;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = ((x as f32 + 0.5) / n as f32 * 2.0 - 1.0, (y as f32 + 0.5) / n as f32 * 2.0 - 1.0);
            let a = (1.0 - (u * u + v * v).sqrt()).clamp(0.0, 1.0).powf(1.5);
            data.extend_from_slice(&[235, 245, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// Gives fountains, showers and sinks an emitter at their effect slot.
fn attach(
    mut commands: Commands,
    objects: Query<(Entity, &GameObject), Without<FxChecked>>,
    ui: Option<Res<crate::icons::GameUi>>,
    data: Res<crate::baked::Baked>,
    mut slots: Local<Option<HashMap<s3bake::Key, Vec<[f32; 3]>>>>,
) {
    let Some(ui) = ui else { return };
    let slots = slots.get_or_insert_with(|| ui.data.fx_slots.iter().cloned().collect());
    for (e, o) in &objects {
        commands.entity(e).insert(FxChecked);
        let script = data.0.catalog_entry(&o.objd).map_or("", |c| c.script.as_str());
        let spray = match o.kind {
            ObjectKind::Shower => Spray::Shower,
            ObjectKind::Sink => Spray::Tap,
            _ if script.contains(".Environment.Fountain") && !script.contains("FountainJet") => Spray::Fountain,
            _ => continue,
        };
        if spray == Spray::Fountain {
            commands.entity(e).insert(FountainWater);
        }
        // The highest slot: the shower head, the tap, the fountain's top.
        let Some(slot) = slots.get(&o.objd).and_then(|s| s.iter().max_by(|a, b| a[1].total_cmp(&b[1])).copied()) else { continue };
        if spray != Spray::Tap && slot[1] < 0.3 {
            continue;
        }
        commands.entity(e).insert(Emitter { slot: Vec3::from(slot), spray, owed: 0.0 });
    }
}

/// A fountain's water surfaces: glossy, see-through blue-green instead of the plain grey of an
/// untextured part.
fn fountain_water(
    mut commands: Commands,
    q: Query<(Entity, &Children), With<FountainWater>>,
    parts: Query<&MeshMaterial3d<StandardMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut water: Local<Option<Handle<StandardMaterial>>>,
) {
    for (e, children) in &q {
        let w = water
            .get_or_insert_with(|| {
                mats.add(StandardMaterial {
                    base_color: Color::srgba(0.32, 0.6, 0.66, 0.82),
                    perceptual_roughness: 0.04,
                    reflectance: 0.6,
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                })
            })
            .clone();
        for c in children {
            let Ok(m) = parts.get(*c) else { continue };
            if mats.get(&m.0).is_some_and(|m| m.base_color_texture.is_none() && m.alpha_mode == AlphaMode::Opaque) {
                commands.entity(*c).insert(MeshMaterial3d(w.clone()));
            }
        }
        commands.entity(e).remove::<FountainWater>();
    }
}

/// Fountains play; showers and taps run while someone's at them.
#[allow(clippy::type_complexity)]
fn emit(
    mut commands: Commands,
    time: Res<Time>,
    mut emitters: Query<(&mut Emitter, &GlobalTransform, &UsedBy, &InheritedVisibility)>,
    users: Query<&GlobalTransform, With<crate::sim::Sim>>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut looks: Local<Looks>,
) {
    let dt = time.delta_secs().min(0.1);
    let mut rng = rand::rng();
    for (mut em, gt, used, vis) in &mut emitters {
        if !vis.get() {
            continue;
        }
        let on = match em.spray {
            Spray::Fountain => true,
            // (Someone at it, not just on their way.)
            _ => used.0.and_then(|u| users.get(u).ok()).is_some_and(|u| u.translation().xz().distance(gt.translation().xz()) < 1.3),
        };
        if !on {
            em.owed = 0.0;
            continue;
        }
        if em.owed == 0.0 && em.spray != Spray::Fountain {
            debug!("{:?} running at {:?}", em.spray, gt.transform_point(em.slot));
        }
        let rate = match em.spray {
            Spray::Fountain => 170.0,
            Spray::Shower => 110.0,
            Spray::Tap => 40.0,
        };
        em.owed += rate * dt;
        let quad = looks.quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
        let mat = looks
            .mat
            .get_or_insert_with(|| {
                mats.add(StandardMaterial {
                    base_color: Color::srgba(0.88, 0.95, 1.0, 0.85),
                    base_color_texture: Some(images.add(dot_image())),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .clone();
        let origin = gt.transform_point(em.slot);
        while em.owed >= 1.0 {
            em.owed -= 1.0;
            let a = rng.random_range(0.0..std::f32::consts::TAU);
            let (dir, speed, life, size, jitter) = match em.spray {
                // A crown of jets thrown up and out.
                Spray::Fountain => {
                    let out = rng.random_range(0.08..0.4);
                    (Vec3::new(a.cos() * out, 1.0, a.sin() * out), rng.random_range(3.0..4.2), rng.random_range(0.9..1.2), 0.09, 0.05)
                }
                Spray::Shower => {
                    let out = rng.random_range(0.0..0.22);
                    (Vec3::new(a.cos() * out, -1.0, a.sin() * out), rng.random_range(1.2..2.0), 0.85, 0.035, 0.06)
                }
                Spray::Tap => (Vec3::new(0.0, -1.0, 0.0), 0.4, 0.3, 0.025, 0.005),
            };
            let at = origin + Vec3::new(rng.random_range(-jitter..jitter), 0.0, rng.random_range(-jitter..jitter));
            commands.spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(at).with_scale(Vec3::splat(size)),
                Droplet { vel: dir.normalize() * speed, age: 0.0, life },
                NotShadowCaster,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
}

/// Droplets fall, face the camera and are gone at the end of their flight.
fn fall(
    mut commands: Commands,
    time: Res<Time>,
    cams: Query<&GlobalTransform, With<SimsCamera>>,
    mut drops: Query<(Entity, &mut Droplet, &mut Transform)>,
) {
    let dt = time.delta_secs().min(0.1);
    let cam = cams.single().ok().map(|c| c.translation());
    for (e, mut d, mut tf) in &mut drops {
        d.age += dt;
        if d.age >= d.life {
            commands.entity(e).despawn();
            continue;
        }
        d.vel.y -= 9.8 * dt;
        tf.translation += d.vel * dt;
        if let Some(c) = cam {
            tf.rotation = Transform::from_translation(tf.translation).looking_at(c, Vec3::Y).rotation;
        }
    }
}
