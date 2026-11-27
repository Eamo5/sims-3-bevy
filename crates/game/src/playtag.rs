//! Play Tag, as the game's "Play Tag" social and its `PlayTagSituation`: a child (or a Childish
//! teen or older: `TestPlayTag`) tags someone, children and up, and if they're game the one who
//! asked is "it". The other runs off somewhere outdoors on the lot, six to ten metres at a time
//! (no further than forty from where it began: `kMaxDistanceFromOrigin`), laughing as they go
//! (`vo_playtag_runA`, a child's `runC`), and after each run may have to stop and catch their
//! breath (`loopBreathe`: more likely the heavier and less fit they are, at least three times in
//! ten: `kCatchBreathWeightMultiplier`, `kCatchBreathMinumumChance`). A moment after they're off
//! (`kHunterDelay`), whoever's "it" runs after them, to where they ran; caught out of breath and
//! within three metres, they're tagged (`Tagger_accept`), and once they've their breath back
//! they're "it" and the other runs. It's fun (50 an hour) and social (100), and they get on
//! (`kRelationshipSTCDelta`: friendly 75 an hour, liking 2), till one's had all the fun they
//! can, a need's pressing, they're called away or told to stop, or the ways about are blocked
//! too often (`kMaxNumberOfRouteFailures`).

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue};
use crate::life::Trait;
use crate::nav::{Floor, NavGrid, PathFollow, UpperFloors, WalkStyle};
use crate::sim::{Age, Motives, Sim};

pub struct PlayTagPlugin;

impl Plugin for PlayTagPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Games>().add_systems(Update, (play, tag_test).chain().after(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words for it (the social's, and the situation's interaction on the queue).
pub const PLAY_TAG: &str = "Play Tag";

/// Its tuning (`PlayTagSituation`'s): the hunter's wait (game minutes), how far from where it
/// began the running goes, a run's reach, the chance of catching breath (weight times so much,
/// less fitness, at least the least), where along a run they laugh, and how close a tag is.
const HUNTER_DELAY: f64 = 1.0;
const MAX_FROM_ORIGIN: f32 = 40.0;
const RUN: (f32, f32) = (6.0, 10.0);
const BREATH_WEIGHT: f32 = 1.5;
const BREATH_MIN: f32 = 0.3;
const LAUGH_AT: (f32, f32) = (0.3, 0.8);
const TAG_REACH: f32 = 3.0;
const MAX_ROUTE_FAILURES: u8 = 8;
/// The hunter's run stops this short of where the prey ran (`PlanToPointRadius`).
const CHASE_SHORT: f32 = 0.7;
/// What playing gives, an hour (its ITUN's `Fun` 50, `Social` 100; the relationship's
/// `kRelationshipSTCDelta`, `kRelationshipLTRDelta`).
const FUN_PER_HOUR: f32 = 50.0;
const SOCIAL_PER_HOUR: f32 = 100.0;
const STC_PER_HOUR: f32 = 75.0;
const LIKING_PER_HOUR: f32 = 2.0;
/// All the fun they can have (the interaction's `funExit`).
const FUN_FULL: f32 = 99.0;
/// The clips' lengths (game minutes, as clip seconds at normal speed): catching breath, a tag.
const BREATHE_SECS: f64 = 7.0;
const TAG_SECS: f64 = 2.33;

/// Whether `actor` may ask someone of `target` age to play (the social's ages, C to E both, and
/// `TestPlayTag`: a teen or older only if Childish); not of their own accord if they hate the
/// outdoors (its `ATRA`).
pub fn may_play(actor: &Sim, target: Age, autonomous: bool) -> bool {
    let plays = |a: Age| a == Age::Child || a == Age::Teen || a.is_grown();
    plays(actor.age) && plays(target) && (actor.age == Age::Child || actor.traits.contains(&Trait::Childish)) && !(autonomous && actor.traits.contains(&Trait::HatesTheOutdoors))
}

/// Whether they're keener on it (its `ATE`: the outdoorsy and the Childish).
pub fn keen(actor: &Sim) -> bool {
    actor.traits.contains(&Trait::LovesTheOutdoors) || actor.traits.contains(&Trait::Childish)
}

/// The chance of having to catch their breath after a run.
pub fn breath_chance(weight: f32, fitness: f32) -> f32 {
    (weight * BREATH_WEIGHT - fitness).max(BREATH_MIN)
}

/// Their side of a pair's clip: a child's `c`, anyone older's `a`.
fn side(age: Age) -> char {
    if age == Age::Child { 'c' } else { 'a' }
}

/// The social's clips: the tagger's (`a2a_`, `a2c_`, `c2a_`, `c2c_`) and the one tagged's (the
/// grown-ups' or the children's), taken well or not.
pub fn social_clips(actor: Age, target: Age, accepted: bool) -> (crate::anim::ActionClip, crate::anim::ActionClip) {
    let how = if accepted { "accept" } else { "reject" };
    let (x, y) = (side(actor), side(target));
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let tagger = leak(format!("{x}2{y}_playTag_Tagger_{how}_x"));
    let taggee = leak(format!("{y}2{y}_playTag_Taggee_{how}_y"));
    (crate::anim::ActionClip::new(Some(tagger), IDLE), crate::anim::ActionClip::new(Some(taggee), IDLE))
}

/// Stood waiting between runs: the everyday idles (a child's own).
const IDLE: &[&str] = &["a_idle_neutral_loop_"];

/// Catching breath (`loopBreathe`), and a tag (`Tagger_accept`, at the one tagged's height).
fn breathe_clip(age: Age) -> &'static str {
    if age == Age::Child { "c2c_playTag_loopBreathe_y" } else { "a2a_playTag_loopBreathe_y" }
}

fn tag_clip(hunter: Age, prey: Age) -> &'static str {
    match (side(hunter), side(prey)) {
        ('c', 'c') => "c2c_playTag_Tagger_accept_x",
        ('c', _) => "c2a_playTag_Tagger_accept_x",
        (_, 'c') => "a2c_playTag_Tagger_accept_x",
        _ => "a2a_playTag_Tagger_accept_x",
    }
}

