//! The magic gnomes' night-time trickery, as the game's (`MagicGnomeBase.EnactTrickery`): once
//! a night, at a time of its own between nine and four (`kTrickeryStartHour`,
//! `kTrickeryEndHour`, the XML's), a magic gnome on the household's lot vanishes in a puff of
//! sparkles (`ep2GnomePoof`) and turns up somewhere else, as the game's weights have it
//! (`kChance...`): off to some other thing on the lot (`Move`), likelier outdoors, by a plant or by
//! something dear (`GetGnomeDesirability`: `kWeightOutside`, `kWeightPlant`, `kWeightExpensive` over
//! `kExpensiveThreshold`, less where another gnome's fond of it); up onto a table or a counter
//! (`MoveToSlot`); by its favourite thing on the lot (`VisitFavoriteObject`); cleaning the
//! dirtiest thing there, or mopping a puddle (`Clean`); switching the television on, or the stereo
//! (`UseObject`, `kChanceUseTV`); or just turned about (`ChangePose`). One kicked over is stood up
//! again. Put off a quarter of an hour while someone's at it (`kTrickeryRescheduleHours`).

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind};

pub struct GnomeTricksPlugin;

impl Plugin for GnomeTricksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tricks_test, tricks).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The game's tuning (the XML's over the script's).
const START_HOUR: f64 = 21.0;
const END_HOUR: f64 = 4.0;
const RETRY: f64 = 15.0;
const CHANCE_POSE: f32 = 0.1;
const CHANCE_MOVE: f32 = 0.3;
const CHANCE_SLOT: f32 = 0.2;
const CHANCE_FAVORITE: f32 = 0.2;
const CHANCE_CLEAN: f32 = 0.1;
const CHANCE_USE: f32 = 0.1;
const CHANCE_TV: f64 = 0.6;
const WEIGHT_OUTSIDE: f32 = 3.0;
const WEIGHT_PLANT: f32 = 3.0;
const WEIGHT_EXPENSIVE: f32 = 2.0;
const EXPENSIVE: i32 = 1500;
const WEIGHT_OTHER_GNOME: f32 = 1.0;

/// What a gnome gets up to (`MagicGnomePlan`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plan {
    ChangePose,
    Move,
    MoveToSlot,
    VisitFavorite,
    Clean,
    UseObject,
}

impl Plan {
    const WEIGHTED: [(Plan, f32); 6] =
        [(Plan::ChangePose, CHANCE_POSE), (Plan::Move, CHANCE_MOVE), (Plan::MoveToSlot, CHANCE_SLOT), (Plan::VisitFavorite, CHANCE_FAVORITE), (Plan::Clean, CHANCE_CLEAN), (Plan::UseObject, CHANCE_USE)];

    /// One by the game's weights, by a roll from 0 to 1.
    pub fn pick(roll: f32) -> Plan {
        let total: f32 = Self::WEIGHTED.iter().map(|w| w.1).sum();
        let mut at = roll * total;
        for (p, w) in Self::WEIGHTED {
            if at < w {
                return p;
            }
            at -= w;
        }
        Plan::Move
    }
}

/// When a gnome's next up to something (game minutes): at its moment (a roll from 0 to 1 through
/// the night's hours) of the first night after `after`.
pub fn next_trick(after: f64, roll: f32) -> f64 {
    let start = START_HOUR * 60.0;
    let len = (END_HOUR + 24.0 - START_HOUR) * 60.0;
    let at = start + roll as f64 * len;
    let mut day = ((after - at) / 1440.0).floor();
    loop {
        let t = day * 1440.0 + at;
        if t > after {
            return t;
        }
        day += 1.0;
    }
}

/// Whether it's still the night's trickery hours.
fn night(minutes: f64) -> bool {
    let h = (minutes / 60.0).rem_euclid(24.0);
    h >= START_HOUR || h < END_HOUR
}

