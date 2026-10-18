//! Sims react to what goes on about them, as the game's do (its reaction broadcasters and
//! `PlayReaction`), each with one of the game's own `Reactions` clips, turned to face it:
//!
//! - near someone smelly (in the same room, or in sight out of doors), a sniff and a grimace,
//!   and the Disgusted moodlet while they're about (not the Slobs);
//! - at someone wetting themselves, shock, or what their traits make of it when they're not
//!   friends (a laugh from the Mean-Spirited, a grumble from the Grumpy, tears from the
//!   Over-Emotional); and the one it happened to, embarrassed, when anyone was near;
//! - at a death, shock, then tears for someone close, a laugh from the Evil or an enemy, or a
//!   distressed look. Their partner is heartbroken; family and friends mourn;
//! - to some moodlets as they come (as the game's buffs do): embarrassment, a wipe of the brow
//!   when worn out, tears of joy at a first kiss, queasiness, a start when scared, a jolt when
//!   singed;
//! - to the weather, out in it as it starts: rain, snow, hail (the game's `ReactRain`...).
//!
//! A reaction waits for its Sim to be standing free (between things, as the game's
//! `AfterInteraction`), for a while, then lapses. Sitting at a table or on a sofa, they react
//! where they sit, with the game's seated clips, and carry on.

use std::collections::{HashMap, HashSet};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::anim::{ActionClip, ClipLibrary, FaceOverlay};
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, ObjectKind, Phase};
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::nav::{Floor, NavGrid, UpperFloors};
use crate::sim::{Age, HouseholdMember, Pose, Sim, SimAnim};
use crate::social::{FRIEND_THRESHOLD, RelStatus, Relationship, Relationships};
use crate::visit::OnLot;

pub struct ReactionsPlugin;

impl Plugin for ReactionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Spectacle>().add_systems(Update, (smelly_about, accidents, deaths, on_moodlets, weather, play, turn, seated).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// Something for those about to react to.
#[derive(Message, Clone, Copy)]
pub enum Spectacle {
    /// A Sim couldn't make it to the bathroom in time.
    Accident(Entity),
}

/// Reactions a Sim's to play in turn (the `Reactions` table's kinds), facing `toward`, if
/// they're free to before `until` (game minutes).
#[derive(Component)]
pub struct Pending {
    kinds: Vec<&'static str>,
    toward: Option<Vec3>,
    until: f64,
}

/// Turning to face what they react to.
#[derive(Component)]
struct Facing(Quat);

/// A reaction played sitting down, over what they're doing there: the clip it took the place
/// of (put back after), the action it was over, and how long it has left (seconds at the
/// game's pace).
#[derive(Component)]
struct SeatedReaction {
    prev: Option<ActionClip>,
    label: String,
    left: f32,
}

/// How far off those who see something react (the game's `kDistanceToReact`).
const REACT_RANGE: f32 = 10.0;
/// How near someone must be for an accident to embarrass (`kDistanceForEmbarrassedBuff`).
const EMBARRASS_RANGE: f32 = 5.0;
/// How far a smell carries (the game's fills the room; out of doors, this far).
const SMELL_RANGE: f32 = 10.0;
/// How often those about someone smelly notice again (game minutes).
const SMELL_PULSE: f64 = 10.0;
/// How long a reaction waits for its Sim to be free (game minutes): a passing sniff, or
/// something worth stopping for.
const BRIEF: f64 = 10.0;
const LASTING: f64 = 30.0;
/// The friendship and romance at which a death breaks a partner's heart, and the friendship at
/// which a friend mourns (the game's `kLikingValueForPartner`, `kMinLikingAddMourning`).
const PARTNER_LIKING: f32 = 45.0;
const MOURNING_LIKING: f32 = 20.0;

/// Where a Sim is: a spot on a floor of a lot (`None`, home or the street).
struct Spot {
    at: Vec3,
    floor: u8,
    lot: Option<usize>,
}

fn spot(tf: &Transform, floor: Option<&Floor>, lot: Option<&OnLot>) -> Spot {
    Spot { at: tf.translation, floor: floor.map_or(1, |f| f.0), lot: lot.map(|l| l.0) }
}

/// The floors' walk grids, to see by: what's between two Sims.
#[derive(SystemParam)]
struct Sight<'w> {
    grid: Option<Res<'w, NavGrid>>,
    upper: Option<Res<'w, UpperFloors>>,
    visited: Option<Res<'w, crate::visit::VisitedLot>>,
}

