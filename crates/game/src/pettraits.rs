//! Pets' traits, the Pets pack's (its `Traits` table's pet rows, by `AgeSpeciesVisible`): the
//! premade pets' own, and for a pet adopted, three drawn at random for a grown one and two for a
//! young one (never two opposites: the table's `ReinforcableOppositeTrait`). What they do is
//! the game's `TraitTuning`:
//! - Lazy: energy falls a quarter faster (`kLazyPetEnergyDecayMultiplier`), Hyper a quarter
//!   slower, and sleep restores a Hyper pet 15% faster (`kHyperPetTraitSleepEnergyGainMultiplier`);
//! - Piggy: hunger falls a quarter faster (`kPiggyPetHungerDecayMultiplier`), eats sloppily,
//!   never minds a dirty litter box; Neat and Proud pets refuse a dirty one four times in five
//!   and a filthy one always (`kChanceProudNotUseDirtyBox`, `kChanceNormalNotUseVeryDirtyBox`);
//! - Independent: social falls a quarter slower (`kIndependentPetSocialDecayMultiplier`), and
//!   fun's a quarter more (`kIndependentPetTraitFunMultiplier`); Shy: company and fun a quarter
//!   more (`kShyPetFunMultiplier`, `kShyPetSocialMultiplier`); a Playful horse's social and
//!   exercise fall a quarter faster;
//! - Friendly: takes to Sims 15% faster (`kFriendlyPetTraitWithHumansLtrGainMultiplier`);
//! - Proud cats groom themselves from half clean (`kProudCatHygieneMotiveThreshold`), Skittish
//!   ones are startled off the litter box half the time (`kSkittishTraitScaredChance`); a
//!   Genius pet has a quarter of the chance of going indoors (`kGeniusPetPeeInsideChanceModifier`);
//! - Aggressive pets turn Sims down with the pack's aggressive rejection, Skittish ones with its
//!   cowardly one; Aggressive, Playful, Piggy and Lazy pets eat in their own styles at the bowl,
//!   and a Piggy cat kicks its litter about.

use rand::Rng;
use rand::seq::IndexedRandom;

/// A pet trait: its name in the game's table, the species it's for ('c' cats, 'd' dogs of
/// either size, 'h' horses), its opposite, and whether it's drawn at random (`RandomWeight`).
pub struct PetTrait {
    pub hex: &'static str,
    species: &'static str,
    opposite: Option<&'static str>,
    random: bool,
}

const fn t(hex: &'static str, species: &'static str, opposite: Option<&'static str>) -> PetTrait {
    PetTrait { hex, species, opposite, random: true }
}

pub static PET_TRAITS: &[PetTrait] = &[
    t("LazyPet", "cd", Some("HyperPet")),
    t("LazyHorse", "h", Some("FastPet")),
    t("MeanPet", "h", Some("GentlePet")),
    t("NoisyPet", "cdh", Some("QuietPet")),
    t("LoyalPet", "d", None),
    t("HunterPet", "cd", None),
    t("NeatPet", "cdh", Some("PiggyPet")),
    t("NonDestructivePet", "cd", Some("DestructivePet")),
    t("AdventurousPet", "cd", Some("SkittishPet")),
    t("AggressivePet", "cdh", Some("FriendlyPet")),
    t("CluelessPet", "cdh", None),
    t("DestructivePet", "cd", Some("NonDestructivePet")),
    t("FriendlyPet", "cdh", Some("AggressivePet")),
    t("GeniusPet", "cdh", None),
    t("HyperPet", "cd", Some("LazyPet")),
    t("IndependentPet", "cd", None),
    t("PiggyPet", "cdh", Some("NeatPet")),
    t("PlayfulPet", "cdh", None),
    t("ProudPet", "cd", None),
    t("QuietPet", "cdh", Some("NoisyPet")),
    PetTrait { hex: "ShyPet", species: "cdh", opposite: None, random: false },
    t("SkittishPet", "cd", Some("AdventurousPet")),
    t("HydrophobicPet", "d", Some("LikesSwimmingPet")),
    t("LikesSwimmingPet", "d", Some("HydrophobicPet")),
    t("AgilePet", "h", Some("HatesJumpingPet")),
    t("BravePet", "h", None),
    t("FastPet", "h", Some("LazyHorse")),
    t("GentlePet", "h", Some("MeanPet")),
    t("NervousPet", "h", None),
    t("HatesJumpingPet", "h", Some("AgilePet")),
];

