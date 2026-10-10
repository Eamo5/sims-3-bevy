//! Paintings, as the game's: at an easel a Sim paints on a small, medium or large canvas, and
//! what they paint is one of the game's own pictures, chosen from the Painting skill's picture
//! table by the canvas, their skill and their traits (an evil Sim's version of it, a gloomy
//! one's), now and then a brilliant painting or a masterpiece. Finished paintings go in the
//! painter's inventory, pictured, to sell or to hang on a wall (Buy mode holds them up to it;
//! hung, they can be moved or sold like the furniture). While a Sim paints, the canvas stands on
//! the easel, blank at first and then with the picture coming.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use s3bake::Key;
use s3bake::gamedata::{PaintingInfo, PaintingsBaked};

use crate::interact::{ActionKind, ActionQueue, GameObject, Phase, Skills, Special};
use crate::inventory::Stack;
use crate::life::Trait;
use crate::objects::{AssetCtx, ObjectAssets, spawn_parts};

pub struct PaintingsPlugin;

impl Plugin for PaintingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaintingImages>().add_systems(Update, easel_canvases.run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The canvases, how long a painting on each takes (minutes) and how its worth goes with its
/// size.
pub const CANVASES: [&str; 3] = ["Small Canvas", "Medium Canvas", "Large Canvas"];
pub const CANVAS_MINUTES: [f32; 3] = [180.0, 300.0, 480.0];
const CANVAS_WORTH: [f32; 3] = [1.0, 1.6, 2.5];

/// Easel's canvas times and kDabbleModifier for children; Brushmasters take half as long.
pub fn painting_minutes(size: u8, child: bool, journal: Option<&crate::journal::SkillJournal>) -> f32 {
    CANVAS_MINUTES[size.min(2) as usize]
        * if child { 1.5 } else { 1.0 }
        * if crate::journal::earned(journal, "Brushmaster") { 0.5 } else { 1.0 }
}

/// The canvas a Sim's been asked to paint on (the pie menu's choice), and what they're painting
/// on it once they've started.
#[derive(Component, Clone)]
pub struct PaintPlan {
    pub size: u8,
    pub painted: Option<Painted>,
}

impl PaintPlan {
    pub fn new(size: u8) -> Self {
        Self { size, painted: None }
    }
}

/// A painting hung on a wall: the inventory item it is (its picture, name and worth).
#[derive(Component, Clone, Debug)]
pub struct Hung(pub Stack);

/// The canvas a Sim paints on: the one asked for, else (painting of their own accord) a small
/// one while they're learning and a medium one after.
pub fn canvas(plan: Option<&PaintPlan>, level: u32) -> u8 {
    plan.map_or(if level < 4 { 0 } else { 1 }, |p| p.size.min(2))
}

/// The table's trait column for a trait with versions of the pictures (`Grumpy` Sims paint
/// the gloomy ones).
fn trait_column(t: Trait) -> Option<&'static str> {
    Some(match t {
        Trait::Artistic => "Artistic",
        Trait::CantStandArt => "CantStandArt",
        Trait::ComputerWhiz => "ComputerWhiz",
        Trait::Evil => "Evil",
        Trait::Genius => "Genius",
        Trait::Grumpy => "Grumpy",
        Trait::Insane => "Insane",
        Trait::Neurotic => "Neurotic",
        Trait::Virtuoso => "Virtuoso",
        _ => return None,
    })
}

/// A finished painting.
#[derive(Clone)]
pub struct Painted {
    /// The inventory item's key (its canvas and picture) and name.
    pub key: String,
    pub name: &'static str,
    pub worth: i64,
}

