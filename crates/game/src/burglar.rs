//! Burglars: some nights, in the small hours, a burglar in black slips onto the household's lot,
//! makes for the priciest thing in the house and bags it. A burglar alarm calls the police at
//! once (so does a household Sim who's awake to see it, a little later); if the officer gets
//! there before the burglar's gone, the burglar is cuffed and taken away, and nothing is lost.

use bevy::prelude::*;
use rand::Rng;

use crate::anim::ActionClip;
use crate::clock::GameClock;
use crate::interact::{ActionQueue, GameObject, Notifications};
use crate::nav::{Floor, PathFollow};
use crate::sim::{Age, HouseholdMember, Sim};
use crate::{AppState, PlayMode};

pub struct BurglarPlugin;

impl Plugin for BurglarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Break>()
            .add_systems(Update, (break_in, burglar, police, recover).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnEnter(PlayMode::Live), |mut b: ResMut<Break>| *b = Break::default());
    }
}

/// Tonight's break-in, if any: the burglar, the officer, and when the police were called.
#[derive(Resource, Default)]
pub struct Break {
    /// The day last decided on (one chance a night).
    decided: Option<u32>,
    burglar: Option<Entity>,
    officer: Option<Entity>,
    called: Option<f64>,
    /// When an awake household Sim first saw the burglar.
    seen: Option<f64>,
    /// Force a break-in tonight (tests).
    pub force: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    Sneaking,
    Stealing,
    Leaving,
    Caught,
}

/// A burglar on the lot: making for `target`, since when at this stage, and what they've bagged
/// (its catalogue key, where it stood, whether the household bought it, and its name).
#[derive(Component)]
pub struct Burglar {
    stage: Stage,
    target: Option<Entity>,
    since: f64,
    loot: Option<(s3bake::Key, Transform, bool, String)>,
}

/// Loot the police got back, to put back where it was.
#[derive(Resource)]
struct Recovered(s3bake::Key, Transform, bool);

/// A police officer come for the burglar.
#[derive(Component)]
pub struct Officer {
    cuffing: Option<f64>,
    leaving: bool,
}

/// The chance of a break-in each night, when it can happen, how long a theft takes and how long
/// the police take to come.
const NIGHTLY_CHANCE: f64 = 0.12;
const HOURS: std::ops::Range<f32> = 1.0..4.5;
const STEAL_MINUTES: f64 = 20.0;
const RESPONSE_MINUTES: f64 = 15.0;
const CUFF_MINUTES: f64 = 6.0;

/// An NPC body (no needs, no autonomy) in the given outfit, at the lot's way in.
fn npc(commands: &mut Commands, sim: Sim, at: Vec3) -> Entity {
    commands
        .spawn((
            Transform::from_translation(at),
            Visibility::default(),
            sim,
            crate::sim::SimAnim::default(),
            crate::anim::ClipPlayer::default(),
            crate::aging::NeedsNewBody,
            Floor(1),
            ActionQueue::default(),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|c| {
            c.spawn((Transform::default(), Visibility::default()));
        })
        .id()
}

/// Some nights a burglar comes: for the most valuable thing on the ground floor.
#[allow(clippy::too_many_arguments)]
fn break_in(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut b: ResMut<Break>,
    (exit, grid, world): (Option<Res<crate::interact::LotExit>>, Option<Res<crate::nav::NavGrid>>, Res<crate::loading::CurrentWorld>),
    objects: Query<(Entity, &GameObject, &Transform, Option<&Floor>), Without<crate::visit::LotObject>>,
    cas: Option<Res<crate::simbody::CasData>>,
    data: Res<crate::baked::Baked>,
    mut notes: ResMut<Notifications>,
) {
    let (Some(exit), Some(grid)) = (exit, grid) else { return };
    let day = (clock.minutes / 1440.0) as u32;
    if b.burglar.is_some() || !b.force && (!HOURS.contains(&clock.hour_f()) || b.decided == Some(day)) {
        return;
    }
    b.decided = Some(day);
    let mut rng = rand::rng();
    if !b.force && !rng.random_bool(NIGHTLY_CHANCE) {
        return;
    }
    b.force = false;
    // The priciest thing within reach (not a door or window).
    let target = objects
        .iter()
        .filter(|(_, o, _, f)| o.price >= 50 && f.is_none_or(|f| f.0 <= 1))
        .filter(|(_, o, ..)| data.0.catalog.iter().find(|c| c.objd == o.objd).is_none_or(|c| crate::building::is_opening(&c.script).is_none()))
        .max_by_key(|(_, o, ..)| o.price)
        .map(|(e, _, tf, _)| (e, tf.translation));
    let Some((target, at)) = target else { return };
    let part = |name: &str| cas.as_ref().and_then(|c| c.parts.iter().find(|p| p.name == name && p.baked).map(|p| p.key));
    let base = crate::sim::random_sim(&mut rng, "Burglar", Some(false), Age::Adult);
    let burglar = Sim { first: "Burglar".into(), outfit: crate::sim::OutfitChoice { full: part("amBodyNinjaOutfit"), ..base.outfit }, ..base };
    let p = exit.0;
    let e = npc(&mut commands, burglar, Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y));
    commands.entity(e).insert(Burglar { stage: Stage::Sneaking, target: Some(target), since: clock.minutes, loot: None });
    if let Some(wp) = crate::nav::plan_route(&grid, None, p, 1, at.xz(), 1) {
        commands.entity(e).insert(PathFollow::new(wp));
    }
    b.burglar = Some(e);
    b.seen = None;
    // The alarm goes off as they come in.
    let alarm = objects.iter().any(|(_, o, ..)| data.0.catalog.iter().any(|c| c.objd == o.objd && c.script.contains("BurglarAlarm")));
    if alarm {
        b.called = Some(clock.minutes);
        notes.push("The burglar alarm went off! The police are on their way.");
    }
}

