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
use bevy::render::render_resource::{AsBindGroup, ShaderType, Extent3d, TextureDimension, TextureFormat};
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
    pub home_to_local: Mat4,
    pub away_to_local: Mat4,
    /// xy: home lot dimensions, zw: visited lot dimensions.
    pub shelter_size: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct PrecipExt {
    #[uniform(100)]
    pub precip: PrecipParams,
    #[texture(101, dimension = "2d_array", sample_type = "float", filterable = false)]
    pub shelter: Handle<Image>,
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

/// Each tile stores the highest shelter ceiling for its four floor triangles.
/// The two array layers are the home and visited lots, each in its own local space.
fn shelter_map(building: Option<&crate::building::ActiveBuilding>) -> (Image, Mat4, Mat4, Vec4) {
    let lots = [building, building.and_then(|b| b.away.as_deref())];
    let width = lots.iter().flatten().map(|b| b.data.width).max().unwrap_or(1).max(1);
    let depth = lots.iter().flatten().map(|b| b.data.depth).max().unwrap_or(1).max(1);
    let mut pixels = vec![[-1.0e6f32; 4]; (width * depth * 2) as usize];
    let mut transforms = [Mat4::IDENTITY; 2];
    let mut sizes = Vec4::ZERO;
    for (layer, lot) in lots.into_iter().enumerate() {
        let Some(b) = lot else { continue };
        transforms[layer] = Mat4::from_rotation_translation(b.rot, b.corner).inverse();
        sizes[layer * 2] = b.data.width as f32;
        sizes[layer * 2 + 1] = b.data.depth as f32;
        for floor in &b.data.floors {
            if floor.kind == s3bake::ROOM_OUTSIDE || floor.kind == s3bake::ROOM_PORCH || floor.x as u32 >= width || floor.z as u32 >= depth { continue; }
            for triangle in 0..4 {
                if floor.mask & (1 << triangle) == 0 { continue; }
                let offset = [Vec2::NEG_Y, Vec2::X, Vec2::Y, Vec2::NEG_X][triangle] / 3.0;
                let p = b.world(floor.x as f32 + 0.5 + offset.x, floor.z as f32 + 0.5 + offset.y, 0.0);
                let Some(y) = b.floor_y(floor.level, p) else { continue };
                let index = layer * (width * depth) as usize + floor.z as usize * width as usize + floor.x as usize;
                pixels[index][triangle] = pixels[index][triangle].max(y + s3bake::building::LEVEL_HEIGHT);
            }
        }
    }
    let bytes = pixels.iter().flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes())).collect();
    let image = Image::new(Extent3d { width, height: depth, depth_or_array_layers: 2 }, TextureDimension::D2, bytes, TextureFormat::Rgba32Float, RenderAssetUsages::default());
    (image, transforms[0], transforms[1], sizes)
}

/// PRECIP_SHELTER_TEST=1 audits the uploaded mask against the live room queries.
fn verify_shelter(image: &Image, b: &crate::building::ActiveBuilding, params: PrecipParams) {
    let bytes = image.data.as_ref().expect("CPU shelter texture retained");
    let width = image.texture_descriptor.size.width as usize;
    let depth = image.texture_descriptor.size.height as usize;
    let mut indoors = 0;
    let mut outdoors = 0;
    for (layer, lot) in [Some(b), b.away.as_deref()].into_iter().enumerate() {
        let Some(lot) = lot else { continue };
        let transform = if layer == 0 { params.home_to_local } else { params.away_to_local };
        for z in 0..lot.data.depth {
            for x in 0..lot.data.width {
                for triangle in 0..4 {
                    let offset = [Vec2::NEG_Y, Vec2::X, Vec2::Y, Vec2::NEG_X][triangle] / 3.0;
                    let local = Vec2::new(x as f32 + 0.5, z as f32 + 0.5) + offset;
                    let world = lot.world(local.x, local.y, 0.0);
                    assert!(transform.transform_point3(world).xz().distance(local) < 0.001, "rotated lot maps into the shelter texture");
                    let at = ((layer * width * depth + z as usize * width + x as usize) * 4 + triangle) * 4;
                    let ceiling = f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
                    let mut enclosed = false;
                    for level in 0..lot.levels.len() {
                        if lot.room_at(level as u8, world).is_some_and(|k| k != s3bake::ROOM_OUTSIDE && k != s3bake::ROOM_PORCH) {
                            enclosed = true;
                            assert!(ceiling >= lot.floor_y(level as u8, world).unwrap() + s3bake::building::LEVEL_HEIGHT - 0.001);
                        }
                    }
                    if enclosed { indoors += 1; } else {
                        outdoors += 1;
                        assert!(ceiling < -100000.0, "open-air triangles must allow precipitation");
                    }
                }
            }
        }
    }
    assert!(indoors > 0 && outdoors > 0);
    info!("precipitation shelter PASS: {indoors} indoor and {outdoors} outdoor triangles, rotated lot coordinates checked");
}

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
    mut images: ResMut<Assets<Image>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut verified: Local<bool>,
) {
    let (Some(clock), Some(w)) = (clock, weather) else { return };
    let Ok((p, mut vis)) = fx.single_mut() else {
        let (shelter, home_to_local, away_to_local, shelter_size) = shelter_map(building.as_deref());
        let mat = mats.add(PrecipMaterial {
            base: StandardMaterial { unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, double_sided: true, fog_enabled: false, ..default() },
            extension: PrecipExt { precip: PrecipParams { home_to_local, away_to_local, shelter_size, ..default() }, shelter: images.add(shelter) },
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
    if let Some(mut m) = mats.get_mut(&p.0) {
        let missing = building.is_none() && m.extension.precip.shelter_size != Vec4::ZERO;
        if missing || building.as_ref().is_some_and(|b| b.is_changed()) {
            let (image, home, away, sizes) = shelter_map(building.as_deref());
            if let Some(mut existing) = images.get_mut(&m.extension.shelter) { *existing = image; }
            m.extension.precip.home_to_local = home;
            m.extension.precip.away_to_local = away;
            m.extension.precip.shelter_size = sizes;
        }
        if !*verified && std::env::var_os("PRECIP_SHELTER_TEST").is_some()
            && let Some(b) = building.as_deref()
            && let Some(image) = images.get(&m.extension.shelter)
            && !b.data.floors.is_empty()
        {
            verify_shelter(image, b, m.extension.precip);
            *verified = true;
        }
    }
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
            ..m.extension.precip
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
