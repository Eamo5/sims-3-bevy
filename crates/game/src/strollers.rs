//! Strollers, Generations' (`Stroller`: the Twinbrook Roller, the Air-Strol LX). A teen or
//! grown-up "Go[es] on Stroll With... ›" a baby or toddler of theirs (the one in their arms, or
//! one about, picked up first, out of their crib if need be: `GetValidFollowers`). From behind
//! the stroller (its `_route_adult` slot) they put the little one in (the game's
//! `holdBaby2stroller`, a toddler's `holdTodder2stroller` and its second part: a baby passed from
//! the hand to the seat, `_ctnm_baby`, a toddler to the footwell, `_ctnm_toddler`, as the clips'
//! parent events have them) and push it along in the stroller walk (`StrollerWalk`: the
//! stroller's own half of each step, its wheels turning) out to the street and up and down its
//! sidewalk, the little one breathing, looking about or fretting in it as they feel
//! (`happy_idle`, `unhappy_idle`: the game's `Cry` while a need's going badly). After two hours
//! (`kTimeToStrollInMinutes`), or told to stop, they come home, take the little one back into
//! their arms (`holdStroller2baby`, `holdStroller2toddler`) and leave the stroller where it
//! stood. Strolling is fun and company for the one pushing (`GoOnStrollSocial`: Fun 200,
//! Social 200 an hour) and for the little one (`kToddlerCommodityDeltas`: Social 50, Fun 50,
//! Energy 5, the XML's), and brings them closer (`kRelationshipDeltaLTR`, 5 an hour); three
//! quarters of an hour of it (`kTimeToAddBuffInMinutes`) and the little one's Had a Good Stroll
//! (+10, three hours from the stroll's end). By day, one of the household short of fun goes of
//! their own accord now and then.

use bevy::prelude::*;
use rand::Rng;

use crate::anim::{ActionClip, ClipLibrary, ClipPlayer};
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::nav::{PathFollow, Waypoint, WalkStyle};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct StrollersPlugin;

