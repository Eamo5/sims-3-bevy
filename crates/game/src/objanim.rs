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
            .add_systems(Update, animate_objects.after(crate::anim::drive_skeletons))
            .add_systems(PostUpdate, attach_skins.before(bevy::transform::TransformSystems::Propagate));
    }
}

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
    let inverse: Vec<Mat4> = (0..bind.len()).map(|i| world(i, &bind, &parents, 0).inverse()).collect();
    Some(RigData {
        hashes: Arc::new(skin.bones.iter().map(|b| b.0).collect()),
        parents: Arc::new(parents),
        bind: Arc::new(bind),
        inverse: bindposes.add(SkinnedMeshInverseBindposes::from(inverse)),
    })
}

/// Skinned parts get their model's bones (under the object, at rest), once per object.
fn attach_skins(
    mut commands: Commands,
    data: Option<Res<Baked>>,
    mut rigs: ResMut<Rigs>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    parts: Query<(Entity, &SkinPart, &ChildOf), Without<SkinnedMesh>>,
    roots: Query<&ObjectRig>,
) {
    let Some(data) = data else { return };
    let mut made: HashMap<(Entity, Key), Vec<Entity>> = HashMap::new();
    for (e, part, parent) in &parts {
        let root = parent.parent();
        let Some(rig) = rigs.0.entry(part.0).or_insert_with(|| load_rig(&data, part.0, &mut bindposes)).clone() else {
            commands.entity(e).remove::<SkinPart>();
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

/// While a Sim plays a clip at an object with moving parts, the object plays its half of it
/// (in step); once nobody's using it, it's back at rest.
fn animate_objects(
    data: Option<Res<Baked>>,
    mut lib: ResMut<ClipLibrary>,
    sims: Query<(&ClipPlayer, &ActionQueue)>,
    mut objects: Query<(Entity, &mut ObjectRig, Option<&UsedBy>)>,
    mut bones: Query<&mut Transform, Without<ObjectRig>>,
    mut companions: Local<HashMap<String, Option<String>>>,
    mut animated: Local<Vec<Entity>>,
) {
    let Some(data) = data else { return };
    animated.clear();
    for (player, queue) in &sims {
        let Some(target) = queue.0.front().and_then(|a| match a.kind {
            ActionKind::Object { target, .. } | ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } => Some(target),
            _ => None,
        }) else {
            continue;
        };
        let Ok((_, mut rig, _)) = objects.get_mut(target) else { continue };
        let Some(name) = companions.entry(player.name.clone()).or_insert_with(|| companion(&data, &player.name)).clone() else { continue };
        let Some(clip) = lib.get(&data, &name) else { continue };
        let t = player.time.clamp(0.0, clip.duration.max(0.0));
        for inst in &rig.rigs {
            for (i, h) in inst.rig.hashes.iter().enumerate() {
                let (Some(track), Some(&bone)) = (clip.tracks.get(h), inst.bones.get(i)) else { continue };
                let Ok(mut tf) = bones.get_mut(bone) else { continue };
                let mut pose = inst.rig.bind[i];
                if let Some(p) = sample_track_vec(&track.translation, t) {
                    pose.translation = p;
                }
                if let Some(r) = sample_track_quat(&track.rotation, t) {
                    pose.rotation = r;
                }
                *tf = pose;
            }
        }
        rig.posed = true;
        animated.push(target);
    }
    // At rest again once nobody's using it.
    for (e, mut rig, used) in &mut objects {
        if !rig.posed || animated.contains(&e) || used.is_some_and(|u| u.0.is_some()) {
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
