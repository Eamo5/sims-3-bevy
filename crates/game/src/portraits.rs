//! Sim portraits, like the game's household thumbnails: a small studio camera looks each Sim in
//! the face for a moment and renders into that Sim's picture, in front of a backdrop card and
//! under a light that only the studio camera sees. Pictures are taken when first asked for and
//! again whenever the Sim's body is rebuilt (new clothes, a birthday).

use std::collections::{HashMap, VecDeque};

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::AppState;
use crate::sim::{Age, Pose, Sim, SimAnim};
use crate::simbody::Skeleton;

pub struct PortraitsPlugin;

impl Plugin for PortraitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Portraits>()
            .add_systems(OnEnter(AppState::InGame), spawn_studio)
            .add_systems(OnExit(AppState::InGame), forget)
            .add_systems(Update, (retake_rebuilt, take_portraits).chain().run_if(in_state(AppState::InGame)));
    }
}

/// The studio's own light and camera layer.
const STUDIO_LAYER: usize = 7;
const SIZE: u32 = 128;
/// The portraits' backdrop: the game's soft blue, lighter towards the top.
const BACKDROP: Color = Color::srgb(0.56, 0.72, 0.88);
const BACKDROP_TOP: [u8; 3] = [206, 226, 244];
const BACKDROP_BOTTOM: [u8; 3] = [112, 150, 196];

/// Each Sim's portrait and the ones waiting to be taken.
#[derive(Resource, Default)]
pub struct Portraits {
    images: HashMap<Entity, Handle<Image>>,
    queue: VecDeque<Entity>,
    /// The picture being taken.
    busy: Option<Shot>,
    /// When each Sim's body was (re)built: their skin takes a moment to be ready.
    built: HashMap<Entity, f32>,
    /// When each picture is due to be taken again (soon after the first, in case the face was
    /// still loading, then now and then).
    retake: HashMap<Entity, (f32, u32)>,
}