impl Plugin for StrollersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (hold, start).chain().before(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, (stroll, autonomy, stroller_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, place.after(crate::anim::drive_skeletons).before(crate::objanim::animate_objects).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's words (`Gameplay/Objects/Vehicles/Stroller`).
pub const GO_ON_STROLL: &str = "Go on Stroll With...";
/// The game's tuning (its XML's): how long a stroll lasts and how far in the little one's had a
/// good one (minutes); what it's worth an hour to the one pushing (fun, company), to the little
/// one (company, fun, energy), and to how they get on.
pub const STROLL_MINUTES: f64 = 120.0;
const GOOD_STROLL_AFTER: f64 = 45.0;
const PUSHER_FUN: f32 = 200.0;
const PUSHER_SOCIAL: f32 = 200.0;
const LITTLE_SOCIAL: f32 = 50.0;
const LITTLE_FUN: f32 = 50.0;
const LITTLE_ENERGY: f32 = 5.0;
const LIKING: f32 = 5.0;
/// Where the one pushing stands: the stroller's `_route_adult`, this far behind its root.
pub const BEHIND: f32 = 0.763;
/// How far before a grown-up a toddler's held (the carry jig: `carry::apart`).
const TODDLER_JIG: f32 = 0.6;
/// The stroller's own clips and the one pushing's.
const WALK: &str = "a2o_stroller_walk_x";
const STROLLER_WALK: &str = "a2o_stroller_walk_stroller";
const HOLD: &str = "a2o_Stroller_WalkOverlay_x";
/// The stroller's bones the little ones ride in (fnv32 of `_ctnm_baby`, `_ctnm_toddler`), and the
/// pusher's right hand slot a baby's handed over from (`b__R_Hand_slot`).
const SEAT_BABY: u32 = 0x38F3_B031;
const SEAT_TODDLER: u32 = 0xE62C_CE5F;
const HAND: &str = "b__R_Hand_slot";

/// Where a teen or grown-up stands to push a stroller standing so, and the way they face.
pub fn behind(stroller: &Transform) -> (Vec2, Vec2) {
    let at = stroller.transform_point(Vec3::new(0.0, 0.0, -BEHIND)).xz();
    (at, (stroller.rotation * Vec3::Z).xz().normalize_or(Vec2::Y))
}

/// Where the stroller is as it's pushed by someone standing so.
pub fn pushed(pusher: &Transform) -> Transform {
    Transform { translation: pusher.transform_point(Vec3::new(0.0, 0.0, BEHIND)), rotation: pusher.rotation, scale: Vec3::ONE }
}

/// Who pushes one: teens and up (`CanBeUsedBy`).
pub fn can_push(age: Age) -> bool {
    crate::carry::can_carry(age)
}

/// A stroller's menu: "Go on Stroll With... ›" each little one to take (none to take, none).
pub fn options(stroller: Entity, stf: &Transform, littles: &[(Entity, String)], pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>, submenu: impl Fn(usize) -> ActionKind) -> Vec<(String, ActionKind)> {
    if littles.is_empty() {
        return Vec::new();
    }
    let (at, face) = behind(stf);
    let list: Vec<(String, ActionKind)> = littles.iter().map(|(c, n)| (n.clone(), ActionKind::Stroll { stroller, child: *c, at, face })).collect();
    let out = vec![(format!("{GO_ON_STROLL} ›"), submenu(pie_submenus.len()))];
    pie_submenus.push((GO_ON_STROLL.to_string(), list));
    out
}

/// A little one in a stroller (placed by it, not the arms).
#[derive(Component)]
pub struct InStroller;

/// A part of a stroll: the little one put in (a toddler's in two), pushed along, taken out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    In(u8),
    Walk,
    Out(u8),
}

/// The clips of a part, the pusher's and the little one's (`a2b_` a baby's, `a2p_` a toddler's).
pub fn part_clips(part: Part, baby: bool) -> Option<(&'static str, &'static str)> {
    Some(match (part, baby) {
        (Part::In(0), true) => ("a2b_stroller_transition_holdBaby2stroller_x", "a2b_stroller_transition_holdBaby2stroller_y"),
        (Part::Out(0), true) => ("a2b_stroller_transition_holdStroller2baby_x", "a2b_stroller_transition_holdStroller2baby_y"),
        (Part::In(0), false) => ("a2p_stroller_transition_holdTodder2stroller_x", "a2p_stroller_transition_holdTodder2stroller_y"),
        (Part::In(1), false) => ("a2p_stroller_transition_holdTodder2stroller_pt2_x", "a2p_stroller_transition_holdTodder2stroller_pt2_y"),
        (Part::Out(0), false) => ("a2p_stroller_transition_holdStroller2toddler_x", "a2p_stroller_transition_holdStroller2toddler_y"),
        (Part::Out(1), false) => ("a2p_stroller_transition_holdStroller2toddler_pt2_x", "a2p_stroller_transition_holdStroller2toddler_pt2_y"),
        _ => return None,
    })
}

/// Where a little one is through a part, as its clip's parent events have it: a baby in the
/// pusher's hand till it's handed over (0.73 s in), then in the seat; taken out, the other way
/// round (0.67 s in); a toddler before the pusher as they're held (the carry jig), till they're
/// in. And whether they're out of sight just then (the clips' fades: `babyStartFadeOut`...).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hold {
    Hand,
    Jig,
    Seat,
}

pub fn hold_at(part: Part, baby: bool, t: f32) -> (Hold, bool) {
    match (part, baby) {
        (Part::In(_), true) => (if t < 0.733 { Hold::Hand } else { Hold::Seat }, (0.5..1.167).contains(&t)),
        (Part::Out(_), true) => (if t < 0.667 { Hold::Seat } else { Hold::Hand }, t < 0.667),
        (Part::In(_) | Part::Out(_), false) => (Hold::Jig, false),
        (Part::Walk, _) => (Hold::Seat, false),
    }
}

/// A little one's loops in the stroller: content, or fretting while a need's going badly.
pub fn idles(baby: bool, crying: bool) -> &'static [&'static str] {
    match (baby, crying) {
        (true, false) => &["b2o_Stroller_breathe", "b2o_Stroller_happy_idle"],
        (true, true) => &["b2o_Stroller_unhappy_idle"],
        (false, false) => &["p2o_Stroller_breath", "p2o_Stroller_happy_idle"],
        (false, true) => &["p2o_Stroller_unhappy_idle"],
    }
}

