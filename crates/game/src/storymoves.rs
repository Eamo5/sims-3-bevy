//! Story progression's moves, as the game's `StoryProgressionService` steers them by the town's
//! demographics: the town keeps about half its houses lived in (`Lot_Density`, best at 45 to
//! 55%). With too few lived in, a family about town with no home moves into an empty furnished
//! house (one that knows the household three times as likely), or a new family moves to town
//! into one (the game's `CreateAndMoveIn`, its `MakeTypicalFamily`); with too many, a family moves
//! out to live about town, or leaves town altogether (those the household knows far less
//! likely). Otherwise now and then a family moves to a house that suits them better, a bigger
//! one when they've more Sims than beds, a smaller one when they've beds to spare. The household
//! being played, and those played before, are never moved. The news says so, and the household
//! hears of it when it's a family they know.

use rand::Rng;

/// The share of the town's houses it keeps lived in (the game's best utility, 45 to 55%), and
/// the least empty houses left before more families move in (more than 3, and 5% of them).
const LOW: f32 = 0.45;
const HIGH: f32 = 0.55;
const MIN_EMPTY: usize = 4;
const MIN_EMPTY_SHARE: f32 = 0.05;
/// How likely a family the household knows is to be the one (the game's multipliers): moving
/// in, out, away.
const KNOWN_IN: f32 = 3.0;
const KNOWN_OUT: f32 = 0.33;
const KNOWN_AWAY: f32 = 0.1;
/// Of the families moving out with too many houses lived in, the share leaving town.
const LEAVE_TOWN: f64 = 0.3;

/// A family as the town's moves see it: who, where they live (a lot's place), how many of them,
/// and whether the household knows them.
#[derive(Clone, Debug)]
pub struct Family {
    pub id: u64,
    pub home: Option<usize>,
    pub size: usize,
    pub known: bool,
}

/// A house: its place in the world's lots, whether it's lived in, furnished, and its beds.
#[derive(Clone, Copy, Debug)]
pub struct House {
    pub lot: usize,
    pub lived_in: bool,
    pub furnished: bool,
    pub beds: usize,
}

/// A move the town makes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Move {
    /// A family with no home into an empty house.
    In { family: u64, lot: usize },
    /// A family out of their house, to live about town.
    Out { family: u64 },
    /// A family away from town.
    Away { family: u64 },
    /// A family to a house that suits them better (bigger or not).
    House { family: u64, lot: usize, bigger: bool },
    /// A new family to town, into a house (of so many Sims).
    New { lot: usize, size: usize },
}

/// How big new families come (the game's `Family_Size` weights, one to seven).
const FAMILY_SIZES: [f32; 7] = [0.45, 0.6, 0.65, 0.45, 0.2, 0.08, 0.05];
/// How likely a new family is rather than one about town moving in (the game's multipliers:
/// Create and Move In 2, Move In 1.5).
const NEW_FAMILY: f64 = 2.0 / 3.5;

/// How many Sims a house sleeps: its beds, a double sleeping two.
pub fn beds(b: &s3bake::LotBuildingBaked) -> usize {
    b.objects
        .iter()
        .filter(|o| o.level <= 4)
        .map(|o| o.script.to_ascii_lowercase())
        .filter(|s| s.contains(".beds."))
        .map(|s| if s.contains("double") { 2 } else { 1 })
        .sum()
}

/// One weighted at random.
fn weighted<T: Copy>(items: &[(T, f32)], rng: &mut impl Rng) -> Option<T> {
    let total: f32 = items.iter().map(|i| i.1.max(0.0)).sum();
    if total <= 0.0 {
        return None;
    }
    let mut r = rng.random_range(0.0..total);
    for (t, w) in items {
        if r < *w {
            return Some(*t);
        }
        r -= w.max(0.0);
    }
    items.last().map(|i| i.0)
}

