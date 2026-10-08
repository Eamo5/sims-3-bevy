//! Pets (the Pets pack): cats, dogs, little dogs and horses, in the game's own bodies on their
//! own skeletons, each in one of the game's breeds (its coat painted at the bake from the
//! breed's colours and markings), moving with the pack's own animations: walking and trotting,
//! standing about looking round, sitting, grooming, lying down. Strays wander the street by the
//! lot by day, stopping to sit or lie about, and curl up asleep at night.

use std::sync::Arc;

use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use s3bake::PetsBaked;
use s3formats::sim::Rig;

use crate::anim::{ClipLibrary, sample_track_quat, sample_track_vec};
use crate::baked::Baked;
use crate::clock::{GameClock, SPEED_RATES};
use crate::objects::{AssetCtx, ObjectAssets};
use crate::{AppState, PlayMode};

pub struct PetsPlugin;

impl Plugin for PetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PetData>()
            .add_systems(Update, (load_pet_data, spawn_strays, strays, animate_pets).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(Update, pet_cam.run_if(in_state(PlayMode::Live)));
    }
}

/// The baked pets (kinds and breeds), once loaded.
#[derive(Resource, Default)]
pub struct PetData(pub Option<Arc<PetsBaked>>);

fn load_pet_data(mut data: ResMut<PetData>, mut tried: Local<bool>) {
    if *tried {
        return;
    }
    *tried = true;
    data.0 = s3bake::load_pets(&s3bake::default_root()).map(Arc::new);
    if let Some(d) = &data.0 {
        info!("pets: {} kinds, {} breeds", d.kinds.len(), d.breeds.len());
    }
}

/// A pet: its kind (`ac` a cat, `ad` a dog...), its breed (index into the baked breeds) and its
/// name.
#[derive(Component, Clone, Debug)]
pub struct Pet {
    pub kind: String,
    pub breed: usize,
    pub name: String,
}

impl Pet {
    /// What it is, in words.
    pub fn species(&self) -> &'static str {
        match &self.kind[1..] {
            "c" => "Cat",
            "d" => "Dog",
            "l" => "Little Dog",
            _ => "Horse",
        }
    }

    /// Its walking pace (metres a second at normal speed, as its walk animation steps).
    fn walk_speed(&self) -> f32 {
        match self.kind.as_str() {
            "ac" => 0.5,
            "ad" => 0.9,
            "al" => 0.75,
            "ah" | "ch" => 1.0,
            _ => 0.3,
        }
    }
}

/// A pet's skeleton: an entity per bone, and each bone's rest pose.
#[derive(Component)]
struct PetSkeleton {
    rig: Arc<Rig>,
    joints: Vec<Entity>,
    bind: Vec<Transform>,
}

/// The animation a pet's playing (looped), with a short fade from the last.
#[derive(Component, Default)]
pub struct PetAnim {
    pub clip: String,
    time: f32,
    from: Vec<Transform>,
    blend: f32,
}

impl PetAnim {
    pub fn play(&mut self, clip: String) {
        if self.clip != clip {
            self.clip = clip;
            self.time = 0.0;
            self.blend = 1.0;
        }
    }
}

/// The body's shaders: fur (and the horse's coat), the nose and pads, the eyes.
const SHADER_PET_FUR: u32 = 0xA78F71C4;
const SHADER_HORSE: u32 = 0x05F75EF1;
const SHADER_SKIN: u32 = 0x548394B9;
const SHADER_EYES: u32 = 0xCF8A70B4;