/// A player's laugh as they run (the game's `PlayVoxCallback`).
fn laugh(age: Age) -> &'static str {
    if age == Age::Child { "vo_playtag_runC" } else { "vo_playtag_runA" }
}

/// What a player's at.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Leg {
    /// Stood, till then (game minutes).
    Waiting(f64),
    /// Running there, to laugh with so far still to go.
    Running { to: Vec2, laugh_at: f32, laughed: bool },
    /// Catching their breath, or tagging, till then.
    Breathing(f64),
    Tagging(f64),
}

/// A game of tag: who's "it" and who they're after, where it began (and on which lot), where the
/// prey's run to, whether they're out of breath or being tagged, and each player's run and how
/// often their way's been blocked.
pub struct Game {
    pub hunter: Entity,
    pub prey: Entity,
    origin: Vec2,
    lot: Option<usize>,
    dest: Option<Vec2>,
    /// The prey's run the hunter last set off after.
    chased: Option<Vec2>,
    breathless: bool,
    tagging: bool,
    legs: [(Entity, Leg, u8); 2],
    pub rounds: u32,
}

impl Game {
    fn leg(&mut self, e: Entity) -> Option<&mut (Entity, Leg, u8)> {
        self.legs.iter_mut().find(|l| l.0 == e)
    }
}

/// The games under way.
#[derive(Resource, Default)]
pub struct Games(pub Vec<Game>);

impl Games {
    pub fn playing(&self, e: Entity) -> bool {
        self.0.iter().any(|g| g.hunter == e || g.prey == e)
    }
}

