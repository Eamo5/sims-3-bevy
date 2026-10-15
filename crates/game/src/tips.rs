//! Playing for tips, and watching someone play, as the game's `MusicalInstrument`: "Play for
//! Tips" on an instrument that's outdoors or on a community lot (the game's `TipsDefinition`),
//! the guitar's case opened before the player or a tip jar set out for the rest
//! (`PlaceTipsContainer`). Anyone else about may "Watch" someone playing (the game's
//! `WatchBase`): standing before them, liking them a little more each half hour
//! (`LikingIncreaseForWatchPlaying`, `MinutesForLikingGain`), enjoying the music from a player
//! of level five or better (`LevelToProvideEnjoyingMusicBuff`, ten and five more a level), and
//! at the end of each song (`ShortestSongLengthForTip` to `LongestSongLengthForTip`) applauding,
//! or not. Played for tips, every five minutes (`TimePerTipTest`) a watcher from outside the
//! household may decide to tip, at the player's skill's chance (`ChanceOfTipPerLevel`: none
//! below level five, then five in a hundred a level; twice that for the Good and the Easily
//! Impressed, half for the Evil, the Frugal and the Snobs), and at the song's end goes up to the
//! tips and puts in ten simoleons a level, give or take a quarter (`MoneyPerLevel`, `GiveTip`).

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, Notifications, ObjectKind, Phase, Skills, Special, interactions_for};
use crate::life::Trait;
use crate::sim::{HouseholdMember, Sim};
use crate::social::Relationships;

pub struct TipsPlugin;

impl Plugin for TipsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Performances>()
            .add_systems(Update, (performances.after(crate::interact::run_actions), tip_jars.after(performances), tip_test).run_if(in_state(PlayMode::Live)));
    }
}

/// How the game names playing for tips.
pub const PLAY_FOR_TIPS: &str = "Play for Tips";

/// `TimePerTipTest`, `ChanceOfTipPerLevel`, and the traits' `MoreLikelyToTipMultiplier` and
/// `LessLikelyToTipMultiplier`.
const TIP_TEST_MINUTES: f64 = 5.0;
const TIP_CHANCE: [f64; 11] = [0.0, 0.0, 0.0, 0.0, 0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3];
const MORE_LIKELY: [Trait; 2] = [Trait::EasilyImpressed, Trait::Good];
const LESS_LIKELY: [Trait; 3] = [Trait::Evil, Trait::Frugal, Trait::Snob];
/// `MoneyPerLevel`.
const MONEY_PER_LEVEL: f64 = 10.0;
/// `LikingIncreaseForWatchPlaying` every `MinutesForLikingGain`.
const LIKING_FOR_WATCHING: f32 = 5.0;
const MINUTES_FOR_LIKING: f64 = 30.0;
/// `ShortestSongLengthForTip`, `LongestSongLengthForTip`.
const SONG_MINUTES: std::ops::Range<f64> = 15.0..30.0;
/// `LevelToProvideEnjoyingMusicBuff`, `MoodScoreForEnjoyingMusic` and
/// `MoodScoreBonusPerLevelAboveRequirement`.
const ENJOYING_LEVEL: u32 = 5;
const ENJOYING_MOOD: i32 = 10;
const ENJOYING_PER_LEVEL: i32 = 5;
/// How long the music's enjoyed after (game minutes).
const ENJOYING_MINUTES: f64 = 120.0;
/// How far before the player a watcher stands, and how near the tips a tipper goes (the game's
/// `mfMinRadius`).
const WATCH_DISTANCE: f32 = 2.2;
const TIP_DISTANCE: f32 = 0.9;

/// The watchers' clips, as the instruments' state machines have them: grooving to it, cheering
/// a song (or booing it), and putting a tip in.
pub const WATCHING: &[&str] = &["a_Neutral_groove_idle_x", "a2o_tv_watch_idle2_standing_x"];
const APPLAUD: [&str; 2] = ["a_react_cheer_x", "a_react_cheer2_standing_x"];
const BOO: &str = "a_react_boo_standing_x";
const TIP: &str = "a2o_guitar_tip_x";
/// The tips' containers (the game's `GuitarCase`, `TipJar`).
const GUITAR_CASE: &str = "GuitarCase";
const TIP_JAR: &str = "tipJar";

