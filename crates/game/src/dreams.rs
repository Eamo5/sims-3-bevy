//! The game's wishes ("Dreams and Promises", its `DreamsAndPromisesManager`), from its own dream
//! trees (`s3bake::dnp`). Each household Sim, a child or older, has every tree armed: its root
//! listens for its event (or is true already: a Sim's born, has the trait); a node done, its
//! children are weighed (by age, traits, skills, career, mood, the subject's relationship, as the
//! node says) and armed, and one visible child per happening (a skill up, a social, a purchase,
//! a morning's waking) is offered by weighted chance (a "Null Dream" picked, nothing is). Three
//! offers show at once, each for six hours; promised (four slots), a wish lasts till it's done
//! or given up, and done pays its points (its score, times 1.15 for the Ambitious, rounded down
//! to the game's steps) and a stacking Fulfilled. Every half hour a full update; every two
//! hours the promises and offers are tested again (true already: done; no longer possible:
//! gone). START nodes bring a Sim's trees up to where they stand (the skills they know, the
//! job they have, the friends they've made). The trees' nodes are the game's; what each type
//! does is by the game's function names (`check`, `feedback`, `start`): those not done here
//! yet drop their nodes and all below them, as the game drops what isn't installed.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use s3bake::dnp::{DnpData, DnpNode, DnpPrimitive, TIMER};
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::careers::Job;
use crate::dreamfns::{Ctx, Facts, Person, Town, age_name, check, feedback, has_trait, input_subject, ltr_name, of_category, start};
use crate::interact::{Household, Skills};
use crate::life::{Mood, MoodletKind, Moodlets, Trait};
use crate::sim::{HouseholdMember, Sim};
use crate::social::Relationships;

pub struct DreamsPlugin;

impl Plugin for DreamsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DreamEvent>().add_systems(
            Update,
            (life_events, relationship_events, wake_events, activity_events, journal_events, state_events, heartbeat, run_dreams, wish_test, promise_test, sync_wishes).chain().after(crate::save::apply_loaded_game).run_if(in_state(PlayMode::Live)),
        );
    }
}

/// The game's wish ids that matter to the engine: a "Null Dream" (offer nothing), "Satisfy on
/// Next Heart Beat", "Is Born".
const NULL_DREAM: u32 = 11_712_587;
/// Minutes between full updates, hours between retests, the offers shown at once, the promise
/// slots, the Fulfilled moodlet's stacking.
const FULL_UPDATE: f64 = 30.0;
const RETEST: f64 = 120.0;
const SHOWN: usize = 3;
const SLOTS: usize = 4;
const AMBITIOUS: f32 = 1.15;
/// A wish towards the lifetime wish: half again (`IsHelperDream`).
const HELPER: f32 = 1.5;
/// The lifetime wishes' tree's root (the "CAS Major Dream Feeder"'s).
const LIFETIME_ROOT: &str = "Lifetime dream aquired";
const FULFILLED_STEP: i32 = 5;
const FULFILLED_STACKS: i32 = 5;

/// What a wish is about: nothing yet (any will do), a Sim (by id), a skill, a career, an object
/// category, a meal or anything else by its key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Subject {
    #[default]
    None,
    Sim(u64),
    Skill(String),
    Career(String),
    Category(String),
    Key(String),
}

/// Something that happened to a Sim, as the game's `EventTracker` events (`kSkillLevelUp`...):
/// with whom, what skill and level, which social (given or received, taken well), how much, what
/// object (its catalogue key).
#[derive(Message, Clone, Debug, Default)]
pub struct DreamEvent {
    pub sim: Option<Entity>,
    pub kind: &'static str,
    pub other: Option<Entity>,
    pub skill: Option<String>,
    pub level: u32,
    pub social: Option<String>,
    pub recipient: bool,
    pub accepted: bool,
    pub amount: f32,
    pub objd: Option<s3bake::Key>,
    /// The object's class (from its catalogue entry, filled in on the way).
    pub class: Option<String>,
    /// What it was, by name: the activity, the fish, the meal, the plant, the book...
    pub key: Option<String>,
    /// How good it was (the game's `Quality`, 0 none to 11 perfect), where it matters.
    pub quality: i32,
    /// For everyone (a meal served to the group).
    pub group: bool,
    /// They won (a fight).
    pub won: bool,
}

impl DreamEvent {
    pub fn new(sim: Entity, kind: &'static str) -> Self {
        Self { sim: Some(sim), kind, accepted: true, ..default() }
    }
}

/// What testing a node says (the game's `CheckResult`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Active,
    Hidden,
    AutoSatisfy,
    ThrowAway,
    NotSupported,
}

/// An armed node: which, about what, since when, its count and list, whether it's promised (and
/// the promise it was spawned for), done (by which happening), done without offering (a START or
/// a root true from the first), asleep till when, a timer's beats and next.
#[derive(Clone, Debug)]
pub struct Active {
    uid: u64,
    tree: usize,
    node: usize,
    pub subject: Subject,
    since: f64,
    pub count: f32,
    pub list: Vec<String>,
    promised: bool,
    parent_promise: Option<u64>,
    done: Option<u64>,
    silent: bool,
    asleep_till: Option<f64>,
    timer: Option<(u32, f64)>,
    offered_at: Option<f64>,
}

impl Active {
    /// One about a subject, armed just now (to try a function on).
    pub fn about(subject: Subject) -> Self {
        Active { uid: 0, tree: 0, node: 0, subject, since: 0.0, count: 0.0, list: Vec::new(), promised: false, parent_promise: None, done: None, silent: false, asleep_till: None, timer: None, offered_at: None }
    }
}

/// A Sim's wishes: their armed nodes, those offered (oldest first), the four promised, and when
/// they were last brought up to date.
#[derive(Component, Default)]
pub struct Dreams {
    active: Vec<Active>,
    queue: Vec<u64>,
    promised: [Option<u64>; SLOTS],
    next_uid: u64,
    last_full: f64,
    last_retest: f64,
    ready: bool,
    bucket: u64,
    /// What the panel was last given, to tell what was promised or given up there.
    shown: Vec<u64>,
    /// The lifetime wish's type and its node, armed with the chains that lead to it.
    lifetime: Option<(u32, u64)>,
}

/// A wish offered or promised, for the panel: its node, subject, words, icons and points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DreamWish {
    pub tree: u64,
    pub node: u32,
    pub subject: Subject,
    pub uid: u64,
    pub text: String,
    pub description: String,
    pub icon: String,
    pub icon2: String,
    /// How far along it is ("2/5"), where the game says.
    #[serde(default)]
    pub progress: String,
    /// It helps towards the lifetime wish (half again the points; lit in its slot).
    #[serde(default)]
    pub helper: bool,
}

/// A catalogue object's class.
fn script_of(baked: Option<&crate::baked::Baked>, k: &s3bake::Key) -> Option<String> {
    let b = baked?;
    b.0.catalog_index.get(k).and_then(|i| b.0.catalog.get(*i)).map(|c| c.script.clone())
}

/// `Interpolate(v, max, min)`: nothing at the least, all at the most, in between between.
fn interpolate(v: f32, max: f32, min: f32) -> f32 {
    if max == min {
        if v >= max { 1.0 } else { 0.0 }
    } else {
        ((v - max) / (max - min) + 1.0).clamp(0.0, 1.0)
    }
}

