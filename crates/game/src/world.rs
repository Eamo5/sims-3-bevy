//! The world's own content from the baked cache: objects placed by the town designers
//! (street lights, rocks, decorations), pre-built lot imposters (house shells), and trees.

use std::collections::{HashMap, HashSet};

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::VisibilityRange;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use s3bake::{InstanceBaked, Key, TreeBaked, TreeKindBaked, WorldBaked};

use crate::AppState;
use crate::baked::{Baked, BakedData};
use crate::objects::{AssetCtx, CpuPart, ObjectAssets, cpu_model, cpu_texture, spawn_parts};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/tree.wgsl");
        app.add_plugins(MaterialPlugin::<TreeMaterial>::default()).add_systems(OnEnter(AppState::InGame), spawn_world_content);
    }
}

/// A tree drawn from its 360° billboard: the picture of it from the side it's seen from, turned
/// to face the view (see `shaders/tree.wgsl`).
pub type TreeMaterial = ExtendedMaterial<StandardMaterial, TreeExt>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TreeExt {
    #[uniform(100)]
    pub billboard: TreeBillboard,
}

#[derive(ShaderType, Reflect, Debug, Clone)]
pub struct TreeBillboard {
    /// The views round the tree, in order (uv rectangles).
    pub views: [Vec4; 16],
    /// x: how many views, y: the atlas's width / height, z: the tree's height.
    pub params: Vec4,
}

impl MaterialExtension for TreeExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://sims3/shaders/tree.wgsl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://sims3/shaders/tree.wgsl".into()
    }
}

/// The quad a 360° billboard is drawn on (the shader places and sizes it), and the room it
/// takes up whichever way it's turned.
fn billboard_quad(k: &TreeKindBaked) -> (Mesh, Aabb) {
    let h = k.height.max(0.3);
    let w = k.views.iter().map(|v| h * (v[2] - v[0]) * k.atlas_aspect / (v[3] - v[1]).max(1e-3)).fold(0.5, f32::max);
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-w * 0.5, 0.0, 0.0], [w * 0.5, 0.0, 0.0], [w * 0.5, h, 0.0], [-w * 0.5, h, 0.0]]);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
    m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    // (Leaning back towards the view, its top can reach out as far as it's tall.)
    let r = (w * 0.5).max(h);
    (m, Aabb::from_min_max(Vec3::new(-r, 0.0, -r), Vec3::new(r, h * 1.05, r)))
}

/// World content decoded on the loading thread.
#[derive(Resource, Default)]
pub struct WorldBuild {
    pub models: Vec<(Key, Vec<CpuPart>)>,
    pub textures: Vec<(Key, Option<Image>)>,
    pub instances: Vec<InstanceBaked>,
    pub trees: Vec<TreeBaked>,
    pub tree_kinds: Vec<TreeKindBaked>,
}

/// A world tree (stand-in for a SpeedTree).
#[derive(Component)]
pub struct Tree;

/// Marks the pre-built shell of a lot, hidden when the household moves onto it.
#[derive(Component)]
pub struct LotImposter(pub usize);

/// Runs `f` over `items` on all CPU cores.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    s3bake::bake::par_map(items, f)
}

pub fn build_world(baked: &BakedData, world: &WorldBaked) -> WorldBuild {
    let keys: Vec<Key> = world.instances.iter().map(|i| i.model).collect::<HashSet<_>>().into_iter().collect();
    let models: Vec<(Key, Vec<CpuPart>)> =
        par_map(&keys, |k| (*k, baked.model(k).map(cpu_model).unwrap_or_default()));
    let tex_keys: Vec<Key> = models
        .iter()
        .flat_map(|(_, p)| p.iter().filter_map(|x| x.tex))
        .chain(world.tree_kinds.iter().map(|k| k.billboard))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let textures = par_map(&tex_keys, |k| (*k, cpu_texture(baked, *k)));
    WorldBuild {
        models,
        textures,
        instances: world.instances.clone(),
        trees: world.trees.clone(),
        tree_kinds: world.tree_kinds.clone(),
    }
}

