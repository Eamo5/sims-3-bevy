//! Late Night's celebrities, as the game's `CelebrityManager` keeps them (its `Celebrity` table
//! and `CelebrityManager` tuning):
//!
//! - Each Sim's celebrity level, 0 to 5 (Fame Leecher, Notable Figure, B-List Celebrity, Renowned
//!   VIP, Superstar), and the points towards the next: 100 to leave level 0, then 1,500, 2,000,
//!   4,000, 6,000 and 10,000 (`Levels`' `Points`), the rest carried over. The world's own
//!   celebrities are as its `CharacterImport` table has them (Bridgeport's twenty), kept with the
//!   town's story.
//! - Points come from getting closer to a celebrity (`RelationshipBenefits`: a Friend 1,000, a Good
//!   Friend 500, a Best Friend 1,000, a Romantic Interest 1,100, a Partner 1,750, a Fiancée 2,000, a
//!   Spouse 3,000), awarded as the relationship reaches each, if it's never been closer, and scaled
//!   by how they stand (`kCelebrityPointsMultiplierForLtrUpdates`: more the more famous the
//!   celebrity is than they are, none at all from someone five levels below); from an actor's
//!   Take Headshots at work (a point a minute); and a tenth more for a Film Sim from level 4
//!   (`kFilmSimCelebrityPointMultiplier`). A level reached is told with the game's own words.
//! - A celebrity is recognized where they pay at a rabbit hole: let in free now and then
//!   (`FreebieChance`, from level 2) or given a discount (`DiscountChance`: half to seven tenths
//!   of the price, `kMinMaxCelebrityDiscount`), and Got Recognized for it.
//! - Free stuff from the makers (`FreeStuff`): every 96 hours at level 1 to every 48 at level 5
//!   (`kFreeStuffAlarmFrequencyHours`), as likely as not and likelier the more famous
//!   (`kFreeStuffChance`), an object for their level put in the family inventory.
//! - The town's celebrities rise and fall with story progression (`CelebrityLevelModification`):
//!   a Sim at level 7 or more of their career, or of a household worth §100,000, half again as
//!   likely to rise and a tenth as likely to fall.
//! - The Simology panel's Celebrity button opens the game's Celebrity Journal: the level's stars,
//!   the progress to the next, and the celebrity's statistics. Film's `CelebrityLevel` metric
//!   counts it.

use std::collections::BTreeMap;

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::Notifications;
use crate::layout::{Spawned, UiAssets};
use crate::sim::{Age, HouseholdMember, Sim};

pub struct CelebrityPlugin;

impl Plugin for CelebrityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Journal>()
            .add_systems(Update, (seed_fame, mark_famous, friends_of_the_famous, free_stuff, town_fame, fame_test, journal).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(crate::AppState::InGame), |mut j: ResMut<Journal>| *j = Journal::default());
    }
}

