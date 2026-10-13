//! Standing about with nothing to do, Sims fidget as the game's do (its `Idles` tuning, picked
//! as its `IdleManager` picks): now and then one of the everyday standing idles, their mood's,
//! one of their moodlets' (yawning when tired, the bladder dance, a sniff of an armpit) or one
//! of their traits', weighted as the game weights them (1, 3, 6 and 6). Half of those with a
//! thought balloon show it.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use s3bake::gamedata::IdleAnim;

use crate::PlayMode;
use crate::anim::{ActionClip, ClipLibrary};
use crate::clock::{GameClock, SPEED_RATES};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct IdlesPlugin;

impl Plugin for IdlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, idle.run_if(in_state(PlayMode::Live)));
    }
}

/// An idle under way: its first clip (to know it's still the one playing) and how long it has
/// left (seconds at the game's pace).
#[derive(Component)]
pub struct Idling {
    first: &'static str,
    left: f32,
    total: f32,
}

impl Idling {
    /// Its first clip, and how far through it is (0 to 1).
    pub fn progress(&self) -> (&'static str, f32) {
        (self.first, 1.0 - self.left / self.total.max(0.01))
    }
}

/// When a Sim standing about next fidgets (game minutes).
#[derive(Component)]
struct NextIdle(f64);

/// The game's weights for the kinds of idle: an everyday standing one, a mood's, a moodlet's,
/// a trait's (`kStandingIdleChance`, `kMoodIdleChance`, `kBuffIdleChance`, `kTraitIdleChance`).
const WEIGHTS: [u32; 4] = [1, 3, 6, 6];
/// The share of idles with a thought balloon that show it (`kThoughtBalloonChance`).
const BALLOON_CHANCE: f64 = 0.5;

/// The table's letter for an age.
pub(crate) fn age_letter(age: Age) -> Option<&'static str> {
    match age {
        Age::Child => Some("C"),
        Age::Teen => Some("T"),
        Age::YoungAdult => Some("Y"),
        Age::Adult => Some("A"),
        Age::Elder => Some("E"),
        _ => None,
    }
}

/// The game's mood flavour for how a Sim feels (the tables' `MoodFlavor`s), the nearest ones
/// first.
fn mood_flavours(mood: f32) -> &'static [&'static str] {
    match mood {
        m if m >= 80.0 => &["Elated", "VeryHappy", "Happy"],
        m if m >= 50.0 => &["VeryHappy", "Happy"],
        m if m >= 20.0 => &["Happy"],
        m if m <= -60.0 => &["Miserable", "Depressed", "Sad"],
        m if m <= -35.0 => &["Tense", "Uncomfortable"],
        m if m <= -20.0 => &["Uncomfortable"],
        _ => &[],
    }
}

