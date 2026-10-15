//! The town's venues' own menus, as the game's rabbit holes have them (`ShowVenue`, `Stadium`,
//! `Restaurant`, `DaySpa`, `Grocery`, `Mausoleum`, `VisitRabbitHole`):
//!
//! - the theatre's shows: an evening show every night from five (§40) and a matinee at the
//!   weekend from noon (§30), a movie most nights, at the weekend one night in two a play, a
//!   symphony or a concert, each with the game's moodlet (Saw a Great Play...);
//! - the stadium's games, Sundays and Wednesdays from two (§60, Saw a Great Game), and concerts
//!   on Friday and Saturday evenings (§40);
//! - eating at the bistro (§50, §75 for dinner, a fifth off before six) or the diner (§12, §18),
//!   and drinks, filling the Sim up, the bistro's meal better than the diner's;
//! - the day spa's massages, facials, body treatments, nails and day packages, three levels
//!   each at the game's prices and lengths, each with its moodlet, the better ones taking bad
//!   moodlets away;
//! - selling the produce and fish carried at the grocery (a tenth over their worth), and going in
//!   to shop there (`shop`: its ingredients, by tab or recipe, and selling);
//! - a visit to city hall, the science lab, the military base, the theatre or the mausoleum:
//!   Bored, by age, or Excited, Fascinated, Intrigued, Impressed or Educated; and exploring the
//!   mausoleum's catacombs, for what's to be found (and now and then a fright).

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::life::MoodletKind;
use crate::rabbitholes::Activity;
use crate::sim::Age;

pub struct VenuesPlugin;

impl Plugin for VenuesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, venue_done.run_if(in_state(PlayMode::Live)));
    }
}

/// Visit activity numbers for the venues' own: this plus the index in `ACTS`.
pub const VENUE_TASK: usize = 4000;

/// What a venue's activity does on coming out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// A moodlet of this strength, this long (minutes).
    Moodlet(MoodletKind, i32, f64),
    /// A spa treatment's moodlet, taking this many bad moodlets away.
    Treatment(MoodletKind, i32, f64, usize),
    /// A meal out: hungry no more, and a moodlet as good as the place.
    Meal(i32),
    /// Drinks: hungry no more.
    Drinks,
    /// A visit: Bored by the age's chance (child, teen, grown-up, in a hundred), else one of the
    /// good ones.
    Visit([u8; 3]),
    /// The catacombs explored.
    Explore,
    /// The produce and fish sold.
    Sell,
    /// Gone in to shop (`shop`'s dialog, while they're there).
    Shop,
}

/// A venue's activity: where (words in the lot's name), when (days of the week, Monday 0; and
/// the hours it can be started in), and what it does.
pub struct VenueAct {
    pub venue: &'static [&'static str],
    pub days: &'static [u32],
    pub activity: Activity,
    pub effect: Effect,
}

const EVERY_DAY: &[u32] = &[0, 1, 2, 3, 4, 5, 6];
const WEEKEND: &[u32] = &[5, 6];
const FRI_SAT_SUN: &[u32] = &[4, 5, 6];
const MON_TO_THU: &[u32] = &[0, 1, 2, 3];
const GAME_DAYS: &[u32] = &[2, 6];
const FRI_SAT: &[u32] = &[4, 5];

//                                         hunger bladder energy social hygiene fun
const AT_SHOW: [f32; 6] = [-6.0, -8.0, -3.0, 20.0, 0.0, 60.0];
const AT_GAME: [f32; 6] = [-8.0, -10.0, -4.0, 30.0, -4.0, 55.0];
const EATING: [f32; 6] = [0.0, -8.0, 0.0, 25.0, 0.0, 20.0];
const DRINKING: [f32; 6] = [0.0, -15.0, -3.0, 45.0, 0.0, 30.0];
const PAMPERED: [f32; 6] = [-4.0, -4.0, 20.0, 5.0, 40.0, 25.0];
const VISITING: [f32; 6] = [-5.0, -5.0, -3.0, 10.0, 0.0, 20.0];
const EXPLORING: [f32; 6] = [-8.0, -6.0, -10.0, 0.0, -15.0, 35.0];
const SELLING: [f32; 6] = [0.0, 0.0, 0.0, 5.0, 0.0, 0.0];

