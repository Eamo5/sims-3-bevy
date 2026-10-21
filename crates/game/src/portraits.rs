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
            .add_systems(Update, (retake_rebuilt, take_portraits, follow_portraits).chain().run_if(in_state(AppState::InGame)));
    }
}

/// The studio's own light and camera layer (and the Sim having their picture taken).
pub const STUDIO_LAYER: usize = 7;

/// The Sim having their picture taken steps into the studio (or out again): only they are
/// pictured, never anyone standing close by.
fn into_studio(commands: &mut Commands, children: &Query<&Children>, parts: &Query<(), (With<crate::simbody::SimModelPart>, With<Mesh3d>)>, pet_parts: &Query<(), With<Mesh3d>>, sim: Entity, pet: bool, on: bool) {
    let layers = if on {
        RenderLayers::from_layers(&[0, STUDIO_LAYER])
    } else if pet {
        RenderLayers::layer(0)
    } else {
        crate::simbody::sim_layers()
    };
    for c in children.get(sim).into_iter().flatten() {
        if parts.contains(*c) || (pet && pet_parts.contains(*c)) {
            commands.entity(*c).insert(layers.clone());
        }
    }
}

/// Where the camera goes for a pet's face: out along where it looks, by its size (a cat close
/// up, a horse from further off), a touch above.
fn pet_eye(kind: &str, head: Vec3, rot: Quat) -> (Vec3, Vec3, f32) {
    let (dist, up) = match kind {
        "ah" => (1.1, 0.05),
        "ch" => (0.85, 0.04),
        "ad" => (0.6, 0.04),
        "cd" | "al" | "cl" => (0.45, 0.03),
        _ => (0.4, 0.02),
    };
    let face = head + Vec3::Y * up;
    let ahead = (rot * Vec3::Z).normalize_or(Vec3::Z);
    (face + ahead * dist + Vec3::Y * 0.04, face, dist)
}

/// A pet's head and which way its face points (from the head to the nose, level).
fn pet_aim(skel: &crate::pets::PetSkeleton, joints: &Query<&GlobalTransform, Without<Sim>>, body: Quat) -> Option<(Vec3, Quat)> {
    let head = skel.bone("b__Head__", joints)?;
    let rot = match skel.bone("b__NoseBase__", joints).map(|n| (n - head).with_y(0.0)) {
        Some(f) if f.length() > 1e-3 => Quat::from_rotation_y(f.x.atan2(f.z)),
        _ => body,
    };
    Some((head, rot))
}
const SIZE: u32 = 128;
/// The portraits' backdrop card (put away: the Sims are pictured cut out): the game's soft blue,
/// lighter towards the top.
const BACKDROP_TOP: [u8; 3] = [206, 226, 244];
const BACKDROP_BOTTOM: [u8; 3] = [112, 150, 196];

/// Each Sim's portrait and the ones waiting to be taken.
#[derive(Resource, Default)]
pub struct Portraits {
    images: HashMap<Entity, Handle<Image>>,
    /// The picture being taken goes here, and is swapped in once it's finished (so a picture
    /// is never seen half made).
    spare: HashMap<Entity, Handle<Image>>,
    queue: VecDeque<Entity>,
    /// Pictures finished this game: the first few are held a moment longer, while the studio's
    /// shaders for the Sims' skin and hair are still being readied (until then they don't draw).
    finished: u32,
    /// The picture being taken.
    busy: Option<Shot>,
    /// When each Sim's body was (re)built: their skin takes a moment to be ready.
    built: HashMap<Entity, f32>,
    /// When each picture is due to be taken again (soon after the first, in case the face was
    /// still loading, then now and then).
    retake: HashMap<Entity, (f32, u32)>,
}

impl Portraits {
    /// A Sim's portrait (blank until it has been taken). It changes handle when retaken: UI
    /// showing it carries [`PortraitOf`] to follow along.
    pub fn portrait(&mut self, images: &mut Assets<Image>, sim: Entity) -> Handle<Image> {
        if let Some(h) = self.images.get(&sim) {
            return h.clone();
        }
        let h = blank(images);
        self.images.insert(sim, h.clone());
        self.queue.push_back(sim);
        h
    }

    /// The image the next picture of a Sim is taken into.
    fn spare(&mut self, images: &mut Assets<Image>, sim: Entity) -> Handle<Image> {
        self.spare.entry(sim).or_insert_with(|| blank(images)).clone()
    }
}

