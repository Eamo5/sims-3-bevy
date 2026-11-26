//! The board breaker, World Adventures' (`BoardBreaker`): martial artists of level two and up
//! (`UnlockBoardBreaker`) break boards on it, teens and older, from either of its sides ("Practice...
//! ›": a Foam Board, a Balsa Wood Board, an Oak Board, a Thin Stone Block or a Space Rock Block,
//! each a kind of the game's board, `Board.GetMaterialStateName`, or the space rock). In their gi,
//! they bow in (`a_neutral2MA`), set the boards out (`a2o_boardBreaker_swipe`: as many as they've
//! managed of that kind before, one at first, another after every few broken,
//! `kBoardBreaksToIncreaseStackSize`, five at most), step up (`_start`) and ready themselves (the
//! `_loop`s: ten minutes, twenty for a space rock, `kTimeToPrepareFor...`), and strike. The chance it
//! goes wrong is the game's (`ComputeChanceOfFailure`): thirty in a hundred at the level the board
//! comes at, falling away the further past it they are (`kFailureDecayConstant`) and rising towards
//! a hundred the further short, more when their mood's low, less for the Disciplined and a Timber
//! Terminator; past the board's mark (`kEpicFailureChance`) it's a hurt hand (`_epicFail`, Hurt
//! Hand!) and they're done, else a miss (`_fail`, the board's own thud, `bbreaker_fail_...`) or the
//! boards splitting (`_break`: flying to bits with the game's crack, `fx_bbreaker_break_...`). They
//! learn Martial Arts as the board's tough (`kSkillGainRateToPractice`) and Athletic, Hygiene going
//! down; fifty Oak boards or tougher broken make them a Timber Terminator, who now and then finds a
//! gem splitting a space rock. "Change Board... ›" swaps to another kind there and then; the
//! Fatigued, those with a hurt hand and those not yet skilled enough can't (greyed, the game's
//! words). Others about may "Watch Martial Artist" (Fun 30 an hour, liking 2 every quarter hour,
//! `kMinutesForLikingGain`), clapping or cheering a break and laughing or booing a miss as the game
//! has them (`kBaseChanceToCheerOrBooWhenBoardBreaks`, the Friendly, Daredevil and Athletic keener to
//! cheer, the Mean-Spirited and Evil to boo). The skilled practise of their own accord now and then.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Skills};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct BoardBreakerPlugin;

impl Plugin for BoardBreakerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Breakers>()
            .add_message::<Struck>()
            .add_systems(Update, hold_on.before(crate::interact::run_actions).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (run, watch, autonomy, boards_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)));
    }
}

/// The kinds of board, softest first (the game's `BoardType`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Board {
    Foam,
    Balsa,
    Oak,
    ThinStone,
    SpaceRock,
}

impl Board {
    pub const ALL: [Board; 5] = [Board::Foam, Board::Balsa, Board::Oak, Board::ThinStone, Board::SpaceRock];

    fn i(self) -> usize {
        self as usize
    }

    /// The game's name for it (`BoardBreaker:FoamBoard`...).
    pub fn name(self) -> &'static str {
        ["Foam Board", "Balsa Wood Board", "Oak Board", "Thin Stone Block", "Space Rock Block"][self.i()]
    }

    /// The Martial Arts level it comes at (the `Skills` table's `ToughestBoardType`).
    pub fn level(self) -> u8 {
        [2, 4, 6, 8, 10][self.i()]
    }

    /// Its look: the board's material state (none: the space rock, a model of its own).
    fn state(self) -> Option<u32> {
        use s3formats::model::*;
        [Some(STATE_FOAM), Some(STATE_BALSA), Some(STATE_OAK), Some(STATE_STONE), None][self.i()]
    }

    /// Its thud when it won't break (`kBoardBreakFailSoundName`).
    fn fail_sound(self) -> &'static str {
        ["bbreaker_fail_foam", "bbreaker_fail_balsa", "bbreaker_fail_oak", "bbreaker_fail_stonea", "bbreaker_fail_stoneb"][self.i()]
    }

    /// Its crack as a stack of them splits (the break effects' sounds: more for a bigger stack).
    fn break_sound(self, stack: u8) -> String {
        if self == Board::SpaceRock {
            return "fx_bbreaker_break_stone".into();
        }
        let kind = ["foam", "balsa", "oak", "stone"][self.i()];
        let n = ['a', 'b', 'c', 'd'][(stack.max(1) - 1).min(3) as usize];
        format!("fx_bbreaker_break_{kind}_{n}")
    }

    /// The colour of its pieces.
    fn color(self) -> Color {
        [Color::srgb(0.93, 0.9, 0.78), Color::srgb(0.86, 0.75, 0.55), Color::srgb(0.55, 0.38, 0.22), Color::srgb(0.6, 0.6, 0.58), Color::srgb(0.35, 0.3, 0.38)][self.i()]
    }

    /// How long they ready themselves for it (game minutes).
    fn prep(self) -> f64 {
        if self == Board::SpaceRock { PREP_SPACE } else { PREP_REGULAR }
    }

    /// Its catalogue model.
    fn model(self) -> &'static str {
        if self == Board::SpaceRock { "BoardBreakerSpaceRock" } else { "BoardBreakerBoard" }
    }

    /// Its journal key, for the most broken at once.
    fn stack_key(self) -> String {
        format!("board_stack:{}", self.i())
    }
}

