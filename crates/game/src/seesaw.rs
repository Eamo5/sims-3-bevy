//! Seesaws, Generations' (`Seesaw`: the See-Me See-You Seesaw). A Sim, a child or older, gets
//! on ("Play On"; a Daredevil's "Play On Extreme") from one of its places (a grown-up from either
//! end, a child from the ends or beside the seats: the seesaw's routing slots), with the game's
//! `Seesaw` clips: onto the raised seat and pulling it down, or onto the low one; a Daredevil,
//! half the time the raised end's up (`kPercentageGetOnAsDaredevil`), runs up the plank from the
//! low end to sit on the high one. Alone, they sit and wait for someone ("Teeter",
//! `WaitToTeeter`), the plank down their end; "Seesaw With... ›" someone about (within 30 m,
//! `kMaxDistanceToAsk`) has them call them over (`CallOver`), who comes and joins; or anyone
//! "Join"s one waiting (children and the Childish keen to, `kJoinInteractionScoreMultiplier`).
//! The one joining gets on the raised end, the two come level (`Two People Idling`) and teeter,
//! up and down in turn (`Two People Teetering`), with the plank's own half of each clip. Leaving,
//! a pair stop and both get off together (`Exit Two People`, the seesaw left with its north end
//! up), one alone gets off where they sit, the seesaw left down their end. Waiting's fun
//! (`WaitToTeeter`: Fun 50 an hour), teetering fun and company (`Teeter`: Fun 50, Social 70), and
//! the two chat as they go (`TryGroupTalk`). Children and teens short of fun go and sit on one of
//! their own accord, and others join one waiting.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct SeesawPlugin;

impl Plugin for SeesawPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Boards>()
            .add_systems(Update, hold.before(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, (manage, play, autonomy, seesaw_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, (place, rest).chain().after(crate::anim::drive_skeletons).before(crate::objanim::animate_objects));
    }
}

/// The game's words (`Gameplay/Objects/Environment/Seesaw`).
pub const PLAY_ON: &str = "Play On";
pub const PLAY_ON_EXTREME: &str = "Play On Extreme";
pub const JOIN: &str = "Join";
pub const TEETER: &str = "Teeter";
pub const SEESAW_WITH: &str = "Seesaw With...";
pub const NO_ONE: &str = "There are no other Sims on this lot to play with.";
/// The game's tuning (its XML's): how far off someone's asked to play, how likely a Daredevil
/// gets on the daredevil's way, how long one waits alone of their own accord (minutes); what
/// waiting and teetering are worth an hour.
pub const ASK_WITHIN: f32 = 30.0;
const DAREDEVIL_CHANCE: f64 = 0.5;
const WAIT_ALONE: f64 = 20.0;
const WAIT_FUN: f32 = 50.0;
const TEETER_FUN: f32 = 50.0;
const TEETER_SOCIAL: f32 = 70.0;
/// Liking between the two as they chat, an hour of it.
const CHAT_LIKING: f32 = 6.0;

/// A seesaw's places (its routing slots, `RoutingSlot_0`...): either end (the north out along
/// +Z), and beside each seat to either side; where each is and the way it faces.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Spot {
    N,
    NE,
    NW,
    S,
    SW,
    SE,
}

impl Spot {
    pub const ALL: [Spot; 6] = [Spot::N, Spot::NE, Spot::NW, Spot::S, Spot::SW, Spot::SE];

    /// Where it is on the seesaw and the way it faces (the RSLT's: 2.5 m out from the middle at
    /// the ends, half a metre beside the seats).
    pub fn place(self) -> (Vec2, Vec2) {
        match self {
            Spot::N => (Vec2::new(0.0, 2.5), Vec2::NEG_Y),
            Spot::NE => (Vec2::new(-0.5, 2.0), Vec2::X),
            Spot::NW => (Vec2::new(0.5, 2.0), Vec2::NEG_X),
            Spot::S => (Vec2::new(0.0, -2.5), Vec2::Y),
            Spot::SW => (Vec2::new(0.5, -2.0), Vec2::NEG_X),
            Spot::SE => (Vec2::new(-0.5, -2.0), Vec2::X),
        }
    }

    pub fn north(self) -> bool {
        matches!(self, Spot::N | Spot::NE | Spot::NW)
    }

    /// Its name in the clips' (`GetOn_Up_N_NE`).
    fn dir(self) -> &'static str {
        match self {
            Spot::N => "NN",
            Spot::NE => "NE",
            Spot::NW => "NW",
            Spot::S => "SS",
            Spot::SW => "SW",
            Spot::SE => "SE",
        }
    }

    /// A side's end.
    pub fn end(north: bool) -> Spot {
        if north { Spot::N } else { Spot::S }
    }

    /// Where someone comes to join on a side (the game's `Join`): a child beside the seat, a
    /// grown-up at the end.
    pub fn joining(north: bool, child: bool) -> Spot {
        match (north, child) {
            (true, true) => Spot::NE,
            (true, false) => Spot::N,
            (false, true) => Spot::SW,
            (false, false) => Spot::S,
        }
    }

    /// The places a Sim gets on from: a child's any, a grown-up's the ends (`mAdultSlots`).
    pub fn for_age(child: bool) -> &'static [Spot] {
        if child { &Spot::ALL } else { &[Spot::N, Spot::S] }
    }

    /// The place in the world, on a seesaw standing so.
    pub fn world(self, seesaw: &Transform) -> (Vec2, Vec2) {
        let (p, f) = self.place();
        let at = seesaw.transform_point(Vec3::new(p.x, 0.0, p.y)).xz();
        let face = (seesaw.rotation * Vec3::new(f.x, 0.0, f.y)).xz().normalize_or(Vec2::Y);
        (at, face)
    }
}

fn side(north: bool) -> &'static str {
    if north { "N" } else { "S" }
}

/// Who can play: children and up.
pub fn can_play(age: Age) -> bool {
    !matches!(age, Age::Baby | Age::Toddler)
}

