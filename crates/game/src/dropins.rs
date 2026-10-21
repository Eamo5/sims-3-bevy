//! Friends dropping by uninvited, as the game's meta-autonomy `VisitLot` on the active
//! household's home: someone the family knows (and nobody of it dislikes) comes over in the late
//! afternoon or evening while the household's at home, rings at the front door, and waits to be
//! invited inside (`doorbell`). The game's `VisitPlayer` venue wants half a visitor from 18:00 to
//! 20:00 (none before 15:00 or after 20:06), at most one at a time, and the lot not visited for
//! a day (`Lot.kMinTimeSinceLastVisitForActiveLot`); each Sim visits on their own at most every
//! three days (`kAutonomousVisitFrequencyInDays`).
//!
//! (The game's lot clock is reset by anyone stepping onto the lot, the paperboy too, which in
//! practice keeps such visits rare; here it's the visits themselves that count, with a chance at
//! each look in the evening standing in for how often the town's Sims would pick it.)

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{Invited, OffLot, Visitor};
use crate::sim::{Age, HouseholdMember, Sim};
use crate::social::Relationships;

pub struct DropInsPlugin;

impl Plugin for DropInsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LotVisits>().add_systems(Update, (note_visits, drop_ins).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// When someone last came to visit the household's home (game minutes).
#[derive(Resource, Default)]
pub struct LotVisits {
    pub last: Option<f64>,
}

/// When a Sim last dropped in uninvited (game minutes): the game's `LastAutonomousVisitDate`.
#[derive(Component)]
pub struct DroppedIn(pub f64);

/// The game's tunables: a day between visits to the home, three days between a Sim's own.
const LOT_GAP_MINUTES: f64 = 24.0 * 60.0;
const SIM_GAP_MINUTES: f64 = 3.0 * 24.0 * 60.0;
/// How often a look's taken (game minutes), and the chance at each at the venue's peak (its half
/// a visitor wanted).
const LOOK_EVERY: f64 = 30.0;
const PEAK_CHANCE: f64 = 0.2;
/// How long it takes them to come over (game minutes).
const TRAVEL: std::ops::Range<f64> = 20.0..45.0;

/// The game's `VisitPlayer` curve (with the household's Sim at home): how many visitors the home
/// wants at an hour of the day.
pub fn wanted_at(hour: f64) -> f64 {
    match hour {
        h if h < 15.0 => 0.0,
        h if h < 18.0 => (h - 15.0) / 3.0 * 0.5,
        h if h < 20.0 => 0.5,
        h if h < 20.1 => 0.5 * (20.1 - h) / 0.1,
        _ => 0.0,
    }
}

/// Visitors arriving: the home visited now.
fn note_visits(clock: Res<crate::clock::GameClock>, mut visits: ResMut<LotVisits>, arrived: Query<(), (Added<Visitor>, Without<HouseholdMember>)>) {
    if !arrived.is_empty() {
        visits.last = Some(clock.minutes);
    }
}

/// Asked for by the testing cheats' "Force Visitor": someone comes by now.
pub static FORCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Now and then of an evening, someone the family knows comes over unasked.
#[allow(clippy::type_complexity)]
fn drop_ins(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut visits: ResMut<LotVisits>,
    mut last_look: Local<f64>,
    members: Query<(Entity, &Sim, &Relationships, Option<&crate::visit::OnLot>, Has<crate::careers::AtWork>, Has<crate::sim::Selected>), With<HouseholdMember>>,
    town: Query<(Entity, &Sim, Option<&DroppedIn>), (With<OffLot>, Without<Invited>, Without<HouseholdMember>)>,
    coming: Query<(), (Or<(With<Invited>, With<crate::doorbell::AtTheDoor>)>, Without<HouseholdMember>)>,
    party: Option<Res<crate::interact::Party>>,
) {
    // (The testing cheats' "Force Visitor": now, whatever the hour.)
    let forced = FORCE.load(std::sync::atomic::Ordering::Relaxed);
    if !forced && clock.minutes - *last_look < LOOK_EVERY {
        return;
    }
    *last_look = clock.minutes;
    let hour = clock.hour_f() as f64;
    let wanted = if forced { 1.0 } else { wanted_at(hour) };
    // (DROP_IN=1 or a first name, tests: no waiting for the evening's chance or the day since
    // the last.)
    let test = std::env::var("DROP_IN").is_ok() || forced;
    FORCE.store(false, std::sync::atomic::Ordering::Relaxed);
    if wanted <= 0.0 || party.is_some() || !coming.is_empty() || (!test && visits.last.is_some_and(|t| clock.minutes - t < LOT_GAP_MINUTES)) {
        return;
    }
    // (The household's Sim at home, and so someone to come to.)
    let home = |on: Option<&crate::visit::OnLot>, work: bool| on.is_none() && !work;
    if !members.iter().any(|m| m.5 && home(m.3, m.4)) {
        return;
    }
    let mut rng = rand::rng();
    if !test && !rng.random_bool((wanted / 0.5 * PEAK_CHANCE).clamp(0.0, 1.0)) {
        return;
    }
    let child_home = members.iter().any(|m| m.1.age == Age::Child && home(m.3, m.4));
    // Who might: known to someone of the household, disliked by none, not a little one (a child
    // only to see a child), not been round on their own in three days.
    // (DROP_IN=<first name>: that one.)
    let only = std::env::var("DROP_IN").ok().filter(|n| n != "1");
    let candidates: Vec<(Entity, f32)> = town
        .iter()
        .filter(|(_, s, _)| only.as_ref().is_none_or(|n| s.first.eq_ignore_ascii_case(n)))
        .filter(|(_, s, _)| !s.age.is_little() && (s.age != Age::Child || child_home))
        .filter(|(_, _, d)| d.is_none_or(|d| clock.minutes - d.0 >= SIM_GAP_MINUTES))
        .filter_map(|(e, ..)| {
            let rels: Vec<f32> = members.iter().filter_map(|m| m.2.0.get(&e).map(|r| r.friendship)).collect();
            (!rels.is_empty() && rels.iter().all(|f| *f >= 0.0)).then(|| (e, rels.iter().copied().fold(0.0, f32::max)))
        })
        .collect();
    // (The better liked, the likelier.)
    let total: f32 = candidates.iter().map(|c| 10.0 + c.1).sum();
    if total <= 0.0 {
        return;
    }
    let mut roll = rng.random_range(0.0..total);
    let Some((who, _)) = candidates.iter().find(|c| {
        roll -= 10.0 + c.1;
        roll <= 0.0
    }) else {
        return;
    };
    let arrive_at = clock.minutes + rng.random_range(TRAVEL);
    info!("drop-in: {who} comes over (at {:.0})", arrive_at);
    commands.entity(*who).insert((Invited { arrive_at }, DroppedIn(clock.minutes)));
    // (Counted from now: one at a time.)
    visits.last = Some(clock.minutes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visitors_wanted_in_the_evening() {
        assert_eq!(wanted_at(10.0), 0.0);
        assert_eq!(wanted_at(15.0), 0.0);
        assert!((wanted_at(16.5) - 0.25).abs() < 1e-9);
        assert_eq!(wanted_at(19.0), 0.5);
        assert_eq!(wanted_at(20.5), 0.0);
        assert_eq!(wanted_at(23.0), 0.0);
    }
}