/// A child's weight, as the game's `CalculateWeight` (-1: ruled out).
fn weight(n: &DnpNode, parent_subject: &Subject, cx: &Ctx) -> f32 {
    use s3bake::dnp::{AGE, CAREERS, MOOD, RELATIONSHIP, SKILLS, TRAITS};
    let any_weight = n.weight.iter().any(|w| *w > 0.0);
    let any_zero = n.zero.iter().any(|z| *z);
    let any_trait_rule = n.traits.iter().any(|t| t.required || t.excluded);
    if !any_weight && !any_zero && !any_trait_rule {
        return 1.0;
    }
    let mut sum = if any_weight { 0.0 } else { 1.0 };
    let on = |i: usize| n.weight[i] > 0.0 || n.zero[i];
    if on(SKILLS) {
        let mut skills: Vec<String> = n.skills.clone();
        if n.inherit_skill
            && let Subject::Skill(s) = parent_subject
        {
            skills.push(s.clone());
        }
        let s = skills
            .iter()
            .map(|k| {
                let l = cx.level(k) as f32;
                if l <= 0.0 { 0.0 } else if n.skill_range == [0.0, 0.0] { 1.0 } else { interpolate(l, n.skill_range[1], n.skill_range[0]) }
            })
            .fold(0.0, f32::max);
        if s <= 0.0 && n.zero[SKILLS] {
            return -1.0;
        }
        sum += n.weight[SKILLS] * s;
    }
    if on(AGE) {
        let a = if n.ages.iter().any(|x| x.eq_ignore_ascii_case(age_name(cx.sim.age))) { 1.0 } else { 0.0 };
        if a <= 0.0 && n.zero[AGE] {
            return -1.0;
        }
        sum += n.weight[AGE] * a;
    }
    if on(CAREERS) {
        let c = match cx.job {
            Some(j) if n.careers.iter().any(|c| c.eq_ignore_ascii_case(j.career().key)) || (n.inherit_career && matches!(parent_subject, Subject::Career(k) if k.eq_ignore_ascii_case(j.career().key))) => {
                if n.career_range == [0.0, 0.0] { 1.0 } else { interpolate((j.level + 1) as f32, n.career_range[1], n.career_range[0]) }
            }
            _ => 0.0,
        };
        if c <= 0.0 && n.zero[CAREERS] {
            return -1.0;
        }
        sum += n.weight[CAREERS] * c;
    }
    if on(TRAITS) || any_trait_rule {
        if n.traits.iter().any(|t| (t.required && !has_trait(&cx.sim.traits, &t.name)) || (t.excluded && has_trait(&cx.sim.traits, &t.name))) {
            return -1.0;
        }
        let matched = n.traits.iter().filter(|t| !t.required && !t.excluded && has_trait(&cx.sim.traits, &t.name)).count() as f32;
        let t = if n.min_traits <= 0.0 { 1.0 } else { (matched / n.min_traits).clamp(0.0, 1.0) };
        if t <= 0.0 && n.zero[TRAITS] {
            return -1.0;
        }
        sum += n.weight[TRAITS] * t;
    }
    if on(MOOD) {
        let m = interpolate(cx.mood, n.mood_range[1], n.mood_range[0]);
        if m <= 0.0 && n.zero[MOOD] {
            return -1.0;
        }
        sum += n.weight[MOOD] * m;
    }
    if on(RELATIONSHIP)
        && let Subject::Sim(id) = parent_subject
    {
        let r = match cx.rel_with(*id) {
            Some(r) if n.relationships.is_empty() || n.relationships.iter().any(|x| x.replace(' ', "").eq_ignore_ascii_case(&ltr_name(&r).replace(' ', ""))) => interpolate(r.friendship, n.rel_range[1], n.rel_range[0]),
            _ => 0.0,
        };
        if r <= 0.0 && n.zero[RELATIONSHIP] {
            return -1.0;
        }
        sum += n.weight[RELATIONSHIP] * r;
    }
    sum / 100.0
}

/// A wish's points, as the game's `AchievementPoints`: its score, times 1.15 for the Ambitious and
/// 1.5 for one that helps towards the lifetime wish, rounded down to a fortieth of the next power
/// of ten (5 to 500).
pub fn points(score: f32, ambitious: bool, helper: bool) -> u32 {
    let x = score * if ambitious { AMBITIOUS } else { 1.0 } * if helper { HELPER } else { 1.0 };
    if x <= 0.0 {
        return 0;
    }
    let step = (10f32.powf(1.0 + x.log10().floor()) * 0.025).clamp(5.0, 500.0);
    (x - x % step).round() as u32
}

/// The words for a node: its own keys or its type's, a Sim subject's by their sex, the rest by
/// the wisher's, `{0}` its number, `{1}` its subject.
fn words(p: &DnpPrimitive, n: &DnpNode, subject: &Subject, cx: &Ctx, ui: Option<&crate::layout::UiAssets>, gdata: &s3bake::GameDataBaked) -> (String, String) {
    let own = |i: usize| (!n.text_keys[i].is_empty()).then(|| ui.and_then(|u| u.localize(&n.text_keys[i]))).flatten();
    let subject_name: Option<(String, bool)> = match subject {
        Subject::None => None,
        Subject::Sim(id) => cx.people.get(id).map(|o| (o.first.clone(), o.female)),
        Subject::Skill(k) => gdata.skill(k).map(|s| (s.name.clone(), cx.sim.female)),
        Subject::Career(k) => crate::careers::careers().iter().find(|c| c.key.eq_ignore_ascii_case(k)).map(|c| (c.name.to_string(), cx.sim.female)),
        Subject::Category(k) => cx.data.categories.iter().find(|c| c.key == *k).map(|c| (if c.name.is_empty() { c.key.clone() } else { c.name.clone() }, cx.sim.female)),
        Subject::Key(k) => Some((key_name(k, gdata), cx.sim.female)),
    };
    let generic = own(0).unwrap_or_else(|| p.generic.clone());
    let text = match &subject_name {
        Some((name, female)) => {
            let t = if *female { own(2).unwrap_or_else(|| p.female.clone()) } else { own(1).unwrap_or_else(|| p.male.clone()) };
            let t = if t.is_empty() { generic.clone() } else { t };
            t.replace("{1.String}", name).replace("{1.SimFirstName}", name).replace("{1.SimName}", name).replace("{RecipeName}", name).replace("{FishName}", name).replace("{1.ObjectName}", name)
        }
        None => generic.clone(),
    };
    let n_text = if n.input_number.fract() == 0.0 { format!("{}", n.input_number as i64) } else { format!("{}", n.input_number) };
    let text = text.replace("{0.Number}", &n_text).replace("{1.Number}", &n_text).replace("{0.Money}", &format!("§{}", crate::lifetime::group(n.input_number as i64))).replace("{2.SimFirstName}", &cx.sim.first);
    let text = crate::layout::gendered(&text, cx.sim.female);
    let text = if text.trim().is_empty() { p.name.replace("$n", &n_text) } else { text };
    let desc = own(3).unwrap_or_else(|| p.description.clone());
    (text, crate::layout::gendered(&desc.replace("{0.Number}", &n_text), cx.sim.female))
}

/// A meal's, a fish's (or anything else's) name, by its key.
fn key_name(k: &str, gdata: &s3bake::GameDataBaked) -> String {
    gdata
        .recipes
        .iter()
        .find(|r| r.key.eq_ignore_ascii_case(k))
        .map(|r| r.name.clone())
        .or_else(|| gdata.collectibles.iter().find(|c| c.key.eq_ignore_ascii_case(k)).map(|c| c.name.clone()))
        .unwrap_or_else(|| k.to_string())
}

/// A node's icons: its type's, else its subject's (a skill's, a career's, a category's), else
/// the kind's default; and the corner one (a number's, where it's one).
fn icons(p: &DnpPrimitive, n: &DnpNode, subject: &Subject, cx: &Ctx, gdata: &s3bake::GameDataBaked) -> (String, String) {
    let primary = if !p.icon.is_empty() {
        p.icon.clone()
    } else {
        let by_subject = match subject {
            Subject::Skill(k) => gdata.skill(k).map(|s| s.wish_icon.clone()),
            Subject::Career(k) => crate::careers::careers().iter().find(|c| c.key.eq_ignore_ascii_case(k)).map(|c| c.icon.to_string()),
            Subject::Category(k) => cx.data.categories.iter().find(|c| c.key == *k).map(|c| c.icon.clone()),
            _ => None,
        };
        by_subject
            .filter(|s| !s.is_empty())
            .or_else(|| cx.data.subject_icons.iter().find(|(k, _)| *k == p.subject).map(|(_, i)| i.clone()))
            .unwrap_or_else(|| "dr_i_plan_a_party".to_string())
    };
    let secondary = if p.icon2 == "[Dynamic Number Icon]" {
        let k = (n.input_number as u32).min(11);
        cx.data.number_icons.iter().find(|(i, _)| *i == k).map(|(_, s)| s.clone()).unwrap_or_default()
    } else {
        p.icon2.clone()
    };
    (primary, secondary)
}

impl Dreams {
    fn uid(&mut self) -> u64 {
        self.next_uid += 1;
        self.next_uid
    }

    fn is_promised(&self, uid: u64) -> bool {
        self.promised.contains(&Some(uid))
    }

    /// A node armed (its subject from its parent's where it takes it, else its own).
    fn arm(&mut self, tree: usize, node: usize, subject: Subject, now: f64, parent_promise: Option<u64>) -> u64 {
        let uid = self.uid();
        self.active.push(Active {
            uid,
            tree,
            node,
            subject,
            since: now,
            count: 0.0,
            list: Vec::new(),
            promised: false,
            parent_promise,
            done: None,
            silent: false,
            asleep_till: None,
            timer: None,
            offered_at: None,
        });
        uid
    }
}

