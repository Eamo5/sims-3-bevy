//! Writing books: at a computer, a Sim writes a novel in a genre they've opened up (by Writing
//! skill, traits and books already written, as the game's Writing tuning has it), page by
//! page at their own speed, paid a little for partial work as they go. A finished book gets
//! one of the game's titles and turns out a flop, a success, a hit or a best seller (by skill,
//! and more likely good in a genre they've written often); royalties come in at noon once a
//! week, six times — more for longer genres, more skilled writers, suited traits and better
//! books.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::clock::{GameClock, SimDelta};
use crate::interact::{ActionKind, ActionQueue, GameObject, Household, Notifications, Phase, Skills, Special, interactions_for};
use crate::life::Trait;
use crate::sim::{Age, HouseholdMember, Sim};

pub struct WritingPlugin;

impl Plugin for WritingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (write_pages, pay_royalties).run_if(in_state(PlayMode::Live)));
    }
}

pub struct Genre {
    /// The tuning's name for it (`kLengthSciFiMin`).
    pub key: &'static str,
    pub name: &'static str,
    /// Traits whose writers earn more from it.
    pub traits: &'static [Trait],
}

const fn genre(key: &'static str, name: &'static str, traits: &'static [Trait]) -> Genre {
    Genre { key, name, traits }
}

pub static GENRES: [Genre; 14] = [
    genre("Fiction", "Fiction", &[]),
    genre("NonFiction", "Non-Fiction", &[Trait::Bookworm]),
    genre("SciFi", "Sci-Fi", &[Trait::Genius, Trait::ComputerWhiz]),
    genre("Trashy", "Trashy", &[Trait::Flirty]),
    genre("Drama", "Drama", &[Trait::OverEmotional]),
    genre("Childrens", "Children's", &[Trait::Childish, Trait::FamilyOriented]),
    genre("Humor", "Humor", &[Trait::GoodSenseOfHumor]),
    genre("Satire", "Satire", &[Trait::GoodSenseOfHumor, Trait::Inappropriate]),
    genre("Fantasy", "Fantasy", &[Trait::Insane]),
    genre("Historical", "Historical", &[Trait::Snob]),
    genre("Mystery", "Mystery", &[Trait::Neurotic]),
    genre("Romance", "Romance", &[Trait::HopelessRomantic]),
    genre("Vaudeville", "Vaudeville", &[Trait::PartyAnimal]),
    genre("Masterpiece", "Masterpiece", &[Trait::Perfectionist]),
];

pub fn genre_index(key: &str) -> Option<usize> {
    GENRES.iter().position(|g| g.key == key)
}

/// How a book was received.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Flop,
    Success,
    Hit,
    BestSeller,
}

impl Quality {
    pub fn name(self) -> &'static str {
        match self {
            Quality::Flop => "a flop",
            Quality::Success => "a success",
            Quality::Hit => "a hit",
            Quality::BestSeller => "a best seller",
        }
    }
}

/// A book under way.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub genre: String,
    pub title: String,
    pub pages: f32,
    pub length: f32,
    /// Fifths sent in as partial work.
    pub sent: u32,
}

/// A finished book and its royalties.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub title: String,
    pub genre: String,
    pub quality: Quality,
    /// Paid each time.
    pub royalty: i64,
    pub payments_left: u32,
    /// The day of the next payment.
    pub next_pay: u32,
}

/// A Sim's writing: the book under way and those finished.
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Author {
    pub draft: Option<Draft>,
    pub books: Vec<Book>,
}

impl Author {
    pub fn written(&self, genre: &str) -> u32 {
        self.books.iter().filter(|b| b.genre == genre).count() as u32
    }
    /// Royalties still coming in each week.
    pub fn weekly_royalties(&self) -> i64 {
        self.books.iter().filter(|b| b.payments_left > 0).map(|b| b.royalty).sum()
    }
}

/// The genre picked for the next book.
#[derive(Component)]
pub struct NovelPlan(pub usize);