/// The game's tuning (the XML's, over the script's): the mood below which they're likelier to
/// miss and by how much each point; the miss chance at the board's level and at its worst, its
/// falling away past it; a Timber Terminator's and the Disciplined's better chance; how long they
/// ready themselves; the miss chances past which it's a hurt hand; the space rock's extra; breaks
/// to another board on the stack; skill points a minute by the board, Athletic's; the audience's
/// chances, traits and likings; liking now and then; boards broken to be a Timber Terminator.
const MIN_MOOD: f32 = 0.0;
const PER_MOOD: f32 = 0.5;
const MIN_FAIL: f32 = 30.0;
const MAX_FAIL: f32 = 100.0;
const DECAY: f32 = 0.3;
const ADVANCED_BONUS: f32 = 25.0;
const DISCIPLINED_BONUS: f32 = 10.0;
const PREP_REGULAR: f64 = 10.0;
const PREP_SPACE: f64 = 20.0;
const EPIC: [f32; 5] = [80.0, 40.0, 46.0, 50.0, 42.0];
const SPACE_EXTRA: f32 = 20.0;
const BREAKS_TO_GROW: [u8; 5] = [2, 3, 4, 5, 1];
const MOST_BOARDS: u8 = 5;
const SKILL_PER_MINUTE: [f32; 5] = [10.0, 12.0, 15.0, 20.0, 26.0];
const ATHLETIC_PER_MINUTE: f32 = 3.0;
const HYGIENE_PER_HOUR: f32 = -40.0;
const DISCIPLINED_FUN: f32 = 20.0;
const WATCH_FUN: f32 = 30.0;
const CHEER_BASE: f32 = 50.0;
const TRAIT_BONUS: f32 = 10.0;
const LTR_BONUS: f32 = 20.0;
const LTR_CHEER: f32 = 25.0;
const LTR_BOO: f32 = -20.0;
const LIKING_EVERY: f64 = 15.0;
const LIKING_GAIN: f32 = 2.0;
const ADVANCED_BOARDS: f64 = 50.0;
const UNLOCK_LEVEL: f32 = 2.0;
/// A Timber Terminator's gems from a space rock (`kMaximumNumberOfGemsAwarded`, `kChanceToGive...`,
/// `kGemPriceThreshold...`).
const MOST_GEMS: u32 = 3;
const NO_GEM: f32 = 30.0;
const LOW_GEM: f32 = 80.0;
const MEDIUM_GEM: f32 = 95.0;
const GEM_LOW_MEDIUM: i32 = 50;
const GEM_MEDIUM_HIGH: i32 = 200;

/// Where its sides are (its `_route_0_N` and `_route_0_S`, model space), and the boards between
/// (`_ctnm_N_board`, a board's breadth apart, the space rock's `_ctnm_0_stone`).
const SIDE_NEAR: f32 = 0.404562;
const SIDE_FAR: f32 = -0.895;
const BOARDS_AT: Vec3 = Vec3::new(0.0, 0.59504, -0.245);
const BOARD_STEP: f32 = 0.025;
const STONE_Y: f32 = 0.662;

/// The clips: bowing in and out, setting the boards out, stepping up, readying, and the strike.
const BOW_IN: &str = "a_neutral2MA_x";
const BOW_OUT: &str = "a_MA2neutral_x";
const SWIPE: &str = "a2o_boardBreaker_swipe_x";
const STEP_UP: &str = "a2o_boardBreaker_start_x";
const READY: &[&str] = &["a2o_boardBreaker_loop"];
const BREAK: &str = "a2o_boardBreaker_break_x";
const MISS: &str = "a2o_boardBreaker_fail_x";
const HURT: &str = "a2o_boardBreaker_epicFail_x";
const STEP_BACK: &str = "a2o_boardBreaker_stop_x";
/// When the clips have the boards out (`Board_appear`), split (`Board_break`) and thud
/// (`failure`), seconds in.
const BOARDS_OUT_AT: f32 = 0.4;
const SPLIT_AT: f32 = 0.767;
const MISS_AT: f32 = 0.733;
const HURT_AT: f32 = 2.633;
/// The watchers' clips (the game's `watch_martial_artist`).
const WATCHING: &[&str] = &["a_fire_watch"];

/// The game's words.
pub const PRACTICE: &str = "Practice...";
pub const CHANGE_BOARD: &str = "Change Board...";
pub const WATCH: &str = "Watch Martial Artist";

/// Who's breaking boards on which breaker, and what kind.
#[derive(Resource, Default)]
pub struct Breakers(pub HashMap<Entity, (Entity, Board)>);

/// How a strike went.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Strike {
    Success,
    Miss,
    Hurt,
}

/// A strike on a breaker, for those watching.
#[derive(Message, Clone, Copy)]
pub struct Struck {
    pub breaker: Entity,
    pub how: Strike,
}

/// The toughest board a level breaks (`GetMostCompatibleBoard`).
pub fn most_compatible(level: f32) -> Option<Board> {
    Board::ALL.into_iter().rev().find(|b| level.floor() >= b.level() as f32)
}

/// The chance in a hundred a strike goes wrong (`ComputeChanceOfFailure` less
/// `ComputeChanceOfSuccess`).
pub fn miss_chance(board: Board, level: f32, mood: f32, advanced: bool, disciplined: bool) -> f32 {
    let past = level.floor() as i32 - board.level() as i32;
    let base = if past > 0 { MIN_FAIL / (past as f32 * DECAY).exp() } else { MIN_FAIL + (MAX_FAIL - MIN_FAIL) * (past.abs() as f32 / 10.0) };
    let moody = if mood < MIN_MOOD { (MIN_MOOD - mood) * PER_MOOD } else { 0.0 };
    let better = if advanced { ADVANCED_BONUS } else { 0.0 } + if disciplined { DISCIPLINED_BONUS } else { 0.0 };
    (base + moody - better).clamp(0.0, 100.0)
}

/// How a strike goes, by its chance and a roll (0 to 100: `GetStrikeResult`).
pub fn strike(board: Board, chance: f32, roll: f32) -> Strike {
    if chance >= EPIC[board.i()] {
        return Strike::Hurt;
    }
    let chance = chance + if board == Board::SpaceRock { SPACE_EXTRA } else { 0.0 };
    if roll < chance { Strike::Miss } else { Strike::Success }
}

/// How a watcher takes a strike (`ReactToStrikeOnBoard`): a break clapped (a child cheers) or
/// cheered, the likelier for their traits and liking; a miss laughed at or booed; a hurt hand
/// either. Rolls 0 to 100.
pub fn reaction(how: Strike, child: bool, supports: bool, against: bool, liking: f32, roll: f32, coin: bool) -> &'static str {
    match how {
        Strike::Success => {
            let mut chance = CHEER_BASE + if supports { TRAIT_BONUS } else { 0.0 };
            if liking > LTR_CHEER {
                chance += LTR_BONUS;
            }
            if child || roll < chance.min(100.0) { "Cheer" } else { "Clap" }
        }
        Strike::Miss => {
            let mut chance = CHEER_BASE + if against { TRAIT_BONUS } else { 0.0 };
            if liking < LTR_BOO {
                chance += LTR_BONUS;
            }
            if roll < chance.min(100.0) { "Boo" } else { "Laugh" }
        }
        Strike::Hurt => {
            if coin { "Boo" } else { "Laugh" }
        }
    }
}