/// Play Tag taken up: the one who asked is "it" (`PlayTagSituation.Create(actor, target)`), both
/// at it (what else they'd have done of their own accord dropped), the prey off first.
pub fn start(w: &mut World, hunter: Entity, prey: Entity, autonomous: bool) {
    if w.resource::<Games>().playing(hunter) || w.resource::<Games>().playing(prey) {
        return;
    }
    let Some(origin) = w.get::<Transform>(hunter).map(|t| t.translation) else { return };
    let lot = w.get_resource::<crate::loading::CurrentWorld>().and_then(|cw| crate::rabbitholes::lot_at(&cw.data.lots, origin));
    for (me, other) in [(hunter, prey), (prey, hunter)] {
        let Some(mut q) = w.get_mut::<ActionQueue>(me) else { return };
        for a in q.0.iter_mut().filter(|a| a.autonomous) {
            a.cancel = true;
        }
        q.0.push_front(Action::new(PLAY_TAG, ActionKind::Outro { clips: &[], then: None, secs: f32::MAX, stand_at: None, target: other }, autonomous));
    }
    let now = w.resource::<crate::clock::GameClock>().minutes;
    w.resource_mut::<Games>().0.push(Game {
        hunter,
        prey,
        origin: origin.xz(),
        lot,
        dest: None,
        chased: None,
        breathless: false,
        tagging: false,
        legs: [(hunter, Leg::Waiting(now + HUNTER_DELAY), 0), (prey, Leg::Waiting(now), 0)],
        rounds: 0,
    });
    info!("play tag: begun on lot {lot:?}");
}

/// Whether a Sim's still playing: their game on their queue, not told to stop, nothing else asked
/// of them (a reaction or an ending's just a pause).
fn still_playing(q: &ActionQueue) -> bool {
    q.0.iter().any(|a| a.label == PLAY_TAG && !a.cancel) && !q.0.iter().any(|a| a.label != PLAY_TAG && !a.cancel && !matches!(a.kind, ActionKind::Outro { .. }))
}

/// Had all the fun they can, or a need pressing: done.
fn had_enough(m: &Motives) -> bool {
    m.0[crate::sim::FUN] >= FUN_FULL || [crate::sim::HUNGER, crate::sim::BLADDER, crate::sim::ENERGY].iter().any(|i| m.0[*i] < -40.0)
}

/// Somewhere to run: six to ten metres off (anywhere outdoors on the lot, if they're indoors),
/// within reach of where it began, where `ok` says they can go.
pub fn pick_run(from: Vec2, origin: Vec2, indoors: bool, lot_point: impl Fn(&mut rand::rngs::ThreadRng) -> Option<Vec2>, ok: impl Fn(Vec2) -> bool, rng: &mut rand::rngs::ThreadRng) -> Option<Vec2> {
    (0..32).find_map(|_| {
        let p = if indoors {
            lot_point(rng)?
        } else {
            from + Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU)) * rng.random_range(RUN.0..RUN.1)
        };
        (p.distance(origin) <= MAX_FROM_ORIGIN && ok(p)).then_some(p)
    })
}

#[derive(bevy::ecs::system::SystemParam)]
struct Ways<'w> {
    grid: Option<Res<'w, NavGrid>>,
    upper: Option<Res<'w, UpperFloors>>,
    visited: Option<Res<'w, crate::visit::VisitedLot>>,
    building: Option<Res<'w, crate::building::ActiveBuilding>>,
    world: Res<'w, crate::loading::CurrentWorld>,
}

