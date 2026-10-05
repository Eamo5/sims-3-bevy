//! Props in Sims' hands: the plate and fork while eating, the book being read, the toothbrush,
//! the guitar. A Sim clip like `a2o_eat_stand_fork_neat_x` comes with clips for its props
//! (`…_fork`), whose `transformBone` track places the prop in the slot it is parented to (the
//! clips' parent events name the hand slots). Each prop is the game's own object, attached to
//! that slot joint for as long as the Sim's clip uses it.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::PlayMode;
use crate::anim::{ClipLibrary, ClipPlayer};
use crate::baked::Baked;
use crate::objects::{AssetCtx, ObjectAssets};
use crate::simbody::Skeleton;
use s3bake::types::Key;

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, hold_props.after(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)));
    }
}

/// The props the game's clips use: the clip actor, the object it is, the slot it is held in
/// (from the clips' parent events: Sims eat standing with the plate in the right hand and the
/// fork in the left), and where the object's `transformBone` sits in its model (its rig's bind
/// pose: the guitar's model stands on the floor, its middle 0.44 m up), and a nudge in the
/// slot: the game rests the guitar against the body with IK the clips leave to the engine.
const PROPS: &[(&str, &str, &str, [f32; 3], [f32; 3])] = &[
    ("fork", "UtensilFork", "b__L_Hand_slot", [0.0; 3], [0.0; 3]),
    ("plateDinner", "Plate", "b__R_Hand_slot", [0.0; 3], [0.0; 3]),
    ("book", "BookGeneral", "b__R_Hand_slot", [0.0; 3], [0.0; 3]),
    ("toothbrush", "Toothbrush", "b__R_Hand_slot", [0.0; 3], [0.0; 3]),
    ("guitar", "musicalInstrumentGuitar", "b__R_carry_slot", [0.0, 0.43762, -0.020687], [0.0, -0.08, 0.17]),
    ("spatula", "Spatula", "b__R_Hand_slot", [0.0; 3], [0.0; 3]),
    ("scythe", "DeathScythe", "b__R_Hand_slot", [0.0; 3], [0.0; 3]),
];

/// The clip-actor suffixes that are props (for the clip bake).
pub fn prop_actor(suffix: &str) -> bool {
    PROPS.iter().any(|(a, ..)| a.eq_ignore_ascii_case(suffix))
}

/// The props a Sim is holding: which one and its entity, and the object put away while its
/// copy is in hand (the guitar played from where it stood).
#[derive(Component, Default)]
struct HeldProps(Vec<(usize, Entity)>, Option<Entity>);

/// A prop in a Sim's hand.
#[derive(Component)]
struct Prop;

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn hold_props(
    mut commands: Commands,
    data: Res<Baked>,
    mut lib: ResMut<ClipLibrary>,
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut sims: Query<(Entity, &ClipPlayer, &Skeleton, Option<&mut HeldProps>, &crate::interact::ActionQueue, Has<crate::anim::ActionClip>)>,
    mut props: Query<&mut Transform, With<Prop>>,
    mut placed: Query<(&crate::interact::GameObject, &mut Visibility), Without<Prop>>,
    mut objects: Local<Option<HashMap<String, Key>>>,
    mut companions: Local<HashMap<String, Vec<(usize, String)>>>,
) {
    // The props' objects, by the game's internal names.
    let objects = objects.get_or_insert_with(|| {
        data.0.catalog.iter().filter(|c| PROPS.iter().any(|(_, o, ..)| *o == c.instance_name)).map(|c| (c.instance_name.clone(), c.objd)).collect()
    });
    for (sim, player, skel, held, queue, acting) in &mut sims {
        // The props this clip has clips for.
        let want = companions
            .entry(player.name.clone())
            .or_insert_with(|| {
                let Some(stem) = player.name.strip_suffix("_x") else { return Vec::new() };
                PROPS
                    .iter()
                    .enumerate()
                    .filter_map(|(i, (actor, ..))| {
                        let name = format!("{stem}_{actor}");
                        data.0.clip_names.iter().find(|n| n.eq_ignore_ascii_case(&name)).map(|n| (i, n.clone()))
                    })
                    .collect()
            })
            .clone();
        let Some(mut held) = held else {
            if !want.is_empty() {
                commands.entity(sim).insert(HeldProps::default());
            }
            continue;
        };
        // The object being used, when the Sim holds its copy (shown again afterwards).
        let target = queue.0.front().and_then(|a| match a.kind {
            crate::interact::ActionKind::Object { target, .. } => Some(target),
            _ => None,
        });
        let in_hand = target.filter(|t| {
            placed.get(*t).is_ok_and(|(o, _)| want.iter().any(|(i, _)| objects.get(PROPS[*i].1) == Some(&o.objd)))
        });
        if held.1 != in_hand {
            if let Some(old) = held.1
                && let Ok((_, mut v)) = placed.get_mut(old)
            {
                *v = Visibility::Inherited;
            }
            if let Some(new) = in_hand
                && let Ok((_, mut v)) = placed.get_mut(new)
            {
                *v = Visibility::Hidden;
            }
            held.1 = in_hand;
        }
        // Put away what the action no longer uses (props stay in hand through the action's
        // clips that don't move them, like reading between page turns), or that went with a
        // rebuilt body.
        held.0.retain(|(i, e)| {
            let keep = (acting || want.iter().any(|(w, _)| w == i)) && props.contains(*e);
            if !keep {
                commands.entity(*e).try_despawn();
            }
            keep
        });
        for (i, clip_name) in &want {
            let (_, object, slot, bind, nudge) = PROPS[*i];
            let entity = match held.0.iter().find(|(h, _)| h == i) {
                Some((_, e)) => *e,
                None => {
                    let Some(bone) = skel.rig.bones.iter().position(|b| b.name.eq_ignore_ascii_case(slot)) else { continue };
                    let Some(objd) = objects.get(object) else { continue };
                    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                    let parts = assets.object(&mut ctx, *objd);
                    if parts.is_empty() {
                        continue;
                    }
                    // (The model hangs from its transformBone.)
                    let model = crate::objects::spawn_parts(&mut commands, &parts, Transform::from_translation(-Vec3::from(bind)));
                    let e = commands.spawn((Transform::default(), Visibility::default(), Prop)).add_child(model).id();
                    commands.entity(skel.joints[bone]).add_child(e);
                    held.0.push((*i, e));
                    continue;
                }
            };
            // Placed in the slot as the prop's clip has it.
            let Some(clip) = lib.get(&data, clip_name) else { continue };
            let t = if clip.duration > 0.0 { player.time % clip.duration } else { 0.0 };
            let Some(track) = clip.tracks.get(&TRANSFORM_BONE) else { continue };
            if let Ok(mut tf) = props.get_mut(entity) {
                if let Some(p) = crate::anim::sample_track_vec(&track.translation, t) {
                    tf.translation = p + Vec3::from(nudge);
                }
                if let Some(r) = crate::anim::sample_track_quat(&track.rotation, t) {
                    tf.rotation = r;
                }
            }
        }
    }
}

/// fnv32("transformBone"): a prop's root in its clips.
const TRANSFORM_BONE: u32 = 0xCD68F001;