/// A stroll under way: the stroller and where it stood, the little one, the part and how far in,
/// how long it is, since when it's been walked, whether they're on their way home, which leg of
/// the way, the Had a Good Stroll given.
#[derive(Component)]
pub struct Strolling {
    pub stroller: Entity,
    pub child: Entity,
    pub baby: bool,
    home: Transform,
    pub part: Part,
    t: f32,
    length: f32,
    since: f64,
    pub returning: bool,
    leg: u32,
    good: bool,
    /// Told to stop (cancelled from the queue): home.
    told: bool,
}

/// Told to stop midway: they head home and take the little one out first, rather than drop
/// them where they are.
fn hold(mut sims: Query<(&mut ActionQueue, Option<&mut Strolling>)>) {
    for (mut q, s) in &mut sims {
        let Some(a) = q.0.front_mut() else { continue };
        if !matches!(a.kind, ActionKind::Stroll { .. }) || !a.cancel {
            continue;
        }
        if let Some(mut s) = s {
            a.cancel = false;
            s.told = true;
        }
    }
}

/// Setting off for a stroll with a little one not in their arms: picked up first (once: not
/// got hold of, no stroll).
fn start(mut sims: Query<(Entity, &mut ActionQueue, Option<&crate::carry::Holding>)>, names: Query<&Sim>, look: crate::carry::CarryLook, mut tried: Local<std::collections::HashSet<Entity>>) {
    for (e, mut q, holding) in &mut sims {
        let Some(a) = q.0.front() else { continue };
        let ActionKind::Stroll { child, .. } = a.kind else { continue };
        if a.phase != Phase::Start || a.cancel || holding.is_some_and(|h| h.0 == child) {
            tried.remove(&e);
            continue;
        }
        if !tried.insert(e) {
            q.0.pop_front();
            continue;
        }
        let step = look.pick_up(child);
        let name = names.get(child).map_or(String::new(), |s| s.first.clone());
        q.0.push_front(Action::new(crate::carry::label(step, &name, false), ActionKind::Carry { child, step, secs: f32::INFINITY }, false));
    }
}

