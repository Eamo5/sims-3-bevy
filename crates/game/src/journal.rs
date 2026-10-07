//! Skill journals: what each Sim has done with their skills (dishes prepared, fish caught,
//! objects repaired…), and the game's skill challenges, each earned by doing enough of one
//! thing and bringing a reward: Plumbers' repairs never break again, Electricians are never
//! shocked, Master Painters' paintings sell for double. The challenges, their thresholds (the
//! script's own tuning) and their texts are the game's. A journal is opened by clicking a skill
//! in the Skills tab; it's kept in saves.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{Notifications, Skills};
use crate::sim::{HouseholdMember, Sim};
use crate::social::Relationships;

pub struct JournalPlugin;

impl Plugin for JournalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Did>().add_systems(Update, (keep_tallies, earn_challenges).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// What's counted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stat {
    StrengthHours,
    CardioHours,
    Jokes,
    Dishes,
    Fish,
    Harvested,
    PerfectHarvested,
    ElectricalRepairs,
    PlumbingRepairs,
    Paintings,
    BrilliantPaintings,
    Masterpieces,
    Royalties,
    StarsFound,
    GuitarHours,
    KmJogged,
}

impl Stat {
    fn key(self) -> &'static str {
        match self {
            Stat::StrengthHours => "strength_hours",
            Stat::CardioHours => "cardio_hours",
            Stat::Jokes => "jokes",
            Stat::Dishes => "dishes",
            Stat::Fish => "fish",
            Stat::Harvested => "harvested",
            Stat::PerfectHarvested => "perfect_harvested",
            Stat::ElectricalRepairs => "electrical_repairs",
            Stat::PlumbingRepairs => "plumbing_repairs",
            Stat::Paintings => "paintings",
            Stat::BrilliantPaintings => "brilliant_paintings",
            Stat::Masterpieces => "masterpieces",
            Stat::Royalties => "royalties",
            Stat::StarsFound => "stars_found",
            Stat::GuitarHours => "guitar_hours",
            Stat::KmJogged => "km_jogged",
        }
    }
}

/// Kinds of things counted once each (fish caught, plants planted, upgrades made).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kinds {
    FishTypes,
    PlantTypes,
    Upgrades,
}

impl Kinds {
    fn key(self) -> &'static str {
        match self {
            Kinds::FishTypes => "fish_types",
            Kinds::PlantTypes => "plant_types",
            Kinds::Upgrades => "upgrades",
        }
    }
}

/// Something done towards a journal: a count, or one more of a kind of thing.
#[derive(Message, Clone, Debug)]
pub struct Did {
    pub sim: Entity,
    pub what: Deed,
}

#[derive(Clone, Debug)]
pub enum Deed {
    Count(Stat, f64),
    Kind(Kinds, String),
}

impl Did {
    pub fn count(sim: Entity, stat: Stat, n: f64) -> Self {
        Self { sim, what: Deed::Count(stat, n) }
    }
    pub fn kind(sim: Entity, kinds: Kinds, name: impl Into<String>) -> Self {
        Self { sim, what: Deed::Kind(kinds, name.into()) }
    }
}

/// A Sim's skill journal: their tallies, the kinds of things they've done, and the challenges
/// earned.
#[derive(Component, Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct SkillJournal {
    #[serde(default)]
    pub tally: BTreeMap<String, f64>,
    #[serde(default)]
    pub kinds: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub earned: Vec<String>,
}

impl SkillJournal {
    pub fn is_empty(&self) -> bool {
        self.tally.is_empty() && self.kinds.is_empty() && self.earned.is_empty()
    }
    pub fn get(&self, s: Stat) -> f64 {
        self.tally.get(s.key()).copied().unwrap_or(0.0)
    }
    pub fn kinds(&self, k: Kinds) -> usize {
        self.kinds.get(k.key()).map_or(0, |v| v.len())
    }
    /// Whether they've earned the challenge (by name).
    pub fn has(&self, challenge: &str) -> bool {
        self.earned.iter().any(|c| c == challenge)
    }
    fn add(&mut self, d: &Deed) {
        match d {
            Deed::Count(s, n) => *self.tally.entry(s.key().to_string()).or_default() += n,
            Deed::Kind(k, name) => {
                let v = self.kinds.entry(k.key().to_string()).or_default();
                if !v.contains(name) {
                    v.push(name.clone());
                }
            }
        }
    }
}

/// Whether a Sim (perhaps without a journal) has earned a challenge.
pub fn earned(j: Option<&SkillJournal>, challenge: &str) -> bool {
    j.is_some_and(|j| j.has(challenge))
}