/// The move the town makes today, if any: steered by how many of its houses are lived in.
/// `families` are those it may move (not the household's own, nor those played before).
pub fn choose(houses: &[House], families: &[Family], rng: &mut impl Rng) -> Option<Move> {
    if houses.is_empty() {
        return None;
    }
    let lived = houses.iter().filter(|h| h.lived_in).count();
    let density = lived as f32 / houses.len() as f32;
    // (Empty lots count towards there being room; families move into furnished houses.)
    let vacant = houses.iter().filter(|h| !h.lived_in).count();
    let empty: Vec<&House> = houses.iter().filter(|h| !h.lived_in && h.furnished).collect();
    let roomy = |size: usize| empty.iter().filter(|h| h.beds >= size).map(|h| h.lot).collect::<Vec<_>>();
    if density < LOW && vacant >= MIN_EMPTY && vacant as f32 > houses.len() as f32 * MIN_EMPTY_SHARE {
        // Families about town with no home: one into a house with beds enough.
        let homeless: Vec<(u64, f32)> = families.iter().filter(|f| f.home.is_none() && f.size > 0 && !roomy(f.size).is_empty()).map(|f| (f.id, if f.known { KNOWN_IN } else { 1.0 })).collect();
        // A new family, now and then (or with none about town to move in): as big as the house
        // sleeps, or smaller.
        if homeless.is_empty() || rng.random_bool(NEW_FAMILY) {
            let lots: Vec<&House> = empty.iter().copied().filter(|h| h.beds > 0).collect();
            if !lots.is_empty() {
                let h = lots[rng.random_range(0..lots.len())];
                let sizes: Vec<(usize, f32)> = FAMILY_SIZES.iter().enumerate().filter(|(n, _)| n + 1 <= h.beds).map(|(n, w)| (n + 1, *w)).collect();
                let size = weighted(&sizes, rng).unwrap_or(1);
                return Some(Move::New { lot: h.lot, size });
            }
        }
        // (No family with nowhere to live, or nowhere for them: perhaps a better house for one.)
        if let Some(family) = weighted(&homeless, rng) {
            let size = families.iter().find(|f| f.id == family).map_or(1, |f| f.size);
            let lots = roomy(size);
            return Some(Move::In { family, lot: lots[rng.random_range(0..lots.len())] });
        }
    }
    if density > HIGH {
        let away = rng.random_bool(LEAVE_TOWN);
        let homed: Vec<(u64, f32)> = families
            .iter()
            .filter(|f| f.home.is_some())
            .map(|f| (f.id, match (f.known, away) {
                (true, true) => KNOWN_AWAY,
                (true, false) => KNOWN_OUT,
                _ => 1.0,
            }))
            .collect();
        let family = weighted(&homed, rng)?;
        return Some(if away { Move::Away { family } } else { Move::Out { family } });
    }
    // A house that suits them better: bigger with too few beds, smaller with three or more to
    // spare.
    let beds_at = |lot: usize| houses.iter().find(|h| h.lot == lot).map_or(0, |h| h.beds);
    let movers: Vec<(Move, f32)> = families
        .iter()
        .filter_map(|f| {
            let home = f.home?;
            let have = beds_at(home);
            let (bigger, fits): (bool, Vec<usize>) = if have < f.size {
                (true, empty.iter().filter(|h| h.beds >= f.size && h.beds > have).map(|h| h.lot).collect())
            } else if have >= f.size + 3 {
                (false, empty.iter().filter(|h| h.beds >= f.size && h.beds < have).map(|h| h.lot).collect())
            } else {
                return None;
            };
            let lot = *fits.get(rng.random_range(0..fits.len().max(1)))?;
            Some((Move::House { family: f.id, lot, bigger }, if bigger { 3.0 } else { 1.5 }))
        })
        .collect();
    weighted(&movers, rng)
}

/// The surnames new families come with.
const SURNAMES: [&str; 40] = [
    "Abbott", "Barnes", "Castillo", "Dalton", "Ellison", "Fairbanks", "Garrison", "Holloway", "Ingram", "Jansen", "Kowalski", "Lindqvist", "Marlowe", "Nakamura", "Okafor",
    "Prescott", "Quintero", "Rasmussen", "Sterling", "Thornton", "Underhill", "Valdez", "Whitfield", "Yardley", "Zamora", "Ashworth", "Bellamy", "Crowley", "Delacroix",
    "Everhart", "Finnegan", "Goldberg", "Hawthorne", "Iverson", "Kensington", "Langley", "Moreau", "Nightingale", "Osborne", "Pemberton",
];

/// One of a new family, made at random (their name, look, traits and voice).
fn member(members: &mut Vec<s3formats::premade::PremadeSim>, last: &str, age: crate::sim::Age, female: bool, rng: &mut impl Rng) -> u64 {
    use crate::sim::Age;
    use s3formats::premade::{AGE_ADULT, AGE_CHILD, AGE_ELDER, AGE_TEEN, AGE_TODDLER, AGE_YOUNG_ADULT};
    let bits = match age {
        Age::Toddler => AGE_TODDLER,
        Age::Child => AGE_CHILD,
        Age::Teen => AGE_TEEN,
        Age::YoungAdult => AGE_YOUNG_ADULT,
        Age::Elder => AGE_ELDER,
        _ => AGE_ADULT,
    };
    let s = crate::sim::random_sim(rng, last, Some(female), age);
    let c = s.hair.to_srgba();
    let hair = 0xFF00_0000 | ((c.red * 255.0) as u32) << 16 | ((c.green * 255.0) as u32) << 8 | (c.blue * 255.0) as u32;
    members.push(s3formats::premade::PremadeSim {
        id: s.id,
        first_name: s.first.clone(),
        last_name: last.to_string(),
        age: bits,
        female,
        traits: s.traits.iter().map(|t| t.game_id()).collect(),
        skin_shade: rng.random_range(0.0..1.0),
        hair_color: Some(hair),
        voice: rng.random_range(0..3),
        voice_pitch: rng.random_range(0.3..0.7),
        ..Default::default()
    });
    s.id
}