/// What a Sim paints on a canvas: a picture for their skill (a child's a dabble), in their
/// traits' version of it if it has one; brilliant paintings and masterpieces come to the
/// skilled now and then, and are worth more.
#[allow(clippy::too_many_arguments)]
pub fn paint(data: Option<&PaintingsBaked>, size: u8, level: u32, traits: &[Trait], child: bool, extra_creative: bool, proficient: bool, rng: &mut impl Rng) -> Painted {
    // PaintingSkill's base chances, with TraitTuning's additive bonuses.
    let perfectionist = traits.contains(&Trait::Perfectionist);
    let brilliant = if proficient { 40 } else { 20 } + if perfectionist { 20 } else { 0 } + if extra_creative { 25 } else { 0 };
    let masterpiece = if proficient { 35 } else { 10 } + if perfectionist { 15 } else { 0 } + if extra_creative { 15 } else { 0 };
    let quality = if level >= 9 && rng.random_bool(masterpiece as f64 / 100.0) {
        3
    } else if level >= 6 && rng.random_bool(brilliant as f64 / 100.0) {
        2
    } else {
        0
    };
    let name = match (quality, level) {
        (3, _) => "Masterpiece",
        (2, _) => "Brilliant Painting",
        (_, 0..=2) => "Amateur Painting",
        _ => "Fine Painting",
    };
    let mut worth = (15 + level as i64 * level as i64 * 12 + rng.random_range(0..20)) as f32 * CANVAS_WORTH[size.min(2) as usize];
    worth *= match quality {
        3 => 3.0,
        2 => 1.5,
        _ => 1.0,
    };
    if extra_creative {
        worth *= 1.5;
    }
    let picture = data.and_then(|d| {
        let kind = if quality > 0 {
            quality
        } else if child {
            1
        } else {
            0
        };
        let on = |p: &&PaintingInfo| p.size == size && p.kind == kind && (kind != 0 || (p.min as u32..=p.max as u32).contains(&level));
        let mut pool: Vec<&PaintingInfo> = d.paintings.iter().filter(on).collect();
        if pool.is_empty() {
            pool = d.paintings.iter().filter(|p| p.size == size && p.kind == 0 && p.min as u32 <= level).collect();
        }
        let p = pool.choose(rng)?;
        // (Mostly in their traits' version of it, when it has one.)
        let mine: Vec<&String> = p.traits.iter().filter(|(t, _)| traits.iter().any(|x| trait_column(*x) == Some(t.as_str()))).map(|(_, n)| n).collect();
        Some(match mine.choose(rng) {
            Some(n) if rng.random_bool(0.75) => (*n).clone(),
            _ => p.name.clone(),
        })
    });
    let key = match picture {
        Some(pic) => format!("painting:{size}:{pic}#{}", rng.random::<u32>()),
        None => format!("painting#{}", rng.random::<u32>()),
    };
    Painted { key, name, worth: worth.round() as i64 }
}

/// A painting's canvas and picture. (Paintings from before they had pictures: a medium one of
/// their quality, the same each time.)
pub fn picture(data: &PaintingsBaked, s: &Stack) -> Option<(u8, String)> {
    if let Some(p) = key_picture(&s.key) {
        return Some(p);
    }
    let (kind, level) = match s.name.as_str() {
        "Masterpiece" => (3, 10),
        "Brilliant Painting" => (2, 8),
        "Amateur Painting" => (0, 1),
        _ => (0, 4),
    };
    let pool: Vec<&PaintingInfo> = data.paintings.iter().filter(|p| p.size == 1 && p.kind == kind && (kind != 0 || (p.min..=p.max).contains(&level))).collect();
    let p = pool.get((s3pkg::fnv64(&s.key) % pool.len().max(1) as u64) as usize)?;
    Some((1, p.name.clone()))
}

/// The canvas and picture a painting's key names (`painting:<size>:<picture>#<n>`).
fn key_picture(key: &str) -> Option<(u8, String)> {
    let (size, rest) = key.strip_prefix("painting:")?.split_once(':')?;
    let name = rest.split('#').next()?;
    Some((size.parse::<u8>().ok()?.min(2), name.to_string()))
}

/// The canvas on an easel while a Sim paints there.
#[derive(Component)]
pub struct EaselCanvas {
    painter: Entity,
    /// Whether the picture's showing yet.
    shown: bool,
}

