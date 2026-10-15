//! Gameplay data from the game's own tuning tables (`Game/Bin/Gameplay/GameplayData.package`,
//! XML exported from the designers' spreadsheets): the base game's moodlets (buffs), traits and
//! skills with their names and descriptions in English and the interface icons that go with
//! them. The icons (PNG, `0x2F7D0004`, instance = fnv64 of the lowercase name) are copied to
//! `icons.pack` as they are.

use std::collections::{BTreeSet, HashMap, HashSet};

use s3pkg::{Package, PackageSet};
use serde::{Deserialize, Serialize};

use crate::bake::BakeRoot;
use crate::pack::{PackReader, PackWriter, read_value, write_value};

pub const GAMEDATA_VERSION: u32 = 118;
/// Interface images.
pub const T_ICON: u32 = 0x2F7D0004;
const T_XML: u32 = 0x0333406C;
/// Interaction tuning (ITUN).
const T_ITUN: u32 = 0x03B3_3DDF;
/// Textures (balloon icons are 64 px DDS pictures, instance = fnv64 of the lowercase name).
const T_DDS: u32 = 0x00B2D882;
/// Buy-mode catalogue thumbnails, 128 px PNG (`Thumbnails/AllThumbnails.package`): instance =
/// the object's OBJD instance, group = its colour variant.
const T_THUMB_LARGE: u32 = 0x0580A2B6;

/// Create-a-Sim part thumbnails, 128 px PNG (`Thumbnails/CasThumbnails.package`): instance =
/// the CASP instance, group = its colour preset.
const T_CAS_THUMB: u32 = 0x626F60CD;

/// The icon name an object's catalogue thumbnail is stored under.
pub fn thumb_name(objd_instance: u64) -> String {
    format!("thumb_{objd_instance:016x}")
}

/// The icon name a CAS part's thumbnail is stored under.
pub fn cas_thumb_name(casp_instance: u64) -> String {
    format!("casthumb_{casp_instance:016x}")
}
const T_NMAP: u32 = 0x0166038C;

/// A moodlet: the game's buff table row.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BuffInfo {
    pub hex: String,
    pub name: String,
    pub desc: String,
    pub icon: String,
    /// The mood it affects ("Happy", "Uncomfortable"…) and by how much.
    pub axis: String,
    pub value: i32,
    /// Minutes until it wears off (0 = while its cause lasts).
    pub timeout: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TraitInfo {
    pub hex: String,
    pub name: String,
    pub desc: String,
    /// Large icon (Create-a-Sim) and the small one used in menus.
    pub icon: String,
    pub icon_small: String,
    pub category: String,
    /// A lifetime reward's cost in lifetime happiness (`RewardMotive`, `RewardShop`... traits).
    pub points: u32,
    /// A personality trait's score (the same `Points`: +10 good, -10 bad, -20 or less horrible)
    /// and its weight when drawn at random (`RandomWeight`), as a newborn's are; whether it's
    /// ever drawn so (`CanBeLearnedRandomly`).
    #[serde(default)]
    pub score: i32,
    #[serde(default)]
    pub weight: f32,
    #[serde(default)]
    pub random: bool,
    /// How it's said of someone, once it's learned ("{0.SimFirstName} is a Bookworm": its
    /// `Predicate`).
    #[serde(default)]
    pub predicate: String,
}

/// A skill book (the game's `BookSkill`): for the skill between its levels, so many points a
/// page, so many pages (read so many a minute, a Bookworm more), at its price.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkillBookInfo {
    pub key: String,
    pub name: String,
    pub author: String,
    pub skill: String,
    pub min: u32,
    pub max: u32,
    pub points_per_page: f32,
    pub pages: u32,
    pub price: i64,
    pub pages_per_minute: f32,
    pub bookworm_pages_per_minute: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkillInfo {
    pub hex: String,
    pub name: String,
    pub desc: String,
    /// Small icon, the wish icon and the picture of the skill's object.
    pub icon: String,
    pub wish_icon: String,
    pub object_icon: String,
    pub max_level: u32,
    /// The skill points it takes to reach each level (running totals, level 1 first: the
    /// table's `Level_N`, the first row's where it has none).
    #[serde(default)]
    pub points: Vec<u32>,
}

/// One of a career level's performance metrics.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CareerMetric {
    pub kind: String,
    pub mappings: [f32; 3],
    pub arg: String,
}

/// One level of a career (a row of the career's own table).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CareerLevelInfo {
    pub level: u32,
    /// "Base" until the career branches, then the branch's name.
    pub branch: String,
    pub title: String,
    pub title_female: String,
    /// Simoleons per hour.
    pub hourly: f32,
    pub start: f32,
    pub hours: f32,
    /// When a Sim at this level gets up for work (the table's `WakeupTime`; 0 where it has none).
    #[serde(default)]
    pub wakeup: f32,
    /// How performance is judged at this level (the table's `Metric1..4`, `Mappings1..4` and
    /// `Args1..4`): each metric (`Mood`, `RelBoss`, `RelCoworkers`, `SkillX`...), the values it
    /// scores -3, 0 and +3 at, and its skill.
    #[serde(default)]
    pub metrics: Vec<CareerMetric>,
    /// Whether there's a boss at this level (`HasBoss`).
    #[serde(default)]
    pub has_boss: bool,
    /// Workdays: bit 0 = Monday.
    pub days: u8,
    /// Skills that count towards performance at this level.
    pub skills: Vec<String>,
    /// The uniform (an outfit's name, or empty for everyday clothes) for men, women, elderly
    /// men and elderly women.
    pub outfits: [String; 4],
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CareerInfo {
    pub hex: String,
    pub name: String,
    pub desc: String,
    /// How fast performance flows at work: an hour at the best (`MaxPerfFlow`) and worst
    /// (`MinPerfFlow`) metrics, the same at the top levels (`...MaxLevel`), and an hour missed
    /// (`MissWorkPerf`); a promotion's bonus in hours' pay (`BonusAmountInHours`); fun an hour at
    /// work (`FunStressPerHour`); how many coworkers (`MinCoworkers`, `MaxCoworkers`).
    #[serde(default)]
    pub flow: [f32; 4],
    #[serde(default)]
    pub miss_work: f32,
    #[serde(default)]
    pub bonus_hours: f32,
    #[serde(default)]
    pub fun_per_hour: f32,
    #[serde(default)]
    pub coworkers: (u8, u8),
    /// What the job listings say of it (`Text_JobOffer`: "The Hospital could always use...").
    #[serde(default)]
    pub offer: String,
    pub icon: String,
    pub part_time: bool,
    pub levels: Vec<CareerLevelInfo>,
    /// What can happen at work (the game's `CareerEvents` table).
    #[serde(default)]
    pub events: Vec<CareerEventInfo>,
}

/// One of a career's events at work (a row of the game's `CareerEvents` table): a message with
/// its rewards (`EventReward`), a choice (`EventChoice`: yes might win or lose, no), or a career
/// opportunity offered (`EventOpportunity`); how likely each work day, at which levels (1-based;
/// none: any) of which branch ("All" or one), whether it's the day's sure one; its words (for a
/// man and for a woman); what it gives (the game's reward strings: `CareerPerformance,20`,
/// `Buff,Fatigued`...).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CareerEventInfo {
    pub key: String,
    pub kind: String,
    pub likelihood: f32,
    pub branch: String,
    pub levels: Vec<u32>,
    pub sure: bool,
    pub text: [String; 2],
    pub yes: [String; 2],
    pub no: [String; 2],
    pub yes_result: [String; 2],
    pub yes_lose_result: [String; 2],
    pub no_result: [String; 2],
    /// The chance (percent) a yes wins, and what changes it (`Mood,30`, `Skill,Cooking,5`).
    pub chance: f32,
    pub modifiers: Vec<String>,
    pub rewards_yes: Vec<String>,
    pub rewards_yes_lose: Vec<String>,
    pub rewards_no: Vec<String>,
    pub opportunity: String,
}

/// The events of one career in the game's `CareerEvents` table (rows under the career's own
/// tag, the first its defaults), their words from the career's `CareerEvents` sheet (a woman's
/// under the key's `_Female`, where there's one).
pub fn career_events(xml: &str, career: &str, text: &dyn Fn(&str) -> String) -> Vec<CareerEventInfo> {
    let text = |k: &str| -> [String; 2] {
        let m = text(k);
        let f = if k.is_empty() { String::new() } else { text(&format!("{k}_Female")) };
        let f = if f.is_empty() { m.clone() } else { f };
        [m, f]
    };
    let rows = records(xml, career);
    let default = rows.first().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for r in rows.iter().skip(1) {
        let get = |k: &str| r.get(k).or(default.get(k)).map(|v| v.trim().to_string()).unwrap_or_default();
        let name = get("EventName");
        let class = name.split(',').next().unwrap_or("").rsplit(['+', '.']).next().unwrap_or("").to_string();
        let opportunity = get("OpportunityName");
        let kind = match class.as_str() {
            "EventReward" => "Reward",
            "EventChoice" => "Choice",
            "" if !opportunity.is_empty() => "Opportunity",
            _ => continue,
        };
        let mut levels = Vec::new();
        for part in get("AvailableLevels").split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (a, b) = part.split_once(':').unwrap_or((part, part));
            if let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                levels.extend(a..=b);
            }
        }
        let list = |prefix: &str| (1..=3).map(|i| get(&format!("{prefix}{i}"))).filter(|v| !v.is_empty()).collect::<Vec<_>>();
        out.push(CareerEventInfo {
            key: get("EventType"),
            kind: kind.into(),
            likelihood: get("Likelihood").parse().unwrap_or(0.0),
            branch: { let b = get("BranchName"); if b.is_empty() { "All".into() } else { b } },
            levels,
            sure: get("SureShotEvent") == "1",
            text: text(&get("Description")),
            yes: text(&get("YesText")),
            no: text(&get("NoText")),
            yes_result: text(&get("YesResultText")),
            yes_lose_result: text(&get("YesLoseResultText")),
            no_result: text(&get("NoResultText")),
            chance: get("ChanceToWin").parse().unwrap_or(100.0),
            modifiers: list("ChanceModifier"),
            rewards_yes: list("RewardYes"),
            rewards_yes_lose: list("RewardYesLose"),
            rewards_no: list("RewardNo"),
            opportunity,
        });
    }
    out
}

/// An opportunity done at a rabbit hole (the game's most common kind): go to a venue of a type
/// during its hours and spend some time there, for a reward.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OpportunityInfo {
    pub guid: String,
    pub name: String,
    pub desc: String,
    pub completion: String,
    pub failure: String,
    pub icon: String,
    /// The rabbit hole type (`CityHall`, `Restaurant`, `ScienceLab`...).
    pub rabbit_hole: String,
    /// What the Sim does there, and for how long (minutes), between these hours.
    pub interaction: String,
    pub minutes: f32,
    pub open: f32,
    pub close: f32,
    /// Days to do it in (0: no limit).
    pub days: f32,
    /// A career (its key, like `Business`) or skill (`Logic`, min..max level) it's offered for.
    pub career: String,
    pub skill: String,
    pub skill_min: u32,
    pub skill_max: u32,
    /// Rewards: simoleons, job performance, a raise (percent), skill (percent of a level).
    pub money: i64,
    pub performance: f32,
    pub raise: f32,
    pub skill_reward: f32,
    pub repeat: bool,
}

/// A garden plant from the game's `plants` table, with what it bears (`ingredients`).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PlantInfo {
    pub name: String,
    /// Common, Uncommon, Rare or Special.
    pub rarity: String,
    /// Its seed's name ("Tomato Seed", "Money Tree Seed").
    #[serde(default)]
    pub seed: String,
    /// The plant object (`GardenPlantBush`, `PlantLargeTree`, `PlantSmallVine`) and how high its
    /// produce grows (Low, Medium, High: the harvesting animation).
    pub model: String,
    pub height: String,
    /// What it bears and what each is worth.
    pub produce: String,
    pub price: i64,
    pub harvest_min: u32,
    pub harvest_max: u32,
    /// Harvests in its life.
    pub lifetime: u32,
    /// Water lost per hour (percent), and how weedy it gets.
    pub water_decay: f32,
    pub weeds: f32,
    /// Gardening skill points for planting and harvesting.
    pub skill_plant: f32,
    pub skill_harvest: f32,
    /// The produce's model (in `produce.pack`; its group is the geometry state's hash).
    pub produce_model: Option<crate::types::Key>,
}

/// A fence from the build catalogue, with its pieces' models (in `fences.pack`).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FenceStyle {
    /// The CFEN.
    pub key: crate::types::Key,
    pub name: String,
    pub price: i32,
    pub straight: Option<crate::types::Key>,
    pub diagonal: Option<crate::types::Key>,
    pub post: Option<crate::types::Key>,
}

/// A wallpaper or floor covering from the build catalogue.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PatternInfo {
    pub name: String,
    pub price: i32,
    pub floor: bool,
    /// Original CWAL material-category flags, interpreted separately for floors and walls.
    #[serde(default)]
    pub sort_flags: u32,
    /// Its rendered texture in the texture store.
    pub texture: crate::types::Key,
    /// The catalogue pattern (CWAL instance); its swatches (the game's colour presets for it,
    /// each rendered: the first is `texture`), and each swatch's colour channels for Create a
    /// Style.
    #[serde(default)]
    pub cwal: u64,
    #[serde(default)]
    pub swatches: Vec<crate::types::Key>,
    #[serde(default)]
    pub channels: Vec<Vec<(u8, [f32; 3])>>,
}

/// How many of a wall or floor pattern's colour presets are offered as swatches.
const PATTERN_SWATCHES: usize = 8;

/// One choice in a balloon list: an icon, or another list to draw from (`refkey`), or one of the
/// game's special pickers (an icon name like "GetSpeechBalloonImageForChat").
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BalloonEntry {
    pub icon: String,
    pub refkey: String,
    /// 0 neutral, 1 like (a smile badge), 2 dislike (crossed out).
    pub axis: u8,
    pub weight: f32,
}

/// The game's balloon table (`Balloons`): what Sims think and say, keyed by need, moodlet or
/// trait (idle thoughts), social interaction, conversation topic, and random sets.
/// One of the game's social interactions (its `SocialData` tables): what it's called, its tone
/// (`Friendly`, `Funny`, `Amorous`, `Insulting`...), the ages that do it and have it done to
/// them, whether Sims do it of their own accord or only when told, the traits it asks of the one
/// it's done to (`AKTT`) and of the one who does it (`ATREQ`), those that rule it out (`ATR`, and
/// of their own accord `ATRA`) and those that make a Sim keener on it (`ATE`), the liking it needs
/// (`ALTR`), the game's own test for it (when it has one), and its animations' graph and suite.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SocialAction {
    pub key: String,
    pub name: String,
    pub tone: String,
    pub actor_ages: Vec<String>,
    pub target_ages: Vec<String>,
    pub autonomous: bool,
    pub user_only: bool,
    /// The traits the one it's done to must have, each of them (the game's `AKTT`: unless the one
    /// doing it has it too).
    pub target_traits: Vec<String>,
    pub keen: Vec<String>,
    /// The traits that rule it out for the one who'd do it (`ATR`).
    pub restrict: Vec<String>,
    pub liking: Option<(f32, f32)>,
    pub test: String,
    pub graph: String,
    pub suite: String,
    /// The topics it's offered under (the game's `SocializingActionAvailability`): a topic's
    /// name (`Evil Trait`, `Buff Fired`, `Birthday`...), and whether it's offered to the Sim
    /// with the topic (or to others, with them). None: it's always offered.
    #[serde(default)]
    pub topics: Vec<(String, bool)>,
    /// The traits that rule it out of a Sim's own accord (`ATRA`).
    #[serde(default)]
    pub restrict_auto: Vec<String>,
    /// The traits one of which the one doing it must have (`ATREQ`, the robots' chips and modes
    /// left out).
    #[serde(default)]
    pub required: Vec<String>,
    /// What kind of talk it is (`<SA key>`: Talk, Boast, Request...; empty: none).
    #[serde(default)]
    pub speech_act: String,
    /// How many times running it goes down well before it's repetition, and how many more chances
    /// doing something else between earns back (`<Misc c r>`, 2 and 2 where not given).
    #[serde(default)]
    pub chances: u8,
    #[serde(default)]
    pub recharges: u8,
    /// How it strikes a partner who sees it (`<Misc JL>`: 1 low, a flirt; 2 medium, a kiss; 3
    /// high, WooHoo; 0 not at all).
    #[serde(default)]
    pub jealousy: u8,
    /// Its own rules (`<LHS>`), each with the result it gives.
    #[serde(default)]
    pub rules: Vec<SocialRule>,
    /// What each result does (`<RHS>`): the conversation's push towards it and the relationship's
    /// flags it sets or clears.
    #[serde(default)]
    pub outcomes: Vec<SocialOutcome>,
}

/// One of the game's social rules: a general one (the `Socializing` table's `<GR>` rows, for
/// socials of one intended commodity) or a social's own (`<LHS>`). What it asks of the
/// conversation, the two Sims and their relationship, and the result (a commodity) it gives;
/// the first that fits, most specific first, decides. Negations are written `~X`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SocialRule {
    /// The intended commodity a general rule is for (`GC`; empty for a social's own).
    pub commodity: String,
    pub result: String,
    pub spec: f32,
    /// The kind of talk it's for (`SpAct`; empty: any).
    pub speech_act: String,
    /// 0: judged by the one it's done to; 1: by onlookers only; 2: both.
    pub third_party: u8,
    /// The conversation's commodity positive (1) or negative (-1), or either (0).
    pub stc_positive: i8,
    pub st_commodity: String,
    pub st_context: String,
    /// The target's mood at least this (or, below 0, at most); -101: any.
    pub mood: f32,
    pub relationship: String,
    pub liking: (f32, f32),
    pub actor_traits: Vec<String>,
    pub target_traits: Vec<String>,
    /// The actor's skill at least `skill_level` (`~Skill`: below it).
    pub skill: String,
    pub skill_level: f32,
    /// Increased and reduced effectiveness asked of the actor and of the one judging.
    pub actor_increased: String,
    pub actor_reduced: String,
    pub target_increased: String,
    pub target_reduced: String,
    /// The game's own test it runs (`SocialTest.<name>`).
    pub proc_test: String,
    /// 1: must be so; -1: must not; 0: either.
    pub partner: i8,
    pub wrong_gender: i8,
    pub repetition: i8,
    pub betrayed: i8,
    pub actor_ages: Vec<String>,
    pub target_ages: Vec<String>,
    /// Liking changed by this, flat, instead of by the conversation (`LTROverride`).
    pub ltr_override: f32,
}

/// What one of a social's results does (its `<RHS>`): the commodity, how far it moves the
/// conversation (`<TUN val>`, 40 where not given), and the relationship's flags (the game's
/// `InteractionBits`) it sets and clears.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SocialOutcome {
    pub result: String,
    pub amount: f32,
    pub bits_add: u32,
    pub bits_remove: u32,
    /// The Charisma points it's worth when it goes down well (`<TUN charInc>`; -1: the
    /// context's).
    #[serde(default = "no_charisma_override")]
    pub charisma: f32,
    /// The topics it raises (`<Topic x y teo>`): the actor's, the target's, and the
    /// conversation's (both of theirs).
    #[serde(default)]
    pub topics: [String; 3],
    /// Whether it may teach the actor one of the target's traits (`<TUN is>`: the game's
    /// `LearnRandomTrait`, Chat, Deep Conversation, Share Interests...).
    #[serde(default)]
    pub learn_random: bool,
    /// The traits it teaches if they have them (`<Trait x y>`: the actor's, learned by the
    /// target, and the target's, learned by the actor; comma lists).
    #[serde(default)]
    pub teaches: [String; 2],
}

fn no_charisma_override() -> f32 {
    -1.0
}

/// One of the game's short-term contexts (the `Socializing` table's `<STC>` rows): the
/// conversation's commodity and its range, the liking change a social makes in it (`Y1` below
/// liking `X2`, `Y2` below `X3`, else `Y3`), the liking it drifts by a day above and below
/// nothing, and the mood a request is accepted at.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct StcInfo {
    pub name: String,
    pub commodity: String,
    pub min: f32,
    pub max: f32,
    pub x2: f32,
    pub x3: f32,
    pub y: [f32; 3],
    pub decay_above: f32,
    pub decay_below: f32,
    pub mood_to_accept: f32,
    pub charisma: f32,
}

/// Which socials the game offers in a conversation (its `SocializingActionAvailability`
/// table's `SAA` rows): in a context (`Cat`), between Sims in a relationship state (`Gr`: one of
/// them, `Default` for the rest, `All` for any), the socials' keys.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SocialMenu {
    pub stc: String,
    pub group: String,
    pub socials: Vec<String>,
}

/// The `SAA` rows as menus: a `Cat` row starts a context, `Gr` rows the relationship states
/// (consecutive ones sharing what follows), `FPA` rows the socials.
pub fn social_menus(xml: &str) -> Vec<SocialMenu> {
    let mut out: Vec<SocialMenu> = Vec::new();
    let mut stc = String::new();
    // (The groups taking the socials that follow, and whether a social came since the last.)
    let mut groups: Vec<usize> = Vec::new();
    let mut since_social = true;
    for r in records(xml, "SAA").iter().skip(1) {
        let get = |k: &str| r.get(k).map(|v| v.trim()).filter(|v| !v.is_empty());
        if let Some(c) = get("Cat") {
            stc = c.to_string();
            groups.clear();
            since_social = true;
        }
        if let Some(g) = get("Gr") {
            if since_social {
                groups.clear();
            }
            since_social = false;
            out.push(SocialMenu { stc: stc.clone(), group: g.to_string(), socials: Vec::new() });
            groups.push(out.len() - 1);
        }
        if let Some(a) = get("FPA") {
            since_social = true;
            for &g in &groups {
                if !out[g].socials.iter().any(|s| s == a) {
                    out[g].socials.push(a.to_string());
                }
            }
        }
    }
    out
}

/// A rule's specificity where the table doesn't give it (`SocialRuleLHS.Specificity`).
fn rule_specificity(r: &SocialRule, general: bool) -> f32 {
    let mut s = if general { 0.9 } else { 1.0 };
    let set = |v: &str| !v.is_empty() && v != "None";
    if set(&r.speech_act) {
        s += 0.5;
    }
    if r.stc_positive != 0 {
        s += 0.5;
    }
    if set(&r.st_commodity) {
        s += 1.0;
    }
    if set(&r.st_context) {
        s += 2.0;
    }
    if r.mood != -101.0 {
        s += 10.0;
    }
    if set(&r.relationship) {
        s += 3.0;
    }
    if r.liking != (-100.0, 100.0) {
        s += 3.0 + 1.0 - (r.liking.1 - r.liking.0) / 200.0;
    }
    s += 4.0 * r.actor_traits.len() as f32 + 8.0 * r.target_traits.len() as f32;
    if set(&r.skill) {
        s += 4.5 + 0.1 * r.skill_level;
    }
    for (v, w) in [(&r.actor_increased, 5.0), (&r.actor_reduced, 5.0), (&r.target_increased, 4.0), (&r.target_reduced, 4.0)] {
        if set(v) {
            s += w;
        }
    }
    if set(&r.proc_test) {
        s += 25.0;
    }
    if r.partner != 0 {
        s += 8.0;
    }
    if r.repetition != 0 {
        s += 10.0;
    }
    if !r.actor_ages.is_empty() {
        s += 20.0;
    }
    if !r.target_ages.is_empty() {
        s += 20.0;
    }
    if r.betrayed != 0 {
        s += 20.0;
    }
    if r.wrong_gender != 0 {
        s += 30.0;
    }
    if r.third_party == 1 {
        s += 50.0;
    }
    s
}

/// A tri-state flag as the tables write it.
fn maybe(v: Option<&str>) -> i8 {
    match v.map(str::trim) {
        Some("True") | Some("1") => 1,
        Some("False") | Some("-1") => -1,
        _ => 0,
    }
}

