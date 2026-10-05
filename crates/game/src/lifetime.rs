//! Lifetime wishes: the dream of a Sim's life, from the game's own list of lifetime wishes
//! (what each asks for — the top of a career, two skills mastered, twenty friends, §50,000 in
//! the bank — its icon, and the lifetime happiness it's worth). Five that suit a Sim's traits
//! are offered in Create-a-Sim and when a child becomes a teen; Sims who move in without one
//! take whichever suits them best. Its progress shows in the Simology panel and beside the
//! wishes; fulfilling it is worth tens of thousands of lifetime happiness points.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::PlayMode;
use crate::careers::Job;
use crate::clock::GameClock;
use crate::dialog::{Answer, Answered, Ask, Question, Questions};
use crate::interact::{GameObject, Household, Notifications, Skills};
use crate::life::{LifeEvent, LifeEventKind, MoodletKind, Moodlets, Trait};
use crate::sim::{Age, HouseholdMember, Sim};
use crate::social::{RelStatus, Relationships};

pub struct LifetimePlugin;

impl Plugin for LifetimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChosenLifetimeWishes>()
            .add_systems(Update, (assign_lifetime_wishes, lifetime_answers, track_lifetime_wishes).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// What a lifetime wish asks for (`n` being its number).
#[derive(Clone, Copy, Debug)]
pub enum Goal {
    /// Reach level `n` of a career, along one of its branches.
    Career { career: &'static str, branch: Option<&'static str> },
    /// Master each of these skills.
    Master(&'static [&'static str]),
    /// Master any `n` skills.
    MasterAny,
    /// Have `n` simoleons in the bank.
    Cash,
    /// Be worth `n` simoleons, home and belongings included.
    NetWorth,
    /// Have `n` friends or better.
    Friends,
    /// Go steady with `n` different Sims.
    Sweethearts,
    /// See `n` children grow up into teens.
    RaiseChildren,
    /// Reach level 5 in `n` careers.
    CareerHopper,
}

pub struct LifetimeWishDef {
    /// The game's check for it: the key to its row of the converted table.
    pub check: &'static str,
    pub name: &'static str,
    /// What it asks, with `{n}` for its number.
    pub desc: &'static str,
    pub goal: Goal,
    /// Its number, icon and score as the game's table has them (used until that's converted).
    pub n: f32,
    pub icon: &'static str,
    pub score: u32,
    /// The traits that make a Sim likely to want it.
    pub traits: &'static [Trait],
}

impl LifetimeWishDef {
    fn baked<'a>(&self, data: Option<&'a s3bake::GameDataBaked>) -> Option<&'a s3bake::gamedata::LifetimeWishInfo> {
        data?.lifetime_wishes.iter().find(|w| w.check == self.check)
    }
    pub fn number(&self, data: Option<&s3bake::GameDataBaked>) -> f32 {
        self.baked(data).map(|w| w.number).filter(|n| *n > 0.0).unwrap_or(self.n)
    }
    pub fn icon(&self, data: Option<&s3bake::GameDataBaked>) -> String {
        self.baked(data).map_or(self.icon.to_string(), |w| w.icon.clone())
    }
    /// The lifetime happiness it's worth: ten times its fulfillment score.
    pub fn points(&self, data: Option<&s3bake::GameDataBaked>) -> u32 {
        self.baked(data).map(|w| w.score).filter(|s| *s > 0).unwrap_or(self.score) * 10
    }
    pub fn describe(&self, data: Option<&s3bake::GameDataBaked>) -> String {
        self.desc.replace("{n}", &group(self.number(data) as i64))
    }
    /// The branch of a career it lies along.
    pub fn career_branch(&self, career: &str) -> Option<&'static str> {
        match self.goal {
            Goal::Career { career: c, branch } if c == career => branch,
            _ => None,
        }
    }
}

