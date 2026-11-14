//! Hopscotch, as Generations has it (`HopscotchCourt`: the Hopscotch Court and Hopscotch
//! Asteroids). "Practice Hopscotch" alone, "Join" a game going (up to four, `kMaxNumPlayers`), or
//! "Play Hopscotch With... ›" another Sim about. The players line up beside the court waiting
//! their turn ("Waiting", `WaitToHop`: a little fun and company); at their turn they go to its
//! end ("Hop", `TakeTurn`), toss the marker to the three and hop it, and done cleanly, toss it
//! to the six and hop again (the game's `a2o_Hopscotch_throw_three`, `_success_three` or
//! `_fail_three`...; a child's `c2o_`). How likely a clean hop is goes by the game's tuning: half,
//! and the hopscotch skill's say (−25 to +25), and the Athletic skill's (a child's own athletics,
//! up to +20): `DetermineSuccessModifier`. A stumble sets the others laughing. When everyone's
//! had a turn, those who hopped both cleanly win (there being others to beat): the game's
//! notice and sting, the winners cheering, the rest booing or sulking or cheering them, and the
//! game's over; with no winner, round again. Hopping is Athletic practice (a child's, its own
//! athletics) and hopscotch practice, and fun (the turn's tuning: Athletic 100, Child Athletic
//! 25, Hopscotch 175, Fun 50 an hour; with others, company too, and liking between them, +1 a
//! turn). Children and teens with little fun go and play of their own accord.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use rand::Rng;

use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct HopscotchPlugin;

