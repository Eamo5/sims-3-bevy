//! Chess, as the game's `ChessTable`: played sitting in one of the two dining chairs slotted into
//! the table (`DiningChair_Containment_0`, white's, before it, and `_1`, black's, behind it),
//! alone ("Practice Chess") or against whoever has the other chair ("Play Chess With..."). A move
//! every `kMinutesToMove` (12, give or take 2), white first, each side's reached across from its
//! own side of the board (the game's `_otherSide` clips for black), the one waiting now and then
//! reacting to the position (`kChanceToReact`). Five moves in it's decided, as the game's
//! `ShouldXWin` weighs the two (Logic times `kScoreSkillMultiplier`, a Genius two better, the
//! Absent-Minded two worse, Lucky and Unlucky likewise; alone, a coin's toss): the winner's win,
//! the loser's applause or knocked-over king (as the two get on), the board cleared and set again
//! by the loser, and on to the next. Someone to play is twice the fun and the Logic
//! (`kPlayWithFunMultiplier`, `kPlayWithLogicMultiplier`), three times the Logic against a Grand
//! Master, and a little liking each game (`kLTRimprovementForPlayingWithSomeone`). Anyone else may
//! "Watch Chess Match" from beside the table, a friend of the winner's (liking over
//! `kLikingToReactToWin`) cheering them.
//!
//! Ranked chess, by the game's Logic tuning: a Sim plays ranked matches at a chess table against
//! an opponent of their own rank (whose chess skill rises with it, `1, 2, 3, 4, 5, 6`), winning
//! by Logic skill and a little luck, and climbs from Unranked to Grand Master after
//! `2, 4, 8, 12, 15` wins. Their record shows under Logic in the Skills tab, is saved, and a
//! Grand Master who has mastered Logic is a Chess Legend.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, Notifications, ObjectKind, Phase, Skills, Special, UsedBy, interactions_for};
use crate::life::{LifeEvent, LifeEventKind, Trait};
use crate::sim::{FUN, Motives, Sim};
use crate::social::Relationships;

pub struct ChessPlugin;

impl Plugin for ChessPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChessTables>()
            .add_systems(Update, (take_seats.before(crate::interact::run_actions), play_chess.after(crate::interact::run_actions), ranked_matches, watch_test).run_if(in_state(PlayMode::Live)));
    }
}

/// The ranks (the game's names), and the wins that reach each one after the first.
pub const RANKS: [&str; 6] = ["Unranked", "Apprentice (Rank 1)", "Tenderfoot (Rank 2)", "Journeyman (Rank 3)", "Instructor (Rank 4)", "Grand Master (Rank 5)"];
const WINS_FOR_RANK: [u32; 5] = [2, 4, 8, 12, 15];
/// The chess skill of an opponent at each rank.
const OPPONENT_SKILL: [f32; 6] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];

/// How the game names playing someone ("Play Chess With Bella Goth"), and its menu of them.
pub const PLAY_WITH: &str = "Play Chess With";
/// Playing alone.
pub const PRACTICE: &str = "Practice Chess";
/// Why it can't be played (the game's `NotUseable` and `NotUseableTableFull`).
const NO_CHAIR: &str = "Must have a chair to use.";
const TABLE_FULL: &str = "The chess table is currently full";

/// `kMinutesToMove`: a move every this many minutes, give or take two.
const MINUTES_TO_MOVE: f64 = 12.0;
/// The moves a game's decided after (the game's geometry states, `mCurrentGeometryState < 5`).
const MOVES_A_GAME: u8 = 5;
/// `kChanceToReact`: the one waiting reacts to the board.
const CHANCE_TO_REACT: f64 = 0.25;
/// `kScoreSkillMultiplier`, and the traits' bumps (`kGeniusTraitScoreBump`...).
const SCORE_SKILL_MULTIPLIER: f32 = 10.0;
const TRAIT_BUMP: f32 = 2.0;
/// `kLTRimprovementForPlayingWithSomeone`.
const LIKING_FOR_A_GAME: f32 = 3.0;
/// `kLikingToReactToWin`: a watcher who likes the winner this much cheers.
const LIKING_TO_CHEER: f32 = 30.0;
/// `kPlayWithFunMultiplier` and `kPlayWithLogicMultiplier` (added to the one alone), and
/// `kLogicSkillGainRateWithChessMasterOpponent`.
const PLAY_WITH_BONUS: f32 = 1.0;
const AGAINST_GRAND_MASTER: f32 = 3.0;
/// Time for the end of a game's clips before the next one's first move (or getting up).
const BETWEEN_GAMES: f64 = 6.0;
const ENDING_MINUTES: f32 = 8.0;

