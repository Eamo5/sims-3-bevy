//! Life the Sims 3 way: personality traits, moodlets (temporary feelings from needs and
//! events) and the mood they add up to.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::SliceRandom;

use crate::PlayMode;
use crate::clock::{GameClock, SimDelta};
use crate::interact::Skills;
use crate::sim::*;

pub struct LifePlugin;

impl Plugin for LifePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LifeEvent>().add_systems(
            Update,
            (need_moodlets, need_failures, life_events, expire_and_sum.after(crate::weather::sim_temperatures)).chain().run_if(in_state(PlayMode::Live)),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Traits

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Trait {
    Absentminded,
    Ambitious,
    Angler,
    Artistic,
    Athletic,
    Bookworm,
    Brave,
    CantStandArt,
    Charismatic,
    Childish,
    Clumsy,
    CommitmentIssues,
    ComputerWhiz,
    CouchPotato,
    Coward,
    Daredevil,
    DislikesChildren,
    EasilyImpressed,
    Evil,
    Excitable,
    FamilyOriented,
    Flirty,
    Friendly,
    Frugal,
    Genius,
    Good,
    GoodSenseOfHumor,
    GreatKisser,
    GreenThumb,
    Grumpy,
    Handy,
    HatesTheOutdoors,
    HeavySleeper,
    HopelessRomantic,
    HotHeaded,
    Hydrophobic,
    Inappropriate,
    Insane,
    Kleptomaniac,
    Lazy,
    LightSleeper,
    Loner,
    Loser,
    LovesTheOutdoors,
    Lucky,
    MeanSpirited,
    Mooch,
    NaturalCook,
    Neat,
    Neurotic,
    NeverNude,
    NightOwl,
    NoSenseOfHumor,
    OverEmotional,
    PartyAnimal,
    Perfectionist,
    Schmoozer,
    Slob,
    Snob,
    Technophobe,
    Unflirty,
    Unlucky,
    Vegetarian,
    Virtuoso,
    Workaholic,
}

impl Trait {
    /// The trait's id in the game's tables.
    pub fn game_id(self) -> String {
        match self {
            Trait::Absentminded => "AbsentMinded".into(),
            Trait::Bookworm => "BookWorm".into(),
            Trait::HatesTheOutdoors => "HatesOutdoors".into(),
            Trait::Technophobe => "AntiTV".into(),
            Trait::NightOwl => "NightOwlTrait".into(),
            // (Only the pets' version is in the table; its icon is the same.)
            Trait::Lazy => "LazyPet".into(),
            t => format!("{t:?}"),
        }
    }

    /// Selectable human traits. LazyPet is retained in the enum only for old saves.
    pub const ALL: [Trait; 64] = [
        Trait::Absentminded,
        Trait::Ambitious,
        Trait::Angler,
        Trait::Artistic,
        Trait::Athletic,
        Trait::Bookworm,
        Trait::Brave,
        Trait::CantStandArt,
        Trait::Charismatic,
        Trait::Childish,
        Trait::Clumsy,
        Trait::CommitmentIssues,
        Trait::ComputerWhiz,
        Trait::CouchPotato,
        Trait::Coward,
        Trait::Daredevil,
        Trait::DislikesChildren,
        Trait::EasilyImpressed,
        Trait::Evil,
        Trait::Excitable,
        Trait::FamilyOriented,
        Trait::Flirty,
        Trait::Friendly,
        Trait::Frugal,
        Trait::Genius,
        Trait::Good,
        Trait::GoodSenseOfHumor,
        Trait::GreatKisser,
        Trait::GreenThumb,
        Trait::Grumpy,
        Trait::Handy,
        Trait::HatesTheOutdoors,
        Trait::HeavySleeper,
        Trait::HopelessRomantic,
        Trait::HotHeaded,
        Trait::Hydrophobic,
        Trait::Inappropriate,
        Trait::Insane,
        Trait::Kleptomaniac,
        Trait::LightSleeper,
        Trait::Loner,
        Trait::Loser,
        Trait::LovesTheOutdoors,
        Trait::Lucky,
        Trait::MeanSpirited,
        Trait::Mooch,
        Trait::NaturalCook,
        Trait::Neat,
        Trait::Neurotic,
        Trait::NeverNude,
        Trait::NightOwl,
        Trait::NoSenseOfHumor,
        Trait::OverEmotional,
        Trait::PartyAnimal,
        Trait::Perfectionist,
        Trait::Schmoozer,
        Trait::Slob,
        Trait::Snob,
        Trait::Technophobe,
        Trait::Unflirty,
        Trait::Unlucky,
        Trait::Vegetarian,
        Trait::Virtuoso,
        Trait::Workaholic,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Trait::Vegetarian => "Vegetarian",
            Trait::Unlucky => "Unlucky",
            Trait::Schmoozer => "Schmoozer",
            Trait::OverEmotional => "Over-Emotional",
            Trait::NoSenseOfHumor => "No Sense of Humor",
            Trait::NeverNude => "Never Nude",
            Trait::Neurotic => "Neurotic",
            Trait::Mooch => "Mooch",
            Trait::Lucky => "Lucky",
            Trait::Loser => "Loser",
            Trait::Kleptomaniac => "Kleptomaniac",
            Trait::Inappropriate => "Inappropriate",
            Trait::HatesTheOutdoors => "Hates the Outdoors",
            Trait::GreenThumb => "Green Thumb",
            Trait::GreatKisser => "Great Kisser",
            Trait::Evil => "Evil",
            Trait::EasilyImpressed => "Easily Impressed",
            Trait::DislikesChildren => "Dislikes Children",
            Trait::Daredevil => "Daredevil",
            Trait::Coward => "Coward",
            Trait::CommitmentIssues => "Commitment Issues",
            Trait::CantStandArt => "Can't Stand Art",
            Trait::Angler => "Angler",
            Trait::Absentminded => "Absent-Minded",
            Trait::Ambitious => "Ambitious",
            Trait::Artistic => "Artistic",
            Trait::Athletic => "Athletic",
            Trait::Bookworm => "Bookworm",
            Trait::Brave => "Brave",
            Trait::Charismatic => "Charismatic",
            Trait::Childish => "Childish",
            Trait::Clumsy => "Clumsy",
            Trait::ComputerWhiz => "Computer Whiz",
            Trait::CouchPotato => "Couch Potato",
            Trait::Excitable => "Excitable",
            Trait::FamilyOriented => "Family-Oriented",
            Trait::Flirty => "Flirty",
            Trait::Friendly => "Friendly",
            Trait::Frugal => "Frugal",
            Trait::Genius => "Genius",
            Trait::Good => "Good",
            Trait::GoodSenseOfHumor => "Good Sense of Humor",
            Trait::Grumpy => "Grumpy",
            Trait::Handy => "Handy",
            Trait::HeavySleeper => "Heavy Sleeper",
            Trait::HopelessRomantic => "Hopeless Romantic",
            Trait::HotHeaded => "Hot-Headed",
            Trait::Hydrophobic => "Hydrophobic",
            Trait::Insane => "Insane",
            Trait::Lazy => "Lazy",
            Trait::LightSleeper => "Light Sleeper",
            Trait::Loner => "Loner",
            Trait::LovesTheOutdoors => "Loves the Outdoors",
            Trait::MeanSpirited => "Mean-Spirited",
            Trait::NaturalCook => "Natural Cook",
            Trait::Neat => "Neat",
            Trait::NightOwl => "Night Owl",
            Trait::PartyAnimal => "Party Animal",
            Trait::Perfectionist => "Perfectionist",
            Trait::Slob => "Slob",
            Trait::Snob => "Snob",
            Trait::Technophobe => "Technophobe",
            Trait::Unflirty => "Unflirty",
            Trait::Virtuoso => "Virtuoso",
            Trait::Workaholic => "Workaholic",
        }
    }

    pub fn from_name(n: &str) -> Option<Trait> {
        // Early saves could assign the pet-only trait to humans. Preserve those saves
        // without offering that trait to new Sims or in the human trait picker.
        if n == "Lazy" { return Some(Trait::Lazy); }
        Trait::ALL.into_iter().find(|t| t.name() == n)
    }

    /// Human age restrictions from Traits.xml's AgeSpeciesVisible column.
    pub fn allowed_at(self, age: Age) -> bool {
        use Trait::*;
        match self {
            Lazy => false,
            Charismatic | Childish | CommitmentIssues | DislikesChildren | Flirty | GreatKisser
            | GreenThumb | Handy | HopelessRomantic | NaturalCook | Schmoozer | Unflirty => {
                matches!(age, Age::Teen | Age::YoungAdult | Age::Adult | Age::Elder)
            }
            Absentminded | Artistic | Athletic | Brave | Clumsy | CouchPotato | EasilyImpressed
            | Evil | Excitable | Friendly | Genius | Good | Grumpy | HatesTheOutdoors
            | HeavySleeper | Insane | LightSleeper | Loner | LovesTheOutdoors | Neurotic
            | Slob | Virtuoso => true,
            _ => !age.is_little(),
        }
    }

    /// Traits sharing a SetNumbers exclusion group in the installed Traits.xml.
    fn conflicts(self, other: Trait) -> bool {
        use Trait::*;
        let pairs = [
            (Artistic, CantStandArt),
            (Athletic, CouchPotato),
            (Brave, Coward),
            (Brave, Loser),
            (CantStandArt, EasilyImpressed),
            (Charismatic, Loser),
            (Childish, DislikesChildren),
            (CommitmentIssues, HopelessRomantic),
            (ComputerWhiz, Technophobe),
            (CouchPotato, Technophobe),
            (Coward, Daredevil),
            (DislikesChildren, FamilyOriented),
            (EasilyImpressed, Snob),
            (Evil, Good),
            (Excitable, Grumpy),
            (Flirty, Unflirty),
            (Friendly, MeanSpirited),
            (GoodSenseOfHumor, NoSenseOfHumor),
            (Grumpy, HotHeaded),
            (HatesTheOutdoors, LovesTheOutdoors),
            (HeavySleeper, LightSleeper),
            (Loner, PartyAnimal),
            (Lucky, Unlucky),
            (Neat, Slob),
            // Retain the historical constraints only for old saves containing Lazy.
            (Lazy, Athletic),
            (Lazy, Workaholic),
            (Lazy, Ambitious),
        ];
        pairs.iter().any(|&(a, b)| (a == self && b == other) || (a == other && b == self))
    }

    pub fn compatible(self, list: &[Trait]) -> bool {
        !list.contains(&self) && !list.iter().any(|t| t.conflicts(self))
    }
}

