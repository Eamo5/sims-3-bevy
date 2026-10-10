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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn royalties_begin_next_sunday_noon_after_publication() {
        assert_eq!(next_royalty_day(0.0, 12.0), 6);
        assert_eq!(next_royalty_day(6.0 * 1440.0 + 719.0, 12.0), 6);
        assert_eq!(next_royalty_day(6.0 * 1440.0 + 720.0, 12.0), 13);
        assert_eq!(next_royalty_day(7.0 * 1440.0, 12.0), 13);
    }

    #[test]
    fn royalty_payments_keep_sunday_schedule_across_reload_and_missed_weeks() {
        let mut app = App::new();
        app.init_resource::<GameClock>().init_resource::<Notifications>()
            .add_message::<crate::journal::Did>().add_systems(Update, pay_royalties);
        let author = Author { draft: None, books: vec![Book {
            title: "Sunday Book".into(), genre: "Fiction".into(), quality: Quality::Success,
            royalty: 100, payments_left: 6, next_pay: 0, // Legacy weekday is normalized.
        }] };
        let sim = crate::sim::random_sim(&mut rand::rng(), "Writer", Some(true), Age::Adult);
        let e = app.world_mut().spawn((sim, HouseholdMember, author)).id();
        app.world_mut().resource_mut::<GameClock>().minutes = 6.0 * 1440.0 + 720.0;
        app.update();
        assert_eq!(app.world().get::<Author>(e).unwrap().books[0].payments_left, 6, "no household means no payment consumed");
        app.insert_resource(Household { name: "Authors".into(), funds: 0, lot_index: 0, last_bill_day: 0, bills: vec![] });
        app.world_mut().resource_mut::<GameClock>().minutes -= 1.0;
        app.update();
        assert_eq!(app.world().resource::<Household>().funds, 0);
        app.world_mut().resource_mut::<GameClock>().minutes += 1.0;
        app.update();
        assert_eq!(app.world().resource::<Household>().funds, 100);
        let restored: Author = serde_json::from_str(&serde_json::to_string(app.world().get::<Author>(e).unwrap()).unwrap()).unwrap();
        app.world_mut().entity_mut(e).insert(restored);
        app.update();
        assert_eq!(app.world().resource::<Household>().funds, 100, "reload must not repeat a paid installment");
        // Monday morning after two more due Sundays: collect both, retaining cadence.
        app.world_mut().resource_mut::<GameClock>().minutes = 21.0 * 1440.0 + 480.0;
        app.update();
        assert_eq!(app.world().resource::<Household>().funds, 300);
        let book = &app.world().get::<Author>(e).unwrap().books[0];
        assert_eq!((book.payments_left, book.next_pay), (3, 27));
        app.world_mut().resource_mut::<GameClock>().minutes = 100.0 * 1440.0;
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Household>().funds, 600, "exactly six installments total");
        assert_eq!(app.world().get::<Author>(e).unwrap().books[0].payments_left, 0);
    }

    #[test]
    fn perfectionist_page_rate_scales_skill_and_bookworm_bonuses() {
        let mut rng = rand::rng();
        let mut sim = crate::sim::random_sim(&mut rng, "Author", Some(true), Age::Adult);
        for level in [0.0, 5.0, 10.0] {
            let skills = Skills([("Writing", level)].into_iter().collect());
            for bookworm in [false, true] {
                sim.traits = if bookworm { vec![Trait::Bookworm] } else { vec![] };
                let normal = pages_per_minute(&sim, &skills, None);
                sim.traits.push(Trait::Perfectionist);
                assert!((pages_per_minute(&sim, &skills, None) - normal * 0.8).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn slower_writing_delays_partial_work_payment_until_pages_are_written() {
        use crate::interact::{Action, ObjectKind};
        let mut app = App::new();
        app.init_resource::<GameClock>().init_resource::<Notifications>()
            .insert_resource(SimDelta(180.0))
            .insert_resource(Household { name: "Authors".into(), funds: 0, lot_index: 0, last_bill_day: 0, bills: vec![] })
            .add_systems(Update, write_pages);
        let computer = app.world_mut().spawn(GameObject {
            kind: ObjectKind::Computer, name: "Computer".into(), objd: (0, 0, 0), price: 0,
            center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None,
        }).id();
        let def = interactions_for(ObjectKind::Computer).iter().position(|d| d.special == Special::WriteNovel).unwrap();
        let mut action = Action::new("Write Novel", ActionKind::Object { target: computer, def }, false);
        action.phase = Phase::Running(0.0);
        let mut queue = ActionQueue::default();
        queue.0.push_back(action);
        let mut sim = crate::sim::random_sim(&mut rand::rng(), "Author", Some(true), Age::Adult);
        sim.traits = vec![Trait::Perfectionist];
        let writer = app.world_mut().spawn((sim, HouseholdMember, queue, Skills::default(), Author {
            draft: Some(Draft { genre: "Fiction".into(), title: "A Careful Novel".into(), pages: 0.0, length: 100.0, sent: 0 }), books: vec![],
        })).id();
        app.update();
        let draft = app.world().get::<Author>(writer).unwrap().draft.as_ref().unwrap();
        assert!((draft.pages - 17.28).abs() < 0.001);
        assert_eq!(draft.sent, 0);
        assert_eq!(app.world().resource::<Household>().funds, 0);
        app.world_mut().resource_mut::<SimDelta>().0 = 30.0;
        app.update();
        let draft = app.world().get::<Author>(writer).unwrap().draft.as_ref().unwrap();
        assert!((draft.pages - 20.16).abs() < 0.001);
        assert_eq!(draft.sent, 1);
        assert_eq!(app.world().resource::<Household>().funds, 10);
        app.world_mut().resource_mut::<SimDelta>().0 = 0.0;
        app.update();
        assert_eq!(app.world().resource::<Household>().funds, 10, "partial work must not pay twice");
    }
}

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
    let rate = tune(data, "kRateBasePPM", 0.12)
        + if sim.traits.contains(&Trait::Bookworm) { tune(data, "kRateBookWormBonusPPM", 0.03) } else { 0.0 }
        + tune(data, "kRateMaxWritingSkillPPM", 0.25) * level / 10.0;
    // TraitTuning.kPerfectionistTraitWritingPagesSlowerPerMinuteMultiplier.
    rate * if sim.traits.contains(&Trait::Perfectionist) { 0.8 } else { 1.0 }
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
fn publish(d: &Draft, sim: &Sim, skills: &Skills, author: &Author, minutes: f64, data: Option<&s3bake::GameDataBaked>, rng: &mut impl Rng) -> Book {
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
        next_pay: next_royalty_day(minutes, tune(data, "kRoyaltyPayHour", 12.0)),
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
            let mut book = publish(&d, sim, skills, &author, clock.minutes, data, &mut rng);
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

/// The next Sunday payment strictly after publication (day zero is Monday).
fn next_royalty_day(minutes: f64, hour: f32) -> u32 {
    let day = (minutes / 1440.0).floor() as u32;
    let sunday = day + (6 - day % 7);
    if minutes >= sunday as f64 * 1440.0 + hour as f64 * 60.0 { sunday + 7 } else { sunday }
}

/// Royalties: Sunday at noon, once a week for each book still paying.
fn pay_royalties(
    clock: Res<GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    household: Option<ResMut<Household>>,
    mut authors: Query<(Entity, &Sim, &mut Author, Option<&crate::wishes::Wishes>), With<HouseholdMember>>,
    mut notes: ResMut<Notifications>,
    mut did: MessageWriter<crate::journal::Did>,
) {
    let data = ui.as_ref().map(|u| &*u.data);
    let Some(mut household) = household else { return };
    let pay_hour = tune(data, "kRoyaltyPayHour", 12.0) as f64;
    for (e, sim, mut a, wishes) in &mut authors {
        // (Bigger checks for a High Roller.)
        let factor = if crate::wishes::has(wishes, "HighRoller") { 1.33333 } else { 1.0 };
        let mut paid = 0;
        let mut titles = Vec::new();
        for b in a.books.iter_mut().filter(|b| b.payments_left > 0) {
            // Older saves used the publication weekday. Keep unpaid installments,
            // but align their next deadline to the following Sunday.
            b.next_pay += 6 - b.next_pay % 7;
            let mut installments = 0;
            while b.payments_left > 0 && clock.minutes >= b.next_pay as f64 * 1440.0 + pay_hour * 60.0 {
                paid += (b.royalty as f32 * factor).round() as i64;
                b.payments_left -= 1;
                b.next_pay += 7;
                installments += 1;
            }
            if installments > 0 { titles.push(format!("“{}”", b.title)); }
        }
        if paid > 0 {
            household.funds += paid;
            notes.push(format!("{} received §{paid} in royalties for {}.", sim.first, titles.join(", ")));
            did.write(crate::journal::Did::count(e, crate::journal::Stat::Royalties, paid as f64));
        }
    }
}