/// How a challenge is measured.
#[derive(Clone, Copy, Debug)]
pub enum Measure {
    Stat(Stat),
    Kinds(Kinds),
    Friends,
    BestFriends,
    RecipesKnown,
    ChessRank,
    BooksWritten,
    /// The most books written in any one genre.
    OneGenre,
}

/// One of the game's skill challenges.
pub struct Challenge {
    pub skill: &'static str,
    pub name: &'static str,
    /// The journal's description ({0}: what's needed).
    pub desc: &'static str,
    pub need: f64,
    pub measure: Measure,
    /// The notice when it's earned, after the Sim's name.
    pub done: &'static str,
    /// What's needed shown as money.
    pub money: bool,
}

const fn ch(skill: &'static str, name: &'static str, need: f64, measure: Measure, desc: &'static str, done: &'static str) -> Challenge {
    Challenge { skill, name, desc, need, measure, done, money: false }
}

/// The game's skill challenges for the skills there are (their thresholds the scripts' own).
pub static CHALLENGES: &[Challenge] = &[
    ch(
        "Athletic",
        "Body Builder",
        200.0,
        Measure::Stat(Stat::StrengthHours),
        "Body Builders have dedicated at least {0} hours to strength workouts. This dedication pays off, because they are never fatigued after strength workouts.",
        "has dedicated enough hours to strength training to complete the Body Builder Skill Challenge!",
    ),
    ch(
        "Athletic",
        "Fitness Nut",
        200.0,
        Measure::Stat(Stat::CardioHours),
        "Fitness Nuts have spent {0} hours focusing on cardio workouts. All that time experiencing the burn means they are no longer fatigued after cardio workouts.",
        "has dedicated enough hours to cardio exercise to complete the Fitness Nut Skill Challenge!",
    ),
    ch(
        "Athletic",
        "Marathon Runner",
        100.0,
        Measure::Stat(Stat::KmJogged),
        "Marathon Runners must run at least {0} kilometers before they earn the title. However, accomplishing this incredible feat guarantees them a longer, healthier life.",
        "has run far enough to complete Marathon Runner Skill Challenge!",
    ),
    ch(
        "Charisma",
        "Super Friendly",
        30.0,
        Measure::Friends,
        "Super Friendly Sims can honestly say they have at least {0} Friends. It seems like an impossibly large number of relationships to juggle, but for Super Friendly Sims, friendships never decay.",
        "has made enough Friends to complete the Super Friendly Skill Challenge!",
    ),
    ch(
        "Charisma",
        "Everybody's Best Friend",
        15.0,
        Measure::BestFriends,
        "To be Everybody's Best Friend, have at least {0} best friends. Your Friends skip Good Friend and jump immediately to Best Friends.",
        "has made enough Best Friends to complete the Everybody's Best Friend Skill Challenge!",
    ),
    ch(
        "Charisma",
        "Comedian",
        2.0,
        Measure::Stat(Stat::Jokes),
        "Comedians have successfully told {0} jokes, which amounts to quite a few laughs. Jokes told by comedians rarely fall flat.",
        "has told enough successful jokes to complete the Comedian Skill Challenge!",
    ),
    ch(
        "Cooking",
        "Star Chef",
        50.0,
        Measure::Stat(Stat::Dishes),
        "Star Chefs have prepared at least {0} dishes, and it shows: their cooking comes out a little better.",
        "has prepared enough meals to complete the Star Chef Skill Challenge!",
    ),
    ch(
        "Cooking",
        "World-Class Chef",
        100.0,
        Measure::Stat(Stat::Dishes),
        "World-Class Chefs have prepared at least {0} dishes and are masters of the kitchen. World-Class chefs are known to prepare meals significantly faster.",
        "has prepared enough meals to complete the World-Class Chef Skill Challenge!",
    ),
    ch(
        "Cooking",
        "Menu Maven",
        28.0,
        Measure::RecipesKnown,
        "Menu Mavens have learned to prepare at least {0} recipes. Recipes are earned by improving the cooking skill and can be purchased at the bookstore. Menu Mavens prepare higher quality food.",
        "has learned 28 recipes to complete the Menu Maven Skill Challenge!",
    ),
    ch(
        "Fishing",
        "Commercial Fisherman",
        150.0,
        Measure::Stat(Stat::Fish),
        "Commercial Fishermen have caught at least {0} fish. They often catch more fish in less time than normal Sims.",
        "has caught enough fish to complete the Commercial Fisherman Skill Challenge!",
    ),
    ch(
        "Fishing",
        "Amateur Ichthyologist",
        20.0,
        Measure::Kinds(Kinds::FishTypes),
        "Amateur Ichthyologists have caught at least {0} types of fish. Their deep understanding of marine life helps them catch bigger fish than normal Sims.",
        "has caught 20 types of fish to complete the Amateur Ichthyologist Skill Challenge!",
    ),
    ch(
        "Gardening",
        "Master Farmer",
        1000.0,
        Measure::Stat(Stat::Harvested),
        "Master Farmers have harvested at least {0} fruits and vegetables. The plants of Master Farmers remain watered and fertilized longer, meaning their gardens are more efficient.",
        "has harvested enough fruits and vegetables to complete the Master Farmer Skill Challenge!",
    ),
    ch(
        "Gardening",
        "Botanical Boss",
        100.0,
        Measure::Stat(Stat::PerfectHarvested),
        "Botanical Bosses must harvest at least {0} Perfect fruits and vegetables. After so many Perfect harvestables, the plants of Botanical Bosses almost never die from neglect.",
        "has harvested enough perfect fruits and vegetables to complete the Botanical Boss Skill Challenge!",
    ),
    ch(
        "Handiness",
        "Electrician",
        50.0,
        Measure::Stat(Stat::ElectricalRepairs),
        "Electricians have repaired at least {0} electrical objects. The experience gained means they will never be electrocuted by an electrical object again – what insurance!",
        "has repaired enough electrical objects to complete the Electrician Skill Challenge!",
    ),
    ch(
        "Handiness",
        "Plumber",
        50.0,
        Measure::Stat(Stat::PlumbingRepairs),
        "Plumbers have repaired at least {0} plumbing objects. They are so good at repairs that plumbing objects repaired by them never break again.",
        "has repaired enough plumbing objects to complete the Plumber Skill Challenge!",
    ),
    ch(
        "Handiness",
        "Tinkerer",
        10.0,
        Measure::Kinds(Kinds::Upgrades),
        "Tinkerers have made at least {0} different upgrades to the household's objects. A Tinkerer can unlock the potential in any household object using their own two hands!",
        "has upgraded enough objects to complete the Tinkerer Skill Challenge!",
    ),
    ch(
        "Logic",
        "Celestial Explorer",
        30.0,
        Measure::Stat(Stat::StarsFound),
        "Celestial Explorers have discovered {0} celestial bodies through their telescope. Their extensive knowledge of the heavens allows them to discuss the stars with their friends and neighbors.",
        "has discovered enough celestial bodies to complete the Celestial Explorer Skill Challenge!",
    ),
    ch(
        "Logic",
        "Chess Grand Master",
        5.0,
        Measure::ChessRank,
        "Chess Grand Masters have reached the coveted fifth level of the competitive chess circuit. There, they sit upon their throne, gazing downwards upon their victims. Those who engage Grand Masters in chess improve their abilities in logic and chess more quickly.",
        "has reached the fifth level of the competitive chess circuit and has completed the Chess Grand Master Skill Challenge!",
    ),
    ch(
        "Painting",
        "Brushmaster",
        10.0,
        Measure::Stat(Stat::Paintings),
        "Brushmasters have painted at least {0} paintings, and as a result, paint much faster than normal painters.",
        "has painted enough paintings to complete the Brushmaster Skill Challenge!",
    ),
    ch(
        "Painting",
        "Proficient Painter",
        10.0,
        Measure::Stat(Stat::BrilliantPaintings),
        "Proficient Painters have proven their worth by painting at least {0} brilliant paintings. They tend to paint far more brilliant paintings and masterpieces than less proficient Sims.",
        "has painted enough brilliant paintings to complete the Proficient Painter Skill Challenge!",
    ),
    ch(
        "Painting",
        "Master Painter",
        10.0,
        Measure::Stat(Stat::Masterpieces),
        "Master Painters have painted at least {0} masterpieces. Every painting they sell is worth oodles more than the work of normal artists.",
        "has painted enough masterpieces to complete the Master Painter Skill Challenge!",
    ),
    ch(
        "Writing",
        "Prolific Writer",
        100.0,
        Measure::BooksWritten,
        "Prolific Writers have written at least {0} books in their career. They are so well known that they tend to write far more hits and best-sellers than their counterparts.",
        "has written enough books to complete the Prolific Writer Skill Challenge!",
    ),
    Challenge {
        money: true,
        ..ch(
            "Writing",
            "Speed Writer",
            10000.0,
            Measure::Stat(Stat::Royalties),
            "Speed Writers are so prolific that they've earned {0} in royalties. Speed Writers write more quickly than normal writers.",
            "has earned enough royalties to complete the Speed Writer Skill Challenge!",
        )
    },
    ch(
        "Writing",
        "Specialist Writer",
        10.0,
        Measure::OneGenre,
        "Specialist Writers have written at least {0} novels in a single genre, and know it inside out.",
        "has written enough novels in a single genre to complete the Specialist Writer Skill Challenge!",
    ),
];