/// The highest level (`CelebrityManager.HighestLevel`).
pub const HIGHEST: u8 = 5;
/// The points that take a Sim from each level to the next (`Levels`' `Points`).
const GOAL: [f32; 6] = [100.0, 1500.0, 2000.0, 4000.0, 6000.0, 10000.0];
/// Each level's chance of going free, and of a discount (`FreebieChance`, `DiscountChance`).
const FREEBIE: [f32; 6] = [0.0, 0.0, 0.1, 0.15, 0.2, 0.25];
const DISCOUNT: [f32; 6] = [0.0, 0.2, 0.35, 0.5, 0.6, 0.7];
/// What's paid of the price with a discount (`kMinMaxCelebrityDiscount`).
const DISCOUNT_RANGE: (f32, f32) = (0.5, 0.7);
/// The points for each relationship with a celebrity (`RelationshipBenefits`).
const LTR_POINTS: [(&str, f32); 8] = [
    ("Friend", 1000.0),
    ("Good Friend", 500.0),
    ("Best Friend", 1000.0),
    ("Best Friends Forever", 1500.0),
    ("Romantic Interest", 1100.0),
    ("Partner", 1750.0),
    ("Fiancee", 2000.0),
    ("Spouse", 3000.0),
];
/// The relationships' states, as `conversation::relationship_state` names them (by their place
/// here: what a Sim's been to a celebrity).
const STATES: [&str; 12] = ["Acquaintance", "Disliked", "Friend", "Good Friend", "Best Friend", "Romantic Interest", "Partner", "Fiancee", "Spouse", "Enemy", "Ex", "Best Friends Forever"];
/// The points scaled by how the two stand: at `HIGHEST + mine - theirs`
/// (`kCelebrityPointsMultiplierForLtrUpdates`).
const LTR_SCALE: [f32; 11] = [2.25, 1.9, 1.6, 1.3, 1.0, 0.8, 0.5, 0.25, 0.1, 0.05, 0.0];
/// A Film Sim's points from this level of the career, and how many more
/// (`kFilmSimMultiplierStartingLevel`, `kFilmSimCelebrityPointMultiplier`).
const FILM_LEVEL: usize = 4;
const FILM_MULTIPLIER: f32 = 1.1;
/// Points a minute of Take Headshots (`Film.TakeHeadShotsTone.kCelebrityPointsPerSimMinute`).
pub const HEADSHOT_POINTS: f32 = 1.0;
/// Hours between gifts at each level from 1, and their chances (`kFreeStuffAlarmFrequencyHours`,
/// `kFreeStuffChance`).
const GIFT_HOURS: [f64; 5] = [96.0, 84.0, 72.0, 60.0, 48.0];
const GIFT_CHANCE: [f32; 5] = [0.65, 0.7, 0.75, 0.8, 0.85];
/// What the makers send at each level, and how likely each is (`FreeStuff`: objects the
/// catalogue hasn't, the cars, aren't sent).
const FREE_STUFF: &[(u8, &str, u32)] = &[
    (1, "SculptureWallMountedFish", 1),
    (1, "sculptureRoofWeatherVaneRoosterCF", 1),
    (1, "PaintingAnimalGiraffe", 1),
    (1, "FireplaceToolRack", 1),
    (1, "ClutterTablePaperTowelRoll", 1),
    (1, "SculpturePlantPerennials", 1),
    (2, "RugTraditionalSquare4x4", 1),
    (2, "DishwasherStainless", 1),
    (2, "TelescopeCheap", 1),
    (2, "DresserMediumColonial", 1),
    (3, "BarbequeGrillGrand", 1),
    (3, "LoveseatExpensiveCamelBack", 1),
    (3, "StoveModern", 1),
    (3, "RefrigeratorModern", 1),
    (3, "BookshelfTraditionalHigh3x1", 1),
    (4, "RefrigeratorModern", 1),
    (4, "BookshelfTraditionalHigh3x1", 1),
    (4, "BathtubModern", 1),
    (4, "ToiletExpensive", 1),
    (4, "fountainLoungeLit2x1", 1),
    (4, "BedDoubleColonial", 1),
    (4, "BedDoubleContemporary", 1),
    (4, "ComputerLaptop", 1),
    (4, "hotTubSmall", 1),
    (4, "TelevisionWall1x2", 1),
    (5, "CarSports", 1),
    (5, "CarExpensive1", 1),
    (5, "CarExpensive2", 1),
    (5, "FoosballTable", 5),
    (5, "magicGnomeCelebrity", 1),
    (5, "fishTankWallDigital5x1", 2),
    (5, "VRGoggles", 2),
    (5, "hotTubGrotto", 3),
    (5, "PianoGrand", 5),
    (5, "HotTubGrottoCeleb", 4),
    (5, "carCelebGift", 2),
    (5, "guitarCelebGift", 4),
    (5, "bedDoubleCeleb", 4),
    (5, "tvWallCeleb3x1", 4),
];
/// Story progression's chances a day of one of the town's Sims rising a level, and of one of its
/// celebrities falling one; and what makes a Sim likelier to rise and less to fall
/// (`CelebrityLevelModification`'s `kCareerLevelThreshold`, `kHouseholdWorthThreshold` and their
/// multipliers).
const TOWN_RISE: f32 = 0.35;
const TOWN_FALL: f32 = 0.25;
const CAREER_THRESHOLD: u32 = 7;
const WORTH_THRESHOLD: i64 = 100_000;
const RISE_MORE: f32 = 1.5;
const FALL_LESS: f32 = 0.1;
/// The level names, where the game's words aren't to be had.
const NAMES: [&str; 6] = ["", "Fame Leecher", "Notable Figure", "B-List Celebrity", "Renowned VIP", "Superstar"];

