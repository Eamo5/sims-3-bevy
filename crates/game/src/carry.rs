//! Babies and toddlers picked up, carried about and put down, as the game's (`PickUpChild`,
//! `CarryingChildPosture`, `PutDownChild`, `PutDownChildHere`, the crib's `Hold` and
//! `PutCarriedChildInCrib`): a teen or grown-up lifts a little one from the floor or out of its
//! crib, walks about with them in their arms, and puts them down again: where they stand, on a
//! spot picked out, or in a crib. Anything that needs their hands has them put the little one
//! down first (a baby into an empty crib if there's one, as the game's own put-downs choose),
//! and a baby left lying on the floor is put in a crib by whoever of the family's free.
//!
//! A baby rides the carrier's right hand (`b__R_Hand_slot`, where the game's clips parent it);
//! a toddler's clips are made with it 0.6 m before the carrier, facing them.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use bevy::prelude::*;

use crate::anim::{ActionClip, Carrying, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase, UsedBy};
use crate::little::{Bedtime, Carried, InCrib, OnFloor, CRIB_HEIGHT};
use crate::nav::{Floor, PathFollow};
use crate::sim::{Age, HouseholdMember, Pose, Sim, SimAnim};
use crate::PlayMode;

pub struct CarryPlugin;

impl Plugin for CarryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (care_in_arms, put_down_first, hold_still).chain().before(crate::interact::run_actions).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (carry_moves, held_idle, tidy_babies, carry_test).chain().after(crate::interact::run_actions).run_if(in_state(PlayMode::Live)))
            .add_systems(PostUpdate, follow.after(bevy::transform::TransformSystems::Propagate));
    }
}

/// What's done with a little one: picked up (where they are, or out of a crib), put down
/// (where the carrier stands, or at a spot picked out), or put in a crib.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CarryStep {
    PickUp { crib: Option<Entity> },
    PutDown(Option<Vec2>),
    InCrib(Entity),
}

/// A teen or grown-up with a little one in their arms.
#[derive(Component, Clone, Copy)]
pub struct Holding(pub Entity);

/// A little one being picked up or put down just now: the clips have them.
#[derive(Component)]
pub struct BeingMoved;

/// A pick-up or put-down under way: its clips, how far along (game minutes, as the clips'
/// seconds), and when the little one changes hands (is taken up, or let go).
#[derive(Component)]
pub struct CarryMove {
    child: Entity,
    step: CarryStep,
    t: f32,
    /// When the little one's taken into the arms or let go of.
    at: f32,
    end: f32,
    begun: bool,
    switched: bool,
    /// The little one's clip (to set them down where it leaves them).
    clip: &'static str,
}

impl CarryMove {
    pub fn new(child: Entity, step: CarryStep) -> Self {
        Self { child, step, t: 0.0, at: 0.0, end: 0.0, begun: false, switched: false, clip: "" }
    }
}

/// How far before a grown-up a little one's picked up from or put down at (the game's jigs: a
/// baby's 0.7 m, a toddler's 0.6).
pub fn apart(age: Age) -> f32 {
    if age == Age::Baby { 0.7 } else { 0.6 }
}

/// Where to stand to reach a spot from where one is: `apart` from it, on this side.
pub fn beside(spot: Vec2, from: Vec2, apart: f32) -> Vec2 {
    spot + (from - spot).normalize_or(Vec2::X) * apart
}

/// Who can pick a little one up: teens and older (the game's `ChildOrBelow` can't).
pub fn can_carry(age: Age) -> bool {
    !age.is_little() && age != Age::Child
}

/// A step's words on the menu (the game's `PickUp`, `PutDown`, `PutDownHere`, `PutInCrib`,
/// `PutToBed`).
pub fn label(step: CarryStep, first: &str, tired: bool) -> String {
    match step {
        CarryStep::PickUp { .. } => format!("Pick Up {first}"),
        CarryStep::PutDown(None) => format!("Put Down {first}"),
        CarryStep::PutDown(Some(_)) => format!("Put Down {first} Here"),
        CarryStep::InCrib(_) if tired => format!("Put {first} to Bed"),
        CarryStep::InCrib(_) => format!("Put {first} in Crib"),
    }
}

/// A baby's pick-up from the floor, as the game's `PickUpChild` picks it: by the way the baby
/// lies to the spot it's picked up at (facing out of it: `slot_facing`), in quarters (none:
/// lying as it's put down there), and how far round the one picking it up turns from where
/// they face to line the clip up with it.
pub fn pick_up_variant(slot_facing: Vec2, baby_facing: Vec2) -> (&'static str, f32) {
    let (f, b) = (slot_facing, baby_facing);
    let x = f.x * b.x + f.y * b.y;
    let y = f.x * b.y - f.y * b.x;
    let theta = y.atan2(x);
    let q = 2.0 * (theta - FRAC_PI_4).rem_euclid(2.0 * PI) / PI;
    let (v, off) = if q < 1.0 {
        ("_270", FRAC_PI_2)
    } else if q < 2.0 {
        ("_180", PI)
    } else if q < 3.0 {
        ("_90", -FRAC_PI_2)
    } else {
        ("", 0.0)
    };
    // (Within half a turn either way: the same turn as one a whole turn round.)
    let rest = (theta - off + PI).rem_euclid(2.0 * PI) - PI;
    (v, rest)
}