impl Plugin for HopscotchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Courts>().add_systems(Update, (manage, turns, laughing, autonomy, hopscotch_test).chain().after(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's words (`Gameplay/Objects/HobbiesSkills/Hopscotch`).
pub const PRACTICE: &str = "Practice Hopscotch";
pub const JOIN: &str = "Join";
pub const HOP: &str = "Hop";
pub const WAIT: &str = "Waiting";
pub const PLAY_WITH: &str = "Play Hopscotch With...";
pub const FULL: &str = "Maximum number of players reached.";
pub const NO_ONE: &str = "No one to invite.";
/// The game's tuning: players, the skills' say in a clean hop, liking a turn, what hopping and
/// waiting are worth an hour.
pub const MAX_PLAYERS: usize = 4;
const HOP_SKILL: [f32; 11] = [-20.0, -15.0, -10.0, -5.0, 0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 25.0];
/// Waiting, now and then a cheer or a bout of excitement (`kWaitReactions`), every few minutes
/// (`kReactionPeriod`).
const WAIT_REACTIONS: [&str; 2] = ["Cheer", "Excited"];
const REACTION_PERIOD: f64 = 4.0;
const ATHLETIC: [f32; 11] = [0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0];
const CHILD_ATHLETIC: [f32; 4] = [0.0, 10.0, 15.0, 20.0];
const LIKING: f32 = 1.0;
const TURN_FUN: f32 = 50.0;
const TURN_ATHLETIC: f32 = 100.0;
const TURN_CHILD_ATHLETIC: f32 = 25.0;
const TURN_HOPSCOTCH: f32 = 175.0;
const WAIT_FUN: f32 = 5.0;
const WAIT_SOCIAL: f32 = 5.0;
const PLAYING_SOCIAL: f32 = 20.0;
/// Where the court's end is, the routing slot a hopper starts from (out along +Z, facing back
/// down the court).
const START: f32 = 2.27081;

/// A court's game: who's in it in their turn's order, who's won this round, who's hopped, and
/// whether it's over.
#[derive(Default, Debug, Clone)]
pub struct Court {
    pub queue: VecDeque<Entity>,
    winners: Vec<Entity>,
    hopped: Vec<Entity>,
    over: bool,
}

impl Court {
    fn reset(&mut self) {
        *self = Court::default();
    }

    fn remove(&mut self, e: Entity) {
        self.queue.retain(|x| *x != e);
        self.hopped.retain(|x| *x != e);
    }

    /// A turn done (clean or not): its hopper to the back of the line; everyone's had a turn,
    /// the winners, if any (the game over), else round again.
    pub fn turn_done(&mut self, e: Entity, clean: bool) -> Option<Vec<Entity>> {
        if clean && self.queue.len() > 1 && !self.winners.contains(&e) {
            self.winners.push(e);
        }
        if !self.hopped.contains(&e) {
            self.hopped.push(e);
        }
        if let Some(i) = self.queue.iter().position(|x| *x == e) {
            self.queue.remove(i);
        }
        self.queue.push_back(e);
        if self.hopped.len() >= self.queue.len() {
            if !self.winners.is_empty() {
                self.over = true;
                return Some(self.winners.clone());
            }
            self.hopped.clear();
        }
        None
    }
}

/// The courts' games.
#[derive(Resource, Default)]
pub struct Courts(pub HashMap<Entity, Court>);

impl Courts {
    pub fn queue_len(&self, court: Entity) -> usize {
        self.0.get(&court).filter(|c| !c.over).map_or(0, |c| c.queue.len())
    }

    pub fn has(&self, court: Entity, sim: Entity) -> bool {
        self.0.get(&court).is_some_and(|c| !c.over && c.queue.contains(&sim))
    }
}

/// The chance (percent) of a clean hop: half, and the skills' say.
pub fn clean_chance(child: bool, athletic: u32, child_athletic: u32, hopscotch: u32) -> f32 {
    let skill = if child { CHILD_ATHLETIC[child_athletic.min(3) as usize] } else { ATHLETIC[athletic.min(10) as usize] };
    50.0 + skill + HOP_SKILL[hopscotch.min(10) as usize]
}

/// Where a hopper starts, and the way they face (down the court).
pub fn start_spot(court: &Transform) -> (Vec2, Vec2) {
    let at = court.transform_point(Vec3::new(0.0, 0.0, START)).xz();
    (at, (court.rotation * Vec3::NEG_Z).xz().normalize_or(Vec2::Y))
}

/// Where the waiting stand, beside the court (three to four metres from its middle,
/// `kMinRouteDistance`, `kMaxRouteDistance`), facing it: one place each.
pub fn wait_spot(court: &Transform, i: usize) -> (Vec2, Vec2) {
    let side = if i % 2 == 0 { 1.0 } else { -1.0 };
    let along = if i < 2 { 1.6 } else { -0.8 };
    let at = court.transform_point(Vec3::new(2.9 * side, 0.0, along)).xz();
    let to = court.translation.xz() - at;
    (at, to.normalize_or(Vec2::Y))
}

/// A Sim's clips' age (`a2o` grown and teens, `c2o` children).
fn who(age: Age) -> &'static str {
    if age == Age::Child { "c" } else { "a" }
}

/// The clips for a hop at a target: the toss, and the hop clean or a stumble (on the way out,
/// or on the way back, `failReturning`).
pub fn hop_clips(age: Age, target: &str, clean: bool, back: bool) -> [String; 2] {
    let w = who(age);
    let hop = if clean { "success" } else if back { "failReturning" } else { "fail" };
    [format!("{w}2o_Hopscotch_throw_{target}_x"), format!("{w}2o_Hopscotch_{hop}_{target}_x")]
}

/// Who can play: children and up.
pub fn can_play(age: Age) -> bool {
    !matches!(age, Age::Baby | Age::Toddler)
}

/// The court's menu for a Sim: practice (no game going), join (one going, room in it), and play
/// with someone about.
pub fn options(court: Entity, courts: &Courts, me: Entity, wait: (Vec2, Vec2), others: &[(Entity, String, (Vec2, Vec2))], pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>, submenu: impl Fn(usize) -> ActionKind) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    if courts.has(court, me) {
        return out;
    }
    let n = courts.queue_len(court);
    let mine = |with: Option<(Entity, Vec2, Vec2)>| ActionKind::Hopscotch { court, turn: false, at: wait.0, face: wait.1, with };
    match n {
        0 => out.push((PRACTICE.to_string(), mine(None))),
        n if n >= MAX_PLAYERS => out.push((JOIN.to_string(), ActionKind::Greyed(FULL.into()))),
        _ => out.push((JOIN.to_string(), mine(None))),
    }
    if n + 1 >= MAX_PLAYERS {
        out.push((PLAY_WITH.to_string(), ActionKind::Greyed(FULL.into())));
    } else if others.is_empty() {
        out.push((PLAY_WITH.to_string(), ActionKind::Greyed(NO_ONE.into())));
    } else {
        let list: Vec<(String, ActionKind)> = others.iter().map(|(o, name, (at, face))| (name.clone(), mine(Some((*o, *at, *face))))).collect();
        out.push((format!("{PLAY_WITH} ›"), submenu(pie_submenus.len())));
        pie_submenus.push((PLAY_WITH.to_string(), list));
    }
    out
}