/// How many traits a Sim of an age has.
pub fn trait_slots(age: Age) -> usize {
    match age {
        Age::Baby | Age::Toddler => 2,
        Age::Child => 3,
        Age::Teen => 4,
        _ => 5,
    }
}

pub fn random_traits(rng: &mut impl Rng, age: Age) -> Vec<Trait> {
    let mut all = Trait::ALL.to_vec();
    all.shuffle(rng);
    let mut out = Vec::new();
    for t in all {
        if out.len() >= trait_slots(age) {
            break;
        }
        if t.allowed_at(age) && t.compatible(&out) {
            out.push(t);
        }
    }
    out
}

/// The next trait after `current` (in list order) that fits with the others.
pub fn next_trait(current: Option<Trait>, others: &[Trait], age: Age) -> Option<Trait> {
    let start = current.and_then(|c| Trait::ALL.iter().position(|t| *t == c)).map_or(0, |i| i + 1);
    (0..Trait::ALL.len()).map(|k| Trait::ALL[(start + k) % Trait::ALL.len()]).find(|t| t.allowed_at(age) && t.compatible(others))
}

#[cfg(test)]
mod trait_age_tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn original_trait_learning_multipliers_are_skill_specific() {
        use Trait::*;
        for (t, skill, expected) in [
            (Artistic, "Painting", 1.25), (Artistic, "Guitar", 1.1), (Artistic, "Writing", 1.1),
            (Virtuoso, "Guitar", 1.25), (Genius, "Logic", 1.3), (NaturalCook, "Cooking", 1.3),
            (Athletic, "Athletic", 1.2), (Charismatic, "Charisma", 1.25),
            (GreenThumb, "Gardening", 1.25), (Handy, "Handiness", 1.25),
        ] {
            assert_eq!(skill_rate(&[t], skill), expected, "{t:?}: {skill}");
            assert_eq!(skill_rate(&[t], "Fishing"), 1.0);
        }
        assert!((skill_rate(&[Artistic, Virtuoso], "Guitar") - 1.375).abs() < 0.0001);
        assert_eq!(skill_rate(&[GreenThumb, Handy], "Gardening"), 1.25);
    }

    #[test]
    fn mood_learning_rates_follow_original_negative_and_super_mood_bands() {
        for (mood, expected) in [(-200.0, 0.25), (-100.0, 0.25), (-50.0, 0.625), (0.0, 1.0), (25.0, 1.0), (50.0, 1.0), (100.0, 1.5), (150.0, 2.0), (200.0, 2.0)] {
            assert!((Mood(mood).skill_rate() - expected).abs() < 0.0001, "mood {mood}");
        }
        let reward = crate::wishes::Wishes::restored(0, vec!["FastLearner".into()], 0.0);
        let rate = Mood(100.0).skill_rate() * crate::wishes::reward_skill_rate(Some(&reward));
        assert!((rate - 1.725).abs() < 0.0001, "mood and lifetime learning bonuses stack");
    }

    #[test]
    fn original_trait_conflicts_are_symmetric_and_allow_unrelated_personalities() {
        use Trait::*;
        for (a, b) in [(Friendly, Evil), (Excitable, OverEmotional), (Frugal, Snob), (Neat, Kleptomaniac), (Loner, Friendly)] {
            assert!(a.compatible(&[b]), "original game permits {a:?} + {b:?}");
        }
        for (a, b) in [(Athletic, CouchPotato), (Childish, DislikesChildren), (CommitmentIssues, HopelessRomantic), (Grumpy, HotHeaded)] {
            assert!(!a.compatible(&[b]), "original exclusion group for {a:?} + {b:?}");
        }
        let mut pairs = 0;
        for (i, a) in Trait::ALL.iter().enumerate() {
            assert!(!a.compatible(&[*a]));
            for b in &Trait::ALL[i + 1..] {
                assert_eq!(a.compatible(&[*b]), b.compatible(&[*a]));
                if a.conflicts(*b) { pairs += 1; }
            }
        }
        assert_eq!(pairs, 24);
    }

    #[test]
    fn original_age_groups_and_random_traits_remain_valid() {
        assert_eq!(Trait::ALL.iter().filter(|t| t.allowed_at(Age::Baby)).count(), 22);
        assert_eq!(Trait::ALL.iter().filter(|t| t.allowed_at(Age::Child)).count(), 52);
        assert_eq!(Trait::ALL.iter().filter(|t| t.allowed_at(Age::Teen)).count(), 64);
        assert_eq!(Trait::from_name("Lazy"), Some(Trait::Lazy));
        let mut rng = rand::rngs::StdRng::seed_from_u64(123);
        for age in [Age::Baby, Age::Toddler, Age::Child, Age::Teen, Age::YoungAdult, Age::Adult, Age::Elder] {
            for _ in 0..64 {
                let traits = random_traits(&mut rng, age);
                assert_eq!(traits.len(), trait_slots(age));
                for (i, t) in traits.iter().enumerate() {
                    assert!(t.allowed_at(age));
                    assert!(t.compatible(&traits[..i]));
                }
            }
            let mut picked = Vec::new();
            for _ in 0..trait_slots(age) {
                let t = next_trait(None, &picked, age).expect("enough compatible age-appropriate traits");
                assert!(t.allowed_at(age));
                picked.push(t);
            }
        }
    }
}

