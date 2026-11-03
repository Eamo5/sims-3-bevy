//! The Equestrian Center's competitions, the Pets pack's (`EquestrianCenter` and its
//! `EnterEquestrianCompetition`). A Sim up on a horse picks "Enter Equestrian Competition..." on
//! the center's menu, then the competition (Racing, Show Jumping, Cross-Country) and its level
//! (Beginner, Advanced, International): greyed without a horse ("You need to bring a horse to
//! compete!"), outside the competitions' hours (every evening from six, and Sunday's matinee from
//! noon, entries closing two hours before they end), below the Riding each competition takes
//! (`kLevelForEnter…Competition`: 2, 3 and 6) or the horse's Racing or Jumping for its level
//! (`kLevelFor…Comps`: 0, 3 and 7). The rider rides off the lot's edge to it, and the two are
//! inside a hundred minutes: ten before it starts, its eighty, ten after.
//!
//! The competition goes as the game's does: its eleven others, a leg every five minutes, each
//! leg's score the rider's Riding's bonus and the horse's Racing's and Jumping's for the
//! competition (half each for Cross-Country), the pace's (the panel's "Steady Pace", "Take Risks"
//! and "Go for Broke": a chance at a bonus, else a penalty, the chance lifted by the rider's
//! Riding), and the horse's traits' (Fast and Lazy at racing, Agile and Hates Jumping at jumping:
//! `kPetTraitBonus`, `kPetTraitPenalty`); the two's mood counts too (`kMoodScoreMultiplier`). The
//! place is the best whose score it reaches on the center's table (`Scoring`), a racer's
//! projected over the whole race as it goes. Through it the game's competition panel shows the
//! competition, how far along it is, the rider's picture, the pace to pick and the standings, the
//! rider's row lit; the rider's hunger, bladder, fun and company and the horse's needs hold. At
//! the end, the game's words for how it went (a poor finish from 7th, a victory first), the prize
//! for the place, Riding for the rider and Racing and Jumping for the horse (not one that needs a
//! break: `kSkillPointsAdded`), and a winner's trophy in the rider's inventory.

use bevy::prelude::*;
use rand::Rng;
use s3bake::gamedata::EquestrianTables;

use crate::PlayMode;
use crate::interact::{ActionKind, Household, Notifications, Skills};
use crate::layout::{Spawned, UiAssets};
use crate::pets::Pet;
use crate::petneeds::PetMotives;
use crate::rabbitholes::{Activity, AtRabbitHole};
use crate::sim::{Selected, Sim};

pub struct EquestrianPlugin;

impl Plugin for EquestrianPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (compete_test, enter, compete, sell, back).chain().after(crate::riding::ride).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, panel.run_if(in_state(PlayMode::Live)));
    }
}

/// The competitions (`CompetitionType`) and their levels (`CompetitionLevel`).
pub const KINDS: [&str; 3] = ["Racing", "Jumping", "CrossCountry"];
pub const LEVELS: [&str; 3] = ["Beginner", "Advanced", "International"];
/// Their names on the menu, where the game's words aren't to be had.
const KIND_WORDS: [&str; 3] = ["Racing Competition", "Show Jumping Competition", "Cross-Country Competition"];
/// The paces (`Pace`): the panel's tones.
pub const PACES: [&str; 3] = ["SteadyPace", "TakeRisks", "GoForBroke"];
const PACE_WORDS: [(&str, &str); 3] = [
    ("Steady Pace", "Don't take any risks and get a reliable outcome."),
    ("Take Risks", "You might get a higher place, or you might fall behind."),
    ("Go for Broke", "Win it all or go home empty-handed!"),
];

/// The center's tuning (`EquestrianCenter`'s): a leg every five minutes over eighty, twelve
/// competing, ten minutes before it starts and ten after; each pace's chance, bonus and penalty;
/// the skill points at the end (rider's Riding, horse's Racing, horse's Jumping); the horse
/// traits' bonus and penalty (at racing, at jumping); the mood's weight; the places that make a
/// standard finish and a victory (`kPositionsForFinishTypes`).
const LEG_MINUTES: f64 = 5.0;
const LENGTH_MINUTES: f64 = 80.0;
const COMPETITORS: usize = 12;
const BEFORE_MINUTES: f64 = 10.0;
const AFTER_MINUTES: f64 = 10.0;
const TONE_CHANCE: [f32; 3] = [0.8, 0.5, 0.35];
const TONE_BONUS: [i32; 3] = [1, 8, 16];
const TONE_PENALTY: [i32; 3] = [0, 8, 16];
const SKILL_POINTS: [f32; 3] = [900.0; 3];
const TRAIT_BONUS: [i32; 2] = [2, 2];
const TRAIT_PENALTY: [i32; 2] = [2, 2];
const MOOD_SCORE: f32 = 0.1;
const FINISH_PLACES: [u8; 3] = [7, 2, 1];
/// The Riding to enter each competition (`RidingSkill`'s `kLevelForEnter…Competition`), and the
/// horse's Racing or Jumping for each level (`Racing`'s and `Jumping`'s `kLevelFor…Comps`).
pub const RIDING_TO_ENTER: [u32; 3] = [2, 3, 6];
pub const HORSE_LEVEL: [u32; 3] = [0, 3, 7];
/// The competitions' hours (`ShowVenue`'s): every evening from six for six hours, Sunday's
/// matinee from noon for five; entries close the length of a visit before the end
/// (`kAttendInteractionDurationMinutes`).
const EVENING: (f32, f32) = (18.0, 6.0);
const MATINEE: (f32, f32) = (12.0, 5.0);
const SUNDAY: usize = 6;
const ENTRY_CLOSES_HOURS: f32 = 2.0;
/// The winner's trophies (`AddCompetitionTrophy`), by competition.
const TROPHIES: [&str; 3] = ["sculptureRewardHorseRace", "sculptureRewardHorseJumping", "sculptureRewardHorseCrossCountry"];

/// Time at the center: before, the competition, after (the drive there and back on top); a
/// horse's sale (`kSellHorseInteractionLength`).
pub static COMPETE: Activity = crate::rabbitholes::act("Compete", (BEFORE_MINUTES + LENGTH_MINUTES + AFTER_MINUTES) as f32, 0, [0.0; 6], None);
pub static SELL: Activity = crate::rabbitholes::act("Sell Horse", 5.0, 0, [0.0; 6], None);

/// Whether a rabbit-hole visit is one to the center with a horse.
pub fn with_horse(a: &Activity) -> bool {
    std::ptr::eq(a, &COMPETE) || std::ptr::eq(a, &SELL)
}

/// What a Sim rides off to the center for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CenterTask {
    /// A competition (Racing, Jumping, Cross-Country) at a level (Beginner, Advanced,
    /// International).
    Compete { kind: u8, level: u8 },
    /// The horse sold (`SellHorse`).
    Sell,
}

