//! Fish bowls, as the game's: a fish from a Sim's inventory put in a bowl swims there (the
//! catch's own model, small), and can be taken out again. Bowls of perfect fish of different
//! kinds count towards The Perfect Aquarium lifetime wish. What swims where is kept in saves.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{GameObject, Notifications, ObjectKind};
use crate::inventory::{Inventory, ItemKind, Stack};
use crate::sim::Sim;

pub struct FishBowlPlugin;

impl Plugin for FishBowlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerfectFish>()
            .init_resource::<PendingBowls>()
            .add_systems(Update, (restore_bowls, bowl_requests, swim, census).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The fish swimming in a bowl, and its model.
#[derive(Component, Clone)]
pub struct BowlFish {
    pub fish: Stack,
    model: Option<Entity>,
}

/// A Sim at a bowl: put a fish in, or take it out.
#[derive(Component, Clone)]
pub enum BowlRequest {
    Place { bowl: Entity, key: String, quality: u8 },
    Take(Entity),
}

/// Kinds of perfect fish in the household's bowls.
#[derive(Resource, Default)]
pub struct PerfectFish(pub usize);

/// A bowl's fish as saved: where the bowl stands and what's in it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedBowl {
    pub at: [f32; 3],
    pub fish: Stack,
}

/// Bowls' fish from a save, waiting for the bowls to be set out.
#[derive(Resource, Default)]
pub struct PendingBowls(pub Vec<SavedBowl>);

/// The best fish quality (the gardening qualities' Perfect).
pub const PERFECT: u8 = 9;

/// The fish's model, small, in the bowl.
fn spawn_fish(commands: &mut Commands, bowl: Entity, obj: &GameObject, fish: &Stack, data: &crate::baked::Baked, ui: &crate::icons::GameUi, assets: &mut crate::objects::ObjectAssets, ctx: &mut crate::objects::AssetCtx) -> Option<Entity> {
    let model = ui.data.collectibles.iter().find(|c| c.key == fish.key)?.model.clone();
    let objd = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&model))?.objd;
    let parts = assets.object(ctx, objd);
    let tf = Transform::from_translation(Vec3::new(obj.center.x, obj.height * 0.45, obj.center.y)).with_scale(Vec3::splat(0.45));
    let e = crate::objects::spawn_parts(commands, &parts, tf);
    commands.entity(e).insert((ChildOf(bowl), BowlSwimmer));
    Some(e)
}

/// The fish model swimming round its bowl.
#[derive(Component)]
struct BowlSwimmer;

fn swim(time: Res<Time>, mut q: Query<&mut Transform, With<BowlSwimmer>>) {
    for mut tf in &mut q {
        tf.rotation = Quat::from_rotation_y(time.elapsed_secs() * 0.8);
    }
}

/// Reserve the exact fish before any model or bowl state is created.
fn take_fish(inv: &mut Inventory, key: &str, quality: u8) -> Option<Stack> {
    let i = inv.0.iter().position(|s| s.kind == ItemKind::Fish && s.key == key && s.quality == quality && s.count > 0)?;
    let mut fish = inv.0[i].clone();
    fish.worth = inv.take_one(i)?;
    fish.count = 1;
    Some(fish)
}

#[cfg(test)]
mod transfer_tests {
    use super::*;

    #[test]
    fn fish_reservation_is_immediate_exact_and_preserves_total_value() {
        let mut inv = Inventory(vec![Stack { kind: ItemKind::Fish, key: "Goldfish".into(), name: "Perfect Goldfish".into(), quality: 9, count: 2, worth: 101 }]);
        assert!(take_fish(&mut inv, "Goldfish", 8).is_none());
        assert!(take_fish(&mut inv, "Minnow", 9).is_none());
        assert_eq!(inv.0[0].count, 2);
        let first = take_fish(&mut inv, "Goldfish", 9).unwrap();
        assert_eq!(first.count, 1);
        assert_eq!(inv.0[0].count, 1);
        assert_eq!(first.worth + inv.0[0].worth, 101);
        let second = take_fish(&mut inv, "Goldfish", 9).unwrap();
        assert!(inv.0.is_empty());
        assert_eq!(first.worth + second.worth, 101);
        assert!(take_fish(&mut inv, "Goldfish", 9).is_none(), "another request cannot reuse a reserved fish");
    }