/// A picture to take (clear until it's taken: the Sim is cut out, as the game's thumbnails
/// are, over whatever frame shows them).
pub(crate) fn blank(images: &mut Assets<Image>) -> Handle<Image> {
    let mut img = Image::new_target_texture(SIZE, SIZE, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb));
    img.data = Some(vec![0; (SIZE * SIZE * 4) as usize]);
    images.add(img)
}

/// A Sim brought under the world for their picture (nothing else moves them meanwhile).
#[derive(Component)]
pub struct Staged {
    /// Gameplay position and orientation, before the temporary portrait staging.
    pub original: Transform,
}

/// An image showing a Sim's portrait, kept to the latest picture.
#[derive(Component)]
pub struct PortraitOf(pub Entity);

fn follow_portraits(portraits: Res<Portraits>, mut nodes: Query<(&PortraitOf, &mut ImageNode)>) {
    for (p, mut node) in &mut nodes {
        if let Some(h) = portraits.images.get(&p.0)
            && node.image != *h
        {
            node.image = h.clone();
        }
    }
}

/// A picture in progress: Sims who are away (at work, off the lot) are brought to a spot
/// deep under the world for it, visible for a moment, and put back afterwards.
struct Shot {
    sim: Entity,
    frame: u8,
    /// When the camera was pointed (seconds).
    aimed_at: f32,
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
        // (Put away: the Sims are pictured cut out.)
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        StudioCamera,
        Camera3d::default(),
        Camera { order: -5, is_active: false, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        Projection::Perspective(PerspectiveProjection { fov: 24f32.to_radians(), near: 0.08, far: 1.0, ..default() }),
        AmbientLight { color: Color::srgb(0.9, 0.93, 1.0), brightness: 1500.0, ..default() },
        // (The studio only: the world's walls and furniture never get in the way, and its sun
        // and evening light don't colour the face.)
        RenderLayers::layer(STUDIO_LAYER),
        DespawnOnExit(AppState::InGame),
    ));
}

fn forget(mut portraits: ResMut<Portraits>) {
    *portraits = Portraits::default();
}

/// A rebuilt body gets a new picture; Sims who have left the world lose theirs.
/// Which way a Sim's face points: from the head to the tip of the nose (or between the eyes),
/// level; the body's facing if the face bones aren't found. (The body can be turned away from
/// where the head looks, mid-step or mid-turn.)
pub(crate) fn facing(skel: &Skeleton, joints: &Query<&GlobalTransform, Without<Sim>>, head: Vec3, body: Quat) -> Option<Quat> {
    let bone = |n: &str| skel.rig.bones.iter().position(|b| b.name == n).and_then(|i| joints.get(skel.joints[i]).ok()).map(|g| g.translation());
    let front = bone("b__NoseTip__").or_else(|| Some((bone("b__LeftEye__")? + bone("b__RightEye__")?) * 0.5));
    let Some(f) = front.map(|f| f - head) else { return Some(body) };
    // (Lying down the face looks up: no picture then.)
    let level = f.with_y(0.0);
    if level.length() <= f.length() * 0.6 {
        return None;
    }
    // Its pitch too, within reason: a Sim looking down at something is pictured from a little
    // below, into the face, not over the top of the head.
    let pitch = f.y.atan2(level.length()).clamp(-0.6, 0.4);
    Some(Quat::from_rotation_y(level.x.atan2(level.z)) * Quat::from_rotation_x(-pitch))
}

/// Where the camera goes for a face: `dist` out along where it looks, a touch above; far enough
/// for the whole head, hair and all, and the neck (as the game's thumbnails: never cut off at the
/// top of the picture).
pub(crate) fn eye_for(head: Vec3, rot: Quat, little: bool) -> (Vec3, Vec3, f32) {
    let face = head + Vec3::Y * if little { 0.05 } else { 0.08 };
    let ahead = (rot * Vec3::Z).normalize_or(Vec3::Z);
    let dist = if little { 0.66 } else { 0.95 };
    (face + ahead * dist + Vec3::Y * 0.03, face, dist)
}