/// A watcher's clip for a reaction, then back to watching.
fn reaction_clip(r: &str) -> ActionClip {
    match r {
        "Clap" => ActionClip::steps("a_wedding_genericClap_start_x", &["a_wedding_genericClap_loop_x", "a_wedding_genericClap_stop_x"], WATCHING),
        "Cheer" => ActionClip::steps("a_react_cheer_x", &[], WATCHING),
        "Boo" => ActionClip::steps("a_react_boo_standing_x", &[], WATCHING),
        _ => ActionClip::steps("a_react_point_laugh_x", &[], WATCHING),
    }
}

/// Where a Sim breaks boards from a side, and the way they face.
pub fn side_spot(btf: &Transform, far: bool) -> (Vec2, Vec2) {
    let at = btf.transform_point(Vec3::new(0.0, 0.0, if far { SIDE_FAR } else { SIDE_NEAR })).xz();
    let mid = btf.transform_point(Vec3::new(0.0, 0.0, BOARDS_AT.z)).xz();
    (at, (mid - at).normalize_or(Vec2::NEG_Y))
}

/// Breaking a kind of board, from the side nearer them.
pub fn practise(breaker: Entity, btf: &Transform, board: Board, here: Vec2) -> ActionKind {
    let far = side_spot(btf, true).0.distance(here) < side_spot(btf, false).0.distance(here);
    let (at, face) = side_spot(btf, far);
    ActionKind::BoardBreak { breaker, board, side: far, at, face }
}

/// Watching from a couple of metres off to the side nearer them.
pub fn watch_spot(breaker: Entity, btf: &Transform, here: Vec2) -> ActionKind {
    let mid = btf.transform_point(Vec3::new(0.0, 0.0, BOARDS_AT.z));
    let across = (btf.rotation * Vec3::X).xz();
    let a = mid.xz() + across * 2.0;
    let b = mid.xz() - across * 2.0;
    let at = if a.distance(here) < b.distance(here) { a } else { b };
    ActionKind::WatchBoardBreak { breaker, at, face: (mid.xz() - at).normalize_or(Vec2::Y) }
}

/// Why they can't break boards, if they can't (`CanSimPracticeOnMe`'s words).
pub fn why_not(full_name: &str, level: f32, fatigued: bool, hurt: bool) -> Option<String> {
    if level < UNLOCK_LEVEL {
        Some(format!("{full_name} is not skilled enough to break boards."))
    } else if fatigued {
        Some(format!("{full_name} is Fatigued."))
    } else if hurt {
        Some(format!("{full_name} has a hurt hand!"))
    } else {
        None
    }
}

/// A board breaker's menu: "Practice... ›" its boards (or why not), "Change Board... ›" while at
/// it, "Watch Martial Artist" while another is.
#[allow(clippy::too_many_arguments)]
pub fn options(
    breaker: Entity,
    btf: &Transform,
    (me, sim): (Entity, &Sim),
    level: f32,
    (fatigued, hurt): (bool, bool),
    here: Vec2,
    breakers: &Breakers,
    pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>,
    submenu: impl Fn(usize) -> ActionKind,
) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    if sim.age.is_little() {
        return out;
    }
    match breakers.0.get(&breaker) {
        Some((user, board)) if *user == me => {
            let list = Board::ALL
                .into_iter()
                .map(|b| (b.name().to_string(), if b == *board { ActionKind::Greyed(format!("{} is currently breaking this type of board.", sim.full_name())) } else { ActionKind::ChangeBoard { breaker, board: b } }))
                .collect();
            out.push((format!("{CHANGE_BOARD} ›"), submenu(pie_submenus.len())));
            pie_submenus.push((CHANGE_BOARD.to_string(), list));
        }
        Some(_) => out.push((WATCH.to_string(), watch_spot(breaker, btf, here))),
        None if sim.age == Age::Child => {}
        None => match why_not(&sim.full_name(), level, fatigued, hurt) {
            Some(why) => out.push((PRACTICE.to_string(), ActionKind::Greyed(why))),
            None => {
                let list = Board::ALL.into_iter().map(|b| (b.name().to_string(), practise(breaker, btf, b, here))).collect();
                out.push((format!("{PRACTICE} ›"), submenu(pie_submenus.len())));
                pie_submenus.push((PRACTICE.to_string(), list));
            }
        },
    }
    out
}

/// The kind of board changed there and then (`ChangeBoard`): taken up when they're next readying.
pub fn change(w: &mut World, sim: Entity, breaker: Entity, board: Board) {
    if let Some(mut b) = w.get_mut::<Breaking>(sim)
        && b.breaker == breaker
    {
        b.pending = Some(board);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    Dress,
    BowIn,
    Arrange,
    StepUp,
    Ready,
    Strike(Strike),
    Out,
}

/// At a board breaker: what kind, the next kind asked for, how far along, how many to a stack and
/// broken since it grew, the boards out, from which side, whether this step's moment has passed,
/// whether they've been told to stop.
#[derive(Component)]
pub struct Breaking {
    breaker: Entity,
    board: Board,
    pending: Option<Board>,
    stage: Stage,
    t: f32,
    len: f32,
    readied: f64,
    stack: u8,
    breaks: u8,
    boards: Vec<Entity>,
    far: bool,
    fired: bool,
    stop: bool,
    spun: bool,
}

/// Told to stop: they finish the step they're at and bow out, the boards cleared away.
fn hold_on(mut sims: Query<(&mut ActionQueue, Option<&mut Breaking>)>) {
    for (mut q, breaking) in &mut sims {
        let Some(a) = q.0.front_mut() else { continue };
        if a.cancel
            && matches!(a.phase, Phase::Running(_))
            && matches!(a.kind, ActionKind::BoardBreak { .. })
            && let Some(mut b) = breaking
            && b.stage != Stage::Out
        {
            a.cancel = false;
            b.stop = true;
        }
    }
}

/// The boards set out on a breaker: the stack of them, or the space rock, at the side's slots.
#[allow(clippy::too_many_arguments)]
fn set_out(commands: &mut Commands, assets: &mut crate::objects::ObjectAssets, ctx: &mut crate::objects::AssetCtx, breaker: Entity, board: Board, stack: u8, far: bool) -> Vec<Entity> {
    let Some(objd) = ctx.baked.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(board.model())).map(|c| c.objd) else { return Vec::new() };
    let parts: Vec<crate::objects::ModelPart> = assets
        .object(ctx, objd)
        .into_iter()
        .map(|mut p| {
            if let Some(m) = board.state().and_then(|s| p.states.iter().find(|x| x.0 == s).map(|x| x.1.clone())) {
                p.material = m;
            }
            p.states.clear();
            p
        })
        .collect();
    let turn = if far { Quat::from_rotation_y(std::f32::consts::PI) } else { Quat::IDENTITY };
    let n = if board == Board::SpaceRock { 1 } else { stack.clamp(1, MOST_BOARDS) };
    (0..n)
        .map(|i| {
            let at = if board == Board::SpaceRock { Vec3::new(BOARDS_AT.x, STONE_Y, BOARDS_AT.z) } else { BOARDS_AT + Vec3::Y * BOARD_STEP * i as f32 };
            let e = crate::objects::spawn_parts(commands, &parts, Transform::from_translation(at).with_rotation(turn));
            commands.entity(breaker).add_child(e);
            e
        })
        .collect()
}

