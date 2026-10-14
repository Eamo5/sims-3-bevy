//! Laundry, as the game's (Generations): where there's a washing machine, Sims who change their
//! clothes at home now and then leave a pile of dirty clothes on the floor. Piles are picked up
//! (the Neat at once, slobs never) and, once there's a load, someone does the laundry with the
//! game's clips: into the washer, an hour's wash, the wet clothes into the dryer (or pegged on
//! the line), and once they're dry and taken out the household's in Fresh Clothing (dried on
//! the line, Very Fresh). Kept in saves.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Special, interactions_for};
use crate::life::{MoodletKind, Moodlets};
use crate::sim::{HouseholdMember, Sim};

pub struct LaundryPlugin;

impl Plugin for LaundryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Laundry>()
            .init_resource::<PendingLaundry>()
            .add_message::<DropClothes>()
            .add_systems(Update, (drop_piles, gather_piles, requests, run_machines, board, restore).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The household's laundry: clothes picked up and waiting to be washed (in the hamper), the
/// piles about the house, and what each machine (washer, dryer, line) has in it, kept for the
/// menus and autonomy to ask.
#[derive(Resource, Default)]
pub struct Laundry {
    pub waiting: u32,
    pub piles: usize,
    pub loads: HashMap<Entity, Load>,
    /// Dryers and lines, and how many are free.
    pub driers: usize,
    pub free_driers: usize,
}

/// A machine's load: running (until when, game minutes) or done.
#[derive(Component, Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Load {
    Running(f64),
    Done,
}

/// A pile of dirty clothes on the floor.
#[derive(Component)]
pub struct ClothingPile;

/// A Sim has changed out of their clothes (see `dressing`): perhaps they leave them lying.
#[derive(Message, Clone, Copy)]
pub struct DropClothes(pub Entity);

/// What a Sim has just done with the laundry (set by the interactions, handled here).
#[derive(Component, Clone, Copy, Debug)]
pub enum LaundryRequest {
    PickedUp(Entity),
    Wash(Entity),
    /// Took the wet clothes out of the washer, for the dryer or the line.
    Unload(Entity),
    Dry(Entity),
    Hang(Entity),
    /// Took the dry clothes out: line-dried (very fresh) or not.
    Fresh(Entity, bool),
}

/// The chance a change of clothes leaves a pile, and the most piles about at once.
const PILE_CHANCE: f64 = 0.4;
const MAX_PILES: usize = 8;
/// How many clothes make a load worth washing, without being asked.
pub const LOAD: u32 = 4;
/// How long the washer, the dryer and the line take (game minutes).
const WASH_MINUTES: f64 = 60.0;
const DRY_MINUTES: f64 = 60.0;
const LINE_MINUTES: f64 = 180.0;
/// The most piles gathered up on the way to the washer.
const GATHER: usize = 4;

impl Laundry {
    /// Whether an object's laundry step is on offer now (for the menus; autonomy asks
    /// `worth_doing` besides).
    pub fn offers(&self, special: Special, kind: ObjectKind, target: Entity) -> bool {
        let load = self.loads.get(&target).copied();
        match special {
            Special::DoLaundry => load.is_none() && self.waiting as usize + self.piles > 0,
            Special::MoveLaundry => load == Some(Load::Done) && (self.driers == 0 || self.free_driers > 0),
            Special::EmptyDryer => load == Some(Load::Done),
            Special::LoadDryer => kind != ObjectKind::Washer && load.is_none(),
            _ => true,
        }
    }

    /// Whether a Sim would see to it of their own accord.
    pub fn worth_doing(&self, special: Special) -> bool {
        match special {
            Special::DoLaundry => self.waiting as usize + self.piles >= LOAD as usize,
            Special::LoadDryer => false,
            _ => true,
        }
    }
}

/// Whether a kind of object is one of the laundry's.
fn machine(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Washer | ObjectKind::Dryer | ObjectKind::Clothesline)
}