/// The stroll played out part by part: the little one put in, pushed out along the street and
/// back, taken out; what it's worth as they go.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn stroll(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut pushers: Query<(Entity, &Sim, &mut ActionQueue, &mut Transform, &mut SimAnim, &mut crate::sim::Motives, Option<&mut Strolling>, Option<&crate::carry::Holding>, Option<&PathFollow>), (Without<InStroller>, Without<crate::little::Carried>)>,
    mut littles: Query<(&Sim, &mut crate::sim::Motives, &mut crate::life::Moodlets, Option<&ActionClip>), (With<crate::little::Carried>, Without<Strolling>)>,
    mut strollers: Query<&mut Transform, (With<GameObject>, Without<Sim>)>,
    (grid, exit, sidewalk): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::interact::LotExit>>, Option<Res<crate::town::Sidewalk>>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let hours = delta.0 / 60.0;
    let mut rng = rand::rng();
    for (e, sim, mut q, mut tf, mut anim, mut m, strolling, holding, path) in &mut pushers {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(|a| match a.kind {
            ActionKind::Stroll { stroller, child, .. } => Some((stroller, child)),
            _ => None,
        });
        let Some(mut s) = strolling else {
            let Some((stroller, child)) = front else { continue };
            // (Not in their arms after all: no stroll.)
            if holding.is_none_or(|h| h.0 != child) {
                q.0.pop_front();
                continue;
            }
            let (Ok(stf), Ok((little, ..))) = (strollers.get(stroller), littles.get(child)) else {
                q.0.pop_front();
                continue;
            };
            let baby = little.age == Age::Baby;
            info!("strollers: {} takes {} out in the stroller", sim.first, little.first);
            commands.entity(e).remove::<crate::anim::Carrying>().insert(Strolling {
                stroller,
                child,
                baby,
                home: *stf,
                part: Part::In(0),
                t: 0.0,
                length: 0.0,
                since: clock.minutes,
                returning: false,
                leg: 0,
                good: false,
                told: false,
            });
            commands.entity(child).insert(InStroller);
            continue;
        };
        // (The stroller gone, or the little one: over where they are.)
        if front.is_none_or(|f| f.0 != s.stroller || f.1 != s.child) || strollers.get(s.stroller).is_err() || littles.get(s.child).is_err() {
            finish(&mut commands, e, &s, &mut strollers);
            if front.is_some() {
                q.0.pop_front();
            }
            continue;
        }
        let (child, baby) = (s.child, s.baby);
        let Ok((little, mut lm, mut moods, lclip)) = littles.get_mut(child) else { continue };
        match s.part {
            Part::In(_) | Part::Out(_) => {
                if s.length == 0.0 {
                    let Some((x, y)) = part_clips(s.part, baby) else { continue };
                    s.length = lib.get(&data, x).map_or(1.0, |c| c.duration).max(0.2);
                    s.t = 0.0;
                    anim.pose = Pose::Use;
                    commands.entity(e).remove::<PathFollow>().insert(ActionClip::new(Some(x), &[]));
                    commands.entity(child).insert(ActionClip::new(Some(y), &[]));
                    debug!("strollers: {:?}: {x}", s.part);
                    continue;
                }
                s.t += dt;
                if s.t < s.length {
                    continue;
                }
                s.length = 0.0;
                s.part = match s.part {
                    Part::In(0) if !baby => Part::In(1),
                    Part::In(_) => {
                        s.since = clock.minutes;
                        Part::Walk
                    }
                    Part::Out(0) if !baby => Part::Out(1),
                    _ => {
                        // Home: the little one back in their arms, the stroller where it stood.
                        info!("strollers: {} is back from the stroll with {} ({:.0} minutes)", sim.first, little.first, clock.minutes - s.since);
                        if s.good {
                            moods.add(crate::life::MoodletKind::HadAGoodStroll, clock.minutes);
                        }
                        finish(&mut commands, e, &s, &mut strollers);
                        q.0.pop_front();
                        anim.pose = Pose::Stand;
                        continue;
                    }
                };
                if s.part == Part::Walk {
                    commands.entity(e).insert(ActionClip::new(None, &[HOLD]));
                }
            }
            Part::Walk => {
                // What it's worth: fun and company for both, the little one's energy, liking.
                if hours > 0.0 {
                    m.add(crate::sim::FUN, PUSHER_FUN * hours);
                    m.add(crate::sim::SOCIAL, PUSHER_SOCIAL * hours);
                    lm.add(crate::sim::SOCIAL, LITTLE_SOCIAL * hours);
                    lm.add(crate::sim::FUN, LITTLE_FUN * hours);
                    lm.add(crate::sim::ENERGY, LITTLE_ENERGY * hours);
                    let l = LIKING * hours;
                    commands.queue(move |w: &mut World| {
                        for (a, b) in [(e, child), (child, e)] {
                            if let Some(mut r) = w.get_mut::<crate::social::Relationships>(a) {
                                r.add(b, l, 0.0);
                            }
                        }
                    });
                }
                if !s.good && clock.minutes - s.since >= GOOD_STROLL_AFTER {
                    s.good = true;
                    moods.add(crate::life::MoodletKind::HadAGoodStroll, clock.minutes);
                    info!("strollers: {} has had a good stroll", little.first);
                }
                // (The little one content or fretting in it.)
                let want = idles(baby, crate::carry::crying(&lm));
                if lclip.is_none_or(|c| c.loops != want) {
                    commands.entity(child).insert(ActionClip::new(None, want));
                }
                // (Time's up, or a need of theirs pressing: home.)
                let needy = [crate::sim::HUNGER, crate::sim::BLADDER, crate::sim::ENERGY].iter().any(|i| m.0[*i] < -50.0);
                let told = s.told || q.0.len() > 1;
                if !s.returning && (clock.minutes - s.since >= STROLL_MINUTES || needy || told) {
                    s.returning = true;
                    s.leg = 0;
                    commands.entity(e).remove::<PathFollow>();
                    info!("strollers: {} heads home", sim.first);
                    continue;
                }
                if path.is_some_and(|p| !p.done) {
                    continue;
                }
                let here = tf.translation.xz();
                let (home_at, home_face) = behind(&s.home);
                let g = grid.as_deref();
                let route = |from: Vec2, to: Vec2| g.and_then(|g| g.find_path(from, to)).unwrap_or_else(|| vec![to]);
                let mut way: Vec<Vec2> = Vec::new();
                if s.returning {
                    match s.leg {
                        // (Home through the lot; from the street, in at the lot's edge first.)
                        0 => {
                            s.leg = 1;
                            way = match (g.and_then(|g| g.find_path(here, home_at)), exit.as_deref()) {
                                (Some(w), _) => w,
                                (None, Some(x)) => std::iter::once(x.0).chain(route(x.0, home_at)).collect(),
                                (None, None) => vec![home_at],
                            };
                        }
                        _ => {
                            // Home: turned to where the stroller stood, the little one taken out.
                            tf.translation = Vec3::new(home_at.x, tf.translation.y, home_at.y);
                            tf.rotation = Quat::from_rotation_y(home_face.x.atan2(home_face.y));
                            commands.entity(e).remove::<PathFollow>();
                            s.part = Part::Out(0);
                            s.length = 0.0;
                            continue;
                        }
                    }
                } else {
                    match (exit.as_deref(), sidewalk.as_deref()) {
                        // Out to the street, then up and down its sidewalk.
                        (Some(x), Some(walk)) => {
                            s.leg += 1;
                            if s.leg == 1 {
                                way = route(here, x.0);
                                let onto = walk.center + walk.along * (x.0 - walk.center).dot(walk.along);
                                way.push(onto);
                            }
                            let end = walk.center + walk.along * walk.half_length * 0.85 * if s.leg % 2 == 0 { -1.0 } else { 1.0 };
                            way.push(end);
                        }
                        // (Away from home: about the lot.)
                        _ => {
                            s.leg += 1;
                            let to = (0..16).find_map(|_| {
                                let p = here + Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU)) * rng.random_range(8.0..20.0);
                                g.map_or(Some(vec![p]), |g| g.find_path(here, p))
                            });
                            way = to.unwrap_or_default();
                        }
                    }
                }
                if way.is_empty() {
                    continue;
                }
                let wp: Vec<Waypoint> = way.into_iter().map(|p| Waypoint { p, level: 1, climb: None, spiral: None }).collect();
                commands.entity(e).insert(PathFollow::new(wp).with_style(WalkStyle::Stroller));
            }
        }
    }
}

