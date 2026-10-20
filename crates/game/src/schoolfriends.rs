//! A friend home from school, as the game's `School.FriendsAfterSchool`: once a school day, a
//! child or teen of the household coming home brings a classmate along one time in seven or so
//! (`BringSomeoneHomeChance`, tuned to 15%): a friend from among them three times in four
//! (`BringSomeoneHomeFriendChance`), else any of their class. They come home together, in
//! already (no ringing at the door: the game's `VisitSituation` greets a school friend), and
//! stay a while as visitors do. Strangers till then have met and like each other a little
//! (`kInitialLTRForSimAfterSchool`).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::interact::{AutonomyTimer, Notifications, OffLot, Skills, Visitor};
use crate::sim::{Age, HouseholdMember, Sim};
use crate::social::Relationships;

pub struct SchoolFriendsPlugin;

impl Plugin for SchoolFriendsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, school_friends.run_if(in_state(PlayMode::Live)));
    }
}

/// The game's tunables (as tuned): the chance of bringing someone (percent), of it being a
/// friend, the liking two strangers start at, and what counts as friends.
const BRING_CHANCE: f64 = 15.0;
const FRIEND_CHANCE: f64 = 0.75;
const STRANGER_LIKING: f32 = 10.0;
const FRIENDS: f32 = 40.0;
/// How long a classmate stays (game minutes), and how late they can be out (hour).
const STAY: f64 = 180.0;
const HOME_BY: f64 = 20.0;

/// Whether two Sims are in the same class: children together, teens together.
fn classmates(a: Age, b: Age) -> bool {
    matches!((a, b), (Age::Child, Age::Child) | (Age::Teen, Age::Teen))
}

