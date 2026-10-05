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
        app.add_systems(Update, (attach, fountain_water, emit, fall, tv_screens).chain().run_if(in_state(PlayMode::Live)));
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

/// A TV's screen: where it is, and the picture on it while it's on.
#[derive(Component)]
struct TvScreen {
    slot: Vec3,
    width: f32,
    shown: Option<Entity>,
}

/// A TV that's on.
#[derive(Component)]
pub struct TvOn;

/// A TV picture: its frames' materials, and when it next changes.
#[derive(Component)]
struct TvPicture {
    next: f32,
    light: Entity,
}

/// Frames of programmes: a sky and ground, a few bright shapes (people, cars, cartoons).
fn tv_frames(images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Vec<Handle<StandardMaterial>> {
    let mut rng = rand::rng();
    (0..10)
        .map(|_| {
            let (w, h) = (48u32, 27u32);
            let hue = rng.random_range(0.0..360.0);
            let sky = Color::hsl(hue, 0.5, 0.55).to_srgba();
            let ground = Color::hsl((hue + 120.0) % 360.0, 0.45, 0.35).to_srgba();
            let horizon = rng.random_range(10..20);
            let shapes: Vec<(u32, u32, u32, Srgba)> =
                (0..rng.random_range(2..6)).map(|_| (rng.random_range(0..w), rng.random_range(4..h), rng.random_range(2..7), Color::hsl(rng.random_range(0.0..360.0), 0.8, 0.6).to_srgba())).collect();
            let mut data = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                for x in 0..w {
                    let mut c = if y < horizon { sky } else { ground };
                    for &(sx, sy, r, sc) in &shapes {
                        let (dx, dy) = (x as i32 - sx as i32, y as i32 - sy as i32);
                        if dx * dx + dy * dy <= (r * r) as i32 {
                            c = sc;
                        }
                    }
                    // Scanlines.
                    let k = if y % 2 == 0 { 1.0 } else { 0.82 };
                    data.extend_from_slice(&[(c.red * 255.0 * k) as u8, (c.green * 255.0 * k) as u8, (c.blue * 255.0 * k) as u8, 255]);
                }
            }
            let img = images.add(Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD));
            mats.add(StandardMaterial { base_color: Color::BLACK, emissive: LinearRgba::rgb(1.6, 1.6, 1.6), emissive_texture: Some(img), unlit: false, ..default() })
        })
        .collect()
}

/// TVs show a programme while someone's watching: the picture changes now and then, and its
/// light flickers on the room.
#[allow(clippy::type_complexity)]
fn tv_screens(
    mut commands: Commands,
    time: Res<Time>,
    mut tvs: Query<(Entity, &mut TvScreen, &UsedBy)>,
    watchers: Query<&crate::interact::ActionQueue>,
    mut pictures: Query<(&mut TvPicture, &mut MeshMaterial3d<StandardMaterial>)>,
    mut lights: Query<&mut PointLight>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut frames: Local<Vec<Handle<StandardMaterial>>>,
    mut quad: Local<Option<Handle<Mesh>>>,
) {
    let now = time.elapsed_secs();
    let mut rng = rand::rng();
    // (Several can watch at once without any of them having the set to themselves.)
    let watched: std::collections::HashSet<Entity> = watchers
        .iter()
        .filter_map(|q| match q.0.front().map(|a| (&a.kind, a.phase)) {
            Some((crate::interact::ActionKind::Object { target, .. }, crate::interact::Phase::Running(_))) => Some(*target),
            _ => None,
        })
        .collect();
    for (e, mut tv, used) in &mut tvs {
        match (used.0.is_some() || watched.contains(&e), tv.shown) {
            (true, None) => {
                if frames.is_empty() {
                    *frames = tv_frames(&mut images, &mut mats);
                }
                let q = quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
                let (w, h) = (tv.width, tv.width * 0.56);
                let light = commands
                    .spawn((PointLight { color: Color::srgb(0.7, 0.8, 1.0), intensity: 4000.0, range: 4.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 0.0, 0.6)))
                    .id();
                let pic = commands
                    .spawn((
                        Mesh3d(q),
                        MeshMaterial3d(frames[rng.random_range(0..frames.len())].clone()),
                        Transform::from_translation(tv.slot + Vec3::Z * 0.035).with_scale(Vec3::new(w, h, 1.0)),
                        TvPicture { next: now + rng.random_range(0.5..2.0), light },
                        NotShadowCaster,
                        ChildOf(e),
                    ))
                    .id();
                commands.entity(light).insert(ChildOf(pic));
                tv.shown = Some(pic);
                commands.entity(e).insert(TvOn);
            }
            (false, Some(pic)) => {
                commands.entity(pic).try_despawn();
                commands.entity(e).remove::<TvOn>();
                tv.shown = None;
            }
            (true, Some(pic)) => {
                if let Ok((mut p, mut m)) = pictures.get_mut(pic)
                    && now >= p.next
                    && !frames.is_empty()
                {
                    p.next = now + rng.random_range(0.4..2.5);
                    m.0 = frames[rng.random_range(0..frames.len())].clone();
                    if let Ok(mut l) = lights.get_mut(p.light) {
                        l.intensity = rng.random_range(2500.0..6000.0);
                    }
                }
            }
            (false, None) => {}
        }
    }
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
        // A TV's screen: the effect slot in its middle, up off the floor.
        if o.kind == ObjectKind::Tv {
            let screen = slots.get(&o.objd).and_then(|s| s.iter().filter(|p| p[1] > 0.3 && p[0].abs() < 0.2).max_by(|a, b| a[1].total_cmp(&b[1])).copied());
            if let Some(s) = screen {
                commands.entity(e).insert(TvScreen { slot: Vec3::from(s), width: (o.half.x * 2.0 * 0.75).clamp(0.35, 1.4), shown: None });
            }
            continue;
        }
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
