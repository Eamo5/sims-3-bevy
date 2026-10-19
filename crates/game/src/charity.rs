//! Donating to charity at the mailbox, as the game's `Mailbox.DonateToCharity`: a Good Sim (or
//! an Evil one, to undermine it) gives §100, §250 or §1000 once a day to one of two causes, the
//! game's `Charity` table's (any of its general ones, and those of their traits: The Arts for the
//! artistic, Volcano Preservation for the evil), chosen afresh each week. Good Sims come away
//! Charitable, the Evil Fiendishly Delighted, the more so the more they gave
//! (its tuning's `kLow/Medium/HighCharitableBuffMood`, 10, 15, 30).

use bevy::prelude::*;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::Sim;

pub struct CharityPlugin;

impl Plugin for CharityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, donated.run_if(in_state(PlayMode::Live)));
    }
}

/// The game's `Charity` table: each cause, and the traits it's offered for (none: to anyone).
const CAUSES: &[(&str, &str)] = &[
    ("FamilyPrograms", "FamilyOriented, Childish"),
    ("Poor", "Frugal, CommitmentIssues, Charismatic, Kleptomaniac"),
    ("WorldPeace", "Ambitious, NoSenseOfHumor, EasilyImpressed"),
    ("ParanormalResearch", "Insane, ComputerWhiz"),
    ("NaturalParks", "LovesTheOutdoors, GreenThumb"),
    ("UnderfundedGyms", "Athletic, Loser"),
    ("TheArts", "Artistic, Charismatic"),
    ("StarvingWriters", "BookWorm, Frugal, Loner"),
    ("SpottedElephantFund", "Insane, Childish, GoodSenseOfHumor"),
    ("LakePreservation", "Angler, LovesTheOutdoors"),
    ("CranialResearch", "AbsentMinded, Clumsy, Loser, Insane"),
    ("FireDepartment", "Brave, Daredevil, Handy, Coward"),
    ("PoliceDepartment", "Coward"),
    ("TheInternet", "ComputerWhiz, Genius"),
    ("MediaConglomerates", "CouchPotato"),
    ("VolcanoPreservation", "Evil, Inappropriate, HotHeaded"),
    ("TheCupidFoundation", "Flirty, HopelessRomantic"),
    ("InsuranceCompanies", "Snob, Mooch"),
    ("CleanlinessConservancy", "Neat"),
    ("ManufacturersOfCutoffs", "NeverNude"),
    ("WorldHunger", ""),
    ("UnderprivilegedLlamas", ""),
    ("TheBellaBeneficiary", ""),
    ("SocialBunniesUnited", ""),
    ("VeronavilleCommunityChest", ""),
    ("SecretSquidSociety", ""),
    ("LocalSocietyForASmarterTomorrow", ""),
    ("GastronomyClub", ""),
    ("AlternativeSportSociety", ""),
    ("RecoveryOfExtinctSpecies", ""),
    ("GeeseUnlimited", ""),
    ("GPLNF", ""),
    ("WalledInSimsSociety", ""),
    ("SaveTheEarthWormsFoundation", ""),
    ("KleptosAnonymous", ""),
    ("SimlishAsASecondLanguage", ""),
    ("ToiletsForTots", ""),
    ("ProtestersWithoutCause", ""),
    ("PlantAMoneyTreeFoundation", ""),
    ("LifefruitForElders", ""),
    ("TheOcean", ""),
    ("TheRich", "Snob, Insane"),
    ("FishKid", ""),
    ("BerjesHoganUnited", ""),
];

/// The game's tuned donations and how much each pleases them.
pub const AMOUNTS: [(i64, i32); 3] = [(1000, 30), (250, 15), (100, 10)];
/// How long the moodlets last (game minutes): the game's Charitable, Fiendishly Delighted.
const CHARITABLE_MINUTES: f64 = 1440.0;
const DELIGHTED_MINUTES: f64 = 360.0;

/// A Sim's causes this week, and when they last gave (kept with the game).
#[derive(Component, Clone, Default, Debug, Serialize, Deserialize)]
pub struct Charities {
    pub causes: Vec<String>,
    pub chosen_on: u32,
    pub gave_on: Option<u32>,
}