/// Getting on first, from a place, with the north end up or not (the game's `EnterNorth`...):
/// onto the raised seat (`GetOn_Up_N_NN`), onto the low one (`GetOn_Down_N`), the daredevil's
/// way (`GetOn_Daredevil_N`).
pub fn get_on(spot: Spot, north_up: bool, daredevil: bool) -> String {
    if daredevil {
        return "a2o_Seesaw_GetOn_Daredevil_N_x".into();
    }
    let s = side(spot.north());
    match (spot.north() == north_up, spot) {
        (true, _) => format!("a2o_Seesaw_GetOn_Up_{s}_{}_x", spot.dir()),
        (false, Spot::N | Spot::S) => format!("a2o_Seesaw_GetOn_Down_{s}_x"),
        (false, _) => format!("a2o_Seesaw_GetOn_Down_{s}_{}_x", spot.dir()),
    }
}

/// The place a clip's played from (its `world_offset` is that place seen from the Sim): getting
/// on, the place they came from (the daredevil's, the low end); the one already on as another
/// joins, and sitting alone, their end; the two level and teetering, the south's end and beside
/// the north's seat; getting off together, the north's end (a child's beside its seat).
pub fn anchor(clip: &str, north: bool, from: Spot, child: bool) -> Spot {
    let n = clip.get(11..).unwrap_or("");
    let l = n.to_ascii_lowercase();
    if l.starts_with("geton_daredevil") {
        Spot::S
    } else if l.starts_with("geton_up_") && l.ends_with("_y") {
        from
    } else if l == "geton_up_n_x" || l == "geton_up_s_x" {
        Spot::end(north)
    } else if l.starts_with("geton_") {
        from
    } else if l.starts_with("getoff_balanced_n") {
        if child { Spot::NE } else { Spot::N }
    } else if l.starts_with("getoff_balanced_s") {
        Spot::S
    } else if ["idle_balanced_", "start_", "seesaw_", "stop_"].iter().any(|p| l.starts_with(p)) {
        if l.ends_with("_y") { Spot::NE } else { Spot::S }
    } else {
        Spot::end(north)
    }
}

/// Where a clip has a Sim by `t`, on a seesaw standing so: its place, and the clip's travel
/// from it.
pub fn root(seesaw: &Transform, at: Spot, clip: &s3formats::sim::Clip, t: f32) -> (Vec3, Quat) {
    let (p, f) = at.place();
    let turn = Quat::from_rotation_y(f.x.atan2(f.y));
    let (d, r) = crate::pets::root_motion(clip, t).unwrap_or((Vec3::ZERO, Quat::IDENTITY));
    let local = Vec3::new(p.x, 0.0, p.y) + turn * d;
    (seesaw.transform_point(local), seesaw.rotation * turn * r)
}

/// What's going on at a seesaw.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Part {
    #[default]
    Empty,
    GetOn,
    Wait,
    CallOver,
    Join,
    Balanced,
    Start,
    Teeter,
    Stop,
    GetOffTwo,
    GetOffOne,
}

/// Someone on a seesaw: who, which end, the place they got on from, the daredevil's way, a child.
#[derive(Clone, Copy, Debug)]
pub struct Rider {
    pub sim: Entity,
    pub north: bool,
    pub spot: Spot,
    pub daredevil: bool,
    pub child: bool,
}

/// A seesaw: which end's up at rest, who's on it (the first, then who joined), what they're at
/// and how far in, who's been called over and who's come to join, and whether they're leaving.
#[derive(Debug)]
pub struct Board {
    pub north_up: bool,
    pub riders: Vec<Rider>,
    pub part: Part,
    t: f32,
    length: f32,
    plank: Option<&'static str>,
    parity: bool,
    pub calling: Option<Entity>,
    pub joining: Option<Rider>,
    pub leaving: bool,
    since: f64,
    alone_since: f64,
}

impl Default for Board {
    fn default() -> Self {
        Self { north_up: true, riders: Vec::new(), part: Part::Empty, t: 0.0, length: 0.0, plank: None, parity: false, calling: None, joining: None, leaving: false, since: 0.0, alone_since: 0.0 }
    }
}

impl Board {
    pub fn has(&self, sim: Entity) -> bool {
        self.riders.iter().any(|r| r.sim == sim) || self.joining.is_some_and(|j| j.sim == sim)
    }

    /// Room for one more: one on it waiting (not on their way off), nobody coming to join.
    pub fn joinable(&self) -> bool {
        self.riders.len() == 1 && self.joining.is_none() && !matches!(self.part, Part::GetOffOne)
    }

    /// The clips of a part: each rider's (by the game's clips' names; a child's are found by the
    /// player), and the plank's half.
    pub fn clips(&self, part: Part) -> (Vec<String>, Option<String>) {
        let kids = !self.riders.is_empty() && self.riders.iter().all(|r| r.child);
        let w = |child: bool| if child { "c" } else { "a" };
        let pair = |stem: &str| -> (Vec<String>, Option<String>) {
            let riders = self.riders.iter().map(|r| format!("a2o_Seesaw_{stem}_{}", if r.north { "y" } else { "x" })).collect();
            (riders, Some(format!("{}2o_Seesaw_{stem}_Seesaw", w(kids))))
        };
        let Some(first) = self.riders.first() else { return (Vec::new(), None) };
        let s = side(first.north);
        let one = |stem: String| -> (Vec<String>, Option<String>) {
            let plank = format!("{}2o_Seesaw_{}_Seesaw", w(first.child), stem.trim_start_matches("a2o_Seesaw_").trim_end_matches("_x"));
            (vec![stem], Some(plank))
        };
        match part {
            Part::Empty => (Vec::new(), None),
            Part::GetOn => one(get_on(first.spot, self.north_up, first.daredevil)),
            Part::Wait => one(format!("a2o_Seesaw_Idle_01_Down_{s}_x")),
            Part::CallOver => one(format!("a2o_Seesaw_CallOver_Down_{s}_x")),
            Part::GetOffOne => one(format!("a2o_Seesaw_GetOff_Down_{s}_x")),
            // (The one joining's half is `_y`, the one on it `_x`, named for the joiner's end.)
            Part::Join => {
                let Some(j) = self.riders.get(1) else { return (Vec::new(), None) };
                let js = side(j.north);
                (vec![format!("a2o_Seesaw_GetOn_Up_{js}_x"), format!("a2o_Seesaw_GetOn_Up_{js}_y")], Some(format!("{}2o_Seesaw_GetOn_Up_{js}_Seesaw", w(j.child))))
            }
            Part::Balanced => pair("Idle_Balanced"),
            Part::Start => pair("Start"),
            Part::Teeter => pair("Seesaw"),
            Part::Stop => pair("Stop"),
            Part::GetOffTwo => {
                let riders = self.riders.iter().map(|r| if r.north { "a2o_Seesaw_GetOff_Balanced_N_NE_y".to_string() } else { "a2o_Seesaw_GetOff_Balanced_S_SW_x".to_string() }).collect();
                (riders, Some(format!("{}2o_Seesaw_GetOff_Balanced_Seesaw", w(kids))))
            }
        }
    }