/// A Sim's fame, as their `CelebrityManager` keeps it.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct Fame {
    pub level: u8,
    #[serde(default)]
    pub points: f32,
    /// What they've been to each celebrity they know (by id: the state's place in `STATES`, and
    /// the most they've liked them), for the points as they get closer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub known: BTreeMap<u64, (u8, f32)>,
    /// When the makers next think of them (game minutes; none yet).
    #[serde(default)]
    pub next_gift: f64,
    /// Their statistics: discounts, freebies (and gifts), what's been saved.
    #[serde(default)]
    pub discounts: u32,
    #[serde(default)]
    pub freebies: u32,
    #[serde(default)]
    pub saved: i64,
}

impl Fame {
    /// Points gained (a Film Sim's a tenth more from level 4): the levels reached for them.
    pub fn gain(&mut self, points: f32) -> Vec<u8> {
        let mut reached = Vec::new();
        self.points += points.max(0.0);
        while self.level < HIGHEST && self.points >= goal(self.level) {
            self.points -= goal(self.level);
            self.level += 1;
            reached.push(self.level);
        }
        if self.level >= HIGHEST {
            self.points = self.points.min(goal(HIGHEST));
        }
        reached
    }

    /// How far to the next level (0 to 1; full at the top).
    pub fn progress(&self) -> f32 {
        if self.level >= HIGHEST { 1.0 } else { (self.points / goal(self.level)).clamp(0.0, 1.0) }
    }
}

/// The points that take a Sim from a level to the next.
pub fn goal(level: u8) -> f32 {
    GOAL[level.min(HIGHEST) as usize]
}

/// A Sim on show who's famous: their level (kept in step with the town's story).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Famous(pub u8);

/// What a relationship reaching a state is worth to a Sim of level `mine` with a celebrity of
/// level `theirs` (none from a state that's worth nothing, or one far beneath them).
pub fn relationship_points(state: &str, mine: u8, theirs: u8) -> f32 {
    let Some(base) = LTR_POINTS.iter().find(|p| p.0 == state).map(|p| p.1) else { return 0.0 };
    let i = (HIGHEST as i32 + mine as i32 - theirs as i32).max(0) as usize;
    LTR_SCALE.get(i).map_or(0.0, |s| base * s)
}

/// What's paid of a price by a celebrity (`GetCelebrityDiscount`): nothing now and then, a
/// discount more often, the more famous the likelier.
pub fn discount(level: u8, rng: &mut impl Rng) -> f32 {
    let l = level.min(HIGHEST) as usize;
    if l == 0 {
        return 1.0;
    }
    if rng.random::<f32>() < FREEBIE[l] {
        0.0
    } else if rng.random::<f32>() < DISCOUNT[l] {
        rng.random_range(DISCOUNT_RANGE.0..DISCOUNT_RANGE.1)
    } else {
        1.0
    }
}

/// Whether a Sim can be famous (`CanBeCelebrity`: a child or older).
pub fn can_be_famous(sim: &Sim) -> bool {
    !matches!(sim.age, Age::Baby | Age::Toddler)
}

/// A level's name (the game's: "Fame Leecher" to "Superstar").
pub fn level_name(ui: Option<&UiAssets>, level: u8) -> String {
    let l = level.min(HIGHEST);
    ui.and_then(|u| u.localize(&format!("Gameplay/Excel/Celebrity/Levels:Level{l}"))).filter(|s| !s.is_empty()).unwrap_or_else(|| NAMES[l as usize].to_string())
}

/// The game's words for a level reached (`LevelUpTns`).
fn level_up_words(ui: Option<&UiAssets>, sim: &Sim, level: u8) -> String {
    let name = level_name(ui, level);
    ui.and_then(|u| u.localize(&format!("Gameplay/Excel/Celebrity/Levels:Level{level}Tns")))
        .filter(|s| !s.is_empty())
        .map_or_else(
            || format!("{} is now a {name}.", sim.full_name()),
            |t| crate::layout::gendered(&t, sim.female).replace("{0.String}", &name).replace("{1.SimName}", &sim.full_name()).replace("{1.SimFirstName}", &sim.first),
        )
}

