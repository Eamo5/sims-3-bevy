//! Fishing bait, as the game's: produce from a Sim's inventory on the hook, or (from the fifth
//! level of the skill, `kSkillLevelForLiveBait`) a fish, live bait. Each fish likes a bait (the
//! `Fishing` table's): with it on, that fish bites twelve times as often (`kMultiplierBaitGives`),
//! even beyond the Sim's skill, if less so for each level beyond
//! (`kPercentLostBaitBelowSkillLevel`), and the line comes up empty less often
//! (`kNothingMultiplierWhenUsingBait`). A third of the time a catch uses the bait up
//! (`kPercentChanceBaitLost`); with the last piece gone, the Sim fishes on without. Catching a
//! fish on the bait it likes teaches the Sim that bait. The fishing spot's menu fishes with the
//! bait used last ("Fish with Apple", or "Fish with Apple (Minnow)" once the fish it's for is
//! known), or "Choose Bait...": "No Bait", or each bait held, the best of each.
//!
//! What's caught weighs what the game's `Fish.CreateFish` makes it: heavier for a Sim more skilled
//! than the fish needs, on better bait it likes, and the more of the kind they've caught; and its
//! weight makes its quality (Normal to Perfect) and its price.

use rand::Rng;
use s3bake::GameDataBaked;
use s3bake::gamedata::{CollectKind, CollectibleInfo};

use crate::inventory::{Inventory, ItemKind, Stack};
use crate::journal::SkillJournal;

/// The game's tuning (`FishingData`, `FishHere`, `FishWithSelectedBait`).
const BAIT_GIVES: f32 = 12.0;
const LOST_PER_LEVEL: f32 = 0.15;
const NOTHING_WITH_BAIT: f64 = 0.75;
pub const USED_UP: f64 = 0.33;
pub const LIVE_BAIT_LEVEL: u32 = 5;

/// How the menus name them (`Gameplay/Objects/Fishing`).
pub const CHOOSE_BAIT: &str = "Choose Bait";
pub const NO_BAIT: &str = "No Bait";
const FISH_WITH: &str = "Fish with ";

/// What can go on the hook: produce, and (at the skill for live bait) fish.
fn usable(s: &Stack, level: u32) -> bool {
    s.count > 0 && (s.kind == ItemKind::Produce || (s.kind == ItemKind::Fish && level >= LIVE_BAIT_LEVEL))
}

/// The baits a Sim holds, the best of each kind (as the game's picker lists them).
pub fn baits(inv: &Inventory, level: u32) -> Vec<&Stack> {
    let mut out: Vec<&Stack> = Vec::new();
    for s in inv.0.iter().filter(|s| usable(s, level)) {
        match out.iter_mut().find(|o| o.kind == s.kind && o.key == s.key) {
            Some(o) if s.quality > o.quality => *o = s,
            Some(_) => {}
            None => out.push(s),
        }
    }
    out
}

/// The bait used last, if they still hold one: of its quality if they can, else another of its
/// kind (as the game's `FindProperBait`).
pub fn held<'a>(inv: &'a Inventory, bait: &(String, u8), level: u32) -> Option<&'a Stack> {
    let of_kind = || inv.0.iter().filter(|s| usable(s, level) && s.key == bait.0);
    of_kind().find(|s| s.quality == bait.1).or_else(|| of_kind().max_by_key(|s| s.quality))
}

/// A bait's name as the menus show it ("Perfect Apple"; a normal one, plainly).
fn name(s: &Stack) -> &str {
    s.name.strip_prefix("Normal ").unwrap_or(&s.name)
}

/// The fish a bait's known to catch, by the Sim's journal (its name).
pub fn known_for<'a>(data: &'a GameDataBaked, journal: Option<&SkillJournal>, key: &str) -> Option<&'a str> {
    let j = journal?;
    data.collectibles.iter().find(|c| c.kind == CollectKind::Fish && c.bait == key && j.knows_bait(&c.key)).map(|c| c.name.as_str())
}

/// "Fish with {0}", or "Fish with {0} ({1})" with the fish it's known for.
pub fn fish_with(s: &Stack, known: Option<&str>) -> String {
    match known {
        Some(f) => format!("{FISH_WITH}{} ({f})", name(s)),
        None => format!("{FISH_WITH}{}", name(s)),
    }
}

/// The bait a menu's label names, from those held: its kind and quality.
pub fn chosen(label: &str, inv: &Inventory, level: u32) -> Option<(String, u8)> {
    let rest = label.strip_prefix(FISH_WITH)?;
    inv.0
        .iter()
        .filter(|s| usable(s, level))
        .find(|s| rest == name(s) || rest.strip_prefix(name(s)).is_some_and(|r| r.starts_with(" (")))
        .map(|s| (s.key.clone(), s.quality))
}