/// Boards broken on breakers, step by step.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib, ui, catalog): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>, Option<Res<crate::icons::GameUi>>, Res<crate::loading::Catalog>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut sims: Query<
        (
            Entity,
            &Sim,
            &mut ActionQueue,
            &mut Transform,
            &mut SimAnim,
            (&mut Skills, &mut crate::sim::Motives, Option<&crate::life::Mood>, Option<&mut crate::life::Moodlets>),
            Option<&mut Breaking>,
            (Option<&crate::simbody::Wearing>, Has<crate::dressing::ClothesSpin>, Option<&crate::journal::SkillJournal>),
        ),
        Without<GameObject>,
    >,
    breakers_q: Query<(&GlobalTransform, &Transform), (With<GameObject>, Without<Sim>)>,
    mut breakers: ResMut<Breakers>,
    (mut sounds, mut chips, mut struck, mut did, mut life): (
        MessageWriter<crate::sound::PlaySound>,
        MessageWriter<crate::effects::Chips>,
        MessageWriter<Struck>,
        MessageWriter<crate::journal::Did>,
        MessageWriter<crate::life::LifeEvent>,
    ),
    (world, building, mut notes): (Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>, ResMut<crate::interact::Notifications>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let minutes = delta.0 as f64;
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    let len = |lib: &mut crate::anim::ClipLibrary, c: &str| lib.get(&data, c).map_or(2.0, |x| x.duration);
    for (e, sim, mut q, mut tf, mut anim, (mut skills, mut motives, mood, moodlets), breaking, (wearing, spinning, journal)) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(|a| match a.kind {
            ActionKind::BoardBreak { breaker, board, side, at, face } => Some((breaker, board, side, at, face)),
            _ => None,
        });
        let Some((breaker, board, far, spot, face)) = front else {
            // (Off it some other way: the boards cleared, the breaker free.)
            if let Some(b) = breaking {
                for x in &b.boards {
                    commands.entity(*x).try_despawn();
                }
                breakers.0.remove(&b.breaker);
                commands.entity(e).remove::<(Breaking, ActionClip)>();
            }
            continue;
        };
        let Ok((bgt, btf)) = breakers_q.get(breaker) else {
            q.0.pop_front();
            continue;
        };
        let Some(mut b) = breaking else {
            // (Right at its side, facing it.)
            tf.translation.x = spot.x;
            tf.translation.z = spot.y;
            tf.rotation = Quat::from_rotation_y(face.x.atan2(face.y));
            anim.pose = Pose::Use;
            let best = journal.map_or(0.0, |j| j.most(&board.stack_key())) as u8;
            breakers.0.insert(breaker, (e, board));
            commands.entity(e).insert(Breaking {
                breaker,
                board,
                pending: None,
                stage: Stage::Dress,
                t: 0.0,
                len: 0.0,
                readied: 0.0,
                stack: best.clamp(1, MOST_BOARDS),
                breaks: 0,
                boards: Vec::new(),
                far,
                fired: false,
                stop: false,
                spun: false,
            });
            info!("board breaker: {} breaks {}s", sim.first, board.name());
            continue;
        };
        breakers.0.insert(breaker, (e, b.board));
        // (What it's worth as they go: the skills, hygiene, the Disciplined's fun, shaping up.)
        if hours > 0.0 && !matches!(b.stage, Stage::Dress) {
            let rate = crate::life::skill_rate(&sim.traits, "MartialArts");
            for (sk, pts) in [("MartialArts", SKILL_PER_MINUTE[b.board.i()] * rate), ("Athletic", ATHLETIC_PER_MINUTE)] {
                if let Some(level) = skills.add_points(sk, pts * delta.0) {
                    notes.push(format!("{} reached level {level} in {}!", sim.first, crate::interact::skill_title(sk)));
                    life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: sk, level }));
                }
            }
            motives.add(crate::sim::HYGIENE, HYGIENE_PER_HOUR * hours);
            if sim.traits.contains(&crate::life::Trait::Disciplined) {
                motives.add(crate::sim::FUN, DISCIPLINED_FUN * hours);
            }
            commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| crate::aging::reshape(&mut w, -0.03 * hours, 0.05 * hours));
            did.write(crate::journal::Did::count(e, crate::journal::Stat::StrengthHours, hours as f64));
        }
        let fatigued = moodlets.as_ref().is_some_and(|m| m.has(crate::life::MoodletKind::Fatigued));
        if fatigued {
            b.stop = true;
        }
        let floor = bgt.translation().y;
        let board_at = btf.transform_point(Vec3::new(0.0, if b.board == Board::SpaceRock { STONE_Y } else { BOARDS_AT.y + BOARD_STEP * b.stack as f32 }, BOARDS_AT.z));
        b.t += dt;
        let done = b.t >= b.len;
        let go = |b: &mut Breaking, stage: Stage, clip: ActionClip, len: f32, commands: &mut Commands| {
            b.stage = stage;
            b.t = 0.0;
            b.len = len;
            b.fired = false;
            commands.entity(e).insert(clip);
        };
        match b.stage {
            // Into their gi first, with a spin.
            Stage::Dress => {
                if spinning {
                    continue;
                }
                let dressed = wearing.is_some_and(|w| w.0 == crate::simbody::OutfitKind::MartialArts);
                if !dressed && !b.spun && crate::dressing::spins(sim) {
                    b.spun = true;
                    crate::dressing::start(&mut commands, e, Some(crate::simbody::OutfitKind::MartialArts), false);
                    continue;
                }
                let l = len(&mut lib, BOW_IN);
                go(&mut b, Stage::BowIn, ActionClip::new(Some(BOW_IN), &[]), l, &mut commands);
            }
            Stage::BowIn => {
                if done {
                    let l = len(&mut lib, SWIPE);
                    go(&mut b, Stage::Arrange, ActionClip::new(Some(SWIPE), &[]), l, &mut commands);
                }
            }
            // The boards set out (another on the stack after enough broken).
            Stage::Arrange => {
                if !b.fired && b.t >= BOARDS_OUT_AT {
                    b.fired = true;
                    if b.breaks >= BREAKS_TO_GROW[b.board.i()] {
                        b.breaks = 0;
                        if b.board != Board::SpaceRock && b.stack < MOST_BOARDS {
                            b.stack += 1;
                        }
                    }
                    for x in b.boards.drain(..) {
                        commands.entity(x).try_despawn();
                    }
                    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                    b.boards = set_out(&mut commands, &mut assets, &mut ctx, breaker, b.board, b.stack, b.far);
                    did.write(crate::journal::Did { sim: e, what: crate::journal::Deed::Most(b.board.stack_key(), b.stack as f64) });
                }
                if done {
                    let l = len(&mut lib, STEP_UP);
                    go(&mut b, Stage::StepUp, ActionClip::new(Some(STEP_UP), &[]), l, &mut commands);
                }
            }
            Stage::StepUp => {
                if done {
                    b.readied = 0.0;
                    go(&mut b, Stage::Ready, ActionClip::new(None, READY), f32::MAX, &mut commands);
                }
            }
            // Readying themselves; then the strike (or another board, or done).
            Stage::Ready => {
                b.readied += minutes;
                if let Some(next) = b.pending.take().filter(|n| *n != b.board) {
                    for x in b.boards.drain(..) {
                        commands.entity(x).try_despawn();
                    }
                    b.board = next;
                    b.breaks = 0;
                    b.stack = (journal.map_or(0.0, |j| j.most(&next.stack_key())) as u8).clamp(1, MOST_BOARDS);
                    breakers.0.insert(breaker, (e, next));
                    info!("board breaker: {} changes to {}s", sim.first, next.name());
                    let l = len(&mut lib, SWIPE);
                    go(&mut b, Stage::Arrange, ActionClip::new(Some(SWIPE), &[]), l, &mut commands);
                    continue;
                }
                if b.stop {
                    let l = len(&mut lib, STEP_BACK) + len(&mut lib, BOW_OUT);
                    go(&mut b, Stage::Out, ActionClip::steps(STEP_BACK, &[BOW_OUT], &[]), l, &mut commands);
                    continue;
                }
                let forced = std::env::var("BOARDS_RESULT").ok();
                if b.readied >= b.board.prep() || forced.is_some() && b.readied >= 1.0 {
                    let level = skills.0.get("MartialArts").copied().unwrap_or(0.0);
                    let advanced = journal.is_some_and(|j| j.get(crate::journal::Stat::HardBoardsBroken) >= ADVANCED_BOARDS);
                    let chance = miss_chance(b.board, level, mood.map_or(0.0, |m| m.0), advanced, sim.traits.contains(&crate::life::Trait::Disciplined));
                    let how = match forced.as_deref() {
                        Some("success") => Strike::Success,
                        Some("miss") => Strike::Miss,
                        Some("hurt") => Strike::Hurt,
                        _ => strike(b.board, chance, rng.random_range(0.0..100.0)),
                    };
                    let clip = match how {
                        Strike::Success => BREAK,
                        Strike::Miss => MISS,
                        Strike::Hurt => HURT,
                    };
                    if how == Strike::Hurt
                        && let Some(mut m) = moodlets
                    {
                        m.add(crate::life::MoodletKind::HurtHand, now);
                    }
                    struck.write(Struck { breaker, how });
                    info!("board breaker: {} strikes the {} ({how:?}, {chance:.0} in 100 to miss)", sim.first, b.board.name());
                    let l = len(&mut lib, clip);
                    go(&mut b, Stage::Strike(how), ActionClip::new(Some(clip), &[]), l, &mut commands);
                }
            }
            Stage::Strike(how) => {
                let at = match how {
                    Strike::Success => SPLIT_AT,
                    Strike::Miss => MISS_AT,
                    Strike::Hurt => HURT_AT,
                };
                if !b.fired && b.t >= at {
                    b.fired = true;
                    if how == Strike::Success {
                        // Split: the boards to bits with their crack, and what it's worth.
                        for x in b.boards.drain(..) {
                            commands.entity(x).try_despawn();
                        }
                        let n = if b.board == Board::SpaceRock { 18 } else { 6 + 5 * b.stack as u32 };
                        chips.write(crate::effects::Chips { at: board_at, away: board_at - tf.translation, color: b.board.color(), n, size: if b.board == Board::SpaceRock { 0.07 } else { 0.05 }, floor });
                        sounds.write(crate::sound::PlaySound { name: b.board.break_sound(b.stack), at: Some(board_at), volume: 1.0, speed: 1.0 });
                        b.breaks += 1;
                        let broken = if b.board == Board::SpaceRock { 1.0 } else { b.stack as f64 };
                        did.write(crate::journal::Did::count(e, crate::journal::Stat::BoardsBroken, broken));
                        if b.board >= Board::Oak {
                            let was = journal.map_or(0.0, |j| j.get(crate::journal::Stat::HardBoardsBroken));
                            did.write(crate::journal::Did::count(e, crate::journal::Stat::HardBoardsBroken, broken));
                            if was < ADVANCED_BOARDS && was + broken >= ADVANCED_BOARDS {
                                notes.push(format!("{0} has gained the title of \"Timber Terminator\" from breaking boundless boards. {0} now has a better chance of breaking boards. Increased hand eye coordination allows occasional recovery of gems when breaking space rocks.", sim.full_name()));
                            }
                        }
                        // (A Timber Terminator's gem, now and then, out of a space rock.)
                        let advanced = journal.is_some_and(|j| j.get(crate::journal::Stat::HardBoardsBroken) >= ADVANCED_BOARDS);
                        if advanced
                            && b.board == Board::SpaceRock
                            && let Some(ui) = ui.as_deref()
                        {
                            let rolls = rng.random_range(1..=MOST_GEMS);
                            let roll = (0..rolls).map(|_| rng.random_range(0.0..100.0f32)).last().unwrap_or(0.0);
                            if let Some(key) = gem_for(roll, ui, &mut rng) {
                                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                                lay_gem(&mut commands, &mut assets, &mut ctx, &catalog, ui, &world.data, building.as_deref(), &key, bgt.translation(), &mut rng);
                                info!("board breaker: a {key} out of the space rock");
                            }
                        }
                    } else {
                        sounds.write(crate::sound::PlaySound { name: b.board.fail_sound().into(), at: Some(board_at), volume: 1.0, speed: 1.0 });
                    }
                }
                if done {
                    match how {
                        _ if b.stop || how == Strike::Hurt => {
                            let l = len(&mut lib, STEP_BACK) + len(&mut lib, BOW_OUT);
                            go(&mut b, Stage::Out, ActionClip::steps(STEP_BACK, &[BOW_OUT], &[]), l, &mut commands);
                        }
                        Strike::Success => {
                            let l = len(&mut lib, SWIPE);
                            go(&mut b, Stage::Arrange, ActionClip::new(Some(SWIPE), &[]), l, &mut commands);
                        }
                        _ => {
                            let l = len(&mut lib, STEP_UP);
                            go(&mut b, Stage::StepUp, ActionClip::new(Some(STEP_UP), &[]), l, &mut commands);
                        }
                    }
                }
            }
            // Bowed out: the boards away, and done.
            Stage::Out => {
                if done {
                    for x in b.boards.drain(..) {
                        commands.entity(x).try_despawn();
                    }
                    breakers.0.remove(&breaker);
                    q.0.pop_front();
                    anim.pose = Pose::Stand;
                    commands.entity(e).remove::<(Breaking, ActionClip)>();
                    info!("board breaker: {} is done", sim.first);
                }
            }
        }
    }
    // (Breakers whose Sim is gone: free again.)
    breakers.0.retain(|_, (s, _)| sims.contains(*s));
}

