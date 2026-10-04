//! The world's own content from the baked cache: objects placed by the town designers
//! (street lights, rocks, decorations), pre-built lot imposters (house shells), and trees.

use std::collections::{HashMap, HashSet};

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use s3bake::{InstanceBaked, Key, TreeBaked, WorldBaked};

use crate::AppState;
use crate::baked::{Baked, BakedData};
use crate::objects::{AssetCtx, CpuPart, ObjectAssets, cpu_model, cpu_texture, spawn_parts};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_world_content);
    }
}

/// World content decoded on the loading thread.
#[derive(Resource, Default)]
pub struct WorldBuild {
    pub models: Vec<(Key, Vec<CpuPart>)>,
    pub textures: Vec<(Key, Option<Image>)>,
    pub instances: Vec<InstanceBaked>,
    pub trees: Vec<TreeBaked>,
}

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
    let tex_keys: Vec<Key> =
        models.iter().flat_map(|(_, p)| p.iter().filter_map(|x| x.tex)).collect::<HashSet<_>>().into_iter().collect();
    let textures = par_map(&tex_keys, |k| (*k, cpu_texture(baked, *k)));
    WorldBuild { models, textures, instances: world.instances.clone(), trees: world.trees.clone() }
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

    // Trees: stand-ins for SpeedTree models (trunk + crown), varied per species.
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
        let conifer = t.kind % 3 == 0;
        let leaf = leaves[(t.kind as usize / 3) % 3].clone();
        let tf = Transform::from_translation(Vec3::from(t.position))
            .with_rotation(quat(t.rotation))
            .with_scale(Vec3::splat(t.scale));
        commands
            .spawn((tf, Visibility::default(), DespawnOnExit(AppState::InGame), VisibilityRange::abrupt(0.0, 900.0)))
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
