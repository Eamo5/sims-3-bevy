//! Sims: identity, motives (needs), mood, and their bodies.


use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::SimDelta;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimAssets>()
            .add_systems(Startup, init_sim_assets)
            .add_systems(
                Update,
                (decay_motives, animate_bodies, update_plumbobs).run_if(in_state(PlayMode::Live)),
            );
    }
}

pub const MOTIVE_NAMES: [&str; 6] = ["Hunger", "Bladder", "Energy", "Social", "Hygiene", "Fun"];
pub const HUNGER: usize = 0;
pub const BLADDER: usize = 1;
pub const ENERGY: usize = 2;
pub const SOCIAL: usize = 3;
pub const HYGIENE: usize = 4;
pub const FUN: usize = 5;

/// Change per game hour while awake and idle.
const DECAY_PER_HOUR: [f32; 6] = [-7.5, -11.0, -5.5, -6.0, -4.5, -6.5];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Age {
    Baby,
    Toddler,
    Child,
    Teen,
    YoungAdult,
    Adult,
    Elder,
}

impl Age {
    /// Babies and toddlers, who can't look after themselves.
    pub fn is_little(self) -> bool {
        matches!(self, Age::Baby | Age::Toddler)
    }
    /// Young adults, adults and elders.
    pub fn is_grown(self) -> bool {
        matches!(self, Age::YoungAdult | Age::Adult | Age::Elder)
    }
}

#[derive(Component, Clone)]
#[require(crate::nav::Floor, crate::life::Moodlets, crate::life::Mood)]
pub struct Sim {
    /// Stable identity (saves, relationships).
    pub id: u64,
    /// Clothes and hair picked in Create-a-Sim (empty: chosen from `look`).
    pub outfit: OutfitChoice,
    /// Seed for the CAS outfit, so a Sim always looks the same.
    pub look: u64,
    pub first: String,
    pub last: String,
    pub female: bool,
    pub age: Age,
    pub traits: Vec<crate::life::Trait>,
    pub skin: Color,
    pub hair: Color,
    /// Eye colour (the iris's, as Create a Sim's colours are: about half-strength, doubled
    /// over the iris's own shading).
    pub eyes: Color,
    /// Which of the game's three voices they speak Simlish in (0..2).
    pub voice: u8,
    pub top: Color,
    pub bottom: Color,
    /// Body shape: weight from thin (-1) to heavy (1), and fitness (0..1).
    pub weight: f32,
    pub fitness: f32,
}

/// CAS parts chosen for a Sim (baked part keys).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutfitChoice {
    pub hair: Option<s3bake::Key>,
    pub top: Option<s3bake::Key>,
    pub bottom: Option<s3bake::Key>,
    pub full: Option<s3bake::Key>,
    pub shoes: Option<s3bake::Key>,
    /// Facial hair, glasses and make-up (`NONE`: chosen to go without).
    pub beard: Option<s3bake::Key>,
    pub glasses: Option<s3bake::Key>,
    pub lipstick: Option<s3bake::Key>,
    pub eyeshadow: Option<s3bake::Key>,
    /// The clothes chosen for their other outfits (in `OTHER_OUTFITS`' order).
    pub other: [Clothes; 4],
}

/// Clothes chosen for one of a Sim's outfits (none chosen: as their look gives them).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clothes {
    pub top: Option<s3bake::Key>,
    pub bottom: Option<s3bake::Key>,
    pub full: Option<s3bake::Key>,
    pub shoes: Option<s3bake::Key>,
}

/// The outfits besides their everyday clothes that can be chosen, as the game's Create a Sim
/// has them.
pub const OTHER_OUTFITS: [crate::simbody::OutfitKind; 4] =
    [crate::simbody::OutfitKind::Formal, crate::simbody::OutfitKind::Sleepwear, crate::simbody::OutfitKind::Athletic, crate::simbody::OutfitKind::Swimwear];

impl OutfitChoice {
    /// A face choice of nothing at all.
    pub const NONE: s3bake::Key = (0, 0, 0);

