//! The Dreamer's Costume Chest, Generations' (`CostumeChest`). A child at it "Transform[s]
//! Into... ›" a Tyrannosaurus Rex, an astronaut, a prince or a princess, in one of three colours
//! each (the Red Roaring T-Rex, the Moon Walker White astronaut...): the lid lifted and a costume
//! taken out (`c2o_childCostumeChest_openCloseBox`, the chest's own half with it), a spin into it
//! (the game's uniforms, `kCostumeUniformNames`, worn over their own clothes), and a moment in
//! character. Dressed up, they "Play Pretend" (at the chest, or of their own accord when short of
//! fun): wandering three to five metres at a time in the costume's own walk (the T-Rex's stomp,
//! the astronaut's moon walk, the prince's march, the princess's glide; `RequestWalkStyle`),
//! thinking of dinosaur eggs and mammoths, aliens and spaceships, dragons and castles
//! (`kCostumeTumbnailNames`), and playing the part (`Idle Dino`...); fun, a hundred an hour, till
//! they've had their fill. Those about (within 5 m, three times in four, their queue empty:
//! `kReactToPlayPretendBroadcasterTuning`, `kChanceReact`) stop to watch: a cheer, a laugh or
//! excitement from those who like them, a boo, a pointed laugh or a yawn from those who don't,
//! and from the Childless at heart, the humourless and the mean (`ReactToPretender`). One
//! playing can "Ask To Join ›" another child about, who dresses up as something of their own
//! (`PickRandomCostumeType`) and comes to play two or three metres off (`kMinJoinRouteDistance`).
//! A costume's kept on till it's time for other clothes (bed, the bath, a swim); after four hours
//! in it (`kTimeToAddChangeSocial`) a grown-up may "Ask To Remove Costume", and the child's back
//! in everyday clothes.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::simbody::{OutfitKind, Wearing};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct CostumesPlugin;

impl Plugin for CostumesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (put_on, pretend, react, worn, autonomy, costume_test).chain().after(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's words (`Gameplay/Objects/Toys/CostumeChest`, `Gameplay/Actors/Sim`).
pub const TRANSFORM_INTO: &str = "Transform Into...";
pub const PLAY_PRETEND: &str = "Play Pretend";
pub const ASK_TO_JOIN: &str = "Ask To Join";
pub const ASK_TO_PLAY: &str = "Ask To Play Pretend With";
/// The grown-ups' social for a child long in costume (`AskToRemoveCostume`'s words).
pub const ASK_OUT_OF_COSTUME: &str = "Ask to Change Out of Costume";
/// The game's tuning (its XML's): how far a wander goes, how far off one asked to join plays,
/// how near and how likely others react, how long in a costume before it's to come off (minutes);
/// what playing pretend is worth an hour.
const WANDER: (f32, f32) = (3.0, 5.0);
const JOIN_OFF: (f32, f32) = (2.0, 3.0);
const REACT_WITHIN: f32 = 5.0;
const REACT_CHANCE: f64 = 0.75;
pub const CHANGE_AFTER: f64 = 240.0;
const PRETEND_FUN: f32 = 100.0;
/// How long one Sim leaves before reacting to the same pretender again (game minutes).
const REACT_AGAIN: f64 = 30.0;
/// The topic a child's long in costume gives (`HasWornCostumeForLongTime`).
pub const LONG_IN_COSTUME: &str = "HasWornCostumeForLongTime";

/// The costumes (`CostumeType`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum CostumeType {
    Dino,
    Astronaut,
    Prince,
    Princess,
}

impl CostumeType {
    pub const ALL: [CostumeType; 4] = [CostumeType::Dino, CostumeType::Astronaut, CostumeType::Prince, CostumeType::Princess];

