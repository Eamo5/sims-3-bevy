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
            .init_resource::<PendingPets>()
            .add_systems(
                Update,
                (load_pet_data, spawn_strays, spawn_household_pets, social_partners, strays, home_pets, animate_pets).chain().run_if(in_state(PlayMode::Live)),
            )
            .add_systems(Update, (pet_cam, pet_do, adopted_pets).run_if(in_state(PlayMode::Live)));
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

/// What a Sim can do with a pet (the Pets pack's socials): its name, its animation (both sides:
/// `a2ac_soc_neutral_<clip>_x` the Sim, `…_y` the cat), the kinds it's for, how long it takes
/// (minutes) and the Sim's fun and social from it (an hour).
pub struct PetSocial {
    pub name: &'static str,
    clip: &'static str,
    kinds: &'static [&'static str],
    pub minutes: f32,
    pub fun: f32,
    pub social: f32,
}

const ALL: &[&str] = &["ac", "cc", "ad", "cd", "al", "cl", "ah", "ch"];
const GROWN: &[&str] = &["ac", "ad", "al", "ah"];

pub static PET_SOCIALS: [PetSocial; 7] = [
    PetSocial { name: "Pet", clip: "petFloor_friendly_neutral", kinds: ALL, minutes: 8.0, fun: 40.0, social: 70.0 },
    PetSocial { name: "Let Sniff Hand", clip: "letSniffHand_neutral_neutral", kinds: GROWN, minutes: 4.0, fun: 10.0, social: 40.0 },
    PetSocial { name: "Praise", clip: "praise_friendly_neutral", kinds: GROWN, minutes: 4.0, fun: 15.0, social: 50.0 },
    PetSocial { name: "Give Hug", clip: "giveLoveHug_friendly_neutral", kinds: &["ac", "ad", "al"], minutes: 5.0, fun: 30.0, social: 80.0 },
    PetSocial { name: "Feed Treat", clip: "feedTreat_friendly_neutral", kinds: GROWN, minutes: 5.0, fun: 20.0, social: 40.0 },
    PetSocial { name: "Rub Neck", clip: "rubNeck_friendly_neutral", kinds: &["ah"], minutes: 8.0, fun: 30.0, social: 60.0 },
    PetSocial { name: "Scold", clip: "scold_neutral_neutral", kinds: GROWN, minutes: 4.0, fun: -10.0, social: 10.0 },
];

impl PetSocial {
    pub fn suits(&self, kind: &str) -> bool {
        self.kinds.contains(&kind)
    }

    /// Its animation for a Sim (a child's, where there is one) with a pet of this kind, the Sim's
    /// side ('x') or the pet's ('y'), if the game has it (interned: animations are named for
    /// good).
    pub fn clip_for(&self, data: &Baked, child: bool, kind: &str, side: char) -> Option<&'static str> {
        let want = |who: char| format!("{who}2{kind}_soc_neutral_{}_{side}", self.clip);
        let has = |n: &str| data.0.clip_names.iter().any(|c| c.eq_ignore_ascii_case(n));
        let name = [if child { Some(want('c')) } else { None }, Some(want('a'))].into_iter().flatten().find(|n| has(n))?;
        Some(intern(name))
    }
}

/// A name kept for good (animation names are few, and asked for again and again).
fn intern(s: String) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap();
    if let Some(n) = names.iter().find(|n| **n == s) {
        return n;
    }
    let n: &'static str = Box::leak(s.into_boxed_str());
    names.push(n);
    n
}

/// How far from a pet a Sim stands for a social with it (the game's jigs: further for horses).
pub fn social_distance(kind: &str) -> f32 {
    match kind {
        "ah" | "ch" => 1.25,
        "ad" | "cd" => 0.8,
        _ => 0.7,
    }
}

/// A pet a Sim is seeing to: it stays put (and faces them, playing its side, once they're there).
#[derive(Component)]
pub struct PetBusy;