/// Skill learning speed, using TraitTuning's skill-specific multipliers.
pub fn skill_rate(traits: &[Trait], skill: &str) -> f32 {
    let mut r = 1.0;
    for t in traits {
        r *= match (t, skill) {
            (Trait::Artistic, "Painting") => 1.25,
            (Trait::Artistic, "Guitar" | "Writing") => 1.1,
            (Trait::Virtuoso, "Guitar") => 1.25,
            (Trait::Bookworm, "Writing") => 1.5,
            (Trait::Genius, "Logic" | "Chess" | "Hacking" | "Mooch") => 1.3,
            (Trait::NaturalCook, "Cooking") => 1.3,
            (Trait::Athletic, "Athletic") => 1.2,
            (Trait::Charismatic, "Charisma") => 1.25,
            (Trait::GreenThumb, "Gardening") => 1.25,
            (Trait::Handy, "Handiness") => 1.25,
            (Trait::Lazy, _) => 0.85,
            _ => 1.0,
        };
    }
    r
}

/// Need decay speed.
pub fn decay_rate(traits: &[Trait], motive: usize) -> f32 {
    let mut r = 1.0;
    for t in traits {
        r *= match (t, motive) {
            (Trait::Lazy, ENERGY) => 1.25,
            (Trait::Loner, SOCIAL) => 0.5,
            (Trait::PartyAnimal | Trait::Friendly, SOCIAL) => 1.2,
            (Trait::Slob, HYGIENE) => 0.75,
            (Trait::Excitable, FUN) => 1.2,
            (Trait::CouchPotato, ENERGY) => 0.9,
            _ => 1.0,
        };
    }
    r
}