/// A Sim at the court: their front action's, if it's hopscotch.
fn at_court(q: &ActionQueue) -> Option<(Entity, bool)> {
    match q.0.front()?.kind {
        ActionKind::Hopscotch { court, turn, .. } => Some((court, turn)),
        _ => None,
    }
}

/// The lines kept: Sims joining, bringing whom they asked along, turns given as they come up,
/// those who've gone off something else left out, a game over sent home; and what waiting's
/// worth.
#[allow(clippy::type_complexity)]
fn manage(
    mut courts: ResMut<Courts>,
    delta: Res<crate::clock::SimDelta>,
    mut sims: Query<(Entity, &mut ActionQueue, &mut crate::sim::Motives, &Sim)>,
    objects: Query<(&GameObject, &Transform)>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let hours = delta.0 / 60.0;
    // (Gone from the court, or off doing something else: out of its game.)
    courts.0.retain(|c, _| objects.get(*c).is_ok_and(|(o, _)| o.kind == ObjectKind::Hopscotch));
    let playing: HashMap<Entity, Entity> = sims.iter().filter_map(|(e, q, ..)| at_court(&q).map(|(c, _)| (e, c))).collect();
    for (c, court) in courts.0.iter_mut() {
        let gone: Vec<Entity> = court.queue.iter().copied().filter(|e| playing.get(e) != Some(c)).collect();
        for e in gone {
            court.remove(e);
        }
        if court.queue.is_empty() && court.over {
            court.reset();
        }
    }
    let mut invited: Vec<(Entity, Entity, Vec2, Vec2)> = Vec::new();
    for (e, mut q, mut m, sim) in &mut sims {
        let Some(Action { kind: ActionKind::Hopscotch { court: c, turn, with, .. }, phase, .. }) = q.0.front_mut() else { continue };
        let (c, turn) = (*c, *turn);
        let Ok((_, ctf)) = objects.get(c) else {
            q.0.pop_front();
            continue;
        };
        if let Some(w) = with.take() {
            invited.push((w.0, c, w.1, w.2));
        }
        let running = matches!(phase, Phase::Running(_));
        let court = courts.0.entry(c).or_default();
        if court.over {
            // (The game's over: off they go, once they've hopped.)
            if !turn {
                q.0.pop_front();
            }
            continue;
        }
        if !court.queue.contains(&e) {
            if court.queue.len() >= MAX_PLAYERS {
                notes.push(format!("{}: {FULL}", sim.first));
                q.0.pop_front();
                continue;
            }
            if court.queue.is_empty() {
                court.reset();
            }
            court.queue.push_back(e);
        }
        if turn {
            continue;
        }
        // (Their turn: to the court's end.)
        if court.queue.front() == Some(&e) {
            let (at, face) = start_spot(ctf);
            q.0.pop_front();
            q.0.push_front(Action::new(HOP, ActionKind::Hopscotch { court: c, turn: true, at, face, with: None }, false));
            continue;
        }
        if running && hours > 0.0 {
            m.add(crate::sim::FUN, WAIT_FUN * hours);
            m.add(crate::sim::SOCIAL, (WAIT_SOCIAL + PLAYING_SOCIAL) * hours);
        }
    }
    // (Those asked along: in the line too, what they were at put off.)
    for (o, c, at, face) in invited {
        let Ok((_, mut q, _, sim)) = sims.get_mut(o) else { continue };
        if at_court(&q).is_some() || !can_play(sim.age) {
            continue;
        }
        let court = courts.0.entry(c).or_default();
        if court.queue.len() >= MAX_PLAYERS {
            continue;
        }
        court.queue.push_back(o);
        for a in q.0.iter_mut() {
            if a.autonomous {
                a.cancel = true;
            }
        }
        q.0.push_front(Action::new(WAIT, ActionKind::Hopscotch { court: c, turn: false, at, face, with: None }, false));
        info!("hopscotch: {} asked to play", sim.first);
    }
}

