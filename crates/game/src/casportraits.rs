//! Create a Sim's pictures of the household's Sims, as the game's CAS thumbnails (the puck's
//! buttons, the family screen, the genetics screen, the traits picker): each Sim built for a
//! moment out of sight, in their everyday clothes, and pictured by a camera that sees only them;
//! again whenever their looks change.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::AppState;
use crate::portraits::STUDIO_LAYER;
use crate::sim::Sim;
use crate::simbody::{SimModelPart, SimRenderCtx, SimSkinMaterial, SimTextures, Skeleton};

pub struct CasPortraitsPlugin;

impl Plugin for CasPortraitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CasPortraits>()
            .add_systems(OnEnter(AppState::CreateHousehold), spawn_studio)
            .add_systems(Update, take.run_if(in_state(AppState::CreateHousehold)))
            .add_systems(OnExit(AppState::CreateHousehold), |mut p: ResMut<CasPortraits>| p.busy = None);
    }
}

/// Each Sim's picture (by their id): the image, and the looks it was taken of (none: not yet).
#[derive(Resource, Default)]
pub struct CasPortraits {
    pics: HashMap<u64, (Handle<Image>, Option<u64>)>,
    /// Sims pictured who aren't in the household (yet): the genetics screen's child.
    pub extra: Vec<Sim>,
    /// When each Sim's looks last changed: pictures wait for them to settle (a slider dragged).
    changed: HashMap<u64, (u64, f32)>,
    busy: Option<Shot>,
    /// Pictures taken this time in Create a Sim (the first waits a moment longer, while the
    /// studio's shaders are readied).
    taken: u32,
}

impl CasPortraits {
    /// A Sim's picture, once it's been taken (the same image ever after: retaken in place).
    pub fn get(&self, sim: u64) -> Option<Handle<Image>> {
        self.pics.get(&sim).filter(|p| p.1.is_some()).map(|p| p.0.clone())
    }
}

/// A picture being taken: whose, of which looks, the body built for it, the frame counted
/// from, and the frame the camera was pointed.
struct Shot {
    sim: u64,
    looks: u64,
    model: Entity,
    started: f32,
    frame: u32,
    aimed: Option<u32>,
}

#[derive(Component)]
struct CasStudioCamera;

#[derive(Component)]
struct CasStudioLight;

/// The body built for a picture.
#[derive(Component)]
struct CasStudioModel;

/// Where the bodies are built: well under the dressing room's floor.
const UNDER: f32 = -60.0;

/// What a picture shows of a Sim: their age and sex, colouring, build, face and clothes.
fn looks_of(s: &Sim) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", ((s.look, s.female, s.age, s.skin, s.hair, s.eyes, s.top, s.bottom), (s.weight, s.fitness, &s.face, &s.face_presets, &s.outfit))).hash(&mut h);
    s.occult.map(|o| o.name()).hash(&mut h);
    h.finish()
}

