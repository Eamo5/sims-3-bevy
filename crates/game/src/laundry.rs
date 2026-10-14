//! Laundry, as the game's (Generations, its `LaundryManager`): where there's laundry to do it
//! with, Sims taking off their everyday clothes indoors at home leave them in a pile on the
//! floor, or in the hamper if there's one in the room with room in it (slobs, as often as not,
//! drop them anyway): two piles a day each at most (a slob's four). Piles are picked up (the Neat
//! at once, slobs never) and, once there's a load, someone does the laundry with the game's
//! clips: half an hour in the washer, ten minutes in the dryer (or half an hour on the line).
//! For twelve hours after it's dry and taken out, whoever dresses is in Fresh Clothing (Very
//! Fresh, line-dried or from fine machines). Kept in saves.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Special, interactions_for};
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{HouseholdMember, Sim};
use crate::simbody::OutfitKind;

pub struct LaundryPlugin;

impl Plugin for LaundryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Laundry>()
            .init_resource::<PendingLaundry>()
            .add_message::<ChangedClothes>()
            .add_systems(Update, (changes, gather_piles, requests, run_machines, board, restore).chain().run_if(in_state(PlayMode::Live)));
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
    /// Dryers and lines, and how many are free; free washers.
    pub driers: usize,
    pub free_driers: usize,
    pub free_washers: usize,
    /// Until when clean laundry gives Fresh Clothing to those who dress (game minutes), and
    /// whether it's Very Fresh; whether the last wash was in a fine machine.
    pub fresh_until: f64,
    pub very: bool,
    fine_wash: bool,
    /// The piles each Sim has left today (the day, and how many).
    left_today: HashMap<Entity, (u32, u32)>,
    /// Not before then is the laundry tried again of anyone's own accord (game minutes): once a
    /// load came to nothing, the piles out of reach.
    pub rest_until: f64,
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

/// A Sim has changed their clothes (see `dressing`), from one outfit into another (none: their
/// everyday clothes): perhaps leaving the old ones lying, perhaps into fresh ones.
#[derive(Message, Clone, Copy)]
pub struct ChangedClothes {
    pub sim: Entity,
    pub from: Option<OutfitKind>,
    pub to: Option<OutfitKind>,
}

/// What a Sim has just done with the laundry (set by the interactions, handled here).
#[derive(Component, Clone, Copy, Debug)]
pub enum LaundryRequest {
    PickedUp(Entity),
    /// Took the clothes out of the hamper, for a washer.
    FromHamper,
    Wash(Entity),
    /// Took the wet clothes out of the washer, for the dryer or the line.
    Unload(Entity),
    Dry(Entity),
    Hang(Entity),
    /// Took the dry clothes out: line-dried (very fresh) or not.
    Fresh(Entity, bool),
}

/// The most piles a Sim leaves in a day, a slob's, and the chance a slob drops them by a hamper
/// (the game's `kMaximumNumberOfClothingPilesPerSim`, `...PerSlob`, `kChanceSlobBehavior`).
const PILES_A_DAY: u32 = 2;
const SLOB_PILES_A_DAY: u32 = 4;
const SLOB_DROPS: f64 = 0.5;
/// What a hamper holds (the game's `kCapacity`), and how near it must be (in the room).
const HAMPER_HOLDS: u32 = 16;
const HAMPER_REACH: f32 = 6.0;
/// How many clothes make a load worth washing, without being asked.
pub const LOAD: u32 = 4;
/// How long the laundry's left after a load came to nothing (game minutes).
const REST_MINUTES: f64 = 60.0;
/// How far from where they stand a pile may be left, for floor that can be walked to.
const PILE_REACH: i32 = 3;
/// How long the washer, the dryer and the line take (the game's `kTimeToWashNormal`, `DryTime`,
/// the line's `kDryTime`), and how long the laundry keeps giving Fresh Clothing
/// (`FreshClothingBuffTimeInHrs`), in game minutes.
const WASH_MINUTES: f64 = 30.0;
const DRY_MINUTES: f64 = 10.0;
const LINE_MINUTES: f64 = 30.0;
const FRESH_MINUTES: f64 = 12.0 * 60.0;
/// The price from which a washer or dryer is a fine one (both: Very Fresh Clothing).
const FINE_PRICE: i32 = 1000;
/// The most piles gathered up on the way to the washer.
const GATHER: usize = 4;