impl Sight<'_> {
    /// Whether one Sim sees another: on the same floor of the same lot, near enough, with no
    /// wall between.
    fn sees(&self, a: &Spot, b: &Spot, range: f32) -> bool {
        if a.lot != b.lot || a.floor != b.floor || a.at.distance(b.at) > range {
            return false;
        }
        let grid = match a.lot {
            Some(l) => self.visited.as_deref().filter(|v| v.lot == l).and_then(|v| if a.floor == 1 { Some(&v.grid) } else { v.upper.grids.get(&a.floor) }),
            None if a.floor == 1 => self.grid.as_deref(),
            None => self.upper.as_deref().and_then(|u| u.grids.get(&a.floor)),
        };
        !grid.is_some_and(|g| g.crosses(a.at.xz(), b.at.xz()))
    }
}

impl Pending {
    pub fn new(kinds: &[&'static str], toward: Option<Vec3>, until: f64) -> Self {
        Self { kinds: kinds.to_vec(), toward, until }
    }
}

fn partners(r: &Relationship) -> bool {
    matches!(r.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married) || (r.romantic && r.friendship > PARTNER_LIKING)
}

/// What a Sim's traits make of something bad befalling someone not their friend (the game's
/// `ReactionByTrait`), if anything.
fn by_trait(sim: &Sim, child: bool, rng: &mut impl Rng) -> Option<&'static str> {
    let mut kinds: Vec<&'static str> = [
        (Trait::Absentminded, "TraitAbsentMinded"),
        (Trait::Coward, "TraitCoward"),
        (Trait::Evil, "TraitEvil"),
        (Trait::Grumpy, "TraitGrumpy"),
        (Trait::Insane, "TraitInsane"),
        (Trait::MeanSpirited, "PointLaugh"),
        (Trait::Inappropriate, "PointLaugh"),
        (Trait::OverEmotional, "TraitOverEmotional"),
    ]
    .into_iter()
    .filter(|(t, _)| sim.traits.contains(t))
    .map(|(_, k)| k)
    .collect();
    if child && sim.traits.contains(&Trait::DislikesChildren) {
        kinds.push("TraitDislikesChildren");
    }
    kinds.dedup();
    kinds.choose(rng).copied()
}

/// Sims about someone smelly: a sniff and a grimace when they first notice, and Disgusted
/// while they're near (the game's `BuffSmelly` broadcaster).
#[allow(clippy::type_complexity)]
fn smelly_about(
    mut commands: Commands,
    clock: Res<GameClock>,
    sight: Sight,
    mut next: Local<f64>,
    mut sims: Query<(Entity, &Sim, &Transform, Option<&Floor>, Option<&OnLot>, &mut Moodlets, &SimAnim, &InheritedVisibility)>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + SMELL_PULSE;
    let smelly: Vec<(Entity, Spot)> = sims
        .iter()
        .filter(|(_, sim, _, _, _, m, _, shown)| shown.get() && !sim.age.is_little() && m.has(MoodletKind::Smelly))
        .map(|(e, _, tf, f, l, ..)| (e, spot(tf, f, l)))
        .collect();
    if smelly.is_empty() {
        return;
    }
    for (e, sim, tf, f, l, mut m, anim, shown) in &mut sims {
        if !shown.get() || sim.age.is_little() || sim.traits.contains(&Trait::Slob) || anim.pose == Pose::Lie {
            continue;
        }
        let me = spot(tf, f, l);
        let Some((_, s)) = smelly.iter().find(|(o, s)| *o != e && sight.sees(&me, s, SMELL_RANGE)) else { continue };
        if !m.has(MoodletKind::Disgusted) {
            commands.entity(e).insert(Pending::new(&["SmellsBad"], Some(s.at), clock.minutes + BRIEF));
        }
        m.add(MoodletKind::Disgusted, clock.minutes);
    }
}

