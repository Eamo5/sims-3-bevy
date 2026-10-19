//! The game's wish functions (its `DreamsAndPromisesDelegateFunctions`), by their names: each
//! wish type's check (whether a wish can be had, is true already, or is out of the question),
//! feedback (whether a happening fulfils it, and what it was about) and start (what's so of a
//! Sim already). Those not done here leave their wishes out, as the game leaves out what isn't
//! installed. The thresholds are the class tuning's (`kMinimumWorkoutTime` a quarter hour, Go
//! Fishing and Watch TV fifteen minutes, jogging fifteen).

use std::collections::HashMap;

use bevy::prelude::*;
use s3bake::dnp::{DnpData, DnpNode, DnpPrimitive};

use crate::careers::Job;
use crate::dreams::{Active, Check, DreamEvent, Subject};
use crate::interact::Skills;
use crate::life::Trait;
use crate::sim::{Age, Sim};
use crate::social::{RelStatus, Relationship, Relationships};

/// The skills a child can learn (the game's `Skills` table's ages); the rest are a teen's on.
const CHILD_SKILLS: [&str; 4] = ["Fishing", "Logic", "Painting", "Writing"];
/// Minutes of a workout, a fishing trip, TV, a jog that count (the class tuning's).
const WORKOUT_HOURS: f32 = 0.25;
const FISHING_MINUTES: f32 = 15.0;
const TV_MINUTES: f32 = 15.0;
const JOG_MINUTES: f32 = 15.0;
const WORK_AT_HOME_MINUTES: f32 = 15.0;
/// The kinds of fish there are to catch.
const FISH_TYPES: usize = 20;
/// The liking past which "Let's Just Be Friends" is as good as done (`kJustBeFreindsRelationShipThreshold`).
const JUST_FRIENDS: f32 = 25.0;

/// Another Sim, as the functions need them.
#[derive(Clone)]
pub struct Person {
    pub entity: Entity,
    pub first: String,
    pub female: bool,
    pub age: Age,
    pub member: bool,
    /// Days left in their life stage.
    pub days_left: f32,
    pub pregnant: bool,
    pub mood: f32,
    pub traits: Vec<Trait>,
    /// Their career and level (1-based), where they have one.
    pub career: Option<(&'static str, usize)>,
    /// Their favourite food (a recipe's key).
    pub favorite_food: String,
}

/// The town's venues by kind (`Park`, `Library`, `Pool`, `Graveyard`, `ArtGallery`) and its rabbit
/// holes (`Bookstore`, `Theatre`, `DaySpa`...): those it has, and those of each lot.
#[derive(Default)]
pub struct Town {
    pub has: Vec<&'static str>,
    pub lots: HashMap<usize, Vec<&'static str>>,
    pub community: Vec<usize>,
}

impl Town {
    pub fn of(world: &crate::loading::WorldInfo) -> Self {
        let mut t = Town::default();
        for (i, l) in world.lots.iter().enumerate() {
            if l.is_residential() {
                continue;
            }
            t.community.push(i);
            let n = l.internal_name.to_ascii_lowercase();
            let mut kinds: Vec<&'static str> = Vec::new();
            for (k, kind) in [("park", "Park"), ("library", "Library"), ("pool", "Pool"), ("graveyard", "Graveyard"), ("cemetery", "Graveyard"), ("museum", "ArtGallery"), ("gallery", "ArtGallery"), ("spa", "DaySpa"), ("gym", "Gym")] {
                if n.contains(k) && !kinds.contains(&kind) {
                    kinds.push(kind);
                }
            }
            for k in crate::opportunities::lot_types(world, i) {
                if !kinds.contains(&k) {
                    kinds.push(k);
                }
            }
            for k in &kinds {
                if !t.has.contains(k) {
                    t.has.push(k);
                }
            }
            t.lots.insert(i, kinds);
        }
        t
    }
}

/// What's so of a Sim besides their skills, job and relationships.
#[derive(Default)]
pub struct Facts<'a> {
    pub motives: [f32; 6],
    /// The lot they're on, away from home (none: at home).
    pub lot: Option<usize>,
    pub at_work: bool,
    pub journal: Option<&'a crate::journal::SkillJournal>,
    pub author: Option<&'a crate::writing::Author>,
    pub books_read: u32,
    pub recipes: Option<&'a crate::meals::KnownRecipes>,
    pub classes: Option<&'a crate::classes::ClassesTaken>,
    pub chess: Option<crate::chess::ChessRecord>,
    pub pregnant: bool,
    pub grade: Option<f32>,
    pub homework: bool,
    pub party: bool,
    pub date: Option<u64>,
    pub plants: u32,
    /// They've a lifetime wish already.
    pub has_lifetime: bool,
}

/// Where a Sim stands, for the functions.
pub struct Ctx<'a> {
    pub sim: &'a Sim,
    pub skills: &'a Skills,
    pub job: Option<&'a Job>,
    pub rels: &'a Relationships,
    pub mood: f32,
    pub funds: i64,
    pub worth: i64,
    pub people: &'a HashMap<u64, Person>,
    pub ids: &'a HashMap<Entity, u64>,
    /// The object categories the household owns, each with the dearest's price.
    pub owned: &'a [(String, i64)],
    pub data: &'a DnpData,
    pub facts: &'a Facts<'a>,
    pub town: &'a Town,
    pub family: Option<&'a crate::family::Genealogy>,
    pub gdata: Option<&'a s3bake::GameDataBaked>,
}

impl Ctx<'_> {
    pub fn level(&self, skill: &str) -> u32 {
        self.skills.0.iter().find(|(k, _)| k.eq_ignore_ascii_case(skill)).map_or(0, |(_, v)| (*v as u32).min(10))
    }
    pub fn rel_with(&self, id: u64) -> Option<Relationship> {
        let e = self.people.get(&id)?.entity;
        self.rels.0.get(&e).copied()
    }
    pub fn career_key(&self) -> Option<&'static str> {
        self.job.map(|j| j.career().key)
    }
    fn my_id(&self) -> u64 {
        self.sim.id
    }
    /// Family close enough to rule out romance.
    fn related(&self, id: u64) -> bool {
        self.family.is_some_and(|f| f.kin(self.my_id(), id).is_some())
    }
    fn partner(&self) -> Option<u64> {
        self.rels.0.iter().find(|(_, r)| matches!(r.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married)).and_then(|(e, _)| self.ids.get(e).copied())
    }
    fn spouse(&self) -> Option<u64> {
        self.rels.0.iter().find(|(_, r)| r.status == RelStatus::Married).and_then(|(e, _)| self.ids.get(e).copied())
    }
    fn owns(&self, category: &str, at_least: i64) -> bool {
        self.owned.iter().any(|(c, p)| c == category && *p >= at_least)
    }
    fn knows_skill(&self, skill: &str) -> bool {
        crate::save::SKILLS.iter().any(|s| s.eq_ignore_ascii_case(skill))
    }
    fn skill_for_age(&self, skill: &str) -> bool {
        match self.sim.age {
            Age::Child => CHILD_SKILLS.iter().any(|s| s.eq_ignore_ascii_case(skill)),
            a => a == Age::Teen || a.is_grown(),
        }
    }
    fn tally(&self, key: &str) -> f64 {
        self.facts.journal.and_then(|j| j.tally.get(key)).copied().unwrap_or(0.0)
    }
    fn kinds(&self, key: &str) -> usize {
        self.facts.journal.and_then(|j| j.kinds.get(key)).map_or(0, |v| v.len())
    }
    fn here(&self) -> &[&'static str] {
        self.facts.lot.and_then(|l| self.town.lots.get(&l)).map_or(&[], |v| v.as_slice())
    }
    fn children(&self) -> usize {
        self.family.map_or(0, |f| f.children(self.my_id()).len())
    }
    fn household(&self, id: u64) -> bool {
        self.people.get(&id).is_some_and(|p| p.member)
    }
}

/// The game's relationship levels by their names, and where each stands.
pub fn ltr_at_least(r: &Relationship, name: &str) -> bool {
    match name {
        "Friend" => r.friendship >= crate::social::FRIEND_THRESHOLD,
        "GoodFriend" | "Good Friend" => r.friendship >= 60.0,
        "BestFriend" | "Best Friend" | "BFF" => r.friendship >= 80.0,
        "Partner" | "RomanticInterest" => matches!(r.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married),
        "Spouse" => r.status == RelStatus::Married,
        _ => false,
    }
}

/// The game's name for where two Sims stand (the trees' `Relationships` lists).
pub fn ltr_name(r: &Relationship) -> &'static str {
    match r.status {
        RelStatus::Married => "Spouse",
        RelStatus::Engaged => "Fiancee",
        RelStatus::Partner => "Partner",
        _ => match r.friendship {
            v if v <= -20.0 => {
                if r.enemy {
                    "Enemy"
                } else {
                    "Disliked"
                }
            }
            v if v < crate::social::FRIEND_THRESHOLD => "Acquaintance",
            v if v < 60.0 => "Friend",
            v if v < 80.0 => "Good Friend",
            _ => "Best Friend",
        },
    }
}

/// How far along a level is (the game's `LTRData` scores, in order).
pub fn ltr_score(name: &str) -> i32 {
    match name {
        "Enemy" => -3,
        "Disliked" => -2,
        "Friend" => 1,
        "Good Friend" => 2,
        "Best Friend" => 3,
        "Partner" => 4,
        "Fiancee" => 5,
        "Spouse" => 6,
        _ => 0,
    }
}

pub fn age_name(a: Age) -> &'static str {
    match a {
        Age::Baby => "Baby",
        Age::Toddler => "Toddler",
        Age::Child => "Child",
        Age::Teen => "Teen",
        Age::YoungAdult => "YoungAdult",
        Age::Adult => "Adult",
        Age::Elder => "Elder",
    }
}

/// Whether a Sim has a trait, by the game's name for it.
pub fn has_trait(traits: &[Trait], name: &str) -> bool {
    // (The trait trees' roots call a few by other names.)
    let name = match name {
        "Mean" => "MeanSpirited",
        "Computer Wiz" => "ComputerWhiz",
        "Can't Stand Art" => "CantStandArt",
        n => n,
    };
    let squashed = name.replace(' ', "");
    traits.iter().any(|t| t.game_id().eq_ignore_ascii_case(&squashed) || format!("{t:?}").eq_ignore_ascii_case(&squashed))
}

/// A node's subject from its data (`InputSubject`, by its type's subject kind).
pub fn input_subject(p: &DnpPrimitive, n: &DnpNode) -> Subject {
    if n.input_subject.is_empty() {
        return Subject::None;
    }
    let s = n.input_subject.clone();
    match p.subject.as_str() {
        "Skill" => Subject::Skill(s),
        "Career" => Subject::Career(s),
        "ObjectCategory" => Subject::Category(s),
        _ => Subject::Key(s),
    }
}

/// Whether a bought object (by its class) is of a category's classes (the game's `is`: its own
/// class, or one deriving from it, as their names go: `TV` takes in `TVCheap`).
pub fn of_category(class: &str, category: &s3bake::dnp::DnpCategory) -> bool {
    let last = class.rsplit('.').next().unwrap_or(class);
    category.classes.iter().any(|c| {
        let mut want = c.rsplit('.').next().unwrap_or(c);
        // (An interface by what it's for: `IShowerable`, showers.)
        if want.len() > 2 && want.starts_with('I') && want.as_bytes()[1].is_ascii_uppercase() {
            want = want[1..].trim_end_matches("able");
        }
        !want.is_empty() && last.starts_with(want)
    })
}

