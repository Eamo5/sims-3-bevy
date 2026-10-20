//! Story progression's moves, as the game's `StoryProgressionService` steers them by the town's
//! demographics: the town keeps about half its houses lived in (`Lot_Density`, best at 45 to
//! 55%). With too few lived in, a family about town with no home moves into an empty furnished
//! house (one that knows the household three times as likely); with too many, a family moves
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
}

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
        assert_eq!(choose(&houses, &families, &mut rng), Some(Move::In { family: 1, lot: 5 }));
        // (Too many: a family moves out or away; one played isn't among them to move.)
        let houses: Vec<House> = (0..10).map(|i| house(i, i < 8, 2)).collect();
        let families = [Family { id: 3, home: Some(1), size: 2, known: false }];
        let m = choose(&houses, &families, &mut rng);
        assert!(matches!(m, Some(Move::Out { family: 3 } | Move::Away { family: 3 })), "{m:?}");
        // (About right: a family short of beds to a bigger house.)
        let houses = [house(0, true, 1), house(1, false, 3), house(2, true, 2), house(3, false, 1)];
        let families = [Family { id: 4, home: Some(0), size: 3, known: true }, Family { id: 5, home: Some(2), size: 2, known: false }];
        assert_eq!(choose(&houses, &families, &mut rng), Some(Move::House { family: 4, lot: 1, bigger: true }));
        // (Everyone suited: nothing.)
        let families = [Family { id: 5, home: Some(2), size: 2, known: false }];
        assert_eq!(choose(&houses, &families, &mut rng), None);
    }
}