fn retake_rebuilt(mut portraits: ResMut<Portraits>, rebuilt: Query<Entity, Changed<Skeleton>>, alive: Query<(), Or<(With<Sim>, With<crate::pets::Pet>)>>, time: Res<Time>) {
    portraits.images.retain(|e, _| alive.contains(*e));
    portraits.spare.retain(|e, _| alive.contains(*e));
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

/// The studio's own light.
#[derive(Component)]
struct StudioLight;

#[allow(clippy::type_complexity)]
fn take_portraits(
    mut commands: Commands,
    mut portraits: ResMut<Portraits>,
    mut cam: Query<(Entity, &mut Camera, &mut Transform), With<StudioCamera>>,
    mut backdrop: Query<&mut Transform, (With<Backdrop>, Without<StudioCamera>)>,
    sims: Query<(&Sim, &SimAnim, &GlobalTransform, &Skeleton, &InheritedVisibility)>,
    placed: Query<
        (&Transform, &Visibility, Has<crate::visit::Trip>, Has<crate::nav::PathFollow>),
        (With<Sim>, Without<StudioCamera>, Without<Backdrop>),
    >,
    joints: Query<&GlobalTransform, Without<Sim>>,
    lights: Query<Entity, With<StudioLight>>,
    time: Res<Time>,
    (grid, visited): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::visit::VisitedLot>>),
    mut images: ResMut<Assets<Image>>,
    (children, parts, pet_parts): (Query<&Children>, Query<(), (With<crate::simbody::SimModelPart>, With<Mesh3d>)>, Query<(), With<Mesh3d>>),
    pets: Query<(&crate::pets::Pet, &crate::pets::PetSkeleton, &GlobalTransform, &InheritedVisibility), Without<Sim>>,
) {
    let now = time.elapsed_secs();
    let Ok((cam_e, mut camera, mut cam_tf)) = cam.single_mut() else { return };
    // (A pet's face: where the camera goes for it.)
    let pet_target = |e: Entity| pets.get(e).ok().and_then(|(p, skel, tf, _)| pet_aim(skel, &joints, tf.rotation()).map(|(head, rot)| pet_eye(&p.kind, head, rot)));
    // The light that only the studio sees (a new one for each game: it goes with the world).
    let light_e = lights.iter().next().unwrap_or_else(|| {
        commands
            .spawn((
                StudioLight,
                PointLight { intensity: 40_000.0, range: 4.0, shadow_maps_enabled: false, ..default() },
                Transform::default(),
                RenderLayers::layer(STUDIO_LAYER),
                DespawnOnExit(AppState::InGame),
            ))
            .id()
    });

    if let Some(mut shot) = portraits.busy.take() {
        shot.frame = shot.frame.saturating_add(1);
        let hold = if portraits.finished < 3 { 1.2 } else { 0.0 };
        let mut done = false;
        let is_pet = pets.contains(shot.sim);
        match shot.aimed {
            // A pet: looked in the face, as a Sim is.
            None if is_pet => match (pet_target(shot.sim), portraits.images.contains_key(&shot.sim)) {
                (Some((eye, face, dist)), true) => {
                    let img = portraits.spare(&mut images, shot.sim);
                    *cam_tf = Transform::from_translation(eye).looking_at(face, Vec3::Y);
                    let right = cam_tf.right().as_vec3();
                    commands.entity(light_e).insert(Transform::from_translation(eye + right * 0.5 + Vec3::Y * 0.45));
                    commands.entity(cam_e).insert((RenderTarget::Image(img.into()), Projection::Perspective(PerspectiveProjection { fov: 24f32.to_radians(), near: 0.08, far: dist + 1.2, ..default() })));
                    camera.is_active = true;
                    into_studio(&mut commands, &children, &parts, &pet_parts, shot.sim, true, true);
                    shot.aimed = Some(shot.frame);
                    shot.aimed_at = now;
                }
                _ => {
                    portraits.queue.push_back(shot.sim);
                    return;
                }
            },
            // A staged Sim's pose settles under the world first.
            None if shot.staged.is_none() || shot.frame >= 3 => {
                let e = shot.sim;
                let aim = sims.get(e).ok().and_then(|(sim, _, tf, skel, _)| {
                    let head = skel.rig.bones.iter().position(|b| b.name == "b__Head__").and_then(|i| joints.get(skel.joints[i]).ok())?;
                    // (Babies are pictured lying in their cribs, and toddlers crawling about
                    // facing the way they go.)
                    let face = facing(skel, &joints, head.translation(), tf.rotation()).or(sim.age.is_little().then(|| tf.rotation()))?;
                    Some((sim.age.is_little(), head.translation(), face))
                });
                let img = portraits.images.contains_key(&e).then(|| portraits.spare(&mut images, e));
                // (Not mid-crouch: if the head isn't up where it belongs, try again later.)
                let standing = sims.get(e).ok().map(|(sim, _, tf, _, _)| (sim.age, tf.translation().y));
                let aim = aim.filter(|(_, head, _)| match standing {
                    Some((age, root_y)) => {
                        let tall = match age {
                            Age::Baby => 0.0,
                            // (A toddler who can't walk yet crawls, head low.)
                            Age::Toddler => 0.25,
                            Age::Child => 0.95,
                            _ => 1.35,
                        };
                        head.y - root_y >= tall || shot.staged.is_some() && head.y - root_y >= tall * 0.6
                    }
                    None => false,
                });
                if aim.is_none() && img.is_some() {
                    // Back in the queue, and anyone staged goes back.
                    if let Some((tf, vis)) = shot.staged {
                        commands.entity(shot.sim).insert((tf, vis)).remove::<Staged>();
                    }
                    portraits.queue.push_back(shot.sim);
                    return;
                }
                match (aim, img) {
                    (Some((little, head, rot)), Some(img)) => {
                        let (eye, face, dist) = eye_for(head, rot, little);
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
                            // (Only the Sim and the card are in the studio: the far plane can be
                            // generous, so a Sim who shifts a little isn't cut away.)
                            Projection::Perspective(PerspectiveProjection { fov: 24f32.to_radians(), near: 0.08, far: dist + 1.2, ..default() }),
                        ));
                        camera.is_active = true;
                        into_studio(&mut commands, &children, &parts, &pet_parts, shot.sim, false, true);

                        shot.aimed = Some(shot.frame);
                        shot.aimed_at = now;

                    }
                    _ => shot.aimed = Some(0),
                }
            }
            // A Sim who drops out of view mid-shot (driving off, say) is taken again later.
            Some(_) if shot.staged.is_none() && sims.get(shot.sim).is_ok_and(|s| !s.4.get()) => {
                camera.is_active = false;
                into_studio(&mut commands, &children, &parts, &pet_parts, shot.sim, is_pet, false);
                let taken = portraits.retake.get(&shot.sim).map_or(0, |r| r.1);
                portraits.retake.insert(shot.sim, (now + 2.0, taken));
                done = true;
            }
            // Until it renders, the camera follows the face (Sims move between frames).
            Some(at) if shot.frame < at.saturating_add(4) || now - shot.aimed_at < hold => {
                let aim = sims.get(shot.sim).ok().and_then(|(sim, _, tf, skel, _)| {
                    let head = skel.rig.bones.iter().position(|b| b.name == "b__Head__").and_then(|i| joints.get(skel.joints[i]).ok())?;
                    // (Babies are pictured lying in their cribs, and toddlers crawling about
                    // facing the way they go.)
                    let face = facing(skel, &joints, head.translation(), tf.rotation()).or(sim.age.is_little().then(|| tf.rotation()))?;
                    Some((sim.age.is_little(), head.translation(), face))
                });
                if let Some((eye, face, dist)) = aim.map(|(little, head, rot)| eye_for(head, rot, little)).or_else(|| pet_target(shot.sim)) {
                    *cam_tf = Transform::from_translation(eye).looking_at(face, Vec3::Y);
                    let right = cam_tf.right().as_vec3();
                    commands.entity(light_e).insert(Transform::from_translation(eye + right * 0.5 + Vec3::Y * 0.45));
                    if let Ok(mut b) = backdrop.single_mut() {
                        let behind = dist + 0.3;
                        let size = 2.0 * behind * (12f32.to_radians()).tan() * 1.3;
                        let at = eye + (face - eye).normalize() * behind;
                        *b = Transform::from_translation(at).looking_to(at - eye, Vec3::Y).with_scale(Vec3::splat(size));
                    }
                }
            }
            // The picture renders the frame after the camera is pointed; then the camera
            // rests and a staged Sim goes back.
            Some(_) => {
                portraits.finished += 1;
                camera.is_active = false;
                into_studio(&mut commands, &children, &parts, &pet_parts, shot.sim, is_pet, false);
                if let Some((tf, vis)) = shot.staged {
                    commands.entity(shot.sim).insert((tf, vis)).remove::<Staged>();
                }
                // Whether it came out: the camera still looking them in the face, as far off and
                // as high as it was put (a Sim can turn to something, or set off, mid-shot).
                let good = sims.get(shot.sim).ok().and_then(|(sim, _, tf, skel, _)| {
                    let head = skel.rig.bones.iter().position(|b| b.name == "b__Head__").and_then(|i| joints.get(skel.joints[i]).ok())?.translation();
                    let rot = facing(skel, &joints, head, tf.rotation()).or(sim.age.is_little().then(|| tf.rotation()))?;
                    let (eye, ..) = eye_for(head, rot, sim.age.is_little());
                    let ok = cam_tf.translation.distance(eye) < 0.22;
                    if !ok {
                        debug!("portrait of {} spoilt (camera {:?} from where it belongs): taken again", sim.first, cam_tf.translation - eye);
                    }
                    Some(ok)
                }).or_else(|| pet_target(shot.sim).map(|(eye, ..)| cam_tf.translation.distance(eye) < 0.22)) == Some(true);
                let first = !portraits.retake.contains_key(&shot.sim);
                let before = portraits.retake.get(&shot.sim).map_or(0, |r| r.1);
                if good {
                    let taken = before + 1;
                    portraits.retake.insert(shot.sim, (now + if taken < 2 { 12.0 } else { 240.0 }, taken));
                    // The finished picture is shown; the old one takes the next.
                    if let (Some(old), Some(new)) = (portraits.images.remove(&shot.sim), portraits.spare.remove(&shot.sim)) {
                        portraits.images.insert(shot.sim, new);
                        portraits.spare.insert(shot.sim, old);
                    }
                } else if first {
                    // (A first picture is retried straight away.)
                    portraits.queue.push_back(shot.sim);
                } else {
                    // (The old picture stays up meanwhile.)
                    portraits.retake.insert(shot.sim, (now + 2.0, before));
                }
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
        // (A pet: pictured where it is, once it's in view and its coat's had a moment to load.)
        if let Ok((_, _, _, vis)) = pets.get(e) {
            if !vis.get() || now < 6.0 || pet_target(e).is_none() {
                portraits.queue.push_back(e);
                continue;
            }
            portraits.busy = Some(Shot { sim: e, frame: 0, aimed_at: 0.0, staged: None, aimed: None });
            break;
        }
        let (Ok((sim, anim, _, _, vis)), Ok((tf, visibility, driving, walking))) = (sims.get(e), placed.get(e)) else {
            // (The body isn't built yet.)
            portraits.queue.push_back(e);
            continue;
        };
        // A Sim's first picture is taken wherever they are (staged if away); retakes wait until
        // they're in view, standing or walking about.
        // (Someone on their way to a community lot is pictured when they get there.)
        // The studio render layer already isolates visible Sims from the world.
        // Only an away Sim needs staging; moving a visible Sim interrupts live play.
        let first = portraits.retake.get(&e).is_none();
        let away = first && !vis.get() && !driving;
        // (Retakes in view wait for them to stand still: the camera can't keep up with a Sim
        // on the move, least of all at speed.)
        let upright = if first { anim.pose != Pose::Lie } else { matches!(anim.pose, Pose::Stand | Pose::Talk) && !walking } || sim.age == Age::Baby;
        // (Faces and clothes finish loading a few seconds after a body is built.)
        let settled = portraits.built.get(&e).is_none_or(|t| now - t > 4.0) && now > 6.0;
        // A retake needs room in front of the face for the camera (not a stall or a corner).
        let room = first || {
            let eye = (tf.translation + (tf.rotation * Vec3::Z).with_y(0.0).normalize_or_zero() * 0.72).xz();
            let clear = |g: &crate::nav::NavGrid| g.cell_of(eye).map(|(x, z)| !g.is_blocked(x, z));
            visited.as_ref().and_then(|v| clear(&v.grid)).or_else(|| grid.as_deref().and_then(clear)).unwrap_or(true)
        };
        if !settled || !room || !(vis.get() && upright || away) {
            portraits.queue.push_back(e);
            continue;
        }
        let staged = (!vis.get()).then(|| (*tf, *visibility));
        if let Some((tf, _)) = staged {
            debug!("staging portrait of {}: world position {:?}", sim.first, tf.translation);
            commands.entity(e).insert((Transform { translation: tf.translation - Vec3::Y * UNDERGROUND, ..tf }, Visibility::Visible, Staged { original: tf }));
        }
        portraits.busy = Some(Shot { sim: e, frame: 0, aimed_at: 0.0, staged, aimed: None });
        break;
    }
}
