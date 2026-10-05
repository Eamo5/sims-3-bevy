//! Water: the sea and the ponds, with drifting ripples, the sky and the sun reflected in them
//! (more at grazing angles), a colour that deepens with the water's depth over the terrain, and
//! clear shallows with a line of foam where the water meets the shore.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use s3bake::Heightmap;

use crate::AppState;
use crate::sky::SkyNow;

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExt>;

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct WaterParams {
    /// xyz: towards the sun, w: daylight.
    pub sun: Vec4,
    pub zenith: Vec4,
    pub horizon: Vec4,
    pub sun_color: Vec4,
    /// Colour of deep and of shallow water (linear).
    pub deep: Vec4,
    pub shallow: Vec4,
    /// x: time (s), y: heightmap size (samples), z: metres per height unit, w: 0 sea, 1 pond.
    pub misc: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WaterExt {
    #[uniform(100)]
    pub water: WaterParams,
    /// The terrain's heights (for the water's depth).
    #[texture(101, sample_type = "u_int")]
    pub heights: Handle<Image>,
    /// Tiling ripple slopes (rg).
    #[texture(102)]
    #[sampler(103)]
    pub ripples: Handle<Image>,
}

impl MaterialExtension for WaterExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://sims3/shaders/water.wgsl".into()
    }
}

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/water.wgsl");
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default()).add_systems(Update, (pool_water, update_water).run_if(in_state(AppState::InGame)));
    }
}

/// A body of water's material.
#[derive(Component)]
pub struct Water(pub Handle<WaterMaterial>);

/// A pool's water, to be given the pool water material.
#[derive(Component)]
pub struct PoolWater;

/// The pools' water material (clear, turquoise, still).
#[derive(Resource)]
pub struct PoolWaterMaterial(pub Handle<WaterMaterial>);

fn pool_water(mut commands: Commands, q: Query<Entity, With<PoolWater>>, mat: Option<Res<PoolWaterMaterial>>) {
    let Some(mat) = mat else { return };
    for e in &q {
        commands.entity(e).remove::<PoolWater>().insert((MeshMaterial3d(mat.0.clone()), Water(mat.0.clone()), NotShadowCaster));
    }
}

/// The terrain's heights as a texture (raw 16-bit samples).
pub fn height_image(hm: &Heightmap) -> Image {
    let data: Vec<u8> = hm.data.iter().flat_map(|v| v.to_le_bytes()).collect();
    Image::new(
        Extent3d { width: hm.width as u32, height: hm.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::R16Uint,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// A tiling field of small waves: the slopes of a sum of waves whose frequencies are whole
/// numbers of cycles per tile, with a full mip chain.
pub fn ripple_image() -> Image {
    const N: usize = 256;
    let mut rng = 0x2545F4914F6CDD1Du64;
    let mut rand = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 11) as f32 / (1u64 << 53) as f32
    };
    // (kx, kz, amplitude, phase)
    let mut waves = Vec::new();
    for _ in 0..48 {
        let f = 1.0 + rand() * 14.0;
        let a = rand() * std::f32::consts::TAU;
        let (kx, kz) = ((a.cos() * f).round(), (a.sin() * f).round());
        if kx == 0.0 && kz == 0.0 {
            continue;
        }
        let k = (kx * kx + kz * kz).sqrt();
        waves.push((kx, kz, 0.6 / k.powf(1.3), rand() * std::f32::consts::TAU));
    }
    let mut slopes = vec![[0.0f32; 2]; N * N];
    let mut max = 0.0f32;
    for z in 0..N {
        for x in 0..N {
            let (u, v) = (x as f32 / N as f32, z as f32 / N as f32);
            let mut s = [0.0f32; 2];
            for &(kx, kz, a, ph) in &waves {
                let th = std::f32::consts::TAU * (kx * u + kz * v) + ph;
                let d = a * th.cos();
                s[0] += d * kx;
                s[1] += d * kz;
            }
            max = max.max(s[0].abs()).max(s[1].abs());
            slopes[z * N + x] = s;
        }
    }
    let mut level: Vec<[f32; 2]> = slopes.iter().map(|s| [s[0] / max, s[1] / max]).collect();
    let mut data = Vec::new();
    let mut size = N;
    let mut mips = 0;
    loop {
        data.extend(level.iter().flat_map(|s| [((s[0] * 0.5 + 0.5) * 255.0) as u8, ((s[1] * 0.5 + 0.5) * 255.0) as u8, 128, 255]));
        mips += 1;
        if size == 1 {
            break;
        }
        let half = size / 2;
        level = (0..half * half)
            .map(|i| {
                let (x, z) = (i % half * 2, i / half * 2);
                let p = [level[z * size + x], level[z * size + x + 1], level[(z + 1) * size + x], level[(z + 1) * size + x + 1]];
                [(p[0][0] + p[1][0] + p[2][0] + p[3][0]) * 0.25, (p[0][1] + p[1][1] + p[2][1] + p[3][1]) * 0.25]
            })
            .collect();
        size = half;
    }
    let mut img = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = mips;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    img
}

/// A pool's water: clear and turquoise over its tiled floor.
pub fn pool_material(heights: Handle<Image>, ripples: Handle<Image>, samples: u32, scale: f32) -> WaterMaterial {
    let mut m = water_material(heights, ripples, samples, scale, true);
    let lin = |r: f32, g: f32, b: f32| Color::srgb(r, g, b).to_linear().to_vec4();
    m.extension.water.deep = lin(0.08, 0.42, 0.62);
    m.extension.water.shallow = lin(0.38, 0.78, 0.86);
    m.extension.water.misc.w = 2.0;
    m
}

/// The water's material: the sea's blue-green, or a pond's darker green.
pub fn water_material(heights: Handle<Image>, ripples: Handle<Image>, samples: u32, scale: f32, pond: bool) -> WaterMaterial {
    let lin = |r: f32, g: f32, b: f32| Color::srgb(r, g, b).to_linear().to_vec4();
    WaterMaterial {
        base: StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.06, reflectance: 0.3, alpha_mode: AlphaMode::Blend, ..default() },
        extension: WaterExt {
            water: WaterParams {
                deep: if pond { lin(0.03, 0.10, 0.11) } else { lin(0.02, 0.13, 0.23) },
                shallow: if pond { lin(0.14, 0.30, 0.30) } else { lin(0.16, 0.52, 0.56) },
                misc: Vec4::new(0.0, samples as f32, scale, pond as u32 as f32),
                ..default()
            },
            heights,
            ripples,
        },
    }
}

/// The water follows the sky: its colours, the sun, and time for the ripples.
fn update_water(time: Res<Time>, sky: Option<Res<SkyNow>>, water: Query<&Water>, mut mats: ResMut<Assets<WaterMaterial>>) {
    let Some(sky) = sky else { return };
    for w in &water {
        if let Some(mut m) = mats.get_mut(&w.0) {
            let p = &mut m.extension.water;
            p.sun = sky.0.sun;
            p.zenith = sky.0.zenith;
            p.horizon = sky.0.horizon;
            p.sun_color = sky.0.sun_color;
            p.misc.x = time.elapsed_secs() % 3600.0;
        }
    }
}

/// Spawns a body of water.
pub fn spawn_water(commands: &mut Commands, mesh: Handle<Mesh>, mat: Handle<WaterMaterial>, tf: Transform) {
    commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat.clone()), Water(mat), tf, NotShadowCaster, DespawnOnExit(AppState::InGame)));
}
