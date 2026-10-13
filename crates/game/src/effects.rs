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
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Special, UsedBy, interactions_for};
use crate::{AppState, PlayMode};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaterLooks>()
            .add_message::<SpinSparkle>()
            .add_systems(Update, (attach, fountain_water, bubble_baths, sprinkler_spray, emit, fall, tv_screens, censor, sparkle).chain().run_if(in_state(PlayMode::Live)));
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
    /// Arcs thrown out round a garden sprinkler, while it's on.
    Sprinkler,
}

/// Where an object's water comes from (object space) and how.
#[derive(Component)]
struct Emitter {
    slot: Vec3,
    spray: Spray,
    owed: f32,
    /// Who the shower's running for (its water's sound, and their singing, theirs).
    sounding: Option<Entity>,
}

/// The game's shower water (`shower_running_lp`, the shower's `EventCallbackStartShoweringSound`)
/// and singing in it (`vo_shower_singA`, a child's `C`), now and then (its `ChanceOfSinging`).
const SHOWER_WATER: &str = "shower_running_lp";
const SING_CHANCE: f64 = 0.25;

/// An object already looked at for effects.
#[derive(Component)]
struct FxChecked;

/// A sprinkler's spray: the dome of water in its model (the game draws it in the sprinkler's
/// blue, see-through), shown while it's on.
#[derive(Component)]
struct SprayDome;

/// A fountain whose water surfaces (its untextured parts, drawn by the game's water shader)
/// are still to be made water (a bathtub's too: its water, raised as it's run).
#[derive(Component)]
struct FountainWater;

/// A bathtub: its water is clear, not a fountain's green.
#[derive(Component)]
struct BathWater;

/// A shower-tub's spray: only for a shower, not a bath.
#[derive(Component)]
struct ShowerOnly;