/// A stroll over: the stroller back where it stood, at rest; the little one in the arms again.
fn finish(commands: &mut Commands, e: Entity, s: &Strolling, strollers: &mut Query<&mut Transform, (With<GameObject>, Without<Sim>)>) {
    if let Ok(mut stf) = strollers.get_mut(s.stroller) {
        *stf = s.home;
    }
    commands.entity(s.stroller).remove::<crate::objanim::PlayingClip>();
    commands.entity(e).remove::<(Strolling, ActionClip, PathFollow)>();
    commands.entity(s.child).remove::<(InStroller, ActionClip)>().insert(Visibility::Inherited);
}

/// The stroller before the one pushing it, its wheels going round in step with them; the little
/// one in it (or handed in and out, as the part has it).
#[allow(clippy::type_complexity)]
fn place(
    mut commands: Commands,
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    pushers: Query<(&Transform, &Strolling, &ClipPlayer, &crate::simbody::Skeleton, &SimAnim), (With<Sim>, Without<InStroller>)>,
    mut strollers: Query<(&mut Transform, &crate::objanim::ObjectRig), (With<GameObject>, Without<Sim>)>,
    mut littles: Query<(&mut Transform, &mut Visibility), (With<InStroller>, Without<GameObject>, Without<Strolling>)>,
    joints: Query<&GlobalTransform>,
) {
    for (ptf, s, player, skel, anim) in &pushers {
        let Ok((mut stf, rig)) = strollers.get_mut(s.stroller) else { continue };
        *stf = pushed(ptf);
        // (Walking: the stroller's half of the step, in time with theirs.)
        let walking = anim.pose == Pose::Walk && player.name.eq_ignore_ascii_case(WALK);
        let clip = walking.then(|| lib.get(&data, STROLLER_WALK)).flatten();
        let t = clip.as_ref().map_or(0.0, |c| if c.duration > 0.0 { player.time.rem_euclid(c.duration) } else { 0.0 });
        if walking {
            commands.entity(s.stroller).insert(crate::objanim::PlayingClip { name: STROLLER_WALK, time: player.time, looping: true });
        } else {
            commands.entity(s.stroller).remove::<crate::objanim::PlayingClip>();
        }
        let Ok((mut ltf, mut vis)) = littles.get_mut(s.child) else { continue };
        let (hold, hidden) = hold_at(s.part, s.baby, s.t);
        *vis = if hidden { Visibility::Hidden } else { Visibility::Inherited };
        match hold {
            Hold::Seat => {
                let bone = if s.baby { SEAT_BABY } else { SEAT_TODDLER };
                if let Some(m) = rig.bone_in_model(clip.as_deref(), t, bone) {
                    let w = stf.to_matrix() * m;
                    let (_, r, p) = w.to_scale_rotation_translation();
                    ltf.translation = p;
                    ltf.rotation = r;
                }
            }
            Hold::Jig => {
                ltf.translation = ptf.transform_point(Vec3::new(0.0, 0.0, TODDLER_JIG));
                ltf.rotation = ptf.rotation * Quat::from_rotation_y(std::f32::consts::PI);
            }
            Hold::Hand => {
                if let Some(hand) = skel.rig.bones.iter().position(|b| b.name == HAND).and_then(|i| joints.get(*skel.joints.get(i)?).ok()) {
                    let (_, r, p) = hand.to_scale_rotation_translation();
                    ltf.translation = p;
                    ltf.rotation = r;
                }
            }
        }
    }
}