/// A pet kind's species letter in the table's terms (little dogs are dogs).
fn species(kind: &str) -> char {
    match kind.chars().nth(1) {
        Some('l') => 'd',
        Some(c) => c,
        None => 'c',
    }
}

/// Traits drawn for a pet of a kind: three for a grown one, two for the young, no two opposites.
pub fn random(kind: &str, rng: &mut impl Rng) -> Vec<String> {
    let sp = species(kind);
    let want = if kind.starts_with('a') { 3 } else { 2 };
    let mut out: Vec<&'static str> = Vec::new();
    for _ in 0..40 {
        if out.len() >= want {
            break;
        }
        let pool: Vec<&PetTrait> = PET_TRAITS
            .iter()
            .filter(|p| p.random && p.species.contains(sp) && !out.contains(&p.hex) && !p.opposite.is_some_and(|o| out.contains(&o)))
            .collect();
        let Some(p) = pool.choose(rng) else { break };
        out.push(p.hex);
    }
    out.into_iter().map(str::to_string).collect()
}

/// The pet kinds (`ac` a cat… `ch` a foal).
pub const KINDS: [&str; 8] = ["ac", "cc", "ad", "cd", "al", "cl", "ah", "ch"];

/// As many traits as a pet can have (the game's `GetMaxTraitsFor`: three young, five grown).
pub fn max_traits(kind: &str) -> usize {
    if kind.starts_with('a') { 5 } else { 3 }
}

/// The traits a pet of a kind can have, as Create a Pet offers them.
pub fn for_kind(kind: &str) -> Vec<&'static str> {
    let sp = species(kind);
    PET_TRAITS.iter().filter(|p| p.species.contains(sp)).map(|p| p.hex).collect()
}

/// Whether a trait goes with those a pet has (never its opposite).
pub fn compatible(list: &[&str], hex: &str) -> bool {
    let opposite = |a: &str, b: &str| PET_TRAITS.iter().any(|p| p.hex == a && p.opposite == Some(b));
    !list.iter().any(|t| opposite(t, hex) || opposite(hex, t))
}

/// Traits drawn at random, as many as `n` (Create a Pet's randomize: its traits filled).
pub fn random_up_to(kind: &str, n: usize, rng: &mut impl Rng) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let all = for_kind(kind);
    for _ in 0..60 {
        if out.len() >= n {
            break;
        }
        let pool: Vec<&&str> = all.iter().filter(|h| !out.contains(h) && compatible(&out, h) && PET_TRAITS.iter().any(|p| p.hex == **h && p.random)).collect();
        let Some(h) = pool.choose(rng) else { break };
        out.push(h);
    }
    out
}

/// Whether a pet has a trait.
pub fn has(traits: &[String], hex: &str) -> bool {
    traits.iter().any(|t| t == hex)
}

/// How fast each of a pet's needs falls for its traits (hunger, bladder, energy, social, hygiene,
/// fun, urge, thirst), as the game's `Motive.UpdateMotive` scales them.
pub fn decay_multiplier(kind: &str, traits: &[String], need: usize) -> f32 {
    use crate::petneeds::{ENERGY, HUNGER, SOCIAL, URGE};
    let horse = species(kind) == 'h';
    match need {
        ENERGY if has(traits, "LazyPet") || has(traits, "LazyHorse") => 1.25,
        ENERGY if has(traits, "HyperPet") => 0.75,
        HUNGER if has(traits, "PiggyPet") => 1.25,
        SOCIAL if horse && has(traits, "PlayfulPet") => 1.25,
        SOCIAL if has(traits, "IndependentPet") => 0.75,
        URGE if horse && has(traits, "PlayfulPet") => 1.25,
        _ => 1.0,
    }
}

/// How fast sleep restores a pet, for its traits (a Hyper one's 15% faster).
pub fn sleep_multiplier(traits: &[String]) -> f32 {
    if has(traits, "HyperPet") { 1.15 } else { 1.0 }
}