/// Points for a Sim (its levels told, if they're one of the household's).
fn add_points(fame: &mut Fame, sim: &Sim, job: Option<&crate::careers::Job>, points: f32, member: bool, ui: Option<&UiAssets>, notes: &mut Notifications) {
    if !can_be_famous(sim) || points <= 0.0 {
        return;
    }
    let film = job.is_some_and(film_bonus);
    for level in fame.gain(points * if film { FILM_MULTIPLIER } else { 1.0 }) {
        info!("{} reached celebrity level {level}", sim.first);
        if member {
            notes.push(level_up_words(ui, sim, level));
        }
    }
}

/// Celebrity points for a Sim (queued: from a tone at work, `careers`).
pub fn award(w: &mut World, e: Entity, points: f32) {
    let Some(sim) = w.get::<Sim>(e).cloned() else { return };
    if !can_be_famous(&sim) || points <= 0.0 {
        return;
    }
    let film = w.get::<crate::careers::Job>(e).is_some_and(film_bonus);
    let member = w.get::<HouseholdMember>(e).is_some();
    let Some(mut story) = w.get_resource_mut::<crate::story::TownStory>() else { return };
    let reached = story.fame.entry(sim.id).or_default().gain(points * if film { FILM_MULTIPLIER } else { 1.0 });
    if reached.is_empty() {
        return;
    }
    info!("{} reached celebrity level {reached:?}", sim.first);
    if member {
        let words: Vec<String> = reached.iter().map(|l| level_up_words(w.get_resource::<UiAssets>(), &sim, *l)).collect();
        let mut notes = w.resource_mut::<Notifications>();
        for t in words {
            notes.push(t);
        }
    }
}

/// Whether a Sim's points come a tenth more (a Film Sim from level 4).
fn film_bonus(j: &crate::careers::Job) -> bool {
    j.career().key == "Film" && j.level + 1 >= FILM_LEVEL
}

/// A celebrity recognized where they'd pay (`ShowPositiveReactionToDiscount`): the game's words,
/// Got Recognized, and the counts.
pub fn recognized(w: &mut World, e: Entity, cost: i64, paid: i64) {
    let Some(sim) = w.get::<Sim>(e).cloned() else { return };
    let now = w.resource::<GameClock>().minutes;
    if let Some(mut story) = w.get_resource_mut::<crate::story::TownStory>() {
        let f = story.fame.entry(sim.id).or_default();
        if paid == 0 {
            f.freebies += 1;
        } else {
            f.discounts += 1;
        }
        f.saved += cost - paid;
    }
    let (key, or) = if paid == 0 {
        ("Gameplay/Excel/Notifications/Notifications:CelebrityFreebieTNS", format!("Wow, it's {}! No, you don't have to pay. This is on us.", sim.full_name()))
    } else {
        ("Gameplay/Excel/Notifications/Notifications:CelebrityDiscountTNS", format!("No way! {}?! Normally it costs §{cost}, but you'll only have to pay §{paid}.", sim.full_name()))
    };
    let words = w.get_resource::<UiAssets>().and_then(|u| u.localize(key)).map_or(or, |t| {
        crate::layout::gendered(&t, sim.female).replace("{0.SimName}", &sim.full_name()).replace("{1.Money}", &format!("§{cost}")).replace("{2.Money}", &format!("§{paid}"))
    });
    w.resource_mut::<Notifications>().push(words);
    if let Some(mut m) = w.get_mut::<crate::life::Moodlets>(e) {
        m.add(crate::life::MoodletKind::GotRecognized, now);
    }
}

/// The world's celebrities, as its `CharacterImport` has them, once a game.
fn seed_fame(mut story: ResMut<crate::story::TownStory>, town: Option<Res<crate::premade::TownPremades>>) {
    if story.fame_seeded {
        return;
    }
    let Some(town) = town else { return };
    story.fame_seeded = true;
    let mut n = 0;
    for m in town.0.households.iter().flat_map(|h| h.members.iter()).filter(|m| m.celebrity > 0) {
        story.fame.entry(m.id).or_insert_with(|| Fame { level: m.celebrity.min(HIGHEST), ..default() });
        n += 1;
    }
    if n > 0 {
        info!("celebrities: {n} in town");
    }
}

