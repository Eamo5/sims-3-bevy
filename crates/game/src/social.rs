//! Relationships and social interactions: separate friendship and romance, relationship
//! statuses (going steady, engaged, married), socials grouped like the game's pie menu, and
//! the chance a Sim accepts them.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::life::Trait;
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
    /// Their conversation's short-term context (the game's, one between the two).
    pub stc: crate::conversation::Stc,
    /// Sworn enemies (the game's `MakeEnemy` flag: enmity takes a declaration, not just dislike).
    pub enemy: bool,
    /// Romantically involved (the game's `Romantic` flag: a first kiss, a confessed attraction;
    /// cleared by "Let's Just Be Friends").
    pub romantic: bool,
    /// Feeling betrayed by them (the game's `Betrayed` buff, for this cheater): until when (game
    /// minutes) they have to make it up, and the liking it left.
    pub betrayed: Option<(f64, f32)>,
}

impl Relationship {
    /// A romance between them (the game's romantic relationship types: a romantic interest,
    /// going steady, engaged, married; not exes).
    pub fn is_romance(&self) -> bool {
        matches!(self.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married) || (self.romantic && self.status != RelStatus::Ex)
    }
}

/// Days without seeing or speaking to each other before a relationship starts to fade.
pub const FADE_AFTER_DAYS: f64 = 3.0;
/// The liking that makes Sims friends (the game's `Friend` row; Good Friends at 60, Best at 80).
pub const FRIEND_THRESHOLD: f32 = 40.0;

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

/// Relationships fade as the game's do (its `Relationship.DecayAll`, each midnight): liking above
/// nothing drifts down and below it back up, by their conversation's context (2 a day in a good
/// one; a grudge softens by 3, or 1 while it's still sour), and the context itself cools.
/// Romance, ours, cools unless they're together, after a few days apart. The household's
/// relationships, both ways (the town's Sims among themselves are left as they are).
pub fn fade_relationships(
    clock: Res<crate::clock::GameClock>,
    (mut last_hour, mut last_day): (Local<i64>, Local<Option<u32>>),
    mut q: Query<(Entity, &crate::sim::Sim, &mut Relationships, Has<crate::sim::HouseholdMember>, Option<&crate::wishes::Wishes>, Option<&crate::journal::SkillJournal>)>,
    family: Res<crate::family::Genealogy>,
) {
    let hour = (clock.minutes / 60.0) as i64;
    if hour == *last_hour {
        return;
    }
    *last_hour = hour;
    let day = clock.day();
    let midnight = last_day.is_some_and(|d| d != day);
    *last_day = Some(day);
    let household: std::collections::HashSet<Entity> = q.iter().filter(|x| x.3).map(|x| x.0).collect();
    let ids: HashMap<Entity, u64> = q.iter().map(|x| (x.0, x.1.id)).collect();
    for (_, sim, mut rels, member, wishes, journal) in &mut q {
        // (A Long Distance Friend's friends never drift, nor a Super Friendly Sim's.)
        let steadfast = crate::wishes::has(wishes, "LongDistanceFriend") || crate::journal::earned(journal, "Super Friendly");
        for (other, r) in rels.0.iter_mut() {
            if r.last <= 0.0 {
                r.last = clock.minutes;
                continue;
            }
            if !member && !household.contains(other) {
                continue;
            }
            if midnight {
                if !steadfast {
                    r.friendship = daily_liking(r.friendship, &r.stc);
                }
                r.stc.decay();
            }
            let together = matches!(r.status, RelStatus::Married | RelStatus::Engaged | RelStatus::Partner);
            if steadfast || together || (clock.minutes - r.last) / 1440.0 < FADE_AFTER_DAYS {
                continue;
            }
            let kin = ids.get(other).is_some_and(|o| family.kin(sim.id, *o).is_some());
            let step = if kin { 0.25 } else { 1.0 } / 24.0;
            if r.romance > 0.0 {
                r.romance = (r.romance - step).max(0.0);
            }
        }
    }
}

