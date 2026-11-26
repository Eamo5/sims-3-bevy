//! Playing on the playground, as Generations has it (`PlayGroundObject.PlayOnPlayGround`): a
//! child at any of a lot's playground things (swings, slides, jungle gyms, spring riders,
//! seesaws, sandboxes) may "Play on Playground", offered where there's another of them on the lot
//! to go on to (`HaveEnoughPlaygroundObjects`). They play on the first as its own play would
//! have them (sliding, riding, swinging, sitting on the seesaw), and once they've spent the
//! game's time on it (`GetTimeToSpendOnThisObject`, the XML's: a spring rider a quarter hour,
//! `kTimeRangeForRiding`'s first; the lookout tower a quarter hour, the slide twenty minutes,
//! `TimeSpentBeforeSwitchingToAnotherObject`; the swings, seesaw and sandbox twenty,
//! `kTimeToSpendOnThisObject`) they go on to another (`TryMovingToAnotherObject`): first one with
//! someone on it waiting for a playmate (a seesaw with one sitting alone,
//! `PlaygroundObjectNeedsPlayers`), then something of another kind, then another of the same,
//! each picked at random among those free; with nothing else free they stay where they are. A go
//! over before its time's up (down the slide), they're drawn to one waiting for a playmate or have
//! another go. They stop once they've had all the fun they can (`IsMax(Fun)`), or something's
//! asked of them next, or they're told to. Children short of fun go and play of their own accord
//! now and then; the Childish all the more (`TraitChildish`, `TraitImaginaryFriend` its 200).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase, UsedBy};
use crate::seesaw::{Boards, Part};
use crate::sim::{Age, Sim};

pub struct PlaygroundPlugin;

impl Plugin for PlaygroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, begin.before(crate::seesaw::hold).before(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, (rotate, autonomy, playground_test).chain().after(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's words (`PlayGroundObject/PlayOnPlayGround`).
pub const PLAY_ON_PLAYGROUND: &str = "Play on Playground";
/// How full fun is when they've had all they can (the game's `IsMax`).
const FUN_FULL: f32 = 99.0;
/// How many goes in a row may come to nothing (a way there blocked) before they give up.
const MAX_TRIES: u8 = 3;

/// The playground things (Generations' `PlayGroundObject`s).
pub fn is_playground(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::SwingSet | ObjectKind::JungleGym | ObjectKind::Slide | ObjectKind::SpringRider | ObjectKind::Seesaw | ObjectKind::Sandbox | ObjectKind::Treehouse)
}

/// Who plays on the playground: children (the tuning's `AgeSpeciesAvail` C).
pub fn can_play(age: Age) -> bool {
    age == Age::Child
}

/// How long a child spends on a thing before going on to another (game minutes, the XML's).
pub fn minutes_on(kind: ObjectKind) -> f64 {
    match kind {
        ObjectKind::SpringRider | ObjectKind::JungleGym => 15.0,
        // (A treehouse's `kPlayOnPlaygroundDuration`.)
        ObjectKind::Treehouse => 40.0,
        _ => 20.0,
    }
}

/// A playground thing as one child sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub e: Entity,
    pub kind: ObjectKind,
    /// Free for them: nobody else on it or on their way to it (a seesaw with one waiting is free
    /// to join).
    pub free: bool,
    /// Someone on it waiting for a playmate (a seesaw with one sitting alone).
    pub needs_players: bool,
}

/// How a thing stands for a child: who's using it (`UsedBy`), the seesaws, who's up the
/// treehouses, and how many others have it in their queues.
pub fn candidate(me: Entity, e: Entity, kind: ObjectKind, used: Option<Entity>, boards: &Boards, houses: &crate::treehouse::Houses, others: usize) -> Candidate {
    // (A treehouse: room for them, four at most counting those on their way.)
    if kind == ObjectKind::Treehouse {
        let free = houses.room(e, me, false) && houses.get(e).inside.iter().filter(|x| x.0 != me).count() + others < crate::treehouse::OCCUPANTS;
        return Candidate { e, kind, free, needs_players: false };
    }
    if kind == ObjectKind::Seesaw {
        let b = boards.0.get(&e);
        let empty = b.is_none_or(|b| b.riders.is_empty() && b.joining.is_none() && b.part == Part::Empty);
        // (One sat waiting, nobody else on their way to join.)
        let waiting = b.is_some_and(|b| b.joinable() && !b.has(me)) && others <= 1;
        return Candidate { e, kind, free: (empty && others == 0) || waiting, needs_players: waiting };
    }
    Candidate { e, kind, free: used.is_none_or(|u| u == me) && others == 0, needs_players: false }
}

