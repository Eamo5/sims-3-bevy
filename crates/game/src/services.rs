//! Town services: the babysitter, who comes whenever the little ones would otherwise be home
//! alone, feeds them, changes them and plays with them, and is paid by the hour; the social
//! worker, who takes away a child left starving, and brings home a child to adopt; the
//! repairman, who fixes what's broken; the maid, who comes every morning to clear the dishes
//! and empty the trash; and the pizza delivery, who brings the pizza in. The maid, the
//! repairman and the delivery wear the game's uniforms and come in its service cars.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::careers::AtWork;
use crate::interact::{Broken, GameObject, ObjectKind, UsedBy};
use crate::simbody::ServiceUniform;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, Household, Notifications, Skills};
use crate::loading::CurrentWorld;
use crate::nav::{Floor, PathFollow};
use crate::sim::{Age, BLADDER, DecayScale, FUN, HUNGER, HouseholdMember, Motives, SOCIAL, Sim};
use crate::social::{SOCIALS, social_index};
use crate::{AppState, PlayMode};

pub struct ServicesPlugin;

impl Plugin for ServicesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sitter>()
            .init_resource::<Welfare>()
            .init_resource::<MaidService>()
            .add_systems(OnEnter(AppState::Loading), |mut maid: ResMut<MaidService>| *maid = MaidService::default())
            .add_systems(Update, (babysitting, social_worker, adoption, repairman, maid, pizza_delivery).run_if(in_state(PlayMode::Live)));
    }
}

/// The town's service Sims (the babysitter, the social worker, the maid, the repairman, the
/// pizza delivery): they come and go, and aren't kept in saves.
#[derive(Component)]
pub struct ServiceNpc;