/// Uses up one piece of a bait (of its quality if there is one, else another of its kind): the
/// bait fished with from now on, if any's left.
pub fn use_one(inv: &mut Inventory, bait: &(String, u8)) -> Option<(String, u8)> {
    let i = inv.0.iter().position(|s| s.key == bait.0 && s.quality == bait.1 && matches!(s.kind, ItemKind::Produce | ItemKind::Fish) && s.count > 0)?;
    inv.take_one(i);
    let next = inv.0.iter().filter(|s| s.key == bait.0 && matches!(s.kind, ItemKind::Produce | ItemKind::Fish) && s.count > 0);
    let mut next: Vec<&Stack> = next.collect();
    next.sort_by_key(|s| (s.quality != bait.1, std::cmp::Reverse(s.quality)));
    next.first().map(|s| (s.key.clone(), s.quality))
}

/// How often a fish bites, from its spawner's weight: those within the Sim's skill (or one
/// level over); and, with the bait it likes on, twelve times as often, even beyond the skill,
/// if less so for each level beyond (the game's `GetSpawnerChances`).
pub fn bite_weight(c: &CollectibleInfo, weight: f32, level: u8, bait: Option<&str>) -> f32 {
    if bait.is_some_and(|b| !b.is_empty() && b == c.bait) {
        let beyond = c.level.saturating_sub(level) as f32;
        return weight * BAIT_GIVES * (1.0 - LOST_PER_LEVEL * beyond).max(0.0);
    }
    if c.level <= level + 1 { weight } else { 0.0 }
}

/// The game's `Fish` tuning.
const LEVELS_FOR_MAX_WEIGHT: f32 = 6.0;
const BAIT_QUALITY_LEVELS: f32 = 0.5;
const WEIGHT_VARIANCE: f32 = 0.4;
const GROWTH_PER_CATCH: f32 = 0.005;
const ICHTHYOLOGIST_WEIGHT: f32 = 1.2;

/// A quality tier (of the gardening qualities) as the game numbers it (`Quality`: Neutral 5,
/// Perfect 11).
fn game_quality(tier: u8) -> f32 {
    match tier {
        0 => 1.0,
        1 => 2.0,
        2 => 4.0,
        t => t as f32 + 2.0,
    }
}

/// How heavy a catch is (lb): heaviest six levels of skill beyond what the fish needs, half a
/// level more for each step of quality above normal of bait it likes (`bait`), and heavier the
/// more of the kind they've caught; varied by up to two-fifths either way; an Amateur
/// Ichthyologist's a fifth heavier again.
pub fn catch_weight(c: &CollectibleInfo, skill: u32, bait: Option<u8>, caught_before: f32, ichthyologist: bool, rng: &mut impl Rng) -> f32 {
    let (min, max) = (c.weight.0, c.weight.1.max(c.weight.0));
    let ahead = skill as f32 + BAIT_QUALITY_LEVELS * (bait.map_or(5.0, game_quality) - 5.0) - c.level as f32;
    let mut w = (ahead * (max - min) / LEVELS_FOR_MAX_WEIGHT + min).clamp(min, max) * (1.0 + caught_before * GROWTH_PER_CATCH);
    if !c.over_max {
        w = w.min(max);
    }
    let r = (rng.random::<f32>() + rng.random::<f32>() + rng.random::<f32>()) / 3.0;
    let mut w = w * (1.0 - WEIGHT_VARIANCE + 2.0 * WEIGHT_VARIANCE * r);
    if ichthyologist {
        w *= ICHTHYOLOGIST_WEIGHT;
    }
    (w.max(min) * 100.0).round() / 100.0
}

/// A catch's quality tier (of the gardening qualities), by where its weight falls in its kind's
/// range: Normal to Perfect.
pub fn catch_tier(c: &CollectibleInfo, w: f32) -> u8 {
    let step = (c.weight.1 - c.weight.0) / 7.0;
    3 + if step > 0.0 { ((w - c.weight.0) / step).clamp(0.0, 6.0) as u8 } else { 0 }
}

/// What a catch fetches: its kind's lowest price, and more for each pound over its lightest.
pub fn catch_value(c: &CollectibleInfo, w: f32) -> i64 {
    let range = c.weight.1 - c.weight.0;
    let slope = if range > 0.0 { (c.max_price - c.min_price) as f32 / range } else { 0.0 };
    ((w - c.weight.0) * slope + c.min_price as f32) as i64
}