/// A hopper's turn under way: the round (to the three, then the six), its clips, how far in.
#[derive(Component)]
struct Hopping {
    court: Entity,
    round: u8,
    clean: bool,
    time: f32,
    length: f32,
    gap: u8,
    since: f64,
}

/// Turns hopped: the toss and the hop (clean or not by the skills' say), the six after a clean
/// three; then the line moves on, a winner declared once everyone's hopped.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn turns(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut courts: ResMut<Courts>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut crate::interact::Skills, &mut crate::sim::Motives, &mut SimAnim, Option<&mut Hopping>, Has<crate::sim::HouseholdMember>)>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    court_tfs: Query<&Transform, (With<GameObject>, Without<Sim>)>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let hours = delta.0 / 60.0;
    let mut rng = rand::rng();
    let mut ended: Vec<(Entity, Vec<Entity>, Vec<Entity>)> = Vec::new();
    let mut laughs: Vec<Entity> = Vec::new();
    let mut liking: Vec<(Entity, Vec<Entity>)> = Vec::new();
    for (e, sim, mut q, mut skills, mut m, mut anim, hopping, _) in &mut sims {
        let running = q.0.front().and_then(|a| match a.kind {
            ActionKind::Hopscotch { court, turn: true, .. } if matches!(a.phase, Phase::Running(_)) && !a.cancel => Some(court),
            _ => None,
        });
        let Some(mut h) = hopping else {
            let Some(court) = running else { continue };
            commands.entity(e).insert(Hopping { court, round: 0, clean: true, time: 0.0, length: 0.0, gap: 0, since: clock.minutes });
            continue;
        };
        if running != Some(h.court) {
            commands.entity(e).remove::<(Hopping, ActionClip)>();
            continue;
        }
        // (Practice: athletics, hopscotch, fun; with others, company.)
        if hours > 0.0 {
            let child = sim.age == Age::Child;
            skills.add_points(if child { "ChildAthletic" } else { "Athletic" }, (if child { TURN_CHILD_ATHLETIC } else { TURN_ATHLETIC }) * hours);
            skills.add_points("Hopscotch", TURN_HOPSCOTCH * hours);
            m.add(crate::sim::FUN, TURN_FUN * hours);
            if courts.queue_len(h.court) > 1 {
                m.add(crate::sim::SOCIAL, PLAYING_SOCIAL * hours);
            }
        }
        if h.gap > 0 {
            // (A moment between hops, the last one's travel set down.)
            h.gap -= 1;
            continue;
        }
        if h.length > 0.0 {
            h.time += dt;
            if h.time < h.length {
                continue;
            }
            commands.entity(e).remove::<ActionClip>();
            h.round += 1;
            h.length = 0.0;
            h.gap = 3;
            if h.round < 2 && h.clean {
                continue;
            }
            // The turn's done: the line moves on.
            let court = courts.0.entry(h.court).or_default();
            let others: Vec<Entity> = court.queue.iter().copied().filter(|x| *x != e).collect();
            if !others.is_empty() {
                liking.push((e, others));
            }
            info!("hopscotch: {} hopped {} ({:.0} minutes)", sim.first, if h.clean { "cleanly" } else { "and stumbled" }, clock.minutes - h.since);
            if let Some(winners) = court.turn_done(e, h.clean) {
                ended.push((h.court, winners, court.queue.iter().copied().collect::<Vec<_>>()));
            }
            q.0.pop_front();
            anim.pose = Pose::Stand;
            commands.entity(e).remove::<Hopping>();
            // (Solo practice: until they've had their fun, or need something.)
            let solo = court.queue.len() == 1;
            let needy = [crate::sim::HUNGER, crate::sim::BLADDER, crate::sim::ENERGY].iter().any(|i| m.0[*i] < -40.0);
            if !court.over && !(solo && (m.0[crate::sim::FUN] >= 95.0 || needy)) {
                // (Back to the side to wait their turn: their place in the line's.)
                let c = h.court;
                let i = court.queue.iter().position(|x| *x == e).unwrap_or(0);
                let (at, face) = court_tfs.get(c).map_or((Vec2::ZERO, Vec2::Y), |ctf| wait_spot(ctf, i));
                q.0.push_front(Action::new(WAIT, ActionKind::Hopscotch { court: c, turn: false, at, face, with: None }, false));
            } else {
                court.remove(e);
            }
            continue;
        }
        // The next hop: to the three, then the six.
        let target = if h.round == 0 { "three" } else { "six" };
        let child = sim.age == Age::Child;
        let chance = clean_chance(child, skills.level("Athletic"), skills.level("ChildAthletic"), skills.level("Hopscotch"));
        let clean = rng.random_range(0.0..100.0) < chance;
        let [toss, hop] = hop_clips(sim.age, target, clean, rng.random_bool(0.5));
        let length: f32 = [&toss, &hop].iter().map(|c| lib.get(&data, c).map_or(2.0, |k| k.duration)).sum::<f32>() - 0.05;
        h.clean = clean;
        h.time = 0.0;
        h.length = length.max(0.5);
        anim.pose = Pose::Use;
        let toss: &'static str = crate::pets::intern(toss);
        let hop: &'static [&'static str] = Box::leak(Box::new([crate::pets::intern(hop)]));
        commands.entity(e).insert(ActionClip::steps(toss, hop, &[]));
        if !clean {
            laughs.push(h.court);
        }
        debug!("hopscotch: {} tosses to the {target} ({chance:.0}%: {})", sim.first, if clean { "clean" } else { "stumble" });
    }
    // (Liking between those playing, a turn's worth.)
    for (e, others) in liking {
        commands.queue(move |w: &mut World| {
            for o in &others {
                for (a, b) in [(e, *o), (*o, e)] {
                    if let Some(mut r) = w.get_mut::<crate::social::Relationships>(a) {
                        r.add(b, LIKING, 0.0);
                    }
                }
            }
        });
    }
    // (A stumble: the others laughing.)
    for c in laughs {
        if let Some(court) = courts.0.get(&c) {
            for o in court.queue.iter().skip(1) {
                commands.entity(*o).insert(Laugh(rng.random_range(5.0..8.0)));
            }
        }
    }
    // A game won: the notice and its sting, and how each takes it.
    for (c, winners, players) in ended {
        let names: Vec<String> = winners.iter().filter_map(|w| sims.get(*w).ok().map(|s| s.1.first.clone())).collect();
        let ours = winners.iter().any(|w| sims.get(*w).is_ok_and(|s| s.7));
        if ours {
            notes.push(match names.len() {
                1 => format!("{} won the game of hopscotch!", names[0]),
                n if n >= MAX_PLAYERS => "Everyone won the game of hopscotch!".to_string(),
                _ => format!("{} won the game of hopscotch together!", join(&names)),
            });
            play.write(crate::sound::PlaySound::ui("sting_hopscotch_win"));
        }
        info!("hopscotch: game over, won by {}", join(&names));
        for p in players {
            let Ok((_, sim, ..)) = sims.get(p) else { continue };
            let kind = if winners.contains(&p) {
                WIN
            } else if sim.traits.contains(&crate::life::Trait::HotHeaded) {
                "Angry"
            } else if sim.traits.iter().any(|t| matches!(t, crate::life::Trait::Grumpy | crate::life::Trait::MeanSpirited | crate::life::Trait::Snob)) {
                if rng.random_bool(0.5) { "Boo" } else { "Angry" }
            } else {
                "Cheer"
            };
            commands.entity(p).insert(EndReaction(kind, 0.5, court_tfs.get(c).map_or(Vec3::ZERO, |t| t.translation)));
        }
    }
}