/// Clip names made `'static` once, for the animation scripts (a game's worth of idles).
#[derive(Default)]
struct Names(HashMap<String, (&'static str, &'static [&'static str])>);

impl Names {
    fn of(&mut self, clips: &[String]) -> (&'static str, &'static [&'static str]) {
        let key = clips.join(",");
        *self.0.entry(key).or_insert_with(|| {
            let all: Vec<&'static str> = clips.iter().map(|c| &*Box::leak(c.clone().into_boxed_str())).collect();
            let rest: &'static [&'static str] = Box::leak(all[1..].to_vec().into_boxed_slice());
            (all[0], rest)
        })
    }
}

/// A table's idles under a name (a trait's, a moodlet's, a mood's) that suit.
fn group_of<'a>(all: &'a [(String, Vec<IdleAnim>)], key: &str, fits: &dyn Fn(&&IdleAnim) -> bool) -> Vec<&'a IdleAnim> {
    all.iter().filter(|g| g.0.eq_ignore_ascii_case(key)).flat_map(|g| g.1.iter()).filter(|a| fits(a)).collect()
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn idle(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    data: Res<crate::baked::Baked>,
    mut lib: ResMut<ClipLibrary>,
    mut sims: Query<
        (
            Entity,
            &Sim,
            &crate::interact::ActionQueue,
            &SimAnim,
            Option<&mut Idling>,
            Option<&mut NextIdle>,
            Option<&ActionClip>,
            Option<&crate::nav::PathFollow>,
            (&InheritedVisibility, &crate::life::Mood, Option<&crate::life::Moodlets>),
        ),
        (Without<crate::swim::Swimming>, Without<crate::death::Dying>, Without<crate::fire::OnFire>, Without<crate::dressing::ClothesSpin>),
    >,
    (mut known, mut names): (Local<Option<HashSet<String>>>, Local<Names>),
) {
    let Some(ui) = ui else { return };
    let tables = &ui.data.idles;
    let known = known.get_or_insert_with(|| data.0.clip_names.iter().map(|n| n.to_ascii_lowercase()).collect());
    let dt = time.delta_secs().min(0.1) * SPEED_RATES[clock.speed];
    let mut rng = rand::rng();
    for (e, sim, queue, anim, idling, next, clip, path, (shown, mood, moodlets)) in &mut sims {
        let walking = path.is_some_and(|p| !p.done);
        let free = queue.0.is_empty() && !walking && anim.pose == Pose::Stand && shown.get();
        // An idle under way: done when its clips are, or as soon as there's something to do.
        if let Some(mut i) = idling {
            i.left -= dt;
            if !free || i.left <= 0.0 {
                commands.entity(e).remove::<Idling>();
                if clip.is_some_and(|c| c.start == Some(i.first)) {
                    commands.entity(e).remove::<ActionClip>();
                }
            }
            continue;
        }
        let Some(age) = age_letter(sim.age) else { continue };
        let Some(mut next) = next else {
            commands.entity(e).insert(NextIdle(clock.minutes + rng.random_range(5.0..15.0)));
            continue;
        };
        if !free || clip.is_some() || clock.minutes < next.0 {
            continue;
        }
        next.0 = clock.minutes + rng.random_range(8.0..25.0);
        let fits = |a: &&IdleAnim| {
            !a.clips.is_empty()
                && a.ages.iter().any(|x| x == age)
                && (a.trait_required.is_empty() || sim.traits.iter().any(|t| t.game_id().eq_ignore_ascii_case(&a.trait_required)))
                && a.clips.iter().all(|c| known.contains(&c.to_ascii_lowercase()))
        };
        // The four kinds, each with what this Sim has of it.
        let standing: Vec<&IdleAnim> = tables.standing.iter().filter(fits).collect();
        let by_mood: Vec<&IdleAnim> = mood_flavours(mood.0).iter().map(|f| group_of(&tables.mood, f, &fits)).find(|v| !v.is_empty()).unwrap_or_default();
        let by_buff: Vec<&IdleAnim> = moodlets.map(|m| m.0.iter().flat_map(|b| group_of(&tables.buffs, b.kind.buff().0, &fits)).collect()).unwrap_or_default();
        let by_trait: Vec<&IdleAnim> = sim.traits.iter().flat_map(|t| group_of(&tables.traits, &t.game_id(), &fits)).collect();
        let kinds = [standing, by_mood, by_buff, by_trait];
        let total: u32 = kinds.iter().zip(WEIGHTS).filter(|(k, _)| !k.is_empty()).map(|(_, w)| w).sum();
        if total == 0 {
            continue;
        }
        let mut roll = rng.random_range(0..total);
        let Some(pool) = kinds.iter().zip(WEIGHTS).filter(|(k, _)| !k.is_empty()).find_map(|(k, w)| {
            if roll < w {
                Some(k)
            } else {
                roll -= w;
                None
            }
        }) else {
            continue;
        };
        // (Within a kind, by each idle's own likelihood.)
        let Ok(pick) = pool.choose_weighted(&mut rng, |a| a.probability.max(1)) else { continue };
        let length: f32 = pick.clips.iter().filter_map(|c| lib.get(&data, c)).map(|c| c.duration).sum();
        if length <= 0.0 {
            continue;
        }
        let (first, rest) = names.of(&pick.clips);
        debug!("{} idles: {} ({:.1} s)", sim.first, pick.clips.join(", "), length);
        commands.entity(e).insert((ActionClip::steps(first, rest, &[]), Idling { first, left: length, total: length }));
        if !pick.balloon.is_empty()
            && rng.random_bool(BALLOON_CHANCE)
            && let Some(b) = ui.data.balloons.list(&pick.balloon).and_then(|l| l.choose(&mut rng))
        {
            commands.entity(e).insert(crate::balloons::BalloonRequest { kind: crate::balloons::BalloonKind::Thought, icon: b.icon.clone(), axis: b.axis });
        }
    }
}
