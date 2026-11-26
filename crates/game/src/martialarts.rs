//! Martial Arts, World Adventures' skill (`MartialArts`), at the training dummy (`TrainingDummy`,
//! the Sheng Hai Training Dummy): teens and older "Practice" on it (Martial Arts, Fun 45 and
//! Hygiene -40 an hour, and Athletic as they go: `kAthleticSkillGainRateDuringPractice`), into the
//! stance (`a_neutral2MA`) and at it as their skill has them (the beginner's loops below level
//! four, the intermediate's below seven, the expert's after: `kSkillLevelForIntermediate`,
//! `kSkillLevelForExpert`), the dummy spinning to their blows. Every five minutes
//! (`kMinutesPerFailureDiceRoll`) a blow may go wrong, more likely the less skilled and the worse
//! their mood (`kChanceOfFailureAtMinMaxSkillLevel` and `...MoodLevel`, nine in ten down to one in
//! ten, half each: `kFailureChanceSkillWeight`), and they take a tumble (`epicFail`) before going
//! on. They practise in their karate gi (`ChangeIntoKarateGi`, a spin into it), its belt their
//! level's (`amMartialArts01`...: the `Skills` table's outfits, none at five or seven, where the
//! last is kept; workout clothes before they've a level); a level with a new belt shows it off
//! when they're done ("Through tireless training and determination, ... has earned a new belt!",
//! `CheckOutMyAwesomeBelt`'s `a_react_newMartialArtsBelt`). The Disciplined have fun of it
//! (`kDisciplinedTraitMartialArtsFun`), it shapes them up as a workout does, and the Fatigued
//! can't (greyed: "... is too fatigued to practice").

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Skills};
use crate::sim::{Age, Sim};

pub struct MartialArtsPlugin;

impl Plugin for MartialArtsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (practice, show_belt, dummy_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)));
    }
}

/// Showing off a new belt (its clip begun).
#[derive(Component)]
struct ShowingBelt;

/// A new belt shown off, its turn come: the game's `a_react_newMartialArtsBelt`.
fn show_belt(mut commands: Commands, sims: Query<(Entity, &ActionQueue, Has<ShowingBelt>, &crate::simbody::Wearing)>, others: Query<Entity, (With<ShowingBelt>, Without<crate::simbody::Wearing>)>) {
    for (e, q, showing, wearing) in &sims {
        let now = q.0.front().is_some_and(|a| matches!(a.phase, Phase::Running(_)) && matches!(a.kind, ActionKind::Outro { clips, .. } if clips.first() == Some(&NEW_BELT)));
        // (In the gi first: `CheckOutMyAwesomeBelt`'s spin into it.)
        if now && !showing && wearing.0 == crate::simbody::OutfitKind::MartialArts {
            commands.entity(e).insert((ShowingBelt, ActionClip::new(Some(NEW_BELT), &[])));
        } else if !now && showing {
            commands.entity(e).remove::<ShowingBelt>();
        }
    }
    for e in &others {
        commands.entity(e).remove::<ShowingBelt>();
    }
}

/// The game's tuning.
const INTERMEDIATE: f32 = 4.0;
const EXPERT: f32 = 7.0;
const ROLL_EVERY: f64 = 5.0;
const FAIL_AT_SKILL: (f64, f64) = (0.9, 0.1);
const FAIL_AT_MOOD: (f64, f64) = (0.9, 0.1);
const SKILL_WEIGHT: f64 = 0.5;
const ATHLETIC_PER_HOUR: f32 = 180.0;
const SKILL: &str = "MartialArts";
const STANCE_IN: &str = "a_neutral2MA_x";
const STANCE_OUT: &str = "a_MA2neutral_x";
const TUMBLE: &str = "a2o_trainingDummy_epicFail_x";
/// A new belt shown off, and the game's words for it.
pub const NEW_BELT: &str = "a_react_newMartialArtsBelt_x";
pub const CHECK_OUT_BELT: &str = "Check Out My Awesome Belt";
/// The Disciplined's fun of it an hour (the XML's, over the script's 50); a workout's shaping
/// up an hour (as `interact`'s).
const DISCIPLINED_FUN: f32 = 20.0;
const FITNESS_PER_HOUR: f32 = 0.05;
const WEIGHT_PER_HOUR: f32 = -0.03;

