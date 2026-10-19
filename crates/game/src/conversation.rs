//! Conversations as the game has them (its `Conversation`, `ShortTermContext` and social rules):
//! each pair of Sims talking is in a short-term context, one commodity (Friendly, Funny, Amorous,
//! Boring, Insulting...) and how far it's gone (Ok, Friendly, Very Friendly; Flirty, Seductive,
//! Hot...). A social's outcome is decided by the game's rules, its own and the general ones for
//! what it's meant to be, the most specific that fits first: by the context, the relationship,
//! the target's mood, the traits and Charisma on either side, repetition. The outcome pushes the
//! context its way, and liking then moves by the context's own step (more while they barely
//! know each other, less as they grow close).

use std::collections::HashMap;
use std::sync::OnceLock;

use s3bake::gamedata::{SocialOutcome, SocialRule, StcInfo};

use crate::life::{MoodletKind, Trait};
use crate::sim::{Age, Sim};

/// The game's social commodities: what a social is meant as, and what it comes across as.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Commodity {
    Neutral,
    Boring,
    Creepy,
    Insulting,
    #[default]
    Friendly,
    Funny,
    Amorous,
    Impressive,
    Awkward,
    Steamed,
}

impl Commodity {
    pub const ALL: [Commodity; 10] = {
        use Commodity::*;
        [Neutral, Boring, Creepy, Insulting, Friendly, Funny, Amorous, Impressive, Awkward, Steamed]
    };

    pub fn name(self) -> &'static str {
        match self {
            Commodity::Neutral => "Neutral",
            Commodity::Boring => "Boring",
            Commodity::Creepy => "Creepy",
            Commodity::Insulting => "Insulting",
            Commodity::Friendly => "Friendly",
            Commodity::Funny => "Funny",
            Commodity::Amorous => "Amorous",
            Commodity::Impressive => "Impressive",
            Commodity::Awkward => "Awkward",
            Commodity::Steamed => "Steamed",
        }
    }

    pub fn from_name(s: &str) -> Option<Commodity> {
        Commodity::ALL.into_iter().find(|c| c.name().eq_ignore_ascii_case(s.trim()))
    }

    /// Whether it goes down well (the game's `CommodityDefinition.IsPositive`).
    pub fn positive(self) -> bool {
        matches!(self, Commodity::Neutral | Commodity::Friendly | Commodity::Funny | Commodity::Amorous | Commodity::Impressive)
    }
}

/// A pair's short-term context: the one commodity it's in, and how far (0 to 1000).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stc {
    pub commodity: Commodity,
    pub value: f32,
}

/// How far a context can go (`ShortTermContext.SetSTC`), and how much of it is left when a
/// conversation ends or a day passes (`kSTCDecayRateMultiplier`, under 100 back to nothing).
const STC_MAX: f32 = 1000.0;
const STC_DECAY: f32 = 0.6;

impl Stc {
    /// Its row in the game's table (Ok when nothing's going on).
    pub fn row(&self) -> Option<&'static StcInfo> {
        let stcs = &tables()?.stcs;
        let c = self.commodity.name();
        stcs.iter()
            .find(|s| s.commodity == c && s.min <= self.value && self.value < s.max)
            .or_else(|| stcs.iter().find(|s| s.commodity == "Friendly" && s.min <= 0.0))
    }

    /// The context's name, as the game shows it ("OK", "Very Friendly", "Flirty").
    pub fn name(&self) -> &'static str {
        self.row().map_or("OK", |r| r.name.as_str())
    }

    /// A social's effect on it (`ShortTermContext.Update`): a neutral one halves a good
    /// context; any other pushes its own commodity on, and the rest are forgotten.
    pub fn update(&mut self, c: Commodity, amount: f32) {
        if c == Commodity::Neutral {
            if self.commodity.positive() {
                self.value *= 0.5;
            }
            return;
        }
        let min = if c == Commodity::Friendly { 0.0 } else { 1.0 };
        let base = if self.commodity == c { self.value } else { 0.0 };
        self.commodity = c;
        self.value = (base + amount).clamp(min, STC_MAX);
    }

    /// When the conversation ends or the day turns (`DecayCurrentCommodityValue`).
    pub fn decay(&mut self) {
        self.value *= STC_DECAY;
        if self.value < 100.0 {
            *self = Stc { commodity: Commodity::Friendly, value: 1.0 };
        }
    }
}

/// How much liking one social changes, in a context, at a liking (the game's
/// `STCData.GetCommodityChange`: its `Y1` below `X2`, `Y2` below `X3`, else `Y3`).
pub fn liking_change(stc: &Stc, liking: f32) -> f32 {
    let Some(r) = stc.row() else { return 0.0 };
    if liking < r.x2 {
        r.y[0]
    } else if liking < r.x3 {
        r.y[1]
    } else {
        r.y[2]
    }
}

/// What a social is meant as and the rules its outcome goes by.
pub struct Model {
    pub intended: Commodity,
    pub speech_act: String,
    pub chances: u8,
    pub recharges: u8,
    /// How it strikes a partner who sees it (`jealousy::Romanced`; 0 not at all).
    pub jealousy: u8,
    /// Its own rules, most specific first.
    pub rules: Vec<SocialRule>,
    /// What each result does; none: anything the rules give goes.
    pub outcomes: Vec<SocialOutcome>,
}

struct Tables {
    general: HashMap<Commodity, Vec<SocialRule>>,
    stcs: Vec<StcInfo>,
    models: HashMap<usize, Model>,
    /// The socials offered in each context and relationship state (both written as `same`
    /// compares them), and those the menus govern at all.
    menus: HashMap<(String, String), std::collections::HashSet<String>>,
    in_menus: std::collections::HashSet<String>,
    /// What each social's meant as, by its key.
    intended: HashMap<String, Commodity>,
}

static TABLES: OnceLock<Tables> = OnceLock::new();

fn tables() -> Option<&'static Tables> {
    TABLES.get()
}