/// A gem by a roll (`UpdateBoardBreakerStatisticsAndProvideRewards`): none, or one priced low,
/// middling or high.
fn gem_for(roll: f32, ui: &crate::icons::GameUi, rng: &mut impl Rng) -> Option<String> {
    use rand::seq::IndexedRandom;
    let (lo, hi) = if roll <= NO_GEM {
        return None;
    } else if roll <= LOW_GEM {
        (0, GEM_LOW_MEDIUM)
    } else if roll <= MEDIUM_GEM {
        (GEM_LOW_MEDIUM, GEM_MEDIUM_HIGH)
    } else {
        (GEM_MEDIUM_HIGH, i32::MAX)
    };
    let gems: Vec<&s3bake::gamedata::CollectibleInfo> = ui.data.collectibles.iter().filter(|c| c.kind == s3bake::gamedata::CollectKind::Gem && c.min_price >= lo && c.min_price < hi).collect();
    gems.choose(rng).map(|c| c.key.clone())
}

/// A gem left on the ground by the breaker, to be picked up.
#[allow(clippy::too_many_arguments)]
fn lay_gem(
    commands: &mut Commands,
    assets: &mut crate::objects::ObjectAssets,
    ctx: &mut crate::objects::AssetCtx,
    catalog: &crate::loading::Catalog,
    ui: &crate::icons::GameUi,
    world: &crate::loading::WorldInfo,
    building: Option<&crate::building::ActiveBuilding>,
    key: &str,
    near: Vec3,
    rng: &mut impl Rng,
) {
    let Some(c) = ui.data.collectibles.iter().find(|c| c.key == key) else { return };
    let Some(objd) = ctx.baked.catalog.iter().find(|o| o.instance_name == c.model).map(|o| o.objd) else { return };
    let a = rng.random_range(0.0..std::f32::consts::TAU);
    let p = near.xz() + Vec2::from_angle(a) * 1.0;
    let y = crate::building::walk_height(world, building, Vec3::new(p.x, near.y, p.y));
    let Some(o) = crate::home::spawn_game_object(commands, assets, ctx, catalog, objd, Vec3::new(p.x, y, p.y), a) else { return };
    let key = key.to_string();
    commands.entity(o.entity).remove::<crate::nav::Obstacle>().insert(crate::collecting::Pickup { key }).queue_silenced(|mut w: EntityWorldMut| {
        if let Some(mut g) = w.get_mut::<GameObject>() {
            g.kind = ObjectKind::Collectible;
            g.name = "Gem".into();
        }
    });
}