impl Laundry {
    /// Whether an object's laundry step is on offer now (for the menus; autonomy asks
    /// `worth_doing` besides).
    pub fn offers(&self, special: Special, kind: ObjectKind, target: Entity) -> bool {
        let load = self.loads.get(&target).copied();
        match special {
            Special::DoLaundry => load.is_none() && self.waiting as usize + self.piles > 0,
            Special::EmptyHamper => self.waiting > 0 && self.free_washers > 0,
            Special::MoveLaundry => load == Some(Load::Done) && (self.driers == 0 || self.free_driers > 0),
            Special::EmptyDryer => load == Some(Load::Done),
            Special::LoadDryer => kind != ObjectKind::Washer && load.is_none(),
            _ => true,
        }
    }

    /// Whether a Sim would see to it of their own accord (at `now`, game minutes).
    pub fn worth_doing(&self, special: Special, now: f64) -> bool {
        match special {
            Special::DoLaundry | Special::EmptyHamper => self.waiting as usize + self.piles >= LOAD as usize && now >= self.rest_until,
            Special::LoadDryer => false,
            _ => true,
        }
    }
}

/// Whether a kind of object is one of the laundry's.
fn machine(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Washer | ObjectKind::Dryer | ObjectKind::Clothesline)
}

/// A change of clothes at home: out of everyday clothes indoors, perhaps a pile on the floor (or
/// into the hamper); into clean ones while the laundry's fresh, Fresh Clothing; into nothing or
/// a swimsuit, the fresh clothes off.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn changes(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut changed: MessageReader<ChangedClothes>,
    mut laundry: ResMut<Laundry>,
    mut sims: Query<(&Sim, &Transform, &mut Moodlets), (With<HouseholdMember>, Without<crate::visit::OnLot>, Without<crate::swim::Swimming>)>,
    objects: Query<(&GameObject, &Transform)>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (grid, upper): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::nav::UpperFloors>>),
) {
    let mut rng = rand::rng();
    let day = clock.day();
    for c in changed.read() {
        let Ok((sim, tf, mut moodlets)) = sims.get_mut(c.sim) else { continue };
        // Into clean clothes, or out of them.
        let dressed = |o: Option<OutfitKind>| matches!(o, None | Some(OutfitKind::Formal | OutfitKind::Athletic | OutfitKind::Career | OutfitKind::Sleepwear | OutfitKind::Outerwear));
        if matches!(c.to, Some(OutfitKind::Naked | OutfitKind::Swimwear)) {
            moodlets.remove(MoodletKind::FreshClothing);
            moodlets.remove(MoodletKind::VeryFreshClothing);
        } else if dressed(c.to) && clock.minutes < laundry.fresh_until && !moodlets.has(MoodletKind::Smelly) {
            moodlets.add(if laundry.very { MoodletKind::VeryFreshClothing } else { MoodletKind::FreshClothing }, clock.minutes);
        }
        // Out of the day's clothes (not out of nightwear, a swimsuit or nothing), indoors at
        // home where the laundry's done.
        let taken_off = !matches!(c.from, Some(OutfitKind::Naked | OutfitKind::Sleepwear | OutfitKind::Swimwear));
        // (In a room on the floor they're on: a bathroom upstairs over the porch is indoors.)
        let indoors = building.as_deref().is_some_and(|b| b.room_at(b.level_at(tf.translation.y + 0.3), tf.translation).is_some_and(|k| k != s3bake::ROOM_OUTSIDE && k != s3bake::ROOM_PORCH));
        if !taken_off || sim.age.is_little() || !indoors || !objects.iter().any(|(o, _)| machine(o.kind)) {
            trace!("{}'s clothes not left: taken off {taken_off}, indoors {indoors}", sim.first);
            continue;
        }
        let slob = sim.traits.contains(&Trait::Slob);
        let left = laundry.left_today.entry(c.sim).or_insert((day, 0));
        if left.0 != day {
            *left = (day, 0);
        }
        if left.1 >= if slob { SLOB_PILES_A_DAY } else { PILES_A_DAY } {
            continue;
        }
        left.1 += 1;
        // (Into a hamper in the room with room in it, unless a slob can't be bothered.)
        let hampers = objects.iter().filter(|(o, t)| o.kind == ObjectKind::Hamper && t.translation.distance(tf.translation) < HAMPER_REACH && (t.translation.y - tf.translation.y).abs() < 1.5).count() as u32;
        if hampers > 0 && laundry.waiting < hampers * HAMPER_HOLDS && !(slob && rng.random_bool(SLOB_DROPS)) {
            debug!("{} puts their clothes in the hamper", sim.first);
            laundry.waiting += 1;
            continue;
        }
        // (Dropped where they stand, or as near as there's open floor to pick it up from: not in
        // the shower stall they undressed in, nor the sink in front of them.)
        let level = building.as_deref().map_or(1, |b| b.level_at(tf.translation.y + 0.3));
        let near = (0..=PILE_REACH)
            .flat_map(|r| (-r..=r).flat_map(move |dx| (-r..=r).filter(move |dz| dx.abs().max(dz.abs()) == r).map(move |dz| (dx, dz))))
            .map(|(dx, dz)| tf.translation.xz() + Vec2::new(dx as f32, dz as f32) * 0.5)
            .find(|p| crate::puddles::free_at(grid.as_deref(), upper.as_deref(), level, *p));
        let Some(at) = near.map(|p| Vec3::new(p.x, tf.translation.y, p.y)) else {
            debug!("{}'s clothes not left: no open floor about", sim.first);
            continue;
        };
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        match spawn_pile(&mut commands, &mut assets, &mut ctx, &catalog, at, rng.random_range(0.0..std::f32::consts::TAU)) {
            Some(p) => debug!("{} leaves their clothes in a pile ({p})", sim.first),
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
) {
    // (What's in a machine now: two Sims may have set out for the same one, and a machine seen
    // to this frame waits for the next.)
    let mut touched: Vec<Entity> = Vec::new();
    for (me, sim, req, tf, mut queue) in &mut sims {
        commands.entity(me).remove::<LaundryRequest>();
        let machine = match *req {
            LaundryRequest::PickedUp(_) | LaundryRequest::FromHamper => None,
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
            LaundryRequest::FromHamper => {
                // (Off to the nearest free washer with them.)
                let washer = objects
                    .iter()
                    .filter(|(_, o, _, load)| o.kind == ObjectKind::Washer && load.is_none())
                    .min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
                if let Some((w, ..)) = washer
                    && let Some(def) = interactions_for(ObjectKind::Washer).iter().position(|d| d.special == Special::DoLaundry)
                {
                    queue.0.push_front(Action::new("Do Laundry", ActionKind::Object { target: w, def }, true));
                }
            }
            // (Nothing gathered to wash: the piles were out of reach. Left a while.)
            LaundryRequest::Wash(_) if laundry.waiting == 0 => {
                debug!("{} found nothing to wash", sim.first);
                laundry.rest_until = clock.minutes + REST_MINUTES;
            }
            LaundryRequest::Wash(washer) if load(washer).is_none() => {
                info!("{} puts on a load of laundry", sim.first);
                laundry.waiting = 0;
                laundry.fine_wash = objects.get(washer).is_ok_and(|o| o.1.price >= FINE_PRICE);
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
                    None => fresh(&mut laundry, &clock, false),
                }
            }
            LaundryRequest::Dry(drier) if load(drier).is_none() => {
                commands.entity(drier).insert(Load::Running(clock.minutes + DRY_MINUTES));
            }
            LaundryRequest::Hang(line) if load(line).is_none() => {
                commands.entity(line).insert(Load::Running(clock.minutes + LINE_MINUTES));
            }
            LaundryRequest::Fresh(drier, line) if load(drier) == Some(Load::Done) => {
                let very = line || (laundry.fine_wash && objects.get(drier).is_ok_and(|o| o.1.price >= FINE_PRICE));
                info!("{} takes the laundry out ({})", sim.first, if very { "very fresh" } else { "fresh" });
                commands.entity(drier).remove::<Load>();
                fresh(&mut laundry, &clock, very);
            }
            _ => {}
        }
    }
}

/// Clean laundry: Fresh Clothing (or Very Fresh) for whoever dresses in the next twelve hours
/// (the game's `OnDryerCycleFinished`).
fn fresh(laundry: &mut Laundry, clock: &GameClock, very: bool) {
    laundry.very = if clock.minutes < laundry.fresh_until { laundry.very || very } else { very };
    laundry.fresh_until = clock.minutes + FRESH_MINUTES;
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
    l.free_washers = 0;
    for (e, o, load) in &objects {
        if !machine(o.kind) {
            continue;
        }
        if let Some(load) = load {
            l.loads.insert(e, *load);
        }
        if o.kind == ObjectKind::Washer {
            l.free_washers += usize::from(load.is_none());
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
    /// How long the clean laundry has left to give Fresh Clothing (game minutes), and whether
    /// Very Fresh.
    #[serde(default)]
    pub fresh_left: f64,
    #[serde(default)]
    pub very: bool,
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
    laundry.fresh_until = clock.minutes + saved.fresh_left;
    laundry.very = saved.very;
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
            fresh_left: (self.laundry.fresh_until - self.clock.minutes).max(0.0),
            very: self.laundry.very,
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
        assert_eq!(k("Sims3.Gameplay.Objects.Appliances.Mimics.WasherVenueLaundry", "washerVenueLaundry"), ObjectKind::Washer);
        assert_eq!(k("Sims3.Gameplay.Objects.Appliances.Mimics.DryerVenueLaundry", "dryerVenueLaundry"), ObjectKind::Dryer);
        assert_eq!(k("Sims3.Gameplay.Objects.Appliances.Mimics.RefrigeratorPatio", "RefrigeratorPatio"), ObjectKind::Fridge);
        assert_eq!(k("Sims3.Gameplay.Objects.Appliances.Clothesline", "clothesLineStandard4x1"), ObjectKind::Clothesline);
        assert_eq!(k("Sims3.Gameplay.Objects.Miscellaneous.ClothingPileDry", "clothingPileDry"), ObjectKind::ClothingPile);
        assert_eq!(k("Sims3.Gameplay.Objects.Miscellaneous.WasteChute", "trashChute"), ObjectKind::WasteChute);
        assert_eq!(k("Sims3.Gameplay.Objects.Miscellaneous.Hamper", "hamperStandard"), ObjectKind::Hamper);
    }

    #[test]
    fn clean_laundry_stays_fresh_for_half_a_day() {
        let mut l = Laundry::default();
        let mut clock = GameClock::default();
        clock.minutes = 100.0;
        fresh(&mut l, &clock, false);
        assert_eq!(l.fresh_until, 100.0 + FRESH_MINUTES);
        assert!(!l.very);
        // (A line-dried load while the last is still fresh: very fresh, and fresh for longer.)
        clock.minutes = 200.0;
        fresh(&mut l, &clock, true);
        assert!(l.very && l.fresh_until == 200.0 + FRESH_MINUTES);
        // (Once it's worn off, the next is as it is.)
        clock.minutes = 2000.0;
        fresh(&mut l, &clock, false);
        assert!(!l.very);
    }

    #[test]
    fn laundry_is_offered_by_its_state() {
        let mut l = Laundry::default();
        let (w, d) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        // Nothing to wash: no laundry.
        assert!(!l.offers(Special::DoLaundry, ObjectKind::Washer, w));
        l.piles = 1;
        assert!(l.offers(Special::DoLaundry, ObjectKind::Washer, w));
        assert!(!l.worth_doing(Special::DoLaundry, 0.0));
        l.waiting = LOAD - 1;
        assert!(l.worth_doing(Special::DoLaundry, 0.0));
        // (Left a while after a load came to nothing.)
        l.rest_until = 60.0;
        assert!(!l.worth_doing(Special::DoLaundry, 30.0));
        assert!(l.worth_doing(Special::DoLaundry, 61.0));
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