/// What a horse is worth, as the game's `HorseManager.GetHorseCost` with its tuning: an adult's
/// own worth, two hundred for each good trait and a hundred off for each bad one, six hundred a
/// level of Racing and of Jumping, four to six hundred besides (`base`, 0 to 1), a fifth more to
/// buy; fifty at the least.
pub fn horse_cost(racing: u32, jumping: u32, trait_scores: &[i32], base: f32, buy: bool) -> i64 {
    let mut n = COST_ADULT;
    for s in trait_scores {
        n += if *s > 0 { COST_GOOD_TRAIT } else if *s < 0 { -COST_BAD_TRAIT } else { 0 };
    }
    n += (racing.min(10) as f32 * COST_SKILL[0]) as i64 + (jumping.min(10) as f32 * COST_SKILL[1]) as i64;
    n += (COST_BASE[0] + (COST_BASE[1] - COST_BASE[0]) * base.clamp(0.0, 1.0)) as i64;
    if buy {
        n = (n as f32 * COST_BUY) as i64;
    }
    n.max(MIN_COST)
}

/// `HorseManager`'s tuning: an adult's worth, each level of Racing's and Jumping's, the range
/// besides, a buyer's markup, good and bad traits', the least; and the sale's words by price
/// (`kHorseSellPriceTNSValue`).
const COST_ADULT: i64 = 0;
const COST_SKILL: [f32; 2] = [600.0, 600.0];
const COST_BASE: [f32; 2] = [400.0, 600.0];
const COST_BUY: f32 = 1.2;
const COST_GOOD_TRAIT: i64 = 200;
const COST_BAD_TRAIT: i64 = 100;
const MIN_COST: i64 = 50;
const SELL_WORDS_AT: [i64; 2] = [5000, 10000];

/// Whether a lot is the Equestrian Center.
pub fn is_center(world: &crate::loading::WorldInfo, lot: usize) -> bool {
    crate::opportunities::lot_types(world, lot).contains(&"EquestrianCenter")
}

/// Whether entries are open now: `Some(true)` open, `Some(false)` a competition on but entries
/// closed, `None` no competition.
pub fn doors(weekday: usize, hour: f32) -> Option<bool> {
    [(EVENING, true), (MATINEE, weekday == SUNDAY)].into_iter().filter(|(_, today)| *today).find_map(|((start, hours), _)| (hour >= start && hour < start + hours).then_some(hour < start + hours - ENTRY_CLOSES_HOURS))
}

/// The hour the next competition starts (and whether it's tomorrow), for the center's tooltip.
pub fn next_start(weekday: usize, hour: f32) -> (f32, bool) {
    let today: Vec<f32> = [(EVENING.0, true), (MATINEE.0, weekday == SUNDAY)].into_iter().filter(|(h, t)| *t && *h > hour).map(|(h, _)| h).collect();
    match today.into_iter().reduce(f32::min) {
        Some(h) => (h, false),
        None => (if (weekday + 1) % 7 == SUNDAY { MATINEE.0 } else { EVENING.0 }, true),
    }
}

/// What the center's tag says of its competitions: one on, or when the next is.
pub fn show_text(ui: Option<&UiAssets>, weekday: usize, hour: f32) -> String {
    const KEY: &str = "Gameplay/Objects/RabbitHoles/EquestrianCenter:";
    if doors(weekday, hour).is_some() {
        return words(ui, &format!("{KEY}ShowInProgress"), "Equestrian competition in progress.");
    }
    let (h, tomorrow) = next_start(weekday, hour);
    let at = crate::clock::clock_time(h as u64, 0);
    if tomorrow {
        words(ui, &format!("{KEY}NextShowTomorrowAt"), "Next equestrian competition tomorrow at {0.String}").replace("{0.String}", &at)
    } else {
        words(ui, &format!("{KEY}NextShowAt"), "Next equestrian competition at {0.String}").replace("{0.String}", &at)
    }
}

/// The game's words by key, else these.
fn words(ui: Option<&UiAssets>, key: &str, or: &str) -> String {
    ui.and_then(|u| u.localize(key)).filter(|s| !s.is_empty()).unwrap_or_else(|| or.to_string())
}

const ENTER_KEY: &str = "Gameplay/Objects/RabbitHoles/EquestrianCenter/EnterCompetition:";

/// The center's competitions on its menu for a Sim (up on a horse, its skills): each
/// competition's levels to pick, or why it can't be entered; or why none can be.
pub type Menu = Result<Vec<(String, Result<Vec<(String, ActionKind)>, String>)>, String>;

pub fn menu(ui: Option<&UiAssets>, lot: usize, sim: &Sim, riding: u32, horse: Option<(&Pet, Option<&crate::horsejumps::HorseSkills>)>, weekday: usize, hour: f32) -> Menu {
    let say = |k: &str, or: &str| crate::layout::gendered(&words(ui, &format!("{ENTER_KEY}{k}"), or), sim.female);
    match doors(weekday, hour) {
        None => return Err(say("DoorsNeedToBeOpen", "No competitions are happening right now. Come back later!")),
        Some(false) => return Err(say("DoorsClosed", "The deadline for entry has passed. Enter earlier next time!")),
        Some(true) => {}
    }
    let Some((pet, skills)) = horse else {
        return Err(say("RequiresHorse", "You need to bring a horse to compete!").replace("{0.SimFirstName}", &sim.first));
    };
    let racing = skills.map_or(0, |s| s.racing_level());
    let jumping = skills.map_or(0, |s| s.level());
    Ok((0..3)
        .map(|k| {
            let label = words(ui, &format!("Gameplay/Objects/RabbitHoles/EquestrianCenter:{}", KINDS[k]), KIND_WORDS[k]);
            if riding < RIDING_TO_ENTER[k] {
                return (label, Err(say("NotEnoughRidingSkill", "{0.SimName}'s Riding Skill is too low to enter this competition.").replace("{0.SimName}", &sim.full_name())));
            }
            let levels = (0..3)
                .map(|l| {
                    let name = words(ui, &format!("Gameplay/Objects/RabbitHoles/EquestrianCenter:{}", LEVELS[l]), LEVELS[l]);
                    let short = |key: &str, or: &str| crate::layout::gendered(&words(ui, &format!("{ENTER_KEY}{key}"), or), pet.female).replace("{0.SimFirstName}", &pet.name);
                    let kind = if (k == 0 || k == 2) && racing < HORSE_LEVEL[l] {
                        ActionKind::Greyed(short("NotEnoughRacingSkill", "{0.SimFirstName} does not have enough Racing Skill to qualify for this competition."))
                    } else if (k == 1 || k == 2) && jumping < HORSE_LEVEL[l] {
                        ActionKind::Greyed(short("NotEnoughJumpingSkill", "{0.SimFirstName} does not have enough Jumping Skill to qualify for this competition."))
                    } else {
                        ActionKind::Equestrian { lot, task: CenterTask::Compete { kind: k as u8, level: l as u8 } }
                    };
                    (name, kind)
                })
                .collect();
            (label, Ok(levels))
        })
        .collect())
}