/// The general rules (the `Socializing` table's `<GR>` rows; the first is the defaults).
pub fn general_rules(xml: &str) -> Vec<SocialRule> {
    let rows = records(xml, "GR");
    let Some(defaults) = rows.first().cloned() else { return Vec::new() };
    let field = |r: &HashMap<String, String>, k: &str| r.get(k).filter(|v| !v.is_empty()).or_else(|| defaults.get(k).filter(|v| !v.is_empty())).cloned().unwrap_or_default();
    let num = |r: &HashMap<String, String>, k: &str, d: f32| field(r, k).trim().parse::<f32>().unwrap_or(d);
    rows.iter()
        .skip(1)
        .map(|r| {
            let mut rule = SocialRule {
                commodity: field(r, "GC"),
                result: field(r, "Result"),
                speech_act: field(r, "SpAct").replace("None", ""),
                third_party: num(r, "ThP", 0.0) as u8,
                stc_positive: num(r, "STCPos", 0.0) as i8,
                st_commodity: field(r, "STCom"),
                st_context: field(r, "STContext"),
                mood: num(r, "Mood", -101.0),
                relationship: field(r, "LTRelationship"),
                liking: (num(r, "LTRMin", -100.0), num(r, "LTRMax", 100.0)),
                actor_traits: list(Some(&field(r, "AcTr"))),
                target_traits: list(Some(&field(r, "TaTr"))),
                skill: field(r, "Skill"),
                skill_level: num(r, "SkillLevel", 1.0),
                actor_increased: field(r, "AIE"),
                actor_reduced: field(r, "ARE"),
                target_increased: field(r, "TIE"),
                target_reduced: field(r, "TRE"),
                proc_test: field(r, "Proc"),
                partner: num(r, "Partner", 0.0) as i8,
                wrong_gender: num(r, "WrongGender", 0.0) as i8,
                repetition: num(r, "Repetition", 0.0) as i8,
                betrayed: num(r, "TargetBetrayed", 0.0) as i8,
                actor_ages: list(Some(&field(r, "ActorAge"))),
                target_ages: list(Some(&field(r, "TargetAge"))),
                ltr_override: num(r, "LTROverride", 0.0),
                spec: num(r, "Spec", 0.0),
            };
            if rule.spec == 0.0 {
                rule.spec = rule_specificity(&rule, true);
            }
            rule
        })
        .collect()
}

/// A social's own rules (its `<LHS>`) and outcomes (its `<RHS>`), from its `<Rules>`.
fn social_rules(body: &str) -> (Vec<SocialRule>, Vec<SocialOutcome>) {
    let Some(rules) = body.find("<Rules>").map(|s| &body[s..]) else { return (Vec::new(), Vec::new()) };
    let mut out = Vec::new();
    let mut rest = rules;
    while let Some(a) = rest.find("<LHS ") {
        let head_end = rest[a..].find('>').map_or(rest.len(), |e| a + e);
        let head = &rest[a..head_end];
        let end = if head.ends_with('/') { head_end } else { rest[head_end..].find("</LHS>").map_or(rest.len(), |e| head_end + e) };
        let inner = &rest[head_end..end];
        rest = &rest[end..];
        // (`~` before what must not be so: the tables' `is`/`val` flags false.)
        let flagged = |v: Option<&str>, is: Option<&str>| match v.filter(|v| !v.is_empty() && *v != "Undefined" && *v != "None") {
            Some(v) if is == Some("True") => v.to_string(),
            Some(v) => format!("~{v}"),
            None => String::new(),
        };
        let lltr = tag(inner, "LLTR");
        let sc = tag(inner, "SC");
        let sk = tag(inner, "SK");
        let se = tag(inner, "SE");
        let age = tag(inner, "AGE");
        let traits = tag(inner, "Trait");
        let mb = tag(inner, "MAYBE");
        let num = |t: Option<&str>, k: &str, d: f32| t.and_then(|t| attr(t, k)).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
        let effect = |k: &str| se.and_then(|t| attr(t, k)).filter(|v| *v != "None").unwrap_or("").to_string();
        let mut rule = SocialRule {
            result: attr(head, "com").unwrap_or("").to_string(),
            speech_act: attr(head, "SA").filter(|v| *v != "None").unwrap_or("").to_string(),
            third_party: attr(head, "TP").and_then(|v| v.parse().ok()).unwrap_or(0),
            stc_positive: attr(head, "cp").and_then(|v| v.parse().ok()).unwrap_or(0),
            st_commodity: flagged(sc.and_then(|t| attr(t, "com")), sc.and_then(|t| attr(t, "is"))),
            st_context: flagged(sc.and_then(|t| attr(t, "stc")), sc.and_then(|t| attr(t, "val"))),
            mood: num(lltr, "mood", -101.0),
            relationship: flagged(lltr.and_then(|t| attr(t, "key")), lltr.and_then(|t| attr(t, "is"))),
            liking: (num(lltr, "min", -100.0), num(lltr, "max", 100.0)),
            actor_traits: list(traits.and_then(|t| attr(t, "x"))),
            target_traits: list(traits.and_then(|t| attr(t, "y"))),
            skill: match sk.and_then(|t| attr(t, "key")).filter(|v| !v.is_empty() && *v != "None") {
                Some(k) if sk.and_then(|t| attr(t, "is")) == Some("True") => k.to_string(),
                Some(k) => format!("~{k}"),
                None => String::new(),
            },
            skill_level: num(sk, "val", -1.0),
            actor_increased: effect("x"),
            actor_reduced: effect("remX"),
            target_increased: effect("y"),
            target_reduced: effect("remY"),
            proc_test: attr(head, "ProcTest").unwrap_or("").to_string(),
            partner: maybe(mb.and_then(|t| attr(t, "part"))),
            wrong_gender: maybe(mb.and_then(|t| attr(t, "wgp"))),
            repetition: maybe(mb.and_then(|t| attr(t, "rep"))),
            betrayed: maybe(mb.and_then(|t| attr(t, "tb"))),
            actor_ages: list(age.and_then(|t| attr(t, "x"))),
            target_ages: list(age.and_then(|t| attr(t, "y"))),
            ltr_override: attr(head, "ltro").and_then(|v| v.parse().ok()).unwrap_or(0.0),
            spec: attr(head, "val").and_then(|v| v.parse().ok()).unwrap_or(0.0),
            commodity: String::new(),
        };
        if rule.spec == 0.0 {
            rule.spec = rule_specificity(&rule, false);
        }
        out.push(rule);
    }
    let mut outcomes = Vec::new();
    let mut rest = rules;
    while let Some(a) = rest.find("<RHS ") {
        let head_end = rest[a..].find('>').map_or(rest.len(), |e| a + e);
        let head = &rest[a..head_end];
        let end = rest[head_end..].find("</RHS>").map_or(rest.len(), |e| head_end + e);
        let inner = &rest[head_end..end];
        rest = &rest[end..];
        let hex = |v: Option<&str>| v.and_then(|v| u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok()).unwrap_or(0);
        let bits = tag(inner, "LTRBits");
        outcomes.push(SocialOutcome {
            result: attr(head, "com").unwrap_or("").to_string(),
            amount: tag(inner, "TUN").and_then(|t| attr(t, "val")).and_then(|v| v.parse().ok()).unwrap_or(40.0),
            bits_add: hex(bits.and_then(|t| attr(t, "val"))),
            bits_remove: hex(bits.and_then(|t| attr(t, "rem"))),
            charisma: tag(inner, "TUN").and_then(|t| attr(t, "charInc")).and_then(|v| v.parse().ok()).unwrap_or(-1.0),
            learn_random: tag(inner, "TUN").and_then(|t| attr(t, "is")).is_some_and(|v| v.eq_ignore_ascii_case("true")),
            teaches: ["x", "y"].map(|k| tag(inner, "Trait").and_then(|t| attr(t, k)).unwrap_or("").to_string()),
            topics: ["x", "y", "teo"].map(|k| tag(inner, "Topic").and_then(|t| attr(t, k)).unwrap_or("").to_string()),
        });
    }
    (out, outcomes)
}

/// The short-term contexts (the `Socializing` table's `<STC>` rows; the first is the defaults).
pub fn stc_infos(xml: &str) -> Vec<StcInfo> {
    let rows = records(xml, "STC");
    rows.iter()
        .skip(1)
        .filter(|r| r.get("Commodity").is_some_and(|c| !c.is_empty()))
        .map(|r| {
            let num = |k: &str| r.get(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
            StcInfo {
                name: r.get("Name").cloned().unwrap_or_default(),
                commodity: r.get("Commodity").cloned().unwrap_or_default(),
                min: num("RangeMin"),
                max: num("RangeMax"),
                x2: num("X2"),
                x3: num("X3"),
                y: [num("Y1"), num("Y2"), num("Y3")],
                decay_above: num("LTRDecayFromAbove"),
                decay_below: num("LTRDecayFromBelow"),
                mood_to_accept: num("MoodThresholdForAcceptRequest"),
                charisma: num("CharismaIncrease"),
            }
        })
        .collect()
}

/// How an object breaks and is repaired, as the game tunes each class (its `RepairableComponent`
/// tuning): unbreakable that many hours once new or repaired, then a chance a use (per cent)
/// growing with each time it's broken; the repair's complexity (the Handiness it takes to do it
/// quickest and safest), its time (game minutes, at no skill and at that skill), and the chance
/// of a shock (per cent, likewise), and whether standing in water means one.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RepairTuning {
    /// The class it's for (`TVCheap`, `ToiletCheap`...).
    pub class: String,
    pub hours_unbreakable: f32,
    pub start_break: f32,
    pub break_increase: f32,
    pub complexity: f32,
    pub min_time: f32,
    pub max_time: f32,
    pub min_shock: f32,
    pub max_shock: f32,
    pub shock_in_puddle: bool,
    /// Handiness points for mending it from broken to whole (`RepairSkillGain`), and for its
    /// never-breaks upgrade (`UpgradeUnbreakable`'s `SkillGainRate`).
    #[serde(default)]
    pub repair_skill: f32,
    #[serde(default)]
    pub upgrade_skill: f32,
}

/// An object class's repair tuning, from its tuning XML (the first of each value; none when it
/// doesn't break).
pub fn repair_tuning(class: &str, xml: &str) -> Option<RepairTuning> {
    let val = |k: &str| -> Option<&str> {
        let at = xml.find(&format!("<{k} value=\""))? + k.len() + 9;
        xml[at..].split('"').next()
    };
    let num = |k: &str| val(k).and_then(|v| v.trim().parse::<f32>().ok());
    let (start, inc) = (num("StartPercentChanceBreak")?, num("PercentChanceBreakIncrease").unwrap_or(0.0));
    if start == 0.0 && inc == 0.0 {
        return None;
    }
    Some(RepairTuning {
        class: class.to_string(),
        hours_unbreakable: num("HoursUnbreakable").unwrap_or(24.0),
        start_break: start,
        break_increase: inc,
        complexity: num("RepairComplexity").unwrap_or(1.0),
        min_time: num("MinRepairTime").unwrap_or(30.0),
        max_time: num("MaxRepairTime").unwrap_or(90.0),
        min_shock: num("MinChanceOfElectrocution").unwrap_or(0.0),
        max_shock: num("MaxChanceOfElectrocution").unwrap_or(0.0),
        shock_in_puddle: val("ShouldShockInPuddle").is_some_and(|v| v.eq_ignore_ascii_case("true")),
        repair_skill: num("RepairSkillGain").unwrap_or(1000.0),
        upgrade_skill: xml.find("<UpgradeUnbreakable>").and_then(|at| {
            let rest = &xml[at..];
            let rest = &rest[rest.find("<SkillGainRate value=\"")? + 22..];
            rest.split('"').next()?.trim().parse::<f32>().ok()
        }).unwrap_or(3600.0),
    })
}

/// How a class of stove or grill cooks (its tuning): food points it adds to what's cooked on it
/// (`ApplianceBonusFoodPoints`), how much likelier the food is to burn there
/// (`ChanceOfRandomFailureMultiplier`), and the chance (per cent) burning sets it alight
/// (`ChanceOfRandomFireAfterFailure`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ApplianceCooking {
    pub class: String,
    pub bonus_points: f32,
    pub failure: f32,
    pub fire: f32,
}

/// A stove's or grill's cooking, from its class's tuning; none when it isn't one.
pub fn appliance_cooking(class: &str, xml: &str) -> Option<ApplianceCooking> {
    let num = |k: &str| -> Option<f32> {
        let at = xml.find(&format!("<{k} value=\""))? + k.len() + 9;
        xml[at..].split('"').next()?.trim().parse().ok()
    };
    Some(ApplianceCooking {
        class: class.to_string(),
        bonus_points: num("ApplianceBonusFoodPoints")?,
        failure: num("ChanceOfRandomFailureMultiplier").unwrap_or(1.0),
        fire: num("ChanceOfRandomFireAfterFailure").unwrap_or(5.0),
    })
}

/// What one of the game's interactions does to needs on a class of object (its interaction
/// tuning, ITUN): each need's `ContinuousFlow` change an hour and its `ImmediateDelta` once,
/// in the order hunger, bladder, energy, social, hygiene, fun.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct InteractionRates {
    /// The object's class (`TVCheap`) and the interaction (`WatchTV`).
    pub class: String,
    pub interaction: String,
    pub per_hour: [f32; 6],
    pub once: [f32; 6],
}

/// The game's interaction tuning (an ITUN's XML) as need changes; none when it changes none.
pub fn interaction_rates(xml: &str) -> Option<InteractionRates> {
    let attr = |tag: &str| -> Option<&str> {
        let at = xml.find(&format!("<{tag} name=\""))? + tag.len() + 8;
        xml[at..].split('"').next()
    };
    // (`...Electronics.TV+WatchTV+Definition` or `...Electronics.PlayComputerGames+Definition`:
    // the interaction's own name; the object's class last of its namespace.)
    let interaction = attr("Interaction")?.trim_end_matches("+Definition").rsplit(['+', '.']).next()?.to_string();
    let class = attr("Object")?.rsplit('.').next()?.to_string();
    let mut r = InteractionRates { class, interaction, ..Default::default() };
    let mut any = false;
    for change in xml.split("<Change ").skip(1) {
        let val = |k: &str| -> Option<&str> {
            let at = change.find(&format!(" {k}=\""))? + k.len() + 3;
            change[at..].split('"').next()
        };
        let i = match change.split('"').nth(1) {
            Some("Hunger") => 0,
            Some("Bladder") => 1,
            Some("Energy") => 2,
            Some("Social") => 3,
            Some("Hygiene") => 4,
            Some("Fun") => 5,
            _ => continue,
        };
        let Some(v) = val("actual").and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| *v != 0.0) else { continue };
        match val("updateType") {
            Some("ContinuousFlow") => r.per_hour[i] = v,
            Some("ImmediateDelta") => r.once[i] = v,
            _ => continue,
        }
        any = true;
    }
    any.then_some(r)
}

/// One of the game's conversation topics (its `ActiveTopic` rows): how long it lasts once a Sim
/// has it (game hours; 0 as long as what gives it), whether others talk to the Sim about it, and
/// whether a social under it is done only once between two Sims.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TopicInfo {
    pub name: String,
    pub hours: f32,
    pub for_other: bool,
    pub used_up: bool,
}

/// The game's conversation topics (its `SocializingActionAvailability` table's `ActiveTopic`
/// rows; the first row's values stand for any left out).
pub fn topic_infos(xml: &str) -> Vec<TopicInfo> {
    let field = |row: &str, name: &str| -> Option<String> {
        let (open, close) = (format!("<{name}>"), format!("</{name}>"));
        let a = row.find(&open)? + open.len();
        let b = row[a..].find(&close)?;
        Some(row[a..a + b].trim().to_string()).filter(|v| !v.is_empty())
    };
    let mut out = Vec::new();
    let mut defaults: Option<(f32, bool, bool)> = None;
    let mut rest = xml;
    while let Some(a) = rest.find("<ActiveTopic>") {
        let Some(b) = rest[a..].find("</ActiveTopic>") else { break };
        let row = &rest[a..a + b];
        rest = &rest[a + b..];
        let num = |k: &str| field(row, k).and_then(|v| v.parse::<f32>().ok());
        let d = *defaults.get_or_insert((num("Duration").unwrap_or(0.0), num("AvailableForOtherSim") == Some(1.0), num("UsedUpAfterDoneOnce") == Some(1.0)));
        let Some(name) = field(row, "Name") else { continue };
        out.push(TopicInfo {
            name,
            hours: num("Duration").unwrap_or(d.0),
            for_other: num("AvailableForOtherSim").map_or(d.1, |v| v == 1.0),
            used_up: num("UsedUpAfterDoneOnce").map_or(d.2, |v| v == 1.0),
        });
    }
    out
}

/// Whether a trait the game's socials ask for is the robots' own (their chips and modes), which
/// it asks only of them.
fn bot_trait(t: &str) -> bool {
    t.ends_with("Chip") || t.starts_with("Bot") || t == "RobotHiddenTrait" || t == "CareerCashBooster"
}

/// The game's topics and the socials each offers (its `SocializingActionAvailability` table's
/// `TAA` rows, in order: a `Cat` names a topic, then its `FPA`s are offered to the Sim with it and
/// its `SPA`s to others with them).
pub fn social_topics(xml: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let mut topic = String::new();
    let field = |row: &str, name: &str| -> Option<String> {
        let (open, close) = (format!("<{name}>"), format!("</{name}>"));
        let a = row.find(&open)? + open.len();
        let b = row[a..].find(&close)?;
        Some(row[a..a + b].trim().to_string()).filter(|v| !v.is_empty())
    };
    let mut rest = xml;
    while let Some(a) = rest.find("<TAA>") {
        let Some(b) = rest[a..].find("</TAA>") else { break };
        let row = &rest[a..a + b];
        rest = &rest[a + b..];
        if let Some(cat) = field(row, "Cat") {
            topic = cat;
            continue;
        }
        if topic.is_empty() {
            continue;
        }
        if let Some(f) = field(row, "FPA") {
            out.push((topic.clone(), f, true));
        }
        if let Some(s) = field(row, "SPA") {
            out.push((topic.clone(), s, false));
        }
    }
    out
}

/// An attribute's value in a tag's text (`key="..."`).
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    let len = tag[at..].find('"')?;
    Some(&tag[at..at + len])
}

/// The text of the first `<name .../>` tag in a body.
fn tag<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let a = body.find(&format!("<{name} "))?;
    let b = body[a..].find('>')?;
    Some(&body[a..a + b])
}

fn list(v: Option<&str>) -> Vec<String> {
    v.map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}

/// The social interactions in one of the game's `SocialData` tables, named in English by `name`
/// (their text keys).
pub fn social_actions(xml: &str, name: &dyn Fn(&str) -> String) -> Vec<SocialAction> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(a) = rest.find("<Action ") {
        let head_end = rest[a..].find('>').map_or(rest.len(), |e| a + e);
        let head = &rest[a..head_end];
        let Some(close) = rest[head_end..].find("</Action>") else { break };
        let body = &rest[head_end..head_end + close];
        rest = &rest[head_end + close..];
        let Some(key) = attr(head, "key") else { continue };
        // (Its availability: `<AA>`, or with attributes, `<AA oaend="1">`.)
        let aa = body.find("<AA>").or_else(|| body.find("<AA ")).and_then(|s| body[s..].find("</AA>").map(|e| &body[s..s + e])).unwrap_or("");
        let age = tag(aa, "AGE");
        let available = tag(aa, "Available");
        let graph = tag(body, "Graph");
        let text = tag(body, "Text").and_then(|t| attr(t, "key")).unwrap_or(key);
        let liking = tag(aa, "ALTR").and_then(|t| Some((attr(t, "min")?.parse().ok()?, attr(t, "max")?.parse().ok()?)));
        let (rules, outcomes) = social_rules(body);
        let misc = tag(body, "Misc");
        out.push(SocialAction {
            key: key.to_string(),
            name: Some(name(text)).filter(|n| !n.is_empty()).unwrap_or_else(|| key.to_string()),
            tone: attr(head, "com").unwrap_or("").to_string(),
            actor_ages: list(age.and_then(|t| attr(t, "x"))),
            target_ages: list(age.and_then(|t| attr(t, "y"))),
            autonomous: available.and_then(|t| attr(t, "auto")) == Some("True"),
            user_only: available.and_then(|t| attr(t, "udo")) == Some("True"),
            target_traits: list(tag(aa, "AKTT").and_then(|t| attr(t, "val"))),
            keen: list(tag(aa, "ATE").and_then(|t| attr(t, "val"))),
            restrict: list(tag(aa, "ATR").and_then(|t| attr(t, "val"))),
            restrict_auto: list(tag(aa, "ATRA").and_then(|t| attr(t, "val"))),
            required: list(tag(aa, "ATREQ").and_then(|t| attr(t, "val"))).into_iter().filter(|t| !bot_trait(t)).collect(),
            liking,
            test: tag(aa, "ProcTest").and_then(|t| attr(t, "key")).unwrap_or("").to_string(),
            graph: graph.and_then(|t| attr(t, "val")).unwrap_or("").to_string(),
            suite: graph.and_then(|t| attr(t, "suite")).unwrap_or("").to_string(),
            topics: Vec::new(),
            speech_act: tag(body, "SA").and_then(|t| attr(t, "key")).filter(|v| *v != "None").unwrap_or("").to_string(),
            chances: misc.and_then(|t| attr(t, "c")).and_then(|v| v.parse().ok()).unwrap_or(2),
            recharges: misc.and_then(|t| attr(t, "r")).and_then(|v| v.parse().ok()).unwrap_or(2),
            jealousy: misc.and_then(|t| attr(t, "JL")).map_or(0, |v| match v {
                "Low" => 1,
                "Medium" => 2,
                "High" => 3,
                _ => 0,
            }),
            rules,
            outcomes,
        });
    }
    out
}

/// One of the game's idle animations (its `Idles` tuning): its clips (a start, a loop and a stop,
/// or one), how likely it is among its fellows, the ages it's for (the table's letters: C, T,
/// Y, A, E, and the pets'), and the thought balloon list it may show.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct IdleAnim {
    pub clips: Vec<String>,
    pub probability: u32,
    pub ages: Vec<String>,
    pub balloon: String,
    /// Only for Sims with this trait (the game's trait name; empty: anyone).
    #[serde(default)]
    pub trait_required: String,
}

/// The game's idle tables: the everyday standing idles, and those by mood flavour (`Sad`,
/// `Happy`...), trait (`Neurotic`...) and moodlet (`Hungry`, `HasToPee`...).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct IdleTables {
    /// The facial expressions by kind (`FacialHappy`, `FacialSad`...): each clip and its weight.
    #[serde(default)]
    pub faces: Vec<(String, Vec<(String, f32)>)>,
    /// The reactions (`Shocked`, `SmellsBad`, `PointLaugh`, `Cry`...): each one's clips.
    #[serde(default)]
    pub reactions: Vec<(String, Vec<IdleAnim>)>,
    pub standing: Vec<IdleAnim>,
    pub mood: Vec<(String, Vec<IdleAnim>)>,
    pub traits: Vec<(String, Vec<IdleAnim>)>,
    pub buffs: Vec<(String, Vec<IdleAnim>)>,
}