/// A change of clothes at home, where there's a washer, may leave the old ones on the floor.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drop_piles(
    mut commands: Commands,
    mut dropped: MessageReader<DropClothes>,
    sims: Query<(&Sim, &Transform), (With<HouseholdMember>, Without<crate::visit::OnLot>, Without<crate::swim::Swimming>)>,
    objects: Query<&GameObject>,
    piles: Query<(), With<ClothingPile>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let mut count = piles.iter().count();
    let mut rng = rand::rng();
    for d in dropped.read() {
        let Ok((sim, tf)) = sims.get(d.0) else {
            debug!("clothes changed, not by someone at home");
            continue;
        };
        if sim.age.is_little() || count >= MAX_PILES || !objects.iter().any(|o| o.kind == ObjectKind::Washer) || !rng.random_bool(PILE_CHANCE) {
            debug!("{} keeps their clothes off the floor ({count} piles; a washer: {})", sim.first, objects.iter().any(|o| o.kind == ObjectKind::Washer));
            continue;
        }
        let ahead = tf.rotation * Vec3::Z;
        let at = tf.translation + ahead.with_y(0.0).normalize_or(Vec3::Z) * 0.35;
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        match spawn_pile(&mut commands, &mut assets, &mut ctx, &catalog, at, rng.random_range(0.0..std::f32::consts::TAU)) {
            Some(p) => {
                debug!("{} leaves their clothes in a pile ({p})", sim.first);
                count += 1;
            }
            None => warn!("no clothing pile to leave"),
        }
    }
}

fn spawn_pile(commands: &mut Commands, assets: &mut crate::objects::ObjectAssets, ctx: &mut crate::objects::AssetCtx, catalog: &crate::loading::Catalog, at: Vec3, yaw: f32) -> Option<Entity> {
    let e = crate::meals::spawn_dish(commands, assets, ctx, catalog, "clothingPileDry", ObjectKind::ClothingPile, "Dirty Clothes", at, yaw)?;
    commands.entity(e).insert(ClothingPile);
    Some(e)
}

/// On the way to do the laundry, the piles about the house are gathered up first (the nearest
/// few on the same floor; not those they couldn't reach lately).
#[allow(clippy::type_complexity)]
fn gather_piles(
    mut sims: Query<(&Transform, &mut ActionQueue, Option<&crate::interact::RouteFailed>)>,
    objects: Query<&GameObject>,
    piles: Query<(Entity, &Transform, &crate::interact::UsedBy), With<ClothingPile>>,
    clock: Res<GameClock>,
) {
    let Some(pick) = interactions_for(ObjectKind::ClothingPile).iter().position(|d| d.special == Special::PickUpClothes) else { return };
    for (tf, mut queue, failed) in &mut sims {
        let Some(front) = queue.0.front() else { continue };
        let ActionKind::Object { target, def } = front.kind else { continue };
        if front.phase != Phase::Start || front.cancel {
            continue;
        }
        let Ok(o) = objects.get(target) else { continue };
        if interactions_for(o.kind).get(def).is_none_or(|d| d.special != Special::DoLaundry) {
            continue;
        }
        let autonomous = front.autonomous;
        let mut near: Vec<(Entity, f32)> = piles
            .iter()
            .filter(|(p, ptf, used)| {
                used.0.is_none() && (ptf.translation.y - tf.translation.y).abs() < 1.5 && !failed.is_some_and(|f| f.0.iter().any(|(x, t)| x == p && clock.minutes - t < 120.0))
            })
            .map(|(p, ptf, _)| (p, ptf.translation.distance(tf.translation)))
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (p, _) in near.into_iter().take(GATHER).rev() {
            queue.0.push_front(Action::new("Pick Up", ActionKind::Object { target: p, def: pick }, autonomous));
        }
    }
}