/// A Writing tuning value.
fn tune(data: Option<&s3bake::GameDataBaked>, key: &str, default: f32) -> f32 {
    data.and_then(|d| d.writing.get(key).copied()).unwrap_or(default)
}

/// Whether a Sim can write in a genre yet.
pub fn unlocked(g: &Genre, sim: &Sim, skills: &Skills, author: Option<&Author>, data: Option<&s3bake::GameDataBaked>) -> bool {
    let level = skills.level("Writing") as f32;
    let has = |t: Trait| sim.traits.contains(&t);
    let written = |k: &str| author.map_or(0, |a| a.written(k)) as f32;
    let t = |k: &str, d: f32| tune(data, k, d);
    match g.key {
        "Fiction" | "NonFiction" => true,
        "SciFi" => level >= t("kMinLevelForSciFi", 1.0),
        "Trashy" => level >= t("kMinLevelForTrashy", 2.0),
        "Drama" => level >= t("kMinLevelForDrama", 3.0),
        "Childrens" => level >= t("kMinLevelForChildrens", 3.0) && skills.level("Painting") as f32 >= t("kMinLevelPaintingForChildrens", 4.0),
        "Humor" => level >= if has(Trait::GoodSenseOfHumor) { t("kMinLevelForHumorGoodSenseOfHumor", 2.0) } else { t("kMinLevelForHumor", 5.0) },
        "Satire" => written("Humor") >= t("kNumHumorWrittenForSatire", 3.0),
        "Fantasy" => written("SciFi") >= t("kNumSciFiWrittenForFantasy", 3.0),
        "Historical" => level >= t("kMinLevelForHistory", 6.0) && sim.age == Age::Elder,
        "Mystery" => level >= t("kMinLevelForMystery", 8.0),
        "Romance" => level >= if has(Trait::HopelessRomantic) { t("kMinLevelForRomanceHelplessRomantic", 5.0) } else { t("kMinLevelForRomance", 10.0) },
        "Vaudeville" => ["Drama", "SciFi", "Humor", "Mystery", "Romance"].iter().all(|k| written(k) >= t("kNumEachGenreForVaudeville", 2.0)),
        "Masterpiece" => author.map_or(0, |a| a.books.len()) as f32 >= t("kNumBooksWrittenForMasterpiece", 25.0),
        _ => false,
    }
}

/// Pages a minute: the base rate, more for bookworms and with skill.
fn pages_per_minute(sim: &Sim, skills: &Skills, data: Option<&s3bake::GameDataBaked>) -> f32 {
    let level = skills.level("Writing") as f32;
    tune(data, "kRateBasePPM", 0.12)
        + if sim.traits.contains(&Trait::Bookworm) { tune(data, "kRateBookWormBonusPPM", 0.03) } else { 0.0 }
        + tune(data, "kRateMaxWritingSkillPPM", 0.25) * level / 10.0
}

fn start_draft(g: &Genre, author: &Author, data: Option<&s3bake::GameDataBaked>, rng: &mut impl Rng) -> Draft {
    let (lo, hi) = (tune(data, &format!("kLength{}Min", g.key), 100.0), tune(data, &format!("kLength{}Max", g.key), 120.0));
    let used: Vec<&str> = author.books.iter().map(|b| b.title.as_str()).collect();
    let titles: Vec<&String> = data
        .and_then(|d| d.book_titles.iter().find(|(k, _)| k == g.key))
        .map(|(_, t)| t.iter().filter(|t| !used.contains(&t.as_str())).collect())
        .unwrap_or_default();
    let title = titles.choose(rng).map_or_else(|| format!("{} Novel", g.name), |t| t.to_string());
    Draft { genre: g.key.to_string(), title, pages: 0.0, length: rng.random_range(lo..=hi.max(lo)).round(), sent: 0 }
}