/// 50000 → "50,000".
pub fn group(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

macro_rules! career {
    ($check:literal, $name:literal, $desc:literal, $career:literal, $branch:expr, $n:literal, $icon:literal, $score:literal, $traits:expr) => {
        LifetimeWishDef { check: $check, name: $name, desc: $desc, goal: Goal::Career { career: $career, branch: $branch }, n: $n, icon: $icon, score: $score, traits: $traits }
    };
}

use Trait as T;

/// The base game's lifetime wishes that can be fulfilled here.
pub static LIFETIME_WISHES: &[LifetimeWishDef] = &[
    career!("Level8OfBusinessMajorDreamCheckFunction", "CEO of a Mega-Corporation", "Reach level {n} of the Business career", "Business", None, 8.0, "w_lifetime_business_career", 2750, &[T::Ambitious, T::Charismatic, T::Workaholic, T::Schmoozer]),
    career!("Level10OfPoliticalMajorDreamCheckFunction", "Leader of the Free World", "Reach level {n} of the Political career", "Political", None, 10.0, "w_lifetime_political_career", 3000, &[T::Charismatic, T::Ambitious, T::Schmoozer, T::Friendly]),
    career!("Level10OfMilitaryMajorDreamCheckFunction", "Become an Astronaut", "Reach level {n} of the Military career", "Military", None, 10.0, "w_lifetime_military_career", 3000, &[T::Brave, T::Athletic, T::Daredevil, T::LovesTheOutdoors]),
    career!("Level10OfJournalismMajorDreamCheckFunction", "Star News Anchor", "Reach level {n} of the Journalism career", "Journalism", None, 10.0, "w_lifetime_journalism_career", 3000, &[T::Charismatic, T::Bookworm, T::Schmoozer, T::Inappropriate]),
    career!("Level10OfMedicalMajorDreamCheckFunction", "World Renowned Surgeon", "Reach level {n} of the Medical career", "Medical", None, 10.0, "w_lifetime_medical_career", 3250, &[T::Good, T::Genius, T::Brave, T::Perfectionist]),
    career!("Level10OfCulinaryMajorDreamCheckFunction", "Celebrated Five-Star Chef", "Reach level {n} of the Culinary career", "Culinary", None, 10.0, "w_lifetime_culinary_career", 3000, &[T::NaturalCook, T::Perfectionist, T::Snob]),
    career!("Level9OfScienceMajorDreamCheckFunction", "Become a Creature-Robot Cross Breeder", "Reach level {n} of the Science career", "Science", None, 9.0, "w_lifetime_science_career", 2900, &[T::Genius, T::GreenThumb, T::Insane, T::Handy]),
    career!("Level9OfProSportsMajorDreamCheckFunction", "Become a Superstar Athlete", "Reach level {n} of the Professional Sports career", "Professional Sports", None, 9.0, "w_lifetime_sports_career", 2900, &[T::Athletic, T::Daredevil, T::HotHeaded]),
    career!("Level10OfCriminalThiefBranchMajorDreamCheckFunction", "Become a Master Thief", "Reach level {n} of the Criminal career, on the Thief path", "Criminal", Some("Thief"), 10.0, "w_lifetime_criminal_thief", 3250, &[T::Kleptomaniac, T::Evil, T::NightOwl]),
    career!("Level10OfCriminalEvilBranchMajorDreamCheckFunction", "The Emperor of Evil", "Reach level {n} of the Criminal career, on the Evil path", "Criminal", Some("Evil"), 10.0, "w_lifetime_criminal_evil", 3250, &[T::Evil, T::MeanSpirited, T::Insane, T::HotHeaded]),
    career!("Level10OfLawEnforcementSpecialAgentBranchMajorDreamCheckFunction", "International Super Spy", "Reach level {n} of the Law Enforcement career, on the Special Agent path", "Law Enforcement", Some("SpecialAgent"), 10.0, "w_lifetime_law_career_spy", 3000, &[T::Brave, T::Daredevil, T::Athletic]),
    career!("Level10OfLawEnforcementForensicBranchMajorDreamCheckFunction", "Forensic Specialist: Dynamic DNA Profiler", "Reach level {n} of the Law Enforcement career, on the Forensic Analyst path", "Law Enforcement", Some("ForensicAnalyst"), 10.0, "w_lifetime_law_career_forensic", 3000, &[T::Genius, T::Neat, T::Perfectionist]),
    career!("Level10OfMusicRockBranchMajorDreamCheckFunction", "Rock Star", "Reach level {n} of the Music career, on the Electric Rock path", "Music", Some("ElectricRock"), 10.0, "w_lifetime_music_career_rock", 3000, &[T::Virtuoso, T::PartyAnimal, T::Excitable]),
    career!("Level10OfMusicSymphonicBranchMajorDreamCheckFunction", "Hit Movie Composer", "Reach level {n} of the Music career, on the Symphonic path", "Music", Some("Symphonic"), 10.0, "w_lifetime_music_career_composer", 3000, &[T::Virtuoso, T::Artistic, T::Perfectionist]),
    LifetimeWishDef { check: "NSimoleonsInCashMajorDreamCheckFunction", name: "Swimming in Cash", desc: "Have §{n} in the household's funds", goal: Goal::Cash, n: 50000.0, icon: "w_lifetime_simoleon_cash", score: 3500, traits: &[T::Frugal, T::Ambitious, T::Workaholic, T::Snob] },
    LifetimeWishDef { check: "NNetWorthMajorDreamCheckFunction", name: "Lifestyle of the Rich and Famous", desc: "Be worth §{n}, home and belongings included", goal: Goal::NetWorth, n: 100000.0, icon: "w_lifetime_net_worth", score: 3250, traits: &[T::Snob, T::Ambitious, T::Charismatic] },
    LifetimeWishDef { check: "HaveNFriendsOrBetterMajorDreamCheckFunction", name: "Super Popular", desc: "Have {n} friends", goal: Goal::Friends, n: 20.0, icon: "w_lifetime_have_n_friends", score: 3000, traits: &[T::Friendly, T::PartyAnimal, T::Charismatic, T::Schmoozer] },
    LifetimeWishDef { check: "BoyfriendOrGirlfriendNDifferentSimsMajorDreamCheckFunction", name: "Heartbreaker", desc: "Go steady with {n} different Sims", goal: Goal::Sweethearts, n: 10.0, icon: "W_lifetime_date_different_sims", score: 3000, traits: &[T::Flirty, T::CommitmentIssues, T::GreatKisser, T::HopelessRomantic] },
    LifetimeWishDef { check: "CharismaAndGutarL10MajorDreamCheckFunction", name: "Golden Tongue, Golden Fingers", desc: "Master the Charisma and Guitar skills", goal: Goal::Master(&["Charisma", "Guitar"]), n: 0.0, icon: "w_lifetime_charisma_guitar_L10", score: 3000, traits: &[T::Charismatic, T::Virtuoso, T::Schmoozer] },
    LifetimeWishDef { check: "PaintingAndGuitarL10MajorDreamCheckFunction", name: "Master of the Arts", desc: "Master the Painting and Guitar skills", goal: Goal::Master(&["Painting", "Guitar"]), n: 0.0, icon: "w_lifetime_painting_guitar", score: 3000, traits: &[T::Artistic, T::Virtuoso] },
    LifetimeWishDef { check: "PaintingAndWritingL10MajorDreamCheckFunction", name: "Illustrious Author", desc: "Master the Painting and Writing skills", goal: Goal::Master(&["Painting", "Writing"]), n: 0.0, icon: "w_lifetime_painting_writing", score: 3000, traits: &[T::Artistic, T::Bookworm] },
    LifetimeWishDef { check: "LogicAndAthleticL10MajorDreamCheckFunction", name: "Perfect Mind, Perfect Body", desc: "Master the Logic and Athletic skills", goal: Goal::Master(&["Logic", "Athletic"]), n: 0.0, icon: "w_lifetime_logic_and_athletic", score: 3000, traits: &[T::Genius, T::Athletic, T::Perfectionist] },
    LifetimeWishDef { check: "LogicAndHandinesL10MajorDreamCheckFunction", name: "The Tinkerer", desc: "Master the Logic and Handiness skills", goal: Goal::Master(&["Logic", "Handiness"]), n: 0.0, icon: "w_lifetime_logic_handiness", score: 3000, traits: &[T::Handy, T::Genius, T::Technophobe] },
    LifetimeWishDef { check: "ReachLevel10InNSkillsMajorDreamCheckFunction", name: "Renaissance Sim", desc: "Master {n} skills", goal: Goal::MasterAny, n: 3.0, icon: "w_lifetime_reach_L10_skills", score: 3500, traits: &[T::Genius, T::Perfectionist, T::Bookworm, T::Ambitious] },
    LifetimeWishDef { check: "RaiseNChildrenFromBabyToYoungAdultCheckFunction", name: "Surrounded by Family", desc: "See {n} children grow up into teens", goal: Goal::RaiseChildren, n: 5.0, icon: "w_lifetime_raise_baby_to_YA", score: 3500, traits: &[T::FamilyOriented, T::Good, T::Childish] },
    LifetimeWishDef { check: "ReachLevel5In4CareersMajorDreamCheckFunction", name: "Jack of All Trades", desc: "Reach level 5 in {n} different careers", goal: Goal::CareerHopper, n: 4.0, icon: "w_lifetime_L5_in4_careers", score: 3500, traits: &[T::Ambitious, T::Excitable, T::Absentminded] },
];

/// A Sim's lifetime wish and how far along it is.
#[derive(Component, Clone, Debug, Default)]
pub struct LifetimeWish {
    /// Index into `LIFETIME_WISHES`.
    pub wish: usize,
    pub fulfilled: bool,
    /// Careers in which the Sim has reached level 5.
    pub careers: Vec<String>,
    /// Children seen growing up into teens.
    pub raised: u32,
    /// How far along (0..1) and what that means, as last checked.
    pub progress: f32,
    pub status: String,
    /// Picked for them (not by the player).
    pub auto: bool,
}

impl LifetimeWish {
    pub fn new(wish: usize) -> Self {
        Self { wish: wish.min(LIFETIME_WISHES.len() - 1), ..default() }
    }
    pub fn def(&self) -> &'static LifetimeWishDef {
        &LIFETIME_WISHES[self.wish.min(LIFETIME_WISHES.len() - 1)]
    }
    /// By the game's check name (saves).
    pub fn by_check(check: &str) -> Option<usize> {
        LIFETIME_WISHES.iter().position(|d| d.check == check)
    }
}