    /// The clothes chosen for an outfit.
    pub fn clothes(&self, kind: crate::simbody::OutfitKind) -> Clothes {
        match OTHER_OUTFITS.iter().position(|k| *k == kind) {
            Some(i) => self.other[i],
            None => Clothes { top: self.top, bottom: self.bottom, full: self.full, shoes: self.shoes },
        }
    }

    /// Chooses a piece of clothing for an outfit (a top or bottom in place of an all-in-one,
    /// and the other way round).
    pub fn wear(&mut self, kind: crate::simbody::OutfitKind, t: u32, key: s3bake::Key) {
        use s3formats::sim::{CT_BODY, CT_BOTTOM, CT_SHOES, CT_TOP};
        let mut c = self.clothes(kind);
        match t {
            CT_TOP => {
                c.top = Some(key);
                c.full = None;
            }
            CT_BOTTOM => {
                c.bottom = Some(key);
                c.full = None;
            }
            CT_BODY => {
                c.full = Some(key);
                c.top = None;
                c.bottom = None;
            }
            CT_SHOES => c.shoes = Some(key),
            _ => return,
        }
        match OTHER_OUTFITS.iter().position(|k| *k == kind) {
            Some(i) => self.other[i] = c,
            None => {
                self.top = c.top;
                self.bottom = c.bottom;
                self.full = c.full;
                self.shoes = c.shoes;
            }
        }
    }
}

impl Sim {
    pub fn full_name(&self) -> String {
        format!("{} {}", self.first, self.last)
    }
}

/// Needs in the range -100 (desperate) to 100 (fully satisfied).
#[derive(Component, Clone)]
pub struct Motives(pub [f32; 6]);

impl Default for Motives {
    fn default() -> Self {
        Self([70.0, 60.0, 80.0, 50.0, 70.0, 50.0])
    }
}

impl Motives {
    pub fn add(&mut self, i: usize, v: f32) {
        self.0[i] = (self.0[i] + v).clamp(-100.0, 100.0);
    }

    /// Overall mood from -100 to 100.
    pub fn mood(&self) -> f32 {
        let weights = [1.3, 1.1, 1.2, 0.8, 0.8, 0.9];
        let mut s = 0.0;
        let mut w = 0.0;
        for i in 0..6 {
            // Low motives drag mood down more than high ones lift it.
            let v = self.0[i];
            let v = if v < 0.0 { v * 1.5 } else { v };
            s += v * weights[i];
            w += weights[i];
        }
        (s / w).clamp(-100.0, 100.0)
    }

    pub fn lowest(&self) -> (usize, f32) {
        let mut best = (0, f32::MAX);
        for i in 0..6 {
            if self.0[i] < best.1 {
                best = (i, self.0[i]);
            }
        }
        best
    }
}

/// Multipliers applied to the normal decay (e.g. sleeping slows hunger).
#[derive(Component, Clone, Copy)]
pub struct DecayScale(pub [f32; 6]);

impl Default for DecayScale {
    fn default() -> Self {
        Self([1.0; 6])
    }
}

#[derive(Component)]
pub struct HouseholdMember;

#[derive(Component)]
pub struct Selected;

pub use crate::social::Relationships;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pose {
    #[default]
    Stand,
    Walk,
    Sit,
    Lie,
    Use,
    Talk,
    Dance,
    Exercise,
}

/// Current animation state of a sim.
#[derive(Component, Default)]
pub struct SimAnim {
    pub pose: Pose,
    pub phase: f32,
    /// Height offset used for sitting / lying on furniture.
    pub seat_height: f32,
}

/// Handles to the stand-in body parts (used when no CAS body could be built).
#[derive(Component)]
pub struct SimBody {
    pub root: Entity,
    pub torso: Entity,
    pub head: Entity,
    pub legs: [Entity; 2],
    pub arms: [Entity; 2],
}

/// The plumbob floating above the sim.
#[derive(Component)]
pub struct PlumbobRef(pub Entity);

#[derive(Resource, Default)]
pub struct SimAssets {
    pub limb: Handle<Mesh>,
    pub torso: Handle<Mesh>,
    pub head: Handle<Mesh>,
    pub hair: Handle<Mesh>,
    pub plumbob: Handle<Mesh>,
    pub plumbob_mat: Handle<StandardMaterial>,
}

