//! Sims-style camera: orbit around a ground focus point, pan with WASD / arrows,
//! rotate with Q/E or middle/right mouse drag, zoom with the wheel.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use crate::AppState;
use crate::loading::CurrentWorld;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_camera)
            .add_systems(Update, camera_control.run_if(in_state(AppState::InGame)));
    }
}

#[derive(Component)]
pub struct SimsCamera {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Smoothed values actually used for the transform.
    smooth_focus: Vec3,
    smooth_distance: f32,
}

impl SimsCamera {
    pub fn new(focus: Vec3) -> Self {
        Self {
            focus,
            yaw: 0.7,
            pitch: 0.75,
            distance: 30.0,
            smooth_focus: focus,
            smooth_distance: 30.0,
        }
    }

    /// Moves the camera focus to a point (e.g. a selected sim).
    pub fn look_at(&mut self, p: Vec3) {
        self.focus = p;
    }
}

/// Requests that the camera jumps to a location at the start of play.
#[derive(Resource)]
pub struct CameraStart(pub Vec3);

fn spawn_camera(mut commands: Commands, world: Res<CurrentWorld>, start: Option<Res<CameraStart>>) {
    let hm = &world.data.heightmap;
    let size = (hm.width - 1) as f32;
    let mut focus = start.map(|s| s.0).unwrap_or(Vec3::new(size * 0.5, 0.0, size * 0.5));
    focus.y = hm.sample(focus.x, focus.z);
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 50f32.to_radians(),
            near: 0.1,
            far: 6000.0,
            ..default()
        }),
        Transform::from_translation(focus + Vec3::new(20.0, 20.0, 20.0)).looking_at(focus, Vec3::Y),
        SimsCamera::new(focus),
        DistanceFog {
            color: Color::srgba(0.62, 0.75, 0.90, 1.0),
            falloff: FogFalloff::Linear { start: 700.0, end: 2600.0 },
            ..default()
        },
        AmbientLight { color: Color::srgb(0.75, 0.82, 1.0), brightness: 700.0, ..default() },
        DespawnOnExit(AppState::InGame),
    ));

    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.96, 0.88),
            illuminance: 9000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.9, -0.85, 0.0)),
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            first_cascade_far_bound: 25.0,
            maximum_distance: 400.0,
            ..default()
        }
        .build(),
        DespawnOnExit(AppState::InGame),
    ));
}

fn camera_control(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    world: Res<CurrentWorld>,
    mut q: Query<(&mut SimsCamera, &mut Transform)>,
) {
    let Ok((mut cam, mut tf)) = q.single_mut() else { return };
    let dt = time.delta_secs();

    // Zoom
    let wheel = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if wheel != 0.0 {
        cam.distance = (cam.distance * (1.0 - wheel * 0.12)).clamp(3.0, 900.0);
    }
    if keys.pressed(KeyCode::KeyZ) || keys.pressed(KeyCode::Equal) {
        cam.distance = (cam.distance * (1.0 - dt * 1.5)).max(3.0);
    }
    if keys.pressed(KeyCode::KeyX) || keys.pressed(KeyCode::Minus) {
        cam.distance = (cam.distance * (1.0 + dt * 1.5)).min(900.0);
    }

    // Rotate
    if mouse.pressed(MouseButton::Middle) || mouse.pressed(MouseButton::Right) {
        cam.yaw -= motion.delta.x * 0.005;
        cam.pitch = (cam.pitch + motion.delta.y * 0.004).clamp(0.12, 1.5);
    }
    if keys.pressed(KeyCode::KeyQ) {
        cam.yaw += dt * 1.6;
    }
    if keys.pressed(KeyCode::KeyE) {
        cam.yaw -= dt * 1.6;
    }
    if keys.pressed(KeyCode::PageUp) {
        cam.pitch = (cam.pitch + dt).min(1.5);
    }
    if keys.pressed(KeyCode::PageDown) {
        cam.pitch = (cam.pitch - dt).max(0.12);
    }

    // Pan relative to view direction
    let forward = Vec3::new(-cam.yaw.sin(), 0.0, -cam.yaw.cos());
    let right = Vec3::new(cam.yaw.cos(), 0.0, -cam.yaw.sin());
    let mut pan = Vec3::ZERO;
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        pan += forward;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        pan -= forward;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        pan += right;
    }
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        pan -= right;
    }
    let speed = (cam.distance * 1.2).max(8.0) * if keys.pressed(KeyCode::ShiftLeft) { 3.0 } else { 1.0 };
    let pan_delta = pan.normalize_or_zero() * speed * dt;
    cam.focus += pan_delta;

    let hm = &world.data.heightmap;
    let size = (hm.width - 1) as f32;
    cam.focus.x = cam.focus.x.clamp(0.0, size);
    cam.focus.z = cam.focus.z.clamp(0.0, size);
    let ground = hm.sample(cam.focus.x, cam.focus.z).max(crate::terrain::SEA_LEVEL);
    cam.focus.y = ground;

    let k = 1.0 - (-dt * 10.0).exp();
    let target_focus = cam.focus;
    let target_dist = cam.distance;
    cam.smooth_focus = cam.smooth_focus.lerp(target_focus, k);
    cam.smooth_distance += (target_dist - cam.smooth_distance) * k;

    let dir = Vec3::new(
        cam.yaw.sin() * cam.pitch.cos(),
        cam.pitch.sin(),
        cam.yaw.cos() * cam.pitch.cos(),
    );
    let mut eye = cam.smooth_focus + dir * cam.smooth_distance;
    // Keep the camera above the terrain.
    let min_y = hm.sample(eye.x, eye.z) + 1.5;
    if eye.y < min_y {
        eye.y = min_y;
    }
    *tf = Transform::from_translation(eye).looking_at(cam.smooth_focus, Vec3::Y);
}