/// Whether "Play on Playground" is offered at a thing: it's free, and there's another free on
/// the lot to go on to.
pub fn offered(target: Entity, cands: &[Candidate]) -> bool {
    cands.iter().any(|c| c.e == target && c.free) && cands.iter().any(|c| c.e != target && c.free)
}

/// Where to go on to from a thing (`TryMovingToAnotherObject`): one needing players, then
/// (their time up) one of another kind, then one of the same kind, at random in each. None: stay.
pub fn pick(current: Entity, current_kind: ObjectKind, cands: &[Candidate], time_up: bool, rng: &mut impl Rng) -> Option<Entity> {
    let free: Vec<&Candidate> = cands.iter().filter(|c| c.e != current && c.free).collect();
    let needs: Vec<Entity> = free.iter().filter(|c| c.kind != current_kind && c.needs_players).map(|c| c.e).collect();
    let others: Vec<Entity> = free.iter().filter(|c| c.kind != current_kind && !c.needs_players && time_up).map(|c| c.e).collect();
    let same: Vec<Entity> = free.iter().filter(|c| c.kind == current_kind && time_up).map(|c| c.e).collect();
    [needs, others, same].into_iter().find_map(|l| l.choose(rng).copied())
}

/// The play a child's set to at a thing: its own (sitting on the seesaw, or joining the one
/// waiting there), with its words.
pub fn action_for(e: Entity, kind: ObjectKind, tf: &Transform, here: Vec2, sim: &Sim, boards: &Boards, rng: &mut impl Rng) -> (String, ActionKind) {
    if kind == ObjectKind::Seesaw {
        let child = sim.age == Age::Child;
        if let Some(b) = boards.0.get(&e).filter(|b| b.joinable()) {
            return (crate::seesaw::JOIN.to_string(), crate::seesaw::join(e, tf, b.riders[0].north, child));
        }
        let daredevil = sim.traits.contains(&crate::life::Trait::Daredevil);
        let (spot, extreme) = crate::seesaw::pick_spot(tf, here, child, boards.north_up(e), daredevil, rng.random_range(0.0..1.0));
        let label = if daredevil { crate::seesaw::PLAY_ON_EXTREME } else { crate::seesaw::PLAY_ON };
        return (label.to_string(), crate::seesaw::sit(e, tf, spot, extreme, None));
    }
    // (Up the treehouse to play in it, the game's `PlayIn` as playground play.)
    if kind == ObjectKind::Treehouse {
        return (PLAY_ON_PLAYGROUND.to_string(), crate::treehouse::act(e, tf, crate::treehouse::Act::Play));
    }
    let name = crate::interact::interactions_for(kind).first().map_or(PLAY_ON_PLAYGROUND, |d| d.name);
    (name.to_string(), ActionKind::Object { target: e, def: 0 })
}

/// A child playing on the playground: the thing they're at and since when (game minutes), the
/// one they're leaving for another (its play told to stop: not the player's doing), whether
/// they're to stop once done with it, how many goes in a row have come to nothing, and whether
/// it was of their own accord.
#[derive(Component, Debug)]
pub struct OnPlayground {
    pub object: Entity,
    pub since: f64,
    leaving: Option<Entity>,
    pub stopped: bool,
    tries: u8,
    autonomous: bool,
}

