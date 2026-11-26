//! Treehouses, Generations' (`Treehouse`: The Princess and the Pauper Castle, the Kid's
//! Clubhouse, the Sci-Fi Hideaway). Anyone may "Get in" one (`GetIn`): up its ladder with the
//! game's climb (`c2o_treehouse_getIn`, a grown-up's `a2o_treehouse_woohoo_getIn`), out of sight
//! once inside (its `disappear_sim` event), their plumbob floating over the roof (its plumbob
//! slots). Up to four at once (`kNumberOfOccupants`), children and grown-ups never together: a
//! grown-up getting in first calls the children down ("Ask Everyone to Come Down",
//! `AskToComeDown`: from two to five metres off, the game's call-over, and down they come, a Heavy
//! Sleeper asleep in it half the time not). Children "Play in" it (`PlayIn`: Fun 60 an hour): now
//! inside, now at one of its windows looking out (the castle's, the clubhouse's with a spyglass,
//! the hideaway's with an alien scanner: `Look Out`), now sitting out on its balcony (`Sit
//! Outside`), a quarter to half an hour at each (`kPlayWithPartDuration...`), never the same part
//! twice running and never one another child's at; and play its pretend game ("Hold Great Feast",
//! "Fight Pirate Invaders", "Explore New Worlds": `PlayPretend`, Fun 60), the treehouse sparkling
//! and sounding with it (its `ep4PretendTreeHouse...` effect and `thouse_pretend_..._loop`) and
//! the other children playing in it joining in. They chat together as they play (`TryGroupTalk`).
//! A child may "Sleep" in it (Energy 21.33 an hour) till rested, woken by any pretending there.
//! Leaving (`GetOut`, the posture's standing transition: whatever's asked of them next outside
//! it), children slide down the clubhouse's slide, the castle's pole, or are beamed down from the
//! hideaway (`Repoman.FireTeleportEffect`); grown-ups climb down the ladder. It's one of the
//! playground's things for "Play on Playground" (forty minutes in it, the XML's
//! `kPlayOnPlaygroundDuration`), and children short of fun go and play in it of their own accord.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct TreehousePlugin;

impl Plugin for TreehousePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Houses>()
            .add_systems(Update, tag.run_if(in_state(crate::AppState::InGame)))
            .add_systems(Update, begin.after(crate::playground::begin).before(crate::seesaw::hold).before(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, (run, show, pretend_effects, autonomy, treehouse_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's words (`Gameplay/Objects/Environment/Treehouse`).
pub const GET_OUT: &str = "Get Out";
pub const SLEEP: &str = "Sleep";
pub const ASK_DOWN: &str = "Ask Everyone to Come Down";
pub const FULL: &str = "The treehouse is already full";
pub fn get_in(name: &str) -> String {
    format!("Get in {name}")
}
pub fn play_in(name: &str) -> String {
    format!("Play in {name}")
}

/// The game's tuning (its XML's): how many fit in one, how long a child plays at one part
/// (minutes), what it's worth an hour; the call-over's distance; how likely a Heavy Sleeper stays
/// asleep when called down.
pub const OCCUPANTS: usize = 4;
const PART_MINUTES: (f64, f64) = (15.0, 30.0);
const PLAY_FUN: f32 = 60.0;
const SLEEP_ENERGY: f32 = 21.33;
const CALL_FROM: (f32, f32) = (2.0, 5.0);
const HEAVY_SLEEPER_STAYS: f64 = 0.5;
/// Chatting together as they play (`TryGroupTalk`): company an hour, and liking.
const CHAT_SOCIAL: f32 = 40.0;
const CHAT_LIKING: f32 = 3.0;
/// How often the treehouse sparkles while pretending's going on in it (seconds).
const SPARKLE_EVERY: f32 = 2.5;

/// The three treehouses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Castle,
    Club,
    Space,
}

impl Theme {
    pub fn of(script: &str) -> Theme {
        let s = script.to_ascii_lowercase();
        if s.contains("castle") {
            Theme::Castle
        } else if s.contains("space") {
            Theme::Space
        } else {
            Theme::Club
        }
    }

    /// Its pretend game (`PlayPretendInteractionName...`).
    pub fn pretend(self) -> &'static str {
        match self {
            Theme::Castle => "Hold Great Feast",
            Theme::Club => "Fight Pirate Invaders",
            Theme::Space => "Explore New Worlds",
        }
    }

    /// Its word in the clips' names (`c2o_treehouse_castle_playPretend1`, `windowClubHouse`).
    fn word(self) -> &'static str {
        match self {
            Theme::Castle => "castle",
            Theme::Club => "clubHouse",
            Theme::Space => "spaceAge",
        }
    }

    fn window(self) -> &'static str {
        match self {
            Theme::Castle => "Castle",
            Theme::Club => "ClubHouse",
            Theme::Space => "SpaceAge",
        }
    }

    /// The pretend game's sounds about the treehouse (`PlayPretendSoundLoopName`, `...PListName`).
    pub fn sounds(self) -> (&'static str, &'static str) {
        match self {
            Theme::Castle => ("thouse_pretend_castle_loop", "thouse_pretend_castle_plist"),
            Theme::Club => ("thouse_pretend_pirates_loop", "thouse_pretend_pirates_plist"),
            Theme::Space => ("thouse_pretend_space_loop", "thouse_pretend_space_plist"),
        }
    }

    /// How a child comes down (`ExitSlot`): the castle's pole, the clubhouse's slide, the
    /// hideaway's beam; its slot, and its clip (the beam has none).
    fn exit(self) -> (&'static str, Option<&'static str>) {
        match self {
            Theme::Castle => ("_route_exit_pole", Some("c2o_treehouse_getOut_castlePole_x")),
            Theme::Club => ("_route_exit_slide", Some("c2o_treehouse_getOut_clubHouseSlide_x")),
            Theme::Space => ("_route_exit_teleport", None),
        }
    }
}

/// The treehouse's parts a child plays at (`UsableParts`): its windows and balconies.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Part {
    WindowA,
    WindowB,
    WindowC,
    BalconyA,
    BalconyB,
}

impl Part {
    /// The ones played at alone (`PlayableParts`: the second balcony only with another on the
    /// first).
    pub const PLAYABLE: [Part; 4] = [Part::WindowA, Part::WindowB, Part::WindowC, Part::BalconyA];