    /// Its place in the pie (`CostumeTypeDino`...).
    pub fn label(self) -> &'static str {
        match self {
            CostumeType::Dino => "A Tyrannosaurus Rex...",
            CostumeType::Astronaut => "An Astronaut...",
            CostumeType::Prince => "A Prince...",
            CostumeType::Princess => "A Princess...",
        }
    }

    /// Its three colours' names (`CostumeSubTypeDinoVar1`...).
    pub fn variants(self) -> [&'static str; 3] {
        match self {
            CostumeType::Dino => ["Red Roaring T-Rex", "Gargantuan Green T-Rex", "Pouncing Pink T-Rex"],
            CostumeType::Astronaut => ["Moon Walker White", "Astro Orange", "Pulsar Purple"],
            CostumeType::Prince => ["Knightly Grey", "Heroic Blue", "Royal Red"],
            CostumeType::Princess => ["Lavish Lavender", "Regal Rose", "Proper Periwinkle"],
        }
    }

    /// Its word in the clips' names.
    fn word(self) -> &'static str {
        match self {
            CostumeType::Dino => "dinosaur",
            CostumeType::Astronaut => "astronaut",
            CostumeType::Prince => "prince",
            CostumeType::Princess => "princess",
        }
    }

    /// Its walk while playing pretend (`WalkStyle.Dinosaur`...).
    pub fn walk(self) -> crate::nav::WalkStyle {
        use crate::nav::WalkStyle as W;
        match self {
            CostumeType::Dino => W::Dinosaur,
            CostumeType::Astronaut => W::Astronaut,
            CostumeType::Prince => W::Prince,
            CostumeType::Princess => W::Princess,
        }
    }

    /// Its idles in character (`Idle Dino`...: the prince has but the one).
    pub fn idles(self) -> Vec<String> {
        let n: &[u8] = if self == CostumeType::Prince { &[2] } else { &[1, 2] };
        n.iter().map(|i| format!("c_childCostumeChest_{}_idle0{i}_x", self.word())).collect()
    }

    /// What they think of playing it.
    pub fn balloons(self) -> [&'static str; 3] {
        let b = s3bake::gamedata::PRETEND_BALLOONS;
        match self {
            CostumeType::Dino => [b[0], b[1], b[2]],
            CostumeType::Astronaut => [b[3], b[4], b[5]],
            CostumeType::Prince | CostumeType::Princess => [b[6], b[7], b[8]],
        }
    }
}

/// A costume: which, in which colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Costume {
    pub kind: CostumeType,
    pub variant: u8,
}

impl Costume {
    /// Its uniform (as baked: by its name, in lower case).
    pub fn outfit(self) -> &'static str {
        let i = (self.variant.min(2)) as usize;
        let k = CostumeType::ALL.iter().position(|t| *t == self.kind).unwrap_or(0);
        crate::pets::intern(s3bake::gamedata::COSTUME_OUTFITS[k * 3 + i].to_ascii_lowercase())
    }

    pub fn name(self) -> &'static str {
        self.kind.variants()[self.variant.min(2) as usize]
    }

    /// One picked for a child of their own accord, or asked to join (`PickRandomCostumeType`: a
    /// T-Rex, an astronaut or a prince, a girl's a princess; any colour).
    pub fn random(female: bool, rng: &mut impl Rng) -> Costume {
        let kind = match rng.random_range(0..3) {
            0 => CostumeType::Dino,
            1 => CostumeType::Astronaut,
            _ if female => CostumeType::Princess,
            _ => CostumeType::Prince,
        };
        Costume { kind, variant: rng.random_range(0..3) }
    }
}

/// The costume a child has on (and since when, game minutes), while they're dressed in it.
#[derive(Component, Clone, Copy, Debug)]
pub struct WornCostume(pub Costume, pub f64);

/// Whether a Sim's dressed up now.
pub fn dressed_up(worn: Option<&WornCostume>, wearing: Option<&Wearing>) -> Option<Costume> {
    worn.filter(|_| wearing.is_some_and(|w| w.0 == OutfitKind::Costume)).map(|w| w.0)
}

/// Who plays with it: children.
pub fn can_play(age: Age) -> bool {
    age == Age::Child
}

/// Where a child stands at a chest to open it, and the way they face (its routing slot).
pub fn chest_spot(obj: &GameObject, tf: &Transform) -> (Vec2, Vec2) {
    let at = obj.use_point(tf);
    let face = obj.route.map(|(_, f)| (tf.rotation * Vec3::new(f.x, 0.0, f.y)).xz()).unwrap_or_else(|| tf.translation.xz() - at).normalize_or(Vec2::Y);
    (at, face)
}