/// Ages in order, youngest first.
fn rank(a: Age) -> u8 {
    match a {
        Age::Baby => 0,
        Age::Toddler => 1,
        Age::Child => 2,
        Age::Teen => 3,
        Age::YoungAdult => 4,
        Age::Adult => 5,
        Age::Elder => 6,
    }
}

/// The age bands romance keeps to (children, teens, the grown).
fn band(a: Age) -> u8 {
    match a {
        Age::Baby | Age::Toddler | Age::Child => 0,
        Age::Teen => 1,
        _ => 2,
    }
}

/// The game's `RomanticCheckFunction`: possible with them, family and ages allowing.
fn romantic(subject: &Subject, cx: &Ctx) -> Check {
    match subject {
        Subject::Sim(id) => {
            if cx.related(*id) {
                return Check::ThrowAway;
            }
            match cx.people.get(id) {
                Some(o) if band(o.age) == band(cx.sim.age) && band(o.age) > 0 => Check::Active,
                Some(_) => Check::Hidden,
                None => Check::ThrowAway,
            }
        }
        _ => Check::Active,
    }
}

/// The game's `SocialCore`: a social it has (by its name), for the romantic ones romance's test.
fn social_core(name: &str, subject: &Subject, cx: &Ctx) -> Check {
    let Some((_, def)) = crate::social::all_socials().find(|(_, s)| crate::conversation::game_key(s.name).eq_ignore_ascii_case(name) || s.name.eq_ignore_ascii_case(name)) else {
        return Check::ThrowAway;
    };
    if let Subject::Sim(id) = subject
        && !cx.people.contains_key(id)
    {
        return Check::ThrowAway;
    }
    if def.cat == crate::social::SocialCat::Romantic { romantic(subject, cx) } else { Check::Active }
}

/// The game's `ReachLevelNOfSkillHelper`.
fn reach_level(subject: &Subject, level: u32, cx: &Ctx) -> Check {
    match subject {
        Subject::None => Check::Active,
        Subject::Skill(k) if cx.knows_skill(k) => {
            if cx.level(k) >= level {
                Check::AutoSatisfy
            } else if !cx.skill_for_age(k) {
                Check::Hidden
            } else {
                Check::Active
            }
        }
        _ => Check::ThrowAway,
    }
}

/// The game's age checks of a subject Sim: `exact` already that age (`AutoSatisfy`), younger
/// still to come.
fn subject_age_is(subject: &Subject, age: Age, cx: &Ctx) -> Check {
    match subject {
        Subject::Sim(id) => match cx.people.get(id) {
            Some(o) if o.age == age => Check::AutoSatisfy,
            Some(o) if rank(o.age) < rank(age) => Check::Active,
            _ => Check::ThrowAway,
        },
        _ => Check::ThrowAway,
    }
}

fn subject_becomes(subject: &Subject, age: Age, cx: &Ctx) -> Check {
    match subject {
        Subject::Sim(id) if cx.people.get(id).is_some_and(|o| rank(o.age) < rank(age)) => Check::Active,
        _ => Check::ThrowAway,
    }
}

/// The owner's own age milestones: there already, still to come, or past.
fn own_age(age: Age, cx: &Ctx) -> Check {
    if cx.sim.age == age {
        Check::AutoSatisfy
    } else if rank(cx.sim.age) < rank(age) {
        Check::Active
    } else {
        Check::ThrowAway
    }
}

/// The game's `BeFriendsWithSimHelper`: the grown don't befriend the town's children, nor those who
/// dislike children any.
fn may_befriend(subject: &Subject, cx: &Ctx) -> bool {
    match subject {
        Subject::Sim(id) => cx.people.get(id).is_none_or(|o| {
            let young = matches!(o.age, Age::Baby | Age::Toddler | Age::Child | Age::Teen);
            !(cx.sim.age.is_grown() && young && !o.member) && !(cx.sim.traits.contains(&Trait::DislikesChildren) && matches!(o.age, Age::Baby | Age::Toddler | Age::Child))
        }),
        _ => true,
    }
}

fn ltr_or_better(subject: &Subject, level: &str, cx: &Ctx) -> Check {
    match subject {
        Subject::Sim(id) if cx.rel_with(*id).is_some_and(|r| ltr_at_least(&r, level)) => Check::AutoSatisfy,
        _ => Check::Active,
    }
}

/// The game's major-dream career checks: there (`AutoSatisfy`), or a career the town has.
fn career_level(cx: &Ctx, career: &str, level: usize, branch: &str) -> Check {
    match cx.job {
        Some(j) if j.career().key.eq_ignore_ascii_case(career) && j.level + 1 >= level && (branch == "Base" || j.path().branch.eq_ignore_ascii_case(branch)) => Check::AutoSatisfy,
        _ if crate::careers::careers().iter().any(|c| c.key.eq_ignore_ascii_case(career)) => Check::Active,
        _ => Check::ThrowAway,
    }
}

fn two_skills(cx: &Ctx, a: &str, b: &str) -> Check {
    if cx.level(a) >= 10 && cx.level(b) >= 10 { Check::AutoSatisfy } else { Check::Active }
}

/// A meal wish's (the game's `BaseMealCheckFunction`): a recipe it has, not a snack, one a
/// vegetarian would make.
fn base_meal(subject: &Subject, cx: &Ctx, gdata: Option<&s3bake::GameDataBaked>) -> Check {
    match subject {
        Subject::Key(k) => {
            let Some(r) = gdata.and_then(|g| g.recipes.iter().find(|r| r.key.eq_ignore_ascii_case(k))) else { return Check::ThrowAway };
            if cx.sim.traits.contains(&Trait::Vegetarian) && !r.vegetarian {
                Check::ThrowAway
            } else {
                Check::Active
            }
        }
        _ => Check::Active,
    }
}