/// Whether they'd give at all (the Good, and the Evil to undermine): not again the same day.
pub fn gives(sim: &Sim, charities: Option<&Charities>, day: u32) -> bool {
    (sim.traits.contains(&Trait::Good) || sim.traits.contains(&Trait::Evil)) && charities.is_none_or(|c| c.gave_on.is_none_or(|d| d < day))
}

/// Their two causes this week (chosen afresh after seven days): the general ones, and their traits'.
pub fn causes_for(sim: &Sim, charities: Option<&Charities>, day: u32) -> Vec<String> {
    if let Some(c) = charities.filter(|c| c.causes.len() == 2 && day < c.chosen_on + 7) {
        return c.causes.clone();
    }
    let ids: Vec<String> = sim.traits.iter().map(|t| t.game_id()).collect();
    let pool: Vec<&str> = CAUSES.iter().filter(|(_, traits)| traits.is_empty() || traits.split(',').any(|t| ids.iter().any(|i| i == t.trim()))).map(|(c, _)| *c).collect();
    let mut rng = rand::rng();
    pool.choose_multiple(&mut rng, 2).map(|c| c.to_string()).collect()
}

/// A cause's name, in the game's words (`Charity/Sheet1:`).
pub fn cause_name(ui: Option<&crate::layout::UiAssets>, cause: &str) -> String {
    ui.and_then(|u| u.localize(&format!("Gameplay/Excel/Charity/Sheet1:{cause}"))).unwrap_or_else(|| cause.to_string())
}

/// The menu's words for a cause: "Donate to The Arts..." (the Evil's "Donate to Undermine...").
pub fn menu_title(sim: &Sim, name: &str) -> String {
    if sim.traits.contains(&Trait::Good) { format!("Donate to {name}") } else { format!("Donate to Undermine {name}") }
}

/// A donation made at the mailbox (its outcome to be had).
#[derive(Component)]
pub struct Donated {
    pub cause: String,
    pub amount: i64,
}

fn donated(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &Donated, Option<&Charities>, &mut Moodlets)>,
    mut household: Option<ResMut<crate::interact::Household>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut dreams: MessageWriter<crate::dreams::DreamEvent>,
) {
    for (e, sim, d, charities, mut m) in &mut sims {
        commands.entity(e).remove::<Donated>();
        let Some(h) = household.as_mut().filter(|h| h.funds >= d.amount) else {
            notes.push(format!("{} doesn't have enough money for this donation.", sim.first));
            continue;
        };
        h.funds -= d.amount;
        // (The game's `kDonatedToCharity`, or to undermine one.)
        let kind = if sim.traits.contains(&Trait::Good) { "kDonatedToCharity" } else { "kDonatedToUndermineCharity" };
        dreams.write(crate::dreams::DreamEvent { amount: d.amount as f32, ..crate::dreams::DreamEvent::new(e, kind) });
        let strength = AMOUNTS.iter().find(|(a, _)| *a == d.amount).map_or(10, |(_, s)| *s);
        if sim.traits.contains(&Trait::Good) {
            m.add_valued(MoodletKind::Charitable, clock.minutes, strength, CHARITABLE_MINUTES);
            notes.push(format!("{} donated §{} to {}.", sim.first, d.amount, d.cause));
        } else {
            m.add_valued(MoodletKind::FiendishlyDelighted, clock.minutes, strength, DELIGHTED_MINUTES);
            notes.push(format!("{} donated §{} to undermine {}, cackling.", sim.first, d.amount, d.cause));
        }
        let mut c = charities.cloned().unwrap_or_default();
        c.gave_on = Some(clock.day());
        commands.entity(e).insert(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_good_give_once_a_day_to_causes_of_their_own() {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "G", Some(true), crate::sim::Age::Adult);
        s.traits = vec![Trait::Good, Trait::Artistic];
        assert!(gives(&s, None, 3));
        assert!(!gives(&s, Some(&Charities { gave_on: Some(3), ..Default::default() }), 3));
        let c = causes_for(&s, None, 3);
        assert_eq!(c.len(), 2);
        assert!(c.iter().all(|x| CAUSES.iter().any(|(k, t)| k == x && (t.is_empty() || t.contains("Artistic") || t.contains("Good")))));
        s.traits = vec![Trait::Artistic];
        assert!(!gives(&s, None, 3));
    }
}