impl Ways<'_> {
    /// The walks of the lot a Sim's on (a visited one's, else home's), and whether it's away.
    fn of(&self, lot: Option<&crate::visit::OnLot>) -> Option<(&NavGrid, Option<&UpperFloors>, bool)> {
        match lot.and_then(|l| self.visited.as_deref().filter(|v| v.lot == l.0)) {
            Some(v) => Some((&v.grid, Some(&v.upper), true)),
            None => self.grid.as_deref().map(|g| (g, self.upper.as_deref(), false)),
        }
    }

    /// A way there at a run, from their floor to the ground.
    fn route(&self, lot: Option<&crate::visit::OnLot>, from: Vec2, level: u8, to: Vec2) -> Option<Vec<crate::nav::Waypoint>> {
        let (g, u, _) = self.of(lot)?;
        crate::nav::plan_route(g, u, from, level, to, 1)
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    mut games: ResMut<Games>,
    clock: Res<crate::clock::GameClock>,
    delta: Res<crate::clock::SimDelta>,
    ways: Ways,
    mut sims: Query<(&Sim, &mut ActionQueue, &mut Motives, &mut Transform, Option<&PathFollow>, Option<&Floor>, Option<&crate::visit::OnLot>, &mut crate::social::Relationships)>,
    mut cues: MessageWriter<crate::sound::ClipCue>,
) {
    let now = clock.minutes;
    let h = delta.0 / 60.0;
    let mut rng = rand::rng();
    let mut over: Vec<usize> = Vec::new();
    for (gi, g) in games.0.iter_mut().enumerate() {
        let players = [g.hunter, g.prey];
        // (Still both at it: else it's over, for both.)
        let fine = players.iter().all(|p| sims.get(*p).is_ok_and(|s| still_playing(&s.1) && !had_enough(&s.2)));
        let blocked = g.legs.iter().any(|l| l.2 > MAX_ROUTE_FAILURES);
        if !fine || blocked {
            if blocked {
                for l in g.legs.iter().filter(|l| l.2 > MAX_ROUTE_FAILURES) {
                    commands.entity(l.0).try_insert(crate::balloons::BalloonRequest::thought("t_balloon_routefail"));
                }
            }
            over.push(gi);
            continue;
        }
        // (Fun and company for both, and they get on.)
        if h > 0.0 {
            for (me, other) in [(g.hunter, g.prey), (g.prey, g.hunter)] {
                let Ok((_, _, mut m, _, _, _, _, mut rels)) = sims.get_mut(me) else { continue };
                m.add(crate::sim::FUN, FUN_PER_HOUR * h);
                m.add(crate::sim::SOCIAL, SOCIAL_PER_HOUR * h);
                rels.entry(other).stc.update(crate::conversation::Commodity::Friendly, STC_PER_HOUR * h);
                rels.add(other, LIKING_PER_HOUR * h, 0.0);
            }
        }
        // (The laugh as they run, so far from the end.)
        for l in g.legs.iter_mut() {
            if let Leg::Running { to, laugh_at, laughed: false } = l.1
                && let Ok((sim, _, _, tf, ..)) = sims.get(l.0)
                && tf.translation.xz().distance(to) <= laugh_at
            {
                l.1 = Leg::Running { to, laugh_at, laughed: true };
                cues.write(crate::sound::ClipCue { sim: l.0, name: laugh(sim.age).to_string(), action: s3formats::sim::SoundAction::Play });
            }
        }
        let arrived = |e: Entity, sims: &Query<(&Sim, &mut ActionQueue, &mut Motives, &mut Transform, Option<&PathFollow>, Option<&Floor>, Option<&crate::visit::OnLot>, &mut crate::social::Relationships)>| {
            sims.get(e).is_ok_and(|s| s.4.is_none_or(|p| p.done))
        };
        // The prey.
        let prey = g.prey;
        let (prey_age, weight, fitness, prey_at, prey_level) = match sims.get(prey) {
            Ok(s) => (s.0.age, s.0.weight, s.0.fitness, s.3.translation, s.5.map_or(1, |f| f.0)),
            Err(_) => continue,
        };
        let Some(&(_, leg, _)) = g.legs.iter().find(|l| l.0 == prey) else { continue };
        match leg {
            Leg::Waiting(t) if now >= t => {
                let away = ways.of(sims.get(prey).ok().and_then(|s| s.6)).is_some_and(|w| w.2);
                let indoors = |p: Vec2| !away && ways.building.as_deref().is_some_and(|b| b.is_indoors(Vec3::new(p.x, 0.0, p.y)));
                let lots = &ways.world.data.lots;
                let on_lot = |p: Vec2| g.lot.is_none() || crate::rabbitholes::lot_at(lots, Vec3::new(p.x, 0.0, p.y)) == g.lot;
                let open = |p: Vec2| ways.of(sims.get(prey).ok().and_then(|s| s.6)).and_then(|(grid, ..)| grid.cell_of(p).map(|(x, z)| !grid.is_blocked(x, z))).unwrap_or(false);
                let lot_point = |rng: &mut rand::rngs::ThreadRng| {
                    let l = lots.get(g.lot?)?;
                    let local = Vec3::new(rng.random_range(0.0..l.width as f32), 0.0, rng.random_range(0.0..l.depth as f32));
                    Some((Vec3::from(l.corner) + Quat::from_rotation_y(l.rotation) * local).xz())
                };
                let here = prey_at.xz();
                let inside = prey_level != 1 || indoors(here);
                let lot_ref = sims.get(prey).ok().and_then(|s| s.6).copied();
                let way = pick_run(here, g.origin, inside, lot_point, |p| on_lot(p) && open(p) && !indoors(p), &mut rng).and_then(|to| ways.route(lot_ref.as_ref(), here, prey_level, to).map(|wp| (to, wp)));
                let Some(l) = g.leg(prey) else { continue };
                match way {
                    Some((to, wp)) => {
                        let length = PathFollow::length_from(&wp, here);
                        commands.entity(prey).try_remove::<crate::anim::ActionClip>().try_insert(PathFollow::new(wp).with_style(WalkStyle::Run));
                        l.1 = Leg::Running { to, laugh_at: length * rng.random_range(LAUGH_AT.0..LAUGH_AT.1), laughed: false };
                        g.dest = Some(to);
                        debug!("play tag: the prey runs {length:.0} m");
                    }
                    None => {
                        l.2 += 1;
                        l.1 = Leg::Waiting(now + 1.0);
                    }
                }
            }
            Leg::Running { .. } if arrived(prey, &sims) => {
                commands.entity(prey).try_remove::<PathFollow>();
                let Some(l) = g.leg(prey) else { continue };
                if rng.random::<f32>() < breath_chance(weight, fitness) || std::env::var_os("TAG_BREATHE").is_some() {
                    commands.entity(prey).try_insert(crate::anim::ActionClip::new(Some(breathe_clip(prey_age)), IDLE));
                    l.1 = Leg::Breathing(now + BREATHE_SECS);
                    g.breathless = true;
                } else {
                    l.1 = Leg::Waiting(now);
                }
            }
            Leg::Breathing(t) if now >= t => {
                commands.entity(prey).try_remove::<crate::anim::ActionClip>();
                g.breathless = false;
                if let Some(l) = g.leg(prey) {
                    l.1 = Leg::Waiting(now);
                }
                // (Tagged: "it", and the other runs.)
                if g.tagging {
                    g.tagging = false;
                    (g.hunter, g.prey) = (g.prey, g.hunter);
                    g.dest = None;
                    g.chased = None;
                    g.rounds += 1;
                    let (h, p) = (g.hunter, g.prey);
                    for l in g.legs.iter_mut() {
                        l.1 = Leg::Waiting(if l.0 == h { now + HUNTER_DELAY } else { now });
                    }
                    commands.entity(p).try_remove::<crate::anim::ActionClip>();
                    info!("play tag: round {} over, the other's \"it\"", g.rounds);
                    continue;
                }
            }
            _ => {}
        }
        // The hunter.
        let hunter = g.hunter;
        let (hunter_age, hunter_at, hunter_level, hunter_lot) = match sims.get(hunter) {
            Ok(s) => (s.0.age, s.3.translation, s.5.map_or(1, |f| f.0), s.6.copied()),
            Err(_) => continue,
        };
        let Some(&(_, leg, _)) = g.legs.iter().find(|l| l.0 == hunter) else { continue };
        match leg {
            Leg::Waiting(t) if now >= t && !g.tagging => {
                let Some(dest) = g.dest else { continue };
                // (After the prey's run, not the one already run after, unless they've gone from it.)
                if g.chased == Some(dest) && hunter_at.xz().distance(dest) < TAG_REACH {
                    continue;
                }
                let here = hunter_at.xz();
                let to = dest + (here - dest).normalize_or_zero() * CHASE_SHORT.min(here.distance(dest));
                let Some(l) = g.leg(hunter) else { continue };
                match ways.route(hunter_lot.as_ref(), here, hunter_level, to) {
                    Some(wp) => {
                        let length = PathFollow::length_from(&wp, here);
                        commands.entity(hunter).try_remove::<crate::anim::ActionClip>().try_insert(PathFollow::new(wp).with_style(WalkStyle::Run));
                        l.1 = Leg::Running { to, laugh_at: length * rng.random_range(LAUGH_AT.0..LAUGH_AT.1), laughed: false };
                        g.chased = Some(dest);
                    }
                    None => {
                        l.2 += 1;
                        l.1 = Leg::Waiting(now + 1.0);
                    }
                }
            }
            Leg::Running { .. } if arrived(hunter, &sims) => {
                commands.entity(hunter).try_remove::<PathFollow>();
                let caught = g.breathless && hunter_at.distance(prey_at) < TAG_REACH;
                if caught {
                    g.tagging = true;
                }
                let Some(l) = g.leg(hunter) else { continue };
                if caught {
                    // (Caught: turned to them, and tagged.)
                    let d = prey_at - hunter_at;
                    if let Ok((.., mut tf, _, _, _, _)) = sims.get_mut(hunter) {
                        tf.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
                    }
                    commands.entity(hunter).try_insert(crate::anim::ActionClip::new(Some(tag_clip(hunter_age, prey_age)), IDLE));
                    l.1 = Leg::Tagging(now + TAG_SECS);
                    info!("play tag: tagged");
                } else {
                    l.1 = Leg::Waiting(now);
                }
            }
            Leg::Tagging(t) if now >= t => {
                commands.entity(hunter).try_remove::<crate::anim::ActionClip>();
                if let Some(l) = g.leg(hunter) {
                    l.1 = Leg::Waiting(now);
                }
            }
            _ => {}
        }
    }
    // (Over: both stop, where they are.)
    for gi in over.into_iter().rev() {
        let g = games.0.remove(gi);
        for p in [g.hunter, g.prey] {
            if let Ok((sim, mut q, ..)) = sims.get_mut(p) {
                for a in q.0.iter_mut().filter(|a| a.label == PLAY_TAG) {
                    a.cancel = true;
                }
                info!("play tag: {} stops playing ({} rounds)", sim.first, g.rounds);
            }
            commands.entity(p).try_remove::<(PathFollow, crate::anim::ActionClip)>();
        }
    }
}

