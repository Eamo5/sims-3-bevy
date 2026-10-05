//! The sky: a dome around the camera drawn with the game's own sky textures (its cloud noise, the
//! night's star map, the sun's and moon's halos), coloured by the time of day, and the distance
//! fog matched to its horizon.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

use crate::AppState;
use crate::camera::SimsCamera;

pub type SkyMaterial = ExtendedMaterial<StandardMaterial, SkyExt>;

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct SkyParams {
    pub sun: Vec4,
    pub zenith: Vec4,
    pub horizon: Vec4,
    pub sun_color: Vec4,
    pub params: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct SkyExt {
    #[uniform(100)]
    pub sky: SkyParams,
    #[texture(101)]
    #[sampler(102)]
    pub clouds: Option<Handle<Image>>,
    #[texture(103)]
    #[sampler(104)]
    pub stars: Option<Handle<Image>>,
    #[texture(105)]
    #[sampler(106)]
    pub halo: Option<Handle<Image>>,
}

impl MaterialExtension for SkyExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://sims3/shaders/sky.wgsl".into()
    }
}

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sky.wgsl");
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .add_systems(Update, (spawn_sky, update_sky).chain().run_if(in_state(AppState::InGame)));
    }
}

#[derive(Component)]
struct SkyDome(Handle<SkyMaterial>);

/// Far enough to be behind the world, near enough for the camera's far plane.
const RADIUS: f32 = 4500.0;

/// A texture from the gameplay icons with a repeating sampler.
fn tiled(ui: &crate::icons::GameUi, images: &mut Assets<Image>, name: &str, repeat: bool, srgb: bool) -> Option<Handle<Image>> {
    let png = ui.png(name)?;
    let (w, h, px) = s3bake::gamedata::decode_icon(&png)?;
    let mut img = Image::new(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        px,
        // (Noise is data, not colour.)
        if srgb { bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb } else { bevy::render::render_resource::TextureFormat::Rgba8Unorm },
        RenderAssetUsages::RENDER_WORLD,
    );
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: mode, address_mode_v: mode, ..ImageSamplerDescriptor::linear() });
    Some(images.add(img))
}

fn spawn_sky(
    mut commands: Commands,
    existing: Query<(), With<SkyDome>>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<SkyMaterial>>,
) {
    if !existing.is_empty() {
        return;
    }
    let Some(ui) = ui else { return };
    let mat = mats.add(SkyMaterial {
        base: StandardMaterial { unlit: true, cull_mode: None, double_sided: true, fog_enabled: false, ..default() },
        extension: SkyExt {
            sky: SkyParams::default(),
            clouds: tiled(&ui, &mut images, "CloudNoiseBase", true, false),
            stars: tiled(&ui, &mut images, "NightSkyStarsFlat", true, true),
            halo: tiled(&ui, &mut images, "Sky_SunHalo", false, true),
        },
    });
    commands.spawn((
        SkyDome(mat.clone()),
        Mesh3d(meshes.add(Sphere::new(RADIUS).mesh().uv(48, 24))),
        MeshMaterial3d(mat),
        Transform::default(),
        NotShadowCaster,
        NotShadowReceiver,
        DespawnOnExit(AppState::InGame),
    ));
}

#[allow(clippy::type_complexity)]
fn update_sky(
    time: Res<Time>,
    clock: Option<Res<crate::clock::GameClock>>,
    night: Option<Res<crate::clock::Night>>,
    mut dome: Query<(&SkyDome, &mut Transform), Without<SimsCamera>>,
    mut cam: Query<(&GlobalTransform, Option<&mut DistanceFog>), With<SimsCamera>>,
    sun: Query<&GlobalTransform, With<DirectionalLight>>,
    mut mats: ResMut<Assets<SkyMaterial>>,
) {
    let Ok((dome, mut tf)) = dome.single_mut() else { return };
    let Ok((cam_tf, fog)) = cam.single_mut() else { return };
    tf.translation = cam_tf.translation();
    let h = clock.as_ref().map_or(12.0, |c| c.hour_f());
    let elev = ((h - 6.0) / 12.0 * std::f32::consts::PI).sin();
    let day = elev.clamp(0.0, 1.0);
    let dark = night.map_or(0.0, |n| n.0);
    let twilight = (1.0 - (elev.abs() * 3.5).min(1.0)).max(0.0);
    // The sun's direction: against the light's travel.
    let to_sun = sun.iter().next().map_or(Vec3::Y, |s| -s.forward().as_vec3());
    let to_sun = if elev < 0.0 { Vec3::new(to_sun.x, elev, to_sun.z).normalize_or(Vec3::Y) } else { to_sun };
    let lin = |c: Color| c.to_linear().to_vec4();
    let mix = |a: Color, b: Color, t: f32| a.mix(&b, t.clamp(0.0, 1.0));
    let zenith = mix(mix(Color::srgb(0.02, 0.03, 0.08), Color::srgb(0.24, 0.45, 0.82), day), Color::srgb(0.30, 0.32, 0.55), twilight * 0.6);
    let horizon = mix(mix(Color::srgb(0.06, 0.08, 0.15), Color::srgb(0.70, 0.82, 0.95), day), Color::srgb(0.98, 0.62, 0.42), twilight * 0.8);
    let sun_color = mix(Color::srgb(1.0, 0.96, 0.85), Color::srgb(1.0, 0.55, 0.30), twilight);
    if let Some(mut m) = mats.get_mut(&dome.0) {
        m.extension.sky = SkyParams {
            sun: to_sun.extend(day),
            zenith: lin(zenith),
            horizon: lin(horizon),
            sun_color: lin(sun_color).truncate().extend(twilight),
            params: Vec4::new(time.elapsed_secs(), dark, 0.45, 0.0),
        };
    }
    // Distant things fade into the horizon.
    if let Some(mut f) = fog {
        f.color = horizon;
    }
}
