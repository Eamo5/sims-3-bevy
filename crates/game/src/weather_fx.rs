//! The weather as it's seen and heard: rain, snow and hail falling round the camera (see
//! `shaders/precip.wgsl`), lightning flashes and the game's thunder after them, and the game's
//! rain, snow, hail and wind loops, louder the heavier it comes down. (The sky, the light, the
//! fog and the ground take the weather up in their own modules.)

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::audio::Volume;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::AppState;
use crate::camera::SimsCamera;
use crate::weather::{Intensity, Thunder, Weather, WeatherKind};

pub type PrecipMaterial = ExtendedMaterial<StandardMaterial, PrecipExt>;

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct PrecipParams {
    /// xyz: the box's centre, w: time (s).
    pub centre: Vec4,
    /// x: kind (0 rain, 1 snow, 2 hail), y: how many drops fall (0..1), zw: wind.
    pub params: Vec4,
    /// x: the box's size, y: drop size scale, z: daylight.
    pub look: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct PrecipExt {
    #[uniform(100)]
    pub precip: PrecipParams,
}

impl MaterialExtension for PrecipExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://sims3/shaders/precip.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://sims3/shaders/precip.wgsl".into()
    }
}

pub struct WeatherFxPlugin;

impl Plugin for WeatherFxPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/precip.wgsl");
        app.add_plugins(MaterialPlugin::<PrecipMaterial>::default())
            .init_resource::<Lightning>()
            .add_systems(Update, (precipitation, thunder, weather_sounds, seasonal_looks).run_if(in_state(AppState::InGame)));
    }
}

/// When lightning last flashed (real seconds) and how bright (far strikes dimmer).
#[derive(Resource, Default, Clone, Copy)]
pub struct Lightning {
    pub at: f32,
    pub strength: f32,
}

impl Lightning {
    /// The flash now (0..1): a bright blink, a flicker, and gone.
    pub fn flash(&self, now: f32) -> f32 {
        let t = now - self.at;
        if !(0.0..0.6).contains(&t) {
            return 0.0;
        }
        let blink = (-t * 14.0).exp() + if (0.18..0.26).contains(&t) { 0.6 } else { 0.0 };
        (blink * self.strength).min(1.0)
    }
}

/// The falling rain, snow or hail.
#[derive(Component)]
struct Precipitation(Handle<PrecipMaterial>);

/// Drops in the box.
const DROPS: usize = 9000;

fn drop_mesh() -> Mesh {
    let mut rng = rand::rng();
    let (mut pos, mut nrm, mut uv, mut idx) = (Vec::with_capacity(DROPS * 4), Vec::with_capacity(DROPS * 4), Vec::with_capacity(DROPS * 4), Vec::with_capacity(DROPS * 6));
    for i in 0..DROPS as u32 {
        let seed = [rng.random::<f32>(), rng.random::<f32>(), rng.random::<f32>()];
        let r: f32 = rng.random();
        for c in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            pos.push(seed);
            nrm.push([r, 0.0, 0.0]);
            uv.push(c);
        }
        let b = i * 4;
        idx.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(idx))
}