    /// What comes after a part (the game's state machine): waiting on, calling someone over, a
    /// joiner getting on as the wait comes round, level, teetering, stopping and off.
    pub fn next(&self) -> Part {
        match self.part {
            Part::Empty | Part::GetOffTwo | Part::GetOffOne => Part::Empty,
            Part::GetOn | Part::Wait | Part::CallOver => {
                if self.riders.len() > 1 {
                    Part::Join
                } else if self.leaving {
                    Part::GetOffOne
                } else if self.calling.is_some() && self.part != Part::CallOver {
                    Part::CallOver
                } else {
                    Part::Wait
                }
            }
            Part::Join => Part::Balanced,
            Part::Balanced if self.leaving => Part::GetOffTwo,
            Part::Balanced => Part::Start,
            Part::Start => Part::Teeter,
            Part::Teeter if self.leaving => Part::Stop,
            Part::Teeter => Part::Teeter,
            Part::Stop => Part::GetOffTwo,
        }
    }
}

/// The seesaws.
#[derive(Resource, Default)]
pub struct Boards(pub HashMap<Entity, Board>);

impl Boards {
    pub fn on(&self, sim: Entity) -> bool {
        self.0.values().any(|b| b.has(sim))
    }

    pub fn north_up(&self, seesaw: Entity) -> bool {
        self.0.get(&seesaw).is_none_or(|b| b.north_up)
    }
}

/// The action that gets a Sim on from a place.
fn sit(seesaw: Entity, stf: &Transform, spot: Spot, daredevil: bool, call: Option<Entity>) -> ActionKind {
    let (at, face) = spot.world(stf);
    ActionKind::Seesaw { seesaw, spot, join: false, daredevil, call, at, face }
}

/// The action that has a Sim join the one on it, from their side's place.
pub fn join(seesaw: Entity, stf: &Transform, first_north: bool, child: bool) -> ActionKind {
    let spot = Spot::joining(!first_north, child);
    let (at, face) = spot.world(stf);
    ActionKind::Seesaw { seesaw, spot, join: true, daredevil: false, call: None, at, face }
}

/// Where a Sim gets on: the nearest of their places; a Daredevil, half the time the north end's
/// up, from the low end the daredevil's way.
pub fn pick_spot(stf: &Transform, here: Vec2, child: bool, north_up: bool, daredevil: bool, roll: f64) -> (Spot, bool) {
    if daredevil && north_up && roll < DAREDEVIL_CHANCE {
        return (Spot::S, true);
    }
    let spot = Spot::for_age(child).iter().copied().min_by(|a, b| a.world(stf).0.distance(here).total_cmp(&b.world(stf).0.distance(here))).unwrap_or(Spot::N);
    (spot, false)
}

/// A seesaw's menu for a Sim: "Play On" (a Daredevil's "Play On Extreme") with nobody on it,
/// "Join" one waiting, "Seesaw With... ›" someone about (nobody on it, or they're on it alone).
#[allow(clippy::too_many_arguments)]
pub fn options(
    seesaw: Entity,
    stf: &Transform,
    boards: &Boards,
    me: Entity,
    sim: &Sim,
    here: Vec2,
    others: &[(Entity, String)],
    pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>,
    submenu: impl Fn(usize) -> ActionKind,
) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    let b = boards.0.get(&seesaw);
    let child = sim.age == Age::Child;
    let daredevil = sim.traits.contains(&crate::life::Trait::Daredevil);
    let empty = b.is_none_or(|b| b.riders.is_empty() && b.joining.is_none() && b.part == Part::Empty);
    let mine = b.is_some_and(|b| b.riders.first().is_some_and(|r| r.sim == me));
    let (spot, extreme) = pick_spot(stf, here, child, boards.north_up(seesaw), daredevil, rand::rng().random_range(0.0..1.0));
    if empty {
        out.push((if daredevil { PLAY_ON_EXTREME } else { PLAY_ON }.to_string(), sit(seesaw, stf, spot, extreme, None)));
    } else if let Some(b) = b.filter(|b| b.joinable() && !b.has(me)) {
        out.push((JOIN.to_string(), join(seesaw, stf, b.riders[0].north, child)));
    }
    if empty || (mine && b.is_some_and(|b| b.joinable() && b.calling.is_none())) {
        if others.is_empty() {
            out.push((SEESAW_WITH.to_string(), ActionKind::Greyed(NO_ONE.into())));
        } else {
            let from = b.and_then(|b| b.riders.first()).map_or(spot, |r| r.spot);
            let list: Vec<(String, ActionKind)> = others.iter().map(|(o, name)| (name.clone(), sit(seesaw, stf, from, extreme && !mine, Some(*o)))).collect();
            out.push((format!("{SEESAW_WITH} ›"), submenu(pie_submenus.len())));
            pie_submenus.push((SEESAW_WITH.to_string(), list));
        }
    }
    out
}