/// Pets being seen to stop what they're doing and wait; once the Sim's there, they turn to them
/// and play their side of it. Free again when it's over.
fn social_partners(
    mut commands: Commands,
    data: Res<Baked>,
    sims: Query<(&crate::interact::ActionQueue, &Transform, &crate::sim::Sim), Without<Pet>>,
    mut pets: Query<(Entity, &Pet, &mut Transform, &mut PetAnim, Has<PetBusy>)>,
) {
    let mut seen: Vec<Entity> = Vec::new();
    for (q, stf, sim) in &sims {
        let Some(a) = q.0.front() else { continue };
        let crate::interact::ActionKind::PetSocial { target, social, .. } = a.kind else { continue };
        let Ok((e, pet, mut tf, mut anim, busy)) = pets.get_mut(target) else { continue };
        seen.push(e);
        if !busy {
            commands.entity(e).insert(PetBusy);
        }
        match a.phase {
            crate::interact::Phase::Running(_) => {
                let to = stf.translation.xz() - tf.translation.xz();
                tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
                let child = sim.age == crate::sim::Age::Child;
                if let Some(c) = PET_SOCIALS.get(social).and_then(|s| s.clip_for(&data, child, &pet.kind, 'y')) {
                    anim.play(c.to_string());
                }
            }
            _ => anim.play(format!("{}_idle_stand_breathe_x", pet.kind)),
        }
    }
    for (e, pet, _, mut anim, busy) in &mut pets {
        if busy && !seen.contains(&e) {
            commands.entity(e).remove::<PetBusy>();
            anim.play(format!("{}_idle_stand_breathe_x", pet.kind));
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

/// `PET_DO=<pet name>:<social>` (tests): the selected Sim does that with the pet, once it's home.
fn pet_do(
    mut sel: Query<&mut crate::interact::ActionQueue, With<crate::sim::Selected>>,
    pets: Query<(Entity, &Pet, &GlobalTransform), With<HomePet>>,
    time: Res<Time>,
    mut done: Local<bool>,
) {
    let Some((who, what)) = std::env::var("PET_DO").ok().and_then(|v| v.split_once(':').map(|(a, b)| (a.to_string(), b.to_string()))) else { return };
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    let (Ok(mut q), Some((e, pet, tf))) = (sel.single_mut(), pets.iter().find(|(_, p, _)| p.name.eq_ignore_ascii_case(&who))) else { return };
    let Some(i) = PET_SOCIALS.iter().position(|s| s.name.eq_ignore_ascii_case(&what)) else { return };
    let kind: &'static str = ["ac", "cc", "ad", "cd", "al", "cl", "ah", "ch"].into_iter().find(|k| *k == pet.kind).unwrap_or("ac");
    *done = true;
    info!("pet test: {} with {}", what, pet.name);
    q.0.clear();
    q.push_player(crate::interact::Action::new(PET_SOCIALS[i].name, crate::interact::ActionKind::PetSocial { target: e, social: i, at: tf.translation().xz(), kind }, false));
}

/// `PET_CAM=<n or name>` (tests): the camera on the nth pet (or the one by that name),
/// `PET_DIST` away.
fn pet_cam(pets: Query<(&GlobalTransform, &Pet)>, mut cams: Query<&mut crate::camera::SimsCamera>) {
    let Ok(v) = std::env::var("PET_CAM") else { return };
    let p = match v.parse::<usize>() {
        Ok(n) => pets.iter().nth(n),
        Err(_) => pets.iter().find(|(_, p)| p.name.eq_ignore_ascii_case(&v)),
    };
    let (Some((p, _)), Ok(mut c)) = (p, cams.single_mut()) else { return };
    c.look_at(p.translation());
    c.distance = std::env::var("PET_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(4.0);
}

/// A household's pet, as kept in saves: who it is, what kind, its breed (by the game's outfit
/// for it) and where it was.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SavedPet {
    pub id: u64,
    pub name: String,
    pub kind: String,
    pub outfit: u64,
    #[serde(default)]
    pub position: Option<[f32; 3]>,
}

/// Pets to put on the home lot once it's ready (a premade household's, or a save's).
#[derive(Resource, Default)]
pub struct PendingPets(pub Vec<SavedPet>);

/// A pet adopted by phone, to come home.
#[derive(Resource, Clone)]
pub struct AdoptPetOrder {
    pub kind: &'static str,
    pub name: String,
}

/// What a kind of pet is called.
pub fn kind_name(kind: &str) -> &'static str {
    match kind {
        "ac" => "Cat",
        "cc" => "Kitten",
        "ad" => "Dog",
        "cd" => "Puppy",
        "al" => "Little Dog",
        "cl" => "Little Puppy",
        "ah" => "Horse",
        "ch" => "Foal",
        _ => "Pet",
    }
}

/// What adopting one costs (horses dear).
pub fn adoption_fee(kind: &str) -> i64 {
    match kind {
        "ah" => 2500,
        "ch" => 1500,
        "ad" | "al" => 400,
        "cd" | "cl" => 350,
        _ => 300,
    }
}

/// A name for a new pet.
pub fn random_pet_name(rng: &mut impl Rng) -> String {
    const NAMES: [&str; 40] = [
        "Biscuit", "Whiskers", "Patches", "Pepper", "Mittens", "Shadow", "Buttons", "Ginger", "Pumpkin", "Oreo", "Socks", "Muffin", "Cocoa", "Pickles",
        "Noodle", "Sprinkles", "Maple", "Bramble", "Tinker", "Juniper", "Clover", "Domino", "Waffles", "Rusty", "Scout", "Duke", "Daisy", "Bandit", "Pippin",
        "Thistle", "Comet", "Hazel", "Midnight", "Sunny", "Bubbles", "Chester", "Rosie", "Barley", "Captain", "Truffle",
    ];
    NAMES.choose(rng).copied().unwrap_or("Biscuit").to_string()
}

/// A pet adopted by phone comes home: one of the game's breeds of its kind, by the lot's front.
fn adopted_pets(mut commands: Commands, order: Option<Res<AdoptPetOrder>>, pets: Res<PetData>, mut pending: ResMut<PendingPets>) {
    let (Some(o), Some(data)) = (order, pets.0.as_ref()) else { return };
    commands.remove_resource::<AdoptPetOrder>();
    let mut rng = rand::rng();
    let wears = if o.kind == "cl" { "cd" } else { o.kind };
    let breeds: Vec<&s3bake::pets::PetBreed> = data.breeds.iter().filter(|b| b.kind == wears).collect();
    let Some(b) = breeds.choose(&mut rng) else { return };
    pending.0.push(SavedPet { id: rng.random(), name: o.name.clone(), kind: o.kind.to_string(), outfit: b.outfit, position: None });
}

/// The kind of pet a premade one is: its species and whether it's young.
fn kind_of(species: u32, age: u32) -> Option<&'static str> {
    let young = age & 0x07 != 0;
    Some(match (species, young) {
        (3, false) => "ac",
        (3, true) => "cc",
        (4, false) => "ad",
        (4, true) => "cd",
        (5, false) => "al",
        (5, true) => "cl",
        (2, false) => "ah",
        (2, true) => "ch",
        _ => return None,
    })
}

impl SavedPet {
    /// A premade household's pet: one of the game's breeds of its kind, the same each time (its
    /// own coat isn't in the install).
    pub fn premade(p: &s3formats::premade::PremadeSim, data: &PetsBaked) -> Option<SavedPet> {
        let kind = kind_of(p.species, p.age)?;
        // (Little puppies wear puppies' breeds: their bodies are the same.)
        let wears = if kind == "cl" && !data.breeds.iter().any(|b| b.kind == "cl") { "cd" } else { kind };
        let breeds: Vec<&s3bake::pets::PetBreed> = data.breeds.iter().filter(|b| b.kind == wears).collect();
        let b = breeds.get((p.id % breeds.len().max(1) as u64) as usize)?;
        Some(SavedPet { id: p.id, name: p.first_name.clone(), kind: kind.to_string(), outfit: b.outfit, position: None })
    }
}

/// A household's pet, at home on the lot.
#[derive(Component)]
pub struct HomePet {
    pub id: u64,
    path: Vec<Vec2>,
    until: f64,
}

#[allow(clippy::too_many_arguments)]
fn spawn_household_pets(
    mut commands: Commands,
    pets: Res<PetData>,
    mut pending: ResMut<PendingPets>,
    grid: Option<Res<crate::nav::NavGrid>>,
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<crate::loading::CurrentWorld>,
    data: Res<Baked>,
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut materials, mut bindposes): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>, ResMut<Assets<SkinnedMeshInverseBindposes>>),
) {
    if pending.0.is_empty() {
        return;
    }
    let (Some(p), Some(grid)) = (pets.0.as_ref(), grid) else { return };
    let mut rng = rand::rng();
    let near = exit.map_or(grid.center_of(grid.w / 2, grid.h / 2), |e| e.0);
    for pet in std::mem::take(&mut pending.0) {
        let Some(breed) = p.breeds.iter().position(|b| b.outfit == pet.outfit) else { continue };
        // (Where it was, else somewhere free in the yard by the front.)
        let at = pet.position.map(|q| Vec2::new(q[0], q[2])).filter(|q| grid.cell_of(*q).is_some()).unwrap_or_else(|| {
            let q = near + Vec2::new(rng.random_range(-3.0..3.0), rng.random_range(-3.0..3.0));
            grid.nearest_free(q).map_or(q, |(x, z)| grid.center_of(x, z))
        });
        let y = pet.position.map_or_else(|| world.data.heightmap.sample(at.x, at.y), |q| q[1]);
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        if let Some(e) = spawn_pet(&mut commands, p, breed, pet.name.clone(), Vec3::new(at.x, y, at.y), rng.random_range(0.0..6.28), &mut ctx, &mut assets, &mut bindposes) {
            commands.entity(e).insert(HomePet { id: pet.id, path: Vec::new(), until: 0.0 });
            info!("pet {} the {} ({}) is home", pet.name, p.breeds[breed].kind, pet.outfit);
        }
    }
}