/// A flat direction turned (from x towards z).
fn turned(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Facing a flat direction (the Sims' +z forward).
fn facing(d: Vec2) -> Quat {
    Quat::from_rotation_y(d.x.atan2(d.y))
}

/// Where a flat Transform faces.
fn forward(q: Quat) -> Vec2 {
    (q * Vec3::Z).xz().normalize_or(Vec2::Y)
}

/// The clips of a step: the carrier's, the little one's, and when the little one changes hands
/// (the clips' own parenting events: picked up at 1.267 s, out of a crib at 1.5; put down at
/// 2.667, in a crib at 3.667; toddlers by the script, at the start or the end).
fn clips(step: CarryStep, age: Age, variant: &str) -> (&'static str, &'static str, Option<f32>) {
    let baby = age == Age::Baby;
    match (step, baby) {
        (CarryStep::PickUp { crib: Some(_) }, true) => ("a2b_crib_pullOut_x", "a2b_crib_pullOut_y", Some(1.5)),
        (CarryStep::PickUp { crib: Some(_) }, false) => ("a2p_crib_pullOut_x", "a2p_crib_pullOut_y", None),
        (CarryStep::PickUp { crib: None }, true) => match variant {
            "_90" => ("a2b_pickUp_90_x", "a2b_pickUp_90_y", Some(1.267)),
            "_180" => ("a2b_pickUp_180_x", "a2b_pickUp_180_y", Some(1.267)),
            "_270" => ("a2b_pickUp_270_x", "a2b_pickUp_270_y", Some(1.267)),
            _ => ("a2b_pickUp_x", "a2b_pickUp_y", Some(1.267)),
        },
        (CarryStep::PickUp { crib: None }, false) => ("a2p_pickUp_x", "a2p_pickUp_y", Some(0.0)),
        (CarryStep::PutDown(_), true) => ("a2b_putDown_x", "a2b_putDown_y", Some(2.667)),
        (CarryStep::PutDown(_), false) => ("a2p_putDown_x", "a2p_putDown_y", None),
        (CarryStep::InCrib(_), true) => ("a2b_crib_putIn_x", "a2b_crib_putIn_y", Some(3.667)),
        (CarryStep::InCrib(_), false) => ("a2p_crib_putIn_x", "a2p_crib_putIn_y", Some(0.0)),
    }
}

/// A held little one's loop: breathing in the arms, or crying there.
fn held_clip(age: Age, crying: bool) -> ActionClip {
    match (age == Age::Baby, crying) {
        (true, false) => ActionClip::new(None, &["a2b_idle_carry_breathe_y"]),
        (true, true) => ActionClip::new(Some("b_motDistress_cry_carryStart_y"), &["b_motDistress_cry_carryLoop_y"]),
        (false, false) => ActionClip::new(None, &["a2p_idle_carry_breathe_y"]),
        (false, true) => ActionClip::new(Some("p_motDistress_cry_carryStart_y"), &["p_motDistress_cry_carryLoop_y"]),
    }
}

/// The carrier's arms round them, over whatever else they're doing (the game's `CarryPose`).
fn carry_pose(age: Age) -> Carrying {
    Carrying(if age == Age::Baby { "a2b_carryPose_x" } else { "a2p_carryPose_x" })
}

/// Whether a little one wants something badly enough to cry for it (as `little`'s).
fn crying(m: &crate::sim::Motives) -> bool {
    use crate::sim::{BLADDER, HUNGER, SOCIAL};
    [HUNGER, BLADDER, SOCIAL].iter().any(|k| m.0[*k] < -40.0)
}

/// Whether a carrier can go on with what's next with the little one in their arms (the game's
/// `MayCarryChild`): walking about, the little one's own care that's done in the arms, and these
/// steps; anything else (their care on the floor too: teaching them to walk, reading to them)
/// has them put down first.
fn with_child(kind: &ActionKind, child: Entity, age: Age) -> bool {
    match kind {
        ActionKind::GoHere(..) | ActionKind::Outro { .. } | ActionKind::WakeUp { .. } => true,
        ActionKind::Social { target, social } => *target == child && in_arms(crate::social::social(*social).name, age),
        ActionKind::Carry { child: c, step, .. } => *c == child && !matches!(step, CarryStep::PickUp { .. }),
        _ => false,
    }
}

/// A crib no little one's in (or being put in), nearest a place.
pub fn free_crib(at: Vec3, cribs: impl Iterator<Item = (Entity, Vec3)>, taken: &[Vec3], within: f32) -> Option<Entity> {
    cribs
        .filter(|(_, p)| p.distance(at) < within && !taken.iter().any(|t| t.xz().distance(p.xz()) < 0.3))
        .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
        .map(|(e, _)| e)
}

/// How far off a crib can be for a baby to be put in it rather than down (about the lot).
const CRIB_REACH: f32 = 40.0;

/// Where the cribs are taken: the babies in them, the toddlers napping in them, and those being
/// put in them.
fn cribs_taken(littles: &Query<(Entity, &Sim, &Transform, Option<&InCrib>, Has<Bedtime>, Has<Carried>), Without<CarryMove>>, moves: &[CarryStep], cribs: &Query<(Entity, &GameObject, &Transform), Without<Sim>>) -> Vec<Vec3> {
    let mut taken: Vec<Vec3> = littles.iter().filter(|l| !l.5).filter_map(|l| l.3.map(|c| c.0).or(l.4.then_some(l.2.translation))).collect();
    taken.extend(moves.iter().filter_map(|s| if let CarryStep::InCrib(c) = s { cribs.get(*c).ok().map(|x| x.2.translation) } else { None }));
    taken
}

/// Before anything that needs their hands, the little one's put down: a baby in the nearest
/// empty crib, if there's one about, else on the floor where they stand (the game's
/// `CarryingChildPosture.GetStandingTransition`).
#[allow(clippy::type_complexity)]
fn put_down_first(
    mut commands: Commands,
    mut carriers: Query<(Entity, &Holding, &mut ActionQueue, &Transform, Has<HouseholdMember>), (Without<CarryMove>, With<Sim>, Without<Carried>)>,
    littles: Query<(Entity, &Sim, &Transform, Option<&InCrib>, Has<Bedtime>, Has<Carried>), Without<CarryMove>>,
    cribs: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    moving: Query<&CarryMove>,
) {
    for (me, holding, mut queue, tf, member) in &mut carriers {
        let child = holding.0;
        let Some(front) = queue.0.front() else { continue };
        let Ok((_, sim, ..)) = littles.get(child) else { continue };
        // (Someone not of the family — the babysitter — walks nowhere with them: off home,
        // they'd take them along.)
        let walk_off = !member && matches!(front.kind, ActionKind::GoHere(..));
        if front.cancel || !matches!(front.phase, Phase::Start | Phase::Routing) || (with_child(&front.kind, child, sim.age) && !walk_off) {
            continue;
        }
        // (Set out for it already, this frame: stopped, to put the little one down first.)
        if matches!(front.phase, Phase::Routing) {
            commands.entity(me).remove::<PathFollow>();
            if let Some(a) = queue.0.front_mut() {
                a.phase = Phase::Start;
            }
        }
        let steps: Vec<CarryStep> = moving.iter().map(|m| m.step).collect();
        let taken = cribs_taken(&littles, &steps, &cribs);
        let crib = (sim.age == Age::Baby)
            .then(|| free_crib(tf.translation, cribs.iter().filter(|c| c.1.kind == ObjectKind::Crib).map(|c| (c.0, c.2.translation)), &taken, CRIB_REACH))
            .flatten();
        let step = crib.map_or(CarryStep::PutDown(None), CarryStep::InCrib);
        debug!("carry: {} put down first ({step:?})", sim.first);
        queue.0.push_front(Action::new(label(step, &sim.first, false), ActionKind::Carry { child, step, secs: f32::INFINITY }, true));
    }
}

/// The care socials done with the little one in the carrier's arms (the game's `CarryingChild`
/// ones: GiveBottle, ChangeDiaper, Snuggle, PlayWith, TossInAir, Tickle); a toddler's fed on
/// the floor or in a high chair.
fn in_arms(name: &str, age: Age) -> bool {
    match name {
        "Feed" => age == Age::Baby,
        "Change Diaper" | "Cuddle" | "Play With" => true,
        _ => false,
    }
}

/// A little one's care that's done in the arms has them picked up first (out of the crib, if
/// they're in one), and they're held still after it, till something needs the carrier's hands;
/// held already, there's no walk to them. "Put to Bed", for one not in a crib, is a carry to an
/// empty one.
#[allow(clippy::type_complexity)]
fn care_in_arms(
    mut commands: Commands,
    mut carriers: Query<(Entity, &Sim, &mut ActionQueue, Option<&Holding>, Has<PathFollow>, &Transform), (Without<CarryMove>, Without<Carried>)>,
    littles: Query<(Entity, &Sim, &Transform, Option<&InCrib>, Has<Bedtime>, Has<Carried>), Without<CarryMove>>,
    states: Query<(Option<&Carried>, Has<BeingMoved>, Has<OnFloor>)>,
    cribs: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    moving: Query<&CarryMove>,
) {
    for (me, sim, mut queue, holding, walking, tf) in &mut carriers {
        let Some(front) = queue.0.front() else { continue };
        let ActionKind::Social { target, social } = front.kind else { continue };
        if front.cancel || !matches!(front.phase, Phase::Start | Phase::Routing) || !can_carry(sim.age) {
            continue;
        }
        let routing = matches!(front.phase, Phase::Routing);
        let Ok((_, little, ltf, in_crib, bedtime, _)) = littles.get(target) else { continue };
        let Ok((carried, moved, on_floor)) = states.get(target) else { continue };
        if !little.age.is_little() || moved || carried.is_some_and(|c| c.by != me) {
            continue;
        }
        let name = crate::social::social(social).name;
        let held = holding.is_some_and(|h| h.0 == target);
        let autonomous = front.autonomous;
        let crib_of = || {
            let in_one = in_crib.is_some() || (little.age == Age::Toddler && bedtime);
            let at = in_crib.map_or(ltf.translation, |c| c.0);
            in_one.then(|| cribs.iter().filter(|c| c.1.kind == ObjectKind::Crib && c.2.translation.xz().distance(at.xz()) < 0.5).map(|c| c.0).next()).flatten()
        };
        // (Set out for them already, this frame: stopped, to pick them up first.)
        let mut restart = |queue: &mut ActionQueue| {
            if routing {
                commands.entity(me).remove::<PathFollow>();
                if let Some(a) = queue.0.front_mut() {
                    a.phase = Phase::Start;
                }
            }
        };
        // (Put to bed: carried to an empty crib, when they're not in one.)
        if name == "Put to Bed" && crib_of().is_none() && (held || on_floor || little.age == Age::Toddler) {
            let steps: Vec<CarryStep> = moving.iter().map(|m| m.step).collect();
            let taken = cribs_taken(&littles, &steps, &cribs);
            let Some(crib) = free_crib(ltf.translation, cribs.iter().filter(|c| c.1.kind == ObjectKind::Crib).map(|c| (c.0, c.2.translation)), &taken, CRIB_REACH) else { continue };
            restart(&mut queue);
            queue.0.pop_front();
            let down = CarryStep::InCrib(crib);
            queue.0.push_front(Action::new(label(down, &little.first, true), ActionKind::Carry { child: target, step: down, secs: f32::INFINITY }, autonomous));
            if !held {
                let up = CarryStep::PickUp { crib: None };
                queue.0.push_front(Action::new(label(up, &little.first, false), ActionKind::Carry { child: target, step: up, secs: f32::INFINITY }, autonomous));
            }
            continue;
        }
        if !in_arms(name, little.age) {
            continue;
        }
        if held {
            // (No walk to the one in their arms: it's begun where they stand.)
            if !walking && !routing {
                let mut path = PathFollow::new(Vec::new());
                path.done = true;
                commands.entity(me).insert(path);
                if let Some(a) = queue.0.front_mut() {
                    a.phase = Phase::Routing;
                }
            }
            let _ = tf;
            continue;
        }
        let up = CarryStep::PickUp { crib: crib_of() };
        restart(&mut queue);
        debug!("carry: {} picks up {} for {name}", sim.first, little.first);
        queue.0.push_front(Action::new(label(up, &little.first, false), ActionKind::Carry { child: target, step: up, secs: f32::INFINITY }, autonomous));
    }
}

/// A toddler about to be picked up waits for it (the game's "Be Picked Up").
fn hold_still(mut commands: Commands, mut sims: Query<(&Sim, &mut ActionQueue)>) {
    let waiting: Vec<Entity> = sims
        .iter()
        .filter_map(|(_, q)| match q.0.front().map(|a| &a.kind) {
            Some(ActionKind::Carry { child, step: CarryStep::PickUp { crib: None }, .. }) => Some(*child),
            _ => None,
        })
        .collect();
    for child in waiting {
        if let Ok((sim, mut theirs)) = sims.get_mut(child)
            && sim.age == Age::Toddler
            && !theirs.0.is_empty()
        {
            theirs.0.clear();
            commands.entity(child).remove::<PathFollow>();
        }
    }
}

/// Picks-up and puts-down played out: the clips on both, the little one taken into the arms or
/// let go of when the clips have it, and set down where they leave it.
#[allow(clippy::type_complexity)]
fn carry_moves(
    mut commands: Commands,
    data: Option<Res<Baked>>,
    mut lib: ResMut<ClipLibrary>,
    delta: Res<crate::clock::SimDelta>,
    mut carriers: Query<(Entity, &mut CarryMove, &mut ActionQueue, &mut Transform, &mut SimAnim, &Floor), With<Sim>>,
    mut littles: Query<(&Sim, &mut Transform, &mut SimAnim, &crate::simbody::Skeleton, &mut Floor), Without<CarryMove>>,
    mut cribs: Query<(&GameObject, &Transform, &mut UsedBy), Without<Sim>>,
    joints: Query<&GlobalTransform>,
) {
    let Some(data) = data else { return };
    for (me, mut mv, mut queue, mut tf, mut anim, floor) in &mut carriers {
        let child = mv.child;
        let ours = queue.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Carry { child: c, step, .. } if c == child && step == mv.step) && !a.cancel);
        let Ok((sim, mut ctf, mut canim, skel, mut cfloor)) = littles.get_mut(child) else {
            commands.entity(me).remove::<CarryMove>();
            end_secs(&mut queue, child, 0.0);
            continue;
        };
        let age = sim.age;
        let crib_of = |step: CarryStep| match step {
            CarryStep::PickUp { crib } => crib,
            CarryStep::InCrib(c) => Some(c),
            CarryStep::PutDown(_) => None,
        };
        if !mv.begun {
            if !ours {
                commands.entity(me).remove::<CarryMove>();
                continue;
            }
            mv.begun = true;
            let mine = tf.translation.xz();
            let mut variant = "";
            match mv.step {
                CarryStep::PickUp { crib: None } if age == Age::Baby => {
                    // (Lined up with the baby: the quarter it lies in, and the rest turned.)
                    let to = (ctf.translation.xz() - mine).normalize_or(forward(tf.rotation));
                    let (v, rest) = pick_up_variant(-to, forward(ctf.rotation));
                    variant = v;
                    let fwd = -turned(-to, rest);
                    tf.rotation = facing(fwd);
                    let at = ctf.translation.xz() - fwd * apart(age);
                    tf.translation = Vec3::new(at.x, tf.translation.y, at.y);
                }
                CarryStep::PickUp { crib: None } => {
                    // (Face to face, the toddler where its clips have it.)
                    let to = (ctf.translation.xz() - mine).normalize_or(forward(tf.rotation));
                    tf.rotation = facing(to);
                }
                CarryStep::PickUp { crib: Some(c) } | CarryStep::InCrib(c) => {
                    if let Ok((o, otf, mut used)) = cribs.get_mut(c) {
                        let middle = otf.transform_point(Vec3::new(o.center.x, 0.0, o.center.y)).xz();
                        let to = (middle - mine).normalize_or(forward(tf.rotation));
                        tf.rotation = facing(to);
                        used.0 = Some(me);
                    }
                }
                CarryStep::PutDown(Some(at)) => {
                    let to = (at - mine).normalize_or(forward(tf.rotation));
                    tf.rotation = facing(to);
                }
                CarryStep::PutDown(None) => {}
            }
            let (x, y, at) = clips(mv.step, age, variant);
            let mut dur = |n: &str| lib.get(&data, n).map_or(3.0, |c| c.duration);
            mv.end = dur(x).max(dur(y));
            mv.at = at.unwrap_or(mv.end);
            mv.clip = y;
            end_secs(&mut queue, child, mv.end);
            anim.pose = Pose::Use;
            canim.pose = Pose::Stand;
            commands.entity(me).remove::<Carrying>().insert(ActionClip::new(Some(x), &[]));
            commands.entity(child).insert((ActionClip::new(Some(y), &[]), BeingMoved));
            debug!("carry: {} {:?} ({x}, {y})", sim.first, mv.step);
            // (A toddler's own clips have them before the carrier from the start; in the crib,
            // its from the start too.)
            if age == Age::Toddler {
                match mv.step {
                    CarryStep::PickUp { crib: None } => {
                        commands.entity(child).insert(Carried { by: me });
                        commands.entity(me).insert(Holding(child));
                    }
                    CarryStep::InCrib(c) => {
                        commands.entity(child).remove::<Carried>();
                        commands.entity(me).remove::<Holding>();
                        if let Ok((_, otf, _)) = cribs.get(c) {
                            ctf.translation = otf.translation + Vec3::Y * CRIB_HEIGHT;
                            ctf.rotation = otf.rotation;
                        }
                    }
                    _ => {}
                }
            }
        }
        let dt = delta.0;
        mv.t += dt;
        let settle = !ours || mv.t >= mv.end;
        // The little one changes hands.
        if !mv.switched && (mv.t >= mv.at || (settle && ours)) {
            mv.switched = true;
            match mv.step {
                CarryStep::PickUp { .. } => {
                    commands.entity(child).remove::<(InCrib, OnFloor, Bedtime)>().insert(Carried { by: me });
                    commands.entity(me).insert(Holding(child));
                    // (For the wishes: the game's `kPickedUpSim`.)
                    commands.queue(move |w: &mut World| {
                        w.write_message(crate::dreams::DreamEvent { other: Some(child), ..crate::dreams::DreamEvent::new(me, "kPickedUpSim") });
                    });
                }
                CarryStep::PutDown(_) => {
                    commands.entity(child).remove::<Carried>();
                    commands.entity(me).remove::<Holding>();
                    if age == Age::Baby {
                        // (Laid where the clip leaves them: its place for them from here on,
                        // upright, on the floor the carrier's on.)
                        let tb = skel.rig.bones.iter().position(|b| b.name == "transformBone").and_then(|i| joints.get(*skel.joints.get(i)?).ok()).map(|g| g.compute_transform());
                        let local = lib.get(&data, mv.clip).and_then(|c| c.tracks.get(&s3pkg::fnv32("transformBone")).map(|t| {
                            let p = crate::anim::sample_track_vec(&t.translation, mv.at + 0.01).unwrap_or(Vec3::ZERO);
                            let r = crate::anim::sample_track_quat(&t.rotation, mv.at + 0.01).unwrap_or(Quat::IDENTITY);
                            Transform { translation: p, rotation: r, scale: Vec3::ONE }
                        }));
                        let fallback = {
                            let f = forward(tf.rotation);
                            let p = tf.translation.xz() + f * apart(age);
                            Transform::from_translation(Vec3::new(p.x, tf.translation.y, p.y)).with_rotation(facing(-f))
                        };
                        let root = match (tb, local) {
                            (Some(w), Some(l)) => {
                                let m = w.to_matrix() * l.to_matrix().inverse();
                                let (_, r, p) = m.to_scale_rotation_translation();
                                Transform::from_translation(Vec3::new(p.x, tf.translation.y, p.z)).with_rotation(facing(forward(r)))
                            }
                            _ => fallback,
                        };
                        // (Never somewhere else altogether: within reach of the carrier.)
                        *ctf = if root.translation.distance(fallback.translation) < 1.0 { root } else { fallback };
                        cfloor.0 = floor.0;
                        commands.entity(child).insert(OnFloor);
                    }
                }
                CarryStep::InCrib(c) => {
                    commands.entity(child).remove::<Carried>();
                    commands.entity(me).remove::<Holding>();
                    if let Ok((_, otf, _)) = cribs.get(c) {
                        ctf.translation = otf.translation + Vec3::Y * CRIB_HEIGHT;
                        ctf.rotation = otf.rotation;
                        commands.entity(child).insert(InCrib(otf.translation, otf.rotation));
                        // (A toddler naps there.)
                        if age == Age::Toddler {
                            commands.entity(child).insert(Bedtime);
                        }
                    }
                    cfloor.0 = floor.0;
                }
            }
        }
        if !settle {
            continue;
        }
        // Done: the little one left to their own clips, the carrier's arms round them if held.
        let held = match mv.step {
            CarryStep::PickUp { .. } => mv.switched,
            _ => !mv.switched,
        };
        // (A toddler let down sits where it was put.)
        if age == Age::Toddler && matches!(mv.step, CarryStep::PutDown(_)) && mv.switched {
            ctf.translation.y = tf.translation.y;
            ctf.rotation = facing(-forward(tf.rotation));
            cfloor.0 = floor.0;
        }
        commands.entity(child).remove::<(BeingMoved, ActionClip)>();
        commands.entity(me).remove::<(CarryMove, ActionClip)>();
        if held {
            commands.entity(me).insert(carry_pose(age));
        }
        if let Some(c) = crib_of(mv.step)
            && let Ok((_, _, mut used)) = cribs.get_mut(c)
            && used.0 == Some(me)
        {
            used.0 = None;
        }
        anim.pose = Pose::Stand;
        canim.pose = Pose::Stand;
        end_secs(&mut queue, child, 0.0);
    }
}