/// The clips, each side's (white's, then black's from the other side of the board).
const IDLE: &[&str] = &["a2o_chessTable_loop"];
const MOVES: [[&str; 3]; 2] = [
    ["a2o_chessTable_move1_x", "a2o_chessTable_move2_x", "a2o_chessTable_move3_x"],
    ["a2o_chessTable_move1_otherSide_x", "a2o_chessTable_move2_otherSide_x", "a2o_chessTable_move3_otherSide_x"],
];
const REACTS: [&str; 3] = ["a2o_chessTable_react1_x", "a2o_chessTable_react2_x", "a2o_chessTable_react3_x"];
const WIN: [&str; 2] = ["a2o_chessTable_win_x", "a2o_chessTable_win_otherSide_x"];
/// Losing well (applauding the winner) and badly (knocking a piece over: the game's
/// `Lose_Positive` and `Lose_Negative`).
const LOSE_WELL: [&str; 2] = ["a2o_chessTable_lose1_x", "a2o_chessTable_lose1_otherSide_x"];
const LOSE_BADLY: [&str; 2] = ["a2o_chessTable_lose2_x", "a2o_chessTable_lose2_otherSide_x"];
/// The board cleared and set again.
const RESET: [&[&str]; 2] = [&["a2o_chessTable_clear_x", "a2o_chessTable_set_x"], &["a2o_chessTable_clear_otherSide_x", "a2o_chessTable_set_otherSide_x"]];
/// A watcher's cheer, and their watching.
const CHEER: &str = "a_react_cheer_x";
pub const WATCHING: &[&str] = &["a_wedding_watching_loop_x", "a_idle_neutral_look_upAndOver_x"];

/// A Sim's ranked chess record.
#[derive(Component, Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ChessRecord {
    pub wins: u32,
    pub losses: u32,
    pub rank: u8,
}

impl ChessRecord {
    pub fn rank_name(&self) -> &'static str {
        RANKS[(self.rank as usize).min(RANKS.len() - 1)]
    }
}

/// A ranked match just finished: won or lost.
#[derive(Component)]
pub struct MatchPlayed(pub bool);

/// Where a Sim plays: the table, the chair they have at it, and whether it's white's.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChessSeat {
    pub table: Entity,
    pub chair: Entity,
    pub white: bool,
}

/// A game under way at a table.
#[derive(Clone, Debug, Default)]
pub struct ChessGame {
    /// Who's playing, white then black.
    pub players: [Option<Entity>; 2],
    /// The side to move.
    to_move: usize,
    /// When the next move's made (game minutes).
    next_move: f64,
    moves: u8,
    /// Whether white wins this one (decided as it begins, as the game's `SelectNewChessGame`).
    white_wins: bool,
    /// Each side's clips, and the watchers', played turn about (see `once`).
    flip: [bool; 3],
}

/// The chess tables: their games, and how many chairs each has free.
#[derive(Resource, Default)]
pub struct ChessTables {
    pub games: HashMap<Entity, ChessGame>,
    pub free: HashMap<Entity, usize>,
}

impl ChessTables {
    /// Who's playing at a table.
    pub fn players(&self, table: Entity) -> Vec<Entity> {
        self.games.get(&table).map(|g| g.players.iter().flatten().copied().collect()).unwrap_or_default()
    }
    /// Whether a Sim's playing at any table.
    pub fn playing(&self, sim: Entity) -> bool {
        self.games.values().any(|g| g.players.contains(&Some(sim)))
    }
    /// The chairs free at a table (none, with none at it).
    pub fn free_chairs(&self, table: Entity) -> usize {
        self.free.get(&table).copied().unwrap_or(0)
    }
    /// Whether a Sim could watch the game at a table (someone's playing, and it's not them: the
    /// game's `CanSimWatchCurrentGame`).
    pub fn watchable(&self, table: Entity, sim: Entity) -> bool {
        let players = self.players(table);
        !players.is_empty() && !players.contains(&sim)
    }
}