/// The game's check functions, those done here (anything else isn't supported). Some set the
/// subject (the boss, the spouse, someone disliked).
pub fn check(f: &str, p: &DnpPrimitive, n: &DnpNode, subject: &mut Subject, cx: &Ctx, gdata: Option<&s3bake::GameDataBaked>) -> Check {
    use Check::*;
    let age = cx.sim.age;
    let grown = age.is_grown();
    let teen_up = grown || age == Age::Teen;
    let child_or_below = matches!(age, Age::Baby | Age::Toddler | Age::Child);
    let number = n.input_number;
    let subject_sim = |s: &Subject| match s {
        Subject::Sim(id) => cx.people.get(id),
        _ => None,
    };
    let toddler_or_below = |s: &Subject| subject_sim(s).is_some_and(|o| matches!(o.age, Age::Baby | Age::Toddler));
    // ("Have <trait> Trait": the trait trees' roots, true for those with it.)
    if let Some(t) = p.name.strip_prefix("Have ").and_then(|r| r.strip_suffix(" Trait")) {
        return if has_trait(&cx.sim.traits, t) { AutoSatisfy } else { Hidden };
    }
    match f {
        "" | "DefaultCheckFunction" | "DefaultDeadSimCheckFunction" | "GrowHarvestableCheckFunction" | "SimAgesUpWellCheckFunction" | "CleanEntireHouseCheckFunction" | "BuySomeHarvestableCheckFunction" => Active,
        "IsBornCheckFunction" | "CASBornCheckFunction" | "IsCasBornCheckFunction" => AutoSatisfy,
        "NewOffspringcCheckFunction" => {
            if *subject == Subject::None { Active } else { ThrowAway }
        }
        // Skills.
        "LearnSkillCheckFunction" => match &*subject {
            Subject::None => Active,
            Subject::Skill(k) if cx.knows_skill(k) && cx.level(k) < 1 => {
                if cx.skill_for_age(k) { Active } else { Hidden }
            }
            _ => ThrowAway,
        },
        "ReachLevelNOfSkillCheckFunction" => reach_level(subject, number as u32, cx),
        "ImproveAtSkillCheckFunction" | "MasterSkillCheckFunction" => reach_level(subject, 10, cx),
        "TakeASkillClassCheckFunction" => match &*subject {
            Subject::None => Active,
            Subject::Skill(k) if cx.knows_skill(k) => {
                let taken = cx.facts.classes.is_some_and(|c| c.0.iter().any(|x| x.eq_ignore_ascii_case(k)));
                if !cx.skill_for_age(k) {
                    Hidden
                } else if !taken && cx.level(k) < 10 {
                    Active
                } else {
                    ThrowAway
                }
            }
            _ => ThrowAway,
        },
        "ReadBookAboutSkillCheckFunction" | "BuyBookAboutSkillCheckFunction" => match &*subject {
            Subject::None => Active,
            Subject::Skill(k) if cx.level(k) < 10 && gdata.is_some_and(|g| g.skill_books.iter().any(|b| b.skill.eq_ignore_ascii_case(k))) => {
                if cx.skill_for_age(k) { Active } else { Hidden }
            }
            _ => ThrowAway,
        },
        "PaintingAndWritingL10MajorDreamCheckFunction" => two_skills(cx, "Painting", "Writing"),
        "PaintingAndGuitarL10MajorDreamCheckFunction" => two_skills(cx, "Painting", "Guitar"),
        "CharismaAndGutarL10MajorDreamCheckFunction" => two_skills(cx, "Charisma", "Guitar"),
        "LogicAndHandinesL10MajorDreamCheckFunction" => two_skills(cx, "Logic", "Handiness"),
        "LogicAndAthleticL10MajorDreamCheckFunction" => two_skills(cx, "Logic", "Athletic"),
        "ReachLevel10InNSkillsMajorDreamCheckFunction" => {
            if cx.skills.0.iter().filter(|(_, v)| **v >= 10.0).count() as f32 >= number { AutoSatisfy } else { Active }
        }
        "LogicL10AndChessMasterRankMajorDreamCheckFunction" => {
            if cx.level("Logic") >= 10 && cx.facts.chess.is_some_and(|c| c.rank as usize >= crate::chess::WINS_FOR_RANK.len()) { AutoSatisfy } else { Active }
        }
        "PlayAnInstrumentAgeCheckFunction" | "PlayTheGuitarATheParkCheckFunction" | "DoAStrengthWorkoutCheckFunction" | "GoJoggingCheckFunction" => {
            if child_or_below { Hidden } else { Active }
        }
        "LearnANewGuitarCompositionCheckFunction" => {
            if child_or_below { Hidden } else { Active }
        }
        // Careers.
        "JoinACareerCheckFunction" => {
            if !teen_up {
                return Hidden;
            }
            match &*subject {
                Subject::Career(c) => match crate::careers::careers().iter().find(|t| t.key.eq_ignore_ascii_case(c)) {
                    None => ThrowAway,
                    Some(t) if age == Age::Teen && !t.part_time => Hidden,
                    Some(t) if cx.career_key() != Some(t.key) => Active,
                    _ => Hidden,
                },
                _ if cx.job.is_none() => Active,
                _ => Hidden,
            }
        }
        "ReachLevelNOfCareerCheckFunction" => match &*subject {
            Subject::None => {
                if cx.job.is_none_or(|j| ((j.level + 1) as f32) < number) { Active } else { ThrowAway }
            }
            Subject::Career(c) => match crate::careers::careers().iter().find(|t| t.key.eq_ignore_ascii_case(c)) {
                Some(t) if t.paths.iter().map(|p| p.levels.len()).max().unwrap_or(0) as f32 >= number => match cx.job {
                    None => Active,
                    Some(j) if j.career().key == t.key && ((j.level + 1) as f32) < number => Active,
                    _ => ThrowAway,
                },
                _ => ThrowAway,
            },
            _ => ThrowAway,
        },
        "GetPromotedCheckFunction" => match cx.job {
            Some(j) if teen_up && j.level + 1 < j.levels().len() => Active,
            _ => Hidden,
        },
        "HaveAJobCheckFunction" | "WorkAtHomeCheckFunction" | "HasNonSkillBasedCareer" => {
            if cx.job.is_some() { Active } else { Hidden }
        }
        "GoToWorkCheckFunction" => {
            if teen_up && cx.job.is_some() { Active } else { Hidden }
        }
        "HaveNoJobCheckFunction" => {
            if cx.job.is_some() { Active } else { AutoSatisfy }
        }
        "NotAtWorkCheckFunction" => {
            if cx.job.is_none() || cx.facts.at_work { ThrowAway } else { AutoSatisfy }
        }
        "RetireCheckFunction" => {
            if cx.job.is_some() && age == Age::Elder { Active } else { Hidden }
        }
        "ReachTheHighestLevelOfCareerCheckFunction" => match (&*subject, cx.job) {
            (Subject::None, _) | (_, None) => Active,
            (Subject::Career(c), Some(j)) if j.career().key.eq_ignore_ascii_case(c) && j.level + 1 >= j.levels().len() => AutoSatisfy,
            (Subject::Career(_), _) => Active,
            _ => Hidden,
        },
        "HaveBossCheckFunction" => match cx.job.and_then(|j| j.boss) {
            Some(b) if cx.people.contains_key(&b) => {
                *subject = Subject::Sim(b);
                AutoSatisfy
            }
            _ => Active,
        },
        "BecomeFriendsWithAllCoworkersCheckFunction" => match cx.job {
            Some(j) if !j.coworkers.is_empty() => {
                if j.coworkers.iter().all(|c| cx.rel_with(*c).is_some_and(|r| ltr_at_least(&r, "Friend"))) { AutoSatisfy } else { Active }
            }
            _ => ThrowAway,
        },
        "BecomeFriendsWithBossCheckFunction" => match cx.job.and_then(|j| j.boss) {
            Some(b) => {
                if cx.rel_with(b).is_some_and(|r| ltr_at_least(&r, "Friend")) { AutoSatisfy } else { Active }
            }
            None => ThrowAway,
        },
        "Level10OfCriminalEvilBranchMajorDreamCheckFunction" => career_level(cx, "Criminal", 10, "Evil"),
        "Level10OfCriminalThiefBranchMajorDreamCheckFunction" => career_level(cx, "Criminal", 10, "Thief"),
        "Level9OfScienceMajorDreamCheckFunction" => career_level(cx, "Science", 9, "Base"),
        "Level10OfLawEnforcementSpecialAgentBranchMajorDreamCheckFunction" => career_level(cx, "LawEnforcement", 10, "SpecialAgent"),
        "Level10OfLawEnforcementForensicBranchMajorDreamCheckFunction" => career_level(cx, "LawEnforcement", 10, "ForensicAnalyst"),
        "Level10OfMedicalMajorDreamCheckFunction" => career_level(cx, "Medical", 10, "Base"),
        "Level10OfMilitaryMajorDreamCheckFunction" => career_level(cx, "Military", 10, "Base"),
        "Level9OfProSportsMajorDreamCheckFunction" => career_level(cx, "ProfessionalSports", 9, "Base"),
        "Level8OfBusinessMajorDreamCheckFunction" => career_level(cx, "Business", 8, "Base"),
        "Level10OfCulinaryMajorDreamCheckFunction" => career_level(cx, "Culinary", 10, "Base"),
        "Level10OfPoliticalMajorDreamCheckFunction" => career_level(cx, "Political", 10, "Base"),
        "Level10OfJournalismMajorDreamCheckFunction" => career_level(cx, "Journalism", 10, "Base"),
        "Level10OfMusicRockBranchMajorDreamCheckFunction" => career_level(cx, "Music", 10, "ElectricRock"),
        "Level10OfMusicSymphonicBranchMajorDreamCheckFunction" => career_level(cx, "Music", 10, "Symphonic"),
        // Buying.
        "BuyFirstCategoryCheckFunction" => match &*subject {
            Subject::Category(c) if !cx.owns(c, i64::MIN) => Active,
            _ => ThrowAway,
        },
        "BuyACategoryCheckFunction" => Active,
        "BuyACategorywWorthAtLeastNsimoleonsAndDontHaveOneMoreExpensiveAlreadyCheckFunction" => match &*subject {
            Subject::Category(c) if !cx.owns(c, number as i64) => Active,
            _ => ThrowAway,
        },
        "HaveACategoryCheckFunction" => match &*subject {
            Subject::Category(c) if !cx.owns(c, i64::MIN) => ThrowAway,
            _ => AutoSatisfy,
        },
        "HaveNoCategoryCheckFunction" => match &*subject {
            Subject::Category(c) if !cx.owns(c, i64::MIN) => AutoSatisfy,
            _ => ThrowAway,
        },
        // Socials.
        "SocialCheckFunction" | "SocialCheckFunctionIsUniversityWorld" => social_core(&p.name, subject, cx),
        "BeginSocializingWithSimCheckFunction" | "GeneralGameCheckFunction" | "PlayCatchCheckFunction" | "ScareSimCheckFunction" => {
            if toddler_or_below(subject) { Hidden } else { Active }
        }
        "FightCheckFunction" => {
            if subject_sim(subject).is_some_and(|o| o.pregnant) || cx.facts.pregnant {
                Hidden
            } else {
                social_core("Fight!", subject, cx)
            }
        }
        "DoAMeanSocialCheckFunction" => match &*subject {
            Subject::None => Active,
            Subject::Sim(_) => {
                if child_or_below || subject_sim(subject).is_some_and(|o| matches!(o.age, Age::Baby | Age::Toddler | Age::Child)) { Hidden } else { Active }
            }
            _ => ThrowAway,
        },
        "ToddlerSocialCheckFunction" => match &*subject {
            Subject::None => Active,
            Subject::Sim(id) => match cx.people.get(id).map(|o| o.age) {
                Some(Age::Baby) => Hidden,
                Some(Age::Toddler) => Active,
                _ => ThrowAway,
            },
            _ => ThrowAway,
        },
        "PlayTagCheckFunction" => {
            if toddler_or_below(subject) || (teen_up && !cx.sim.traits.contains(&Trait::Childish)) { Hidden } else { Active }
        }
        "MakeOutCheckFunction" => social_core("Make Out", subject, cx),
        "AskForACampaignDonationCheckFunction" => {
            if subject_sim(subject).is_some_and(|o| !o.age.is_grown()) { Hidden } else { social_core("Ask for Campaign Donation", subject, cx) }
        }
        "ProposeMarrigeCheckFunction" => {
            if cx.spouse().is_some() && *subject == Subject::None { Hidden } else { social_core(&p.name, subject, cx) }
        }
        "JustBeFriendsWithSimCheckFunction" => match &*subject {
            Subject::Sim(id) if cx.rel_with(*id).is_some_and(|r| r.is_romance()) => Active,
            _ => ThrowAway,
        },
        "SnubOrBeSnubbedCheckFunction" => {
            if age == Age::Teen && subject_sim(subject).is_none_or(|o| o.age == Age::Teen) { Active } else { Hidden }
        }
        "BeInFlirtyContextCheckFunction" => Active,
        "CuddleCheckFunction" | "HotTubAskToJoinCheckFunction" => romantic(subject, cx),
        "RomanticCheckFunction" => romantic(subject, cx),
        "KissSimForTheFirstTimeCheckFunction" => match &*subject {
            Subject::Sim(id) if cx.rel_with(*id).is_some_and(|r| r.kissed) || cx.related(*id) => ThrowAway,
            _ if child_or_below => Hidden,
            s => romantic(s, cx),
        },
        "HaveFirstKissEverCheckFunction" => {
            if cx.rels.0.values().any(|r| r.kissed) { ThrowAway } else if child_or_below { Hidden } else { Active }
        }
        "CanGoOnDateCheckFunction" => match &*subject {
            _ if child_or_below => ThrowAway,
            Subject::Sim(id) if *id == cx.my_id() => ThrowAway,
            Subject::Sim(id) => match cx.people.get(id) {
                Some(o) if band(o.age) != band(age) || o.age == Age::Child => Hidden,
                Some(_) => Active,
                None => ThrowAway,
            },
            _ => Active,
        },
        "WooHooWithSimCheckFunction" | "WooHooWithSimForTheFirstTimeCheckFunction" | "HaveAChildWithSimCheckFunction" => match &*subject {
            Subject::Sim(id) if cx.related(*id) => ThrowAway,
            s if grown => romantic(s, cx),
            _ => Hidden,
        },
        "FallInLoveWithSimCheckFunction" => match &*subject {
            Subject::Sim(id) if cx.related(*id) => ThrowAway,
            _ if child_or_below => Hidden,
            Subject::Sim(id) => match (cx.people.get(id), cx.rel_with(*id)) {
                (Some(o), _) if band(o.age) != band(age) => Hidden,
                (_, Some(r)) if r.is_romance() => AutoSatisfy,
                _ => romantic(subject, cx),
            },
            s => romantic(s, cx),
        },
        "BePartnerWithSimCheckFunction" => match &*subject {
            Subject::Sim(id) if cx.related(*id) => ThrowAway,
            _ if child_or_below => Hidden,
            Subject::Sim(id) => match (cx.people.get(id), cx.rel_with(*id)) {
                (Some(o), _) if matches!(o.age, Age::Baby | Age::Toddler | Age::Child) => Hidden,
                (_, Some(r)) if r.status == RelStatus::Partner => AutoSatisfy,
                (_, Some(r)) if matches!(r.status, RelStatus::Engaged | RelStatus::Married) => Hidden,
                _ => romantic(subject, cx),
            },
            s => romantic(s, cx),
        },
        "GetMarriedCheckFunction" => match &*subject {
            Subject::Sim(id) if cx.related(*id) => ThrowAway,
            s if (cx.spouse().is_none() || matches!(s, Subject::Sim(id) if Some(*id) != cx.partner())) && grown => {
                if matches!(s, Subject::Sim(_)) { romantic(s, cx) } else { Active }
            }
            _ => Hidden,
        },
        "BreakUpWithSimCheckFunction" => match &*subject {
            Subject::Sim(id) if *id == cx.my_id() => ThrowAway,
            Subject::None if cx.partner().is_some() => Active,
            Subject::Sim(id) if Some(*id) == cx.partner() => Active,
            _ => Hidden,
        },
        "InviteSimOverCheckFunction" | "BeInvitedOverBySimCheckFunction" => match subject_sim(subject) {
            Some(o) if o.member || band(o.age) != band(age) => Hidden,
            _ => Active,
        },
        "IsGenderPreferenceCheckFunction" => match subject_sim(subject) {
            Some(_) => AutoSatisfy,
            None => ThrowAway,
        },
        // Relationships.
        "BeFriendsWithSimCheckFunction" => {
            if may_befriend(subject, cx) { ltr_or_better(subject, "Friend", cx) } else { Hidden }
        }
        "BeGoodFriendsWithSimCheckFunction" => {
            if may_befriend(subject, cx) { ltr_or_better(subject, "Good Friend", cx) } else { Hidden }
        }
        "BeBestFriendsWithSimCheckFunction" | "BecomeBFFWithSimCheckFunction" => {
            if may_befriend(subject, cx) { ltr_or_better(subject, "Best Friend", cx) } else { Hidden }
        }
        "BeEnemyWithSimCheckFunction" => match subject_sim(subject) {
            Some(o) if matches!(o.age, Age::Baby | Age::Toddler | Age::Child) => Hidden,
            _ => match &*subject {
                Subject::Sim(id) if cx.rel_with(*id).is_some_and(|r| r.enemy) => AutoSatisfy,
                _ => Active,
            },
        },
        "StopBeingFriendsWithSimCheckFunction" => match &*subject {
            Subject::None => Active,
            Subject::Sim(id) if cx.rel_with(*id).is_some_and(|r| ltr_at_least(&r, "Friend")) => Active,
            _ => Hidden,
        },
        "HaveNFriendsOrBetterCheckFunction" | "HaveNFriendsOrBetterMajorDreamCheckFunction" => {
            if friends_count(cx) as f32 >= number { AutoSatisfy } else { Active }
        }
        "HaveNEnemiesCheckFunction" => {
            if cx.rels.0.values().filter(|r| r.enemy).count() as f32 >= number { AutoSatisfy } else { Active }
        }
        "HaveNRichFriendsCheckFunction" => Active,
        "BeInNoRomanticRelationshipCheckFunction" => {
            if cx.rels.0.values().any(|r| r.is_romance()) { Active } else { AutoSatisfy }
        }
        "BeInARomanticRelationshipCheckFunction" => {
            if cx.rels.0.values().any(|r| r.is_romance()) { AutoSatisfy } else { Active }
        }
        "HaveNegativeRelationshipWithSimCheckFunction" => match &*subject {
            Subject::Sim(id) => {
                if cx.rel_with(*id).is_some_and(|r| r.friendship < 0.0) { AutoSatisfy } else { Active }
            }
            _ => match cx.rels.0.iter().find(|(_, r)| r.friendship < 0.0).and_then(|(e, _)| cx.ids.get(e)) {
                Some(id) => {
                    *subject = Subject::Sim(*id);
                    AutoSatisfy
                }
                None => ThrowAway,
            },
        },
        "HaveDatedNDifferentSimsCheckFunction" => {
            if dated(cx) as f32 >= number { AutoSatisfy } else { Active }
        }
        "BeMarriedCheckFunction" => match cx.spouse() {
            Some(s) if matches!(subject, Subject::None) || *subject == Subject::Sim(s) => {
                *subject = Subject::Sim(s);
                AutoSatisfy
            }
            _ => Active,
        },
        "SimLivesInMyHouseholdRightNowCheckFunction" => match &*subject {
            Subject::Sim(id) if cx.household(*id) => AutoSatisfy,
            _ => ThrowAway,
        },
        "SimInHouseholdForNewDreamCheckFunction" => match &*subject {
            Subject::Sim(id) if !cx.household(*id) => ThrowAway,
            _ => Active,
        },
        "OnSameLotAsSimCheckFunction" | "NotOnSameLotAsSimCheckFunction" => match &*subject {
            Subject::Sim(id) => {
                let same = cx.people.get(id).is_some_and(|o| o.member) == cx.facts.lot.is_none();
                if same == (f == "OnSameLotAsSimCheckFunction") { AutoSatisfy } else { ThrowAway }
            }
            _ => ThrowAway,
        },
        "BeOnDateWithSimCheckFunction" => match (&*subject, cx.facts.date) {
            (Subject::Sim(id), Some(d)) if *id == d => AutoSatisfy,
            (Subject::None, Some(d)) => {
                *subject = Subject::Sim(d);
                AutoSatisfy
            }
            _ => ThrowAway,
        },
        "IAmAtAPartyCheckFunction" => {
            if cx.facts.party { AutoSatisfy } else { ThrowAway }
        }
        "MoveInWithSimCheckFunction" | "VisitTheHomeOfSimCheckFunction" => match &*subject {
            Subject::Sim(id) if cx.household(*id) => ThrowAway,
            _ => Active,
        },
        "SeeSimBecomeEvilCheckFunction" | "SeeSimBecomeAGeniusCheckFunction" => {
            let t = if f.contains("Evil") { Trait::Evil } else { Trait::Genius };
            match &*subject {
                Subject::Sim(id) if !cx.household(*id) => ThrowAway,
                Subject::Sim(id) => match cx.people.get(id) {
                    Some(o) if !o.traits.contains(&t) => {
                        if o.age.is_grown() { Hidden } else { Active }
                    }
                    _ => ThrowAway,
                },
                _ => ThrowAway,
            }
        }
        "NonPlayableGhostSubjectCheckFunction" => match &*subject {
            Subject::Sim(_) => Hidden,
            _ => Active,
        },
        // Money.
        "BeWorthMoreThanNSimoleonsCheckFunction" | "NNetWorthMajorDreamCheckFunction" => {
            if cx.worth as f32 > number { AutoSatisfy } else { Active }
        }
        "HaveMoreThanNSimoleonsCheckFunction" | "NSimoleonsInCashMajorDreamCheckFunction" => {
            if cx.funds as f32 >= number { AutoSatisfy } else { Active }
        }
        "HaveLessThanNSimoleonsCheckFunction" => {
            if (cx.funds as f32) < number { AutoSatisfy } else { Active }
        }
        // Ages.
        "IsYoungAdultOrOlderCheckFunction" => {
            if grown { AutoSatisfy } else { Active }
        }
        "IsTeenOrOlderCheckFunction" => {
            if teen_up { AutoSatisfy } else { Active }
        }
        "BecomeAChildCheckFunction" => {
            if age == Age::Child { AutoSatisfy } else { ThrowAway }
        }
        "BecomeATeenCheckFunction" => own_age(Age::Teen, cx),
        "BecomeAnAdultCheckFunction" => own_age(Age::Adult, cx),
        "BecomeAnElderCheckFunction" => own_age(Age::Elder, cx),
        "SimIsABabyCheckFunction" => subject_age_is(subject, Age::Baby, cx),
        "SimIsAToddlerCheckFunction" => subject_age_is(subject, Age::Toddler, cx),
        "SimIsAChildCheckFunction" => subject_age_is(subject, Age::Child, cx),
        "SimIsATeenCheckFunction" => subject_age_is(subject, Age::Teen, cx),
        "SimBecomesAToddlerCheckFunction" => subject_becomes(subject, Age::Toddler, cx),
        "SimBecomesAChildCheckFunction" => subject_becomes(subject, Age::Child, cx),
        "SimBecomesATeenCheckFunction" => subject_becomes(subject, Age::Teen, cx),
        "SimBecomesAYoungAdultCheckFunction" => subject_becomes(subject, Age::YoungAdult, cx),
        "SimIsAYoungAdultOrOlderCheckFunction" => match subject_sim(subject) {
            Some(o) if o.age.is_grown() => AutoSatisfy,
            Some(_) => Active,
            None => ThrowAway,
        },
        "SimIsNDaysFromAgingCheckFunction" => days_from_aging(subject, number, cx),
        "BabyToddlerSnuggleCheckFunction" | "PickUpSimCheckFunction" => match subject_sim(subject) {
            Some(o) if matches!(o.age, Age::Baby | Age::Toddler) => Active,
            Some(_) => ThrowAway,
            None if f == "PickUpSimCheckFunction" => Active,
            None => ThrowAway,
        },
        "TeenCurfewCheckFunction" => match age {
            Age::Teen => Active,
            a if a.is_grown() => ThrowAway,
            _ => Hidden,
        },
        // Needs and moods.
        "BeHungryOrMoreCheckFunction" => {
            if cx.facts.motives[crate::sim::HUNGER] < -25.0 { AutoSatisfy } else { Active }
        }
        "BeSleepyOrMoreCheckFunction" => {
            if cx.facts.motives[crate::sim::ENERGY] < -25.0 { AutoSatisfy } else { Active }
        }
        "HaveZeroFunCheckFunction" => {
            if cx.facts.motives[crate::sim::FUN] <= 0.0 { AutoSatisfy } else { Active }
        }
        "SimIsInBadOrWorseMoodCheckFunction" => match subject_sim(subject) {
            Some(o) if o.mood < -30.0 => AutoSatisfy,
            Some(_) => Active,
            None => ThrowAway,
        },
        "BeOutsideCheckFunction" => Active,
        // Meals.
        "CookAMealCheckFunction" | "BaseMealCheckFunction" => base_meal(subject, cx, gdata),
        "MealIsNotAmbrosiaCheckFunction" => match &*subject {
            Subject::Key(k) if k == "Ambrosia" => ThrowAway,
            s => base_meal(s, cx, gdata),
        },
        "MakeAMealUsingFreshCaughtFishCheckFunction" => {
            if cx.sim.traits.contains(&Trait::Vegetarian) { ThrowAway } else { Active }
        }
        "CookFavoriteMealOfSimCheckFunction" => match &*subject {
            Subject::Sim(_) => Active,
            _ => ThrowAway,
        },
        "LearnANewRecipieCheckFunction" | "BuyANewRecipeCheckFunction" => match &*subject {
            Subject::Key(k) if cx.facts.recipes.is_some_and(|r| r.0.iter().any(|x| x.eq_ignore_ascii_case(k))) => ThrowAway,
            s => base_meal(s, cx, gdata),
        },
        "KnowEveryCookingRecipieMajorDreamCheckFunction" => {
            let known = cx.facts.recipes.map_or(0, |r| r.0.len());
            if known >= gdata.map_or(usize::MAX, |g| g.recipes.len()) { AutoSatisfy } else { Active }
        }
        // Fishing.
        "CatchNFishCheckFunction" => match &*subject {
            Subject::Key(k) => {
                if cx.tally(&format!("caught:{k}")) as f32 >= number { AutoSatisfy } else { Active }
            }
            _ => ThrowAway,
        },
        "CatchFirstTypeOfFishCheckFunction" => match &*subject {
            Subject::Key(k) if cx.tally(&format!("caught:{k}")) <= 0.0 => Active,
            Subject::None => Active,
            _ => ThrowAway,
        },
        "CatchNNewTypesOfFishCheckFunction" | "LearnBaitForFishCheckFunction" => Active,
        // Writing and reading.
        "CurrentlyHasANovelInProgressCheckFunction" => {
            if cx.facts.author.is_some_and(|a| a.draft.is_some()) { Active } else { Hidden }
        }
        "WriteNNovelsCheckFunction" => {
            if novels(cx) as f32 >= number { AutoSatisfy } else { Active }
        }
        "WriteNBestSellersCheckFunction" => {
            if best_sellers(cx) as f32 >= number { AutoSatisfy } else { Active }
        }
        "MakeAtLeastNSimoleansAWeekInRoyaltiesCheckFunction" | "NSimoleonsPerWeekInRoyaltiesMajorDreamCheckFunction" => {
            if royalties(cx) as f32 >= number { AutoSatisfy } else { Active }
        }
        "ReadNUniqueBooksCheckFunction" => {
            if cx.facts.books_read as f32 >= number { AutoSatisfy } else { Active }
        }
        // The rest of the skills' doings.
        "DiscoveredStarCountCheckFunction" => {
            if cx.tally("stars_found") as f32 >= number { AutoSatisfy } else { Active }
        }
        "UpgradeNObjectsCheckFunction" => {
            if cx.kinds("upgrades") as f32 >= number { AutoSatisfy } else { Active }
        }
        "UpgradeObjectCheckFunction" | "MakeObjectSelfCleaningCheckFunction" | "RepairObjectCheckFunction" | "CleanObjectCheckFunction" => match &*subject {
            Subject::None => Active,
            _ => ThrowAway,
        },
        "HaveNPlantsCheckFunction" => {
            if cx.facts.plants as f32 >= number { AutoSatisfy } else { Active }
        }
        // Family.
        "HaveNTotalChildrenCheckFunction" => {
            if (cx.children() as f32) < number { Active } else { AutoSatisfy }
        }
        "HaveFirstChildCheckFunction" => {
            if cx.children() == 0 && grown { Active } else { ThrowAway }
        }
        "IHaveADadCheckFunction" | "IHaveAMomCheckFunction" => {
            let female = f.contains("Mom");
            let parent = cx.family.and_then(|fam| fam.parents(cx.my_id()).iter().copied().find(|p| cx.people.get(p).is_some_and(|o| o.female == female)));
            match parent {
                Some(id) if matches!(subject, Subject::None) || *subject == Subject::Sim(id) => {
                    *subject = Subject::Sim(id);
                    Active
                }
                _ => ThrowAway,
            }
        }
        // School.
        "HasSchoolCheckFunction" => {
            if matches!(age, Age::Child | Age::Teen) { Active } else { ThrowAway }
        }
        "GetAnAInSchoolCheckFunction" | "GetOnTheHonorRollCheckFunction" => {
            if !matches!(age, Age::Child | Age::Teen) {
                ThrowAway
            } else if cx.facts.grade.is_some_and(|g| g >= A_GRADE) {
                Hidden
            } else {
                Active
            }
        }
        "DoHomeworkCheckFunction" => {
            if !matches!(age, Age::Child | Age::Teen) {
                ThrowAway
            } else if cx.facts.homework {
                Active
            } else {
                Hidden
            }
        }
        "TutorSimForSchoolCheckFunction" => {
            if age == Age::Teen { Active } else { Hidden }
        }
        "SeeSimGetAnACheckFunction" | "SeeSimGetOnTheHonorRollCheckFunction" | "SeeSimGetAnFCheckFunction" => match subject_sim(subject) {
            _ if matches!(subject, Subject::Sim(id) if !cx.household(*id)) => ThrowAway,
            Some(o) if matches!(o.age, Age::Child | Age::Teen) => Active,
            Some(o) if matches!(o.age, Age::Baby | Age::Toddler) => Hidden,
            Some(_) => ThrowAway,
            None => Active,
        },
        // The town.
        "ParkExistsInWorldCheckFunction" => exists(cx, "Park"),
        "LibraryExistsInWorldCheckFunction" => exists(cx, "Library"),
        "GraveyardExistsInWorldCheckFunction" => exists(cx, "Graveyard"),
        "ArtGalleryExistsInWorldCheckFunction" => exists(cx, "ArtGallery"),
        "PublicPoolExistsInWorldCheckFunction" => exists(cx, "Pool"),
        "BookstoreExistsInWorldCCheckFunction" => exists(cx, "Bookstore"),
        "TheatreExistsInWorldCheckFunction" => exists(cx, "Theatre"),
        "SpaExistsInWorldCheckFunction" => exists(cx, "DaySpa"),
        "GoOutCheckFunction" => {
            if cx.facts.lot.is_some_and(|l| cx.town.community.contains(&l)) { AutoSatisfy } else { Active }
        }
        "EscapeFromLotCheckFunction" | "GoHomeCheckFunction" => Active,
        // Traits and the rest.
        "DonateToCharityCheckFunction" => {
            if cx.sim.traits.contains(&Trait::Good) { Active } else { Hidden }
        }
        "DonateToUndermineCharityCheckFunction" => {
            if cx.sim.traits.contains(&Trait::Evil) { Active } else { Hidden }
        }
        "StealCandyFromABabyCheckFunction" => {
            if teen_up { Active } else { Hidden }
        }
        "OrderPizzaCheckFunction" => {
            if child_or_below { Hidden } else { Active }
        }
        _ => NotSupported,
    }
}