/// While a Sim paints, the canvas stands on the easel: blank to begin with, their picture once
/// a third of the way through. (What they paint is decided as they start.)
#[allow(clippy::type_complexity)]
fn easel_canvases(
    mut commands: Commands,
    mut painters: Query<(Entity, &crate::sim::Sim, &ActionQueue, &Skills, Option<&mut PaintPlan>, Option<&crate::wishes::Wishes>, Option<&crate::journal::SkillJournal>)>,
    objects: Query<&GameObject>,
    canvases: Query<(Entity, &EaselCanvas)>,
    baked: Option<Res<crate::baked::Baked>>,
    mut assets: ResMut<ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(baked) = baked else { return };
    let data = &baked.0.paintings;
    // (Painter, easel, canvas, picture, whether it's showing.)
    let mut painting: Vec<(Entity, Entity, u8, Option<Key>, bool)> = Vec::new();
    for (me, sim, queue, skills, plan, wishes, journal) in &mut painters {
        let Some(a) = queue.0.front() else { continue };
        let (ActionKind::Object { target, def }, Phase::Running(elapsed)) = (&a.kind, &a.phase) else { continue };
        let Ok(obj) = objects.get(*target) else { continue };
        if crate::interact::interactions_for(obj.kind).get(*def).is_none_or(|d| d.special != Special::SellPainting) {
            continue;
        }
        let level = skills.level("Painting");
        let size = canvas(plan.as_deref(), level);
        let chosen = || paint(Some(data), size, level, &sim.traits, sim.age == crate::sim::Age::Child, crate::wishes::has(wishes, "ExtraCreative"), crate::journal::earned(journal, "Proficient Painter"), &mut rand::rng());
        let painted = match plan {
            Some(mut p) => p.painted.get_or_insert_with(chosen).clone(),
            None => {
                let p = chosen();
                commands.entity(me).insert(PaintPlan { size, painted: Some(p.clone()) });
                p
            }
        };
        let design = key_picture(&painted.key).map(|(_, pic)| s3bake::gamedata::painting_texture(&pic));
        painting.push((me, *target, size, design, *elapsed >= painting_minutes(size, sim.age == crate::sim::Age::Child, journal) / 3.0));
    }
    // The canvases of those who've stopped go (and one's put up afresh as the picture comes).
    for (e, c) in &canvases {
        if !painting.iter().any(|p| p.0 == c.painter && p.4 == c.shown) {
            commands.entity(e).despawn();
        }
    }
    for (me, easel, size, design, shown) in painting {
        if canvases.iter().any(|(_, c)| c.painter == me && c.shown == shown) {
            continue;
        }
        let Some(c) = data.canvases.get(size as usize) else { continue };
        let mut ctx = AssetCtx { baked: &baked.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let parts = assets.object_design(&mut ctx, c.objd, design.filter(|_| shown));
        // (On the easel's ledge for its size.)
        let slot = objects.get(easel).ok().and_then(|o| data.easels.iter().find(|s| s.objd == o.objd)).map(|s| if size == 0 { s.small } else { s.large });
        let at = slot.map_or(Transform::IDENTITY, |(p, r)| Transform::from_translation(Vec3::from(p)).with_rotation(Quat::from_array(r)));
        let e = spawn_parts(&mut commands, &parts, at);
        commands.entity(e).insert(EaselCanvas { painter: me, shown });
        commands.entity(easel).add_child(e);
    }
}

/// A painting's catalogue object (its canvas) and the design that is its picture.
pub fn object(data: &PaintingsBaked, s: &Stack) -> Option<(Key, Key)> {
    let (size, pic) = picture(data, s)?;
    Some((data.canvases.get(size as usize)?.objd, s3bake::gamedata::painting_texture(&pic)))
}

/// Paintings' pictures for the inventory, loaded once.
#[derive(Resource, Default)]
pub struct PaintingImages(HashMap<Key, Option<(Handle<Image>, UVec2)>>);

/// A painting's picture: its texture and the part of it the canvas shows (pixels).
pub fn image(cache: &mut PaintingImages, images: &mut Assets<Image>, baked: &crate::baked::BakedData, s: &Stack) -> Option<(Handle<Image>, Rect)> {
    let data = &baked.paintings;
    let (size, pic) = picture(data, s)?;
    let key = s3bake::gamedata::painting_texture(&pic);
    let (h, dim) = cache
        .0
        .entry(key)
        .or_insert_with(|| {
            let img = crate::objects::cpu_texture(baked, key)?;
            let dim = img.size();
            Some((images.add(img), dim))
        })
        .clone()?;
    let uv = data.canvases.get(size as usize)?.uv;
    let (w, h_px) = (dim.x as f32, dim.y as f32);
    Some((h, Rect::new(uv[0] * w, uv[1] * h_px, uv[2] * w, uv[3] * h_px)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use s3bake::gamedata::CanvasInfo;

    #[test]
    fn brushmaster_duration_uses_the_earned_challenge_after_reload() {
        let journal = crate::journal::SkillJournal { earned: vec!["Brushmaster".into()], ..default() };
        let restored = serde_json::from_str(&serde_json::to_string(&journal).unwrap()).unwrap();
        for (size, adult, child) in [(0, 180.0, 270.0), (1, 300.0, 450.0), (2, 480.0, 720.0)] {
            assert_eq!(painting_minutes(size, false, None), adult);
            assert_eq!(painting_minutes(size, true, None), child);
            assert_eq!(painting_minutes(size, false, Some(&restored)), adult / 2.0);
            assert_eq!(painting_minutes(size, true, Some(&restored)), child / 2.0);
        }
        let other = crate::journal::SkillJournal { earned: vec!["Proficient Painter".into(), "Master Painter".into()], ..default() };
        assert_eq!(painting_minutes(0, false, Some(&other)), 180.0, "quality and value challenges do not accelerate painting");
    }

    #[test]
    fn original_quality_rolls_apply_trait_bonuses_without_bypassing_skill_gates() {
        use rand::SeedableRng;
        for (traits, extra, proficient, brilliant, masterpiece) in [
            (vec![], false, false, 0.20, 0.10),
            (vec![Trait::Perfectionist], false, false, 0.40, 0.25),
            (vec![], true, false, 0.45, 0.25),
            (vec![Trait::Perfectionist], true, false, 0.65, 0.40),
            (vec![], false, true, 0.40, 0.35),
            (vec![Trait::Perfectionist], false, true, 0.60, 0.50),
            (vec![], true, true, 0.65, 0.50),
            (vec![Trait::Perfectionist], true, true, 0.85, 0.65),
        ] {
            let mut rng = rand::rngs::StdRng::seed_from_u64(718);
            for _ in 0..100 {
                let p = paint(None, 0, 5, &traits, false, extra, proficient, &mut rng);
                assert_eq!(p.name, "Fine Painting");
            }
            for level in [6, 8, 9, 10] {
                let mut brilliant_count = 0;
                let mut masterpiece_count = 0;
                for _ in 0..20_000 {
                    match paint(None, 0, level, &traits, false, extra, proficient, &mut rng).name {
                        "Brilliant Painting" => brilliant_count += 1,
                        "Masterpiece" => masterpiece_count += 1,
                        _ => {}
                    }
                }
                let expected_masterpiece = if level >= 9 { masterpiece } else { 0.0 };
                let expected_brilliant = (1.0 - expected_masterpiece) * brilliant;
                assert!((masterpiece_count as f64 / 20_000.0 - expected_masterpiece).abs() < 0.015);
                assert!((brilliant_count as f64 / 20_000.0 - expected_brilliant).abs() < 0.015);
            }
        }
    }

    fn table() -> PaintingsBaked {
        let p = |size, name: &str, min, max, kind, traits: &[(&str, &str)]| PaintingInfo {
            size,
            name: name.into(),
            min,
            max,
            kind,
            traits: traits.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        };
        PaintingsBaked {
            paintings: vec![
                p(0, "0_1_Small", 0, 1, 0, &[("Evil", "0_1_Small_Evil")]),
                p(0, "5_1_Small", 4, 6, 0, &[]),
                p(0, "0_1_Small", 0, 10, 1, &[]),
                p(1, "0_1_Medium", 0, 2, 0, &[]),
                p(1, "Brilliant_1_Medium", 0, 10, 2, &[]),
            ],
            canvases: vec![CanvasInfo::default(); 3],
            face: (0, 0, 0),
            easels: Vec::new(),
        }
    }

    #[test]
    fn paints_for_skill_and_traits() {
        let d = table();
        let mut rng = rand::rng();
        for _ in 0..50 {
            let p = paint(Some(&d), 0, 5, &[], false, false, false, &mut rng);
            assert!(p.key.starts_with("painting:0:5_1_Small#"), "{}", p.key);
            let s = Stack { kind: crate::inventory::ItemKind::Painting, key: p.key, name: p.name.into(), quality: 0, count: 1, worth: p.worth };
            assert_eq!(picture(&d, &s), Some((0, "5_1_Small".to_string())));
        }
        // An evil beginner paints the evil version mostly, never another level's.
        let evil = (0..200).filter(|_| paint(Some(&d), 0, 0, &[Trait::Evil], false, false, false, &mut rng).key.contains("0_1_Small_Evil#")).count();
        assert!(evil > 100, "{evil}");
        // Without the table: a painting all the same.
        assert!(paint(None, 2, 3, &[], false, false, false, &mut rng).key.starts_with("painting#"));
    }

    #[test]
    fn old_paintings_get_a_picture() {
        let d = table();
        let s = Stack { kind: crate::inventory::ItemKind::Painting, key: "painting#123".into(), name: "Amateur Painting".into(), quality: 0, count: 1, worth: 40 };
        let a = picture(&d, &s);
        assert_eq!(a, Some((1, "0_1_Medium".to_string())));
        assert_eq!(picture(&d, &s), a);
        let b = Stack { name: "Brilliant Painting".into(), ..s };
        assert_eq!(picture(&d, &b), Some((1, "Brilliant_1_Medium".to_string())));
    }
}