/// The carry at the front of a queue told how long it lasts.
fn end_secs(queue: &mut ActionQueue, child: Entity, to: f32) {
    if let Some(a) = queue.0.front_mut()
        && let ActionKind::Carry { child: c, secs, .. } = &mut a.kind
        && *c == child
    {
        *secs = to;
    }
}

/// A held little one breathes (or cries) in the carrier's arms, and the carrier's arms stay round
/// them; let go of, the carrier's arms are their own again.
#[allow(clippy::type_complexity)]
fn held_idle(
    mut commands: Commands,
    carriers: Query<(Entity, Option<&Holding>, &ActionQueue, Option<&Carrying>, Has<CarryMove>), With<Sim>>,
    littles: Query<(&Sim, &Carried, &crate::sim::Motives, Option<&ActionClip>, Has<BeingMoved>)>,
) {
    for (me, holding, queue, carrying, moving) in &carriers {
        let held = holding.and_then(|h| littles.get(h.0).ok().filter(|l| l.1.by == me).map(|l| (h.0, l)));
        let Some((child, (sim, _, motives, clip, being_moved))) = held else {
            // (Let go: the carry pose off, when it's a little one's.)
            if holding.is_some() {
                commands.entity(me).remove::<Holding>();
            }
            if carrying.is_some_and(|c| c.0 == "a2b_carryPose_x" || c.0 == "a2p_carryPose_x") && !moving {
                commands.entity(me).remove::<Carrying>();
            }
            continue;
        };
        if moving || being_moved {
            continue;
        }
        if carrying.is_none() {
            commands.entity(me).insert(carry_pose(sim.age));
        }
        // (Their own care going on: its clips.)
        let caring = queue.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Social { target, .. } if target == child) && matches!(a.phase, Phase::Running(_)));
        if caring {
            continue;
        }
        let want = held_clip(sim.age, crying(motives));
        if clip.is_none_or(|c| c.loops != want.loops) {
            commands.entity(child).insert(want);
        }
    }
}