/// Whether a Sim's playing pretend.
pub fn pretending(q: &ActionQueue) -> bool {
    q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::PlayPretend { .. }))
}

/// The chest's menu for a child: "Transform Into... ›" each costume's colours; "Play Pretend"
/// dressed up; "Ask To Join ›" a child about, playing pretend.
#[allow(clippy::too_many_arguments)]
pub fn chest_options(
    chest: Entity,
    spot: (Vec2, Vec2),
    dressed: Option<Costume>,
    playing: bool,
    others: &[(Entity, String)],
    pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>,
    submenu: impl Fn(usize) -> ActionKind,
) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    let mut kinds = Vec::new();
    for k in CostumeType::ALL {
        let list: Vec<(String, ActionKind)> = (0..3u8)
            .map(|variant| {
                let costume = Costume { kind: k, variant };
                (costume.name().to_string(), ActionKind::PutOnCostume { chest, costume, asker: None, at: spot.0, face: spot.1 })
            })
            .collect();
        kinds.push((format!("{} ›", k.label()), submenu(pie_submenus.len())));
        pie_submenus.push((k.label().to_string(), list));
    }
    out.push((format!("{TRANSFORM_INTO} ›"), submenu(pie_submenus.len())));
    pie_submenus.push((TRANSFORM_INTO.to_string(), kinds));
    if dressed.is_some() && !playing {
        out.push((PLAY_PRETEND.to_string(), ActionKind::PlayPretend { asker: None, at: None }));
    }
    if playing && !others.is_empty() {
        let list: Vec<(String, ActionKind)> = others.iter().map(|(o, n)| (n.clone(), ActionKind::AskToPretend { child: *o })).collect();
        out.push((format!("{ASK_TO_JOIN} ›"), submenu(pie_submenus.len())));
        pie_submenus.push((ASK_TO_JOIN.to_string(), list));
    }
    out
}

/// Another child's menu for one playing pretend: "Ask To Play Pretend With" them (dressed up,
/// or with a costume chest about to dress up from).
pub fn ask_option(child: Entity, target: &Sim, target_dressed: bool, target_busy: bool, chest_about: bool) -> Option<(String, ActionKind)> {
    (target.age == Age::Child && !target_busy && (target_dressed || chest_about)).then(|| (ASK_TO_PLAY.to_string(), ActionKind::AskToPretend { child }))
}

/// A Sim's own menu dressed up: "Play Pretend".
pub fn self_options(dressed: Option<Costume>, playing: bool) -> Vec<(String, ActionKind)> {
    if dressed.is_some() && !playing { vec![(PLAY_PRETEND.to_string(), ActionKind::PlayPretend { asker: None, at: None })] } else { Vec::new() }
}

/// Someone asked to come and play pretend with `asker`: dressed up already, they come; else to
/// the nearest chest first, for something of their own (`CreateChangeToCostumeAndJoinInstance`).
pub fn ask(w: &mut World, asker: Entity, child: Entity) {
    let at = w.get::<Transform>(asker).map(|t| t.translation.xz());
    let here = w.get::<Transform>(child).map(|t| t.translation.xz());
    let (Some(at), Some(here)) = (at, here) else { return };
    let dressed = dressed_up(w.get::<WornCostume>(child), w.get::<Wearing>(child)).is_some();
    let female = w.get::<Sim>(child).is_some_and(|s| s.female);
    let mut rng = rand::rng();
    let kind = if dressed {
        ActionKind::PlayPretend { asker: Some(asker), at: Some(near(at, here, &mut rng)) }
    } else {
        let mut q = w.query::<(Entity, &GameObject, &Transform)>();
        let Some((chest, spot)) = q.iter(w).filter(|c| c.1.kind == ObjectKind::CostumeChest).min_by(|a, b| a.2.translation.xz().distance(here).total_cmp(&b.2.translation.xz().distance(here))).map(|c| (c.0, chest_spot(c.1, c.2))) else {
            return;
        };
        ActionKind::PutOnCostume { chest, costume: Costume::random(female, &mut rng), asker: Some(asker), at: spot.0, face: spot.1 }
    };
    if let Some(mut q) = w.get_mut::<ActionQueue>(child) {
        let label = if dressed { PLAY_PRETEND } else { TRANSFORM_INTO };
        q.push_player(Action::new(label, kind, false));
    }
}

