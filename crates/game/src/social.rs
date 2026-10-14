//! Relationships and social interactions: separate friendship and romance, relationship
//! statuses (going steady, engaged, married), socials grouped like the game's pie menu, and
//! the chance a Sim accepts them.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::life::{Mood, Trait};
use crate::sim::{Age, Sim};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RelStatus {
    #[default]
    None,
    /// "Going steady".
    Partner,
    Engaged,
    Married,
    /// Broke up or divorced.
    Ex,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Relationship {
    pub friendship: f32,
    pub romance: f32,
    pub status: RelStatus,
    pub kissed: bool,
    /// When they last spent time together (game minutes; 0: not known, taken as now).
    pub last: f64,
}

/// Days without seeing or speaking to each other before a relationship starts to fade.
pub const FADE_AFTER_DAYS: f64 = 3.0;
pub const FRIEND_THRESHOLD: f32 = 15.0;

/// Keeps track of when Sims last spent time together (a social in person or on the phone).
pub fn note_contact(clock: Res<crate::clock::GameClock>, mut events: MessageReader<crate::life::LifeEvent>, mut rels: Query<&mut Relationships>) {
    for ev in events.read() {
        if let crate::life::LifeEventKind::Socialized { other, .. } = ev.kind
            && let Ok(mut r) = rels.get_mut(ev.sim)
        {
            r.entry(other).last = clock.minutes;
        }
    }
}

/// Relationships fade when Sims don't see or speak to each other for a few days, as the game's
/// do: friends drift towards acquaintances (good friends more slowly), romance cools unless
/// they're together, and grudges soften. Spouses, partners and family stay close. The
/// household's relationships, both ways (the town's Sims among themselves are left as they
/// are).
pub fn fade_relationships(
    clock: Res<crate::clock::GameClock>,
    mut last_hour: Local<i64>,
    mut q: Query<(Entity, &crate::sim::Sim, &mut Relationships, Has<crate::sim::HouseholdMember>, Option<&crate::wishes::Wishes>, Option<&crate::journal::SkillJournal>)>,
    family: Res<crate::family::Genealogy>,
) {
    let hour = (clock.minutes / 60.0) as i64;
    if hour == *last_hour {
        return;
    }
    *last_hour = hour;
    let household: std::collections::HashSet<Entity> = q.iter().filter(|x| x.3).map(|x| x.0).collect();
    let ids: HashMap<Entity, u64> = q.iter().map(|x| (x.0, x.1.id)).collect();
    for (_, sim, mut rels, member, wishes, journal) in &mut q {
        // (A Long Distance Friend's friends never drift, nor a Super Friendly Sim's.)
        if crate::wishes::has(wishes, "LongDistanceFriend") || crate::journal::earned(journal, "Super Friendly") {
            continue;
        }
        for (other, r) in rels.0.iter_mut() {
            if r.last <= 0.0 {
                r.last = clock.minutes;
                continue;
            }
            if (!member && !household.contains(other)) || (clock.minutes - r.last) / 1440.0 < FADE_AFTER_DAYS {
                continue;
            }
            let together = matches!(r.status, RelStatus::Married | RelStatus::Engaged | RelStatus::Partner);
            let kin = ids.get(other).is_some_and(|o| family.kin(sim.id, *o).is_some());
            let per_day = if together || kin {
                0.25
            } else if r.friendship >= 60.0 {
                0.5
            } else {
                1.0
            };
            let step = per_day / 24.0;
            let floor = if together || kin { 40.0 } else { 0.0 };
            if r.friendship > floor {
                r.friendship = (r.friendship - step).max(floor);
            } else if r.friendship < 0.0 {
                r.friendship = (r.friendship + step * 0.5).min(0.0);
            }
            if !together && r.romance > 0.0 {
                r.romance = (r.romance - step).max(0.0);
            }
        }
    }
}

impl Relationship {
    /// What they are to one another, in the words for `other` (a woman or a man): "Wife",
    /// "Boyfriend", "Fiancée", "Good Friend".
    pub fn label_for(&self, other_female: bool) -> String {
        let pick = |f: &'static str, m: &'static str| if other_female { f } else { m };
        match self.status {
            RelStatus::Married => pick("Wife", "Husband").into(),
            RelStatus::Engaged => pick("Fiancée", "Fiancé").into(),
            RelStatus::Partner => pick("Girlfriend", "Boyfriend").into(),
            _ => self.label(),
        }
    }

    pub fn label(&self) -> String {
        let friend = match self.friendship {
            v if v < -60.0 => "Enemy",
            v if v < -20.0 => "Disliked",
            v if v < FRIEND_THRESHOLD => "Acquaintance",
            v if v < 40.0 => "Friend",
            v if v < 75.0 => "Good Friend",
            _ => "Best Friend",
        };
        match self.status {
            RelStatus::Married => "Spouse".into(),
            RelStatus::Engaged => "Fiancé(e)".into(),
            RelStatus::Partner => "Partner".into(),
            RelStatus::Ex => format!("Ex · {friend}"),
            RelStatus::None if self.romance >= 20.0 => format!("{friend} · Romantic Interest"),
            RelStatus::None => friend.into(),
        }
    }
}

#[derive(Component, Default, Clone)]
pub struct Relationships(pub HashMap<Entity, Relationship>);

impl Relationships {
    pub fn get(&self, e: Entity) -> Relationship {
        self.0.get(&e).copied().unwrap_or_default()
    }
    pub fn friendship(&self, e: Entity) -> f32 {
        self.get(e).friendship
    }
    pub fn entry(&mut self, e: Entity) -> &mut Relationship {
        self.0.entry(e).or_default()
    }
    /// Returns true only when this change establishes a friendship.
    pub fn add(&mut self, e: Entity, friendship: f32, romance: f32) -> bool {
        let r = self.entry(e);
        let was_friend = r.friendship >= FRIEND_THRESHOLD;
        r.friendship = (r.friendship + friendship).clamp(-100.0, 100.0);
        r.romance = (r.romance + romance).clamp(-100.0, 100.0);
        !was_friend && r.friendship >= FRIEND_THRESHOLD
    }
    /// The Sim's current partner, fiancé(e) or spouse.
    pub fn partner(&self) -> Option<(Entity, RelStatus)> {
        self.0.iter().find(|(_, r)| matches!(r.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married)).map(|(e, r)| (*e, r.status))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SocialCat {
    Friendly,
    Funny,
    Romantic,
    Mean,
    Special,
    /// Looking after a baby or toddler.
    Care,
}

impl SocialCat {
    pub const ALL: [SocialCat; 6] = [SocialCat::Care, SocialCat::Friendly, SocialCat::Funny, SocialCat::Romantic, SocialCat::Mean, SocialCat::Special];
    pub fn name(self) -> &'static str {
        match self {
            SocialCat::Friendly => "Friendly",
            SocialCat::Funny => "Funny",
            SocialCat::Romantic => "Romantic",
            SocialCat::Mean => "Mean",
            SocialCat::Special => "Special",
            SocialCat::Care => "Care",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SocialEffect {
    None,
    /// Throwing a ball back and forth, a few metres apart (with a ball about).
    PlayCatch,
    Kiss,
    GoSteady,
    Propose,
    Marry,
    BreakUp,
    MoveIn,
    AskToLeave,
    WooHoo,
    TryForBaby,
    PutToBed,
    AskOnDate,
    /// A toddler's lesson in walking or talking.
    TeachWalk,
    TeachTalk,
    /// A scuffle: one wins, the other is left embarrassed.
    Fight,
    /// Making up after falling out.
    Apologize,
    DeclareNemesis,
    /// Comforting a Sim in a bad mood.
    CheerUp,
    /// A massage that leaves the other Sim comfortable.
    BackRub,
    /// Helping a child or teen through their homework (it's done, and done well).
    HelpHomework,
    /// Greeting a caller at the door (who then comes in).
    Greet,
}

pub struct SocialDef {
    pub name: &'static str,
    pub cat: SocialCat,
    pub minutes: f32,
    pub social_per_hour: f32,
    pub fun_per_hour: f32,
    pub friendship: f32,
    pub romance: f32,
    pub min_friendship: f32,
    pub min_romance: f32,
    /// Only for Sims who don't get on this well (a fight, an apology).
    pub max_friendship: f32,
    pub autonomous: bool,
    pub effect: SocialEffect,
    /// What a care social does for the little one's needs, per hour.
    pub care: [f32; 6],
    /// Only for a Sim of this life state (a vampire's hypnotic gaze, a witch's spells).
    pub needs: Option<crate::sim::Occult>,
    /// One of the game's own socials (from its social data): what more it asks, and its clips.
    pub game: Option<&'static GameRules>,
}

/// What one of the game's own socials asks beyond the hand-made ones' rules: the ages that do it
/// and have it done to them, the traits wanted in the one it's done to (any of them), the traits
/// that make a Sim keener on it or less keen, and its animations (the clips' names without their
/// side, `_x` / `_y`).
pub struct GameRules {
    pub actor_ages: Vec<Age>,
    pub target_ages: Vec<Age>,
    pub target_traits: Vec<Trait>,
    pub keen: Vec<Trait>,
    pub unkeen: Vec<Trait>,
    pub clips: &'static [&'static str],
    /// The topics it's offered under (the game's `<Trait> Trait` and `Buff <Moodlet>` topics,
    /// and a few more): offered when the one who does it has the trait or the moodlet (`true`),
    /// or the one it's done to (`false`). None: always.
    pub topics: Vec<(Topic, bool)>,
}

/// A topic a social's offered under: a Sim's trait, or a moodlet of theirs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Topic {
    Trait(Trait),
    Moodlet(crate::life::MoodletKind),
}

/// The game's topic, if it's one kept here: `<Trait> Trait`, `Buff <Moodlet>` (by the game's
/// buff name), and the pregnancy, a new job and a new home.
fn topic(name: &str) -> Option<Topic> {
    use crate::life::MoodletKind as M;
    let squash = |s: &str| s.replace(' ', "");
    if let Some(t) = name.strip_suffix(" Trait") {
        let t = squash(t);
        return Trait::ALL.iter().find(|x| x.game_id().eq_ignore_ascii_case(&t) || format!("{x:?}").eq_ignore_ascii_case(&t)).map(|x| Topic::Trait(*x));
    }
    if let Some(b) = name.strip_prefix("Buff ") {
        let b = squash(b);
        // (The game's names for some of them.)
        let b = match b.as_str() {
            "Stinky" => "Smelly",
            "Hunger" => "Hungry",
            other => other,
        };
        return M::ALL.iter().find(|m| m.buff().0.eq_ignore_ascii_case(b)).map(|m| Topic::Moodlet(*m));
    }
    match name {
        "Pregnancy" => Some(Topic::Moodlet(M::Pregnant)),
        "New Job" => Some(Topic::Moodlet(M::NewJob)),
        "New House" => Some(Topic::Moodlet(M::NewHome)),
        _ => None,
    }
}

const fn sd(name: &'static str, cat: SocialCat, minutes: f32, social: f32, fun: f32, friendship: f32, romance: f32) -> SocialDef {
    SocialDef {
        name,
        cat,
        minutes,
        social_per_hour: social,
        fun_per_hour: fun,
        friendship,
        romance,
        min_friendship: -100.0,
        min_romance: -100.0,
        max_friendship: 101.0,
        autonomous: false,
        effect: SocialEffect::None,
        care: [0.0; 6],
        needs: None,
        game: None,
    }
}

const fn occult(o: crate::sim::Occult, def: SocialDef) -> SocialDef {
    SocialDef { needs: Some(o), ..def }
}

use crate::sim::Occult as O;

use SocialCat::*;
pub static SOCIALS: [SocialDef; 62] = [
    SocialDef { autonomous: true, ..sd("Chat", Friendly, 25.0, 110.0, 10.0, 8.0, 0.0) },
    SocialDef { effect: SocialEffect::Greet, ..sd("Greet", Friendly, 3.0, 80.0, 5.0, 4.0, 0.0) },
    SocialDef { effect: SocialEffect::HelpHomework, ..sd("Help with Homework", Friendly, 40.0, 60.0, -10.0, 5.0, 0.0) },
    SocialDef { autonomous: true, ..sd("Get to Know", Friendly, 15.0, 100.0, 5.0, 9.0, 0.0) },
    sd("Compliment", Friendly, 8.0, 80.0, 0.0, 7.0, 0.0),
    SocialDef { min_friendship: 15.0, autonomous: true, ..sd("Talk About Hobbies", Friendly, 20.0, 110.0, 15.0, 9.0, 0.0) },
    SocialDef { min_friendship: 30.0, ..sd("Hug", Friendly, 6.0, 140.0, 20.0, 8.0, 0.0) },
    SocialDef { min_friendship: 20.0, ..sd("Dance Together", Friendly, 30.0, 80.0, 80.0, 10.0, 2.0) },
    SocialDef { min_friendship: 40.0, ..sd("High Five", Friendly, 4.0, 90.0, 30.0, 6.0, 0.0) },
    SocialDef { autonomous: true, ..sd("Tell Joke", Funny, 12.0, 90.0, 80.0, 6.0, 0.0) },
    SocialDef { min_friendship: 10.0, ..sd("Do Funny Impression", Funny, 10.0, 80.0, 90.0, 6.0, 0.0) },
    SocialDef { min_friendship: 20.0, ..sd("Tickle", Funny, 6.0, 90.0, 70.0, 5.0, 1.0) },
    SocialDef { min_friendship: 5.0, ..sd("Flirt", Romantic, 8.0, 100.0, 20.0, 2.0, 10.0) },
    SocialDef { min_romance: 10.0, ..sd("Compliment Appearance", Romantic, 6.0, 90.0, 10.0, 3.0, 8.0) },
    SocialDef { min_romance: 25.0, ..sd("Hold Hands", Romantic, 8.0, 120.0, 20.0, 3.0, 9.0) },
    SocialDef { min_romance: 30.0, effect: SocialEffect::Kiss, ..sd("Kiss", Romantic, 6.0, 160.0, 40.0, 4.0, 14.0) },
    SocialDef { min_romance: 50.0, effect: SocialEffect::Kiss, ..sd("Make Out", Romantic, 15.0, 180.0, 60.0, 5.0, 16.0) },
    SocialDef { min_romance: 50.0, effect: SocialEffect::GoSteady, ..sd("Ask to Go Steady", Romantic, 8.0, 120.0, 10.0, 5.0, 10.0) },
    SocialDef { min_romance: 12.0, effect: SocialEffect::AskOnDate, ..sd("Ask on Date", Romantic, 6.0, 80.0, 20.0, 3.0, 4.0) },
    SocialDef { min_romance: 70.0, effect: SocialEffect::Propose, ..sd("Propose Marriage", Romantic, 10.0, 140.0, 20.0, 10.0, 15.0) },
    SocialDef { min_romance: 70.0, effect: SocialEffect::Marry, ..sd("Get Married", Romantic, 30.0, 160.0, 40.0, 15.0, 20.0) },
    SocialDef { min_romance: 60.0, effect: SocialEffect::WooHoo, ..sd("WooHoo", Romantic, 30.0, 200.0, 120.0, 6.0, 15.0) },
    sd("Insult", Mean, 6.0, 40.0, -30.0, -12.0, -5.0),
    sd("Argue", Mean, 15.0, 60.0, -60.0, -15.0, -5.0),
    SocialDef { min_friendship: -100.0, ..sd("Slap", Mean, 4.0, 30.0, -40.0, -20.0, -15.0) },
    SocialDef { effect: SocialEffect::BreakUp, ..sd("Break Up", Mean, 10.0, 40.0, -60.0, -30.0, -60.0) },
    SocialDef { min_friendship: 40.0, effect: SocialEffect::MoveIn, ..sd("Ask to Move In", Special, 10.0, 100.0, 10.0, 5.0, 0.0) },
    SocialDef { effect: SocialEffect::AskToLeave, ..sd("Say Goodbye", Special, 4.0, 40.0, 0.0, 1.0, 0.0) },
    SocialDef { min_romance: 60.0, effect: SocialEffect::TryForBaby, ..sd("Try for Baby", Romantic, 30.0, 200.0, 120.0, 6.0, 15.0) },
    // Hunger, bladder, energy, social, hygiene, fun (per hour, for the little one).
    SocialDef { autonomous: true, care: [500.0, 0.0, 0.0, 60.0, 0.0, 0.0], ..sd("Feed", Care, 15.0, 40.0, 10.0, 5.0, 0.0) },
    SocialDef { autonomous: true, care: [0.0, 700.0, 0.0, 30.0, 400.0, 0.0], ..sd("Change Diaper", Care, 10.0, 30.0, -10.0, 4.0, 0.0) },
    SocialDef { autonomous: true, care: [0.0, 0.0, 0.0, 300.0, 0.0, 200.0], ..sd("Play With", Care, 20.0, 120.0, 120.0, 8.0, 0.0) },
    SocialDef { care: [0.0, 0.0, 0.0, 250.0, 0.0, 120.0], ..sd("Read to", Care, 30.0, 80.0, 60.0, 8.0, 0.0) },
    SocialDef { effect: SocialEffect::PutToBed, care: [0.0, 0.0, 0.0, 60.0, 0.0, 0.0], ..sd("Put to Bed", Care, 6.0, 40.0, 0.0, 3.0, 0.0) },
    SocialDef { effect: SocialEffect::TeachWalk, care: [0.0, 0.0, -20.0, 150.0, 0.0, 60.0], ..sd("Teach to Walk", Care, 30.0, 60.0, 20.0, 6.0, 0.0) },
    SocialDef { effect: SocialEffect::TeachTalk, care: [0.0, 0.0, 0.0, 200.0, 0.0, 40.0], ..sd("Teach to Talk", Care, 30.0, 70.0, 10.0, 6.0, 0.0) },
    // More of the game's socials.
    SocialDef { autonomous: true, min_friendship: 10.0, ..sd("Tell Funny Story", Funny, 12.0, 100.0, 50.0, 8.0, 0.0) },
    SocialDef { min_friendship: 10.0, ..sd("Tell Dramatic Story", Friendly, 15.0, 100.0, 20.0, 7.0, 0.0) },
    sd("Brag", Friendly, 8.0, 70.0, 5.0, 2.0, 0.0),
    SocialDef { autonomous: true, min_friendship: 20.0, ..sd("Goof Around", Funny, 10.0, 90.0, 70.0, 7.0, 0.0) },
    sd("Make Silly Face", Funny, 5.0, 60.0, 40.0, 4.0, 0.0),
    SocialDef { min_friendship: 40.0, ..sd("Cry on Shoulder", Friendly, 10.0, 120.0, -10.0, 10.0, 0.0) },
    SocialDef { effect: SocialEffect::CheerUp, min_friendship: 15.0, ..sd("Cheer Up", Friendly, 8.0, 90.0, 20.0, 8.0, 0.0) },
    SocialDef { effect: SocialEffect::Apologize, max_friendship: 0.0, ..sd("Apologize", Special, 6.0, 40.0, 0.0, 12.0, 0.0) },
    SocialDef { effect: SocialEffect::Fight, max_friendship: -20.0, ..sd("Fight", Mean, 8.0, -20.0, 10.0, -20.0, -10.0) },
    sd("Yell At", Mean, 6.0, -10.0, -5.0, -9.0, -3.0),
    sd("Irritate", Mean, 6.0, -5.0, 5.0, -6.0, 0.0),
    SocialDef { effect: SocialEffect::DeclareNemesis, max_friendship: -60.0, ..sd("Declare Nemesis", Mean, 5.0, -20.0, 0.0, -20.0, -10.0) },
    SocialDef { min_romance: 15.0, ..sd("Embrace", Romantic, 6.0, 120.0, 20.0, 5.0, 8.0) },
    SocialDef { min_romance: 20.0, ..sd("Gaze Into Eyes", Romantic, 6.0, 100.0, 20.0, 3.0, 9.0) },
    SocialDef { effect: SocialEffect::BackRub, min_friendship: 30.0, ..sd("Give Back Rub", Romantic, 12.0, 120.0, 40.0, 6.0, 6.0) },
    SocialDef { min_romance: 50.0, ..sd("Leap Into Arms", Romantic, 5.0, 140.0, 40.0, 4.0, 10.0) },
    SocialDef { effect: SocialEffect::Kiss, min_romance: 60.0, ..sd("Dip Kiss", Romantic, 6.0, 150.0, 30.0, 4.0, 12.0) },
    SocialDef { effect: SocialEffect::PlayCatch, ..sd("Play Catch", Funny, 30.0, 60.0, 120.0, 6.0, 0.0) },
    // The Supernatural pack's (and Late Night's): each for its own kind.
    occult(O::Vampire, sd("Hypnotic Gaze", Romantic, 6.0, 100.0, 30.0, 4.0, 8.0)),
    occult(O::Vampire, sd("Talk About the Joy of Plasma", Friendly, 15.0, 100.0, 20.0, 6.0, 0.0)),
    occult(O::Fairy, sd("Frolic", Funny, 20.0, 110.0, 120.0, 8.0, 0.0)),
    occult(O::Fairy, sd("Play Fairy Trick", Mean, 6.0, 30.0, 60.0, -6.0, 0.0)),
    occult(O::Fairy, sd("Brag About Wings", Friendly, 8.0, 80.0, 20.0, 3.0, 0.0)),
    occult(O::Werewolf, sd("Practice Fighting", Funny, 15.0, 90.0, 90.0, 6.0, 0.0)),
    occult(O::Witch, sd("Cast Charm Spell", Friendly, 8.0, 80.0, 40.0, 15.0, 4.0)),
    occult(O::Witch, sd("Cast Curse", Mean, 8.0, 20.0, 30.0, -15.0, -5.0)),
];

/// How far apart the two Sims stand for a social: a few metres to play catch, else close
/// enough to talk.
pub fn apart(def: &SocialDef) -> f32 {
    if def.effect == SocialEffect::PlayCatch { 3.6 } else { 0.9 }
}

pub fn social_index(name: &str) -> Option<usize> {
    all_socials().find(|(_, s)| s.name == name).map(|(i, _)| i)
}

/// The game's own socials, numbered on from the hand-made ones (`SOCIALS`): those from its
/// social data it has the animations for and whose conditions are known here. Filled in once
/// its data is loaded (see `load_game_socials`).
static GAME_SOCIALS: std::sync::OnceLock<Vec<SocialDef>> = std::sync::OnceLock::new();

/// A social by its number: one of the hand-made ones, or the game's own after them.
pub fn social(i: usize) -> &'static SocialDef {
    SOCIALS.get(i).or_else(|| GAME_SOCIALS.get()?.get(i.checked_sub(SOCIALS.len())?)).unwrap_or(&SOCIALS[0])
}

/// Every social, with its number.
pub fn all_socials() -> impl Iterator<Item = (usize, &'static SocialDef)> {
    SOCIALS.iter().chain(GAME_SOCIALS.get().into_iter().flatten()).enumerate()
}

/// The game's tests (its `ProcTest`s) a social of its may have that are taken here as passed:
/// those that only ask what's asked anyway (how well they get on, how romantic they are).
const PASSED_TESTS: &[&str] = &["TestChat", "TalkAboutWeatherTest", "ShareInterestsProceduralTest", "TestFriendlyHug", "TestKissOnCheek", "TestAskAboutRecentAction"];

/// The word in their clips' names for the game's socials with a graph of their own (not its
/// `social_generic` with a suite): asking for things, sharing interests, compliments...
const GRAPH_CLIPS: &[(&str, &str)] = &[
    ("social_askFor", "AskFor"),
    ("social_share_interests", "ShareInterests"),
    ("FriendlyCompliment", "Compliment"),
    ("FlirtyCompliment", "Compliment"),
    ("social_boastabout", "BoastAbout"),
    ("social_insult", "ExtraInsult"),
    ("social_neutral_telljoke", "tellJoke"),
    ("FriendlyHug", "FriendlyHug"),
    ("socialization_kissmakeout", "kissMakeOut"),
    ("social_apologize", "Apologize"),
    ("social_yellAt", "YellAt"),
    ("social_massage", "Massage"),
    ("social_tell_funny_story", "TellFunnyStory"),
    ("social_boretodeath", "BoreToDeath"),
    ("social_shoo", "Shoo"),
    ("social_shykiss", "ShyKiss"),
    // (Just talking: the chat's own.)
    ("social_neutral_talk", "Gossip"),
];

/// Clip names for how a social goes down badly: left out of one that's taken well.
const BADLY: &[&str] = &["reject", "creepy", "boring", "awkward"];

/// The game's socials left out: those that need what isn't kept here (the dark, a skill to
/// teach, old times, a tattoo parlour, someone else's affairs), and its "Goodbye" (it's "Say
/// Goodbye" here).
const LEFT_OUT: &[&str] = &[
    "Fret Over Darkness",
    "Ask to be Tattooed",
    "Persuade To Change Body Shape",
    "Teach Skill",
    "Goodbye",
    "Trait Incompatibility",
    "Talk About Shared Traits",
    "Reminisce",
];

/// The age a letter of the game's tables stands for.
fn age_of(letter: &str) -> Option<Age> {
    match letter {
        "T" => Some(Age::Teen),
        "Y" => Some(Age::YoungAdult),
        "A" => Some(Age::Adult),
        "E" => Some(Age::Elder),
        _ => None,
    }
}

/// The game's own socials, from its social data: each one not hand-made already (by name), with
/// a test that's known or none, for teens and up (children's have their own clips), its tone
/// telling its kind (friendly, funny, romantic, mean) and what it does, and its suite's clips
/// (those `clip_names` has: `a2a_soc_<stance>_<suite>_..._x` and the `_y` to go with it).
pub fn game_socials(actions: &[s3bake::gamedata::SocialAction], clip_names: &[String]) -> Vec<SocialDef> {
    let leak = |s: &str| -> &'static str { Box::leak(s.to_string().into_boxed_str()) };
    let traits = |ids: &[String]| -> Vec<Trait> { ids.iter().filter_map(|id| Trait::ALL.iter().find(|t| t.game_id().eq_ignore_ascii_case(id)).copied()).collect() };
    let mut out: Vec<SocialDef> = Vec::new();
    for a in actions {
        // (Its clips' word: its suite, or its graph's.)
        let word = if a.suite.is_empty() { GRAPH_CLIPS.iter().find(|g| g.0.eq_ignore_ascii_case(&a.graph)).map(|g| g.1) } else { Some(a.suite.as_str()) };
        let Some(word) = word else { continue };
        if !a.test.is_empty() && !PASSED_TESTS.contains(&a.test.as_str()) {
            continue;
        }
        if SOCIALS.iter().chain(out.iter()).any(|s| s.name.eq_ignore_ascii_case(&a.name)) || LEFT_OUT.contains(&a.name.as_str()) || a.name.contains('{') {
            continue;
        }
        // (Offered under topics: those of traits and moodlets are known here; what's only offered
        // under others, a birthday, a burglary, is left out.)
        let topics: Vec<(Topic, bool)> = a.topics.iter().filter_map(|(t, first)| Some((topic(t)?, *first))).collect();
        if !a.topics.is_empty() && topics.is_empty() {
            continue;
        }
        let (cat, minutes, social, fun, friendship, romance) = match a.tone.as_str() {
            "Friendly" | "Neutral" | "Impressive" => (SocialCat::Friendly, 8.0, 90.0, 5.0, 5.0, 0.0),
            "Funny" => (SocialCat::Funny, 8.0, 85.0, 60.0, 4.0, 0.0),
            "Amorous" => (SocialCat::Romantic, 8.0, 100.0, 20.0, 2.0, 7.0),
            "Insulting" | "Steamed" => (SocialCat::Mean, 6.0, 40.0, -20.0, -10.0, -3.0),
            _ => continue,
        };
        let ages = |l: &[String]| -> Vec<Age> { l.iter().filter_map(|x| age_of(x)).collect() };
        let (actor_ages, target_ages) = (ages(&a.actor_ages), ages(&a.target_ages));
        if actor_ages.is_empty() || target_ages.is_empty() {
            continue;
        }
        // (Its clips: the Sims' sides of its suite's, both there.)
        let suite = format!("_{}_", word.to_ascii_lowercase());
        let mut clips: Vec<&'static str> = clip_names
            .iter()
            .filter(|n| n.starts_with("a2a_soc_") && n.to_ascii_lowercase().contains(&suite))
            .filter_map(|n| n.strip_suffix("_x"))
            .filter(|base| clip_names.iter().any(|m| m.strip_suffix("_y") == Some(*base)))
            .map(leak)
            .collect();
        clips.sort();
        clips.dedup();
        // (Taken well, unless it's meant to go down badly; in its own tone, where there are those.)
        let well: Vec<&'static str> = clips.iter().copied().filter(|c| !BADLY.iter().any(|b| c.to_ascii_lowercase().contains(b))).collect();
        if cat != SocialCat::Mean && !well.is_empty() {
            clips = well;
        }
        let tones: &[&str] = match cat {
            SocialCat::Friendly => &["_friendly"],
            SocialCat::Funny => &["_funny", "_friendly"],
            SocialCat::Romantic => &["_amorous", "_flirty"],
            _ => &["_insulting", "_steamed", "_bad"],
        };
        let toned: Vec<&'static str> = clips.iter().copied().filter(|c| tones.iter().any(|t| c.to_ascii_lowercase().contains(t))).collect();
        if !toned.is_empty() {
            clips = toned;
        }
        // (The neutral stance's first, as most begin from it.)
        clips.sort_by_key(|c| !c.to_ascii_lowercase().starts_with("a2a_soc_neutral_"));
        clips.truncate(4);
        if clips.is_empty() {
            continue;
        }
        let (min_friendship, max_friendship) = a.liking.map_or((-100.0, 101.0), |(lo, hi)| (lo, if hi >= 100.0 { 101.0 } else { hi }));
        let rules: &'static GameRules = Box::leak(Box::new(GameRules {
            actor_ages,
            target_ages,
            target_traits: traits(&a.target_traits),
            keen: traits(&a.keen),
            unkeen: traits(&a.unkeen),
            clips: Box::leak(clips.into_boxed_slice()),
            topics,
        }));
        // (Romance only where there's some already; asked for or not, the friendly and the funny
        // are what Sims get up to of their own accord.)
        let min_romance = if cat == SocialCat::Romantic { 10.0 } else { -100.0 };
        let autonomous = a.autonomous && !a.user_only && matches!(cat, SocialCat::Friendly | SocialCat::Funny) && rules.target_traits.is_empty() && rules.topics.is_empty();
        out.push(SocialDef {
            min_friendship,
            max_friendship,
            min_romance,
            autonomous,
            game: Some(rules),
            ..sd(leak(&a.name), cat, minutes, social, fun, friendship, romance)
        });
    }
    out
}