/// A day's drift in liking (the game's `DecayAll`): above nothing by the context's
/// `LTRDecayFromAbove`, never below it; below by its `LTRDecayFromBelow`, never above it.
pub fn daily_liking(liking: f32, stc: &crate::conversation::Stc) -> f32 {
    // (An OK context's, -2 and +3, before the game's table is in.)
    let (above, below) = stc.row().map_or((-2.0, 3.0), |r| (r.decay_above, r.decay_below));
    if liking > 0.0 {
        (liking + above).max(0.0)
    } else if liking < 0.0 {
        (liking + below).min(0.0)
    } else {
        liking
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

    /// Their relationship's state by the game's name ("Friend", "Romantic Interest", "Spouse").
    pub fn state(&self) -> &'static str {
        crate::conversation::relationship_state(self.friendship, self.status, self.romantic, self.enemy)
    }

    /// What they are to one another by liking (the game's relationship states: Friends at 40,
    /// Good Friends at 60, Best at 80, Disliked from -20; Enemies only sworn ones).
    pub fn friendship_label(&self) -> &'static str {
        match self.friendship {
            v if self.enemy && v <= -1.0 => "Enemy",
            v if v <= -20.0 => "Disliked",
            v if v < FRIEND_THRESHOLD => "Acquaintance",
            v if v < 60.0 => "Friend",
            v if v < 80.0 => "Good Friend",
            _ => "Best Friend",
        }
    }

    pub fn label(&self) -> String {
        let friend = self.friendship_label();
        match self.status {
            RelStatus::Married => "Spouse".into(),
            RelStatus::Engaged => "Fiancé(e)".into(),
            RelStatus::Partner => "Partner".into(),
            RelStatus::Ex => format!("Ex · {friend}"),
            RelStatus::None if self.romantic && self.friendship >= 1.0 => format!("{friend} · Romantic Interest"),
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
    /// How many romances the Sim has going (the game's active romances).
    pub fn romances(&self) -> usize {
        self.0.values().filter(|r| r.is_romance()).count()
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
    /// Telling a child or teen to do their homework (the game's "Request Do Your Homework").
    RequestHomework,
    /// A Mooch's way with others' money and food (the game's "Mooch Money", "Mooch Food").
    MoochSmall,
    MoochLarge,
    MoochFood,
    /// A recipe learned from another cook (the game's "Ask for Recipe": one from the books, at
    /// the asker's Cooking).
    LearnRecipe,
    /// Asking the boss for a promotion or a raise (the game's `JobPromotion`, `PayRise`), done
    /// when it's taken well.
    AskPromotion,
    AskRaise,
    /// Having it out with a cheating romance, or owning up to cheating (the game's
    /// `OnAccuseOfCheating`, `OnConfessCheating`): `jealousy::CheatedOn`.
    Accuse,
    Confess,
}

/// What a Mooch gets out of others (the game's `MoochAmount`, at the hidden Mooch skill's first
/// level: a small sum, and a large one two levels up), as often doubled (a half) or quadrupled
/// (a quarter), give or take three tenths.
pub const MOOCH_AMOUNTS: (i64, i64) = (2, 10);

/// How much a child or teen must like someone not of their household to do their homework when
/// told (the game's `LLTR min`).
pub const HOMEWORK_LIKING: f32 = 25.0;

/// The game's socials that do something of their own here, by their key.
const GAME_EFFECTS: &[(&str, SocialEffect)] = &[
    ("Request Do Your Homework", SocialEffect::RequestHomework),
    ("Mooch Money (Small)", SocialEffect::MoochSmall),
    ("Mooch Money (Large)", SocialEffect::MoochLarge),
    ("Mooch Food", SocialEffect::MoochFood),
    ("Ask for Recipe", SocialEffect::LearnRecipe),
    ("Trade Kitchen Secrets", SocialEffect::LearnRecipe),
    ("Ask for a Promotion", SocialEffect::AskPromotion),
    ("Ask for a Raise", SocialEffect::AskRaise),
    ("Accuse of Cheating", SocialEffect::Accuse),
    ("Confess Cheating", SocialEffect::Confess),
];

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
/// and have it done to them, the traits wanted in the one it's done to (each of them, unless the
/// one doing it has it too) and in the one who does it (one of them), the traits that rule it
/// out (or out of a Sim's own accord), those that make a Sim keener on it, the game's test it
/// passes, and its animations (the clips' names without their side, `_x` / `_y`).
pub struct GameRules {
    /// Its key in the game's tables (`Accuse of Being Evil`).
    pub key: &'static str,
    pub actor_ages: Vec<Age>,
    pub target_ages: Vec<Age>,
    pub target_traits: Vec<Trait>,
    pub required: Vec<Trait>,
    pub keen: Vec<Trait>,
    pub restrict: Vec<Trait>,
    pub restrict_auto: Vec<Trait>,
    pub test: Option<GameTest>,
    /// Whether Sims do it unasked (the game's `Available auto`, and not `udo`), under its topics.
    pub auto: bool,
    pub clips: &'static [&'static str],
    /// The topics it's offered under (the game's `<Trait> Trait` and `Buff <Moodlet>` topics,
    /// and a few more): offered when the one who does it has the trait or the moodlet (`true`),
    /// or the one it's done to (`false`). None: always.
    pub topics: Vec<(Topic, bool)>,
}

/// A topic a social's offered under: a Sim's trait, or a moodlet of theirs; one that's always
/// there, between those of a household, a grown-up and a child, those in the same line of work;
/// or something that's happened to them lately (the game's event topics, `topics::Topics`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Topic {
    Trait(Trait),
    Moodlet(crate::life::MoodletKind),
    Always,
    SameHousehold,
    AdultToChild,
    SameWork,
    /// The greetings of a Sim with that much Charisma (the game's "Greet Charisma Level N").
    Charisma(u32),
    /// The line of work the other's in (the game's "Learn About ... Career": what's said of it
    /// once known), by a word of its name.
    Career(&'static str),
    Event(&'static str),
}

/// The game's tests of its socials that are made here (its `ProcTest`s): the one it's done to
/// is the one the topic's about (thanks for cooking...), the one doing it has a job, both have.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GameTest {
    About,
    ActorWorks,
    BothWork,
    TargetWorks,
    /// Not to the one they're with (announcing an engagement).
    NotPartner,
    /// A guest to their host (complimenting the home).
    GuestToHost,
    /// To the one doing it's boss (asking for a raise); and below level 9 (for a promotion).
    Boss,
    BossBelowNine,
    /// The game's `CommonCheatingTest`: romantically involved, the cheater with another romance
    /// besides, the other not betrayed by them already. Accusing: the one it's said to cheats;
    /// confessing: the one saying it.
    Accuse,
    Confess,
}

/// What's known of a Sim for the game's socials beyond their `Sim`: who they are, their
/// moodlets, their topics and their job (its career).
#[derive(Clone, Copy, Default)]
pub struct Side<'a> {
    pub sim: Option<Entity>,
    pub moods: Option<&'a crate::life::Moodlets>,
    pub topics: Option<&'a crate::topics::Topics>,
    pub career: Option<usize>,
    /// Whether they've homework to do.
    pub homework: bool,
    /// Whether they're of the household.
    pub member: bool,
    /// Their Charisma skill.
    pub charisma: u32,
    /// Whether they cook at all (have the Cooking skill).
    pub cooks: bool,
    /// Their boss (by id), their level (0-based) and performance at work.
    pub boss: Option<u64>,
    pub level: usize,
    pub performance: f32,
    /// Their relationships.
    pub rels: Option<&'a Relationships>,
}

