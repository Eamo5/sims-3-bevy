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
            .add_systems(Update, (run, tricks, asking, woohoo, show, pretend_effects, autonomy, mischief, treehouse_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(crate::PlayMode::Live)));
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
/// A pretend game's lines (`kTnsCooldownTime`: a quarter hour apart at least; about one in twenty
/// game minutes after, the game's `kTnsChance` a tick).
const PRETEND_TNS_EVERY: f64 = 15.0;
const PRETEND_TNS_CHANCE: f64 = 0.05;
/// Water dumped on someone (`DumpWaterOnB`'s once-off Fun and Hygiene, the liking lost or, putting
/// out a fire, won: `kLtrLoss`, `kLtrGainExtinguish`; the fun of it, `DumpWaterOnA`'s); how long
/// one waits for the other, beckoning (`kGiveupTimeForSyncedInteractions`, game minutes).
const TRICKED_FUN: f32 = -40.0;
const TRICKED_HYGIENE: f32 = -40.0;
const TRICK_LIKING: f32 = -5.0;
const EXTINGUISH_LIKING: f32 = 10.0;
const TRICK_FUN: f32 = 20.0;
const GIVE_UP: f64 = 40.0;
/// When the clips hide or show the child at the window (seconds: their `disappear_sim` and
/// `appear_sim` events).
const BECKON_HIDE: f32 = 6.6;
/// When the bucket's water comes out (its `ep4TreeHouseDumpWater` event, seconds into the
/// dump), and how long it pours.
const POUR_AT: f32 = 1.8;
const POUR_SECS: f32 = 0.6;
/// How long the one below waves up before the water comes (seconds).
const WAVE_BEFORE: f32 = 2.0;
const DUMP_HIDE: f32 = 7.63;
const ANSWER_SHOW: f32 = 0.83;
const ANSWER_HIDE: f32 = 6.6;
/// WooHoo up a treehouse (`kSplinterChance`, twice as likely and three quarters again for the
/// Unlucky and Clumsy, never the Lucky; `kTryForBabySuccessChance`).
const SPLINTER_CHANCE: f64 = 0.5;
const SPLINTER_MORE: f64 = 1.75;
const BABY_CHANCE: f64 = 0.25;

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

    /// What the pretend game has them say now and then (`PlayPretendTns...`).
    pub fn pretend_lines(self) -> [&'static str; 5] {
        match self {
            Theme::Castle => [
                "Ho there court jester! Such a lackadaisical performance will be lucky to earn thee a bread crust for your belly. Tell us a tale or it's off to the scullery with you.",
                "Verily, this turkey leg is a size larger than my head!",
                "King Launergan has dispatched a regiment of his finest cavalry to grace us with his presence and you all stand about nattering like a pack of louts!",
                "Gather 'round my loyal subjects for a spot of tea and crumpets. I shan't accept any dallying!",
                "Oh but these endless proclamations, declamations, recitations, and pontifications do grow tiring! For want of a reprieve a feast was lost!",
            ],
            Theme::Club => [
                "Arrrr! They be attacking from all sides! Defend yourselves to the last breath!",
                "Batten down the hatches, there be a nasty storm brewing and enemies closing fast! You lad, look sharp!",
                "99 bottles of juice on the wall, 99 bottle of juice... take one down, pass it around, 98 bottles of juice on the wall!",
                "Avast maties! Have you seen me parrot? The daft creature done flown off with me fake eye!",
                "Raise the alarm, alert the crew, we've been boarded! I'll take my cutlass and run them thru!",
            ],
            Theme::Space => [
                "The aliens won't take me, no siree Bob! I see them on the scanner... watching, waiting, hiding in the shadows. I'll outsmart the lot of them!",
                "A long time ago in a treehouse far far away... a child decided to boldly go where no child has gone before!",
                "Divert all power to the engines, prepare for liftoff!",
                "Freezer Bunnies were created by Sims. They evolved. They rebelled. There are many copies. And they have a plan.",
                "Captain's log, Earth date May 22nd, 2048. Alien life forms were detected on the scanners at 0600. Initial contact with the extraterrestrials proved futile, as greetings across all frequencies were met with silence. We will continue to monitor the alien menace for signs of hostility.",
            ],
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
    /// A child up there tipping a bucket of water on someone below (`DumpWaterOnA`), and the one
    /// tricked, called under the window (`DumpWaterOnB`).
    DumpWater(Entity),
    BeTricked(Entity),
    /// A child below asking to come up (`AskToComeUpA`), and the one up there answering at the
    /// window (`AskToComeUpB`).
    AskUp,
    BeAskedUp(Entity),
    /// WooHoo in it with someone (`WooHoo`; trying for a baby, `TryForBaby`).
    WooHoo(Entity, bool),
}

impl Act {
    /// Done inside it (so gone up first).
    pub fn inside(self) -> bool {
        matches!(self, Act::Play | Act::Pretend | Act::Sleep | Act::DumpWater(_) | Act::BeAskedUp(_) | Act::WooHoo(..))
    }

    /// Playing there (the game's `PlayBase`, and sleeping): what one asked up interrupts.
    fn playing(self) -> bool {
        matches!(self, Act::Play | Act::Pretend | Act::Sleep)
    }
}

/// Who may play in a treehouse: once a child's in it, those children only (`mAllowedSims`, till
/// the last's out), others asking to come up; and whether it's no boys or no girls (the sign on
/// its door, `SetRestrictions`).
#[derive(Component, Default, Debug)]
pub struct HouseRules {
    pub allowed: Vec<Entity>,
    pub restriction: Restriction,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Restriction {
    #[default]
    None,
    NoBoys,
    NoGirls,
}

impl Restriction {
    pub fn label(self) -> &'static str {
        match self {
            Restriction::None => "No Restrictions",
            Restriction::NoBoys => "No Boys Allowed",
            Restriction::NoGirls => "No Girls Allowed",
        }
    }
}

impl HouseRules {
    /// Whether they may play in it (`IsAllowedSim`).
    pub fn allows(&self, me: Entity) -> bool {
        self.allowed.is_empty() || self.allowed.contains(&me)
    }

    /// Whether the sign keeps them out (`IsRestricted`): of their own accord always; asked to,
    /// only with one of the others up there.
    pub fn restricted(&self, female: bool, autonomous: bool, girls_in: usize, boys_in: usize) -> bool {
        match (self.restriction, female) {
            (Restriction::NoBoys, false) => autonomous || girls_in > 0,
            (Restriction::NoGirls, true) => autonomous || boys_in > 0,
            _ => false,
        }
    }

    /// A child let in (`AddAllowedSim`): the sign against them taken down.
    pub fn let_in(&mut self, child: Entity, female: bool) {
        if !self.allowed.contains(&child) {
            self.allowed.push(child);
        }
        if (female && self.restriction == Restriction::NoGirls) || (!female && self.restriction == Restriction::NoBoys) {
            self.restriction = Restriction::None;
        }
    }
}

/// What's done there and then from a treehouse's menu (immediate, as the game's): another child
/// invited up (`InviteUp`), the sign on the door changed (`SetRestrictions`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Now {
    InviteUp(Entity),
    Restrict(Restriction),
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
    /// A child on their way down the slide or pole (or beamed down).
    pub exit: Option<Entity>,
    /// Children playing about inside, free to be asked out onto the second balcony; and whether
    /// one's on their way there.
    pub idle: Vec<Entity>,
    pub b_coming: bool,
    /// Those on their way down.
    pub leaving: Vec<Entity>,
    /// The girls and boys up there, and the children playing (or asleep) there, who'd answer one
    /// asking to come up.
    pub girls: usize,
    pub boys: usize,
    pub players: Vec<Entity>,
    /// Grown-ups going up for WooHoo together: each, and whom with.
    pub couples: Vec<(Entity, Entity)>,
}