/// The game's happenings from the Sims' life events.
fn life_events(mut life: MessageReader<crate::life::LifeEvent>, mut out: MessageWriter<DreamEvent>) {
    use crate::life::LifeEventKind as K;
    for ev in life.read() {
        let s = ev.sim;
        let mut send = |e: DreamEvent| {
            out.write(e);
        };
        match &ev.kind {
            K::SkillUp { skill, level } => send(DreamEvent { skill: Some(skill.to_string()), level: *level, ..DreamEvent::new(s, "kSkillLevelUp") }),
            K::Promoted => {
                send(DreamEvent::new(s, "kEventCareerPromotion"));
                send(DreamEvent::new(s, "kCareerDataChanged"));
            }
            K::NewJob => {
                send(DreamEvent::new(s, "kEventCareerHired"));
                send(DreamEvent::new(s, "kCareerDataChanged"));
            }
            K::Demoted => send(DreamEvent::new(s, "kCareerDataChanged")),
            K::HomeFromWork { pay, .. } => {
                send(DreamEvent::new(s, "kFinishedWork"));
                send(DreamEvent { amount: *pay as f32, ..DreamEvent::new(s, "kGotPaid") });
            }
            K::Married => send(DreamEvent::new(s, "kMarriage")),
            K::Birthday => send(DreamEvent::new(s, "kSimAgeTransition")),
            // (The town's rabbit holes' own activities: those not also at home.)
            K::Finished { activity, .. } => {
                let (kinds, key, hours): (&[&'static str], &str, f32) = match *activity {
                    "Browse the Shelves" => (&["kReceivedRabbitHoleTreatment"], "Bookstore", 1.0),
                    "Get a Massage" | "Get a Makeover" => (&["kReceivedRabbitHoleTreatment"], "DaySpa", 1.5),
                    "See a Show" => (&["kReceivedRabbitHoleTreatment", "kSawShowType"], "Theatre", 2.0),
                    "Take a Yoga Class" => (&["kWorkedOut"], "", 1.5),
                    "Go Swimming" => (&["kSwimming"], "", 1.5),
                    "Go Fishing" => (&["kWentFishing"], "", 2.0),
                    "Eat a Meal" => (&["kAteAtRestaurant"], "Diner", 1.0),
                    _ => (&[], "", 0.0),
                };
                for kind in kinds {
                    let key = if *kind == "kSawShowType" { "Play" } else { key };
                    send(DreamEvent { key: (!key.is_empty()).then(|| key.to_string()), amount: hours, ..DreamEvent::new(s, kind) });
                }
            }
            _ => {}
        }
    }
}

/// A social done (`kSocialInteraction`): for the one who did it and the one it was done to, by
/// the game's name for it, taken well or not, and a fight's winner.
pub fn social(w: &mut World, actor: Entity, target: Entity, name: &str, accepted: bool, actor_won: Option<bool>) {
    let social = crate::conversation::game_key(name).to_string();
    for (me, other, recipient) in [(actor, target, false), (target, actor, true)] {
        w.write_message(DreamEvent {
            other: Some(other),
            social: Some(social.clone()),
            recipient,
            accepted,
            won: actor_won.is_some_and(|won| won != recipient),
            ..DreamEvent::new(me, "kSocialInteraction")
        });
    }
}

/// Where each household Sim stands with everyone, by the game's levels: met someone new
/// (`kMetSim`), a level changed (`kRelationshipLTRChanged`).
fn relationship_events(sims: Query<(Entity, &Relationships), With<HouseholdMember>>, mut seen: Local<HashMap<(Entity, Entity), &'static str>>, mut out: MessageWriter<DreamEvent>, time: Res<Time>, mut last: Local<f32>) {
    if time.elapsed_secs() - *last < 1.0 {
        return;
    }
    *last = time.elapsed_secs();
    for (me, rels) in &sims {
        for (other, r) in &rels.0 {
            let now = ltr_name(r);
            match seen.insert((me, *other), now) {
                None => {
                    out.write(DreamEvent { other: Some(*other), ..DreamEvent::new(me, "kMetSim") });
                }
                Some(was) if was != now => {
                    out.write(DreamEvent { other: Some(*other), key: Some(was.to_string()), ..DreamEvent::new(me, "kRelationshipLTRChanged") });
                }
                _ => {}
            }
        }
    }
}

/// Up from a sleep (`kSimWokeUp`, its hours asleep).
fn wake_events(
    sims: Query<(Entity, &crate::interact::ActionQueue), With<HouseholdMember>>,
    clock: Res<crate::clock::GameClock>,
    (mut asleep, mut seen): (Local<HashMap<Entity, f64>>, Local<std::collections::HashSet<Entity>>),
    mut out: MessageWriter<DreamEvent>,
) {
    for (e, q) in &sims {
        let sleeping = q.0.front().is_some_and(|a| matches!(a.label.as_str(), "Sleep" | "Nap") && matches!(a.phase, crate::interact::Phase::Running(_)));
        // (One asleep when the game's loaded: a night's sleep, as likely as not.)
        let first = seen.insert(e);
        match (sleeping, asleep.get(&e).copied()) {
            (true, None) => {
                asleep.insert(e, clock.minutes - if first { NIGHT } else { 0.0 });
            }
            (false, Some(from)) => {
                asleep.remove(&e);
                out.write(DreamEvent { amount: ((clock.minutes - from) / 60.0) as f32, ..DreamEvent::new(e, "kSimWokeUp") });
            }
            _ => {}
        }
    }
}

/// What each Sim's at, followed: an activity left (done or not) is the game's event for it, its
/// hours the amount (`activity_kinds`).
fn activity_events(sims: Query<(Entity, &crate::interact::ActionQueue), With<HouseholdMember>>, mut doing: Local<HashMap<Entity, (String, f32)>>, mut out: MessageWriter<DreamEvent>) {
    for (e, q) in &sims {
        let now = q.0.front().and_then(|a| match a.phase {
            crate::interact::Phase::Running(t) => Some((a.label.clone(), t)),
            _ => None,
        });
        let was = doing.get(&e).cloned();
        match (&was, &now) {
            (Some((l, t)), Some((l2, t2))) if l == l2 && t2 >= t => {}
            _ => {
                if let Some((label, minutes)) = was {
                    for kind in activity_kinds(&label, minutes) {
                        out.write(DreamEvent { amount: minutes / 60.0, key: Some(label.clone()), ..DreamEvent::new(e, kind) });
                    }
                }
            }
        }
        match now {
            Some(n) => {
                doing.insert(e, n);
            }
            None => {
                doing.remove(&e);
            }
        }
    }
}

/// Where the household stands, followed for the game's events: a moodlet come (`kGotBuff`, by the
/// game's buff), off to work (`kWentToWork`), to another lot (`kEventSimMovedToLot`), the funds
/// changed (`kFamilyFundsChanged`), a date over (`kFinishedDate`), another Sim seen about
/// (`kSawSim`, each one an hour at most).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn state_events(
    sims: Query<(Entity, &Moodlets, Option<&crate::visit::OnLot>, Has<crate::careers::AtWork>, &Transform, &Visibility, (Has<Job>, Has<crate::careers::Pension>)), With<HouseholdMember>>,
    mut life: MessageReader<crate::life::LifeEvent>,
    others: Query<(Entity, &Transform, &Visibility, Option<&crate::visit::OnLot>), With<Sim>>,
    household: Option<Res<Household>>,
    date: Option<Res<crate::dates::Date>>,
    clock: Res<crate::clock::GameClock>,
    mut seen: Local<StateSeen>,
    mut out: MessageWriter<DreamEvent>,
) {
    let now = clock.minutes;
    let first = !seen.ready;
    seen.ready = true;
    for ev in life.read() {
        if matches!(ev.kind, crate::life::LifeEventKind::Fired) {
            seen.fired.insert(ev.sim, now);
        }
    }
    for (e, moodlets, lot, at_work, tf, vis, (employed, pensioned)) in &sims {
        // (A job left: the game's `kLeftJob`, why: fired, retired, or quit.)
        if seen.employed.insert(e, employed).is_some_and(|was| was && !employed) {
            let why = if seen.fired.get(&e).is_some_and(|t| now - t < 60.0) {
                "Fired"
            } else if pensioned {
                "Retired"
            } else {
                "Quit"
            };
            out.write(DreamEvent { key: Some(why.into()), ..DreamEvent::new(e, "kLeftJob") });
        }
        let kinds: Vec<MoodletKind> = moodlets.0.iter().map(|m| m.kind).collect();
        let was = seen.moodlets.insert(e, kinds.clone());
        if let Some(was) = was {
            for k in kinds.iter().filter(|k| !was.contains(k)) {
                out.write(DreamEvent { key: Some(k.buff().0.to_string()), ..DreamEvent::new(e, "kGotBuff") });
            }
        }
        let lot_now = lot.map(|l| l.0);
        if seen.lots.insert(e, lot_now).is_some_and(|was| was != lot_now) {
            out.write(DreamEvent::new(e, "kEventSimMovedToLot"));
        }
        if !seen.working.insert(e, at_work).unwrap_or(at_work) && at_work {
            out.write(DreamEvent::new(e, "kWentToWork"));
        }
        // (Others about: within sight, on the same lot.)
        if *vis == Visibility::Hidden {
            continue;
        }
        for (o, otf, ovis, olot) in &others {
            if o == e || *ovis == Visibility::Hidden || olot.map(|l| l.0) != lot_now || otf.translation.distance(tf.translation) > SIGHT {
                continue;
            }
            if seen.saw.get(&(e, o)).is_some_and(|t| now - t < 60.0) {
                continue;
            }
            seen.saw.insert((e, o), now);
            out.write(DreamEvent { other: Some(o), ..DreamEvent::new(e, "kSawSim") });
        }
    }
    let funds = household.as_ref().map(|h| h.funds);
    if let (Some(f), Some(was)) = (funds, seen.funds)
        && f != was
    {
        for (e, ..) in &sims {
            out.write(DreamEvent { amount: (f - was) as f32, ..DreamEvent::new(e, "kFamilyFundsChanged") });
        }
    }
    seen.funds = funds;
    let on_date = date.as_ref().map(|d| (d.a, d.b));
    if let Some((a, b)) = seen.date
        && on_date != Some((a, b))
        && !first
    {
        out.write(DreamEvent { other: Some(b), ..DreamEvent::new(a, "kFinishedDate") });
        out.write(DreamEvent { other: Some(a), ..DreamEvent::new(b, "kFinishedDate") });
    }
    seen.date = on_date;
}