/// Those who see someone wet themselves react (the game's `CriticalOneShotReaction`); with
/// anyone near, they're embarrassed (and a Loser alone cries).
#[allow(clippy::type_complexity)]
fn accidents(
    mut commands: Commands,
    clock: Res<GameClock>,
    sight: Sight,
    mut seen: MessageReader<Spectacle>,
    mut sims: Query<(Entity, &Sim, &Transform, Option<&Floor>, Option<&OnLot>, Option<&mut Moodlets>, Option<&Relationships>, &InheritedVisibility)>,
) {
    let mut rng = rand::rng();
    for s in seen.read() {
        let Spectacle::Accident(who) = *s;
        let Ok((_, victim, tf, f, l, ..)) = sims.get(who) else { continue };
        let (there, child, loser) = (spot(tf, f, l), matches!(victim.age, Age::Child | Age::Toddler | Age::Baby), victim.traits.contains(&Trait::Loser));
        let mut near = false;
        for (e, sim, tf, f, l, _, rels, shown) in &sims {
            let me = spot(tf, f, l);
            if e == who || !shown.get() || sim.age.is_little() || !sight.sees(&me, &there, REACT_RANGE) {
                continue;
            }
            near |= me.at.distance(there.at) <= EMBARRASS_RANGE;
            let r = rels.map(|r| r.get(who)).unwrap_or_default();
            let friends = r.friendship >= FRIEND_THRESHOLD || partners(&r);
            let kind = if friends { None } else { by_trait(sim, child, &mut rng) };
            commands.entity(e).insert(Pending::new(&[kind.unwrap_or("Shocked")], Some(there.at), clock.minutes + LASTING));
        }
        if near {
            if let Ok((.., Some(mut m), _, _)) = sims.get_mut(who) {
                m.add(MoodletKind::Embarrassed, clock.minutes);
            }
        } else if loser {
            commands.entity(who).insert(Pending::new(&["Cry"], None, clock.minutes + LASTING));
        }
    }
}

/// A death: those who see it react (the game's `DeathReaction`), and those it leaves behind
/// grieve (its `FinalizeSimDeathRelationships`): a partner heartbroken, family and friends in
/// mourning.
#[allow(clippy::type_complexity)]
fn deaths(
    mut commands: Commands,
    clock: Res<GameClock>,
    sight: Sight,
    dying: Query<(Entity, &Transform, Option<&Floor>, Option<&OnLot>, Has<HouseholdMember>), Added<crate::death::Dying>>,
    mut sims: Query<
        (Entity, &Sim, &Transform, Option<&Floor>, Option<&OnLot>, &mut Moodlets, Option<&Relationships>, Has<HouseholdMember>, &InheritedVisibility),
        (Without<crate::death::Dying>, Without<crate::death::GrimReaper>),
    >,
) {
    for (dead, tf, f, l, family) in &dying {
        let there = spot(tf, f, l);
        for (e, sim, tf, f, l, mut m, rels, member, shown) in &mut sims {
            let r = rels.map(|r| r.get(dead)).unwrap_or_default();
            let (partner, kin) = (partners(&r), family && member);
            if partner {
                m.add(MoodletKind::Heartbroken, clock.minutes);
            } else if kin || r.friendship > MOURNING_LIKING {
                m.add(MoodletKind::Mourning, clock.minutes);
            }
            if !shown.get() || sim.age.is_little() || !sight.sees(&spot(tf, f, l), &there, REACT_RANGE) {
                continue;
            }
            let then = if partner || kin || r.friendship >= 40.0 {
                "Cry"
            } else if sim.traits.contains(&Trait::Evil) {
                "EvilLaugh"
            } else if r.friendship < -60.0 {
                "EnemyLaugh"
            } else {
                "ReactionWatch"
            };
            commands.entity(e).insert(Pending::new(&["Shocked", then], Some(there.at), clock.minutes + LASTING));
        }
    }
}