/// The chance that a spell's try lands something: better with skill, and with bait on.
pub fn bite_chance(level: u8, baited: bool) -> f64 {
    let p = (0.3 + level as f64 * 0.05).min(0.85);
    if baited { 1.0 - (1.0 - p) * NOTHING_WITH_BAIT } else { p }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fish(key: &str, level: u8, bait: &str) -> CollectibleInfo {
        CollectibleInfo {
            key: key.into(),
            name: key.into(),
            kind: CollectKind::Fish,
            min_price: 5,
            max_price: 10,
            rarity: String::new(),
            level,
            model: String::new(),
            bait: bait.into(),
            weight: (1.0, 15.0),
            over_max: false,
        }
    }

    #[test]
    fn catches_grow_with_skill_and_good_bait() {
        let mut rng = rand::rng();
        let trout = fish("RainbowTrout", 2, "Egg");
        let mut mean = |skill, bait| (0..400).map(|_| catch_weight(&trout, skill, bait, 0.0, false, &mut rng)).sum::<f32>() / 400.0;
        let (novice, master, perfect_bait) = (mean(2, None), mean(8, None), mean(2, Some(9)));
        assert!(novice < 2.0 && master > 11.0 && perfect_bait > novice + 2.0, "{novice} {master} {perfect_bait}");
        // (Its weight its quality and price: the lightest Normal at the lowest price, the
        // heaviest Perfect at the highest.)
        assert_eq!((catch_tier(&trout, 1.0), catch_value(&trout, 1.0)), (3, 5));
        assert_eq!((catch_tier(&trout, 15.0), catch_value(&trout, 15.0)), (9, 10));
        assert!(catch_weight(&trout, 10, Some(9), 50.0, true, &mut rng) <= 15.0 * 1.4 * 1.2 + 0.01);
    }

    #[test]
    fn bait_draws_its_fish_even_beyond_the_skill() {
        let shark = fish("Shark", 7, "RedHerring");
        assert_eq!(bite_weight(&shark, 2.0, 3, None), 0.0);
        // (Four levels beyond: 60% off the twelvefold.)
        let w = bite_weight(&shark, 2.0, 3, Some("RedHerring"));
        assert!((w - 2.0 * 12.0 * 0.4).abs() < 1e-4, "{w}");
        assert_eq!(bite_weight(&shark, 2.0, 7, Some("RedHerring")), 24.0);
        // (Other fish as ever; past seven levels beyond, nothing.)
        assert_eq!(bite_weight(&fish("Minnow", 0, "Apple"), 3.0, 3, Some("RedHerring")), 3.0);
        assert_eq!(bite_weight(&fish("Deathfish", 10, "Angelfish"), 1.0, 2, Some("Angelfish")), 0.0);
        assert!(bite_chance(2, true) > bite_chance(2, false));
    }

    #[test]
    fn baits_are_named_chosen_and_used_up() {
        let mut inv = Inventory::default();
        inv.add(ItemKind::Produce, "Apple", "Normal Apple", 3, 4);
        inv.add(ItemKind::Produce, "Apple", "Perfect Apple", 9, 16);
        inv.add(ItemKind::Fish, "Minnow", "Minnow", 3, 5);
        inv.add(ItemKind::Find, "Ruby", "Ruby", 0, 50);
        // (The best apple; a minnow only for those who can fish with live bait.)
        let b = baits(&inv, 2);
        assert_eq!(b.len(), 1);
        assert_eq!(fish_with(b[0], None), "Fish with Perfect Apple");
        assert_eq!(baits(&inv, 5).len(), 2);
        assert_eq!(fish_with(&inv.0[0], Some("Minnow")), "Fish with Apple (Minnow)");
        assert_eq!(chosen("Fish with Apple (Minnow)", &inv, 2), Some(("Apple".into(), 3)));
        assert_eq!(chosen("Fish with Perfect Apple", &inv, 2), Some(("Apple".into(), 9)));
        assert_eq!(chosen("Fish with Minnow", &inv, 2), None);
        assert_eq!(held(&inv, &("Apple".into(), 5), 2).map(|s| s.quality), Some(9));
        // (The last normal apple gone: on with the perfect ones; those gone, none.)
        assert_eq!(use_one(&mut inv, &("Apple".into(), 3)), Some(("Apple".into(), 9)));
        assert_eq!(use_one(&mut inv, &("Apple".into(), 9)), None);
        assert!(held(&inv, &("Apple".into(), 3), 2).is_none());
    }
}