/// A Sim's front action at a seesaw: which, and whether it's under way.
fn at_seesaw(q: &ActionQueue) -> Option<(Entity, &Action)> {
    let a = q.0.front()?;
    match a.kind {
        ActionKind::Seesaw { seesaw, .. } => Some((seesaw, a)),
        _ => None,
    }
}

/// A rider told to stop (their seesaw cancelled from the queue, or put off by something else
/// asked of them): they get off first, as the game's `Stand` has them, rather than vanish off it.
fn hold(mut boards: ResMut<Boards>, mut sims: Query<(Entity, &mut ActionQueue)>) {
    for (e, mut q) in &mut sims {
        let Some(a) = q.0.front_mut() else { continue };
        let ActionKind::Seesaw { seesaw, .. } = a.kind else { continue };
        if !a.cancel || !matches!(a.phase, Phase::Running(_)) {
            continue;
        }
        if let Some(b) = boards.0.get_mut(&seesaw)
            && b.riders.iter().any(|r| r.sim == e)
        {
            a.cancel = false;
            b.leaving = true;
        }
    }
}

/// Sims arriving at their places: the first on gets on, someone coming to join waits for the one
/// on it; someone sent to sit where another's sat already joins them instead; the one on it
/// asking someone over from the pie.
#[allow(clippy::type_complexity)]
fn manage(
    mut boards: ResMut<Boards>,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue)>,
    seesaws: Query<(&GameObject, &Transform), Without<Sim>>,
) {
    boards.0.retain(|e, _| seesaws.get(*e).is_ok_and(|s| s.0.kind == ObjectKind::Seesaw));
    for (e, sim, mut q) in &mut sims {
        // (The one on it asking someone over: their seesaw's caller.)
        if let Some((s, _)) = at_seesaw(&q)
            && let Some(b) = boards.0.get_mut(&s)
            && b.riders.first().is_some_and(|r| r.sim == e)
        {
            let mut call = None;
            q.0.retain(|a| match a.kind {
                ActionKind::Seesaw { seesaw, call: Some(o), .. } if seesaw == s && !matches!(a.phase, Phase::Running(_)) => {
                    call = Some(o);
                    false
                }
                _ => true,
            });
            if call.is_some() && b.joinable() && b.calling.is_none() {
                b.calling = call;
            }
        }
        let Some(Action { kind: ActionKind::Seesaw { seesaw, spot, join: joining, daredevil, call, .. }, phase: Phase::Running(_), cancel: false, .. }) = q.0.front() else { continue };
        let (s, spot, joining, daredevil, call) = (*seesaw, *spot, *joining, *daredevil, *call);
        let Ok((_, stf)) = seesaws.get(s) else {
            q.0.pop_front();
            continue;
        };
        let b = boards.0.entry(s).or_default();
        if b.has(e) {
            continue;
        }
        let child = sim.age == Age::Child;
        let rider = Rider { sim: e, north: spot.north() || daredevil, spot, daredevil, child };
        if !joining {
            if b.riders.is_empty() && b.part == Part::Empty && b.joining.is_none() {
                info!("seesaw: {} gets on from {spot:?}{}", sim.first, if daredevil { " (the daredevil's way)" } else { "" });
                b.riders.push(rider);
                b.calling = call;
                b.leaving = false;
                b.part = Part::Empty;
                b.since = clock.minutes;
                b.alone_since = clock.minutes;
                continue;
            }
            // (Someone's on it: join them instead, from their own side's place.)
            if b.joinable() {
                let first = b.riders[0].north;
                if let Some(a) = q.0.front_mut() {
                    a.kind = join(s, stf, first, child);
                    a.label = JOIN.to_string();
                    a.phase = Phase::Start;
                }
                continue;
            }
            q.0.pop_front();
            continue;
        }
        // Joining: waiting at their place till the one on it's done what they're at.
        if b.joinable() {
            info!("seesaw: {} comes to join", sim.first);
            b.joining = Some(rider);
            b.calling = None;
        } else {
            q.0.pop_front();
        }
    }
}

/// A one-off clip (played once, held at its end), told apart from the same one just played so it
/// plays again.
fn one_shot(name: &'static str, parity: bool) -> ActionClip {
    let c = ActionClip::new(Some(name), &[]);
    if parity { c.in_order() } else { c }
}

/// A part under way: each rider's clip (in step), the plank's, as long as the longest.
fn begin(commands: &mut Commands, lib: &mut ClipLibrary, data: &Baked, b: &mut Board, part: Part, dt: f32) {
    b.part = part;
    b.parity = !b.parity;
    b.t = dt;
    let (clips, plank) = b.clips(part);
    let mut length: f32 = 0.0;
    for (r, c) in b.riders.iter().zip(clips) {
        let c: &'static str = crate::pets::intern(c);
        let shown = lib.variants(data, c, None, r.child).first().cloned();
        length = length.max(shown.and_then(|n| lib.get(data, &n)).map_or(1.0, |k| k.duration));
        commands.entity(r.sim).insert(one_shot(c, b.parity));
    }
    b.plank = plank.filter(|p| data.0.clip_names.iter().any(|n| n == p)).map(crate::pets::intern);
    b.length = length.max(0.2);
    debug!("seesaw: {part:?} ({:.2}s)", b.length);
}