/// How a finished book does, and what it earns each week.
fn publish(d: &Draft, sim: &Sim, skills: &Skills, author: &Author, day: u32, data: Option<&s3bake::GameDataBaked>, rng: &mut impl Rng) -> Book {
    let level = skills.level("Writing").clamp(1, 10);
    let t = |k: String, d: f32| tune(data, &k, d);
    // Chances (percent) by skill level; books written in the genre make good ones likelier.
    let practice = (author.written(&d.genre) as f32 * t("kQualityPercentChangePerHiddenSkillPoint".into(), 1.5)).min(t("kQualityMaxPercentChangeForHiddenSkill".into(), 60.0));
    let flop = (t(format!("kQualityLevel{level}ChanceFlop"), 10.0) - practice).max(0.0);
    let hit = t(format!("kQualityLevel{level}ChanceHit"), 10.0) + practice;
    let best = t(format!("kQualityLevel{level}ChanceBestSeller"), 5.0) + practice;
    let r = rng.random_range(0.0..100.0);
    let quality = if r < best {
        Quality::BestSeller
    } else if r < best + hit {
        Quality::Hit
    } else if r < best + hit + flop {
        Quality::Flop
    } else {
        Quality::Success
    };
    let g = &GENRES[genre_index(&d.genre).unwrap_or(0)];
    let (lo, hi) = (t(format!("kRoyalty{}Min", g.key), 25.0), t(format!("kRoyalty{}Max", g.key), 35.0));
    let base = rng.random_range(lo..=hi.max(lo));
    let skill = 1.0 + t("kRoyaltyMaxWritingSkillMultiplier".into(), 1.0) * skills.level("Writing") as f32 / 10.0;
    let hidden = (1.0 + t("kRoayltyMultiplierChangePerHiddenSkillPoint".into(), 0.1) * author.written(&d.genre) as f32).min(t("kRoyaltyMaxPercentHiddenSkill".into(), 2.0));
    let traits = 1.0 + t("kRoyaltyTraitMultiplier".into(), 0.25) * g.traits.iter().filter(|x| sim.traits.contains(x)).count() as f32;
    let reception = match quality {
        Quality::Flop => t("kRoyaltyQualityMultiplierFlop".into(), 0.35),
        Quality::Success => 1.0,
        Quality::Hit => t("kRoyaltyQualityMultiplierHit".into(), 1.25),
        Quality::BestSeller => t("kRoyaltyQualityMultiplierBestSeller".into(), 1.6),
    };
    Book {
        title: d.title.clone(),
        genre: d.genre.clone(),
        quality,
        royalty: (base * skill * hidden * traits * reception).round() as i64,
        payments_left: t("kRoyaltyLength".into(), 6.0) as u32,
        next_pay: day,
    }
}