/// A skill journal's statistics: what's counted for a skill, by name.
pub fn statistics(skill: &str) -> &'static [(&'static str, Measure)] {
    match skill {
        "Athletic" => &[
            ("Hours of strength training", Measure::Stat(Stat::StrengthHours)),
            ("Hours of cardio", Measure::Stat(Stat::CardioHours)),
            ("Kilometers jogged", Measure::Stat(Stat::KmJogged)),
        ],
        "Charisma" => &[("Friends", Measure::Friends), ("Best friends", Measure::BestFriends), ("Successful jokes", Measure::Stat(Stat::Jokes))],
        "Cooking" => &[("Dishes prepared", Measure::Stat(Stat::Dishes)), ("Recipes known", Measure::RecipesKnown)],
        "Fishing" => &[("Fish caught", Measure::Stat(Stat::Fish)), ("Types of fish caught", Measure::Kinds(Kinds::FishTypes))],
        "Gardening" => &[
            ("Fruits and vegetables harvested", Measure::Stat(Stat::Harvested)),
            ("Perfect harvests", Measure::Stat(Stat::PerfectHarvested)),
            ("Kinds of plants planted", Measure::Kinds(Kinds::PlantTypes)),
        ],
        "Guitar" => &[("Hours played", Measure::Stat(Stat::GuitarHours))],
        "Handiness" => &[
            ("Electrical objects repaired", Measure::Stat(Stat::ElectricalRepairs)),
            ("Plumbing objects repaired", Measure::Stat(Stat::PlumbingRepairs)),
            ("Different upgrades made", Measure::Kinds(Kinds::Upgrades)),
        ],
        "Logic" => &[("Celestial bodies discovered", Measure::Stat(Stat::StarsFound)), ("Chess rank", Measure::ChessRank)],
        "Painting" => &[("Paintings", Measure::Stat(Stat::Paintings)), ("Brilliant paintings", Measure::Stat(Stat::BrilliantPaintings)), ("Masterpieces", Measure::Stat(Stat::Masterpieces))],
        "Writing" => &[("Books written", Measure::BooksWritten), ("Royalties earned", Measure::Stat(Stat::Royalties)), ("Most in one genre", Measure::OneGenre)],
        _ => &[],
    }
}