/// The chance a Sim out in the weather as it starts reacts to it.
const WEATHER_CHANCE: f64 = 0.6;

/// Rain, snow or hail starting: those out in it (not under a roof) look up at it, as the game's
/// Sims do, once a spell.
#[allow(clippy::type_complexity)]
fn weather(
    mut commands: Commands,
    clock: Res<GameClock>,
    weather: Option<Res<crate::weather::Weather>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut seen: Local<Option<f64>>,
    sims: Query<(Entity, &Sim, &Transform, &InheritedVisibility), Without<crate::swim::Swimming>>,
) {
    use crate::weather::WeatherKind;
    let Some(w) = weather else { return };
    let kind = match w.kind {
        WeatherKind::Rain => "ReactRain",
        WeatherKind::Snow => "ReactSnow",
        WeatherKind::Hail => "ReactHail",
        _ => return,
    };
    // (Once it's begun to come down, once a spell.)
    if *seen == Some(w.since) || w.falling(clock.minutes) < 0.1 {
        return;
    }
    *seen = Some(w.since);
    let mut rng = rand::rng();
    for (e, sim, tf, shown) in &sims {
        if !shown.get() || sim.age.is_little() || crate::weather::sheltered(building.as_deref(), tf.translation) || !rng.random_bool(WEATHER_CHANCE) {
            continue;
        }
        debug!("{} looks up at the {kind}", sim.first);
        commands.entity(e).insert(Pending::new(&[kind], None, clock.minutes + BRIEF));
    }
}

/// The moodlets with a reaction as they come (the game's buffs' `OnAddition`).
const ON_MOODLET: [(MoodletKind, &str); 7] = [
    (MoodletKind::Embarrassed, "Embarrassed"),
    (MoodletKind::Fatigued, "WipeBrow"),
    (MoodletKind::FirstKiss, "JoyousCrying"),
    (MoodletKind::Nauseous, "Nauseous"),
    (MoodletKind::Scared, "Surprise"),
    (MoodletKind::Singed, "Shocked"),
    (MoodletKind::RudeAwakening, "Angry"),
];

/// Which of those moodlets a Sim has, a bit each.
fn reacted_moodlets(m: &Moodlets) -> u8 {
    ON_MOODLET.iter().enumerate().filter(|(_, (k, _))| m.has(*k)).fold(0, |bits, (i, _)| bits | 1 << i)
}

/// A reaction to a moodlet just come (not those a Sim had when first seen: a game loaded).
fn on_moodlets(mut commands: Commands, clock: Res<GameClock>, mut had: Local<HashMap<Entity, u8>>, sims: Query<(Entity, &Moodlets), Changed<Moodlets>>) {
    for (e, m) in &sims {
        let now = reacted_moodlets(m);
        let Some(before) = had.insert(e, now) else { continue };
        if let Some((_, kind)) = ON_MOODLET.iter().enumerate().find(|(i, _)| now & !before & (1 << i) != 0).map(|(_, k)| k) {
            commands.entity(e).insert(Pending::new(&[kind], None, clock.minutes + LASTING));
        }
    }
}