/// Writing at a computer: pages written, partial work sent in, and the book finished.
#[allow(clippy::type_complexity)]
fn write_pages(
    mut commands: Commands,
    clock: Res<GameClock>,
    delta: Res<SimDelta>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut household: Option<ResMut<Household>>,
    mut writers: Query<(Entity, &Sim, &mut ActionQueue, &Skills, Option<&mut Author>, Option<&NovelPlan>, Option<&crate::wishes::Wishes>), With<HouseholdMember>>,
    objects: Query<&GameObject>,
    mut notes: ResMut<Notifications>,
) {
    let data = ui.as_ref().map(|u| &*u.data);
    let mut rng = rand::rng();
    for (e, sim, mut queue, skills, author, plan, wishes) in &mut writers {
        let Some(front) = queue.0.front_mut() else { continue };
        let (ActionKind::Object { target, def }, Phase::Running(_)) = (&front.kind, &front.phase) else { continue };
        let writing = objects.get(*target).ok().and_then(|o| interactions_for(o.kind).get(*def)).is_some_and(|d| d.special == Special::WriteNovel);
        if !writing {
            continue;
        }
        let Some(mut author) = author else {
            commands.entity(e).insert(Author::default());
            continue;
        };
        // A new book, in the genre chosen (or Fiction), setting aside any under way.
        if let Some(p) = plan {
            commands.entity(e).remove::<NovelPlan>();
            let g = &GENRES[p.0.min(GENRES.len() - 1)];
            if let Some(old) = author.draft.take() {
                notes.push(format!("{} set aside “{}” to start something new.", sim.first, old.title));
            }
            let d = start_draft(g, &author, data, &mut rng);
            notes.push(format!("{} started writing a {} book: “{}”.", sim.first, g.name, d.title));
            author.draft = Some(d);
        }
        if author.draft.is_none() {
            let d = start_draft(&GENRES[0], &author, data, &mut rng);
            notes.push(format!("{} started writing a Fiction book: “{}”.", sim.first, d.title));
            author.draft = Some(d);
        }
        let ppm = pages_per_minute(sim, skills, data);
        let level = skills.level("Writing").clamp(1, 10);
        let Some(d) = author.draft.as_mut() else { continue };
        d.pages = (d.pages + ppm * delta.0).min(d.length);
        // Partial work goes in every fifth of the way, at so much a page.
        let every = tune(data, "kPartialWorkSubmitEveryXPercent", 20.0) / 100.0;
        let fifths = ((d.pages / d.length) / every).floor() as u32;
        if fifths > d.sent && d.pages < d.length {
            let pages = (fifths - d.sent) as f32 * every * d.length;
            d.sent = fifths;
            let pay = (pages * tune(data, &format!("kPartialWorkPageValue{level}"), 0.5)).round() as i64;
            if let Some(h) = household.as_mut() {
                h.funds += pay;
            }
        }
        if d.pages >= d.length {
            let d = author.draft.take().unwrap_or_else(|| start_draft(&GENRES[0], &Author::default(), data, &mut rng));
            let mut book = publish(&d, sim, skills, &author, clock.day(), data, &mut rng);
            // The Extra Creative's work sells better.
            if crate::wishes::has(wishes, "ExtraCreative") {
                book.royalty = book.royalty * 3 / 2;
            }
            let g = GENRES[genre_index(&book.genre).unwrap_or(0)].name;
            notes.push(format!(
                "{} finished writing “{}”, a {g} book. It's {}! Royalties of §{} will come in each week, {} times.",
                sim.first,
                book.title,
                book.quality.name(),
                book.royalty,
                book.payments_left
            ));
            author.books.push(book);
            if let Some(f) = queue.0.front_mut() {
                f.cancel = true;
            }
        }
    }
}

/// Royalties: at noon, once a week for each book still paying.
fn pay_royalties(
    clock: Res<GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut household: Option<ResMut<Household>>,
    mut authors: Query<(Entity, &Sim, &mut Author, Option<&crate::wishes::Wishes>), With<HouseholdMember>>,
    mut notes: ResMut<Notifications>,
    mut did: MessageWriter<crate::journal::Did>,
) {
    let data = ui.as_ref().map(|u| &*u.data);
    if clock.hour_f() < tune(data, "kRoyaltyPayHour", 12.0) {
        return;
    }
    let day = clock.day();
    for (e, sim, mut a, wishes) in &mut authors {
        // (Bigger checks for a High Roller.)
        let factor = if crate::wishes::has(wishes, "HighRoller") { 1.5 } else { 1.0 };
        let mut paid = 0;
        let mut titles = Vec::new();
        for b in a.books.iter_mut().filter(|b| b.payments_left > 0 && b.next_pay <= day) {
            paid += (b.royalty as f32 * factor).round() as i64;
            b.payments_left -= 1;
            b.next_pay = day + 7;
            titles.push(format!("“{}”", b.title));
        }
        if paid > 0 {
            if let Some(h) = household.as_mut() {
                h.funds += paid;
            }
            notes.push(format!("{} received §{paid} in royalties for {}.", sim.first, titles.join(", ")));
            did.write(crate::journal::Did::count(e, crate::journal::Stat::Royalties, paid as f64));
        }
    }
}