/// The game's own social a hand-made one is (where their names differ).
pub fn game_key(name: &str) -> &str {
    match name {
        "Hug" => "Friendly Hug",
        "Fight" => "Fight!",
        "Ask to Go Steady" => "Propose Going Steady",
        "Ask on Date" => "Lets Go On Date",
        "Say Goodbye" => "Goodbye",
        "Give Back Rub" => "Massage",
        "Try for Baby" => "Try For Baby",
        // (The game's "Get to Know" is its "Share Interests".)
        "Get to Know" => "Share Interests",
        n => n,
    }
}

/// What a hand-made social's kind is meant as.
fn intended_of(cat: crate::social::SocialCat) -> Commodity {
    use crate::social::SocialCat as C;
    match cat {
        C::Friendly => Commodity::Friendly,
        C::Funny => Commodity::Funny,
        C::Romantic => Commodity::Amorous,
        C::Mean => Commodity::Insulting,
        C::Special | C::Care => Commodity::Neutral,
    }
}

/// Sorted most specific first, the table's order among equals.
fn by_specificity(mut rules: Vec<SocialRule>) -> Vec<SocialRule> {
    rules.sort_by(|a, b| b.spec.total_cmp(&a.spec));
    rules
}

/// The game's rules and contexts, and each social's model (hand-made ones by their game
/// counterpart, else by their kind).
pub fn load(data: &s3bake::gamedata::GameDataBaked) {
    if TABLES.get().is_some() {
        return;
    }
    let mut general: HashMap<Commodity, Vec<SocialRule>> = HashMap::new();
    for r in &data.general_rules {
        if let Some(c) = Commodity::from_name(&r.commodity) {
            general.entry(c).or_default().push(r.clone());
        }
    }
    let general = general.into_iter().map(|(c, v)| (c, by_specificity(v))).collect();
    let actions: HashMap<String, &s3bake::gamedata::SocialAction> = data.socials.iter().map(|a| (a.key.to_ascii_lowercase(), a)).collect();
    let mut models = HashMap::new();
    let mut intended_by_key = HashMap::new();
    for (i, def) in crate::social::all_socials() {
        let key = def.game.map_or_else(|| game_key(def.name), |g| g.key);
        let action = actions.get(&key.to_ascii_lowercase()).copied();
        let intended = action.and_then(|a| Commodity::from_name(&a.tone)).unwrap_or_else(|| intended_of(def.cat));
        intended_by_key.insert(norm(key), intended);
        let speech_act = action.map(|a| a.speech_act.clone()).unwrap_or_else(|| if def.name == "Brag" { "Boast".into() } else { String::new() });
        models.insert(
            i,
            Model {
                intended,
                speech_act,
                chances: action.map_or(2, |a| a.chances.max(1)),
                recharges: action.map_or(2, |a| a.recharges),
                jealousy: action.map_or(0, |a| a.jealousy),
                rules: action.map(|a| by_specificity(a.rules.clone())).unwrap_or_default(),
                outcomes: action.map(|a| a.outcomes.clone()).unwrap_or_default(),
            },
        );
    }
    let mut menus: HashMap<(String, String), std::collections::HashSet<String>> = HashMap::new();
    let mut in_menus = std::collections::HashSet::new();
    for m in &data.social_menus {
        let set = menus.entry((norm(&m.stc), norm(&m.group))).or_default();
        for s in &m.socials {
            set.insert(norm(s));
            in_menus.insert(norm(s));
        }
    }
    bevy::log::info!("conversations: {} general rules, {} contexts, {} socials with their own rules, {} in the menus", data.general_rules.len(), data.stcs.len(), models.values().filter(|m| !m.rules.is_empty()).count(), in_menus.len());
    let _ = TABLES.set(Tables { general, stcs: data.stcs.clone(), models, menus, in_menus, intended: intended_by_key });
}

pub fn model(social: usize) -> Option<&'static Model> {
    tables()?.models.get(&social)
}

/// The kinds of effectiveness traits and moodlets lend (the game's `SocialComponent`
/// increased/reduced effectiveness, from its `Traits` and `Buffs` tables).
fn effectiveness(traits: &[Trait], moodlets: &[MoodletKind]) -> (Vec<&'static str>, Vec<&'static str>) {
    let (mut up, mut down) = (Vec::new(), Vec::new());
    for t in traits {
        match t {
            Trait::Charismatic => up.extend(["Impressive", "Talk"]),
            Trait::EasilyImpressed => up.push("AcceptingOfImpressive"),
            Trait::Flirty => up.extend(["Romance", "AcceptingOfRomance"]),
            Trait::Friendly => up.extend(["Social", "AcceptingOfSocials", "AcceptingOfTalk"]),
            Trait::GoodSenseOfHumor => up.extend(["Humor", "AcceptingOfHumor"]),
            Trait::HopelessRomantic => up.push("Romance"),
            Trait::Schmoozer => up.extend(["Talk", "AcceptingOfSocials", "AcceptingOfTalk"]),
            Trait::Grumpy => down.extend(["Social", "AcceptingOfTalk"]),
            Trait::Loner => down.push("AcceptingOfSocials"),
            Trait::Loser => down.push("Impressive"),
            Trait::NoSenseOfHumor => down.extend(["Humor", "AcceptingOfHumor"]),
            Trait::Snob => down.push("AcceptingOfImpressive"),
            Trait::Unflirty => down.extend(["Romance", "AcceptingOfRomance"]),
            _ => {}
        }
    }
    for m in moodlets {
        match m {
            // (Gussied up at a mirror: the game's Attractive buff, romance the better for it.)
            MoodletKind::MintyBreath | MoodletKind::Attractive => up.push("Romance"),
            MoodletKind::Heartbroken => up.push("AcceptingOfRomance"),
            MoodletKind::Smelly => down.extend(["Social", "Romance"]),
            _ => {}
        }
    }
    (up, down)
}