/// The seesaws' parts played out, one after another: getting on, waiting (calling someone over),
/// a joiner getting on, level, teetering, stopping, getting off; what each rider wants of it
/// weighed as each part comes round.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    mut boards: ResMut<Boards>,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut crate::sim::Motives, &mut SimAnim)>,
    seesaws: Query<&Transform, (With<GameObject>, Without<Sim>)>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let hours = delta.0 / 60.0;
    let mut calls: Vec<(Entity, Entity, bool)> = Vec::new();
    let mut chats: Vec<(Entity, Entity, f32)> = Vec::new();
    // (Whose front action's at which seesaw.)
    let at: HashMap<Entity, Entity> = sims.iter().filter_map(|(e, _, q, ..)| at_seesaw(q).map(|(s, _)| (e, s))).collect();
    for (&s, b) in boards.0.iter_mut() {
        // (Off it otherwise: their seesaw gone from their queue.)
        let still = |e: Entity| at.get(&e) == Some(&s);
        let gone: Vec<Entity> = b.riders.iter().map(|r| r.sim).filter(|e| !still(*e)).collect();
        if !gone.is_empty() {
            for e in &gone {
                commands.entity(*e).remove::<ActionClip>();
            }
            b.riders.retain(|r| !gone.contains(&r.sim));
            if b.riders.is_empty() {
                *b = Board { north_up: b.north_up, ..default() };
                continue;
            }
            // (The other gets off alone, from where they are.)
            b.leaving = true;
            if b.part != Part::GetOffOne {
                begin(&mut commands, &mut lib, &data, b, Part::GetOffOne, dt);
            }
        }
        if b.joining.is_some_and(|j| !still(j.sim)) {
            b.joining = None;
        }
        if b.riders.is_empty() {
            if let Some(j) = b.joining.take()
                && let Ok((_, _, mut q, ..)) = sims.get_mut(j.sim)
            {
                q.0.pop_front();
            }
            continue;
        }
        // What being on it's worth: waiting's fun; teetering's fun and company, and liking.
        let two = matches!(b.part, Part::Balanced | Part::Start | Part::Teeter | Part::Stop);
        if hours > 0.0 {
            for r in &b.riders {
                if let Ok((_, _, _, mut m, _)) = sims.get_mut(r.sim) {
                    if two {
                        m.add(crate::sim::FUN, TEETER_FUN * hours);
                        m.add(crate::sim::SOCIAL, TEETER_SOCIAL * hours);
                    } else if matches!(b.part, Part::Wait | Part::CallOver) {
                        m.add(crate::sim::FUN, WAIT_FUN * hours);
                    }
                }
            }
            if two && b.riders.len() == 2 {
                chats.push((b.riders[0].sim, b.riders[1].sim, CHAT_LIKING * hours));
            }
        }
        if b.part == Part::Empty {
            begin(&mut commands, &mut lib, &data, b, Part::GetOn, dt);
            continue;
        }
        b.t += dt;
        if b.t < b.length {
            continue;
        }
        // The part's done: what they want now.
        let alone = b.riders.len() == 1;
        for r in &b.riders {
            let Ok((_, sim, q, m, _)) = sims.get(r.sim) else { continue };
            let theirs = |a: &Action| matches!(a.kind, ActionKind::Seesaw { seesaw, .. } if seesaw == s);
            let other = q.0.iter().skip(1).any(|a| !a.cancel && !theirs(a));
            let needy = [crate::sim::HUNGER, crate::sim::BLADDER, crate::sim::ENERGY].iter().any(|i| m.0[*i] < -40.0);
            let had_fun = m.0[crate::sim::FUN] >= 95.0 && clock.minutes - b.since > 10.0;
            let autonomous = q.0.front().is_some_and(|a| a.autonomous);
            let waited = alone && autonomous && b.joining.is_none() && b.calling.is_none() && clock.minutes - b.alone_since > WAIT_ALONE;
            if other || needy || had_fun || waited {
                if !b.leaving {
                    info!("seesaw: {} wants to get off{}", sim.first, if waited { " (nobody came)" } else { "" });
                }
                b.leaving = true;
            }
        }
        // (Someone come to join, as the one on it's done waiting: on they get.)
        if alone
            && !b.leaving
            && matches!(b.part, Part::GetOn | Part::Wait | Part::CallOver)
            && let Some(j) = b.joining.take()
        {
            b.riders.push(j);
            b.since = clock.minutes;
            for r in &b.riders {
                if let Ok((_, _, mut q, _, mut anim)) = sims.get_mut(r.sim)
                    && let Some(a) = q.0.front_mut()
                {
                    a.label = TEETER.to_string();
                    anim.pose = Pose::Use;
                }
            }
            info!("seesaw: two on it");
        }
        let was = b.part;
        let next = b.next();
        // (Called over: asked to come and join.)
        if was == Part::CallOver
            && let Some(o) = b.calling.take()
        {
            calls.push((o, s, b.riders[0].north));
        }
        if was == Part::GetOn
            && let Some(r) = b.riders.first()
            && let Ok((_, _, mut q, ..)) = sims.get_mut(r.sim)
            && let Some(a) = q.0.front_mut()
        {
            a.label = TEETER.to_string();
        }
        if next == Part::Empty {
            // Off: the seesaw left as they left it.
            b.north_up = match was {
                Part::GetOffOne => !b.riders[0].north,
                _ => true,
            };
            for r in &b.riders {
                commands.entity(r.sim).remove::<ActionClip>();
                if let Ok((_, sim, mut q, _, mut anim)) = sims.get_mut(r.sim) {
                    q.0.pop_front();
                    anim.pose = Pose::Stand;
                    info!("seesaw: {} gets off", sim.first);
                }
            }
            if let Some(j) = b.joining.take()
                && let Ok((_, _, mut q, ..)) = sims.get_mut(j.sim)
            {
                q.0.pop_front();
            }
            *b = Board { north_up: b.north_up, ..default() };
            continue;
        }
        begin(&mut commands, &mut lib, &data, b, next, dt);
    }
    // (Liking between the two as they chat.)
    for (a, o, l) in chats {
        commands.queue(move |w: &mut World| {
            for (x, y) in [(a, o), (o, a)] {
                if let Some(mut r) = w.get_mut::<crate::social::Relationships>(x) {
                    r.add(y, l, 0.0);
                }
            }
        });
    }
    // (Those called over: on their way to join, what they were at put off.)
    for (o, s, first_north) in calls {
        let Ok(stf) = seesaws.get(s) else { continue };
        let Ok((_, sim, mut q, ..)) = sims.get_mut(o) else { continue };
        if !can_play(sim.age) || at_seesaw(&q).is_some() {
            continue;
        }
        let kind = join(s, stf, first_north, sim.age == Age::Child);
        q.push_player(Action::new(JOIN, kind, false));
        info!("seesaw: {} called over", sim.first);
    }
}