fn octahedron() -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let v = [
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(0.0, -1.0, 0.0),
    ];
    let faces = [[0, 2, 1], [0, 3, 2], [0, 4, 3], [0, 1, 4], [5, 1, 2], [5, 2, 3], [5, 3, 4], [5, 4, 1]];
    let mut pos = Vec::new();
    let mut nrm = Vec::new();
    for f in faces {
        let (a, b, c) = (v[f[0]] * Vec3::new(0.5, 1.0, 0.5), v[f[1]] * Vec3::new(0.5, 1.0, 0.5), v[f[2]] * Vec3::new(0.5, 1.0, 0.5));
        let n = (b - a).cross(c - a).normalize();
        for p in [a, b, c] {
            pos.push(p.to_array());
            nrm.push(n.to_array());
        }
    }
    let n = pos.len() as u32;
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    m.insert_indices(Indices::U32((0..n).collect()));
    m
}

fn init_sim_assets(
    mut a: ResMut<SimAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    a.limb = meshes.add(Capsule3d::new(0.065, 0.34));
    a.torso = meshes.add(Capsule3d::new(0.17, 0.32));
    a.head = meshes.add(Sphere::new(0.13).mesh().uv(24, 16));
    a.hair = meshes.add(Sphere::new(0.14).mesh().uv(24, 16));
    a.plumbob = meshes.add(octahedron());
    a.plumbob_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.95, 0.2),
        emissive: LinearRgba::rgb(0.1, 0.8, 0.1),
        ..default()
    });
}

const FIRST_F: [&str; 16] = [
    "Bella", "Cassandra", "Nina", "Ursula", "Jocasta", "Pauline", "Holly", "Agnes", "Molly", "Darcy", "Tori",
    "Iris", "Hana", "Nancy", "Gretchen", "Mia",
];
const FIRST_M: [&str; 16] = [
    "Mortimer", "Gunther", "Ethan", "Hank", "Jared", "Darren", "Nick", "Tony", "Malcolm", "Geoffrey", "Bill",
    "Vlad", "Mike", "Parker", "Elvis", "Jamie",
];
const LAST: [&str; 12] = [
    "Goth", "Landgraab", "Alto", "Bunch", "Keaton", "Frio", "Hart", "Wolff", "Steel", "Andrews", "Clavell", "Kennedy",
];
pub const SKINS: [(f32, f32, f32); 5] = [(0.96, 0.80, 0.69), (0.87, 0.68, 0.53), (0.72, 0.53, 0.38), (0.55, 0.38, 0.26), (0.38, 0.26, 0.18)];
/// Hair colours, as Create-a-Sim offers them: black, dark brown, brown, blonde, red and grey.
pub const HAIRS: [(f32, f32, f32); 6] = [(0.08, 0.06, 0.05), (0.30, 0.18, 0.08), (0.55, 0.35, 0.15), (0.85, 0.70, 0.40), (0.60, 0.20, 0.10), (0.55, 0.55, 0.55)];

/// Create a Sim's eye colours (the eye colour part's presets): hazel, blue, olive, green, brown,
/// amber, grey and violet.
pub const EYES: [(f32, f32, f32); 8] = [
    (0.5647, 0.5098, 0.4392),
    (0.4902, 0.5490, 0.6471),
    (0.5255, 0.5137, 0.3098),
    (0.4275, 0.5059, 0.4667),
    (0.5882, 0.4118, 0.3333),
    (0.6275, 0.5098, 0.3333),
    (0.5490, 0.5490, 0.5490),
    (0.5373, 0.4353, 0.5333),
];

/// An eye colour from the presets, by a Sim's look (for Sims made before eye colours).
pub fn eyes_by_look(look: u64) -> Color {
    // (The look's bits mixed, so a family's looks don't all land on one colour.)
    let h = (look ^ 0xE7E5_C010).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let (r, g, b) = EYES[((h >> 32) % EYES.len() as u64) as usize];
    Color::srgb(r, g, b)
}