const fn a(name: &'static str, minutes: f32, cost: i64, per_hour: [f32; 6], open: f32, close: f32) -> Activity {
    Activity { name, minutes, cost, per_hour, skill: None, open, close }
}

const fn v(venue: &'static [&'static str], days: &'static [u32], activity: Activity, effect: Effect) -> VenueAct {
    VenueAct { venue, days, activity, effect }
}

const THEATRE: &[&str] = &["theater", "theatre"];
const STADIUM: &[&str] = &["stadium"];
const BISTRO: &[&str] = &["bistro"];
const DINER: &[&str] = &["diner"];
const SPA: &[&str] = &["spa_", "dayspa", "com_spa"];
const GROCERY: &[&str] = &["grocery"];
const MAUSOLEUM: &[&str] = &["mausoleum", "graveyard"];
const CITY_HALL: &[&str] = &["cityhall"];
const SCIENCE: &[&str] = &["science"];
const MILITARY: &[&str] = &["military"];

/// A great show: sixteen hours.
const SHOW_MINUTES: f64 = 960.0;

pub static ACTS: [VenueAct; 34] = [
    // The theatre (`ShowVenue`): evenings from five (doors close two hours before the end).
    v(THEATRE, MON_TO_THU, a("See a Movie", 120.0, 40, AT_SHOW, 17.0, 21.0), Effect::Moodlet(MoodletKind::SawGreatMovie, 10, SHOW_MINUTES)),
    v(THEATRE, FRI_SAT_SUN, a("See a Movie", 120.0, 40, AT_SHOW, 17.0, 21.0), Effect::Moodlet(MoodletKind::SawGreatMovie, 10, SHOW_MINUTES)),
    v(THEATRE, FRI_SAT_SUN, a("See a Play", 120.0, 40, AT_SHOW, 17.0, 21.0), Effect::Moodlet(MoodletKind::SawGreatPlay, 25, SHOW_MINUTES)),
    v(THEATRE, FRI_SAT_SUN, a("See a Symphony", 120.0, 40, AT_SHOW, 17.0, 21.0), Effect::Moodlet(MoodletKind::SawGreatSymphony, 30, SHOW_MINUTES)),
    v(THEATRE, FRI_SAT_SUN, a("See a Concert", 120.0, 40, AT_SHOW, 17.0, 21.0), Effect::Moodlet(MoodletKind::SawGreatConcert, 25, SHOW_MINUTES)),
    // (The weekend's matinee, from noon.)
    v(THEATRE, WEEKEND, a("See a Matinee", 120.0, 30, AT_SHOW, 12.0, 15.0), Effect::Moodlet(MoodletKind::SawGreatPlay, 25, SHOW_MINUTES)),
    v(THEATRE, EVERY_DAY, a("Visit the Theatre", 60.0, 0, VISITING, 8.0, 22.0), Effect::Visit([15, 5, 10])),
    // The stadium: games on Sundays and Wednesdays (doors from two), concerts Friday and Saturday.
    v(STADIUM, GAME_DAYS, a("Attend the Game", 180.0, 60, AT_GAME, 14.0, 17.5), Effect::Moodlet(MoodletKind::SawGreatGame, 20, SHOW_MINUTES)),
    v(STADIUM, FRI_SAT, a("Attend a Concert", 120.0, 40, AT_SHOW, 17.0, 21.0), Effect::Moodlet(MoodletKind::SawGreatConcert, 25, SHOW_MINUTES)),
    // The bistro (eleven to eleven) and the diner (all day and night).
    v(BISTRO, EVERY_DAY, a("Eat Here", 90.0, 50, EATING, 11.0, 23.0), Effect::Meal(25)),
    v(BISTRO, EVERY_DAY, a("Get Drinks", 30.0, 20, DRINKING, 11.0, 23.0), Effect::Drinks),
    v(DINER, EVERY_DAY, a("Eat Here", 60.0, 12, EATING, 0.0, 24.0), Effect::Meal(10)),
    v(DINER, EVERY_DAY, a("Get Drinks", 30.0, 5, DRINKING, 0.0, 24.0), Effect::Drinks),
    // The day spa: three levels of each (the game's prices, lengths and moodlets).
    v(SPA, EVERY_DAY, a("Get a Basic Massage", 60.0, 50, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::Relaxed, 25, 720.0, 1)),
    v(SPA, EVERY_DAY, a("Get a Deluxe Massage", 90.0, 250, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::Relaxed, 50, 720.0, 2)),
    v(SPA, EVERY_DAY, a("Get an Ultimate Massage", 120.0, 500, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::Relaxed, 75, 720.0, 3)),
    v(SPA, EVERY_DAY, a("Get a Basic Facial", 90.0, 50, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::SmoothSkin, 10, 2880.0, 0)),
    v(SPA, EVERY_DAY, a("Get a Deluxe Facial", 90.0, 200, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::SmoothSkin, 20, 2880.0, 0)),
    v(SPA, EVERY_DAY, a("Get an Ultimate Facial", 90.0, 500, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::SmoothSkin, 30, 2880.0, 0)),
    v(SPA, EVERY_DAY, a("Get a Basic Body Treatment", 120.0, 250, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::Rejuvenated, 30, 1440.0, 5)),
    v(SPA, EVERY_DAY, a("Get a Deluxe Body Treatment", 120.0, 750, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::Rejuvenated, 60, 1440.0, 5)),
    v(SPA, EVERY_DAY, a("Get an Ultimate Body Treatment", 120.0, 1500, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::Rejuvenated, 100, 1440.0, 5)),
    v(SPA, EVERY_DAY, a("Get a Basic Manicure", 60.0, 25, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::NiceNails, 5, 2880.0, 0)),
    v(SPA, EVERY_DAY, a("Get a Deluxe Manicure", 60.0, 50, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::NiceNails, 10, 2880.0, 0)),
    v(SPA, EVERY_DAY, a("Get an Ultimate Manicure", 60.0, 100, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::NiceNails, 15, 2880.0, 0)),
    v(SPA, EVERY_DAY, a("Have a Basic Spa Day", 360.0, 1000, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::CompletelyAtEase, 20, 10080.0, 5)),
    v(SPA, EVERY_DAY, a("Have a Deluxe Spa Day", 360.0, 3000, PAMPERED, 0.0, 24.0), Effect::Treatment(MoodletKind::CompletelyAtEase, 40, 10080.0, 5)),
    // The grocery: the produce and fish carried, sold.
    v(GROCERY, EVERY_DAY, a("Sell Produce and Fish", 15.0, 0, SELLING, 0.0, 24.0), Effect::Sell),
    // Visits (the game's `VisitRabbitHole`: Bored, by age, else something good), and the
    // catacombs.
    v(CITY_HALL, EVERY_DAY, a("Visit City Hall", 60.0, 0, VISITING, 8.0, 20.0), Effect::Visit([20, 35, 20])),
    v(SCIENCE, EVERY_DAY, a("Visit the Science Facility", 60.0, 0, VISITING, 8.0, 20.0), Effect::Visit([15, 35, 10])),
    v(MILITARY, EVERY_DAY, a("Visit the Military Base", 60.0, 0, VISITING, 8.0, 20.0), Effect::Visit([10, 20, 2])),
    v(MAUSOLEUM, EVERY_DAY, a("Visit the Mausoleum", 60.0, 0, VISITING, 0.0, 24.0), Effect::Visit([10, 5, 2])),
    v(MAUSOLEUM, EVERY_DAY, a("Explore the Catacombs", 180.0, 0, EXPLORING, 0.0, 24.0), Effect::Explore),
    // The grocery's shop (`Grocery.ShopByRecipe`), from seven till midnight.
    v(GROCERY, EVERY_DAY, a("Shop", 30.0, 0, SELLING, 7.0, 24.0), Effect::Shop),
];