/// A's grade (of 100: the game's A).
const A_GRADE: f32 = 80.0;

fn exists(cx: &Ctx, kind: &str) -> Check {
    if cx.town.has.contains(&kind) { Check::Active } else { Check::Hidden }
}

/// Friends (or more) who aren't family (the game's `HaveNFriendsOrBetter` count).
fn friends_count(cx: &Ctx) -> usize {
    cx.rels
        .0
        .iter()
        .filter(|(_, r)| ltr_at_least(r, "Friend") || r.is_romance())
        .filter(|(e, _)| cx.ids.get(e).is_none_or(|id| !cx.related(*id)))
        .count()
}

/// Those they've been romantic with, as many as they've dated.
fn dated(cx: &Ctx) -> usize {
    cx.rels.0.values().filter(|r| r.romantic || r.romance > 0.0 || matches!(r.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married | RelStatus::Ex)).count()
}

fn novels(cx: &Ctx) -> usize {
    cx.facts.author.map_or(0, |a| a.books.len())
}

fn best_sellers(cx: &Ctx) -> usize {
    cx.facts.author.map_or(0, |a| a.books.iter().filter(|b| b.quality == crate::writing::Quality::BestSeller).count())
}

fn royalties(cx: &Ctx) -> i64 {
    cx.facts.author.map_or(0, |a| a.weekly_royalties())
}