/// The playground thing an action's at.
fn target_of(a: &Action, kind_of: &impl Fn(Entity) -> Option<ObjectKind>) -> Option<Entity> {
    match a.kind {
        ActionKind::Object { target, .. } if kind_of(target).is_some_and(is_playground) => Some(target),
        ActionKind::Seesaw { seesaw, .. } => Some(seesaw),
        ActionKind::PlayOnPlayground { target } => Some(target),
        // (Up, in and down a treehouse, all its play.)
        ActionKind::Treehouse { house, .. } => Some(house),
        _ => None,
    }
}

/// Who has each thing in their queue.
fn claims<'a>(queues: impl Iterator<Item = (Entity, &'a ActionQueue)>, kind_of: &impl Fn(Entity) -> Option<ObjectKind>) -> HashMap<Entity, Vec<Entity>> {
    let mut out: HashMap<Entity, Vec<Entity>> = HashMap::new();
    for (e, q) in queues {
        for a in q.0.iter().filter(|a| !a.cancel) {
            if let Some(t) = target_of(a, kind_of) {
                out.entry(t).or_default().push(e);
            }
        }
    }
    out
}

type Objects<'w, 's> = Query<'w, 's, (Entity, &'static GameObject, &'static Transform, Option<&'static crate::visit::LotObject>, Option<&'static UsedBy>), Without<Sim>>;

/// The playground things on a lot (none: the household's own) as a child sees them.
fn on_lot(me: Entity, age: Age, lot: Option<usize>, objects: &Objects, (boards, houses): (&Boards, &crate::treehouse::Houses), claimed: &HashMap<Entity, Vec<Entity>>) -> Vec<Candidate> {
    objects
        .iter()
        .filter(|(_, o, _, l, _)| is_playground(o.kind) && o.kind.usable_by(age) && l.map(|l| l.0) == lot)
        .map(|(e, o, _, _, used)| {
            let others = claimed.get(&e).map_or(0, |v| v.iter().filter(|c| **c != me).count());
            candidate(me, e, o.kind, used.and_then(|u| u.0), boards, houses, others)
        })
        .collect()
}

/// Had their fill, or a need pressing: done playing.
fn done(m: &crate::sim::Motives) -> bool {
    m.0[crate::sim::FUN] >= FUN_FULL || [crate::sim::HUNGER, crate::sim::BLADDER, crate::sim::ENERGY].iter().any(|i| m.0[*i] < -40.0)
}

/// "Play on Playground" begun: made the first thing's own play; and a stop asked of a child
/// playing (any of their playground goes cancelled, not by the switching): done after this one.
#[allow(clippy::type_complexity)]
pub(crate) fn begin(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    (boards, houses): (Res<Boards>, Res<crate::treehouse::Houses>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &Transform, Option<&mut OnPlayground>, Option<&crate::visit::OnLot>)>,
    objects: Objects,
) {
    let kind_of = |e: Entity| objects.get(e).ok().map(|o| o.1.kind);
    let claimed = claims(sims.iter().map(|s| (s.0, &*s.2)), &kind_of);
    let mut rng = rand::rng();
    for (e, sim, mut q, tf, pg, lot) in &mut sims {
        if let Some(mut pg) = pg
            && !pg.stopped
            && q.0.iter().any(|a| a.cancel && target_of(a, &kind_of).is_some_and(|t| pg.leaving != Some(t)))
        {
            pg.stopped = true;
            info!("playground: {} told to stop playing", sim.first);
        }
        let Some(a) = q.0.front_mut() else { continue };
        let ActionKind::PlayOnPlayground { target } = a.kind else { continue };
        if a.cancel || !can_play(sim.age) {
            q.0.pop_front();
            continue;
        }
        let lot = lot.map(|l| l.0);
        let cands = on_lot(e, sim.age, lot, &objects, (&boards, &houses), &claimed);
        let kind = kind_of(target).unwrap_or(ObjectKind::Other);
        // (Taken meanwhile: somewhere else free.)
        let first = cands.iter().find(|c| c.e == target && c.free).map(|c| c.e).or_else(|| pick(target, kind, &cands, true, &mut rng));
        let Some((o, okind, otf)) = first.and_then(|o| objects.get(o).ok()).map(|o| (o.0, o.1.kind, *o.2)) else {
            info!("playground: nothing free for {} to play on", sim.first);
            q.0.pop_front();
            continue;
        };
        let (label, kind) = action_for(o, okind, &otf, tf.translation.xz(), sim, &boards, &mut rng);
        info!("playground: {} plays on the playground, from {label}", sim.first);
        let autonomous = a.autonomous;
        a.kind = kind;
        a.label = label;
        a.phase = Phase::Start;
        commands.entity(e).insert(OnPlayground { object: o, since: clock.minutes, leaving: None, stopped: false, tries: 0, autonomous });
    }
}