/// Riders put where their clips have them (each clip played from its place on the seesaw), and
/// the plank playing its half in step with them.
#[allow(clippy::type_complexity)]
fn place(
    mut commands: Commands,
    boards: Res<Boards>,
    mut sims: Query<(&mut Transform, &crate::anim::ClipPlayer), With<Sim>>,
    seesaws: Query<&Transform, (With<GameObject>, Without<Sim>)>,
) {
    for (&s, b) in &boards.0 {
        let Ok(stf) = seesaws.get(s) else { continue };
        for r in &b.riders {
            let Ok((mut tf, player)) = sims.get_mut(r.sim) else { continue };
            if !player.name.get(1..11).is_some_and(|p| p.eq_ignore_ascii_case("2o_Seesaw_")) {
                continue;
            }
            let Some(clip) = player.clip.as_ref() else { continue };
            let t = player.time.clamp(0.0, clip.duration.max(0.0));
            let (p, q) = root(stf, anchor(&player.name, r.north, r.spot, r.child), clip, t);
            tf.translation = Vec3::new(p.x, stf.translation.y, p.z);
            tf.rotation = q;
        }
        if let Some(name) = b.plank.filter(|_| !b.riders.is_empty()) {
            commands.entity(s).insert(crate::objanim::PlayingClip { name, time: b.t, looping: false });
        }
    }
}

/// Seesaws nobody's on rest down one end (`Static_Down_S`, the north end up, or `Static_Down_N`).
fn rest(mut commands: Commands, boards: Res<Boards>, seesaws: Query<(Entity, &GameObject, Option<&crate::objanim::HeldPose>, Has<crate::objanim::PlayingClip>)>) {
    for (e, o, held, playing) in &seesaws {
        if o.kind != ObjectKind::Seesaw {
            continue;
        }
        let b = boards.0.get(&e);
        let busy = b.is_some_and(|b| !b.riders.is_empty());
        if playing && !busy {
            commands.entity(e).remove::<crate::objanim::PlayingClip>();
        }
        let pose = if boards.north_up(e) { "a2o_Seesaw_Static_Down_S_Seesaw" } else { "a2o_Seesaw_Static_Down_N_Seesaw" };
        if held.is_none_or(|h| h.0 != pose) {
            commands.entity(e).insert(crate::objanim::HeldPose(pose));
        }
    }
}

/// Children and teens short of fun go and sit on a seesaw on the lot now and then; and someone
/// sat waiting draws another (a child, the Childish, or anyone short of fun nearby) to join.
#[allow(clippy::type_complexity)]
fn autonomy(
    clock: Res<crate::clock::GameClock>,
    boards: Res<Boards>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &crate::sim::Motives, &Transform), (With<crate::sim::HouseholdMember>, Without<crate::visit::OnLot>)>,
    seesaws: Query<(Entity, &GameObject, &Transform), (Without<crate::visit::LotObject>, Without<Sim>)>,
    mut next: Local<(f64, f64)>,
) {
    if std::env::var_os("SEESAW_TEST").is_some() {
        return;
    }
    let mut rng = rand::rng();
    // (Joining someone waiting: every ten minutes or so.)
    if clock.minutes >= next.1 {
        next.1 = clock.minutes + 10.0;
        for (s, b) in &boards.0 {
            if !b.joinable() || b.calling.is_some() || b.part != Part::Wait {
                continue;
            }
            let Ok((_, _, stf)) = seesaws.get(*s) else { continue };
            let first = b.riders[0];
            let keen = |sim: &Sim, m: &crate::sim::Motives| sim.age == Age::Child || sim.traits.contains(&crate::life::Trait::Childish) || m.0[crate::sim::FUN] < 30.0;
            let pick = sims
                .iter_mut()
                .filter(|(e, sim, q, m, tf)| *e != first.sim && q.0.is_empty() && can_play(sim.age) && keen(sim, m) && tf.translation.distance(stf.translation) < ASK_WITHIN)
                .min_by(|a, b| a.4.translation.distance(stf.translation).total_cmp(&b.4.translation.distance(stf.translation)));
            if let Some((_, sim, mut q, ..)) = pick
                && rng.random_bool(0.6)
            {
                q.0.push_back(Action::new(JOIN, join(*s, stf, first.north, sim.age == Age::Child), true));
                info!("seesaw: {} goes to join of their own accord", sim.first);
            }
        }
    }
    if clock.minutes < next.0 {
        return;
    }
    next.0 = clock.minutes + 45.0;
    for (_, sim, mut q, m, tf) in &mut sims {
        if !q.0.is_empty() || !matches!(sim.age, Age::Child | Age::Teen) || m.0[crate::sim::FUN] > 40.0 || !rng.random_bool(0.4) {
            continue;
        }
        let near = seesaws.iter().filter(|(e, o, _)| o.kind == ObjectKind::Seesaw && boards.0.get(e).is_none_or(|b| b.part == Part::Empty && b.joining.is_none())).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
        let Some((s, _, stf)) = near else { continue };
        let daredevil = sim.traits.contains(&crate::life::Trait::Daredevil);
        let (spot, extreme) = pick_spot(stf, tf.translation.xz(), sim.age == Age::Child, boards.north_up(s), daredevil, rng.random_range(0.0..1.0));
        q.0.push_back(Action::new(if daredevil { PLAY_ON_EXTREME } else { PLAY_ON }, sit(s, stf, spot, extreme, None), true));
        info!("seesaw: {} goes to play of their own accord", sim.first);
    }
}