/// A spot two or three metres from someone, toward where one comes from.
fn near(at: Vec2, from: Vec2, rng: &mut impl Rng) -> Vec2 {
    let dir = (from - at).normalize_or(Vec2::X);
    let turn = rng.random_range(-0.6f32..0.6);
    at + Vec2::from_angle(turn).rotate(dir) * rng.random_range(JOIN_OFF.0..JOIN_OFF.1)
}

/// A child out of their costume (asked, `RemoveCostume`): a spin into everyday clothes.
pub fn take_off(w: &mut World, child: Entity) {
    if dressed_up(w.get::<WornCostume>(child), w.get::<Wearing>(child)).is_none() {
        return;
    }
    if let Some(mut t) = w.get_mut::<crate::topics::Topics>(child) {
        t.0.retain(|h| h.name != LONG_IN_COSTUME);
    }
    if let Some(mut q) = w.get_mut::<ActionQueue>(child) {
        q.0.retain(|a| !matches!(a.kind, ActionKind::PlayPretend { .. }));
    }
    w.entity_mut(child).remove::<WornCostume>();
    crate::dressing::start_in(w, child, None, false);
    info!("costumes: out of costume");
}

/// Dressing up at the chest, step by step: the lid up and the costume out, the spin into it, a
/// moment in character.
#[derive(Component)]
struct Dressing {
    step: u8,
    t: f32,
    length: f32,
}

#[allow(clippy::type_complexity)]
fn put_on(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut SimAnim, Option<&mut Dressing>, Has<crate::dressing::ClothesSpin>)>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let mut rng = rand::rng();
    for (e, sim, mut q, mut anim, dressing, spinning) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).and_then(|a| match a.kind {
            ActionKind::PutOnCostume { costume, asker, .. } => Some((costume, asker)),
            _ => None,
        });
        let Some((costume, asker)) = front else {
            if dressing.is_some() {
                commands.entity(e).remove::<Dressing>();
            }
            continue;
        };
        let Some(mut d) = dressing else {
            // (Not a child's to wear: put back.)
            if !can_play(sim.age) {
                q.0.pop_front();
                continue;
            }
            let open = "c2o_childCostumeChest_openCloseBox_x";
            let length = lib.get(&data, open).map_or(2.7, |c| c.duration);
            anim.pose = Pose::Use;
            commands.entity(e).insert((ActionClip::new(Some(open), &[]), Dressing { step: 0, t: 0.0, length }));
            continue;
        };
        if spinning {
            continue;
        }
        d.t += dt;
        if d.t < d.length {
            continue;
        }
        match d.step {
            // The costume out: into it with a spin.
            0 => {
                info!("costumes: {} dresses up as a {:?} ({})", sim.first, costume.kind, costume.name());
                commands.entity(e).insert(WornCostume(costume, clock.minutes));
                crate::dressing::start(&mut commands, e, Some(OutfitKind::Costume), true);
                d.step = 1;
                d.t = 0.0;
                d.length = 0.0;
            }
            // In character a moment.
            1 => {
                let idles = costume.kind.idles();
                let idle = crate::pets::intern(idles.choose(&mut rng).cloned().unwrap_or_default());
                d.length = lib.get(&data, idle).map_or(3.0, |c| c.duration);
                d.t = 0.0;
                d.step = 2;
                anim.pose = Pose::Use;
                commands.entity(e).insert(ActionClip::new(Some(idle), &[]));
            }
            _ => {
                q.0.pop_front();
                commands.entity(e).remove::<(Dressing, ActionClip)>();
                anim.pose = Pose::Stand;
                // (Asked to join: off to play with them.)
                if let Some(a) = asker {
                    commands.queue(move |w: &mut World| {
                        let at = w.get::<Transform>(a).map(|t| t.translation.xz());
                        let here = w.get::<Transform>(e).map(|t| t.translation.xz());
                        let spot = at.zip(here).map(|(at, here)| near(at, here, &mut rand::rng()));
                        if let Some(mut q) = w.get_mut::<ActionQueue>(e) {
                            q.0.push_front(Action::new(PLAY_PRETEND, ActionKind::PlayPretend { asker: Some(a), at: spot }, false));
                        }
                    });
                }
            }
        }
    }
}