/// Puts a pet of this breed down at `pos`, facing `yaw`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_pet(
    commands: &mut Commands,
    data: &PetsBaked,
    breed: usize,
    name: String,
    pos: Vec3,
    yaw: f32,
    ctx: &mut AssetCtx,
    assets: &mut ObjectAssets,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) -> Option<Entity> {
    let b = data.breeds.get(breed)?;
    let kind = data.kinds.iter().find(|k| k.code == b.kind)?;
    let rig = Arc::new(kind.rig.clone()?);
    let root = commands
        .spawn((
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            Pet { kind: b.kind.clone(), breed, name },
            PetAnim::default(),
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    // The skeleton, as a Sim's.
    let mut joints = Vec::with_capacity(rig.bones.len());
    let mut bind = Vec::with_capacity(rig.bones.len());
    let mut world: Vec<Mat4> = Vec::with_capacity(rig.bones.len());
    for bone in &rig.bones {
        let local = Transform {
            translation: Vec3::from(bone.position),
            rotation: Quat::from_xyzw(bone.rotation[0], bone.rotation[1], bone.rotation[2], bone.rotation[3]).normalize(),
            scale: Vec3::from(bone.scale),
        };
        let w = if bone.parent >= 0 && (bone.parent as usize) < world.len() { world[bone.parent as usize] * local.to_matrix() } else { local.to_matrix() };
        world.push(w);
        bind.push(local);
        joints.push(commands.spawn((local, Visibility::default())).id());
    }
    for (i, bone) in rig.bones.iter().enumerate() {
        let p = if bone.parent >= 0 && (bone.parent as usize) < joints.len() { joints[bone.parent as usize] } else { root };
        commands.entity(p).add_child(joints[i]);
    }
    let inverse = bindposes.add(SkinnedMeshInverseBindposes::from(world.iter().map(|m| m.inverse()).collect::<Vec<_>>()));
    // The coat over the fur, nose and pads; dark glossy eyes.
    let coat = assets.texture(ctx, b.coat);
    let fur = ctx.materials.add(StandardMaterial { base_color_texture: coat, perceptual_roughness: 0.92, reflectance: 0.15, ..default() });
    let eyes = ctx.materials.add(StandardMaterial { base_color: Color::srgb(0.06, 0.05, 0.04), perceptual_roughness: 0.15, reflectance: 0.6, ..default() });
    let named = |list: &[s3bake::pets::PetPart], name: &Option<String>| -> Vec<s3bake::SkinMesh> {
        name.as_ref().and_then(|n| list.iter().find(|p| &p.name == n)).map(|p| p.meshes.clone()).unwrap_or_default()
    };
    let parts = kind.body.iter().cloned().chain(named(&kind.tails, &b.tail)).chain(named(&kind.ears, &b.ears)).chain(named(&kind.manes, &b.mane));
    for m in parts {
        let material = match m.shader {
            SHADER_EYES => eyes.clone(),
            SHADER_PET_FUR | SHADER_HORSE | SHADER_SKIN => fur.clone(),
            _ => fur.clone(),
        };
        let mesh = ctx.meshes.add(crate::simbody::skin_mesh(m));
        let e = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                SkinnedMesh { inverse_bindposes: inverse.clone(), joints: joints.clone() },
                Transform::default(),
                bevy::camera::visibility::NoFrustumCulling,
            ))
            .id();
        commands.entity(root).add_child(e);
    }
    commands.entity(root).insert(PetSkeleton { rig, joints, bind });
    Some(root)
}

/// Plays each pet's animation on its skeleton (looping), fading from the last over a moment.
fn animate_pets(
    time: Res<Time>,
    clock: Res<GameClock>,
    data: Res<Baked>,
    mut lib: ResMut<ClipLibrary>,
    mut pets: Query<(&PetSkeleton, &mut PetAnim)>,
    mut joints: Query<&mut Transform, Without<PetSkeleton>>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed].min(3.0);
    for (skel, mut a) in &mut pets {
        if a.clip.is_empty() {
            continue;
        }
        let Some(clip) = lib.get(&data, &a.clip) else { continue };
        if a.blend >= 1.0 {
            a.from = skel.joints.iter().map(|j| joints.get(*j).copied().unwrap_or_default()).collect();
        }
        a.time += dt;
        a.blend = (a.blend - dt * 4.0).max(0.0);
        let t = if clip.duration > 0.0 { a.time % clip.duration } else { 0.0 };
        for (i, bone) in skel.rig.bones.iter().enumerate() {
            let Ok(mut tf) = joints.get_mut(skel.joints[i]) else { continue };
            let mut target = skel.bind[i];
            if let Some(track) = clip.tracks.get(&bone.hash) {
                // (The root's travel is the pet's own walking.)
                if i != 0
                    && let Some(p) = sample_track_vec(&track.translation, t)
                {
                    target.translation = p;
                }
                if let Some(r) = sample_track_quat(&track.rotation, t) {
                    target.rotation = r;
                }
            }
            if i == 0 {
                target.rotation = skel.bind[i].rotation;
            }
            if a.blend > 0.0
                && let Some(from) = a.from.get(i)
            {
                target.translation = target.translation.lerp(from.translation, a.blend);
                target.rotation = target.rotation.slerp(from.rotation, a.blend);
            }
            *tf = target;
        }
    }
}