/// Now and then by day, a teen or grown-up of the household short of fun with nothing to do
/// takes an awake little one of theirs out in the stroller of their own accord (`GoOnStroll`'s
/// advertising: Fun, Social).
#[allow(clippy::type_complexity)]
fn autonomy(
    clock: Res<crate::clock::GameClock>,
    mut grown: Query<(&Sim, &mut ActionQueue, &crate::sim::Motives, &Transform), (With<crate::sim::HouseholdMember>, Without<crate::visit::OnLot>, Without<Strolling>)>,
    littles: Query<(Entity, &Sim, &crate::sim::Motives), (With<crate::sim::HouseholdMember>, Without<InStroller>)>,
    strollers: Query<(Entity, &GameObject, &Transform), (Without<crate::visit::LotObject>, Without<Sim>)>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 90.0;
    if std::env::var_os("STROLLER_TEST").is_some() || !(8.0..18.0).contains(&clock.hour_f()) {
        return;
    }
    let mut rng = rand::rng();
    let Some((st, _, stf)) = strollers.iter().find(|s| s.1.kind == ObjectKind::Stroller) else { return };
    let awake: Vec<Entity> = littles.iter().filter(|l| l.1.age.is_little() && l.2.0[crate::sim::ENERGY] > 0.0).map(|l| l.0).collect();
    let Some(&child) = awake.first() else { return };
    if grown.iter().any(|g| g.1.0.iter().any(|a| matches!(a.kind, ActionKind::Stroll { .. }))) {
        return;
    }
    let (at, face) = behind(stf);
    let pick = grown.iter_mut().filter(|g| can_push(g.0.age) && g.1.0.is_empty() && g.2.0[crate::sim::FUN] < 40.0).min_by(|a, b| a.3.translation.distance(stf.translation).total_cmp(&b.3.translation.distance(stf.translation)));
    if let Some((sim, mut q, ..)) = pick
        && rng.random_bool(0.3)
    {
        q.0.push_back(Action::new(GO_ON_STROLL, ActionKind::Stroll { stroller: st, child, at, face }, true));
        info!("strollers: {} goes for a stroll of their own accord", sim.first);
    }
}