/// A household's pets potter about the lot: off somewhere (by the lot's walk grid), then a
/// while standing about, sitting or lying down, and asleep at night.
#[allow(clippy::too_many_arguments)]
fn home_pets(
    time: Res<Time>,
    clock: Res<GameClock>,
    grid: Option<Res<crate::nav::NavGrid>>,
    world: Res<crate::loading::CurrentWorld>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    household: Option<Res<crate::interact::Household>>,
    mut q: Query<(&Pet, &mut HomePet, &mut Transform, &mut PetAnim), Without<PetBusy>>,
) {
    let Some(grid) = grid else { return };
    let lot = household.as_ref().and_then(|h| world.data.lots.get(h.lot_index));
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    let night = !(6.0..21.0).contains(&clock.hour_f());
    for (pet, mut h, mut tf, mut anim) in &mut q {
        let here = Vec2::new(tf.translation.x, tf.translation.z);
        if let Some(&next) = h.path.first() {
            let d = next - here;
            let step = pet.walk_speed() * dt;
            if d.length() <= step.max(0.05) {
                h.path.remove(0);
                if h.path.is_empty() {
                    h.until = clock.minutes + rng.random_range(10.0..60.0) as f64;
                    anim.play(idle_clip(&pet.kind, night, &mut rng));
                }
            } else {
                let p = here + d.normalize() * step;
                let y = crate::nav::floor_height(&world.data, building.as_deref(), 1, Vec3::new(p.x, tf.translation.y, p.y));
                tf.translation = Vec3::new(p.x, y, p.y);
                tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
                anim.play(format!("{}_walk_x", pet.kind));
            }
            continue;
        }
        if clock.minutes < h.until || night {
            if anim.clip.is_empty() || (night && !anim.clip.contains("sleep") && !pet.kind.ends_with('h')) {
                anim.play(idle_clip(&pet.kind, night, &mut rng));
            }
            continue;
        }
        // Somewhere else on the lot (never off it, into the street).
        let to = match lot {
            Some(l) => {
                let (x, z) = (rng.random_range(1.0..(l.width as f32 - 1.0).max(1.5)), rng.random_range(1.0..(l.depth as f32 - 1.0).max(1.5)));
                let (s, c) = l.rotation.sin_cos();
                let p = Vec3::from(l.corner) + Vec3::new(x * c + z * s, 0.0, -x * s + z * c);
                Vec2::new(p.x, p.z)
            }
            None => here + Vec2::new(rng.random_range(-8.0..8.0), rng.random_range(-8.0..8.0)),
        };
        // (Horses keep out of the house.)
        let horse = pet.kind.ends_with('h');
        let indoors = |p: Vec2| building.as_deref().is_some_and(|b| b.is_indoors(Vec3::new(p.x, 0.0, p.y)));
        if let Some((x, z)) = grid.nearest_free(to).filter(|(x, z)| !(horse && indoors(grid.center_of(*x, *z))))
            && let Some(path) = grid.find_path(here, grid.center_of(x, z)).filter(|p| !horse || !p.iter().any(|q| indoors(*q)))
        {
            h.path = path;
        } else {
            h.until = clock.minutes + 10.0;
        }
    }
}