/// Each Sim on show marked as famous as they are (and their careers' metric kept in step).
fn mark_famous(
    mut commands: Commands,
    story: Res<crate::story::TownStory>,
    sims: Query<(Entity, &Sim, Option<&Famous>)>,
    mut jobs: Query<&mut crate::careers::Job>,
) {
    for (e, sim, famous) in &sims {
        let level = story.fame.get(&sim.id).map_or(0, |f| f.level);
        match (famous, level) {
            (Some(f), l) if f.0 == l => {}
            (None, 0) => {}
            (Some(_), 0) => {
                commands.entity(e).remove::<Famous>();
            }
            _ => {
                commands.entity(e).insert(Famous(level));
            }
        }
        if let Ok(mut j) = jobs.get_mut(e)
            && j.tally.celebrity != level
        {
            j.tally.celebrity = level;
        }
    }
}

/// Points as the household gets closer to celebrities (`LongTermRelationship.UpdateCelebrityPoints`):
/// a relationship reaching a state worth something, never closer before.
#[allow(clippy::type_complexity)]
fn friends_of_the_famous(
    mut story: ResMut<crate::story::TownStory>,
    members: Query<(&Sim, &crate::social::Relationships, Option<&crate::careers::Job>), With<HouseholdMember>>,
    famous: Query<(&Sim, &Famous)>,
    ui: Option<Res<UiAssets>>,
    mut notes: ResMut<Notifications>,
) {
    for (sim, rels, job) in &members {
        if !can_be_famous(sim) {
            continue;
        }
        for (other, rel) in &rels.0 {
            let Ok((celeb, f)) = famous.get(*other) else { continue };
            if celeb.id == sim.id || f.0 == 0 {
                continue;
            }
            let state = rel.state();
            let place = STATES.iter().position(|s| *s == state).unwrap_or(0) as u8;
            let was = story.fame.get(&sim.id).and_then(|m| m.known.get(&celeb.id)).copied();
            if was.is_some_and(|(p, _)| p == place) {
                continue;
            }
            let me = story.bypass_change_detection().fame.entry(sim.id).or_default();
            let mine = me.level;
            // (What they were when first seen together is where it starts from.)
            let Some((_, best)) = was else {
                me.known.insert(celeb.id, (place, rel.friendship));
                continue;
            };
            me.known.insert(celeb.id, (place, best.max(rel.friendship)));
            if rel.friendship < best {
                continue;
            }
            let points = relationship_points(state, mine, f.0);
            if points > 0.0 {
                info!("{} is now {state} with {} (celebrity level {}): {points} celebrity points", sim.first, celeb.first, f.0);
                let me = story.fame.entry(sim.id).or_default();
                add_points(me, sim, job, points, true, ui.as_deref(), &mut notes);
            }
        }
    }
}

/// Free stuff from the makers for the household's celebrities (`GiveFreeStuff`).
fn free_stuff(
    clock: Res<GameClock>,
    mut story: ResMut<crate::story::TownStory>,
    members: Query<&Sim, With<HouseholdMember>>,
    baked: Option<Res<crate::baked::Baked>>,
    ui: Option<Res<UiAssets>>,
    mut family: ResMut<crate::familyinv::FamilyInventory>,
    mut notes: ResMut<Notifications>,
) {
    let Some(baked) = baked else { return };
    let now = clock.minutes;
    for sim in &members {
        let Some(level) = story.fame.get(&sim.id).map(|f| f.level).filter(|l| *l > 0) else { continue };
        let every = GIFT_HOURS[level as usize - 1] * 60.0;
        let due = story.fame.get(&sim.id).map_or(0.0, |f| f.next_gift);
        if due > 0.0 && now < due {
            continue;
        }
        let f = story.fame.entry(sim.id).or_default();
        f.next_gift = if due <= 0.0 { now + every } else { due.max(now - every) + every };
        if due <= 0.0 {
            continue;
        }
        let mut rng = rand::rng();
        if rng.random::<f32>() >= GIFT_CHANCE[level as usize - 1] {
            continue;
        }
        let choices: Vec<(&s3bake::CatalogEntry, u32)> = FREE_STUFF
            .iter()
            .filter(|(l, ..)| *l == level)
            .filter_map(|(_, name, weight)| baked.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(name)).map(|c| (c, *weight)))
            .collect();
        let total: u32 = choices.iter().map(|c| c.1).sum();
        if total == 0 {
            continue;
        }
        let mut roll = rng.random_range(0..total);
        let Some((c, _)) = choices.iter().find(|(_, w)| {
            let hit = roll < *w;
            roll = roll.saturating_sub(*w);
            hit
        }) else {
            continue;
        };
        family.0.push(crate::familyinv::stored(c.objd, None, None, 0));
        f.freebies += 1;
        info!("celebrity gift: {} was sent {} ({})", sim.first, c.name, c.instance_name);
        let placed = ui
            .as_deref()
            .and_then(|u| u.localize("Gameplay/CelebritySystem/CelebrityManager:FreeStuffPlacedInFamilyInventory"))
            .map_or_else(|| format!("It has been placed in {}'s shared family inventory.", sim.first), |t| crate::layout::gendered(&t, sim.female).replace("{0.SimFirstName}", &sim.first));
        let words = ui.as_deref().and_then(|u| u.localize("Gameplay/Excel/Notifications/Notifications:CelebrityFreeStuffReceived")).map_or_else(
            || format!("{} has been sent a {} from the manufacturer just for being a celebrity! {placed}", sim.first, c.name),
            |t| crate::layout::gendered(&t, sim.female).replace("{0.SimFirstName}", &sim.first).replace("{1.ObjectName}", &c.name).replace("{2.String}", &placed),
        );
        notes.push(words);
    }
}