/// The chess table's chairs (the game's containment slots, 0.85 m either side of it): chairs
/// there facing it, white's on its front (+Z) side, black's behind.
pub fn chairs_at<'a>(table: &Transform, chairs: impl Iterator<Item = (Entity, &'a GameObject, &'a Transform)>) -> Vec<(Entity, bool)> {
    let inverse = table.compute_affine().inverse();
    chairs
        .filter(|(_, o, _)| o.kind == ObjectKind::Chair)
        .filter_map(|(e, _, t)| {
            let local = inverse.transform_point3(t.translation);
            let facing = (t.rotation * Vec3::Z).with_y(0.0).normalize_or_zero();
            let to_table = (table.translation - t.translation).with_y(0.0).normalize_or_zero();
            let placed = local.x.abs() < 0.35 && (0.45..1.3).contains(&local.z.abs()) && local.y.abs() < 0.5 && facing.dot(to_table) > 0.7;
            placed.then_some((e, local.z > 0.0))
        })
        .collect()
}

/// The game's `GetScoreForSim`: Logic times ten, two more for a Genius, two less for the
/// Absent-Minded, two either way for luck.
pub fn score(logic: u32, traits: &[Trait]) -> f32 {
    let has = |t: Trait| traits.contains(&t);
    let mut s = logic as f32 * SCORE_SKILL_MULTIPLIER;
    if has(Trait::Genius) {
        s += TRAIT_BUMP;
    }
    if has(Trait::Absentminded) {
        s -= TRAIT_BUMP;
    }
    if has(Trait::Lucky) {
        s += TRAIT_BUMP;
    }
    if has(Trait::Unlucky) {
        s -= TRAIT_BUMP;
    }
    s
}

/// `ShouldXWin`: each side's score with the bumps' worth added (so none's below nothing), the
/// chance of winning in proportion to it.
pub fn win_chance(mine: f32, theirs: f32) -> f32 {
    let (a, b) = ((mine + 2.0 * TRAIT_BUMP).max(0.0), (theirs + 2.0 * TRAIT_BUMP).max(0.0));
    if a + b <= 0.0 { 0.5 } else { a / (a + b) }
}

/// A ranked opponent's chance of losing to a Sim: their Logic against the rank's chess skill
/// (a Genius plays a little better).
pub fn ranked_chance(record: Option<&ChessRecord>, logic: u32, traits: &[Trait]) -> f64 {
    let rank = record.map_or(0, |r| r.rank as usize).min(5);
    let opponent = OPPONENT_SKILL[rank] * 1.6;
    let mine = logic as f32 + if traits.contains(&Trait::Genius) { 1.5 } else { 0.0 };
    (0.5 + (mine - opponent) * 0.1).clamp(0.05, 0.95) as f64
}