/// Who's playing what, and where it could be played for tips.
#[derive(Resource, Default)]
pub struct Performances {
    /// Each instrument being played: its player, their skill level, and whether for tips.
    pub playing: HashMap<Entity, (Entity, u32, bool)>,
    /// The instruments outdoors or on a community lot.
    pub tips_ok: HashSet<Entity>,
}

impl Performances {
    /// Whether a Sim could watch an instrument (someone else is playing it).
    pub fn watchable(&self, instrument: Entity, sim: Entity) -> bool {
        self.playing.get(&instrument).is_some_and(|p| p.0 != sim)
    }
    pub fn tips_ok(&self, instrument: Entity) -> bool {
        self.tips_ok.contains(&instrument)
    }
}

/// The instruments.
pub fn instrument(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Guitar | ObjectKind::Piano | ObjectKind::Keyboard | ObjectKind::Drums | ObjectKind::LaserHarp)
}

/// The way the player faces to play (into the instrument, or along its routing slot).
fn facing(obj: &GameObject, otf: &Transform) -> Vec2 {
    let to = (obj.world_center(otf).xz() - obj.use_point(otf)).normalize_or(Vec2::Y);
    match obj.route {
        Some((_, f)) => {
            let w = otf.rotation * Vec3::new(f.x, 0.0, f.y);
            Vec2::new(w.x, w.z).normalize_or(to)
        }
        None => to,
    }
}

/// Where a watcher stands: a couple of metres before the player, spread out about them so a
/// crowd doesn't stand in one place.
pub fn watch_spot(obj: &GameObject, otf: &Transform, me: Entity) -> Vec2 {
    let spread = ((me.index_u32() % 7) as f32 - 3.0) * 0.3;
    obj.use_point(otf) + Vec2::from_angle(spread).rotate(facing(obj, otf)) * WATCH_DISTANCE
}

/// A watcher's chance of deciding to tip at each test: the player's level's, by the watcher's
/// traits.
pub fn tip_chance(level: u32, traits: &[Trait]) -> f64 {
    let mut p = TIP_CHANCE[(level as usize).min(TIP_CHANCE.len() - 1)];
    for t in traits {
        if MORE_LIKELY.contains(t) {
            p *= 2.0;
        }
        if LESS_LIKELY.contains(t) {
            p *= 0.5;
        }
    }
    p.min(1.0)
}

/// A tip's worth: ten a level, give or take a quarter (`roll` from nothing to one), and one more.
pub fn tip_amount(level: u32, roll: f64) -> i64 {
    let base = level as f64 * MONEY_PER_LEVEL;
    (base * (0.75 + 0.5 * roll)) as i64 + 1
}

/// Someone watching a player.
#[derive(Component, Debug)]
pub struct Watching {
    instrument: Entity,
    player: Entity,
    next_liking: f64,
    next_test: f64,
    song_ends: f64,
    will_tip: bool,
    /// Clips turn about (see `chess::once`).
    flip: bool,
}

/// On the way to put a tip in (its worth), and who for.
#[derive(Component, Debug)]
pub struct Tipping {
    amount: i64,
    player: Entity,
}

/// The tips set out before a player: the case or jar.
#[derive(Component)]
pub struct TipsOut(pub Entity);

/// A case or jar set out for tips, and whose.
#[derive(Component)]
pub struct TipJarOf(pub Entity);