fn join(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A waiting player laughing at a stumble, in so many seconds.
#[derive(Component)]
struct Laugh(f32);

/// A reaction playing (seconds left), and whether only while they wait (a laugh at a stumble,
/// the game's `PushFailReactions`; a cheer while waiting) or come what may (the game's end).
#[derive(Component)]
struct Laughing(f32, bool);

/// How a player takes the game's end (the `Reactions` table's kind), in so many seconds: played
/// at once wherever they are (the game's high-priority `PlayReaction`).
#[derive(Component)]
struct EndReaction(&'static str, f32, Vec3);

/// A winner's (the `Reactions` table's `GameWinExcited`, its hopscotch clips).
const WIN: &str = "GameWinExcited";

#[allow(clippy::type_complexity)]
fn laughing(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib, ui): (Res<Baked>, ResMut<ClipLibrary>, Option<Res<crate::icons::GameUi>>),
    mut waiting: Query<(Entity, &Sim, &mut ActionQueue, &Transform, Option<&mut Laugh>, Option<&mut Laughing>, Option<&ActionClip>, Option<&mut EndReaction>)>,
    (mut known, mut next): (Local<Option<HashSet<String>>>, Local<HashMap<Entity, f64>>),
) {
    let Some(ui) = ui else { return };
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let known = known.get_or_insert_with(|| data.0.clip_names.iter().map(|n| n.to_ascii_lowercase()).collect());
    let mut rng = rand::rng();
    for (e, sim, mut q, tf, laugh, laughing, clip, end) in &mut waiting {
        let waiting = matches!(at_court(&q), Some((_, false))) && q.0.front().is_some_and(|a| matches!(a.phase, Phase::Running(_)));
        if let Some(mut l) = laughing {
            l.0 -= dt;
            if l.0 <= 0.0 || (l.1 && !waiting) {
                commands.entity(e).remove::<(Laughing, ActionClip)>();
            }
            continue;
        }
        // (The game's end: cheered, booed, sulked at, once they've stopped.)
        if let Some(mut r) = end {
            r.1 -= dt;
            if r.1 > 0.0 || matches!(at_court(&q), Some((_, true))) {
                continue;
            }
            commands.entity(e).remove::<EndReaction>();
            // (A win: hopscotch's own cheers, a child's theirs.)
            let win = (r.0 == WIN).then(|| vec![format!("{}2o_Hopscotch_win_react{}_x", who(sim.age), rng.random_range(1..=3))]).filter(|c| known.contains(&c[0].to_ascii_lowercase()));
            let Some(clips) = win.or_else(|| crate::reactions::clips_for(&ui.data.idles.reactions, r.0, sim, false, known, &mut rng)) else {
                debug!(
                    "hopscotch: no {} for {}: {:?}",
                    r.0,
                    sim.first,
                    ui.data.idles.reactions.iter().filter(|g| g.0.eq_ignore_ascii_case(r.0)).flat_map(|g| g.1.iter().map(|a| (a.ages.clone(), a.clips.clone()))).collect::<Vec<_>>()
                );
                continue;
            };
            let secs: f32 = clips.iter().filter_map(|c| lib.get(&data, c)).map(|c| c.duration).sum();
            let leaked: Vec<&'static str> = clips.into_iter().map(crate::pets::intern).collect();
            let leaked: &'static [&'static str] = Box::leak(leaked.into_boxed_slice());
            debug!("hopscotch: {} takes the game's end: {}", sim.first, leaked.join(", "));
            // (Where they stand, turned to the court, before anything they'd go on to.)
            for a in q.0.iter_mut() {
                if a.autonomous {
                    a.cancel = true;
                }
            }
            if let Some(a) = q.0.front_mut()
                && matches!(a.phase, Phase::Routing)
            {
                a.phase = Phase::Start;
            }
            q.0.push_front(Action::new("React", ActionKind::Outro { clips: leaked, then: None, secs, stand_at: None, target: e }, true));
            commands.entity(e).remove::<crate::nav::PathFollow>().try_insert(ActionClip::steps(leaked[0], &leaked[1..], &[]));
            let d = r.2 - tf.translation;
            if d.xz().length() > 0.2 {
                commands.entity(e).insert(crate::reactions::Facing(Quat::from_rotation_y(d.x.atan2(d.z))));
            }
            continue;
        }
        if !waiting {
            next.remove(&e);
            if laugh.is_some() {
                commands.entity(e).remove::<Laugh>();
            }
            continue;
        }
        // (Now and then while they wait, a cheer or some excitement.)
        let due = *next.entry(e).or_insert(clock.minutes + rng.random_range(1.0..REACTION_PERIOD * 2.0));
        let kind = match laugh {
            Some(mut l) => {
                l.0 -= dt;
                if l.0 > 0.0 || clip.is_some() {
                    continue;
                }
                commands.entity(e).remove::<Laugh>();
                if rng.random_bool(0.5) { "PointLaugh" } else { "LaughAt" }
            }
            None if clock.minutes >= due && clip.is_none() => {
                next.insert(e, clock.minutes + rng.random_range(REACTION_PERIOD..REACTION_PERIOD * 3.0));
                WAIT_REACTIONS[rng.random_range(0..WAIT_REACTIONS.len())]
            }
            None => continue,
        };
        let Some(clips) = crate::reactions::clips_for(&ui.data.idles.reactions, kind, sim, false, known, &mut rng) else { continue };
        let secs: f32 = clips.iter().filter_map(|c| lib.get(&data, c)).map(|c| c.duration).sum();
        let leaked: Vec<&'static str> = clips.into_iter().map(crate::pets::intern).collect();
        let leaked: &'static [&'static str] = Box::leak(leaked.into_boxed_slice());
        commands.entity(e).insert((ActionClip::steps(leaked[0], &leaked[1..], &[]), Laughing(secs, true)));
    }
}