/// The karate gi a martial artist wears at a level (the last level's at or below theirs with
/// one), by age and sex: none for children, nor before a level's earned.
pub fn gi(sim: &Sim, level: f32) -> Option<String> {
    let age = match sim.age {
        Age::Teen => "t",
        Age::YoungAdult | Age::Adult => "a",
        Age::Elder => "e",
        _ => return None,
    };
    let l = gi_level(level)?;
    Some(format!("{age}{}martialarts{l:02}", if sim.female { "f" } else { "m" }))
}

/// The level whose belt is worn at a level.
pub fn gi_level(level: f32) -> Option<u8> {
    s3bake::gamedata::GI_LEVELS.iter().rev().copied().find(|l| *l as f32 <= level.floor())
}

/// Why they can't practise, worn out (`Practice:FatiguedToolTip`).
pub fn too_fatigued(first: &str) -> String {
    format!("{first} is too fatigued to practice")
}

/// The loops for a Martial Arts level: the beginner's, intermediate's or expert's.
pub fn loops(level: f32) -> &'static [&'static str] {
    if level >= EXPERT {
        &["a2o_trainingDummy_exp_loop"]
    } else if level >= INTERMEDIATE {
        &["a2o_trainingDummy_med_loop"]
    } else {
        &["a2o_trainingDummy_beg_loop"]
    }
}

/// How likely a blow is to go wrong (`ShouldFail`): by their skill (0 to 10) and mood (-100 to
/// 100), half each.
pub fn fail_chance(level: f32, mood: f32) -> f64 {
    let lerp = |(a, b): (f64, f64), t: f64| a + (b - a) * t.clamp(0.0, 1.0);
    let by_skill = lerp(FAIL_AT_SKILL, level as f64 / 10.0);
    let by_mood = lerp(FAIL_AT_MOOD, (mood as f64 + 100.0) / 200.0);
    by_skill * SKILL_WEIGHT + by_mood * (1.0 - SKILL_WEIGHT)
}

/// At the dummy: the loops they're at (by their level), when the next roll is, a tumble's end,
/// and the belt they've on.
#[derive(Component)]
struct Practising {
    band: &'static [&'static str],
    next_roll: f64,
    tumbling: Option<f32>,
    belt: Option<u8>,
}

