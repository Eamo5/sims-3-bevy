//! Fish bowls, as the game's: a fish from a Sim's inventory put in a bowl swims there (the
//! catch's own model, small), and can be taken out again. It wants feeding (the game's `Feed`):
//! hungry a day and a half after the last time (its tuning's `kHoursUntilFishWillBeFedByAutonomy`
//! 36, when Sims feed it of their own accord), dead after three days unfed
//! (`kHoursUntilFishDiesIfNotFed` 72: belly up, till someone cleans the bowl out, `CleanOut`).
//! Bowls of perfect fish of different kinds count towards The Perfect Aquarium lifetime wish.
//! What swims where, and when it was fed, is kept in saves.

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
            .add_systems(Update, (restore_bowls, bowl_requests, hunger, swim, census).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The fish swimming in a bowl, and its model; when it was last fed (game minutes), and whether
/// it's died for want of it.
#[derive(Component, Clone)]
pub struct BowlFish {
    pub fish: Stack,
    model: Option<Entity>,
    pub fed_at: f64,
    pub dead: bool,
}

impl BowlFish {
    /// Hungry: due feeding (and Sims see to it of their own accord).
    pub fn hungry(&self, now: f64) -> bool {
        !self.dead && now - self.fed_at >= HUNGRY_HOURS * 60.0
    }
}

/// The game's tuned hours: hungry, and dead, after the last feeding.
const HUNGRY_HOURS: f64 = 36.0;
const DIES_HOURS: f64 = 72.0;

/// A Sim at a bowl: put a fish in, take it out, feed it, or clean the dead one out.
#[derive(Component, Clone)]
pub enum BowlRequest {
    Place { bowl: Entity, key: String, quality: u8 },
    Take(Entity),
    Feed(Entity),
    Clean(Entity),
}

/// Kinds of perfect fish in the household's bowls.
#[derive(Resource, Default)]
pub struct PerfectFish(pub usize);

/// A bowl's fish as saved: where the bowl stands and what's in it, when it was fed and whether
/// it's died.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedBowl {
    pub at: [f32; 3],
    pub fish: Stack,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fed_at: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dead: bool,
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

/// Unfed three days: dead, belly up in the bowl.
fn hunger(mut commands: Commands, clock: Res<crate::clock::GameClock>, mut bowls: Query<&mut BowlFish>, mut notes: ResMut<Notifications>) {
    for mut b in &mut bowls {
        if b.dead || clock.minutes - b.fed_at < DIES_HOURS * 60.0 {
            continue;
        }
        b.dead = true;
        notes.push(format!("The {} in the fish bowl has died: it wasn't fed. The bowl needs cleaning out.", b.fish.name));
        if let Some(m) = b.model {
            belly_up(&mut commands, m);
        }
    }
}

/// A dead fish: still, and upside down.
fn belly_up(commands: &mut Commands, model: Entity) {
    commands.entity(model).remove::<BowlSwimmer>().queue_silenced(|mut e: EntityWorldMut| {
        if let Some(mut tf) = e.get_mut::<Transform>() {
            tf.rotation = Quat::from_rotation_z(std::f32::consts::PI);
        }
    });
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
    clock: Res<crate::clock::GameClock>,
) {
    let (Some(data), Some(ui)) = (data, ui) else { return };
    let mut transferred = std::collections::HashSet::new();
    for (e, sim, req, inv) in &mut sims {
        commands.entity(e).remove::<BowlRequest>();
        let bowl = match req { BowlRequest::Place { bowl, .. } | BowlRequest::Take(bowl) | BowlRequest::Feed(bowl) | BowlRequest::Clean(bowl) => *bowl };
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
                // (FISH_FED_AGO=<hours>, tests: fed that long ago.)
                let ago = std::env::var("FISH_FED_AGO").ok().and_then(|h| h.parse::<f64>().ok()).unwrap_or(0.0) * 60.0;
                commands.entity(bowl).insert(BowlFish { fish, model, fed_at: clock.minutes - ago, dead: false });
            }
            BowlRequest::Feed(_) => {
                if let Ok((_, Some(mut held))) = bowls.get_mut(bowl)
                    && !held.dead
                {
                    held.fed_at = clock.minutes;
                    notes.push(format!("{} fed the {}.", sim.first, held.fish.name));
                }
            }
            BowlRequest::Clean(_) => {
                let Ok((_, Some(held))) = bowls.get(bowl) else { continue };
                if !held.dead {
                    continue;
                }
                if let Some(m) = held.model {
                    commands.entity(m).try_despawn();
                }
                commands.entity(bowl).remove::<BowlFish>();
                notes.push(format!("{} cleaned out the fish bowl.", sim.first));
            }
            BowlRequest::Take(_) => {
                let Ok((obj, Some(held))) = bowls.get(bowl) else { continue };
                if obj.kind != ObjectKind::FishBowl || held.dead { continue; }
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
    clock: Res<crate::clock::GameClock>,
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
        // (Saved before feeding was kept: fed just now.)
        commands.entity(e).insert(BowlFish { fish: b.fish.clone(), model, fed_at: b.fed_at.unwrap_or(clock.minutes), dead: b.dead });
        if b.dead
            && let Some(m) = model
        {
            belly_up(&mut commands, m);
        }
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
    bowls.iter().map(|(b, tf)| SavedBowl { at: tf.translation.to_array(), fish: b.fish.clone(), fed_at: Some(b.fed_at), dead: b.dead }).collect()
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
            fed_at: 0.0,
            dead: false,
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