/// Playing pretend: a wander in the costume's walk, a thought of what they're playing, the part
/// played; fun as they go, till they've had their fill or something else wants doing.
#[derive(Component)]
pub struct Pretending {
    step: u8,
    t: f32,
    length: f32,
    pub kind: CostumeType,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn pretend(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut SimAnim, &Transform, &mut crate::sim::Motives, (Option<&WornCostume>, Option<&Wearing>, Option<&mut Pretending>, Option<&crate::nav::PathFollow>))>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let hours = delta.0 / 60.0;
    let mut rng = rand::rng();
    // (Kept in the room they're in, as the game's `Wander` in their room: indoors or out.)
    let indoors = |p: Vec2| building.as_deref().is_some_and(|b| b.is_indoors(Vec3::new(p.x, 0.0, p.y)));
    for (e, sim, mut q, mut anim, tf, mut m, (worn, wearing, pretending, path)) in &mut sims {
        let playing = q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::PlayPretend { .. }) && matches!(a.phase, Phase::Running(_)) && !a.cancel);
        let dressed = dressed_up(worn, wearing);
        let Some(mut p) = pretending else {
            if !playing {
                continue;
            }
            // (Out of costume: nothing to play.)
            let Some(c) = dressed else {
                q.0.pop_front();
                continue;
            };
            info!("costumes: {} plays pretend ({:?})", sim.first, c.kind);
            commands.entity(e).insert(Pretending { step: 0, t: 0.0, length: 0.0, kind: c.kind });
            continue;
        };
        if !playing || dressed.is_none() {
            commands.entity(e).remove::<(Pretending, ActionClip)>();
            // (Off somewhere else: not in the costume's walk.)
            if path.is_some_and(|f| costume_walk(f.style)) {
                commands.entity(e).remove::<crate::nav::PathFollow>();
            }
            if !playing {
                continue;
            }
            q.0.pop_front();
            continue;
        }
        if hours > 0.0 {
            m.add(crate::sim::FUN, PRETEND_FUN * hours);
        }
        // (Had their fill, or something wants seeing to: done.)
        let needy = [crate::sim::HUNGER, crate::sim::BLADDER, crate::sim::ENERGY].iter().any(|i| m.0[*i] < -40.0);
        if p.step == 0 && (m.0[crate::sim::FUN] >= 95.0 || needy || q.0.len() > 1) {
            info!("costumes: {} stops playing pretend", sim.first);
            q.0.pop_front();
            commands.entity(e).remove::<(Pretending, ActionClip)>();
            anim.pose = Pose::Stand;
            continue;
        }
        match p.step {
            // A wander, in the costume's walk.
            0 => {
                let here = tf.translation.xz();
                let g = grid.as_deref();
                let inside = indoors(here);
                let to = (0..16).find_map(|_| {
                    let d = Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU)) * rng.random_range(WANDER.0..WANDER.1);
                    let to = here + d;
                    if indoors(to) != inside {
                        return None;
                    }
                    match g {
                        Some(g) => g.find_path(here, to).filter(|w| w.len() <= 8),
                        None => Some(vec![to]),
                    }
                });
                if let Some(way) = to {
                    let wp: Vec<crate::nav::Waypoint> = way.into_iter().map(|p| crate::nav::Waypoint { p, level: 1, climb: None, spiral: None }).collect();
                    commands.entity(e).remove::<ActionClip>().insert(crate::nav::PathFollow::new(wp).with_style(p.kind.walk()));
                }
                p.step = 1;
            }
            // There: a thought of what they play, and the part played.
            1 => {
                if path.is_some_and(|f| !f.done) {
                    continue;
                }
                commands.entity(e).remove::<crate::nav::PathFollow>();
                let idles = p.kind.idles();
                let idle = crate::pets::intern(idles.choose(&mut rng).cloned().unwrap_or_default());
                p.length = lib.get(&data, idle).map_or(3.0, |c| c.duration);
                p.t = 0.0;
                p.step = 2;
                anim.pose = Pose::Use;
                let b = p.kind.balloons();
                commands.entity(e).insert((ActionClip::new(Some(idle), &[]), crate::balloons::BalloonRequest::thought(b[rng.random_range(0..3)])));
            }
            _ => {
                p.t += dt;
                if p.t >= p.length {
                    p.step = 0;
                }
            }
        }
    }
}