/// How much an activity's fun (and the wish to do it) is scaled by personality.
pub fn activity_affinity(traits: &[Trait], activity: &str) -> f32 {
    let mut r = 1.0;
    for t in traits {
        r *= match (t, activity) {
            (Trait::CouchPotato, "Watch TV" | "Watch Cooking Channel") => 2.0,
            (Trait::Bookworm, "Read a Book") => 2.0,
            (Trait::Athletic, "Work Out") => 2.0,
            (Trait::Lazy, "Work Out") => 0.3,
            (Trait::Lazy, "Nap" | "Relax" | "Watch TV") => 1.5,
            (Trait::ComputerWhiz, "Play Computer Games") => 1.5,
            (Trait::Technophobe, "Play Computer Games" | "Write Novel" | "Watch TV") => 0.3,
            (Trait::Artistic, "Paint" | "Play Guitar") => 1.6,
            (Trait::Virtuoso, "Play Guitar") => 2.0,
            (Trait::PartyAnimal, "Dance" | "Dance Together") => 1.8,
            (Trait::NaturalCook, "Cook Dinner") => 1.8,
            (Trait::Genius, "Play Chess") => 1.6,
            (Trait::Hydrophobic, "Take Shower" | "Take Bath") => 0.4,
            (Trait::Neat, "Take Shower" | "Take Bath" | "Wash Hands") => 1.4,
            (Trait::Childish, "Play Computer Games" | "Dance") => 1.4,
            _ => 1.0,
        };
    }
    r
}

/// Relationship gained from a social interaction, scaled by both personalities.
pub fn social_affinity(traits: &[Trait], social: &str) -> f32 {
    let mut r = 1.0;
    for t in traits {
        r *= match (t, social) {
            (Trait::Friendly, "Chat" | "Compliment" | "Hug") => 1.5,
            (Trait::Charismatic, _) => 1.25,
            (Trait::GoodSenseOfHumor, "Tell Joke") => 1.8,
            (Trait::Grumpy, "Tell Joke") => 0.4,
            (Trait::HopelessRomantic | Trait::Flirty, "Flirt" | "Kiss" | "Make Out" | "Hug") => 1.5,
            (Trait::Unflirty, "Flirt" | "Kiss" | "Make Out") => 0.4,
            (Trait::MeanSpirited, "Argue" | "Insult") => 1.6,
            (Trait::Evil, "Argue" | "Insult" | "Slap") => 1.8,
            (Trait::GreatKisser, "Kiss" | "Make Out") => 1.8,
            (Trait::Inappropriate, "Flirt" | "Tell Joke") => 1.2,
            (Trait::Schmoozer, "Compliment" | "Chat") => 1.3,
            (Trait::Mooch, _) => 0.95,
            (Trait::Loner, _) => 0.8,
            _ => 1.0,
        };
    }
    r
}

/// Career performance speed.
pub fn work_rate(traits: &[Trait]) -> f32 {
    let mut r = 1.0;
    for t in traits {
        r *= match t {
            Trait::Ambitious => 1.25,
            Trait::Workaholic => 1.2,
            Trait::Lazy => 0.8,
            Trait::Absentminded => 0.9,
            Trait::Schmoozer => 1.15,
            Trait::Loser => 0.85,
            Trait::Lucky => 1.05,
            Trait::Unlucky => 0.95,
            _ => 1.0,
        };
    }
    r
}

/// Energy regained while sleeping.
pub fn sleep_rate(traits: &[Trait]) -> f32 {
    if traits.contains(&Trait::HeavySleeper) {
        1.2
    } else if traits.contains(&Trait::LightSleeper) {
        0.85
    } else {
        1.0
    }
}

// ---------------------------------------------------------------------------------------------
// Moodlets

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MoodletKind {
    // From needs (present while the need is low).
    Hungry,
    Starving,
    StrainedBladder,
    Tired,
    Exhausted,
    Lonely,
    Smelly,
    Bored,
    // From events.
    WellRested,
    Comfy,
    SqueakyClean,
    GoodMeal,
    AmazingMeal,
    HavingFun,
    GoodConversation,
    EnjoyingMusic,
    EnjoyingAGoodBook,
    Pumped,
    Fatigued,
    Inspired,
    Embarrassed,
    PassedOut,
    Uncomfortable,
    Promoted,
    Demoted,
    Fired,
    NewJob,
    Annoyed,
    Flirty,
    FirstKiss,
    InLove,
    Heartbroken,
    JustMarried,
    NewHome,
    Birthday,
    WishFulfilled,
    Nauseous,
    Pregnant,
    NewBaby,
    GreatParty,
    LameParty,
    AwesomeParty,
    Scared,
    GreatDate,
    BadDate,
    Singed,
    DirtySurroundings,
    FilthySurroundings,
    // The seasons: a Sim's temperature, and rain.
    GettingChilly,
    TeethChattering,
    Frostbitten,
    GettingWarm,
    SweatingProfusely,
    Soaked,
    // Supernaturals.
    TooMuchSun,
    Werewolf,
}