fn once(first: &'static str, loops: &'static [&'static str], flip: &mut bool) -> ActionClip {
    let mut c = ActionClip::new(Some(first), loops);
    *flip = !*flip;
    if *flip {
        c.before = c.start.take();
    }
    c
}

/// What a Sim's doing at the front of their queue, if it's running at an object: the object
/// and the interaction's index.
fn running_at(queue: &ActionQueue) -> Option<(Entity, usize)> {
    let a = queue.0.front()?;
    match (&a.kind, a.phase) {
        (ActionKind::Object { target, def }, Phase::Running(_)) if !a.cancel => Some((*target, *def)),
        _ => None,
    }
}

/// The players and their watchers: liking, the music enjoyed, the songs applauded, and tips.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn performances(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut perf: ResMut<Performances>,
    mut sims: Query<
        (Entity, &Sim, &mut ActionQueue, &mut Transform, &Skills, &mut Relationships, Option<&mut Watching>, Option<&Tipping>, Has<HouseholdMember>, Option<&crate::nav::Floor>),
        Without<GameObject>,
    >,
    mut moodlets: Query<&mut crate::life::Moodlets>,
    objects: Query<(Entity, &GameObject, &Transform, Has<crate::visit::LotObject>), Without<Sim>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    jars: Query<&TipsOut>,
    mut household: Option<ResMut<crate::interact::Household>>,
    mut notes: ResMut<Notifications>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    // Who's playing what (and for tips), and where tips could be played for.
    let mut playing = HashMap::new();
    for (me, _, queue, _, skills, ..) in &sims {
        let Some((target, def)) = running_at(&queue) else { continue };
        let Ok((_, o, ..)) = objects.get(target) else { continue };
        let Some(d) = interactions_for(o.kind).get(def).filter(|d| instrument(o.kind) && d.special != Special::WatchMusic) else { continue };
        let level = d.skill.map_or(0, |s| skills.level(s));
        playing.insert(target, (me, level, d.special == Special::PlayForTips));
    }
    perf.playing = playing;
    perf.tips_ok = objects
        .iter()
        .filter(|(_, o, t, community)| instrument(o.kind) && (*community || !building.as_deref().is_some_and(|b| b.is_indoors(t.translation))))
        .map(|(e, ..)| e)
        .collect();
    // The watchers.
    let mut liking: Vec<(Entity, Entity)> = Vec::new();
    let mut enjoying: Vec<(Entity, u32)> = Vec::new();
    for (me, sim, mut queue, mut tf, _, _, watching, tipping, ours, floor) in &mut sims {
        let watched = running_at(&queue).filter(|(t, d)| objects.get(*t).is_ok_and(|o| interactions_for(o.1.kind).get(*d).is_some_and(|d| d.special == Special::WatchMusic)));
        let Some((instrument, _)) = watched else {
            if watching.is_some() && tipping.is_none() {
                commands.entity(me).remove::<Watching>();
            }
            continue;
        };
        // (Nothing to watch once the playing's stopped.)
        let Some(&(player, level, for_tips)) = perf.playing.get(&instrument).filter(|p| p.0 != me) else {
            if let Some(a) = queue.0.front_mut() {
                a.cancel = true;
            }
            continue;
        };
        let Ok((_, o, otf, _)) = objects.get(instrument) else { continue };
        // (Turned to the player.)
        let to = (o.use_point(otf) - tf.translation.xz()).normalize_or_zero();
        if to != Vec2::ZERO {
            tf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
        }
        let Some(mut w) = watching.filter(|w| w.instrument == instrument && w.player == player) else {
            commands.entity(me).insert(Watching {
                instrument,
                player,
                next_liking: now + MINUTES_FOR_LIKING,
                next_test: now + TIP_TEST_MINUTES,
                song_ends: now + rng.random_range(SONG_MINUTES),
                will_tip: false,
                flip: false,
            });
            continue;
        };
        if now >= w.next_liking {
            w.next_liking += MINUTES_FOR_LIKING;
            liking.push((me, player));
        }
        if level >= ENJOYING_LEVEL {
            enjoying.push((me, level));
        }
        // (Only those not of the household tip, and only for tips: outdoors or on a community
        // lot.)
        let tips = for_tips && !ours && perf.tips_ok(instrument);
        if tips && !w.will_tip && now >= w.next_test {
            w.next_test += TIP_TEST_MINUTES;
            w.will_tip = rng.random_bool(tip_chance(level, &sim.traits));
        }
        if now < w.song_ends {
            continue;
        }
        // A song's over: applauded (or not), and the tip, if they meant to give one.
        w.song_ends = now + rng.random_range(SONG_MINUTES);
        let tip_to = jars.get(player).ok().and_then(|j| objects.get(j.0).ok()).map(|j| j.2.translation);
        if w.will_tip
            && let Some(at) = tip_to
        {
            w.will_tip = false;
            let from = tf.translation.xz();
            let stand = at.xz() + (from - at.xz()).normalize_or(Vec2::Y) * TIP_DISTANCE;
            queue.0.push_front(Action::new("Tip", ActionKind::GoHere(stand, floor.map_or(1, |f| f.0)), true));
            commands.entity(me).insert(Tipping { amount: tip_amount(level, rng.random()), player });
            continue;
        }
        let clip = if rng.random_bool(0.5) { APPLAUD[rng.random_range(0..2)] } else { BOO };
        commands.entity(me).insert(once(clip, WATCHING, &mut w.flip));
    }
    // (Liked a little more for the listening.)
    for (me, player) in liking {
        for (x, y) in [(me, player), (player, me)] {
            if let Ok((.., mut rels, _, _, _, _)) = sims.get_mut(x) {
                rels.add(y, LIKING_FOR_WATCHING, 0.0);
            }
        }
    }
    for (me, level) in enjoying {
        if let Ok(mut m) = moodlets.get_mut(me)
            && !m.has(crate::life::MoodletKind::EnjoyingMusic)
        {
            let mood = ENJOYING_MOOD + ENJOYING_PER_LEVEL * (level - ENJOYING_LEVEL) as i32;
            m.add_valued(crate::life::MoodletKind::EnjoyingMusic, now, mood, ENJOYING_MINUTES);
        }
    }
    // Tippers at the tips: in it goes, to the player (their household's, if they've one).
    let mut paid: Vec<(Entity, i64, Entity)> = Vec::new();
    for (me, _, queue, _, _, _, mut watching, tipping, ..) in &mut sims {
        let Some(t) = tipping else { continue };
        let going = queue.0.front().is_some_and(|a| a.label == "Tip" && matches!(a.kind, ActionKind::GoHere(..)));
        if going {
            continue;
        }
        commands.entity(me).remove::<Tipping>();
        let mut flip = false;
        let flip = watching.as_deref_mut().map_or(&mut flip, |w| &mut w.flip);
        commands.entity(me).insert(once(TIP, WATCHING, flip));
        paid.push((me, t.amount, t.player));
    }
    for (me, amount, player) in paid {
        let (Ok(tipper), Ok(to)) = (sims.get(me), sims.get(player)) else { continue };
        if to.8
            && let Some(h) = household.as_deref_mut()
        {
            h.funds += amount;
            notes.push(format!("{} tipped {} §{amount}.", tipper.1.first, to.1.first));
        }
    }
}