/// Watching a martial artist: since when they last liked them more.
#[derive(Component)]
struct Watching {
    next_liking: f64,
}

/// Those watching: watching, fun of it, liking the martial artist the more, and taking each
/// strike as the game has them; done when the breaking is.
#[allow(clippy::type_complexity)]
fn watch(
    mut commands: Commands,
    (clock, delta): (Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut SimAnim, &mut crate::sim::Motives, Option<&mut Watching>, Option<&mut crate::social::Relationships>, &mut Transform)>,
    breakers: Res<Breakers>,
    mut struck: MessageReader<Struck>,
) {
    let strikes: Vec<Struck> = struck.read().copied().collect();
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    let users: HashMap<Entity, Vec3> = sims.iter().filter(|s| breakers.0.values().any(|(u, _)| *u == s.0)).map(|s| (s.0, s.7.translation)).collect();
    for (e, sim, mut q, mut anim, mut motives, watching, rels, mut tf) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(|a| match a.kind {
            ActionKind::WatchBoardBreak { breaker, .. } => Some(breaker),
            _ => None,
        });
        let Some(breaker) = front else {
            if watching.is_some() {
                commands.entity(e).remove::<(Watching, ActionClip)>();
            }
            continue;
        };
        // (Nobody at it: nothing to watch.)
        let Some((user, _)) = breakers.0.get(&breaker).copied() else {
            q.0.pop_front();
            anim.pose = Pose::Stand;
            commands.entity(e).remove::<(Watching, ActionClip)>();
            continue;
        };
        if let Some(at) = users.get(&user) {
            let d = (*at - tf.translation).xz();
            if d.length() > 0.1 {
                tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
            }
        }
        let Some(mut w) = watching else {
            anim.pose = Pose::Use;
            commands.entity(e).insert((Watching { next_liking: now + LIKING_EVERY }, ActionClip::new(None, WATCHING)));
            info!("board breaker: {} watches", sim.first);
            continue;
        };
        motives.add(crate::sim::FUN, WATCH_FUN * hours);
        let mut rels = rels;
        if now >= w.next_liking {
            w.next_liking = now + LIKING_EVERY;
            if let Some(r) = rels.as_mut() {
                r.add(user, LIKING_GAIN, 0.0);
            }
        }
        for s in strikes.iter().filter(|s| s.breaker == breaker) {
            let has = |t: crate::life::Trait| sim.traits.contains(&t);
            use crate::life::Trait as T;
            let supports = has(T::Friendly) || has(T::Daredevil) || has(T::Athletic);
            let against = has(T::MeanSpirited) || has(T::Evil);
            let liking = rels.as_ref().map_or(0.0, |r| r.friendship(user));
            let r = reaction(s.how, sim.age == Age::Child, supports, against, liking, rng.random_range(0.0..100.0), rng.random_bool(0.5));
            commands.entity(e).insert(reaction_clip(r));
            info!("board breaker: {} reacts: {r}", sim.first);
        }
    }
}