/// "Sell Horse" on the center's menu: up on a horse, or greyed with why not.
pub fn sell_option(ui: Option<&UiAssets>, lot: usize, sim: &Sim, horse: Option<&Pet>) -> (String, ActionKind) {
    const KEY: &str = "Gameplay/Objects/RabbitHoles/EquestrianCenter/SellHorse:";
    let label = words(ui, &format!("{KEY}InteractionName"), "Sell Horse");
    match horse {
        Some(p) if p.kind == "ah" => (label, ActionKind::Equestrian { lot, task: CenterTask::Sell }),
        Some(_) => (label, ActionKind::Greyed(words(ui, "Gameplay/Objects/RabbitHoles/EquestrianCenter/OfferStallionAsStud:NeedAdultHorse", "{0.SimName} must bring an adult horse to do that.").replace("{0.SimName}", &sim.full_name()))),
        None => (label, ActionKind::Greyed(words(ui, &format!("{KEY}SimNeedsToBeRidingAHorse"), "{0.SimName} needs to ride or lead a horse into the Equestrian Center to sell it.").replace("{0.SimName}", &sim.full_name()))),
    }
}

/// "Enter Equestrian Competition..."
pub fn enter_label(ui: Option<&UiAssets>) -> String {
    format!("{}...", words(ui, &format!("{ENTER_KEY}EnterEquestrianCompetition"), "Enter Equestrian Competition"))
}

/// A leg's score, as the game's `CurrentLegScore`: the skills' bonuses for the competition, the
/// pace's (`hit`: whether its chance came off), the horse's traits'.
pub fn leg_score(t: &EquestrianTables, kind: usize, riding: u32, racing: u32, jumping: u32, pace: usize, hit: bool, traits: &[String]) -> i32 {
    let row = |l: u32| t.skills.iter().find(|r| u32::from(r.level) == l.min(10));
    let mut s = row(riding).map_or(0, |r| r.riding);
    s += row(racing).map_or(0, |r| match kind {
        0 => r.racing,
        2 => r.racing / 2,
        _ => 0,
    });
    s += row(jumping).map_or(0, |r| match kind {
        1 => r.jumping,
        2 => r.jumping / 2,
        _ => 0,
    });
    s += if hit { TONE_BONUS[pace] } else { -TONE_PENALTY[pace] };
    s + trait_score(kind, traits)
}

/// The horse's traits' part (the game's `GetScoreForTrait`).
pub fn trait_score(kind: usize, traits: &[String]) -> i32 {
    let has = |t: &str| crate::pettraits::has(traits, t);
    let racing = kind == 0 || kind == 2;
    let jumping = kind == 1 || kind == 2;
    let mut s = 0;
    if racing && has("FastPet") {
        s += TRAIT_BONUS[0];
    }
    if racing && has("LazyHorse") {
        s -= TRAIT_PENALTY[0];
    }
    if jumping && has("AgilePet") {
        s += TRAIT_BONUS[1];
    }
    if jumping && has("HatesJumpingPet") {
        s -= TRAIT_PENALTY[1];
    }
    s
}

/// The chance a pace comes off for a rider of a Riding level.
pub fn pace_chance(t: &EquestrianTables, pace: usize, riding: u32) -> f32 {
    TONE_CHANCE[pace] * t.skills.iter().find(|r| u32::from(r.level) == riding.min(10)).map_or(1.0, |r| r.tone)
}

/// The place a score earns, and its prize (`GetPlaceInRace`: the best place whose score it
/// reaches; last, and nothing, below every one).
pub fn place(t: &EquestrianTables, kind: usize, level: usize, score: i32) -> (u8, i32) {
    t.scoring
        .iter()
        .filter(|p| usize::from(p.kind) == kind && usize::from(p.level) == level && score >= p.score && usize::from(p.place) <= COMPETITORS)
        .min_by_key(|p| p.place)
        .map_or((COMPETITORS as u8, 0), |p| (p.place, p.money))
}

/// How a place finishes (`FinishType`): 0 poor, 1 standard, 2 a victory.
pub fn finish(place: u8) -> usize {
    if place == FINISH_PLACES[2] {
        2
    } else if place >= FINISH_PLACES[1] && place < FINISH_PLACES[0] {
        1
    } else {
        0
    }
}

/// The two's mood's part (`CalculateMoodBonus`).
pub fn mood_bonus(rider: f32, horse: f32) -> i32 {
    ((rider + horse) * MOOD_SCORE * 0.5) as i32
}

/// The score that places a competitor so far: a racer's projected over the whole race.
pub fn standing(kind: usize, score: i32, legs: u32, mood: i32) -> i32 {
    if kind == 0 && legs > 0 {
        (score as f64 * LENGTH_MINUTES / (LEG_MINUTES * legs as f64)) as i32 + mood
    } else {
        score + mood
    }
}

/// Ridden off to the center (`riding`, at the lot's edge): away to it.
#[derive(Component)]
pub struct ToCenter {
    pub lot: usize,
    pub task: CenterTask,
    pub horse: Entity,
}

/// At the center to sell the horse: whether the price has been asked.
#[derive(Component)]
pub struct SellingHorse {
    pub horse: Entity,
    asked: bool,
}

/// A Sim at a competition with their horse: which, its name, the pace, the others (rider, horse),
/// the legs run, the score, the place, and how it went.
#[derive(Component)]
pub struct Competing {
    pub horse: Entity,
    pub kind: usize,
    pub level: usize,
    pub name: String,
    pub pace: usize,
    pub others: Vec<(String, String)>,
    pub legs: u32,
    pub score: i32,
    pub mood: i32,
    pub place: u8,
    /// The competition's start (game minutes), and when the panel's put away.
    pub start: f64,
    pub announced: bool,
    pub started: bool,
    pub done: bool,
    /// The needs held through it: the rider's fun and company, the horse's.
    held: Option<([f32; 2], [f32; 8])>,
}

/// Back home from a competition (`rabbitholes`): the horse with them again.
#[derive(Component)]
pub struct BackFromCompetition;

/// The horse at a competition with its rider.
#[derive(Component)]
pub struct AtCompetition;