/// One of the game's own socials a Sim might start of their own accord with someone of an age
/// they get on with this well: the friendly and the funny, weighted to what their traits favour
/// (three times as likely) or not (a fifth).
pub fn pick_game_social(actor: &Sim, target_age: Age, rel: &Relationship, rng: &mut impl rand::Rng) -> Option<&'static str> {
    use rand::seq::IndexedRandom;
    let fits: Vec<(&'static SocialDef, f32)> = all_socials()
        .filter_map(|(_, s)| {
            let g = s.game?;
            let ok = s.autonomous
                && g.actor_ages.contains(&actor.age)
                && g.target_ages.contains(&target_age)
                && rel.friendship >= s.min_friendship
                && rel.friendship <= s.max_friendship
                && rel.romance >= s.min_romance;
            let keen = if g.keen.iter().any(|t| actor.traits.contains(t)) { 3.0 } else { 1.0 };
            let unkeen = if g.unkeen.iter().any(|t| actor.traits.contains(t)) { 0.2 } else { 1.0 };
            ok.then_some((s, keen * unkeen))
        })
        .collect();
    fits.choose_weighted(rng, |f| f.1).ok().map(|f| f.0.name)
}

/// The phrases the game's socials of a kind share, gathered on their own menu ("Ask About ›",
/// then Day, Career...) when there are three or more.
const GROUPS: &[&str] = &["Accuse of ", "Ask About ", "Ask for ", "Ask to ", "Boast About ", "Brag About ", "Complain About ", "Compliment ", "Discuss ", "Enthuse About ", "Enthuse about ", "Joke About ", "Mock ", "Talk About ", "Announce ", "Tell "];