/// A service Sim arrived at the curb, in their uniform if they wear one.
pub(crate) fn arrive(commands: &mut Commands, sim: Sim, at: Vec3, uniform: Option<ServiceUniform>, skills: Skills) -> Entity {
    let e = commands
        .spawn((
            Transform::from_translation(at),
            Visibility::default(),
            sim,
            Motives::default(),
            DecayScale::default(),
            crate::social::Relationships::default(),
            skills,
            crate::sim::SimAnim::default(),
            crate::anim::ClipPlayer::default(),
            crate::aging::NeedsNewBody,
            Floor(1),
            ActionQueue::default(),
            ServiceNpc,
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|c| {
            c.spawn((Transform::default(), Visibility::default()));
        })
        .id();
    if let Some(u) = uniform {
        commands.entity(e).insert(u);
    }
    e
}

/// Back to the curb and away: true once they're gone.
fn leave(commands: &mut Commands, me: Entity, queue: &mut ActionQueue, tf: &Transform, path: Option<&PathFollow>, exit: Vec2) -> bool {
    if tf.translation.xz().distance(exit) < 2.0 {
        commands.entity(me).despawn();
        return true;
    }
    if queue.0.is_empty() && path.is_none() {
        queue.0.push_back(Action::new("Go Home", ActionKind::GoHere(exit, 1), false));
    }
    false
}

// ---------------------------------------------------------------------------------------------
// The repairman

/// The repairman at work: what he's been fixing, and when he came.
#[derive(Component)]
pub struct Repairman {
    arrived: f64,
    fixed: Vec<Entity>,
    leaving: bool,
}

/// The longest the repairman stays (game minutes).
const REPAIR_SHIFT: f64 = 8.0 * 60.0;

/// The repairman comes when he's called, in his pickup, and fixes everything broken on the lot
/// in turn (a master of the trade: quick, and never shocked); then he's paid his call-out fee
/// and a little for each thing fixed.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn repairman(
    mut commands: Commands,
    visit: Option<Res<crate::interact::RepairmanVisit>>,
    clock: Res<GameClock>,
    mut household: Option<ResMut<Household>>,
    broken: Query<(Entity, &Transform, &UsedBy), (With<Broken>, With<GameObject>, Without<crate::visit::LotObject>)>,
    mut men: Query<(Entity, &mut Repairman, &mut ActionQueue, &Transform, Option<&PathFollow>)>,
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<CurrentWorld>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let Some(exit) = exit else { return };
    if let Some(v) = visit
        && clock.minutes >= v.arrive_at
    {
        commands.remove_resource::<crate::interact::RepairmanVisit>();
        if men.is_empty() {
            let mut rng = rand::rng();
            let female = rng.random_bool(0.15);
            let sim = crate::sim::random_sim(&mut rng, "Repairman", Some(female), Age::Adult);
            let p = exit.0;
            let at = Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y);
            let skills = Skills([("Handiness", 10.0)].into_iter().collect());
            let e = arrive(&mut commands, sim, at, Some(ServiceUniform::Repair), skills);
            commands.entity(e).insert(Repairman { arrived: clock.minutes, fixed: Vec::new(), leaving: false });
            rides.0.push((p, "CarServiceHandymanPickup"));
            notes.push("The repairman is here.");
        }
    }
    for (me, mut r, mut queue, tf, path) in &mut men {
        if r.leaving {
            leave(&mut commands, me, &mut queue, tf, path, exit.0);
            continue;
        }
        if !queue.0.is_empty() {
            continue;
        }
        // The nearest thing still broken (that nobody else is fixing), or he's done.
        let next = broken
            .iter()
            .filter(|(e, _, used)| used.0.is_none_or(|u| u == me) && r.fixed.iter().filter(|f| *f == e).count() < 2)
            .min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation)));
        // (Something still broken that someone else is fixing: he waits to see.)
        let pending = broken.iter().any(|(e, ..)| r.fixed.iter().filter(|f| **f == e).count() < 2);
        let on_shift = clock.minutes - r.arrived < REPAIR_SHIFT;
        match next {
            Some((target, ..)) if on_shift => {
                r.fixed.push(target);
                queue.0.push_back(Action::new("Repair", ActionKind::Repair { target }, false));
            }
            None if pending && on_shift => {}
            _ => {
                r.leaving = true;
                let mut fixed = r.fixed.clone();
                fixed.dedup();
                let n = fixed.iter().filter(|e| !broken.contains(**e)).count();
                let cost = crate::interact::REPAIRMAN_PRICE + 25 * n as i64;
                if let Some(h) = household.as_mut() {
                    h.funds -= cost;
                }
                notes.push(match n {
                    0 => format!("The repairman came by but found nothing to fix (§{cost})."),
                    1 => format!("The repairman fixed the broken object (§{cost})."),
                    n => format!("The repairman fixed {n} broken objects (§{cost})."),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The maid

/// What the maid is paid an hour.
pub const MAID_WAGE: i64 = 15;
/// When she comes each morning (hour of the day), and the longest she stays (game minutes).
const MAID_HOUR: f64 = 9.0;
const MAID_SHIFT: f64 = 4.0 * 60.0;

/// The household's maid: whether one's hired, the day she last came, and her while she's here.
#[derive(Resource, Default)]
pub struct MaidService {
    pub hired: bool,
    last_day: Option<u32>,
    maid: Option<Entity>,
}

#[derive(Component)]
pub struct Maid {
    arrived: f64,
    tried: Vec<Entity>,
    leaving: bool,
}

/// The maid comes every morning at nine while she's hired, clears away the dishes and spoiled
/// food, empties the trash cans, and goes, paid by the hour.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn maid(
    mut commands: Commands,
    mut service: ResMut<MaidService>,
    clock: Res<GameClock>,
    mut household: Option<ResMut<Household>>,
    mess: Query<(Entity, &GameObject, &Transform, &UsedBy, Option<&crate::surroundings::TrashFill>), Without<crate::visit::LotObject>>,
    mut maids: Query<(Entity, &mut Maid, &mut ActionQueue, &Transform, Option<&PathFollow>)>,
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<CurrentWorld>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let Some(exit) = exit else { return };
    let day = clock.day();
    let Some(me) = service.maid else {
        let hour = clock.hour_f();
        if service.hired && service.last_day != Some(day) && (MAID_HOUR..MAID_HOUR + 6.0).contains(&(hour as f64)) {
            service.last_day = Some(day);
            let mut rng = rand::rng();
            let age = if rng.random_bool(0.5) { Age::YoungAdult } else { Age::Adult };
            let sim = crate::sim::random_sim(&mut rng, "Maid", Some(true), age);
            let first = sim.first.clone();
            let p = exit.0;
            let at = Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y);
            let e = arrive(&mut commands, sim, at, Some(ServiceUniform::Maid), Skills::default());
            commands.entity(e).insert(Maid { arrived: clock.minutes, tried: Vec::new(), leaving: false });
            service.maid = Some(e);
            rides.0.push((p, "CarServiceMaid"));
            notes.push(format!("{first} the maid has come to clean."));
        }
        return;
    };
    let Ok((me, mut m, mut queue, tf, path)) = maids.get_mut(me) else {
        service.maid = None;
        return;
    };
    if m.leaving {
        if leave(&mut commands, me, &mut queue, tf, path, exit.0) {
            service.maid = None;
        }
        return;
    }
    if !queue.0.is_empty() {
        return;
    }
    // The nearest mess she hasn't seen to: dishes and spoiled food, then the trash.
    let chore = |o: &GameObject, fill: Option<&crate::surroundings::TrashFill>| match o.kind {
        ObjectKind::DirtyDishes => Some("Clean Up"),
        ObjectKind::TrashCan if fill.is_some_and(|f| f.0 > 0) => Some("Empty Trash"),
        _ => None,
    };
    let next = mess
        .iter()
        .filter(|(e, o, _, used, fill)| chore(o, *fill).is_some() && used.0.is_none_or(|u| u == me) && !m.tried.contains(e))
        .min_by(|a, b| {
            let rank = |o: &GameObject| (o.kind == ObjectKind::TrashCan) as u8;
            (rank(a.1), a.2.translation.distance(tf.translation)).partial_cmp(&(rank(b.1), b.2.translation.distance(tf.translation))).unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|(e, o, _, _, fill)| {
            let name = chore(o, fill)?;
            let def = crate::interact::interactions_for(o.kind).iter().position(|d| d.name == name)?;
            Some((e, name, def))
        });
    // (With nothing to do she still comes in and has a look round the kitchen first.)
    let kitchen = mess.iter().find(|(_, o, ..)| matches!(o.kind, ObjectKind::Stove | ObjectKind::Fridge | ObjectKind::Sink)).map(|(_, _, t, ..)| t.translation.xz());
    match next {
        Some((target, name, def)) if clock.minutes - m.arrived < MAID_SHIFT => {
            m.tried.push(target);
            queue.0.push_back(Action::new(name, ActionKind::Object { target, def }, false));
        }
        None if clock.minutes - m.arrived < 30.0 && path.is_none() && kitchen.is_some_and(|k| k.distance(tf.translation.xz()) > 2.5) => {
            if let Some(k) = kitchen {
                queue.0.push_back(Action::new("Look Round", ActionKind::GoHere(k, 1), false));
            }
        }
        None if clock.minutes - m.arrived < 30.0 => {}
        _ => {
            m.leaving = true;
            let hours = ((clock.minutes - m.arrived) / 60.0).ceil().max(1.0) as i64;
            let wage = hours * MAID_WAGE;
            if let Some(h) = household.as_mut() {
                h.funds -= wage;
            }
            notes.push(format!("The maid finished cleaning: {hours} hour{} came to §{wage}.", if hours == 1 { "" } else { "s" }));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The pizza delivery

/// The pizza on its way in: where it's to be set down.
#[derive(Component)]
pub struct PizzaDelivery {
    to: Vec3,
    since: f64,
    delivered: bool,
}

/// The pizza arrives with the delivery, who carries it in to the kitchen counter and leaves it
/// there (or by the door when there's no counter) before driving off.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn pizza_delivery(
    mut commands: Commands,
    order: Option<Res<crate::meals::PizzaOrder>>,
    clock: Res<GameClock>,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
    mut couriers: Query<(Entity, &mut PizzaDelivery, &mut ActionQueue, &Transform, Option<&PathFollow>)>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<CurrentWorld>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let Some(exit) = exit else { return };
    if let Some(o) = order
        && clock.minutes >= o.arrive_at
    {
        commands.remove_resource::<crate::meals::PizzaOrder>();
        let p = exit.0;
        let curb = Vec3::new(p.x, world.data.heightmap.sample(p.x, p.y), p.y);
        let Some(to) = crate::meals::pizza_spot(&objects, curb) else {
            notes.push("The pizza came, but there was nowhere to put it.");
            return;
        };
        let mut rng = rand::rng();
        let age = if rng.random_bool(0.6) { Age::Teen } else { Age::YoungAdult };
        let sim = crate::sim::random_sim(&mut rng, "Delivery", None, age);
        let e = arrive(&mut commands, sim, curb, Some(ServiceUniform::PizzaDelivery), Skills::default());
        commands.entity(e).insert(PizzaDelivery { to, since: clock.minutes, delivered: false });
        rides.0.push((p, "CarServiceHatchback"));
        notes.push("The pizza delivery is here.");
    }
    for (me, mut d, mut queue, tf, path) in &mut couriers {
        if d.delivered {
            leave(&mut commands, me, &mut queue, tf, path, exit.0);
            continue;
        }
        let near = tf.translation.xz().distance(d.to.xz()) < 1.6;
        // There (or as near as the way in allows, or kept waiting too long): the pizza goes down.
        if near || (queue.0.is_empty() && path.is_none() && clock.minutes - d.since > 5.0) || clock.minutes - d.since > 120.0 {
            d.delivered = true;
            queue.0.clear();
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
            if let Some(p) = crate::meals::spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, "FoodPizza", ObjectKind::Meal, "Pizza", d.to, 0.0) {
                commands.entity(p).insert(crate::meals::Meal { servings: 6 });
                notes.push("The pizza has arrived! It's on the kitchen counter.");
            }
            continue;
        }
        if queue.0.is_empty() && path.is_none() {
            // Up to the counter's front.
            let at = d.to.xz() + (tf.translation.xz() - d.to.xz()).normalize_or(Vec2::X) * 0.8;
            queue.0.push_back(Action::new("Deliver the Pizza", ActionKind::GoHere(at, 1), false));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The babysitter

/// What the babysitter is paid an hour.
pub const SITTER_WAGE: i64 = 10;

#[derive(Resource, Default)]
struct Sitter {
    /// When the little ones were first left alone.
    alone_since: Option<f64>,
    sitter: Option<Entity>,
}

#[derive(Component)]
pub struct Babysitter {
    arrived: f64,
    leaving: bool,
}

// ---------------------------------------------------------------------------------------------
// The social worker

/// How long a baby or toddler (and a child) may go starving before the social worker comes
/// (game minutes).
const NEGLECT_LITTLE: f64 = 6.0 * 60.0;
const NEGLECT_CHILD: f64 = 12.0 * 60.0;

#[derive(Resource, Default)]
struct Welfare {
    /// Children going hungry, since when, and whether the family's been warned.
    neglected: Vec<(Entity, f64, bool)>,
    /// The social worker on her way, and the child she's come for.
    worker: Option<(Entity, Entity)>,
}

#[derive(Component)]
pub struct SocialWorker {
    leaving: bool,
}

/// A child to adopt, on the way.
#[derive(Resource)]
pub struct AdoptionOrder {
    pub arrive_at: f64,
    pub age: Age,
    pub female: bool,
}

/// "a baby girl", "a child".
pub fn adoptee(age: Age, female: bool) -> String {
    let who = if female { "girl" } else { "boy" };
    match age {
        Age::Baby => format!("a baby {who}"),
        Age::Toddler => format!("a toddler {who}"),
        _ => format!("a {who}"),
    }
}

/// The social worker's arrival with a child to adopt, who joins the household at the door.
#[allow(clippy::too_many_arguments)]
fn adoption(
    mut commands: Commands,
    clock: Res<GameClock>,
    order: Option<Res<AdoptionOrder>>,
    mut state: ResMut<Welfare>,
    household: Option<Res<Household>>,
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<CurrentWorld>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let (Some(order), Some(exit)) = (order, exit) else { return };
    if clock.minutes < order.arrive_at || state.worker.is_some() {
        return;
    }
    commands.remove_resource::<AdoptionOrder>();
    let mut rng = rand::rng();
    let last = household.as_ref().map_or("Sim".to_string(), |h| h.name.clone());
    let p = exit.0;
    let y = world.data.heightmap.sample(p.x, p.y);
    let body = |commands: &mut Commands, sim: Sim, at: Vec3| {
        commands
            .spawn((
                Transform::from_translation(at),
                Visibility::default(),
                sim,
                Motives::default(),
                DecayScale::default(),
                crate::social::Relationships::default(),
                Skills::default(),
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
    };
    let worker = body(&mut commands, crate::sim::random_sim(&mut rng, "Social Worker", Some(true), Age::Adult), Vec3::new(p.x, y, p.y));
    commands.entity(worker).insert((SocialWorker { leaving: true }, ServiceNpc));
    let child = crate::sim::random_sim(&mut rng, &last, Some(order.female), order.age);
    let name = child.first.clone();
    let c = body(&mut commands, child, Vec3::new(p.x + 0.8, y, p.y));
    commands.entity(c).insert((HouseholdMember, crate::aging::Aging::default()));
    rides.0.push((p, "CarServiceSedan"));
    notes.push(format!("The social worker brought {} home: welcome to the family, {name}!", adoptee(order.age, order.female)));
    // (She leaves straight away.)
    state.worker = Some((worker, c));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn social_worker(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut state: ResMut<Welfare>,
    members: Query<(Entity, &Sim, &Motives, &Transform, Has<crate::sim::Selected>, Out), (With<HouseholdMember>, Without<SocialWorker>)>,
    mut workers: Query<(Entity, &mut SocialWorker, &mut ActionQueue, &Transform, Option<&PathFollow>), Without<HouseholdMember>>,
    mut moods: Query<&mut crate::life::Moodlets>,
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<CurrentWorld>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let Some(exit) = exit else { return };
    let out = |o: (bool, bool, bool, bool)| o.0 || o.1 || o.2 || o.3;
    // Children going hungry at home.
    let starving: Vec<Entity> = members
        .iter()
        .filter(|(_, s, m, _, _, o)| (s.age.is_little() || s.age == Age::Child) && m.0[HUNGER] <= -90.0 && !out(*o))
        .map(|q| q.0)
        .collect();
    state.neglected.retain(|(e, ..)| starving.contains(e));
    for e in &starving {
        if !state.neglected.iter().any(|(x, ..)| x == e) {
            state.neglected.push((*e, clock.minutes, false));
        }
    }
    if let Some((we, child)) = state.worker {
        let Ok((me, mut w, mut queue, tf, path)) = workers.get_mut(we) else {
            state.worker = None;
            return;
        };
        if w.leaving {
            if tf.translation.xz().distance(exit.0) < 2.0 {
                commands.entity(me).despawn();
                state.worker = None;
            } else if queue.0.is_empty() && path.is_none() {
                queue.0.push_back(Action::new("Leave", ActionKind::GoHere(exit.0, 1), false));
            }
            return;
        }
        let Ok((ce, sim, _, ctf, selected, _)) = members.get(child) else {
            w.leaving = true;
            return;
        };
        if ctf.translation.distance(tf.translation) < 1.6 {
            // She takes the child away; the family is heartbroken.
            notes.push(format!("{} has been taken away by the social worker.", sim.full_name()));
            let others: Vec<Entity> = members.iter().map(|q| q.0).filter(|e| *e != ce).collect();
            for e in &others {
                if let Ok(mut m) = moods.get_mut(*e) {
                    m.add(crate::life::MoodletKind::Heartbroken, clock.minutes);
                }
            }
            if selected && let Some(&s) = others.first() {
                commands.entity(s).insert(crate::sim::Selected);
            }
            commands.entity(ce).despawn();
            state.neglected.retain(|(e, ..)| *e != ce);
            w.leaving = true;
            queue.0.clear();
        } else if queue.0.is_empty() && path.is_none() {
            let to = ctf.translation.xz() + (tf.translation.xz() - ctf.translation.xz()).normalize_or(Vec2::X) * 0.9;
            queue.0.push_back(Action::new("Take the Child", ActionKind::GoHere(to, 1), false));
        }
        return;
    }
    // Neglected long enough: she comes for them (warned halfway).
    let mut coming = None;
    for (e, since, warned) in state.neglected.iter_mut() {
        let Ok((_, sim, ..)) = members.get(*e) else { continue };
        let limit = if sim.age.is_little() { NEGLECT_LITTLE } else { NEGLECT_CHILD };
        let gone = clock.minutes - *since;
        if !*warned && gone >= limit / 2.0 {
            *warned = true;
            notes.push(format!("{} is going hungry! If no one feeds {}, the social worker will be called.", sim.first, if sim.female { "her" } else { "him" }));
        }
        if gone >= limit {
            coming = Some(*e);
            break;
        }
    }
    if let Some(e) = coming
        && let Ok((_, sim, ..)) = members.get(e)
    {
        let mut rng = rand::rng();
        let base = crate::sim::random_sim(&mut rng, "Social Worker", Some(true), Age::Adult);
        let p = exit.0;
        let y = world.data.heightmap.sample(p.x, p.y);
        let w = commands
            .spawn((
                Transform::from_xyz(p.x, y, p.y),
                Visibility::default(),
                base,
                Motives::default(),
                DecayScale::default(),
                crate::social::Relationships::default(),
                Skills::default(),
                crate::sim::SimAnim::default(),
                crate::anim::ClipPlayer::default(),
                crate::aging::NeedsNewBody,
                Floor(1),
                ActionQueue::default(),
                SocialWorker { leaving: false },
                ServiceNpc,
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                c.spawn((Transform::default(), Visibility::default()));
            })
            .id();
        rides.0.push((p, "CarServiceSedan"));
        notes.push(format!("The social worker has come for {}, who has been left to go hungry.", sim.first));
        state.worker = Some((w, e));
    }
}

/// Whether a household member is out (at work or school, at a rabbit hole, out on the town).
type Out = (Has<AtWork>, Has<crate::rabbitholes::AtRabbitHole>, Has<crate::visit::Trip>, Has<crate::visit::OnLot>);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn babysitting(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut state: ResMut<Sitter>,
    mut household: Option<ResMut<Household>>,
    members: Query<(Entity, &Sim, &Motives, &Transform, Out), (With<HouseholdMember>, Without<Babysitter>)>,
    mut sitters: Query<(Entity, &mut Babysitter, &mut ActionQueue, &Transform, Option<&PathFollow>), Without<HouseholdMember>>,
    exit: Option<Res<crate::interact::LotExit>>,
    world: Res<CurrentWorld>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let Some(exit) = exit else { return };
    let out = |o: (bool, bool, bool, bool)| o.0 || o.1 || o.2 || o.3;
    let littles: Vec<(Entity, &Sim, &Motives, &Transform)> =
        members.iter().filter(|(_, s, _, _, o)| s.age.is_little() && !out(*o)).map(|(e, s, m, t, _)| (e, s, m, t)).collect();
    let carer_home = members.iter().any(|(_, s, _, _, o)| (s.age == Age::Teen || s.age.is_grown()) && !out(o));
    let needed = !littles.is_empty() && !carer_home;
    let mut rng = rand::rng();

    let Some(se) = state.sitter else {
        if !needed {
            state.alone_since = None;
            return;
        }
        let since = *state.alone_since.get_or_insert(clock.minutes);
        if clock.minutes - since < 15.0 {
            return;
        }
        // She arrives by car at the curb.
        let (female, age) = (rng.random_bool(0.8), if rng.random_bool(0.5) { Age::YoungAdult } else { Age::Adult });
        let base = crate::sim::random_sim(&mut rng, "Babysitter", Some(female), age);
        let first = base.first.clone();
        let p = exit.0;
        let y = world.data.heightmap.sample(p.x, p.y);
        let e = commands
            .spawn((
                Transform::from_xyz(p.x, y, p.y),
                Visibility::default(),
                base,
                Motives::default(),
                DecayScale::default(),
                crate::social::Relationships::default(),
                Skills::default(),
                crate::sim::SimAnim::default(),
                crate::anim::ClipPlayer::default(),
                crate::aging::NeedsNewBody,
                Floor(1),
                ActionQueue::default(),
                Babysitter { arrived: clock.minutes, leaving: false },
                ServiceNpc,
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                c.spawn((Transform::default(), Visibility::default()));
            })
            .id();
        state.sitter = Some(e);
        rides.0.push((p, "CarServiceSedan"));
        let names: Vec<&str> = littles.iter().map(|l| l.1.first.as_str()).collect();
        notes.push(format!("{first} the babysitter has come to look after {} (§{SITTER_WAGE} an hour).", names.join(" and ")));
        return;
    };
    let Ok((me, mut b, mut queue, tf, path)) = sitters.get_mut(se) else {
        state.sitter = None;
        return;
    };
    if b.leaving {
        let at_exit = tf.translation.xz().distance(exit.0) < 2.0;
        if at_exit {
            commands.entity(me).despawn();
            state.sitter = None;
            state.alone_since = None;
        } else if queue.0.is_empty() && path.is_none() {
            queue.0.push_back(Action::new("Go Home", ActionKind::GoHere(exit.0, 1), false));
        }
        return;
    }
    if !needed {
        // Someone's home: she's paid and goes.
        b.leaving = true;
        let hours = ((clock.minutes - b.arrived) / 60.0).ceil().max(1.0) as i64;
        let wage = hours * SITTER_WAGE;
        if let Some(h) = household.as_mut() {
            h.funds -= wage;
        }
        queue.0.clear();
        notes.push(format!("The babysitter went home. {hours} hour{} came to §{wage}.", if hours == 1 { "" } else { "s" }));
        return;
    }
    if !queue.0.is_empty() {
        return;
    }
    // The neediest little one: food, a change, a nap or some fun.
    let mut best: Option<(f32, Entity, &str)> = None;
    for (e, _, m, _) in &littles {
        let wants = [("Feed", m.0[HUNGER]), ("Change Diaper", m.0[BLADDER]), ("Play With", m.0[SOCIAL].min(m.0[FUN]))];
        for (what, v) in wants {
            if v < 40.0 && best.is_none_or(|b| v < b.0) {
                best = Some((v, *e, what));
            }
        }
    }
    match best.and_then(|(_, e, what)| social_index(what).map(|si| (e, si))) {
        Some((target, si)) => {
            debug!("the babysitter sees to {}: {}", littles.iter().find(|l| l.0 == target).map_or("?", |l| l.1.first.as_str()), SOCIALS[si].name);
            queue.0.push_back(Action::new(SOCIALS[si].name, ActionKind::Social { target, social: si }, false));
        }
        None => {
            // Keep an eye on them.
            if let Some((_, _, _, ltf)) = littles.choose(&mut rng)
                && ltf.translation.distance(tf.translation) > 4.0
            {
                let a = rng.random_range(0.0..std::f32::consts::TAU);
                let to = ltf.translation.xz() + Vec2::new(a.cos(), a.sin()) * 1.8;
                queue.0.push_back(Action::new("Keep an Eye on the Children", ActionKind::GoHere(to, 1), false));
            }
        }
    }
}