/// Off to the center: the rider there (`AtRabbitHole`), the horse with them.
#[allow(clippy::type_complexity)]
fn enter(
    mut commands: Commands,
    (clock, world, ui): (Res<crate::clock::GameClock>, Res<crate::loading::CurrentWorld>, Option<Res<UiAssets>>),
    sims: Query<(Entity, &Sim, &ToCenter)>,
    pets: Query<&Pet>,
    mut notes: ResMut<Notifications>,
) {
    let mut rng = rand::rng();
    for (e, sim, ec) in &sims {
        commands.entity(e).remove::<ToCenter>();
        let Ok(horse) = pets.get(ec.horse) else { continue };
        let place = world.data.lots.get(ec.lot).map_or_else(|| "the Equestrian Center".to_string(), |l| crate::rabbitholes::lot_title(l, world.data.lot_names.get(ec.lot).map_or("", |s| s.as_str())));
        let inside_from = clock.minutes + crate::rabbitholes::DRIVE_MINUTES;
        let (kind, level) = match ec.task {
            CenterTask::Compete { kind, level } => (usize::from(kind).min(2), usize::from(level).min(2)),
            // (A sale: the price asked once they're in.)
            CenterTask::Sell => {
                info!("equestrian: {} rides {} off to {place} to sell {}", sim.first, horse.name, horse.name);
                notes.push(format!("{} rode {} off to {} to sell {}.", sim.first, horse.name, place, horse.name));
                let until = inside_from + SELL.minutes as f64 + crate::rabbitholes::DRIVE_MINUTES;
                commands.entity(e).insert((AtRabbitHole { lot: ec.lot, activity: &SELL, inside_from, until, place }, Visibility::Hidden, SellingHorse { horse: ec.horse, asked: false }));
                commands.entity(ec.horse).insert((Visibility::Hidden, AtCompetition));
                continue;
            }
        };
        let until = inside_from + COMPETE.minutes as f64 + crate::rabbitholes::DRIVE_MINUTES;
        let name = words(ui.as_deref(), &format!("Gameplay/Objects/RabbitHoles/EquestrianCenter:CompetitionName{}{}", KINDS[kind], LEVELS[level]), &format!("{} {}", LEVELS[level], KIND_WORDS[kind]));
        // (The others, none with the same name, nor their horses: `SetCompetitorsNames`.)
        let mut others: Vec<(String, String)> = Vec::new();
        for _ in 0..COMPETITORS * 20 {
            if others.len() == COMPETITORS - 1 {
                break;
            }
            let rider = format!("{} {}", crate::sim::random_first_name(&mut rng), crate::sim::random_last_name(&mut rng));
            let mount = crate::pets::random_pet_name(&mut rng);
            if rider != sim.full_name() && mount != horse.name && !others.iter().any(|(r, h)| *r == rider || *h == mount) {
                others.push((rider, mount));
            }
        }
        info!("equestrian: {} rides {} off to the {name} ({} {}) at {place}", sim.first, horse.name, LEVELS[level], KINDS[kind]);
        notes.push(format!("{} rode {} off to {} to compete.", sim.first, horse.name, place));
        commands.entity(e).insert((
            AtRabbitHole { lot: ec.lot, activity: &COMPETE, inside_from, until, place },
            Visibility::Hidden,
            Competing { horse: ec.horse, kind, level, name, pace: 0, others, legs: 0, score: 0, mood: 0, place: COMPETITORS as u8, start: inside_from + BEFORE_MINUTES, announced: false, started: false, done: false, held: None },
        ));
        commands.entity(ec.horse).insert((Visibility::Hidden, AtCompetition));
    }
}

/// The competition: announced once they're inside, then a leg every five minutes, the needs held,
/// and at the end how it went.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn compete(
    mut commands: Commands,
    (clock, ui, gu, data): (Res<crate::clock::GameClock>, Option<Res<UiAssets>>, Option<Res<crate::icons::GameUi>>, Res<crate::baked::Baked>),
    mut sims: Query<(Entity, &Sim, &AtRabbitHole, &mut Competing, &mut Skills, &mut crate::sim::Motives, Option<&crate::life::Mood>)>,
    mut horses: Query<(&Pet, Option<&mut crate::horsejumps::HorseSkills>, Option<&mut PetMotives>, Option<&crate::pets::PetTraits>)>,
    (mut notes, mut household, mut play, mut life): (ResMut<Notifications>, Option<ResMut<Household>>, MessageWriter<crate::sound::PlaySound>, MessageWriter<crate::life::LifeEvent>),
) {
    let Some(gu) = gu else { return };
    let tables = &gu.data.equestrian;
    let mut rng = rand::rng();
    let now = clock.minutes;
    for (e, sim, at, mut c, mut skills, mut motives, mood) in &mut sims {
        if now < at.inside_from {
            continue;
        }
        let Ok((horse, mut hskills, mut hmotives, traits)) = horses.get_mut(c.horse) else { continue };
        let notice = |key: &str, or: String| -> String {
            ui.as_deref().and_then(|u| u.localize(&format!("Gameplay/Excel/Notifications/Notifications:{key}"))).filter(|s| !s.is_empty()).map_or(or, |t| crate::layout::gendered(&t, sim.female))
        };
        if !c.announced {
            c.announced = true;
            let key = format!("EquestrianCenterStart{}{}", KINDS[c.kind], LEVELS[c.level]);
            let t = notice(&key, format!("Welcome to the {}! {} ridden by {} had better be ready, because it starts soon!", c.name, horse.name, sim.full_name()));
            notes.push(t.replace("{0.SimFirstName}", &horse.name).replace("{0.SimName}", &horse.name).replace("{1.SimName}", &sim.full_name()).replace("{1.SimFirstName}", &sim.first).replace("{2.String}", &c.name));
        }
        if now < c.start {
            continue;
        }
        let riding = skills.level("Riding");
        let (racing, jumping) = hskills.as_deref().map_or((0, 0), |s| (s.racing_level(), s.level()));
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        if !c.started {
            c.started = true;
            c.mood = mood_bonus(mood.map_or(0.0, |m| m.0), hmotives.as_deref().map_or(0.0, crate::petneeds::mood));
            c.held = Some(([motives.0[crate::sim::SOCIAL], motives.0[crate::sim::FUN]], hmotives.as_deref().map_or([0.0; 8], |m| m.0)));
            play.write(crate::sound::PlaySound::ui("rhole_eques_comp_start"));
            info!("equestrian: the {} starts (mood bonus {})", c.name, c.mood);
        }
        let elapsed = now - c.start;
        if !c.done {
            // (Their needs held: the rider's hunger and bladder are the center's, their fun and
            // company as they were; the horse's.)
            if let Some((rider, h)) = c.held {
                motives.0[crate::sim::SOCIAL] = rider[0];
                motives.0[crate::sim::FUN] = rider[1];
                if let Some(m) = hmotives.as_deref_mut() {
                    for i in [crate::petneeds::HUNGER, crate::petneeds::BLADDER, crate::petneeds::THIRST, crate::petneeds::URGE, crate::petneeds::SOCIAL] {
                        m.0[i] = h[i];
                    }
                }
            }
            // A leg as each five minutes begins.
            let due = ((elapsed.min(LENGTH_MINUTES - 1e-6) / LEG_MINUTES).floor() as u32 + 1).min((LENGTH_MINUTES / LEG_MINUTES) as u32);
            while c.legs < due {
                let hit = rng.random::<f32>() < pace_chance(tables, c.pace, riding);
                c.score += leg_score(tables, c.kind, riding, racing, jumping, c.pace, hit, traits);
                c.legs += 1;
                if c.kind == 0 {
                    shuffle(&mut c.others, &mut rng);
                }
                c.place = place(tables, c.kind, c.level, standing(c.kind, c.score, c.legs, c.mood)).0;
                debug!("equestrian: leg {} ({}): score {}, {}", c.legs, PACES[c.pace], c.score, c.place);
            }
        }
        if elapsed < LENGTH_MINUTES || c.done {
            continue;
        }
        // Over: the place, the prize, the skills, the words for it.
        c.done = true;
        c.score += c.mood;
        if c.kind == 0 {
            shuffle(&mut c.others, &mut rng);
        }
        let (placed, money) = place(tables, c.kind, c.level, c.score);
        c.place = placed;
        if money > 0
            && let Some(h) = household.as_deref_mut()
        {
            h.funds += i64::from(money);
        }
        if let Some(l) = skills.add_points("Riding", SKILL_POINTS[0]) {
            notes.push(format!("{} reached level {l} in Riding!", sim.first));
            life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: "Riding", level: l }));
        }
        let mut hs = hskills.as_deref().cloned().unwrap_or_default();
        if !hs.worn_out {
            if (c.kind == 0 || c.kind == 2)
                && let Some(l) = hs.add_racing(SKILL_POINTS[1])
            {
                notes.push(format!("{} reached level {l} in Racing!", horse.name));
            }
            if (c.kind == 1 || c.kind == 2)
                && let Some(l) = hs.add_points(SKILL_POINTS[2])
            {
                notes.push(format!("{} reached level {l} in Jumping!", horse.name));
            }
        }
        match hskills.as_deref_mut() {
            Some(s) => *s = hs,
            None => {
                commands.entity(c.horse).insert(hs);
            }
        }
        let place_words = words(ui.as_deref(), &format!("UI/Caption/HudCompetitionPanel/Place:Position{placed}"), &ordinal(placed));
        let fill = |t: String| t.replace("{0.SimFirstName}", &horse.name).replace("{0.SimName}", &horse.name).replace("{1.SimName}", &sim.full_name()).replace("{1.SimFirstName}", &sim.first);
        let how = finish(placed);
        let text = match how {
            2 => fill(notice("EquestrianCenterVictory", "{0.SimFirstName} ridden by {1.SimName} has placed first in the {2.String}! In addition to the prize money, {1.SimName} has received a trophy and the adoration of the crowd! Huzzah!".into())).replace("{2.String}", &c.name),
            1 => fill(notice("EquestrianCenterStandardFinish", "Congratulations, {0.SimFirstName} ridden by {1.SimName} has placed {2.String} in the {3.String}! Enjoy your prize money!".into())).replace("{2.String}", &place_words).replace("{3.String}", &c.name),
            _ => fill(notice("EquestrianCenterPoorFinish", "{0.SimName} ridden by {1.SimName} has placed {2.String} in the {3.String}. Better luck next time!".into())).replace("{2.String}", &place_words).replace("{3.String}", &c.name),
        };
        let text = if money > 0 { format!("{text} (§{money})") } else { text };
        notes.push(text);
        play.write(crate::sound::PlaySound::ui(["sting_eques_poor_finish", "sting_eques_standard_finish", "sting_eques_victory_finish"][how]));
        // (A winner's trophy, in the rider's inventory.)
        if how == 2
            && let Some(t) = data.0.catalog.iter().find(|t| t.instance_name.eq_ignore_ascii_case(TROPHIES[c.kind]))
        {
            crate::inventory::give(&mut commands, e, crate::inventory::ItemKind::Object, t.instance_name.clone(), t.name.trim().to_string(), 0, i64::from(t.price.max(0)), 1);
        }
        info!("equestrian: {} and {} placed {placed} in the {} (score {}, §{money})", sim.first, horse.name, c.name, c.score);
    }
}