/// Children and teens short of fun go and play at a court on the lot (or join a game going),
/// now and then.
#[allow(clippy::type_complexity)]
fn autonomy(
    clock: Res<crate::clock::GameClock>,
    courts: Res<Courts>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &crate::sim::Motives, &Transform), (With<crate::sim::HouseholdMember>, Without<crate::visit::OnLot>)>,
    objects: Query<(Entity, &GameObject, &Transform), Without<crate::visit::LotObject>>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 45.0;
    if std::env::var_os("HOPSCOTCH_TEST").is_some() {
        return;
    }
    let mut rng = rand::rng();
    for (e, sim, mut q, m, tf) in &mut sims {
        if !q.0.is_empty() || !matches!(sim.age, Age::Child | Age::Teen) || m.0[crate::sim::FUN] > 40.0 || !rng.random_bool(0.5) {
            continue;
        }
        let near = objects.iter().filter(|(c, o, _)| o.kind == ObjectKind::Hopscotch && courts.queue_len(*c) < MAX_PLAYERS).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
        let Some((c, _, ctf)) = near else { continue };
        let (at, face) = wait_spot(ctf, courts.queue_len(c));
        let label = if courts.queue_len(c) == 0 { PRACTICE } else { JOIN };
        q.0.push_back(Action::new(label, ActionKind::Hopscotch { court: c, turn: false, at, face, with: None }, true));
        info!("hopscotch: {} goes to play of their own accord", sim.first);
    }
}