/// How near another Sim is seen (metres).
const SIGHT: f32 = 10.0;

/// What `state_events` last saw.
#[derive(Default)]
struct StateSeen {
    ready: bool,
    moodlets: HashMap<Entity, Vec<MoodletKind>>,
    lots: HashMap<Entity, Option<usize>>,
    working: HashMap<Entity, bool>,
    saw: HashMap<(Entity, Entity), f64>,
    employed: HashMap<Entity, bool>,
    fired: HashMap<Entity, f64>,
    funds: Option<i64>,
    date: Option<(Entity, Entity)>,
}

/// What's counted towards the skill journals, as the game's events: a fish caught (its kind and
/// weight), a dish made, something harvested, repaired, upgraded, a star found, tutoring.
fn journal_events(mut did: MessageReader<crate::journal::Did>, mut out: MessageWriter<DreamEvent>) {
    use crate::journal::{Deed, Kinds, Stat};
    for d in did.read() {
        let s = d.sim;
        match &d.what {
            Deed::Caught(fish, weight) => {
                out.write(DreamEvent { key: Some(fish.clone()), amount: *weight as f32, ..DreamEvent::new(s, "kCaughtFish") });
            }
            Deed::Kind(Kinds::Upgrades, what) => {
                out.write(DreamEvent { key: Some(what.clone()), ..DreamEvent::new(s, "kUpgradedObject") });
            }
            Deed::Painted(worth, grade) => {
                out.write(DreamEvent { amount: *worth as f32, quality: i32::from(*grade), ..DreamEvent::new(s, "kFinishedPainting") });
            }
            Deed::Count(Stat::Harvested, n) => {
                out.write(DreamEvent { amount: *n as f32, ..DreamEvent::new(s, "kHarvestedHarvestable") });
            }
            Deed::Count(Stat::ElectricalRepairs | Stat::PlumbingRepairs, _) => {
                out.write(DreamEvent::new(s, "kRepairedObject"));
            }
            Deed::Count(Stat::StarsFound, _) => {
                out.write(DreamEvent::new(s, "kDiscoveredStar"));
            }
            Deed::Count(Stat::TutoringHours, h) => {
                out.write(DreamEvent { amount: *h as f32, ..DreamEvent::new(s, "kTutoredSim") });
            }
            _ => {}
        }
    }
}

/// The game's events for an activity, by its name and the minutes it went on.
fn activity_kinds(label: &str, minutes: f32) -> Vec<&'static str> {
    // (A moment's start and stop isn't doing it.)
    if minutes < 10.0 {
        return Vec::new();
    }
    match label {
        "Watch TV" | "Watch the TV" | "Watch" => vec!["kWatchedTv"],
        "Play Chess" => vec!["kPlayedChess"],
        "Play Guitar" => vec!["kPlayedGuitar"],
        "Work Out" | "Pump Iron" | "Strength Training" => vec!["kWorkedOut", "kStrengthTimeAdded"],
        "Cardio Exercise" => vec!["kWorkedOut", "kCardioTimeAdded"],
        "Go Jogging" => vec!["kWorkedOut", "kCardioTimeAdded", "kJogged"],
        "Take Bath" | "Take Bubble Bath" => vec!["kEventTakeBath"],
        "Clean" | "Clean Up" | "Mop Up" | "Scrub" | "Wash Dishes" => vec!["kCleanedObject"],
        "Fish" => vec!["kWentFishing"],
        "Water" => vec!["kWateredPlant", "kGardened"],
        "Weed" | "Talk to Plant" | "Harvest" => vec!["kGardened"],
        "Fertilize" => vec!["kFertilizedPlant", "kGardened"],
        "Relax in Hot Tub" => vec!["kGoHotTubbing"],
        "Swim" => vec!["kSwimming"],
        "Hack" => vec!["kHacked"],
        "Practice Speech" => vec!["kPracticedSpeech"],
        "Check Self Out" | "Admire Self" => vec!["kCheckedSelfOutInMirror"],
        "Stargaze" | "Search Galaxy" | "View" => vec!["kUsedTelescope"],
        "Paint" => vec!["kPainted"],
        "Write Novel" => vec!["kWorkedOnNovel"],
        "Play Video Games" | "Play Computer Games" | "Play Arcade Game" => vec!["kGamePlayed"],
        "Read" | "Read Book" | "Read a Book" => vec!["kReading"],
        _ => Vec::new(),
    }
}

/// WISH=<type name>[=<subject>] (tests): the selected Sim promised a wish of that type (its first
/// visible node in the trees), about that (a skill, a career, a category, a Sim by first name, a
/// meal's or fish's key).
fn wish_test(mut sims: Query<&mut Dreams, With<crate::sim::Selected>>, people: Query<&Sim>, gu: Option<Res<crate::icons::GameUi>>, clock: Res<crate::clock::GameClock>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Ok(spec) = std::env::var("WISH") else {
        *done = true;
        return;
    };
    let (Some(gu), Ok(mut d)) = (gu, sims.single_mut()) else { return };
    if !d.ready {
        return;
    }
    *done = true;
    let (name, about) = spec.split_once('=').map_or((spec.as_str(), None), |(a, b)| (a, Some(b)));
    let data = &gu.data.dnp;
    let found = data.trees.iter().enumerate().find_map(|(ti, t)| t.nodes.iter().position(|n| n.visible && data.primitive(n.prim).is_some_and(|p| p.name.eq_ignore_ascii_case(name))).map(|ni| (ti, ni)));
    let Some((ti, ni)) = found else {
        warn!("dreams: no wish of type {name:?}");
        return;
    };
    let Some(p) = data.primitive(data.trees[ti].nodes[ni].prim) else { return };
    let subject = match about {
        None => Subject::None,
        Some(s) => match p.subject.as_str() {
            "Skill" => Subject::Skill(s.into()),
            "Career" => Subject::Career(s.into()),
            "ObjectCategory" => Subject::Category(s.into()),
            "Sim" | "SimOrPet" => people.iter().find(|x| x.first.eq_ignore_ascii_case(s)).map(|x| Subject::Sim(x.id)).unwrap_or_default(),
            _ => Subject::Key(s.into()),
        },
    };
    let uid = d.arm(ti, ni, subject, clock.minutes, None);
    if let Some(slot) = d.promised.iter().position(|s| s.is_none()) {
        d.promised[slot] = Some(uid);
        if let Some(a) = d.active.iter_mut().find(|a| a.uid == uid) {
            a.promised = true;
        }
    }
    info!("dreams: test wish {:?} promised (in {})", p.name, data.trees[ti].name);
}

/// PROMISE_WISHES=1: every wish offered promised, while there's a slot (to see them through).
fn promise_test(mut sims: Query<&mut crate::wishes::Wishes, With<Dreams>>, on: Local<Option<bool>>) {
    let mut on = on;
    if !*on.get_or_insert_with(|| std::env::var("PROMISE_WISHES").is_ok()) {
        return;
    }
    for mut w in &mut sims {
        if !w.offered.is_empty() && w.promised.len() < SLOTS {
            info!("dreams: test promising {}", w.offered[0].text());
            w.promise(0);
        }
    }
}