/// `SEESAW_TEST=1` (tests): ten seconds in, a seesaw put down near the selected Sim (or
/// `SEESAW_WHO=<first name>` of the household), who plays on it: with `SEESAW_WITH=<first name>`
/// calling them over, with `SEESAW_JOIN=<first name>` joined by them; `SEESAW_DAREDEVIL=1` a
/// Daredevil getting on the daredevil's way; `SEESAW_MINUTES=<n>` told to stop after that long.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn seesaw_test(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    boards: Res<Boards>,
    mut sims: Query<(Entity, &mut Sim, &Transform, &mut ActionQueue, Has<crate::sim::Selected>), With<crate::sim::HouseholdMember>>,
    seesaws: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32, f64)>,
    grid: Option<Res<crate::nav::NavGrid>>,
) {
    if std::env::var_os("SEESAW_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let who = std::env::var("SEESAW_WHO").ok();
    let is_actor = |s: &Sim, selected: bool| who.as_ref().map_or(selected, |w| s.first.eq_ignore_ascii_case(w));
    let seesaw = seesaws.iter().find(|s| s.1.kind == ObjectKind::Seesaw).map(|s| (s.0, *s.2));
    if let (Ok(mut c), Some((_, stf))) = (cams.single_mut(), seesaw) {
        let side = (stf.rotation * Vec3::X).xz();
        c.look_at(stf.translation + Vec3::Y * 0.6);
        // (SEESAW_YAW=<radians>, SEESAW_PITCH: from elsewhere.)
        let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        c.yaw = side.x.atan2(side.y) + env("SEESAW_YAW", 0.35);
        c.pitch = env("SEESAW_PITCH", 0.5);
        c.distance = std::env::var("SEESAW_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(7.0);
    }
    match stage.0 {
        0 => {
            *stage = (1, now, clock.minutes);
            // (SEESAW_NEAR=<first name>: put down near them instead.)
            let near = std::env::var("SEESAW_NEAR").ok();
            let Some((_, _, tf, ..)) = sims.iter().find(|s| near.as_ref().map_or(is_actor(&s.1, s.4), |n| s.1.first.eq_ignore_ascii_case(n))) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("seesawClassic")).map(|c| c.objd) else { return };
            // (On open ground near them, its ends and sides free.)
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let clear = |c: Vec2| (-4..=4).all(|i| (-2..=2).all(|j| free(c + Vec2::new(j as f32 * 0.6, i as f32 * 0.8))));
            let here = tf.translation.xz();
            // (SEESAW_R=<metres>: at least that far off.)
            let from = std::env::var("SEESAW_R").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
            let spot = (from..from + 20).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32)).find(|c| clear(*c)).unwrap_or(here + Vec2::new(0.0, 5.0));
            let p = Vec3::new(spot.x, tf.translation.y, spot.y);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("seesaw test: a seesaw at {p:.1}");
            }
        }
        // (SEESAW_PIE=empty: its menu opened for the selected Sim instead; =join, once the one on it
        // is waiting.)
        1 if now - stage.1 > 2.0 && std::env::var("SEESAW_PIE").is_ok_and(|p| p == "empty") => {
            let Some((s, _)) = seesaw else { return };
            *stage = (9, now, clock.minutes);
            commands.insert_resource(crate::hud::PieFor(s));
            info!("seesaw test: its menu");
        }
        1 if now - stage.1 > 2.0 => {
            let Some((s, stf)) = seesaw else { return };
            *stage = (2, now, clock.minutes);
            let with = std::env::var("SEESAW_WITH").ok().and_then(|n| sims.iter().find(|x| x.1.first.eq_ignore_ascii_case(&n)).map(|x| x.0));
            let devil = std::env::var_os("SEESAW_DAREDEVIL").is_some();
            let Some((_, mut sim, tf, mut q, _)) = sims.iter_mut().find(|s| is_actor(&s.1, s.4)) else { return };
            if devil && !sim.traits.contains(&crate::life::Trait::Daredevil) {
                sim.traits.push(crate::life::Trait::Daredevil);
            }
            let (spot, extreme) = pick_spot(&stf, tf.translation.xz(), sim.age == Age::Child, boards.north_up(s), devil, if devil { 0.0 } else { 1.0 });
            let label = if with.is_some() { SEESAW_WITH } else if devil { PLAY_ON_EXTREME } else { PLAY_ON };
            q.push_player(Action::new(label, sit(s, &stf, spot, extreme, with), false));
            info!("seesaw test: {} plays on it from {spot:?}{}", sim.first, if with.is_some() { ", calling someone over" } else { "" });
        }
        2 => {
            let Some((s, stf)) = seesaw else { return };
            let Some(b) = boards.0.get(&s) else { return };
            if b.part != Part::Wait {
                return;
            }
            *stage = (3, now, clock.minutes);
            if std::env::var("SEESAW_PIE").is_ok_and(|p| p == "join") {
                commands.insert_resource(crate::hud::PieFor(s));
                info!("seesaw test: its menu, someone on it");
            }
            if let Ok(n) = std::env::var("SEESAW_JOIN")
                && let Some((_, sim, _, mut q, _)) = sims.iter_mut().find(|x| x.1.first.eq_ignore_ascii_case(&n))
            {
                q.push_player(Action::new(JOIN, join(s, &stf, b.riders[0].north, sim.age == Age::Child), false));
                info!("seesaw test: {} joins", sim.first);
            }
        }
        3 => {
            let Some(m) = std::env::var("SEESAW_MINUTES").ok().and_then(|v| v.parse::<f64>().ok()) else { return };
            if clock.minutes - stage.2 < m {
                return;
            }
            *stage = (4, now, clock.minutes);
            if let Some((_, sim, _, mut q, _)) = sims.iter_mut().find(|s| is_actor(&s.1, s.4))
                && let Some(a) = q.0.front_mut()
                && matches!(a.kind, ActionKind::Seesaw { .. })
            {
                a.cancel = true;
                info!("seesaw test: {} told to stop", sim.first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getting_on_is_the_games_clip_for_the_place_and_the_seesaw() {
        assert_eq!(get_on(Spot::N, true, false), "a2o_Seesaw_GetOn_Up_N_NN_x");
        assert_eq!(get_on(Spot::N, false, false), "a2o_Seesaw_GetOn_Down_N_x");
        assert_eq!(get_on(Spot::NE, false, false), "a2o_Seesaw_GetOn_Down_N_NE_x");
        assert_eq!(get_on(Spot::S, true, false), "a2o_Seesaw_GetOn_Down_S_x");
        assert_eq!(get_on(Spot::SW, false, false), "a2o_Seesaw_GetOn_Up_S_SW_x");
        assert_eq!(get_on(Spot::S, true, true), "a2o_Seesaw_GetOn_Daredevil_N_x");
        assert_eq!(Spot::for_age(false), [Spot::N, Spot::S]);
        assert_eq!(Spot::joining(true, true), Spot::NE);
        assert_eq!(Spot::joining(false, false), Spot::S);
    }

    #[test]
    fn each_clip_is_played_from_its_place() {
        assert_eq!(anchor("c2o_Seesaw_GetOn_Up_N_NE_x", true, Spot::NE, true), Spot::NE);
        assert_eq!(anchor("c2o_Seesaw_Idle_01_Down_N_x", true, Spot::NE, true), Spot::N);
        assert_eq!(anchor("a2o_Seesaw_GetOn_Daredevil_N_x", true, Spot::S, false), Spot::S);
        assert_eq!(anchor("a2o_Seesaw_GetOn_Up_N_x", false, Spot::S, false), Spot::S, "the one on it, from their end");
        assert_eq!(anchor("a2o_Seesaw_GetOn_Up_N_y", true, Spot::N, false), Spot::N, "the one joining, from where they came");
        assert_eq!(anchor("a2o_Seesaw_Seesaw_y", true, Spot::N, false), Spot::NE);
        assert_eq!(anchor("a2o_Seesaw_Idle_Balanced_x", false, Spot::S, false), Spot::S);
        assert_eq!(anchor("a2o_Seesaw_GetOff_Balanced_N_NE_y", true, Spot::N, false), Spot::N);
        assert_eq!(anchor("c2o_Seesaw_GetOff_Balanced_N_NE_y", true, Spot::NE, true), Spot::NE);
    }

    /// A clip whose `world_offset` is the anchor seen from the Sim (as the game's are).
    fn clip(p: [f32; 3], r: Quat) -> s3formats::sim::Clip {
        let mut c = s3formats::sim::Clip { duration: 1.0, ..Default::default() };
        c.tracks.insert(0x0EB4_037B, s3formats::sim::Track { translation: vec![(0.0, p)], rotation: vec![(0.0, r.to_array())] });
        c
    }

    #[test]
    fn the_clips_sit_them_on_the_seats() {
        let tf = Transform::IDENTITY;
        // (A child on from beside the north seat, turned to face along it: on the seat.)
        let c = clip([0.504, 0.0, 0.015], Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2));
        let (p, q) = root(&tf, Spot::NE, &c, 1.0);
        assert!((p.xz() - Vec2::new(0.0, 2.015)).length() < 0.01, "{p}");
        assert!(((q * Vec3::Z).xz() - Vec2::NEG_Y).length() < 0.01, "facing down the plank");
        // (The south one level, half a metre in from their end.)
        let (p, q) = root(&tf, Spot::S, &clip([0.0, 0.0, -0.46], Quat::IDENTITY), 0.0);
        assert!((p.xz() - Vec2::new(0.0, -2.04)).length() < 0.01 && ((q * Vec3::Z).xz() - Vec2::Y).length() < 0.01);
        // (The north one level, from beside their seat.)
        let (p, _) = root(&tf, Spot::NE, &clip([0.5, 0.0, 0.03], Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)), 0.0);
        assert!((p.xz() - Vec2::new(0.0, 2.03)).length() < 0.01, "{p}");
        // (A daredevil up the plank from the south end to the north seat, turned about.)
        let (p, q) = root(&tf, Spot::S, &clip([0.0, 0.0, 4.518], Quat::from_rotation_y(std::f32::consts::PI)), 0.0);
        assert!((p.xz() - Vec2::new(0.0, 2.018)).length() < 0.01 && ((q * Vec3::Z).xz() - Vec2::NEG_Y).length() < 0.01, "{p}");
    }

    #[test]
    fn the_parts_go_as_the_games_state_machine() {
        let sim = |n: u32| Entity::from_raw_u32(n).unwrap();
        let mut b = Board { riders: vec![Rider { sim: sim(1), north: false, spot: Spot::S, daredevil: false, child: false }], part: Part::GetOn, ..default() };
        assert_eq!(b.clips(Part::GetOn).0, ["a2o_Seesaw_GetOn_Down_S_x"]);
        assert_eq!(b.clips(Part::GetOn).1.as_deref(), Some("a2o_Seesaw_GetOn_Down_S_Seesaw"));
        assert_eq!(b.next(), Part::Wait);
        b.calling = Some(sim(3));
        assert_eq!(b.next(), Part::CallOver);
        b.part = Part::CallOver;
        b.calling = None;
        assert_eq!(b.next(), Part::Wait);
        // (A child joins at the north: their half and the one on it's, named for the north.)
        b.riders.push(Rider { sim: sim(2), north: true, spot: Spot::NE, daredevil: false, child: true });
        assert_eq!(b.next(), Part::Join);
        assert_eq!(b.clips(Part::Join).0, ["a2o_Seesaw_GetOn_Up_N_x", "a2o_Seesaw_GetOn_Up_N_y"]);
        assert_eq!(b.clips(Part::Join).1.as_deref(), Some("c2o_Seesaw_GetOn_Up_N_Seesaw"));
        assert_eq!(b.clips(Part::Teeter).0, ["a2o_Seesaw_Seesaw_x", "a2o_Seesaw_Seesaw_y"]);
        b.part = Part::Teeter;
        assert_eq!(b.next(), Part::Teeter);
        b.leaving = true;
        assert_eq!(b.next(), Part::Stop);
        b.part = Part::Stop;
        assert_eq!(b.next(), Part::GetOffTwo);
        assert_eq!(b.clips(Part::GetOffTwo).0, ["a2o_Seesaw_GetOff_Balanced_S_SW_x", "a2o_Seesaw_GetOff_Balanced_N_NE_y"]);
    }
}