/// `HOPSCOTCH_TEST=1` (tests): ten seconds in, a hopscotch court put down before the selected
/// Sim, who plays on it, with `HOPSCOTCH_WITH=<first name>` that Sim of the household too.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn hopscotch_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sel: Query<(Entity, &Transform, &mut ActionQueue), With<crate::sim::Selected>>,
    sims: Query<(Entity, &Sim)>,
    courts_q: Query<(Entity, &GameObject, &Transform), Without<crate::sim::Selected>>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
    grid: Option<Res<crate::nav::NavGrid>>,
) {
    if std::env::var_os("HOPSCOTCH_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let Ok((me, tf, mut q)) = sel.single_mut() else { return };
    let court = courts_q.iter().find(|(_, o, _)| o.kind == ObjectKind::Hopscotch);
    if let (Ok(mut c), Some((_, _, ctf))) = (cams.single_mut(), court) {
        let side = (ctf.rotation * Vec3::X).xz();
        c.look_at(ctf.translation);
        c.yaw = side.x.atan2(side.y);
        c.pitch = 0.8;
        c.distance = 8.0;
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("rugHopscotchTraditional")).map(|c| c.objd) else { return };
            // (On open ground near them: the court, its end and the places beside it free.)
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let clear = |c: Vec2| {
                let t = Transform::from_xyz(c.x, 0.0, c.y);
                (-3..=3).all(|i| (-1..=1).all(|j| free(c + Vec2::new(j as f32 * 0.9, i as f32 * 0.8)))) && (0..4).all(|i| free(wait_spot(&t, i).0)) && free(start_spot(&t).0)
            };
            let here = tf.translation.xz();
            let spot = (2..14).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32)).find(|c| clear(*c)).unwrap_or(here + Vec2::new(0.0, 5.0));
            let p = Vec3::new(spot.x, tf.translation.y, spot.y);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("hopscotch test: a court at {p:.1}");
            }
        }
        1 if now - stage.1 > 2.0 => {
            let Some((c, _, ctf)) = court else { return };
            *stage = (2, now);
            // (HOPSCOTCH_SKILL=<level>: everyone that good at it, to see a game won.)
            if let Some(l) = std::env::var("HOPSCOTCH_SKILL").ok().and_then(|v| v.parse::<f32>().ok()) {
                commands.queue(move |w: &mut World| {
                    let mut q = w.query::<&mut crate::interact::Skills>();
                    for mut s in q.iter_mut(w) {
                        s.0.insert("Hopscotch", l);
                        s.0.insert("Athletic", l);
                    }
                });
            }
            let with = std::env::var("HOPSCOTCH_WITH").ok().and_then(|n| sims.iter().find(|(x, s)| *x != me && s.first.eq_ignore_ascii_case(&n)).map(|(x, _)| x));
            let other = with.map(|o| {
                let (at, face) = wait_spot(ctf, 1);
                (o, at, face)
            });
            let (at, face) = wait_spot(ctf, 0);
            q.push_player(Action::new(if other.is_some() { PLAY_WITH } else { PRACTICE }, ActionKind::Hopscotch { court: c, turn: false, at, face, with: other }, false));
            info!("hopscotch test: playing{}", if other.is_some() { " with someone" } else { "" });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_hop_goes_by_the_skills() {
        assert_eq!(clean_chance(false, 0, 0, 0), 30.0);
        assert_eq!(clean_chance(false, 10, 0, 10), 95.0);
        assert_eq!(clean_chance(true, 10, 3, 5), 75.0, "a child's own athletics, not the grown-ups'");
        assert_eq!(hop_clips(Age::Child, "six", false, true), ["c2o_Hopscotch_throw_six_x".to_string(), "c2o_Hopscotch_failReturning_six_x".to_string()]);
        assert_eq!(hop_clips(Age::Teen, "three", true, false)[1], "a2o_Hopscotch_success_three_x");
    }

    #[test]
    fn a_game_is_won_once_everyone_has_hopped() {
        let (a, b) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let mut c = Court::default();
        c.queue.extend([a, b]);
        assert_eq!(c.turn_done(a, true), None, "b's still to hop");
        assert_eq!(c.queue, [b, a]);
        assert_eq!(c.turn_done(b, false), Some(vec![a]));
        assert!(c.over);
        let mut solo = Court::default();
        solo.queue.push_back(a);
        assert_eq!(solo.turn_done(a, true), None, "no one to beat: practice");
        assert!(!solo.over);
        let mut nobody = Court::default();
        nobody.queue.extend([a, b]);
        nobody.turn_done(a, false);
        assert_eq!(nobody.turn_done(b, false), None, "no winner: round again");
        assert!(nobody.hopped.is_empty());
    }

    #[test]
    fn the_court_offers_what_the_game_does() {
        let (court, me, kid) = (Entity::from_raw_u32(7).unwrap(), Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let mut courts = Courts::default();
        let spot = (Vec2::ZERO, Vec2::Y);
        let mut subs = Vec::new();
        let none = options(court, &courts, me, spot, &[], &mut subs, |_| ActionKind::Dismount);
        assert_eq!(none.iter().map(|o| o.0.as_str()).collect::<Vec<_>>(), [PRACTICE, PLAY_WITH]);
        assert!(matches!(none[1].1, ActionKind::Greyed(_)), "no one to invite");
        courts.0.entry(court).or_default().queue.extend([kid]);
        let some = options(court, &courts, me, spot, &[(kid, "Kid".into(), spot)], &mut subs, |_| ActionKind::Dismount);
        assert_eq!(some[0].0, JOIN);
        courts.0.get_mut(&court).unwrap().queue.extend([Entity::from_raw_u32(3).unwrap(), Entity::from_raw_u32(4).unwrap(), Entity::from_raw_u32(5).unwrap()]);
        let full = options(court, &courts, me, spot, &[], &mut subs, |_| ActionKind::Dismount);
        assert!(matches!(full[0].1, ActionKind::Greyed(_)), "four's the most");
    }
}