pub fn random_sim(rng: &mut impl Rng, last: &str, female: Option<bool>, age: Age) -> Sim {
    let female = female.unwrap_or_else(|| rng.random_bool(0.5));
    let first = if female { FIRST_F[rng.random_range(0..FIRST_F.len())] } else { FIRST_M[rng.random_range(0..FIRST_M.len())] };
    let (sr, sg, sb) = SKINS[rng.random_range(0..SKINS.len())];
    let (hr, hg, hb) = HAIRS[rng.random_range(0..HAIRS.len())];
    let hue = rng.random_range(0.0..360.0);
    Sim {
        id: rng.random(),
        outfit: OutfitChoice::default(),
        look: rng.random(),
        first: first.to_string(),
        last: last.to_string(),
        female,
        age,
        traits: crate::life::random_traits(rng, age),
        skin: Color::srgb(sr, sg, sb),
        hair: Color::srgb(hr, hg, hb),
        top: Color::hsl(hue, 0.55, 0.5),
        bottom: Color::hsl((hue + 180.0) % 360.0, 0.3, 0.3),
        // Most Sims are of middling build; a few are thin, heavy or very fit.
        weight: if age.is_little() { 0.0 } else { ((rng.random_range(-1.0f32..1.0) + rng.random_range(-1.0f32..1.0)) * 0.45).clamp(-1.0, 1.0) },
        fitness: if age.is_little() { 0.0 } else { (rng.random_range(0.0f32..1.0) * rng.random_range(0.0f32..1.0)).min(1.0) },
        eyes: {
            let (r, g, b) = EYES[rng.random_range(0..EYES.len())];
            Color::srgb(r, g, b)
        },
        voice: rng.random_range(0..3),
    }
}

pub fn random_last_name(rng: &mut impl Rng) -> String {
    LAST[rng.random_range(0..LAST.len())].to_string()
}

/// Asset stores needed to spawn a sim body.
pub struct SimSpawnCtx<'a> {
    pub assets: &'a SimAssets,
    pub render: crate::simbody::SimRenderCtx<'a>,
}

/// Spawns a sim: a real CAS body when `model` is given, otherwise the stand-in body.
pub fn spawn_sim_full(
    commands: &mut Commands,
    ctx: &mut SimSpawnCtx,
    sim: Sim,
    pos: Vec3,
    model: Option<crate::simbody::SimModelCpu>,
) -> Entity {
    let Some(model) = model else {
        return spawn_sim(commands, ctx.assets, ctx.render.mats, sim, pos);
    };
    let scale = if sim.age == Age::Child { 1.0 } else { 1.0 };
    let entity = commands
        .spawn((
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
            Visibility::default(),
            sim,
            Motives::default(),
            DecayScale::default(),
            Relationships::default(),
            SimAnim::default(),
            crate::anim::ClipPlayer::default(),
        ))
        .id();
    crate::simbody::spawn_sim_model(commands, entity, model, &mut ctx.render);
    let plumbob = commands
        .spawn((
            Mesh3d(ctx.assets.plumbob.clone()),
            MeshMaterial3d(ctx.render.mats.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.95, 0.2),
                emissive: LinearRgba::rgb(0.1, 0.9, 0.1),
                ..default()
            })),
            Transform::from_xyz(0.0, 2.15, 0.0).with_scale(Vec3::splat(0.22)),
            Visibility::Hidden,
        ))
        .id();
    commands.entity(entity).add_child(plumbob).insert(PlumbobRef(plumbob));
    entity
}