/// The town's celebrities rising and falling with story progression, a day at a time.
#[allow(clippy::type_complexity)]
fn town_fame(
    clock: Res<GameClock>,
    settings: Res<crate::options::Settings>,
    mut story: ResMut<crate::story::TownStory>,
    town: Option<Res<crate::premade::TownPremades>>,
    members: Query<&Sim, With<HouseholdMember>>,
    dormant: Res<crate::household::Dormant>,
) {
    let Some(town) = town else { return };
    let today = clock.day();
    if !story.fame_seeded || story.fame_day >= today || clock.hour_f() < 6.0 || members.is_empty() {
        return;
    }
    let first = story.fame_day == 0;
    story.fame_day = today;
    if first || !settings.story_progression {
        return;
    }
    let playing: Vec<u64> = members.iter().map(|s| s.id).chain(dormant.member_ids()).collect();
    // (Who might: the town's own, not the household's, nor service Sims, nor the dead or gone;
    // a child or older; with their career's level and their household's worth.)
    let mut people: Vec<(u64, u32, i64)> = Vec::new();
    for h in &town.0.households {
        for m in &h.members {
            if playing.contains(&m.id) || m.first_name.is_empty() {
                continue;
            }
            let t = story.sims.get(&m.id);
            if t.is_some_and(|t| t.dead || t.gone) {
                continue;
            }
            let age = t.and_then(|t| t.age.as_deref()).map_or_else(|| crate::premade::age_of(m.age), crate::save::age_from_name);
            if matches!(age, Age::Baby | Age::Toddler) {
                continue;
            }
            let level = t.and_then(|t| t.job.as_ref()).map(|j| j.1).or_else(|| m.career.as_ref().map(|c| c.1 as u32)).unwrap_or(0);
            people.push((m.id, level, h.funds));
        }
    }
    let mut rng = rand::rng();
    let level_of = |story: &crate::story::TownStory, id: u64| story.fame.get(&id).map_or(0, |f| f.level);
    let rising: Vec<&(u64, u32, i64)> = people.iter().filter(|p| level_of(&story, p.0) < HIGHEST).collect();
    if let Some(&&(id, career, worth)) = rising.get(rng.random_range(0..rising.len().max(1))) {
        let mut chance = TOWN_RISE;
        if career >= CAREER_THRESHOLD {
            chance *= RISE_MORE;
        }
        if worth >= WORTH_THRESHOLD {
            chance *= RISE_MORE;
        }
        if rng.random::<f32>() < chance {
            let f = story.fame.entry(id).or_default();
            f.level += 1;
            f.points = 0.0;
            info!("town celebrity: {id} up to level {}", f.level);
        }
    }
    let falling: Vec<&(u64, u32, i64)> = people.iter().filter(|p| level_of(&story, p.0) > 0).collect();
    if let Some(&&(id, career, worth)) = falling.get(rng.random_range(0..falling.len().max(1))) {
        let mut chance = TOWN_FALL;
        if career >= CAREER_THRESHOLD {
            chance *= FALL_LESS;
        }
        if worth >= WORTH_THRESHOLD {
            chance *= FALL_LESS;
        }
        if rng.random::<f32>() < chance
            && let Some(f) = story.fame.get_mut(&id)
        {
            f.level -= 1;
            f.points = 0.0;
            info!("town celebrity: {id} down to level {}", f.level);
        }
    }
}