/// What `Side::of` reads of a Sim.
pub type SideQuery<'a> = (
    Option<&'a crate::life::Moodlets>,
    Option<&'a crate::topics::Topics>,
    Option<&'a crate::careers::Job>,
    Has<crate::rabbitholes::Homework>,
    Has<crate::sim::HouseholdMember>,
    Option<&'a crate::interact::Skills>,
    Option<&'a Relationships>,
);

impl<'a> Side<'a> {
    #[allow(clippy::type_complexity)]
    pub fn of(
        sim: Entity,
        q: Option<(
            Option<&'a crate::life::Moodlets>,
            Option<&'a crate::topics::Topics>,
            Option<&'a crate::careers::Job>,
            bool,
            bool,
            Option<&'a crate::interact::Skills>,
            Option<&'a Relationships>,
        )>,
    ) -> Self {
        let (moods, topics, job, homework, member, skills, rels) = q.unwrap_or_default();
        Self {
            sim: Some(sim),
            moods,
            topics,
            career: job.map(|j| j.track),
            homework,
            member,
            charisma: skills.map_or(0, |s| s.level("Charisma")),
            cooks: skills.is_some_and(|s| s.0.get("Cooking").is_some_and(|v| *v > 0.0)),
            boss: job.and_then(|j| j.boss),
            level: job.map_or(0, |j| j.level),
            performance: job.map_or(0.0, |j| j.performance),
            rels,
        }
    }
}