    #[test]
    fn empty_or_nonfish_stacks_cannot_create_a_bowl_fish() {
        for (kind, count) in [(ItemKind::Fish, 0), (ItemKind::Produce, 1)] {
            let mut inv = Inventory(vec![Stack { kind, key: "Goldfish".into(), name: "Invalid".into(), quality: 9, count, worth: 20 }]);
            assert!(take_fish(&mut inv, "Goldfish", 9).is_none());
            assert_eq!(inv.0[0].count, count);
            assert_eq!(inv.0[0].worth, 20);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn bowl_requests(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &BowlRequest, Option<&mut Inventory>)>,
    mut bowls: Query<(&GameObject, Option<&mut BowlFish>)>,
    (data, ui, mut assets): (Option<Res<crate::baked::Baked>>, Option<Res<crate::icons::GameUi>>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut notes: ResMut<Notifications>,
) {
    let (Some(data), Some(ui)) = (data, ui) else { return };
    let mut transferred = std::collections::HashSet::new();
    for (e, sim, req, inv) in &mut sims {
        commands.entity(e).remove::<BowlRequest>();
        let bowl = match req { BowlRequest::Place { bowl, .. } | BowlRequest::Take(bowl) => *bowl };
        if transferred.contains(&bowl) { continue; }
        match req {
            BowlRequest::Place { key, quality, .. } => {
                let (Ok((obj, held)), Some(mut inv)) = (bowls.get_mut(bowl), inv) else { continue };
                if obj.kind != ObjectKind::FishBowl || held.is_some() {
                    continue;
                }
                let Some(fish) = take_fish(&mut inv, key, *quality) else { continue };
                transferred.insert(bowl);
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let model = spawn_fish(&mut commands, bowl, obj, &fish, &data, &ui, &mut assets, &mut ctx);
                notes.push(format!("{} put {} {} in the fish bowl.", sim.first, if fish.name.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" }, fish.name));
                commands.entity(bowl).insert(BowlFish { fish, model });
            }
            BowlRequest::Take(_) => {
                let Ok((obj, Some(held))) = bowls.get(bowl) else { continue };
                if obj.kind != ObjectKind::FishBowl { continue; }
                transferred.insert(bowl);
                let fish = held.fish.clone();
                if let Some(m) = held.model {
                    commands.entity(m).try_despawn();
                }
                commands.entity(bowl).remove::<BowlFish>();
                notes.push(format!("{} took the {} out of the fish bowl.", sim.first, fish.name));
                crate::inventory::give(&mut commands, e, ItemKind::Fish, fish.key.clone(), fish.name.clone(), fish.quality, fish.worth, 1);
            }
        }
    }
}

/// Fish from a save, back in their bowls once the bowls are out.
#[allow(clippy::too_many_arguments)]
fn restore_bowls(
    mut commands: Commands,
    mut pending: ResMut<PendingBowls>,
    bowls: Query<(Entity, &GameObject, &Transform), Without<BowlFish>>,
    (data, ui, mut assets): (Option<Res<crate::baked::Baked>>, Option<Res<crate::icons::GameUi>>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    if pending.0.is_empty() {
        return;
    }
    let (Some(data), Some(ui)) = (data, ui) else { return };
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    pending.0.retain(|b| {
        let at = Vec3::from(b.at);
        let Some((e, obj, _)) = bowls.iter().filter(|(_, o, _)| o.kind == ObjectKind::FishBowl).find(|(_, _, tf)| tf.translation.distance(at) < 0.3) else {
            return true;
        };
        let model = spawn_fish(&mut commands, e, obj, &b.fish, &data, &ui, &mut assets, &mut ctx);
        commands.entity(e).insert(BowlFish { fish: b.fish.clone(), model });
        false
    });
}

/// The kinds of perfect fish in bowls on the home lot.
fn census(bowls: Query<&BowlFish, (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>)>, mut perfect: ResMut<PerfectFish>) {
    let mut kinds: Vec<&str> = bowls.iter().filter(|b| b.fish.quality >= PERFECT).map(|b| b.fish.key.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    if perfect.0 != kinds.len() {
        perfect.0 = kinds.len();
    }
}

/// The bowls' fish, for saving.
pub fn saved(bowls: &Query<(&BowlFish, &Transform), (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>)>) -> Vec<SavedBowl> {
    bowls.iter().map(|(b, tf)| SavedBowl { at: tf.translation.to_array(), fish: b.fish.clone() }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undoable_sales_do_not_count_or_save_the_sold_fish() {
        let mut app = App::new();
        app.init_resource::<PerfectFish>().add_systems(Update, census);
        let bowl = |key: &str| BowlFish {
            fish: Stack { kind: ItemKind::Fish, key: key.into(), name: key.into(), quality: PERFECT, count: 1, worth: 100 },
            model: None,
        };
        app.world_mut().spawn((bowl("salmon"), Transform::default()));
        let sold = app.world_mut().spawn((bowl("trout"), Transform::default(), crate::buyhistory::HistoryHidden)).id();
        app.update();
        assert_eq!(app.world().resource::<PerfectFish>().0, 1);
        let mut query = bevy::ecs::system::SystemState::<Query<(&BowlFish, &Transform), (Without<crate::visit::LotObject>, Without<crate::buyhistory::HistoryHidden>)>>::new(app.world_mut());
        assert_eq!(saved(&query.get(app.world()).unwrap()).len(), 1);
        app.world_mut().entity_mut(sold).remove::<crate::buyhistory::HistoryHidden>();
        app.update();
        assert_eq!(app.world().resource::<PerfectFish>().0, 2);
        assert_eq!(saved(&query.get(app.world()).unwrap()).len(), 2);
    }
}