/// An idle table's rows, grouped under the row that names them (`key`; the rows after it
/// without a name are more of its idles). The first row holds the defaults the others leave
/// out. With no `key`, one group.
fn idle_rows(xml: &str, row: &str, key: &str) -> Vec<(String, Vec<IdleAnim>)> {
    let rows = records(xml, row);
    let Some(defaults) = rows.first().cloned() else { return Vec::new() };
    let field = |r: &HashMap<String, String>, k: &str| r.get(k).filter(|v| !v.is_empty()).or_else(|| defaults.get(k)).cloned().unwrap_or_default();
    let mut out: Vec<(String, Vec<IdleAnim>)> = Vec::new();
    if key.is_empty() {
        out.push((String::new(), Vec::new()));
    }
    for r in rows.iter().skip(1) {
        let name = if key.is_empty() { String::new() } else { r.get(key).cloned().unwrap_or_default() };
        if !name.is_empty() {
            out.push((name, Vec::new()));
        }
        let anim = r.get("AnimationName").cloned().unwrap_or_default();
        let Some(group) = out.last_mut() else { continue };
        // (Create a Sim's alone: the trait's preview there.)
        if anim.is_empty() || field(r, "CASOnly") == "True" {
            continue;
        }
        let trait_required = field(r, "TraitRequired");
        // (A looping one, named by its state machine (`wedding_genericClap`, "Looping, 15, 30"):
        // its clips, `a_wedding_genericClap_start_x`, the loop a few times, the stop.)
        let looping = r.get("AnimationType").is_some_and(|t| t.starts_with("Looping")) && !anim.contains(',') && !anim.starts_with("a_");
        let clips: Vec<String> = if looping {
            let n = anim.trim();
            vec![format!("a_{n}_start_x"), format!("a_{n}_loop_x"), format!("a_{n}_loop_x"), format!("a_{n}_loop_x"), format!("a_{n}_stop_x")]
        } else {
            anim.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };
        group.1.push(IdleAnim {
            clips,
            probability: field(r, "AnimationProbability").parse().unwrap_or(1),
            ages: field(r, "AgesSpeciesAllowed").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            balloon: r.get("ThoughtBalloon").cloned().unwrap_or_default(),
            trait_required: if trait_required == "Unknown" { String::new() } else { trait_required },
        });
    }
    out.retain(|g| !g.1.is_empty());
    out
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BalloonTable {
    pub idle: HashMap<String, Vec<BalloonEntry>>,
    pub social: HashMap<String, Vec<BalloonEntry>>,
    pub topic: HashMap<String, Vec<BalloonEntry>>,
    pub random: HashMap<String, Vec<BalloonEntry>>,
}

impl BalloonTable {
    /// A list by key from any of the tables (the order references are looked up in).
    pub fn list(&self, key: &str) -> Option<&Vec<BalloonEntry>> {
        self.idle.get(key).or_else(|| self.topic.get(key)).or_else(|| self.random.get(key)).or_else(|| self.social.get(key))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GameDataBaked {
    pub version: u32,
    /// The idle animations Sims play standing about (the game's `Idles`): by trait, moodlet and
    /// mood, and the everyday ones.
    #[serde(default)]
    pub idles: IdleTables,
    /// The game's social interactions (its `SocialData` tables).
    #[serde(default)]
    pub socials: Vec<SocialAction>,
    /// The conversation topics they're offered under.
    #[serde(default)]
    pub social_topics: Vec<TopicInfo>,
    /// How each class of object breaks and is repaired.
    #[serde(default)]
    pub repairs: Vec<RepairTuning>,
    /// What the game's interactions do to needs on each class of object, and how much faster
    /// each class of bed restores energy (its `EnergyMultiplier`).
    #[serde(default)]
    pub interaction_rates: Vec<InteractionRates>,
    #[serde(default)]
    pub energy_multipliers: Vec<(String, f32)>,
    /// What each class of object adds to the look of a room (its `EnvironmentScore`).
    #[serde(default)]
    pub environment_scores: Vec<(String, f32)>,
    /// How each class of stove and grill cooks.
    #[serde(default)]
    pub appliance_cooking: Vec<ApplianceCooking>,
    /// The game's general social rules and its short-term contexts.
    #[serde(default)]
    pub general_rules: Vec<SocialRule>,
    #[serde(default)]
    pub stcs: Vec<StcInfo>,
    /// The socials offered in each context and relationship.
    #[serde(default)]
    pub social_menus: Vec<SocialMenu>,
    pub buffs: Vec<BuffInfo>,
    pub traits: Vec<TraitInfo>,
    pub skills: Vec<SkillInfo>,
    pub careers: Vec<CareerInfo>,
    pub patterns: Vec<PatternInfo>,
    pub balloons: BalloonTable,
    pub opportunities: Vec<OpportunityInfo>,
    pub plants: Vec<PlantInfo>,
    pub roofs: Vec<RoofPattern>,
    pub collectibles: Vec<CollectibleInfo>,
    pub spawners: Vec<SpawnerInfo>,
    /// The build catalogue's fences.
    pub fences: Vec<FenceStyle>,
    /// Each object's effect slots (model space): where showers spray, fountains gush and
    /// fires burn.
    pub fx_slots: Vec<(crate::types::Key, Vec<[f32; 3]>)>,
    /// Each object's routing slots (model space): where Sims stand to use it, by slot number,
    /// and the way they face.
    pub route_slots: Vec<(crate::types::Key, Vec<(u8, [f32; 3], [f32; 3])>)>,
    pub lifetime_wishes: Vec<LifetimeWishInfo>,
    /// The Writing skill's tuning (`kLengthRomanceMin`, `kRoyaltyRomanceMax`, `kRateBasePPM`,
    /// `kQualityLevel5ChanceHit`...).
    pub writing: HashMap<String, f32>,
    /// Titles for the books Sims write, by genre (`Fiction`, `SciFi`, `Romance`...).
    pub book_titles: Vec<(String, Vec<String>)>,
    /// The skill books (`Books`' `BookSkill` rows).
    #[serde(default)]
    pub skill_books: Vec<SkillBookInfo>,
    pub recipes: Vec<RecipeInfo>,
    /// The Seasons tuning (`Seasons` table and SeasonsManager's managers).
    pub seasons: SeasonsTuning,
    /// The catalogue's trees, shrubs and flowers drawn by SpeedTree (no models of their own):
    /// each OBJD with its species' billboard, to plant in Build mode.
    pub garden: Vec<(crate::types::Key, crate::types::TreeKindBaked)>,
    /// The catalogue objects that can be stolen (their OBJDs' flag), by OBJD instance, sorted.
    #[serde(default)]
    pub stealable: Vec<u64>,
    /// The ingredients, of the kinds there are (the game's `ItemTypes` tree).
    #[serde(default)]
    pub ingredients: Vec<IngredientInfo>,
}

/// An ingredient: a base game row of `ingredients`, or a fish of `Fishing`. What kind of thing it
/// is (the game's `ItemTypes`, by `Parent`: a Tomato a Vegetable, a Minnow a Fish), and what
/// the grocery asks for it.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct IngredientInfo {
    /// `Tomato`, `Hot Dogs`, `Minnow`; or a kind of them (`Vegetable`, `Fish`, `Ingredient`).
    pub key: String,
    pub name: String,
    /// The kind it's of (`ROOT` at the top).
    pub parent: String,
    /// A kind of ingredient, not one in itself.
    pub kind: bool,
    pub price: i32,
    /// Sold at the grocery.
    pub can_buy: bool,
    /// A plant's produce: the grocery asks its quality's price for it.
    pub grown: bool,
    /// The grocery's tab for it (`Meat`, `Produce`, `Fish`).
    pub tab: String,
}

/// The seasons' weather, from the game's `Seasons` table: temperatures through the day, how
/// quickly snow melts or freezes, and each kind of weather's chances; plus the ground cover
/// tuning (`SeasonsManager+TerrainCoverManager`). Seasons are `Summer`, `Fall`, `Winter`,
/// `Spring`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SeasonsTuning {
    /// Each season's temperatures (°F), morning, noon, evening and night, each {min, max}.
    pub temperature: Vec<(String, [[f32; 2]; 4])>,
    /// {temperature (°F), rate per hour}: negative freezes (ponds), positive melts (snow).
    pub freeze_melt: Vec<[f32; 2]>,
    pub weather: Vec<WeatherProfile>,
    /// `TerrainCoverManager` values (`kSnowAccumulationTime`, `kRainDryTime`...).
    pub cover: HashMap<String, Vec<f32>>,
}

/// A kind of weather in a season: `Sunny`, `Rain`, `Snow`, `Fog`, `Hail`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WeatherProfile {
    pub kind: String,
    pub season: String,
    pub weight: f32,
    /// How long it lasts (hours).
    pub min_length: f32,
    pub max_length: f32,
    /// The temperatures it comes at (°F).
    pub min_temp: f32,
    pub max_temp: f32,
    /// Light, moderate and heavy (precipitation).
    pub intensity: [f32; 3],
}

/// Meal times a recipe is cooked for (`RecipeInfo::meals`).
pub const MEAL_BREAKFAST: u8 = 1;
pub const MEAL_BRUNCH: u8 = 2;
pub const MEAL_LUNCH: u8 = 4;
pub const MEAL_DINNER: u8 = 8;
pub const MEAL_DESSERT: u8 = 16;

/// A recipe: a base game row of `RecipeMasterList` that's cooked as a meal, or one of its
/// snacks (the fridge's "Have Quick Meal..." menu: cereal, juice...), after the meals.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RecipeInfo {
    /// `Spaghetti`, `GoopyCarbonara`...
    pub key: String,
    pub name: String,
    /// The Cooking level it takes.
    pub level: u8,
    /// Known once a Sim reaches its level (the rest are learned from recipe books).
    pub auto: bool,
    /// The meal times it's for (`MEAL_*` bits).
    pub meals: u8,
    pub vegetarian: bool,
    /// What it costs at a restaurant.
    pub cost: i32,
    pub ingredients: Vec<String>,
    /// The serving dish full, half-full and emptied, and a plateful and the empty plate: models in
    /// `food.pack` (each key's group is its geometry state's hash).
    pub group: Option<crate::types::Key>,
    pub group_half: Option<crate::types::Key>,
    pub group_empty: Option<crate::types::Key>,
    /// What its recipe book costs at the bookstore (0: there's none).
    pub book_price: i32,
    pub single: Option<crate::types::Key>,
    pub single_half: Option<crate::types::Key>,
    pub single_empty: Option<crate::types::Key>,
    /// A snack, taken out of the fridge rather than cooked (no meal times of its own).
    #[serde(default)]
    pub snack: bool,
    /// Heated in a microwave (the Microwave Meal).
    #[serde(default)]
    pub microwave: bool,
    /// Whether it's made for one ("Have Dinner...") and for a group ("Serve Dinner...").
    #[serde(default)]
    pub for_one: bool,
    #[serde(default)]
    pub for_group: bool,
}

/// A lifetime wish: one of the base game's "Lifetime Dreams" (`DreamsAndPromisesNodes`), with
/// its fulfillment score from `DreamNodeInstanceDefaults`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LifetimeWishInfo {
    pub id: u32,
    /// The game's check for it, e.g. `NSimoleonsInCashMajorDreamCheckFunction` (empty for a
    /// few that are fulfilled by an event instead).
    pub check: String,
    /// The number it asks for (simoleons, friends, career level), when it takes one.
    pub number: f32,
    pub icon: String,
    /// Fulfillment score: a tenth of the lifetime happiness it's worth.
    pub score: u32,
}

/// What kind of find a collectible is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CollectKind {
    Gem,
    Metal,
    SpaceRock,
    Butterfly,
    Beetle,
    Fish,
}

/// Something to find about town: a gem, metal, space rock, insect or fish (base game rows of the
/// `RockGemMetal`, `Insects` and `Fishing` tables).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CollectibleInfo {
    /// The tables' key ("Ruby", "ButterflyMoth", "Minnow").
    pub key: String,
    pub name: String,
    pub kind: CollectKind,
    pub min_price: i32,
    pub max_price: i32,
    pub rarity: String,
    /// The skill level needed to find it (fishing for fish).
    pub level: u8,
    /// The catalogue object showing it (by instance name): "Gem", "Metal", "MeteorMedium",
    /// "fishMinnow"...
    pub model: String,
    /// A fish's preferred bait: the ingredient or fish it bites on most ("Apple", "Minnow").
    #[serde(default)]
    pub bait: String,
    /// A fish's weights (lb), lightest to heaviest, and whether one can grow heavier still.
    #[serde(default)]
    pub weight: (f32, f32),
    #[serde(default)]
    pub over_max: bool,
}

/// What a spawner (placed on lots by the world builders) turns up, by its script class.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SpawnerInfo {
    /// The spawner's class ("RockGemMetalSpawner11", "InsectSpawner3", "FishingSpawner6").
    pub class: String,
    /// Collectible keys with relative weights ("None" = nothing).
    pub items: Vec<(String, f32)>,
    /// How many can be lying about at once.
    pub capacity: u8,
    /// Hours between finds.
    pub hours: (f32, f32),
}

/// A roof pattern from the build catalogue.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoofPattern {
    pub name: String,
    /// The roof material's diffuse map: an atlas of the field of tiles with the ridge and hip
    /// pieces stamped over it.
    pub texture: crate::types::Key,
    /// Just the field of tiles (covering [`ROOF_ATLAS_METRES`] square), for repeating over a
    /// roof (see [`roof_tile`]).
    pub tile: crate::types::Key,
}

/// How much roof (metres) a roof atlas covers.
pub const ROOF_ATLAS_METRES: f32 = 3.0;

/// The plain field of tiles from a roof atlas (covering as much roof as the atlas). The field
/// repeats along one axis or both, and the ridge and hip pieces sit over parts of it, so the
/// median of every repeat of each pixel leaves just the tiles.
pub fn roof_tile(atlas: &s3formats::dds::Rgba) -> Option<s3formats::dds::Rgba> {
    let (w, h) = (atlas.width, atlas.height);
    if w < 64 || h < 64 {
        return None;
    }
    let lum: Vec<f32> = atlas.data.chunks_exact(4).map(|p| p[0] as f32 * 0.3 + p[1] as f32 * 0.59 + p[2] as f32 * 0.11).collect();
    // The repeat along an axis: the shift (16 px or more) where the atlas best matches itself,
    // if that's clearly better than shifts in general; with how much better (lower is clearer).
    let period = |horizontal: bool| -> Option<(usize, f32)> {
        let n = if horizontal { w } else { h };
        let diff = |p: usize| {
            let (mut sum, mut cnt) = (0.0f32, 0usize);
            let (xs, ys) = if horizontal { (w - p, h) } else { (w, h - p) };
            for y in (0..ys).step_by(2) {
                for x in (0..xs).step_by(2) {
                    let (x2, y2) = if horizontal { (x + p, y) } else { (x, y + p) };
                    sum += (lum[y * w + x] - lum[y2 * w + x2]).abs();
                    cnt += 1;
                }
            }
            sum / cnt.max(1) as f32
        };
        let diffs: Vec<f32> = (0..=n / 2).map(|p| if p >= 15 { diff(p) } else { 0.0 }).collect();
        let mut sorted: Vec<f32> = diffs[16..].to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let med = sorted[sorted.len() / 2].max(1e-3);
        (16..n / 2)
            .filter(|&p| diffs[p] <= diffs[p - 1] && diffs[p] <= diffs[p + 1] && diffs[p] < 0.85 * med)
            .min_by(|&a, &b| diffs[a].total_cmp(&diffs[b]))
            .map(|p| (p, diffs[p] / med))
    };
    let (mut px, mut py) = (period(true), period(false));
    // The clearer repeat, and the other only when it's clear too.
    match (px, py) {
        (Some(x), Some(y)) if x.1 <= y.1 && y.1 >= 0.8 => py = None,
        (Some(x), Some(y)) if y.1 < x.1 && x.1 >= 0.8 => px = None,
        _ => {}
    }
    let tx = px.map_or(w, |p| p.0);
    let ty = py.map_or(h, |p| p.0);
    let (kx, ky) = (w / tx, h / ty);
    let mut tile = vec![0u8; tx * ty * 4];
    let mut samples: Vec<u8> = Vec::with_capacity(kx * ky);
    for y in 0..ty {
        for x in 0..tx {
            for c in 0..4 {
                samples.clear();
                for j in 0..ky {
                    for i in 0..kx {
                        samples.push(atlas.data[((y + j * ty) * w + x + i * tx) * 4 + c]);
                    }
                }
                samples.sort_unstable();
                tile[(y * tx + x) * 4 + c] = samples[samples.len() / 2];
            }
        }
    }
    // Repeated over the atlas's area again, at 256 x 256.
    let (rx, ry) = (((w as f32 / tx as f32).round() as usize).max(1), ((h as f32 / ty as f32).round() as usize).max(1));
    let size = 256usize;
    let mut data = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let sx = (x * tx * rx / size) % tx;
            let sy = (y * ty * ry / size) % ty;
            let o = (y * size + x) * 4;
            data[o..o + 4].copy_from_slice(&tile[(sy * tx + sx) * 4..(sy * tx + sx) * 4 + 4]);
        }
    }
    Some(s3formats::dds::Rgba { width: size, height: size, data })
}


impl GameDataBaked {
    pub fn buff(&self, hex: &str) -> Option<&BuffInfo> {
        self.buffs.iter().find(|b| b.hex.eq_ignore_ascii_case(hex))
    }
    pub fn trait_info(&self, hex: &str) -> Option<&TraitInfo> {
        self.traits.iter().find(|t| t.hex.eq_ignore_ascii_case(hex))
    }
    pub fn skill(&self, hex: &str) -> Option<&SkillInfo> {
        self.skills.iter().find(|s| s.hex.eq_ignore_ascii_case(hex))
    }
}

pub fn gamedata_ready(root: &BakeRoot) -> bool {
    let g = root.global_dir();
    crate::PackReader::current(&g.join("icons.pack")) && read_value::<GameDataBaked>(&g.join("gamedata.bin")).is_ok_and(|d| d.version == GAMEDATA_VERSION)
}

pub fn load_gamedata(root: &BakeRoot) -> Option<GameDataBaked> {
    read_value(&root.global_dir().join("gamedata.bin")).ok().filter(|d: &GameDataBaked| d.version == GAMEDATA_VERSION)
}

/// The baked interface icons (PNG bytes by name).
pub struct Icons(PackReader);

impl Icons {
    pub fn open(root: &BakeRoot) -> Option<Self> {
        PackReader::open(&root.global_dir().join("icons.pack")).ok().map(Self)
    }
    /// The PNG of an icon, by name.
    pub fn png(&self, name: &str) -> Option<Vec<u8>> {
        self.0.get(&icon_key(name))
    }
}

pub fn icon_key(name: &str) -> crate::types::Key {
    (T_ICON, 0, s3pkg::fnv64(&name.to_ascii_lowercase()))
}

/// Decodes an icon's PNG into RGBA8 (width, height, pixels).
pub fn decode_icon(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = crate::bake::decode_png(png)?;
    Some((img.width as u32, img.height as u32, img.data))
}

/// The records of a tuning table: each `<row>` element's child tags and their text.
fn records(xml: &str, row: &str) -> Vec<HashMap<String, String>> {
    let (open, close) = (format!("<{row}>"), format!("</{row}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(a) = rest.find(&open) {
        let body_start = a + open.len();
        let Some(b) = rest[body_start..].find(&close) else { break };
        let body = &rest[body_start..body_start + b];
        let mut f = HashMap::new();
        let mut s = body;
        while let Some(lt) = s.find('<') {
            let Some(gt) = s[lt..].find('>') else { break };
            let tag = &s[lt + 1..lt + gt];
            if tag.starts_with('/') || tag.ends_with('/') || tag.starts_with('!') {
                s = &s[lt + gt + 1..];
                continue;
            }
            let after = &s[lt + gt + 1..];
            let end_tag = format!("</{tag}>");
            match after.find(&end_tag) {
                Some(e) => {
                    let v = after[..e].trim();
                    f.insert(tag.to_string(), unescape(v));
                    s = &after[e + end_tag.len()..];
                }
                None => s = after,
            }
        }
        out.push(f);
        rest = &rest[body_start + b + close.len()..];
    }
    out
}

/// A tuning file's current values: `<kName value="1.5">` → ("kName", 1.5).
fn tuning_values(xml: &str) -> HashMap<String, f32> {
    let cur = xml.split("<Current_Tuning>").nth(1).unwrap_or(xml);
    let cur = cur.split("</Current_Tuning>").next().unwrap_or(cur);
    let mut out = HashMap::new();
    let mut rest = cur;
    while let Some(i) = rest.find("<k") {
        rest = &rest[i + 1..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        let name = tag.split_whitespace().next().unwrap_or("");
        if let Some(v) = tag.split("value=\"").nth(1).and_then(|v| v.split('"').next()).and_then(|v| v.trim().parse::<f32>().ok()) {
            out.insert(name.to_string(), v);
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Converts the base game's buffs, traits and skills and their icons.
/// The careers' uniforms (`outfits.bin`): each outfit's parts with their designs rendered from
/// the outfit's presets into the texture store, and the meshes of parts the everyday wardrobe
/// doesn't have (`outfits.pack`). Formal wear that isn't everyday wear too (suits, tuxedos,
/// cocktail dresses) joins the wardrobe the same way (`wardrobe.bin`).
/// The face sliders every Sim's face is shaped with, in opposing pairs (a Sim leans one way or
/// the other on each, by their look). The game shapes faces by moving the face's bones: a
/// slider is one bone adjustment (BOND) per age and sex, `<age><sex>FaceDeformations<Slider>`.
pub const FACE_SLIDERS: [(&str, &str); 17] = [
    ("JawWide", "JawThin"),
    ("JawChinScaleUp", "JawChinScaleDown"),
    ("JawChinUp", "JawChinDown"),
    ("MouthWide", "MouthThin"),
    ("MouthOut", "MouthIn"),
    ("TranslateMouthUp", "TranslateMouthDown"),
    ("EyesScaleUp", "EyesScaleDown"),
    ("EyesApart", "EyesIn"),
    ("TranslateEyesUp", "TranslateEyesDown"),
    ("EyesBrowsUp", "EyesBrowsDown"),
    ("NoseScaleUp", "NoseScaleDown"),
    ("NoseWide", "NoseThin"),
    ("NoseUp", "NoseDown"),
    ("NoseTipScaleUp", "NoseTipScaleDown"),
    ("JawCheeksBoneUp", "JawCheeksBoneDown"),
    ("JawCheeksOut", "JawCheeksIn"),
    ("HeadWide", "HeadThin"),
];

/// The age-and-sex prefixes of the face sliders' bone adjustments.
pub const FACE_PREFIXES: [&str; 10] = ["am", "af", "ym", "yf", "em", "ef", "tm", "tf", "cu", "pu"];

/// One bone's adjustment by a face slider (at full strength).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct FaceBone {
    /// FNV-32 of the bone's name (lowercase).
    pub bone: u32,
    pub offset: [f32; 3],
    pub scale: [f32; 3],
    pub rotation: [f32; 4],
}

/// The face sliders' bone adjustments by age-and-sex prefix: per slider (index into the
/// flattened `FACE_SLIDERS`), its bones.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FaceBones {
    pub prefix: String,
    pub sliders: Vec<(u8, Vec<FaceBone>)>,
}

/// Eye colours, as the game draws them: the iris (a greyscale image, its alpha the iris's
/// shape) in the eye colour, at twice the image's shade times the colour, over the eyes' own
/// texture (every face's eyes have their iris in the same place); and Create a Sim's presets.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EyeColors {
    /// The iris (in the texture store).
    pub overlay: Option<crate::types::Key>,
    pub presets: Vec<[f32; 3]>,
}

/// Texture-store type of a catalogue object's designs: `(T_DESIGN, design, OBJD instance)`.
pub const T_DESIGN: u32 = 0x0DE5_1600;
/// How big designs are drawn (the longer side).
const DESIGN_SIZE: usize = 256;

/// A catalogue object's designs (the game's colour and pattern presets for it): how many drawn
/// (none when it comes in just the one), and the texture of its meshes designs stand in for
/// (the object's composited texture; the lots' furniture is in designs of its own too).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ObjectDesigns {
    pub objd: crate::types::Key,
    pub count: u8,
    pub texture: crate::types::Key,
    /// Each design's solid colour channels (A to D as 0 to 3) and their colours (what Create a
    /// Style changes), and the size designs are drawn at.
    #[serde(default)]
    pub channels: Vec<Vec<(u8, [f32; 3])>>,
    #[serde(default)]
    pub size: (u16, u16),
}

/// Every buyable object's designs, each drawn from its complate into the texture store (objects
/// whose meshes share one composited texture: the designs are recipes for it).
fn bake_object_designs(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    let g = root.global_dir();
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog.bin: {e}"))?;
    let models = PackReader::open(&g.join("models.pack")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.textures_dir()).ok();
    // (Every catalogue object, sold or not: the towns' lots use variants the catalogue doesn't
    // sell, in designs of their own.)
    let buyable: Vec<&crate::types::CatalogEntry> = catalog.iter().collect();
    // (DESIGN_DEBUG=<instance>: why an object has no designs.)
    if let Some(i) = std::env::var("DESIGN_DEBUG").ok().and_then(|v| v.parse::<u64>().ok()) {
        for c in catalog.iter().filter(|c| c.objd.2 == i) {
            let o = pkgs.read(&crate::types::rkey(c.objd)).and_then(|d| s3formats::object::parse_objd(&d).ok());
            let txtcs: Vec<_> = c.models.iter().filter_map(|m| models.get::<crate::types::BakedModel>(m)).flat_map(|m| m.parts.into_iter().map(|p| p.texture)).collect();
            eprintln!("design debug {}: price {} presets {:?} part textures {txtcs:?}", c.instance_name, c.price, o.map(|o| o.presets.len()));
        }
    }
    let out: Vec<ObjectDesigns> = crate::bake::par_map(&buyable, |c| {
        let o = s3formats::object::parse_objd(&pkgs.read(&crate::types::rkey(c.objd))?).ok()?;
        if o.presets.is_empty() {
            return None;
        }
        // The one composited texture the designs are recipes for.
        let mut txtcs: Vec<crate::types::Key> = c
            .models
            .iter()
            .filter_map(|m| models.get::<crate::types::BakedModel>(m))
            .flat_map(|m| m.parts.into_iter().filter_map(|p| p.texture))
            .filter(|t| t.0 == s3pkg::types::TXTC)
            .collect();
        txtcs.sort();
        txtcs.dedup();
        let [texture] = txtcs[..] else { return None };
        // (Drawn in the shape of that texture.)
        let (w, h) = pkgs
            .read(&crate::types::rkey(texture))
            .and_then(|d| s3formats::txtc::Txtc::parse(&d).ok())
            .map(|t| s3formats::compositor::Compositor::new(pkgs).output_size(&t))
            .filter(|(w, h)| *w > 0 && *h > 0)
            .unwrap_or((DESIGN_SIZE, DESIGN_SIZE));
        let scale = DESIGN_SIZE as f32 / w.max(h) as f32;
        let (w, h) = (((w as f32 * scale) as usize).max(16), ((h as f32 * scale) as usize).max(16));
        let mut count = 0u8;
        for (i, p) in o.presets.iter().enumerate().take(16).filter(|_| o.presets.len() > 1) {
            let key = (T_DESIGN, i as u32, c.objd.2);
            if !root.tex_path(key).exists() || s3formats::complate::uses_escaped_names(&p.complate) && crate::bake::drawn_before_names_fixed(&root.tex_path(key)) {
                let Some(img) = s3formats::complate::render(pkgs, &p.complate, &p.keys, w, h) else { break };
                let _ = std::fs::write(root.tex_path(key), crate::ddsw::encode_dds(&img));
            }
            count = i as u8 + 1;
        }
        let channels = o.presets.iter().take(16).map(|p| p.complate.solid_channels()).collect();
        Some(ObjectDesigns { objd: c.objd, count, texture, channels, size: (w as u16, h as u16) })
    })
    .into_iter()
    .flatten()
    .collect();
    let n = out.len();
    write_value(&g.join("object_designs.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// One of the game's painting pictures (a row of the Painting skill's picture table, base game).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PaintingInfo {
    /// The canvas: 0 small, 1 medium, 2 large.
    pub size: u8,
    /// The picture's name (its texture is the DDS of instance fnv64 of it).
    pub name: String,
    /// The Painting skill levels it's painted at.
    pub min: u8,
    pub max: u8,
    /// 0 an ordinary painting, 1 a dabble, 2 a brilliant painting, 3 a masterpiece.
    pub kind: u8,
    /// The versions painted by Sims of a trait: (the table's trait column, picture).
    pub traits: Vec<(String, String)>,
}

/// A canvas size: the finished painting's catalogue object, its model (drawn at its size) and
/// where on a picture's texture the painted face shows it (u0, v0, u1, v1).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CanvasInfo {
    pub objd: crate::types::Key,
    pub model: crate::types::Key,
    pub uv: [f32; 4],
}

/// The paintings Sims paint, and the canvases they're painted on (small, medium, large).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PaintingsBaked {
    pub paintings: Vec<PaintingInfo>,
    pub canvases: Vec<CanvasInfo>,
    /// The texture of the canvas's painted face (the picture stands in for it, as a design).
    pub face: crate::types::Key,
    /// Where each easel holds the canvas being painted.
    pub easels: Vec<EaselSlots>,
}

/// Where an easel holds a canvas (its rig's canvas slots, in its model space): the small one,
/// and the medium and large ones. (Translation, and rotation as x, y, z, w.)
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EaselSlots {
    pub objd: crate::types::Key,
    pub small: ([f32; 3], [f32; 4]),
    pub large: ([f32; 3], [f32; 4]),
}

/// A rig bone's place in the rig's (the object's model) space: its parents' transforms
/// composed with its own.
fn bone_transform(rig: &s3formats::sim::Rig, i: usize) -> ([f32; 3], [f32; 4]) {
    let qmul = |a: [f32; 4], b: [f32; 4]| {
        [
            a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
            a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
            a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
            a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
        ]
    };
    let rotate = |q: [f32; 4], v: [f32; 3]| {
        let p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]]);
        [p[0], p[1], p[2]]
    };
    let b = &rig.bones[i];
    match usize::try_from(b.parent).ok().filter(|&p| p < rig.bones.len() && p != i) {
        Some(p) => {
            let (pp, pr) = bone_transform(rig, p);
            let d = rotate(pr, b.position);
            ([pp[0] + d[0], pp[1] + d[1], pp[2] + d[2]], qmul(pr, b.rotation))
        }
        None => (b.position, b.rotation),
    }
}