/// A reaction's clips for a Sim of an age (a child's are the `c_` ones), if the game has them.
fn clips_for(table: &[(String, Vec<s3bake::gamedata::IdleAnim>)], kind: &str, sim: &Sim, seated: bool, known: &HashSet<String>, rng: &mut impl Rng) -> Option<Vec<String>> {
    let letter = crate::idles::age_letter(sim.age)?;
    let child = sim.age == Age::Child;
    // (Sitting down: the game's seated version, `_sitting_x` for a standing one's `_x` or
    // `_standing_x`.)
    let sitting = |c: String| -> String {
        if !seated {
            return c;
        }
        let l = c.to_ascii_lowercase();
        let base = l.strip_suffix("_standing_x").or_else(|| l.strip_suffix("_x")).unwrap_or(&l);
        format!("{base}_sitting_x")
    };
    let fitting: Vec<(Vec<String>, u32)> = table
        .iter()
        .filter(|g| g.0.eq_ignore_ascii_case(kind))
        .flat_map(|g| g.1.iter())
        .filter(|a| a.ages.is_empty() || a.ages.iter().any(|x| x == letter))
        .filter_map(|a| {
            let clips: Vec<String> = a.clips.iter().map(|c| sitting(if child && c.starts_with("a_") { format!("c_{}", &c[2..]) } else { c.clone() })).collect();
            (!clips.is_empty() && clips.iter().all(|c| known.contains(&c.to_ascii_lowercase()))).then_some((clips, a.probability.max(1)))
        })
        .collect();
    fitting.choose_weighted(rng, |a| a.1).ok().map(|a| a.0.clone())
}

/// Reactions played as their Sims come free: their clips where they stand, facing what they
/// react to; or lapsed, left too long.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play(
    mut commands: Commands,
    clock: Res<GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    data: Res<crate::baked::Baked>,
    mut lib: ResMut<ClipLibrary>,
    mut sims: Query<
        (Entity, &Sim, &mut ActionQueue, &SimAnim, &Transform, &Pending, Option<&ActionClip>, Has<crate::idles::Idling>, Option<&crate::nav::PathFollow>, &InheritedVisibility),
        (
            Without<crate::swim::Swimming>,
            Without<crate::death::Dying>,
            Without<crate::fire::OnFire>,
            Without<crate::fire::Panicking>,
            Without<crate::dressing::ClothesSpin>,
            Without<SeatedReaction>,
        ),
    >,
    objects: Query<&crate::interact::GameObject>,
    (mut known, mut names): (Local<Option<HashSet<String>>>, Local<HashMap<String, &'static [&'static str]>>),
) {
    let Some(ui) = ui else { return };
    let known = known.get_or_insert_with(|| data.0.clip_names.iter().map(|n| n.to_ascii_lowercase()).collect());
    let mut rng = rand::rng();
    for (e, sim, mut queue, anim, tf, p, clip, idling, path, shown) in &mut sims {
        if clock.minutes > p.until || !shown.get() {
            commands.entity(e).remove::<Pending>();
            continue;
        }
        // (Sitting at a table or on a sofa: reacted to where they sit.)
        let seat = queue.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && anim.pose == Pose::Sit).and_then(|a| match a.kind {
            ActionKind::Object { target, .. } => objects
                .get(target)
                .ok()
                .filter(|o| matches!(o.kind, ObjectKind::Chair | ObjectKind::Stool | ObjectKind::Sofa | ObjectKind::PicnicTable))
                .map(|_| a.label.clone()),
            _ => None,
        });
        if let Some(label) = seat {
            let all: Vec<String> = p.kinds.iter().filter_map(|k| clips_for(&ui.data.idles.reactions, k, sim, true, known, &mut rng)).flatten().collect();
            let secs: f32 = all.iter().filter_map(|c| lib.get(&data, c)).map(|c| c.duration).sum();
            commands.entity(e).remove::<Pending>();
            if all.is_empty() || secs <= 0.0 {
                continue;
            }
            let clips: &'static [&'static str] = *names.entry(all.join(",")).or_insert_with(|| Box::leak(all.iter().map(|c| &*Box::leak(c.clone().into_boxed_str())).collect::<Vec<_>>().into_boxed_slice()));
            debug!("{} reacts where they sit: {} ({:.1} s)", sim.first, all.join(", "), secs);
            commands.entity(e).remove::<FaceOverlay>().insert((ActionClip::steps(clips[0], &clips[1..], &[]), SeatedReaction { prev: clip.cloned(), label, left: secs }));
            continue;
        }
        // (Standing, between things.)
        let walking = path.is_some_and(|p| !p.done);
        let between = queue.0.front().is_none_or(|a| a.phase == Phase::Start && !matches!(a.kind, ActionKind::Outro { .. } | ActionKind::MotiveFail(_)));
        if anim.pose != Pose::Stand || walking || !between {
            continue;
        }
        // (A fidget gives way; anything else playing, they're busy.)
        if idling {
            commands.entity(e).remove::<(crate::idles::Idling, ActionClip)>();
            continue;
        }
        if clip.is_some() {
            continue;
        }
        commands.entity(e).remove::<Pending>();
        let all: Vec<String> = p.kinds.iter().filter_map(|k| clips_for(&ui.data.idles.reactions, k, sim, false, known, &mut rng)).flatten().collect();
        let secs: f32 = all.iter().filter_map(|c| lib.get(&data, c)).map(|c| c.duration).sum();
        if all.is_empty() || secs <= 0.0 {
            continue;
        }
        let clips: &'static [&'static str] = *names.entry(all.join(",")).or_insert_with(|| Box::leak(all.iter().map(|c| &*Box::leak(c.clone().into_boxed_str())).collect::<Vec<_>>().into_boxed_slice()));
        debug!("{} reacts: {} ({:.1} s)", sim.first, all.join(", "), secs);
        queue.0.push_front(Action::new("React", ActionKind::Outro { clips, then: None, secs, stand_at: None, target: e }, true));
        commands.entity(e).remove::<FaceOverlay>().insert(ActionClip::steps(clips[0], &clips[1..], &[]));
        if let Some(at) = p.toward {
            let d = at - tf.translation;
            if d.xz().length() > 0.2 {
                commands.entity(e).insert(Facing(Quat::from_rotation_y(d.x.atan2(d.z))));
            }
        }
    }
}

