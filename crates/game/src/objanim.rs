//! Objects' own animations: a model with moving parts (a fridge's door, a dresser's drawers, a
//! shower's door, a swing's seat) is skinned to its rig (`skins.pack`), and while a Sim plays
//! a clip using it, the object plays the clip's other half, the game's `<clip>_<object>` (the
//! fridge door swinging open with `a2o_fridge_openDoor`). Left alone, it comes to rest again.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::prelude::*;
use s3bake::Key;

use crate::anim::{ClipLibrary, ClipPlayer, sample_track_quat, sample_track_vec};
use crate::baked::Baked;
use crate::interact::{ActionKind, ActionQueue, UsedBy};

pub struct ObjectAnimPlugin;

impl Plugin for ObjectAnimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Rigs>()
            .add_systems(Update, (animate_objects, swing_doors).chain().after(crate::anim::drive_skeletons))
            .add_systems(PostUpdate, attach_skins.before(bevy::transform::TransformSystems::Propagate));
    }
}

/// An object held in a pose of its own while nobody's using it, the last of a clip of its own:
/// a trash can knocked over (`a2o_trashCan_knock_over_pose_trashcan`).
#[derive(Component, Clone, Copy)]
pub struct HeldPose(pub &'static str);

/// A mesh part skinned to its model's rig (the model's key): given its bones once spawned.
#[derive(Component, Clone, Copy)]
pub struct SkinPart(pub Key);

/// An object's rigs (one per skinned model), and whether a clip has them out of their rest pose.
#[derive(Component, Default)]
pub struct ObjectRig {
    rigs: Vec<RigInst>,
    posed: bool,
}

struct RigInst {
    model: Key,
    bones: Vec<Entity>,
    rig: RigData,
}

/// A model's rig: its bones' name hashes, parents and rest poses, and inverse bind poses.
#[derive(Clone)]
struct RigData {
    hashes: Arc<Vec<u32>>,
    parents: Arc<Vec<i16>>,
    bind: Arc<Vec<Transform>>,
    inverse: Handle<SkinnedMeshInverseBindposes>,
    /// Each bone's partner across the model's middle (x = 0): the bone at its place mirrored
    /// (itself, down the middle), for playing a clip mirrored; and where across it each lies.
    partner: Arc<Vec<usize>>,
    across: Arc<Vec<f32>>,
}

impl RigData {
    fn parent(&self, i: usize) -> Option<usize> {
        usize::try_from(self.parents[i]).ok().filter(|&p| p < self.bind.len() && p != i)
    }

    /// A pose mirrored across the model's middle: each bone placed as its partner is,
    /// reflected (in the model's space, so whatever its own axes).
    fn mirrored(&self, pose: &[Transform]) -> Vec<Transform> {
        let n = pose.len();
        let mut global: Vec<Option<Mat4>> = vec![None; n];
        fn place(i: usize, rig: &RigData, pose: &[Transform], global: &mut Vec<Option<Mat4>>, depth: usize) -> Mat4 {
            if let Some(g) = global[i] {
                return g;
            }
            let local = pose[i].to_matrix();
            let g = match rig.parent(i).filter(|_| depth < 64) {
                Some(p) => place(p, rig, pose, global, depth + 1) * local,
                None => local,
            };
            global[i] = Some(g);
            g
        }
        let m = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        let reflected: Vec<Mat4> = (0..n).map(|j| m * place(self.partner[j], self, pose, &mut global, 0) * m).collect();
        (0..n)
            .map(|j| {
                let g = match self.parent(j) {
                    Some(p) => reflected[p].inverse() * reflected[j],
                    None => reflected[j],
                };
                Transform::from_matrix(g)
            })
            .collect()
    }
}

impl ObjectRig {
    /// Its bones as a clip has them at time `t` (those its tracks move), but for the bone named
    /// `held` and those above it (a prop's `transformBone`, placed in the hand by its holder).
    pub(crate) fn clip_pose(&self, clip: &s3formats::sim::Clip, t: f32, held: u32) -> Vec<(Entity, Transform)> {
        let mut out = Vec::new();
        for inst in &self.rigs {
            let mut above = vec![false; inst.rig.hashes.len()];
            if let Some(mut i) = inst.rig.hashes.iter().position(|h| *h == held) {
                above[i] = true;
                while let Some(p) = inst.rig.parent(i) {
                    if above[p] {
                        break;
                    }
                    above[p] = true;
                    i = p;
                }
            }
            for (i, h) in inst.rig.hashes.iter().enumerate() {
                let Some(track) = clip.tracks.get(h).filter(|_| !above[i]) else { continue };
                let mut tf = inst.rig.bind[i];
                if let Some(p) = sample_track_vec(&track.translation, t) {
                    tf.translation = p;
                }
                if let Some(r) = sample_track_quat(&track.rotation, t) {
                    tf.rotation = r;
                }
                out.push((inst.bones[i], tf));
            }
        }
        out
    }
}

/// The rigs loaded so far, by model.
#[derive(Resource, Default)]
struct Rigs(HashMap<Key, Option<RigData>>);

fn load_rig(data: &Baked, model: Key, bindposes: &mut Assets<SkinnedMeshInverseBindposes>) -> Option<RigData> {
    let skin = data.0.skin(&model)?;
    let bind: Vec<Transform> = skin
        .bones
        .iter()
        .map(|(_, _, p, r)| Transform { translation: Vec3::from(*p), rotation: Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize(), scale: Vec3::ONE })
        .collect();
    let parents: Vec<i16> = skin.bones.iter().map(|b| b.1).collect();
    // Each bone's place in the model: its parents' composed with its own.
    fn world(i: usize, bind: &[Transform], parents: &[i16], depth: usize) -> Mat4 {
        let local = bind[i].to_matrix();
        match usize::try_from(parents[i]).ok().filter(|&p| p < bind.len() && p != i && depth < 64) {
            Some(p) => world(p, bind, parents, depth + 1) * local,
            None => local,
        }
    }
    let worlds: Vec<Mat4> = (0..bind.len()).map(|i| world(i, &bind, &parents, 0)).collect();
    let inverse: Vec<Mat4> = worlds.iter().map(|w| w.inverse()).collect();
    // (A bone's partner: the one at its place mirrored, itself when it's in the middle.)
    let at: Vec<Vec3> = worlds.iter().map(|w| w.w_axis.truncate()).collect();
    let partner: Vec<usize> = (0..at.len())
        .map(|i| {
            let want = Vec3::new(-at[i].x, at[i].y, at[i].z);
            if at[i].distance(want) < 0.01 {
                return i;
            }
            (0..at.len()).filter(|&j| at[j].distance(want) < 0.01).min_by_key(|&j| j.abs_diff(i)).unwrap_or(i)
        })
        .collect();
    Some(RigData {
        hashes: Arc::new(skin.bones.iter().map(|b| b.0).collect()),
        parents: Arc::new(parents),
        bind: Arc::new(bind),
        inverse: bindposes.add(SkinnedMeshInverseBindposes::from(inverse)),
        partner: Arc::new(partner),
        across: Arc::new(at.iter().map(|p| p.x).collect()),
    })
}

/// Skinned parts get their model's bones (under the object, at rest), once per object.
fn attach_skins(
    mut commands: Commands,
    data: Option<Res<Baked>>,
    mut rigs: ResMut<Rigs>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    parts: Query<(Entity, &SkinPart, &ChildOf, &Mesh3d), Without<SkinnedMesh>>,
    roots: Query<&ObjectRig>,
    (mut assets, mut meshes): (ResMut<crate::objects::ObjectAssets>, ResMut<Assets<Mesh>>),
) {
    let Some(data) = data else { return };
    let mut made: HashMap<(Entity, Key), Vec<Entity>> = HashMap::new();
    for (e, part, parent, mesh) in &parts {
        let root = parent.parent();
        let Some(rig) = rigs.0.entry(part.0).or_insert_with(|| load_rig(&data, part.0, &mut bindposes)).clone() else {
            // (No rig to move it: drawn still, without its bones.)
            let still = assets.still(&mut meshes, &mesh.0);
            commands.entity(e).remove::<SkinPart>().insert(Mesh3d(still));
            continue;
        };
        let existing = roots.get(root).ok().and_then(|r| r.rigs.iter().find(|x| x.model == part.0)).map(|x| x.bones.clone());
        let bones = match made.get(&(root, part.0)).cloned().or(existing) {
            Some(b) => b,
            None => {
                let bones: Vec<Entity> = rig.bind.iter().map(|t| commands.spawn((*t, Visibility::default())).id()).collect();
                for (i, p) in rig.parents.iter().enumerate() {
                    let up = usize::try_from(*p).ok().filter(|&p| p < bones.len() && p != i).map_or(root, |p| bones[p]);
                    commands.entity(up).add_child(bones[i]);
                }
                made.insert((root, part.0), bones.clone());
                let inst = RigInst { model: part.0, bones: bones.clone(), rig: rig.clone() };
                commands.entity(root).queue_silenced(move |mut r: EntityWorldMut| match r.get_mut::<ObjectRig>() {
                    Some(mut o) => o.rigs.push(inst),
                    None => {
                        r.insert(ObjectRig { rigs: vec![inst], posed: false });
                    }
                });
                bones
            }
        };
        // (Its moving parts can swing outside the model's bounds.)
        commands.entity(e).insert((SkinnedMesh { inverse_bindposes: rig.inverse.clone(), joints: bones }, NoFrustumCulling));
    }
}

/// The doors' kinds of clips (the game's `GenericDoor` machine plays `o_<kind>_<state>_door<n>`
/// on a door's leaves), each door taking the kind whose clips move its bones.
const DOOR_KINDS: [&str; 6] = ["doorSingle", "doorDouble", "doorSliding", "doorSlidingDouble2x1", "door2x1TwoStory", "door4x1TwoStory"];
/// How near a door a Sim is to have it open (metres), and how long after the last has passed it
/// shuts (game minutes: `Door.kMinimumTimeBeforeClosingDoor`).
const DOOR_REACH: f32 = 1.1;
const DOOR_SHUT_AFTER: f64 = 10.0;

/// A door's leaves: the kind of clips, how many, which way they last opened (`Mirrored`: away from
/// the back), and what each is doing (the clip and since when, game seconds of real time).
#[derive(Component, Default)]
pub struct DoorSwing {
    kind: Option<&'static str>,
    leaves: u8,
    /// Each leaf's bones (those its opening moves): a leaf's clips move only its own.
    leaf_bones: Vec<Vec<u32>>,
    /// The doorway's middle, across the model (a two-tile door's is half a tile over).
    middle: f32,
    mirrored: bool,
    state: DoorState,
    since: f32,
    clear_since: f64,
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
enum DoorState {
    #[default]
    Shut,
    Opening,
    Open,
    Closing,
}

/// Whether a clip's track moves its bone (turns or slides it) between its start and end.
fn moves(t: &s3formats::sim::Track, duration: f32) -> bool {
    let turned = match (sample_track_quat(&t.rotation, 0.0), sample_track_quat(&t.rotation, duration)) {
        (Some(a), Some(b)) => a.angle_between(b) > 0.05,
        _ => false,
    };
    let slid = match (sample_track_vec(&t.translation, 0.0), sample_track_vec(&t.translation, duration)) {
        (Some(a), Some(b)) => a.distance(b) > 0.02,
        _ => false,
    };
    turned || slid
}

/// Whether a Sim's way, from where they are, goes through a doorway within the next couple of
/// metres (crosses the door's plane between its jambs): the door's own lane, as the game's
/// routes through a door portal.
fn goes_through(to_local: &bevy::math::Affine3A, at: Vec3, waypoints: &[crate::nav::Waypoint], middle: f32, half: f32) -> bool {
    let mut from = to_local.transform_point3(at);
    let mut gone = 0.0;
    for w in waypoints.iter().take(4) {
        let to = to_local.transform_point3(Vec3::new(w.p.x, at.y, w.p.y));
        if (from.z <= 0.0) != (to.z <= 0.0) {
            let t = from.z / (from.z - to.z);
            let x = from.x + (to.x - from.x) * t - middle;
            if x.abs() < half + 0.1 {
                return true;
            }
        }
        gone += from.distance(to);
        if gone > 2.0 * DOOR_REACH {
            break;
        }
        from = to;
    }
    false
}

/// Doors swing open as a Sim comes up to them (away from them: `open` from the front,
/// `openMirrored` from behind), stay open while anyone's at them, and shut ten game minutes after
/// the last has gone (the game's `DoorPortalComponent` on its lanes).
#[allow(clippy::type_complexity)]
fn swing_doors(
    mut commands: Commands,
    data: Option<Res<Baked>>,
    mut lib: ResMut<ClipLibrary>,
    clock: Res<crate::clock::GameClock>,
    time: Res<Time>,
    mut doors: Query<(Entity, &crate::interact::GameObject, &mut ObjectRig, &GlobalTransform, Option<&mut DoorSwing>)>,
    sims: Query<(&GlobalTransform, &Visibility, Option<&crate::nav::PathFollow>), With<crate::sim::Sim>>,
    mut bones: Query<&mut Transform, Without<ObjectRig>>,
) {
    let Some(data) = data else { return };
    let now = time.elapsed_secs();
    for (e, obj, mut rig, tf, swing) in &mut doors {
        if obj.kind != crate::interact::ObjectKind::Door {
            continue;
        }
        let Some(mut swing) = swing else {
            // (Which kind of clips move it, and how many leaves.)
            let hashes: Vec<u32> = rig.rigs.iter().flat_map(|r| r.rig.hashes.iter().copied()).collect();
            // (How many of its bones a kind's opening swings or slides: a kind's leaves' clips
            // looked for only where it has them.)
            let mut opening = |kind: &str, leaf: u32| {
                let name = format!("o_{kind}_open_door{leaf}");
                data.0.clip_names.iter().any(|n| *n == name).then(|| lib.get(&data, &name)).flatten()
            };
            let mut fits = |kind: &str| (0..2).map(|leaf| opening(kind, leaf).map_or(0, |c| c.tracks.iter().filter(|(h, t)| hashes.contains(h) && moves(t, c.duration)).count())).sum::<usize>();
            // (The first that moves the most of it: an ordinary door's own before a two-storey's,
            // whose leaves are named the same.)
            let mut kind: Option<(&'static str, usize)> = None;
            for k in DOOR_KINDS {
                let n = fits(k);
                if n > 0 && kind.is_none_or(|(_, best)| n > best) {
                    kind = Some((k, n));
                }
            }
            let kind = kind.map(|(k, _)| k);
            let leaf_bones: Vec<Vec<u32>> = match kind {
                Some(k) => (0..2)
                    .map(|leaf| opening(k, leaf).map(|c| c.tracks.iter().filter(|(h, t)| hashes.contains(h) && moves(t, c.duration)).map(|(h, _)| *h).collect()).unwrap_or_default())
                    .filter(|b: &Vec<u32>| !b.is_empty())
                    .collect(),
                None => Vec::new(),
            };
            let leaves = leaf_bones.len().max(1) as u8;
            // (Where across it the doorway's middle is: between a pair's hinges.)
            let middle = if leaves == 2 {
                let hinges: Vec<f32> = rig.rigs.iter().flat_map(|r| r.rig.hashes.iter().zip(r.rig.across.iter()).filter(|(h, _)| leaf_bones.iter().flatten().any(|b| b == *h)).map(|(_, x)| *x)).collect();
                hinges.iter().sum::<f32>() / hinges.len().max(1) as f32
            } else {
                0.0
            };
            commands.entity(e).insert(DoorSwing { kind, leaves, leaf_bones, middle, ..default() });
            continue;
        };
        let Some(kind) = swing.kind else { continue };
        // Who's going through it, or standing in it, and from which side (its front: the
        // model's -z, as the game's).
        let to_local = tf.affine().inverse();
        let half = 0.5 * swing.leaves as f32;
        let mut near: Option<f32> = None;
        for (stf, vis, path) in &sims {
            if *vis == Visibility::Hidden {
                continue;
            }
            let at = stf.translation();
            let p = to_local.transform_point3(at);
            let x = p.x - swing.middle;
            if p.y.abs() > 2.0 || p.z.abs() > DOOR_REACH || x.abs() > half + DOOR_REACH {
                continue;
            }
            let standing_in = p.z.abs() < 0.4 && x.abs() < half;
            if standing_in || path.is_some_and(|f| !f.done && goes_through(&to_local, at, &f.waypoints, swing.middle, half)) {
                near = Some(p.z);
            }
        }
        // (DOOR_TEST=front|behind: every door held open, for a look at it.)
        if let Ok(t) = std::env::var("DOOR_TEST") {
            near = Some(if t == "behind" { 1.0 } else { -1.0 });
        }
        if near.is_some() {
            swing.clear_since = clock.minutes;
        }
        let state = swing.state;
        match (state, near) {
            (DoorState::Shut | DoorState::Closing, Some(z)) => {
                debug!("door {} opens from the {}", obj.name, if z > 0.0 { "back" } else { "front" });
                swing.mirrored = z > 0.0;
                swing.state = DoorState::Opening;
                swing.since = now;
            }
            (DoorState::Open, None) if clock.minutes - swing.clear_since >= DOOR_SHUT_AFTER => {
                swing.state = DoorState::Closing;
                swing.since = now;
            }
            _ => {}
        }
        let m = if swing.mirrored { "Mirrored" } else { "" };
        let (clip, t, done) = match swing.state {
            DoorState::Shut => continue,
            DoorState::Opening => (format!("o_{kind}_open{m}_door"), now - swing.since, true),
            DoorState::Open => (format!("o_{kind}_open{m}Pose_door"), 0.0, false),
            DoorState::Closing => (format!("o_{kind}_close{m}_door"), now - swing.since, true),
        };
        let mut finished = true;
        for leaf in 0..swing.leaves {
            let Some(c) = lib.get(&data, &format!("{clip}{leaf}")) else { continue };
            let t = if done { t.min(c.duration.max(0.0)) } else { c.duration.max(0.0) };
            if done && now - swing.since < c.duration {
                finished = false;
            }
            let own = swing.leaf_bones.get(leaf as usize);
            for inst in &rig.rigs {
                for (i, h) in inst.rig.hashes.iter().enumerate() {
                    if own.is_some_and(|o| !o.contains(h)) {
                        continue;
                    }
                    let Some(track) = c.tracks.get(h) else { continue };
                    let Ok(mut b) = bones.get_mut(inst.bones[i]) else { continue };
                    if let Some(p) = sample_track_vec(&track.translation, t) {
                        b.translation = p;
                    }
                    if let Some(r) = sample_track_quat(&track.rotation, t) {
                        b.rotation = r;
                    }
                }
            }
        }
        rig.posed = true;
        if done && finished {
            swing.state = if swing.state == DoorState::Opening { DoorState::Open } else { DoorState::Shut };
            if swing.state == DoorState::Shut {
                for inst in &rig.rigs {
                    for (i, &bone) in inst.bones.iter().enumerate() {
                        if let Ok(mut b) = bones.get_mut(bone) {
                            *b = inst.rig.bind[i];
                        }
                    }
                }
                rig.posed = false;
            }
        }
    }
}

/// The object's half of a Sim's clip (`a2o_fridge_openDoor_x` → `a2o_fridge_openDoor_fridge`):
/// a clip of the same name for an actor that's neither the Sim nor a prop.
fn companion(data: &Baked, name: &str) -> Option<String> {
    let stem = name.strip_suffix("_x")?;
    let want = format!("{}_", stem.to_ascii_lowercase());
    data.0
        .clip_names
        .iter()
        .find(|n| {
            let l = n.to_ascii_lowercase();
            l.strip_prefix(&want).is_some_and(|rest| !rest.is_empty() && !rest.contains('_') && rest != "x" && rest != "y" && !crate::props::prop_actor(rest))
        })
        .cloned()
}

/// The pose an object's held in through a Sim's clips that don't move it: a bath's water up
/// (`a2o_bathtub_waterUpPose_bathtub`) once the taps are on, until they're out of the tub.
fn held_pose(data: &Baked, name: &str) -> Option<String> {
    let l = name.to_ascii_lowercase();
    // (A grown-up's or a child's clip at the tub.)
    let bathing = l.get(1..).is_some_and(|r| r.starts_with("2o_bathtub_")) && !["repair", "clean", "upgrade"].iter().any(|n| l.contains(n));
    bathing.then(|| data.0.clip_names.iter().find(|n| n.eq_ignore_ascii_case("a2o_bathtub_waterUpPose_bathtub")).cloned()).flatten()
}

/// While a Sim plays a clip at an object with moving parts, the object plays its half of it
/// (in step); once nobody's using it, it's back at rest.
#[allow(clippy::type_complexity)]
fn animate_objects(
    data: Option<Res<Baked>>,
    mut lib: ResMut<ClipLibrary>,
    sims: Query<(&ClipPlayer, &ActionQueue, Option<&crate::anim::ActionClip>, &GlobalTransform)>,
    mut objects: Query<(Entity, &mut ObjectRig, Option<&UsedBy>, &GlobalTransform, Has<crate::interact::Unmade>, Option<&HeldPose>), Without<DoorSwing>>,
    mut bones: Query<&mut Transform, Without<ObjectRig>>,
    mut companions: Local<HashMap<String, Option<String>>>,
    mut animated: Local<Vec<Entity>>,
) {
    let Some(data) = data else { return };
    animated.clear();
    let target_of = |queue: &ActionQueue| {
        queue.0.front().and_then(|a| match a.kind {
            ActionKind::Object { target, .. } | ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } | ActionKind::Outro { target, .. } => Some(target),
            // (A crib's side let down for a little one lifted out or put in.)
            ActionKind::Carry { step: crate::carry::CarryStep::PickUp { crib: Some(c) } | crate::carry::CarryStep::InCrib(c), .. } => Some(c),
            _ => None,
        })
    };
    // (How many are at each object: two in a double bed each move their own side of it.)
    // (Counting those whose clip moves it: WooHoo's other part leaves the covers to the first.)
    let mut users: HashMap<Entity, usize> = HashMap::new();
    for (player, queue, ..) in &sims {
        if let Some(t) = target_of(queue)
            && companions.entry(player.name.clone()).or_insert_with(|| companion(&data, &player.name).or_else(|| held_pose(&data, &player.name))).is_some()
        {
            *users.entry(t).or_default() += 1;
        }
    }
    for (player, queue, action, sim_tf) in &sims {
        let Some(target) = target_of(queue) else { continue };
        let Ok((_, mut rig, _, obj_tf, ..)) = objects.get_mut(target) else { continue };
        let Some(name) = companions.entry(player.name.clone()).or_insert_with(|| companion(&data, &player.name).or_else(|| held_pose(&data, &player.name))).clone() else { continue };
        let Some(clip) = lib.get(&data, &name) else { continue };
        let t = player.time.clamp(0.0, clip.duration.max(0.0));
        // (Played mirrored with the Sim's: the far side of a double bed.)
        let mirror = action.is_some_and(|a| a.mirror);
        // (Shared: only the bones on this Sim's side of the object's middle.)
        let side = (users.get(&target).copied().unwrap_or(0) > 1).then(|| obj_tf.affine().inverse().transform_point3(sim_tf.translation()).x.signum());
        for inst in &rig.rigs {
            let mut pose: Vec<Transform> = inst.rig.bind.to_vec();
            let mut moved = vec![false; pose.len()];
            for (i, h) in inst.rig.hashes.iter().enumerate() {
                let Some(track) = clip.tracks.get(h) else { continue };
                if let Some(p) = sample_track_vec(&track.translation, t) {
                    pose[i].translation = p;
                }
                if let Some(r) = sample_track_quat(&track.rotation, t) {
                    pose[i].rotation = r;
                }
                moved[i] = true;
            }
            if mirror {
                pose = inst.rig.mirrored(&pose);
                moved = (0..pose.len()).map(|j| moved[inst.rig.partner[j]]).collect();
            }
            for (i, &bone) in inst.bones.iter().enumerate() {
                let x = inst.rig.across.get(i).copied().unwrap_or(0.0);
                if !moved[i] || side.is_some_and(|s| x.abs() > 0.05 && x.signum() != s) {
                    continue;
                }
                if let Ok(mut tf) = bones.get_mut(bone) {
                    *tf = pose[i];
                }
            }
        }
        rig.posed = true;
        animated.push(target);
    }
    // At rest again once nobody's using it.
    // (An unmade bed's covers stay as they were left.)
    for (e, mut rig, used, _, unmade, held) in &mut objects {
        // (Held in its own pose: the clip's last.)
        if let Some(h) = held.filter(|_| !animated.contains(&e))
            && let Some(clip) = lib.get(&data, h.0)
        {
            let t = clip.duration.max(0.0);
            for inst in &rig.rigs {
                for (i, h) in inst.rig.hashes.iter().enumerate() {
                    let Some(track) = clip.tracks.get(h) else { continue };
                    let Ok(mut tf) = bones.get_mut(inst.bones[i]) else { continue };
                    if let Some(p) = sample_track_vec(&track.translation, t) {
                        tf.translation = p;
                    }
                    if let Some(r) = sample_track_quat(&track.rotation, t) {
                        tf.rotation = r;
                    }
                }
            }
            rig.posed = true;
            continue;
        }
        if !rig.posed || animated.contains(&e) || used.is_some_and(|u| u.0.is_some()) || unmade {
            continue;
        }
        rig.posed = false;
        for inst in &rig.rigs {
            for (i, &bone) in inst.bones.iter().enumerate() {
                if let Ok(mut tf) = bones.get_mut(bone) {
                    *tf = inst.rig.bind[i];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::Waypoint;

    fn way(points: &[(f32, f32)]) -> Vec<Waypoint> {
        points.iter().map(|&(x, z)| Waypoint { p: Vec2::new(x, z), level: 1, climb: None, spiral: None }).collect()
    }

    #[test]
    fn walking_through_a_doorway() {
        // A door at (10, 0, 20), turned a quarter: its front faces the world's -x.
        let door = Transform::from_xyz(10.0, 0.0, 20.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let to_local = door.compute_affine().inverse();
        let at = Vec3::new(9.2, 0.0, 20.0);
        // Straight through it.
        assert!(goes_through(&to_local, at, &way(&[(10.8, 20.0), (12.0, 20.0)]), 0.0, 0.5));
        // Along the wall it's in, past it: not through.
        assert!(!goes_through(&to_local, at, &way(&[(9.2, 22.0), (9.2, 24.0)]), 0.0, 0.5));
        // Through the wall a couple of metres along: not this doorway.
        assert!(!goes_through(&to_local, at, &way(&[(9.2, 22.0), (10.8, 22.0)]), 0.0, 0.5));
        // A two-tile door's doorway is a tile wide either side of its middle.
        assert!(goes_through(&to_local, Vec3::new(9.2, 0.0, 20.9), &way(&[(10.8, 20.9)]), 0.5, 1.0) != goes_through(&to_local, Vec3::new(9.2, 0.0, 20.9), &way(&[(10.8, 20.9)]), -0.5, 1.0));
    }
}