/// The others' order shaken up a little, as a race goes (`RaceShuffle`: each now and then
/// swapped with one a place or two off).
fn shuffle(others: &mut [(String, String)], rng: &mut impl Rng) {
    let n = others.len();
    for i in (1..n).rev() {
        if rng.random_bool(0.5) {
            let j = (i as i32 - 2 + rng.random_range(0..4)).clamp(0, n as i32 - 1) as usize;
            others.swap(i, j);
        }
    }
}

/// "1st", "2nd"...
fn ordinal(n: u8) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Home again: the horse with them, as it was (unless it's been sold).
fn back(mut commands: Commands, sims: Query<(Entity, Option<&Competing>, Option<&SellingHorse>), (With<BackFromCompetition>, Without<AtRabbitHole>)>, horses: Query<(), With<Pet>>) {
    for (e, c, s) in &sims {
        commands.entity(e).remove::<(BackFromCompetition, Competing, SellingHorse)>();
        if let Some(h) = c.map(|c| c.horse).or(s.map(|s| s.horse)).filter(|h| horses.contains(*h)) {
            commands.entity(h).remove::<AtCompetition>().insert(Visibility::Inherited);
        }
    }
}

/// A horse's sale: its price asked once they're in (`SellHorse`'s "Are you sure?"); sold, the
/// price paid, the game's words for it by how much, and the horse gone (the rider home on foot);
/// not, home with it.
#[allow(clippy::type_complexity)]
fn sell(
    mut commands: Commands,
    (clock, ui, gu): (Res<crate::clock::GameClock>, Option<Res<UiAssets>>, Option<Res<crate::icons::GameUi>>),
    mut sims: Query<(Entity, &Sim, &mut AtRabbitHole, &mut SellingHorse)>,
    horses: Query<(&Pet, Option<&crate::horsejumps::HorseSkills>, Option<&crate::pets::PetTraits>, Option<&crate::riding::Tack>)>,
    (mut questions, mut answers): (ResMut<crate::dialog::Questions>, MessageReader<crate::dialog::Answered>),
    (mut notes, mut household, mut play): (ResMut<Notifications>, Option<ResMut<Household>>, MessageWriter<crate::sound::PlaySound>),
) {
    for a in answers.read() {
        let crate::dialog::Question::SellHorse { sim, horse, price } = a.about else { continue };
        let Ok((_, s, mut at, _)) = sims.get_mut(sim) else { continue };
        let Ok((pet, _, _, tack)) = horses.get(horse) else { continue };
        if a.answer != 0 {
            // (Not sold: home with it.)
            at.until = at.until.min(clock.minutes + crate::rabbitholes::DRIVE_MINUTES);
            continue;
        }
        if let Some(h) = household.as_deref_mut() {
            h.funds += price;
        }
        let key = if price > SELL_WORDS_AT[1] {
            "EquestrianCenterHighPriceSellTNS"
        } else if price > SELL_WORDS_AT[0] {
            "EquestrianCenterMediumPriceSellTNS"
        } else {
            "EquestrianCenterLowPriceSellTNS"
        };
        let or = format!("Thanks! We'll find a buyer for {} soon!", pet.name);
        let t = ui.as_deref().and_then(|u| u.localize(&format!("Gameplay/Excel/Notifications/Notifications:{key}"))).filter(|t| !t.is_empty()).map_or(or, |t| crate::layout::gendered(&t, pet.female).replace("{0.SimFirstName}", &pet.name));
        notes.push(format!("{t} (§{price})"));
        play.write(crate::sound::PlaySound::ui("ui_object_sell"));
        info!("equestrian: {} sold {} for §{price}", s.first, pet.name);
        for p in tack.map_or(&[][..], |t| &t.0[..]) {
            commands.entity(*p).try_despawn();
        }
        commands.entity(horse).try_despawn();
    }
    for (e, s, at, mut sh) in &mut sims {
        if sh.asked || clock.minutes < at.inside_from {
            continue;
        }
        sh.asked = true;
        let Ok((pet, skills, traits, _)) = horses.get(sh.horse) else { continue };
        let scores: Vec<i32> = traits.map_or(&[][..], |t| &t.0[..]).iter().filter_map(|t| gu.as_deref().and_then(|g| g.data.traits.iter().find(|x| x.hex == *t)).map(|x| x.score)).collect();
        let price = horse_cost(skills.map_or(0, |s| s.racing_level()), skills.map_or(0, |s| s.level()), &scores, rand::rng().random(), false);
        const KEY: &str = "Gameplay/Objects/RabbitHoles/EquestrianCenter/SellHorse:";
        let text = words(ui.as_deref(), &format!("{KEY}SellHorse"), "{1.SimFirstName} will sell for {2.Money}. Are you sure you want to sell {1.SimFirstName}? You might not be able to buy {MA.him}{FA.her} back if you change your mind.");
        let text = crate::layout::gendered(&text, pet.female).replace("{1.SimFirstName}", &pet.name).replace("{2.Money}", &format!("§{price}"));
        let (yes, no) = (words(ui.as_deref(), "Ui/Caption/Global:Yes", "Yes"), words(ui.as_deref(), "Ui/Caption/Global:No", "No"));
        questions.ask(crate::dialog::Ask {
            about: crate::dialog::Question::SellHorse { sim: e, horse: sh.horse, price },
            icon: String::new(),
            heading: String::new(),
            title: words(ui.as_deref(), &format!("{KEY}InteractionName"), "Sell Horse"),
            text,
            answers: vec![crate::dialog::Answer { label: yes, detail: String::new(), icon: String::new() }, crate::dialog::Answer { label: no, detail: String::new(), icon: String::new() }],
        });
        info!("equestrian: {} asked to sell {} for §{price}", s.first, pet.name);
    }
}

/// The game's competition panel (`HudCompetitionPanel`), up while the selected Sim is in a
/// competition (from going in till ten minutes after it ends): its name and level, how far along
/// it is, the rider's picture, the pace to pick, and the standings, the rider's row lit.
#[derive(Default)]
struct PanelState {
    s: Option<Spawned>,
    sim: Option<Entity>,
    rows: Option<Entity>,
    shown: Option<String>,
}

// `HudCompetitionPanel`'s windows.
const TITLE: u32 = 0x0c3d_8a80;
const DESCRIPTION: u32 = 0x0c3d_8a81;
const PROGRESS: u32 = 0x0c3d_8a82;
const THUMBNAIL: u32 = 0x0c3d_8a83;
const TABLE: u32 = 0x0c3d_8a84;
const PACE: u32 = 0x0c3d_8a85;
const PACE_TIP: u32 = 0x0c3d_8a86;
/// The progress bar's clip and fill (`FillBarController`'s), and its groove's height.
const BAR_CLIP: u32 = 0x000f_bc02;
const BAR_FILL: u32 = 0x000f_bc01;
const BAR_HEIGHT: f32 = 21.0;
/// The standings' columns as the table lays them out (place, horse, rider), its header, rows.
const COLUMNS: [f32; 3] = [76.0, 160.0, 200.0];
const COLUMN_HEADERS: [(&str, &str); 3] = [("PlaceColumn", "Position"), ("HorseColumn", "Horse"), ("RiderColumn", "Rider")];
const HEADER: f32 = 27.0;
const ROW: f32 = 23.0;
/// The rider's row's colour (`TableTextWithBackground`'s highlight), the others' every other.
const LIT: u32 = 0xff46_628d;
const ALTERNATE: u32 = 0xff2f_2b2b;

#[allow(clippy::type_complexity)]
fn panel(
    mut commands: Commands,
    mut state: Local<PanelState>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    clock: Res<crate::clock::GameClock>,
    mut sel: Query<(Entity, &Sim, Option<&mut Competing>), With<Selected>>,
    pets: Query<&Pet>,
    mut combos: Query<&mut crate::layout::UiCombo>,
    mut bars: Query<&mut crate::layout::UiFillBar>,
    mut portraits: Option<ResMut<crate::portraits::Portraits>>,
) {
    let Some(mut ui) = ui else { return };
    let current = sel.single_mut().ok().filter(|(_, _, c)| c.as_ref().is_some_and(|c| c.announced && clock.minutes < c.start + LENGTH_MINUTES + AFTER_MINUTES));
    let close = |commands: &mut Commands, state: &mut PanelState| {
        if let Some(r) = state.s.take().and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
        *state = PanelState::default();
    };
    let Some((me, sim, Some(mut c))) = current else {
        if state.s.is_some() {
            close(&mut commands, &mut state);
        }
        return;
    };
    if state.sim != Some(me) {
        close(&mut commands, &mut state);
    }
    let horse = pets.get(c.horse).map_or_else(|_| String::new(), |p| p.name.clone());
    if state.s.is_none() {
        let Some(mut w) = ui.layout("HudCompetitionPanel").cloned() else { return };
        let (pw, ph) = (w.area[2] - w.area[0], w.area[3] - w.area[1]);
        w.area = [0.0, 0.0, pw, ph];
        w.place = s3bake::ui::UiPlace::Center(0.5);
        w.flags |= s3bake::ui::WIN_VISIBLE;
        // (The pace's words beside its list come from its tooltip's box.)
        let tip_drawable = w.find(PACE_TIP).and_then(|t| t.drawable.clone());
        let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
        let Some(root) = s.root else { return };
        commands.entity(root).insert((GlobalZIndex(5), crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(crate::AppState::InGame)));
        if let Some(t) = s.id(PACE_TIP) {
            commands.entity(t).insert(Visibility::Hidden);
        }
        let paces: Vec<(String, String)> = (0..3).map(|i| (words(Some(&*ui), &format!("Gameplay/Objects/RabbitHoles/EquestrianCenter:{}Tone", PACES[i]), PACE_WORDS[i].0), words(Some(&*ui), &format!("Gameplay/Objects/RabbitHoles/EquestrianCenter:{}ToneDescription", PACES[i]), PACE_WORDS[i].1))).collect();
        if let Some(p) = s.id(PACE) {
            let selected = c.pace;
            commands.entity(p).queue(move |mut ew: EntityWorldMut| {
                if let Some(mut combo) = ew.get_mut::<crate::layout::UiCombo>() {
                    combo.items = paces.iter().map(|p| p.0.clone()).collect();
                    combo.tips = paces.iter().map(|p| p.1.clone()).collect();
                    combo.selected = selected;
                    combo.tip_drawable = tip_drawable;
                }
            });
        }
        // (The rider's picture.)
        if let (Some(t), Some(portraits)) = (s.id(THUMBNAIL), portraits.as_deref_mut()) {
            let h = portraits.portrait(&mut images, me);
            commands.entity(t).despawn_children();
            commands.spawn((ImageNode::new(h), crate::portraits::PortraitOf(me), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE, ChildOf(t)));
        }
        // The standings' headers: the game's column buttons, its words.
        if let (Some(t), Some(col)) = (s.id(TABLE), ui.layout("ColumnControl").cloned()) {
            commands.entity(t).despawn_children();
            let mut x = 0.0;
            for (k, (key, or)) in COLUMN_HEADERS.iter().enumerate() {
                let mut h = col.clone();
                h.place = s3bake::ui::UiPlace::Fixed;
                h.area = [x, 0.0, x + COLUMNS[k], HEADER];
                h.caption = words(Some(&*ui), &format!("Ui/Caption/Hud/RacingPanel:{key}"), or);
                let b = ui.spawn_under(&mut commands, &mut images, &mut fonts, &h, t);
                if let Some(r) = b.root {
                    commands.entity(r).insert(Pickable::IGNORE);
                }
                x += COLUMNS[k];
            }
        }
        // (The bar's fill in its groove: the layout leaves its clip window standing above it.)
        if let Some(p) = s.id(PROGRESS) {
            for (id, top) in [(BAR_CLIP, 0.0), (BAR_FILL, 0.0)] {
                if let Some(w) = s.within(p, id) {
                    commands.entity(w).queue(move |mut ew: EntityWorldMut| {
                        if let Some(mut n) = ew.get_mut::<Node>() {
                            n.top = Val::Px(top);
                            n.height = Val::Px(BAR_HEIGHT);
                        }
                    });
                }
            }
        }
        for id in [TITLE, DESCRIPTION] {
            if let Some(t) = s.text(id) {
                commands.entity(t).insert(Text::new(""));
            }
        }
        state.s = Some(s);
        state.sim = Some(me);
    }
    let Some(s) = state.s.clone() else { return };
    // The pace picked.
    if let Some(combo) = s.id(PACE).and_then(|p| combos.get_mut(p).ok())
        && !combo.open
        && combo.selected != c.pace
        && combo.selected < 3
        && !c.done
    {
        c.pace = combo.selected;
        info!("equestrian: pace {}", PACES[c.pace]);
    }
    // How far along.
    let along = ((clock.minutes - c.start) / LENGTH_MINUTES).clamp(0.0, 1.0) as f32;

    if let Some(mut b) = s.id(PROGRESS).and_then(|p| bars.get_mut(p).ok())
        && (b.value - along).abs() > 1e-3
    {
        b.value = along;
    }
    let key = format!("{} {} {} {} {:?}", c.name, c.place, c.legs, c.done, c.others);
    if state.shown.as_ref() == Some(&key) {
        return;
    }
    state.shown = Some(key);
    if let Some(t) = s.text(TITLE) {
        commands.entity(t).insert(Text::new(c.name.clone()));
    }
    if let Some(t) = s.text(DESCRIPTION) {
        let level = words(Some(&*ui), &format!("Gameplay/Objects/RabbitHoles/EquestrianCenter:{}", LEVELS[c.level]), LEVELS[c.level]);
        let kind = words(Some(&*ui), &format!("Gameplay/Objects/RabbitHoles/EquestrianCenter:{}", KINDS[c.kind]), KIND_WORDS[c.kind]);
        let d = words(Some(&*ui), &format!("{ENTER_KEY}Competition"), "{0.String} {1.String}").replace("{0.String}", &level).replace("{1.String}", &kind);
        commands.entity(t).insert(Text::new(d));
    }
    // The standings, afresh: the others in order, the rider at their place.
    if let Some(r) = state.rows.take() {
        commands.entity(r).try_despawn();
    }
    let Some(table) = s.id(TABLE) else { return };
    let (font, line) = ui.text_font(&mut fonts, 0);
    let holder = commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(HEADER), width: Val::Px(COLUMNS.iter().sum()), height: Val::Px(ROW * COMPETITORS as f32), flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() },
            Pickable::IGNORE,
            ChildOf(table),
        ))
        .id();
    for (n, (rider, horse, me)) in standings(&c.others, c.place, &sim.full_name(), &horse).into_iter().enumerate() {
        let bg = if me { crate::layout::color(LIT) } else if n % 2 == 1 { crate::layout::color(ALTERNATE) } else { Color::NONE };
        let row = commands.spawn((Node { height: Val::Px(ROW), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() }, BackgroundColor(bg), Pickable::IGNORE, ChildOf(holder))).id();
        let place = words(Some(&*ui), &format!("UI/Caption/HudCompetitionPanel/Place:Position{}", n + 1), &ordinal(n as u8 + 1));
        for (w, text) in COLUMNS.iter().zip([place, horse, rider]) {
            let cell = commands.spawn((Node { width: Val::Px(*w), padding: UiRect::left(Val::Px(8.0)), overflow: Overflow::clip(), ..default() }, Pickable::IGNORE, ChildOf(row))).id();
            commands.spawn((Text::new(text), font.clone(), line, TextColor(Color::WHITE), Pickable::IGNORE, ChildOf(cell)));
        }
    }
    state.rows = Some(holder);
}