/// What a Sim asleep at load is taken to have slept already, in minutes.
const NIGHT: f64 = 6.0 * 60.0;

/// Every half hour, each Sim's heartbeat (`kSimHeartbeat`, `kDnPSynchronizeEvent`).
fn heartbeat(sims: Query<Entity, With<Dreams>>, clock: Res<crate::clock::GameClock>, mut last: Local<f64>, mut out: MessageWriter<DreamEvent>) {
    if clock.minutes - *last < FULL_UPDATE {
        return;
    }
    *last = clock.minutes;
    for e in &sims {
        out.write(DreamEvent::new(e, "kSimHeartbeat"));
        out.write(DreamEvent::new(e, "kDnPSynchronizeEvent"));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run_dreams(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, Option<&mut Dreams>, &Skills, Option<&Job>, &Relationships, &Mood, Option<&mut crate::wishes::Wishes>, &mut Moodlets, Option<&crate::lifetime::LifetimeWish>), With<HouseholdMember>>,
    people: PeopleQuery,
    extra: ExtraQuery,
    mut events: MessageReader<DreamEvent>,
    (gu, ui, household, clock): (Option<Res<crate::icons::GameUi>>, Option<Res<crate::layout::UiAssets>>, Option<Res<Household>>, Res<crate::clock::GameClock>),
    objects: Query<&crate::interact::GameObject, Without<crate::visit::LotObject>>,
    baked: Option<Res<crate::baked::Baked>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    (world, family, party, date): (Res<crate::loading::CurrentWorld>, Option<Res<crate::family::Genealogy>>, Option<Res<crate::parties::Party>>, Option<Res<crate::dates::Date>>),
    (plants, mut town): (Query<&crate::gardening::GrowingPlant>, Local<Option<(String, Town)>>),
) {
    let Some(gu) = gu else { return };
    let gdata = gu.data.clone();
    let data = &gdata.dnp;
    if data.trees.is_empty() {
        return;
    }
    // (A bought object's class, from the catalogue.)
    let evs: Vec<DreamEvent> = events
        .read()
        .cloned()
        .map(|mut e| {
            e.class = e.objd.and_then(|k| script_of(baked.as_deref(), &k));
            e
        })
        .collect();
    let now = clock.minutes;
    let (roster, ids) = roster(&people);
    let funds = household.as_ref().map_or(0, |h| h.funds);
    let worth = funds + objects.iter().map(|o| o.price as i64).sum::<i64>();
    // (Each category the household has, and the dearest of it.)
    let classes: Vec<(String, i64)> = objects.iter().filter_map(|o| script_of(baked.as_deref(), &o.objd).map(|s| (s, o.price as i64))).collect();
    let owned: Vec<(String, i64)> = data
        .categories
        .iter()
        .filter_map(|c| classes.iter().filter(|(s, _)| of_category(s, c)).map(|(_, p)| *p).max().map(|p| (c.key.clone(), p)))
        .collect();
    // (The town's venues, once a world.)
    if town.as_ref().is_none_or(|(name, _)| *name != world.name) {
        *town = Some((world.name.clone(), Town::of(&world.data)));
    }
    let Some((_, town)) = town.as_ref() else { return };
    let grown_plants = plants.iter().filter(|p| !p.dead).count() as u32;
    let prim = |id: u32| data.primitive(id);
    let mut rng = rand::rng();
    for (me, sim, dreams, skills, job, rels, mood, wishes, mut moodlets, lifetime) in &mut sims {
        // (Children and older; a Sim new to it, theirs to be armed.)
        if sim.age.is_little() {
            continue;
        }
        let Some(mut d) = dreams else {
            commands.entity(me).insert(Dreams::default());
            continue;
        };
        let d = &mut *d;
        let facts = facts_of(me, &extra, &gdata, &ids, party.as_deref(), date.as_deref(), grown_plants);
        let cx = Ctx { sim, skills, job, rels, mood: mood.0, funds, worth, people: &roster, ids: &ids, owned: &owned, data, facts: &facts, town, family: family.as_deref(), gdata: Some(&gdata) };
        // Arming every tree, as the Sim stands (the game's `SetupForNewSim`, `StartupTree`).
        if !d.ready {
            d.ready = true;
            d.last_full = now;
            d.last_retest = now;
            for (ti, t) in data.trees.iter().enumerate() {
                let Some(root) = t.nodes.get(t.root) else { continue };
                let Some(p) = prim(root.prim) else { continue };
                // (The lifetime wishes' trees aren't armed: they're the catalogue of them.)
                if p.name == "Lifetime dream aquired" {
                    continue;
                }
                let mut subject = input_subject(p, root);
                match check(&p.check, p, root, &mut subject, &cx, Some(&gdata)) {
                    Check::NotSupported | Check::ThrowAway => continue,
                    c => {
                        let uid = d.arm(ti, t.root, subject, now, None);
                        if c == Check::AutoSatisfy
                            && let Some(a) = d.active.iter_mut().find(|a| a.uid == uid)
                        {
                            a.done = Some(0);
                            a.silent = true;
                        }
                    }
                }
                // (START nodes, done already for what's so of the Sim.)
                for (ni, n) in t.nodes.iter().enumerate().filter(|(_, n)| n.start) {
                    let Some(p) = prim(n.prim) else { continue };
                    for s in start(&p.start, n, &cx) {
                        let uid = d.arm(ti, ni, s, now, None);
                        if let Some(a) = d.active.iter_mut().find(|a| a.uid == uid) {
                            a.done = Some(0);
                            a.silent = true;
                        }
                    }
                }
            }
            // (Promises kept in a save: back in their slots.)
            if let Some(w) = wishes.as_ref() {
                let kept: Vec<DreamWish> = w.promised.iter().filter_map(|x| x.dream().cloned()).collect();
                for k in kept {
                    let Some(ti) = data.trees.iter().position(|t| t.id == k.tree) else { continue };
                    let Some(ni) = data.trees[ti].nodes.iter().position(|n| n.id == k.node) else { continue };
                    let uid = d.arm(ti, ni, k.subject.clone(), now, None);
                    if let Some(slot) = d.promised.iter().position(|s| s.is_none()) {
                        d.promised[slot] = Some(uid);
                        if let Some(a) = d.active.iter_mut().find(|a| a.uid == uid) {
                            a.promised = true;
                        }
                    }
                }
            }
            info!("dreams: {} armed for {} ({} trees)", d.active.len(), sim.first, data.trees.len());
        }
        // The lifetime wish: its node armed, and the chains towards it begun (its promise links,
        // as when the game's `TryAddLifetimeWish` promises it); a new one in place of the old.
        let ltw = lifetime.filter(|l| !l.fulfilled).and_then(|l| data.primitives.iter().find(|p| p.check == l.def().check && p.name.starts_with('*')).map(|p| p.id));
        if d.lifetime.map(|x| x.0) != ltw {
            if let Some((_, old)) = d.lifetime.take() {
                d.active.retain(|a| a.uid != old && a.parent_promise != Some(old));
                d.queue.retain(|u| d.active.iter().any(|a| a.uid == *u));
            }
            let at = ltw.and_then(|want| {
                data.trees.iter().enumerate().find_map(|(ti, t)| {
                    let root = t.nodes.get(t.root)?;
                    if prim_name(data, root.prim) != Some(LIFETIME_ROOT) {
                        return None;
                    }
                    root.children.iter().copied().find(|c| t.nodes[*c].prim == want).map(|ni| (ti, ni))
                })
            });
            if let (Some(want), Some((ti, ni))) = (ltw, at) {
                let n = &data.trees[ti].nodes[ni];
                let subject = prim(n.prim).map(|p| input_subject(p, n)).unwrap_or_default();
                let uid = d.arm(ti, ni, subject, now, None);
                d.lifetime = Some((want, uid));
                if let Some(a) = d.active.iter().find(|a| a.uid == uid).cloned() {
                    let mut parent = a.clone();
                    d.bucket += 1;
                    parent.done = Some(d.bucket);
                    let mut buckets = HashMap::new();
                    for ci in n.children.iter().copied().filter(|c| data.trees[ti].nodes[*c].promise_link) {
                        score_child(d, &cx, data, &parent, ci, true, Some(uid), now, &mut buckets);
                    }
                    for (_, cands) in buckets {
                        if let Some((u, _)) = cands.into_iter().max_by(|x, y| x.1.total_cmp(&y.1)) {
                            d.queue.push(u);
                        }
                    }
                }
                info!("dreams: {}'s lifetime wish armed: {}", sim.first, prim_name(data, want).unwrap_or(""));
            }
        }
        // Happenings: the nodes listening for them, fulfilled.
        for ev in evs.iter().filter(|e| e.sim == Some(me)) {
            debug!("dreams: {} {} {:?} {:?} {}", sim.first, ev.kind, ev.skill, ev.social, ev.amount);
            d.bucket += 1;
            let bucket = d.bucket;
            for i in 0..d.active.len() {
                let a = &d.active[i];
                if a.done.is_some() || a.asleep_till.is_some() || a.timer.is_some() {
                    continue;
                }
                let n = &data.trees[a.tree].nodes[a.node];
                let Some(p) = prim(n.prim) else { continue };
                let listens = if p.event.is_empty() && p.social { "kSocialInteraction" } else { p.event.as_str() };
                if listens != ev.kind {
                    continue;
                }
                let mut a = d.active[i].clone();
                if let Some(captured) = feedback(feedback_of(p), p, n, &mut a, ev, &cx, Some(&gdata)) {
                    // (What it turned out to be about, where it wasn't about anything yet.)
                    if a.subject == Subject::None {
                        a.subject = captured;
                    }
                    a.done = Some(bucket);
                }
                d.active[i] = a;
            }
        }
        // Timers' beats.
        for a in d.active.iter_mut() {
            let n = &data.trees[a.tree].nodes[a.node];
            if n.prim != TIMER || a.done.is_some() {
                continue;
            }
            let (fired, next) = *a.timer.get_or_insert((0, if n.satisfy_on_start { now } else { a.since + n.between_satisfies as f64 * 60.0 }));
            if now >= next {
                a.timer = Some((fired + 1, now + n.between_satisfies.max(0.1) as f64 * 60.0));
                d.bucket += 1;
                a.done = Some(d.bucket);
            }
        }
        // The promised done: points, Fulfilled.
        let done_promises: Vec<u64> = d.active.iter().filter(|a| a.done.is_some() && a.promised).map(|a| a.uid).collect();
        for uid in &done_promises {
            let Some(a) = d.active.iter().find(|a| a.uid == *uid) else { continue };
            let n = &data.trees[a.tree].nodes[a.node];
            let pts = points(n.score, sim.traits.contains(&Trait::Ambitious), helps(data, d.lifetime, n, &a.subject));
            let p = prim(n.prim);
            let text = p.map(|p| words(p, n, &a.subject, &cx, ui.as_deref(), &gdata).0).unwrap_or_default();
            info!("dreams: {} fulfilled {text} (+{pts})", sim.first);
            notes.push(format!("{} fulfilled a wish: {text} (+{pts} lifetime happiness)", sim.first));
            play.write(crate::sound::PlaySound::ui("sting_dream_fullfill"));
            commands.entity(me).queue(move |mut w: EntityWorldMut| {
                if let Some(mut ws) = w.get_mut::<crate::wishes::Wishes>() {
                    ws.points = ws.points.saturating_add(pts);
                }
            });
            // (Fulfilled, a little more each time while it lasts.)
            if moodlets.has(MoodletKind::WishFulfilled) {
                if let Some(m) = moodlets.0.iter_mut().find(|m| m.kind == MoodletKind::WishFulfilled)
                    && m.value < MoodletKind::WishFulfilled.def().value + FULFILLED_STEP * FULFILLED_STACKS
                {
                    m.value += FULFILLED_STEP;
                    m.until = now + MoodletKind::WishFulfilled.def().hours as f64 * 60.0;
                }
            } else {
                moodlets.add(MoodletKind::WishFulfilled, now);
            }
            for s in d.promised.iter_mut() {
                if *s == Some(*uid) {
                    *s = None;
                }
            }
        }
        // Children of what's done: weighed, armed, one offered per happening.
        let done: Vec<usize> = (0..d.active.len()).filter(|i| d.active[*i].done.is_some()).collect();
        for i in &done {
            let a = &d.active[*i];
            debug!("dreams: {} done {:?} ({:?}){}{} in {} #{}", sim.first, prim(data.trees[a.tree].nodes[a.node].prim).map(|p| p.name.as_str()).unwrap_or("timer"), a.subject, if a.silent { " silently" } else { "" }, if a.promised { " PROMISED" } else { "" }, data.trees[a.tree].name, a.uid);
        }
        let mut buckets: HashMap<u64, Vec<(u64, f32)>> = HashMap::new();
        for i in &done {
            let parent = d.active[*i].clone();
            let pn = &data.trees[parent.tree].nodes[parent.node];
            let roll = !parent.silent && rng.random_range(0.0..100.0) < pn.next_chance;
            // (A chain begun for a promise stops when the promise is gone.)
            if parent.parent_promise.is_some_and(|pp| !d.is_promised(pp) && !d.active.iter().any(|a| a.uid == pp)) {
                continue;
            }
            let promise_parent = if parent.promised { Some(parent.uid) } else { parent.parent_promise };
            for &ci in &pn.children {
                let cn = &data.trees[parent.tree].nodes[ci];
                if cn.promise_link {
                    continue;
                }
                score_child(d, &cx, data, &parent, ci, roll, promise_parent, now, &mut buckets);
            }
        }
        // Offered: one per happening, by weighted chance.
        for (_, mut cands) in buckets {
            while !cands.is_empty() {
                let total: f32 = cands.iter().map(|c| c.1).sum();
                let mut pick = rng.random_range(0.0..total.max(1e-6));
                let mut at = 0;
                for (k, c) in cands.iter().enumerate() {
                    if pick < c.1 {
                        at = k;
                        break;
                    }
                    pick -= c.1;
                    at = k;
                }
                let (uid, _) = cands.remove(at);
                let Some(a) = d.active.iter().find(|a| a.uid == uid).cloned() else { continue };
                let n = &data.trees[a.tree].nodes[a.node];
                if n.prim == NULL_DREAM {
                    break;
                }
                if already_shown(&d, data, &a) {
                    continue;
                }
                d.queue.push(uid);
                if let Some(x) = d.active.iter_mut().find(|x| x.uid == uid) {
                    x.offered_at = Some(now);
                }
                info!("dreams: {} offered {:?} ({:?}) from {} #{uid}", sim.first, prim(n.prim).map(|p| p.name.as_str()).unwrap_or(""), a.subject, data.trees[a.tree].name);
                break;
            }
        }
        // What's done: re-armed if it repeats (asleep a while, maybe), a timer kept, else gone.
        let mut keep: Vec<Active> = Vec::new();
        for mut a in std::mem::take(&mut d.active) {
            if a.done.is_none() {
                keep.push(a);
                continue;
            }
            let n = &data.trees[a.tree].nodes[a.node];
            let root = a.node == data.trees[a.tree].root;
            // (Roots repeat, but those true once for all: born, a CAS Sim.)
            let special_root = prim(n.prim).is_some_and(|p| matches!(p.name.as_str(), "Is Born" | "CAS Born"));
            if n.prim == TIMER && a.timer.is_some_and(|(fired, _)| fired < n.satisfies.max(1)) {
                a.done = None;
                keep.push(a);
            } else if (n.repeatable || root) && !special_root && (root || !a.silent) {
                a.done = None;
                a.silent = false;
                a.count = 0.0;
                if !n.inherit_subject {
                    a.subject = prim(n.prim).map(|p| input_subject(p, n)).unwrap_or_default();
                }
                if n.between_repeats > 0.0 {
                    a.asleep_till = Some(now + n.between_repeats as f64 * 60.0);
                }
                keep.push(a);
            }
        }
        d.active = keep;
        let alive: Vec<u64> = d.active.iter().map(|a| a.uid).collect();
        d.queue.retain(|u| alive.contains(u));
        for s in d.promised.iter_mut() {
            if s.is_some_and(|u| !alive.contains(&u)) {
                *s = None;
            }
        }
        // Asleep ones woken; offers past their hours withdrawn; unpromised ones past their time gone.
        for a in d.active.iter_mut() {
            if a.asleep_till.is_some_and(|t| now >= t) {
                a.asleep_till = None;
                a.since = now;
            }
        }
        let expired: Vec<u64> = d
            .queue
            .iter()
            .copied()
            .filter(|u| d.active.iter().find(|a| a.uid == *u).is_some_and(|a| a.offered_at.is_some_and(|t| now - t > data.trees[a.tree].nodes[a.node].display_hours as f64 * 60.0)))
            .collect();
        d.queue.retain(|u| !expired.contains(u));
        let promised: Vec<u64> = d.promised.iter().flatten().copied().collect();
        let queued = d.queue.clone();
        d.active.retain(|a| {
            let n = &data.trees[a.tree].nodes[a.node];
            let root = a.node == data.trees[a.tree].root;
            let timed_out = n.satisfaction_hours > 0.0 && !promised.contains(&a.uid) && !root && now - a.since > n.satisfaction_hours as f64 * 60.0;
            // (A leaf offered and left: gone with its offer.)
            let lapsed = n.children.is_empty() && a.offered_at.is_some() && !queued.contains(&a.uid) && !promised.contains(&a.uid);
            !timed_out && !lapsed
        });
        // Every two hours: the promises and offers tested again.
        if now - d.last_retest >= RETEST {
            d.last_retest = now;
            let tested: Vec<u64> = d.promised.iter().flatten().copied().chain(d.queue.iter().copied()).collect();
            for uid in tested {
                let Some(a) = d.active.iter().find(|a| a.uid == uid).cloned() else { continue };
                let n = &data.trees[a.tree].nodes[a.node];
                let Some(p) = prim(n.prim) else { continue };
                let mut subject = a.subject.clone();
                match check(&p.check, p, n, &mut subject, &cx, Some(&gdata)) {
                    Check::AutoSatisfy => {
                        if let Some(x) = d.active.iter_mut().find(|x| x.uid == uid) {
                            d.bucket += 1;
                            x.done = Some(d.bucket);
                        }
                    }
                    Check::Hidden => d.queue.retain(|u| *u != uid),
                    Check::ThrowAway | Check::NotSupported => {
                        d.queue.retain(|u| *u != uid);
                        d.active.retain(|x| x.uid != uid);
                        for s in d.promised.iter_mut() {
                            if *s == Some(uid) {
                                *s = None;
                            }
                        }
                    }
                    Check::Active => {}
                }
            }
        }
        d.last_full = now;
    }
}

/// A child of a node done: tested, weighed, armed (if it can lead anywhere), and a candidate to
/// be offered (the game's `ScoreChildren`).
#[allow(clippy::too_many_arguments)]
fn score_child(d: &mut Dreams, cx: &Ctx, data: &DnpData, parent: &Active, ci: usize, roll: bool, promise_parent: Option<u64>, now: f64, buckets: &mut HashMap<u64, Vec<(u64, f32)>>) {
    let cn = &data.trees[parent.tree].nodes[ci];
    let cp = if cn.prim == TIMER { None } else { data.primitive(cn.prim) };
    if cn.prim != TIMER && cp.is_none() {
        return;
    }
    let mut subject = match cp {
        Some(p) if cn.inherit_subject || p.subject == "None" || p.subject.is_empty() => {
            if cn.input_subject.is_empty() { parent.subject.clone() } else { input_subject(p, cn) }
        }
        Some(p) => {
            let own = input_subject(p, cn);
            if own == Subject::None { Subject::None } else { own }
        }
        None => parent.subject.clone(),
    };
    // (Not twice the same.)
    if d.active.iter().any(|a| a.tree == parent.tree && a.node == ci && a.subject == subject && a.done.is_none()) {
        return;
    }
    let result = match cp {
        None => Check::Active,
        // (A wish that could never come true, its feedback not done here: as unsupported.)
        Some(p) if cn.visible && cn.prim != NULL_DREAM && !crate::dreamfns::fulfillable(feedback_of(p), p, cn, cx, cx.gdata) => Check::NotSupported,
        Some(p) => check(&p.check, p, cn, &mut subject, cx, cx.gdata),
    };
    match result {
        Check::Active => {
            // (A timer's never offered.)
            let w = if cn.visible && cn.prim != TIMER && !parent.silent && roll {
                let base = weight(cn, &parent.subject, cx);
                if base < 0.0 {
                    -1.0
                } else {
                    let span = if cn.repeatable { data.trees[parent.tree].nodes[parent.node].repeat_timeout } else { data.trees[parent.tree].nodes[parent.node].satisfaction_hours };
                    let elapsed = ((now - parent.since) / 60.0) as f32;
                    let life = if span == 0.0 { 1.0 } else { ((span - elapsed) / span).max(0.0) };
                    base * (cn.max_ad - cn.min_ad) * life + cn.min_ad
                }
            } else {
                0.0
            };
            let null = cn.prim == NULL_DREAM;
            if !null && (w > 0.0 || !cn.children.is_empty()) || null && w > 0.0 {
                let uid = d.arm(parent.tree, ci, subject, now, promise_parent);
                if w > 0.0 {
                    buckets.entry(parent.done.unwrap_or(0)).or_default().push((uid, w));
                }
            }
        }
        Check::Hidden => {
            if !cn.children.is_empty() {
                d.arm(parent.tree, ci, subject, now, promise_parent);
            }
        }
        Check::AutoSatisfy => {
            let uid = d.arm(parent.tree, ci, subject, now, promise_parent);
            if let Some(a) = d.active.iter_mut().find(|a| a.uid == uid) {
                a.done = parent.done;
            }
        }
        Check::ThrowAway | Check::NotSupported => {}
    }
}

/// A primitive's name.
fn prim_name(data: &DnpData, id: u32) -> Option<&str> {
    data.primitive(id).map(|p| p.name.as_str())
}

/// Whether a wish helps towards the lifetime wish (a `LinkedWishes` row's type, about what it
/// names, of its number: the game's `UpdateLink`).
fn helps(data: &DnpData, lifetime: Option<(u32, u64)>, n: &DnpNode, subject: &Subject) -> bool {
    let Some((ltw, _)) = lifetime else { return false };
    let about = match subject {
        Subject::Skill(s) | Subject::Career(s) | Subject::Category(s) | Subject::Key(s) => s.clone(),
        _ => String::new(),
    };
    data.links.iter().any(|l| l.lifetime == ltw && l.prim == n.prim && (l.subject.is_empty() || l.subject.eq_ignore_ascii_case(&about)) && l.number.is_none_or(|x| (x - n.input_number).abs() < 0.01))
}

/// A type's feedback function (a social's, by default, the game's `SocialFeedBackFunction`).
fn feedback_of(p: &DnpPrimitive) -> &str {
    if p.feedback.is_empty() && p.social { "SocialFeedBackFunction" } else { p.feedback.as_str() }
}

/// Whether an equivalent wish is promised or on offer already (the game's `NodeIsAlreadyDisplayed`).
fn already_shown(d: &Dreams, data: &DnpData, a: &Active) -> bool {
    let n = &data.trees[a.tree].nodes[a.node];
    let group = |p: u32| data.equivalent.iter().position(|g| g.contains(&p));
    d.promised.iter().flatten().chain(d.queue.iter()).filter(|u| **u != a.uid).filter_map(|u| d.active.iter().find(|x| x.uid == *u)).any(|x| {
        let xn = &data.trees[x.tree].nodes[x.node];
        let same = xn.prim == n.prim || group(xn.prim).is_some_and(|g| Some(g) == group(n.prim));
        same && xn.input_number == n.input_number && (x.subject == a.subject || x.subject == Subject::None || a.subject == Subject::None)
    })
}

/// Everyone, as the functions need them: by id, and their ids.
type PeopleQuery<'w, 's> = Query<'w, 's, (Entity, &'static Sim, Has<HouseholdMember>, Option<&'static crate::aging::Aging>, Has<crate::little::Pregnancy>, Option<&'static Mood>, Option<&'static Job>)>;

/// What else of each household Sim the functions look at.
type ExtraQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static crate::sim::Motives,
        Option<&'static crate::visit::OnLot>,
        Has<crate::careers::AtWork>,
        Option<&'static crate::journal::SkillJournal>,
        Option<&'static crate::writing::Author>,
        Option<&'static crate::novels::Novels>,
        Option<&'static crate::skillbooks::SkillBooks>,
        Option<&'static crate::meals::KnownRecipes>,
        Option<&'static crate::classes::ClassesTaken>,
        Option<&'static crate::chess::ChessRecord>,
        Option<&'static crate::rabbitholes::SchoolGrades>,
        Has<crate::rabbitholes::Homework>,
        Has<crate::little::Pregnancy>,
    ),
    With<HouseholdMember>,