/// Friendship from which someone is a friend, and a best friend.
pub const FRIEND: f32 = 15.0;
pub const BEST_FRIEND: f32 = 75.0;

/// What's needed to measure a Sim's challenges beyond their journal.
pub struct Measures<'a> {
    pub journal: Option<&'a SkillJournal>,
    pub rels: &'a Relationships,
    pub cooking: u32,
    pub recipes: Option<&'a crate::meals::KnownRecipes>,
    pub chess: Option<&'a crate::chess::ChessRecord>,
    pub author: Option<&'a crate::writing::Author>,
    pub data: Option<&'a s3bake::GameDataBaked>,
}

impl Measures<'_> {
    pub fn value(&self, m: Measure) -> f64 {
        match m {
            Measure::Stat(s) => self.journal.map_or(0.0, |j| j.get(s)),
            Measure::Kinds(k) => self.journal.map_or(0, |j| j.kinds(k)) as f64,
            Measure::Friends => self.rels.0.values().filter(|r| r.friendship >= FRIEND).count() as f64,
            Measure::BestFriends => self.rels.0.values().filter(|r| r.friendship >= BEST_FRIEND).count() as f64,
            Measure::RecipesKnown => self.data.map_or(0, |d| d.recipes.iter().filter(|r| crate::meals::knows(r, self.cooking, self.recipes)).count()) as f64,
            Measure::ChessRank => self.chess.map_or(0, |c| c.rank) as f64,
            Measure::BooksWritten => self.author.map_or(0, |a| a.books.len()) as f64,
            Measure::OneGenre => self.author.map_or(0, |a| a.books.iter().map(|b| a.written(&b.genre)).max().unwrap_or(0)) as f64,
        }
    }
}