/// How much a gnome cares for a thing (`GetGnomeDesirability`).
pub fn desirability(outside: bool, plant: bool, price: i32, others_fond: usize) -> f32 {
    let mut w = 0.0;
    if outside {
        w += WEIGHT_OUTSIDE;
    }
    if plant {
        w += WEIGHT_PLANT;
    }
    if price > EXPENSIVE {
        w += WEIGHT_EXPENSIVE;
    }
    w - WEIGHT_OTHER_GNOME * others_fond as f32
}

/// A magic gnome's nights: when it's next up to something, and its favourite thing.
#[derive(Component)]
pub struct Trickery {
    pub at: f64,
    favorite: Option<Entity>,
}

/// Whether a catalogue script is a magic gnome's.
fn magic(script: &str) -> bool {
    crate::ornaments::Ornament::of(script) == crate::ornaments::Ornament::MagicGnome
}

/// The gnomes' tricks, night by night.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn tricks(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    data: Res<crate::baked::Baked>,
    mut gnomes: Query<(Entity, &GameObject, &mut Transform, Option<&mut Trickery>, Option<&crate::interact::UsedBy>), (Without<crate::visit::LotObject>, Without<crate::sim::Sim>)>,
    others: Query<(Entity, &GameObject, &GlobalTransform, Option<&crate::grime::Grime>), (Without<crate::visit::LotObject>, Without<crate::sim::Sim>)>,
    sims: Query<&ActionQueue, With<crate::sim::Sim>>,
    (building, grid): (Option<Res<crate::building::ActiveBuilding>>, Option<Res<crate::nav::NavGrid>>),
    mut sparkles: MessageWriter<crate::effects::SpinSparkle>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    let force = std::env::var("GNOME_TRICK").ok();
    // (Who's at a gnome: kicking it, standing it up.)
    let busy: Vec<Entity> = sims
        .iter()
        .filter_map(|q| match q.0.front().map(|a| &a.kind) {
            Some(ActionKind::Ornament { thing, .. }) => Some(*thing),
            Some(ActionKind::Object { target, .. }) => Some(*target),
            _ => None,
        })
        .collect();
    let script_of = |o: &GameObject| data.0.catalog.iter().find(|c| c.objd == o.objd).map(|c| c.script.as_str()).unwrap_or("");
    let favorites: Vec<Entity> = gnomes.iter().filter_map(|g| g.3.and_then(|t| t.favorite)).collect();
    for (e, obj, mut tf, trick, used) in &mut gnomes {
        if obj.kind != ObjectKind::LawnOrnament || !magic(script_of(obj)) {
            continue;
        }
        let Some(mut t) = trick else {
            commands.entity(e).insert(Trickery { at: next_trick(now, rng.random()), favorite: None });
            continue;
        };
        if now < t.at {
            continue;
        }
        // (Someone at it: a quarter of an hour on, or tomorrow night.)
        if busy.contains(&e) || used.is_some_and(|u| u.0.is_some()) {
            t.at = if night(now + RETRY) { now + RETRY } else { next_trick(now + 8.0 * 60.0, rng.random()) };
            continue;
        }
        t.at = next_trick(now + 8.0 * 60.0, rng.random());
        let here = tf.translation;
        let plan = match force.as_deref() {
            Some("move") => Plan::Move,
            Some("slot") => Plan::MoveToSlot,
            Some("favorite") => Plan::VisitFavorite,
            Some("clean") => Plan::Clean,
            Some("tv") | Some("stereo") => Plan::UseObject,
            Some("pose") => Plan::ChangePose,
            _ => Plan::pick(rng.random()),
        };
        // The things about the lot it might go to, and how much it cares for each.
        let things: Vec<(Entity, &GameObject, Vec3, f32)> = others
            .iter()
            .filter(|(o, g, gt, _)| *o != e && gt.translation().distance(here) < 60.0 && !matches!(g.kind, ObjectKind::Door | ObjectKind::Light) && !g.name.is_empty())
            .map(|(o, g, gt, _)| {
                let p = gt.translation();
                let outside = building.as_deref().is_none_or(|b| !b.is_indoors(p));
                let fond = favorites.iter().filter(|f| **f == o).count();
                (o, g, p, desirability(outside, matches!(g.kind, ObjectKind::Plant | ObjectKind::GardenPlant), g.price, fond))
            })
            .collect();
        let weighted = |rng: &mut rand::rngs::ThreadRng| -> Option<(Entity, Vec3)> {
            let total: f32 = things.iter().map(|t| t.3.max(0.0) + 0.25).sum();
            let mut at = rng.random_range(0.0..total.max(0.001));
            for t in &things {
                let w = t.3.max(0.0) + 0.25;
                if at < w {
                    return Some((t.0, t.2));
                }
                at -= w;
            }
            None
        };
        let near = |p: Vec3, rng: &mut rand::rngs::ThreadRng| -> Option<Vec3> {
            let free = |q: Vec2| grid.as_deref().and_then(|g| g.cell_of(q).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            (0..24).find_map(|_| {
                let a = rng.random_range(0.0..std::f32::consts::TAU);
                let q = p.xz() + Vec2::from_angle(a) * rng.random_range(0.6..1.4);
                free(q).then(|| Vec3::new(q.x, p.y, q.y))
            })
        };
        let mut to: Option<(Vec3, Vec3)> = None;
        let mut plan = plan;
        // (Each falls back to a move when there's nothing for it.)
        for _ in 0..2 {
            match plan {
                Plan::ChangePose => to = Some((here, here + Vec3::new(rng.random_range(-1.0..1.0), 0.0, rng.random_range(-1.0..1.0)))),
                Plan::Move => to = weighted(&mut rng).and_then(|(_, p)| near(p, &mut rng).map(|q| (q, p))),
                Plan::MoveToSlot => {
                    let surfaces: Vec<&(Entity, &GameObject, Vec3, f32)> = things.iter().filter(|t| t.1.kind.category() == "Surfaces" && t.1.height > 0.3 && t.1.height < 1.3).collect();
                    if surfaces.is_empty() {
                        plan = Plan::Move;
                        continue;
                    }
                    let s = surfaces[rng.random_range(0..surfaces.len())];
                    let top = s.2 + Vec3::Y * s.1.height;
                    to = Some((top, top + Vec3::new(rng.random_range(-1.0..1.0), 0.0, rng.random_range(-1.0..1.0))));
                }
                Plan::VisitFavorite => {
                    if t.favorite.is_none_or(|f| !things.iter().any(|x| x.0 == f)) {
                        t.favorite = weighted(&mut rng).map(|x| x.0);
                    }
                    match t.favorite.and_then(|f| things.iter().find(|x| x.0 == f)) {
                        Some(f) => to = near(f.2, &mut rng).map(|q| (q, f.2)),
                        None => {
                            plan = Plan::Move;
                            continue;
                        }
                    }
                }
                Plan::Clean => {
                    let dirty = others.iter().filter(|o| o.3.is_some_and(|g| g.0 > 0.0) && o.2.translation().distance(here) < 60.0).max_by(|a, b| a.3.map_or(0.0, |g| g.0).total_cmp(&b.3.map_or(0.0, |g| g.0)));
                    let puddle = things.iter().find(|x| x.1.kind == ObjectKind::Puddle);
                    if let Some((d, _, gt, _)) = dirty {
                        commands.entity(d).insert(crate::grime::Grime(0.0));
                        to = near(gt.translation(), &mut rng).map(|q| (q, gt.translation()));
                    } else if let Some(p) = puddle {
                        commands.entity(p.0).try_despawn();
                        to = Some((p.2, p.2 + Vec3::X));
                    } else {
                        plan = Plan::Move;
                        continue;
                    }
                }
                Plan::UseObject => {
                    let tv = force.as_deref() != Some("stereo") && (force.as_deref() == Some("tv") || rng.random_bool(CHANCE_TV));
                    let kind = if tv { ObjectKind::Tv } else { ObjectKind::Stereo };
                    let found: Vec<&(Entity, &GameObject, Vec3, f32)> = things.iter().filter(|x| x.1.kind == kind).collect();
                    if found.is_empty() {
                        plan = Plan::Move;
                        continue;
                    }
                    let x = found[rng.random_range(0..found.len())];
                    if tv {
                        commands.entity(x.0).insert(crate::effects::TvLeftOn);
                    } else {
                        commands.entity(x.0).insert(crate::music::StereoOn(None));
                    }
                    to = near(x.2, &mut rng).map(|q| (q, x.2));
                }
            }
            break;
        }
        let Some((at, look)) = to else {
            info!("gnome tricks: {} found nowhere to go ({plan:?})", obj.name);
            continue;
        };
        // Gone in a puff of sparkles, and there.
        sparkles.write(crate::effects::SpinSparkle { sim: e, height: 0.7, at: Some(here), spread: 0.5 });
        sparkles.write(crate::effects::SpinSparkle { sim: e, height: 0.7, at: Some(at), spread: 0.5 });
        let d = (look - at).xz();
        tf.translation = at;
        if d.length() > 0.01 {
            tf.rotation = Quat::from_rotation_y(d.x.atan2(d.y));
        }
        crate::ornaments::stand_up(&mut commands, e);
        info!("gnome tricks: {} {plan:?} to {at:.1}", obj.name);
    }
}