fn spawn_studio(mut commands: Commands) {
    commands.spawn((
        CasStudioCamera,
        Camera3d::default(),
        Camera { order: -6, is_active: false, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        Projection::Perspective(PerspectiveProjection { fov: 24f32.to_radians(), near: 0.08, far: 2.5, ..default() }),
        AmbientLight { color: Color::srgb(0.9, 0.93, 1.0), brightness: 1500.0, ..default() },
        RenderLayers::layer(STUDIO_LAYER),
        DespawnOnExit(AppState::CreateHousehold),
    ));
    commands.spawn((
        CasStudioLight,
        PointLight { intensity: 40_000.0, range: 4.0, shadow_maps_enabled: false, ..default() },
        Transform::default(),
        RenderLayers::layer(STUDIO_LAYER),
        DespawnOnExit(AppState::CreateHousehold),
    ));
}

/// The household's pictures: the next Sim whose looks have changed (and settled) built out of
/// sight, the camera put in front of their face, the picture rendered, the body put away.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn take(
    mut commands: Commands,
    mut portraits: ResMut<CasPortraits>,
    pending: Res<crate::home::PendingHousehold>,
    scene: Option<Res<crate::cas::CasScene>>,
    time: Res<Time>,
    mut cam: Query<(Entity, &mut Camera, &mut Transform), With<CasStudioCamera>>,
    mut light: Query<&mut Transform, (With<CasStudioLight>, Without<CasStudioCamera>)>,
    (skeletons, joints, children, parts): (Query<&Skeleton, With<CasStudioModel>>, Query<&GlobalTransform, Without<Sim>>, Query<&Children>, Query<(), With<SimModelPart>>),
    (mut meshes, mut images, mut mats, mut skin_mats, mut bindposes, mut textures): (
        ResMut<Assets<Mesh>>,
        ResMut<Assets<Image>>,
        ResMut<Assets<StandardMaterial>>,
        ResMut<Assets<SimSkinMaterial>>,
        ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
        ResMut<SimTextures>,
    ),
) {
    let Some(scene) = scene else { return };
    let Ok((cam_e, mut camera, mut cam_tf)) = cam.single_mut() else { return };
    let now = time.elapsed_secs();
    let ids: Vec<u64> = pending.members.iter().chain(&portraits.extra).map(|m| m.id).collect();
    portraits.pics.retain(|id, _| ids.contains(id));
    portraits.changed.retain(|id, _| ids.contains(id));

    if let Some(mut shot) = portraits.busy.take() {
        shot.frame += 1;
        // (Seen only by the studio.)
        if shot.frame <= 2 {
            for c in children.get(shot.model).into_iter().flatten() {
                if parts.contains(*c) {
                    commands.entity(*c).insert(RenderLayers::layer(STUDIO_LAYER));
                }
            }
        }
        let hold = if portraits.taken == 0 { 1.0 } else { 0.15 };
        match shot.aimed {
            None if shot.frame >= 3 && now - shot.started >= hold => {
                let aim = skeletons.get(shot.model).ok().and_then(|skel| {
                    let head = skel.rig.bones.iter().position(|b| b.name == "b__Head__").and_then(|i| joints.get(skel.joints[i]).ok())?.translation();
                    let rot = crate::portraits::facing(skel, &joints, head, Quat::IDENTITY).unwrap_or(Quat::IDENTITY);
                    Some((head, rot))
                });
                let little = pending.members.iter().chain(&portraits.extra).find(|m| m.id == shot.sim).is_some_and(|m| m.age.is_little());
                let img = portraits.pics.get(&shot.sim).map(|p| p.0.clone());
                match (aim, img) {
                    (Some((head, rot)), Some(img)) => {
                        let (eye, face, _) = crate::portraits::eye_for(head, rot, little);
                        *cam_tf = Transform::from_translation(eye).looking_at(face, Vec3::Y);
                        if let Ok(mut l) = light.single_mut() {
                            *l = Transform::from_translation(eye + cam_tf.right().as_vec3() * 0.5 + Vec3::Y * 0.45);
                        }
                        commands.entity(cam_e).insert(RenderTarget::Image(img.into()));
                        camera.is_active = true;
                        shot.aimed = Some(shot.frame);
                    }
                    _ => {
                        // (No body to picture: tried again when their looks next change.)
                        commands.entity(shot.model).try_despawn();
                        portraits.changed.insert(shot.sim, (shot.looks, f32::MAX));
                        return;
                    }
                }
            }
            // The picture renders the frame the camera's pointed; then the camera rests and the
            // body goes.
            Some(at) if shot.frame >= at + 2 => {
                camera.is_active = false;
                commands.entity(shot.model).try_despawn();
                if let Some(p) = portraits.pics.get_mut(&shot.sim) {
                    p.1 = Some(shot.looks);
                }
                portraits.taken += 1;
                return;
            }
            _ => {}
        }
        portraits.busy = Some(shot);
        return;
    }

    // The next Sim whose picture is out of date, once their looks have stayed put a moment.
    let sims: Vec<Sim> = pending.members.iter().chain(&portraits.extra).cloned().collect();
    for m in &sims {
        let looks = looks_of(m);
        if portraits.pics.get(&m.id).is_some_and(|p| p.1 == Some(looks)) {
            continue;
        }
        let since = match portraits.changed.get(&m.id) {
            Some((l, t)) if *l == looks => now - t,
            _ => {
                portraits.changed.insert(m.id, (looks, now));
                0.0
            }
        };
        if since < 0.4 {
            continue;
        }
        let outfit = crate::simbody::pick_outfit_for(&scene.cas, m, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(m.look), crate::simbody::OutfitKind::Everyday);
        let Some(model) = crate::simbody::build_sim_model(&scene.baked, &scene.cas, m, &outfit, crate::simbody::tone_of(m)) else {
            portraits.changed.insert(m.id, (looks, f32::MAX));
            continue;
        };
        let root = commands.spawn((CasStudioModel, Transform::from_xyz(0.0, UNDER, 0.0), Visibility::default(), DespawnOnExit(AppState::CreateHousehold))).id();
        let mut ctx = SimRenderCtx { meshes: &mut meshes, images: &mut images, mats: &mut mats, skin_mats: &mut skin_mats, bindposes: &mut bindposes, textures: &mut textures };
        crate::simbody::spawn_sim_model(&mut commands, root, model, &mut ctx);
        if !portraits.pics.contains_key(&m.id) {
            let img = crate::portraits::blank(&mut images);
            portraits.pics.insert(m.id, (img, None));
        }
        portraits.busy = Some(Shot { sim: m.id, looks, model: root, started: now, frame: 0, aimed: None });
        break;
    }
}