/// The game's `SimIsNDaysFromAging`: on the day (`AutoSatisfy`), still to come, or past.
fn days_from_aging(subject: &Subject, n: f32, cx: &Ctx) -> Check {
    let Subject::Sim(id) = subject else { return Check::ThrowAway };
    let Some(o) = cx.people.get(id) else { return Check::ThrowAway };
    if (n - o.days_left).abs() < 0.5 {
        Check::AutoSatisfy
    } else if n < o.days_left + 0.5 {
        Check::Active
    } else {
        Check::ThrowAway
    }
}

/// The game's feedback functions done here: whether a happening fulfils a node, with what it
/// was about (to be the node's subject, where it has none).
pub fn feedback(f: &str, p: &DnpPrimitive, n: &DnpNode, a: &mut Active, ev: &DreamEvent, cx: &Ctx, gdata: Option<&s3bake::GameDataBaked>) -> Option<Subject> {
    feedback_with(f, p, n, a, ev, cx, gdata, &mut false)
}

/// Whether a wish type's feedback is one done here (else its wishes could never come true, and
/// aren't offered).
pub fn fulfillable(f: &str, p: &DnpPrimitive, n: &DnpNode, cx: &Ctx, gdata: Option<&s3bake::GameDataBaked>) -> bool {
    let mut unknown = false;
    feedback_with(f, p, n, &mut Active::about(Subject::None), &DreamEvent::default(), cx, gdata, &mut unknown);
    !unknown
}