/// A menu's socials with those that share a phrase gathered on their own menu (made by `submenu`
/// from its label and options; it gives back what opens it), named there by the rest of their
/// names.
pub fn grouped<K: Clone>(list: Vec<(String, K)>, mut submenu: impl FnMut(&str, Vec<(String, K)>) -> K) -> Vec<(String, K)> {
    let mut out: Vec<(String, K)> = Vec::new();
    let mut done: Vec<&str> = Vec::new();
    for (label, kind) in &list {
        let Some(g) = GROUPS.iter().find(|g| label.starts_with(**g)) else {
            out.push((label.clone(), kind.clone()));
            continue;
        };
        let members: Vec<&(String, K)> = list.iter().filter(|(l, _)| l.starts_with(g)).collect();
        if members.len() < 3 {
            out.push((label.clone(), kind.clone()));
            continue;
        }
        if done.contains(g) {
            continue;
        }
        done.push(g);
        let rest = |l: &str| {
            let r = l[g.len()..].trim();
            let mut c = r.chars();
            c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        };
        let sub: Vec<(String, K)> = members.iter().map(|(l, k)| (rest(l), k.clone())).collect();
        let title = format!("{} ›", g.trim());
        let opens = submenu(g.trim(), sub);
        out.push((title, opens));
    }
    out
}