/// FAME=<level>[:<points>] (tests): the selected Sim that famous five seconds in; FAME_JOURNAL=1
/// opens their Celebrity Journal eight seconds in.
fn fame_test(
    time: Res<Time>,
    mut story: ResMut<crate::story::TownStory>,
    sel: Query<(Entity, &Sim), With<crate::sim::Selected>>,
    mut journal: ResMut<Journal>,
    mut done: Local<(bool, bool)>,
) {
    let Ok((e, sim)) = sel.single() else { return };
    if !done.0
        && time.elapsed_secs() > 5.0
        && let Ok(v) = std::env::var("FAME")
    {
        done.0 = true;
        let (level, points) = v.split_once(':').map_or((v.as_str(), "0"), |(a, b)| (a, b));
        let f = story.fame.entry(sim.id).or_default();
        f.level = level.parse::<u8>().unwrap_or(1).min(HIGHEST);
        f.points = points.parse().unwrap_or(0.0);
        f.next_gift = 1.0;
        info!("fame test: {} at celebrity level {}", sim.first, f.level);
    }
    if !done.1 && time.elapsed_secs() > 8.0 && std::env::var("FAME_JOURNAL").is_ok() {
        done.1 = true;
        journal.sim = Some(e);
    }
}

// `HUDCelebrityJournalDialog`'s windows (`CelebrityJournalDialog.ControlIDs`).
const OKAY: u32 = 0x05ef_6bd1;
const THUMBNAIL: u32 = 0x05ef_6bd2;
const TITLE: u32 = 0x05ef_6bd4;
const STARS: u32 = 0x05ef_6be1;
const TRACKED_STATS: u32 = 0x05ef_6be2;
const PROGRESS_BAR: u32 = 0x0919_0b13;
/// The bar's colours: gold at the top, green on the way (`MainColor`).
const BAR_TOP: u32 = 0xfff8_e000;
const BAR_ON: u32 = 0xff73_b437;

/// The Celebrity Journal: whose, and the dialog up.
#[derive(Resource, Default)]
pub struct Journal {
    pub sim: Option<Entity>,
    s: Option<Spawned>,
}