/// Lifetime wishes picked in Create-a-Sim, by Sim id, until the Sims arrive.
#[derive(Resource, Default)]
pub struct ChosenLifetimeWishes(pub HashMap<u64, usize>);

/// Waiting for the player to pick a lifetime wish.
#[derive(Component)]
pub struct ChoosingLifetimeWish;

/// Whether a Sim could want this at all.
fn suits(d: &LifetimeWishDef, sim: &Sim) -> bool {
    !(matches!(d.goal, Goal::RaiseChildren) && sim.traits.contains(&Trait::DislikesChildren))
}

/// The `k` lifetime wishes that suit a Sim best (by their traits, the rest in an order of their
/// own).
pub fn suggestions(sim: &Sim, k: usize) -> Vec<usize> {
    let mut ranked: Vec<(u64, usize)> = LIFETIME_WISHES
        .iter()
        .enumerate()
        .filter(|(_, d)| suits(d, sim))
        .map(|(i, d)| {
            let matches = d.traits.iter().filter(|t| sim.traits.contains(t)).count() as u64;
            let jitter = sim.id.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(i as u64 * 0xBF58_476D_1CE4_E5B9).rotate_left(17) >> 40;
            (matches << 32 | jitter, i)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    ranked.into_iter().take(k).map(|(_, i)| i).collect()
}

/// Asks the player to pick a lifetime wish for a Sim from five that suit them.
pub fn ask_lifetime_wish(questions: &mut Questions, data: Option<&s3bake::GameDataBaked>, e: Entity, sim: &Sim) {
    let wishes = suggestions(sim, 5);
    let answers = wishes
        .iter()
        .map(|&i| {
            let d = &LIFETIME_WISHES[i];
            Answer { label: d.name.to_string(), detail: format!("{} · {} lifetime happiness", d.describe(data), group(d.points(data) as i64)), icon: d.icon(data) }
        })
        .collect();
    questions.ask(Ask {
        about: Question::LifetimeWish { sim: e, wishes },
        icon: "hud_icon_plumbob_r2".into(),
        heading: format!("Lifetime Wish for {}", sim.first),
        title: format!("{} is growing up!", sim.first),
        text: format!("{} has decided what {} wants out of life. Choose a Lifetime Wish:", sim.first, if sim.female { "she" } else { "he" }),
        answers,
    });
}

/// Gives household Sims old enough a lifetime wish: the one picked in Create-a-Sim, the
/// player's pick for a child just grown into a teen, or the one that suits them best.
#[allow(clippy::type_complexity)]
fn assign_lifetime_wishes(
    mut commands: Commands,
    mut events: MessageReader<LifeEvent>,
    mut chosen: ResMut<ChosenLifetimeWishes>,
    mut questions: ResMut<Questions>,
    ui: Option<Res<crate::icons::GameUi>>,
    sims: Query<(Entity, &Sim, Has<ChoosingLifetimeWish>), (With<HouseholdMember>, Without<LifetimeWish>)>,
) {
    let data = ui.as_ref().map(|u| &*u.data);
    // A child who just became a teen chooses.
    for ev in events.read() {
        if matches!(ev.kind, LifeEventKind::Birthday)
            && let Ok((e, sim, false)) = sims.get(ev.sim)
            && sim.age == Age::Teen
            && !chosen.0.contains_key(&sim.id)
        {
            ask_lifetime_wish(&mut questions, data, e, sim);
            commands.entity(e).insert(ChoosingLifetimeWish);
        }
    }
    for (e, sim, choosing) in &sims {
        if choosing || sim.age.is_little() || sim.age == Age::Child || questions.queue.iter().any(|q| q.about.sim() == Some(e)) {
            continue;
        }
        // (Not over one a saved game is putting back this same frame.)
        match chosen.0.remove(&sim.id) {
            Some(w) => commands.entity(e).insert_if_new(LifetimeWish::new(w)),
            None => commands.entity(e).insert_if_new(LifetimeWish { auto: true, ..LifetimeWish::new(suggestions(sim, 1).first().copied().unwrap_or(0)) }),
        };
    }
}

fn lifetime_answers(mut commands: Commands, mut answers: MessageReader<Answered>, sims: Query<&Sim>, mut notes: ResMut<Notifications>) {
    for a in answers.read() {
        let Question::LifetimeWish { sim: e, wishes } = &a.about else { continue };
        let (Ok(sim), Some(&w)) = (sims.get(*e), wishes.get(a.answer)) else { continue };
        notes.push(format!("{}'s Lifetime Wish: {}.", sim.first, LIFETIME_WISHES[w].name));
        commands.entity(*e).remove::<ChoosingLifetimeWish>().insert(LifetimeWish::new(w));
    }
}

/// Where a Sim stands, for measuring their lifetime wish.
struct Standing<'a> {
    skills: &'a Skills,
    job: Option<&'a Job>,
    rels: &'a Relationships,
    funds: i64,
    worth: i64,
    raised: u32,
    careers: &'a [String],
}

/// How far along a lifetime wish is (0..1, past 1 when done) and what that means.
fn measure(d: &LifetimeWishDef, data: Option<&s3bake::GameDataBaked>, at: &Standing) -> (f32, String) {
    let n = d.number(data);
    let skills = at.skills;
    let (progress, status) = match d.goal {
        Goal::Career { career, branch } => match at.job.filter(|j| j.career().name == career) {
            Some(j) if branch.is_some_and(|b| j.path().branch != b) && j.career().branch_at.is_some_and(|at| j.level >= at) => {
                (0.0, format!("On the {} path — this wish needs the {} path", j.path().label(), crate::careers::branch_label(branch.unwrap_or(""))))
            }
            Some(j) => ((j.level + 1) as f32 / n, format!("{}: level {} of {n}", j.info().title, j.level + 1)),
            None => (0.0, format!("Join the {career} career")),
        },
        Goal::Master(list) => {
            let have: Vec<String> = list.iter().map(|s| format!("{s} {}/10", skills.level(s))).collect();
            (list.iter().map(|s| skills.level(s) as f32 / 10.0).sum::<f32>() / list.len() as f32, have.join(" · "))
        }
        Goal::MasterAny => {
            let mastered = skills.0.keys().filter(|s| skills.level(s) >= 10).count();
            let mut levels: Vec<u32> = skills.0.keys().map(|s| skills.level(s)).collect();
            levels.sort_by(|a, b| b.cmp(a));
            let sum: u32 = levels.into_iter().take(n as usize).sum();
            (sum as f32 / (10.0 * n), format!("{mastered} of {n} skills mastered"))
        }
        Goal::Cash => (at.funds as f32 / n, format!("§{} of §{}", group(at.funds), group(n as i64))),
        Goal::NetWorth => (at.worth as f32 / n, format!("Worth §{} of §{}", group(at.worth), group(n as i64))),
        Goal::Friends => {
            let k = at.rels.0.values().filter(|r| r.friendship >= 15.0).count();
            (k as f32 / n, format!("{k} of {n} friends"))
        }
        Goal::Sweethearts => {
            let k = at.rels.0.values().filter(|r| r.status != RelStatus::None).count();
            (k as f32 / n, format!("Gone steady with {k} of {n}"))
        }
        Goal::RaiseChildren => (at.raised as f32 / n, format!("{} of {n} children grown into teens", at.raised)),
        Goal::CareerHopper => (at.careers.len() as f32 / n, format!("Level 5 in {} of {n} careers", at.careers.len())),
    };
    (progress.clamp(0.0, 1.0), status)
}

/// Checks each lifetime wish's progress, and rewards the ones fulfilled.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn track_lifetime_wishes(
    clock: Res<GameClock>,
    mut events: MessageReader<LifeEvent>,
    household: Option<Res<Household>>,
    ui: Option<Res<crate::icons::GameUi>>,
    objects: Query<&GameObject>,
    mut sims: Query<(Entity, &Sim, &mut LifetimeWish, &Skills, Option<&Job>, &Relationships, Option<&mut crate::wishes::Wishes>, &mut Moodlets), With<HouseholdMember>>,
    ages: Query<&Sim, With<HouseholdMember>>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    mut next_check: Local<f64>,
) {
    let data = ui.as_ref().map(|u| &*u.data);
    let mut changed = false;
    for ev in events.read() {
        changed = true;
        match ev.kind {
            // A child grown into a teen counts for each grown-up of the household.
            LifeEventKind::Birthday if ages.get(ev.sim).is_ok_and(|s| s.age == Age::Teen) => {
                for (_, sim, mut w, ..) in &mut sims {
                    if !sim.age.is_little() && sim.age != Age::Child && sim.age != Age::Teen {
                        w.raised += 1;
                    }
                }
            }
            _ => {}
        }
    }
    if !changed && clock.minutes < *next_check {
        return;
    }
    *next_check = clock.minutes + 15.0;
    let funds = household.as_ref().map_or(0, |h| h.funds);
    let worth = funds + objects.iter().map(|o| o.price as i64).sum::<i64>();
    for (_, sim, mut w, skills, job, rels, wishes, mut moodlets) in &mut sims {
        // Careers where they've reached level 5.
        if let Some(j) = job
            && j.level >= 4
            && !w.careers.iter().any(|c| c == j.career().name)
        {
            w.careers.push(j.career().name.to_string());
        }
        let careers = w.careers.clone();
        let at = Standing { skills, job, rels, funds, worth, raised: w.raised, careers: &careers };
        // One picked for them that's half done already isn't much of a dream: the next that
        // suits them instead.
        if w.auto && w.status.is_empty() {
            let fresh = suggestions(sim, LIFETIME_WISHES.len()).into_iter().find(|&i| measure(&LIFETIME_WISHES[i], data, &at).0 < 0.5);
            if let Some(i) = fresh {
                w.wish = i;
            }
        }
        let d = w.def();
        let (progress, status) = measure(d, data, &at);
        if w.fulfilled {
            if w.status != "Fulfilled!" {
                w.progress = 1.0;
                w.status = "Fulfilled!".into();
            }
            continue;
        }
        if (w.progress - progress).abs() > 1e-4 || w.status != status {
            w.progress = progress;
            w.status = status;
        }
        if progress >= 1.0 {
            w.fulfilled = true;
            w.status = "Fulfilled!".into();
            let points = d.points(data);
            if let Some(mut wishes) = wishes {
                wishes.points += points;
            }
            moodlets.add(MoodletKind::WishFulfilled, clock.minutes);
            play.write(crate::sound::PlaySound::ui("sting_good_event").with_volume(0.8));
            notes.push(format!("{} has fulfilled {} Lifetime Wish, {}, and earned {} lifetime happiness!", sim.first, if sim.female { "her" } else { "his" }, d.name, group(points as i64)));
        }
    }
}