fn clip(start: &'static str, band: &'static [&'static str]) -> ActionClip {
    ActionClip::new(Some(start), band).ending(&[STANCE_OUT], 1.5)
}

/// Practice at a training dummy: the loops their skill has them at, Athletic with it, and a
/// tumble now and then.
#[allow(clippy::type_complexity)]
fn practice(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Skills, Option<&crate::life::Mood>, Option<&mut Practising>, &mut crate::sim::Motives)>,
    objects: Query<&GameObject>,
    (mut notes, mut did): (ResMut<crate::interact::Notifications>, MessageWriter<crate::journal::Did>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, sim, mut q, mut skills, mood, practising, mut motives) in &mut sims {
        let at_dummy = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).is_some_and(|a| match a.kind {
            ActionKind::Object { target, .. } => objects.get(target).is_ok_and(|o| o.kind == ObjectKind::TrainingDummy),
            _ => false,
        });
        if !at_dummy {
            if practising.is_some() {
                commands.entity(e).remove::<Practising>();
            }
            continue;
        }
        let level = skills.0.get(SKILL).copied().unwrap_or(0.0);
        let band = loops(level);
        // (Athletic as they go; the Disciplined's fun of it; shaping up, as a workout.)
        if hours > 0.0 {
            let table = crate::interact::skill_points("Athletic");
            let was = skills.0.get("Athletic").copied().unwrap_or(0.0);
            let to = crate::interact::points_to_level(table, crate::interact::level_to_points(table, was) + ATHLETIC_PER_HOUR * hours).min(10.0);
            skills.0.insert("Athletic", to);
            if sim.traits.contains(&crate::life::Trait::Disciplined) {
                motives.add(crate::sim::FUN, DISCIPLINED_FUN * hours);
            }
            commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| crate::aging::reshape(&mut w, WEIGHT_PER_HOUR * hours, FITNESS_PER_HOUR * hours));
            did.write(crate::journal::Did::count(e, crate::journal::Stat::CardioHours, hours as f64));
        }
        let belt = gi_level(level);
        let Some(mut p) = practising else {
            commands.entity(e).insert((Practising { band, next_roll: now + ROLL_EVERY, tumbling: None, belt }, clip(STANCE_IN, band)));
            info!("martial arts: {} practises at level {level:.1}", sim.first);
            continue;
        };
        // (A level with a new belt: into it, and shown off once they're done.)
        if belt > p.belt && gi(sim, level).is_some() {
            p.belt = belt;
            commands.entity(e).insert(crate::aging::NeedsNewBody);
            let secs = lib.get(&data, NEW_BELT).map_or(4.0, |c| c.duration);
            let at = 1.min(q.0.len());
            q.0.insert(at, Action::new(CHECK_OUT_BELT, ActionKind::Outro { clips: &[NEW_BELT], then: None, secs, stand_at: None, target: e }, false));
            notes.push(format!("Through tireless training and determination, {} has earned a new belt!", sim.full_name()));
            info!("martial arts: {} earns a new belt (level {level:.1})", sim.first);
        }
        if let Some(t) = p.tumbling.as_mut() {
            *t -= dt;
            if *t <= 0.0 {
                p.tumbling = None;
            }
            continue;
        }
        // (A level gained: the loops for it.)
        if p.band != band {
            p.band = band;
            commands.entity(e).insert(ActionClip::new(None, band).ending(&[STANCE_OUT], 1.5));
        }
        if now >= p.next_roll {
            p.next_roll = now + ROLL_EVERY;
            let mood = mood.map_or(0.0, |m| m.0);
            if std::env::var_os("DUMMY_NO_FAIL").is_none() && rng.random_bool(fail_chance(level, mood)) {
                p.tumbling = Some(lib.get(&data, TUMBLE).map_or(4.0, |c| c.duration));
                // (Up again and back into the stance: the game's practice begun anew.)
                commands.entity(e).insert(ActionClip::steps(TUMBLE, &[STANCE_IN], band).ending(&[STANCE_OUT], 1.5));
                info!("martial arts: {} takes a tumble", sim.first);
            }
        }
    }
}