/// Held little ones go where the carrier does: a baby in their right hand, a toddler before
/// them, facing them (where the game's clips have them); their floor the carrier's.
#[allow(clippy::type_complexity)]
fn follow(
    mut commands: Commands,
    mut littles: Query<(Entity, &Carried, &Sim, &mut Transform, &mut Floor), Without<CarryMove>>,
    carriers: Query<(&Transform, &crate::simbody::Skeleton, &Floor), (Without<Carried>, With<Sim>)>,
    joints: Query<&GlobalTransform>,
) {
    for (e, c, sim, mut tf, mut floor) in &mut littles {
        let Ok((ctf, skel, cfloor)) = carriers.get(c.by) else {
            commands.entity(e).remove::<Carried>();
            continue;
        };
        floor.0 = cfloor.0;
        if sim.age == Age::Baby {
            let Some(hand) = skel.rig.bones.iter().position(|b| b.name == "b__R_Hand_slot").and_then(|i| joints.get(*skel.joints.get(i)?).ok()) else { continue };
            let (_, r, p) = hand.to_scale_rotation_translation();
            tf.translation = p;
            tf.rotation = r;
        } else {
            let f = forward(ctf.rotation);
            tf.translation = ctf.translation + Vec3::new(f.x, 0.0, f.y) * apart(sim.age);
            tf.rotation = ctf.rotation * Quat::from_rotation_y(PI);
        }
    }
}