/// The game's topic, if it's one kept here: `<Trait> Trait`, `Buff <Moodlet>` (by the game's
/// buff name), the pregnancy, a new job and a new home, those always there or between Sims of a
/// kind, and the rest by name (offered once a Sim has one, `topics::Topics`).
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
        "Pregnancy" | "Pregnant" => Some(Topic::Moodlet(M::Pregnant)),
        "New Job" => Some(Topic::Moodlet(M::NewJob)),
        "New House" => Some(Topic::Moodlet(M::NewHome)),
        "Always On" => Some(Topic::Always),
        "Same Household" => Some(Topic::SameHousehold),
        "Adult to Child" => Some(Topic::AdultToChild),
        "Work General" => Some(Topic::SameWork),
        "" => None,
        g if g.starts_with("Greet Charisma Level ") => g["Greet Charisma Level ".len()..].trim().parse().ok().map(Topic::Charisma),
        g if g.starts_with("Learn ") && g.ends_with(" Career") => {
            let w = g["Learn ".len()..g.len() - " Career".len()].trim_start_matches("About ").replace(' ', "").to_ascii_lowercase();
            Some(Topic::Career(Box::leak(w.into_boxed_str())))
        }
        other => Some(Topic::Event(Box::leak(other.to_string().into_boxed_str()))),
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
const PASSED_TESTS: &[&str] = &["TestChat", "TalkAboutWeatherTest", "ShareInterestsProceduralTest", "TestFriendlyHug", "TestKissOnCheek", "TestAskAboutRecentAction", "TestComplimentParty", "TestWoo", "TestComplainAboutDirtyDishes", "TestCharismaGreets"];

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
        "C" => Some(Age::Child),
        "T" => Some(Age::Teen),
        "Y" => Some(Age::YoungAdult),
        "A" => Some(Age::Adult),
        "E" => Some(Age::Elder),
        _ => None,
    }
}

/// The game's own socials, from its social data: each one not hand-made already (by name), with
/// a test that's known or none, for the ages it's for (a child plays the children's clips:
/// `interact::child_social_clips`), its tone
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
        let effect = GAME_EFFECTS.iter().find(|g| g.0 == a.key).map_or(SocialEffect::None, |g| g.1);
        let test = match a.test.as_str() {
            "" => None,
            t if PASSED_TESTS.contains(&t) => None,
            // (Its own effect's test: `available`.)
            "TestRequestDoYourHomework" if effect == SocialEffect::RequestHomework => None,
            "TestMoochMoney" | "TestMoochFood" | "TestAskForRecipe" => None,
            "TestThankForCooking" | "TestWorryAboutGrades" | "TestThankForTutoring" => Some(GameTest::About),
            "TestActorHasWork" => Some(GameTest::ActorWorks),
            "TestActorAndTargetHasWork" => Some(GameTest::BothWork),
            "TestComplimentHome" => Some(GameTest::GuestToHost),
            "TestTargetHasWork" => Some(GameTest::TargetWorks),
            "TestAnnouncePromotion" => Some(GameTest::ActorWorks),
            "TestAnnounceEngagement" => Some(GameTest::NotPartner),
            "TestAskForRaise" => Some(GameTest::Boss),
            "TestAskForPromotion" => Some(GameTest::BossBelowNine),
            "TestAccuseOfCheating" => Some(GameTest::Accuse),
            "TestConfessCheating" => Some(GameTest::Confess),
            // (What's said of a line of work: its topic is the other's being in it.)
            t if t.starts_with("TargetHas") && t.ends_with("Career") && a.topics.iter().any(|(n, _)| n.starts_with("Learn ")) => None,
            _ => continue,
        };
        if SOCIALS.iter().chain(out.iter()).any(|s| s.name.eq_ignore_ascii_case(&a.name)) || LEFT_OUT.contains(&a.name.as_str()) || a.name.contains('{') {
            continue;
        }
        // (Offered under topics; under a trait or a moodlet not known here, left out.)
        let topics: Vec<(Topic, bool)> = a.topics.iter().filter_map(|(t, first)| Some((topic(t)?, *first))).collect();
        if !a.topics.is_empty() && topics.is_empty() {
            continue;
        }
        // (Asking traits not known here, left out.)
        let (target_traits, required) = (traits(&a.target_traits), traits(&a.required));
        if target_traits.len() < a.target_traits.len() || (required.is_empty() && !a.required.is_empty()) {
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
            key: leak(&a.key),
            actor_ages,
            target_ages,
            target_traits,
            required,
            keen: traits(&a.keen),
            restrict: traits(&a.restrict),
            restrict_auto: traits(&a.restrict_auto),
            test,
            auto: a.autonomous && !a.user_only,
            clips: Box::leak(clips.into_boxed_slice()),
            topics,
        }));
        // (Romance only where there's some already; asked for or not, the friendly and the funny
        // are what Sims get up to of their own accord.)
        let min_romance = if cat == SocialCat::Romantic { 10.0 } else { -100.0 };
        let autonomous = a.autonomous && !a.user_only && matches!(cat, SocialCat::Friendly | SocialCat::Funny) && rules.target_traits.is_empty() && rules.test.is_none() && rules.topics.iter().all(|(t, _)| *t == Topic::Always);
        out.push(SocialDef {
            min_friendship,
            max_friendship,
            min_romance,
            autonomous,
            game: Some(rules),
            effect,
            ..sd(leak(&a.name), cat, minutes, social, fun, friendship, romance)
        });
    }
    out
}