/// The burglar makes for the loot, bags it, and slips away (unless caught).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn burglar(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut b: ResMut<Break>,
    mut burglars: Query<(Entity, &mut Burglar, &Transform, Option<&PathFollow>)>,
    loot: Query<(&GameObject, &Transform, Has<crate::save::Bought>), Without<Burglar>>,
    awake: Query<(&Sim, &Transform, &crate::sim::SimAnim), (With<HouseholdMember>, Without<Burglar>, Without<crate::interact::AtWork>, Without<crate::rabbitholes::AtRabbitHole>)>,
    (exit, grid): (Option<Res<crate::interact::LotExit>>, Option<Res<crate::nav::NavGrid>>),
    mut removed: ResMut<crate::save::RemovedLotObjects>,
    mut notes: ResMut<Notifications>,
) {
    let Some(be) = b.burglar else { return };
    let Ok((me, mut bur, tf, path)) = burglars.get_mut(be) else {
        b.burglar = None;
        return;
    };
    let (Some(exit), Some(grid)) = (exit, grid) else { return };
    // An awake household Sim close by sees them, and calls the police.
    if b.called.is_none() && bur.stage != Stage::Leaving {
        let witness = awake.iter().find(|(s, stf, anim)| anim.pose != crate::sim::Pose::Lie && s.age.is_grown() && stf.translation.distance(tf.translation) < 15.0);
        if let Some((s, ..)) = witness {
            let since = *b.seen.get_or_insert(clock.minutes);
            if clock.minutes - since > 5.0 {
                b.called = Some(clock.minutes);
                notes.push(format!("{} spotted a burglar and called the police!", s.first));
            }
        }
    }
    let arrived = path.is_none_or(|p| p.done);
    match bur.stage {
        Stage::Sneaking if arrived => {
            commands.entity(me).remove::<PathFollow>();
            if let Some(t) = bur.target
                && let Ok((_, ttf, _)) = loot.get(t)
            {
                let to = (ttf.translation - tf.translation).with_y(0.0);
                commands.entity(me).insert((Transform { rotation: Quat::from_rotation_y(to.x.atan2(to.z)), ..*tf }, ActionClip::new(None, &["a2o_burglar_steal_x"])));
            }
            bur.stage = Stage::Stealing;
            bur.since = clock.minutes;
        }
        Stage::Stealing if clock.minutes - bur.since > STEAL_MINUTES => {
            if let Some(t) = bur.target.take()
                && let Ok((obj, otf, bought)) = loot.get(t)
            {
                if !bought {
                    crate::save::note_removed(&mut removed, obj, otf);
                }
                notes.push(format!("A burglar stole the {}!", obj.name));
                bur.loot = Some((obj.objd, *otf, bought, obj.name.clone()));
                commands.entity(t).despawn();
            }
            commands.entity(me).remove::<ActionClip>();
            if let Some(wp) = crate::nav::plan_route(&grid, None, tf.translation.xz(), 1, exit.0, 1) {
                commands.entity(me).insert(PathFollow::new(wp));
            }
            bur.stage = Stage::Leaving;
        }
        Stage::Leaving if arrived => {
            commands.entity(me).despawn();
            b.burglar = None;
            b.called = None;
        }
        _ => {}
    }
}

/// Recovered loot goes back where it stood.
#[allow(clippy::too_many_arguments)]
fn recover(
    mut commands: Commands,
    recovered: Option<Res<Recovered>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut removed: ResMut<crate::save::RemovedLotObjects>,
    mut grid: Option<ResMut<crate::nav::NavGrid>>,
) {
    let Some(r) = recovered else { return };
    commands.remove_resource::<Recovered>();
    let Recovered(objd, tf, bought) = *r;
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    if let Some(o) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, objd, tf.translation, tf.rotation) {
        if bought {
            commands.entity(o.entity).insert(crate::save::Bought);
        } else if let Some(i) = removed.0.iter().rposition(|s| s.objd == objd && Vec3::from(s.position).distance(tf.translation) < 0.1) {
            removed.0.remove(i);
        }
    }
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
}