/// What the pie menu needs to offer picking up and putting down: who's holding whom, where the
/// little ones are, and the cribs.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CarryLook<'w, 's> {
    holding: Query<'w, 's, &'static Holding>,
    littles: Query<'w, 's, (Entity, &'static Sim, &'static Transform, Option<&'static InCrib>, Has<Bedtime>, Has<Carried>), Without<CarryMove>>,
    carried: Query<'w, 's, (&'static Carried, Has<BeingMoved>, &'static crate::sim::Motives)>,
    cribs: Query<'w, 's, (Entity, &'static GameObject, &'static Transform), Without<Sim>>,
    moving: Query<'w, 's, &'static CarryMove>,
}

/// How much a Sim must like a little one to pick them up when they're not family (the game's
/// `kBabyToddlerLikingThresholdToAllowInteractionsWithStrangers`, as tuned).
pub const STRANGER_LIKING: f32 = 30.0;

/// Tired enough to be put to bed (the game's Tired buff on a little one).
fn tired(m: &crate::sim::Motives) -> bool {
    m.0[crate::sim::ENERGY] < -20.0
}

impl CarryLook<'_, '_> {
    /// The little one someone's holding.
    pub fn held_by(&self, who: Entity) -> Option<Entity> {
        self.holding.get(who).ok().map(|h| h.0)
    }