/// The texture of a painting picture.
pub fn painting_texture(name: &str) -> crate::types::Key {
    (T_DDS, 0, s3pkg::fnv64(name))
}

/// Model-store group of the canvases drawn at each size (over the canvas model's instance).
pub const CANVAS_GROUP: u32 = 0x0CA4_7A50;

/// The Painting skill's picture table and the pictures, and the canvas model at each size
/// (one model for all three, its geometry states the sizes).
/// A catalogue model with parts that move (a fridge's door, a swing's seat): its rig, and how
/// its meshes are skinned to it (in `skins.pack`, by MODL key).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ObjectSkin {
    /// The rig's bones: FNV-32 of the name (as the clips' tracks name them), parent (-1 for
    /// none) and bind pose relative to it (translation; rotation x, y, z, w).
    pub bones: Vec<(u32, i16, [f32; 3], [f32; 4])>,
    /// Each skinned mesh of the model, in model order.
    pub meshes: Vec<SkinMesh>,
}

/// A mesh's skinning: its vertex count and first vertex (to find its baked part), and each
/// vertex's four rig bones (indices into `ObjectSkin::bones`) and weights (in 255ths).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkinMesh {
    pub verts: u32,
    pub first: [f32; 3],
    pub bones: Vec<[u16; 4]>,
    pub weights: Vec<[u8; 4]>,
}

/// An object's rig (OBJD → OBJK → its VPXY, which names the rig).
fn object_rig(pkgs: &PackageSet, objd: crate::types::Key) -> Option<s3formats::sim::Rig> {
    pkgs.read(&crate::types::rkey(objd))
        .and_then(|d| s3formats::object::objd_objk(pkgs, &d))
        .and_then(|o| o.model_key)
        .and_then(|k| pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)))
        .and_then(|v| s3formats::model::vpxy_keys(&v).into_iter().find(|k| k.t == 0x8EAF13DE))
        .and_then(|k| pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)))
        .and_then(|d| s3formats::sim::Rig::parse(&d).ok())
}

/// The catalogue's models with moving parts (meshes skinned to more than the object's root
/// bones), with their rigs, for the object animations the clips play: `skins.pack`.
fn bake_object_skins(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    let g = root.global_dir();
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog.bin: {e}"))?;
    let fixed = ["transformBone", "b__ROOT__", "offsetBone"].map(s3pkg::fnv32);
    let mut done: HashSet<crate::types::Key> = HashSet::new();
    let mut n = 0;
    let mut pack = PackWriter::create(&g.join("skins.pack")).map_err(|e| e.to_string())?;
    for c in &catalog {
        if c.models.iter().all(|m| done.contains(m)) {
            continue;
        }
        let Some(rig) = object_rig(pkgs, c.objd) else { continue };
        if rig.bones.len() < 2 {
            continue;
        }
        let hashes: Vec<u32> = rig.bones.iter().map(|b| s3pkg::fnv32(&b.name)).collect();
        for &modl in &c.models {
            if !done.insert(modl) {
                continue;
            }
            let meshes = s3formats::model::load_model(pkgs, &crate::types::rkey(modl)).unwrap_or_default();
            let mut moves = false;
            let mut out = ObjectSkin {
                bones: rig.bones.iter().zip(&hashes).map(|(b, h)| (*h, b.parent.clamp(-1, i16::MAX as i32) as i16, b.position, b.rotation)).collect(),
                meshes: Vec::new(),
            };
            for m in meshes.iter().filter(|m| !m.blend_indices.is_empty() && !m.positions.is_empty()) {
                // (Each of the mesh's joints as a rig bone: the root when the rig hasn't it.)
                let joints: Vec<u16> = m.joints.iter().map(|j| hashes.iter().position(|h| h == j).unwrap_or(0) as u16).collect();
                let mut bones = Vec::with_capacity(m.blend_indices.len());
                let mut weights = Vec::with_capacity(m.blend_indices.len());
                for (ix, w) in m.blend_indices.iter().zip(&m.blend_weights) {
                    let b = ix.map(|i| joints.get(i as usize).copied().unwrap_or(0));
                    let w = w.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8);
                    if b.iter().zip(&w).any(|(b, w)| *w > 0 && !fixed.contains(&hashes[*b as usize])) {
                        moves = true;
                    }
                    bones.push(b);
                    weights.push(w);
                }
                out.meshes.push(SkinMesh { verts: m.positions.len() as u32, first: m.positions[0], bones, weights });
            }
            if moves {
                pack.add(modl, &out).map_err(|e| e.to_string())?;
                n += 1;
            }
        }
    }
    pack.finish().map_err(|e| e.to_string())?;
    Ok(n)
}