#[allow(clippy::too_many_arguments)]
fn feedback_with(f: &str, p: &DnpPrimitive, n: &DnpNode, a: &mut Active, ev: &DreamEvent, cx: &Ctx, gdata: Option<&s3bake::GameDataBaked>, unknown: &mut bool) -> Option<Subject> {
    let other = ev.other.and_then(|e| cx.ids.get(&e).copied());
    let number = n.input_number;
    let sim_kind = matches!(p.subject.as_str(), "Sim" | "SimOrPet" | "OutOfWorldSim" | "Pet");
    // (The Sim it's about: theirs, or any where the type takes a Sim.)
    let sim_m = |a: &Active, o: Option<u64>| -> Option<Subject> {
        let o = o?;
        match &a.subject {
            Subject::Sim(s) => (*s == o).then_some(Subject::Sim(o)),
            Subject::None if sim_kind => Some(Subject::Sim(o)),
            _ => None,
        }
    };
    let skill_m = |a: &Active| -> Option<Subject> {
        let k = ev.skill.clone()?;
        match &a.subject {
            Subject::Skill(s) => s.eq_ignore_ascii_case(&k).then_some(Subject::Skill(k)),
            _ => Some(Subject::Skill(k)),
        }
    };
    let career_m = |a: &Active| -> Option<Subject> {
        let k = cx.career_key()?;
        match &a.subject {
            Subject::Career(s) => s.eq_ignore_ascii_case(k).then(|| Subject::Career(k.to_string())),
            Subject::None => Some(Subject::Career(k.to_string())),
            _ => None,
        }
    };
    let key_m = |a: &Active| -> Option<Subject> {
        let k = ev.key.clone()?;
        match &a.subject {
            Subject::Key(s) => s.eq_ignore_ascii_case(&k).then_some(Subject::Key(k)),
            _ => Some(Subject::Key(k)),
        }
    };
    let category_m = |a: &Active| -> Option<Subject> {
        let class = ev.class.as_ref()?;
        match &a.subject {
            Subject::Category(c) => cx.data.categories.iter().find(|x| x.key == *c).is_some_and(|x| of_category(class, x)).then(|| Subject::Category(c.clone())),
            _ => cx.data.categories.iter().find(|x| of_category(class, x)).map(|x| Subject::Category(x.key.clone())),
        }
    };
    let ok = |b: bool| b.then_some(Subject::None);
    // (A social by name: given (or received, as the node has it), taken well (or, rejected,
    // turned down).)
    let social_named = |a: &Active, names: &[&str], either: bool| -> Option<Subject> {
        let s = ev.social.as_deref()?;
        if !names.iter().any(|x| x.eq_ignore_ascii_case(s)) {
            return None;
        }
        let role_ok = either || ev.recipient == (n.social_role == "kReciever");
        let outcome_ok = if n.social_role == "kRejected" { !ev.accepted } else { ev.accepted };
        if role_ok && outcome_ok { sim_m(a, other) } else { None }
    };
    let meal_ok = |a: &Active| -> Option<Subject> {
        let k = ev.key.clone()?;
        if cx.sim.traits.contains(&Trait::Vegetarian) && !gdata.and_then(|g| g.recipes.iter().find(|r| r.key == k)).is_some_and(|r| r.vegetarian) {
            return None;
        }
        match &a.subject {
            Subject::Key(s) => s.eq_ignore_ascii_case(&k).then_some(Subject::Key(k)),
            _ => Some(Subject::Key(k)),
        }
    };
    let counted = |a: &mut Active, by: f32| {
        a.count += by;
        a.count >= number
    };
    let checked = |name: &str, a: &Active| {
        let mut s = a.subject.clone();
        matches!(check(name, p, n, &mut s, cx, gdata), Check::AutoSatisfy)
    };
    let here_is = |kind: &str| cx.here().contains(&kind);
    match f {
        "" | "DefaultFeedbackFunction" => Some(other.map(Subject::Sim).filter(|_| sim_kind).unwrap_or(Subject::None)),
        // Sims.
        "SimMatchesSubjectFeedbackFunction" | "SimDescriptionMatchesSubjectFeedbackFunction" | "MeetSimFeedbackFunction" | "SocializeFeedBackFunction" | "HotTubSocialFeedbackFunction" => sim_m(a, other),
        "SeeAStrangerFeedbackFunction" => other.filter(|o| cx.rel_with(*o).is_none_or(|r| r.friendship.abs() < 1.0)).map(Subject::Sim),
        // Objects and buying.
        "ObjectMatchesCategoryFeedbackFunction" => category_m(a),
        "BuyACategoryWorthAtLeastNSimoleonsFeedbackFunction" => {
            if ev.amount >= number { category_m(a) } else { None }
        }
        "BuyAnObjectWorthAtLeastNSimoleonsFeedbackFunction" => ok(ev.amount >= number),
        "ObjectMatchesSubjectFeedbackFunction" => ok(a.subject == Subject::None),
        "UpgradeNObjectsFeedbackFunction" => ok(checked("UpgradeNObjectsCheckFunction", a)),
        // Skills.
        "LearnSkillFeedbackFunction" => {
            let k = ev.skill.clone()?;
            if ev.level < 1 || a.list.contains(&k) {
                return None;
            }
            let s = skill_m(a)?;
            a.list.push(k);
            Some(s)
        }
        "ImproveAtSkillFeedbackFunction" => {
            if ev.level >= 2 { skill_m(a) } else { None }
        }
        "SkillGreaterThanOrEqualToN" => {
            if ev.level as f32 >= number { skill_m(a) } else { None }
        }
        "SkillMasterSkill" => {
            if ev.level >= 10 { skill_m(a) } else { None }
        }
        "SkillMatchesSubjectFeedbackFunction" => skill_m(a),
        "ReadABookAboutSkillFeedbackFunction" | "BuyABookAboutSkillFeedbackFunction" => {
            if ev.skill.is_some() { skill_m(a) } else { None }
        }
        "PaintingAndWritingLevel10FeedbackFunction" => ok(cx.level("Painting") >= 10 && cx.level("Writing") >= 10),
        "PaintingAndGuitarLevel10FeedbackFunction" => ok(cx.level("Painting") >= 10 && cx.level("Guitar") >= 10),
        "CharismaAndGuitarLevel10FeedbackFunction" => ok(cx.level("Charisma") >= 10 && cx.level("Guitar") >= 10),
        "LogicAndHandinessLevel10FeedbackFunction" => ok(cx.level("Logic") >= 10 && cx.level("Handiness") >= 10),
        "LogicAndAthleticLevel10FeedbackFunction" => ok(cx.level("Logic") >= 10 && cx.level("Athletic") >= 10),
        "ReachLevel10inNSkillsFeedbackFunction" => ok(checked("ReachLevel10InNSkillsMajorDreamCheckFunction", a)),
        "LogicSkillLevel10AndChessMasterFeedbackFunction" => ok(checked("LogicL10AndChessMasterRankMajorDreamCheckFunction", a)),
        // Careers.
        "CareerLevelGreaterThanOrEqualToN" => {
            if cx.job.is_some_and(|j| (j.level + 1) as f32 >= number) { career_m(a) } else { None }
        }
        "JoinACareerFeedbackFunction" => career_m(a),
        "GetAPartTimeJobFeedbackFunction" => {
            if cx.job.is_some_and(|j| j.career().part_time) { career_m(a).or(Some(Subject::None)) } else { None }
        }
        "ChangeJobsFeedbackFunction" => career_m(a),
        "CareerMasterCareer" => {
            if cx.job.is_some_and(|j| j.level + 1 >= j.levels().len()) { career_m(a) } else { None }
        }
        "GoToWorkFeedbackFunction" => ok(true),
        "QuitJobFeedbackFunction" => ok(ev.key.as_deref() == Some("Quit")),
        "HaveNoJobFeedbackFunction" => ok(matches!(ev.key.as_deref(), Some("Quit" | "Fired" | "Retired"))),
        "RetireFeedbackFunction" => ok(ev.key.as_deref() == Some("Retired")),
        "GetFiredFeedbackFunction" => ok(ev.key.as_deref() == Some("Fired")),
        "WorkAtHomeFeedbackFunction" => ok(ev.amount * 60.0 >= WORK_AT_HOME_MINUTES),
        "EarnNSimoleonsFeedbackFunction" => ok(counted(a, ev.amount)),
        "HaveBossFeedbackFunction" => {
            let mut s = a.subject.clone();
            (check("HaveBossCheckFunction", p, n, &mut s, cx, gdata) == Check::AutoSatisfy).then_some(s)
        }
        "BecomeFriendsWithAllCoworkersFeedBackFunction" => ok(checked("BecomeFriendsWithAllCoworkersCheckFunction", a)),
        "BecomeFriendsWithBossFeedbackFunction" => ok(checked("BecomeFriendsWithBossCheckFunction", a)),
        "Level10CriminalEvilBranchFeedbackFunction" => ok(career_level(cx, "Criminal", 10, "Evil") == Check::AutoSatisfy),
        "Level10CriminalThiefBranchFeedbackFunction" => ok(career_level(cx, "Criminal", 10, "Thief") == Check::AutoSatisfy),
        "Level10LawEnforcementSpecialAgentBranchFeedbackFunction" => ok(career_level(cx, "LawEnforcement", 10, "SpecialAgent") == Check::AutoSatisfy),
        "Level10LawEnforcementForensicBranchFeedbackFunction" => ok(career_level(cx, "LawEnforcement", 10, "ForensicAnalyst") == Check::AutoSatisfy),
        "Level10MusicRockBranchFeedbackFunction" => ok(career_level(cx, "Music", 10, "ElectricRock") == Check::AutoSatisfy),
        "Level10MusicSymphonicBranchFeedbackFunction" => ok(career_level(cx, "Music", 10, "Symphonic") == Check::AutoSatisfy),
        "SeeSimBecomeProAthleteFeedbackFunction" | "SeeSimBecomeDoctorFeedbackFunction" | "SeeSimBecomeCriminalMasterMindFeedbackFunction" | "SeeSimBecomeScientistFeedbackFunction" => {
            let career = match f {
                "SeeSimBecomeProAthleteFeedbackFunction" => "ProfessionalSports",
                "SeeSimBecomeDoctorFeedbackFunction" => "Medical",
                "SeeSimBecomeCriminalMasterMindFeedbackFunction" => "Criminal",
                _ => "Science",
            };
            let o = other?;
            let (c, level) = cx.people.get(&o)?.career?;
            if c.eq_ignore_ascii_case(career) && level as f32 >= number { sim_m(a, Some(o)) } else { None }
        }
        // Waking, needs, moods, money, ages.
        "WakeupAfterAtLeast5HoursFeedbackFunction" => ok(ev.amount > 5.0),
        "BeHungryOrMoreFeedbackFunction" => ok(checked("BeHungryOrMoreCheckFunction", a)),
        "BeSleepyOrMoreFeedbackFunction" => ok(checked("BeSleepyOrMoreCheckFunction", a)),
        "HaveZeroFunFeedbackFunction" => ok(checked("HaveZeroFunCheckFunction", a)),
        "HaveFunBelowNValueFeedbackFunction" => ok(cx.facts.motives[crate::sim::FUN] < number),
        "SimIsInBadOrWorseMoodFeedbackFunction" => {
            let mut s = a.subject.clone();
            (check("SimIsInBadOrWorseMoodCheckFunction", p, n, &mut s, cx, gdata) == Check::AutoSatisfy).then_some(s)
        }
        "BeWorthMoreThanNSimoleonsFeedbackFunction" | "NNetWorthMajorDreamFeedbackFunction" => ok(cx.worth as f32 > number),
        "HaveMoreThanNSimoleonsFeedbackFunction" | "NSimoleonsInCashMajorDreamFeedbackFunction" => ok(cx.funds as f32 >= number),
        "HaveLessThanNSimoleonsFeedbackFunction" => ok(cx.funds as f32 <= number),
        "SpendNSimoleonsFeedbackFunction" => ok(ev.amount < 0.0 && counted(a, -ev.amount)),
        "IsYoungAdultOrOlderFeedbackFunction" => ok(cx.sim.age.is_grown()),
        "IsTeenOrOlderFeedbackFunction" => ok(cx.sim.age == Age::Teen || cx.sim.age.is_grown()),
        "BecomeAChildFeedbackFunction" => ok(cx.sim.age == Age::Child),
        "BecomeATeenFeedbackFunction" => ok(cx.sim.age == Age::Teen),
        "BecomeAnAdultFeedbackFunction" => ok(cx.sim.age == Age::Adult),
        "BecomeAnElderFeedbackFunction" => ok(cx.sim.age == Age::Elder),
        "SimBecomesAToddlerFeedbackFunction" | "SimBecomesAChildFeedbackFunction" | "SimBecomesATeenFeedbackFunction" | "SimBecomesAYoungAdultFeedbackFunction" => {
            let want = match f {
                "SimBecomesAToddlerFeedbackFunction" => Age::Toddler,
                "SimBecomesAChildFeedbackFunction" => Age::Child,
                "SimBecomesATeenFeedbackFunction" => Age::Teen,
                _ => Age::YoungAdult,
            };
            match &a.subject {
                Subject::Sim(id) => ok(cx.people.get(id).is_some_and(|o| o.age == want)),
                _ => ok(true),
            }
        }
        "SimIsNDaysFromAgingFeedbackFunction" => ok(matches!(days_from_aging(&a.subject, number, cx), Check::AutoSatisfy | Check::ThrowAway)),
        // Relationships.
        "BeFriendsWithSimFeedbackFunction" => {
            let o = other?;
            let improved = ev.key.as_deref().is_none_or(|was| cx.rel_with(o).is_some_and(|r| ltr_score(ltr_name(&r)) > ltr_score(was)));
            let r = cx.rel_with(o)?;
            let friends = if matches!(a.subject, Subject::Sim(_)) { ltr_at_least(&r, "Friend") || r.is_romance() } else { ltr_at_least(&r, "Friend") };
            if improved && friends { sim_m(a, Some(o)) } else { None }
        }
        "BeGoodFriendsWithSimFeedbackFunction" => {
            let o = other?;
            if cx.rel_with(o).is_some_and(|r| ltr_name(&r) == "Good Friend") { sim_m(a, Some(o)) } else { None }
        }
        "BeBestFriendsWithSimFeedbackFunction" => {
            let o = other?;
            if cx.rel_with(o).is_some_and(|r| ltr_at_least(&r, "Best Friend")) { sim_m(a, Some(o)) } else { None }
        }
        "BePartnerWithSimFeedbackFunction" => {
            let o = other?;
            if cx.rel_with(o).is_some_and(|r| r.status == RelStatus::Partner) { sim_m(a, Some(o)) } else { None }
        }
        "BeEnemyWithSimFeedbackFunction" => {
            let o = other?;
            if cx.rel_with(o).is_some_and(|r| r.enemy) { sim_m(a, Some(o)) } else { None }
        }
        "StopBeingFriendsWithSimFeedbackFunction" => {
            let o = other?;
            let was = ev.key.as_deref()?;
            let r = cx.rel_with(o)?;
            if ltr_score(was) >= 1 && !ltr_at_least(&r, "Friend") && !r.is_romance() { sim_m(a, Some(o)) } else { None }
        }
        "FallInLoveWithSimFeedbackFunction" => {
            let o = other?;
            if cx.rel_with(o).is_some_and(|r| r.is_romance()) { sim_m(a, Some(o)) } else { None }
        }
        "HaveNegativeRelationshipWithSimFeedbackFunction" => {
            let o = other?;
            if cx.rel_with(o).is_some_and(|r| r.friendship < 0.0) { sim_m(a, Some(o)) } else { None }
        }
        "HaveNFrindsOrBetterFeedbackFunction" => ok(friends_count(cx) as f32 >= number),
        "HaveNEnemiesFeedbackFunction" => ok(checked("HaveNEnemiesCheckFunction", a)),
        "BeInNoRomanticRelationshipFeedbackFunction" => ok(checked("BeInNoRomanticRelationshipCheckFunction", a)),
        "BeInARomanticRelationshipFeedbackFunction" => ok(checked("BeInARomanticRelationshipCheckFunction", a)),
        "HaveDatedNDifferentSimsFeedbackFunction" => ok(dated(cx) as f32 >= number),
        "GetMarriedFeedbackFunction" => sim_m(a, cx.spouse()),
        "BreakUpWithSimFeedbackFunction" => {
            let s = ev.social.as_deref()?;
            if !ev.recipient && (s == "Break Up" || s == "Divorce") {
                sim_m(a, other)
            } else {
                None
            }
        }
        // Socials.
        "SocialFeedBackFunction" => social_named(a, &[p.name.as_str()], false),
        "SocialSucceedOnGiveOrRecieveFeedbackFunction" => social_named(a, &[p.name.as_str()], true),
        "SocialSucceedOnAcceptOrRejectFeedbackFunction" => {
            let s = ev.social.as_deref()?;
            if s.eq_ignore_ascii_case(&p.name) && !ev.recipient { sim_m(a, other) } else { None }
        }
        "KissSimFeedbackFunction" => social_named(a, &["Kiss", "Make Out", "Goodbye Kiss", "Kiss on Cheek", "First Kiss"], true),
        "HugFeedbackFunction" => social_named(a, &["Family Hug", "Friendly Hug"], true),
        "InsultFeedbackFunction" => social_named(a, &["Insult", "Insult Home"], false),
        "FlirtFeedbackFunction" => social_named(a, &["Flirt", "Flirty Joke", "Greet Flirty"], false),
        "AskToStayOverFeedbackFunction" => social_named(a, &["Ask NPC To Stay Over", "Ask To Stay Over", "Ask NPC To Stay Over Romantically", "Ask To Stay Over Romantically"], false),
        "MockFeedbackFunction" => social_named(a, &["Mock"], false),
        "MoochSmallFeedbackFunction" => social_named(a, &["Mooch Money (Small)", "Mooch Money"], false),
        "WinAFightFeedBackFunction" | "LoseAFightFeedBackFunction" => {
            if ev.social.as_deref() != Some("Fight!") {
                return None;
            }
            if ev.won == (f == "WinAFightFeedBackFunction") { sim_m(a, other) } else { None }
        }
        "WinNFightsFeedBackFunction" => ok(ev.social.as_deref() == Some("Fight!") && ev.won && counted(a, 1.0)),
        "JustBeFriendsWithSimFeedbackFunction" => {
            let o = other?;
            match cx.rel_with(o) {
                Some(r) if r.friendship >= JUST_FRIENDS && !r.is_romance() => sim_m(a, Some(o)),
                _ => social_named(a, &["Lets Just Be Friends"], false),
            }
        }
        "BeInFlirtyContextFeedbackFunction" => {
            let o = other?;
            if cx.rel_with(o).is_some_and(|r| r.stc.name() == "Seductive") { sim_m(a, Some(o)) } else { None }
        }
        // Meals.
        "CookAMealFeedbackFunction" | "EatAMealFeedbackFunction" => meal_ok(a),
        "MealIsPerfectOrBetterFeedbackFunction" => {
            if ev.quality >= 11 { ev.key.clone().map(Subject::Key) } else { None }
        }
        "MealIsOutstandingOrBetterFeedbackFunction" => {
            if ev.quality >= 10 { ev.key.clone().map(Subject::Key) } else { None }
        }
        "MealIsGreatOrBetterFeedbackFunction" => {
            if ev.quality >= 8 { ev.key.clone().map(Subject::Key) } else { None }
        }
        "ServeAGreatMealFeedbackFunction" => {
            if ev.group && ev.quality >= 8 { ev.key.clone().map(Subject::Key) } else { None }
        }
        "ServeAPerfectMealFeedbackFunction" => {
            if ev.group && ev.quality >= 11 { ev.key.clone().map(Subject::Key) } else { None }
        }
        "CookNMealsPerfectlyFeedbackFunction" => {
            let k = ev.key.clone()?;
            if ev.quality >= 11 && !a.list.contains(&k) {
                a.list.push(k);
            }
            ok(a.list.len() as f32 >= number)
        }
        "CookFavoriteMealOfSimFeedbackFunction" => {
            let Subject::Sim(id) = &a.subject else { return None };
            let k = ev.key.as_deref()?;
            cx.people.get(id).filter(|o| o.favorite_food.eq_ignore_ascii_case(k)).map(|_| Subject::Sim(*id))
        }
        "LearnANewRecipieFeedbackFunction" => key_m(a),
        "LearnAllRecipiesFeedbackFunction" => ok(checked("KnowEveryCookingRecipieMajorDreamCheckFunction", a)),
        "EatMealAtThePublicPoolFeedbackFunction" => {
            if here_is("Pool") { meal_ok(a) } else { None }
        }
        "EatMealAtTheParkFeedbackFunction" => {
            if here_is("Park") { meal_ok(a) } else { None }
        }
        "EatAtBistroFeedbackFunction" => ok(ev.key.as_deref() == Some("Bistro")),
        // Fishing.
        "CatchAFishFeedbackFunction" => key_m(a),
        "CatchAFishAboveNWeightFeedbackFunction" => {
            if ev.amount >= number { key_m(a) } else { None }
        }
        "CatchARecordSizedFishFeedbackFunction" => {
            let k = ev.key.as_deref()?;
            if ev.amount as f64 >= cx.tally(&format!("heaviest:{k}")) { key_m(a) } else { None }
        }
        "CatchNFishFeedbackFunction" => {
            let mut s = a.subject.clone();
            (check("CatchNFishCheckFunction", p, n, &mut s, cx, gdata) == Check::AutoSatisfy).then_some(s)
        }
        "CatchEveryTypeOfFishFeedbackFunction" => ok(cx.kinds("fish_types") >= FISH_TYPES),
        "GoFishingFeedbackFunction" => ok(ev.amount * 60.0 >= FISHING_MINUTES),
        // Gardening.
        "PlantMatchesSubjectFeedbackFunction" => key_m(a),
        "GrowAPerfectPlantableFeedbackFunction" => {
            if ev.quality >= 11 { key_m(a) } else { None }
        }
        "GrowAnOutstandingPlantableFeedbackFunction" => {
            if ev.quality >= 10 { key_m(a) } else { None }
        }
        "GrowAGreatPlantableFeedbackFunction" => {
            if ev.quality >= 8 { key_m(a) } else { None }
        }
        "PlantIsMineFeedbackFunction" => ok(true),
        "HaveNPlantsFeedbackFunction" => ok(cx.facts.plants as f32 >= number),
        // Painting, writing, reading, the stars.
        "ValueFeedbackFunction" => ok(ev.amount >= number),
        "PaintSomethingBrilliantFeedbackFunction" => ok(ev.quality >= 1),
        "PaintAMasterpieceFeedbackFunction" => ok(ev.quality == 2),
        "PaintNMasterpiecesFeedbackFunction" => ok(ev.quality == 2 && counted(a, 1.0) || ev.quality != 2 && a.count >= number),
        "WriteNNovelsFeedbackFunction" => ok(novels(cx) as f32 >= number),
        "WriteNBestSellersFeedbackFunction" => ok(best_sellers(cx) as f32 >= number),
        "WriteABestSellerFeedbackFunction" => ok(ev.quality == 4),
        "WriteARomanceNovelFeedbackFunction" => ok(ev.key.as_deref() == Some("Romance")),
        "WriteASciFiNovelFeedbackFunction" => ok(ev.key.as_deref() == Some("SciFi")),
        "WriteAnAutobiographyFeedbackFunction" => ok(ev.key.as_deref() == Some("Autobiography")),
        "WriteAMasterpieceFeedbackFunction" => ok(ev.key.as_deref() == Some("Masterpiece")),
        "MakeAtLeastNSimoleansAWeekInRoyaltiesFeedbackFunction" => ok(royalties(cx) as f32 >= number),
        "WriteANovelWorthAtLeastNSimoleansAWeekInRoyaltiesFeedbackFunction" => ok(ev.amount >= number),
        "ReadABookFeedbackFunction" => ok(a.subject == Subject::None),
        "ReadNUniqueBooksFeedbackFunction" => ok(cx.facts.books_read as f32 >= number),
        "DiscoveredStarCountFeedbackFunction" => ok(cx.tally("stars_found") as f32 >= number),
        // Doings.
        "WorkOutFeedbackFunction" => {
            a.count += ev.amount;
            ok(a.count >= WORKOUT_HOURS)
        }
        "WorkoutForNHoursFeedbackFunction" => ok(counted(a, ev.amount)),
        "WorkoutForNHoursStraightFeedbackFunction" => ok(ev.amount >= number),
        "JogFeedbackFunction" => {
            a.count += ev.amount;
            ok(a.count * 60.0 >= JOG_MINUTES)
        }
        "WatchTvFeedbackFunction" => ok(ev.amount * 60.0 > TV_MINUTES),
        "DoSomethingForNHoursFeedbackFunction" => ok(counted(a, ev.amount)),
        "BeAtTheParkFeedbackFunction" => ok(here_is("Park")),
        "BeAtAPartyFeedbackFunction" => ok(cx.facts.party),
        "CountFeedbackFunction" => ok(counted(a, 1.0)),
        "WinNRankedChessMatchesInARowFeedbackFunction" => {
            if ev.won {
                a.count += 1.0;
            } else {
                a.count = 0.0;
            }
            ok(a.count >= number)
        }
        "DoHomeworkFeedbackFunction" => ok(true),
        "GetAnAFeedbackFunction" => ok(cx.facts.grade.is_some_and(|g| g >= A_GRADE)),
        "OrderPizzaFeedbackFunction" => ok(ev.key.as_deref() == Some("PizzaDelivery")),
        "DonateNSimoleonsToCharityFeedbackFunction" | "DonateNSimoleonsToUndermineCharityFeedbackFunction" => ok(counted(a, ev.amount)),
        // Parties.
        "ThrowAPartyFeedbackFunction" => ok(ev.key.as_deref() == Some("House")),
        "ThrowAPartyAtHomeFeedbackFunction" | "ThrowAHousePartyFeedbackFunction" => ok(ev.key.as_deref() == Some("House")),
        "ThrowABirthdayPartyFeedbackFunction" => ok(ev.key.as_deref() == Some("Birthday")),
        "ThrowAWeddingPartyFeedbackFunction" => ok(ev.key.as_deref() == Some("Wedding")),
        "ThrowNPartiesFeedbackFunction" => ok(counted(a, 1.0)),
        "ThrowAGreatPartyFeedbackFunction" => ok(ev.quality >= 1),
        "ThrowNGreatPartiesFeedbackFunction" => ok(ev.quality >= 1 && counted(a, 1.0)),
        // Dates.
        "GoOnFirstDateFeedbackFunction" => sim_m(a, other),
        // Venues and rabbit holes.
        "VisitAParkFeedbackFunction" => ok(here_is("Park")),
        "GoToTheLibraryFeedbackFunction" => ok(here_is("Library")),
        "VisitTheGraveyardFeedbackFunction" => ok(here_is("Graveyard")),
        "VisitTheArtGalleryFeedbackFunction" => ok(here_is("ArtGallery")),
        "VisitThePublicPoolFeedbackFunction" => ok(here_is("Pool")),
        "GoToTheBookstoreFeedbackFunction" => ok(ev.key.as_deref() == Some("Bookstore")),
        "GoToTheTheatreFeedbackFunction" => ok(ev.key.as_deref() == Some("Theatre")),
        "GoToTheSpaFeedbackFunction" => ok(ev.key.as_deref() == Some("DaySpa")),
        "SeeAMovieFeedbackFunction" => ok(ev.key.as_deref() == Some("Movie")),
        "SeeASymphonyFeedbackFunction" => ok(ev.key.as_deref() == Some("Symphony")),
        "SeeAConcertFeedbackFunction" => ok(ev.key.as_deref() == Some("Concert")),
        "SeeAGameFeedbackFunction" => ok(ev.key.as_deref() == Some("StadiumGame")),
        "GoHomeFeedbackFunction" => ok(cx.facts.lot.is_none()),
        // Family.
        "HaveNTotalChildrenFeedbackFunction" => ok(cx.children() as f32 >= number),
        _ => {
            *unknown = true;
            None
        }
    }
}