    /// The empty crib nearest a place, if there's one about.
    fn crib_near(&self, at: Vec3) -> Option<Entity> {
        let steps: Vec<CarryStep> = self.moving.iter().map(|m| m.step).collect();
        let taken = cribs_taken(&self.littles, &steps, &self.cribs);
        free_crib(at, self.cribs.iter().filter(|c| c.1.kind == ObjectKind::Crib).map(|c| (c.0, c.2.translation)), &taken, CRIB_REACH)
    }

    /// The crib a little one's in, if they are.
    fn crib_of(&self, child: Entity) -> Option<Entity> {
        let (_, sim, tf, in_crib, bedtime, carried) = self.littles.get(child).ok()?;
        if carried || !(in_crib.is_some() || (sim.age == Age::Toddler && bedtime)) {
            return None;
        }
        let at = in_crib.map_or(tf.translation, |c| c.0);
        self.cribs.iter().filter(|c| c.1.kind == ObjectKind::Crib && c.2.translation.xz().distance(at.xz()) < 0.5).map(|c| c.0).next()
    }

    /// A little one's menu for a teen or grown-up: picked up (out of the crib, if they're in
    /// one; family's, or a friend's), or, held by them, put down or put in a crib.
    pub fn on_little(&self, actor: Entity, actor_age: Age, actor_at: Vec3, child: Entity, related: bool) -> Vec<(String, ActionKind)> {
        let Ok((_, sim, ..)) = self.littles.get(child) else { return Vec::new() };
        if !can_carry(actor_age) || !sim.age.is_little() {
            return Vec::new();
        }
        let first = sim.first.clone();
        let carry = |step: CarryStep, tired: bool| (label(step, &first, tired), ActionKind::Carry { child, step, secs: f32::INFINITY });
        match self.carried.get(child) {
            Ok((_, true, _)) => Vec::new(),
            Ok((c, _, motives)) if c.by == actor => {
                let mut out = vec![carry(CarryStep::PutDown(None), false)];
                if let Some(crib) = self.crib_near(actor_at) {
                    out.push(carry(CarryStep::InCrib(crib), tired(motives)));
                }
                out
            }
            // (In someone else's arms: theirs to put down.)
            Ok(_) if self.holding.iter().any(|h| h.0 == child) => Vec::new(),
            _ if related => vec![carry(CarryStep::PickUp { crib: self.crib_of(child) }, false)],
            _ => Vec::new(),
        }
    }