/// A clip once, then others in order, then the game's idle: put in turn about as the start and
/// as what's before it, so the same clip twice running is played again (a new script).
fn once(first: &'static str, then: &'static [&'static str], loops: &'static [&'static str], flip: &mut bool) -> ActionClip {
    let mut c = ActionClip::steps(first, then, loops);
    *flip = !*flip;
    if *flip {
        c.before = c.start.take();
    }
    c
}

/// Chess is played from a chair at the table: the one free nearest the Sim (with none, or none
/// free, the game's why not), named for whoever's at the other.
#[allow(clippy::type_complexity)]
fn take_seats(
    mut commands: Commands,
    mut sims: Query<(Entity, &Transform, &mut ActionQueue, Option<&ChessSeat>), With<Sim>>,
    names: Query<&Sim>,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
    mut notes: ResMut<Notifications>,
) {
    // (The chairs others have, or are on their way to.)
    let mut taken: Vec<(Entity, Entity)> = sims.iter().filter_map(|s| s.3.map(|c| (s.0, c.chair))).collect();
    for (me, tf, mut queue, _) in &mut sims {
        let Some(action) = queue.0.front_mut() else { continue };
        let ActionKind::Object { target, def } = action.kind else { continue };
        if action.cancel || matches!(action.phase, Phase::Running(_)) {
            continue;
        }
        let Ok((_, obj, otf, _)) = objects.get(target) else { continue };
        if obj.kind != ObjectKind::Chess {
            continue;
        }
        let Some(special) = interactions_for(obj.kind).get(def).map(|d| d.special).filter(|s| matches!(s, Special::PlayChess | Special::ChessMatch)) else { continue };
        let Some(seated) = interactions_for(ObjectKind::Chair).iter().position(|d| d.special == special) else { continue };
        let chairs = chairs_at(otf, objects.iter().map(|(e, o, t, _)| (e, o, t)));
        let free: Vec<(Entity, bool)> = chairs
            .iter()
            .copied()
            .filter(|(c, _)| objects.get(*c).is_ok_and(|x| x.3.0.is_none_or(|u| u == me)) && !taken.iter().any(|(who, t)| *who != me && t == c))
            .collect();
        let why = if chairs.is_empty() {
            Some(NO_CHAIR)
        } else {
            free.is_empty().then_some(TABLE_FULL)
        };
        if let Some(why) = why {
            if !action.autonomous {
                notes.push(why.to_string());
            }
            action.cancel = true;
            continue;
        }
        let near = |c: Entity| objects.get(c).map_or(f32::MAX, |x| x.2.translation.distance(tf.translation));
        let Some((chair, white)) = free.into_iter().min_by(|a, b| near(a.0).total_cmp(&near(b.0))) else { continue };
        // (Named as the game names it: "Practice Chess", or, with someone at the other chair,
        // "Play Chess With" them.)
        let other = taken.iter().find(|(who, c)| *who != me && chairs.iter().any(|x| x.0 == *c)).and_then(|(who, _)| names.get(*who).ok());
        if special == Special::PlayChess && !action.label.starts_with(PLAY_WITH) {
            action.label = match other {
                Some(o) => format!("{PLAY_WITH} {}", o.full_name()),
                None => PRACTICE.to_string(),
            };
        }
        debug!("chess: {:?} sits at {:?} ({})", me, chair, if white { "white" } else { "black" });
        if matches!(action.phase, Phase::Routing) {
            commands.entity(me).remove::<crate::nav::PathFollow>();
        }
        action.kind = ActionKind::Object { target: chair, def: seated };
        action.phase = Phase::Start;
        commands.entity(me).insert(ChessSeat { table: target, chair, white });
        taken.retain(|(w, _)| *w != me);
        taken.push((me, chair));
    }
}

/// What's going on at a table: who's playing and from which side, what at, and who's watching.
struct Player {
    sim: Entity,
    side: usize,
    ranked: bool,
    logic: u32,
    traits: Vec<Trait>,
    record: Option<ChessRecord>,
}

/// The games at the tables: the seats taken, the moves made in turn, the game decided and the
/// next begun, the watchers cheering; and someone to play the more fun, and the more Logic.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn play_chess(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    delta: Res<crate::clock::SimDelta>,
    mut tables: ResMut<ChessTables>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Transform, &mut Motives, &mut Skills, &mut Relationships, Option<&ChessSeat>, Option<&ChessRecord>), Without<GameObject>>,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
    mut notes: ResMut<Notifications>,
    mut life: MessageWriter<LifeEvent>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    // (Each table's free chairs: for the menus, and autonomy.)
    let claimed: Vec<Entity> = sims.iter().filter_map(|s| s.7.map(|c| c.chair)).collect();
    let mut free = HashMap::new();
    for (table, o, ttf, _) in &objects {
        if o.kind != ObjectKind::Chess {
            continue;
        }
        let chairs = chairs_at(ttf, objects.iter().map(|(e, o, t, _)| (e, o, t)));
        let n = chairs.iter().filter(|(c, _)| objects.get(*c).is_ok_and(|x| x.3.0.is_none()) && !claimed.contains(c)).count();
        free.insert(table, n);
    }
    tables.free = free;
    // Who's sat playing (or on their way), and who's watching.
    let mut playing: HashMap<Entity, Vec<Player>> = HashMap::new();
    let mut watching: Vec<(Entity, Entity)> = Vec::new();
    for (me, sim, queue, _, _, skills, _, seat, record) in &sims {
        let front = queue.0.front();
        if let Some(seat) = seat {
            let at = front.filter(|a| !a.cancel).and_then(|a| match a.kind {
                ActionKind::Object { target, def } if target == seat.chair => Some((a.phase, interactions_for(ObjectKind::Chair).get(def).map(|d| d.special))),
                _ => None,
            });
            match at {
                Some((Phase::Running(_), Some(s @ (Special::PlayChess | Special::ChessMatch)))) => {
                    playing.entry(seat.table).or_default().push(Player {
                        sim: me,
                        side: if seat.white { 0 } else { 1 },
                        ranked: s == Special::ChessMatch,
                        logic: skills.level("Logic"),
                        traits: sim.traits.clone(),
                        record: record.copied(),
                    });
                }
                Some(_) => {}
                None => {
                    commands.entity(me).remove::<ChessSeat>();
                }
            }
        }
        if let Some(a) = front
            && let ActionKind::Object { target, def } = a.kind
            && matches!(a.phase, Phase::Running(_))
            && objects.get(target).is_ok_and(|o| o.1.kind == ObjectKind::Chess && interactions_for(o.1.kind).get(def).is_some_and(|d| d.special == Special::WatchChess))
        {
            watching.push((me, target));
        }
    }
    // (Nothing to watch at a table no one's playing at; and the table looked at.)
    for (me, table) in &watching {
        let Ok((_, _, ttf, _)) = objects.get(*table) else { continue };
        let Ok((_, _, mut queue, mut tf, ..)) = sims.get_mut(*me) else { continue };
        if !playing.contains_key(table) {
            if let Some(a) = queue.0.front_mut() {
                a.cancel = true;
            }
            continue;
        }
        let to = (ttf.translation - tf.translation).with_y(0.0);
        if to.length_squared() > 1e-4 {
            tf.rotation = Quat::from_rotation_y(to.x.atan2(to.z));
        }
    }
    tables.games.retain(|t, _| playing.contains_key(t));
    let dt = delta.0;
    for (table, players) in playing {
        let game = tables.games.entry(table).or_default();
        let mut seated: [Option<Entity>; 2] = [None, None];
        for p in &players {
            seated[p.side] = seated[p.side].or(Some(p.sim));
        }
        let by_side = |side: usize| players.iter().find(|p| Some(p.sim) == seated[side]);
        // (Someone new sat down: a new game, decided as it begins.)
        let joined = seated.iter().zip(game.players.iter()).any(|(now, was)| now.is_some() && now != was);
        if joined {
            game.white_wins = match (by_side(0), by_side(1)) {
                (Some(w), Some(b)) => rng.random::<f32>() < win_chance(score(w.logic, &w.traits), score(b.logic, &b.traits)),
                (Some(p), None) | (None, Some(p)) if p.ranked => rng.random_bool(ranked_chance(p.record.as_ref(), p.logic, &p.traits)) == (p.side == 0),
                _ => rng.random_bool(0.5),
            };
            game.moves = 0;
            game.to_move = 0;
            game.next_move = now + MINUTES_TO_MOVE + rng.random_range(-2.0..2.0);
            debug!("chess: a new game at {table:?}, {:?} ({} wins)", seated, if game.white_wins { "white" } else { "black" });
        }
        game.players = seated;
        let two = seated.iter().all(|s| s.is_some());
        // (Someone to play: the more fun, and the more Logic, three times as much against a
        // Grand Master.)
        if two && dt > 0.0 {
            for side in 0..2 {
                let (Some(p), Some(o)) = (by_side(side), by_side(1 - side)) else { continue };
                let master = o.record.is_some_and(|r| r.rank as usize >= RANKS.len() - 1);
                let extra = PLAY_WITH_BONUS + if master { AGAINST_GRAND_MASTER - 1.0 } else { 0.0 };
                let Ok((_, sim, _, _, mut motives, mut skills, ..)) = sims.get_mut(p.sim) else { continue };
                let fun = interactions_for(ObjectKind::Chair).iter().find(|d| d.special == Special::PlayChess).map_or(0.0, |d| d.per_hour[FUN]);
                motives.add(FUN, fun * PLAY_WITH_BONUS * dt / 60.0);
                let points = dt / 60.0 * crate::interact::practice_points("Play Chess") * crate::life::skill_rate(&sim.traits, "Logic") * extra;
                if let Some(level) = skills.add_points("Logic", points) {
                    notes.push(format!("{} reached level {level} in Logic!", sim.first));
                    life.write(LifeEvent::new(p.sim, LifeEventKind::SkillUp { skill: "Logic", level }));
                }
            }
        }
        if now < game.next_move {
            continue;
        }
        if game.moves < MOVES_A_GAME {
            // A move: the side to move's, or every one, alone; the other now and then reacting.
            let side = if two { game.to_move } else if seated[0].is_some() { 0 } else { 1 };
            if let Some(mover) = seated[side] {
                let clip = once(MOVES[side][rng.random_range(0..3)], &[], IDLE, &mut game.flip[side]);
                commands.entity(mover).insert(clip);
            }
            if two
                && rng.random_bool(CHANCE_TO_REACT)
                && let Some(other) = seated[1 - side]
            {
                let clip = once(REACTS[rng.random_range(0..3)], &[], IDLE, &mut game.flip[1 - side]);
                commands.entity(other).insert(clip);
            }
            game.moves += 1;
            game.to_move = 1 - game.to_move;
            game.next_move = now + MINUTES_TO_MOVE + rng.random_range(-2.0..2.0);
            continue;
        }
        // The game's over: the winner's win, the loser's taking it (and setting the board up
        // again); alone, the board set up again whichever way it went.
        let winner = if game.white_wins { 0 } else { 1 };
        let winner_sim = seated[winner];
        let mut ranked_done: Vec<(Entity, bool)> = Vec::new();
        for side in 0..2 {
            let Some(me) = seated[side] else { continue };
            let won = side == winner;
            let clip = if !two {
                // (Alone: the side they're on won or lost.)
                if by_side(side).is_some_and(|p| p.ranked) {
                    ranked_done.push((me, won));
                }
                if won {
                    once(WIN[side], RESET[side], IDLE, &mut game.flip[side])
                } else {
                    let lose = if rng.random_bool(0.5) { LOSE_WELL[side] } else { LOSE_BADLY[side] };
                    once(lose, RESET[side], IDLE, &mut game.flip[side])
                }
            } else if won {
                once(WIN[side], &[], IDLE, &mut game.flip[side])
            } else {
                // (Gracious to a friend, sore with anyone else: the game's LTR positive or not.)
                let fond = winner_sim.and_then(|w| sims.get(me).ok().map(|s| s.6.friendship(w) > 0.0)).unwrap_or(false);
                once(if fond { LOSE_WELL[side] } else { LOSE_BADLY[side] }, RESET[side], IDLE, &mut game.flip[side])
            };
            commands.entity(me).insert(clip);
        }
        if two && let (Some(a), Some(b)) = (seated[0], seated[1]) {
            for (x, y) in [(a, b), (b, a)] {
                if let Ok((_, _, _, _, _, _, mut rels, ..)) = sims.get_mut(x)
                    && rels.add(y, LIKING_FOR_A_GAME, 0.0)
                {
                    life.write(LifeEvent::new(x, LifeEventKind::MadeFriend { other: y }));
                }
            }
        }
        // (Watchers fond of the winner cheer.)
        if let Some(w) = winner_sim.filter(|_| two) {
            for (me, _) in watching.iter().filter(|(_, t)| *t == table) {
                if sims.get(*me).is_ok_and(|s| s.6.friendship(w) > LIKING_TO_CHEER) {
                    let clip = once(CHEER, &[], WATCHING, &mut game.flip[2]);
                    commands.entity(*me).insert(clip);
                }
            }
        }
        // A ranked match's played: its result, and up from the table once its ending's played.
        for (me, won) in ranked_done {
            commands.entity(me).insert(MatchPlayed(won));
            if let Ok((_, _, mut queue, ..)) = sims.get_mut(me)
                && let Some(a) = queue.0.front_mut()
                && let ActionKind::Object { def, .. } = a.kind
                && let Some(d) = interactions_for(ObjectKind::Chair).get(def)
                && let Phase::Running(t) = a.phase
            {
                a.phase = Phase::Running(t.max(d.minutes - ENDING_MINUTES));
            }
        }
        // (And the next.)
        game.white_wins = rng.random_bool(0.5);
        if let (Some(w), Some(b)) = (by_side(0), by_side(1)) {
            game.white_wins = rng.random::<f32>() < win_chance(score(w.logic, &w.traits), score(b.logic, &b.traits));
        }
        game.moves = 0;
        game.to_move = 0;
        game.next_move = now + BETWEEN_GAMES + MINUTES_TO_MOVE + rng.random_range(-2.0..2.0);
    }
}