/// The standings' rows (rider, horse, whether it's the Sim's), as the panel's
/// `RepopulateStandingsTable`: the others in order, the Sim's row at their place.
pub fn standings(others: &[(String, String)], place: u8, rider: &str, horse: &str) -> Vec<(String, String, bool)> {
    let mut out: Vec<(String, String, bool)> = others.iter().map(|(r, h)| (r.clone(), h.clone(), false)).collect();
    let at = usize::from(place.max(1) - 1).min(out.len());
    out.insert(at, (rider.to_string(), horse.to_string(), true));
    out
}

/// `COMPETE_TEST=<horse>:<kind>:<level>` (tests; kinds and levels 0 to 2): ten seconds in, the
/// selected Sim rides the household's horse of that name and enters that competition (the clock's
/// to be at a competition's hours: `--hour 18`); `COMPETE_SKILL=<riding>,<racing>,<jumping>` sets
/// the two's skills, `COMPETE_PACE=<0..2>` the pace; `COMPETE_PIE=riding|foot` the camera put on
/// the center instead, up on the horse or not, for its menu to be clicked up (`CLICK_AT`);
/// `COMPETE_SELL=1` the horse ridden off to be sold.
#[allow(clippy::type_complexity)]
fn compete_test(
    mut commands: Commands,
    time: Res<Time>,
    mut stage: Local<(u8, f32)>,
    mut sel: Query<(&mut crate::interact::ActionQueue, Option<&crate::riding::Mounted>, &mut Skills, Option<&mut Competing>), With<Selected>>,
    horses: Query<(Entity, &Pet, &Transform), With<crate::pets::HomePet>>,
    world: Res<crate::loading::CurrentWorld>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
) {
    let Ok(v) = std::env::var("COMPETE_TEST") else { return };
    let parts: Vec<&str> = v.split(':').collect();
    let num = |i: usize| parts.get(i).and_then(|p| p.parse::<u8>().ok()).unwrap_or(0);
    let now = time.elapsed_secs();
    let Ok((mut q, mounted, mut skills, competing)) = sel.single_mut() else { return };
    let Some((h, _, htf)) = horses.iter().find(|(_, p, _)| p.name.eq_ignore_ascii_case(parts[0])) else { return };
    match stage.0 {
        0 if now > 10.0 => {
            if let Some(s) = std::env::var("COMPETE_SKILL").ok() {
                let l: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if let Some(r) = l.first() {
                    skills.0.insert("Riding", *r);
                }
                commands.entity(h).insert(crate::horsejumps::HorseSkills { racing: l.get(1).copied().unwrap_or(0.0), jumping: l.get(2).copied().unwrap_or(0.0), ..default() });
            }
            if std::env::var("COMPETE_PIE").is_ok_and(|p| p == "foot") {
                *stage = (1, now);
                return;
            }
            q.push_player(crate::interact::Action::new("Ride", ActionKind::Ride { horse: h, slot: crate::riding::mount_slot(htf), unsaddle: false }, false));
            *stage = (1, now);
        }
        // (COMPETE_PIE=1: the camera on the center, for its menu to be clicked up instead.)
        1 if let Ok(pie) = std::env::var("COMPETE_PIE")
            && now - stage.1 > 3.0
            && (pie == "foot" || mounted.is_some_and(|m| m.idle())) =>
        {
            if let (Some(lot), Ok(mut cam)) = ((0..world.data.lots.len()).find(|l| is_center(&world.data, *l)), cams.single_mut()) {
                let at = crate::home::lot_center(&world.data.lots[lot]);
                info!("compete test: the camera on the center, at {at}");
                cam.look_at(at);
                cam.distance = 40.0;
            }
            *stage = (9, now);
        }
        1 if now - stage.1 > 3.0 && mounted.is_some_and(|m| m.idle()) => {
            let Some(lot) = (0..world.data.lots.len()).find(|l| is_center(&world.data, *l)) else {
                warn!("compete test: no Equestrian Center here");
                *stage = (9, now);
                return;
            };
            info!("compete test: entering {} {}", KINDS[usize::from(num(1)).min(2)], LEVELS[usize::from(num(2)).min(2)]);
            // (COMPETE_SELL=1: the horse taken to be sold instead.)
            let task = if std::env::var_os("COMPETE_SELL").is_some() { CenterTask::Sell } else { CenterTask::Compete { kind: num(1), level: num(2) } };
            q.push_player(crate::interact::Action::new("Compete", ActionKind::Equestrian { lot, task }, false));
            *stage = (2, now);
        }
        2 => {
            if let (Some(mut c), Some(p)) = (competing, std::env::var("COMPETE_PACE").ok().and_then(|p| p.parse::<usize>().ok())) {
                c.pace = p.min(2);
                *stage = (3, now);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use s3bake::gamedata::{EquestrianPlace, EquestrianSkillRow};

    fn tables() -> EquestrianTables {
        let skills = [(0, 1, 1, 1, 1.0), (1, 1, 2, 2, 1.0), (2, 1, 4, 4, 1.1), (3, 1, 6, 6, 1.1), (4, 2, 8, 8, 1.2), (5, 2, 10, 10, 1.2), (6, 2, 12, 12, 1.3), (7, 3, 15, 15, 1.3), (8, 3, 18, 18, 1.4), (9, 4, 21, 21, 1.4), (10, 4, 25, 25, 1.5)]
            .map(|(level, riding, racing, jumping, tone)| EquestrianSkillRow { level, riding, racing, jumping, tone })
            .to_vec();
        let racing_beginner = [(1, 150, 600), (2, 114, 400), (3, 100, 200), (4, 86, 100), (5, 78, 75), (6, 66, 50), (7, 58, 0), (8, 50, 0), (9, 42, 0), (10, 34, 0), (11, 18, 0), (12, 0, 0)];
        let scoring = racing_beginner.map(|(place, score, money)| EquestrianPlace { kind: 0, level: 0, place, score, money }).to_vec();
        EquestrianTables { scoring, skills }
    }

    #[test]
    fn entries_are_open_in_the_competitions_hours() {
        assert_eq!(doors(2, 17.9), None);
        assert_eq!(doors(2, 18.0), Some(true));
        assert_eq!(doors(2, 21.9), Some(true));
        assert_eq!(doors(2, 22.5), Some(false), "entries close two hours before the end");
        assert_eq!(doors(SUNDAY, 13.0), Some(true), "Sunday's matinee");
        assert_eq!(doors(5, 13.0), None);
        assert_eq!(next_start(5, 19.0), (12.0, true), "Saturday night: Sunday's matinee next");
        assert_eq!(next_start(2, 9.0), (18.0, false));
    }

    #[test]
    fn a_leg_scores_as_the_games_does() {
        let t = tables();
        // (Riding 5, a Racing 10 horse at a race, a steady pace that comes off: 2 + 25 + 1.)
        assert_eq!(leg_score(&t, 0, 5, 10, 0, 0, true, &[]), 28);
        // (Cross-country: half each of Racing's and Jumping's, rounded down; going for broke and
        // missing.)
        assert_eq!(leg_score(&t, 2, 5, 5, 5, 2, false, &[]), 2 + 5 + 5 - 16);
        assert_eq!(trait_score(2, &["FastPet".into(), "HatesJumpingPet".into()]), 0);
        assert_eq!(trait_score(0, &["FastPet".into()]), 2);
        assert_eq!(trait_score(1, &["LazyHorse".into()]), 0);
        assert!((pace_chance(&t, 1, 10) - 0.75).abs() < 1e-6, "a master rider lifts Take Risks to three in four");
    }

    #[test]
    fn a_score_earns_its_place_and_prize() {
        let t = tables();
        assert_eq!(place(&t, 0, 0, 160), (1, 600));
        assert_eq!(place(&t, 0, 0, 113), (3, 200));
        assert_eq!(place(&t, 0, 0, 5), (12, 0));
        assert_eq!((finish(1), finish(4), finish(7), finish(12)), (2, 1, 0, 0));
        // (A racer's projected: 40 in four legs is 160 over the race.)
        assert_eq!(standing(0, 40, 4, 0), 160);
        assert_eq!(standing(1, 40, 4, 3), 43);
        assert_eq!(mood_bonus(80.0, 60.0), 7);
    }

    #[test]
    fn a_horse_is_worth_what_the_game_says() {
        // (Untrained, no traits that matter, the least besides: four hundred; to buy, a fifth more.)
        assert_eq!(horse_cost(0, 0, &[], 0.0, false), 400);
        assert_eq!(horse_cost(0, 0, &[], 0.0, true), 480);
        // (Racing 5 and Jumping 2, a good trait and a bad: 4200 + 200 - 100 + 600.)
        assert_eq!(horse_cost(5, 2, &[10, -10, 0], 1.0, false), 4900);
    }

    #[test]
    fn the_standings_put_the_rider_at_their_place() {
        let others: Vec<(String, String)> = (0..11).map(|i| (format!("R{i}"), format!("H{i}"))).collect();
        let s = standings(&others, 3, "Johnny", "Freebird");
        assert_eq!(s.len(), 12);
        assert_eq!(s[2], ("Johnny".to_string(), "Freebird".to_string(), true));
        assert_eq!(standings(&others, 12, "J", "F")[11].2, true);
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(12), "12th");
        assert_eq!(ordinal(22), "22nd");
    }
}