/// How far along a wish is (the game's count text functions: `GeneralProgressText`'s
/// "{0.Number}/{1.Number}", `MoneyProgressText`'s simoleons); none for most.
pub fn count_text(f: &str, n: &DnpNode, a: &Active, cx: &Ctx) -> String {
    let want = n.input_number;
    let of = |cur: f64| format!("{}/{}", cur.floor() as i64, want as i64);
    let money = |cur: i64| format!("§{}/§{}", crate::lifetime::group(cur), crate::lifetime::group(want as i64));
    match f {
        "ReachLevelNOfSkillCountFunction" => match &a.subject {
            Subject::Skill(k) => of(cx.level(k) as f64),
            _ => String::new(),
        },
        "BeWorthMoreThanNSimoleonsCountFunction" => money(cx.worth),
        "NSimoleonsInCashCountFunction" => money(cx.funds),
        "DefaultCountFunction" => of(a.count as f64),
        "DefaultMoneyCountFunction" => money(a.count as i64),
        "HaveNFriendsCountFunction" => of(friends_count(cx) as f64),
        "WriteNNovelsTotalCountFunction" => of(novels(cx) as f64),
        "WriteNBestSellersTotalCountFunction" => of(best_sellers(cx) as f64),
        "ReadNUniqueBooksCountFunction" => of(cx.facts.books_read as f64),
        "NRoyaltiesPerWeekCountFunction" => money(royalties(cx)),
        "CatchNFishCountFunction" => match &a.subject {
            Subject::Key(k) => of(cx.tally(&format!("caught:{k}"))),
            _ => String::new(),
        },
        "DiscoverNStarsCountFunction" => of(cx.tally("stars_found")),
        "HaveNChildrenTotalCountFunction" => of(cx.children() as f64),
        "CookNDifferentMealsPerfectlyCountFunction" => of(a.list.len() as f64),
        "UpgradeNObjectsCountFunction" => of(cx.kinds("upgrades") as f64),
        "HaveNPlantsCountFunction" => of(cx.facts.plants as f64),
        _ => String::new(),
    }
}

