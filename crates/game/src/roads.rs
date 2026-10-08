//! Roads, sidewalks and intersections: world-space meshes from the baked cache, drawn with a
//! material that layers the overlay (tire tracks, crosswalks, curb corners) and the edge fade
//! over the tiled base texture.

use std::collections::{HashMap, HashSet};

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use s3bake::{Key, WorldBaked};

use crate::AppState;
use crate::baked::BakedData;
use crate::objects::cpu_texture;

pub type RoadMaterial = ExtendedMaterial<StandardMaterial, RoadExt>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct RoadExt {
    /// x: overlay present, y: opacity present, z: snow on it, w: how wet (the weather's).
    #[uniform(100)]
    pub params: Vec4,
    #[texture(101)]
    #[sampler(102)]
    pub overlay: Option<Handle<Image>>,
    #[texture(103)]
    #[sampler(104)]
    pub opacity: Option<Handle<Image>>,
}

impl MaterialExtension for RoadExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://sims3/shaders/road.wgsl".into()
    }
}

pub struct RoadPlugin;

impl Plugin for RoadPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/road.wgsl");
        app.add_plugins(MaterialPlugin::<RoadMaterial>::default())
            .add_systems(OnEnter(AppState::InGame), spawn_roads);
    }
}

type Textures = (Option<Key>, Option<Key>, Option<Key>);

/// Road meshes and their textures, decoded on the loading thread.
#[derive(Resource, Default)]
pub struct RoadBuild {
    parts: Vec<(Mesh, Textures)>,
    textures: Vec<(Key, Option<Image>)>,
}

/// Roads sit a few centimetres above the terrain; lift them a little more so the coarser
/// terrain triangles between road vertices never poke through.
const LIFT: f32 = 0.04;

pub fn build_roads(baked: &BakedData, world: &WorldBaked) -> RoadBuild {
    let parts = world
        .roads
        .iter()
        .map(|r| {
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
            let pos: Vec<[f32; 3]> = r.positions.iter().map(|p| [p[0], p[1] + LIFT, p[2]]).collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, r.normals.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, r.uvs.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, r.uvs1.clone());
            mesh.insert_indices(Indices::U32(r.indices.clone()));
            (mesh, (r.base, r.overlay, r.opacity))
        })
        .collect();
    let keys: Vec<Key> = world
        .roads
        .iter()
        .flat_map(|r| [r.base, r.overlay, r.opacity])
        .flatten()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let textures = crate::world::par_map(&keys, |k| (*k, cpu_texture(baked, *k)));
    RoadBuild { parts, textures }
}

fn spawn_roads(
    mut commands: Commands,
    build: Option<ResMut<RoadBuild>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<RoadMaterial>>,
) {
    let Some(mut build) = build else { return };
    let tex: HashMap<Key, Handle<Image>> =
        build.textures.drain(..).filter_map(|(k, img)| Some((k, images.add(img?)))).collect();
    let mut cache: HashMap<Textures, Handle<RoadMaterial>> = HashMap::new();
    let n = build.parts.len();
    for (mesh, keys) in build.parts.drain(..) {
        let material = cache
            .entry(keys)
            .or_insert_with(|| {
                let get = |k: Option<Key>| k.and_then(|k| tex.get(&k).cloned());
                let (base, overlay, opacity) = (get(keys.0), get(keys.1), get(keys.2));
                let params = Vec4::new(overlay.is_some() as u32 as f32, opacity.is_some() as u32 as f32, 0.0, 0.0);
                mats.add(RoadMaterial {
                    base: StandardMaterial {
                        base_color_texture: base,
                        perceptual_roughness: 0.92,
                        reflectance: 0.2,
                        alpha_mode: if opacity.is_some() { AlphaMode::Blend } else { AlphaMode::Opaque },
                        depth_bias: 40.0,
                        ..default()
                    },
                    extension: RoadExt { params, overlay, opacity },
                })
            })
            .clone();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), DespawnOnExit(AppState::InGame)));
    }
    commands.remove_resource::<RoadBuild>();
    info!("roads: spawned {n} road meshes");
}