#[allow(clippy::too_many_arguments)]
fn precipitation(
    mut commands: Commands,
    time: Res<Time>,
    clock: Option<Res<crate::clock::GameClock>>,
    weather: Option<Res<Weather>>,
    night: Option<Res<crate::clock::Night>>,
    cams: Query<&SimsCamera>,
    mut fx: Query<(&Precipitation, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<PrecipMaterial>>,
) {
    let (Some(clock), Some(w)) = (clock, weather) else { return };
    let Ok((p, mut vis)) = fx.single_mut() else {
        let mat = mats.add(PrecipMaterial {
            base: StandardMaterial { unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, double_sided: true, fog_enabled: false, ..default() },
            extension: PrecipExt { precip: PrecipParams::default() },
        });
        commands.spawn((
            Precipitation(mat.clone()),
            Mesh3d(meshes.add(drop_mesh())),
            MeshMaterial3d(mat),
            Transform::default(),
            Visibility::Hidden,
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
            DespawnOnExit(AppState::InGame),
        ));
        return;
    };
    let Ok(cam) = cams.single() else { return };
    let falling = w.falling(clock.minutes);
    vis.set_if_neq(if falling > 0.01 { Visibility::Inherited } else { Visibility::Hidden });
    if falling <= 0.01 {
        return;
    }
    let kind = match w.kind {
        WeatherKind::Snow => 1.0,
        WeatherKind::Hail => 2.0,
        _ => 0.0,
    };
    // (A bigger box, and bigger drops, the further out the camera is.)
    let size = (cam.distance * 1.1).clamp(26.0, 110.0);
    let wind = match w.intensity {
        Intensity::Heavy => 2.6,
        Intensity::Moderate => 1.4,
        Intensity::Light => 0.6,
    };
    if let Some(mut m) = mats.get_mut(&p.0) {
        m.extension.precip = PrecipParams {
            centre: (cam.focus + Vec3::Y * size * 0.12).extend(time.elapsed_secs()),
            params: Vec4::new(kind, falling, wind, wind * 0.4),
            look: Vec4::new(size, (size / 40.0).max(1.0), 1.0 - night.map_or(0.0, |n| n.0), 0.0),
        };
    }
}

/// The trees through the seasons and the roads under snow and rain (their materials told
/// when it changes).
fn seasonal_looks(
    clock: Option<Res<crate::clock::GameClock>>,
    weather: Option<Res<Weather>>,
    mut trees: ResMut<Assets<crate::world::TreeMaterial>>,
    mut roads: ResMut<Assets<crate::roads::RoadMaterial>>,
    mut last: Local<(Vec3, Vec2, usize, usize)>,
) {
    let (Some(clock), Some(w)) = (clock, weather) else { return };
    let tree = w.trees(clock.minutes);
    let road = Vec2::new(w.snow / 100.0, w.wet / 100.0);
    let (nt, nr) = (trees.len(), roads.len());
    if (tree - last.0).abs().max_element() > 0.01 || nt != last.2 {
        for (_, m) in trees.iter_mut() {
            m.extension.billboard.season = tree.extend(m.extension.billboard.season.w);
        }
    }
    if (road - last.1).abs().max_element() > 0.01 || nr != last.3 {
        for (_, m) in roads.iter_mut() {
            m.extension.params.z = road.x;
            m.extension.params.w = road.y;
        }
    }
    *last = (tree, road, nt, nr);
}

/// Lightning: a flash across the sky (the light module brightens with it) and the game's
/// thunder after it, sooner the nearer the strike.
fn thunder(
    time: Res<Time>,
    mut strikes: MessageReader<Thunder>,
    mut lightning: ResMut<Lightning>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    mut pending: Local<Vec<(f32, &'static str)>>,
) {
    let now = time.elapsed_secs();
    for s in strikes.read() {
        *lightning = Lightning { at: now, strength: [1.0, 0.7, 0.4][s.distance.min(2) as usize] };
        let (delay, name) = [(0.0, "thunder_near"), (0.5, "thunder_medium"), (2.0, "thunder_far")][s.distance.min(2) as usize];
        pending.push((now + delay, name));
    }
    pending.retain(|(at, name)| {
        if now >= *at {
            play.write(crate::sound::PlaySound::ui(name).with_volume(0.8));
            return false;
        }
        true
    });
}

/// A weather loop playing (the game's rain, snow, hail or wind), fading to its level.
#[derive(Component)]
struct WeatherLoop {
    name: &'static str,
    level: f32,
}

/// What's heard of the weather: its loop for how heavy it comes down (and wind with heavier
/// weather), quieter far out; loops fade in and out.
#[allow(clippy::too_many_arguments)]
fn weather_sounds(
    mut commands: Commands,
    time: Res<Time>,
    clock: Option<Res<crate::clock::GameClock>>,
    weather: Option<Res<Weather>>,
    sounds: Option<Res<crate::sound::Sounds>>,
    mut cache: ResMut<crate::sound::SampleCache>,
    mut sources: ResMut<Assets<AudioSource>>,
    settings: Res<crate::options::Settings>,
    cams: Query<&SimsCamera>,
    mut loops: Query<(Entity, &mut WeatherLoop, Option<&mut AudioSink>)>,
) {
    let (Some(clock), Some(w), Some(sounds)) = (clock, weather, sounds) else { return };
    let falling = w.falling(clock.minutes);
    let zoom = cams.single().map_or(1.0, |c| (1.0 - (c.distance - 25.0) / 160.0).clamp(0.35, 1.0));
    let i = (w.intensity as usize).min(2);
    let mut want: Vec<(&'static str, f32)> = Vec::new();
    if falling > 0.01 {
        let name = match w.kind {
            WeatherKind::Rain => ["rain_light_lp", "rain_medium_lp", "rain_heavy_lp"][i],
            WeatherKind::Snow => ["snow_light_lp", "snow_medium_lp", "snow_heavy_lp"][i],
            _ => "hail_lp",
        };
        want.push((name, falling.max(0.4)));
        if w.intensity >= Intensity::Moderate {
            want.push((["wind_light_lp", "wind_light_lp", "wind_medium_lp"][i], falling * 0.6));
        }
    } else if w.kind == WeatherKind::Fog {
        want.push(("wind_light_lp", 0.3));
    }
    let dt = time.delta_secs();
    for (e, mut l, sink) in &mut loops {
        let target = want.iter().find(|(n, _)| *n == l.name).map_or(0.0, |(_, v)| *v);
        l.level += (target - l.level).clamp(-dt * 0.4, dt * 0.4);
        if target == 0.0 && l.level <= 0.001 {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(mut sink) = sink {
            let gain = sounds.def(l.name).map_or(1.0, |d| d.gain) * l.level * 0.55 * zoom * settings.gain(crate::options::Channel::Ambience);
            sink.set_volume(Volume::Linear(gain));
        }
    }
    for (name, _) in want {
        if loops.iter().any(|(_, l, _)| l.name == name) {
            continue;
        }
        let Some(def) = sounds.def(name) else { continue };
        let Some(&id) = def.samples.choose(&mut rand::rng()) else { continue };
        let Some(h) = sounds.sample(id, &mut cache, &mut sources) else { continue };
        commands.spawn((AudioPlayer::new(h), PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)), WeatherLoop { name, level: 0.0 }, DespawnOnExit(AppState::InGame)));
    }
}