/// The laundry's steps, as each is done.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn requests(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut laundry: ResMut<Laundry>,
    mut sims: Query<(Entity, &Sim, &LaundryRequest, &Transform, &mut ActionQueue)>,
    objects: Query<(Entity, &GameObject, &Transform, Option<&Load>)>,
    piles: Query<(), With<ClothingPile>>,
    mut household: Query<&mut Moodlets, With<HouseholdMember>>,
) {
    // (What's in a machine now: two Sims may have set out for the same one, and a machine seen
    // to this frame waits for the next.)
    let mut touched: Vec<Entity> = Vec::new();
    for (me, sim, req, tf, mut queue) in &mut sims {
        commands.entity(me).remove::<LaundryRequest>();
        let machine = match *req {
            LaundryRequest::PickedUp(_) => None,
            LaundryRequest::Wash(e) | LaundryRequest::Unload(e) | LaundryRequest::Dry(e) | LaundryRequest::Hang(e) | LaundryRequest::Fresh(e, _) => Some(e),
        };
        if machine.is_some_and(|m| touched.contains(&m)) {
            continue;
        }
        touched.extend(machine);
        let load = |e: Entity| objects.get(e).ok().and_then(|o| o.3.copied());
        match *req {
            LaundryRequest::PickedUp(pile) => {
                if piles.contains(pile) {
                    commands.entity(pile).try_despawn();
                    laundry.waiting += 1;
                }
            }
            LaundryRequest::Wash(washer) if load(washer).is_none() => {
                info!("{} puts on a load of laundry", sim.first);
                laundry.waiting = 0;
                commands.entity(washer).insert(Load::Running(clock.minutes + WASH_MINUTES));
            }
            LaundryRequest::Unload(washer) if load(washer) == Some(Load::Done) => {
                commands.entity(washer).remove::<Load>();
                // (To the nearest free dryer, else a line; with neither, they're dry enough.)
                let free = |k: ObjectKind| {
                    objects
                        .iter()
                        .filter(move |(_, o, _, load)| o.kind == k && load.is_none())
                        .min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)))
                        .map(|(e, ..)| (e, k))
                };
                match free(ObjectKind::Dryer).or_else(|| free(ObjectKind::Clothesline)) {
                    Some((to, kind)) => {
                        let Some(def) = interactions_for(kind).iter().position(|d| d.special == Special::LoadDryer) else { continue };
                        let label = if kind == ObjectKind::Clothesline { "Hang Clothes" } else { "Load the Dryer" };
                        queue.0.push_front(Action::new(label, ActionKind::Object { target: to, def }, true));
                    }
                    None => fresh(&mut household, &clock, false),
                }
            }
            LaundryRequest::Dry(drier) if load(drier).is_none() => {
                commands.entity(drier).insert(Load::Running(clock.minutes + DRY_MINUTES));
            }
            LaundryRequest::Hang(line) if load(line).is_none() => {
                commands.entity(line).insert(Load::Running(clock.minutes + LINE_MINUTES));
            }
            LaundryRequest::Fresh(drier, very) if load(drier) == Some(Load::Done) => {
                info!("{} takes the laundry out: the household's in {} clothes", sim.first, if very { "line-dried" } else { "fresh" });
                commands.entity(drier).remove::<Load>();
                fresh(&mut household, &clock, very);
            }
            _ => {}
        }
    }
}

/// The household in fresh clothes (line-dried, very fresh), and no longer smelling of the old.
fn fresh(household: &mut Query<&mut Moodlets, With<HouseholdMember>>, clock: &GameClock, very: bool) {
    for mut m in household.iter_mut() {
        m.remove(MoodletKind::FreshClothing);
        m.remove(MoodletKind::VeryFreshClothing);
        m.add(if very { MoodletKind::VeryFreshClothing } else { MoodletKind::FreshClothing }, clock.minutes);
    }
}

/// Washers, dryers and lines finish their loads in time.
fn run_machines(clock: Res<GameClock>, mut loads: Query<&mut Load>) {
    for mut l in &mut loads {
        if let Load::Running(until) = *l
            && clock.minutes >= until
        {
            *l = Load::Done;
        }
    }
}

/// The laundry as the menus and autonomy see it.
fn board(mut laundry: ResMut<Laundry>, objects: Query<(Entity, &GameObject, Option<&Load>)>, piles: Query<(), With<ClothingPile>>) {
    let l = &mut *laundry;
    l.piles = piles.iter().count();
    l.loads.clear();
    l.driers = 0;
    l.free_driers = 0;
    for (e, o, load) in &objects {
        if !machine(o.kind) {
            continue;
        }
        if let Some(load) = load {
            l.loads.insert(e, *load);
        }
        if matches!(o.kind, ObjectKind::Dryer | ObjectKind::Clothesline) {
            l.driers += 1;
            l.free_driers += usize::from(load.is_none());
        }
    }
}

/// A saved household's laundry: what was waiting, the piles (where, which floor), and the
/// machines' loads (which, where, and time left; none: done).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct SavedLaundry {
    pub waiting: u32,
    pub piles: Vec<[f32; 3]>,
    pub loads: Vec<((u32, u32, u64), [f32; 3], Option<f64>)>,
}

/// A loaded game's laundry, put back once the house's furniture is there.
#[derive(Resource, Default)]
pub struct PendingLaundry(pub Option<SavedLaundry>, Option<f32>);

impl PendingLaundry {
    pub fn of(saved: SavedLaundry) -> Self {
        Self(Some(saved), None)
    }
}