/// Crossed billboards of a SpeedTree species: two of its pictures at right angles (three for
/// wide crowns), standing on the ground, sized from the tree's own dimensions.
fn tree_mesh(k: &TreeKindBaked) -> Mesh {
    let (mut pos, mut nrm, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let quads = if k.views.len() >= 3 { 3 } else { 2 };
    for q in 0..quads {
        let v = k.views[q % k.views.len()];
        let aspect = (v[2] - v[0]) * k.atlas_aspect / (v[3] - v[1]).max(1e-3);
        let (h, w) = (k.height.max(0.3), k.height.max(0.3) * aspect);
        let angle = q as f32 * std::f32::consts::PI / quads as f32;
        let (s, c) = angle.sin_cos();
        let right = Vec3::new(c, 0.0, -s) * (w * 0.5);
        let base = pos.len() as u32;
        for (p, t) in [(-right, [v[0], v[3]]), (right, [v[2], v[3]]), (right + Vec3::Y * h, [v[2], v[1]]), (-right + Vec3::Y * h, [v[0], v[1]])] {
            pos.push(p.to_array());
            // Lit as a whole crown from above, so every side looks alike.
            nrm.push([0.0, 1.0, 0.0]);
            uv.push(t);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(idx));
    m
}

fn quat(q: [f32; 4]) -> Quat {
    let q = Quat::from_xyzw(q[0], q[1], q[2], q[3]);
    if q.length_squared() < 1e-6 { Quat::IDENTITY } else { q.normalize() }
}

fn spawn_world_content(
    mut commands: Commands,
    build: Option<ResMut<WorldBuild>>,
    baked: Res<Baked>,
    mut assets: ResMut<ObjectAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut tree_mats: ResMut<Assets<TreeMaterial>>,
) {
    let Some(mut build) = build else { return };
    for (k, img) in build.textures.drain(..) {
        assets.ingest_texture(&mut images, k, img);
    }
    let mut ctx = AssetCtx { baked: &baked.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let mut parts_of = HashMap::new();
    for (k, cpu) in build.models.drain(..) {
        let parts = assets.ingest_model(&mut ctx, k, cpu);
        parts_of.insert(k, parts);
    }
    let mut spawned = 0;
    for inst in build.instances.drain(..) {
        let Some(parts) = parts_of.get(&inst.model) else { continue };
        if parts.is_empty() {
            continue;
        }
        let tf = Transform::from_translation(Vec3::from(inst.position)).with_rotation(quat(inst.rotation));
        let e = spawn_parts(&mut commands, parts, tf);
        commands.entity(e).insert(DespawnOnExit(AppState::InGame));
        if let Some(l) = inst.lot {
            commands.entity(e).insert(LotImposter(l as usize));
        } else {
            // Small props fade out at a distance to keep the frame rate up.
            commands.entity(e).insert(VisibilityRange::abrupt(0.0, 450.0));
        }
        spawned += 1;
    }

    // Trees: each species from its billboard pictures (a tree's from all the way round, turned
    // to the view; shrubs' and flowers' leaf cards crossed); stand-ins when a species has none.
    let mut kinds: HashMap<u64, (Handle<Mesh>, Handle<StandardMaterial>)> = HashMap::new();
    let mut billboards: HashMap<u64, (Handle<Mesh>, Handle<TreeMaterial>, Aabb)> = HashMap::new();
    for k in build.tree_kinds.drain(..) {
        let Some(tex) = assets.texture(&mut AssetCtx { baked: &baked.0, meshes: &mut meshes, images: &mut images, materials: &mut mats }, k.billboard)
        else {
            continue;
        };
        let base = StandardMaterial {
            base_color_texture: Some(tex),
            alpha_mode: AlphaMode::Mask(0.45),
            double_sided: true,
            cull_mode: None,
            perceptual_roughness: 0.95,
            reflectance: 0.1,
            ..default()
        };
        if k.round && k.views.len() >= 3 {
            let mut views = [Vec4::ZERO; 16];
            for (v, r) in views.iter_mut().zip(&k.views) {
                *v = Vec4::from(*r);
            }
            let billboard = TreeBillboard { views, params: Vec4::new(k.views.len().min(16) as f32, k.atlas_aspect, k.height.max(0.3), 0.0) };
            let (mesh, aabb) = billboard_quad(&k);
            billboards.insert(k.kind, (meshes.add(mesh), tree_mats.add(TreeMaterial { base, extension: TreeExt { billboard } }), aabb));
            continue;
        }
        kinds.insert(k.kind, (meshes.add(tree_mesh(&k)), mats.add(base)));
    }
    let trunk = meshes.add(Cylinder::new(0.22, 4.0));
    let round = meshes.add(Sphere::new(2.6).mesh().ico(2).unwrap());
    let cone = meshes.add(Cone { radius: 2.3, height: 7.0 });
    let bark = mats.add(StandardMaterial { base_color: Color::srgb(0.33, 0.24, 0.16), perceptual_roughness: 0.95, ..default() });
    let leaves = [
        mats.add(StandardMaterial { base_color: Color::srgb(0.20, 0.38, 0.14), perceptual_roughness: 0.9, ..default() }),
        mats.add(StandardMaterial { base_color: Color::srgb(0.14, 0.30, 0.16), perceptual_roughness: 0.9, ..default() }),
        mats.add(StandardMaterial { base_color: Color::srgb(0.28, 0.42, 0.16), perceptual_roughness: 0.9, ..default() }),
    ];
    let n_trees = build.trees.len();
    for t in build.trees.drain(..) {
        if let Some((mesh, mat, aabb)) = billboards.get(&t.kind) {
            let tf = Transform::from_translation(Vec3::from(t.position))
                .with_rotation(quat(t.rotation))
                .with_scale(Vec3::splat(t.scale));
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                tf,
                *aabb,
                Tree,
                DespawnOnExit(AppState::InGame),
                VisibilityRange::abrupt(0.0, 900.0),
            ));
            continue;
        }
        if let Some((mesh, mat)) = kinds.get(&t.kind) {
            let tf = Transform::from_translation(Vec3::from(t.position))
                .with_rotation(quat(t.rotation))
                .with_scale(Vec3::splat(t.scale));
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                tf,
                Tree,
                DespawnOnExit(AppState::InGame),
                VisibilityRange::abrupt(0.0, 900.0),
            ));
            continue;
        }
        let conifer = t.kind % 3 == 0;
        let leaf = leaves[(t.kind as usize / 3) % 3].clone();
        let tf = Transform::from_translation(Vec3::from(t.position))
            .with_rotation(quat(t.rotation))
            .with_scale(Vec3::splat(t.scale));
        commands
            .spawn((tf, Visibility::default(), Tree, DespawnOnExit(AppState::InGame), VisibilityRange::abrupt(0.0, 900.0)))
            .with_children(|c| {
                c.spawn((Mesh3d(trunk.clone()), MeshMaterial3d(bark.clone()), Transform::from_xyz(0.0, 2.0, 0.0)));
                if conifer {
                    c.spawn((Mesh3d(cone.clone()), MeshMaterial3d(leaf), Transform::from_xyz(0.0, 6.0, 0.0)));
                } else {
                    c.spawn((
                        Mesh3d(round.clone()),
                        MeshMaterial3d(leaf),
                        Transform::from_xyz(0.0, 5.2, 0.0).with_scale(Vec3::new(1.0, 0.85, 1.0)),
                    ));
                }
            });
    }
    commands.remove_resource::<WorldBuild>();
    info!("world: spawned {spawned} objects/imposters and {n_trees} trees");
}