/// What the rules weigh, as the game adjusts it for them.
pub struct Judging<'a> {
    pub actor: &'a Sim,
    pub target: &'a Sim,
    pub actor_moodlets: &'a [MoodletKind],
    pub target_moodlets: &'a [MoodletKind],
    /// Liking, and the relationship's state by the game's name ("Friend", "Enemy").
    pub liking: f32,
    pub relationship: &'static str,
    pub stc: Stc,
    pub target_mood: f32,
    /// The actor's Charisma (`None`: never learned).
    pub charisma: Option<u32>,
    /// The target has a partner, and it isn't the actor; the actor is the target's partner.
    pub partner_elsewhere: bool,
    pub partners: bool,
    /// Family (the Family-Oriented take kindly to kin).
    pub kin: bool,
    /// The actor's done this social too often running (out of chances).
    pub repetition: bool,
    /// The household's, and the target's in it (for the game's tests that ask).
    pub actor_member: bool,
    pub target_member: bool,
    /// The holiday warmth (the game's +20 liking and mood on Snowflake Day, and for romance on
    /// Love Day).
    pub holiday: bool,
    /// The actor's performance at work, when it's their boss they're talking to.
    pub boss_hears: Option<f32>,
}

/// What a social came to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub result: Commodity,
    /// How far it pushes the context.
    pub amount: f32,
    /// A flat liking change instead of the context's (the game's `LTROverride`).
    pub ltr_override: f32,
    pub bits_add: u32,
    pub bits_remove: u32,
    /// The Charisma points it's worth taken well (-1: the context's).
    pub charisma: f32,
    /// Whether it went down as meant (the game's `IsSocialAccepted`).
    pub accepted: bool,
    /// The topics it raises: the actor's, the target's, and both of theirs ("" for none).
    pub topics: [&'static str; 3],
    /// A trait of the target's the way it went turned on, learned by the actor (the game's
    /// `ConsiderLearningAboutTargetSim`: the rule's target trait, or the trait behind the
    /// target's effectiveness it asked).
    pub learned: Option<Trait>,
    /// Whether it may teach the actor one of the target's traits besides (the game's
    /// `LearnRandomTrait`).
    pub learn_random: bool,
    /// The traits it teaches if they've them: the actor's (to the target) and the target's (to
    /// the actor), comma lists of the game's names.
    pub teaches: [&'static str; 2],
}

/// The Charisma points a social taken well earns its Sim, once they've the skill (the game's
/// `UpdateSimBeforeInteraction`: its outcome's own, else the context's `CharismaIncrease`: 100 in
/// an OK one up to 200 in a very friendly or hilarious one).
pub fn charisma_points(o: &Outcome, stc: &Stc) -> f32 {
    if !o.accepted {
        return 0.0;
    }
    if o.charisma >= 0.0 { o.charisma } else { stc.row().map_or(0.0, |r| r.charisma) }
}

/// The game's `IsSocialAccepted`: a good social taken neutrally isn't taken well; a mean one
/// always lands.
pub fn accepted(intended: Commodity, result: Commodity) -> bool {
    if intended == Commodity::Neutral {
        result.positive()
    } else if intended.positive() {
        result != Commodity::Neutral && result.positive()
    } else {
        true
    }
}

/// Whether a social meant well went down badly (Boring, Awkward, Creepy, Insulting...): it's cut
/// short, a rebuff. A neutral reply plays out (the game's "accept" for it), though it isn't
/// counted a success.
pub fn rebuffed(social: usize, o: &Outcome) -> bool {
    let intended = model(social).map_or(Commodity::Friendly, |m| m.intended);
    intended.positive() && !o.result.positive() && !o.accepted
}

/// How keen a Sim wanting one tone is on a social of another (the game's `CommodityTable`:
/// rows what's wanted, columns what the social's meant as).
pub fn tone_weight(desired: Commodity, intended: Commodity) -> f32 {
    use Commodity::*;
    let order = [Neutral, Boring, Creepy, Insulting, Friendly, Funny, Amorous, Awkward, Steamed];
    const TABLE: [[f32; 9]; 9] = [
        [1.0, 0.1, 0.05, 0.05, 0.1, 0.5, 0.05, 0.3, 0.05],
        [0.2, 1.0, 0.2, 0.6, 0.08, 0.08, 0.01, 0.8, 0.05],
        [0.05, 0.3, 1.0, 0.6, 0.01, 0.01, 0.01, 0.7, 0.05],
        [0.01, 0.01, 0.01, 1.0, 0.01, 0.01, 0.01, 0.01, 0.3],
        [0.1, 0.1, 0.05, 0.01, 1.0, 0.6, 0.1, 0.01, 0.05],
        [0.1, 0.05, 0.05, 0.01, 0.1, 1.0, 0.1, 0.01, 0.05],
        [0.05, 0.05, 0.01, 0.05, 0.2, 0.3, 1.0, 0.01, 0.05],
        [0.3, 0.8, 0.7, 0.5, 0.01, 0.01, 0.01, 1.0, 0.05],
        [0.01, 0.01, 0.01, 0.3, 0.01, 0.01, 0.01, 0.01, 1.0],
    ];
    let at = |c: Commodity| order.iter().position(|o| *o == c);
    match (at(desired), at(intended)) {
        (Some(d), Some(i)) => TABLE[d][i],
        _ => 0.05,
    }
}

/// The tones a trait wants a Sim to strike of their own accord, and those it keeps them from
/// (the game's `Traits` table, `AutonomousDesiredCommodity` and `AutonomousAvoidedCommodity`).
pub fn trait_tones(traits: &[Trait]) -> (Vec<Commodity>, Vec<Commodity>) {
    let (mut want, mut avoid) = (Vec::new(), Vec::new());
    for t in traits {
        match t {
            Trait::Flirty => want.push(Commodity::Amorous),
            Trait::Friendly => {
                want.push(Commodity::Friendly);
                avoid.push(Commodity::Insulting);
            }
            Trait::Good => avoid.push(Commodity::Insulting),
            Trait::GoodSenseOfHumor => want.push(Commodity::Funny),
            Trait::MeanSpirited => want.push(Commodity::Insulting),
            Trait::NoSenseOfHumor => {
                want.push(Commodity::Boring);
                avoid.push(Commodity::Funny);
            }
            Trait::Unflirty => avoid.push(Commodity::Amorous),
            _ => {}
        }
    }
    (want, avoid)
}

/// The tone a Sim wants to strike with someone (the game's `CalculateDesiredSocialCommodity`):
/// romance once things are seductive; else one of what their traits and their relationship
/// favour (friends Friendly, sweethearts Amorous or Friendly, foes Insulting); Friendly
/// otherwise.
pub fn desired_tone(actor: &Sim, stc: Stc, relationship: &str, rng: &mut impl rand::Rng) -> Commodity {
    use rand::seq::IndexedRandom;
    if stc.commodity == Commodity::Amorous && stc.value >= 150.0 {
        return Commodity::Amorous;
    }
    let mut options = trait_tones(&actor.traits).0;
    match relationship {
        "Disliked" | "Enemy" | "Old Enemies" => options.push(Commodity::Insulting),
        "Friend" | "Good Friend" | "Best Friend" | "Old Friend" | "Best Friends Forever" => options.push(Commodity::Friendly),
        "Romantic Interest" | "Partner" | "Fiancee" | "Spouse" => options.extend([Commodity::Amorous, Commodity::Friendly]),
        _ => {}
    }
    options.choose(rng).copied().unwrap_or(Commodity::Friendly)
}

/// What a social's meant as.
pub fn intended(social: usize) -> Commodity {
    model(social).map_or(Commodity::Friendly, |m| m.intended)
}

/// A name with spaces and case aside (the tables write "Insufferably tedious" for
/// InsufferablyTedious).
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn same(a: &str, b: &str) -> bool {
    norm(a) == norm(b)
}

/// Whether the game's menus govern a social (lists it in some context).
pub fn governed(def: &crate::social::SocialDef) -> bool {
    tables().is_some_and(|t| t.in_menus.contains(&norm(def.game.map_or_else(|| game_key(def.name), |g| g.key))))
}

/// Whether the game offers a social now (its `SocializingActionAvailability`, as
/// `Conversation.GetSocialsForSTC`): those it lists for the context of the social's commodity
/// (the conversation's own if it's in that commodity, else the commodity's first: OK, Amusing,
/// Flirty, Impolite...) between Sims in their relationship (or `Default`), and those it lists for
/// `All`. Socials the menus don't list at all (the topics', our own) aren't held back.
pub fn offered(def: &crate::social::SocialDef, stc: Stc, relationship: &str) -> bool {
    let Some(t) = tables() else { return true };
    let key = norm(def.game.map_or_else(|| game_key(def.name), |g| g.key));
    if !t.in_menus.contains(&key) {
        return true;
    }
    let intended = t.intended.get(&key).copied().unwrap_or(Commodity::Friendly);
    // (A steamed social only when things aren't going well.)
    if intended == Commodity::Steamed && stc.commodity.positive() {
        return false;
    }
    let context = if stc.commodity == intended || !t.stcs.iter().any(|s| s.commodity == intended.name()) {
        norm(stc.name())
    } else {
        t.stcs.iter().find(|s| s.commodity == intended.name()).map_or_else(String::new, |s| norm(&s.name))
    };
    let rel = norm(relationship);
    let group = if t.menus.contains_key(&(context.clone(), rel.clone())) { rel } else { norm("Default") };
    let listed = |g: &str| t.menus.get(&(context.clone(), g.to_string())).is_some_and(|s| s.contains(&key));
    listed(&group) || listed(&norm("All"))
}

/// `X` must hold, or (`~X`) mustn't.
fn holds(want: &str, is: impl Fn(&str) -> bool) -> bool {
    match want.strip_prefix('~') {
        Some(w) => !is(w),
        None => is(want),
    }
}

fn has_trait(sim: &Sim, game_id: &str) -> bool {
    sim.traits.iter().any(|t| same(&t.game_id(), game_id))
}

fn ages_hold(ages: &[String], age: Age) -> bool {
    let letter = match age {
        Age::Baby => "B",
        Age::Toddler => "P",
        Age::Child => "C",
        Age::Teen => "T",
        Age::YoungAdult => "Y",
        Age::Adult => "A",
        Age::Elder => "E",
    };
    ages.is_empty() || ages.iter().any(|a| a == letter)
}

/// Whether a rule fits (the game's `Conversation.IsSuitable`, for the one it's done to).
fn suitable(rule: &SocialRule, model: &Model, j: &Judging, liking: f32, mood: f32) -> bool {
    use rand::Rng;
    if rule.third_party == 1 {
        return false;
    }
    if !rule.speech_act.is_empty() && rule.speech_act != model.speech_act {
        return false;
    }
    if rule.stc_positive != 0 && (rule.stc_positive > 0) != j.stc.commodity.positive() {
        return false;
    }
    if !rule.st_commodity.is_empty() && !holds(&rule.st_commodity, |c| same(c, j.stc.commodity.name())) {
        return false;
    }
    if !rule.st_context.is_empty() && !holds(&rule.st_context, |c| same(c, j.stc.name())) {
        return false;
    }
    if rule.mood != -101.0 && !(if rule.mood >= 0.0 { mood >= rule.mood } else { mood <= rule.mood }) {
        return false;
    }
    if !rule.relationship.is_empty() && !holds(&rule.relationship, |r| same(r, j.relationship)) {
        return false;
    }
    if liking < rule.liking.0 || liking > rule.liking.1 {
        return false;
    }
    if !rule.actor_traits.iter().all(|t| holds(t, |t| has_trait(j.actor, t))) || !rule.target_traits.iter().all(|t| holds(t, |t| has_trait(j.target, t))) {
        return false;
    }
    if !rule.skill.is_empty() {
        let (below, skill) = match rule.skill.strip_prefix('~') {
            Some(s) => (true, s),
            None => (false, rule.skill.as_str()),
        };
        // (Only Charisma is asked of in the game's rules we use; any other is never learned.)
        let level = if same(skill, "Charisma") { j.charisma.map_or(-1.0, |l| l as f32) } else { -1.0 };
        if below == (level >= rule.skill_level) {
            return false;
        }
    }
    let (actor_up, actor_down) = effectiveness(&j.actor.traits, j.actor_moodlets);
    let (target_up, target_down) = effectiveness(&j.target.traits, j.target_moodlets);
    let increased = |up: &[&str], down: &[&str], k: &str| up.iter().any(|u| same(u, k)) && !down.iter().any(|d| same(d, k));
    if (!rule.actor_increased.is_empty() && !increased(&actor_up, &actor_down, &rule.actor_increased))
        || (!rule.actor_reduced.is_empty() && !increased(&actor_down, &actor_up, &rule.actor_reduced))
        || (!rule.target_increased.is_empty() && !increased(&target_up, &target_down, &rule.target_increased))
        || (!rule.target_reduced.is_empty() && !increased(&target_down, &target_up, &rule.target_reduced))
    {
        return false;
    }
    if !rule.proc_test.is_empty() {
        let ok = match rule.proc_test.as_str() {
            "SociallyAwkwardChanceTest" => has_trait(j.actor, "SociallyAwkward") && rand::rng().random_bool(0.25),
            "RequestAccepted" => matches!(j.target.age, Age::Baby | Age::Toddler | Age::Child) || mood > request_threshold(j),
            "IsTargetPartnerOfActor" => j.partners,
            "FamilyOrientedSimWithFamilyMember" => has_trait(j.target, "FamilyOriented") && (j.kin || j.partners),
            "IsTargetSelectableWithPartnerAndNotRomanticInterestOfActor" => !j.actor_member && j.target_member && j.partner_elsewhere,
            // (The boss agrees to a promotion at 75 performance, to a raise at 60; a Schmoozer's
            // at 40 and 30.)
            "ShouldReceivePromotion" => j.boss_hears.is_some_and(|p| p >= 75.0 || (has_trait(j.actor, "Schmoozer") && p >= 40.0)),
            "ShouldReceiveRaise" => j.boss_hears.is_some_and(|p| p >= 60.0 || (has_trait(j.actor, "Schmoozer") && p >= 30.0)),
            // (The game's other tests aren't ours to run: the rule stands aside.)
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    let tri = |want: i8, is: bool| want == 0 || (want > 0) == is;
    if !tri(rule.partner, j.partner_elsewhere) || !tri(rule.repetition, j.repetition) || !tri(rule.wrong_gender, false) || !tri(rule.betrayed, false) {
        return false;
    }
    ages_hold(&rule.actor_ages, j.actor.age) && ages_hold(&rule.target_ages, j.target.age)
}

/// The mood a request needs (`GetMoodThresholdToAcceptRequest`: the context's and the
/// relationship's).
fn request_threshold(j: &Judging) -> f32 {
    let ltr = match j.relationship {
        "Stranger" => 80.0,
        "Acquaintance" | "Distant Friend" => 5.0,
        "Disliked" => 100.0,
        "Friend" => -10.0,
        "Good Friend" | "Old Friend" | "Best Friends Forever" | "Partner" => -20.0,
        "Best Friend" | "Fiancee" | "Spouse" => -30.0,
        "Ex" | "Ex Spouse" => 80.0,
        "Enemy" | "Old Enemies" => 100.0,
        _ => 0.0,
    };
    j.stc.row().map_or(0.0, |r| r.mood_to_accept) + ltr
}

/// What a social comes to (the game's `FindMostSuitableRule`: its own rules and the general
/// ones for what it's meant as, most specific first, the first that fits; the first of its
/// outcomes for that result).
pub fn decide(social: usize, j: &Judging) -> Outcome {
    let Some(model) = model(social) else {
        return Outcome { result: Commodity::Friendly, amount: 40.0, ltr_override: 0.0, bits_add: 0, bits_remove: 0, charisma: -1.0, accepted: true, topics: [""; 3], learned: None, learn_random: false, teaches: [""; 2] };
    };
    // (The holiday warms both.)
    let warm = if j.holiday { 20.0 } else { 0.0 };
    let liking = (j.liking + warm).clamp(-100.0, 100.0);
    let mood = j.target_mood + warm;
    let general = tables().and_then(|t| t.general.get(&model.intended)).map_or(&[][..], |v| v.as_slice());
    let own = model.rules.as_slice();
    let has_result = |r: &str| model.outcomes.is_empty() || model.outcomes.iter().any(|o| same(&o.result, r));
    let (mut i, mut k) = (0, 0);
    let mut found: Option<&SocialRule> = None;
    while i < general.len() || k < own.len() {
        let gs = general.get(i).map_or(f32::NEG_INFINITY, |r| r.spec);
        let ss = own.get(k).map_or(f32::NEG_INFINITY, |r| r.spec);
        let r = if gs > ss {
            i += 1;
            let r = &general[i - 1];
            if !has_result(&r.result) {
                continue;
            }
            r
        } else {
            k += 1;
            &own[k - 1]
        };
        if suitable(r, model, j, liking, mood) {
            found = Some(r);
            break;
        }
    }
    let result = found.and_then(|r| Commodity::from_name(&r.result)).unwrap_or(model.intended);
    let rhs = model.outcomes.iter().find(|o| same(&o.result, result.name()));
    Outcome {
        result,
        amount: rhs.map_or(40.0, |o| o.amount),
        ltr_override: found.map_or(0.0, |r| r.ltr_override),
        bits_add: rhs.map_or(0, |o| o.bits_add),
        bits_remove: rhs.map_or(0, |o| o.bits_remove),
        charisma: rhs.map_or(-1.0, |o| o.charisma),
        accepted: accepted(model.intended, result),
        topics: rhs.map_or([""; 3], |o| [o.topics[0].as_str(), o.topics[1].as_str(), o.topics[2].as_str()]),
        learned: found.and_then(|r| learned_of(r, j.target)),
        learn_random: rhs.is_some_and(|o| o.learn_random),
        teaches: rhs.map_or([""; 2], |o| [o.teaches[0].as_str(), o.teaches[1].as_str()]),
    }
}

/// The target's trait a rule turned on (the game's `ConsiderLearningAboutTargetSim`): one it
/// asks of them, else the one lending the effectiveness it asks.
fn learned_of(r: &SocialRule, target: &Sim) -> Option<Trait> {
    let asked = r.target_traits.iter().filter(|t| !t.starts_with('~')).find_map(|n| target.traits.iter().find(|t| same(&t.game_id(), n)).copied());
    asked.or_else(|| {
        let kind = |k: &str, up: bool| target.traits.iter().find(|t| {
            let (u, d) = effectiveness(&[**t], &[]);
            if up { u.contains(&k) } else { d.contains(&k) }
        }).copied();
        (!r.target_increased.is_empty()).then(|| kind(&r.target_increased, true)).flatten().or_else(|| (!r.target_reduced.is_empty()).then(|| kind(&r.target_reduced, false)).flatten())
    })
}

/// What a social does to the two of them (the game's `UpdateSimAfterInteraction`): the
/// conversation pushed its way, then liking changed by the context's step there (or the rule's
/// flat change), more between the witty for a joke and the charming for anything taken well,
/// less for anything not. Liking's the same both ways.
pub fn effect(social: usize, liking: f32, stc: Stc, o: &Outcome, actor: &Sim, target: &Sim) -> (f32, Stc) {
    let mut stc = stc;
    // (The Irresistible stir romance twice as fast: `kSTCUpdateMultiplerIrresistibleTrait`.)
    let amount = if o.result == Commodity::Amorous && actor.traits.contains(&Trait::Irresistible) { o.amount * IRRESISTIBLE } else { o.amount };
    stc.update(o.result, amount);
    let either = |t: Trait| actor.traits.contains(&t) || target.traits.contains(&t);
    if o.ltr_override != 0.0 {
        return (liking_multiplier(o.ltr_override, liking, actor, target), stc);
    }
    let intended = model(social).map_or(Commodity::Friendly, |m| m.intended);
    let mut num: f32 = 1.0;
    // (TraitTuning's humour and Charismatic multipliers, as the game tunes them.)
    if o.result == Commodity::Funny && either(Trait::GoodSenseOfHumor) {
        num = num.max(1.5);
    }
    if either(Trait::Charismatic) {
        if !o.result.positive() {
            num = num.min(0.75);
        } else if intended.positive() {
            num = num.max(1.25);
        }
    }
    (liking_multiplier(liking_change(&stc, liking) * num, liking, actor, target), stc)
}

/// The Irresistible's push to a romantic context, and how much less the Shy warm to anyone
/// (till they're friends) and more the Nurturing to the young (`TraitTuning`).
const IRRESISTIBLE: f32 = 2.0;
const SHY_GAIN: f32 = 0.85;
const NURTURING_GAIN: f32 = 1.25;

/// The game's `LongTermRelationship.UpdateLiking` multipliers on any change in liking.
fn liking_multiplier(change: f32, liking: f32, a: &Sim, b: &Sim) -> f32 {
    let either = |t: Trait| a.traits.contains(&t) || b.traits.contains(&t);
    let young = |s: &Sim| matches!(s.age, Age::Baby | Age::Toddler | Age::Child | Age::Teen);
    let mut c = change;
    if c > 0.0 && either(Trait::Shy) && liking < crate::social::FRIEND_THRESHOLD {
        c *= SHY_GAIN;
    }
    if (a.traits.contains(&Trait::Nurturing) && young(b)) || (b.traits.contains(&Trait::Nurturing) && young(a)) {
        c *= NURTURING_GAIN;
    }
    c
}

/// The game's relationship flags a social's outcome sets and clears that we keep: romance
/// (`Romantic`), the first kiss (`Kissed`) and sworn enmity (`MakeEnemy`).
pub const BIT_ROMANTIC: u32 = 0x2;
pub const BIT_MAKE_ENEMY: u32 = 0x800;
pub const BIT_KISSED: u32 = 0x2000;

/// What a social's outcome makes of the relationship's flags.
pub fn apply_bits(rel: &mut crate::social::Relationship, o: &Outcome) {
    if o.bits_add & (BIT_ROMANTIC | BIT_KISSED) != 0 {
        rel.romantic = true;
    }
    if o.bits_add & BIT_KISSED != 0 {
        rel.kissed = true;
    }
    if o.bits_remove & BIT_ROMANTIC != 0 {
        rel.romantic = false;
    }
    if o.bits_add & BIT_MAKE_ENEMY != 0 {
        rel.enemy = true;
    }
    if o.bits_remove & BIT_MAKE_ENEMY != 0 {
        rel.enemy = false;
    }
}

/// What's the same for both of them after a social: their conversation, liking, enmity and
/// romance.
#[derive(Clone, Copy, Debug)]
pub struct Shared {
    pub stc: Stc,
    pub liking: f32,
    pub enemy: bool,
    pub romantic: bool,
}

impl Shared {
    pub fn of(rel: &crate::social::Relationship) -> Self {
        Self { stc: rel.stc, liking: rel.friendship, enemy: rel.enemy, romantic: rel.romantic }
    }
}

/// Each Sim's memory of what they've said in their conversation.
#[derive(bevy::prelude::Resource, Default)]
pub struct SocialMemories(pub HashMap<bevy::prelude::Entity, SocialMemory>);

/// A Sim's memory of what they've said in this conversation: how many socials, when each was
/// last, and its chances left before it's repetition (the game's `SocialComponent` action
/// memory).
#[derive(bevy::prelude::Component, Default, Debug)]
pub struct SocialMemory {
    pub count: u32,
    pub last_at: f64,
    pub entries: HashMap<usize, (u32, u8, u8)>,
}

/// How long since their last social before it's a new conversation (game minutes).
const NEW_CONVERSATION_MINUTES: f64 = 60.0;

impl SocialMemory {
    /// Before a social: a new conversation forgets the last; chances earned back by saying
    /// something else in between (`UpdateChancesLeft`). Whether it's out of chances.
    pub fn before(&mut self, social: usize, now: f64) -> bool {
        if now - self.last_at > NEW_CONVERSATION_MINUTES {
            self.count = 0;
            self.entries.clear();
        }
        let Some(m) = model(social) else { return false };
        let count = self.count;
        let e = self.entries.entry(social).or_insert((u32::MAX, m.chances, m.recharges));
        if e.0 != u32::MAX && count > e.0 && e.1 < m.chances && e.2 > 0 {
            e.1 += 1;
            e.2 -= 1;
        }
        e.1 == 0
    }

    /// After it (`RegisterActionPerformed`).
    pub fn after(&mut self, social: usize, now: f64) {
        self.count += 1;
        self.last_at = now;
        let chances = model(social).map_or(2, |m| m.chances);
        let recharges = model(social).map_or(2, |m| m.recharges);
        let e = self.entries.entry(social).or_insert((u32::MAX, chances, recharges));
        e.0 = self.count;
        e.1 = e.1.saturating_sub(1);
    }
}

/// The relationship's state by the game's name, for its rules (`CalculateLTRState`).
pub fn relationship_state(liking: f32, status: crate::social::RelStatus, romantic: bool, enemy: bool) -> &'static str {
    use crate::social::RelStatus as S;
    match status {
        S::Married => return "Spouse",
        S::Engaged => return "Fiancee",
        S::Partner => return "Partner",
        _ => {}
    }
    if enemy && liking <= -1.0 {
        return "Enemy";
    }
    if status == S::Ex {
        return "Ex";
    }
    match liking {
        l if romantic && l >= 1.0 => "Romantic Interest",
        l if l >= 80.0 => "Best Friend",
        l if l >= 60.0 => "Good Friend",
        l if l >= 40.0 => "Friend",
        l if l <= -20.0 => "Disliked",
        _ => "Acquaintance",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contexts_move_as_the_games() {
        let mut s = Stc::default();
        s.update(Commodity::Friendly, 40.0);
        s.update(Commodity::Friendly, 40.0);
        assert_eq!(s, Stc { commodity: Commodity::Friendly, value: 80.0 });
        s.update(Commodity::Neutral, 40.0);
        assert_eq!(s.value, 40.0, "a neutral reply halves it");
        s.update(Commodity::Boring, 40.0);
        assert_eq!(s, Stc { commodity: Commodity::Boring, value: 40.0 }, "and a dull one wipes it");
        s.update(Commodity::Boring, 2000.0);
        assert_eq!(s.value, STC_MAX);
        s.decay();
        assert_eq!(s.value, 600.0);
        let mut low = Stc { commodity: Commodity::Funny, value: 120.0 };
        low.decay();
        assert_eq!(low, Stc { commodity: Commodity::Friendly, value: 1.0 }, "under 100, back to nothing");
    }

    #[test]
    fn only_a_good_reply_is_taken_well() {
        assert!(accepted(Commodity::Friendly, Commodity::Friendly));
        assert!(!accepted(Commodity::Friendly, Commodity::Neutral));
        assert!(!accepted(Commodity::Amorous, Commodity::Awkward));
        assert!(accepted(Commodity::Insulting, Commodity::Steamed), "an insult always lands");
        assert!(accepted(Commodity::Neutral, Commodity::Friendly));
    }

    #[test]
    fn states_by_the_games_liking() {
        use crate::social::RelStatus as S;
        assert_eq!(relationship_state(39.0, S::None, false, false), "Acquaintance");
        assert_eq!(relationship_state(40.0, S::None, false, false), "Friend");
        assert_eq!(relationship_state(85.0, S::None, false, false), "Best Friend");
        assert_eq!(relationship_state(-70.0, S::None, false, false), "Disliked", "low liking alone isn't enmity");
        assert_eq!(relationship_state(-70.0, S::None, false, true), "Enemy");
        assert_eq!(relationship_state(10.0, S::None, true, false), "Romantic Interest");
        assert_eq!(relationship_state(10.0, S::Married, true, false), "Spouse");
    }

    /// The game's rules as baked give what its tables do for plain Sims (`SIMS3_CACHE=<baked>
    /// cargo test -p sims3 rules_decide_as_the_games -- --ignored`).
    #[test]
    #[ignore]
    fn rules_decide_as_the_games() {
        let data = s3bake::gamedata::load_gamedata(&s3bake::default_root()).expect("game data");
        let clip_names: Vec<String> = s3bake::read_value(&s3bake::default_root().global_dir().join("clip_names.bin")).unwrap_or_default();
        crate::social::install_game_socials(&data.socials, &clip_names);
        load(&data);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(1);
        let mut a = crate::sim::random_sim(&mut rng, "A", Some(true), Age::Adult);
        let mut b = crate::sim::random_sim(&mut rng, "B", Some(false), Age::Adult);
        a.traits.clear();
        b.traits.clear();
        let index = |name: &str| crate::social::all_socials().find(|(_, s)| s.name == name).map(|(i, _)| i).unwrap();
        let judge = |liking: f32, stc: Stc, mood: f32| Judging {
            actor: &a, target: &b, actor_moodlets: &[], target_moodlets: &[], liking,
            relationship: relationship_state(liking, crate::social::RelStatus::None, false, false), stc, target_mood: mood,
            charisma: None, partner_elsewhere: false, partners: false, kin: false, repetition: false,
            actor_member: true, target_member: false, holiday: false, boss_hears: None,
        };
        let ok = Stc { commodity: Commodity::Friendly, value: 0.0 };
        // (Hugging: Friendly from -19 up, Neutral down to -49, Boring below.)
        let hug = index("Hug");
        assert_eq!(decide(hug, &judge(0.0, ok, 10.0)).result, Commodity::Friendly);
        assert_eq!(decide(hug, &judge(-30.0, ok, 10.0)).result, Commodity::Neutral);
        assert_eq!(decide(hug, &judge(-60.0, ok, 10.0)).result, Commodity::Boring);
        // (A joke: Funny in good company; in a dull conversation it needs liking 0.)
        let joke = index("Tell Joke");
        assert_eq!(decide(joke, &judge(0.0, ok, 10.0)).result, Commodity::Funny);
        let dull = Stc { commodity: Commodity::Boring, value: 40.0 };
        assert_eq!(decide(joke, &judge(-5.0, dull, 10.0)).result, Commodity::Boring);
        // (A flirt: Amorous from liking 10 with the target in a fair mood, Neutral below.)
        let flirt = index("Flirt");
        assert_eq!(decide(flirt, &judge(30.0, ok, 10.0)).result, Commodity::Amorous);
        assert_eq!(decide(flirt, &judge(0.0, ok, 10.0)).result, Commodity::Neutral);
        // (Asking the boss for a promotion: agreed at liking 60 with performance 75, not at 50.)
        let ask = index("Ask for Promotion");
        let boss = |perf: f32| Judging { boss_hears: Some(perf), ..judge(70.0, ok, 10.0) };
        assert_eq!(decide(ask, &boss(80.0)).result, Commodity::Friendly);
        assert_eq!(decide(ask, &Judging { boss_hears: Some(80.0), ..judge(30.0, ok, 10.0) }).result, Commodity::Boring);
        assert_ne!(decide(ask, &boss(50.0)).result, Commodity::Friendly);
        let raise = index("Ask for a Raise");
        assert_eq!(decide(raise, &boss(65.0)).result, Commodity::Friendly);
        // (Discussing work raises the talk of pay that asking needs.)
        let discuss = index("Discuss Work");
        assert_eq!(decide(discuss, &judge(30.0, ok, 10.0)).topics[2], "Work Pay");
        assert_eq!(decide(flirt, &judge(-40.0, ok, 10.0)).result, Commodity::Awkward);
        // (Chat ignores liking: Friendly in good company.)
        assert_eq!(decide(index("Chat"), &judge(-60.0, ok, 10.0)).result, Commodity::Friendly);
        // (Four flirts taken well make it seductive: a kiss is on the menu between friends then,
        // not before; going steady only once it's hot between romantic interests.)
        let mut stc = ok;
        for _ in 0..4 {
            let o = decide(flirt, &judge(45.0, stc, 10.0));
            assert_eq!(o.result, Commodity::Amorous);
            stc.update(o.result, o.amount);
        }
        assert_eq!(stc.name(), "Seductive");
        let kiss = crate::social::all_socials().find(|(_, s)| s.name == "Kiss").unwrap().1;
        assert!(!offered(kiss, ok, "Friend"));
        assert!(offered(kiss, stc, "Friend"));
        let steady = crate::social::all_socials().find(|(_, s)| s.name == "Ask to Go Steady").unwrap().1;
        assert!(!offered(steady, stc, "Romantic Interest"));
        assert!(offered(steady, Stc { commodity: Commodity::Amorous, value: 320.0 }, "Romantic Interest"));
        // (A joke on someone with no sense of humour: the way it goes turns on it, and it's learned.)
        let mut c = b.clone();
        c.traits = vec![crate::life::Trait::NoSenseOfHumor];
        let o = decide(joke, &Judging { target: &c, ..judge(30.0, ok, 10.0) });
        println!("joke on the humourless: {:?}, learned {:?}", o.result, o.learned);
        assert_eq!(o.learned, Some(crate::life::Trait::NoSenseOfHumor));
        // (A chat may teach one of theirs, the game's `LearnRandomTrait`.)
        assert!(decide(index("Chat"), &judge(30.0, ok, 10.0)).learn_random);
    }

    #[test]
    fn traits_bend_liking_as_the_games() {
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(2);
        let mut a = crate::sim::random_sim(&mut rng, "A", Some(true), Age::Adult);
        let mut b = crate::sim::random_sim(&mut rng, "B", Some(false), Age::Adult);
        a.traits.clear();
        b.traits.clear();
        assert_eq!(liking_multiplier(2.0, 10.0, &a, &b), 2.0);
        b.traits.push(Trait::Shy);
        assert_eq!(liking_multiplier(2.0, 10.0, &a, &b), 1.7, "the Shy warm slower");
        assert_eq!(liking_multiplier(2.0, 50.0, &a, &b), 2.0, "once they're friends, not");
        assert_eq!(liking_multiplier(-2.0, 10.0, &a, &b), -2.0);
        b.traits.clear();
        a.traits.push(Trait::Nurturing);
        b.age = Age::Teen;
        assert_eq!(liking_multiplier(2.0, 50.0, &a, &b), 2.5, "the Nurturing to the young");
        // (The Irresistible push romance twice as far.)
        a.traits = vec![Trait::Irresistible];
        let o = Outcome { result: Commodity::Amorous, amount: 40.0, ltr_override: 0.0, bits_add: 0, bits_remove: 0, charisma: -1.0, accepted: true, topics: [""; 3], learned: None, learn_random: false, teaches: [""; 2] };
        let (_, stc) = effect(0, 10.0, Stc::default(), &o, &a, &b);
        assert_eq!(stc, Stc { commodity: Commodity::Amorous, value: 80.0 });
    }

    #[test]
    fn a_new_conversation_forgets_the_last() {
        let mut m = SocialMemory::default();
        m.after(7, 0.0);
        m.after(7, 1.0);
        assert_eq!(m.count, 2);
        m.before(7, 200.0);
        assert_eq!(m.count, 0);
    }
}