/// Spawns a sim with a simple articulated body at `pos`.
pub fn spawn_sim(
    commands: &mut Commands,
    assets: &SimAssets,
    mats: &mut Assets<StandardMaterial>,
    sim: Sim,
    pos: Vec3,
) -> Entity {
    let scale = match sim.age {
        Age::Baby => 0.3,
        Age::Toddler => 0.45,
        Age::Child => 0.65,
        Age::Teen => 0.95,
        Age::Elder => 0.96,
        _ => 1.0,
    };
    let skin = mats.add(StandardMaterial { base_color: sim.skin, perceptual_roughness: 0.6, ..default() });
    let hair = mats.add(StandardMaterial { base_color: sim.hair, perceptual_roughness: 0.8, ..default() });
    let top = mats.add(StandardMaterial { base_color: sim.top, perceptual_roughness: 0.85, ..default() });
    let bottom = mats.add(StandardMaterial { base_color: sim.bottom, perceptual_roughness: 0.85, ..default() });
    let female = sim.female;

    let entity = commands
        .spawn((
            Transform::from_translation(pos),
            Visibility::default(),
            sim,
            Motives::default(),
            DecayScale::default(),
            Relationships::default(),
            SimAnim::default(),
        ))
        .id();

    let root = commands.spawn((Transform::from_scale(Vec3::splat(scale)), Visibility::default())).id();
    let torso = commands
        .spawn((
            Mesh3d(assets.torso.clone()),
            MeshMaterial3d(top.clone()),
            Transform::from_xyz(0.0, 1.12, 0.0).with_scale(if female { Vec3::new(0.88, 1.0, 0.8) } else { Vec3::new(1.0, 1.0, 0.82) }),
        ))
        .id();
    let head = commands
        .spawn((Mesh3d(assets.head.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, 1.62, 0.0)))
        .with_children(|h| {
            let hair_tf = if female {
                Transform::from_xyz(0.0, 0.0, -0.035).with_scale(Vec3::new(1.05, 1.15, 1.1))
            } else {
                Transform::from_xyz(0.0, 0.035, -0.02).with_scale(Vec3::new(1.02, 0.85, 1.02))
            };
            h.spawn((Mesh3d(assets.hair.clone()), MeshMaterial3d(hair.clone()), hair_tf));
        })
        .id();
    let mut limb = |x: f32, y: f32, len_scale: f32, mat: &Handle<StandardMaterial>| {
        // A pivot at the joint with the limb hanging below it.
        let pivot = commands.spawn((Transform::from_xyz(x, y, 0.0), Visibility::default())).id();
        let l = commands
            .spawn((
                Mesh3d(assets.limb.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(0.0, -0.24 * len_scale, 0.0).with_scale(Vec3::new(1.0, len_scale, 1.0)),
            ))
            .id();
        commands.entity(pivot).add_child(l);
        pivot
    };
    let legs = [limb(-0.09, 0.86, 1.65, &bottom), limb(0.09, 0.86, 1.65, &bottom)];
    let arms = [limb(-0.22, 1.40, 1.25, &top), limb(0.22, 1.40, 1.25, &top)];
    let plumbob = commands
        .spawn((
            Mesh3d(assets.plumbob.clone()),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.95, 0.2),
                emissive: LinearRgba::rgb(0.1, 0.9, 0.1),
                ..default()
            })),
            Transform::from_xyz(0.0, 2.15, 0.0).with_scale(Vec3::splat(0.22)),
            Visibility::Hidden,
        ))
        .id();
    commands.entity(root).add_children(&[torso, head, legs[0], legs[1], arms[0], arms[1]]);
    commands.entity(entity).add_children(&[root, plumbob]);
    commands.entity(entity).insert((SimBody { root, torso, head, legs, arms }, PlumbobRef(plumbob)));
    entity
}

#[allow(clippy::type_complexity)]
fn decay_motives(
    delta: Res<SimDelta>,
    // (Sims at home elsewhere in town live off stage: they come over as they were.)
    mut q: Query<(&mut Motives, &DecayScale, &Sim, Option<&crate::wishes::Wishes>, Has<crate::careers::AtWork>, Has<crate::rabbitholes::AtRabbitHole>), Without<crate::interact::OffLot>>,
) {
    let hours = delta.0 / 60.0;
    if hours <= 0.0 {
        return;
    }
    for (mut m, scale, sim, wishes, at_work, away) in &mut q {
        for i in 0..6 {
            // (At work, school or out in town, lunch and the bathrooms are there: what the place
            // does to their hunger and bladder is its own.)
            if (at_work || away) && (i == HUNGER || i == BLADDER) {
                continue;
            }
            let d = DECAY_PER_HOUR[i] * scale.0[i] * crate::life::decay_rate(&sim.traits, i) * crate::wishes::reward_decay(wishes, i) * hours;
            m.add(i, d);
        }
    }
}