impl Portraits {
    /// A Sim's portrait (blank until it has been taken).
    pub fn portrait(&mut self, images: &mut Assets<Image>, sim: Entity) -> Handle<Image> {
        if let Some(h) = self.images.get(&sim) {
            return h.clone();
        }
        let h = images.add(Image::new_target_texture(SIZE, SIZE, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
        self.images.insert(sim, h.clone());
        self.queue.push_back(sim);
        h
    }
}

/// A picture in progress: Sims who are away (at work, off the lot) are brought to a spot
/// deep under the world for it, visible for a moment, and put back afterwards.
struct Shot {
    sim: Entity,
    frame: u8,
    /// Where the Sim was and their visibility, when staged.
    staged: Option<(Transform, Visibility)>,
    /// The frame the camera was pointed.
    aimed: Option<u8>,
}

/// How far under the world away Sims are photographed.
const UNDERGROUND: f32 = 400.0;

#[derive(Component)]
struct StudioCamera;

/// The card behind the Sim's head.
#[derive(Component)]
struct Backdrop;

fn spawn_studio(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let gradient: Vec<u8> = (0..32u32)
        .flat_map(|y| {
            let t = y as f32 / 31.0;
            let c = |i: usize| (BACKDROP_TOP[i] as f32 * (1.0 - t) + BACKDROP_BOTTOM[i] as f32 * t) as u8;
            [c(0), c(1), c(2), 255]
        })
        .collect();
    let tex = images.add(Image::new(
        Extent3d { width: 1, height: 32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        gradient,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));
    commands.spawn((
        Backdrop,
        Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial3d(mats.add(StandardMaterial { base_color_texture: Some(tex), unlit: true, ..default() })),
        Transform::default(),
        RenderLayers::layer(STUDIO_LAYER),
        bevy::light::NotShadowCaster,
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        StudioCamera,
        Camera3d::default(),
        Camera { order: -5, is_active: false, clear_color: ClearColorConfig::Custom(BACKDROP), ..default() },
        Projection::Perspective(PerspectiveProjection { fov: 24f32.to_radians(), near: 0.08, far: 1.0, ..default() }),
        AmbientLight { color: Color::srgb(0.9, 0.93, 1.0), brightness: 1500.0, ..default() },
        RenderLayers::from_layers(&[0, STUDIO_LAYER]),
        DespawnOnExit(AppState::InGame),
    ));
}

fn forget(mut portraits: ResMut<Portraits>) {
    *portraits = Portraits::default();
}

/// A rebuilt body gets a new picture; Sims who have left the world lose theirs.
fn retake_rebuilt(mut portraits: ResMut<Portraits>, rebuilt: Query<Entity, Changed<Skeleton>>, alive: Query<(), With<Sim>>, time: Res<Time>) {
    portraits.images.retain(|e, _| alive.contains(*e));
    portraits.queue.retain(|e| alive.contains(*e));
    portraits.built.retain(|e, _| alive.contains(*e));
    portraits.retake.retain(|e, _| alive.contains(*e));
    let now = time.elapsed_secs();
    let due: Vec<Entity> = portraits.retake.iter().filter(|(_, (t, _))| now >= *t).map(|(e, _)| *e).collect();
    for e in due {
        if let Some(r) = portraits.retake.get_mut(&e) {
            r.0 = f32::MAX;
        }
        if !portraits.queue.contains(&e) {
            portraits.queue.push_back(e);
        }
    }
    for e in &rebuilt {
        portraits.built.insert(e, time.elapsed_secs());
        if portraits.images.contains_key(&e) && !portraits.queue.contains(&e) {
            portraits.queue.push_back(e);
        }
    }
}

#[allow(clippy::type_complexity)]
fn take_portraits(
    mut commands: Commands,
    mut portraits: ResMut<Portraits>,
    mut cam: Query<(Entity, &mut Camera, &mut Transform), With<StudioCamera>>,
    mut backdrop: Query<&mut Transform, (With<Backdrop>, Without<StudioCamera>)>,
    sims: Query<(&Sim, &SimAnim, &GlobalTransform, &Skeleton, &InheritedVisibility)>,
    placed: Query<
        (&Transform, &Visibility, Has<crate::interact::OffLot>, Has<crate::interact::AtWork>, Has<crate::rabbitholes::AtRabbitHole>),
        (With<Sim>, Without<StudioCamera>, Without<Backdrop>),
    >,
    joints: Query<&GlobalTransform, Without<Sim>>,
    mut light: Local<Option<Entity>>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    let Ok((cam_e, mut camera, mut cam_tf)) = cam.single_mut() else { return };
    // The light that only the studio sees.
    let light_e = *light.get_or_insert_with(|| {
        commands
            .spawn((
                PointLight { intensity: 40_000.0, range: 4.0, shadow_maps_enabled: false, ..default() },
                Transform::default(),
                RenderLayers::layer(STUDIO_LAYER),
                DespawnOnExit(AppState::InGame),
            ))
            .id()
    });

    if let Some(mut shot) = portraits.busy.take() {
        shot.frame += 1;
        let mut done = false;
        match shot.aimed {
            // A staged Sim's pose settles under the world first.
            None if shot.staged.is_none() || shot.frame >= 3 => {
                let e = shot.sim;
                let aim = sims.get(e).ok().and_then(|(sim, _, tf, skel, _)| {
                    let head = skel.rig.bones.iter().position(|b| b.name == "b__Head__").and_then(|i| joints.get(skel.joints[i]).ok())?;
                    Some((sim.age.is_little(), head.translation(), tf.rotation()))
                });
                let img = portraits.images.get(&e).cloned();
                match (aim, img) {
                    (Some((little, head, rot)), Some(img)) => {
                        let face = head + Vec3::Y * if little { 0.02 } else { 0.05 };
                        let ahead = (rot * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
                        let dist = if little { 0.5 } else { 0.72 };
                        let eye = face + ahead * dist + Vec3::Y * 0.03;
                        *cam_tf = Transform::from_translation(eye).looking_at(face, Vec3::Y);
                        let right = cam_tf.right().as_vec3();
                        commands.entity(light_e).insert(Transform::from_translation(eye + right * 0.5 + Vec3::Y * 0.45));
                        // The card a little behind the head, filling the picture.
                        if let Ok(mut b) = backdrop.single_mut() {
                            let behind = dist + 0.3;
                            let size = 2.0 * behind * (12f32.to_radians()).tan() * 1.3;
                            let at = eye + (face - eye).normalize() * behind;
                            // (Turned so its front, +Z, faces the camera.)
                            *b = Transform::from_translation(at).looking_to(at - eye, Vec3::Y).with_scale(Vec3::splat(size));
                        }
                        commands.entity(cam_e).insert((
                            RenderTarget::Image(img.into()),
                            Projection::Perspective(PerspectiveProjection { fov: 24f32.to_radians(), near: 0.08, far: dist + 0.28, ..default() }),
                        ));
                        camera.is_active = true;
                        shot.aimed = Some(shot.frame);
                    }
                    _ => shot.aimed = Some(0),
                }
            }
            // The picture renders the frame after the camera is pointed; then the camera
            // rests and a staged Sim goes back.
            Some(at) if shot.frame >= at + 2 => {
                camera.is_active = false;
                if let Some((tf, vis)) = shot.staged {
                    commands.entity(shot.sim).insert((tf, vis));
                }
                let taken = portraits.retake.get(&shot.sim).map_or(0, |r| r.1) + 1;
                portraits.retake.insert(shot.sim, (now + if taken < 2 { 12.0 } else { 240.0 }, taken));
                done = true;
            }
            _ => {}
        }
        if !done {
            portraits.busy = Some(shot);
        }
        return;
    }

    // The next Sim ready for their picture (others wait their turn).
    for _ in 0..portraits.queue.len() {
        let Some(e) = portraits.queue.pop_front() else { break };
        let (Ok((sim, anim, _, _, vis)), Ok((tf, visibility, off_lot, at_work, out))) = (sims.get(e), placed.get(e)) else {
            // (The body isn't built yet.)
            portraits.queue.push_back(e);
            continue;
        };
        let away = off_lot || at_work || out;
        let upright = matches!(anim.pose, Pose::Stand | Pose::Walk | Pose::Talk | Pose::Use) || sim.age == Age::Baby;
        // (Faces and clothes finish loading a few seconds after a body is built.)
        let settled = portraits.built.get(&e).is_none_or(|t| now - t > 4.0) && now > 6.0;
        if !settled || !(vis.get() && upright || away) {
            portraits.queue.push_back(e);
            continue;
        }
        let staged = (!vis.get()).then(|| (*tf, *visibility));
        if let Some((tf, _)) = staged {
            commands.entity(e).insert((Transform { translation: tf.translation - Vec3::Y * UNDERGROUND, ..tf }, Visibility::Visible));
        }
        portraits.busy = Some(Shot { sim: e, frame: 0, staged, aimed: None });
        break;
    }
}
