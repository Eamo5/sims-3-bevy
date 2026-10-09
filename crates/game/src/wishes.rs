//! Wishes and lifetime happiness: Sims wish for things that suit their personality and
//! situation; the player promises up to four; fulfilling them earns lifetime happiness points
//! to spend on lifetime rewards — the game's reward traits, with their names, icons, words and
//! costs, those whose effects are carried out here.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{Notifications, Skills};
use crate::life::{LifeEvent, LifeEventKind, MoodletKind, Moodlets, Trait};
use crate::sim::*;

pub struct WishesPlugin;

impl Plugin for WishesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (offer_wishes, fulfil_wishes, buy_rewards).chain().run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum WishKind {
    Skill { skill: &'static str, level: u32 },
    Activity(&'static str),
    Social(&'static str),
    MakeFriend,
    FirstKiss,
    GoSteady,
    GetEngaged,
    GetMarried,
    JoinCareer,
    Promotion,
    BuySomething { min_price: i32 },
}

#[derive(Clone, Debug)]
pub struct Wish {
    pub kind: WishKind,
    pub points: u32,
}

impl Wish {
    /// The game's icon for the wish.
    pub fn icon(&self, data: &s3bake::GameDataBaked) -> String {
        let s = |n: &str| n.to_string();
        match &self.kind {
            WishKind::Skill { skill, .. } => data.skill(skill).map(|k| k.wish_icon.clone()).unwrap_or_default(),
            WishKind::Activity(a) => s(match *a {
                "Watch TV" => "w_tv",
                "Read a Book" => "w_book",
                "Play Chess" => "w_chess",
                "Dance" => "w_stereo",
                "Cook Dinner" => "w_stove",
                "Take Bath" => "w_bathtub",
                "Work Out" => "w_workout_bench",
                "Paint" => "w_painting",
                "Play Guitar" => "w_guitar",
                "Play Computer Games" => "W_computer",
                _ => "",
            }),
            WishKind::Social(n) => s(match *n {
                "Tell Joke" => "w_joke_around",
                "Flirt" => "w_first_kiss",
                "Try for Baby" => "moodlet_theBabyIsComing",
                _ => "w_friend",
            }),
            WishKind::MakeFriend => s("w_friend"),
            WishKind::FirstKiss => s("w_first_kiss"),
            WishKind::GoSteady => s("moodlet_myLove"),
            WishKind::GetEngaged => s("moodlet_newlyEngaged"),
            WishKind::GetMarried => s("w_wedding_arch"),
            WishKind::JoinCareer => s("w_career_cityhall"),
            WishKind::Promotion => s("w_simoleon"),
            WishKind::BuySomething { .. } => s("w_simoleon_32"),
        }
    }

    pub fn text(&self) -> String {
        match &self.kind {
            WishKind::Skill { skill, level } => format!("Reach level {level} {skill}"),
            WishKind::Activity(a) => (*a).to_string(),
            WishKind::Social(s) => format!("{s} with someone"),
            WishKind::MakeFriend => "Make a new friend".into(),
            WishKind::FirstKiss => "Have a first kiss".into(),
            WishKind::GoSteady => "Go steady".into(),
            WishKind::GetEngaged => "Get engaged".into(),
            WishKind::GetMarried => "Get married".into(),
            WishKind::JoinCareer => "Get a job".into(),
            WishKind::Promotion => "Get promoted".into(),
            WishKind::BuySomething { min_price } => format!("Buy something worth §{min_price}"),
        }
    }
}

/// A Sim's wishes and lifetime happiness.
#[derive(Component, Default, Clone)]
pub struct Wishes {
    pub offered: Vec<Wish>,
    pub promised: Vec<Wish>,
    pub points: u32,
    /// Lifetime rewards bought (their reward traits' names: `SteelBladder`).
    pub rewards: Vec<String>,
    next_offer: f64,
}

impl Wishes {
    pub fn restored(points: u32, rewards: Vec<String>, now: f64) -> Self {
        // (Saves from before kept the rewards' display names.)
        let rewards = rewards
            .into_iter()
            .map(|r| match r.as_str() {
                "Steel Bladder" => "SteelBladder".to_string(),
                "Hardly Hungry" => "HardlyHungry".to_string(),
                "Fast Learner" => "FastLearner".to_string(),
                _ => r,
            })
            .filter(|r| REWARDS.contains(&r.as_str()))
            .collect();
        Self { points, rewards, next_offer: now, ..default() }
    }

    pub fn promise(&mut self, i: usize) {
        if i < self.offered.len() && self.promised.len() < 4 {
            let w = self.offered.remove(i);
            self.promised.push(w);
        }
    }
    pub fn has_reward(&self, r: &str) -> bool {
        self.rewards.iter().any(|x| x == r)
    }
}

/// The lifetime rewards offered: the game's reward traits whose effects are carried out here.
pub const REWARDS: [&str; 28] = [
    "SteelBladder",
    "PermaClean",
    "HardlyHungry",
    "FastLearner",
    "FastMetabolism",
    "ProfessionalSlacker",
    "Opportunistic",
    "Attractive",
    "ExtraCreative",
    "SuperGreenThumb",
    "DiscountDiner",
    "ComplimentaryEntertainment",
    "BookshopBargainer",
    "Haggler",
    "SpeedyCleaner",
    "MultiTasker",
    "HighRoller",
    "Vacationer",
    "LegendaryHost",
    "LongDistanceFriend",
    "FertilityTreatment",
    "ChangeLifetimeWish",
    "MidLifeCrisis",
    "FoodReplicator",
    "BodySculptor",
    "MoodModifier",
    "Teleporter",
    "CollectionHelper",
];

/// The rewards that are objects: the reward, and the catalogue object it is (given to the Sim
/// to place on their lot).
pub const REWARD_OBJECTS: [(&str, &str); 5] = [
    ("FoodReplicator", "FoodReplicator"),
    ("BodySculptor", "BodySculptor"),
    ("MoodModifier", "MoodletManager"),
    ("Teleporter", "Teleporter"),
    ("CollectionHelper", "CollectionHelper"),
];

/// Whether a Sim has a lifetime reward.
pub fn has(w: Option<&Wishes>, r: &str) -> bool {
    w.is_some_and(|w| w.has_reward(r))
}

/// Need decay changes from lifetime rewards.
pub fn reward_decay(w: Option<&Wishes>, motive: usize) -> f32 {
    match motive {
        BLADDER if has(w, "SteelBladder") => 0.0,
        HUNGER if has(w, "HardlyHungry") => 0.25,
        HYGIENE if has(w, "PermaClean") => 0.25,
        _ => 1.0,
    }
}

pub fn reward_skill_rate(w: Option<&Wishes>) -> f32 {
    if has(w, "FastLearner") { 1.25 } else { 1.0 }
}

/// What a Sim pays for a rabbit hole's activity: nothing at restaurants for a Discount
/// Diner, or at shows for Complimentary Entertainment; less at the shops for a Haggler.
pub fn price_factor(w: Option<&Wishes>, activity: &str) -> f32 {
    match activity {
        "Eat a Meal" | "Have a Drink with Friends" if has(w, "DiscountDiner") => 0.0,
        "See a Show" if has(w, "ComplimentaryEntertainment") => 0.0,
        "Buy Seeds" if has(w, "Haggler") => 0.75,
        _ => 1.0,
    }
}

/// What a recipe book costs a Sim (Bookshop Bargainers and Hagglers pay less).
pub fn book_price(w: Option<&Wishes>, price: i32) -> i64 {
    let mut p = price as f32;
    if has(w, "BookshopBargainer") {
        p *= 0.5;
    }
    if has(w, "Haggler") {
        p *= 0.75;
    }
    p.round() as i64
}

/// Asks the player which lifetime reward to buy (those offered and not yet bought).
pub fn ask_reward(questions: &mut crate::dialog::Questions, data: &s3bake::GameDataBaked, e: Entity, sim: &Sim, w: &Wishes) {
    let rewards: Vec<String> = REWARDS.iter().filter(|r| !w.has_reward(r)).map(|r| r.to_string()).collect();
    let mut answers: Vec<crate::dialog::Answer> = rewards
        .iter()
        .map(|r| {
            let t = data.traits.iter().find(|t| t.hex == *r);
            let cost = t.map_or(0, |t| t.points);
            let afford = if w.points >= cost { "" } else { " (not enough yet)" };
            let desc = t.map_or(String::new(), |t| t.desc.replace("{0.SimFirstName}", &sim.first));
            crate::dialog::Answer {
                label: format!("{} — {} lifetime happiness{afford}", t.map_or(r.as_str(), |t| t.name.as_str()), crate::lifetime::group(cost as i64)),
                detail: desc,
                icon: t.map_or(String::new(), |t| t.icon.clone()),
            }
        })
        .collect();
    answers.push(crate::dialog::Answer { label: "Close".into(), detail: String::new(), icon: String::new() });
    let owned: Vec<String> = w.rewards.iter().map(|r| data.traits.iter().find(|t| t.hex == *r).map_or(r.clone(), |t| t.name.clone())).collect();
    questions.ask(crate::dialog::Ask {
        about: crate::dialog::Question::Reward { sim: e, rewards },
        icon: "hud_icon_plumbob_r2".into(),
        heading: format!("Lifetime Rewards for {}", sim.first),
        title: format!("{} lifetime happiness to spend", crate::lifetime::group(w.points as i64)),
        text: if owned.is_empty() { "Choose a reward:".into() } else { format!("Already earned: {}. Choose a reward:", owned.join(", ")) },
        answers,
    });
}

/// A lifetime reward chosen: bought, if there's lifetime happiness enough.
fn buy_rewards(
    mut commands: Commands,
    mut answers: MessageReader<crate::dialog::Answered>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut sims: Query<(&Sim, &mut Wishes)>,
    mut questions: ResMut<crate::dialog::Questions>,
    mut notes: ResMut<Notifications>,
) {
    let Some(ui) = ui else { return };
    for a in answers.read() {
        let crate::dialog::Question::Reward { sim: e, rewards } = &a.about else { continue };
        let (Ok((sim, mut w)), Some(r)) = (sims.get_mut(*e), rewards.get(a.answer)) else { continue };
        let Some(t) = ui.data.traits.iter().find(|t| t.hex == *r) else { continue };
        if w.points < t.points {
            notes.push(format!("{} needs {} more lifetime happiness for {}.", sim.first, crate::lifetime::group((t.points - w.points) as i64), t.name));
            continue;
        }
        w.points -= t.points;
        notes.push(format!("{} gained the {} lifetime reward!", sim.first, t.name));
        // A new lifetime wish is chosen there and then; an object's theirs to place; the
        // rest last.
        if let Some((_, object)) = REWARD_OBJECTS.iter().find(|(x, _)| x == r) {
            crate::inventory::give(&mut commands, *e, crate::inventory::ItemKind::Reward, object.to_string(), t.name.clone(), 0, 0, 1);
            notes.push(format!("The {} is in {}'s inventory, to place on the lot.", t.name, sim.first));
            w.rewards.push(r.clone());
        } else if r == "MidLifeCrisis" {
            commands.insert_resource(crate::midlife::TraitPicker::open(*e, &sim.traits, t.points));
        } else if r == "ChangeLifetimeWish" {
            crate::lifetime::ask_lifetime_wish(&mut questions, Some(&ui.data), *e, sim);
            commands.entity(*e).remove::<crate::lifetime::LifetimeWish>().insert(crate::lifetime::ChoosingLifetimeWish);
        } else {
            w.rewards.push(r.clone());
        }
    }
}

/// Wishes that suit this Sim right now.
fn candidates(sim: &Sim, skills: &Skills, has_job: bool, romance: Option<crate::social::RelStatus>, kissed: bool) -> Vec<(WishKind, u32, f32)> {
    let has = |t: Trait| sim.traits.contains(&t);
    let mut out: Vec<(WishKind, u32, f32)> = Vec::new();
    let mut skill = |name: &'static str, weight: f32| {
        let next = skills.level(name) + 1;
        if next <= 10 {
            out.push((WishKind::Skill { skill: name, level: next }, 150 + next * 75, weight));
        }
    };
    skill("Cooking", if has(Trait::NaturalCook) { 3.0 } else { 1.0 });
    skill("Logic", if has(Trait::Genius) { 3.0 } else { 0.7 });
    skill("Painting", if has(Trait::Artistic) { 3.0 } else { 0.6 });
    skill("Guitar", if has(Trait::Virtuoso) || has(Trait::Artistic) { 3.0 } else { 0.6 });
    skill("Writing", if has(Trait::Bookworm) { 3.0 } else { 0.5 });
    skill("Athletic", if has(Trait::Athletic) { 3.0 } else { 0.6 });
    skill("Charisma", if has(Trait::Charismatic) { 3.0 } else { 0.5 });
    let mut act = |a: &'static str, pts: u32, w: f32| out.push((WishKind::Activity(a), pts, w));
    act("Watch TV", 100, if has(Trait::CouchPotato) { 3.0 } else { 0.6 });
    act("Read a Book", 150, if has(Trait::Bookworm) { 3.0 } else { 0.6 });
    act("Play Chess", 150, if has(Trait::Genius) { 2.5 } else { 0.5 });
    act("Dance", 150, if has(Trait::PartyAnimal) { 3.0 } else { 0.6 });
    act("Cook Dinner", 200, if has(Trait::NaturalCook) { 2.5 } else { 0.8 });
    act("Take Bath", 100, if has(Trait::Neat) { 2.0 } else { 0.5 });
    act("Work Out", 200, if has(Trait::Athletic) { 3.0 } else if has(Trait::Lazy) { 0.05 } else { 0.4 });
    act("Paint", 200, if has(Trait::Artistic) { 3.0 } else { 0.4 });
    act("Play Guitar", 200, if has(Trait::Virtuoso) { 3.0 } else { 0.4 });
    act("Play Computer Games", 100, if has(Trait::ComputerWhiz) { 3.0 } else { 0.5 });
    let social_w = if has(Trait::Friendly) || has(Trait::PartyAnimal) { 2.5 } else if has(Trait::Loner) { 0.2 } else { 1.0 };
    out.push((WishKind::Social("Chat"), 100, social_w));
    out.push((WishKind::Social("Tell Joke"), 150, if has(Trait::GoodSenseOfHumor) { 3.0 } else { 0.6 }));
    out.push((WishKind::MakeFriend, 400, social_w));
    if sim.age != Age::Child {
        let romantic = if has(Trait::HopelessRomantic) || has(Trait::Flirty) { 3.0 } else if has(Trait::Unflirty) { 0.1 } else { 0.8 };
        out.push((WishKind::Social("Flirt"), 150, romantic));
        if !kissed {
            out.push((WishKind::FirstKiss, 500, romantic));
        }
        match romance {
            None | Some(crate::social::RelStatus::Ex) | Some(crate::social::RelStatus::None) => out.push((WishKind::GoSteady, 750, romantic * 0.6)),
            Some(crate::social::RelStatus::Partner) => out.push((WishKind::GetEngaged, 1500, romantic + if has(Trait::FamilyOriented) { 1.5 } else { 0.3 })),
            Some(crate::social::RelStatus::Engaged) => out.push((WishKind::GetMarried, 2500, 3.0)),
            Some(crate::social::RelStatus::Married) => {}
        }
        if matches!(romance, Some(crate::social::RelStatus::Partner | crate::social::RelStatus::Engaged | crate::social::RelStatus::Married))
            && sim.age != Age::Teen
            && sim.age != Age::Elder
        {
            let w = if has(Trait::FamilyOriented) { 2.5 } else if has(Trait::DislikesChildren) { 0.0 } else { 0.4 };
            out.push((WishKind::Social("Try for Baby"), 1000, w));
        }
        if has_job {
            out.push((WishKind::Promotion, 1000, if has(Trait::Ambitious) || has(Trait::Workaholic) { 3.0 } else { 0.8 }));
        } else {
            out.push((WishKind::JoinCareer, 500, if has(Trait::Ambitious) { 3.0 } else if has(Trait::Lazy) { 0.3 } else { 1.2 }));
        }
        out.push((WishKind::BuySomething { min_price: 500 }, 300, if has(Trait::Snob) { 2.0 } else if has(Trait::Frugal) { 0.2 } else { 0.6 }));
    }
    out
}

#[allow(clippy::type_complexity)]
fn offer_wishes(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut sims: Query<(Entity, &Sim, &Skills, Option<&crate::careers::Job>, &crate::social::Relationships, Option<&mut Wishes>), With<HouseholdMember>>,
) {
    let mut rng = rand::rng();
    for (e, sim, skills, job, rels, wishes) in &mut sims {
        let Some(mut w) = wishes else {
            commands.entity(e).insert(Wishes { next_offer: clock.minutes, ..default() });
            continue;
        };
        // Babies and toddlers don't have wishes yet.
        if clock.minutes < w.next_offer || sim.age.is_little() {
            continue;
        }
        w.next_offer = clock.minutes + rng.random_range(120.0..300.0);
        let partner = rels.partner().map(|p| p.1);
        let kissed = rels.0.values().any(|r| r.kissed);
        let cands = candidates(sim, skills, job.is_some(), partner, kissed);
        // Replace the oldest offered wish, keeping three on offer.
        if w.offered.len() >= 3 {
            w.offered.remove(0);
        }
        for _ in 0..8 {
            if w.offered.len() >= 3 {
                break;
            }
            let Ok(pick) = cands.choose_weighted(&mut rng, |c| c.2) else { break };
            let dup = w.offered.iter().chain(w.promised.iter()).any(|x| x.kind == pick.0);
            if !dup {
                w.offered.push(Wish { kind: pick.0.clone(), points: pick.1 });
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn fulfil_wishes(
    clock: Res<GameClock>,
    mut events: MessageReader<LifeEvent>,
    mut sims: Query<(&Sim, &mut Wishes, &mut Moodlets, &crate::social::Relationships)>,
    mut notes: ResMut<Notifications>,
) {
    for ev in events.read() {
        let Ok((sim, mut w, mut moodlets, rels)) = sims.get_mut(ev.sim) else { continue };
        let matches = |k: &WishKind| match (&ev.kind, k) {
            (LifeEventKind::SkillUp { skill, level }, WishKind::Skill { skill: s, level: l }) => skill == s && level >= l,
            (LifeEventKind::Finished { activity, .. }, WishKind::Activity(a)) => activity == a,
            (LifeEventKind::Socialized { social, .. }, WishKind::Social(s)) => social == s,
            (LifeEventKind::Socialized { other, .. }, WishKind::MakeFriend) => {
                let f = rels.friendship(*other);
                (15.0..30.0).contains(&f)
            }
            (LifeEventKind::FirstKiss, WishKind::FirstKiss) => true,
            (LifeEventKind::StartedDating, WishKind::GoSteady) => true,
            (LifeEventKind::Engaged, WishKind::GetEngaged) => true,
            (LifeEventKind::Married, WishKind::GetMarried) => true,
            (LifeEventKind::NewJob, WishKind::JoinCareer) => true,
            (LifeEventKind::Promoted, WishKind::Promotion) => true,
            (LifeEventKind::Bought { price }, WishKind::BuySomething { min_price }) => price >= min_price,
            _ => false,
        };
        let mut gained = 0;
        let mut done = Vec::new();
        let w = &mut *w;
        for (promised, list) in [(true, &mut w.promised), (false, &mut w.offered)] {
            list.retain(|x| {
                if matches(&x.kind) {
                    let pts = if promised { x.points } else { x.points / 2 };
                    gained += pts;
                    done.push((x.text(), promised));
                    false
                } else {
                    true
                }
            });
        }
        if gained > 0 {
            w.points += gained;
            moodlets.add(MoodletKind::WishFulfilled, clock.minutes);
            for (t, promised) in done {
                if promised {
                    notes.push(format!("{} fulfilled a wish: {t}!", sim.first));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every lifetime reward offered is one of the game's, with its cost (needs the bake:
    /// `SIMS3_CACHE=<baked> cargo test -p sims3 rewards_are_the_games -- --ignored`).
    #[test]
    #[ignore]
    fn rewards_are_the_games() {
        let data = s3bake::gamedata::load_gamedata(&s3bake::default_root()).expect("gamedata");
        for r in REWARDS {
            let t = data.traits.iter().find(|t| t.hex == r);
            assert!(t.is_some_and(|t| t.points > 0 && !t.name.is_empty()), "{r}: {:?}", t.map(|t| (&t.name, t.points)));
        }
        // (Those not yet offered, with `--nocapture`.)
        for t in data.traits.iter().filter(|t| t.points > 0 && !REWARDS.contains(&t.hex.as_str())) {
            println!("not offered: {} ({}, {})", t.hex, t.name, t.points);
        }
    }
}