/// A social to strike up of their own accord (the game's autonomous choice, its
/// `SocialComponent.SocialScore`): among those offered them as things stand, as keen on each as
/// its tone suits the one they want, a social their traits favour a little more; none in a tone
/// their traits keep them from, nor romance with someone they've none with.
pub fn pick_autonomous(actor: &Sim, target_age: Age, rel: &Relationship, rng: &mut impl rand::Rng) -> Option<usize> {
    use crate::conversation::{self as conv, Commodity};
    use rand::seq::IndexedRandom;
    let state = rel.state();
    let desired = conv::desired_tone(actor, rel.stc, state, rng);
    let (_, avoid) = conv::trait_tones(&actor.traits);
    let romantic = rel.romantic || matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married);
    let fits: Vec<(usize, f32)> = all_socials()
        .filter_map(|(i, s)| {
            if !s.autonomous || s.cat == SocialCat::Care || s.needs.is_some() {
                return None;
            }
            if rel.friendship < s.min_friendship || rel.friendship > s.max_friendship || (!conv::governed(s) && rel.romance < s.min_romance) {
                return None;
            }
            if let Some(g) = s.game
                && (!g.actor_ages.contains(&actor.age) || !g.target_ages.contains(&target_age) || !actor_may(g, actor, true) || !g.topics.is_empty())
            {
                return None;
            }
            let intended = conv::intended(i);
            if avoid.contains(&intended) || (intended == Commodity::Amorous && !romantic) || !conv::offered(s, rel.stc, state) {
                return None;
            }
            let keen = s.game.is_some_and(|g| g.keen.iter().any(|t| actor.traits.contains(t)));
            Some((i, conv::tone_weight(desired, intended) * if keen { 1.2 } else { 1.0 }))
        })
        .collect();
    fits.choose_weighted(rng, |f| f.1).ok().map(|f| f.0)
}

/// Whether a Sim's traits let them do one of the game's socials (of their own accord or not):
/// one of those it asks for, none of those that rule it out.
pub fn actor_may(g: &GameRules, actor: &Sim, autonomous: bool) -> bool {
    let has = |l: &[Trait]| l.iter().any(|t| actor.traits.contains(t));
    (g.required.is_empty() || has(&g.required)) && !has(&g.restrict) && !(autonomous && has(&g.restrict_auto))
}

/// Whether a topic a social's offered under is there between `actor` and `target` (`first`:
/// the actor's own; else the target's). An event topic once used up between the two (the game's
/// `UsedUpAfterDoneOnce`) isn't.
pub fn topic_holds(t: Topic, first: bool, actor: &Sim, target: &Sim, target_in_household: bool, side: [Side; 2], test: Option<GameTest>) -> bool {
    let (sim, me, other) = if first { (actor, side[0], side[1]) } else { (target, side[1], side[0]) };
    let grown = |s: &Sim| s.age.is_grown();
    match t {
        Topic::Trait(x) => sim.traits.contains(&x),
        Topic::Moodlet(m) => me.moods.is_some_and(|ml| ml.has(m)),
        Topic::Always => true,
        Topic::SameHousehold => target_in_household,
        Topic::AdultToChild => (grown(actor) && !grown(target)) == first && grown(actor) != grown(target),
        Topic::SameWork => side[0].career.is_some() && side[0].career == side[1].career,
        Topic::Charisma(level) => first && side[0].charisma == level,
        Topic::Career(word) => side[1].career.is_some_and(|c| career_is(c, word)),
        Topic::Event(name) => me.topics.and_then(|t| t.get(name)).is_some_and(|h| {
            !other.sim.is_some_and(|o| h.used_with.contains(&o)) && (test != Some(GameTest::About) || (h.about.is_some() && h.about == other.sim))
        }),
    }
}