/// The water's looks, made once: a fountain's, a bath's, and a bubble bath's foam.
#[derive(Resource, Default)]
struct WaterLooks {
    fountain: Option<Handle<StandardMaterial>>,
    tub: Option<Handle<StandardMaterial>>,
    foam: Option<Handle<StandardMaterial>>,
}

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
    watchers: Query<(&crate::interact::ActionQueue, Option<&crate::surroundings::WatchingTv>)>,
    mut pictures: Query<(&mut TvPicture, &mut MeshMaterial3d<StandardMaterial>)>,
    mut lights: Query<&mut PointLight>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut frames: Local<Vec<Handle<StandardMaterial>>>,
    mut quad: Local<Option<Handle<Mesh>>>,
) {
    let now = time.elapsed_secs();
    let mut rng = rand::rng();
    // (Several can watch at once without any of them having the set to themselves.)
    // (Or from a seat facing it.)
    let watched: std::collections::HashSet<Entity> = watchers
        .iter()
        .filter_map(|(q, seated)| match q.0.front().map(|a| (&a.kind, a.phase)) {
            Some((crate::interact::ActionKind::Object { target, .. }, crate::interact::Phase::Running(_))) => Some(seated.map_or(*target, |w| w.0)),
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
        if o.kind == ObjectKind::Bathtub {
            commands.entity(e).insert((FountainWater, BathWater));
            continue;
        }
        // (A shower over a tub: the tub's water, and the shower's spray only while showering.)
        if o.kind == ObjectKind::ShowerTub {
            commands.entity(e).insert((FountainWater, BathWater, ShowerOnly));
        }
        let spray = match o.kind {
            ObjectKind::Shower | ObjectKind::ShowerTub => Spray::Shower,
            ObjectKind::Sink => Spray::Tap,
            ObjectKind::Sprinkler => Spray::Sprinkler,
            _ if script.contains(".Environment.Fountain") && !script.contains("FountainJet") => Spray::Fountain,
            _ => continue,
        };
        if spray == Spray::Fountain {
            commands.entity(e).insert(FountainWater);
        }
        if spray == Spray::Sprinkler {
            commands.entity(e).insert(SprayDome);
        }
        // The highest slot: the shower head, the tap, the fountain's top (a sprinkler's head, or
        // its middle when it has no slot).
        let highest = slots.get(&o.objd).and_then(|s| s.iter().max_by(|a, b| a[1].total_cmp(&b[1])).copied());
        let Some(slot) = highest.or((spray == Spray::Sprinkler).then_some([0.0, 0.1, 0.0])) else { continue };
        if !matches!(spray, Spray::Tap | Spray::Sprinkler) && slot[1] < 0.3 {
            continue;
        }
        commands.entity(e).insert(Emitter { slot: Vec3::from(slot), spray, owed: 0.0, sounding: None });
    }
}

/// A fountain's water surfaces: glossy, see-through blue-green instead of the plain grey of an
/// untextured part.
fn fountain_water(
    mut commands: Commands,
    q: Query<(Entity, &Children, Has<BathWater>), With<FountainWater>>,
    parts: Query<&MeshMaterial3d<StandardMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut looks: ResMut<WaterLooks>,
) {
    if looks.fountain.is_none() {
        let mut m = StandardMaterial {
            base_color: Color::srgba(0.32, 0.6, 0.66, 0.82),
            perceptual_roughness: 0.04,
            reflectance: 0.6,
            alpha_mode: AlphaMode::Blend,
            ..default()
        };
        looks.fountain = Some(mats.add(m.clone()));
        m.base_color = Color::srgba(0.62, 0.8, 0.9, 0.6);
        looks.tub = Some(mats.add(m));
        // (A bubble bath's foam: thick and white over the water.)
        looks.foam = Some(mats.add(StandardMaterial { base_color: Color::srgba(0.97, 0.97, 0.99, 0.94), perceptual_roughness: 0.85, alpha_mode: AlphaMode::Blend, ..default() }));
    }
    let (Some(fountain), Some(tub)) = (looks.fountain.clone(), looks.tub.clone()) else { return };
    for (e, children, bath) in &q {
        let w = if bath { tub.clone() } else { fountain.clone() };
        for c in children {
            let Ok(m) = parts.get(*c) else { continue };
            if mats.get(&m.0).is_some_and(|m| m.base_color_texture.is_none() && m.alpha_mode == AlphaMode::Opaque) {
                commands.entity(*c).insert(MeshMaterial3d(w.clone()));
            }
        }
        commands.entity(e).remove::<FountainWater>();
    }
}

/// A bubble bath's water is foam, while it's being had (the game's `BubbleBathFXName`).
fn bubble_baths(
    tubs: Query<(&Children, &UsedBy, &GameObject), With<BathWater>>,
    sims: Query<&ActionQueue>,
    mut parts: Query<&mut MeshMaterial3d<StandardMaterial>>,
    looks: Res<WaterLooks>,
) {
    let (Some(tub), Some(foam)) = (&looks.tub, &looks.foam) else { return };
    for (children, used, o) in &tubs {
        let bubbly = used.0.and_then(|s| sims.get(s).ok()).and_then(|q| q.0.front()).is_some_and(|a| {
            matches!(a.kind, ActionKind::Object { def, .. } if interactions_for(o.kind).get(def).is_some_and(|d| d.special == Special::BubbleBath))
        });
        let want = if bubbly { foam } else { tub };
        for c in children {
            if let Ok(mut m) = parts.get_mut(*c)
                && (m.0 == *tub || m.0 == *foam)
                && m.0 != *want
            {
                m.0 = want.clone();
            }
        }
    }
}

/// A sprinkler's spray dome (its blended part) in see-through water blue, shown while it runs.
#[allow(clippy::type_complexity)]
fn sprinkler_spray(
    mut commands: Commands,
    sprinklers: Query<(&Children, Has<crate::gardening::Sprinkling>), With<SprayDome>>,
    mut parts: Query<(&mut MeshMaterial3d<StandardMaterial>, &mut Visibility)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut water: Local<Option<Handle<StandardMaterial>>>,
) {
    let w = water
        .get_or_insert_with(|| {
            mats.add(StandardMaterial {
                // (A faint shimmer of water: the game draws it with its glass shader.)
                base_color: Color::srgba(0.82, 0.9, 1.0, 0.1),
                perceptual_roughness: 0.05,
                reflectance: 0.8,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })
        })
        .clone();
    for (children, on) in &sprinklers {
        for c in children {
            let Ok((mut m, mut v)) = parts.get_mut(*c) else { continue };
            let dome = m.0 == w || mats.get(&m.0).is_some_and(|m| m.alpha_mode == AlphaMode::Blend);
            if !dome {
                continue;
            }
            if m.0 != w {
                m.0 = w.clone();
                // (Spray casts no shadow.)
                commands.entity(*c).insert(NotShadowCaster);
            }
            v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
}

/// Fountains play; showers and taps run while someone's at them.
#[allow(clippy::type_complexity)]
fn emit(
    mut commands: Commands,
    time: Res<Time>,
    mut emitters: Query<(&mut Emitter, &GlobalTransform, &UsedBy, &InheritedVisibility, Has<crate::gardening::Sprinkling>, Has<ShowerOnly>, Has<crate::interact::Broken>)>,
    users: Query<(&GlobalTransform, &ActionQueue), With<crate::sim::Sim>>,
    (mut cues, sims): (MessageWriter<crate::sound::ClipCue>, Query<&crate::sim::Sim>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut looks: Local<Looks>,
) {
    let dt = time.delta_secs().min(0.1);
    let mut rng = rand::rng();
    for (mut em, gt, used, vis, sprinkling, shower_only, broken) in &mut emitters {
        if !vis.get() {
            continue;
        }
        // (Broken, a tap or a shower sprays all the while: water everywhere, and puddles.)
        let leaking = broken && matches!(em.spray, Spray::Tap | Spray::Shower);
        let on = match em.spray {
            Spray::Fountain => true,
            Spray::Sprinkler => sprinkling,
            _ if leaking => true,
            // (Someone at it, not just on their way.)
            _ => used.0.and_then(|u| users.get(u).ok()).is_some_and(|(u, q)| {
                u.translation().xz().distance(gt.translation().xz()) < 1.3
                    && (!shower_only
                        || q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Object { def, .. } if interactions_for(ObjectKind::ShowerTub).get(def).is_some_and(|d| d.name == "Take Shower"))))
            }),
        };
        // (A shower's water heard while it runs, on its Sim.)
        if em.spray == Spray::Shower {
            let user = used.0.filter(|_| on);
            if em.sounding != user {
                if let Some(was) = em.sounding.take() {
                    cues.write(crate::sound::ClipCue { sim: was, name: String::new(), action: s3formats::sim::SoundAction::StopLoop });
                }
                if let Some(now) = user {
                    cues.write(crate::sound::ClipCue { sim: now, name: SHOWER_WATER.into(), action: s3formats::sim::SoundAction::StartLoop });
                    if rand::rng().random_bool(SING_CHANCE) {
                        let child = sims.get(now).is_ok_and(|s| s.age.is_little() || s.age == crate::sim::Age::Child);
                        let song = if child { "vo_shower_singC" } else { "vo_shower_singA" };
                        cues.write(crate::sound::ClipCue { sim: now, name: song.into(), action: s3formats::sim::SoundAction::StartLoop });
                    }
                    em.sounding = Some(now);
                }
            }
        }
        if !on {
            em.owed = 0.0;
            continue;
        }
        if em.owed == 0.0 && !matches!(em.spray, Spray::Fountain | Spray::Sprinkler) {
            debug!("{:?} running at {:?}", em.spray, gt.transform_point(em.slot));
        }
        let rate = match em.spray {
            Spray::Tap if leaking => 120.0,
            Spray::Fountain => 170.0,
            Spray::Sprinkler => 140.0,
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
                // Out and up in every direction, landing a few metres off.
                Spray::Sprinkler => {
                    let up = rng.random_range(0.5..1.0);
                    (Vec3::new(a.cos(), up, a.sin()), rng.random_range(3.0..4.5), rng.random_range(0.8..1.1), 0.07, 0.04)
                }
                Spray::Shower => {
                    let out = rng.random_range(0.0..0.22);
                    (Vec3::new(a.cos() * out, -1.0, a.sin() * out), rng.random_range(1.2..2.0), 0.85, 0.035, 0.06)
                }
                // (A broken tap spurts up and out.)
                Spray::Tap if leaking => {
                    let out = rng.random_range(0.15..0.6);
                    (Vec3::new(a.cos() * out, 1.0, a.sin() * out), rng.random_range(1.4..2.4), rng.random_range(0.35..0.55), 0.03, 0.01)
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
            commands.entity(e).try_despawn();
            continue;
        }
        d.vel.y -= 9.8 * dt;
        tf.translation += d.vel * dt;
        if let Some(c) = cam {
            tf.rotation = Transform::from_translation(tf.translation).looking_at(c, Vec3::Y).rotation;
        }
    }
}

/// A change of clothes' swirl of sparkles round a Sim (the game's `clothesChangeAdult`): how
/// tall they are.
#[derive(Message)]
pub struct SpinSparkle {
    pub sim: Entity,
    pub height: f32,
}

/// One of a swirl's sparkles: round its Sim and up, fading out.
#[derive(Component)]
struct Sparkle {
    sim: Entity,
    angle: f32,
    turn: f32,
    radius: f32,
    y: f32,
    rise: f32,
    age: f32,
    life: f32,
    size: f32,
}

/// A twinkle: a soft glow with a four-pointed star through it.
fn star_image() -> Image {
    let n = 32u32;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = ((x as f32 + 0.5) / n as f32 * 2.0 - 1.0, (y as f32 + 0.5) / n as f32 * 2.0 - 1.0);
            let r = (u * u + v * v).sqrt();
            let glow = (1.0 - r).clamp(0.0, 1.0).powf(3.0);
            let rays = (-u.abs() * 14.0).exp() * (1.0 - v.abs()).max(0.0).powf(2.0) + (-v.abs() * 14.0).exp() * (1.0 - u.abs()).max(0.0).powf(2.0);
            let a = (glow * 0.9 + rays).min(1.0);
            // (White at the heart and along the rays, blue round them: seen against a white wall.)
            let w = (rays * 1.5 + (1.0 - r * 2.5).max(0.0)).min(1.0);
            let c = |blue: f32| ((blue + (255.0 - blue) * w) as u8);
            data.extend_from_slice(&[c(90.0), c(150.0), 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// Swirls of sparkles round Sims changing their clothes, spiralling up them and fading.
#[allow(clippy::too_many_arguments)]
fn sparkle(
    mut commands: Commands,
    mut asked: MessageReader<SpinSparkle>,
    time: Res<Time>,
    clock: Res<crate::clock::GameClock>,
    (mut meshes, mut mats, mut images): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>, ResMut<Assets<Image>>),
    mut looks: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
    cams: Query<&GlobalTransform, With<SimsCamera>>,
    sims: Query<&GlobalTransform, With<crate::sim::Sim>>,
    mut q: Query<(Entity, &mut Sparkle, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    let mut rng = rand::rng();
    for s in asked.read() {
        let (quad, mat) = looks
            .get_or_insert_with(|| {
                (
                    meshes.add(Rectangle::new(1.0, 1.0)),
                    mats.add(StandardMaterial {
                        base_color: Color::WHITE,
                        base_color_texture: Some(images.add(star_image())),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        double_sided: true,
                        cull_mode: None,
                        ..default()
                    }),
                )
            })
            .clone();
        let Ok(at) = sims.get(s.sim) else { continue };
        let look = mats.get(&mat).cloned().unwrap_or_default();
        for _ in 0..70 {
            let life = rng.random_range(0.7..1.3);
            commands.spawn((
                Mesh3d(quad.clone()),
                // (Each its own material, to fade on its own.)
                MeshMaterial3d(mats.add(look.clone())),
                Transform::from_translation(at.translation()).with_scale(Vec3::ZERO),
                Sparkle {
                    sim: s.sim,
                    angle: rng.random_range(0.0..std::f32::consts::TAU),
                    turn: rng.random_range(5.0..9.0),
                    radius: rng.random_range(0.22..0.42),
                    y: rng.random_range(-0.1..0.35) * s.height,
                    rise: s.height * rng.random_range(0.8..1.25) / life,
                    age: 0.0,
                    life,
                    size: rng.random_range(0.07..0.16),
                },
                NotShadowCaster,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    // (In step with the spin: at the game's speed.)
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let cam = cams.single().ok().map(|c| c.translation());
    for (e, mut s, mut tf, m) in &mut q {
        s.age += dt;
        let Ok(at) = sims.get(s.sim) else {
            commands.entity(e).try_despawn();
            continue;
        };
        if s.age >= s.life {
            commands.entity(e).try_despawn();
            continue;
        }
        s.angle += s.turn * dt;
        s.y += s.rise * dt;
        let f = s.age / s.life;
        // (In quickly, twinkling, and out.)
        let fade = (f * 14.0).min(1.0) * (1.0 - f).powf(0.6) * (0.75 + 0.25 * (s.age * 40.0 + s.angle).sin());
        let r = s.radius * (1.0 - 0.35 * f);
        tf.translation = at.translation() + Vec3::new(s.angle.cos() * r, s.y, s.angle.sin() * r);
        tf.scale = Vec3::splat(s.size * (0.6 + 0.8 * fade));
        if let Some(c) = cam {
            tf.rotation = Transform::from_translation(tf.translation).looking_at(c, Vec3::Y).rotation;
        }
        if let Some(mut mat) = mats.get_mut(&m.0) {
            mat.base_color = Color::srgba(1.0, 1.0, 1.0, fade);
        }
    }
}

/// The game's censor (`simCensorFx`, put on by its `AutoEnableCensor`) over a Sim with nothing
/// on: a mosaic of big squares in their skin's colours, over a man's hips (`LowerBody`, from the
/// pelvis) or a woman's chest and hips (`FullBody`), turned to the camera.
#[derive(Component)]
struct Censor {
    sim: Entity,
    /// Its mosaic's frames, shuffled through as the game's shimmers.
    frames: Vec<Handle<StandardMaterial>>,
}

/// A frame of a censor's mosaic: squares of the skin's colour, lighter and darker.
fn censor_frame(skin: Srgba, female: bool, rng: &mut impl Rng) -> Image {
    let (w, h) = if female { (5u32, 8u32) } else { (5, 5) };
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let k = rng.random_range(0.55..1.3f32);
            let warm = rng.random_range(0.88..1.06f32);
            let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
            data.extend_from_slice(&[c(skin.red * k), c(skin.green * k * warm), c(skin.blue * k * warm), if edge { 200 } else { 255 }]);
        }
    }
    let mut img = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    img.sampler = bevy::image::ImageSampler::nearest();
    img
}

#[allow(clippy::type_complexity)]
fn censor(
    mut commands: Commands,
    sims: Query<(Entity, &crate::sim::Sim, &crate::simbody::Wearing, &crate::simbody::Skeleton, &GlobalTransform)>,
    joints: Query<&GlobalTransform>,
    mut censors: Query<(Entity, &Censor, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
    camera: Query<&GlobalTransform, With<SimsCamera>>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut quad: Local<Option<Handle<Mesh>>>,
    time: Res<Time>,
) {
    use crate::simbody::OutfitKind;
    // Gone with the clothes back on.
    for (e, c, ..) in &censors {
        if !sims.get(c.sim).is_ok_and(|s| s.2.0 == OutfitKind::Naked) {
            commands.entity(e).try_despawn();
        }
    }
    let Ok(cam) = camera.single() else { return };
    let quad = quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
    for (me, sim, wearing, skel, sim_tf) in &sims {
        if wearing.0 != OutfitKind::Naked {
            continue;
        }
        let bone = |n: &str| skel.rig.bones.iter().position(|b| b.name == n).and_then(|i| joints.get(*skel.joints.get(i)?).ok()).map(|g| g.translation());
        let (Some(pelvis), Some(chest)) = (bone("b__Pelvis__"), bone("b__Spine2__")) else { continue };
        // Along the body (up it standing, along it lying in the bath), sized by it.
        let torso = chest - pelvis;
        let len = torso.length().max(0.05);
        let up = torso / len;
        let (centre, size) = if sim.female { (pelvis + torso * 0.25, Vec2::new(len * 1.25, len * 1.7)) } else { (pelvis - up * len * 0.33, Vec2::new(len * 1.0, len * 0.95)) };
        let toward = (cam.translation() - centre).normalize_or_zero();
        let world = Transform::from_translation(centre + toward * 0.3).looking_to(-toward, up).with_scale(size.extend(1.0));
        let local = GlobalTransform::from(world).reparented_to(sim_tf);
        match censors.iter_mut().find(|c| c.1.sim == me) {
            Some((_, c, mut tf, mut mat)) => {
                *tf = local;
                let frame = &c.frames[(time.elapsed_secs() * 6.0) as usize % c.frames.len()];
                if mat.0 != *frame {
                    mat.0 = frame.clone();
                }
            }
            None => {
                let mut rng = rand::rng();
                let frames: Vec<Handle<StandardMaterial>> = (0..4)
                    .map(|_| {
                        let img = images.add(censor_frame(sim.skin.to_srgba(), sim.female, &mut rng));
                        materials.add(StandardMaterial {
                            base_color_texture: Some(img),
                            unlit: true,
                            alpha_mode: AlphaMode::Blend,
                            cull_mode: None,
                            ..default()
                        })
                    })
                    .collect();
                let first = frames[0].clone();
                let e = commands.spawn((Censor { sim: me, frames }, Mesh3d(quad.clone()), MeshMaterial3d(first), local, NotShadowCaster)).id();
                commands.entity(me).add_child(e);
            }
        }
    }
}