/// The case or jar set out for a player playing for tips (before them, or on the instrument),
/// and put away once they stop.
#[allow(clippy::too_many_arguments)]
fn tip_jars(
    mut commands: Commands,
    perf: Res<Performances>,
    players: Query<(Entity, &Transform, Option<&TipsOut>), (With<Sim>, Without<GameObject>)>,
    jars: Query<(Entity, &TipJarOf)>,
    objects: Query<(&GameObject, &Transform), Without<Sim>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let for_tips: HashMap<Entity, Entity> = perf.playing.iter().filter(|(i, p)| p.2 && perf.tips_ok(**i)).map(|(i, p)| (p.0, *i)).collect();
    for (me, tf, out) in &players {
        match (for_tips.get(&me), out) {
            (Some(&inst), None) => {
                let Ok((o, otf)) = objects.get(inst) else { continue };
                let ahead = tf.rotation * Vec3::Z;
                let right = tf.rotation * Vec3::X;
                // (The guitar's case on the ground before them; a jar on the instrument, or on
                // the ground beside a set of drums or a harp.)
                let (name, at) = match o.kind {
                    ObjectKind::Guitar => (GUITAR_CASE, tf.translation + ahead * 0.9 - right * 0.35),
                    ObjectKind::Piano | ObjectKind::Keyboard => (TIP_JAR, otf.transform_point(Vec3::new(o.center.x + o.half.x * 0.6, o.height, o.center.y))),
                    _ => (TIP_JAR, tf.translation + right * 0.8),
                };
                let yaw = tf.rotation.to_euler(EulerRot::YXZ).0 + std::f32::consts::PI;
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                if let Some(jar) = crate::meals::spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, name, ObjectKind::Decoration, "Tips", at, yaw) {
                    commands.entity(jar).insert(TipJarOf(me));
                    commands.entity(me).insert(TipsOut(jar));
                }
            }
            (None, Some(_)) => {
                commands.entity(me).remove::<TipsOut>();
            }
            _ => {}
        }
    }
    // (Put away once they've stopped, or gone.)
    for (jar, of) in &jars {
        if !for_tips.contains_key(&of.0) {
            commands.entity(jar).try_despawn();
        }
    }
}