    /// Its slot, as the treehouse's rig names it.
    fn slot(self) -> &'static str {
        match self {
            Part::WindowA => "_ctnm_window",
            Part::WindowB => "_ctnm_window_01",
            Part::WindowC => "_ctnm_window_02",
            Part::BalconyA => "_ctnm_0_Balcony",
            Part::BalconyB => "_ctnm_1_Balcony",
        }
    }

    /// Its clips: in, its loop, out (the first balcony's played mirrored, as the game's
    /// `isMirrored`).
    pub fn clips(self, theme: Theme) -> (String, String, String, bool) {
        match self {
            Part::BalconyA | Part::BalconyB => ("c2o_treehouse_balconySit_start_x".into(), "c2o_treehouse_balconySit_lookAround_x".into(), "c2o_treehouse_balconySit_stop_x".into(), self == Part::BalconyA),
            _ => {
                let w = theme.window();
                (format!("c2o_treehouse_window{w}_lookOut_start_x"), format!("c2o_treehouse_window{w}_lookOut_loop_x"), format!("c2o_treehouse_window{w}_lookOut_stop_x"), false)
            }
        }
    }
}

/// What a Sim's at in or at a treehouse.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    GetIn,
    Play,
    Pretend,
    Sleep,
    GetOut,
    AskDown,
}

impl Act {
    /// Done inside it (so gone up first).
    pub fn inside(self) -> bool {
        matches!(self, Act::Play | Act::Pretend | Act::Sleep)
    }
}

/// Where its slots are, by the names its rig gives them, should it have none (the castle's): place,
/// and which way they face (degrees about the vertical).
const SLOTS: &[(&str, [f32; 3], f32)] = &[
    ("_route_climb_", [0.0, 0.0, 1.29078], 180.0),
    ("_route_water_", [-0.738261, 0.0, 2.39749], 180.0),
    ("_route_exit_pole", [-1.0, 0.0, -1.0], 0.0),
    ("_route_exit_slide", [1.35996, 0.0, 1.18179], 0.0),
    ("_route_exit_teleport", [-1.78392, 0.0, -0.728089], 0.0),
    ("_ctnm_0_hide", [-1.00038, 4.054342, 0.56], 0.0),
    ("_ctnm_1_hide", [0.05288, 4.054342, 0.56], 0.0),
    ("_ctnm_2_hide", [0.05288, 4.054342, -0.773695], 0.0),
    ("_ctnm_3_hide", [-1.00038, 4.054342, -0.773695], 0.0),
    ("_ctnm_window", [-0.738261, 4.054342, 1.51121], 0.0),
    ("_ctnm_window_01", [-0.738261, 4.054342, -1.51697], 180.0),
    ("_ctnm_window_02", [-0.268959, 5.26955, -0.009584], 90.0),
    ("_ctnm_0_Balcony", [0.966984, 4.054342, -0.571332], 45.0),
    ("_ctnm_1_Balcony", [0.966984, 4.054342, 0.565024], 135.0),
    ("_FX_0_", [0.0, 5.0, 0.0], 0.0),
    ("_FX_0_plumbob", [-1.00039, 6.8, 0.764184], 0.0),
    ("_FX_1_plumbob", [0.05288, 6.8, 0.764184], 0.0),
    ("_FX_2_plumbob", [0.05288, 6.8, -0.773695], 0.0),
    ("_FX_3_plumbob", [-1.00038, 6.8, -0.773695], 0.0),
];

const HIDE: [&str; 4] = ["_ctnm_0_hide", "_ctnm_1_hide", "_ctnm_2_hide", "_ctnm_3_hide"];
const PLUMBOB: [&str; 4] = ["_FX_0_plumbob", "_FX_1_plumbob", "_FX_2_plumbob", "_FX_3_plumbob"];

/// A slot in the world, on a treehouse standing so: from its rig, else as the castle has it.
pub fn slot(house: &Transform, rig: Option<&crate::objanim::ObjectRig>, name: &str) -> Transform {
    let local = rig.and_then(|r| r.bone_in_model(None, 0.0, s3pkg::fnv32(name))).map(|m| {
        let (_, r, p) = m.to_scale_rotation_translation();
        Transform::from_translation(p).with_rotation(r)
    });
    let local = local.or_else(|| {
        SLOTS.iter().find(|s| s.0 == name).map(|(_, p, yaw)| Transform::from_translation(Vec3::from(*p)).with_rotation(Quat::from_rotation_y(yaw.to_radians())))
    });
    let local = local.unwrap_or_default();
    Transform { translation: house.transform_point(local.translation), rotation: house.rotation * local.rotation, scale: Vec3::ONE }
}

/// Where Sims climb in (its `_route_climb_` routing slot), and the way they face there: the same
/// on every treehouse.
pub fn ladder(house: &Transform) -> (Vec2, Vec2) {
    let s = slot(house, None, "_route_climb_");
    (s.translation.xz(), (s.rotation * Vec3::Z).xz().normalize_or(Vec2::NEG_Y))
}

/// A treehouse's own: its theme and its name (its catalogue name, as the game's
/// `RandomName...`).
#[derive(Component, Clone, Debug)]
pub struct HouseInfo {
    pub theme: Theme,
    pub name: String,
}

/// Who's in each treehouse (and how: a grown-up, a child), and who's pretending there; whose turn
/// on the ladder.
#[derive(Resource, Default)]
pub struct Houses(pub HashMap<Entity, House>);

#[derive(Default, Debug, Clone)]
pub struct House {
    pub inside: Vec<(Entity, bool)>,
    pub places: Vec<usize>,
    pub pretending: usize,
    pub parts: Vec<Part>,
    pub ladder: Option<Entity>,
    /// Children playing about inside, free to be asked out onto the second balcony; and whether
    /// one's on their way there.
    pub idle: Vec<Entity>,
    pub b_coming: bool,
    /// Those on their way down.
    pub leaving: Vec<Entity>,
}

impl Houses {
    pub fn get(&self, house: Entity) -> House {
        self.0.get(&house).cloned().unwrap_or_default()
    }

    /// Whether there's room for one more (`IsAvailable`): a grown-up only in an empty one, a child
    /// in one with no grown-ups and a place free.
    pub fn room(&self, house: Entity, me: Entity, grown: bool) -> bool {
        let h = self.get(house);
        let others: Vec<&(Entity, bool)> = h.inside.iter().filter(|x| x.0 != me).collect();
        if grown { others.is_empty() } else { others.len() < OCCUPANTS && !others.iter().any(|x| x.1) }
    }

    /// Children up there and staying (not already on their way down).
    pub fn children_staying(&self, house: Entity) -> bool {
        let h = self.get(house);
        h.inside.iter().any(|x| !x.1 && !h.leaving.contains(&x.0))
    }
}

/// Who's a grown-up here: teens and older (climbing as grown-ups do).
pub fn grown(age: Age) -> bool {
    !matches!(age, Age::Child | Age::Toddler | Age::Baby)
}