/// Whether a route's gone in a costume's walk.
fn costume_walk(s: crate::nav::WalkStyle) -> bool {
    use crate::nav::WalkStyle as W;
    matches!(s, W::Dinosaur | W::Astronaut | W::Prince | W::Princess)
}

/// Those about a child playing pretend stop to watch now and then (`ReactToPretender`): pleased
/// if they like them (`kLTRForPositiveReaction`), else not; never pleased, the Childless at heart
/// (`DislikesChildren`), the humourless and the mean (`kTraitsForBadReaction`).
#[allow(clippy::type_complexity)]
fn react(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    pretenders: Query<(Entity, &Transform), With<Pretending>>,
    watchers: Query<(Entity, &Sim, &Transform, &ActionQueue, Option<&crate::social::Relationships>, Has<crate::reactions::Pending>), Without<Pretending>>,
    mut seen: Local<std::collections::HashMap<(Entity, Entity), f64>>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 1.0;
    seen.retain(|_, t| clock.minutes - *t < REACT_AGAIN);
    let mut rng = rand::rng();
    for (p, ptf) in &pretenders {
        for (w, sim, wtf, q, rels, pending) in &watchers {
            if !q.0.is_empty() || pending || sim.age.is_little() || wtf.translation.distance(ptf.translation) > REACT_WITHIN || seen.contains_key(&(w, p)) {
                continue;
            }
            seen.insert((w, p), clock.minutes);
            if !rng.random_bool(REACT_CHANCE) {
                continue;
            }
            let kind = reaction(sim, rels.map_or(0.0, |r| r.friendship(p)), &mut rng);
            debug!("costumes: {} reacts to a pretender: {kind}", sim.first);
            commands.entity(w).insert(crate::reactions::Pending::new(&[kind], Some(ptf.translation), clock.minutes + 10.0));
        }
    }
}

/// How a Sim takes a child playing pretend (`kPositiveReactions`, `kNegativeReactions`).
pub fn reaction(sim: &Sim, liking: f32, rng: &mut impl Rng) -> &'static str {
    use crate::life::Trait as T;
    const GOOD: [&str; 3] = ["Cheer", "LaughAt", "Excited"];
    const BAD: [&str; 3] = ["Boo", "PointLaugh", "Yawn"];
    let sour = sim.traits.iter().any(|t| matches!(t, T::DislikesChildren | T::NoSenseOfHumor | T::MeanSpirited));
    let list = if liking < 0.0 || sour { BAD } else { GOOD };
    list[rng.random_range(0..3)]
}

/// Long in costume: the topic for a grown-up's asking them out of it; out of it, gone.
#[allow(clippy::type_complexity)]
fn worn(mut commands: Commands, clock: Res<crate::clock::GameClock>, mut sims: Query<(Entity, &WornCostume, Option<&Wearing>, Option<&mut crate::topics::Topics>), Without<crate::dressing::ClothesSpin>>, mut topics: MessageWriter<crate::topics::AddTopic>) {
    for (e, c, wearing, t) in &mut sims {
        if wearing.is_none_or(|w| w.0 != OutfitKind::Costume) {
            commands.entity(e).remove::<WornCostume>();
            if let Some(mut t) = t {
                t.0.retain(|h| h.name != LONG_IN_COSTUME);
            }
            continue;
        }
        if clock.minutes - c.1 >= CHANGE_AFTER && t.is_none_or(|t| t.get(LONG_IN_COSTUME).is_none()) {
            topics.write(crate::topics::AddTopic::new(e, LONG_IN_COSTUME));
        }
    }
}