/// Children playing on the playground going from one thing to the next as their time on each is
/// up, drawn to one waiting for a playmate, having another go when a go's over; done when
/// they've had their fill, something else is asked of them, or they're told to stop.
#[allow(clippy::type_complexity)]
fn rotate(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    (boards, houses): (Res<Boards>, Res<crate::treehouse::Houses>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &crate::sim::Motives, &Transform, Option<&mut OnPlayground>, Option<&crate::visit::OnLot>)>,
    objects: Objects,
) {
    let kind_of = |e: Entity| objects.get(e).ok().map(|o| o.1.kind);
    let claimed = claims(sims.iter().map(|s| (s.0, &*s.2)), &kind_of);
    let mut rng = rand::rng();
    let now = clock.minutes;
    for (e, sim, mut q, m, tf, pg, lot) in &mut sims {
        let Some(mut pg) = pg else { continue };
        let lot = lot.map(|l| l.0);
        let front = q.0.front().and_then(|a| target_of(a, &kind_of).map(|t| (t, matches!(a.phase, Phase::Running(_)))));
        let set_to = |o: Entity, rng: &mut rand::rngs::ThreadRng| objects.get(o).ok().map(|x| action_for(o, x.1.kind, x.2, tf.translation.xz(), sim, &boards, rng));
        match front {
            Some((o, running)) => {
                if o != pg.object {
                    pg.object = o;
                    pg.since = now;
                    pg.leaving = None;
                }
                if running {
                    pg.tries = 0;
                }
                if pg.stopped || pg.leaving == Some(o) || !running {
                    continue;
                }
                // (Something else asked of them next: this is their last.)
                if q.0.iter().skip(1).any(|a| !a.cancel && target_of(a, &kind_of).is_none()) {
                    pg.stopped = true;
                    continue;
                }
                let kind = kind_of(o).unwrap_or(ObjectKind::Other);
                if now - pg.since < minutes_on(kind) {
                    continue;
                }
                if done(m) {
                    info!("playground: {} has had their fill of the playground", sim.first);
                    if let Some(a) = q.0.front_mut() {
                        a.cancel = true;
                    }
                    pg.leaving = Some(o);
                    pg.stopped = true;
                    continue;
                }
                let cands = on_lot(e, sim.age, lot, &objects, (&boards, &houses), &claimed);
                match pick(o, kind, &cands, true, &mut rng).and_then(|n| set_to(n, &mut rng).map(|x| (n, x))) {
                    Some((n, (label, next))) => {
                        info!("playground: {} goes from {:?} on to {label} ({:?})", sim.first, kind, kind_of(n));
                        if let Some(a) = q.0.front_mut() {
                            a.cancel = true;
                        }
                        pg.leaving = Some(o);
                        q.0.push_back(Action::new(label, next, pg.autonomous));
                    }
                    // (Nothing else free: on where they are.)
                    None => pg.since = now,
                }
            }
            None => {
                if pg.stopped || !q.0.is_empty() || pg.tries >= MAX_TRIES || done(m) {
                    let why = if pg.stopped {
                        "told to, or something else to do"
                    } else if !q.0.is_empty() {
                        debug!("playground: {}'s queue: {:?}", sim.first, q.0.iter().map(|a| (&a.label, a.cancel, a.autonomous)).collect::<Vec<_>>());
                        "something else to do"
                    } else if pg.tries >= MAX_TRIES {
                        "can't get to anything"
                    } else {
                        "had their fill"
                    };
                    info!("playground: {} stops playing on the playground ({why})", sim.first);
                    commands.entity(e).remove::<OnPlayground>();
                    continue;
                }
                // A go over: one waiting for a playmate, or (their time up) something else; else
                // another go on the same.
                pg.tries += 1;
                let kind = kind_of(pg.object).unwrap_or(ObjectKind::Other);
                let time_up = now - pg.since >= minutes_on(kind);
                let cands = on_lot(e, sim.age, lot, &objects, (&boards, &houses), &claimed);
                let again = cands.iter().any(|c| c.e == pg.object && c.free).then_some(pg.object);
                let next = pick(pg.object, kind, &cands, time_up, &mut rng).or(again).or_else(|| pick(pg.object, kind, &cands, true, &mut rng));
                match next.and_then(|n| set_to(n, &mut rng)) {
                    Some((label, kind)) => q.0.push_back(Action::new(label, kind, pg.autonomous)),
                    None => {
                        info!("playground: nothing free for {} to go on to", sim.first);
                        commands.entity(e).remove::<OnPlayground>();
                    }
                }
            }
        }
    }
}