fn bake_paintings(root: &BakeRoot, pkgs: &PackageSet, table: Option<String>) -> Result<usize, String> {
    let g = root.global_dir();
    let mut out = PaintingsBaked::default();
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog.bin: {e}"))?;
    let has_tex = |name: &str| pkgs.read_ti(T_DDS, s3pkg::fnv64(name)).is_some();
    let traits = ["Artistic", "CantStandArt", "ComputerWhiz", "Evil", "Genius", "Grumpy", "Insane", "Neurotic", "Virtuoso"];
    for (size, tag) in ["Small", "Medium", "Large"].into_iter().enumerate() {
        let rows = table.as_deref().map(|x| records(x, tag)).unwrap_or_default();
        let Some((defaults, rows)) = rows.split_first() else { continue };
        let field = |f: &HashMap<String, String>, k: &str| f.get(k).or_else(|| defaults.get(k)).cloned().unwrap_or_default();
        for f in rows {
            // (The base game's: the expansions' rows name theirs.)
            if field(f, "SKU") != "BaseGame" || field(f, "Skill") != "Painting" || field(f, "Occupation") != "Undefined" {
                continue;
            }
            let name = field(f, "PaintingNames");
            if name.is_empty() || !has_tex(&name) {
                continue;
            }
            let flag = |k: &str| field(f, k) == "True";
            let kind = if flag("Masterpiece") {
                3
            } else if flag("Brilliant") {
                2
            } else if flag("Dabble") {
                1
            } else {
                0
            };
            out.paintings.push(PaintingInfo {
                size: size as u8,
                min: field(f, "MinLevelToPaint").parse().unwrap_or(0),
                max: field(f, "MaxLevelToPaint").parse().unwrap_or(10),
                kind,
                traits: traits
                    .iter()
                    .filter_map(|t| {
                        let v = f.get(*t)?;
                        (!v.is_empty() && has_tex(v)).then(|| (t.to_string(), v.clone()))
                    })
                    .collect(),
                name,
            });
        }
    }
    std::fs::create_dir_all(root.textures_dir()).ok();
    let names: Vec<String> = out.paintings.iter().flat_map(|p| std::iter::once(p.name.clone()).chain(p.traits.iter().map(|(_, n)| n.clone()))).collect();
    crate::bake::par_map(&names, |n| {
        let k = painting_texture(n);
        if !root.tex_path(k).exists()
            && let Some(dds) = crate::bake::bake_texture(pkgs, k, 256, false)
        {
            let _ = std::fs::write(root.tex_path(k), dds);
        }
    });
    // The canvas: its painted face's three states, smallest to largest, are the sizes.
    let objd = |n: &str| catalog.iter().find(|c| c.instance_name == n).map(|c| (c.objd, c.models.first().copied()));
    if let (Some((small, Some(modl))), Some((medium, _)), Some((large, _))) =
        (objd("EaselCanvasPaintingSmall"), objd("EaselCanvasPaintingMedium"), objd("EaselCanvasPaintingLarge"))
    {
        let meshes = s3formats::model::load_model(pkgs, &crate::types::rkey(modl)).unwrap_or_default();
        if let Some(face) = meshes.iter().filter(|m| m.states.len() == 3).min_by_key(|m| m.indices.len()) {
            out.face = face.material.texture(s3formats::model::P_DIFFUSE_MAP).map(|k| crate::types::key_of(&k)).unwrap_or_default();
            let mut sizes: Vec<(f32, u32, [f32; 4])> = face
                .states
                .iter()
                .map(|(h, ix)| {
                    let (mut lo, mut hi, mut uv) = ([f32::MAX; 2], [f32::MIN; 2], [f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
                    for &i in ix {
                        let (p, t) = (face.positions[i as usize], face.uvs[i as usize]);
                        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
                        uv = [uv[0].min(t[0]), uv[1].min(t[1]), uv[2].max(t[0]), uv[3].max(t[1])];
                    }
                    ((hi[0] - lo[0]) * (hi[1] - lo[1]), *h, uv.map(|v| v.clamp(0.0, 1.0)))
                })
                .collect();
            sizes.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut pack = PackWriter::create(&g.join("canvases.pack")).map_err(|e| e.to_string())?;
            for ((_, state, uv), objd) in sizes.into_iter().zip([small, medium, large]) {
                let model = (modl.0, CANVAS_GROUP + out.canvases.len() as u32, modl.2);
                pack.add(model, &crate::bake::bake_model_state(pkgs, &crate::types::rkey(modl), Some(state))).map_err(|e| e.to_string())?;
                out.canvases.push(CanvasInfo { objd, model, uv });
            }
            pack.finish().map_err(|e| e.to_string())?;
        }
    }
    // The easels' canvas slots (their rigs' `_cntm_…Painting` bones).
    for c in catalog.iter().filter(|c| c.script.contains("HobbiesSkills") && c.script.contains("Easel") && !c.script.contains("Canvas")) {
        // (OBJD → OBJK → its VPXY, which names the rig.)
        let rig = pkgs
            .read(&crate::types::rkey(c.objd))
            .and_then(|d| s3formats::object::objd_objk(pkgs, &d))
            .and_then(|o| o.model_key)
            .and_then(|k| pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)))
            .and_then(|v| s3formats::model::vpxy_keys(&v).into_iter().find(|k| k.t == 0x8EAF13DE))
            .and_then(|k| pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)))
            .and_then(|d| s3formats::sim::Rig::parse(&d).ok());
        let Some(rig) = rig else { continue };
        let slot = |want: &dyn Fn(&str) -> bool| {
            rig.bones.iter().position(|b| {
                let n = b.name.to_ascii_lowercase();
                n.starts_with("_cntm_") && n.contains("painting") && want(&n)
            })
        };
        if let (Some(s), Some(l)) = (slot(&|n| n.contains("small")), slot(&|n| n.contains("large") || n.contains("med"))) {
            out.easels.push(EaselSlots { objd: c.objd, small: bone_transform(&rig, s), large: bone_transform(&rig, l) });
        }
    }
    let n = out.paintings.len();
    write_value(&g.join("paintings.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// The eye colour part's presets: its iris overlay and colours.
fn bake_eye_colors(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    use s3formats::catalog::CValue;
    let mut out = EyeColors::default();
    let part = pkgs.read_ti(s3pkg::types::CASP, s3pkg::fnv64("amFaceEyeColor")).and_then(|d| s3formats::sim::CasPart::parse(&d).ok());
    for p in part.map(|c| c.presets).unwrap_or_default() {
        let Some((c, keys)) = s3formats::complate::preset(&p) else { continue };
        let value = |c: &s3formats::catalog::Complate, name: &str| c.overrides.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        if out.overlay.is_none()
            && let Some(CValue::Tgi(i)) = value(&c, "Face Overlay")
            && let Some(k) = keys.get(i as usize)
        {
            out.overlay = Some((k.t, k.g, k.i));
        }
        let colour = c.blocks.iter().find_map(|b| match value(b, "Color") {
            Some(CValue::Str(s)) => {
                let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (v.len() >= 3).then(|| [v[0], v[1], v[2]])
            }
            _ => None,
        });
        out.presets.extend(colour);
    }
    if let Some(k) = out.overlay
        && !root.tex_path(k).exists()
        && let Some(dds) = crate::bake::bake_texture(pkgs, k, 512, false)
    {
        let _ = std::fs::write(root.tex_path(k), dds);
    }
    let n = out.presets.len();
    write_value(&root.global_dir().join("eye_colors.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

fn bake_face_bones(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    let names: Vec<&str> = FACE_SLIDERS.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let out: Vec<FaceBones> = FACE_PREFIXES
        .iter()
        .map(|prefix| {
            let sliders = names
                .iter()
                .enumerate()
                .filter_map(|(i, n)| {
                    let key = s3pkg::ResourceKey::new(s3formats::sim::T_BOND, 1, s3pkg::fnv32(&format!("{prefix}FaceDeformations{n}")) as u64);
                    let adj = s3formats::sim::parse_bond(&pkgs.read(&key)?).ok()?;
                    let bones: Vec<FaceBone> = adj.iter().map(|a| FaceBone { bone: a.bone, offset: a.offset, scale: a.scale, rotation: a.rotation }).collect();
                    (!bones.is_empty()).then_some((i as u8, bones))
                })
                .collect();
            FaceBones { prefix: prefix.to_string(), sliders }
        })
        .collect();
    let n = out.iter().map(|f| f.sliders.len()).sum();
    write_value(&root.global_dir().join("face_bones.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// Create a Sim's Face tab: beards, glasses, lipsticks and eye shadows.
pub const FACE_TYPES: [u32; 4] = [s3formats::sim::CT_BEARD, s3formats::sim::CT_GLASSES, s3formats::sim::CT_LIPSTICK, s3formats::sim::CT_EYESHADOW];

/// The CAS parts of the town's service uniforms, by name.
pub const SERVICE_UNIFORMS: [&str; 5] = ["BodyFrenchMaid", "HairFrenchMaidBunLoose", "BodyRepair", "BodyMailCarrier", "BodyPizzaDeliveryOutfit"];

fn bake_outfits(root: &BakeRoot, pkgs: &PackageSet, careers: &[CareerInfo]) -> Result<(), String> {
    use crate::types::{CasBaked, CasPartInfo, CasPartMeshes, Key, OutfitInfo, OutfitPartInfo, key_of};
    use s3formats::sim::{CasPart, SimOutfit, T_OUTFIT};
    let g = root.global_dir();
    let mut names: Vec<String> = careers.iter().flat_map(|c| c.levels.iter().flat_map(|l| l.outfits.iter().cloned())).filter(|n| !n.is_empty()).collect();
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let cas: CasBaked = read_value(&g.join("cas.bin")).map_err(|e| format!("cas.bin: {e}"))?;
    let rig = cas.adult_rig.as_ref().ok_or("no adult rig")?;
    let have: std::collections::HashSet<Key> = cas.parts.iter().filter(|p| p.baked).map(|p| p.key).collect();
    // (Of those, the ones with meshes: the rest are layers alone, like eyebrows.)
    let cas_pack = PackReader::open(&g.join("cas.pack")).map_err(|e| e.to_string())?;
    let have_meshes: std::collections::HashSet<Key> =
        have.iter().copied().filter(|k| cas_pack.get::<CasPartMeshes>(k).is_some_and(|m| !m.meshes.is_empty())).collect();
    std::fs::create_dir_all(root.textures_dir()).ok();
    type Baked = (OutfitInfo, Vec<(Key, CasPartMeshes)>);
    let baked: Vec<Option<Baked>> = crate::bake::par_map(&names, |name| {
        let o = SimOutfit::parse(&pkgs.read_ti(T_OUTFIT, s3pkg::fnv64(name))?).ok()?;
        let mut info = OutfitInfo { name: name.to_ascii_lowercase(), parts: Vec::new() };
        let mut meshes = Vec::new();
        for (i, p) in o.parts.iter().enumerate() {
            let Some(c) = pkgs.read(&p.casp).or_else(|| pkgs.read_ti(p.casp.t, p.casp.i)).and_then(|d| CasPart::parse(&d).ok()) else { continue };
            let key = key_of(&p.casp);
            let mut layer_only = have.contains(&key) && !have_meshes.contains(&key);
            if !have.contains(&key) {
                let m = crate::bake::cas_part_meshes(pkgs, &c, rig, cas.baby_rig.as_ref());
                layer_only = m.meshes.is_empty();
                for t in m.meshes.iter().filter_map(|m| m.texture) {
                    if !root.tex_path(t).exists()
                        && let Some(dds) = crate::bake::bake_texture(pkgs, t, 512, false)
                    {
                        let _ = std::fs::write(root.tex_path(t), dds);
                    }
                }
                meshes.push((key, m));
            }
            // The part's layer in the outfit's colours: clothes as a layer over the skin (the
            // compositor's second target), hair whole (its hat drawn over it there).
            let hair = c.clothing_type == s3formats::sim::CT_HAIR;
            let layer: Key = (T_OUTFIT, i as u32, s3pkg::fnv64(name));
            let img = s3formats::complate::render_preset(pkgs, &p.preset, 512, !hair);
            if let Some(img) = &img {
                let _ = std::fs::write(root.tex_path(layer), crate::ddsw::encode_dds(img));
            }
            // (Without a design of its own, the part's default.)
            let fallback = c.diffuse.first().map(key_of).filter(|_| img.is_none());
            if let Some(k) = fallback
                && !root.tex_path(k).exists()
                && let Some(dds) = crate::bake::bake_texture(pkgs, k, 512, !hair)
            {
                let _ = std::fs::write(root.tex_path(k), dds);
            }
            // A hat's own layer, so the hair under it can take the Sim's colour.
            let mut hat = None;
            if hair && p.preset.contains(r#"key="IsHat" value="true""#) {
                let k: Key = (T_OUTFIT, 0x100 + i as u32, s3pkg::fnv64(name));
                if let Some(img) = s3formats::complate::render_preset(pkgs, &p.preset, 512, true) {
                    let _ = std::fs::write(root.tex_path(k), crate::ddsw::encode_dds(&img));
                    hat = Some(k);
                }
            }
            info.parts.push(OutfitPartInfo {
                part: CasPartInfo {
                    key,
                    name: c.name,
                    clothing_type: c.clothing_type,
                    age_gender: c.age_gender,
                    category: c.category,
                    baked: true,
                    layer: img.map(|_| layer).or(fallback),
                },
                layer_only,
                hat,
            });
        }
        Some((info, meshes))
    });
    // Formal wear, with its default design; and the town's service uniforms (the maid's dress
    // and capped bun, the repairman's overalls, the mail carrier's and the pizza delivery's).
    let formal: Vec<(s3pkg::ResourceKey, CasPart)> = crate::bake::par_map(&pkgs.keys_of_type(s3pkg::types::CASP).copied().collect::<Vec<_>>(), |k| {
        let c = CasPart::parse(&pkgs.read(k)?).ok()?;
        let human = matches!((c.age_gender >> 8) & 0xF, 0 | 1) && c.age_gender & 0x7E != 0;
        let formal = c.category & s3formats::sim::CAT_FORMAL != 0
            && c.category & s3formats::sim::CAT_VALID_RANDOM != 0
            && c.category & s3formats::sim::CAT_HIDDEN == 0
            && matches!(c.clothing_type, s3formats::sim::CT_BODY | s3formats::sim::CT_TOP | s3formats::sim::CT_BOTTOM | s3formats::sim::CT_SHOES);
        let service = SERVICE_UNIFORMS.iter().any(|n| c.name.contains(n))
            && !c.name.to_ascii_lowercase().ends_with("_unlock")
            && matches!(c.clothing_type, s3formats::sim::CT_BODY | s3formats::sim::CT_HAIR);
        // The base game's beards, glasses, lipsticks and eye shadows (for Create a Sim's Face tab).
        let face = FACE_TYPES.contains(&c.clothing_type)
            && k.g == 0
            && c.category & s3formats::sim::CAT_HIDDEN == 0
            && !c.name.to_ascii_lowercase().ends_with("_unlock")
            && (matches!(c.clothing_type, s3formats::sim::CT_GLASSES | s3formats::sim::CT_BEARD) || !c.diffuse.is_empty());
        let wanted = human && (formal || service || face) && !have.contains(&key_of(k));
        wanted.then_some((*k, c))
    })
    .into_iter()
    .flatten()
    .collect();
    let formal: Vec<(CasPartInfo, CasPartMeshes)> = crate::bake::par_map(&formal, |(k, c)| {
        let m = crate::bake::cas_part_meshes(pkgs, c, rig, cas.baby_rig.as_ref());
        let layer = c.diffuse.first().map(key_of);
        let textures = m.meshes.iter().filter_map(|m| m.texture).map(|t| (t, false)).chain(layer.map(|l| (l, true)));
        for (t, as_layer) in textures {
            if !root.tex_path(t).exists()
                && let Some(dds) = crate::bake::bake_texture(pkgs, t, 512, as_layer)
            {
                let _ = std::fs::write(root.tex_path(t), dds);
            }
        }
        // (Make-up is layers drawn onto the face, with no mesh of its own, and so are the beards
        // with no mesh (goatees, chinstraps...: the game tints them with the hair colour); glasses
        // and the other beards are meshes.)
        let layer_only = matches!(c.clothing_type, s3formats::sim::CT_LIPSTICK | s3formats::sim::CT_EYESHADOW)
            || c.clothing_type == s3formats::sim::CT_BEARD && m.meshes.is_empty();
        let info = CasPartInfo { key: key_of(k), name: c.name.clone(), clothing_type: c.clothing_type, age_gender: c.age_gender, category: c.category, baked: !m.meshes.is_empty() || layer_only && layer.is_some(), layer };
        Some((info, m))
    })
    .into_iter()
    .flatten()
    .collect();
    let mut w = PackWriter::create(&g.join("outfits.pack")).map_err(|e| e.to_string())?;
    let mut seen = std::collections::HashSet::new();
    let mut wardrobe = Vec::new();
    for (info, m) in formal {
        if seen.insert(info.key) {
            w.add(info.key, &m).map_err(|e| e.to_string())?;
        }
        wardrobe.push(info);
    }
    write_value(&g.join("wardrobe.bin"), &wardrobe).map_err(|e| e.to_string())?;
    let mut outfits = Vec::new();
    for (info, meshes) in baked.into_iter().flatten() {
        for (k, m) in meshes {
            if seen.insert(k) {
                w.add(k, &m).map_err(|e| e.to_string())?;
            }
        }
        outfits.push(info);
    }
    w.finish().map_err(|e| e.to_string())?;
    write_value(&g.join("outfits.bin"), &outfits).map_err(|e| e.to_string())
}

/// Texture-store type of the wardrobe's colourways: `(T_CAS_PRESET, preset, the part's
/// instance)` (preset 0, the part's own, is its default layer).
pub const T_CAS_PRESET: u32 = 0x0CA5_9E70;

/// A wardrobe part's colourways (the game's presets for it, as Create a Sim's swatches show
/// them): each preset's swatch colour (its average), none for one that wouldn't render.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CasColourways {
    pub part: crate::types::Key,
    pub swatches: Vec<Option<[f32; 3]>>,
    /// Each preset's solid colour channels (A to D as 0 to 3) and their colours: what Create a
    /// Style changes.
    #[serde(default)]
    pub channels: Vec<Vec<(u8, [f32; 3])>>,
}

/// The average colour of a clothing layer where it's drawn (alpha over a half).
fn swatch_colour(img: &s3formats::dds::Rgba) -> Option<[f32; 3]> {
    let (mut sum, mut n) = ([0f64; 3], 0usize);
    for px in img.data.chunks_exact(4).step_by(3) {
        if px[3] > 128 {
            for c in 0..3 {
                sum[c] += px[c] as f64;
            }
            n += 1;
        }
    }
    (n > 16).then(|| sum.map(|s| (s / n as f64 / 255.0) as f32))
}

/// The colourways of the wardrobe's clothes and shoes (`cas_presets.bin`): every preset of each
/// part rendered (at 256), the others than its own written to the texture store, with each
/// one's swatch colour.
fn bake_cas_colourways(root: &BakeRoot, pkgs: &PackageSet) -> Result<usize, String> {
    use crate::types::{CasBaked, CasPartInfo, Key};
    use s3formats::sim::{CT_BODY, CT_BOTTOM, CT_SHOES, CT_TOP, CasPart};
    let g = root.global_dir();
    let mut cas: CasBaked = read_value(&g.join("cas.bin")).map_err(|e| format!("cas.bin: {e}"))?;
    cas.parts.extend(read_value::<Vec<CasPartInfo>>(&g.join("wardrobe.bin")).unwrap_or_default());
    let mut parts: Vec<Key> = cas.parts.iter().filter(|p| p.baked && matches!(p.clothing_type, CT_TOP | CT_BOTTOM | CT_BODY | CT_SHOES)).map(|p| p.key).collect();
    parts.sort();
    parts.dedup();
    let ways: Vec<Option<CasColourways>> = crate::bake::par_map(&parts, |&key| {
        let c = CasPart::parse(&pkgs.read(&s3pkg::ResourceKey::new(key.0, key.1, key.2))?).ok()?;
        if c.presets.is_empty() {
            return None;
        }
        let channels = c.presets.iter().take(8).map(|xml| s3formats::complate::solid_channels(xml)).collect();
        let swatches = c
            .presets
            .iter()
            .take(8)
            .enumerate()
            .map(|(i, xml)| {
                let img = s3formats::complate::render_preset(pkgs, xml, 256, true)?;
                let tex: Key = (T_CAS_PRESET, i as u32, key.2);
                if i > 0 && !root.tex_path(tex).exists() {
                    let _ = std::fs::write(root.tex_path(tex), crate::ddsw::encode_dds(&img));
                }
                swatch_colour(&img)
            })
            .collect();
        Some(CasColourways { part: key, swatches, channels })
    });
    // (Those with colourways to choose, or colours to change.)
    let ways: Vec<CasColourways> =
        ways.into_iter().flatten().filter(|w| w.swatches.iter().filter(|s| s.is_some()).count() > 1 || w.channels.iter().any(|c| !c.is_empty())).collect();
    let n = ways.len();
    write_value(&g.join("cas_presets.bin"), &ways).map_err(|e| e.to_string())?;
    Ok(n)
}

/// The catalogue's fences (`fences.pack` holds their pieces).
fn bake_fence_styles(root: &BakeRoot, pkgs: &PackageSet, strings: &HashMap<u64, String>) -> Result<Vec<FenceStyle>, String> {
    let mut keys: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(0x0418FE2A).copied().collect();
    keys.sort();
    keys.dedup_by_key(|k| k.i);
    let styles: Vec<Option<FenceStyle>> = crate::bake::par_map(&keys, |k| {
        let d = pkgs.read(k)?;
        let (guid, price, shown) = crate::fences::catalog_entry(&d)?;
        let name = strings.get(&guid).cloned().filter(|n| !n.is_empty())?;
        if !shown {
            return None;
        }
        let p = crate::fences::pieces(pkgs, k);
        p.straight?;
        Some(FenceStyle { key: crate::types::key_of(k), name, price: price.round() as i32, straight: p.straight, diagonal: p.diagonal, post: p.post })
    });
    let mut styles: Vec<FenceStyle> = styles.into_iter().flatten().collect();
    styles.sort_by_key(|s| (s.price, s.name.clone()));
    let g = root.global_dir();
    let mut w = PackWriter::create(&g.join("fences.pack")).map_err(|e| e.to_string())?;
    let mut seen = BTreeSet::new();
    for s in &styles {
        for m in [s.straight, s.diagonal, s.post].into_iter().flatten() {
            if !seen.insert(m) {
                continue;
            }
            let b = crate::bake::bake_model(pkgs, &crate::types::rkey(m));
            for t in b.parts.iter().filter_map(|p| p.texture) {
                if !root.tex_path(t).exists()
                    && let Some(dds) = crate::bake::bake_texture(pkgs, t, crate::bake::OBJECT_TEX_MAX, false)
                {
                    let _ = std::fs::write(root.tex_path(t), dds);
                }
            }
            w.add(m, &b).map_err(|e| e.to_string())?;
        }
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(styles)
}

/// The catalogue's SpeedTree objects (trees, shrubs, flowers: their OBJK's model is a
/// SpeedTree), each with its species' billboard baked.
fn bake_garden(root: &BakeRoot, pkgs: &PackageSet) -> Vec<(crate::types::Key, crate::types::TreeKindBaked)> {
    const T_SPEEDTREE: u32 = 0x00B552EA;
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
    let found: Vec<(crate::types::Key, u64)> = crate::bake::par_map(&catalog, |c| {
        if c.price <= 0 || !c.models.is_empty() {
            return None;
        }
        let d = pkgs.read(&crate::types::rkey(c.objd))?;
        let objk = s3formats::object::objd_objk(pkgs, &d)?;
        let m = objk.model_key.filter(|k| k.t == T_SPEEDTREE)?;
        Some((c.objd, m.i))
    })
    .into_iter()
    .flatten()
    .collect();
    let mut kinds: Vec<u64> = found.iter().map(|f| f.1).collect();
    kinds.sort();
    kinds.dedup();
    let (baked, textures) = crate::trees::bake_tree_kinds(pkgs, &kinds);
    crate::bake::bake_textures(root, pkgs, &textures, 512, "Converting: garden", &|_| {});
    let by_kind: HashMap<u64, crate::types::TreeKindBaked> = baked.into_iter().map(|k| (k.kind, k)).collect();
    found.into_iter().filter_map(|(objd, kind)| Some((objd, by_kind.get(&kind)?.clone()))).collect()
}

pub fn bake_gamedata(root: &BakeRoot, pkgs: &PackageSet, install_root: &std::path::Path, progress: &dyn Fn(&str)) -> Result<usize, String> {
    progress("Converting: moodlets, traits and skills…");
    let path = install_root.join("Game").join("Bin").join("Gameplay").join("GameplayData.package");
    let pkg = Package::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    // The package's XML by name.
    let mut names: HashMap<u64, String> = HashMap::new();
    for e in pkg.of_type(T_NMAP) {
        if let Ok(d) = pkg.read(e) {
            names.extend(s3formats::audio::parse_name_map(&d));
        }
    }
    // (Tuning for a class is named after it with a hash: `Writing_0x49f73ba878b269dc`.)
    let xml = |name: &str| -> Option<String> {
        let named = |n: &String| n.eq_ignore_ascii_case(name) || n.strip_prefix(name).is_some_and(|rest| rest.starts_with("_0x"));
        let e = pkg.of_type(T_XML).find(|e| names.get(&e.key.i).is_some_and(named))?;
        pkg.read(e).ok().map(|d| String::from_utf8_lossy(&d).into_owned())
    };
    let strings = s3formats::stbl::load_english(pkgs);
    let text = |sheet: &str, key: &str| -> String {
        if key.is_empty() {
            return String::new();
        }
        strings.get(&s3pkg::fnv64(&format!("Gameplay/Excel/{sheet}:{key}").to_ascii_lowercase())).cloned().unwrap_or_default()
    };
    let num = |f: &HashMap<String, String>, k: &str| f.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    let get = |f: &HashMap<String, String>, k: &str| f.get(k).cloned().unwrap_or_default();

    let mut out = GameDataBaked { version: GAMEDATA_VERSION, ..Default::default() };
    // The social interactions: the base game's. (The packs' are left: theirs are mostly bound to
    // what those packs bring, a celebrity, a degree, the future, under tests in their code.)
    // The objects' breakage and repair tuning, every class that breaks.
    for e in pkg.of_type(T_XML) {
        let Some(name) = names.get(&e.key.i) else { continue };
        let class = name.split("_0x").next().unwrap_or(name);
        if class.contains('+') {
            continue;
        }
        let Ok(d) = pkg.read(e) else { continue };
        let x = String::from_utf8_lossy(&d);
        if x.contains("StartPercentChanceBreak")
            && let Some(t) = repair_tuning(class, &x)
            && !out.repairs.iter().any(|r| r.class == t.class)
        {
            out.repairs.push(t);
        }
        if let Some(at) = x.find("<EnergyMultiplier value=\"")
            && let Some(v) = x[at + 25..].split('"').next().and_then(|v| v.trim().parse::<f32>().ok())
            && !out.energy_multipliers.iter().any(|(c, _)| c == class)
        {
            out.energy_multipliers.push((class.to_string(), v));
        }
        if let Some(at) = x.find("<EnvironmentScore value=\"")
            && let Some(v) = x[at + 25..].split('"').next().and_then(|v| v.trim().parse::<f32>().ok())
            && v != 0.0
            && !out.environment_scores.iter().any(|(c, _)| c == class)
        {
            out.environment_scores.push((class.to_string(), v));
        }
        if let Some(c) = appliance_cooking(class, &x)
            && !out.appliance_cooking.iter().any(|o| o.class == c.class)
        {
            out.appliance_cooking.push(c);
        }
    }
    // The interactions' need changes, each class's (the game's ITUNs).
    for e in pkg.of_type(T_ITUN) {
        let Ok(d) = pkg.read(e) else { continue };
        if let Some(r) = interaction_rates(&String::from_utf8_lossy(&d))
            && !out.interaction_rates.iter().any(|o| o.class == r.class && o.interaction == r.interaction)
        {
            out.interaction_rates.push(r);
        }
    }
    let tables = std::iter::once("SocialData_BaseGame".to_string());
    let mut found = false;
    for table in tables {
        let Some(x) = xml(&table) else { continue };
        found = true;
        for a in social_actions(&x, &|key| text("Socializing/Action", key)) {
            if !out.socials.iter().any(|s| s.key == a.key) {
                out.socials.push(a);
            }
        }
    }
    if found {
        // (How they go: the general rules and the conversation's contexts.)
        if let Some(s) = xml("Socializing") {
            out.general_rules = general_rules(&s);
            out.stcs = stc_infos(&s);
        }
        // (And the topics they're offered under, and the contexts.)
        if let Some(t) = xml("SocializingActionAvailability") {
            out.social_menus = social_menus(&t);
            out.social_topics = topic_infos(&t);
            for (topic, action, first) in social_topics(&t) {
                for s in out.socials.iter_mut().filter(|s| s.key == action) {
                    if !s.topics.iter().any(|x| x.0 == topic && x.1 == first) {
                        s.topics.push((topic.clone(), first));
                    }
                }
            }
        }
    }
    // The idle animations.
    if let Some(x) = xml("Idles") {
        out.idles = IdleTables {
            standing: idle_rows(&x, "StandingIdle", "").into_iter().next().map(|g| g.1).unwrap_or_default(),
            mood: idle_rows(&x, "MoodIdle", "MoodFlavor"),
            traits: idle_rows(&x, "TraitIdle", "Trait"),
            buffs: idle_rows(&x, "BuffIdle", "BuffName"),
            reactions: idle_rows(&x, "Reactions", "ReactionType"),
            faces: {
                let mut faces: Vec<(String, Vec<(String, f32)>)> = Vec::new();
                for r in records(&x, "FacialOverlays").into_iter().skip(1) {
                    // (The table names one clip with a doubled underscore: `angry2__noblink`.)
                    let (cat, clip) = (r.get("Category").cloned().unwrap_or_default(), r.get("Clip").cloned().unwrap_or_default().replace("__", "_"));
                    if cat.is_empty() || clip.is_empty() {
                        continue;
                    }
                    let w = r.get("Weight").and_then(|w| w.parse::<f32>().ok()).unwrap_or(1.0);
                    match faces.iter_mut().find(|f| f.0 == cat) {
                        Some(f) => f.1.push((clip, w)),
                        None => faces.push((cat, vec![(clip, w)])),
                    }
                }
                faces
            },
        };
    }
    progress("Converting: object effect slots…");
    let mut objds: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(s3pkg::types::OBJD).copied().collect();
    objds.sort();
    out.fx_slots = crate::bake::par_map(&objds, |k| {
        let fx = s3formats::object::object_fx_slots(pkgs, k);
        (!fx.is_empty()).then(|| (crate::types::key_of(k), fx))
    })
    .into_iter()
    .flatten()
    .collect();
    out.route_slots = crate::bake::par_map(&objds, |k| {
        let r = s3formats::object::object_route_slots(pkgs, k);
        (!r.is_empty()).then(|| (crate::types::key_of(k), r))
    })
    .into_iter()
    .flatten()
    .collect();
    // Buffs: rows without a SKU belong to the base game.
    // (The packs' buffs too, as Seasons' Getting Chilly or Soaked, when the base game hasn't one
    // by that name.)
    let buff_rows = records(&xml("Buffs").ok_or("no Buffs table")?, "BuffList");
    let base = |f: &HashMap<String, String>| f.get("SKU").is_none_or(|s| s == "BaseGame" || s == "None");
    let base_hex: std::collections::HashSet<String> = buff_rows.iter().filter(|f| base(f)).filter_map(|f| f.get("Hex").cloned()).collect();
    for f in buff_rows {
        let hex = get(&f, "Hex");
        if hex.is_empty() || (!base(&f) && base_hex.contains(&hex)) || out.buffs.iter().any(|b| b.hex == hex) {
            continue;
        }
        let name_key = get(&f, "BuffName");
        out.buffs.push(BuffInfo {
            name: text("Buffs/BuffList", &name_key),
            desc: text("Buffs/BuffList", &get(&f, "BuffDescription")),
            icon: get(&f, "ThumbFilename"),
            axis: get(&f, "AxisEffected"),
            value: num(&f, "EffectValue") as i32,
            timeout: num(&f, "TimeoutLength"),
            hex,
        });
    }
    for f in records(&xml("Traits").ok_or("no Traits table")?, "TraitList") {
        let hex = get(&f, "Hex");
        if hex.is_empty() {
            continue;
        }
        let name_key = get(&f, "Name");
        out.traits.push(TraitInfo {
            name: text("Traits/TraitList", &name_key),
            desc: text("Traits/TraitList", &get(&f, "Description")),
            icon: get(&f, "ThumbFilename"),
            icon_small: get(&f, "ThumbPieMenu"),
            category: get(&f, "Category"),
            points: num(&f, "Points").max(0.0) as u32,
            score: num(&f, "Points") as i32,
            weight: num(&f, "RandomWeight"),
            random: !get(&f, "CanBeLearnedRandomly").eq_ignore_ascii_case("false"),
            predicate: text("Traits/TraitList", &get(&f, "Predicate")),
            hex,
        });
    }
    if let Some(skills) = xml("Skills") {
        let rows = records(&skills, "SkillList");
        // (The first row is every skill's default.)
        let default = rows.first().cloned().unwrap_or_default();
        for f in rows {
            let hex = get(&f, "Hex");
            let mut total = 0;
            let points = (1..=10)
                .map(|n| {
                    let k = format!("Level_{n}");
                    total += f.get(&k).or(default.get(&k)).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0);
                    total
                })
                .collect();
            // (The packs' skills that show in the skills panel too: Late Night's piano and
            // drums, Into the Future's laser harp...)
            if hex.is_empty() || (f.get("CodeVersion").is_some_and(|s| s != "BaseGame") && get(&f, "IconKey").is_empty()) {
                continue;
            }
            out.skills.push(SkillInfo {
                name: text("Skills/SkillList", &get(&f, "SkillName")),
                desc: text("Skills/SkillList", &get(&f, "SkillDescription")),
                icon: get(&f, "IconKey"),
                wish_icon: get(&f, "DreamsAndPromisesIcon"),
                object_icon: get(&f, "SkillUIIcon"),
                max_level: num(&f, "MaxSkillLevel") as u32,
                points,
                hex,
            });
        }
    }

    // Careers: the base game's full-time careers and part-time jobs, each with its own table
    // of levels (and branches).
    let events_xml = xml("CareerEvents").unwrap_or_default();
    if let Some(careers) = xml("Careers") {
        let day_bit = |d: &str| match d.trim() {
            "M" => 1u8,
            "T" => 2,
            "W" => 4,
            "R" => 8,
            "F" => 16,
            "S" => 32,
            "U" => 64,
            _ => 0,
        };
        let list = records(&careers, "CareerList");
        // (Columns a row leaves out are the first row's, the defaults.)
        let list_default = list.first().cloned().unwrap_or_default();
        for f in list.iter().skip(1) {
            let f = &{
                let mut f = f.clone();
                for (k, v) in &list_default {
                    f.entry(k.clone()).or_insert_with(|| v.clone());
                }
                f
            };
            let name_key = get(f, "CareerName");
            let table = get(&f, "TableName");
            let category = get(&f, "Category");
            if name_key.is_empty() || table.is_empty() || f.get("ProductVersion").is_some_and(|s| s != "BaseGame") {
                continue;
            }
            if category != "FullTime" && category != "PartTime" {
                continue;
            }
            let mut levels = Vec::new();
            let mut last_skill: Vec<String> = Vec::new();
            let rows = records(&careers, &table);
            let row_default = rows.first().cloned().unwrap_or_default();
            for r in rows {
                let r = {
                    let mut r = r;
                    for (k, v) in &row_default {
                        r.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                    r
                };
                let level = num(&r, "Level") as u32;
                if level == 0 {
                    continue;
                }
                let title_key = get(&r, "Title");
                let skills: Vec<String> = (1..=4)
                    .filter(|i| r.get(&format!("Metric{i}")).is_some_and(|m| m == "SkillX"))
                    .filter_map(|i| r.get(&format!("Args{i}")).cloned())
                    .collect();
                // (Rows that don't name the skill keep the previous level's.)
                if !skills.is_empty() {
                    last_skill = skills;
                }
                let start = num(&r, "StartTime");
                levels.push(CareerLevelInfo {
                    level,
                    branch: r.get("BranchName").cloned().unwrap_or_else(|| "Base".into()),
                    title: text(&format!("Careers/{table}"), &title_key),
                    title_female: text(&format!("Careers/{table}"), &format!("{title_key}_Female")),
                    hourly: num(&r, "BasePay"),
                    start,
                    hours: num(&r, "DayLength"),
                    wakeup: num(&r, "WakeupTime"),
                    metrics: (1..=4)
                        .filter_map(|i| {
                            let m = get(&r, &format!("Metric{i}"));
                            // (`Sims3.Gameplay.Careers.MetricMood, Sims3GameplaySystems` is `Mood`.)
                            let kind = m.split(',').next()?.rsplit('.').next()?.trim().trim_start_matches("Metric").to_string();
                            if kind.is_empty() {
                                return None;
                            }
                            let map: Vec<f32> = get(&r, &format!("Mappings{i}")).split(',').filter_map(|v| v.trim().parse().ok()).collect();
                            Some(CareerMetric { kind, mappings: if map.len() == 3 { [map[0], map[1], map[2]] } else { [0.0; 3] }, arg: get(&r, &format!("Args{i}")) })
                        })
                        .collect(),
                    has_boss: get(&r, "HasBoss").eq_ignore_ascii_case("true") || get(&r, "HasBoss") == "1",
                    days: get(&r, "DaysToWork").split(',').map(day_bit).fold(0, |a, b| a | b),
                    skills: last_skill.clone(),
                    outfits: ["OutfitMale", "OutfitFemale", "OutfitMaleElder", "OutfitFemaleElder"].map(|c| get(&r, c)),
                });
            }
            if levels.is_empty() {
                continue;
            }
            let n = |k: &str, d: f32| f.get(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d);
            out.careers.push(CareerInfo {
                name: text("Careers/CareerList", &name_key),
                desc: text("Careers/CareerList", &get(f, "CareerDescription")),
                offer: text("Careers/CareerList", &get(f, "Text_JobOffer")),
                icon: get(f, "DreamsAndPromisesIcon"),
                flow: [n("MaxPerfFlow", 14.0), n("MinPerfFlow", -8.0), n("MaxPerfFlowMaxLevel", 8.0), n("MinPerfFlowMaxLevel", -10.0)],
                miss_work: n("MissWorkPerf", -10.0),
                bonus_hours: n("BonusAmountInHours", 12.0),
                fun_per_hour: n("FunStressPerHour", -5.0),
                coworkers: (n("MinCoworkers", 2.0) as u8, n("MaxCoworkers", 4.0) as u8),
                part_time: category == "PartTime",
                events: career_events(&events_xml, &name_key, &|k| text(&format!("CareerEvents/{name_key}"), k)),
                hex: name_key,
                levels,
            });
        }
    }

    // The build catalogue's wallpapers and floors, each rendered in its first swatch (under the
    // key lot coverings use for catalogue patterns).
    progress("Converting: wallpapers and floors…");
    {
        let mut keys: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(s3formats::catalog::T_CWAL).copied().collect();
        keys.sort();
        keys.dedup_by_key(|k| k.i);
        let done = std::sync::atomic::AtomicUsize::new(0);
        let found: Vec<PatternInfo> = crate::bake::par_map(&keys, |k| {
            let p = s3formats::catalog::WallFloorPattern::parse(&pkgs.read(k)?).ok()?;
            done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let floor = match p.pattern_type {
                s3formats::catalog::PATTERN_FLOOR => true,
                s3formats::catalog::PATTERN_WALL => false,
                _ => return None,
            };
            let name = strings.get(&p.name_guid).cloned().unwrap_or_default();
            if !p.in_catalog || p.price <= 0.0 || name.is_empty() {
                return None;
            }
            let texture = (crate::types::T_COVER, if floor { 4 } else { 3 }, k.i);
            let (w, h) = if floor { (256, 256) } else { (256, 512) };
            // Its colour presets, each drawn once (the first under the pattern's own key; some
            // patterns list the same preset twice).
            let mut seen: Vec<String> = Vec::new();
            let mut swatches = Vec::new();
            let mut channels = Vec::new();
            for (i, m) in p.materials.iter().enumerate() {
                let sig = format!("{:?}", m.complate.blocks);
                if seen.contains(&sig) || swatches.len() >= PATTERN_SWATCHES {
                    continue;
                }
                seen.push(sig);
                let key = if i == 0 { texture } else { (crate::types::T_COVER, (if floor { 4 } else { 3 }) | (i as u32) << 8, k.i) };
                let path = root.tex_path(key);
                if !path.exists() || s3formats::complate::uses_escaped_names(&m.complate) && crate::bake::drawn_before_names_fixed(&path) {
                    let Some(img) = s3formats::complate::render(pkgs, &m.complate, &m.keys, w, h) else { continue };
                    if std::fs::write(&path, crate::ddsw::encode_dds(&img)).is_err() {
                        continue;
                    }
                }
                swatches.push(key);
                channels.push(m.complate.solid_channels());
            }
            if swatches.first() != Some(&texture) {
                return None;
            }
            Some(PatternInfo { name, price: p.price.round() as i32, floor, sort_flags: p.sort_flags, texture, cwal: k.i, swatches, channels })
        })
        .into_iter()
        .flatten()
        .collect();
        out.patterns = found;
        out.patterns.sort_by(|a, b| a.floor.cmp(&b.floor).then(a.price.cmp(&b.price)).then(a.name.cmp(&b.name)));
        out.patterns.dedup_by(|a, b| a.floor == b.floor && a.name == b.name);
    }

    // Roof patterns: each one's first visual proxy holds its roof material, whose diffuse map
    // is the tiles' texture. The name is a string key after the header (UTF-16BE, 7-bit length).
    progress("Converting: roof patterns…");
    {
        let mut keys: Vec<s3pkg::ResourceKey> = pkgs.keys_of_type(s3formats::catalog::T_ROOF_PATTERN).copied().collect();
        keys.sort();
        keys.dedup_by_key(|k| k.i);
        for k in keys {
            let Some(d) = pkgs.read(&k) else { continue };
            if d.len() < 0x22 {
                continue;
            }
            let n = d[0x20] as usize;
            let key_name: String = if d.len() >= 0x21 + n {
                String::from_utf16_lossy(&d[0x21..0x21 + n].chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>())
            } else {
                String::new()
            };
            let guid = u64::from_le_bytes(d[0x10..0x18].try_into().unwrap());
            let name = strings
                .get(&guid)
                .or_else(|| strings.get(&s3pkg::fnv64(&key_name.to_ascii_lowercase())))
                .cloned()
                .unwrap_or_else(|| key_name.rsplit(':').next().unwrap_or("").trim_start_matches("Roof").trim_start_matches('_').to_string());
            let Ok(tgis) = s3formats::model::tgi_table_at(&d, 4) else { continue };
            let Some(vk) = tgis.iter().find(|t| t.t == s3pkg::types::VPXY) else { continue };
            let Some(vd) = pkgs.read(vk).or_else(|| pkgs.read_ti(vk.t, vk.i)) else { continue };
            let Some(m) = s3formats::model::vpxy_keys(&vd).iter().filter(|t| t.t == 0x01D0E75D).find_map(|mk| s3formats::model::load_matd_resource(pkgs, mk)) else { continue };
            let Some(tex) = m.texture(s3formats::model::P_DIFFUSE_MAP) else { continue };
            let texture: crate::types::Key = (tex.t, tex.g, tex.i);
            let path = root.tex_path(texture);
            if !path.exists() {
                let Some(dds) = crate::bake::bake_texture(pkgs, texture, 512, false) else { continue };
                if std::fs::write(&path, dds).is_err() {
                    continue;
                }
            }
            let tile: crate::types::Key = (crate::types::T_COVER, 6, k.i);
            let tile_path = root.tex_path(tile);
            if !tile_path.exists() {
                let Some(atlas) = std::fs::read(&path).ok().and_then(|d| s3formats::dds::decode(&d, 512)) else { continue };
                let Some(img) = roof_tile(&atlas) else { continue };
                if std::fs::write(&tile_path, crate::ddsw::encode_dds(&img)).is_err() {
                    continue;
                }
            }
            out.roofs.push(RoofPattern { name, texture, tile });
        }
        out.roofs.sort_by(|a, b| a.name.cmp(&b.name));
        out.roofs.dedup_by(|a, b| a.texture == b.texture);
    }

    // Collectibles and the spawners that turn them up. (Base game: rows without a CodeVersion.)
    let base = |f: &HashMap<String, String>| get(f, "CodeVersion").is_empty() && get(f, "RequiredWorld").is_empty();
    let pretty = |key: &str| {
        let mut out = String::new();
        for (i, c) in key.chars().enumerate() {
            if i > 0 && c.is_uppercase() {
                out.push(' ');
            }
            out.push(c);
        }
        out
    };
    let list = |s: &str| -> Vec<String> { s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect() };
    let weights = |items: &str, probs: &str| -> Vec<(String, f32)> {
        let w: Vec<f32> = list(probs).iter().map(|p| p.parse().unwrap_or(0.0)).collect();
        list(items).into_iter().enumerate().map(|(i, k)| (k, w.get(i).copied().unwrap_or(1.0))).collect()
    };
    let short = |class: &str| class.rsplit('.').next().unwrap_or(class).to_string();
    if let Some(x) = xml("RockGemMetal") {
        for (row, kind, sheet) in [("Gems", CollectKind::Gem, "RockGemMetal/Gems"), ("Metals", CollectKind::Metal, "RockGemMetal/Metals"), ("Rocks", CollectKind::SpaceRock, "RockGemMetal/Rocks")] {
            for f in records(&x, row) {
                let label = get(&f, "Name");
                // (Spawners name rocks by their Hex: "SpaceRockSmall".)
                let key = Some(get(&f, "Hex")).filter(|h| !h.is_empty() && h != "0").unwrap_or_else(|| label.clone());
                if label.is_empty() || !base(&f) {
                    continue;
                }
                let name = text(sheet, &label);
                let model = match kind {
                    CollectKind::Gem => "Gem".to_string(),
                    CollectKind::Metal => "Metal".to_string(),
                    _ => if key.contains("Large") { "MeteorLarge" } else { "MeteorMedium" }.to_string(),
                };
                out.collectibles.push(CollectibleInfo {
                    name: if name.is_empty() { pretty(&key) } else { name },
                    key,
                    kind,
                    min_price: num(&f, "MinPrice") as i32,
                    max_price: num(&f, "MaxPrice") as i32,
                    rarity: get(&f, "Rarity"),
                    level: num(&f, "MinHuntSkillLevel") as u8,
                    model,
                    bait: String::new(),
                    weight: (0.0, 0.0),
                    over_max: false,
                });
            }
        }
        for f in records(&x, "Spawners") {
            let class = short(&get(&f, "SpawnerClassName"));
            if class.is_empty() {
                continue;
            }
            out.spawners.push(SpawnerInfo {
                class,
                items: weights(&get(&f, "Spawns"), &get(&f, "Probability")),
                capacity: num(&f, "MaxSpawnCapacity").max(1.0) as u8,
                hours: (num(&f, "SpawnMinTime"), num(&f, "SpawnMaxTime")),
            });
        }
    }
    // Seed spawners (their classes' tuning): seeds of each rarity by its chances ("seed:Common"),
    // a couple at most out, every four to ten hours seven times in ten.
    for class in [
        "SeedSpawnerCommon1", "SeedSpawnerCommon2", "SeedSpawnerCommon3", "SeedSpawnerCommon4", "SeedSpawnerUncommon1", "SeedSpawnerUncommon2", "SeedSpawnerUncommon3", "SeedSpawnerRare1",
        "SeedSpawnerRare2", "SeedSpawnerRare3", "SeedSpawnerSpecial1", "SeedSpawnerSpecial2", "SeedSpawnerSpecial3",
    ] {
        let Some(x) = xml(class) else { continue };
        let val = |k: &str| -> Vec<f32> {
            x.find(&format!("<{k} value=\""))
                .and_then(|at| x[at + k.len() + 9..].split('"').next())
                .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
                .unwrap_or_default()
        };
        let chances = val("SeedRarityChances");
        let every = val("SpawnFrequency");
        let chance = val("SpawnChance").first().copied().unwrap_or(1.0).max(0.05);
        if chances.len() < 4 || chances.iter().all(|c| *c <= 0.0) {
            continue;
        }
        out.spawners.push(SpawnerInfo {
            class: class.to_string(),
            items: ["Common", "Uncommon", "Rare", "Special"].iter().zip(&chances).filter(|(_, c)| **c > 0.0).map(|(r, c)| (format!("seed:{r}"), *c)).collect(),
            capacity: val("MaxSpawnCapacity").first().copied().unwrap_or(2.0).max(1.0) as u8,
            hours: (every.first().copied().unwrap_or(4.0) / chance, every.get(1).copied().unwrap_or(10.0) / chance),
        });
    }
    if let Some(x) = xml("Insects") {
        for (row, kind, sheet) in [("Butterflies", CollectKind::Butterfly, "Insects/Butterflies"), ("Beetles", CollectKind::Beetle, "Insects/Beetles")] {
            for f in records(&x, row) {
                let key = get(&f, "Type");
                let label = get(&f, "LocalizedName");
                if key.is_empty() || label.is_empty() || !base(&f) {
                    continue;
                }
                let name = text(sheet, &label);
                let noun = if kind == CollectKind::Butterfly { "Butterfly" } else { "Beetle" };
                let name = if name.is_empty() { format!("{} {noun}", pretty(&label)) } else { name };
                let value = num(&f, "Value") as i32;
                out.collectibles.push(CollectibleInfo {
                    key,
                    name,
                    kind,
                    min_price: value,
                    max_price: value,
                    rarity: get(&f, "Rarity"),
                    level: 0,
                    model: get(&f, "ModelMaterial").split(':').next().unwrap_or("").to_string(),
                    bait: String::new(),
                    weight: (0.0, 0.0),
                    over_max: false,
                });
            }
        }
        for f in records(&x, "Spawners") {
            let class = short(&get(&f, "SpawnerClassName"));
            if class.is_empty() {
                continue;
            }
            out.spawners.push(SpawnerInfo {
                class,
                items: weights(&get(&f, "Spawns"), &get(&f, "Probability")),
                capacity: num(&f, "MaxSpawnCapacity").max(1.0) as u8,
                hours: (num(&f, "MinSpawnFrequency"), num(&f, "MaxSpawnFrequency")),
            });
        }
    }
    // Writing: the skill's tuning, and the titles written books get.
    if let Some(x) = xml("Writing") {
        out.writing = tuning_values(&x);
    }
    if let Some(x) = xml("Books") {
        let mut by_genre: Vec<(String, Vec<String>)> = Vec::new();
        for f in records(&x, "WrittenBookTitles") {
            let mut genres: Vec<(&String, &String)> = f.iter().collect();
            genres.sort();
            for (genre, key) in genres {
                let title = text("Books/WrittenBookTitles", key);
                if title.is_empty() {
                    continue;
                }
                match by_genre.iter_mut().find(|(g, _)| g == genre) {
                    Some((_, v)) => v.push(title),
                    None => by_genre.push((genre.clone(), vec![title])),
                }
            }
        }
        out.book_titles = by_genre;
        // (The skill books: the first row is the defaults; those of the base game's worlds.)
        let rows = records(&x, "BookSkill");
        let defaults = rows.first().cloned().unwrap_or_default();
        let field = |f: &HashMap<String, String>, k: &str| f.get(k).filter(|v| !v.trim().is_empty()).or(defaults.get(k)).map(|v| v.trim().to_string()).unwrap_or_default();
        for f in rows.iter().skip(1) {
            let key = get(f, "Title");
            let worlds = field(f, "AllowedWorldTypes");
            let version = field(f, "CodeVersion");
            if key.is_empty() || !version.eq_ignore_ascii_case("Basegame") || !worlds.split(',').any(|w| w.trim() == "Base") {
                continue;
            }
            let n = |k: &str| field(f, k).parse::<f32>().unwrap_or(0.0);
            out.skill_books.push(SkillBookInfo {
                name: text("Books/BookSkill", &key),
                author: text("Books/BookSkill", &get(f, "Author")),
                skill: get(f, "Skill"),
                min: n("MinSkill") as u32,
                max: n("MaxSkill") as u32,
                points_per_page: n("SkillPointsPerPage"),
                pages: n("Length") as u32,
                price: n("Value") as i64,
                pages_per_minute: n("PagesMinNorm").max(0.1),
                bookworm_pages_per_minute: n("PagesMinBW").max(0.1),
                key,
            });
        }
    }
    // Recipes, with the food they make (the dish and a plate of it, full and emptied).
    if let Some(x) = xml("RecipeMasterList") {
        progress("Converting: recipes and food…");
        // Food models by name (the name maps), each drawn in the geometry state a recipe names:
        // `foodServeRatatouille#foodFull:Default` is the `foodFull` state of that model.
        let mut food: HashMap<String, u64> = HashMap::new();
        for k in pkgs.keys_of_type(T_NMAP) {
            if let Some(d) = pkgs.read(k) {
                for (inst, n) in s3formats::audio::parse_name_map(&d) {
                    // (Snacks' own have a part in the name: `foodSnackBreadandJam_slice`.)
                    let lower = n.to_ascii_lowercase();
                    if n.starts_with("food") && (!n.contains('_') || lower.starts_with("foodsnack")) && pkgs.find_ti(s3pkg::types::MODL, inst).is_some() {
                        food.insert(n.to_ascii_lowercase(), inst);
                    }
                }
            }
        }
        // (Some are hidden catalogue objects instead: `FoodEatHamburger`.)
        let catalog: Vec<crate::types::CatalogEntry> = read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        let mut models: Vec<(crate::types::Key, s3pkg::ResourceKey, u32)> = Vec::new();
        // (Others are named only by their rig and footprint (`foodServeBelgianWaffles`), which
        // the model's VPXY lists beside it: the models by those names' hashes.)
        let mut by_rig: Option<HashMap<u64, s3pkg::ResourceKey>> = None;
        let mut model_of = |spec: &str| -> Option<crate::types::Key> {
            let (name, rest) = spec.split_once('#').unwrap_or((spec, ""));
            let state = rest.split(':').next().filter(|s| !s.is_empty());
            let modl = match food.get(&name.to_ascii_lowercase()) {
                Some(&inst) => pkgs.find_ti(s3pkg::types::MODL, inst)?,
                None => match catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(name)) {
                    Some(c) => crate::types::rkey(*c.models.first()?),
                    None => *by_rig
                        .get_or_insert_with(|| {
                            let mut m = HashMap::new();
                            for k in pkgs.keys_of_type(s3pkg::types::VPXY).filter(|k| k.g == 1) {
                                let keys = pkgs.read(k).map(|d| s3formats::model::vpxy_keys(&d)).unwrap_or_default();
                                if let Some(modl) = keys.iter().find(|x| x.t == s3pkg::types::MODL) {
                                    for x in keys.iter().filter(|x| x.t == 0x8EAF13DE || x.t == 0xD382BF57) {
                                        m.entry(x.i).or_insert(*modl);
                                    }
                                }
                            }
                            m
                        })
                        .get(&s3pkg::fnv64(name))?,
                },
            };
            let inst = modl.i;
            let hash = state.map_or(0, s3pkg::fnv32);
            let key = (s3pkg::types::MODL, hash, inst);
            if !models.iter().any(|(k, ..)| *k == key) {
                models.push((key, modl, hash));
            }
            Some(key)
        };
        let mut snacks = Vec::new();
        for f in records(&x, "Data") {
            let code = get(&f, "CodeVersion");
            let key = get(&f, "Recipe_Key");
            let meals = [("Breakfast", MEAL_BREAKFAST), ("Brunch", MEAL_BRUNCH), ("Lunch", MEAL_LUNCH), ("Dinner", MEAL_DINNER), ("Dessert", MEAL_DESSERT)]
                .iter()
                .filter(|(m, _)| !get(&f, m).is_empty())
                .fold(0, |a, (_, b)| a | b);
            let snack = !get(&f, "Snack").is_empty();
            if key.is_empty() || !(code.is_empty() || code == "BaseGame") || (!snack && (meals == 0 || get(&f, "Learnable") == "False")) {
                continue;
            }
            let name = text("RecipeMasterList/Data", &key);
            (if snack { &mut snacks } else { &mut out.recipes }).push(RecipeInfo {
                name: if name.is_empty() { pretty(&key) } else { name },
                level: num(&f, "Level") as u8,
                auto: !get(&f, "Auto_Learn").is_empty(),
                meals,
                vegetarian: !get(&f, "Is_Vegetarian").is_empty(),
                cost: num(&f, "RegisterCost") as i32,
                ingredients: ["Ingredient_1", "Ingredient_2", "Ingredient_3"].iter().map(|i| get(&f, i)).filter(|i| !i.is_empty()).collect(),
                group: model_of(&get(&f, "Group_Full")),
                group_half: model_of(&get(&f, "Group_Half")),
                group_empty: model_of(&get(&f, "Group_Empty")),
                single: model_of(&get(&f, "Single_Full")),
                single_half: model_of(&get(&f, "Single_Half")),
                single_empty: model_of(&get(&f, "Single_Empty")),
                book_price: 0,
                snack,
                microwave: get(&f, "Recipe_Class") == "MicrowaveMeal",
                for_one: !get(&f, "Can_Make_Single").is_empty(),
                for_group: !get(&f, "Can_Make_Group").is_empty(),
                key,
            });
        }
        out.recipes.append(&mut snacks);
        // Recipe books (`Books`' `BookRecipe` rows) and their prices.
        if let Some(b) = xml("Books") {
            for f in records(&b, "BookRecipe") {
                let key = get(&f, "Recipe");
                if let Some(r) = out.recipes.iter_mut().find(|r| r.key == key) {
                    r.book_price = num(&f, "Value") as i32;
                }
            }
        }
        // The food models, and their textures.
        let g = root.global_dir();
        std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
        let mut fpack = PackWriter::create(&g.join("food.pack")).map_err(|e| e.to_string())?;
        let mut tex = BTreeSet::new();
        for (key, modl, state) in &models {
            let m = crate::bake::bake_model_state(pkgs, modl, (*state != 0).then_some(*state));
            tex.extend(m.parts.iter().filter_map(|p| p.texture));
            fpack.add(*key, &m).map_err(|e| e.to_string())?;
        }
        fpack.finish().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.textures_dir()).ok();
        for t in tex {
            if !root.tex_path(t).exists()
                && let Some(dds) = crate::bake::bake_texture(pkgs, t, crate::bake::OBJECT_TEX_MAX, false)
            {
                let _ = std::fs::write(root.tex_path(t), dds);
            }
        }
    }
    progress("Converting: career outfits…");
    bake_outfits(root, pkgs, &out.careers)?;
    progress("Converting: clothes' colourways…");
    let ways = bake_cas_colourways(root, pkgs)?;
    progress(&format!("Converting: colourways for {ways} clothes…"));
    let faces = bake_face_bones(root, pkgs)?;
    progress(&format!("Converting: face shapes ({faces} slider bones)…"));
    let eyes = bake_eye_colors(root, pkgs)?;
    progress(&format!("Converting: eye colours ({eyes} presets)…"));
    progress("Converting: object designs…");
    let designs = bake_object_designs(root, pkgs)?;
    progress(&format!("Converting: designs for {designs} objects…"));
    let paintings = bake_paintings(root, pkgs, xml("PaintingSkillDB"))?;
    progress(&format!("Converting: {paintings} paintings…"));
    let skins = bake_object_skins(root, pkgs)?;
    progress(&format!("Converting: {skins} objects with moving parts…"));
    progress("Converting: fences…");
    out.fences = bake_fence_styles(root, pkgs, &strings)?;
    progress("Converting: garden trees and flowers…");
    out.garden = bake_garden(root, pkgs);
    // Catalogue models with alternative geometry states, drawn in their fullest (the objects'
    // models pack has every state at once: a chess table's every game piled on its board).
    progress("Converting: object states…");
    {
        let catalog: Vec<crate::types::CatalogEntry> = read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        let mut keys: Vec<crate::types::Key> = catalog.iter().flat_map(|c| c.models.iter().copied()).collect();
        keys.sort();
        keys.dedup();
        let baked = crate::bake::par_map(&keys, |k| {
            let rk = crate::types::rkey(*k);
            let meshes = s3formats::model::load_model(pkgs, &rk)?;
            let state = crate::bake::default_state(&meshes)?;
            Some((*k, crate::bake::bake_model_state(pkgs, &rk, Some(state))))
        });
        let g = root.global_dir();
        std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
        let mut spack = PackWriter::create(&g.join("states.pack")).map_err(|e| e.to_string())?;
        for (k, m) in baked.into_iter().flatten() {
            spack.add(k, &m).map_err(|e| e.to_string())?;
        }
        spack.finish().map_err(|e| e.to_string())?;
    }
    // Lifetime wishes: the base game's lifetime dreams, with their instances' scores.
    if let (Some(nodes), Some(inst)) = (xml("DreamsAndPromisesNodes"), xml("DreamNodeInstanceDefaults")) {
        let mut scores: HashMap<String, u32> = HashMap::new();
        for f in records(&inst, "DreamNodeInstance") {
            if let Some(s) = f.get("FulfillmentScore").and_then(|s| s.parse().ok()) {
                scores.entry(get(&f, "PrototypeId")).or_insert(s);
            }
        }
        for f in records(&nodes, "Primitives") {
            if get(&f, "LifeEventIsLifetimeDream") != "True" || get(&f, "RequiredProductVersions") != "BaseGame" {
                continue;
            }
            let id = get(&f, "Id");
            out.lifetime_wishes.push(LifetimeWishInfo {
                id: id.parse().unwrap_or(0),
                check: get(&f, "CheckFunction"),
                number: num(&f, "LifeEventInputNumber"),
                icon: get(&f, "PrimaryIcon"),
                score: scores.get(&id).copied().unwrap_or(0),
            });
        }
    }
    if let Some(x) = xml("Fishing") {
        for f in records(&x, "Fish") {
            let key = get(&f, "Name");
            if key.is_empty() || !base(&f) {
                continue;
            }
            let name = text("Fishing/Fish", &key);
            out.collectibles.push(CollectibleInfo {
                name: if name.is_empty() { pretty(&key) } else { name },
                min_price: num(&f, "MinPrice") as i32,
                max_price: num(&f, "MaxPrice") as i32,
                rarity: get(&f, "Rarity"),
                level: num(&f, "Level") as u8,
                model: get(&f, "Model_Name"),
                kind: CollectKind::Fish,
                bait: get(&f, "Bait"),
                // (As the game reads them: a tenth of a pound to ten by default.)
                weight: (
                    get(&f, "MinWeight").parse().unwrap_or(0.1),
                    get(&f, "MaxWeight").parse().unwrap_or(10.0),
                ),
                over_max: get(&f, "OverMax").eq_ignore_ascii_case("true"),
                key,
            });
        }
        for f in records(&x, "Spawner") {
            let class = short(&get(&f, "SpawnerClassName"));
            if class.is_empty() {
                continue;
            }
            out.spawners.push(SpawnerInfo {
                class,
                items: weights(&get(&f, "ActiveFish"), &get(&f, "ActiveProbability")),
                capacity: num(&f, "ActiveMaxFish").max(1.0) as u8,
                hours: (0.25, 1.0),
            });
        }
    }

    // Garden plants (the base game's everyday ones) and their produce.
    if let (Some(p), Some(ing)) = (xml("plants"), xml("ingredients")) {
        let heights: HashMap<String, String> = records(&p, "MedatorInstanceList").into_iter().map(|f| (get(&f, "MedatorName"), get(&f, "PlantHeight"))).collect();
        let produce: HashMap<String, (String, i64, String)> = records(&ing, "Data")
            .into_iter()
            .filter(|f| f.get("CodeVersion").is_none_or(|v| v == "BaseGame"))
            .filter_map(|f| Some((f.get("Plant_Name").filter(|s| !s.is_empty())?.clone(), (get(&f, "Ingredient_Key"), num(&f, "Price") as i64, get(&f, "Model_Name")))))
            .collect();
        // The produce's models by name (`tomatoesWhole#tomatoWhole`: that model in that state).
        // (A name is shared by a model's rig and footprint: only the model's instance counts.)
        let mut names: HashMap<String, u64> = HashMap::new();
        for k in pkgs.keys_of_type(T_NMAP) {
            if let Some(d) = pkgs.read(k) {
                for (i, n) in s3formats::audio::parse_name_map(&d) {
                    if pkgs.find_ti(s3pkg::types::MODL, i).is_some() {
                        names.insert(n.to_ascii_lowercase(), i);
                    }
                }
            }
        }
        // (Or a catalogue object of that name: `apple`.)
        let catalog: Vec<crate::types::CatalogEntry> = read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        let g = root.global_dir();
        std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
        let mut ppack = PackWriter::create(&g.join("produce.pack")).map_err(|e| e.to_string())?;
        let mut baked_models: BTreeSet<crate::types::Key> = BTreeSet::new();
        let mut produce_model = |spec: &str| -> Option<crate::types::Key> {
            let (name, state) = spec.split_once('#').map_or((spec, None), |(n, s)| (n, Some(s)));
            let modl = match names.get(&name.to_ascii_lowercase()) {
                Some(&i) => pkgs.find_ti(s3pkg::types::MODL, i)?,
                None => crate::types::rkey(*catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(name))?.models.first()?),
            };
            let hash = state.map_or(0, s3pkg::fnv32);
            let key = (s3pkg::types::MODL, hash, modl.i);
            if baked_models.insert(key) {
                let m = crate::bake::bake_model_state(pkgs, &modl, state.map(s3pkg::fnv32));
                for t in m.parts.iter().filter_map(|p| p.texture) {
                    if !root.tex_path(t).exists()
                        && let Some(dds) = crate::bake::bake_texture(pkgs, t, crate::bake::OBJECT_TEX_MAX, false)
                    {
                        let _ = std::fs::write(root.tex_path(t), dds);
                    }
                }
                ppack.add(key, &m).ok()?;
            }
            Some(key)
        };
        // (What the special plants bear that isn't food: a Death Flower, a money bag.)
        let not_food: HashMap<String, (String, i64, String)> = HashMap::from([
            ("Death Flower Bush".to_string(), ("Death Flower".to_string(), 25, "DeathFlower".to_string())),
            ("Money Tree".to_string(), ("Money Bag".to_string(), 0, "HarvestMoneyBag".to_string())),
        ]);
        for f in records(&p, "PlantList") {
            let name = get(&f, "PlantName");
            let rarity = get(&f, "Rarity");
            // (The special ones found about town; not those only from the store.)
            if name.is_empty()
                || f.get("CodeVersion").is_some_and(|v| v != "BaseGame")
                || !matches!(rarity.as_str(), "Common" | "Uncommon" | "Rare" | "Special")
                || get(&f, "LimitedAvailability").eq_ignore_ascii_case("true")
            {
                continue;
            }
            let Some((produce, price, spec)) = produce.get(&name).or_else(|| not_food.get(&name)).cloned() else { continue };
            let seed = text("Plants/PlantList", &get(&f, "LocalizationKeySeedName"));
            let seed = if seed.is_empty() { format!("{} Seed", if rarity == "Special" { name.trim_end_matches(" Bush") } else { produce.as_str() }) } else { seed };
            let model = get(&f, "MedatorName");
            let produce_model = produce_model(&spec);
            out.plants.push(PlantInfo {
                height: heights.get(&model).cloned().unwrap_or_else(|| "Medium".into()),
                name,
                rarity,
                seed,
                model,
                produce,
                price,
                harvest_min: num(&f, "NumHarvestablesMin") as u32,
                harvest_max: num(&f, "NumHarvestablesMax") as u32,
                lifetime: num(&f, "NumLifetimeHarvestables") as u32,
                water_decay: num(&f, "WaterDecay"),
                weeds: num(&f, "WeedProblem"),
                skill_plant: num(&f, "SkillPointsPlant"),
                skill_harvest: num(&f, "SkillPointsHarvest"),
                produce_model,
            });
        }
        ppack.finish().map_err(|e| e.to_string())?;
    }
    // The ingredients, and the fish (as ingredients: a kind of Fish).
    if let Some(ing) = xml("ingredients") {
        for f in records(&ing, "Data") {
            let key = get(&f, "Ingredient_Key");
            if key.is_empty() || !f.get("CodeVersion").is_none_or(|v| v.is_empty() || v == "BaseGame") {
                continue;
            }
            let name = text("Ingredients/Data", &get(&f, "Ingredient_Name"));
            out.ingredients.push(IngredientInfo {
                name: if name.is_empty() { key.clone() } else { name },
                parent: get(&f, "Parent"),
                kind: !get(&f, "Abstract").is_empty(),
                price: num(&f, "Price") as i32,
                can_buy: get(&f, "CanBuyFromStore").eq_ignore_ascii_case("true"),
                grown: !get(&f, "Plant_Name").is_empty(),
                tab: get(&f, "UiTab"),
                key,
            });
        }
    }
    if let Some(x) = xml("Fishing") {
        for f in records(&x, "Fish") {
            let key = get(&f, "Name");
            if key.is_empty() || !f.get("CodeVersion").is_none_or(|v| v.is_empty() || v == "BaseGame") || out.ingredients.iter().any(|i| i.key == key) {
                continue;
            }
            let can_buy = get(&f, "CanBuyFromStore").eq_ignore_ascii_case("true");
            let name = out.collectibles.iter().find(|c| c.key == key).map_or(key.clone(), |c| c.name.clone());
            out.ingredients.push(IngredientInfo {
                name,
                parent: "Fish".into(),
                kind: false,
                // (Sold at its least, if at all.)
                price: if can_buy { num(&f, "MinPrice") as i32 } else { 0 },
                can_buy,
                grown: false,
                tab: "Fish".into(),
                key,
            });
        }
    }

    // Opportunities (the base game's), the ones done at a rabbit hole.
    if let Some(x) = xml("Opportunities_BaseGame") {
        let by_guid = |sheet: &str| -> HashMap<String, HashMap<String, String>> {
            records(&x, sheet).into_iter().filter_map(|f| Some((f.get("GUID").filter(|g| !g.is_empty())?.clone(), f))).collect()
        };
        let (setup, reqs, done, names) = (by_guid("OpportunitiesSetup"), by_guid("OpportunitiesRequirements"), by_guid("OpportunitiesCompletion"), by_guid("Names"));
        // The text keys' workbook isn't the XML's name: try the likely ones.
        let opp_text = |key: &str| -> String {
            if key.is_empty() {
                return String::new();
            }
            ["Opportunities_BaseGame/Names", "Opportunities/Names", "Opportunities_BaseGame/OpportunitiesSetup", "Opportunities/OpportunitiesSetup"]
                .iter()
                .find_map(|t| strings.get(&s3pkg::fnv64(&format!("Gameplay/Excel/{t}:{key}").to_ascii_lowercase())).cloned())
                .unwrap_or_default()
        };
        let hour = |t: &str| -> f32 {
            let t = t.trim().to_ascii_uppercase();
            let pm = t.ends_with("PM");
            let core = t.trim_end_matches("AM").trim_end_matches("PM");
            let (h, m) = core.split_once(':').unwrap_or((core, "0"));
            let h: f32 = h.parse().unwrap_or(0.0);
            let m: f32 = m.parse().unwrap_or(0.0);
            (h % 12.0) + if pm { 12.0 } else { 0.0 } + m / 60.0
        };
        let careers = [
            ("BusinessCareer", "Business"),
            ("CriminalCareer", "Criminal"),
            ("CulinaryCareer", "Culinary"),
            ("PoliticalCareer", "Political"),
            ("AthleticCareer", "ProfessionalSports"),
            ("ScienceCareer", "Science"),
            ("JournalismCareer", "Journalism"),
            ("LawEnforcement", "LawEnforcement"),
            ("MilitaryCareer", "Military"),
            ("MusicCareer", "Music"),
            ("MedicalCareer", "Medical"),
        ];
        let mut guids: Vec<&String> = setup.keys().collect();
        guids.sort();
        for g in guids {
            let s = &setup[g];
            // (Only the ones that are done by going there: some also need a feat first, which
            // the game listens for with its event listeners.)
            // (Nor ones that need things brought along: paintings, fish, produce.)
            if get(s, "CompletionEvent") != "kVisitedRabbitHole"
                || get(s, "Target") != "RabbitHole"
                || !get(s, "Object").is_empty()
                || !get(s, "EventListenerInfo1").is_empty()
                || !get(s, "TargetInteractionItemRequired").is_empty()
                || !get(s, "TargetInteractionNumberItemsRequired").is_empty()
            {
                continue;
            }
            let mut o = OpportunityInfo { guid: g.clone(), ..Default::default() };
            o.rabbit_hole = get(s, "TargetData");
            o.icon = get(s, "Icon");
            o.minutes = num(s, "TargetInteractionLength").max(15.0);
            o.open = s.get("TargetInteractionStartTime").map_or(0.0, |t| hour(t));
            o.close = s.get("TargetInteractionEndTime").map_or(24.0, |t| hour(t));
            if o.close <= o.open {
                o.close = 24.0;
            }
            o.days = if get(s, "Timeout") == "SimDays" { num(s, "TimeoutData") } else { 0.0 };
            o.repeat = get(s, "RepeatLevel") == "Always";
            let prefix = g.split('_').next().unwrap_or("");
            if get(s, "OpportunityType") == "Skill" {
                // (The skill comes from the requirements.)
            } else if let Some((_, c)) = careers.iter().find(|(p, _)| *p == prefix) {
                o.career = c.to_string();
            } else {
                continue;
            }
            // Requirements: only the ones this game can tell.
            let mut ok = true;
            for (k, v) in reqs.get(g).into_iter().flatten() {
                if !k.starts_with("Requirement") || v.is_empty() {
                    continue;
                }
                let p: Vec<&str> = v.split(',').map(|x| x.trim()).collect();
                match p[0] {
                    "Skill" if p.len() >= 4 => {
                        o.skill = p[1].to_string();
                        o.skill_min = p[2].parse().unwrap_or(0);
                        o.skill_max = p[3].parse().unwrap_or(10);
                    }
                    "WorldHasRabbitHoleType" => {}
                    _ => ok = false,
                }
            }
            if !ok || (o.career.is_empty() && o.skill.is_empty()) {
                continue;
            }
            for (k, v) in done.get(g).into_iter().flatten() {
                if !k.starts_with("CompletionWinReward") && !k.starts_with("CompletionModifier") {
                    continue;
                }
                let p: Vec<&str> = v.split(',').map(|x| x.trim()).collect();
                let n = |i: usize| p.get(i).and_then(|x| x.parse::<f32>().ok()).unwrap_or(0.0);
                match p[0] {
                    "Money" => o.money += n(1) as i64,
                    "CareerPerformance" => o.performance += n(1),
                    "CareerRaise" => o.raise += n(1),
                    "SkillPercentage" => o.skill_reward += n(2).max(n(1)),
                    "Skill" => o.skill_reward += n(2),
                    _ => {}
                }
            }
            let nm = names.get(g);
            let key = |k: &str| nm.map(|f| get(f, k)).unwrap_or_default();
            o.name = opp_text(&key("OpportunityName"));
            o.desc = opp_text(&key("OpportunityDescription"));
            o.completion = opp_text(&key("CompletionText"));
            o.failure = opp_text(&key("FailureText"));
            o.interaction = opp_text(&get(s, "TargetInteractionName"));
            if o.name.is_empty() {
                continue;
            }
            out.opportunities.push(o);
        }
    }

    // Balloons: each table's rows in order; a row without a key continues the list above it.
    if let Some(b) = xml("Balloons") {
        for (row, key_tag, table) in [
            ("Idle", "IdleKey", &mut out.balloons.idle),
            ("Social", "ActionKey", &mut out.balloons.social),
            ("Topic", "Key", &mut out.balloons.topic),
            ("Random", "Key", &mut out.balloons.random),
        ] {
            let mut current = String::new();
            for f in records(&b, row) {
                let key = get(&f, key_tag);
                if !key.is_empty() {
                    current = key;
                }
                let (icon, refkey) = (get(&f, "BalloonName"), get(&f, "ReferencedKey"));
                if current.is_empty() || (icon.is_empty() && refkey.is_empty()) {
                    continue;
                }
                let axis = match f.get("BalloonAxis").map(String::as_str) {
                    Some("kLike") => 1,
                    Some("kDislike") => 2,
                    _ => 0,
                };
                let weight = f.get("Weight").and_then(|w| w.parse().ok()).unwrap_or(1.0);
                table.entry(current.clone()).or_default().push(BalloonEntry { icon, refkey, axis, weight });
            }
        }
    }

    // Seasons: the weather's tuning table (its first row of each kind holds the defaults).
    if let Some(x) = xml("Seasons") {
        let n = |f: &HashMap<String, String>, k: &str, d: f32| f.get(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d);
        for f in records(&x, "Temperature").into_iter().filter(|f| f.get("Season").is_some_and(|s| !s.is_empty())) {
            let pair = |p: &str| [n(&f, &format!("{p}Min"), 0.0), n(&f, &format!("{p}Max"), 0.0)];
            out.seasons.temperature.push((get(&f, "Season"), [pair("Morning"), pair("Noon"), pair("Evening"), pair("Night")]));
        }
        // (Its rows hold a field of the row's own name: read as pairs.)
        // (Each field's text and where it ends; the first row's are empty.)
        let field = |s: &str, tag: &str| -> Option<(Option<f32>, usize)> {
            let a = s.find(&format!("<{tag}>"))? + tag.len() + 2;
            let b = a + s[a..].find('<')?;
            Some((s[a..b].trim().parse().ok(), b))
        };
        let mut rest = x.split_once("<FreezeMeltRate>").map_or("", |(_, r)| r);
        while let Some((t, at)) = field(rest, "Temperature") {
            rest = &rest[at..];
            let Some((r, at)) = field(rest, "FreezeMeltRate") else { break };
            rest = &rest[at..];
            if let (Some(t), Some(r)) = (t, r) {
                out.seasons.freeze_melt.push([t, r]);
            }
        }
        for kind in ["Sunny", "Rain", "Snow", "Fog", "Hail"] {
            let rows = records(&x, &format!("{kind}Tuning"));
            let Some(def) = rows.iter().find(|f| f.get("Season").is_none_or(|s| s.is_empty())).cloned() else { continue };
            let v = |f: &HashMap<String, String>, k: &str| f.get(k).filter(|s| !s.trim().is_empty()).or(def.get(k)).and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(0.0);
            for f in rows.iter().filter(|f| f.get("Season").is_some_and(|s| !s.is_empty())) {
                out.seasons.weather.push(WeatherProfile {
                    kind: kind.to_string(),
                    season: get(f, "Season"),
                    weight: v(f, "Weight"),
                    min_length: v(f, "MinLength"),
                    max_length: v(f, "MaxLength"),
                    min_temp: if f.contains_key("MinTemp") || def.contains_key("MinTemp") { v(f, "MinTemp") } else { -1000.0 },
                    max_temp: if f.contains_key("MaxTemp") || def.contains_key("MaxTemp") { v(f, "MaxTemp") } else { 1000.0 },
                    intensity: [v(f, "LightWeight"), v(f, "ModerateWeight"), v(f, "HeavyWeight")],
                });
            }
        }
    }
    if let Some(x) = xml("SeasonsManager+TerrainCoverManager") {
        // (`<kRainDryTime value="4">`.)
        let mut rest = x.as_str();
        while let Some(a) = rest.find("<k") {
            let tag = &rest[a + 1..];
            let Some(sp) = tag.find(' ') else { break };
            let name = &tag[..sp];
            if let Some(v) = tag[sp..].strip_prefix(" value=\"").and_then(|r| r.split('"').next()) {
                let vals: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if !vals.is_empty() {
                    out.seasons.cover.insert(name.to_string(), vals);
                }
            }
            rest = &tag[sp..];
        }
    }

    // Icons: everything these tables name, plus interface pieces used directly.
    progress("Converting: interface icons…");
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    // (The fairies' wings, as the Supernatural pack's Create a Sim pictures them.)
    wanted.extend((1..=12).map(|i| format!("FairyWings{i:02}")));
    for b in &out.buffs {
        wanted.insert(b.icon.clone());
    }
    for t in &out.traits {
        wanted.insert(t.icon.clone());
        wanted.insert(t.icon_small.clone());
    }
    for s in &out.skills {
        wanted.extend([s.icon.clone(), s.wish_icon.clone(), s.object_icon.clone()]);
    }
    for c in &out.careers {
        wanted.insert(c.icon.clone());
    }
    for o in &out.opportunities {
        wanted.insert(o.icon.clone());
    }
    for w in &out.lifetime_wishes {
        wanted.insert(w.icon.clone());
    }
    wanted.extend(EXTRA_ICONS.iter().map(|s| s.to_string()));
    // Create a Sim's favourites: a picture of each food (by recipe), music and colour.
    for r in &out.recipes {
        wanted.insert(format!("cas_favorites_food_i_{}_r2", r.key.to_ascii_lowercase()));
    }
    for m in FAVORITE_MUSIC {
        wanted.insert(format!("cas_favorites_music_i_{m}_r2"));
    }
    for c in FAVORITE_COLORS {
        wanted.insert(format!("cas_favorites_color_i_{c}_r2"));
    }
    wanted.remove("");
    let g = root.global_dir();
    std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
    let mut pack = PackWriter::create(&g.join("icons.pack")).map_err(|e| e.to_string())?;
    let mut n = 0;
    for name in &wanted {
        let lower = name.to_ascii_lowercase();
        if let Some(png) = pkgs.read_ti(T_ICON, s3pkg::fnv64(&lower)) {
            pack.add(icon_key(name), &png).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // The catalogue's object thumbnails (the first colour variant of each), for buy mode: the
    // base game's and each installed pack's (`EP*/Thumbnails`, `SP*/Thumbnails`).
    progress("Converting: catalogue thumbnails…");
    let mut dirs = vec![install_root.join("Thumbnails")];
    if let Ok(rd) = std::fs::read_dir(install_root) {
        let mut packs: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path().join("Thumbnails")).filter(|p| p.is_dir()).collect();
        packs.sort();
        dirs.extend(packs);
    }
    // (Objects, then Create-a-Sim parts.)
    for (file, kind, name) in [
        ("AllThumbnails.package", T_THUMB_LARGE, thumb_name as fn(u64) -> String),
        ("CasThumbnails.package", T_CAS_THUMB, cas_thumb_name as fn(u64) -> String),
    ] {
        let mut seen: BTreeSet<u64> = BTreeSet::new();
        for dir in &dirs {
            let Ok(tp) = Package::open(dir.join(file)) else { continue };
            let mut first: HashMap<u64, &s3pkg::IndexEntry> = HashMap::new();
            for e in tp.of_type(kind) {
                let keep = !seen.contains(&e.key.i) && first.get(&e.key.i).is_none_or(|f| e.key.g < f.key.g);
                if keep {
                    first.insert(e.key.i, e);
                }
            }
            for (i, e) in first {
                if let Ok(png) = tp.read(e) {
                    pack.add(icon_key(&name(i)), &png).map_err(|e| e.to_string())?;
                    seen.insert(i);
                    n += 1;
                }
            }
        }
    }

    // Map tags: the venue glyphs cut from the game's map-tag atlas (it has no layout table of its
    // own; these are the glyphs' places in it), stored as `maptag_<venue>`.
    if let Some(atlas) = pkgs.read_ti(T_ICON, s3pkg::fnv64(&"ATLAS_MapTagColors_00".to_ascii_lowercase())).and_then(|p| crate::bake::decode_png(&p)) {
        for (name, [x0, y0, x1, y1]) in MAP_TAG_GLYPHS {
            let (x0, y0) = (x0.saturating_sub(2), y0.saturating_sub(2));
            let (x1, y1) = ((x1 + 2).min(atlas.width), (y1 + 2).min(atlas.height));
            // Square, centred.
            let side = (x1 - x0).max(y1 - y0);
            let mut img = s3formats::dds::Rgba { width: side, height: side, data: vec![0; side * side * 4] };
            let (ox, oy) = ((side - (x1 - x0)) / 2, (side - (y1 - y0)) / 2);
            for y in y0..y1 {
                for x in x0..x1 {
                    let s = (y * atlas.width + x) * 4;
                    let d = ((y - y0 + oy) * side + (x - x0 + ox)) * 4;
                    img.data[d..d + 4].copy_from_slice(&atlas.data[s..s + 4]);
                }
            }
            pack.add(icon_key(&format!("maptag_{name}")), &encode_png(&img)).map_err(|e| e.to_string())?;
            n += 1;
        }
    }

    // Balloon pictures are textures: decoded and stored as PNG beside the interface icons.
    let mut balloon_icons: BTreeSet<String> = BALLOON_FRAMES.iter().chain(SKY_TEXTURES).map(|s| s.to_string()).collect();
    for t in [&out.balloons.idle, &out.balloons.social, &out.balloons.topic, &out.balloons.random] {
        for e in t.values().flatten() {
            balloon_icons.insert(e.icon.clone());
        }
    }
    balloon_icons.retain(|n| !n.is_empty() && !wanted.contains(n));
    let mut have: BTreeSet<String> = BTreeSet::new();
    for name in &balloon_icons {
        let Some(dds) = pkgs.read_ti(T_DDS, s3pkg::fnv64(&name.to_ascii_lowercase())) else { continue };
        let Some(img) = s3formats::dds::decode(&dds, 128) else { continue };
        pack.add(icon_key(name), &encode_png(&img)).map_err(|e| e.to_string())?;
        have.insert(name.clone());
        n += 1;
    }
    // Lists keep only the icons there are pictures for (and the special pickers).
    let known = |e: &BalloonEntry| !e.refkey.is_empty() || have.contains(&e.icon) || wanted.contains(&e.icon) || SPECIAL_PICKERS.contains(&e.icon.as_str());
    for t in [&mut out.balloons.idle, &mut out.balloons.social, &mut out.balloons.topic, &mut out.balloons.random] {
        for list in t.values_mut() {
            list.retain(known);
        }
        t.retain(|_, l| !l.is_empty());
    }
    pack.finish().map_err(|e| e.to_string())?;
    // The objects that can be stolen (a Kleptomaniac's swipes).
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&g.join("catalog.bin")).unwrap_or_default();
    out.stealable = crate::bake::par_map(&catalog.iter().collect::<Vec<_>>(), |c| {
        s3formats::object::parse_objd(&pkgs.read(&crate::types::rkey(c.objd))?).ok().filter(|o| o.stealable).map(|_| c.objd.2)
    })
    .into_iter()
    .flatten()
    .collect();
    out.stealable.sort_unstable();
    out.stealable.dedup();
    write_value(&g.join("gamedata.bin"), &out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// Encodes RGBA8 as PNG.
fn encode_png(img: &s3formats::dds::Rgba) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width as u32, img.height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    if let Ok(mut w) = enc.write_header() {
        let _ = w.write_image_data(&img.data);
    }
    out
}

/// The balloons themselves: thought clouds (the second for dreams) with the little bubbles that
/// lead up to them, speech balloons, and the like / dislike marks.
pub const BALLOON_FRAMES: &[&str] = &[
    "thought_balloon",
    "thought_balloon2",
    "thought_balloonLead",
    "speech_balloon",
    "speech_balloon2",
    "sb_like",
    "sb_dislike",
    "t_balloon_routefail",
];

/// Venue glyphs in `ATLAS_MapTagColors_00` (pixel rectangles x0, y0, x1, y1).
const MAP_TAG_GLYPHS: &[(&str, [usize; 4])] = &[
    ("home_active", [395, 512, 435, 547]),
    ("collectible", [232, 242, 262, 262]),
    ("home", [457, 461, 485, 486]),
    ("lot_empty", [344, 406, 373, 432]),
    ("gym", [64, 424, 93, 445]),
    ("library", [175, 463, 205, 485]),
    ("eatery", [14, 139, 31, 170]),
    ("show", [176, 573, 205, 600]),
    ("spa", [119, 459, 150, 490]),
    ("park", [10, 699, 35, 730]),
    ("pool", [61, 369, 95, 387]),
    ("museum", [119, 347, 149, 379]),
    ("cityhall", [285, 570, 320, 603]),
    ("science", [349, 348, 367, 379]),
    ("hospital", [455, 178, 487, 210]),
    ("grocery", [400, 408, 429, 429]),
    ("bookstore", [175, 463, 205, 485]),
    ("school", [61, 588, 96, 616]),
    ("police", [11, 362, 35, 394]),
    ("firestation", [285, 519, 320, 542]),
    ("military", [343, 522, 374, 539]),
    ("business", [17, 20, 48, 45]),
    ("graveyard", [293, 403, 312, 434]),
    ("stadium", [120, 292, 149, 321]),
    ("bar", [73, 304, 85, 342]),
];

/// The sky's textures: cloud noise, the night's stars, the sun and its halo, the moon's halo.
pub const SKY_TEXTURES: &[&str] = &["CloudNoiseBase", "NightSkyStarsFlat", "Sky_Sun", "Sky_SunHalo", "Sky_MoonHalo"];

/// Balloon "icons" that are really the game's pickers, resolved while playing.
pub const SPECIAL_PICKERS: &[&str] = &[
    "GetSpeechBalloonImageForChat",
    "Thumbnail Target",
    "Thumbnail Actor",
    "Actor Career Topic",
    "Target Career Topic",
    "GetSpeechBalloonIconForCareer",
    "GetSpeechBalloonIconForTargetCareer",
];

/// Interface pieces the game draws directly (not named by the tables).
/// The favourites Create a Sim offers besides foods (which are the recipes): music, and
/// colours (as the game's pictures name them).
pub const FAVORITE_MUSIC: [&str; 10] = ["classical", "custom", "electronica", "indie", "kids", "latin", "pop", "rockabilly", "roots", "soul"];
pub const FAVORITE_COLORS: [&str; 20] = [
    "aqua", "black", "blue", "green", "grey", "hotpink", "irishgreen", "lilac", "lime", "orange", "pink", "purple", "red", "seafoam", "spiceberry", "spicebrown",
    "turquoise", "violet", "white", "yellow",
];

const EXTRA_ICONS: &[&str] = &[
    "hud_icon_plumbob_r2",
    "hud_icon_plumbobGlow_r2",
    "moodlet_itsGirl",
    "moodlet_itsTwins",
    "skill_journal_full_star_r2",
    "hud_icon_maptagbase_r2",
    "hud_icon_maptagbase_shell",
    "skill_journal_empty_star_r2",
    // Wishes.
    "w_tv",
    "w_book",
    "w_chess",
    "w_stereo",
    "w_stove",
    "w_bathtub",
    "w_workout_bench",
    "w_painting",
    "w_guitar",
    "W_computer",
    "w_friend",
    "w_joke_around",
    "w_first_kiss",
    "w_wedding_arch",
    "w_career_cityhall",
    "w_simoleon",
    "w_simoleon_32",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn social_topics_offer_their_actions_to_the_sim_with_them_or_others() {
        let xml = "<X><TAA><Cat></Cat><Gr></Gr><FPA></FPA></TAA><TAA><FPA>Stray</FPA></TAA><TAA><Cat>Evil Trait</Cat></TAA><TAA><Gr>Default</Gr></TAA>\
            <TAA><FPA>Mastermind Plot</FPA><SPA>Accuse of Being Evil</SPA></TAA><TAA><Cat>Buff Fired</Cat></TAA><TAA><SPA>Ask About Work</SPA></TAA></X>";
        let t = social_topics(xml);
        assert_eq!(
            t,
            [
                ("Evil Trait".to_string(), "Mastermind Plot".to_string(), true),
                ("Evil Trait".to_string(), "Accuse of Being Evil".to_string(), false),
                ("Buff Fired".to_string(), "Ask About Work".to_string(), false),
            ]
        );
    }

    #[test]
    fn career_events_are_read_with_their_rows_defaults() {
        let xml = r#"<CareerEvents>
            <Business><EventType></EventType><EventName></EventName><SureShotEvent>0</SureShotEvent><BranchName>All</BranchName><ChanceToWin></ChanceToWin></Business>
            <Business><EventType>BusinessCareer_Memo</EventType><OpportunityName>BusinessCareer_Memo</OpportunityName><Likelihood>0.0875</Likelihood><AvailableLevels>3:5</AvailableLevels></Business>
            <Business><EventType>BusinessCareer_Golf</EventType><EventName>Sims3.Gameplay.Careers.Career+EventChoice, Sims3GameplaySystems</EventName><Description>Golf</Description><YesText>GolfYes</YesText><ChanceToWin>60</ChanceToWin><ChanceModifier1>Mood,30</ChanceModifier1><Likelihood>0.05</Likelihood><AvailableLevels>1,4</AvailableLevels><RewardYes1>RelationshipWithBoss,20</RewardYes1><RewardNo1>CareerPerformance,-10</RewardNo1></Business>
            <Business><EventType>BusinessCareer_Odd</EventType><EventName>Sims3.Gameplay.Careers.Business+ImpromptuMeeting, Sims3GameplaySystems</EventName></Business>
            <Medical><EventType>MedicalCareer_X</EventType><OpportunityName>X</OpportunityName></Medical>
        </CareerEvents>"#;
        let e = career_events(xml, "Business", &|k| if k.is_empty() { String::new() } else { format!("<{k}>") });
        assert_eq!(e.len(), 2, "the game's own classes left out");
        assert_eq!((e[0].kind.as_str(), e[0].levels.clone(), e[0].opportunity.as_str(), e[0].chance), ("Opportunity", vec![3, 4, 5], "BusinessCareer_Memo", 100.0));
        let golf = &e[1];
        assert_eq!((golf.kind.as_str(), golf.text[0].as_str(), golf.text[1].as_str(), golf.yes[0].as_str(), golf.chance, golf.branch.as_str()), ("Choice", "<Golf>", "<Golf_Female>", "<GolfYes>", 60.0, "All"));
        assert_eq!((golf.levels.clone(), golf.modifiers.clone(), golf.rewards_yes.clone(), golf.rewards_no.clone()), (vec![1, 4], vec!["Mood,30".to_string()], vec!["RelationshipWithBoss,20".to_string()], vec!["CareerPerformance,-10".to_string()]));
    }

    #[test]
    fn social_rules_are_read_as_the_game_has_them() {
        let xml = r#"<Social><Action key="Chat" com="Friendly"><SA key="Talk" /><AD val="0x00080512"><Misc ITLA="10" r="6" c="4" /></AD>
            <AA><AGE x="C,T,Y,A,E" y="C,T,Y,A,E" /><Available auto="True" udo="False" /></AA>
            <Rules>
            <RHS com="Friendly" state="friendly"><Anim /><TUN is="True" lvl="20" /></RHS>
            <RHS com="Boring" state="boring"><Anim /><TUN val="80" /><LTRBits val="0x800" rem="0x100" /><Topic teo="Work Pay" /></RHS>
            <LHS com="Boring" val="54" TP="2"><SC /><SE x="None" remX="None" y="None" remY="None" /><AGE x="T,Y,A,E" y="" /><Trait y="~Friendly" /><MAYBE rep="True" /></LHS>
            <LHS com="Friendly" val="53" TP="2"><SC stc="Awkward" val="True" /><Trait /><MAYBE /></LHS>
            <LHS com="Boring" val="52" TP="2"><SC com="Creepy" is="True" /><Trait /><MAYBE /></LHS>
            <LHS com="Awkward" val="1000" ProcTest="SociallyAwkwardChanceTest"><LLTR max="70" /><SC /><MAYBE rep="False" /></LHS>
            <LHS com="Amorous" ltro="5"><LLTR mood="0" min="10" /><SC /><MAYBE /></LHS>
            </Rules></Action></Social>"#;
        let chat = &social_actions(xml, &|k| k.to_string())[0];
        assert_eq!((chat.speech_act.as_str(), chat.chances, chat.recharges), ("Talk", 4, 6));
        assert_eq!(chat.outcomes, vec![
            // (`TUN is="True"`: the game's LearnRandomTrait.)
            SocialOutcome { result: "Friendly".into(), amount: 40.0, bits_add: 0, bits_remove: 0, charisma: -1.0, topics: Default::default(), learn_random: true, teaches: Default::default() },
            SocialOutcome { result: "Boring".into(), amount: 80.0, bits_add: 0x800, bits_remove: 0x100, charisma: -1.0, topics: [String::new(), String::new(), "Work Pay".into()], learn_random: false, teaches: Default::default() },
        ]);
        let r = &chat.rules;
        assert_eq!((r[0].result.as_str(), r[0].spec, r[0].third_party, r[0].repetition), ("Boring", 54.0, 2, 1));
        assert_eq!((r[0].target_traits.clone(), r[0].actor_ages.len()), (vec!["~Friendly".to_string()], 4));
        assert_eq!(r[1].st_context, "Awkward");
        assert_eq!(r[2].st_commodity, "Creepy");
        assert_eq!((r[3].liking, r[3].proc_test.as_str(), r[3].repetition), ((-100.0, 70.0), "SociallyAwkwardChanceTest", -1));
        // (Where the data gives none, the game's own reckoning: 1, +10 for the mood, +3.55 for the liking.)
        assert_eq!((r[4].mood, r[4].ltr_override), (0.0, 5.0));
        assert!((r[4].spec - 14.55).abs() < 1e-4);
        let gr = r#"<Socializing><GR><Key></Key><GC></GC><SpAct>None</SpAct><ThP>0</ThP><STCPos>0</STCPos><Mood>-101</Mood><LTRMin>-100</LTRMin><LTRMax>100</LTRMax><SkillLevel>1</SkillLevel><Spec>0</Spec><Result></Result></GR>
            <GR><Key>GR</Key><GC>Friendly</GC><STCPos>1</STCPos><LTRMin>-19</LTRMin><Spec>1.9</Spec><Result>Friendly</Result></GR>
            <GR><Key>GR</Key><GC>Insulting</GC><Result>Insulting</Result></GR></Socializing>"#;
        let rules = general_rules(gr);
        assert_eq!(rules.len(), 2);
        assert_eq!((rules[0].commodity.as_str(), rules[0].stc_positive, rules[0].liking, rules[0].spec), ("Friendly", 1, (-19.0, 100.0), 1.9));
        assert!((rules[1].spec - 0.9).abs() < 1e-6, "a general rule asking nothing");
    }

    #[test]
    fn social_menus_are_read_by_context_and_relationship() {
        let xml = r#"<SocializingActionAvailability><SAA><Cat></Cat><Gr></Gr><FPA></FPA><SPA></SPA></SAA>
            <SAA><Cat>Flirty</Cat></SAA><SAA><Gr>All</Gr></SAA><SAA><FPA>Wave</FPA></SAA>
            <SAA><Gr>Default</Gr></SAA><SAA><FPA>Flirt</FPA></SAA>
            <SAA><Gr>Partner</Gr></SAA><SAA><Gr>Spouse</Gr></SAA><SAA><FPA>Kiss</FPA></SAA><SAA><FPA>Make Out</FPA></SAA>
            <SAA><Cat>OK</Cat></SAA><SAA><Gr>Default</Gr></SAA><SAA><FPA>Chat</FPA></SAA></SocializingActionAvailability>"#;
        let m = social_menus(xml);
        let of = |stc: &str, g: &str| m.iter().find(|x| x.stc == stc && x.group == g).map(|x| x.socials.clone()).unwrap_or_default();
        assert_eq!(of("Flirty", "All"), vec!["Wave"]);
        assert_eq!(of("Flirty", "Default"), vec!["Flirt"]);
        assert_eq!(of("Flirty", "Partner"), vec!["Kiss", "Make Out"], "consecutive groups share what follows");
        assert_eq!(of("Flirty", "Spouse"), vec!["Kiss", "Make Out"]);
        assert_eq!(of("OK", "Default"), vec!["Chat"]);
    }

    #[test]
    fn stoves_cook_as_tuned() {
        let xml = r#"<base><kStoveTuning><ChanceOfRandomFailureMultiplier value="1.5" /><ApplianceBonusFoodPoints value="0" /><ChanceOfRandomFireAfterFailure value="20" /></kStoveTuning></base>"#;
        assert_eq!(appliance_cooking("StoveCheap", xml), Some(ApplianceCooking { class: "StoveCheap".into(), bonus_points: 0.0, failure: 1.5, fire: 20.0 }));
        assert!(appliance_cooking("Chair", "<base />").is_none());
    }

    #[test]
    fn interaction_tuning_gives_need_changes() {
        let xml = r#"<base><Interaction name="Sims3.Gameplay.Objects.Plumbing.Toilet+UseToilet+Definition" /><Object name="Sims3.Gameplay.Objects.Plumbing.ToiletCheap" /><Output>
            <Change type="Bladder" advertised="200" locked="True" actual="200" updateType="ImmediateDelta" timeDependsOn="False" />
            <Change type="Hygiene" advertised="-10" locked="True" actual="-10" updateType="ImmediateDelta" />
            <Change type="Fun" advertised="5" locked="True" actual="5" updateType="ContinuousFlow" />
            <Change type="SkillLogic" advertised="5" locked="True" actual="5" updateType="ContinuousFlow" />
            <Change type="Energy" advertised="0" locked="True" actual="0" updateType="ContinuousFlow" /></Output></base>"#;
        let r = interaction_rates(xml).unwrap();
        assert_eq!((r.class.as_str(), r.interaction.as_str()), ("ToiletCheap", "UseToilet"));
        assert_eq!(r.once, [0.0, 200.0, 0.0, 0.0, -10.0, 0.0]);
        assert_eq!(r.per_hour, [0.0, 0.0, 0.0, 0.0, 0.0, 5.0]);
        assert!(interaction_rates(r#"<base><Interaction name="A+B+Definition" /><Object name="C" /></base>"#).is_none(), "no need changes");
        let own = interaction_rates(r#"<base><Interaction name="Sims3.Gameplay.Objects.Electronics.PlayComputerGames+Definition" /><Object name="X.ComputerCheap" /><Change type="Fun" advertised="40" locked="False" actual="35" updateType="ContinuousFlow" /></base>"#).unwrap();
        assert_eq!(own.interaction, "PlayComputerGames", "an interaction of its own, not inside the object's class");
    }

    #[test]
    fn repair_tuning_is_read_from_an_objects_tuning() {
        let xml = r#"<base><HoursUnbreakable value="48" /><StartPercentChanceBreak value="6" /><PercentChanceBreakIncrease value="3" /><RepairComplexity value="5" /><MinRepairTime value="25" /><MaxRepairTime value="120" /><ShouldShockInPuddle value="True" /><MinChanceOfElectrocution value="0" /><MaxChanceOfElectrocution value="7" /><RepairSkillGain value="1500" /><UpgradeBoostChannelTuning><SkillGainRate value="5" /></UpgradeBoostChannelTuning><UpgradeUnbreakable><UpgradeTime value="240" /><SkillGainRate value="2640" /></UpgradeUnbreakable></base>"#;
        let t = repair_tuning("TVCheap", xml).unwrap();
        assert_eq!((t.hours_unbreakable, t.start_break, t.break_increase, t.complexity), (48.0, 6.0, 3.0, 5.0));
        assert_eq!((t.min_time, t.max_time, t.min_shock, t.max_shock, t.shock_in_puddle), (25.0, 120.0, 0.0, 7.0, true));
        assert_eq!((t.repair_skill, t.upgrade_skill), (1500.0, 2640.0), "the never-breaks upgrade's, not another's");
        assert!(repair_tuning("Chair", "<base><StartPercentChanceBreak value=\"0\" /></base>").is_none(), "what doesn't break");
    }

    #[test]
    fn topic_infos_take_the_first_rows_defaults() {
        let xml = "<X><ActiveTopic><Name></Name><AvailableForOtherSim>0</AvailableForOtherSim><UsedUpAfterDoneOnce>1</UsedUpAfterDoneOnce><Duration>0</Duration></ActiveTopic>            <ActiveTopic><Name>Birthday</Name><AvailableForOtherSim>1</AvailableForOtherSim><UsedUpAfterDoneOnce>0</UsedUpAfterDoneOnce><Duration>6</Duration></ActiveTopic>            <ActiveTopic><Name>Woken Up</Name><Duration>2</Duration></ActiveTopic></X>";
        let t = topic_infos(xml);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0], TopicInfo { name: "Birthday".into(), hours: 6.0, for_other: true, used_up: false });
        assert_eq!(t[1], TopicInfo { name: "Woken Up".into(), hours: 2.0, for_other: false, used_up: true });
    }

    #[test]
    fn social_actions_are_read_from_the_games_tables() {
        let xml = r#"<Social>
	<Action key="Accuse of Being Evil" com="Insulting">
		<Text key="AccuseofBeingEvil" pvt="BeAccused" />
		<AA>
			<AGE x="T,Y,A,E" y="C,T,Y,A,E" />
			<ALTR min="-19" max="100" />
			<AKTT val="Evil" />
			<ATRA val="Evil" />
			<ATREQ val="Good,BotAngryModeHiddenTrait,EvilChip" />
			<ATR val="Coward" />
			<ATE val="Good" />
			<Available auto="True" udo="False" />
			<ProcTest key="TestSomething" />
		</AA>
		<Jazz>
			<Graph val="social_generic" suite="Accuse" data="BaseGame" x="Bad" y="Bad" />
		</Jazz>
	</Action>
	<Action key="Chat" com="Friendly">
		<AA><AGE x="C,T" y="C,T" /><Available auto="False" udo="True" /></AA>
	</Action>
	<Action key="Ask About Day" com="Friendly">
		<AA oaend="1"><AGE x="T,Y" y="T" /></AA>
	</Action>
</Social>"#;
        let a = social_actions(xml, &|k| if k == "AccuseofBeingEvil" { "Accuse of Being Evil".into() } else { String::new() });
        assert_eq!(a.len(), 3);
        assert_eq!(a[2].actor_ages, ["T", "Y"], "an `<AA>` with attributes read too");
        assert_eq!((a[0].name.as_str(), a[0].tone.as_str()), ("Accuse of Being Evil", "Insulting"));
        assert_eq!(a[0].actor_ages, ["T", "Y", "A", "E"]);
        assert_eq!(a[0].target_ages.len(), 5);
        assert_eq!(a[0].liking, Some((-19.0, 100.0)));
        assert_eq!((a[0].target_traits[0].as_str(), a[0].keen[0].as_str(), a[0].test.as_str()), ("Evil", "Good", "TestSomething"));
        assert_eq!((a[0].restrict_auto.as_slice(), a[0].restrict.as_slice()), (["Evil".to_string()].as_slice(), ["Coward".to_string()].as_slice()));
        assert_eq!(a[0].required, ["Good"], "the robots' own traits left out");
        assert!(a[0].autonomous && !a[0].user_only);
        assert_eq!((a[0].graph.as_str(), a[0].suite.as_str()), ("social_generic", "Accuse"));
        // (No text: its key; no animations: none.)
        assert_eq!(a[1].name, "Chat");
        assert!(a[1].user_only && a[1].suite.is_empty());
    }

    #[test]
    fn idle_rows_group_under_their_name_and_take_the_first_rows_defaults() {
        let xml = "<Idles>            <TraitIdle><Trait></Trait><AnimationName></AnimationName><AnimationProbability>10</AnimationProbability><AgesSpeciesAllowed>T,Y,A,E</AgesSpeciesAllowed><CASOnly>False</CASOnly><TraitRequired>Unknown</TraitRequired></TraitIdle>            <TraitIdle><Trait>Grumpy</Trait><AnimationName>a_idle_grumpy_muttering01_x</AnimationName><AgesSpeciesAllowed>C,T,Y,A,E</AgesSpeciesAllowed><ThoughtBalloon>TraitGrumpy</ThoughtBalloon></TraitIdle>            <TraitIdle><Trait></Trait><AnimationName>a_idle_grumpy_muttering02_x</AnimationName></TraitIdle>            <TraitIdle><Trait>GreenThumb</Trait><AnimationName>a_trait_greenThumb_x</AnimationName><CASOnly>True</CASOnly></TraitIdle>            <TraitIdle><Trait>Neat</Trait><AnimationName>a_start_x,a_loop_x,a_stop_x</AnimationName><TraitRequired>Neat</TraitRequired></TraitIdle>            </Idles>";
        let groups = idle_rows(xml, "TraitIdle", "Trait");
        let names: Vec<&str> = groups.iter().map(|g| g.0.as_str()).collect();
        assert_eq!(names, ["Grumpy", "Neat"], "Create a Sim's alone (GreenThumb) left out");
        let grumpy = &groups[0].1;
        assert_eq!(grumpy.len(), 2, "the unnamed row after Grumpy is Grumpy's too");
        assert_eq!(grumpy[0].ages, ["C", "T", "Y", "A", "E"]);
        assert_eq!(grumpy[1].ages, ["T", "Y", "A", "E"], "left out: the first row's");
        assert_eq!(grumpy[1].probability, 10);
        assert_eq!(grumpy[0].balloon, "TraitGrumpy");
        assert!(grumpy[0].trait_required.is_empty(), "Unknown is no trait");
        assert_eq!(groups[1].1[0].clips, ["a_start_x", "a_loop_x", "a_stop_x"]);
        assert_eq!(groups[1].1[0].trait_required, "Neat");
    }
}