/// The household's martial artists practise now and then of their own accord (the toughest board
/// they can, `PracticeAutonomous`); those about with fun to find watch one at it.
#[allow(clippy::type_complexity)]
fn autonomy(
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &Transform, &Skills, &crate::sim::Motives, Option<&crate::life::Moodlets>), (With<crate::sim::HouseholdMember>, Without<crate::sim::Selected>)>,
    objects: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    breakers: Res<Breakers>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 60.0;
    if std::env::var_os("BOARDS_TEST").is_some() {
        return;
    }
    let mut rng = rand::rng();
    for (_, sim, mut q, tf, skills, m, moodlets) in &mut sims {
        if sim.age.is_little() || !q.0.is_empty() {
            continue;
        }
        let here = tf.translation.xz();
        let near = |busy: bool, reach: f32| {
            objects
                .iter()
                .filter(|(o, g, t)| g.kind == ObjectKind::BoardBreaker && breakers.0.contains_key(o) == busy && t.translation.xz().distance(here) < reach)
                .min_by(|a, b| a.2.translation.xz().distance(here).total_cmp(&b.2.translation.xz().distance(here)))
        };
        let level = skills.0.get("MartialArts").copied().unwrap_or(0.0);
        let unwell = moodlets.is_some_and(|ml| ml.has(crate::life::MoodletKind::Fatigued) || ml.has(crate::life::MoodletKind::HurtHand));
        let keen = sim.traits.contains(&crate::life::Trait::Disciplined) || m.0[crate::sim::FUN] < 50.0;
        if sim.age != Age::Child
            && !unwell
            && keen
            && let Some(board) = most_compatible(level)
            && rng.random_bool(0.25)
            && let Some((b, _, btf)) = near(false, 20.0)
        {
            q.0.push_back(Action::new(format!("Breaking {}", board.name()), practise(b, btf, board, here), true));
            info!("board breaker: {} goes to break {}s of their own accord", sim.first, board.name());
        } else if m.0[crate::sim::FUN] < 70.0
            && rng.random_bool(0.3)
            && let Some((b, _, btf)) = near(true, 10.0)
        {
            q.0.push_back(Action::new(WATCH, watch_spot(b, btf, here), true));
        }
    }
}