/// The grocery's "Shop" (its visit's activity, for `shop` to know a shopper by).
pub fn shop_activity() -> Option<&'static Activity> {
    ACTS.iter().find(|a| a.effect == Effect::Shop).map(|a| &a.activity)
}

/// The show on at the theatre on a day (the game's: Friday to Sunday, 15 in a hundred each a
/// concert, a play, a symphony, the rest a movie; other nights a movie): the index of its act.
pub fn tonights_show(day: u32) -> usize {
    let weekday = day % 7;
    if !FRI_SAT_SUN.contains(&weekday) {
        return 0;
    }
    // (One draw a day, the same all day.)
    let roll = (day.wrapping_mul(2_654_435_761) >> 7) % 100;
    match roll {
        0..15 => 4,
        15..30 => 2,
        30..45 => 3,
        _ => 1,
    }
}

/// A meal's price at the bistro: dinner from five dearer, a fifth off before six.
pub fn meal_price(i: usize, hour: f32) -> i64 {
    let act = &ACTS[i].activity;
    match act.cost {
        50 if hour >= 17.0 => {
            if hour < 18.0 { 60 } else { 75 }
        }
        12 if hour >= 17.0 => 18,
        c => c,
    }
}

/// The venue activity an activity number stands for.
pub fn task(activity: usize) -> Option<&'static Activity> {
    activity.checked_sub(VENUE_TASK).and_then(|i| ACTS.get(i)).map(|v| &v.activity)
}