/// What a pet gets of fun and company from time with Sims, for its traits.
pub fn gain_multiplier(traits: &[String], need: usize) -> f32 {
    use crate::petneeds::{FUN, SOCIAL};
    match need {
        FUN if has(traits, "IndependentPet") || has(traits, "ShyPet") => 1.25,
        SOCIAL if has(traits, "ShyPet") => 1.25,
        _ => 1.0,
    }
}

/// How a pet's liking for a Sim moves, for its traits (a Friendly pet's faster).
pub fn liking_multiplier(traits: &[String]) -> f32 {
    if has(traits, "FriendlyPet") { 1.15 } else { 1.0 }
}

/// The pack's ways of turning a Sim down: plainly, aggressively, cowering.
pub const REJECT_CLIPS: [&str; 3] = [crate::pets::REJECT_CLIP, "genRejectAgressive_boring_neutral", "genRejectCoward_boring_neutral"];

/// How a pet turns a Sim down, by temperament.
pub fn reject_clip(traits: &[String]) -> &'static str {
    if has(traits, "AggressivePet") {
        REJECT_CLIPS[1]
    } else if has(traits, "SkittishPet") {
        REJECT_CLIPS[2]
    } else {
        REJECT_CLIPS[0]
    }
}

/// The ways a pet eats from a bowl for its traits (`PetBowl.GetEatingStylesForSim`: a Lazy
/// dog's lying down, a Playful cat's, an Aggressive pet's, a Piggy one's sloppy; the young and
/// the rest normally), by their clips' names.
pub fn eating_styles(kind: &str, traits: &[String]) -> Vec<&'static str> {
    let mut out = Vec::new();
    if kind.starts_with('a') {
        let sp = species(kind);
        if sp == 'd' && has(traits, "LazyPet") {
            out.push("lazy");
        }
        if sp == 'c' && has(traits, "PlayfulPet") {
            out.push("playful");
        }
        if has(traits, "AggressivePet") {
            out.push("aggressive");
        }
        if has(traits, "PiggyPet") {
            out.push("sloppy");
        }
    }
    if out.is_empty() {
        out.push("normal");
    }
    out
}

/// A bowl's eating clip for a style (the playful and lazy ones have a single loop).
pub fn eat_clip(kind: &str, style: &str, step: u32) -> String {
    let k = if kind == "cl" { "cd" } else { kind };
    let n = if matches!(style, "playful" | "lazy") { 1 } else { 1 + step % 2 };
    format!("{k}2o_petbowl_eat_{style}_loop{n}_x")
}

/// Whether a Skittish cat is startled off the litter box (`kSkittishTraitScaredChance`, half
/// the time), to go where it stands.
pub fn startled_off_box(traits: &[String], roll: f64) -> bool {
    has(traits, "SkittishPet") && roll < 0.5
}

/// Whether a Piggy cat kicks the litter about (`kChancePlayPiggyAnims`, four times in five).
pub fn piggy_litter(traits: &[String], roll: f64) -> bool {
    has(traits, "PiggyPet") && roll < 0.8
}

/// Whether a pet won't use a litter box this dirty (grime 0-100: dirty from 30, filthy from 60):
/// never a Piggy one; a Neat or Proud one a dirty box four times in five, a filthy one always;
/// any other a filthy one four times in five.
pub fn refuses_box(traits: &[String], grime: f32, roll: f64) -> bool {
    if has(traits, "PiggyPet") || grime < 30.0 {
        return false;
    }
    let fussy = has(traits, "NeatPet") || has(traits, "ProudPet");
    match (fussy, grime >= 60.0) {
        (true, true) => true,
        (true, false) => roll < 0.8,
        (false, true) => roll < 0.8,
        (false, false) => false,
    }
}

/// The chance a pet goes indoors after all (`kAdultCatPeeInsideChance` 5, the young's 30, a
/// Genius's a quarter of that).
pub fn pee_inside_chance(kind: &str, traits: &[String]) -> f64 {
    let base = if kind.starts_with('a') { 0.05 } else { 0.30 };
    if has(traits, "GeniusPet") { base * 0.25 } else { base }
}

/// Where a cat's hygiene has to fall to before it grooms itself: half clean for a Proud one
/// (`kProudCatHygieneMotiveThreshold`), else when it's grungy.
pub fn groom_at(traits: &[String]) -> f32 {
    if has(traits, "ProudPet") { 50.0 } else { -20.0 }
}