/// `STROLLER_TEST=<instance>` (tests: `strollerSimple`): ten seconds in, that stroller put down
/// before the selected Sim (or `STROLLER_WHO=<first name>`), who takes the household's first
/// toddler or baby (`STROLLER_CHILD=<first name>`) out in it; `STROLLER_MINUTES=<n>` told to stop
/// after that long.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn stroller_test(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Has<crate::sim::Selected>), With<crate::sim::HouseholdMember>>,
    strollers: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32, f64)>,
) {
    let Ok(name) = std::env::var("STROLLER_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let who = std::env::var("STROLLER_WHO").ok();
    let is_pusher = |s: &Sim, sel: bool| who.as_ref().map_or(sel && can_push(s.age), |w| s.first.eq_ignore_ascii_case(w));
    if let (Ok(mut c), Some((_, _, tf, ..))) = (cams.single_mut(), sims.iter().find(|s| is_pusher(s.1, s.4))) {
        let env = |k: &str| std::env::var(k).ok().and_then(|d| d.parse::<f32>().ok());
        c.look_at(tf.translation + Vec3::Y * 0.6);
        c.distance = env("STROLLER_DIST").unwrap_or(6.0);
        c.pitch = 0.45;
        // (STROLLER_YAW=<radians>: from behind them, turned so far.)
        if let Some(y) = env("STROLLER_YAW") {
            let f = (tf.rotation * Vec3::Z).xz();
            c.yaw = f.x.atan2(f.y) + y;
        }
        // (STROLLER_LOOK=1: at the stroller instead.)
        if std::env::var_os("STROLLER_LOOK").is_some()
            && let Some((_, _, stf)) = strollers.iter().find(|s| s.1.kind == ObjectKind::Stroller)
        {
            c.look_at(stf.translation + Vec3::Y * 0.4);
        }
    }
    match stage.0 {
        0 => {
            *stage = (1, now, clock.minutes);
            let Some((_, _, tf, ..)) = sims.iter().find(|s| is_pusher(s.1, s.4)) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&name)).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = tf.translation.xz();
            let spot = (2..12).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32)).find(|c| (-3..=3).all(|i| (-3..=3).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.5)))).unwrap_or(here + Vec2::Y * 2.0);
            let p = Vec3::new(spot.x, tf.translation.y, spot.y);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("stroller test: a stroller at {p:.1}");
            }
        }
        1 if now - stage.1 > 3.0 => {
            let Some((st, _, stf)) = strollers.iter().find(|s| s.1.kind == ObjectKind::Stroller) else { return };
            // (STROLLER_CHILD=@baby: a baby, once there is one: `--do Baby` brings one.)
            let want = std::env::var("STROLLER_CHILD").ok();
            let fits = |s: &Sim| match want.as_deref() {
                Some("@baby") => s.age == Age::Baby,
                Some(w) => s.first.eq_ignore_ascii_case(w),
                None => s.age.is_little(),
            };
            let Some((child, cname)) = sims.iter().find(|s| fits(s.1)).map(|s| (s.0, s.1.first.clone())) else { return };
            *stage = (2, now, clock.minutes);
            let (at, face) = behind(stf);
            if let Some((_, sim, _, mut q, _)) = sims.iter_mut().find(|s| is_pusher(s.1, s.4)) {
                q.push_player(Action::new(GO_ON_STROLL, ActionKind::Stroll { stroller: st, child, at, face }, false));
                info!("stroller test: {} goes on a stroll with {cname}", sim.first);
            }
        }
        2 => {
            let Some(m) = std::env::var("STROLLER_MINUTES").ok().and_then(|v| v.parse::<f64>().ok()) else { return };
            if clock.minutes - stage.2 < m {
                return;
            }
            *stage = (3, now, clock.minutes);
            if let Some((_, sim, _, mut q, _)) = sims.iter_mut().find(|s| is_pusher(s.1, s.4))
                && let Some(a) = q.0.iter_mut().find(|a| matches!(a.kind, ActionKind::Stroll { .. }))
            {
                a.cancel = true;
                info!("stroller test: {} told to stop", sim.first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stroller_goes_before_the_one_pushing_it() {
        let stroller = Transform::from_xyz(3.0, 0.0, 4.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let (at, face) = behind(&stroller);
        assert!((at - Vec2::new(3.0 - BEHIND, 4.0)).length() < 1e-4, "behind its handle: {at}");
        assert!((face - Vec2::X).length() < 1e-4);
        let pusher = Transform::from_xyz(at.x, 0.0, at.y).with_rotation(stroller.rotation);
        assert!((pushed(&pusher).translation - stroller.translation).length() < 1e-4, "where it stood, from there");
    }

    #[test]
    fn the_little_one_is_handed_in_as_the_clips_have_it() {
        assert_eq!(hold_at(Part::In(0), true, 0.2), (Hold::Hand, false));
        assert_eq!(hold_at(Part::In(0), true, 0.6), (Hold::Hand, true), "faded out as they're handed over");
        assert_eq!(hold_at(Part::In(0), true, 1.3), (Hold::Seat, false));
        assert_eq!(hold_at(Part::Out(0), true, 0.7), (Hold::Hand, false));
        assert_eq!(hold_at(Part::In(1), false, 0.2), (Hold::Jig, false));
        assert_eq!(part_clips(Part::In(1), false).unwrap().1, "a2p_stroller_transition_holdTodder2stroller_pt2_y");
        assert!(part_clips(Part::In(1), true).is_none(), "a baby's goes in in one");
        assert_eq!(idles(true, true), ["b2o_Stroller_unhappy_idle"]);
    }
}