/// Children of the household short of fun go and play on the playground of their own accord now
/// and then, where there are things enough free on the lot; the Childish keener.
#[allow(clippy::type_complexity)]
fn autonomy(
    clock: Res<crate::clock::GameClock>,
    (boards, houses): (Res<Boards>, Res<crate::treehouse::Houses>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &crate::sim::Motives, &Transform, Option<&crate::visit::OnLot>, (Has<crate::sim::HouseholdMember>, Has<OnPlayground>))>,
    objects: Objects,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 30.0;
    if std::env::var_os("PLAYGROUND_TEST").is_some() {
        return;
    }
    let kind_of = |e: Entity| objects.get(e).ok().map(|o| o.1.kind);
    let claimed = claims(sims.iter().map(|s| (s.0, &*s.2)), &kind_of);
    let mut rng = rand::rng();
    for (e, sim, mut q, m, tf, lot, (member, playing)) in &mut sims {
        let childish = sim.traits.iter().any(|t| matches!(t, crate::life::Trait::Childish));
        let bored = m.0[crate::sim::FUN] < if childish { 70.0 } else { 40.0 };
        if !member || playing || !can_play(sim.age) || !q.0.is_empty() || !bored || !rng.random_bool(if childish { 0.6 } else { 0.35 }) {
            continue;
        }
        let cands = on_lot(e, sim.age, lot.map(|l| l.0), &objects, (&boards, &houses), &claimed);
        let here = tf.translation;
        let near = cands
            .iter()
            .filter(|c| offered(c.e, &cands))
            .filter_map(|c| objects.get(c.e).ok().map(|o| (c.e, o.2.translation.distance(here))))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((target, _)) = near {
            q.0.push_back(Action::new(PLAY_ON_PLAYGROUND, ActionKind::PlayOnPlayground { target }, true));
            info!("playground: {} goes to play on the playground of their own accord", sim.first);
        }
    }
}