/// A new family to town, as the game's `MakeTypicalFamily` makes one: so many Sims, the head a
/// young adult or adult (an elder, one time in seven or so, of one or two), roommates of young
/// adults one time in four, else a couple (one alone, single parents, one time in twelve or so;
/// the same sex one time in fifty) and their children, toddlers to teens; together as spouses or
/// partners, as the game has them.
pub fn new_family(size: usize, taken: &[String], rng: &mut impl Rng) -> s3bake::HouseholdBaked {
    use crate::sim::Age;
    let free: Vec<&str> = SURNAMES.iter().copied().filter(|s| !taken.iter().any(|t| t.eq_ignore_ascii_case(s))).collect();
    let last = free.get(rng.random_range(0..free.len().max(1))).copied().unwrap_or("Newcomb").to_string();
    let size = size.clamp(1, 7);
    let elder = size <= 2 && rng.random_bool(0.15);
    let roommates = !elder && size > 1 && rng.random_bool(0.25);
    let head_age = if elder {
        Age::Elder
    } else if roommates || rng.random_bool(0.5) {
        Age::YoungAdult
    } else {
        Age::Adult
    };
    let couple = size > 1 && !roommates && !rng.random_bool(0.08);
    let mut members: Vec<s3formats::premade::PremadeSim> = Vec::new();
    let head_female = rng.random_bool(0.5);
    let head = member(&mut members, &last, head_age, head_female, rng);
    let mut adults = vec![head];
    if size > 1 && (couple || roommates) {
        let same = roommates || rng.random_bool(0.02);
        let other_female = if same { head_female } else { !head_female };
        adults.push(member(&mut members, &last, head_age, other_female, rng));
    }
    let kids: Vec<u64> = (adults.len()..size)
        .map(|_| {
            let age = [Age::Toddler, Age::Child, Age::Teen][rng.random_range(0..3)];
            let female = rng.random_bool(0.5);
            member(&mut members, &last, age, female, rng)
        })
        .collect();
    // (Together: spouses or partners, as often as the game has them with children and without.)
    if couple && adults.len() == 2 {
        let r: f64 = rng.random_range(0.0..1.0);
        let married = if kids.is_empty() { r < 0.35 } else { r < 0.8 };
        let (a, b) = (adults[0], adults[1]);
        for m in members.iter_mut() {
            let other = if m.id == a {
                Some(b)
            } else if m.id == b {
                Some(a)
            } else {
                None
            };
            if let Some(o) = other {
                if married {
                    m.spouse = Some(o);
                } else {
                    m.partner = Some(o);
                }
                m.children = kids.clone();
            }
        }
    }
    let parents: Vec<u64> = if couple || adults.len() == 1 { adults.clone() } else { Vec::new() };
    for m in members.iter_mut().filter(|m| kids.contains(&m.id)) {
        m.parents = parents.clone();
    }
    s3bake::HouseholdBaked { id: rng.random(), name: last, funds: 20_000, members, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn house(lot: usize, lived_in: bool, beds: usize) -> House {
        House { lot, lived_in, furnished: true, beds }
    }

    #[test]
    fn a_town_keeps_about_half_its_houses_lived_in() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        // (Few lived in: a family with no home moves into one with beds enough.)
        let houses: Vec<House> = (0..10).map(|i| house(i, i < 2, if i == 5 { 4 } else { 1 })).collect();
        let families = [Family { id: 1, home: None, size: 4, known: false }, Family { id: 2, home: Some(0), size: 2, known: false }];
        let m = choose(&houses, &families, &mut rng);
        assert!(matches!(m, Some(Move::In { family: 1, lot: 5 } | Move::New { .. })), "{m:?}");
        // (Too many: a family moves out or away; one played isn't among them to move.)
        let houses: Vec<House> = (0..10).map(|i| house(i, i < 8, 2)).collect();
        let families = [Family { id: 3, home: Some(1), size: 2, known: false }];
        let m = choose(&houses, &families, &mut rng);
        assert!(matches!(m, Some(Move::Out { family: 3 } | Move::Away { family: 3 })), "{m:?}");
        // (About right: a family short of beds to a bigger house.)
        let houses = [house(0, true, 1), house(1, false, 3), house(2, true, 2), house(3, false, 1)];
        let families = [Family { id: 4, home: Some(0), size: 3, known: true }, Family { id: 5, home: Some(2), size: 2, known: false }];
        assert_eq!(choose(&houses, &families, &mut rng), Some(Move::House { family: 4, lot: 1, bigger: true }));
        // (A new family: as many as asked, a head grown, the others' parents theirs.)
        for size in 1..=5 {
            let f = new_family(size, &[], &mut rng);
            assert_eq!(f.members.len(), size);
            assert!(f.members[0].age & (s3formats::premade::AGE_YOUNG_ADULT | s3formats::premade::AGE_ADULT | s3formats::premade::AGE_ELDER) != 0);
            assert!(f.members.iter().all(|m| m.last_name == f.name));
        }
        // (Everyone suited: nothing.)
        let families = [Family { id: 5, home: Some(2), size: 2, known: false }];
        assert_eq!(choose(&houses, &families, &mut rng), None);
    }
}