/// A measure's value as the journal shows it.
pub fn shown(m: Measure, v: f64) -> String {
    match m {
        Measure::Stat(Stat::Royalties) => format!("§{}", crate::lifetime::group(v as i64)),
        Measure::Stat(Stat::StrengthHours | Stat::CardioHours | Stat::GuitarHours | Stat::KmJogged) => format!("{v:.1}"),
        Measure::ChessRank => crate::chess::RANKS[(v as usize).min(crate::chess::RANKS.len() - 1)].to_string(),
        _ => format!("{}", v as i64),
    }
}

impl Challenge {
    /// Its description, with what's needed.
    pub fn description(&self) -> String {
        let n = if self.money { format!("§{}", crate::lifetime::group(self.need as i64)) } else { format!("{}", self.need as i64) };
        self.desc.replace("{0}", &n)
    }
}

/// Something found through a telescope.
pub fn celestial_find(rng: &mut impl rand::Rng) -> &'static str {
    const FINDS: [&str; 6] = ["a new star", "a comet", "a distant planet", "a nebula", "a binary star", "an asteroid"];
    FINDS[rng.random_range(0..FINDS.len())]
}

/// The tallies kept as things are done (by the household's Sims).
fn keep_tallies(mut commands: Commands, mut did: MessageReader<Did>, mut journals: Query<Option<&mut SkillJournal>, With<HouseholdMember>>) {
    let mut fresh: Vec<(Entity, SkillJournal)> = Vec::new();
    for d in did.read() {
        match journals.get_mut(d.sim) {
            Ok(Some(mut j)) => j.add(&d.what),
            Ok(None) => match fresh.iter_mut().find(|(e, _)| *e == d.sim) {
                Some((_, j)) => j.add(&d.what),
                None => {
                    let mut j = SkillJournal::default();
                    j.add(&d.what);
                    fresh.push((d.sim, j));
                }
            },
            Err(_) => {}
        }
    }
    for (e, j) in fresh {
        commands.entity(e).try_insert(j);
    }
}

/// Challenges met are earned, with the game's notice.
#[allow(clippy::type_complexity)]
fn earn_challenges(
    mut sims: Query<
        (
            &Sim,
            &mut SkillJournal,
            &Relationships,
            &Skills,
            Option<&crate::meals::KnownRecipes>,
            Option<&crate::chess::ChessRecord>,
            Option<&crate::writing::Author>,
        ),
        With<HouseholdMember>,
    >,
    ui: Option<Res<crate::icons::GameUi>>,
    mut notes: ResMut<Notifications>,
    time: Res<Time>,
    mut last: Local<f32>,
) {
    // (A few times a second is plenty.)
    if time.elapsed_secs() - *last < 0.5 {
        return;
    }
    *last = time.elapsed_secs();
    for (sim, mut journal, rels, skills, recipes, chess, author) in &mut sims {
        let due: Vec<&Challenge> = {
            let m = Measures { journal: Some(&journal), rels, cooking: skills.level("Cooking"), recipes, chess, author, data: ui.as_ref().map(|u| &*u.data) };
            CHALLENGES.iter().filter(|c| !journal.has(c.name) && m.value(c.measure) >= c.need).collect()
        };
        for c in due {
            journal.earned.push(c.name.to_string());
            notes.push(format!("Congratulations! {} {} Check {}'s Skill Journal for the reward.", sim.first, c.done, sim.first));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tallies_and_kinds() {
        let mut j = SkillJournal::default();
        j.add(&Deed::Count(Stat::Fish, 1.0));
        j.add(&Deed::Count(Stat::Fish, 2.0));
        j.add(&Deed::Kind(Kinds::FishTypes, "Minnow".into()));
        j.add(&Deed::Kind(Kinds::FishTypes, "Minnow".into()));
        j.add(&Deed::Kind(Kinds::FishTypes, "Goldfish".into()));
        assert_eq!(j.get(Stat::Fish), 3.0);
        assert_eq!(j.kinds(Kinds::FishTypes), 2);
        let back: SkillJournal = serde_json::from_str(&serde_json::to_string(&j).unwrap()).unwrap();
        assert_eq!(back, j);
    }

    #[test]
    fn every_challenge_has_its_number_and_a_statistic() {
        for c in CHALLENGES {
            assert!(c.desc.contains("{0}") || matches!(c.measure, Measure::ChessRank), "{}", c.name);
            assert!(!c.description().contains("{0}"));
            assert!(!statistics(c.skill).is_empty(), "{}", c.skill);
        }
    }
}