impl MoodletKind {
    pub const ALL: [MoodletKind; 56] = {
        use MoodletKind::*;
        [
            Hungry, Starving, StrainedBladder, Tired, Exhausted, Lonely, Smelly, Bored, WellRested, Comfy, SqueakyClean,
            GoodMeal, AmazingMeal, HavingFun, GoodConversation, EnjoyingMusic, EnjoyingAGoodBook, Pumped, Fatigued,
            Inspired, Embarrassed, PassedOut, Uncomfortable, Promoted, Demoted, Fired, NewJob, Annoyed, Flirty, FirstKiss,
            InLove, Heartbroken, JustMarried, NewHome, Birthday, WishFulfilled, Nauseous, Pregnant, NewBaby, GreatParty,
            LameParty, AwesomeParty, Scared, GreatDate, BadDate, Singed, DirtySurroundings, FilthySurroundings, GettingChilly,
            TeethChattering, Frostbitten, GettingWarm, SweatingProfusely, Soaked, TooMuchSun, Werewolf,
        ]
    };

    pub fn from_name(n: &str) -> Option<MoodletKind> {
        MoodletKind::ALL.into_iter().find(|k| k.def().name == n)
    }
}

pub struct MoodletDef {
    pub name: &'static str,
    pub desc: &'static str,
    pub value: i32,
    /// How long it lasts (game hours); 0 for need moodlets that last while the need is low.
    pub hours: f32,
}

const fn md(name: &'static str, desc: &'static str, value: i32, hours: f32) -> MoodletDef {
    MoodletDef { name, desc, value, hours }
}

impl MoodletKind {
    pub fn def(self) -> MoodletDef {
        use MoodletKind::*;
        match self {
            Hungry => md("Hungry", "Something to eat would be great.", -20, 0.0),
            Starving => md("Starving", "Must eat now!", -80, 0.0),
            StrainedBladder => md("Strained Bladder", "Where's the bathroom?", -20, 0.0),
            Tired => md("Tired", "A nap would be nice.", -20, 0.0),
            Exhausted => md("Exhausted", "Can barely stay awake.", -60, 0.0),
            Lonely => md("Lonely", "Needs some company.", -15, 0.0),
            Smelly => md("Smelly", "Time for a shower.", -15, 0.0),
            Bored => md("Bored", "Nothing to do...", -15, 0.0),
            WellRested => md("Well Rested", "A good night's sleep.", 15, 6.0),
            Comfy => md("Comfy", "Sitting somewhere comfortable.", 10, 1.0),
            SqueakyClean => md("Squeaky Clean", "Freshly washed.", 15, 4.0),
            GoodMeal => md("Good Meal", "That hit the spot.", 10, 3.0),
            AmazingMeal => md("Amazing Meal", "A culinary masterpiece!", 25, 4.0),
            HavingFun => md("Having Fun", "That was a blast.", 10, 2.0),
            GoodConversation => md("Good Conversation", "Nice to have a chat.", 15, 3.0),
            EnjoyingMusic => md("Enjoying the Music", "Great tunes!", 10, 2.0),
            EnjoyingAGoodBook => md("Enjoying a Good Book", "Lost in a story.", 15, 3.0),
            Pumped => md("Pumped", "Feeling the burn!", 15, 3.0),
            Fatigued => md("Fatigued", "That workout was too much.", -10, 2.0),
            Inspired => md("Inspired", "Creativity is flowing.", 15, 3.0),
            Embarrassed => md("Embarrassed", "Didn't make it to the bathroom...", -20, 3.0),
            PassedOut => md("Passed Out", "Collapsed from exhaustion.", -25, 4.0),
            Uncomfortable => md("Uncomfortable", "That was not a proper bed.", -10, 3.0),
            Promoted => md("Promoted!", "Moving up in the world!", 40, 24.0),
            Demoted => md("Demoted", "That stings.", -30, 24.0),
            Fired => md("Fired", "Lost the job.", -40, 48.0),
            NewJob => md("Starting a New Job", "A fresh start!", 15, 12.0),
            Annoyed => md("Annoyed", "That was unpleasant.", -15, 3.0),
            Flirty => md("Flirty", "Feeling a spark.", 15, 3.0),
            FirstKiss => md("First Kiss", "Magical!", 30, 24.0),
            InLove => md("In Love", "Head over heels.", 25, 24.0),
            Heartbroken => md("Heartbroken", "It's over...", -40, 48.0),
            JustMarried => md("Just Married", "Happily ever after!", 50, 72.0),
            NewHome => md("Settling In", "A new place to call home.", 15, 24.0),
            Birthday => md("It's My Birthday!", "Another year older!", 20, 24.0),
            Nauseous => md("Nauseous", "Something doesn't agree with this Sim this morning.", -15, 6.0),
            Pregnant => md("Pregnant", "A little one is on the way!", 20, 48.0),
            NewBaby => md("It's a Baby!", "A new addition to the family!", 40, 24.0),
            GreatParty => md("Threw a Great Party", "Sims love a great party and the host that throws them.", 30, 24.0),
            LameParty => md("Threw a Lame Party", "Not every party is a hit.", -15, 8.0),
            AwesomeParty => md("Awesome Party", "What a party!", 20, 3.0),
            Scared => md("Scared", "Something gave this Sim a terrible fright!", -25, 3.0),
            GreatDate => md("Great Date", "It was an absolutely fantastic date!", 15, 3.0),
            BadDate => md("Bad Date", "Ugh, what a lousy date.", -5, 3.0),
            Singed => md("Singed", "Electrocuted! Another shock now could stop their heart.", -40, 6.0),
            DirtySurroundings => md("Dirty Surroundings", "Dirty dishes, garbage and spoiled food are never a kind sight to the eyes, or nose...", -15, 0.0),
            FilthySurroundings => md("Filthy Surroundings", "The grime and muck is really starting to pile high.", -30, 0.0),
            WishFulfilled => md("Wish Fulfilled", "Dreams come true!", 10, 4.0),
            GettingChilly => md("Getting Chilly", "The air has a bit of a bite to it!", 0, 0.0),
            TeethChattering => md("Teeth Chattering", "So cold the teeth won't stop chattering.", -10, 0.0),
            Frostbitten => md("Frostbitten", "Brrrr!! It is now officially too cold.", -10, 3.0),
            GettingWarm => md("Getting Warm", "A little heat never hurts!", 0, 0.0),
            SweatingProfusely => md("Sweating Profusely", "Whoof! It is a sauna right now!", -10, 0.0),
            Soaked => md("Soaked", "Being soaked is never really comfortable.", -5, 2.0),
            TooMuchSun => md("Too Much Sun", "The sun is draining this vampire.", -20, 0.0),
            Werewolf => md("Werewolf", "The full moon has brought out the beast within.", 15, 0.0),
        }
    }