/// `PLAYGROUND_TEST=1` (tests): ten seconds in, playground things put down in a row near the
/// selected child (or `PLAYGROUND_WHO=<first name>`): `PLAYGROUND_OBJECTS=<instance>,...` (a slide,
/// a spring rider and a seesaw by default), and they play on the playground from the first; with
/// `PLAYGROUND_WITH=<first name>`, another child too, from the last. `PLAYGROUND_PIE=1`: the
/// first thing's menu opened instead.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn playground_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Has<crate::sim::Selected>, &mut crate::sim::Motives), With<crate::sim::HouseholdMember>>,
    objects: Query<(Entity, &GameObject, &Transform), (Without<Sim>, With<crate::save::Bought>)>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32, Option<Vec3>)>,
) {
    if std::env::var_os("PLAYGROUND_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let who = env("PLAYGROUND_WHO");
    let is_kid = |s: &Sim, sel: bool| who.as_ref().map_or(sel && can_play(s.age), |w| s.first.eq_ignore_ascii_case(w));
    let names: Vec<String> = env("PLAYGROUND_OBJECTS").unwrap_or_else(|| "jungleGymSlideGen,springRiderHorse,seesawClassic".into()).split(',').map(|s| s.trim().to_string()).collect();
    if let (Ok(mut c), Some(centre)) = (cams.single_mut(), stage.2) {
        let f = |k: &str, d: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(centre + Vec3::Y * 0.5);
        c.distance = f("PLAYGROUND_DIST", 14.0);
        c.pitch = f("PLAYGROUND_PITCH", 0.55);
        c.yaw = f("PLAYGROUND_YAW", 0.4);
    }
    match stage.0 {
        0 => {
            stage.0 = 1;
            stage.1 = now;
            let Some((_, _, tf, ..)) = sims.iter().find(|s| is_kid(s.1, s.4)) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let n = names.len() as f32;
            let width = n * 5.0;
            let clear = |c: Vec2| (0..=(width as i32)).all(|i| (-4..=4).all(|j| free(c + Vec2::new(i as f32 - width / 2.0, j as f32 * 0.8))));
            let here = tf.translation.xz();
            let centre = (4..30).flat_map(|r| (0..16).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 16.0) * r as f32)).find(|c| clear(*c)).unwrap_or(here + Vec2::new(0.0, 8.0));
            stage.2 = Some(Vec3::new(centre.x, tf.translation.y, centre.y));
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            for (i, name) in names.iter().enumerate() {
                let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(name)).map(|c| c.objd) else {
                    warn!("playground test: no {name}");
                    continue;
                };
                let x = (i as f32 - (n - 1.0) / 2.0) * 5.0;
                let p = Vec3::new(centre.x + x, tf.translation.y, centre.y);
                if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0) {
                    commands.entity(o.entity).insert(crate::save::Bought);
                    info!("playground test: {name} at {p:.1}");
                }
            }
        }
        1 if now - stage.1 > 3.0 => {
            let Some(centre) = stage.2 else { return };
            let mut put: Vec<(Entity, &GameObject, &Transform)> = objects.iter().filter(|o| is_playground(o.1.kind) && o.2.translation.distance(centre) < 20.0).collect();
            if put.len() < names.len() {
                return;
            }
            put.sort_by(|a, b| a.2.translation.x.total_cmp(&b.2.translation.x));
            stage.0 = 2;
            stage.1 = now;
            if env("PLAYGROUND_PIE").is_some() {
                commands.insert_resource(crate::hud::PieFor(put[0].0));
                info!("playground test: its menu");
                return;
            }
            let (first, last) = (put[0].0, put[put.len() - 1].0);
            let with = env("PLAYGROUND_WITH");
            // (PLAYGROUND_FUN=<n>: their fun so low to begin with, to keep them at it.)
            let fun = env("PLAYGROUND_FUN").and_then(|v| v.parse::<f32>().ok());
            for (_, sim, _, mut q, sel, mut m) in &mut sims {
                let target = if is_kid(sim, sel) {
                    first
                } else if with.as_ref().is_some_and(|w| sim.first.eq_ignore_ascii_case(w)) {
                    last
                } else {
                    continue;
                };
                if let Some(f) = fun {
                    m.0[crate::sim::FUN] = f;
                }
                q.push_player(Action::new(PLAY_ON_PLAYGROUND, ActionKind::PlayOnPlayground { target }, false));
                info!("playground test: {} plays on the playground", sim.first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(n: u32) -> Entity {
        Entity::from_raw_u32(n).unwrap()
    }

    fn c(n: u32, kind: ObjectKind, free: bool, needs: bool) -> Candidate {
        Candidate { e: e(n), kind, free, needs_players: needs }
    }

    #[test]
    fn its_offered_with_another_thing_free_on_the_lot() {
        let lot = [c(1, ObjectKind::Slide, true, false), c(2, ObjectKind::SwingSet, true, false)];
        assert!(offered(e(1), &lot));
        assert!(!offered(e(1), &lot[..1]), "the slide alone: nothing to go on to");
        let taken = [c(1, ObjectKind::Slide, true, false), c(2, ObjectKind::SwingSet, false, false)];
        assert!(!offered(e(1), &taken));
        assert!(!offered(e(2), &[c(1, ObjectKind::Slide, true, false), c(2, ObjectKind::SwingSet, false, false)]), "not at a thing someone's on");
    }

    #[test]
    fn they_go_on_as_the_game_has_them() {
        let mut rng = rand::rng();
        let lot = [
            c(1, ObjectKind::Slide, true, false),
            c(2, ObjectKind::Slide, true, false),
            c(3, ObjectKind::SpringRider, true, false),
            c(4, ObjectKind::Seesaw, true, true),
            c(5, ObjectKind::SwingSet, false, false),
        ];
        // (A seesaw with one waiting first, time up or not.)
        assert_eq!(pick(e(1), ObjectKind::Slide, &lot, true, &mut rng), Some(e(4)));
        assert_eq!(pick(e(1), ObjectKind::Slide, &lot, false, &mut rng), Some(e(4)));
        // (Then another kind, never one taken; then the same kind.)
        let no_seesaw = &lot[..3];
        for _ in 0..20 {
            assert_eq!(pick(e(1), ObjectKind::Slide, no_seesaw, true, &mut rng), Some(e(3)));
        }
        assert_eq!(pick(e(1), ObjectKind::Slide, &lot[..2], true, &mut rng), Some(e(2)));
        // (Their time not up, and nobody waiting: they stay.)
        assert_eq!(pick(e(1), ObjectKind::Slide, no_seesaw, false, &mut rng), None);
        assert_eq!(pick(e(1), ObjectKind::Slide, &lot[..1], true, &mut rng), None);
    }

    #[test]
    fn a_seesaw_with_one_waiting_needs_a_playmate() {
        let mut boards = Boards::default();
        let mut houses = crate::treehouse::Houses::default();
        let me = e(10);
        let s = e(1);
        let empty = candidate(me, s, ObjectKind::Seesaw, None, &boards, &houses, 0);
        assert!(empty.free && !empty.needs_players);
        assert!(!candidate(me, s, ObjectKind::Seesaw, None, &boards, &houses, 1).free, "someone on their way to it");
        let mut b = crate::seesaw::Board::default();
        b.riders.push(crate::seesaw::Rider { sim: e(11), north: true, spot: crate::seesaw::Spot::N, daredevil: false, child: true });
        b.part = Part::Wait;
        boards.0.insert(s, b);
        let waiting = candidate(me, s, ObjectKind::Seesaw, None, &boards, &houses, 1);
        assert!(waiting.free && waiting.needs_players, "the one sat on it has it queued");
        assert!(!candidate(me, s, ObjectKind::Seesaw, None, &boards, &houses, 2).free, "another already on their way to join");
        assert!(!candidate(me, e(2), ObjectKind::SwingSet, Some(e(11)), &boards, &houses, 0).free);
        assert!(candidate(me, e(2), ObjectKind::SwingSet, Some(me), &boards, &houses, 0).free);
        assert_eq!(minutes_on(ObjectKind::SpringRider), 15.0);
        assert_eq!(minutes_on(ObjectKind::Seesaw), 20.0);
        // (A treehouse: room for four children, none with a grown-up up there.)
        let t = e(3);
        assert!(candidate(me, t, ObjectKind::Treehouse, None, &boards, &houses, 2).free);
        houses.0.insert(t, crate::treehouse::House { inside: vec![(e(12), false), (e(13), false)], ..Default::default() });
        assert!(!candidate(me, t, ObjectKind::Treehouse, None, &boards, &houses, 2).free, "two in it and two on their way");
        houses.0.insert(t, crate::treehouse::House { inside: vec![(e(12), true)], ..Default::default() });
        assert!(!candidate(me, t, ObjectKind::Treehouse, None, &boards, &houses, 0).free);
        assert_eq!(minutes_on(ObjectKind::Treehouse), 40.0);
    }
}