/// Loads the game's own socials once its data is there.
pub fn load_game_socials(ui: Option<Res<crate::icons::GameUi>>, data: Option<Res<crate::baked::Baked>>) {
    if GAME_SOCIALS.get().is_some() {
        return;
    }
    let (Some(ui), Some(data)) = (ui, data) else { return };
    let list = game_socials(&ui.data.socials, &data.0.clip_names);
    info!("socials: {} of the game's own besides the {} hand-made", list.len(), SOCIALS.len());
    let _ = GAME_SOCIALS.set(list);
}

/// Whether `actor` can start this social with `target` (shown in the pie menu); `kin`: whether
/// they're family (no romance between relatives).
pub fn available(def: &SocialDef, rel: &Relationship, actor: &Sim, target: &Sim, target_in_household: bool, kin: bool, moods: [Option<&crate::life::Moodlets>; 2]) -> bool {
    if rel.friendship < def.min_friendship || rel.romance < def.min_romance || rel.friendship > def.max_friendship {
        return false;
    }
    // (The game's own: the ages it's for, and the traits it asks of the one it's done to.)
    if let Some(g) = def.game
        && (!g.actor_ages.contains(&actor.age) || !g.target_ages.contains(&target.age) || (!g.target_traits.is_empty() && !g.target_traits.iter().any(|t| target.traits.contains(t))))
    {
        return false;
    }
    // (Under a topic: the one with the trait or the moodlet, the Sim or the other.)
    if let Some(g) = def.game
        && !g.topics.is_empty()
        && !g.topics.iter().any(|(t, first)| {
            let (sim, mood) = if *first { (actor, moods[0]) } else { (target, moods[1]) };
            match t {
                Topic::Trait(x) => sim.traits.contains(x),
                Topic::Moodlet(m) => mood.is_some_and(|ml| ml.has(*m)),
            }
        })
    {
        return false;
    }
    if def.needs.is_some_and(|o| actor.occult != Some(o)) {
        return false;
    }
    if kin && (def.cat == SocialCat::Romantic || matches!(def.effect, SocialEffect::WooHoo | SocialEffect::TryForBaby | SocialEffect::Propose | SocialEffect::Marry | SocialEffect::GoSteady)) {
        return false;
    }
    // Babies and toddlers are looked after rather than chatted with.
    if actor.age.is_little() {
        return false;
    }
    if target.age.is_little() {
        let toddler_only = matches!(def.effect, SocialEffect::TeachWalk | SocialEffect::TeachTalk) || def.name == "Read to";
        return def.cat == SocialCat::Care && !matches!(actor.age, Age::Child) && (!toddler_only || target.age == Age::Toddler);
    }
    if def.cat == SocialCat::Care {
        return false;
    }
    if def.effect == SocialEffect::TryForBaby
        && (actor.female == target.female || !matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married))
    {
        return false;
    }
    let grown = |a: Age| !matches!(a, Age::Child | Age::Teen);
    let adults = grown(actor.age) && grown(target.age);
    // Teens may court other teens, but marriage and woohoo are for adults.
    let teens = actor.age == Age::Teen && target.age == Age::Teen;
    if def.cat == SocialCat::Romantic && !adults && !teens {
        return false;
    }
    if !adults && matches!(def.effect, SocialEffect::Propose | SocialEffect::Marry | SocialEffect::WooHoo) {
        return false;
    }
    match def.effect {
        SocialEffect::GoSteady => rel.status == RelStatus::None || rel.status == RelStatus::Ex,
        SocialEffect::Propose => rel.status == RelStatus::Partner,
        SocialEffect::Marry => rel.status == RelStatus::Engaged,
        SocialEffect::BreakUp => matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married),
        SocialEffect::MoveIn => !target_in_household && (rel.friendship >= 40.0 || rel.status != RelStatus::None),
        SocialEffect::AskToLeave => !target_in_household,
        // (A teen or grown-up helps a child or teen; whether there's homework is for the menu.)
        SocialEffect::HelpHomework => actor.age != Age::Child && matches!(target.age, Age::Child | Age::Teen),
        // (Who's at the door is for the menu: the household's own let callers in.)
        SocialEffect::Greet => target_in_household == false,
        SocialEffect::WooHoo => matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married) || rel.romance >= 80.0,
        _ => true,
    }
}