/// A seated reaction over: back to what they were doing there (if they're still at it).
fn seated(mut commands: Commands, time: Res<Time>, clock: Res<GameClock>, mut sims: Query<(Entity, &mut SeatedReaction, &ActionQueue, &SimAnim)>) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    for (e, mut s, queue, anim) in &mut sims {
        s.left -= dt;
        let still = anim.pose == Pose::Sit && queue.0.front().is_some_and(|a| a.label == s.label && matches!(a.phase, Phase::Running(_)));
        if s.left > 0.0 && still {
            continue;
        }
        commands.entity(e).remove::<SeatedReaction>();
        if still {
            match s.prev.take() {
                Some(c) => commands.entity(e).insert(c),
                None => commands.entity(e).remove::<ActionClip>(),
            };
        }
    }
}

/// Turning to face what they react to, as the reaction starts.
fn turn(mut commands: Commands, time: Res<Time>, mut sims: Query<(Entity, &mut Transform, &Facing, &ActionQueue)>) {
    for (e, mut tf, f, queue) in &mut sims {
        tf.rotation = tf.rotation.slerp(f.0, (time.delta_secs() * 8.0).min(1.0));
        let reacting = queue.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Outro { .. }));
        if tf.rotation.angle_between(f.0) < 0.02 || !reacting {
            commands.entity(e).remove::<Facing>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traits_colour_a_reaction_only_when_they_have_one() {
        let mut rng = rand::rng();
        let mut sim = crate::sim::random_sim(&mut rng, "Test", Some(false), Age::Adult);
        sim.traits = vec![Trait::Friendly];
        assert_eq!(by_trait(&sim, false, &mut rng), None);
        sim.traits = vec![Trait::MeanSpirited, Trait::Inappropriate];
        assert_eq!(by_trait(&sim, false, &mut rng), Some("PointLaugh"));
        sim.traits = vec![Trait::DislikesChildren];
        assert_eq!(by_trait(&sim, false, &mut rng), None);
        assert_eq!(by_trait(&sim, true, &mut rng), Some("TraitDislikesChildren"));
    }
}