/// The premade's age, as the household's Sims are aged.
fn age_of(bits: u32) -> Option<Age> {
    use s3formats::premade::{AGE_CHILD, AGE_TEEN};
    if bits & AGE_CHILD != 0 {
        Some(Age::Child)
    } else if bits & AGE_TEEN != 0 {
        Some(Age::Teen)
    } else {
        None
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn school_friends(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut checked: Local<HashMap<Entity, i64>>,
    home_from_school: Query<(Entity, &Sim, &crate::klepto::DayDone, &Transform), (Added<crate::klepto::DayDone>, With<HouseholdMember>)>,
    mut sims: Query<(Entity, &Sim, &mut Relationships, Has<HouseholdMember>, Has<OffLot>, Has<Visitor>)>,
    town: Option<Res<crate::premade::TownPremades>>,
    (data, cas, sim_assets): (Res<crate::baked::Baked>, Option<Res<crate::simbody::CasData>>, Res<crate::sim::SimAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut skin_mats, mut bindposes, mut textures): (
        ResMut<Assets<crate::simbody::SimSkinMaterial>>,
        ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
        ResMut<crate::simbody::SimTextures>,
    ),
    mut notes: ResMut<Notifications>,
) {
    let mut rng = rand::rng();
    for (kid, sim, done, tf) in &home_from_school {
        if done.0 != crate::klepto::FROM_SCHOOL || !matches!(sim.age, Age::Child | Age::Teen) {
            continue;
        }
        // (Once a school day each.)
        let day = clock.day() as i64;
        if checked.get(&kid) == Some(&day) {
            continue;
        }
        checked.insert(kid, day);
        // (SCHOOL_FRIEND=1, tests: every time.)
        let test = std::env::var("SCHOOL_FRIEND").is_ok();
        if !test && rng.random_range(0.0..100.0) > BRING_CHANCE {
            continue;
        }
        // Their class: the town's children (or teens) of other households.
        let ours: Vec<u64> = sims.iter().filter(|s| s.3).map(|s| s.1.id).collect();
        let class: Vec<&s3formats::premade::PremadeSim> = town
            .iter()
            .flat_map(|t| t.0.households.iter())
            .flat_map(|h| h.members.iter())
            .filter(|p| !ours.contains(&p.id) && age_of(p.age).is_some_and(|a| classmates(a, sim.age)))
            .collect();
        // (Already over, or about: not again.)
        let busy = |id: u64| sims.iter().any(|s| s.1.id == id && !s.4);
        let class: Vec<&s3formats::premade::PremadeSim> = class.into_iter().filter(|p| !busy(p.id)).collect();
        let friendship = |id: u64| {
            let e = sims.iter().find(|s| s.1.id == id).map(|s| s.0)?;
            sims.get(kid).ok().and_then(|k| k.2.0.get(&e).map(|r| r.friendship))
        };
        let friends: Vec<&s3formats::premade::PremadeSim> = class.iter().copied().filter(|p| friendship(p.id).is_some_and(|f| f >= FRIENDS)).collect();
        let pick = if !friends.is_empty() && rng.random_bool(FRIEND_CHANCE) { friends.choose(&mut rng) } else { class.choose(&mut rng) };
        let Some(p) = pick.copied() else { continue };
        // Home with them, at the lot's edge where the bus left them.
        let at = tf.translation + Vec3::new(0.6, 0.0, 0.0);
        let existing = sims.iter().find(|s| s.1.id == p.id).map(|s| s.0);
        let e = match existing {
            Some(e) => {
                commands.entity(e).insert(Transform::from_translation(at));
                e
            }
            None => {
                let s = crate::premade::to_sim(p);
                let model = cas.as_ref().and_then(|cas| {
                    let outfit = crate::simbody::pick_outfit(cas, &s, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(s.look));
                    crate::simbody::build_sim_model(&data.0, cas, &s, &outfit, crate::simbody::tone_of(&s))
                });
                let mut sctx = crate::sim::SimSpawnCtx {
                    assets: &sim_assets,
                    render: crate::simbody::SimRenderCtx {
                        meshes: &mut meshes,
                        images: &mut images,
                        mats: &mut mats,
                        skin_mats: &mut skin_mats,
                        bindposes: &mut bindposes,
                        textures: &mut textures,
                    },
                };
                crate::sim::spawn_sim_full(&mut commands, &mut sctx, s, at, model)
            }
        };
        let day_start = (clock.minutes / 1440.0).floor() * 1440.0;
        let leave_at = (clock.minutes + STAY).min(day_start + HOME_BY * 60.0).max(clock.minutes + 60.0);
        commands
            .entity(e)
            .remove::<(OffLot, crate::interact::GoingHome, crate::interact::Invited, crate::town::Townie, crate::visit::LotGuest, crate::nav::PathFollow)>()
            .insert((Visibility::Inherited, Visitor { leave_at }, DespawnOnExit(crate::AppState::InGame)))
            .insert_if_new((crate::interact::ActionQueue::default(), AutonomyTimer(2.0), Skills::default()));
        // (Strangers till now: met, and a little liking between them.)
        if existing.is_some() {
            for (a, b) in [(kid, e), (e, kid)] {
                if let Ok((.., mut rels, _, _, _)) = sims.get_mut(a) {
                    let r = rels.entry(b);
                    r.friendship = r.friendship.max(STRANGER_LIKING);
                }
            }
        } else {
            if let Ok((.., mut rels, _, _, _)) = sims.get_mut(kid) {
                let r = rels.entry(e);
                r.friendship = r.friendship.max(STRANGER_LIKING);
            }
            let me = kid;
            commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| {
                if let Some(mut rels) = w.get_mut::<Relationships>() {
                    let r = rels.entry(me);
                    r.friendship = r.friendship.max(STRANGER_LIKING);
                }
            });
        }
        let friend = crate::premade::to_sim(p);
        info!("school friend: {} brings {} home", sim.first, friend.first);
        // (The game's `School:FriendComesHomeWithSim`.)
        notes.push(format!("{} is coming home after school with {}.", friend.full_name(), sim.full_name()));
    }
}
