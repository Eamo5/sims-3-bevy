//! Visiting other households at home, as the game's (`VisitSituation`): from the map, a home
//! the town's families live in is somewhere to go too. Whoever of the family is home (not at
//! work or school) is there when the visitor pulls up, going about their own lives on their own
//! lot; the visitor goes to the front door and rings, and one of them comes and greets them in.
//! Nobody home, or nobody answering, the visitor's left outside. Late at night (from eleven) the
//! hosts ask the visitor to go home. Those met are kept, with what they thought of each other, as
//! the household's other acquaintances are.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, AutonomyTimer, GoingHome, Notifications, OffLot, Skills, Visitor};
use crate::sim::{Age, HouseholdMember, Sim};
use crate::social::{RelStatus, Relationships};
use crate::visit::{OnLot, VisitedLot};

pub struct HomesPlugin;

impl Plugin for HomesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FamilyHomes>()
            .add_systems(Update, (residents_home, ring_the_bell, asked_to_leave, residents_away, family_homes).chain().run_if(in_state(PlayMode::Live)));
        if std::env::var("HOME_VISIT").is_ok() {
            app.add_systems(Update, home_visit_test.run_if(in_state(PlayMode::Live)));
        }
    }
}

/// The homes of the town's families (their lots and names, nearest the household's first): where
/// the car's "Visit Lot" goes.
#[derive(Resource, Default, Clone)]
pub struct FamilyHomes(pub Vec<(usize, String)>);

/// Who lives where, looked over now and then.
#[allow(clippy::too_many_arguments)]
fn family_homes(
    time: Res<Time>,
    mut last: Local<f32>,
    mut homes: ResMut<FamilyHomes>,
    world: Option<Res<crate::loading::CurrentWorld>>,
    (town, story, dormant, household): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>, Option<Res<crate::interact::Household>>),
) {
    if time.elapsed_secs() - *last < 5.0 && !homes.0.is_empty() {
        return;
    }
    *last = time.elapsed_secs();
    let Some(world) = world else { return };
    let active = household.as_ref().map(|h| h.lot_index);
    let from = active.and_then(|l| world.data.lots.get(l)).map(crate::home::lot_center).unwrap_or_default();
    let mut list: Vec<(usize, String, f32)> = (0..world.data.lots.len())
        .filter(|&l| world.data.lots[l].is_residential())
        .filter_map(|l| family_at(&world, town.as_deref(), &story, &dormant, active, l).map(|f| (l, f.name.clone(), crate::home::lot_center(&world.data.lots[l]).distance(from))))
        .collect();
    list.sort_by(|a, b| a.2.total_cmp(&b.2));
    homes.0 = list.into_iter().map(|(l, n, _)| (l, n)).collect();
}

/// One of a visited family, at home on their lot.
#[derive(Component, Clone, Copy)]
pub struct Resident(pub usize);

/// The hour the hosts start asking visitors to go home (the game's visit ends late at night).
const BEDTIME: u32 = 23;
/// How often (game minutes) the hosts look at whether it's time the visitor went.
const ASK_EVERY: f64 = 30.0;

/// The town's family living on a lot (not the household played), if any.
pub fn family_at<'a>(
    world: &crate::loading::CurrentWorld,
    town: Option<&'a crate::premade::TownPremades>,
    story: &crate::story::TownStory,
    dormant: &crate::household::Dormant,
    active: Option<usize>,
    lot: usize,
) -> Option<&'a s3bake::premades::HouseholdBaked> {
    if Some(lot) == active {
        return None;
    }
    let town = town?;
    let occupants = crate::edittown::occupants(&world.data, Some(&*town.0), story, &dormant.0, active);
    match occupants.get(&lot)? {
        crate::edittown::Occupant::Town(id) => town.0.households.iter().find(|h| h.id == *id),
        _ => None,
    }
}

/// Whether one of a family is home now: grown-ups with a job are out at it on weekdays, nine to
/// five; children and teens at school, eight to three.
pub fn home_now(p: &s3formats::premade::PremadeSim, clock: &GameClock) -> bool {
    if !clock.is_workday() {
        return true;
    }
    let h = clock.hour_f();
    match crate::premade::age_of(p.age) {
        Age::Child | Age::Teen => !(8.0..15.0).contains(&h),
        Age::YoungAdult | Age::Adult if p.career.is_some() => !(9.0..17.0).contains(&h),
        _ => true,
    }
}