/// `PET_CAM=<n>` (tests): the camera on the nth pet, `PET_DIST` away.
fn pet_cam(pets: Query<&GlobalTransform, With<Pet>>, mut cams: Query<&mut crate::camera::SimsCamera>) {
    let Some(n) = std::env::var("PET_CAM").ok().and_then(|v| v.parse::<usize>().ok()) else { return };
    let (Some(p), Ok(mut c)) = (pets.iter().nth(n), cams.single_mut()) else { return };
    c.look_at(p.translation());
    c.distance = std::env::var("PET_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(4.0);
}

/// A stray: wandering about near where it turned up.
#[derive(Component)]
pub struct Stray {
    home: Vec2,
    target: Option<Vec2>,
    /// Game minute it moves on from what it's doing.
    until: f64,
}

/// How many strays wander the street (a cat and a dog, as the game's town has about).
const STRAYS: usize = 2;

#[allow(clippy::too_many_arguments)]
fn spawn_strays(
    mut commands: Commands,
    pets: Res<PetData>,
    sidewalk: Option<Res<crate::town::Sidewalk>>,
    world: Res<crate::loading::CurrentWorld>,
    existing: Query<(), With<Stray>>,
    data: Res<Baked>,
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut materials, mut bindposes): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>, ResMut<Assets<SkinnedMeshInverseBindposes>>),
    mut done: Local<bool>,
) {
    let (Some(p), Some(walk)) = (pets.0.as_ref(), sidewalk) else { return };
    if *done || !existing.is_empty() {
        return;
    }
    *done = true;
    let mut rng = rand::rng();
    for i in 0..STRAYS {
        // (A grown cat, then a dog of either size.)
        let kinds: &[&str] = if i % 2 == 0 { &["ac"] } else { &["ad", "al"] };
        let choices: Vec<usize> = p.breeds.iter().enumerate().filter(|(_, b)| kinds.contains(&b.kind.as_str()) && b.age & 0x20 != 0).map(|(i, _)| i).collect();
        let Some(&breed) = choices.choose(&mut rng) else { continue };
        // (Near the lot's front, where the sidewalk runs straight along the street.)
        let at = walk.center + walk.along * rng.random_range(-10.0..10.0f32).clamp(-walk.half_length, walk.half_length);
        let pos = Vec3::new(at.x, world.data.heightmap.sample(at.x, at.y), at.y);
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        let name = if i % 2 == 0 { "Stray Cat" } else { "Stray Dog" };
        if let Some(e) = spawn_pet(&mut commands, p, breed, name.to_string(), pos, rng.random_range(0.0..6.28), &mut ctx, &mut assets, &mut bindposes) {
            commands.entity(e).insert(Stray { home: at, target: None, until: 0.0 });
        }
    }
}

/// Strays wander: off somewhere nearby, then a while standing about, sitting or lying down
/// (asleep at night), and off again.
fn strays(
    time: Res<Time>,
    clock: Res<GameClock>,
    world: Res<crate::loading::CurrentWorld>,
    sidewalk: Option<Res<crate::town::Sidewalk>>,
    mut q: Query<(&Pet, &mut Stray, &mut Transform, &mut PetAnim)>,
) {
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    let night = !(6.0..21.0).contains(&clock.hour_f());
    for (pet, mut s, mut tf, mut anim) in &mut q {
        let here = Vec2::new(tf.translation.x, tf.translation.z);
        match s.target {
            Some(to) => {
                let d = to - here;
                let step = pet.walk_speed() * dt;
                if d.length() <= step.max(0.05) {
                    s.target = None;
                    s.until = clock.minutes + rng.random_range(8.0..40.0) as f64;
                    // (Something to do while it's stopped.)
                    let k = &pet.kind;
                    let idle = if night {
                        format!("{k}_sleep_loop1_x")
                    } else {
                        let options = [
                            format!("{k}_idle_stand_breathe_x"),
                            format!("{k}_idle_stand_lookAround_x"),
                            format!("{k}_idle_sit_breathe_x"),
                            if k == "ac" { "ac_idle_layDown_breathe_x".to_string() } else { format!("{k}_idle_laydown_breathe_x") },
                        ];
                        options.choose(&mut rng).cloned().unwrap_or_default()
                    };
                    anim.play(idle);
                } else {
                    let p = here + d.normalize() * step;
                    tf.translation = Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y);
                    tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
                    anim.play(format!("{}_walk_x", pet.kind));
                }
            }
            None if clock.minutes >= s.until && !night => {
                // Off a little way along the sidewalk (out of the road), never far from where it
                // turned up.
                let (along, across) = sidewalk.as_ref().map_or((Vec2::X, Vec2::Y), |w| (w.along, Vec2::new(-w.along.y, w.along.x)));
                s.target = Some(s.home + along * rng.random_range(-7.0..7.0) + across * rng.random_range(-0.3..0.3));
            }
            None => {
                if anim.clip.is_empty() {
                    anim.play(format!("{}_idle_stand_breathe_x", pet.kind));
                }
            }
        }
    }
}