/// Whether a career (its track) goes by a word of the game's topics for it ("lawenforcement",
/// "musical", "sports").
fn career_is(track: usize, word: &str) -> bool {
    let Some(t) = crate::careers::careers().get(track) else { return false };
    let name = t.name.replace(' ', "").to_ascii_lowercase();
    name.contains(word) || word.contains(&name) || (word.len() >= 5 && name.starts_with(&word[..5]))
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
    install_game_socials(&ui.data.socials, &data.0.clip_names);
    crate::conversation::load(&ui.data);
}

/// The game's own socials, from its social data and the clips there are, for this session.
pub fn install_game_socials(actions: &[s3bake::gamedata::SocialAction], clip_names: &[String]) {
    let list = game_socials(actions, clip_names);
    info!("socials: {} of the game's own besides the {} hand-made", list.len(), SOCIALS.len());
    let _ = GAME_SOCIALS.set(list);
}

/// Whether `actor` can start this social with `target` (shown in the pie menu); `kin`: whether
/// they're family (no romance between relatives).
pub fn available(def: &SocialDef, rel: &Relationship, actor: &Sim, target: &Sim, target_in_household: bool, kin: bool, side: [Side; 2]) -> bool {
    // (Romance as far along as it asks, where the game's menus don't say: theirs go by the
    // conversation and the relationship.)
    let governed = crate::conversation::governed(def);
    if rel.friendship < def.min_friendship || (!governed && rel.romance < def.min_romance) || rel.friendship > def.max_friendship {
        return false;
    }
    // (What the game offers in their conversation, as things stand between them.)
    if !crate::conversation::offered(def, rel.stc, rel.state()) {
        return false;
    }
    // (The game's own: the ages it's for, the traits it asks of the one it's done to and of the
    // one doing it, and its test.)
    if let Some(g) = def.game
        && (!g.actor_ages.contains(&actor.age)
            || !g.target_ages.contains(&target.age)
            || !g.target_traits.iter().all(|t| actor.traits.contains(t) || target.traits.contains(t))
            || !actor_may(g, actor, false)
            || match g.test {
                Some(GameTest::ActorWorks) => side[0].career.is_none(),
                Some(GameTest::BothWork) => side[0].career.is_none() || side[1].career.is_none(),
                Some(GameTest::GuestToHost) => side[0].member || !side[1].member,
                Some(GameTest::TargetWorks) => side[1].career.is_none(),
                Some(GameTest::NotPartner) => matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married),
                Some(GameTest::Boss) => side[0].boss != Some(target.id),
                Some(GameTest::BossBelowNine) => side[0].boss != Some(target.id) || side[0].level + 1 >= 9,
                Some(GameTest::Accuse) => !rel.is_romance() || side[1].rels.is_none_or(|r| r.romances() < 2) || rel.betrayed.is_some(),
                Some(GameTest::Confess) => {
                    !rel.is_romance()
                        || side[0].rels.is_none_or(|r| r.romances() < 2)
                        || side[1].rels.zip(side[0].sim).is_some_and(|(r, me)| r.get(me).betrayed.is_some())
                }
                _ => false,
            })
    {
        return false;
    }
    // (Under a topic: the one with the trait, the moodlet, what's happened to them...)
    if let Some(g) = def.game
        && !g.topics.is_empty()
        && !g.topics.iter().any(|(t, first)| topic_holds(*t, *first, actor, target, target_in_household, side, g.test))
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
        SocialEffect::RequestHomework => side[1].homework && matches!(target.age, Age::Child | Age::Teen),
        // (Money from those of another household.)
        SocialEffect::MoochSmall | SocialEffect::MoochLarge => !(side[0].member && side[1].member),
        // (A recipe from a cook of another household.)
        SocialEffect::LearnRecipe => !(side[0].member && side[1].member) && side[0].cooks && side[1].cooks,
        SocialEffect::WooHoo => matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married) || rel.romance >= 80.0,
        _ => true,
    }
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
            act("Ask for Directions", "Friendly", "Admire", "TestNotKnownHere"),
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
        // (The friendly done of their own accord, the one asking a trait of the other not.)
        assert_eq!(admire.game.unwrap().actor_ages, [Age::Child, Age::Teen, Age::YoungAdult, Age::Adult]);
        assert!(admire.autonomous && !accuse.autonomous);
        // Asked of the one it's done to: their being evil.
        let mut a = crate::sim::random_sim(&mut rand::rng(), "A", Some(true), Age::Adult);
        let mut b = crate::sim::random_sim(&mut rand::rng(), "B", Some(false), Age::YoungAdult);
        a.traits.clear();
        b.traits = vec![Trait::Good];
        let rel = Relationship::default();
        let none = [Side::default(), Side::default()];
        assert!(!available(accuse, &rel, &a, &b, false, false, none));
        b.traits = vec![Trait::Evil];
        assert!(available(accuse, &rel, &a, &b, false, false, none));
        b.age = Age::Elder;
        assert!(!available(accuse, &rel, &a, &b, false, false, none), "not for the ages it isn't for");
        // Under a trait's topic: offered when the one with it does it; under a birthday's, once
        // they've had one.
        let mut plot = act("Mastermind Plot", "Friendly", "Admire", "");
        plot.topics = vec![("Evil Trait".into(), true)];
        let mut cake = act("Announce Birthday", "Friendly", "Admire", "");
        cake.topics = vec![("Birthday".into(), true)];
        let mut work = act("Ask About Work", "Friendly", "Admire", "");
        work.topics = vec![("Buff Fired".into(), false)];
        let got = game_socials(&[plot, cake, work], &clips);
        assert_eq!(got.len(), 3);
        b.age = Age::YoungAdult;
        assert!(!available(&got[0], &rel, &a, &b, false, false, none));
        a.traits = vec![Trait::Evil];
        assert!(available(&got[0], &rel, &a, &b, false, false, none));
        // Under an event's topic: once the Sim has it, and not again with the same Sim once done.
        let (ae, be) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let mut topics = crate::topics::Topics::default();
        let had = |t: &crate::topics::Topics| available(&got[1], &rel, &a, &b, false, false, [Side { sim: Some(ae), topics: Some(t), ..default() }, Side { sim: Some(be), ..default() }]);
        assert!(!had(&topics));
        topics.add("Birthday", 100.0, None);
        assert!(had(&topics));
        topics.0[0].used_with.push(be);
        assert!(!had(&topics), "used up between them");
        // Under a moodlet's topic: asked about work by others once fired.
        let mut fired = crate::life::Moodlets::default();
        fn moods(m: [Option<&crate::life::Moodlets>; 2]) -> [Side<'_>; 2] {
            [Side { moods: m[0], ..default() }, Side { moods: m[1], ..default() }]
        }
        assert!(!available(&got[2], &rel, &a, &b, false, false, moods([None, Some(&fired)])));
        fired.add(crate::life::MoodletKind::Fired, 0.0);
        assert!(available(&got[2], &rel, &a, &b, false, false, moods([None, Some(&fired)])));
        assert!(!available(&got[2], &rel, &a, &b, false, false, moods([Some(&fired), None])), "not the one fired asking");
    }

    /// Why one of the game's socials is or isn't kept, from the baked game data (SOCIAL_WHY=<part
    /// of its name>): `... social_why -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn social_why() {
        let data: s3bake::GameDataBaked = s3bake::pack::read_value(&s3bake::default_root().global_dir().join("gamedata.bin")).unwrap();
        let want = std::env::var("SOCIAL_WHY").unwrap_or_default().to_ascii_lowercase();
        for a in data.socials.iter().filter(|a| a.name.to_ascii_lowercase().contains(&want) || a.key.to_ascii_lowercase().contains(&want)) {
            let kept = game_socials(std::slice::from_ref(a), &[]);
            println!(
                "{:?} (key {:?}) tone {} graph {} suite {:?} test {:?} topics {:?} required {:?} ages {:?}/{:?} kept-without-clips {}",
                a.name, a.key, a.tone, a.graph, a.suite, a.test, a.topics, a.required, a.actor_ages, a.target_ages, kept.len()
            );
        }
    }

    #[test]
    fn grown_ups_tell_teens_with_homework_to_do_it() {
        use s3bake::gamedata::SocialAction;
        let a = SocialAction {
            key: "Request Do Your Homework".into(),
            name: "Request Do Your Homework".into(),
            tone: "Neutral".into(),
            actor_ages: ["C", "T", "Y", "A", "E"].map(String::from).to_vec(),
            target_ages: ["C", "T"].map(String::from).to_vec(),
            autonomous: true,
            suite: "RequestAction".into(),
            test: "TestRequestDoYourHomework".into(),
            topics: vec![("Adult to Child".into(), true)],
            ..Default::default()
        };
        let clips: Vec<String> = ["a2a_soc_Neutral_RequestAction_Friendly_Neutral_x", "a2a_soc_Neutral_RequestAction_Friendly_Neutral_y"].map(String::from).to_vec();
        let got = game_socials(&[a], &clips);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].effect, SocialEffect::RequestHomework);
        let mom = crate::sim::random_sim(&mut rand::rng(), "Mom", Some(true), Age::Adult);
        // (Teens: children's clips for it are their own.)
        let kid = crate::sim::random_sim(&mut rand::rng(), "Kid", Some(false), Age::Teen);
        let rel = Relationship::default();
        let with = |homework| [Side::default(), Side { homework, ..default() }];
        assert!(!available(&got[0], &rel, &mom, &kid, true, true, with(false)), "no homework to do");
        assert!(available(&got[0], &rel, &mom, &kid, true, true, with(true)));
        assert!(!available(&got[0], &rel, &kid, &mom, true, true, with(true)), "not the child telling the grown-up");
    }

    #[test]
    fn the_games_socials_ask_traits_of_the_one_doing_them() {
        use s3bake::gamedata::SocialAction;
        let act = |name: &str, required: &[&str], restrict: &[&str]| SocialAction {
            key: name.into(),
            name: name.into(),
            tone: "Friendly".into(),
            actor_ages: vec!["A".into()],
            target_ages: vec!["A".into()],
            autonomous: true,
            suite: "Admire".into(),
            required: required.iter().map(|s| s.to_string()).collect(),
            restrict: restrict.iter().map(|s| s.to_string()).collect(),
            restrict_auto: vec!["Grumpy".into()],
            ..Default::default()
        };
        let clips: Vec<String> = ["a2a_soc_Neutral_Admire_Friendly_Neutral_x", "a2a_soc_Neutral_Admire_Friendly_Neutral_y"].map(String::from).to_vec();
        let got = game_socials(&[act("Brighten Day", &["Good"], &["Evil"]), act("Vampire Drink", &["VampireHiddenTrait"], &[])], &clips);
        assert_eq!(got.len(), 1, "asking a trait not known here, left out");
        let mut a = crate::sim::random_sim(&mut rand::rng(), "A", Some(true), Age::Adult);
        let b = crate::sim::random_sim(&mut rand::rng(), "B", Some(false), Age::Adult);
        let rel = Relationship::default();
        let none = [Side::default(), Side::default()];
        a.traits = vec![Trait::Friendly];
        assert!(!available(&got[0], &rel, &a, &b, false, false, none), "only the Good brighten a day");
        a.traits = vec![Trait::Good];
        assert!(available(&got[0], &rel, &a, &b, false, false, none));
        a.traits = vec![Trait::Good, Trait::Evil];
        assert!(!available(&got[0], &rel, &a, &b, false, false, none), "ruled out by a trait");
        a.traits = vec![Trait::Good, Trait::Grumpy];
        let g = got[0].game.unwrap();
        assert!(actor_may(g, &a, false) && !actor_may(g, &a, true), "the Grumpy only when asked");
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
            *rels.entry(other) = Relationship { friendship: 50.0, romance: 20.0, status, last: start, ..Default::default() };
        }
        let mut run_hours = |app: &mut App, from: f64, hours: usize| {
            for h in 0..hours {
                app.world_mut().resource_mut::<crate::clock::GameClock>().minutes = from + h as f64 * 60.0;
                app.update();
            }
        };
        // Two days on: liking two points less each midnight, as the game's; romance as it was.
        run_hours(&mut app, start + 60.0, 48);
        let r = app.world().get::<Relationships>(a).unwrap().get(b);
        assert_eq!((r.friendship, r.romance), (46.0, 20.0));
        // Days apart (three midnights more), romance cools too, both ways.
        run_hours(&mut app, start + 3.5 * 1440.0, 48);
        for (me, other) in [(a, b), (b, a)] {
            let r = app.world().get::<Relationships>(me).unwrap().get(other);
            assert_eq!(r.friendship, 40.0);
            assert!(r.romance < 20.0, "{}", r.romance);
        }
        // A grudge softens, three a day, to nothing.
        app.world_mut().get_mut::<Relationships>(a).unwrap().entry(b).friendship = -4.0;
        run_hours(&mut app, start + 6.0 * 1440.0, 48);
        assert_eq!(app.world().get::<Relationships>(a).unwrap().friendship(b), 0.0);
    }
}