/// `DUMMY_TEST=1` (tests): ten seconds in, a training dummy put down near the selected Sim (or
/// `DUMMY_WHO=<first name>`), who practises on it (at `DUMMY_LEVEL=<n>` Martial Arts).
#[allow(clippy::type_complexity)]
fn dummy_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(&Sim, &Transform, &mut ActionQueue, &mut Skills, Has<crate::sim::Selected>), With<crate::sim::HouseholdMember>>,
    dummies: Query<(Entity, &GameObject, &Transform), Without<Sim>>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    if std::env::var_os("DUMMY_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let who = env("DUMMY_WHO");
    let is_me = |s: &Sim, sel: bool| who.as_ref().map_or(sel, |w| s.first.eq_ignore_ascii_case(w));
    let dummy = dummies.iter().find(|d| d.1.kind == ObjectKind::TrainingDummy);
    if let (Ok(mut c), Some((_, _, dtf))) = (cams.single_mut(), dummy) {
        let f = |k: &str, d: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(dtf.translation);
        c.height_offset = f("DUMMY_LOOK_Y", 0.9);
        c.distance = f("DUMMY_DIST", 4.5);
        c.pitch = f("DUMMY_PITCH", 0.3);
        let fwd = dtf.rotation * Vec3::Z;
        c.yaw = fwd.x.atan2(fwd.z) + f("DUMMY_YAW", 1.2);
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let Some((_, tf, ..)) = sims.iter().find(|s| is_me(s.0, s.4)) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("TrainingDummy")).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = tf.translation.xz();
            let spot = (3..16).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32 * 0.5)).find(|c| (-3..=3).all(|i| (-3..=3).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.4)))).unwrap_or(here + Vec2::X * 2.0);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, Vec3::new(spot.x, tf.translation.y, spot.y), 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("dummy test: put down");
            }
        }
        1 if now - stage.1 > 2.0 => {
            let Some((d, ..)) = dummy else { return };
            *stage = (2, now);
            let level = env("DUMMY_LEVEL").and_then(|v| v.parse::<f32>().ok());
            if let Some((sim, _, mut q, mut skills, _)) = sims.iter_mut().find(|s| is_me(s.0, s.4)) {
                if let Some(l) = level {
                    skills.0.insert(SKILL, l);
                }
                q.push_player(crate::interact::Action::new("Practice", ActionKind::Object { target: d, def: 0 }, false));
                info!("dummy test: {} practises", sim.first);
            }
        }
        // (DUMMY_BELT=<seconds>: that long in, up a level to a new belt; DUMMY_STOP=<seconds>:
        // told to stop then.)
        2 => {
            let after = |k: &str| env(k).and_then(|v| v.parse::<f32>().ok()).is_some_and(|s| now - stage.1 > s);
            if after("DUMMY_BELT")
                && let Some((sim, _, _, mut skills, _)) = sims.iter_mut().find(|s| is_me(s.0, s.4))
            {
                let l = skills.0.get(SKILL).copied().unwrap_or(0.0).floor() + 1.0;
                skills.0.insert(SKILL, l);
                info!("dummy test: {} up to level {l}", sim.first);
                *stage = (3, stage.1);
            }
        }
        3 if env("DUMMY_STOP").and_then(|v| v.parse::<f32>().ok()).is_some_and(|s| now - stage.1 > s) => {
            if let Some((sim, _, mut q, ..)) = sims.iter_mut().find(|s| is_me(s.0, s.4))
                && let Some(a) = q.0.front_mut()
            {
                a.cancel = true;
                info!("dummy test: {} told to stop", sim.first);
            }
            *stage = (4, now);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn practice_goes_as_the_game_tunes_it() {
        assert_eq!(loops(0.0), &["a2o_trainingDummy_beg_loop"]);
        assert_eq!(loops(4.0), &["a2o_trainingDummy_med_loop"]);
        assert_eq!(loops(9.5), &["a2o_trainingDummy_exp_loop"]);
        assert!((fail_chance(0.0, -100.0) - 0.9).abs() < 1e-9);
        assert!((fail_chance(10.0, 100.0) - 0.1).abs() < 1e-9);
        assert!((fail_chance(5.0, 0.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_gi_and_its_belt_are_the_levels() {
        let sim = |age: Age, female: bool| crate::sim::random_sim(&mut rand::rng(), "M", Some(female), age);
        assert_eq!(gi(&sim(Age::Adult, false), 0.9), None, "no belt before level one");
        assert_eq!(gi(&sim(Age::Adult, false), 1.0).as_deref(), Some("ammartialarts01"));
        assert_eq!(gi(&sim(Age::Teen, true), 5.5).as_deref(), Some("tfmartialarts04"), "five keeps four's");
        assert_eq!(gi(&sim(Age::Elder, true), 7.2).as_deref(), Some("efmartialarts06"));
        assert_eq!(gi(&sim(Age::YoungAdult, false), 10.0).as_deref(), Some("ammartialarts10"));
        assert_eq!(gi(&sim(Age::Child, false), 10.0), None);
        assert!(gi_level(8.0) > gi_level(7.9) && gi_level(5.0) == gi_level(4.0));
        assert_eq!(s3bake::gamedata::martial_arts_gis().len(), 48);
        assert_eq!(too_fatigued("Jack"), "Jack is too fatigued to practice");
    }
}