fn animate_bodies(
    time: Res<Time>,
    delta: Res<SimDelta>,
    mut sims: Query<(&mut SimAnim, &SimBody)>,
    mut tfs: Query<&mut Transform, Without<SimAnim>>,
) {
    let paused = delta.0 <= 0.0;
    for (mut anim, body) in &mut sims {
        if !paused {
            let rate = match anim.pose {
                Pose::Walk => 9.0,
                Pose::Dance => 6.0,
                Pose::Exercise => 11.0,
                Pose::Talk => 3.0,
                _ => 1.5,
            };
            anim.phase += time.delta_secs() * rate * (delta.0 / time.delta_secs().max(1e-4)).min(4.0).max(0.5);
        }
        let p = anim.phase;
        let (mut leg, mut arm, mut root_y, mut root_rot, mut arm_out) = (0.0f32, 0.0f32, 0.0f32, Quat::IDENTITY, 0.0f32);
        let mut legs_fwd = 0.0f32;
        match anim.pose {
            Pose::Stand => {
                arm = (p * 0.7).sin() * 0.03;
            }
            Pose::Walk => {
                leg = p.sin() * 0.5;
                arm = -p.sin() * 0.45;
                root_y = (p * 2.0).sin().abs() * 0.03;
            }
            Pose::Sit => {
                legs_fwd = 1.45;
                root_y = anim.seat_height - 0.42;
                arm = 0.35;
            }
            Pose::Lie => {
                root_rot = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                root_y = anim.seat_height + 0.15;
            }
            Pose::Use => {
                arm = 0.9 + (p * 2.0).sin() * 0.15;
            }
            Pose::Talk => {
                arm = 0.25 + (p * 1.3).sin() * 0.25;
                arm_out = (p * 0.9).sin().abs() * 0.3;
            }
            Pose::Dance => {
                leg = p.sin() * 0.3;
                arm = 1.8 + (p * 2.0).sin() * 0.6;
                arm_out = 0.4;
                root_y = (p * 2.0).sin().abs() * 0.08;
            }
            Pose::Exercise => {
                leg = p.sin() * 0.8;
                arm = -p.sin() * 0.8;
                root_y = (p * 2.0).sin().abs() * 0.06;
            }
        }
        if let Ok(mut t) = tfs.get_mut(body.root) {
            let s = t.scale;
            *t = Transform::from_translation(Vec3::new(0.0, root_y, if anim.pose == Pose::Lie { -0.9 } else { 0.0 }))
                .with_rotation(root_rot)
                .with_scale(s);
        }
        for (i, &l) in body.legs.iter().enumerate() {
            if let Ok(mut t) = tfs.get_mut(l) {
                let sign = if i == 0 { 1.0 } else { -1.0 };
                t.rotation = Quat::from_rotation_x(-legs_fwd + leg * sign);
            }
        }
        for (i, &a) in body.arms.iter().enumerate() {
            if let Ok(mut t) = tfs.get_mut(a) {
                let sign = if i == 0 { 1.0 } else { -1.0 };
                let swing = if matches!(anim.pose, Pose::Walk | Pose::Exercise) { arm * sign } else { -arm };
                t.rotation = Quat::from_rotation_x(swing) * Quat::from_rotation_z(-arm_out * sign);
            }
        }
    }
}

pub fn mood_color(mood: f32) -> Color {
    if mood > 25.0 {
        Color::srgb(0.2, 0.95, 0.2)
    } else if mood > -25.0 {
        Color::srgb(0.95, 0.85, 0.15)
    } else {
        Color::srgb(0.95, 0.2, 0.15)
    }
}

fn update_plumbobs(
    time: Res<Time>,
    sims: Query<(&PlumbobRef, &crate::life::Mood, Has<Selected>)>,
    mut q: Query<(&mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for (plumbob, mood, selected) in &sims {
        if let Ok((mut tf, mut vis, mat)) = q.get_mut(plumbob.0) {
            *vis = if selected { Visibility::Inherited } else { Visibility::Hidden };
            tf.rotation = Quat::from_rotation_y(time.elapsed_secs() * 1.5);
            tf.translation.y = 2.15 + (time.elapsed_secs() * 2.0).sin() * 0.03;
            if selected && let Some(mut m) = mats.get_mut(&mat.0) {
                let c = mood_color(mood.level());
                m.base_color = c;
                m.emissive = c.to_linear() * 0.8;
            }
        }
    }
}