/// The game's Celebrity Journal for a Sim (`CelebrityJournalDialog`): their picture, the level's
/// stars, the bar to the next, their statistics; Accept closes it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn journal(
    mut commands: Commands,
    mut j: ResMut<Journal>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut portraits: ResMut<crate::portraits::Portraits>,
    story: Res<crate::story::TownStory>,
    sims: Query<(&Sim, Option<&crate::social::Relationships>)>,
    famous: Query<(), With<Famous>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut ui) = ui else { return };
    let Some(me) = j.sim else { return };
    let close = |commands: &mut Commands, j: &mut Journal| {
        if let Some(r) = j.s.take().and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
        *j = Journal::default();
    };
    let Ok((sim, rels)) = sims.get(me) else {
        close(&mut commands, &mut j);
        return;
    };
    if let Some(s) = &j.s {
        if crate::livehud::pressed(&clicks, s.id(OKAY)) {
            play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
            close(&mut commands, &mut j);
        }
        return;
    }
    let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "HUDCelebrityJournalDialog") else {
        warn!("no HUDCelebrityJournalDialog layout");
        j.sim = None;
        return;
    };
    play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
    let fame = story.fame.get(&sim.id).cloned().unwrap_or_default();
    let title = ui.localize("Ui/Caption/HUD/CelebrityJournal:DialogTitle").map_or_else(|| format!("Celebrity Journal for {}", sim.full_name()), |t| t.replace("{0.String}", &sim.full_name()));
    if let Some(t) = s.text(TITLE) {
        commands.entity(t).insert(Text::new(title));
    }
    if let Some(win) = s.id(THUMBNAIL) {
        let h = portraits.portrait(&mut images, me);
        commands.entity(win).despawn_children().with_children(|c| {
            c.spawn((ImageNode::new(h), crate::portraits::PortraitOf(me), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
        });
    }
    // (The level's stars: one window a level.)
    if let Some(stars) = s.id(STARS) {
        for l in 1..=HIGHEST as u32 {
            if let Some(e) = s.within(stars, l) {
                commands.entity(e).insert(if fame.level as u32 == l { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
        crate::hudpanels::tip(&mut commands, stars, level_name(Some(&ui), fame.level));
    }
    if let Some(bar) = s.id(PROGRESS_BAR) {
        let (f, tint) = (fame.progress(), crate::layout::color(if fame.level >= HIGHEST { BAR_TOP } else { BAR_ON }));
        commands.entity(bar).queue(move |mut e: EntityWorldMut| {
            if let Some(mut b) = e.get_mut::<crate::layout::UiFillBar>() {
                b.value = f;
                b.set_color(tint);
            }
        });
        let next = if fame.level >= HIGHEST { "At the top!".to_string() } else { format!("{:.0} of {:.0} points to {}", fame.points, goal(fame.level), level_name(Some(&ui), fame.level + 1)) };
        crate::hudpanels::tip(&mut commands, bar, next);
    }
    // (Their statistics, the game's lines: those kept here.)
    let known = rels.map_or(0, |r| r.0.keys().filter(|o| famous.contains(**o) && **o != me).count());
    let stat = |ui: &UiAssets, key: &str, or: &str, n: String| {
        ui.localize(&format!("Gameplay/CelebritySystem/CelebrityManager:{key}")).map_or_else(|| format!("{or}: {n}"), |t| crate::layout::gendered(&t, sim.female).replace("{0.Number}", &n).replace("{0.Money}", &n))
    };
    let lines = [
        stat(&ui, "NumberCelebritiesKnown", "Number of Celebrities Known", known.to_string()),
        stat(&ui, "NumberCelebrityDiscountsObtained", "Number of discounts obtained as a Celebrity", fame.discounts.to_string()),
        stat(&ui, "NumberCelebrityFreebiesObtained", "Number of freebies obtained as a Celebrity", fame.freebies.to_string()),
        stat(&ui, "NumberCelebrityMoneySaved", "Amount of money saved as a Celebrity", format!("§{}", fame.saved)),
    ];
    if let Some(t) = s.text(TRACKED_STATS) {
        commands.entity(t).insert(Text::new(lines.join("\n")));
    }
    if let Some(e) = s.id(OKAY) {
        commands.entity(e).insert(crate::icons::Tooltip(ui.localize("Ui/Caption/Global:Accept").unwrap_or_else(|| "Accept".into())));
    }
    j.s = Some(s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_take_a_sim_up_the_levels_as_the_game_counts_them() {
        let mut f = Fame::default();
        assert!(f.gain(99.0).is_empty());
        assert_eq!(f.gain(1.0), vec![1]);
        assert_eq!((f.level, f.points), (1, 0.0));
        // (The rest carried over: 1,500 to level 2, then 2,000 to 3.)
        assert_eq!(f.gain(3600.0), vec![2, 3]);
        assert_eq!((f.level, f.points), (3, 100.0));
        assert!((f.progress() - 0.025).abs() < 1e-6);
        assert_eq!(f.gain(1.0e6), vec![4, 5]);
        assert_eq!(f.progress(), 1.0);
        assert_eq!(f.points, goal(HIGHEST));
    }

    #[test]
    fn closer_to_the_more_famous_is_worth_more() {
        // (A nobody befriending a Superstar: twice and a quarter; of the same level, four fifths.)
        assert_eq!(relationship_points("Friend", 0, 5), 2250.0);
        assert_eq!(relationship_points("Spouse", 3, 3), 2400.0);
        assert_eq!(relationship_points("Good Friend", 5, 0), 0.0);
        assert_eq!(relationship_points("Acquaintance", 0, 5), 0.0);
        let mut rng = rand::rng();
        assert_eq!(discount(0, &mut rng), 1.0);
        for _ in 0..200 {
            let d = discount(5, &mut rng);
            assert!(d == 0.0 || d == 1.0 || (DISCOUNT_RANGE.0..DISCOUNT_RANGE.1).contains(&d));
        }
    }
}