>;

fn roster(people: &PeopleQuery) -> (HashMap<u64, Person>, HashMap<Entity, u64>) {
    let roster = people
        .iter()
        .map(|(e, s, m, aging, pregnant, mood, job)| {
            let days_left = aging.map_or(f32::MAX, |a| crate::aging::stage_days(s.age) - a.days);
            let p = Person {
                entity: e,
                first: s.first.clone(),
                female: s.female,
                age: s.age,
                member: m,
                days_left,
                pregnant,
                mood: mood.map_or(0.0, |m| m.0),
                traits: s.traits.clone(),
                career: job.map(|j| (j.career().key, j.level + 1)),
                favorite_food: s.favorites.food.clone(),
            };
            (s.id, p)
        })
        .collect();
    let ids = people.iter().map(|(e, s, ..)| (e, s.id)).collect();
    (roster, ids)
}

/// A household Sim's needs, whereabouts, doings and records, for the functions.
fn facts_of<'a>(me: Entity, extra: &'a ExtraQuery, gdata: &s3bake::GameDataBaked, ids: &HashMap<Entity, u64>, party: Option<&crate::parties::Party>, date: Option<&crate::dates::Date>, plants: u32) -> Facts<'a> {
    let Ok((motives, lot, at_work, journal, author, novels, books, recipes, classes, chess, grades, homework, pregnant)) = extra.get(me) else { return Facts::default() };
    // (Books read to the end: novels, and skill books to their last page.)
    let novels_read = novels.map_or(0, |n| n.0.iter().filter(|r| r.finished > 0).count());
    let skill_read = books.map_or(0, |b| b.0.iter().filter(|(k, pages)| gdata.skill_books.iter().any(|x| x.key == *k && *pages >= x.pages as f32)).count());
    let lot = lot.map(|l| l.0);
    Facts {
        motives: motives.0,
        lot,
        at_work,
        journal,
        author,
        books_read: (novels_read + skill_read) as u32,
        recipes,
        classes,
        chess: chess.copied(),
        pregnant,
        grade: grades.map(|g| g.0),
        homework,
        party: lot.is_none() && party.is_some_and(|p| p.on()),
        date: date.and_then(|d| if d.a == me { ids.get(&d.b) } else if d.b == me { ids.get(&d.a) } else { None }).copied(),
        plants,
    }
}