/// A Sim in a treehouse (the game's `TreehousePosture`): which, at which of its places, what
/// they're at there; at a window or on a balcony, which, how far through (its way in, its loop,
/// its way out), and till when.
#[derive(Component, Debug)]
pub struct InTreehouse {
    pub house: Entity,
    pub place: usize,
    pub grown: bool,
    pub doing: Option<Act>,
    pub part: Option<Part>,
    stage: u8,
    t: f32,
    len: f32,
    until: f64,
    prev: Option<Part>,
    parity: bool,
    since: f64,
    /// Asked to come and sit on the second balcony beside the one on the first (`RequestBalconyB`).
    want_b: bool,
}

/// Climbing in or out: how far, how long, when they vanish inside or show coming out.
#[derive(Component, Debug)]
pub struct Climbing {
    pub house: Entity,
    pub out: bool,
    t: f32,
    len: f32,
    hide_at: Option<f32>,
    show_at: f32,
}

/// Calling those in it down: how far through the call.
#[derive(Component)]
struct Calling(f32);

/// Treehouses given their theme and name as they come.
fn tag(mut commands: Commands, data: Option<Res<Baked>>, objects: Query<(Entity, &GameObject), Without<HouseInfo>>) {
    let Some(data) = data else { return };
    for (e, o) in &objects {
        if o.kind != ObjectKind::Treehouse {
            continue;
        }
        let script = data.0.catalog.iter().find(|c| c.objd == o.objd).map_or(String::new(), |c| c.script.clone());
        commands.entity(e).insert(HouseInfo { theme: Theme::of(&script), name: o.name.clone() });
    }
}

/// The action for an act at a treehouse: gone up the ladder for it.
pub fn act(house: Entity, htf: &Transform, act: Act) -> ActionKind {
    let (at, face) = ladder(htf);
    ActionKind::Treehouse { house, act, at, face }
}

/// Its words for an act.
pub fn label(act: Act, info: &HouseInfo) -> String {
    match act {
        Act::GetIn => get_in(&info.name),
        Act::Play => play_in(&info.name),
        Act::Pretend => info.theme.pretend().to_string(),
        Act::Sleep => SLEEP.to_string(),
        Act::GetOut => GET_OUT.to_string(),
        Act::AskDown => ASK_DOWN.to_string(),
    }
}

/// A treehouse's menu for a Sim: outside, "Get in" (greyed when full), and a child's "Play in",
/// its pretend game, "Sleep"; a teen or grown-up's "Ask Everyone to Come Down" with children up
/// there. In it, "Get Out", and a child's others but the one they're at.
pub fn options(house: Entity, htf: &Transform, info: &HouseInfo, houses: &Houses, me: Entity, age: Age, inside: Option<&InTreehouse>) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    let child = age == Age::Child;
    let here = inside.filter(|i| i.house == house);
    if let Some(i) = here {
        out.push((GET_OUT.to_string(), act(house, htf, Act::GetOut)));
        if child {
            for a in [Act::Play, Act::Pretend, Act::Sleep] {
                if i.doing != Some(a) {
                    out.push((label(a, info), act(house, htf, a)));
                }
            }
        }
        return out;
    }
    if !child && !grown(age) {
        return out;
    }
    let room = houses.room(house, me, grown(age));
    if grown(age) && houses.children_staying(house) {
        out.push((ASK_DOWN.to_string(), ask_down(house, htf, None)));
    }
    out.push((get_in(&info.name), if room || grown(age) { act(house, htf, Act::GetIn) } else { ActionKind::Greyed(FULL.into()) }));
    if child && room {
        for a in [Act::Play, Act::Pretend, Act::Sleep] {
            out.push((label(a, info), act(house, htf, a)));
        }
    }
    out
}

/// Calling them down from two to five metres off the ladder, on the caller's side.
pub fn ask_down(house: Entity, htf: &Transform, from: Option<Vec2>) -> ActionKind {
    let (l, face) = ladder(htf);
    let away = from.map_or(-face, |f| (f - l).normalize_or(-face));
    let at = l + away * (CALL_FROM.0 + CALL_FROM.1) * 0.5;
    ActionKind::Treehouse { house, act: Act::AskDown, at, face: (l - at).normalize_or(face) }
}

/// The treehouse an action's at, and what.
fn at_house(a: &Action) -> Option<(Entity, Act)> {
    match a.kind {
        ActionKind::Treehouse { house, act, .. } => Some((house, act)),
        _ => None,
    }
}

/// Before anything's done: a child asked to play (or anyone to get in) goes up first; one in a
/// treehouse asked to do something else gets out first (the posture's standing transition);
/// what's done inside begins there and then, with no walk; told to stop playing with nothing
/// else to do up there, they come down.
#[allow(clippy::type_complexity)]
fn begin(mut sims: Query<(Entity, &Sim, &mut ActionQueue, Option<&InTreehouse>, Has<Climbing>)>, houses_q: Query<(&Transform, &HouseInfo)>) {
    for (_, sim, mut q, inside, climbing) in &mut sims {
        // (Part way up or down: finished first, as the game's climb can't be stopped once begun.)
        if climbing {
            if let Some(a) = q.0.front_mut()
                && at_house(a).is_some()
            {
                a.cancel = false;
            }
            continue;
        }
        // (Each change to the queue's head looked at again, till it's settled: nothing done inside
        // is ever walked to.)
        for _ in 0..4 {
            if !settle(sim, &mut q, inside, &houses_q) {
                break;
            }
        }
    }
}