/// Something to do while stopped: standing about, looking round, sitting, lying down; asleep at
/// night.
fn idle_clip(kind: &str, night: bool, rng: &mut impl Rng) -> String {
    // (Horses stand: swishing their tails, flicking their ears, pawing the ground, dozing on
    // their feet at night.)
    if kind == "ah" || kind == "ch" {
        let options = if night {
            vec!["idle_stand_breathe_x", "idle_stand_yawn_x"]
        } else {
            vec!["idle_stand_breathe_x", "idle_stand_breatheSwishTail_x", "idle_stand_lookAround_x", "idle_stand_earFlickL_x", "idle_stand_pawAtFloor_x", "idle_stand_sniffAround_x", "idle_stand_snort_x"]
        };
        return format!("{kind}_{}", options.choose(rng).copied().unwrap_or("idle_stand_breathe_x"));
    }
    if night {
        return format!("{kind}_sleep_loop1_x");
    }
    let options = [
        format!("{kind}_idle_stand_breathe_x"),
        format!("{kind}_idle_stand_lookAround_x"),
        format!("{kind}_idle_sit_breathe_x"),
        if kind == "ac" || kind == "cc" { format!("{kind}_idle_layDown_breathe_x") } else { format!("{kind}_idle_laydown_breathe_x") },
    ];
    options.choose(rng).cloned().unwrap_or_default()
}

/// The household's pets, for the save.
pub fn saved(pets: &Query<(&Pet, &HomePet, &Transform)>, data: Option<&PetsBaked>) -> Vec<SavedPet> {
    let Some(data) = data else { return Vec::new() };
    pets.iter()
        .filter_map(|(p, h, tf)| Some(SavedPet { id: h.id, name: p.name.clone(), kind: p.kind.clone(), outfit: data.breeds.get(p.breed)?.outfit, position: Some(tf.translation.to_array()) }))
        .collect()
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
    mut q: Query<(&Pet, &mut Stray, &mut Transform, &mut PetAnim), Without<PetBusy>>,
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
                    anim.play(idle_clip(&pet.kind, night, &mut rng));
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