/// The panel's lists from the engine's: the offers on show (oldest first) and the promises; and
/// back from the panel, what was promised there or given up.
#[allow(clippy::type_complexity)]
fn sync_wishes(
    mut sims: Query<(Entity, &Sim, &mut Dreams, &mut crate::wishes::Wishes, &Skills, Option<&Job>, &Relationships, &Mood)>,
    people: PeopleQuery,
    extra: ExtraQuery,
    (gu, ui, family): (Option<Res<crate::icons::GameUi>>, Option<Res<crate::layout::UiAssets>>, Option<Res<crate::family::Genealogy>>),
) {
    let Some(gu) = gu else { return };
    let gdata = gu.data.clone();
    let data = &gdata.dnp;
    if data.trees.is_empty() {
        return;
    }
    let (roster, ids) = roster(&people);
    let town = Town::default();
    let owned: Vec<(String, i64)> = Vec::new();
    for (me, sim, mut d, mut w, skills, job, rels, mood) in &mut sims {
        if !d.ready {
            continue;
        }
        let d = &mut *d;
        let facts = facts_of(me, &extra, &gdata, &ids, None, None, 0);
        let cx = Ctx { sim, skills, job, rels, mood: mood.0, funds: 0, worth: 0, people: &roster, ids: &ids, owned: &owned, data, facts: &facts, town: &town, family: family.as_deref(), gdata: Some(&gdata) };
        // From the panel: a wish moved to the promises, or one gone from either.
        let promised_now: Vec<u64> = w.promised.iter().filter_map(|x| x.dream().map(|k| k.uid)).collect();
        let offered_now: Vec<u64> = w.offered.iter().filter_map(|x| x.dream().map(|k| k.uid)).collect();
        for uid in &promised_now {
            if !d.is_promised(*uid)
                && d.queue.contains(uid)
                && let Some(slot) = d.promised.iter().position(|s| s.is_none())
            {
                d.queue.retain(|u| u != uid);
                d.promised[slot] = Some(*uid);
                debug!("dreams: {} promised #{uid}", sim.first);
                if let Some(a) = d.active.iter_mut().find(|a| a.uid == *uid) {
                    a.promised = true;
                }
                // (Its promise-linked children, offered now.)
                if let Some(a) = d.active.iter().find(|a| a.uid == *uid).cloned() {
                    let mut buckets = HashMap::new();
                    let links: Vec<usize> = data.trees[a.tree].nodes[a.node].children.iter().copied().filter(|c| data.trees[a.tree].nodes[*c].promise_link).collect();
                    let mut parent = a.clone();
                    d.bucket += 1;
                    parent.done = Some(d.bucket);
                    for ci in links {
                        score_child(d, &cx, data, &parent, ci, true, Some(*uid), a.since, &mut buckets);
                    }
                    for (_, cands) in buckets {
                        if let Some((u, _)) = cands.into_iter().max_by(|x, y| x.1.total_cmp(&y.1)) {
                            d.queue.push(u);
                        }
                    }
                }
            }
        }
        let panel: Vec<u64> = promised_now.iter().chain(offered_now.iter()).copied().collect();
        for uid in std::mem::take(&mut d.shown) {
            if !panel.contains(&uid) {
                d.queue.retain(|u| *u != uid);
                for s in d.promised.iter_mut() {
                    if *s == Some(uid) {
                        *s = None;
                    }
                }
                d.active.retain(|a| a.uid != uid);
            }
        }
        // To the panel.
        let ambitious = sim.traits.contains(&Trait::Ambitious);
        let wish = |uid: u64| -> Option<crate::wishes::Wish> {
            let a = d.active.iter().find(|a| a.uid == uid)?;
            let t = &data.trees[a.tree];
            let n = &t.nodes[a.node];
            let p = data.primitive(n.prim)?;
            let (text, description) = words(p, n, &a.subject, &cx, ui.as_deref(), &gdata);
            let (icon, icon2) = icons(p, n, &a.subject, &cx, &gdata);
            let progress = crate::dreamfns::count_text(&p.count_text, n, a, &cx);
            let helper = helps(data, d.lifetime, n, &a.subject);
            Some(crate::wishes::Wish { kind: crate::wishes::WishKind::Dream(DreamWish { tree: t.id, node: n.id, subject: a.subject.clone(), uid, text, description, icon, icon2, progress, helper }), points: points(n.score, ambitious, helper) })
        };
        let mut queue = d.queue.clone();
        queue.sort_by(|a, b| {
            let at = |u: &u64| d.active.iter().find(|x| x.uid == *u).and_then(|x| x.offered_at).unwrap_or(0.0);
            at(a).total_cmp(&at(b))
        });
        let offered: Vec<crate::wishes::Wish> = queue.iter().take(SHOWN).filter_map(|u| wish(*u)).collect();
        // (Promises of the old kind, from saves before these: kept till they're done.)
        let promised: Vec<crate::wishes::Wish> = w.promised.iter().filter(|x| x.dream().is_none()).cloned().chain(d.promised.iter().flatten().filter_map(|u| wish(*u))).collect();
        d.shown = offered.iter().chain(promised.iter()).filter_map(|x| x.dream().map(|k| k.uid)).collect();
        if w.offered != offered {
            w.offered = offered;
        }
        if w.promised != promised {
            w.promised = promised;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_round_down_as_the_games() {
        assert_eq!(points(150.0, false, false), 150);
        assert_eq!(points(250.0, true, false), 275);
        assert_eq!(points(500.0, true, false), 575);
        assert_eq!(points(1000.0, true, false), 1000, "1150 rounds down to a thousand");
        assert_eq!(points(30000.0, true, false), 34500);
        // (Towards the lifetime wish: half again, as the game's table has it.)
        assert_eq!(points(150.0, false, true), 225);
        assert_eq!(points(250.0, true, true), 425);
        assert_eq!(points(300.0, true, true), 500);
        assert_eq!(points(1000.0, false, true), 1500);
    }

    #[test]
    fn weights_interpolate_between_their_ends() {
        assert_eq!(interpolate(5.0, 10.0, 0.0), 0.5);
        assert_eq!(interpolate(20.0, 10.0, 0.0), 1.0);
        assert_eq!(interpolate(-5.0, 10.0, 0.0), 0.0);
        assert_eq!(interpolate(3.0, 3.0, 3.0), 1.0);
    }

    #[test]
    fn categories_take_in_their_kinds() {
        let tv = s3bake::dnp::DnpCategory { key: "TV".into(), classes: vec!["Sims3.Gameplay.Objects.Electronics.TV".into()], ..Default::default() };
        assert!(of_category("Sims3.Gameplay.Objects.Electronics.TVCheap", &tv));
        assert!(!of_category("Sims3.Gameplay.Objects.Electronics.Stereo", &tv));
        let shower = s3bake::dnp::DnpCategory { key: "Shower".into(), classes: vec!["Sims3.Gameplay.Objects.Plumbing.IShowerable".into()], ..Default::default() };
        assert!(of_category("Sims3.Gameplay.Objects.Plumbing.ShowerPublic", &shower));
    }
}
