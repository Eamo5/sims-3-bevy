//! The world's own content: objects placed by the town designers (street lights, rocks,
//! decorations, rabbit holes), pre-built lot imposters (house shells), and trees.

use std::collections::{HashMap, HashSet};

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use s3formats::objn::{PlacedObject, load_world_objects};
use s3formats::world::WorldData;
use s3pkg::{Package, PackageSet, ResourceKey, types};

use crate::AppState;
use crate::data::GameData;
use crate::objects::{AssetCtx, CpuPart, ObjectAssets, build_model_cpu, build_texture_cpu, spawn_parts};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_world_content);
    }
}

pub const IMPOSTER_GROUP: u32 = 0x00B0C507;

pub struct WorldInstance {
    pub modl: ResourceKey,
    pub transform: Transform,
    pub lot: Option<usize>,
}

pub struct TreeInstance {
    pub transform: Transform,
    pub kind: u64,
}

/// World content decoded on the loading thread.
#[derive(Resource, Default)]
pub struct WorldBuild {
    pub models: Vec<(ResourceKey, Vec<CpuPart>)>,
    pub textures: Vec<(ResourceKey, Option<Image>)>,
    pub instances: Vec<WorldInstance>,
    pub trees: Vec<TreeInstance>,
}

/// Marks the pre-built shell of a lot, hidden when the household moves onto it.
#[derive(Component)]
pub struct LotImposter(pub usize);

fn quat(q: [f32; 4]) -> Quat {
    let q = Quat::from_xyzw(q[0], q[1], q[2], q[3]);
    if q.length_squared() < 1e-6 { Quat::IDENTITY } else { q.normalize() }
}

fn models_of(pkgs: &PackageSet, o: &PlacedObject) -> Vec<ResourceKey> {
    if let Some(m) = o.model {
        return vec![m];
    }
    if let Some(v) = o.vpxy
        && v.i != 0
        && let Some(d) = pkgs.read(&v).or_else(|| pkgs.read_ti(v.t, v.i))
    {
        return s3formats::model::vpxy_models(&d);
    }
    Vec::new()
}

/// Runs `f` over `items` on all CPU cores.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items.chunks(chunk).map(|c| s.spawn(|| c.iter().map(&f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

pub fn build_world(pkgs: &PackageSet, world_pkg: &Package, world: &WorldData, progress: &dyn Fn(&str)) -> WorldBuild {
    progress("Reading placed objects…");
    let placed = load_world_objects(world_pkg);
    let lot_ids: HashMap<u64, usize> = world.lots.iter().enumerate().map(|(i, l)| (l.id, i)).collect();

    let mut instances = Vec::new();
    let mut trees = Vec::new();
    for (owner, objs) in &placed {
        let on_lot = lot_ids.contains_key(owner);
        for o in objs {
            for t in &o.trees {
                // Stored row-major with the position in row 3, i.e. column-major for glam.
                let m = Mat4::from_cols_array(&t.matrix);
                let (_, rot, pos) = m.to_scale_rotation_translation();
                trees.push(TreeInstance {
                    transform: Transform::from_translation(pos).with_rotation(rot).with_scale(Vec3::splat(t.scale.max(0.2))),
                    kind: o.speedtree.map(|k| k.i).unwrap_or(0),
                });
            }
            // Lots are represented by their imposters; only world-layer objects are placed here.
            if on_lot {
                continue;
            }
            let Some(p) = o.position else { continue };
            for modl in models_of(pkgs, o) {
                instances.push(WorldInstance {
                    modl,
                    transform: Transform::from_translation(Vec3::from(p)).with_rotation(quat(o.rotation)),
                    lot: None,
                });
            }
        }
    }
    for (i, lot) in world.lots.iter().enumerate() {
        let key = ResourceKey::new(types::MODL, IMPOSTER_GROUP, lot.id);
        if pkgs.get_entry(&key).is_some() {
            instances.push(WorldInstance {
                modl: key,
                transform: Transform::from_translation(Vec3::from(lot.corner)).with_rotation(Quat::from_rotation_y(lot.rotation)),
                lot: Some(i),
            });
        }
    }

    let distinct: Vec<ResourceKey> = instances.iter().map(|i| i.modl).collect::<HashSet<_>>().into_iter().collect();
    progress(&format!("Decoding {} world models…", distinct.len()));
    let models: Vec<(ResourceKey, Vec<CpuPart>)> = par_map(&distinct, |k| (*k, build_model_cpu(pkgs, *k)));
    let tex_keys: Vec<ResourceKey> =
        models.iter().flat_map(|(_, parts)| parts.iter().filter_map(|p| p.tex)).collect::<HashSet<_>>().into_iter().collect();
    progress(&format!("Compositing {} world textures…", tex_keys.len()));
    let textures = par_map(&tex_keys, |k| (*k, build_texture_cpu(pkgs, *k, 256)));
    WorldBuild { models, textures, instances, trees }
}

#[derive(Resource)]
pub struct TreeAssets {
    pub trunk: Handle<Mesh>,
    pub crowns: [Handle<Mesh>; 2],
    pub bark: Handle<StandardMaterial>,
    pub leaves: [Handle<StandardMaterial>; 3],
}

fn spawn_world_content(
    mut commands: Commands,
    build: Option<ResMut<WorldBuild>>,
    data: Res<GameData>,
    mut assets: ResMut<ObjectAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mut build) = build else { return };
    for (k, img) in build.textures.drain(..) {
        assets.ingest_texture(&mut images, k, img);
    }
    let mut ctx = AssetCtx { pkgs: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let mut parts_of = HashMap::new();
    for (k, cpu) in build.models.drain(..) {
        let parts = assets.ingest_model(&mut ctx, k, cpu);
        parts_of.insert(k, parts);
    }
    let mut spawned = 0;
    for inst in build.instances.drain(..) {
        let Some(parts) = parts_of.get(&inst.modl) else { continue };
        if parts.is_empty() {
            continue;
        }
        let e = if inst.lot.is_some() {
            // Imposter textures are pre-rendered images of the lot: draw them unlit.
            let unlit: Vec<_> = parts
                .iter()
                .map(|p| {
                    let mut p = p.clone();
                    if let Some(m) = mats.get(&p.material) {
                        let mut m = m.clone();
                        m.unlit = true;
                        p.material = mats.add(m);
                    }
                    p
                })
                .collect();
            spawn_parts(&mut commands, &unlit, inst.transform)
        } else {
            spawn_parts(&mut commands, parts, inst.transform)
        };
        commands.entity(e).insert(DespawnOnExit(AppState::InGame));
        if let Some(l) = inst.lot {
            commands.entity(e).insert(LotImposter(l));
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
        commands
            .spawn((t.transform, Visibility::default(), DespawnOnExit(AppState::InGame), VisibilityRange::abrupt(0.0, 900.0)))
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
    commands.insert_resource(TreeAssets { trunk, crowns: [round, cone], bark, leaves });
    commands.remove_resource::<WorldBuild>();
    info!("world: spawned {spawned} objects/imposters and {n_trees} trees");
}