/// A lot's name, to tell its kind of venue by (an amphitheatre's not the theatre).
fn venue_name(lot: &s3formats::world::LotInfo) -> String {
    lot.internal_name.to_ascii_lowercase().replace("amphitheater", "amphi")
}

/// Whether a lot is a venue with its own menu here (and so none of the plain activities).
pub fn is_venue(lot: &s3formats::world::LotInfo) -> bool {
    let n = venue_name(lot);
    ACTS.iter().any(|a| a.venue.iter().any(|v| n.contains(v)))
}

/// What a venue offers a Sim now: its activities open today and at this hour (the theatre's
/// show of the day), labelled with their prices, and their activity numbers.
pub fn lot_options(lot: &s3formats::world::LotInfo, day: u32, hour: f32, age: Age) -> Vec<(String, usize)> {
    options_for(&venue_name(lot), day, hour, age)
}

/// The venues' rabbit holes by the game's class for them (`opportunities::lot_types`), for a lot
/// of several not named for any (Sunset Valley's plaza, with its grocery).
const BY_TYPE: [(&str, &[&str]); 6] = [("Grocery", GROCERY), ("Theatre", THEATRE), ("Stadium", STADIUM), ("CityHall", CITY_HALL), ("ScienceLab", SCIENCE), ("MilitaryBase", MILITARY)];

/// What a town's lot offers of the venues': by its name, or by the rabbit holes on it.
pub fn world_lot_options(world: &crate::loading::WorldInfo, lot: usize, day: u32, hour: f32, age: Age) -> Vec<(String, usize)> {
    let Some(l) = world.lots.get(lot) else { return Vec::new() };
    if is_venue(l) {
        return lot_options(l, day, hour, age);
    }
    let types = crate::opportunities::lot_types(world, lot);
    let words: Vec<&str> = BY_TYPE.iter().filter(|(t, _)| types.contains(t)).flat_map(|(_, w)| w.iter().copied()).collect();
    if words.is_empty() { Vec::new() } else { options_for(&words.join(" "), day, hour, age) }
}

/// The acts of the venues named in `n` open now.
fn options_for(n: &str, day: u32, hour: f32, age: Age) -> Vec<(String, usize)> {
    if age.is_little() {
        return Vec::new();
    }
    let weekday = day % 7;
    let show = tonights_show(day);
    ACTS.iter()
        .enumerate()
        .filter(|(_, a)| a.venue.iter().any(|v| n.contains(v)) && a.days.contains(&weekday) && (a.activity.open..a.activity.close).contains(&hour))
        // (Of the theatre's evening shows, the one on tonight.)
        .filter(|(i, _)| !(1..=4).contains(i) || *i == show)
        .map(|(i, a)| {
            let price = meal_price(i, hour);
            let label = if price > 0 { format!("{} (§{price})", a.activity.name) } else { a.activity.name.to_string() };
            (label, VENUE_TASK + i)
        })
        .collect()
}

/// Back from a venue's activity.
#[derive(Component)]
pub struct VenueDone(pub usize);

/// Out with someone (the game's "Eat Here With...", "Attend Show With..."): who.
#[derive(Component, Clone, Copy)]
pub struct OutingWith(pub Entity);

/// The ending of an outing's menu's title ("Eat Here With").
pub const WITH: &str = " With";
/// The liking an outing together's worth (the game's companions' 5 to 15).
const OUTING_LIKING: std::ops::RangeInclusive<f32> = 5.0..=15.0;