/// The police come when called: the officer walks up to the burglar, cuffs them, and takes them
/// away.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn police(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut b: ResMut<Break>,
    mut officers: Query<(Entity, &mut Officer, &Transform, Option<&PathFollow>), Without<Burglar>>,
    mut burglars: Query<(Entity, &mut Burglar, &Transform), Without<Officer>>,
    (exit, grid, world): (Option<Res<crate::interact::LotExit>>, Option<Res<crate::nav::NavGrid>>, Res<crate::loading::CurrentWorld>),
    cas: Option<Res<crate::simbody::CasData>>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let (Some(exit), Some(grid)) = (exit, grid) else { return };
    let Some(called) = b.called else { return };
    // The cruiser pulls up.
    if b.officer.is_none() {
        if b.burglar.is_none() {
            b.called = None;
            return;
        }
        if clock.minutes < called + RESPONSE_MINUTES {
            return;
        }
        let mut rng = rand::rng();
        let part = |name: &str| cas.as_ref().and_then(|c| c.parts.iter().find(|p| p.name == name && p.baked).map(|p| p.key));
        let base = crate::sim::random_sim(&mut rng, "Officer", Some(false), Age::Adult);
        let officer = Sim { outfit: crate::sim::OutfitChoice { full: part("amBodyPoliceMan"), hair: part("amHairPoliceman"), ..base.outfit }, ..base };
        let p = exit.0;
        let e = npc(&mut commands, officer, Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y));
        commands.entity(e).insert(Officer { cuffing: None, leaving: false });
        rides.0.push((p, "CarPolice"));
        b.officer = Some(e);
        notes.push("The police are here!");
        return;
    }
    let Some(oe) = b.officer else { return };
    let Ok((me, mut off, tf, path)) = officers.get_mut(oe) else {
        b.officer = None;
        return;
    };
    let walking = path.is_some_and(|p| !p.done);
    if off.leaving {
        if !walking {
            commands.entity(me).despawn();
            b.officer = None;
            b.called = None;
        }
        return;
    }
    let Some((be, mut bur, btf)) = b.burglar.and_then(|e| burglars.get_mut(e).ok()) else {
        // Gone before the police came.
        notes.push("The burglar got away before the police arrived.");
        off.leaving = true;
        if let Some(wp) = crate::nav::plan_route(&grid, None, tf.translation.xz(), 1, exit.0, 1) {
            commands.entity(me).insert(PathFollow::new(wp));
        }
        return;
    };
    match off.cuffing {
        None if !walking => {
            if tf.translation.xz().distance(btf.translation.xz()) < 1.6 {
                // Cuffed: face to face.
                let to = (btf.translation - tf.translation).with_y(0.0);
                commands.entity(me).remove::<PathFollow>().insert((Transform { rotation: Quat::from_rotation_y(to.x.atan2(to.z)), ..*tf }, ActionClip::new(None, &["a2a_burglar_cuff_x"])));
                commands
                    .entity(be)
                    .remove::<PathFollow>()
                    .insert((Transform { rotation: Quat::from_rotation_y((-to.x).atan2(-to.z)), ..*btf }, ActionClip::new(None, &["a2a_burglar_cuff_y"])));
                bur.stage = Stage::Caught;
                off.cuffing = Some(clock.minutes);
            } else if let Some(wp) = crate::nav::plan_route(&grid, None, tf.translation.xz(), 1, btf.translation.xz(), 1) {
                commands.entity(me).insert(PathFollow::new(wp));
            }
        }
        // Still chasing: keep after them.
        None if path.is_some_and(|p| p.waypoints.last().is_some_and(|w| w.p.distance(btf.translation.xz()) > 2.0)) => {
            if let Some(wp) = crate::nav::plan_route(&grid, None, tf.translation.xz(), 1, btf.translation.xz(), 1) {
                commands.entity(me).insert(PathFollow::new(wp));
            }
        }
        Some(since) if clock.minutes - since > CUFF_MINUTES => {
            match bur.loot.take() {
                Some((objd, tf, bought, name)) => {
                    notes.push(format!("The police caught the burglar and got the {name} back!"));
                    commands.insert_resource(Recovered(objd, tf, bought));
                }
                None => notes.push("The police caught the burglar! Nothing was taken."),
            }
            commands.entity(be).despawn();
            b.burglar = None;
            commands.entity(me).remove::<ActionClip>();
            off.leaving = true;
            if let Some(wp) = crate::nav::plan_route(&grid, None, tf.translation.xz(), 1, exit.0, 1) {
                commands.entity(me).insert(PathFollow::new(wp));
            }
        }
        _ => {}
    }
}