    fn from_needs(self) -> bool {
        self.def().hours == 0.0
    }

    /// The game's buff for this moodlet (its icon), and whether it is the same moodlet (so
    /// its name and description apply) rather than a near match.
    pub fn buff(self) -> (&'static str, bool) {
        use MoodletKind::*;
        match self {
            Hungry => ("Hungry", true),
            Starving => ("Starving", true),
            StrainedBladder => ("HasToPee", true),
            Tired => ("Tired", true),
            Exhausted => ("Exhausted", true),
            Lonely => ("Lonely", true),
            Smelly => ("Smelly", true),
            Bored => ("Bored", true),
            WellRested => ("WellRested", true),
            Comfy => ("Comfy", true),
            SqueakyClean => ("SqueakyClean", true),
            GoodMeal => ("Meal", false),
            AmazingMeal => ("DivineMeal", true),
            HavingFun => ("Excited", false),
            GoodConversation => ("BrightenedDay", true),
            EnjoyingMusic => ("EnjoyingMusic", true),
            EnjoyingAGoodBook => ("ReadAMasterpiece", false),
            Pumped => ("Pumped", true),
            Fatigued => ("Fatigued", true),
            Inspired => ("Inspired", true),
            Embarrassed => ("Embarrassed", true),
            PassedOut => ("KnockedOut", false),
            Uncomfortable => ("Backache", true),
            Promoted => ("Victory", false),
            Demoted => ("Disappointed", false),
            Fired => ("Fired", true),
            NewJob => ("FreshStart", false),
            Annoyed => ("Upset", false),
            Flirty => ("Flattered", false),
            FirstKiss => ("FirstKiss", true),
            InLove => ("MyLove", false),
            Heartbroken => ("HeartBroken", true),
            JustMarried => ("JustMarried", true),
            NewHome => ("NewHouse", true),
            Birthday => ("CelebratedBirthday", true),
            WishFulfilled => ("Fulfilled", false),
            Nauseous => ("Nauseous", true),
            Pregnant => ("Pregnant", true),
            NewBaby => ("ItsABoy", false),
            GreatParty => ("ThrewAGreatParty", true),
            LameParty => ("ThrewLameParty", true),
            AwesomeParty => ("AwesomeParty", true),
            Scared => ("Scared", false),
            GreatDate => ("GreatDate", true),
            BadDate => ("BadDate", true),
            Singed => ("SingedElectricity", true),
            DirtySurroundings => ("DirtySurroundings", true),
            FilthySurroundings => ("FilthySurroundings", true),
            GettingChilly => ("GettingChilly", true),
            TeethChattering => ("TeethChattering", true),
            Frostbitten => ("Frostbitten", true),
            GettingWarm => ("GettingWarm", true),
            SweatingProfusely => ("SweatingProfusely", true),
            Soaked => ("Soaked", true),
            TooMuchSun => ("TooMuchSun", true),
            Werewolf => ("Werewolf", true),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Moodlet {
    pub kind: MoodletKind,
    pub value: i32,
    /// Game minute it ends (infinite for need moodlets).
    pub until: f64,
}

#[derive(Component, Default, Clone)]
pub struct Moodlets(pub Vec<Moodlet>);

impl Moodlets {
    /// Adds an event moodlet (refreshing it if already there).
    pub fn add(&mut self, kind: MoodletKind, now: f64) {
        let d = kind.def();
        let until = now + d.hours as f64 * 60.0;
        if let Some(m) = self.0.iter_mut().find(|m| m.kind == kind) {
            m.until = m.until.max(until);
            return;
        }
        self.0.push(Moodlet { kind, value: d.value, until });
    }

    pub fn remove(&mut self, kind: MoodletKind) {
        self.0.retain(|m| m.kind != kind);
    }

    pub fn has(&self, kind: MoodletKind) -> bool {
        self.0.iter().any(|m| m.kind == kind)
    }

    /// A moodlet present while its cause lasts.
    pub fn set_while(&mut self, kind: MoodletKind, on: bool) {
        self.set_need(kind, on);
    }

    fn set_need(&mut self, kind: MoodletKind, on: bool) {
        let present = self.has(kind);
        if on && !present {
            self.0.push(Moodlet { kind, value: kind.def().value, until: f64::INFINITY });
        } else if !on && present {
            self.remove(kind);
        }
    }
}

/// Overall mood: the sum of the moodlets.
#[derive(Component, Default, Clone, Copy)]
pub struct Mood(pub f32);

impl Mood {
    /// MoodManager's learning modifiers: -.75% per negative mood point,
    /// +1% per point in the super-mood band (50..150).
    pub fn skill_rate(self) -> f32 {
        if self.0 < 0.0 {
            1.0 + self.0.max(-100.0) * 0.0075
        } else {
            1.0 + (self.0.clamp(50.0, 150.0) - 50.0) * 0.01
        }
    }

    pub fn label(self) -> &'static str {
        match self.0 {
            m if m >= 60.0 => "Very Happy",
            m if m >= 20.0 => "Happy",
            m if m > -20.0 => "Fine",
            m if m > -60.0 => "Uncomfortable",
            _ => "Miserable",
        }
    }

    /// -100..100 scale for colour and performance.
    pub fn level(self) -> f32 {
        (self.0 * 1.2).clamp(-100.0, 100.0)
    }
}

/// Something that happened to a Sim, feeding moodlets, wishes and careers.
#[derive(Message, Clone, Debug)]
pub struct LifeEvent {
    pub sim: Entity,
    pub kind: LifeEventKind,
}

#[derive(Clone, Debug)]
pub enum LifeEventKind {
    /// An object interaction finished (`completed`: ran its full course or filled its need).
    Finished { activity: &'static str, completed: bool },
    /// A social interaction with someone finished.
    Socialized { other: Entity, social: &'static str },
    MadeFriend { other: Entity },
    Promoted,
    Demoted,
    Fired,
    NewJob,
    SkillUp { skill: &'static str, level: u32 },
    FirstKiss,
    StartedDating,
    Engaged,
    Married,
    BrokeUp,
    MovedIn,
    Birthday,
    Bought { price: i32 },
}

impl LifeEvent {
    pub fn new(sim: Entity, kind: LifeEventKind) -> Self {
        Self { sim, kind }
    }
}

fn need_moodlets(mut q: Query<(&Motives, &Sim, &mut Moodlets)>) {
    for (m, sim, mut ml) in &mut q {
        let v = m.0;
        ml.set_need(MoodletKind::Starving, v[HUNGER] < -70.0);
        ml.set_need(MoodletKind::Hungry, (-70.0..-25.0).contains(&v[HUNGER]));
        ml.set_need(MoodletKind::StrainedBladder, v[BLADDER] < -35.0);
        ml.set_need(MoodletKind::Exhausted, v[ENERGY] < -70.0);
        ml.set_need(MoodletKind::Tired, (-70.0..-25.0).contains(&v[ENERGY]));
        ml.set_need(MoodletKind::Lonely, v[SOCIAL] < -30.0 && !sim.traits.contains(&Trait::Loner));
        ml.set_need(MoodletKind::Smelly, v[HYGIENE] < -35.0 && !sim.traits.contains(&Trait::Slob));
        ml.set_need(MoodletKind::Bored, v[FUN] < -35.0);
    }
}

/// Bladder failure and passing out (the game's own animations: the accident; collapsing,
/// sleeping on the floor a couple of hours and getting up again).
fn need_failures(
    clock: Res<GameClock>,
    mut q: Query<(Entity, &Sim, &mut Motives, &mut Moodlets, &mut crate::interact::ActionQueue, &InheritedVisibility)>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    use crate::interact::{Action, ActionKind};
    for (_, sim, mut m, mut ml, mut queue, vis) in &mut q {
        // (Babies and toddlers are seen to by others; away, it happens off stage.)
        let shown = vis.get() && !sim.age.is_little();
        let failing = |q: &crate::interact::ActionQueue| q.0.iter().any(|a| matches!(a.kind, ActionKind::MotiveFail(_)) && !a.cancel);
        if m.0[BLADDER] <= -99.0 {
            m.0[BLADDER] = 100.0;
            m.add(HYGIENE, -60.0);
            ml.add(MoodletKind::Embarrassed, clock.minutes);
            notes.push(format!("{} couldn't make it to the bathroom in time!", sim.first));
            if shown && !failing(&queue) {
                for a in queue.0.iter_mut() {
                    a.cancel = true;
                }
                queue.0.push_back(Action::new("Accident", ActionKind::MotiveFail(0), false));
            }
        }
        if m.0[ENERGY] <= -99.0 && !failing(&queue) {
            ml.add(MoodletKind::PassedOut, clock.minutes);
            for a in queue.0.iter_mut() {
                a.cancel = true;
            }
            notes.push(format!("{} passed out from exhaustion.", sim.first));
            if shown {
                queue.0.push_back(Action::new("Passed Out", ActionKind::MotiveFail(1), false));
            } else {
                m.0[ENERGY] = 30.0;
            }
        }
    }
}

/// Turns life events into moodlets.
fn life_events(
    clock: Res<GameClock>,
    mut events: MessageReader<LifeEvent>,
    mut q: Query<(&Sim, &Motives, &mut Moodlets, &Skills)>,
) {
    let now = clock.minutes;
    for ev in events.read() {
        let Ok((sim, m, mut ml, skills)) = q.get_mut(ev.sim) else { continue };
        let has = |t: Trait| sim.traits.contains(&t);
        use MoodletKind as K;
        match &ev.kind {
            LifeEventKind::Finished { activity, completed } => match *activity {
                "Sleep" if m.0[ENERGY] >= 95.0 => ml.add(K::WellRested, now),
                "Nap" if *completed => {}
                "Take Shower" | "Take Bath" if m.0[HYGIENE] >= 90.0 => {
                    if has(Trait::Hydrophobic) {
                        ml.add(K::Annoyed, now);
                    } else {
                        ml.add(K::SqueakyClean, now);
                    }
                }
                "Cook Dinner" => {
                    if skills.level("Cooking") >= 5 || has(Trait::NaturalCook) {
                        ml.add(K::AmazingMeal, now);
                    } else {
                        ml.add(K::GoodMeal, now);
                    }
                }
                "Have Quick Meal" | "Microwave Dinner" if *completed => ml.add(K::GoodMeal, now),
                "Sit" => ml.add(K::Comfy, now),
                "Dance" => ml.add(K::EnjoyingMusic, now),
                "Read a Book" if has(Trait::Bookworm) => ml.add(K::EnjoyingAGoodBook, now),
                "Work Out" => {
                    if has(Trait::Athletic) {
                        ml.add(K::Pumped, now)
                    } else if has(Trait::Lazy) {
                        ml.add(K::Fatigued, now)
                    }
                }
                "Paint" | "Play Guitar" | "Write Novel" if has(Trait::Artistic) || has(Trait::Virtuoso) || has(Trait::Bookworm) => {
                    ml.add(K::Inspired, now)
                }
                a if *completed && m.0[FUN] > 70.0 && !matches!(a, "Use Toilet" | "Wash Hands") => ml.add(K::HavingFun, now),
                _ => {}
            },
            LifeEventKind::Socialized { social, .. } => match *social {
                "Argue" | "Insult" => {
                    if !has(Trait::MeanSpirited) {
                        ml.add(K::Annoyed, now)
                    }
                }
                "Flirt" => ml.add(K::Flirty, now),
                "Kiss" | "Make Out" => ml.add(K::InLove, now),
                _ if m.0[SOCIAL] > 70.0 => ml.add(K::GoodConversation, now),
                _ => {}
            },
            LifeEventKind::Promoted => ml.add(K::Promoted, now),
            LifeEventKind::Demoted => ml.add(K::Demoted, now),
            LifeEventKind::Fired => ml.add(K::Fired, now),
            LifeEventKind::NewJob => ml.add(K::NewJob, now),
            LifeEventKind::FirstKiss => ml.add(K::FirstKiss, now),
            LifeEventKind::StartedDating | LifeEventKind::Engaged => ml.add(K::InLove, now),
            LifeEventKind::Married => ml.add(K::JustMarried, now),
            LifeEventKind::BrokeUp => ml.add(K::Heartbroken, now),
            LifeEventKind::MovedIn => ml.add(K::NewHome, now),
            LifeEventKind::Birthday => ml.add(K::Birthday, now),
            LifeEventKind::SkillUp { .. } | LifeEventKind::Bought { .. } | LifeEventKind::MadeFriend { .. } => {}
        }
    }
}

fn expire_and_sum(clock: Res<GameClock>, delta: Res<SimDelta>, mut q: Query<(&mut Moodlets, &mut Mood, Option<&crate::wishes::Wishes>, Option<&crate::weather::BodyTemperature>)>) {
    for (mut ml, mut mood, wishes, temperature) in &mut q {
        let now = clock.minutes;
        if let Some(t) = temperature {
            // SimTemperature: pause below the -60 unpause threshold; above -1,
            // subtract another 60 minutes per hour on top of normal expiration.
            if let Some(m) = ml.0.iter_mut().find(|m| m.kind == MoodletKind::Frostbitten) {
                if t.value <= -60.0 {
                    m.until += delta.0.max(0.0) as f64;
                } else if t.value > -1.0 {
                    m.until -= delta.0.max(0.0) as f64;
                }
            }
        }
        if ml.0.iter().any(|m| m.until <= now) {
            ml.0.retain(|m| m.until > now);
        }
        let sum: i32 = ml.0.iter().map(|m| m.value).sum();
        // A content Sim with nothing on their mind is fine; moodlets push it up or down.
        let _ = wishes;
        let v = sum as f32 + 10.0;
        if (mood.0 - v).abs() > 0.01 {
            mood.0 = v;
        }
    }
}

#[cfg(test)]
mod temperature_moodlet_tests {
    use super::*;

    #[test]
    fn frostbite_timer_pauses_in_cold_and_recovers_faster_when_warm() {
        let mut app = App::new();
        app.init_resource::<GameClock>()
            .insert_resource(SimDelta(30.0))
            .add_systems(Update, expire_and_sum);
        let start = app.world().resource::<GameClock>().minutes;
        let mut ml = Moodlets::default();
        ml.add(MoodletKind::Frostbitten, start);
        let e = app.world_mut().spawn((ml, Mood::default(), crate::weather::BodyTemperature { value: -60.0, in_rain: 0.0 })).id();
        // Four hours of cold must not consume any of the three-hour recovery timer.
        for _ in 0..8 {
            app.world_mut().resource_mut::<GameClock>().minutes += 30.0;
            app.update();
        }
        let remaining = |app: &App| app.world().get::<Moodlets>(e).unwrap().0[0].until - app.world().resource::<GameClock>().minutes;
        assert_eq!(remaining(&app), 180.0);
        // At exactly -1 normal recovery applies.
        app.world_mut().get_mut::<crate::weather::BodyTemperature>(e).unwrap().value = -1.0;
        app.world_mut().resource_mut::<GameClock>().minutes += 30.0;
        app.update();
        assert_eq!(remaining(&app), 150.0);
        app.world_mut().get_mut::<crate::weather::BodyTemperature>(e).unwrap().value = 0.0;
        app.world_mut().resource_mut::<SimDelta>().0 = 0.0;
        app.update();
        assert_eq!(remaining(&app), 150.0, "paused simulation must not recover frostbite");
        app.world_mut().resource_mut::<SimDelta>().0 = 30.0;
        for expected in [90.0, 30.0] {
            app.world_mut().resource_mut::<GameClock>().minutes += 30.0;
            app.update();
            assert_eq!(remaining(&app), expected);
        }
        app.world_mut().resource_mut::<GameClock>().minutes += 30.0;
        app.update();
        assert!(!app.world().get::<Moodlets>(e).unwrap().has(MoodletKind::Frostbitten));
    }
}

/// Short text for the moodlets panel: "Name (+15)".
pub fn moodlet_label(m: &Moodlet) -> String {
    let d = m.kind.def();
    if m.value >= 0 { format!("{} +{}", d.name, m.value) } else { format!("{} {}", d.name, m.value) }
}

pub fn moodlet_is_need(m: &Moodlet) -> bool {
    m.kind.from_needs()
}