/// Chance (0..1) that the target goes along with the social.
pub fn acceptance(def: &SocialDef, rel: &Relationship, target: &Sim, target_mood: &Mood, has_other_partner: bool) -> f32 {
    let has = |t: Trait| target.traits.contains(&t);
    let mood = target_mood.level() / 100.0;
    let mut p = match def.cat {
        // Friendly chat is almost always welcome unless the Sim is in a foul mood or dislikes you.
        SocialCat::Friendly | SocialCat::Funny => 0.97 + rel.friendship.min(0.0) / 150.0 + mood.min(0.0) * 0.2,
        SocialCat::Mean => 1.0,
        SocialCat::Special => 0.6 + rel.friendship / 200.0,
        SocialCat::Care => 1.0,
        SocialCat::Romantic => {
            let base = match def.effect {
                SocialEffect::GoSteady => 0.2 + rel.romance / 120.0,
                SocialEffect::Propose => 0.1 + rel.romance / 110.0,
                SocialEffect::Marry | SocialEffect::WooHoo | SocialEffect::TryForBaby => 0.5 + rel.romance / 200.0,
                _ => 0.35 + rel.romance / 100.0 + rel.friendship / 300.0,
            };
            base + mood * 0.15
        }
    };
    if def.cat == SocialCat::Romantic {
        if has(Trait::Flirty) || has(Trait::HopelessRomantic) {
            p += 0.15;
        }
        if has(Trait::Unflirty) {
            p -= 0.3;
        }
        if has(Trait::CommitmentIssues) && matches!(def.effect, SocialEffect::GoSteady | SocialEffect::Propose | SocialEffect::Marry) {
            p -= 0.35;
        }
        if has_other_partner {
            p -= 0.5;
        }
    }
    if def.cat == SocialCat::Funny {
        if has(Trait::GoodSenseOfHumor) {
            p += 0.1;
        }
        if has(Trait::Grumpy) {
            p -= 0.35;
        }
        if has(Trait::NoSenseOfHumor) {
            p -= 0.5;
        }
    }
    if has(Trait::EasilyImpressed) {
        p += 0.1;
    }
    // The evil and the mean-spirited take an insult in their stride.
    if def.cat == SocialCat::Mean && (has(Trait::Evil) || has(Trait::MeanSpirited)) {
        p = p.max(0.9);
    }
    if def.cat == SocialCat::Friendly && has(Trait::Loner) {
        p -= 0.15;
    }
    p.clamp(0.02, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socials_sharing_a_phrase_get_a_menu_of_their_own() {
        let list: Vec<(String, u32)> = ["Chat", "Ask About Day", "Ask About Career", "Ask About Kids", "Mock Dancing", "Mock Ambition", "Admire"]
            .iter()
            .enumerate()
            .map(|(i, n)| (n.to_string(), i as u32))
            .collect();
        let mut menus: Vec<(String, Vec<(String, u32)>)> = Vec::new();
        let out = grouped(list, |label, sub| {
            menus.push((label.to_string(), sub));
            100 + menus.len() as u32
        });
        // (Three asks gathered; two mocks aren't enough for a menu.)
        let labels: Vec<&str> = out.iter().map(|o| o.0.as_str()).collect();
        assert_eq!(labels, ["Chat", "Ask About ›", "Mock Dancing", "Mock Ambition", "Admire"]);
        assert_eq!(out[1].1, 101);
        assert_eq!(menus[0].0, "Ask About");
        let sub: Vec<&str> = menus[0].1.iter().map(|o| o.0.as_str()).collect();
        assert_eq!(sub, ["Day", "Career", "Kids"]);
        assert_eq!(menus[0].1[0].1, 1);
    }

    #[test]
    fn the_games_own_socials_come_with_their_clips_and_rules() {
        use s3bake::gamedata::SocialAction;
        let act = |name: &str, tone: &str, suite: &str, test: &str| SocialAction {
            key: name.into(),
            name: name.into(),
            tone: tone.into(),
            actor_ages: vec!["C".into(), "T".into(), "Y".into(), "A".into()],
            target_ages: vec!["T".into(), "Y".into()],
            autonomous: true,
            suite: suite.into(),
            test: test.into(),
            target_traits: if name == "Accuse of Being Evil" { vec!["Evil".into()] } else { vec![] },
            liking: Some((-19.0, 100.0)),
            ..Default::default()
        };
        let actions = [
            act("Accuse of Being Evil", "Insulting", "Accuse", ""),
            act("Admire", "Friendly", "Admire", ""),
            // (Hand-made already; a test not known here; no clips for it; a tone not handled.)
            act("Chat", "Friendly", "Chat", ""),
            act("Ask for Promotion", "Friendly", "Admire", "TestAskForPromotion"),
            act("Sing", "Friendly", "Sing", ""),
            act("Mumble", "Boring", "Admire", ""),
        ];
        let clips: Vec<String> =
            ["a2a_soc_Bad_Accuse_Insulting_Bad_x", "a2a_soc_Bad_Accuse_Insulting_Bad_y", "a2a_soc_Neutral_Admire_Friendly_Neutral_x", "a2a_soc_Neutral_Admire_Friendly_Neutral_y", "a2a_soc_Neutral_Admire_Lonely_x"]
                .map(String::from)
                .to_vec();
        let got = game_socials(&actions, &clips);
        let names: Vec<&str> = got.iter().map(|s| s.name).collect();
        assert_eq!(names, ["Accuse of Being Evil", "Admire"]);
        let (accuse, admire) = (&got[0], &got[1]);
        assert_eq!(accuse.cat, SocialCat::Mean);
        assert_eq!(accuse.game.unwrap().clips, ["a2a_soc_Bad_Accuse_Insulting_Bad"]);
        // (Only a clip with both sides.)
        assert_eq!(admire.game.unwrap().clips, ["a2a_soc_Neutral_Admire_Friendly_Neutral"]);
        assert_eq!(admire.min_friendship, -19.0);
        // (Children are left out: their clips are their own; the friendly done of their own
        // accord, the one asking a trait of the other not.)
        assert_eq!(admire.game.unwrap().actor_ages, [Age::Teen, Age::YoungAdult, Age::Adult]);
        assert!(admire.autonomous && !accuse.autonomous);
        // Asked of the one it's done to: their being evil.
        let mut a = crate::sim::random_sim(&mut rand::rng(), "A", Some(true), Age::Adult);
        let mut b = crate::sim::random_sim(&mut rand::rng(), "B", Some(false), Age::YoungAdult);
        a.traits.clear();
        b.traits = vec![Trait::Good];
        let rel = Relationship::default();
        assert!(!available(accuse, &rel, &a, &b, false, false, [None, None]));
        b.traits = vec![Trait::Evil];
        assert!(available(accuse, &rel, &a, &b, false, false, [None, None]));
        b.age = Age::Elder;
        assert!(!available(accuse, &rel, &a, &b, false, false, [None, None]), "not for the ages it isn't for");
        // Under a trait's topic: offered when the one with it does it; under a birthday's (not
        // known here), not at all.
        let mut plot = act("Mastermind Plot", "Friendly", "Admire", "");
        plot.topics = vec![("Evil Trait".into(), true)];
        let mut cake = act("Announce Birthday", "Friendly", "Admire", "");
        cake.topics = vec![("Birthday".into(), true)];
        let mut work = act("Ask About Work", "Friendly", "Admire", "");
        work.topics = vec![("Buff Fired".into(), false)];
        let got = game_socials(&[plot, cake, work], &clips);
        assert_eq!(got.len(), 2);
        b.age = Age::YoungAdult;
        assert!(!available(&got[0], &rel, &a, &b, false, false, [None, None]));
        a.traits = vec![Trait::Evil];
        assert!(available(&got[0], &rel, &a, &b, false, false, [None, None]));
        // Under a moodlet's topic: asked about work by others once fired.
        let mut fired = crate::life::Moodlets::default();
        assert!(!available(&got[1], &rel, &a, &b, false, false, [None, Some(&fired)]));
        fired.add(crate::life::MoodletKind::Fired, 0.0);
        assert!(available(&got[1], &rel, &a, &b, false, false, [None, Some(&fired)]));
        assert!(!available(&got[1], &rel, &a, &b, false, false, [Some(&fired), None]), "not the one fired asking");
    }

    #[test]
    fn friendships_fade_without_contact() {
        let mut app = App::new();
        app.insert_resource(crate::clock::GameClock::default()).init_resource::<crate::family::Genealogy>().add_systems(Update, fade_relationships);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(3);
        let a_sim = crate::sim::random_sim(&mut rng, "Test", Some(true), Age::Adult);
        let b_sim = crate::sim::random_sim(&mut rng, "Test", Some(false), Age::Adult);
        let b = app.world_mut().spawn((b_sim, Relationships::default())).id();
        let a = app.world_mut().spawn((a_sim, crate::sim::HouseholdMember, Relationships::default())).id();
        let start = 1440.0;
        for (me, other, status) in [(a, b, RelStatus::None), (b, a, RelStatus::None)] {
            let mut rels = app.world_mut().get_mut::<Relationships>(me).unwrap();
            *rels.entry(other) = Relationship { friendship: 50.0, romance: 20.0, status, kissed: false, last: start };
        }
        let mut run_hours = |app: &mut App, from: f64, hours: usize| {
            for h in 0..hours {
                app.world_mut().resource_mut::<crate::clock::GameClock>().minutes = from + h as f64 * 60.0;
                app.update();
            }
        };
        // Two days apart: as they were.
        run_hours(&mut app, start + 60.0, 48);
        assert_eq!(app.world().get::<Relationships>(a).unwrap().friendship(b), 50.0);
        // Five days apart: a day and a half of fading, both ways (friendship a point a day,
        // romance too).
        run_hours(&mut app, start + 3.5 * 1440.0, 48);
        for (me, other) in [(a, b), (b, a)] {
            let r = app.world().get::<Relationships>(me).unwrap().get(other);
            assert!(r.friendship < 50.0 && r.friendship > 47.5, "{}", r.friendship);
            assert!(r.romance < 20.0, "{}", r.romance);
        }
        // Spouses stay close.
        app.world_mut().get_mut::<Relationships>(a).unwrap().entry(b).status = RelStatus::Married;
        app.world_mut().get_mut::<Relationships>(a).unwrap().entry(b).friendship = 41.0;
        run_hours(&mut app, start + 10.0 * 1440.0, 24 * 10);
        assert_eq!(app.world().get::<Relationships>(a).unwrap().friendship(b), 40.0);
    }
}