/// `BOARDS_TEST=1` (tests): ten seconds in, a board breaker put down near the selected Sim (or
/// `BOARDS_WHO=<first name>`), who breaks boards on it (`BOARDS_BOARD=<1-5>`, the toughest they
/// can; at `BOARDS_LEVEL=<n>` Martial Arts); `BOARDS_WATCH=<first name>` watches;
/// `BOARDS_RESULT=success|miss|hurt` decides the strikes; `BOARDS_CHANGE=<1-5>` changes the board
/// after twenty seconds; `BOARDS_STOP=<seconds>` stops them; `BOARDS_PIE=1`: its menu.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn boards_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, &mut Skills, Has<crate::sim::Selected>), With<crate::sim::HouseholdMember>>,
    things: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    if std::env::var_os("BOARDS_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let who = env("BOARDS_WHO");
    let is_me = |s: &Sim, sel: bool| who.as_ref().map_or(sel, |w| s.first.eq_ignore_ascii_case(w));
    let breaker = things.iter().find(|d| d.1.kind == ObjectKind::BoardBreaker);
    if let (Ok(mut c), Some((_, _, btf))) = (cams.single_mut(), breaker) {
        let f = |k: &str, d: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(btf.translation);
        c.height_offset = f("BOARDS_LOOK_Y", 0.8);
        c.distance = f("BOARDS_DIST", 4.5);
        c.pitch = f("BOARDS_PITCH", 0.35);
        let fwd = btf.rotation * Vec3::Z;
        c.yaw = fwd.x.atan2(fwd.z) + f("BOARDS_YAW", 1.3);
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let Some((_, _, tf, ..)) = sims.iter().find(|s| is_me(s.1, s.5)) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("BoardBreaker")).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = tf.translation.xz();
            let spot = (3..16).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32 * 0.5)).find(|c| (-4..=4).all(|i| (-4..=4).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.4)))).unwrap_or(here + Vec2::X * 2.0);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, Vec3::new(spot.x, tf.translation.y, spot.y), 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("boards test: put down");
            }
        }
        1 if now - stage.1 > 2.0 => {
            let Some((b, _, btf)) = breaker else { return };
            *stage = (2, now);
            if env("BOARDS_PIE").is_some() {
                if let Some(l) = env("BOARDS_LEVEL").and_then(|v| v.parse::<f32>().ok())
                    && let Some((.., mut skills, _)) = sims.iter_mut().find(|s| is_me(s.1, s.5))
                {
                    skills.0.insert("MartialArts", l);
                }
                commands.insert_resource(crate::hud::PieFor(b));
                return;
            }
            let level = env("BOARDS_LEVEL").and_then(|v| v.parse::<f32>().ok());
            let board = env("BOARDS_BOARD").and_then(|v| v.parse::<usize>().ok()).and_then(|i| Board::ALL.get(i.max(1) - 1).copied());
            let watcher = env("BOARDS_WATCH");
            for (_, sim, tf, mut q, mut skills, sel) in &mut sims {
                if is_me(sim, sel) {
                    if let Some(l) = level {
                        skills.0.insert("MartialArts", l);
                    }
                    let board = board.or_else(|| most_compatible(skills.0.get("MartialArts").copied().unwrap_or(0.0))).unwrap_or(Board::Foam);
                    q.push_player(Action::new(format!("Breaking {}", board.name()), practise(b, btf, board, tf.translation.xz()), false));
                    info!("boards test: {} breaks {}s", sim.first, board.name());
                } else if watcher.as_ref().is_some_and(|w| sim.first.eq_ignore_ascii_case(w)) {
                    q.0.clear();
                    q.push_player(Action::new(WATCH, watch_spot(b, btf, tf.translation.xz()), false));
                    info!("boards test: {} to watch", sim.first);
                }
            }
        }
        2 if now - stage.1 > 20.0 => {
            *stage = (3, now);
            if let Some(n) = env("BOARDS_CHANGE").and_then(|v| v.parse::<usize>().ok()).and_then(|i| Board::ALL.get(i.max(1) - 1).copied())
                && let (Some((e, ..)), Some((b, ..))) = (sims.iter().find(|s| is_me(s.1, s.5)), breaker)
            {
                commands.queue(move |w: &mut World| change(w, e, b, n));
                info!("boards test: change to {}s", n.name());
            }
        }
        3 if env("BOARDS_STOP").and_then(|v| v.parse::<f32>().ok()).is_some_and(|s| now - stage.1 > s) => {
            *stage = (4, now);
            if let Some((_, sim, _, mut q, ..)) = sims.iter_mut().find(|s| is_me(s.1, s.5))
                && let Some(a) = q.0.front_mut()
            {
                a.cancel = true;
                info!("boards test: {} told to stop", sim.first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boards_come_and_break_as_the_game_tunes_them() {
        assert_eq!(most_compatible(1.0), None);
        assert_eq!(most_compatible(2.0), Some(Board::Foam));
        assert_eq!(most_compatible(7.5), Some(Board::Oak));
        assert_eq!(most_compatible(10.0), Some(Board::SpaceRock));
        // (At the board's level, thirty in a hundred; past it less, short of it more.)
        assert!((miss_chance(Board::Foam, 2.0, 50.0, false, false) - 30.0).abs() < 1e-4);
        assert!((miss_chance(Board::Foam, 4.0, 50.0, false, false) - 30.0 / 0.6f32.exp()).abs() < 1e-4);
        assert!((miss_chance(Board::Oak, 2.0, 50.0, false, false) - 58.0).abs() < 1e-4);
        assert!((miss_chance(Board::Foam, 2.0, -20.0, false, false) - 40.0).abs() < 1e-4, "a low mood");
        assert!((miss_chance(Board::Foam, 2.0, 50.0, true, true) - 0.0).abs() < 1e-4);
        // (Past the board's mark, a hurt hand; else the roll.)
        assert_eq!(strike(Board::Balsa, 45.0, 99.0), Strike::Hurt);
        assert_eq!(strike(Board::Foam, 30.0, 10.0), Strike::Miss);
        assert_eq!(strike(Board::Foam, 30.0, 40.0), Strike::Success);
        assert_eq!(strike(Board::SpaceRock, 10.0, 25.0), Strike::Miss, "the space rock's harder");
    }

    #[test]
    fn the_watchers_take_it_as_the_game_has_them() {
        assert_eq!(reaction(Strike::Success, false, false, false, 0.0, 70.0, false), "Clap");
        assert_eq!(reaction(Strike::Success, false, true, false, 30.0, 70.0, false), "Cheer", "a friendly friend");
        assert_eq!(reaction(Strike::Success, true, false, false, 0.0, 99.0, false), "Cheer", "a child");
        assert_eq!(reaction(Strike::Miss, false, false, false, 0.0, 70.0, false), "Laugh");
        assert_eq!(reaction(Strike::Miss, false, true, true, -30.0, 70.0, false), "Boo");
        assert_eq!(reaction(Strike::Hurt, false, false, false, 0.0, 0.0, true), "Boo");
    }

    #[test]
    fn the_boards_and_their_words() {
        assert_eq!(Board::Balsa.break_sound(1), "fx_bbreaker_break_balsa_a");
        assert_eq!(Board::ThinStone.break_sound(5), "fx_bbreaker_break_stone_d");
        assert_eq!(Board::SpaceRock.break_sound(1), "fx_bbreaker_break_stone");
        assert_eq!(Board::ThinStone.fail_sound(), "bbreaker_fail_stonea");
        assert_eq!(why_not("Jack Bunch", 1.0, false, false).as_deref(), Some("Jack Bunch is not skilled enough to break boards."));
        assert_eq!(why_not("Jack Bunch", 3.0, false, true).as_deref(), Some("Jack Bunch has a hurt hand!"));
        assert_eq!(why_not("Jack Bunch", 3.0, false, false), None);
        let tf = Transform::from_xyz(0.0, 0.0, 0.0);
        let (near, face) = side_spot(&tf, false);
        assert!((near - Vec2::new(0.0, 0.404562)).length() < 1e-5 && (face - Vec2::NEG_Y).length() < 1e-5);
        let (far, face) = side_spot(&tf, true);
        assert!((far - Vec2::new(0.0, -0.895)).length() < 1e-5 && (face - Vec2::Y).length() < 1e-5);
    }
}