/// The game's start functions done here: the subjects for which a START node is true already.
pub fn start(f: &str, n: &DnpNode, cx: &Ctx) -> Vec<Subject> {
    let at = |test: &dyn Fn(&Relationship) -> bool| -> Vec<Subject> { cx.rels.0.iter().filter(|(_, r)| test(r)).filter_map(|(e, _)| cx.ids.get(e)).map(|id| Subject::Sim(*id)).collect() };
    let number = n.input_number;
    match f {
        "LearnSkillStartFunction" => cx.skills.0.iter().filter(|(_, v)| **v >= 1.0).map(|(k, _)| Subject::Skill(k.to_string())).collect(),
        "HaveASkillLevelStartFunction" => cx.skills.0.iter().filter(|(_, v)| (**v as u32).min(10) as f32 == number.max(1.0)).map(|(k, _)| Subject::Skill(k.to_string())).collect(),
        "HaveACareerStartFunction" => cx.career_key().map(|k| Subject::Career(k.to_string())).into_iter().collect(),
        "HaveACareerLevelStartFunction" => cx.job.filter(|j| (j.level + 1) as f32 == number).map(|j| Subject::Career(j.career().key.to_string())).into_iter().collect(),
        "KnowASimStartFunction" => at(&|r| r.friendship.abs() >= 1.0 || r.romance > 0.0),
        "BeFriendsWithASimStartFunction" => at(&|r| ltr_at_least(r, "Friend")),
        "BeBestFriendsWithASimStartFunction" => at(&|r| ltr_at_least(r, "Best Friend")),
        "BePartnersStartFunction" => cx.partner().map(Subject::Sim).into_iter().collect(),
        "BeMarriedStartFunction" => cx.spouse().map(Subject::Sim).into_iter().collect(),
        "BeEngagedSimSimStartFunction" => at(&|r| r.status == RelStatus::Engaged),
        "BeEnemiesSimStartFunction" => at(&|r| r.enemy),
        "HaveParentsStartFunction" => cx.family.map(|fam| fam.parents(cx.my_id()).iter().filter(|p| cx.people.contains_key(p)).map(|p| Subject::Sim(*p)).collect()).unwrap_or_default(),
        "HaveAChildStartFunction" => cx.family.map(|fam| fam.children(cx.my_id()).into_iter().filter(|c| cx.people.contains_key(c)).map(Subject::Sim).collect()).unwrap_or_default(),
        "HaveSiblingsStartFunction" => cx.family.map(|fam| fam.siblings_of(cx.my_id()).into_iter().filter(|c| cx.people.contains_key(c)).map(Subject::Sim).collect()).unwrap_or_default(),
        "BeAChildStartFunction" => {
            if cx.sim.age == Age::Child {
                vec![Subject::None]
            } else {
                Vec::new()
            }
        }
        "BeATeenStartFunction" => {
            if cx.sim.age == Age::Teen {
                vec![Subject::None]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        sim: Sim,
        skills: Skills,
        rels: Relationships,
        people: HashMap<u64, Person>,
        ids: HashMap<Entity, u64>,
        data: DnpData,
        facts: Facts<'static>,
        town: Town,
    }

    fn world(age: Age) -> World {
        let sim = crate::sim::random_sim(&mut rand::rng(), "Ann", Some(true), age);
        World { sim, skills: Skills::default(), rels: Relationships::default(), people: HashMap::new(), ids: HashMap::new(), data: DnpData::default(), facts: Facts::default(), town: Town::default() }
    }

    fn cx(w: &World) -> Ctx<'_> {
        Ctx { sim: &w.sim, skills: &w.skills, job: None, rels: &w.rels, mood: 0.0, funds: 1000, worth: 5000, people: &w.people, ids: &w.ids, owned: &[], data: &w.data, facts: &w.facts, town: &w.town, family: None, gdata: None }
    }

    fn prim(name: &str, subject: &str) -> DnpPrimitive {
        DnpPrimitive { name: name.into(), subject: subject.into(), ..Default::default() }
    }

    fn node(number: f32) -> DnpNode {
        DnpNode { input_number: number, ..Default::default() }
    }

    #[test]
    fn a_skill_is_learnt_once_and_only_at_an_age_for_it() {
        let mut w = world(Age::Child);
        let p = prim("Learn $skill", "Skill");
        let mut s = Subject::Skill("Cooking".into());
        assert_eq!(check("LearnSkillCheckFunction", &p, &node(0.0), &mut s, &cx(&w), None), Check::Hidden, "a child can't cook yet");
        let mut s = Subject::Skill("Logic".into());
        assert_eq!(check("LearnSkillCheckFunction", &p, &node(0.0), &mut s, &cx(&w), None), Check::Active);
        w.skills.0.insert("Logic", 1.0);
        assert_eq!(check("LearnSkillCheckFunction", &p, &node(0.0), &mut s, &cx(&w), None), Check::ThrowAway);
        // (A first level, taken as what it's about: once.)
        let mut a = Active::about(Subject::None);
        let ev = DreamEvent { skill: Some("Logic".into()), level: 1, ..DreamEvent::new(Entity::PLACEHOLDER, "kSkillLevelUp") };
        assert_eq!(feedback("LearnSkillFeedbackFunction", &p, &node(0.0), &mut a, &ev, &cx(&w), None), Some(Subject::Skill("Logic".into())));
        assert_eq!(feedback("LearnSkillFeedbackFunction", &p, &node(0.0), &mut a, &ev, &cx(&w), None), None);
    }

    #[test]
    fn a_social_wish_wants_it_given_and_taken_well() {
        let mut w = world(Age::Adult);
        let other = Entity::from_raw_u32(7).unwrap();
        w.ids.insert(other, 42);
        let p = prim("Flirt", "Sim");
        let ev = |recipient: bool, accepted: bool| DreamEvent { other: Some(other), social: Some("Flirt".into()), recipient, accepted, ..DreamEvent::new(Entity::PLACEHOLDER, "kSocialInteraction") };
        let mut a = Active::about(Subject::None);
        let n = node(0.0);
        assert_eq!(feedback("SocialFeedBackFunction", &p, &n, &mut a, &ev(false, true), &cx(&w), None), Some(Subject::Sim(42)));
        assert_eq!(feedback("SocialFeedBackFunction", &p, &n, &mut a, &ev(true, true), &cx(&w), None), None, "received, not given");
        assert_eq!(feedback("SocialFeedBackFunction", &p, &n, &mut a, &ev(false, false), &cx(&w), None), None, "turned down");
        assert_eq!(feedback("SocialSucceedOnGiveOrRecieveFeedbackFunction", &p, &n, &mut a, &ev(true, true), &cx(&w), None), Some(Subject::Sim(42)));
        // (About someone else: not this one.)
        let mut b = Active::about(Subject::Sim(9));
        assert_eq!(feedback("SocialFeedBackFunction", &p, &n, &mut b, &ev(false, true), &cx(&w), None), None);
    }

    #[test]
    fn workouts_add_up_to_a_quarter_hour_and_money_wishes_count_as_the_games() {
        let w = world(Age::Adult);
        let p = prim("Workout", "None");
        let mut a = Active::about(Subject::None);
        let ev = DreamEvent { amount: 0.1, ..DreamEvent::new(Entity::PLACEHOLDER, "kWorkedOut") };
        assert_eq!(feedback("WorkOutFeedbackFunction", &p, &node(0.0), &mut a, &ev, &cx(&w), None), None);
        assert_eq!(feedback("WorkOutFeedbackFunction", &p, &node(0.0), &mut a, &ev, &cx(&w), None), None);
        assert!(feedback("WorkOutFeedbackFunction", &p, &node(0.0), &mut a, &ev, &cx(&w), None).is_some());
        // (At least the sum in hand: met at exactly it; worth more than it: not.)
        let mut s = Subject::None;
        assert_eq!(check("HaveMoreThanNSimoleonsCheckFunction", &p, &node(1000.0), &mut s, &cx(&w), None), Check::AutoSatisfy);
        assert_eq!(check("BeWorthMoreThanNSimoleonsCheckFunction", &p, &node(5000.0), &mut s, &cx(&w), None), Check::Active);
    }

    #[test]
    fn unknown_functions_leave_their_wishes_out() {
        let w = world(Age::Adult);
        let mut s = Subject::None;
        assert_eq!(check("SummonAlienCheckFunction", &prim("x", "None"), &node(0.0), &mut s, &cx(&w), None), Check::NotSupported);
        // (Nor wishes that could never come true here.)
        assert!(!fulfillable("MarryARichSimFeedbackFunction", &prim("Marry a rich sim", "Sim"), &node(0.0), &cx(&w), None));
        assert!(fulfillable("CatchAFishFeedbackFunction", &prim("Catch a $fish", "Fish"), &node(0.0), &cx(&w), None));
        assert!(fulfillable("", &prim("Play chess", "None"), &node(0.0), &cx(&w), None));
    }
}
