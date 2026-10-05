//! The morning paper: delivered by the mailbox at 7 AM each day (the game's rolled newspaper),
//! to read or to look for a job in, then recycle. Unread papers pile up, three at most.

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
        app.add_systems(Update, deliver_paper.run_if(in_state(PlayMode::Live)));
    }
}

#[allow(clippy::too_many_arguments)]
fn deliver_paper(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut delivered: Local<Option<u32>>,
    objects: Query<(&GameObject, &Transform)>,
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let day = clock.day();
    if clock.hour_f() < 7.0 || *delivered == Some(day) {
        return;
    }
    *delivered = Some(day);
    let Some((mailbox, mtf)) = objects.iter().find(|(o, _)| o.kind == ObjectKind::Mailbox) else {
        info!("no mailbox on the lot: no paper");
        return;
    };
    if objects.iter().filter(|(o, _)| o.kind == ObjectKind::Newspaper).count() >= 3 {
        return;
    }
    let Some(entry) = data.0.catalog.iter().find(|c| c.instance_name == "Newspaper") else { return };
    let mut rng = rand::rng();
    let spot = mailbox.use_point(mtf) + Vec2::new(rng.random_range(-0.6..0.6), rng.random_range(-0.3..0.3));
    let at = Vec3::new(spot.x, mtf.translation.y, spot.y);
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, entry.objd, at, Quat::from_rotation_y(rng.random_range(0.0..6.28)));
    info!("the paper was delivered at {at:.1?}");
}