/// A trait's name as the game shows it (its string), else its key without "Pet".
pub fn name(hex: &str, gu: Option<&crate::icons::GameUi>) -> String {
    gu.and_then(|g| g.data.traits.iter().find(|t| t.hex == hex)).map(|t| t.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| hex.trim_end_matches("Pet").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traits_suit_the_species_and_never_clash() {
        let mut rng = rand::rng();
        for _ in 0..200 {
            let cat = random("ac", &mut rng);
            assert_eq!(cat.len(), 3);
            assert!(cat.iter().all(|h| PET_TRAITS.iter().find(|p| p.hex == h).unwrap().species.contains('c')));
            assert!(!(has(&cat, "LazyPet") && has(&cat, "HyperPet")));
            assert!(!(has(&cat, "NeatPet") && has(&cat, "PiggyPet")));
            let foal = random("ch", &mut rng);
            assert_eq!(foal.len(), 2);
            assert!(foal.iter().all(|h| PET_TRAITS.iter().find(|p| p.hex == h).unwrap().species.contains('h')));
            assert!(!random("al", &mut rng).iter().any(|h| h == "ShyPet"), "Shy isn't drawn");
        }
    }

    #[test]
    fn the_games_trait_tuning() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        use crate::petneeds::{ENERGY, HUNGER, SOCIAL};
        assert_eq!(decay_multiplier("ac", &v(&["LazyPet"]), ENERGY), 1.25);
        assert_eq!(decay_multiplier("ad", &v(&["HyperPet"]), ENERGY), 0.75);
        assert_eq!(decay_multiplier("ad", &v(&["PiggyPet"]), HUNGER), 1.25);
        assert_eq!(decay_multiplier("ah", &v(&["PlayfulPet"]), SOCIAL), 1.25);
        assert_eq!(decay_multiplier("ac", &v(&["PlayfulPet"]), SOCIAL), 1.0);
        assert!(!refuses_box(&v(&["PiggyPet"]), 90.0, 0.0));
        assert!(refuses_box(&v(&["NeatPet"]), 90.0, 0.99));
        assert!(!refuses_box(&v(&[]), 40.0, 0.0));
        assert!((pee_inside_chance("ac", &v(&["GeniusPet"])) - 0.0125).abs() < 1e-9);
        assert_eq!(sleep_multiplier(&v(&["HyperPet"])), 1.15);
    }

    #[test]
    fn create_a_pet_offers_the_species_traits() {
        assert!(for_kind("ah").contains(&"FastPet") && !for_kind("ah").contains(&"HunterPet"));
        assert!(for_kind("al").contains(&"LoyalPet") && !for_kind("ac").contains(&"LoyalPet"));
        assert!(!compatible(&["LazyPet"], "HyperPet") && !compatible(&["HyperPet"], "LazyPet"));
        assert!(compatible(&["LazyPet"], "GeniusPet"));
        assert_eq!((max_traits("ac"), max_traits("cd")), (5, 3));
        let mut rng = rand::rng();
        for _ in 0..50 {
            let t = random_up_to("ad", 5, &mut rng);
            assert_eq!(t.len(), 5);
            assert!(t.iter().enumerate().all(|(i, h)| compatible(&t[..i], h)));
        }
    }

    #[test]
    fn pets_eat_their_own_way() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // (A lazy dog lies down to it; a lazy cat hasn't that way, nor a playful dog.)
        assert_eq!(eating_styles("ad", &v(&["LazyPet"])), vec!["lazy"]);
        assert_eq!(eating_styles("ac", &v(&["LazyPet"])), vec!["normal"]);
        assert_eq!(eating_styles("ad", &v(&["PlayfulPet"])), vec!["normal"]);
        assert_eq!(eating_styles("ac", &v(&["PlayfulPet", "PiggyPet"])), vec!["playful", "sloppy"]);
        // (The young eat as the young do.)
        assert_eq!(eating_styles("cd", &v(&["AggressivePet"])), vec!["normal"]);
        assert_eq!(eat_clip("al", "lazy", 3), "al2o_petbowl_eat_lazy_loop1_x");
        assert_eq!(eat_clip("cl", "normal", 1), "cd2o_petbowl_eat_normal_loop2_x");
        assert_eq!(reject_clip(&v(&["AggressivePet"])), "genRejectAgressive_boring_neutral");
    }
}