impl Houses {
    pub fn get(&self, house: Entity) -> House {
        self.0.get(&house).cloned().unwrap_or_default()
    }

    /// Whether there's room for one more (`IsAvailable`): a grown-up only in an empty one (or with
    /// the one they're there for WooHoo with), a child in one with no grown-ups and a place free.
    pub fn room(&self, house: Entity, me: Entity, grown: bool) -> bool {
        let h = self.get(house);
        let others: Vec<&(Entity, bool)> = h.inside.iter().filter(|x| x.0 != me).collect();
        if grown {
            let partner = |o: Entity| h.couples.iter().any(|c| (c.0 == me && c.1 == o) || (c.1 == me && c.0 == o));
            others.iter().all(|x| partner(x.0))
        } else {
            others.len() < OCCUPANTS && !others.iter().any(|x| x.1)
        }
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
    /// At a window but out of sight (waiting there, tucked down).
    hidden: bool,
    /// When their pretend game last had them say something (game minutes).
    tns: f64,
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
        commands.entity(e).insert((HouseInfo { theme: Theme::of(&script), name: o.name.clone() }, HouseRules::default()));
    }
}

/// The action for an act at a treehouse: gone up the ladder for it.
pub fn act(house: Entity, htf: &Transform, act: Act) -> ActionKind {
    let (at, face) = ladder(htf);
    ActionKind::Treehouse { house, act, at, face }
}

/// Its words for an act (the names: whom it's with, `{1.SimFirstName}`).
pub fn label(act: Act, info: &HouseInfo) -> String {
    match act {
        Act::GetIn => get_in(&info.name),
        Act::Play => play_in(&info.name),
        Act::Pretend => info.theme.pretend().to_string(),
        Act::Sleep => SLEEP.to_string(),
        Act::GetOut => GET_OUT.to_string(),
        Act::AskDown => ASK_DOWN.to_string(),
        Act::AskUp => format!("Ask to Come Up in {}", info.name),
        Act::DumpWater(_) | Act::BeTricked(_) | Act::BeAskedUp(_) | Act::WooHoo(..) => String::new(),
    }
}

/// Its words for an act with someone: "Dump Water on Mortimer", "Be Tricked by Darlene", "Be
/// Asked to Come Up by Arlo", "WooHoo with Judy in Kid's Clubhouse".
pub fn label_with(act: Act, info: &HouseInfo, name: &str) -> String {
    match act {
        Act::DumpWater(_) => format!("Dump Water on {name}"),
        Act::BeTricked(_) => format!("Be Tricked by {name}"),
        Act::BeAskedUp(_) => format!("Be Asked to Come Up by {name}"),
        Act::WooHoo(_, false) => format!("WooHoo with {name} in {}", info.name),
        Act::WooHoo(_, true) => format!("Try for Baby with {name} in {}", info.name),
        a => label(a, info),
    }
}

/// The game's words for the treehouse's pickers and its sign.
pub const DUMP_WATER_ON: &str = "Dump Water on...";
pub const INVITE_UP: &str = "Invite Up...";
pub const RESTRICTIONS: &str = "Entry Restrictions...";
pub const RESTRICTION_SET: &str = "This is the currently selected option";
pub const WOOHOO_WITH: &str = "WooHoo With...";
pub const BABY_WITH: &str = "Try for Baby With...";

/// Who's about for a treehouse's menu: the Sims on its lot (age, a girl or woman, up a treehouse
/// already), and those the actor could WooHoo with there, or try for a baby with.
#[derive(Default)]
pub struct About {
    pub sims: Vec<(Entity, String, Age, bool, bool)>,
    pub woohoo: Vec<(Entity, String)>,
    pub baby: Vec<(Entity, String)>,
}