fn ranked_matches(mut commands: Commands, mut sims: Query<(Entity, &Sim, Option<&mut ChessRecord>, &MatchPlayed)>, mut notes: ResMut<Notifications>) {
    for (e, sim, record, played) in &mut sims {
        commands.entity(e).remove::<MatchPlayed>();
        let mut r = record.as_deref().copied().unwrap_or_default();
        if played.0 {
            r.wins += 1;
            let mut text = format!("{} has defeated a ranked chess opponent", sim.first);
            if (r.rank as usize) < WINS_FOR_RANK.len() && r.wins >= WINS_FOR_RANK[r.rank as usize] {
                r.rank += 1;
                text += &format!(" and is now ranked {}!", r.rank_name());
            } else {
                text += ".";
            }
            notes.push(text);
        } else {
            r.losses += 1;
            notes.push(format!("{} has lost to a ranked chess opponent.", sim.first));
        }
        match record {
            Some(mut rec) => *rec = r,
            None => {
                commands.entity(e).insert(r);
            }
        }
    }
}

/// CHESS_WATCH=1 (tests): once a game's under way, someone else about (not a household
/// member's friend especially) goes to watch it, cheering the winner if they're fond of them
/// (CHESS_WATCH=fond: made so first, and in want of some fun).
fn watch_test(mut done: Local<bool>, tables: Res<ChessTables>, mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Relationships, &mut Motives)>) {
    let Ok(how) = std::env::var("CHESS_WATCH") else { return };
    if *done {
        return;
    }
    // (Fond of both, once both are sat down.)
    let Some((table, players)) = tables.games.iter().map(|(t, g)| (*t, g.players)).find(|(_, p)| if how == "fond" { p.iter().all(|p| p.is_some()) } else { p.iter().any(|p| p.is_some()) }) else { return };
    let Some(def) = interactions_for(ObjectKind::Chess).iter().position(|d| d.special == Special::WatchChess) else { return };
    for (e, sim, mut q, mut rels, mut motives) in &mut sims {
        if players.contains(&Some(e)) || sim.age.is_little() {
            continue;
        }
        if how == "fond" {
            for p in players.iter().flatten() {
                rels.entry(*p).friendship = 60.0;
            }
            motives.0[FUN] = -60.0;
        }
        q.0.clear();
        q.push_player(Action::new("Watch Chess Match", ActionKind::Object { target: table, def }, false));
        info!("chess watch test: {} watches", sim.first);
        *done = true;
        return;
    }
}