/// `TAG_TEST=social|play|menu` (tests): ten seconds in, the selected Sim (made Childish if
/// they're past childhood) asks another of the household to Play Tag, or the game's begun with
/// them straight off, or the other's menu is up; the camera follows the selected Sim
/// (`TAG_DIST` off). `TAG_BREATHE=1`: out of breath after every run.
fn tag_test(
    mut commands: Commands,
    time: Res<Time>,
    mut me: Query<(Entity, &mut Sim, &mut ActionQueue, &Transform), With<crate::sim::Selected>>,
    others: Query<(Entity, &Sim, &Transform), (With<crate::sim::HouseholdMember>, Without<crate::sim::Selected>)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut done: Local<bool>,
) {
    let Ok(what) = std::env::var("TAG_TEST") else { return };
    let t = time.elapsed_secs();
    let Ok((e, mut sim, mut q, tf)) = me.single_mut() else { return };
    if t > 7.0
        && let Ok(mut cam) = cams.single_mut()
    {
        cam.look_at(tf.translation);
        cam.distance = std::env::var("TAG_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(14.0);
    }
    if *done || t < 10.0 {
        return;
    }
    *done = true;
    if sim.age != Age::Child && !sim.traits.contains(&Trait::Childish) {
        sim.traits.push(Trait::Childish);
    }
    let Some((other, _, _)) = others.iter().filter(|o| may_play(&sim, o.1.age, false)).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation))) else {
        warn!("tag test: nobody to play with");
        return;
    };
    if what == "play" {
        commands.queue(move |w: &mut World| start(w, e, other, false));
        info!("tag test: playing");
        return;
    }
    // (=menu: the other's menu, for `PHOTO_MENU=Friendly`.)
    if what == "menu" {
        commands.insert_resource(crate::hud::PieFor(other));
        return;
    }
    let Some(i) = crate::social::social_index(PLAY_TAG) else { return };
    q.push_player(Action::new(PLAY_TAG, ActionKind::Social { target: other, social: i }, false));
    info!("tag test: asking");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(age: Age, traits: &[Trait]) -> Sim {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "T", Some(true), age);
        s.traits = traits.to_vec();
        s
    }

    #[test]
    fn children_and_the_childish_play_tag() {
        assert!(may_play(&sim(Age::Child, &[]), Age::Adult, true));
        assert!(!may_play(&sim(Age::Adult, &[]), Age::Child, false));
        assert!(may_play(&sim(Age::Adult, &[Trait::Childish]), Age::Child, false));
        assert!(!may_play(&sim(Age::Child, &[]), Age::Toddler, false));
        // (The outdoors-hater asks only when told to.)
        assert!(!may_play(&sim(Age::Child, &[Trait::HatesTheOutdoors]), Age::Child, true));
        assert!(may_play(&sim(Age::Child, &[Trait::HatesTheOutdoors]), Age::Child, false));
    }

    #[test]
    fn the_heavy_and_unfit_catch_their_breath_more() {
        assert_eq!(breath_chance(-1.0, 1.0), BREATH_MIN);
        assert!((breath_chance(1.0, 0.0) - 1.5).abs() < 1e-6);
        assert!(breath_chance(0.5, 0.1) > breath_chance(0.5, 0.6));
    }

    #[test]
    fn runs_stay_near_where_it_began() {
        let mut rng = rand::rng();
        let origin = Vec2::ZERO;
        for _ in 0..50 {
            let from = Vec2::new(30.0, 0.0);
            let p = pick_run(from, origin, false, |_| None, |_| true, &mut rng).unwrap();
            let d = p.distance(from);
            assert!((RUN.0..=RUN.1).contains(&d) && p.length() <= MAX_FROM_ORIGIN);
        }
        // (Nowhere they can go: no run.)
        assert!(pick_run(Vec2::ZERO, origin, false, |_| None, |_| false, &mut rng).is_none());
    }

    #[test]
    fn the_tag_clips_suit_their_heights() {
        assert_eq!(tag_clip(Age::Child, Age::Adult), "c2a_playTag_Tagger_accept_x");
        assert_eq!(tag_clip(Age::Teen, Age::Child), "a2c_playTag_Tagger_accept_x");
        assert_eq!(breathe_clip(Age::Child), "c2c_playTag_loopBreathe_y");
        let (x, y) = social_clips(Age::Child, Age::Adult, false);
        assert_eq!((x.start, y.start), (Some("c2a_playTag_Tagger_reject_x"), Some("a2a_playTag_Taggee_reject_y")));
        let mut q = ActionQueue::default();
        q.0.push_back(Action::new(PLAY_TAG, ActionKind::Outro { clips: &[], then: None, secs: f32::MAX, stand_at: None, target: Entity::PLACEHOLDER }, true));
        assert!(still_playing(&q));
        q.0.push_back(Action::new("Chat", ActionKind::GoHere(Vec2::ZERO, 1), false));
        assert!(!still_playing(&q));
    }
}