#[allow(clippy::too_many_arguments)]
fn restore(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    mut pending: ResMut<PendingLaundry>,
    mut laundry: ResMut<Laundry>,
    objects: Query<(Entity, &GameObject, &Transform)>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    if pending.0.is_none() {
        return;
    }
    let started = *pending.1.get_or_insert(time.elapsed_secs());
    let Some(saved) = pending.0.as_ref() else { return };
    // (Wait for the machines to be there, a while.)
    let all_there = saved.loads.iter().all(|(objd, at, _)| objects.iter().any(|(_, o, tf)| o.objd == *objd && tf.translation.distance(Vec3::from(*at)) < 0.2));
    if !all_there && time.elapsed_secs() - started < 10.0 {
        return;
    }
    let Some(saved) = pending.0.take() else { return };
    laundry.waiting = saved.waiting;
    for (objd, at, left) in &saved.loads {
        if let Some((e, ..)) = objects.iter().find(|(_, o, tf)| o.objd == *objd && tf.translation.distance(Vec3::from(*at)) < 0.2) {
            commands.entity(e).insert(left.map_or(Load::Done, |m| Load::Running(clock.minutes + m)));
        }
    }
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    for p in &saved.piles {
        spawn_pile(&mut commands, &mut assets, &mut ctx, &catalog, Vec3::from(*p), 0.0);
    }
}

/// The laundry as it is, for a save.
#[derive(bevy::ecs::system::SystemParam)]
pub struct LaundrySave<'w, 's> {
    laundry: Res<'w, Laundry>,
    clock: Res<'w, GameClock>,
    loads: Query<'w, 's, (&'static GameObject, &'static Transform, &'static Load)>,
    piles: Query<'w, 's, &'static Transform, With<ClothingPile>>,
}

impl LaundrySave<'_, '_> {
    pub fn saved(&self) -> SavedLaundry {
        SavedLaundry {
            waiting: self.laundry.waiting,
            piles: self.piles.iter().map(|tf| tf.translation.to_array()).collect(),
            loads: self
                .loads
                .iter()
                .map(|(o, tf, l)| (o.objd, tf.translation.to_array(), match l {
                    Load::Running(until) => Some((until - self.clock.minutes).max(1.0)),
                    Load::Done => None,
                }))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_laundrys_objects_are_known() {
        let k = |s: &str, n: &str| ObjectKind::from_script(s, n);
        assert_eq!(k("Sims3.Gameplay.Objects.Appliances.WashingMachineCheap", "washerCheap"), ObjectKind::Washer);
        assert_eq!(k("Sims3.Gameplay.Objects.Appliances.WashingMachineExpensive", "washerExpensive"), ObjectKind::Washer);
        assert_eq!(k("Sims3.Gameplay.Objects.Appliances.DryerCheap", "dryerCheap"), ObjectKind::Dryer);
        assert_eq!(k("Sims3.Gameplay.Objects.Appliances.Clothesline", "clothesLineStandard4x1"), ObjectKind::Clothesline);
        assert_eq!(k("Sims3.Gameplay.Objects.Miscellaneous.ClothingPileDry", "clothingPileDry"), ObjectKind::ClothingPile);
        assert_eq!(k("Sims3.Gameplay.Objects.Miscellaneous.WasteChute", "trashChute"), ObjectKind::WasteChute);
    }

    #[test]
    fn laundry_is_offered_by_its_state() {
        let mut l = Laundry::default();
        let (w, d) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        // Nothing to wash: no laundry.
        assert!(!l.offers(Special::DoLaundry, ObjectKind::Washer, w));
        l.piles = 1;
        assert!(l.offers(Special::DoLaundry, ObjectKind::Washer, w));
        assert!(!l.worth_doing(Special::DoLaundry));
        l.waiting = LOAD - 1;
        assert!(l.worth_doing(Special::DoLaundry));
        // Washing: nothing more until it's done; then it's dried, where there's room.
        l.loads.insert(w, Load::Running(10.0));
        assert!(!l.offers(Special::DoLaundry, ObjectKind::Washer, w));
        assert!(!l.offers(Special::MoveLaundry, ObjectKind::Washer, w));
        l.loads.insert(w, Load::Done);
        l.driers = 1;
        assert!(!l.offers(Special::MoveLaundry, ObjectKind::Washer, w));
        l.free_driers = 1;
        assert!(l.offers(Special::MoveLaundry, ObjectKind::Washer, w));
        // A dryer's emptied once it's done.
        assert!(!l.offers(Special::EmptyDryer, ObjectKind::Dryer, d));
        l.loads.insert(d, Load::Done);
        assert!(l.offers(Special::EmptyDryer, ObjectKind::Dryer, d));
    }
}