/// The town's relationship state as one of ours: the status, and whether romantic.
fn status_of(state: &str) -> (RelStatus, bool) {
    match state {
        "Spouse" => (RelStatus::Married, true),
        "Fiancee" => (RelStatus::Engaged, true),
        "Partner" => (RelStatus::Partner, true),
        "RomanticInterest" => (RelStatus::None, true),
        "Ex" | "ExSpouse" => (RelStatus::Ex, false),
        _ => (RelStatus::None, false),
    }
}

/// A home lot opened for a visit: those of its family who are home come in from wherever they
/// were (the ones already about town), or are made (the rest), and go about their own lives.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn residents_home(
    mut commands: Commands,
    clock: Res<GameClock>,
    visited: Option<Res<VisitedLot>>,
    world: Res<crate::loading::CurrentWorld>,
    (town, story, dormant, household): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>, Option<Res<crate::interact::Household>>),
    (data, cas, sim_assets): (Res<crate::baked::Baked>, Option<Res<crate::simbody::CasData>>, Res<crate::sim::SimAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut skin_mats, mut bindposes, mut textures): (
        ResMut<Assets<crate::simbody::SimSkinMaterial>>,
        ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
        ResMut<crate::simbody::SimTextures>,
    ),
    mut sims: Query<(Entity, &Sim, &mut Transform, &mut crate::nav::Floor, &mut Relationships, Has<HouseholdMember>)>,
    mut notes: ResMut<Notifications>,
    mut done: Local<Option<usize>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
) {
    let Some(v) = visited else {
        *done = None;
        return;
    };
    if *done == Some(v.lot) {
        return;
    }
    // (Once the lot's walk grid is ready: they're set down where it's free to stand.)
    if !v.grid.ready() {
        return;
    }
    *done = Some(v.lot);
    let Some(l) = world.data.lots.get(v.lot) else { return };
    if !l.is_residential() {
        return;
    }
    let Some(family) = family_at(&world, town.as_deref(), &story, &dormant, household.as_ref().map(|h| h.lot_index), v.lot) else { return };
    let home: Vec<&s3formats::premade::PremadeSim> = family.members.iter().filter(|p| home_now(p, &clock)).collect();
    name_home(v.lot, &family.name);
    let name = crate::visit::place_name(&world.data, v.lot);
    if home.is_empty() {
        notes.push(format!("Nobody from the {} household is home right now.", family.name));
        return;
    }
    let center = crate::home::lot_center(l);
    let mut rng = rand::rng();
    let mut placed: Vec<(Entity, u64)> = Vec::new();
    for (k, p) in home.iter().enumerate() {
        // (Somewhere free inside, about the middle of the house.)
        let mut at = center.xz();
        for _ in 0..40 {
            let c = center.xz() + Vec2::new(rng.random_range(-6.0..6.0), rng.random_range(-6.0..6.0));
            if crate::puddles::free_at(Some(&v.grid), None, 1, c) {
                at = c;
                break;
            }
        }
        let house = building.as_deref().and_then(|b| b.away.as_deref());
        let pos = Vec3::new(at.x, crate::building::walk_height(&world.data, house, Vec3::new(at.x, 0.0, at.y)), at.y);
        let existing = sims.iter().find(|s| s.1.id == p.id).map(|s| (s.0, s.5));
        let e = match existing {
            Some((_, true)) => continue,
            Some((e, false)) => {
                if let Ok((_, _, mut tf, mut floor, ..)) = sims.get_mut(e) {
                    tf.translation = pos;
                    floor.0 = 1;
                }
                commands.entity(e).remove::<(crate::town::Townie, crate::nav::PathFollow, OffLot, Visitor, crate::visit::LotGuest, GoingHome)>();
                e
            }
            None => {
                let sim = crate::premade::to_sim(p);
                let model = cas.as_ref().and_then(|cas| {
                    let outfit = crate::simbody::pick_outfit(cas, &sim, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look));
                    crate::simbody::build_sim_model(&data.0, cas, &sim, &outfit, crate::simbody::tone_of(&sim))
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
                crate::sim::spawn_sim_full(&mut commands, &mut sctx, sim, pos, model)
            }
        };
        commands
            .entity(e)
            .insert((Resident(v.lot), OnLot(v.lot), Visibility::Inherited, DespawnOnExit(crate::AppState::InGame)))
            .insert_if_new((ActionQueue::default(), AutonomyTimer(1.0 + k as f32), Skills::default()));
        placed.push((e, p.id));
    }
    // (Family and friends among themselves, as the town has them.)
    if let Some(t) = town.as_deref() {
        for &(a, ia) in &placed {
            for &(b, ib) in &placed {
                if a == b {
                    continue;
                }
                let Some(r) = t.relationship(ia, ib) else { continue };
                let (status, romantic) = status_of(&r.state);
                if let Ok((.., mut rels, _)) = sims.get_mut(a)
                    && !rels.0.contains_key(&b)
                {
                    let x = rels.entry(b);
                    (x.friendship, x.status, x.romantic) = (r.liking, status, romantic);
                }
            }
        }
    }
    info!("homes: {} of the {} household at home at {name}", placed.len(), family.name);
}

/// The household's Sims at someone's home go to the front door and ring, once (each time they
/// come), when there's someone in to answer.
#[allow(clippy::type_complexity)]
fn ring_the_bell(
    mut commands: Commands,
    visited: Option<Res<VisitedLot>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    world: Res<crate::loading::CurrentWorld>,
    mut visitors: Query<(Entity, &OnLot, &mut ActionQueue), With<HouseholdMember>>,
    residents: Query<&Resident>,
    mut rung: Local<std::collections::HashSet<(Entity, usize)>>,
) {
    let Some(v) = visited else {
        rung.clear();
        return;
    };
    // (Gone from the lot: ringing again when they come back.)
    rung.retain(|(e, lot)| visitors.get(*e).is_ok_and(|q| q.1.0 == *lot));
    if !world.data.lots.get(v.lot).is_some_and(|l| l.is_residential()) || !residents.iter().any(|r| r.0 == v.lot) {
        return;
    }
    let Some((door, door_e)) = building.as_ref().and_then(|b| b.away.as_ref()).and_then(|b| b.front_door(v.exit)) else { return };
    let back = building.as_ref().and_then(|b| b.away.as_ref()).and_then(|b| b.front_door_out(v.exit, crate::doorbell::STAND_BACK));
    for (e, on, mut queue) in &mut visitors {
        if on.0 == v.lot && rung.insert((e, v.lot)) {
            crate::doorbell::come_to_door(&mut commands, e, &mut queue, door, back, door_e.unwrap_or(e));
        }
    }
}

/// The families' homes, by lot, as they're named in messages ("the Goth household").
static HOMES: std::sync::RwLock<Vec<(usize, String)>> = std::sync::RwLock::new(Vec::new());

/// What a family's home is called in messages, if the lot's one.
pub fn home_name(lot: usize) -> Option<String> {
    HOMES.read().ok()?.iter().find(|h| h.0 == lot).map(|h| format!("the {} household", h.1))
}

/// Notes a family's home (as the map offers a visit there).
pub fn name_home(lot: usize, family: &str) {
    if let Ok(mut h) = HOMES.write()
        && !h.iter().any(|x| x.0 == lot && x.1 == family)
    {
        h.retain(|x| x.0 != lot);
        h.push((lot, family.to_string()));
    }
}

/// Late at night the hosts ask their visitors to go home (the game's "Ask to Leave"); asked,
/// the household's Sims drive home.
#[allow(clippy::type_complexity)]
fn asked_to_leave(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut last: Local<f64>,
    visited: Option<Res<VisitedLot>>,
    mut hosts: Query<(&Resident, &mut ActionQueue, &Sim), Without<HouseholdMember>>,
    guests: Query<(Entity, &OnLot), (With<HouseholdMember>, Without<crate::doorbell::AtTheDoor>)>,
    going: Query<Entity, (With<HouseholdMember>, With<OnLot>, Added<GoingHome>)>,
) {
    // (Asked to leave: home they go, from where they are.)
    for e in &going {
        commands.entity(e).remove::<GoingHome>();
        commands.entity(e).queue(|mut w: EntityWorldMut| {
            if let Some(mut q) = w.get_mut::<ActionQueue>() {
                q.0.clear();
                q.0.push_back(Action::new("Go Home", ActionKind::GoHomeFromLot, true));
            }
        });
    }
    let Some(v) = visited else { return };
    if clock.minutes - *last < ASK_EVERY {
        return;
    }
    *last = clock.minutes;
    let h = (clock.minutes / 60.0) % 24.0;
    if !(h >= BEDTIME as f64 || h < 5.0) {
        return;
    }
    // (The game's "Ask to Leave": ours, "Say Goodbye".)
    let Some(ask) = crate::social::all_socials().find(|(_, s)| s.effect == crate::social::SocialEffect::AskToLeave).map(|(i, _)| i) else { return };
    let visitor = guests.iter().find(|g| g.1.0 == v.lot).map(|g| g.0);
    let Some(visitor) = visitor else { return };
    if let Some((_, mut q, _)) = hosts.iter_mut().find(|h| h.0.0 == v.lot && h.2.age.is_grown() && h.2.age != Age::Child && h.1.0.iter().all(|a| a.autonomous)) {
        q.0.clear();
        q.0.push_back(Action::new("Ask to Leave", ActionKind::Social { target: visitor, social: ask }, true));
    }
}

/// The visit over (the lot closed): the family carry on off stage, kept as acquaintances.
fn residents_away(mut commands: Commands, visited: Option<Res<VisitedLot>>, residents: Query<(Entity, &Resident)>) {
    for (e, r) in &residents {
        if visited.as_ref().is_some_and(|v| v.lot == r.0) {
            continue;
        }
        commands.entity(e).remove::<(Resident, OnLot, crate::nav::PathFollow, crate::doorbell::AtTheDoor)>().insert((OffLot, Visibility::Hidden));
        commands.entity(e).queue(|mut w: EntityWorldMut| {
            if let Some(mut q) = w.get_mut::<ActionQueue>() {
                q.0.clear();
            }
        });
    }
}

/// `HOME_VISIT=1` (a test): the selected Sim drives to the nearest home with someone of its
/// family in, and the visit is logged as it goes (who's home, the door, the greeting).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn home_visit_test(
    time: Res<Time>,
    clock: Res<GameClock>,
    world: Res<crate::loading::CurrentWorld>,
    (town, story, dormant, household): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>, Option<Res<crate::interact::Household>>),
    mut sel: Query<(&mut ActionQueue, &Transform, Option<&OnLot>, Has<crate::doorbell::AtTheDoor>), With<crate::sim::Selected>>,
    residents: Query<(&Sim, &ActionQueue), (With<Resident>, Without<crate::sim::Selected>)>,
    mut state: Local<(Option<f32>, bool, f32)>,
) {
    let now = time.elapsed_secs();
    let t0 = *state.0.get_or_insert(now);
    let Ok((mut q, tf, on, at_door)) = sel.single_mut() else { return };
    if !state.1 {
        if now - t0 < 5.0 {
            return;
        }
        state.1 = true;
        let active = household.as_ref().map(|h| h.lot_index);
        let best = (0..world.data.lots.len())
            .filter_map(|i| {
                let f = family_at(&world, town.as_deref(), &story, &dormant, active, i)?;
                f.members.iter().any(|p| home_now(p, &clock) && !crate::premade::age_of(p.age).is_little()).then_some((i, f.name.clone()))
            })
            .min_by(|a, b| {
                let d = |i: usize| crate::home::lot_center(&world.data.lots[i]).distance(tf.translation);
                d(a.0).total_cmp(&d(b.0))
            });
        if let Some((lot, name)) = best {
            info!("home visit test: off to the {name} household at lot {lot}");
            name_home(lot, &name);
            q.0.clear();
            q.push_player(Action::new("Visit Lot", ActionKind::GoToLot { lot }, false));
        }
        return;
    }
    if now - state.2 < 4.0 {
        return;
    }
    state.2 = now;
    let doing: Vec<String> = residents.iter().map(|(s, rq)| format!("{}: {:?}", s.first, rq.0.front().map(|a| a.label.as_str()))).collect();
    info!("home visit test: on lot {:?}, at the door {at_door}, doing {:?}; residents {doing:?}", on.map(|o| o.0), q.0.front().map(|a| a.label.as_str()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_family_is_home_out_of_work_and_school_hours() {
        let mut p = s3formats::premade::PremadeSim { age: s3formats::premade::AGE_ADULT, career: Some(("Business".into(), 3)), ..Default::default() };
        // (Monday at noon: at work; at seven in the evening, home; at the weekend, home.)
        let at = |day: f64, hour: f64| {
            let mut c = GameClock::default();
            c.minutes = day * 1440.0 + hour * 60.0;
            c
        };
        assert!(!home_now(&p, &at(0.0, 12.0)));
        assert!(home_now(&p, &at(0.0, 19.0)));
        assert!(home_now(&p, &at(5.0, 12.0)));
        p.career = None;
        assert!(home_now(&p, &at(0.0, 12.0)));
        p.age = s3formats::premade::AGE_CHILD;
        assert!(!home_now(&p, &at(1.0, 10.0)));
        assert!(home_now(&p, &at(1.0, 16.0)));
        assert_eq!(status_of("Spouse"), (RelStatus::Married, true));
    }
}