    /// A crib's menu: the little one in their arms put in it, if it's empty; the one in it picked
    /// up, if their hands are free (the crib's own "Pick Up").
    pub fn on_crib(&self, actor: Entity, actor_age: Age, crib: Entity) -> Vec<(String, ActionKind)> {
        if !can_carry(actor_age) || self.cribs.get(crib).is_err() {
            return Vec::new();
        }
        let inside = self.littles.iter().find(|l| self.crib_of(l.0) == Some(crib)).map(|l| l.0);
        let (child, step) = match (self.held_by(actor), inside) {
            (Some(child), None) => (child, CarryStep::InCrib(crib)),
            (None, Some(child)) => (child, CarryStep::PickUp { crib: Some(crib) }),
            _ => return Vec::new(),
        };
        let Ok((_, sim, ..)) = self.littles.get(child) else { return Vec::new() };
        let tired = matches!(step, CarryStep::InCrib(_)) && self.carried.get(child).is_ok_and(|c| tired(c.2));
        vec![(label(step, &sim.first, tired), ActionKind::Carry { child, step, secs: f32::INFINITY })]
    }

    /// The ground's menu for someone holding a little one: put down there (the game's
    /// `PutDownChildHere`).
    pub fn on_ground(&self, actor: Entity, at: Vec2) -> Option<(String, ActionKind)> {
        let child = self.held_by(actor)?;
        let (_, sim, ..) = self.littles.get(child).ok()?;
        let step = CarryStep::PutDown(Some(at));
        Some((label(step, &sim.first, false), ActionKind::Carry { child, step, secs: f32::INFINITY }))
    }
}

/// When to look for babies left on the floor again (game minutes).
const TIDY_EVERY: f64 = 5.0;

/// A baby left lying on the floor is put in an empty crib by whoever of the family's free (the
/// game's `PutChildInCrib`, scored 1 for a loose baby on the household's `LookAfterBabyOrToddler`).
#[allow(clippy::type_complexity)]
fn tidy_babies(
    clock: Res<crate::clock::GameClock>,
    mut last: Local<f64>,
    babies: Query<(Entity, &Sim, &Transform), (With<OnFloor>, With<HouseholdMember>, Without<Carried>, Without<BeingMoved>)>,
    mut grown: Query<(&Sim, &Transform, &mut ActionQueue), (With<HouseholdMember>, Without<Holding>, Without<CarryMove>, Without<OnFloor>)>,
    littles: Query<(Entity, &Sim, &Transform, Option<&InCrib>, Has<Bedtime>, Has<Carried>), Without<CarryMove>>,
    cribs: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    moving: Query<&CarryMove>,
) {
    if clock.minutes - *last < TIDY_EVERY {
        return;
    }
    *last = clock.minutes;
    let mut steps: Vec<CarryStep> = moving.iter().map(|m| m.step).collect();
    for (baby, bsim, btf) in &babies {
        // (Someone's on their way to them already.)
        if grown.iter().any(|g| g.2.0.iter().any(|a| matches!(a.kind, ActionKind::Carry { child, .. } if child == baby))) {
            continue;
        }
        let taken = cribs_taken(&littles, &steps, &cribs);
        let Some(crib) = free_crib(btf.translation, cribs.iter().filter(|c| c.1.kind == ObjectKind::Crib).map(|c| (c.0, c.2.translation)), &taken, CRIB_REACH) else { continue };
        let Some((sim, _, mut queue)) = grown
            .iter_mut()
            .filter(|g| can_carry(g.0.age) && g.2.0.is_empty())
            .min_by(|a, b| a.1.translation.distance(btf.translation).total_cmp(&b.1.translation.distance(btf.translation)))
        else {
            continue;
        };
        debug!("carry: {} puts {} in a crib", sim.first, bsim.first);
        let up = CarryStep::PickUp { crib: None };
        let down = CarryStep::InCrib(crib);
        queue.0.push_back(Action::new(label(up, &bsim.first, false), ActionKind::Carry { child: baby, step: up, secs: f32::INFINITY }, true));
        queue.0.push_back(Action::new(label(down, &bsim.first, false), ActionKind::Carry { child: baby, step: down, secs: f32::INFINITY }, true));
        steps.push(down);
    }
}

