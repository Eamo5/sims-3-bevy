//! Moving out, as the game's Move dialog splits a household: grown-ups of it (on their own, or
//! with their partner) leave for a home of their own, taking the game's share of the household's
//! funds (`GameEntryMovingModel`: a tenth of them for each who goes, never more than their share
//! of the household, at least §1,000 where there's that much, at most §1,000,000); the rest stay
//! on where they are. Those who moved out are a household of their own after, living their lives
//! about town, to be played whenever (Change Household).

use bevy::prelude::*;

use crate::save::SaveGame;
use crate::sim::Age;

/// Who's moving out, chosen on the phone (by Sim id): the move then splits them off.
#[derive(Resource, Clone)]
pub struct MoveOutChoice(pub Vec<u64>);

/// The phone menu's "social" indices for moving out alone, or with their partner.
pub const MOVE_OUT: usize = usize::MAX - 2;
pub const MOVE_OUT_TOGETHER: usize = usize::MAX - 3;

/// Those staying, while those moving out choose their new home.
#[derive(Resource)]
pub struct Staying(pub SaveGame);

/// The game's `kMovePerPersonPrecentage`, `kMinMoneyTransferred` and `kMaxMoneyTransferred`.
const PER_PERSON: f64 = 0.1;
const MIN_TRANSFER: i64 = 1_000;
const MAX_TRANSFER: i64 = 1_000_000;

/// What those moving out take of the household's funds: a tenth each (no more than their share
/// of it), within the game's bounds.
pub fn transfer(funds: i64, household: usize, moving: usize) -> i64 {
    let per = PER_PERSON.min(1.0 / household.max(1) as f64);
    let t = (moving as f64 * per * funds.max(0) as f64) as i64;
    t.min(MAX_TRANSFER).max(MIN_TRANSFER.min(funds.max(0)))
}

/// Whether these can move out: grown-ups all of them, leaving a grown-up behind with whoever
/// stays (the game's move keeps no household of children on its own).
pub fn may_move_out(members: &[(u64, Age)], movers: &[u64]) -> bool {
    let grown = |a: Age| matches!(a, Age::YoungAdult | Age::Adult | Age::Elder);
    !movers.is_empty()
        && movers.iter().all(|m| members.iter().any(|(id, a)| id == m && grown(*a)))
        && members.iter().any(|(id, a)| !movers.contains(id) && grown(*a))
}

/// The household split in two, as the game left it saved: those moving out (a household of their
/// own, with their share of the funds, not yet with a home, nothing of the old one's lot with
/// them), and those staying (the rest of the funds, and everything else).
pub fn split(g: &SaveGame, movers: &[u64]) -> (SaveGame, SaveGame) {
    let size = g.sims.iter().filter(|s| s.member).count();
    let share = transfer(g.funds, size, movers.len());
    // (Each side knows the other as they did, out about town: who's married to whom, who's whose
    // friend, goes on.)
    let apart = |s: &mut crate::save::SavedSim| {
        s.member = false;
        s.selected = false;
        s.whereabouts = "away".into();
    };
    let mut rest = g.clone();
    rest.sims.iter_mut().filter(|s| s.member && movers.contains(&s.id)).for_each(apart);
    rest.funds -= share;
    if !rest.sims.iter().any(|s| s.selected) && let Some(s) = rest.sims.iter_mut().find(|s| s.member) {
        s.selected = true;
    }
    let mut out = g.clone();
    out.sims.iter_mut().filter(|s| s.member && !movers.contains(&s.id)).for_each(apart);
    if let Some(s) = out.sims.iter_mut().find(|s| s.member) {
        s.selected = true;
    }
    out.household = out.sims.iter().find(|s| s.member).map_or_else(|| g.household.clone(), |s| s.last.clone());
    out.funds = share;
    out.homeless = true;
    out.dormant.clear();
    // (The old home's lot, its things and its upkeep stay with those staying.)
    out.broken_objects.clear();
    out.object_states.clear();
    out.puddles.clear();
    out.laundry = Default::default();
    out.bills.clear();
    out.bought.clear();
    out.removed.clear();
    out.paint.clear();
    out.seeds.clear();
    out.plants.clear();
    out.alarm = Default::default();
    out.fishbowls.clear();
    out.maid = false;
    out.upgrades.clear();
    out.terrain.clear();
    out.heights.clear();
    out.collection = Default::default();
    out.graves.clear();
    out.leftovers.clear();
    out.fridge = None;
    out.meals.clear();
    out.pets.clear();
    out.family_inventory.clear();
    (out, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn those_moving_out_take_the_games_share() {
        // (A tenth of the funds for each who goes.)
        assert_eq!(transfer(50_000, 5, 1), 5_000);
        assert_eq!(transfer(50_000, 2, 1), 5_000);
        // (At least §1,000, where there's that much.)
        assert_eq!(transfer(3_000, 3, 1), 1_000);
        assert_eq!(transfer(600, 3, 1), 600);
        assert_eq!(transfer(100_000_000, 4, 2), 1_000_000);
    }

    #[test]
    fn each_side_keeps_knowing_the_other() {
        let mut g: SaveGame = serde_json::from_str(
            r#"{"version":1,"world":"w","lot_index":3,"lot_name":"Home","household":"Goth","funds":20000,"last_bill_day":0,"minutes":0.0,"bought":[],"removed":[],"sims":[]}"#,
        )
        .unwrap();
        for (id, first) in [(1, "Mortimer"), (2, "Bella")] {
            let mut s = crate::save::saved_sim_for_test(id, first);
            s.member = true;
            s.selected = id == 1;
            g.sims.push(s);
        }
        let (out, rest) = split(&g, &[2]);
        assert_eq!((out.funds, rest.funds), (2_000, 18_000));
        assert!(out.sims.iter().any(|s| s.id == 2 && s.member && s.selected));
        assert!(out.sims.iter().any(|s| s.id == 1 && !s.member && s.whereabouts == "away"));
        assert!(rest.sims.iter().any(|s| s.id == 2 && !s.member) && rest.sims.iter().any(|s| s.id == 1 && s.member && s.selected));
    }

    #[test]
    fn grown_ups_move_out_leaving_a_grown_up() {
        let members = [(1, Age::Adult), (2, Age::Adult), (3, Age::Teen), (4, Age::YoungAdult)];
        assert!(may_move_out(&members, &[4]));
        assert!(may_move_out(&members, &[1, 4]));
        assert!(!may_move_out(&members, &[3]), "a teen doesn't move out alone");
        assert!(!may_move_out(&members, &[1, 2, 4]), "no grown-up left with the teen");
    }
}