/// One step of `begin` for a Sim's queue: whether it changed what's first.
fn settle(sim: &Sim, q: &mut ActionQueue, inside: Option<&InTreehouse>, houses_q: &Query<(&Transform, &HouseInfo)>) -> bool {
    // (Told to stop what they're at inside, nothing else for up there next: down they come.)
    if let Some(i) = inside
        && let Some(a) = q.0.front_mut()
        && a.cancel
        && matches!(a.phase, Phase::Running(_))
        && at_house(a).is_some_and(|(h, act)| h == i.house && act.inside())
    {
        debug!("treehouse: {} told to stop what they're at in it", sim.first);
        q.0.pop_front();
        let next_inside = q.0.iter().find(|a| !a.cancel).and_then(at_house).is_some_and(|(h, act)| h == i.house && act != Act::GetOut);
        if !next_inside && let Ok((htf, _)) = houses_q.get(i.house) {
            q.0.push_front(Action::new(GET_OUT, act(i.house, htf, Act::GetOut), false));
        }
        return true;
    }
    let Some(a) = q.0.front() else { return false };
    if a.cancel {
        return false;
    }
    let front = at_house(a);
    match (inside, front) {
        // (In one, asked to do something elsewhere: out first.)
        (Some(i), f) if f.is_none_or(|(h, _)| h != i.house) => {
            let Ok((htf, _)) = houses_q.get(i.house) else { return false };
            debug!("treehouse: {} gets out first", sim.first);
            q.0.push_front(Action::new(GET_OUT, act(i.house, htf, Act::GetOut), false));
            true
        }
        // (Inside already: on with it there.)
        (Some(_), Some((_, act))) if act != Act::AskDown => {
            let Some(a) = q.0.front_mut().filter(|a| a.phase == Phase::Start) else { return false };
            if act == Act::GetIn {
                q.0.pop_front();
            } else {
                a.phase = Phase::Running(0.0);
            }
            true
        }
        // (Not in it: up first for what's done inside; getting out of one they're not in,
        // nothing to do.)
        (None, Some((h, act))) if act.inside() && a.phase == Phase::Start => {
            let Ok((htf, info)) = houses_q.get(h) else { return false };
            let autonomous = a.autonomous;
            q.0.push_front(Action::new(get_in(&info.name), self::act(h, htf, Act::GetIn), autonomous));
            true
        }
        (None, Some((_, Act::GetOut))) => {
            q.0.pop_front();
            true
        }
        _ => false,
    }
}

/// A clip's name as a one-clip loop list, kept for good.
fn looping(name: String) -> &'static [&'static str] {
    use std::sync::{Mutex, OnceLock};
    static LISTS: OnceLock<Mutex<HashMap<String, &'static [&'static str]>>> = OnceLock::new();
    let mut m = LISTS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    m.entry(name.clone()).or_insert_with(|| Box::leak(vec![crate::pets::intern(name)].into_boxed_slice()))
}

/// One-off clip, told apart from the last so it plays again.
fn once(name: &'static str, parity: bool) -> ActionClip {
    let c = ActionClip::new(Some(name), &[]);
    if parity { c.in_order() } else { c }
}

