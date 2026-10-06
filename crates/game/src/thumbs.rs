//! Pictures of objects for the interface, rendered as the game renders its ingredient and
//! inventory thumbnails: a studio camera looks at each model once (from above and in front,
//! filling the picture) under a light only it sees, deep under the world, and renders into
//! the picture the interface is already showing.

use std::collections::{HashMap, VecDeque};

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use s3bake::Key;

use crate::AppState;
use crate::baked::Baked;
use crate::objects::{AssetCtx, ObjectAssets};

pub struct ThumbsPlugin;

impl Plugin for ThumbsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModelThumbs>()
            .add_systems(OnEnter(AppState::InGame), spawn_studio)
            .add_systems(OnExit(AppState::InGame), |mut t: ResMut<ModelThumbs>| *t = ModelThumbs::default())
            .add_systems(Update, take_thumbs.run_if(in_state(AppState::InGame)));
    }
}

/// The studio's camera layer.
const THUMB_LAYER: usize = 8;
const SIZE: u32 = 96;
/// Where models are put for their picture.
const STAGE: Vec3 = Vec3::new(0.0, -900.0, 0.0);
const FOV: f32 = 30.0;

/// Each model's picture, and those still to take.
#[derive(Resource, Default)]
pub struct ModelThumbs {
    images: HashMap<Key, Handle<Image>>,
    queue: VecDeque<Key>,
    /// The model on stage and how many frames it's been there.
    busy: Option<(Entity, u8)>,
}

impl ModelThumbs {
    /// A model's picture (clear until it's been taken).
    pub fn get(&mut self, images: &mut Assets<Image>, model: Key) -> Handle<Image> {
        if let Some(h) = self.images.get(&model) {
            return h.clone();
        }
        let mut img = Image::new_target_texture(SIZE, SIZE, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb));
        img.data = Some(vec![0; (SIZE * SIZE * 4) as usize]);
        let h = images.add(img);
        self.images.insert(model, h.clone());
        self.queue.push_back(model);
        h
    }
}

#[derive(Component)]
struct ThumbCamera;

fn spawn_studio(mut commands: Commands) {
    commands.spawn((
        ThumbCamera,
        Camera3d::default(),
        Camera { order: -6, is_active: false, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        Projection::Perspective(PerspectiveProjection { fov: FOV.to_radians(), ..default() }),
        AmbientLight { color: Color::srgb(0.95, 0.95, 1.0), brightness: 900.0, ..default() },
        RenderLayers::layer(THUMB_LAYER),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(1.0, 2.0, 1.5).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(THUMB_LAYER),
        DespawnOnExit(AppState::InGame),
    ));
}

#[allow(clippy::too_many_arguments)]
fn take_thumbs(
    mut commands: Commands,
    mut thumbs: ResMut<ModelThumbs>,
    mut cam: Query<(Entity, &mut Camera, &mut Transform), With<ThumbCamera>>,
    baked: Option<Res<Baked>>,
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let (Ok((cam_e, mut camera, mut cam_tf)), Some(baked)) = (cam.single_mut(), baked) else { return };
    // A picture rendered for a few frames (its textures upload meanwhile), then the next.
    if let Some((root, frame)) = thumbs.busy.as_mut() {
        *frame += 1;
        if *frame < 4 {
            return;
        }
        commands.entity(*root).despawn();
        thumbs.busy = None;
        camera.is_active = false;
    }
    let Some(key) = thumbs.queue.pop_front() else { return };
    let Some(img) = thumbs.images.get(&key).cloned() else { return };
    let mut ctx = AssetCtx { baked: &baked.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let parts = assets.model(&mut ctx, key);
    let Some((lo, hi)) = crate::objects::parts_bounds(&parts) else { return };
    let root = commands
        .spawn((Transform::from_translation(STAGE), Visibility::default()))
        .with_children(|c| {
            for p in &parts {
                c.spawn((Mesh3d(p.mesh.clone()), MeshMaterial3d(p.material.clone()), RenderLayers::layer(THUMB_LAYER)));
            }
        })
        .id();
    // From above and in front, far enough for the whole model to fill the picture.
    let center = STAGE + (lo + hi) * 0.5;
    let radius = ((hi - lo) * 0.5).length().max(0.02);
    let dist = radius / (FOV.to_radians() * 0.5).sin() * 1.02;
    let eye = center + Vec3::new(0.0, 0.75, 1.0).normalize() * dist;
    *cam_tf = Transform::from_translation(eye).looking_at(center, Vec3::Y);
    commands.entity(cam_e).insert((
        RenderTarget::Image(img.into()),
        Projection::Perspective(PerspectiveProjection { fov: FOV.to_radians(), near: (dist - radius * 1.5).max(0.01), far: dist + radius * 1.5, ..default() }),
    ));
    camera.is_active = true;
    thumbs.busy = Some((root, 0));
}