/// TIPS=1 (tests): the selected Sim plays for tips (as good as level eight), and someone not of
/// the household about comes to watch, in want of some fun.
#[allow(clippy::type_complexity)]
fn tip_test(
    (mut done, mut asked): (Local<bool>, Local<bool>),
    perf: Res<Performances>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut crate::sim::Motives), (Without<HouseholdMember>, Without<crate::sim::Selected>)>,
    mut selected: Query<(Entity, &mut ActionQueue), With<crate::sim::Selected>>,
    objects: Query<(Entity, &GameObject)>,
    mut skills: Query<&mut Skills>,
) {
    if *done || std::env::var("TIPS").is_err() {
        return;
    }
    let Ok((me, mut queue)) = selected.single_mut() else { return };
    if !*asked {
        let Some((inst, def)) = objects
            .iter()
            .filter(|(e, o)| instrument(o.kind) && perf.tips_ok(*e))
            .find_map(|(e, o)| Some((e, interactions_for(o.kind).iter().position(|d| d.special == Special::PlayForTips)?)))
        else {
            return;
        };
        queue.0.clear();
        queue.push_player(Action::new(PLAY_FOR_TIPS, ActionKind::Object { target: inst, def }, false));
        if let Ok(mut s) = skills.get_mut(me) {
            for sk in ["Guitar", "Piano", "Drums", "LaserHarp"] {
                s.0.insert(sk, 8.0);
            }
        }
        info!("tips test: the selected Sim plays for tips");
        *asked = true;
        return;
    }
    let Some((&inst, &(player, ..))) = perf.playing.iter().find(|(i, p)| p.0 == me && p.2 && perf.tips_ok(**i)) else { return };
    let Some(def) = [ObjectKind::Guitar, ObjectKind::Piano, ObjectKind::Keyboard, ObjectKind::Drums, ObjectKind::LaserHarp]
        .iter()
        .find_map(|k| interactions_for(*k).iter().position(|d| d.special == Special::WatchMusic))
    else {
        return;
    };
    // (A watcher in want of some fun.)
    for (e, sim, mut q, mut motives) in &mut sims {
        if e == player || sim.age.is_little() {
            continue;
        }
        motives.0[crate::sim::FUN] = -60.0;
        q.0.clear();
        q.push_player(Action::new("Watch", ActionKind::Object { target: inst, def }, false));
        info!("tips test: {} watches", sim.first);
        *done = true;
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tips_come_from_good_players_and_generous_watchers() {
        assert_eq!(tip_chance(4, &[]), 0.0);
        assert_eq!(tip_chance(10, &[]), 0.3);
        assert_eq!(tip_chance(6, &[Trait::Good]), 0.2);
        assert_eq!(tip_chance(6, &[Trait::Snob, Trait::Frugal]), 0.025);
        assert_eq!((tip_amount(8, 0.0), tip_amount(8, 0.5), tip_amount(8, 1.0)), (61, 81, 101));
    }
}