/// Acts played out: the climbs, playing at the parts, pretending, sleeping, calling down; and
/// who's in each treehouse kept count of.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut houses: ResMut<Houses>,
    mut sims: Query<
        (Entity, &Sim, &mut ActionQueue, &mut Transform, &mut SimAnim, &mut crate::sim::Motives, Option<&mut InTreehouse>, Option<&mut Climbing>, Option<&mut Calling>, &mut Visibility),
        Without<HouseInfo>,
    >,
    house_q: Query<(&Transform, &HouseInfo, Option<&crate::objanim::ObjectRig>), Without<Sim>>,
    mut sparkles: MessageWriter<crate::effects::SpinSparkle>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    // Who's where (as it stood last).
    let snapshot = std::mem::take(&mut houses.0);
    let mut fresh: HashMap<Entity, House> = HashMap::new();
    for (e, _, q, _, _, _, inside, climbing, ..) in sims.iter() {
        if let Some(i) = inside {
            let h = fresh.entry(i.house).or_default();
            h.inside.push((e, i.grown));
            h.places.push(i.place);
            if climbing.is_some_and(|c| c.out) || q.0.front().and_then(at_house).is_some_and(|(x, a)| x == i.house && a == Act::GetOut) {
                h.leaving.push(e);
            }
            if i.doing == Some(Act::Pretend) {
                h.pretending += 1;
            }
            if let Some(p) = i.part {
                h.parts.push(p);
            }
            if i.want_b {
                h.b_coming = true;
            } else if i.doing == Some(Act::Play) && i.part.is_none() && !i.grown {
                h.idle.push(e);
            }
        }
        if let Some(c) = climbing {
            fresh.entry(c.house).or_default().ladder = Some(e);
        }
    }
    for (h, old) in &snapshot {
        if let Some(l) = old.ladder
            && fresh.get(h).is_none_or(|f| f.ladder.is_none())
            && sims.get(l).is_ok_and(|s| s.2.0.front().and_then(at_house).is_some_and(|(x, a)| x == *h && a == Act::GetIn))
        {
            fresh.entry(*h).or_default().ladder = Some(l);
        }
    }
    houses.0 = fresh;
    let mut chat: HashMap<Entity, Vec<Entity>> = HashMap::new();
    let mut calls: Vec<Entity> = Vec::new();
    let mut woken: Vec<Entity> = Vec::new();
    let mut balcony_b: Vec<Entity> = Vec::new();
    for (e, sim, mut q, mut tf, mut anim, mut m, mut inside, mut climbing, calling, mut vis) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(at_house);
        let grown_up = grown(sim.age);
        // (A call cut short.)
        if calling.is_some() && front.is_none_or(|f| f.1 != Act::AskDown) {
            commands.entity(e).remove::<(Calling, ActionClip)>();
        }
        // Climbing in or out.
        if let Some(c) = climbing.as_deref_mut() {
            c.t += dt;
            if c.hide_at.is_some_and(|h| c.t >= h) {
                *vis = Visibility::Hidden;
            }
            if c.out && c.t >= c.show_at {
                *vis = Visibility::Inherited;
            }
            if c.t < c.len {
                continue;
            }
            let (house, out) = (c.house, c.out);
            commands.entity(e).remove::<(Climbing, ActionClip)>();
            q.0.pop_front();
            if out {
                commands.entity(e).remove::<(InTreehouse, crate::sim::PlumbobOver)>();
                anim.pose = Pose::Stand;
                info!("treehouse: {} is out", sim.first);
            } else {
                let taken = houses.get(house).places;
                let place = (0..OCCUPANTS).find(|p| !taken.contains(p)).unwrap_or(0);
                let h = houses.0.entry(house).or_default();
                h.inside.push((e, grown_up));
                h.places.push(place);
                commands.entity(e).insert(InTreehouse { house, place, grown: grown_up, doing: None, part: None, stage: 0, t: 0.0, len: 0.0, until: 0.0, prev: None, parity: false, since: now, want_b: false });
                anim.pose = Pose::Use;
                info!("treehouse: {} is in", sim.first);
            }
            continue;
        }
        let Some((house, act)) = front else {
            // (In it with nothing to do: about inside.)
            if let Some(i) = inside.as_deref_mut()
                && i.doing.is_some()
            {
                i.doing = None;
                i.part = None;
                i.stage = 0;
                commands.entity(e).remove::<ActionClip>();
            }
            continue;
        };
        let Ok((htf, info, rig)) = house_q.get(house) else {
            q.0.pop_front();
            continue;
        };
        match act {
            Act::GetIn => {
                let h = houses.get(house);
                if !houses.room(house, e, grown_up) || h.ladder.is_some_and(|l| l != e) {
                    // (A grown-up with children up there: calls them down first.)
                    if grown_up && houses.children_staying(house) && !q.0.iter().any(|a| at_house(a).is_some_and(|(x, a)| x == house && a == Act::AskDown)) {
                        q.0.push_front(Action::new(ASK_DOWN, ask_down(house, htf, Some(tf.translation.xz())), false));
                    }
                    continue;
                }
                houses.0.entry(house).or_default().ladder = Some(e);
                let at = slot(htf, rig, "_route_climb_");
                tf.translation = at.translation;
                tf.rotation = at.rotation;
                let (clip, hide_at): (&'static str, f32) = if grown_up { ("a2o_treehouse_woohoo_getIn_x", 5.4) } else { ("c2o_treehouse_getIn_x", 5.433) };
                let len = lib.get(&data, clip).map_or(6.0, |c| c.duration);
                anim.pose = Pose::Use;
                commands.entity(e).insert((Climbing { house, out: false, t: 0.0, len, hide_at: Some(hide_at), show_at: 0.0 }, ActionClip::new(Some(clip), &[])));
                info!("treehouse: {} climbs into {}", sim.first, info.name);
            }
            Act::GetOut => {
                let Some(i) = inside.as_deref() else {
                    q.0.pop_front();
                    continue;
                };
                let (slot_name, clip): (&str, Option<&'static str>) = if i.grown { ("_route_climb_", Some("a2o_treehouse_woohoo_getOut_x")) } else { info.theme.exit() };
                let at = slot(htf, rig, slot_name);
                tf.translation = at.translation;
                tf.rotation = at.rotation;
                anim.pose = Pose::Use;
                let (len, show_at) = match clip {
                    Some(c) => (lib.get(&data, c).map_or(2.0, |x| x.duration), if i.grown { 0.167 } else { 0.0 }),
                    None => (1.0, 0.0),
                };
                let mut ec = commands.entity(e);
                ec.remove::<crate::sim::PlumbobOver>().insert(Climbing { house, out: true, t: 0.0, len, hide_at: None, show_at });
                match clip {
                    Some(c) => {
                        ec.insert(ActionClip::new(Some(c), &[]));
                    }
                    // (Beamed down from the hideaway.)
                    None => {
                        ec.remove::<ActionClip>();
                        sparkles.write(crate::effects::SpinSparkle::on(e, 1.15));
                    }
                }
                info!("treehouse: {} gets out of {}", sim.first, info.name);
            }
            Act::AskDown => {
                match calling {
                    None => {
                        anim.pose = Pose::Use;
                        commands.entity(e).insert((Calling(0.0), ActionClip::new(Some("a_soc_callOver_x"), &[])));
                    }
                    Some(mut c) => {
                        c.0 += dt;
                        let len = lib.get(&data, "a_soc_callOver_x").map_or(2.0, |x| x.duration);
                        if c.0 >= len {
                            commands.entity(e).remove::<(Calling, ActionClip)>();
                            q.0.pop_front();
                            anim.pose = Pose::Stand;
                            calls.push(house);
                            info!("treehouse: {} calls everyone down from {}", sim.first, info.name);
                        }
                    }
                }
            }
            Act::Play | Act::Pretend | Act::Sleep => {
                let Some(i) = inside.as_deref_mut() else {
                    q.0.pop_front();
                    continue;
                };
                if i.doing != Some(act) {
                    // (On to something else in it: the part they were at left first.)
                    if i.part.is_some() {
                        step_out(&mut commands, e, i, info.theme, &mut lib, &data, dt);
                        continue;
                    }
                    i.doing = Some(act);
                    i.since = now;
                    i.until = 0.0;
                    commands.entity(e).remove::<ActionClip>();
                    if act == Act::Pretend {
                        commands.entity(e).insert(ActionClip::new(None, looping(format!("c2o_treehouse_{}_playPretend", info.theme.word()))));
                        // (The others playing in it join in, `AddNext`.)
                        for (o, _) in houses.get(house).inside.iter().filter(|x| x.0 != e && !x.1) {
                            calls_pretend(&mut commands, *o, house, *htf);
                        }
                        info!("treehouse: {} plays {}", sim.first, info.theme.pretend());
                    } else {
                        info!("treehouse: {} {}", sim.first, if act == Act::Play { "plays in it" } else { "sleeps in it" });
                    }
                    continue;
                }
                // What it's worth.
                if hours > 0.0 {
                    match act {
                        Act::Sleep => m.add(crate::sim::ENERGY, SLEEP_ENERGY * hours),
                        _ => m.add(crate::sim::FUN, PLAY_FUN * hours),
                    }
                }
                if act != Act::Sleep {
                    chat.entry(house).or_default().push(e);
                }
                // Had enough: done (nothing else to do, they stay up there).
                let needy = [crate::sim::HUNGER, crate::sim::BLADDER].iter().any(|k| m.0[*k] < -40.0) || (act != Act::Sleep && m.0[crate::sim::ENERGY] < -60.0);
                let done = match act {
                    Act::Sleep => m.0[crate::sim::ENERGY] >= 98.0,
                    _ => m.0[crate::sim::FUN] >= 99.0 && now - i.since > 30.0,
                };
                if done || needy {
                    if i.part.is_some() {
                        step_out(&mut commands, e, i, info.theme, &mut lib, &data, dt);
                        continue;
                    }
                    info!("treehouse: {} has had enough of it", sim.first);
                    q.0.pop_front();
                    i.doing = None;
                    commands.entity(e).remove::<ActionClip>();
                    continue;
                }
                // (Asleep with pretending going on: woken, and out.)
                if act == Act::Sleep && houses.get(house).pretending > 0 {
                    woken.push(e);
                    continue;
                }
                if act != Act::Play {
                    continue;
                }
                // Playing: at a part a while, then another.
                match (i.part, i.stage) {
                    (Some(p), 0) => {
                        i.t += dt;
                        if i.t >= i.len {
                            let (_, l, _, mirror) = p.clips(info.theme);
                            let c = ActionClip::new(None, looping(l));
                            commands.entity(e).insert(if mirror { c.mirrored() } else { c });
                            i.stage = 1;
                        }
                    }
                    // (Time to move on; or on a balcony with nobody beside them any more.)
                    (Some(p), 1) if now >= i.until || alone_on_balcony(p, &houses.get(house)) => leave_part(&mut commands, e, i, info.theme, &mut lib, &data),
                    (Some(_), 2) => {
                        if step_out(&mut commands, e, i, info.theme, &mut lib, &data, dt) {
                            i.until = now + rng.random_range(PART_MINUTES.0..PART_MINUTES.1) * 0.5;
                        }
                    }
                    (None, _) if now >= i.until => {
                        let h = houses.get(house);
                        let used = h.parts.clone();
                        let mut free: Vec<Part> = Part::PLAYABLE.iter().copied().filter(|p| !used.contains(p) && !same_part(Some(*p), i.prev)).collect();
                        // (The first balcony only with another to sit beside on the second: one
                        // playing about inside, asked out there.)
                        let partner = h.idle.iter().copied().find(|o| *o != e);
                        if partner.is_none() || used.contains(&Part::BalconyB) {
                            free.retain(|p| *p != Part::BalconyA);
                        }
                        // (Nothing free: about inside a while.)
                        let mut pick = if free.is_empty() { None } else { Some(free[rng.random_range(0..free.len())]) };
                        if i.want_b {
                            i.want_b = false;
                            pick = (!used.contains(&Part::BalconyB) && used.contains(&Part::BalconyA)).then_some(Part::BalconyB).or(pick);
                        }
                        if pick == Some(Part::BalconyA)
                            && let Some(o) = partner
                        {
                            balcony_b.push(o);
                        }
                        match pick {
                            Some(p) => {
                                let (s, _, _, mirror) = p.clips(info.theme);
                                let s: &'static str = crate::pets::intern(s);
                                i.part = Some(p);
                                i.stage = 0;
                                i.t = 0.0;
                                i.len = lib.get(&data, s).map_or(2.0, |c| c.duration);
                                i.until = now + rng.random_range(PART_MINUTES.0..PART_MINUTES.1);
                                i.parity = !i.parity;
                                let c = once(s, i.parity);
                                commands.entity(e).insert(if mirror { c.mirrored() } else { c });
                                houses.0.entry(house).or_default().parts.push(p);
                                debug!("treehouse: {} goes to the {p:?}", sim.first);
                            }
                            None => {
                                i.prev = None;
                                i.until = now + rng.random_range(PART_MINUTES.0..PART_MINUTES.1) * 0.5;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // (Chatting together up there: company, and liking.)
    if hours > 0.0 {
        for (_, who) in chat.iter().filter(|(_, w)| w.len() > 1) {
            for &a in who {
                if let Ok((_, _, _, _, _, mut m, ..)) = sims.get_mut(a) {
                    m.add(crate::sim::SOCIAL, CHAT_SOCIAL * hours);
                }
                for &b in who.iter().filter(|b| **b != a) {
                    let l = CHAT_LIKING * hours;
                    commands.queue(move |w: &mut World| {
                        if let Some(mut r) = w.get_mut::<crate::social::Relationships>(a) {
                            r.add(b, l, 0.0);
                        }
                    });
                }
            }
        }
    }
    // (Called down: out they come; a Heavy Sleeper asleep, maybe not.)
    for house in calls {
        let Ok((htf, ..)) = house_q.get(house) else { continue };
        for (o, grown_o) in houses.get(house).inside.clone() {
            if grown_o {
                continue;
            }
            let Ok((_, sim, mut q, .., inside, _, _, _)) = sims.get_mut(o) else { continue };
            let asleep = inside.is_some_and(|i| i.doing == Some(Act::Sleep));
            if asleep && sim.traits.contains(&crate::life::Trait::HeavySleeper) && rng.random_bool(HEAVY_SLEEPER_STAYS) {
                continue;
            }
            come_down(&mut q, house, htf);
        }
    }
    // (Asked out to sit on the second balcony: now.)
    for o in balcony_b {
        if let Ok((.., Some(mut inside), _, _, _)) = sims.get_mut(o) {
            inside.want_b = true;
            inside.until = 0.0;
        }
    }
    for o in woken {
        let Ok((_, sim, mut q, .., inside, _, _, _)) = sims.get_mut(o) else { continue };
        let Some(house) = inside.map(|i| i.house) else { continue };
        let Ok((htf, ..)) = house_q.get(house) else { continue };
        info!("treehouse: {} is woken by the pretending", sim.first);
        come_down(&mut q, house, htf);
    }
}

/// Down out of it, what they were at there over.
fn come_down(q: &mut ActionQueue, house: Entity, htf: &Transform) {
    q.0.retain(|a| at_house(a).is_none_or(|(h, _)| h != house) || !matches!(a.phase, Phase::Running(_)));
    q.0.retain(|a| at_house(a).is_none_or(|(h, act)| h != house || !act.inside()));
    if !q.0.front().and_then(at_house).is_some_and(|(h, a)| h == house && a == Act::GetOut) {
        q.0.push_front(Action::new(GET_OUT, act(house, htf, Act::GetOut), false));
    }
}

/// Another child in it asked to join the pretending (the game's `AddNext`): next, their playing
/// ended.
fn calls_pretend(commands: &mut Commands, o: Entity, house: Entity, htf: Transform) {
    commands.queue(move |w: &mut World| {
        let label = w.get::<HouseInfo>(house).map(|i| i.theme.pretend().to_string()).unwrap_or_default();
        let Some(mut q) = w.get_mut::<ActionQueue>(o) else { return };
        let playing = q.0.front().and_then(at_house).is_some_and(|(h, a)| h == house && a == Act::Play);
        if playing {
            if let Some(a) = q.0.front_mut() {
                a.cancel = true;
            }
            q.0.insert(1, Action::new(label, act(house, &htf, Act::Pretend), true));
        }
    });
}

/// On a balcony with nobody on the other (and nobody coming): time to go in (`ShouldSwitchParts`).
fn alone_on_balcony(p: Part, h: &House) -> bool {
    match p {
        Part::BalconyA => !h.parts.contains(&Part::BalconyB) && !h.b_coming,
        Part::BalconyB => !h.parts.contains(&Part::BalconyA),
        _ => false,
    }
}

/// Whether two parts are the same, the balconies one.
fn same_part(a: Option<Part>, b: Option<Part>) -> bool {
    match (a, b) {
        (Some(Part::BalconyA | Part::BalconyB), Some(Part::BalconyA | Part::BalconyB)) => true,
        (a, b) => a.is_some() && a == b,
    }
}

/// Away from a window or the balcony, a moment at a time: its way out begun, played through, and
/// back inside (true once they are).
fn step_out(commands: &mut Commands, e: Entity, i: &mut InTreehouse, theme: Theme, lib: &mut ClipLibrary, data: &Baked, dt: f32) -> bool {
    if i.part.is_none() {
        return true;
    }
    if i.stage < 2 {
        leave_part(commands, e, i, theme, lib, data);
        return false;
    }
    i.t += dt;
    if i.t < i.len {
        return false;
    }
    i.prev = i.part.take();
    i.stage = 0;
    commands.entity(e).remove::<ActionClip>();
    true
}

/// Away from a window or the balcony: its way out played.
fn leave_part(commands: &mut Commands, e: Entity, i: &mut InTreehouse, theme: Theme, lib: &mut ClipLibrary, data: &Baked) {
    let Some(p) = i.part else { return };
    let (_, _, s, mirror) = p.clips(theme);
    let s: &'static str = crate::pets::intern(s);
    i.stage = 2;
    i.t = 0.0;
    i.len = lib.get(data, s).map_or(2.0, |c| c.duration);
    i.parity = !i.parity;
    let c = once(s, i.parity);
    commands.entity(e).insert(if mirror { c.mirrored() } else { c });
}

/// Those in a treehouse where they are: out of sight at their place inside (the plumbob over the
/// roof), or seen at the window or balcony they're at.
#[allow(clippy::type_complexity)]
fn show(mut sims: Query<(&InTreehouse, &mut Transform, &mut Visibility, Option<&crate::sim::PlumbobOver>, Entity), (Without<Climbing>, Without<HouseInfo>)>, houses: Query<(&Transform, Option<&crate::objanim::ObjectRig>), (With<HouseInfo>, Without<Sim>)>, mut commands: Commands) {
    for (i, mut tf, mut vis, over, e) in &mut sims {
        let Ok((htf, rig)) = houses.get(i.house) else { continue };
        let place = i.place.min(OCCUPANTS - 1);
        let (at, seen) = match i.part {
            Some(p) => (slot(htf, rig, p.slot()), !(i.stage == 2 && i.t >= i.len - 0.2)),
            None => (slot(htf, rig, HIDE[place]), false),
        };
        tf.translation = at.translation;
        tf.rotation = at.rotation;
        vis.set_if_neq(if seen { Visibility::Inherited } else { Visibility::Hidden });
        let bob = slot(htf, rig, PLUMBOB[place]).translation;
        if seen {
            if over.is_some() {
                commands.entity(e).remove::<crate::sim::PlumbobOver>();
            }
        } else if over.is_none_or(|o| o.0.distance(bob) > 0.01) {
            commands.entity(e).insert(crate::sim::PlumbobOver(bob));
        }
    }
}

/// Pretending's sounds about the treehouse while any's going on in it, and its sparkle.
#[allow(clippy::type_complexity)]
fn pretend_effects(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    houses: Res<Houses>,
    q: Query<(Entity, &HouseInfo, &Transform, Option<&crate::objanim::ObjectRig>, Option<&crate::sound::AmbientSound>, Option<&Children>)>,
    plists: Query<(), With<PretendList>>,
    mut sparkles: MessageWriter<crate::effects::SpinSparkle>,
    mut next: Local<HashMap<Entity, f32>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    for (e, info, tf, rig, sound, kids) in &q {
        let on = houses.get(e).pretending > 0;
        let (lp, plist) = info.theme.sounds();
        let list = kids.and_then(|k| k.iter().find(|c| plists.contains(*c)));
        if on {
            if sound.is_none_or(|s| s.0 != lp) {
                commands.entity(e).insert(crate::sound::AmbientSound(lp.to_string()));
            }
            if list.is_none() {
                commands.entity(e).with_child((PretendList, crate::sound::AmbientSound(plist.to_string()), Transform::default(), Visibility::default()));
            }
            let t = next.entry(e).or_insert(0.0);
            *t -= dt;
            if *t <= 0.0 {
                *t = SPARKLE_EVERY;
                let at = slot(tf, rig, "_FX_0_").translation;
                sparkles.write(crate::effects::SpinSparkle { sim: e, height: 1.2, at: Some(at), spread: 3.0 });
            }
        } else {
            if sound.is_some_and(|s| s.0 == lp) {
                commands.entity(e).remove::<crate::sound::AmbientSound>();
            }
            if let Some(l) = list {
                commands.entity(l).despawn();
            }
            next.remove(&e);
        }
    }
}

/// The pretend game's second sound (its `PList`), beside its loop.
#[derive(Component)]
struct PretendList;

/// Children short of fun go and play in a treehouse on the lot now and then (or play its pretend
/// game), the Childish keener.
#[allow(clippy::type_complexity)]
fn autonomy(
    clock: Res<crate::clock::GameClock>,
    houses: Res<Houses>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &crate::sim::Motives, &Transform), (With<crate::sim::HouseholdMember>, Without<crate::visit::OnLot>, Without<InTreehouse>, Without<crate::playground::OnPlayground>)>,
    trees: Query<(Entity, &HouseInfo, &Transform), Without<crate::visit::LotObject>>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 40.0;
    if std::env::var_os("TREEHOUSE_TEST").is_some() || std::env::var_os("PLAYGROUND_TEST").is_some() {
        return;
    }
    let mut rng = rand::rng();
    for (e, sim, mut q, m, tf) in &mut sims {
        let childish = sim.traits.contains(&crate::life::Trait::Childish);
        if sim.age != Age::Child || !q.0.is_empty() || m.0[crate::sim::FUN] > if childish { 70.0 } else { 40.0 } || !rng.random_bool(if childish { 0.5 } else { 0.3 }) {
            continue;
        }
        let near = trees.iter().filter(|(h, ..)| houses.room(*h, e, false)).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
        let Some((h, info, htf)) = near else { continue };
        let a = if rng.random_bool(0.3) { Act::Pretend } else { Act::Play };
        q.0.push_back(Action::new(label(a, info), act(h, htf, a), true));
        info!("treehouse: {} goes to play in {} of their own accord", sim.first, info.name);
    }
}

/// `TREEHOUSE_TEST=<instance>` (tests: `treehouseCastle`, `treehouseClub`, `treehouseSpaceAge`):
/// ten seconds in, that treehouse put down near the selected Sim (or `TREEHOUSE_WHO=<first
/// name>`), who goes and does `TREEHOUSE_DO=play|pretend|sleep|getin` in it; with
/// `TREEHOUSE_WITH=<first name>` another child plays too; `TREEHOUSE_MINUTES=<n>`: told to stop
/// after that long; `TREEHOUSE_CALL=<first name>`: a grown-up asks everyone down then;
/// `TREEHOUSE_PIE=out|in`: the treehouse's menu, outside or once they're in.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn treehouse_test(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Has<crate::sim::Selected>, Has<InTreehouse>), (With<crate::sim::HouseholdMember>, Without<HouseInfo>)>,
    trees: Query<(Entity, &HouseInfo, &Transform), Without<Sim>>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32, f64)>,
) {
    let Ok(name) = std::env::var("TREEHOUSE_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let who = env("TREEHOUSE_WHO");
    let is_kid = |s: &Sim, sel: bool| who.as_ref().map_or(sel, |w| s.first.eq_ignore_ascii_case(w));
    let tree = trees.iter().next();
    if let (Ok(mut c), Some((_, _, htf))) = (cams.single_mut(), tree) {
        let f = |k: &str, d: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(htf.translation);
        c.height_offset = f("TREEHOUSE_LOOK_Y", 3.5);
        c.distance = f("TREEHOUSE_DIST", 12.0);
        c.pitch = f("TREEHOUSE_PITCH", 0.3);
        let fwd = (htf.rotation * Vec3::Z).xz();
        c.yaw = fwd.x.atan2(fwd.y) + f("TREEHOUSE_YAW", 0.6);
    }
    match stage.0 {
        0 => {
            *stage = (1, now, clock.minutes);
            let Some((_, _, tf, ..)) = sims.iter().find(|s| is_kid(s.1, s.4)) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&name)).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = tf.translation.xz();
            let spot = (4..24).flat_map(|r| (0..16).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 16.0) * r as f32)).find(|c| (-6..=6).all(|i| (-6..=6).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.5)))).unwrap_or(here + Vec2::Y * 6.0);
            let p = Vec3::new(spot.x, tf.translation.y, spot.y);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("treehouse test: {name} at {p:.1}");
            }
        }
        1 if now - stage.1 > 3.0 => {
            let Some((h, info, htf)) = tree else { return };
            *stage = (2, now, clock.minutes);
            let what = match env("TREEHOUSE_DO").as_deref() {
                Some("pretend") => Act::Pretend,
                Some("sleep") => Act::Sleep,
                Some("getin") => Act::GetIn,
                _ => Act::Play,
            };
            if env("TREEHOUSE_PIE").is_some_and(|p| p == "out") {
                commands.insert_resource(crate::hud::PieFor(h));
                info!("treehouse test: its menu");
                *stage = (9, now, clock.minutes);
                return;
            }
            let with = env("TREEHOUSE_WITH");
            for (_, sim, _, mut q, sel, _) in &mut sims {
                let a = if is_kid(sim, sel) {
                    what
                } else if with.as_ref().is_some_and(|w| sim.first.eq_ignore_ascii_case(w)) {
                    Act::Play
                } else {
                    continue;
                };
                q.push_player(Action::new(label(a, info), act(h, htf, a), false));
                info!("treehouse test: {} goes to {a:?} in {}", sim.first, info.name);
            }
        }
        2 => {
            let Some((h, _, htf)) = tree else { return };
            if env("TREEHOUSE_PIE").is_some_and(|p| p == "in") && sims.iter().any(|s| is_kid(s.1, s.4) && s.5) && now - stage.1 > 12.0 {
                commands.insert_resource(crate::hud::PieFor(h));
                info!("treehouse test: its menu, from inside");
                *stage = (9, now, clock.minutes);
                return;
            }
            let minutes = env("TREEHOUSE_MINUTES").and_then(|v| v.parse::<f64>().ok());
            if minutes.is_some_and(|m| clock.minutes - stage.2 >= m) {
                *stage = (3, now, clock.minutes);
                if let Some(c) = env("TREEHOUSE_CALL") {
                    if let Some((_, sim, tf, mut q, ..)) = sims.iter_mut().find(|s| s.1.first.eq_ignore_ascii_case(&c)) {
                        q.push_player(Action::new(ASK_DOWN, ask_down(h, htf, Some(tf.translation.xz())), false));
                        info!("treehouse test: {} asks everyone down", sim.first);
                    }
                } else if let Some((_, sim, _, mut q, ..)) = sims.iter_mut().find(|s| is_kid(s.1, s.4))
                    && let Some(a) = q.0.front_mut()
                {
                    a.cancel = true;
                    info!("treehouse test: {} told to stop ({} {:?})", sim.first, a.label, a.phase);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_treehouse_has_its_own_game_and_way_down() {
        assert_eq!(Theme::of("Sims3.Gameplay.Objects.Environment.TreehouseCastle"), Theme::Castle);
        assert_eq!(Theme::of("Sims3.Gameplay.Objects.Environment.TreehouseClubVenue"), Theme::Club);
        assert_eq!(Theme::of("Sims3.Gameplay.Objects.Environment.TreehouseSpace"), Theme::Space);
        assert_eq!(Theme::Club.pretend(), "Fight Pirate Invaders");
        assert_eq!(Theme::Castle.exit().1, Some("c2o_treehouse_getOut_castlePole_x"));
        assert_eq!(Theme::Space.exit().1, None, "beamed down");
        assert_eq!(Part::WindowB.clips(Theme::Club).0, "c2o_treehouse_windowClubHouse_lookOut_start_x");
        assert!(Part::BalconyA.clips(Theme::Space).3, "the first balcony's mirrored");
        assert!(same_part(Some(Part::BalconyA), Some(Part::BalconyB)) && !same_part(Some(Part::WindowA), Some(Part::WindowB)) && !same_part(None, None));
    }

    #[test]
    fn grown_ups_and_children_never_share_one() {
        let e = |n: u32| Entity::from_raw_u32(n).unwrap();
        let mut houses = Houses::default();
        let h = e(1);
        assert!(houses.room(h, e(2), true) && houses.room(h, e(2), false));
        houses.0.insert(h, House { inside: vec![(e(3), false)], ..default() });
        assert!(!houses.room(h, e(2), true), "a grown-up waits for the children");
        assert!(houses.room(h, e(2), false));
        houses.0.insert(h, House { inside: (3..7).map(|n| (e(n), false)).collect(), ..default() });
        assert!(!houses.room(h, e(2), false), "four's the most");
        houses.0.insert(h, House { inside: vec![(e(3), true)], ..default() });
        assert!(!houses.room(h, e(2), false));
    }

    #[test]
    fn the_slots_are_the_castles_without_a_rig() {
        let tf = Transform::from_xyz(10.0, 0.0, 5.0);
        let (at, face) = ladder(&tf);
        assert!((at - Vec2::new(10.0, 6.29078)).length() < 1e-4);
        assert!((face - Vec2::NEG_Y).length() < 1e-4, "facing the trunk");
        let w = slot(&tf, None, "_ctnm_window_02");
        assert!(((w.rotation * Vec3::Z).xz() - Vec2::X).length() < 1e-4);
        assert!((w.translation.y - 5.26955).abs() < 1e-4);
    }
}
