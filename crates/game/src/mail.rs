//! The morning paper: brought by the paperboy each morning (the game's `NewspaperDelivery`,
//! from 7 AM), who walks up to a metre or two from the front door (or the mailbox) and sets it
//! down on the ground there (`DeliverNewspaper`, its put-down on the floor), to read or to look
//! for a job in, then recycle. Unread papers pile up, three at most.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::baked::Baked;
use crate::interact::{GameObject, ObjectKind};
use crate::loading::Catalog;
use crate::objects::{AssetCtx, ObjectAssets};

pub struct MailPlugin;

impl Plugin for MailPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaperRound>().add_systems(Update, deliver_paper.run_if(in_state(PlayMode::Live)));
    }
}

/// The paper's delivery: stopped, or not (the game's recurring `NewspaperDelivery`, cancelled
/// and resumed by phone: `servicesdialog`). Kept in saves.
#[derive(Resource, Default)]
pub struct PaperRound {
    pub cancelled: bool,
}

/// The paperboy on the round: where the paper's to go, since when, and the put-down begun.
#[derive(Component)]
pub struct PaperBoy {
    to: Vec2,
    since: f64,
    putting: Option<f64>,
    done: bool,
}

/// The paperboy's put-down (the game's carried newspaper set on the floor), and how long it
/// takes (game minutes).
const PUT_DOWN: &str = "a2o_newspaper_putDown_floor_x";
const PUT_DOWN_MINUTES: f64 = 2.5;
/// How far from the front door the paper lands (metres: the game's 1 to 2).
const FROM_DOOR: f32 = 1.5;
/// The hour the round's over by.
const LAST_ROUND: f32 = 12.0;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn deliver_paper(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut delivered: Local<Option<u32>>,
    objects: Query<(&GameObject, &Transform), Without<crate::sim::Sim>>,
    mut boys: Query<(Entity, &mut PaperBoy, &mut crate::interact::ActionQueue, &Transform, Option<&crate::nav::PathFollow>), With<crate::sim::Sim>>,
    (exit, world, building, loading, round): (Option<Res<crate::interact::LotExit>>, Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>, Option<Res<crate::save::PendingLoad>>, Res<PaperRound>),
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(exit) = exit else { return };
    // On the round: up to the door, the paper set down, and off.
    for (me, mut boy, mut queue, tf, path) in &mut boys {
        if boy.done {
            crate::services::leave(&mut commands, me, &mut queue, tf, path, exit.0);
            continue;
        }
        let idle = queue.0.is_empty() && path.is_none_or(|p| p.done);
        match boy.putting {
            None => {
                let there = idle && tf.translation.xz().distance(boy.to) < 1.2;
                let stuck = clock.minutes - boy.since > 60.0;
                if !there && !stuck {
                    if idle {
                        queue.0.push_back(crate::interact::Action::new("Deliver the Paper", crate::interact::ActionKind::GoHere(boy.to, 1), false));
                    }
                    continue;
                }
                boy.putting = Some(clock.minutes);
                commands.entity(me).insert(crate::anim::ActionClip::new(Some(PUT_DOWN), &[]));
            }
            Some(t) if clock.minutes - t >= PUT_DOWN_MINUTES => {
                commands.entity(me).remove::<crate::anim::ActionClip>();
                boy.done = true;
                // The paper where they set it down, just before them.
                let Some(entry) = data.0.catalog.iter().find(|c| c.instance_name == "Newspaper") else { continue };
                let mut rng = rand::rng();
                let fwd = (tf.rotation * Vec3::Z).xz().normalize_or(Vec2::Y);
                let spot = tf.translation.xz() + fwd * 0.5;
                let at = Vec3::new(spot.x, tf.translation.y, spot.y);
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, entry.objd, at, Quat::from_rotation_y(rng.random_range(0.0..6.28)));
                info!("the paper was delivered at {at:.1?}");
            }
            Some(_) => {}
        }
    }
    // (Not while a save's coming in: its clock isn't the day's yet.)
    let day = clock.day();
    if loading.is_some() || round.cancelled || clock.hour_f() < 7.0 || *delivered == Some(day) {
        return;
    }
    *delivered = Some(day);
    // (The morning round: come in later in the day, the paper's tomorrow's.)
    if clock.hour_f() >= LAST_ROUND {
        return;
    }
    let Some((mailbox, mtf)) = objects.iter().find(|(o, _)| o.kind == ObjectKind::Mailbox) else {
        info!("no mailbox on the lot: no paper");
        return;
    };
    if objects.iter().filter(|(o, _)| o.kind == ObjectKind::Newspaper).count() >= 3 {
        return;
    }
    // A metre or two before the front door, or the mailbox.
    let to = building.as_deref().and_then(|b| b.front_door_out(exit.0, FROM_DOOR)).unwrap_or_else(|| mailbox.use_point(mtf));
    let mut rng = rand::rng();
    let female = rng.random_bool(0.5);
    let sim = crate::sim::random_sim(&mut rng, "Paperboy", Some(female), crate::sim::Age::Teen);
    let p = crate::interact::arrival_spot(exit.0);
    let at = Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y);
    let e = crate::services::arrive(&mut commands, sim, at, None, crate::interact::Skills::default());
    commands.entity(e).insert(PaperBoy { to, since: clock.minutes, putting: None, done: false });
}
