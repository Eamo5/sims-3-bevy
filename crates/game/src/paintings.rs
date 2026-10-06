//! Paintings, as the game's: at an easel a Sim paints on a small, medium or large canvas, and
//! what they paint is one of the game's own pictures, chosen from the Painting skill's picture
//! table by the canvas, their skill and their traits (an evil Sim's version of it, a gloomy
//! one's), now and then a brilliant painting or a masterpiece. Finished paintings go in the
//! painter's inventory, pictured, to sell or to hang on a wall (Buy mode holds them up to it;
//! hung, they can be moved or sold like the furniture).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use s3bake::Key;
use s3bake::gamedata::{PaintingInfo, PaintingsBaked};

use crate::inventory::Stack;
use crate::life::Trait;

pub struct PaintingsPlugin;

impl Plugin for PaintingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaintingImages>();
    }
}

/// The canvases, how long a painting on each takes (minutes) and how its worth goes with its
/// size.
pub const CANVASES: [&str; 3] = ["Small Canvas", "Medium Canvas", "Large Canvas"];
pub const CANVAS_MINUTES: [f32; 3] = [90.0, 150.0, 240.0];
const CANVAS_WORTH: [f32; 3] = [1.0, 1.6, 2.5];

/// The canvas a Sim's been asked to paint on (the pie menu's choice).
#[derive(Component, Clone, Copy)]
pub struct PaintPlan(pub u8);

/// A painting hung on a wall: the inventory item it is (its picture, name and worth).
#[derive(Component, Clone, Debug)]
pub struct Hung(pub Stack);

/// The canvas a Sim paints on: the one asked for, else (painting of their own accord) a small
/// one while they're learning and a medium one after.
pub fn canvas(plan: Option<&PaintPlan>, level: u32) -> u8 {
    plan.map_or(if level < 4 { 0 } else { 1 }, |p| p.0.min(2))
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
pub struct Painted {
    /// The inventory item's key (its canvas and picture) and name.
    pub key: String,
    pub name: &'static str,
    pub worth: i64,
}

/// What a Sim paints on a canvas: a picture for their skill (a child's a dabble), in their
/// traits' version of it if it has one; brilliant paintings and masterpieces come to the
/// skilled now and then, and are worth more.
pub fn paint(data: Option<&PaintingsBaked>, size: u8, level: u32, traits: &[Trait], child: bool, extra_creative: bool, rng: &mut impl Rng) -> Painted {
    let quality = if level >= 9 && rng.random_bool(0.12) {
        3
    } else if level >= 6 && rng.random_bool(0.08 + (level - 6) as f64 * 0.04) {
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
    if let Some(rest) = s.key.strip_prefix("painting:") {
        let (size, rest) = rest.split_once(':')?;
        let name = rest.split('#').next()?;
        return Some((size.parse::<u8>().ok()?.min(2), name.to_string()));
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
        }
    }

    #[test]
    fn paints_for_skill_and_traits() {
        let d = table();
        let mut rng = rand::rng();
        for _ in 0..50 {
            let p = paint(Some(&d), 0, 5, &[], false, false, &mut rng);
            assert!(p.key.starts_with("painting:0:5_1_Small#"), "{}", p.key);
            let s = Stack { kind: crate::inventory::ItemKind::Painting, key: p.key, name: p.name.into(), quality: 0, count: 1, worth: p.worth };
            assert_eq!(picture(&d, &s), Some((0, "5_1_Small".to_string())));
        }
        // An evil beginner paints the evil version mostly, never another level's.
        let evil = (0..200).filter(|_| paint(Some(&d), 0, 0, &[Trait::Evil], false, false, &mut rng).key.contains("0_1_Small_Evil#")).count();
        assert!(evil > 100, "{evil}");
        // Without the table: a painting all the same.
        assert!(paint(None, 2, 3, &[], false, false, &mut rng).key.starts_with("painting#"));
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