/// CARRY_TEST=<step>[,<step>...]: the selected Sim (or CARRY_BY's) picks up the household's first baby (or
/// toddler, `toddler` first), then each step in turn: `walk` (a few metres off), `down`, `here`
/// (a couple of metres off), `crib`, `up`.
#[allow(clippy::type_complexity)]
fn carry_test(
    mut done: Local<bool>,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Has<crate::sim::Selected>), With<HouseholdMember>>,
    littles: Query<(Entity, &Sim, Option<&InCrib>), With<HouseholdMember>>,
    cribs: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
) {
    if *done || clock.minutes < 1.0 {
        return;
    }
    let Ok(spec) = std::env::var("CARRY_TEST") else {
        *done = true;
        return;
    };
    *done = true;
    let want = if spec.starts_with("toddler") { Age::Toddler } else { Age::Baby };
    let Some((child, csim, in_crib)) = littles.iter().find(|l| l.1.age == want) else {
        warn!("carry test: no {want:?} in the household");
        return;
    };
    let first = csim.first.clone();
    let crib_at = |at: Vec3| cribs.iter().filter(|c| c.1.kind == ObjectKind::Crib).min_by(|a, b| a.2.translation.distance(at).total_cmp(&b.2.translation.distance(at))).map(|c| c.0);
    // (CARRY_BY=<first name>: who carries; else the selected Sim.)
    let by = std::env::var("CARRY_BY").ok();
    let Some((_, sim, tf, mut queue, _)) = sims.iter_mut().filter(|s| can_carry(s.1.age) && by.as_ref().is_none_or(|b| s.1.first == *b)).max_by_key(|s| s.4) else { return };
    let from_crib = in_crib.and_then(|c| crib_at(c.0));
    let mut steps = vec![CarryStep::PickUp { crib: from_crib }];
    let f = forward(tf.rotation);
    for s in spec.split(',').skip(usize::from(spec.starts_with("toddler"))) {
        for step in steps.drain(..) {
            queue.0.push_back(Action::new(label(step, &first, false), ActionKind::Carry { child, step, secs: f32::INFINITY }, false));
        }
        match s {
            "walk" => {
                let p = tf.translation.xz() - f * 3.0;
                queue.0.push_back(Action::new("Go Here", ActionKind::GoHere(p, 1), false));
            }
            "down" => steps.push(CarryStep::PutDown(None)),
            "here" => steps.push(CarryStep::PutDown(Some(tf.translation.xz() + Vec2::new(f.y, -f.x) * 2.0))),
            "crib" => {
                if let Some(c) = crib_at(tf.translation) {
                    steps.push(CarryStep::InCrib(c));
                }
            }
            "up" => steps.push(CarryStep::PickUp { crib: None }),
            _ => {}
        }
    }
    for step in steps.drain(..) {
        queue.0.push_back(Action::new(label(step, &first, false), ActionKind::Carry { child, step, secs: f32::INFINITY }, false));
    }
    info!("carry test: {} with {first}: {spec}", sim.first);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_babys_pick_up_by_how_it_lies() {
        let f = Vec2::new(0.0, -1.0);
        // (Lying as it's put down: the plain clip, nothing turned.)
        let (v, rest) = pick_up_variant(f, f);
        assert_eq!(v, "");
        assert!(rest.abs() < 1e-5);
        // (A quarter round either way, and the other way about.)
        assert_eq!(pick_up_variant(f, turned(f, FRAC_PI_2)).0, "_270");
        assert_eq!(pick_up_variant(f, turned(f, -FRAC_PI_2)).0, "_90");
        assert_eq!(pick_up_variant(f, -f).0, "_180");
        // (Between: the nearer quarter, the rest made up by turning.)
        let (v, rest) = pick_up_variant(f, turned(f, 0.3));
        assert_eq!(v, "");
        assert!((rest - 0.3).abs() < 1e-4);
        let (v, rest) = pick_up_variant(f, turned(f, FRAC_PI_2 + 0.2));
        assert_eq!(v, "_270");
        assert!((rest - 0.2).abs() < 1e-4);
        // (Turned by the rest, the slot lines up with the quarter exactly.)
        let b = turned(f, 2.5);
        let (v, rest) = pick_up_variant(f, b);
        let (v2, rest2) = pick_up_variant(turned(f, rest), b);
        assert_eq!(v, v2);
        assert!(rest2.abs() < 1e-4);
    }

    #[test]
    fn standing_to_reach_and_the_cribs_free() {
        assert_eq!(beside(Vec2::ZERO, Vec2::new(3.0, 0.0), 0.7), Vec2::new(0.7, 0.0));
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        let cribs = [(a, Vec3::new(1.0, 0.0, 0.0)), (b, Vec3::new(5.0, 0.0, 0.0))];
        assert_eq!(free_crib(Vec3::ZERO, cribs.into_iter(), &[], 40.0), Some(a));
        assert_eq!(free_crib(Vec3::ZERO, cribs.into_iter(), &[Vec3::new(1.0, 0.4, 0.1)], 40.0), Some(b));
        assert_eq!(free_crib(Vec3::ZERO, cribs.into_iter(), &[Vec3::new(1.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0)], 40.0), None);
        assert_eq!(free_crib(Vec3::ZERO, cribs.into_iter(), &[], 3.0), Some(a));
        assert!(can_carry(Age::Teen) && !can_carry(Age::Child) && !can_carry(Age::Toddler));
        let kid = Entity::from_bits(9);
        assert!(with_child(&ActionKind::GoHere(Vec2::ZERO, 1), kid, Age::Baby));
        assert!(with_child(&ActionKind::Carry { child: kid, step: CarryStep::PutDown(None), secs: 0.0 }, kid, Age::Baby));
        assert!(!with_child(&ActionKind::Carry { child: kid, step: CarryStep::PickUp { crib: None }, secs: 0.0 }, kid, Age::Baby));
        assert!(!with_child(&ActionKind::Social { target: a, social: 0 }, kid, Age::Baby));
        // (Their care in the arms goes on there; on the floor, they're put down for it.)
        let social = |n: &str| ActionKind::Social { target: kid, social: crate::social::social_index(n).unwrap() };
        assert!(with_child(&social("Feed"), kid, Age::Baby));
        assert!(!with_child(&social("Feed"), kid, Age::Toddler));
        assert!(with_child(&social("Change Diaper"), kid, Age::Toddler));
        assert!(!with_child(&social("Teach to Walk"), kid, Age::Toddler));
    }
}