/// Play Chess With someone: both to the table, each to a chair of their own.
pub fn play_with(queues: &mut Query<&mut ActionQueue>, table: Entity, def: usize, actor: (Entity, &Sim), other: (Entity, &Sim)) {
    for ((who, _), (_, them)) in [(actor, other), (other, actor)] {
        if let Ok(mut q) = queues.get_mut(who) {
            q.push_player(Action::new(format!("{PLAY_WITH} {}", them.full_name()), ActionKind::Object { target: table, def }, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chairs_either_side_facing_the_table_are_its_white_and_black() {
        let object = |kind| GameObject { kind, name: "Test".into(), objd: (0, 0, 0), price: 0, center: Vec2::ZERO, half: Vec2::splat(0.3), height: 0.8, route: None };
        let table = Transform::from_xyz(5.0, 0.0, 5.0).with_rotation(Quat::from_rotation_y(0.5));
        let chair = object(ObjectKind::Chair);
        let at = |z: f32, turn: f32| {
            let p = table.transform_point(Vec3::new(0.0, 0.0, z));
            Transform::from_translation(p).with_rotation(table.rotation * Quat::from_rotation_y(turn))
        };
        // (The game's slots: white's before the table facing back across it, black's behind.)
        let white = at(0.85, std::f32::consts::PI);
        let black = at(-0.85, 0.0);
        let away = at(-0.85, std::f32::consts::PI);
        let far = at(2.5, std::f32::consts::PI);
        let mut world = World::new();
        let ids: Vec<Entity> = (0..4).map(|_| world.spawn_empty().id()).collect();
        let list = [(ids[0], &chair, &white), (ids[1], &chair, &black), (ids[2], &chair, &away), (ids[3], &chair, &far)];
        assert_eq!(chairs_at(&table, list.into_iter()), vec![(ids[0], true), (ids[1], false)]);
    }

    #[test]
    fn the_better_player_usually_wins() {
        let novice = score(0, &[]);
        let master = score(10, &[Trait::Genius]);
        assert_eq!((novice, master), (0.0, 102.0));
        assert!((win_chance(master, novice) - 106.0 / 110.0).abs() < 1e-4);
        assert_eq!(win_chance(novice, novice), 0.5);
        // (The Absent-Minded and Unlucky beginner has no chance at all against anyone.)
        assert_eq!(win_chance(score(0, &[Trait::Absentminded, Trait::Unlucky]), novice), 0.0);
    }
}