/// Children dressed up and short of fun play pretend of their own accord now and then.
#[allow(clippy::type_complexity)]
fn autonomy(clock: Res<crate::clock::GameClock>, mut sims: Query<(&Sim, &mut ActionQueue, &crate::sim::Motives, Option<&WornCostume>, Option<&Wearing>), With<crate::sim::HouseholdMember>>, mut next: Local<f64>) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 30.0;
    if std::env::var_os("COSTUME_TEST").is_some() {
        return;
    }
    let mut rng = rand::rng();
    for (sim, mut q, m, worn, wearing) in &mut sims {
        if q.0.is_empty() && dressed_up(worn, wearing).is_some() && m.0[crate::sim::FUN] < 40.0 && rng.random_bool(0.5) {
            q.0.push_back(Action::new(PLAY_PRETEND, ActionKind::PlayPretend { asker: None, at: None }, true));
            info!("costumes: {} plays pretend of their own accord", sim.first);
        }
    }
}

/// `COSTUME_TEST=<kind>[:<colour>]` (tests: `Dino`, `Princess:2`): ten seconds in, a costume chest
/// put down near the household's first child, who dresses up and plays pretend; with
/// `COSTUME_JOIN=<first name>` asking that child to join.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn costume_test(
    mut commands: Commands,
    time: Res<Time>,
    mut kids: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Option<&WornCostume>, Option<&Wearing>), With<crate::sim::HouseholdMember>>,
    chests: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(spec) = std::env::var("COSTUME_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let who = std::env::var("COSTUME_WHO").ok();
    let is_kid = |s: &Sim| who.as_ref().map_or(s.age == Age::Child, |w| s.first.eq_ignore_ascii_case(w));
    let chest = chests.iter().find(|c| c.1.kind == ObjectKind::CostumeChest);
    if let (Ok(mut c), Some((_, _, ktf, ..))) = (cams.single_mut(), kids.iter().find(|k| is_kid(k.1))) {
        c.look_at(ktf.translation + Vec3::Y * 0.6);
        c.distance = std::env::var("COSTUME_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(6.0);
        c.pitch = 0.45;
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let Some((_, _, ktf, ..)) = kids.iter().find(|k| is_kid(k.1)) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("wardrobeChest")).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = ktf.translation.xz();
            let spot = (2..12).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32)).find(|c| (-3..=3).all(|i| (-3..=3).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.5)))).unwrap_or(here + Vec2::Y * 2.0);
            let p = Vec3::new(spot.x, ktf.translation.y, spot.y);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("costume test: a chest at {p:.1}");
            }
        }
        1 if now - stage.1 > 3.0 => {
            let Some((c, obj, ctf)) = chest else { return };
            *stage = (2, now);
            let (kind, variant) = spec.split_once(':').map_or((spec.as_str(), 0), |(k, v)| (k, v.parse().unwrap_or(0)));
            let kind = CostumeType::ALL.into_iter().find(|t| format!("{t:?}").eq_ignore_ascii_case(kind)).unwrap_or(CostumeType::Dino);
            let costume = Costume { kind, variant };
            let (at, face) = chest_spot(obj, ctf);
            if let Some((_, sim, _, mut q, ..)) = kids.iter_mut().find(|k| is_kid(k.1)) {
                q.push_player(Action::new(TRANSFORM_INTO, ActionKind::PutOnCostume { chest: c, costume, asker: None, at, face }, false));
                q.push_player(Action::new(PLAY_PRETEND, ActionKind::PlayPretend { asker: None, at: None }, false));
                info!("costume test: {} dresses up as {}", sim.first, costume.name());
            }
        }
        2 if now - stage.1 > 25.0 => {
            *stage = (3, now);
            // (COSTUME_REMOVE=<first name>: it's been on four hours, and they ask them out of it.)
            if std::env::var_os("COSTUME_REMOVE").is_some()
                && let Some((k, ..)) = kids.iter().find(|k| is_kid(k.1))
            {
                commands.entity(k).queue(|mut e: EntityWorldMut| {
                    if let Some(mut w) = e.get_mut::<WornCostume>() {
                        w.1 -= CHANGE_AFTER;
                    }
                });
            }
            let Ok(n) = std::env::var("COSTUME_JOIN") else { return };
            let asker = kids.iter().find(|k| is_kid(k.1)).map(|k| k.0);
            let child = kids.iter().find(|k| k.1.first.eq_ignore_ascii_case(&n)).map(|k| k.0);
            if let (Some(a), Some(c)) = (asker, child) {
                commands.queue(move |w: &mut World| ask(w, a, c));
                info!("costume test: {n} asked to join");
            }
        }
        3 if now - stage.1 > 5.0 => {
            let Ok(n) = std::env::var("COSTUME_REMOVE") else { return };
            *stage = (4, now);
            let child = kids.iter().find(|k| is_kid(k.1)).map(|k| k.0);
            let Some(si) = crate::social::social_index(ASK_OUT_OF_COSTUME) else {
                warn!("costume test: no {ASK_OUT_OF_COSTUME} social");
                return;
            };
            if let (Some(target), Some((_, sim, _, mut q, ..))) = (child, kids.iter_mut().find(|k| k.1.first.eq_ignore_ascii_case(&n))) {
                q.push_player(Action::new(ASK_OUT_OF_COSTUME, ActionKind::Social { target, social: si }, false));
                info!("costume test: {} asks them out of their costume", sim.first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_costume_is_the_games() {
        let c = Costume { kind: CostumeType::Princess, variant: 2 };
        assert_eq!(c.outfit(), "cubodyep4dressprincessvar3");
        assert_eq!(c.name(), "Proper Periwinkle");
        assert_eq!(Costume { kind: CostumeType::Dino, variant: 0 }.outfit(), "cubodyep4dinosaurvar1");
        assert_eq!(CostumeType::Prince.idles(), ["c_childCostumeChest_prince_idle02_x"], "the prince has but the one");
        assert_eq!(CostumeType::Astronaut.balloons()[0], "ep4_balloon_alien");
        assert_eq!(CostumeType::Dino.walk(), crate::nav::WalkStyle::Dinosaur);
        let mut rng = rand::rng();
        for _ in 0..50 {
            let girl = Costume::random(true, &mut rng);
            assert_ne!(girl.kind, CostumeType::Prince);
            assert_ne!(Costume::random(false, &mut rng).kind, CostumeType::Princess);
        }
    }

    #[test]
    fn the_chest_offers_each_costume_in_three_colours() {
        let chest = Entity::from_raw_u32(1).unwrap();
        let mut subs = Vec::new();
        let opts = chest_options(chest, (Vec2::ZERO, Vec2::Y), None, false, &[], &mut subs, |i| ActionKind::Notice(i.to_string()));
        assert_eq!(opts.len(), 1, "not dressed up: just the costumes");
        assert_eq!(opts[0].0, "Transform Into... ›");
        let top = subs.iter().find(|s| s.0 == TRANSFORM_INTO).unwrap();
        assert_eq!(top.1.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["A Tyrannosaurus Rex... ›", "An Astronaut... ›", "A Prince... ›", "A Princess... ›"]);
        let dino = subs.iter().find(|s| s.0 == "A Tyrannosaurus Rex...").unwrap();
        assert_eq!(dino.1[1].0, "Gargantuan Green T-Rex");
        let dressed = Costume { kind: CostumeType::Dino, variant: 0 };
        let opts = chest_options(chest, (Vec2::ZERO, Vec2::Y), Some(dressed), false, &[], &mut Vec::new(), |i| ActionKind::Notice(i.to_string()));
        assert!(opts.iter().any(|o| o.0 == PLAY_PRETEND));
        let opts = chest_options(chest, (Vec2::ZERO, Vec2::Y), Some(dressed), true, &[(chest, "Lucy".into())], &mut Vec::new(), |i| ActionKind::Notice(i.to_string()));
        assert!(opts.iter().any(|o| o.0 == "Ask To Join ›") && !opts.iter().any(|o| o.0 == PLAY_PRETEND));
    }

    #[test]
    fn those_who_dont_care_for_children_boo() {
        let mut rng = rand::rng();
        let mut sim = crate::sim::random_sim(&mut rng, "Grumpy", Some(false), Age::Adult);
        sim.traits = vec![crate::life::Trait::DislikesChildren];
        assert!(["Boo", "PointLaugh", "Yawn"].contains(&reaction(&sim, 50.0, &mut rng)));
        sim.traits.clear();
        assert!(["Cheer", "LaughAt", "Excited"].contains(&reaction(&sim, 10.0, &mut rng)));
        assert!(["Boo", "PointLaugh", "Yawn"].contains(&reaction(&sim, -10.0, &mut rng)));
    }
}