/// Whether an activity's one to go to with someone (meals, drinks, shows, games, visits).
pub fn groupable(task: usize) -> bool {
    task.checked_sub(VENUE_TASK).and_then(|i| ACTS.get(i)).is_some_and(|a| matches!(a.effect, Effect::Meal(_) | Effect::Drinks | Effect::Moodlet(..) | Effect::Visit(_) | Effect::Explore))
}

/// The name an outing's activity goes by (its option's, without the price).
pub fn outing_name(task: usize) -> Option<&'static str> {
    task.checked_sub(VENUE_TASK).and_then(|i| ACTS.get(i)).map(|a| a.activity.name)
}

/// The venue's act a rabbit hole activity is.
pub fn act_of(a: &Activity) -> Option<usize> {
    ACTS.iter().position(|v| std::ptr::eq(&v.activity, a))
}

/// The good moodlets a visit may leave (the game's `VisitRabbitHole` buffs).
const VISIT_GOOD: [(MoodletKind, i32); 5] = [
    (MoodletKind::HavingFun, 15),
    (MoodletKind::Fascinated, 10),
    (MoodletKind::Intrigued, 10),
    (MoodletKind::Impressed, 10),
    (MoodletKind::Educated, 10),
];

/// Coming out of a venue: what it did.
#[allow(clippy::type_complexity)]
fn venue_done(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &crate::sim::Sim, &VenueDone, &mut crate::life::Moodlets, &mut crate::sim::Motives, Option<&mut crate::inventory::Inventory>, Option<&OutingWith>)>,
    mut household: Option<ResMut<crate::interact::Household>>,
    mut notes: ResMut<crate::interact::Notifications>,
    (mut garden, ui): (ResMut<crate::gardening::Garden>, Option<Res<crate::icons::GameUi>>),
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, sim, done, mut m, mut motives, inv, with) in &mut sims {
        commands.entity(e).remove::<(VenueDone, OutingWith)>();
        // (Out together: closer for it, both ways.)
        if let Some(&OutingWith(other)) = with {
            let liking = rng.random_range(OUTING_LIKING);
            commands.queue(move |w: &mut World| {
                for (a, b) in [(e, other), (other, e)] {
                    if let Some(mut r) = w.get_mut::<crate::social::Relationships>(a) {
                        r.add(b, liking / 2.0, 0.0);
                    }
                }
            });
            motives.add(crate::sim::SOCIAL, 30.0);
        }
        let Some(act) = ACTS.get(done.0) else { continue };
        debug!("venue: {} back from {}: {:?}", sim.first, act.activity.name, act.effect);
        match act.effect {
            Effect::Moodlet(k, value, minutes) => m.add_valued(k, now, value, minutes),
            Effect::Treatment(k, value, minutes, cures) => {
                m.add_valued(k, now, value, minutes);
                // (The worst of the bad moodlets eased away.)
                let mut bad: Vec<(MoodletKind, i32)> = m.0.iter().filter(|x| x.value < 0 && x.until.is_finite()).map(|x| (x.kind, x.value)).collect();
                bad.sort_by_key(|b| b.1);
                for (k, _) in bad.into_iter().take(cures) {
                    m.remove(k);
                }
            }
            Effect::Meal(value) => {
                motives.0[crate::sim::HUNGER] = 100.0;
                m.add_valued(MoodletKind::GreatMeal, now, value, 480.0);
            }
            Effect::Drinks => motives.0[crate::sim::HUNGER] = 100.0,
            Effect::Visit(bored) => {
                let chance = match sim.age {
                    Age::Child => bored[0],
                    Age::Teen => bored[1],
                    _ => bored[2],
                };
                if rng.random_range(0..100) < chance as u32 {
                    // (Bored: the game's buff is the low-fun one; it's the fun lost here.)
                    motives.add(crate::sim::FUN, -25.0);
                    notes.push(format!("{} found {} boring.", sim.first, act.activity.name.trim_start_matches("Visit ")));
                } else {
                    let (k, value) = VISIT_GOOD[rng.random_range(0..VISIT_GOOD.len())];
                    m.add_valued(k, now, value, 180.0);
                }
            }
            Effect::Explore => {
                // (The game's: a find at each turn, rocks and metal, seeds or a book, else a
                // rubber duck; one in four a fright.)
                let finds = rng.random_range(1..=3);
                let mut found = Vec::new();
                for _ in 0..finds {
                    let r = rng.random_range(0..100);
                    // (A seed of any rarity, the game's `Mausoleum` find: kept, to plant.)
                    if (35..70).contains(&r)
                        && let Some(ui) = ui.as_ref()
                        && let Some(seed) = crate::gardening::find_seed(&mut garden, &ui.data, crate::gardening::RARITIES[rng.random_range(0..4)], &mut rng)
                    {
                        found.push(seed);
                        continue;
                    }
                    let (what, worth) = match r {
                        0..35 => ("a gem", rng.random_range(15..60)),
                        70..90 => ("an old book", rng.random_range(10..40)),
                        _ => ("a rubber duck", 2),
                    };
                    found.push(format!("{what} (sold for §{worth})"));
                    if let Some(h) = household.as_deref_mut() {
                        h.funds += worth;
                    }
                }
                notes.push(format!("{} explored the catacombs and found {}.", sim.first, found.join(", ")));
                if rng.random_bool(0.25) {
                    m.add(MoodletKind::Horrified, now);
                    notes.push(format!("Something in the catacombs horrified {}!", sim.first));
                }
            }
            Effect::Shop => {}
            Effect::Sell => {
                let Some(mut inv) = inv else {
                    notes.push(format!("{} had nothing to sell.", sim.first));
                    continue;
                };
                let worth: i64 = inv.0.iter().filter(|s| matches!(s.kind, crate::inventory::ItemKind::Produce | crate::inventory::ItemKind::Fish)).map(|s| s.worth).sum();
                inv.0.retain(|s| !matches!(s.kind, crate::inventory::ItemKind::Produce | crate::inventory::ItemKind::Fish));
                let paid = (worth as f64 * 1.1).round() as i64;
                if let Some(h) = household.as_deref_mut() {
                    h.funds += paid;
                }
                notes.push(if paid > 0 { format!("{} sold their produce and fish for §{paid}.", sim.first) } else { format!("{} had nothing to sell.", sim.first) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venues_open_as_the_games_do() {
        let lot = |n: &str| s3formats::world::LotInfo { id: 0, internal_name: n.into(), corner: [0.0; 3], rotation: 0.0, width: 20, depth: 20, string_keys: Vec::new() };
        let adult = Age::Adult;
        let theatre = lot("Com_Theater_36x20");
        // (A Monday evening: the movie, and the visit; Monday morning only the visit.)
        let monday_night: Vec<String> = lot_options(&theatre, 0, 19.0, adult).into_iter().map(|o| o.0).collect();
        assert_eq!(monday_night, ["See a Movie (§40)", "Visit the Theatre"]);
        assert_eq!(lot_options(&theatre, 0, 10.0, adult).len(), 1);
        // (The stadium's game: a Wednesday afternoon, not a Tuesday.)
        let stadium = lot("Com_Stadium_48x64");
        assert!(lot_options(&stadium, 2, 15.0, adult).iter().any(|o| o.0.starts_with("Attend the Game (§60)")));
        assert!(lot_options(&stadium, 1, 15.0, adult).is_empty());
        // (The bistro: lunch §50, dinner §75, a fifth off at five.)
        let bistro = lot("Com_Bistro_18x22");
        assert!(lot_options(&bistro, 0, 12.0, adult).iter().any(|o| o.0 == "Eat Here (§50)"));
        assert!(lot_options(&bistro, 0, 17.5, adult).iter().any(|o| o.0 == "Eat Here (§60)"));
        assert!(lot_options(&bistro, 0, 20.0, adult).iter().any(|o| o.0 == "Eat Here (§75)"));
        assert!(lot_options(&bistro, 0, 8.0, adult).is_empty());
        // (The spa's fifteen; nothing for a toddler.)
        assert_eq!(lot_options(&lot("Com_Spa_20x36"), 3, 10.0, adult).len(), 14);
        assert!(lot_options(&lot("Com_Spa_20x36"), 3, 10.0, Age::Toddler).is_empty());
        assert!(is_venue(&lot("Com_CityHall_34x64")) && !is_venue(&lot("Com_Library_20x30")) && !is_venue(&lot("Com_Amphitheater_60x60")));
        // (The weekend's show is one of the four, and the same all day.)
        assert_eq!(tonights_show(4), tonights_show(4));
        assert!((1..=4).contains(&tonights_show(5)));
    }
}