/// `GNOME_TEST=1` (tests): ten seconds in, a Mysterious Mr. Gnome put down by the selected Sim,
/// the camera on it, up to something five game minutes on (`GNOME_TRICK=move|slot|favorite|
/// clean|tv|stereo|pose`: that).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn tricks_test(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    sims: Query<&Transform, With<crate::sim::Selected>>,
    gnomes: Query<(&GameObject, &GlobalTransform), With<Trickery>>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut done: Local<bool>,
) {
    if std::env::var_os("GNOME_TEST").is_none() || time.elapsed_secs() < 10.0 {
        return;
    }
    if let (Ok(mut c), Some((_, g))) = (cams.single_mut(), gnomes.iter().next()) {
        c.look_at(g.translation());
        c.distance = 7.0;
        c.pitch = 0.6;
    }
    if *done {
        return;
    }
    *done = true;
    let Ok(tf) = sims.single() else { return };
    let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("MagicGnome")).map(|c| c.objd) else { return };
    let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
    let here = tf.translation.xz();
    let spot = (2..12).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32 * 0.5)).find(|c| free(*c)).unwrap_or(here + Vec2::X);
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, Vec3::new(spot.x, tf.translation.y, spot.y), 0.0) {
        commands.entity(o.entity).insert((crate::save::Bought, Trickery { at: clock.minutes + 5.0, favorite: None }));
        info!("gnome test: put down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gnome_is_up_to_something_once_a_night() {
        // (From midday: tonight, between nine and four.)
        let t = next_trick(12.0 * 60.0, 0.0);
        assert!((t - 21.0 * 60.0).abs() < 1e-6);
        let t = next_trick(12.0 * 60.0, 1.0);
        assert!((t - 28.0 * 60.0).abs() < 1e-6);
        // (At half past nine already: the same moment tomorrow night if it's passed.)
        assert!((next_trick(21.5 * 60.0, 0.0) - (24.0 + 21.0) * 60.0).abs() < 1e-6);
        assert!(night(23.0 * 60.0) && night(27.5 * 60.0) && !night(12.0 * 60.0));
    }

    #[test]
    fn it_goes_where_the_game_would_have_it() {
        assert_eq!(Plan::pick(0.0), Plan::ChangePose);
        assert_eq!(Plan::pick(0.15), Plan::Move);
        assert_eq!(Plan::pick(0.45), Plan::MoveToSlot);
        assert_eq!(Plan::pick(0.99), Plan::UseObject);
        assert_eq!(desirability(true, true, 2000, 0), 8.0);
        assert_eq!(desirability(false, false, 100, 2), -2.0);
    }
}