/// A treehouse's menu for a Sim: outside, "Get in" (greyed when full); a child's "Play in", its
/// pretend game and "Sleep" (theirs to play in), or "Ask to Come Up" (another's, with children up
/// there); a teen or grown-up's "Ask Everyone to Come Down" with children up there, a grown-up's
/// "WooHoo With... ›" and "Try for Baby With... ›". In it, "Get Out", and a child's others but the
/// one they're at, "Dump Water on... ›", "Invite Up... ›" and "Entry Restrictions... ›".
#[allow(clippy::too_many_arguments)]
pub fn options(
    house: Entity,
    htf: &Transform,
    (info, rules): (&HouseInfo, &HouseRules),
    houses: &Houses,
    (me, sim): (Entity, &Sim),
    inside: Option<&InTreehouse>,
    about: &About,
    pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>,
    submenu: impl Fn(usize) -> ActionKind,
) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    let age = sim.age;
    let child = age == Age::Child;
    let h = houses.get(house);
    let mut pick = |title: &str, list: Vec<(String, ActionKind)>, out: &mut Vec<(String, ActionKind)>| {
        if !list.is_empty() {
            out.push((format!("{title} ›"), submenu(pie_submenus.len())));
            pie_submenus.push((title.to_string(), list));
        }
    };
    let here = inside.filter(|i| i.house == house);
    if let Some(i) = here {
        out.push((GET_OUT.to_string(), act(house, htf, Act::GetOut)));
        if child {
            for a in [Act::Play, Act::Pretend, Act::Sleep] {
                if i.doing != Some(a) {
                    out.push((label(a, info), act(house, htf, a)));
                }
            }
            // (Water on anyone below: child or older, not up here.)
            let below: Vec<(String, ActionKind)> = about.sims.iter().filter(|s| s.0 != me && !s.4 && !matches!(s.2, Age::Baby | Age::Toddler)).map(|s| (s.1.clone(), act(house, htf, Act::DumpWater(s.0)))).collect();
            pick(DUMP_WATER_ON, below, &mut out);
            // (Children about, not up here, asked up.)
            let kids: Vec<(String, ActionKind)> = about.sims.iter().filter(|s| s.0 != me && s.2 == Age::Child && !s.4).map(|s| (s.1.clone(), ActionKind::TreehouseNow { house, now: Now::InviteUp(s.0) })).collect();
            pick(INVITE_UP, kids, &mut out);
            // (Theirs, and not shared by both: the sign on the door.)
            let mixed = h.girls > 0 && h.boys > 0;
            if rules.allowed.contains(&me) && !mixed {
                let mine = if sim.female { Restriction::NoBoys } else { Restriction::NoGirls };
                let list = [mine, Restriction::None]
                    .into_iter()
                    .map(|r| (r.label().to_string(), if rules.restriction == r { ActionKind::Greyed(RESTRICTION_SET.into()) } else { ActionKind::TreehouseNow { house, now: Now::Restrict(r) } }))
                    .collect();
                pick(RESTRICTIONS, list, &mut out);
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
    if child {
        let barred = rules.restricted(sim.female, false, h.girls, h.boys);
        if rules.allows(me) && room && !barred {
            for a in [Act::Play, Act::Pretend, Act::Sleep] {
                out.push((label(a, info), act(house, htf, a)));
            }
        } else if !rules.allows(me) && !h.players.is_empty() {
            out.push((label(Act::AskUp, info), if h.inside.len() >= OCCUPANTS { ActionKind::Greyed(FULL.into()) } else { water_act(house, htf, info.theme, Act::AskUp) }));
        }
    }
    if grown(age) && age != Age::Teen {
        let with = |baby: bool, list: &[(Entity, String)]| list.iter().map(|(o, n)| (n.clone(), act(house, htf, Act::WooHoo(*o, baby)))).collect::<Vec<_>>();
        pick(WOOHOO_WITH, with(false, &about.woohoo), &mut out);
        pick(BABY_WITH, with(true, &about.baby), &mut out);
    }
    out
}

/// Where one stands below its first window (the `_route_water_` routing slot: where a Sim's
/// tricked, or asks to come up), facing up at it: half a metre nearer the trunk under the
/// clubhouse's and the hideaway's than the castle's.
pub fn water_spot(htf: &Transform, theme: Theme) -> Transform {
    let mut s = slot(htf, None, "_route_water_");
    if theme != Theme::Castle {
        s.translation = htf.transform_point(Vec3::new(-0.738261, 0.0, 2.0));
    }
    s
}

/// An act done below its first window, facing up at it.
pub fn water_act(house: Entity, htf: &Transform, theme: Theme, act: Act) -> ActionKind {
    let s = water_spot(htf, theme);
    ActionKind::Treehouse { house, act, at: s.translation.xz(), face: (s.rotation * Vec3::Z).xz().normalize_or(Vec2::NEG_Y) }
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
    // (WooHoo up a treehouse asked of someone: theirs too, as the game's social's continuation.)
    let mut partners: Vec<(Entity, Entity, Entity, bool, String)> = Vec::new();
    for (e, sim, q, ..) in sims.iter() {
        if let Some(a) = q.0.front().filter(|a| a.phase == Phase::Start && !a.cancel)
            && let Some((h, Act::WooHoo(with, baby))) = at_house(a)
        {
            partners.push((with, e, h, baby, sim.first.clone()));
        }
    }
    for (with, me, h, baby, name) in partners {
        let Ok((htf, info)) = houses_q.get(h) else { continue };
        let Ok((_, _, mut q, ..)) = sims.get_mut(with) else { continue };
        if !q.0.iter().any(|a| matches!(at_house(a), Some((_, Act::WooHoo(o, _))) if o == me)) {
            let a = Act::WooHoo(me, baby);
            q.push_player(Action::new(label_with(a, info, &name), act(h, htf, a), false));
        }
    }
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
    mut house_q: Query<(&Transform, &HouseInfo, Option<&crate::objanim::ObjectRig>, &mut HouseRules), Without<Sim>>,
    mut sparkles: MessageWriter<crate::effects::SpinSparkle>,
    mut notes: ResMut<crate::interact::Notifications>,
    selected: Query<(), With<crate::sim::Selected>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    // Who's where (as it stood last).
    let snapshot = std::mem::take(&mut houses.0);
    let mut fresh: HashMap<Entity, House> = HashMap::new();
    for (e, sim, q, _, _, _, inside, climbing, ..) in sims.iter() {
        // (Grown-ups going up for WooHoo together.)
        for a in q.0.iter().filter(|a| !a.cancel) {
            if let Some((h, Act::WooHoo(with, _))) = at_house(a) {
                fresh.entry(h).or_default().couples.push((e, with));
            }
        }
        if let Some(i) = inside {
            let h = fresh.entry(i.house).or_default();
            h.inside.push((e, i.grown));
            h.places.push(i.place);
            if climbing.is_some_and(|c| c.out) || q.0.front().and_then(at_house).is_some_and(|(x, a)| x == i.house && a == Act::GetOut) {
                h.leaving.push(e);
            }
            if !i.grown {
                if sim.female {
                    h.girls += 1;
                } else {
                    h.boys += 1;
                }
                if i.doing.is_some_and(Act::playing) {
                    h.players.push(e);
                }
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
            let h = fresh.entry(c.house).or_default();
            // (A child out by the slide or pole, not the ladder.)
            if c.out && inside.is_some_and(|i| !i.grown) {
                h.exit = Some(e);
            } else {
                h.ladder = Some(e);
            }
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
                // (The last child out: anyone may play in it again, `ClearAllowedSims`.)
                let h = houses.get(house);
                if !h.inside.iter().any(|x| x.0 != e && !x.1)
                    && let Ok((.., mut rules)) = house_q.get_mut(house)
                {
                    rules.allowed.clear();
                }
            } else {
                // (A child in: theirs to play in, `AddAllowedSim`.)
                if !grown_up && let Ok((.., mut rules)) = house_q.get_mut(house) {
                    rules.let_in(e, sim.female);
                }
                let taken = houses.get(house).places;
                let place = (0..OCCUPANTS).find(|p| !taken.contains(p)).unwrap_or(0);
                let h = houses.0.entry(house).or_default();
                h.inside.push((e, grown_up));
                h.places.push(place);
                commands.entity(e).insert(InTreehouse { house, place, grown: grown_up, doing: None, part: None, stage: 0, t: 0.0, len: 0.0, until: 0.0, prev: None, parity: false, since: now, want_b: false, hidden: false, tns: f64::MIN });
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
        let Ok((htf, info, rig, _)) = house_q.get(house) else {
            q.0.pop_front();
            continue;
        };
        match act {
            // (Tricks, asking up and WooHoo: `tricks`, `asking`, `woohoo`.)
            Act::DumpWater(_) | Act::BeTricked(_) | Act::AskUp | Act::BeAskedUp(_) | Act::WooHoo(..) => {}
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
                // (One at a time down the ladder, or down the slide or pole.)
                let h = houses.get(house);
                let busy = if i.grown { h.ladder } else { h.exit };
                if busy.is_some_and(|o| o != e) {
                    continue;
                }
                let h = houses.0.entry(house).or_default();
                if i.grown {
                    h.ladder = Some(e);
                } else {
                    h.exit = Some(e);
                }
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
                // (Pretending, now and then a line of it for the one played: `PlayPretendTns...`, a
                // quarter hour apart at least.)
                if act == Act::Pretend && selected.contains(e) && now - i.tns >= PRETEND_TNS_EVERY && rng.random_bool((PRETEND_TNS_CHANCE * delta.0 as f64).clamp(0.0, 1.0)) {
                    i.tns = now;
                    let lines = info.theme.pretend_lines();
                    notes.push(format!("{}: {}", sim.first, lines[rng.random_range(0..lines.len())]));
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

/// The child done with something at the window: inside again, back to their playing (the game's
/// `PushPlayAsContinuation`) unless something else up there's next.
fn back_to_play(q: &mut ActionQueue, i: &mut InTreehouse, house: Entity, htf: &Transform, info: &HouseInfo, commands: &mut Commands, e: Entity) {
    i.doing = None;
    i.part = None;
    i.stage = 0;
    i.hidden = false;
    q.0.pop_front();
    commands.entity(e).remove::<ActionClip>();
    if !q.0.iter().find(|a| !a.cancel).and_then(at_house).is_some_and(|(h, _)| h == house) {
        q.0.push_front(Action::new(label(Act::Play, info), act(house, htf, Act::Play), true));
    }
}

/// One tricked, waiting below the window: their wave up at it, then soaked.
#[derive(Component)]
struct Tricked {
    soaked: bool,
    t: f32,
    len: f32,
}

/// Water dumped from a treehouse's window (`DumpWaterOnA` and `B`): the child at the first window
/// beckoning (`c2o_treehouse_dumpWaterOn_beckon`), the one below called under it and waving up
/// (`wave`), the bucket tipped (`dump`, its `bucket` with it) as they're soaked (`victim`): Fun and
/// Hygiene 40 down, Soaked, liking 5 less (a fire on them put out, 10 more), a puddle where they
/// stood; the child's fun of it. Given up after forty minutes with no one come.
#[allow(clippy::type_complexity)]
fn tricks(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    houses: Res<Houses>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut SimAnim, Option<&mut InTreehouse>, Option<&mut Tricked>, &mut Transform), Without<HouseInfo>>,
    house_q: Query<(&Transform, &HouseInfo, Option<&crate::objanim::ObjectRig>), Without<Sim>>,
    mut pours: MessageWriter<crate::effects::Pour>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let now = clock.minutes;
    // (Who means to be tricked by whom, and whether they're under the window yet; who's a child.)
    let mut below: HashMap<(Entity, Entity), bool> = HashMap::new();
    let heights: HashMap<Entity, bool> = sims.iter().map(|s| (s.0, s.1.age == Age::Child)).collect();
    for (e, _, q, _, _, tricked, _) in sims.iter() {
        for (n, a) in q.0.iter().enumerate().filter(|(_, a)| !a.cancel) {
            if let Some((_, Act::BeTricked(kid))) = at_house(a) {
                // (There, and waving up a moment, `DumpWaterOnB`'s wave before the water.)
                below.insert((e, kid), n == 0 && matches!(a.phase, Phase::Running(_)) && tricked.is_some_and(|t| !t.soaked && t.t >= WAVE_BEFORE));
            }
        }
    }
    let mut soak: Vec<Entity> = Vec::new();
    let mut done: Vec<(Entity, Entity)> = Vec::new();
    let mut drop: Vec<(Entity, Entity)> = Vec::new();
    for (e, sim, mut q, mut anim, inside, tricked, mut tf) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(at_house);
        match front {
            Some((house, Act::DumpWater(v))) => {
                let (Some(mut i), Ok((htf, info, rig))) = (inside, house_q.get(house)) else {
                    q.0.pop_front();
                    continue;
                };
                if i.doing != Some(Act::DumpWater(v)) {
                    // (Off the part they were at, and the window free, first.)
                    if i.part.is_some() {
                        step_out(&mut commands, e, &mut i, info.theme, &mut lib, &data, dt);
                        continue;
                    }
                    if houses.get(house).parts.contains(&Part::WindowA) {
                        continue;
                    }
                    let beckon = "c2o_treehouse_dumpWaterOn_beckon_x";
                    i.doing = Some(Act::DumpWater(v));
                    i.part = Some(Part::WindowA);
                    i.stage = 10;
                    i.t = 0.0;
                    i.len = lib.get(&data, beckon).map_or(6.7, |c| c.duration);
                    i.hidden = false;
                    i.since = now;
                    i.parity = !i.parity;
                    commands.entity(e).insert(once(beckon, i.parity));
                    let kind = water_act(house, htf, info.theme, Act::BeTricked(e));
                    let words = label_with(Act::BeTricked(e), info, &sim.first);
                    commands.queue(move |w: &mut World| {
                        if let Some(mut q) = w.get_mut::<ActionQueue>(v) {
                            q.push_player(Action::new(words, kind, false));
                        }
                    });
                    info!("treehouse: {} beckons from the window, water at the ready", sim.first);
                    continue;
                }
                match i.stage {
                    10 => {
                        i.t += dt;
                        i.hidden = i.t >= BECKON_HIDE;
                        if i.t >= i.len {
                            i.stage = 11;
                            commands.entity(e).remove::<ActionClip>();
                        }
                    }
                    11 => match below.get(&(v, e)).copied() {
                        // (Under the window: the bucket tipped.)
                        Some(true) => {
                            let dump = "c2o_treehouse_dumpWaterOn_dump_x";
                            i.stage = 12;
                            i.t = 0.0;
                            i.len = lib.get(&data, dump).map_or(7.7, |c| c.duration);
                            i.hidden = false;
                            i.parity = !i.parity;
                            commands.entity(e).insert(once(dump, i.parity));
                            soak.push(v);
                            info!("treehouse: {} tips the bucket", sim.first);
                        }
                        // (Not coming, or too long: given up.)
                        w if (w.is_none() && now - i.since > 2.0) || now - i.since > GIVE_UP => {
                            info!("treehouse: {} gives up waiting with the water", sim.first);
                            drop.push((v, e));
                            back_to_play(&mut q, &mut i, house, htf, info, &mut commands, e);
                        }
                        _ => {}
                    },
                    12 => {
                        // (The water out of the bucket, its `ep4TreeHouseDumpWater`, onto the one
                        // below.)
                        if i.t < POUR_AT && i.t + dt >= POUR_AT {
                            let w = slot(htf, rig, Part::WindowA.slot());
                            let from = w.translation + w.rotation * Vec3::Z * 0.55 + Vec3::Y * 0.85;
                            let head = if heights.get(&v).copied().unwrap_or(false) { 1.15 } else { 1.65 };
                            let to = water_spot(htf, info.theme).translation + Vec3::Y * head;
                            debug!("treehouse: water from {from:.2} onto {to:.2}");
                            pours.write(crate::effects::Pour { from, to, secs: POUR_SECS });
                        }
                        i.t += dt;
                        i.hidden = i.t >= DUMP_HIDE;
                        if i.t >= i.len {
                            done.push((v, e));
                            back_to_play(&mut q, &mut i, house, htf, info, &mut commands, e);
                        }
                    }
                    _ => {}
                }
            }
            Some((house, Act::BeTricked(_))) => match tricked {
                // (Under the window, just where the game has them: a wave up at it.)
                None => {
                    if let Ok((htf, info, _)) = house_q.get(house) {
                        let s = water_spot(htf, info.theme);
                        debug!("treehouse: {} under the window, from {:.2} to {:.2}", sim.first, tf.translation, s.translation);
                        tf.translation.x = s.translation.x;
                        tf.translation.z = s.translation.z;
                        tf.rotation = s.rotation;
                    }
                    let wave = if sim.age == Age::Child { "c2o_treehouse_dumpWaterOn_wave_y" } else { "a2o_treehouse_dumpWaterOn_wave_y" };
                    let len = lib.get(&data, wave).map_or(4.5, |c| c.duration);
                    anim.pose = Pose::Use;
                    // (Waving on till the water comes.)
                    commands.entity(e).insert((Tricked { soaked: false, t: 0.0, len }, ActionClip::new(None, looping(wave.to_string()))));
                }
                Some(mut t) => {
                    t.t += dt;
                    if t.soaked && t.t >= t.len {
                        q.0.pop_front();
                        anim.pose = Pose::Stand;
                        commands.entity(e).remove::<(Tricked, ActionClip)>();
                    }
                }
            },
            _ => {
                if tricked.is_some() {
                    commands.entity(e).remove::<(Tricked, ActionClip)>();
                }
            }
        }
    }
    // (Soaked as the bucket's tipped: a child's own clip.)
    for v in soak {
        let child = sims.get(v).is_ok_and(|s| s.1.age == Age::Child);
        let clip = if child { "c2o_treehouse_dumpWaterOn_victim_y" } else { "a2o_treehouse_dumpWaterOn_victim_y" };
        let len = lib.get(&data, clip).map_or(7.7, |c| c.duration);
        if let Ok((_, _, _, _, _, Some(mut t), _)) = sims.get_mut(v) {
            t.soaked = true;
            t.t = 0.0;
            t.len = len;
        } else {
            commands.entity(v).insert(Tricked { soaked: true, t: 0.0, len });
        }
        commands.entity(v).insert(ActionClip::new(Some(clip), &[]));
    }
    for (v, kid) in drop {
        commands.queue(move |w: &mut World| {
            if let Some(mut q) = w.get_mut::<ActionQueue>(v) {
                q.0.retain(|a| !matches!(at_house(a), Some((_, Act::BeTricked(k))) if k == kid));
            }
        });
    }
    // (What the water did.)
    for (v, kid) in done {
        commands.queue(move |w: &mut World| {
            let on_fire = w.get::<crate::fire::OnFire>(v).is_some();
            let now = w.resource::<crate::clock::GameClock>().minutes;
            let at = w.get::<Transform>(v).map(|t| t.translation);
            if let Some(mut m) = w.get_mut::<crate::sim::Motives>(v) {
                m.add(crate::sim::FUN, TRICKED_FUN);
                m.add(crate::sim::HYGIENE, TRICKED_HYGIENE);
            }
            if let Some(mut m) = w.get_mut::<crate::life::Moodlets>(v) {
                m.add(crate::life::MoodletKind::Soaked, now);
            }
            if let Some(mut m) = w.get_mut::<crate::sim::Motives>(kid) {
                m.add(crate::sim::FUN, TRICK_FUN);
            }
            let liking = if on_fire { EXTINGUISH_LIKING } else { TRICK_LIKING };
            for (a, b) in [(kid, v), (v, kid)] {
                if let Some(mut r) = w.get_mut::<crate::social::Relationships>(a) {
                    r.add(b, liking, 0.0);
                }
            }
            crate::fire::douse(w, v);
            if let Some(at) = at {
                w.write_message(crate::puddles::AddPuddle { at, level: 1, lasts: None });
            }
        });
    }
}

/// Asking to come up (`AskToComeUpA`): below the first window, the child up there the asker likes
/// best (playing or asleep) comes to it (`AskToComeUpB`) and answers (the game's `Ask to Come Up in
/// Treehouse` social: yes to one they don't dislike, never to one the sign keeps out), with its
/// clips (`c2c_treehouse_askToComeUp_accept`, `_reject`); let in, the asker goes up to play.
#[derive(Component)]
struct Asking {
    since: f64,
    reply: Option<(bool, f32, f32)>,
}

#[allow(clippy::type_complexity)]
fn asking(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    houses: Res<Houses>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut SimAnim, Option<&mut InTreehouse>, Option<&mut Asking>, Option<&crate::social::Relationships>, &mut Transform), Without<HouseInfo>>,
    mut house_q: Query<(&Transform, &HouseInfo, &mut HouseRules), Without<Sim>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let now = clock.minutes;
    // (Those below asking, and whether they're there yet; who's a girl.)
    let mut askers: HashMap<Entity, bool> = HashMap::new();
    let mut female: HashMap<Entity, bool> = HashMap::new();
    for (e, sim, q, ..) in sims.iter() {
        female.insert(e, sim.female);
        if let Some(a) = q.0.front().filter(|a| !a.cancel)
            && matches!(at_house(a), Some((_, Act::AskUp)))
        {
            askers.insert(e, matches!(a.phase, Phase::Running(_)));
        }
    }
    let mut asks: Vec<(Entity, Entity, Entity)> = Vec::new();
    let mut replies: Vec<(Entity, bool)> = Vec::new();
    for (e, sim, mut q, mut anim, inside, asking, rels, mut tf) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(at_house);
        match front {
            Some((house, Act::AskUp)) => match asking {
                None => {
                    // (The child up there they like best, to answer.)
                    let h = houses.get(house);
                    let liking = |k: &Entity| rels.map_or(0.0, |r| r.get(*k).friendship);
                    let Some(kid) = h.players.iter().copied().max_by(|a, b| liking(a).total_cmp(&liking(b))) else {
                        info!("treehouse: nobody up there to ask, for {}", sim.first);
                        q.0.pop_front();
                        continue;
                    };
                    // (Just under the window, looking up at it.)
                    if let Ok((htf, info, _)) = house_q.get(house) {
                        let s = water_spot(htf, info.theme);
                        tf.translation.x = s.translation.x;
                        tf.translation.z = s.translation.z;
                        tf.rotation = s.rotation;
                    }
                    anim.pose = Pose::Stand;
                    commands.entity(e).insert(Asking { since: now, reply: None });
                    asks.push((kid, e, house));
                    info!("treehouse: {} asks to come up", sim.first);
                }
                Some(mut a) => {
                    if let Some((yes, t, len)) = a.reply.as_mut() {
                        *t += dt;
                        if *t >= *len {
                            let yes = *yes;
                            q.0.pop_front();
                            commands.entity(e).remove::<(Asking, ActionClip)>();
                            if yes && let Ok((htf, info, _)) = house_q.get(house) {
                                q.0.push_front(Action::new(label(Act::Play, info), act(house, htf, Act::Play), false));
                            }
                        }
                    } else if now - a.since > GIVE_UP {
                        q.0.pop_front();
                        commands.entity(e).remove::<Asking>();
                    }
                }
            },
            Some((house, Act::BeAskedUp(asker))) => {
                let (Some(mut i), Ok((htf, info, mut rules))) = (inside, house_q.get_mut(house)) else {
                    q.0.pop_front();
                    continue;
                };
                if i.doing != Some(Act::BeAskedUp(asker)) {
                    if i.part.is_some() {
                        step_out(&mut commands, e, &mut i, info.theme, &mut lib, &data, dt);
                        continue;
                    }
                    if houses.get(house).parts.contains(&Part::WindowA) {
                        continue;
                    }
                    i.doing = Some(Act::BeAskedUp(asker));
                    i.part = Some(Part::WindowA);
                    i.stage = 20;
                    i.hidden = true;
                    i.since = now;
                    commands.entity(e).remove::<ActionClip>();
                    continue;
                }
                match i.stage {
                    20 => match askers.get(&asker).copied() {
                        Some(true) => {
                            let h = houses.get(house);
                            let likes = rels.map_or(0.0, |r| r.get(asker).friendship) >= 0.0;
                            let barred = rules.restricted(female.get(&asker).copied().unwrap_or(false), false, h.girls, h.boys);
                            let yes = likes && !barred;
                            let clip: &'static str = if yes { "c2c_treehouse_askToComeUp_accept_x" } else { "c2c_treehouse_askToComeUp_reject_x" };
                            i.stage = 21;
                            i.t = 0.0;
                            i.len = lib.get(&data, clip).map_or(7.0, |c| c.duration);
                            i.parity = !i.parity;
                            i.until = if yes { 1.0 } else { 0.0 };
                            commands.entity(e).insert(once(clip, i.parity));
                            replies.push((asker, yes));
                            info!("treehouse: {} {} {asker:?} up", sim.first, if yes { "lets" } else { "won't let" });
                        }
                        Some(false) => {}
                        None if now - i.since > 2.0 => back_to_play(&mut q, &mut i, house, htf, info, &mut commands, e),
                        None => {}
                    },
                    21 => {
                        i.t += dt;
                        i.hidden = !(ANSWER_SHOW..ANSWER_HIDE).contains(&i.t);
                        if i.t >= i.len {
                            if i.until > 0.5 {
                                rules.let_in(asker, female.get(&asker).copied().unwrap_or(false));
                            }
                            back_to_play(&mut q, &mut i, house, htf, info, &mut commands, e);
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                if asking.is_some() {
                    commands.entity(e).remove::<Asking>();
                }
            }
        }
    }
    // (The child up there called to the window: what they were at put by.)
    for (kid, asker, house) in asks {
        let Ok((htf, info, _)) = house_q.get(house) else { continue };
        let name = sims.get(asker).map_or(String::new(), |s| s.1.first.clone());
        let words = label_with(Act::BeAskedUp(asker), info, &name);
        let kind = act(house, htf, Act::BeAskedUp(asker));
        if let Ok((_, _, mut q, ..)) = sims.get_mut(kid) {
            if let Some(a) = q.0.front_mut()
                && at_house(a).is_some_and(|(h, x)| h == house && x.playing())
            {
                a.cancel = true;
                q.0.insert(1, Action::new(words, kind, false));
            } else {
                q.0.push_front(Action::new(words, kind, false));
            }
        }
    }
    // (The answer heard below.)
    for (asker, yes) in replies {
        let clip: &'static str = if yes { "c2c_treehouse_askToComeUp_accept_y" } else { "c2c_treehouse_askToComeUp_reject_y" };
        let len = lib.get(&data, clip).map_or(7.0, |c| c.duration);
        if let Ok((_, _, _, _, _, Some(mut a), _, _)) = sims.get_mut(asker) {
            a.reply = Some((yes, 0.0, len));
        }
        commands.entity(asker).insert(ActionClip::new(Some(clip), &[]));
    }
}

/// WooHoo up a treehouse (`WooHoo`, `TryForBaby`): the two climb in, and once both are up, out of
/// sight in it, the treehouse shakes (`o_treehouse_woohooShake_accept`, the pair's
/// `a2a_treehouse_woohoo_accept`); as the game's WooHoo (liking, and `kWooHooed`), a baby maybe
/// (`kTryForBabySuccessChance`), and a Splinter half the time; then down the ladder.
#[derive(Component)]
struct WooHooing {
    t: f32,
    len: f32,
}

#[allow(clippy::type_complexity)]
fn woohoo(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut SimAnim, Option<&mut InTreehouse>, Option<&mut WooHooing>), Without<HouseInfo>>,
    house_q: Query<&Transform, (With<HouseInfo>, Without<Sim>)>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let now = clock.minutes;
    // (Those up there for it, ready.)
    let ready: Vec<(Entity, Entity)> = sims
        .iter()
        .filter_map(|(e, _, q, _, inside, ..)| {
            let a = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel)?;
            match at_house(a) {
                Some((h, Act::WooHoo(with, _))) if inside.is_some_and(|i| i.house == h) => Some((e, with)),
                _ => None,
            }
        })
        .collect();
    let mut start: Vec<(Entity, Entity, Entity, bool)> = Vec::new();
    for (e, sim, mut q, mut anim, inside, going) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(at_house);
        let Some((house, Act::WooHoo(with, baby))) = front else {
            if going.is_some() {
                commands.entity(e).remove::<WooHooing>();
            }
            continue;
        };
        let Some(mut i) = inside else {
            q.0.pop_front();
            continue;
        };
        let Ok(htf) = house_q.get(house) else { continue };
        if i.doing != Some(Act::WooHoo(with, baby)) {
            i.doing = Some(Act::WooHoo(with, baby));
            i.since = now;
        }
        match going {
            None => {
                let both = ready.contains(&(e, with)) && ready.contains(&(with, e));
                // (The first of the two sets it going.)
                if both && e.to_bits() < with.to_bits() {
                    start.push((e, with, house, baby));
                } else if now - i.since > GIVE_UP {
                    info!("treehouse: {} gives up waiting for WooHoo", sim.first);
                    q.0.pop_front();
                    q.0.push_front(Action::new(GET_OUT, act(house, htf, Act::GetOut), false));
                }
            }
            Some(mut w) => {
                w.t += dt;
                if w.t >= w.len {
                    commands.entity(e).remove::<(WooHooing, ActionClip)>();
                    commands.entity(house).remove::<crate::objanim::PlayingClip>();
                    q.0.pop_front();
                    anim.pose = Pose::Use;
                    i.doing = None;
                    q.0.push_front(Action::new(GET_OUT, act(house, htf, Act::GetOut), false));
                }
            }
        }
    }
    for (a, b, house, baby) in start {
        let shake = "o_treehouse_woohooShake_accept_treehouse";
        let len = lib.get(&data, shake).map_or(7.9, |c| c.duration);
        for (s, clip) in [(a, "a2a_treehouse_woohoo_accept_x"), (b, "a2a_treehouse_woohoo_accept_y")] {
            commands.entity(s).insert((WooHooing { t: 0.0, len }, ActionClip::new(Some(clip), &[])));
        }
        commands.entity(house).insert(crate::objanim::PlayingClip { name: shake, time: 0.0, looping: false });
        info!("treehouse: WooHoo up the treehouse");
        let mut rng = rand::rng();
        let conceived = baby && rng.random_bool(BABY_CHANCE);
        commands.queue(move |w: &mut World| {
            for (x, y) in [(a, b), (b, a)] {
                if let Some(mut r) = w.get_mut::<crate::social::Relationships>(x)
                    && r.add(y, 6.0, 15.0)
                {
                    w.write_message(crate::life::LifeEvent::new(x, crate::life::LifeEventKind::MadeFriend { other: y }));
                }
                w.write_message(crate::life::LifeEvent::new(x, crate::life::LifeEventKind::Socialized { other: y, social: "WooHoo" }));
                w.write_message(crate::dreams::DreamEvent { other: Some(y), ..crate::dreams::DreamEvent::new(x, "kWooHooed") });
                // (A Splinter: the Unlucky and Clumsy likelier, the Lucky never.)
                let traits = w.get::<Sim>(x).map(|s| s.traits.clone()).unwrap_or_default();
                if rand::rng().random_bool(splinter_chance(&traits)) {
                    let now = w.resource::<crate::clock::GameClock>().minutes;
                    if let Some(mut m) = w.get_mut::<crate::life::Moodlets>(x) {
                        m.add(crate::life::MoodletKind::Splinter, now);
                    }
                }
            }
            if conceived {
                w.write_message(crate::little::Conceive { a, b, chance: Some(1.0) });
            }
        });
    }
}

/// How likely a Splinter is from WooHoo up a treehouse (`kSplinterChance...`).
pub fn splinter_chance(traits: &[crate::life::Trait]) -> f64 {
    use crate::life::Trait as T;
    if traits.contains(&T::Lucky) {
        return 0.0;
    }
    let more = traits.iter().any(|t| matches!(t, T::Unlucky | T::Clumsy));
    (SPLINTER_CHANCE * if more { SPLINTER_MORE } else { 1.0 }).min(1.0)
}

/// What's done from a treehouse's menu there and then: a child invited up (theirs to play in now,
/// and up they go to play, or to pretend with one pretending), the sign on the door changed.
pub fn now(w: &mut World, actor: Entity, house: Entity, now: Now) {
    match now {
        Now::InviteUp(child) => {
            let female = w.get::<Sim>(child).is_some_and(|s| s.female);
            if let Some(mut r) = w.get_mut::<HouseRules>(house) {
                r.let_in(child, female);
            }
            let pretending = w.get::<InTreehouse>(actor).is_some_and(|i| i.doing == Some(Act::Pretend));
            let a = if pretending { Act::Pretend } else { Act::Play };
            let (Some(htf), Some(info)) = (w.get::<Transform>(house).copied(), w.get::<HouseInfo>(house).cloned()) else { return };
            if let Some(mut q) = w.get_mut::<ActionQueue>(child) {
                q.push_player(Action::new(label(a, &info), act(house, &htf, a), false));
            }
            info!("treehouse: a child invited up");
        }
        Now::Restrict(r) => {
            if let Some(mut rules) = w.get_mut::<HouseRules>(house) {
                rules.restriction = r;
            }
            info!("treehouse: {}", r.label());
        }
    }
}

/// Children up a treehouse playing, the Evil and the Mean-Spirited above all (their 200), now
/// and then tip water on someone below, the less liked the likelier (`kDumpWaterOnScoreLtrMultiplier`).
#[allow(clippy::type_complexity)]
fn mischief(
    clock: Res<crate::clock::GameClock>,
    mut kids: Query<(Entity, &Sim, &mut ActionQueue, &InTreehouse, Option<&crate::social::Relationships>)>,
    below: Query<(Entity, &Sim, &Transform), (Without<InTreehouse>, Without<Climbing>)>,
    house_q: Query<(&Transform, &HouseInfo)>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 60.0;
    if std::env::var_os("TREEHOUSE_TEST").is_some() {
        return;
    }
    let mut rng = rand::rng();
    for (e, sim, mut q, i, rels) in &mut kids {
        let mean = sim.traits.iter().any(|t| matches!(t, crate::life::Trait::Evil | crate::life::Trait::MeanSpirited));
        if i.grown || i.doing != Some(Act::Play) || !rng.random_bool(if mean { 0.4 } else { 0.05 }) {
            continue;
        }
        let Ok((htf, info)) = house_q.get(i.house) else { continue };
        let liking = |o: Entity| rels.map_or(0.0, |r| r.get(o).friendship);
        let target = below
            .iter()
            .filter(|(o, s, tf)| *o != e && !matches!(s.age, Age::Baby | Age::Toddler) && tf.translation.xz().distance(htf.translation.xz()) < 15.0)
            .min_by(|a, b| liking(a.0).total_cmp(&liking(b.0)));
        let Some((v, vs, _)) = target else { continue };
        if let Some(a) = q.0.front_mut() {
            a.cancel = true;
        }
        q.0.insert(1, Action::new(label_with(Act::DumpWater(v), info, &vs.first), act(i.house, htf, Act::DumpWater(v)), true));
        info!("treehouse: {} means to dump water on {} of their own accord", sim.first, vs.first);
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
            Some(p) => (slot(htf, rig, p.slot()), !i.hidden && !(i.stage == 2 && i.t >= i.len - 0.2)),
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
    (mut stage, mut flags): (Local<(u8, f32, f64)>, Local<u8>),
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
            // (TREEHOUSE_WOOHOO=<first name>: WooHoo up it with them instead.)
            let lover = env("TREEHOUSE_WOOHOO").and_then(|n| sims.iter().find(|s| s.1.first.eq_ignore_ascii_case(&n)).map(|s| s.0));
            for (_, sim, _, mut q, sel, _) in &mut sims {
                let a = if let (true, Some(l)) = (is_kid(sim, sel), lover) {
                    Act::WooHoo(l, env("TREEHOUSE_BABY").is_some())
                } else if is_kid(sim, sel) {
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
            let Some((h, info, htf)) = tree else { return };
            if env("TREEHOUSE_PIE").is_some_and(|p| p == "in") && sims.iter().any(|s| is_kid(s.1, s.4) && s.5) && now - stage.1 > 12.0 {
                commands.insert_resource(crate::hud::PieFor(h));
                info!("treehouse test: its menu, from inside");
                *stage = (9, now, clock.minutes);
                return;
            }
            // (TREEHOUSE_DUMP=<first name>: once they're up, water on them; TREEHOUSE_ASK=<first
            // name>: that child asks to come up.)
            let up = sims.iter().any(|s| is_kid(s.1, s.4) && s.5);
            if up && *flags & 1 == 0 && now - stage.1 > 14.0 {
                *flags |= 1;
                if let Some(v) = env("TREEHOUSE_DUMP").and_then(|n| sims.iter().find(|s| s.1.first.eq_ignore_ascii_case(&n)).map(|s| s.0))
                    && let Some((_, sim, _, mut q, ..)) = sims.iter_mut().find(|s| is_kid(s.1, s.4))
                {
                    if let Some(a) = q.0.front_mut() {
                        a.cancel = true;
                    }
                    q.0.insert(1, Action::new(label_with(Act::DumpWater(v), info, "them"), act(h, htf, Act::DumpWater(v)), false));
                    info!("treehouse test: {} is to dump water", sim.first);
                }
                if let Some(n) = env("TREEHOUSE_ASK")
                    && let Some((_, sim, _, mut q, ..)) = sims.iter_mut().find(|s| s.1.first.eq_ignore_ascii_case(&n))
                {
                    q.push_player(Action::new(label(Act::AskUp, info), water_act(h, htf, info.theme, Act::AskUp), false));
                    info!("treehouse test: {} asks to come up", sim.first);
                }
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

    #[test]
    fn the_sign_on_the_door_keeps_out_whom_it_says() {
        let e = |n: u32| Entity::from_raw_u32(n).unwrap();
        let mut rules = HouseRules::default();
        assert!(rules.allows(e(1)) && !rules.restricted(false, true, 1, 0));
        rules.restriction = Restriction::NoBoys;
        // (Of their own accord, a boy's always kept out; asked, only with girls up there.)
        assert!(rules.restricted(false, true, 0, 0));
        assert!(!rules.restricted(false, false, 0, 0));
        assert!(rules.restricted(false, false, 1, 0));
        assert!(!rules.restricted(true, true, 1, 0), "girls are welcome");
        // (A child in: theirs to play in, others asking; a boy let in takes the sign down.)
        rules.let_in(e(2), true);
        assert!(rules.allows(e(2)) && !rules.allows(e(3)));
        assert_eq!(rules.restriction, Restriction::NoBoys);
        rules.let_in(e(3), false);
        assert_eq!(rules.restriction, Restriction::None);
        assert_eq!(rules.allowed.len(), 2);
        rules.let_in(e(3), false);
        assert_eq!(rules.allowed.len(), 2);
    }

    #[test]
    fn splinters_and_their_words() {
        use crate::life::Trait as T;
        assert_eq!(splinter_chance(&[]), 0.5);
        assert_eq!(splinter_chance(&[T::Clumsy]), 0.875);
        assert_eq!(splinter_chance(&[T::Unlucky, T::Lucky]), 0.0, "never the Lucky");
        let info = HouseInfo { theme: Theme::Club, name: "Kid's Clubhouse".into() };
        let x = Entity::from_raw_u32(9).unwrap();
        assert_eq!(label_with(Act::DumpWater(x), &info, "Arlo"), "Dump Water on Arlo");
        assert_eq!(label_with(Act::WooHoo(x, true), &info, "Judy"), "Try for Baby with Judy in Kid's Clubhouse");
        assert_eq!(label(Act::AskUp, &info), "Ask to Come Up in Kid's Clubhouse");
        assert!(Act::DumpWater(x).inside() && !Act::AskUp.inside() && !Act::BeTricked(x).inside());
        assert_eq!(Theme::Space.pretend_lines().len(), 5);
    }
}
