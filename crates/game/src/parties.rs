//! Parties, as the game's (`Phone.CallThrowParty`, `HouseParty` and its kinds). The host calls
//! round from the phone and picks, in the game's party picker (`partypicker`), what kind of
//! party (a house party; a wedding, with one of the household engaged; a birthday party; a
//! funeral, with someone buried on the lot), what to wear, when it starts and who's asked.
//!
//! Friends (and romances) always come; others as they like the household (a quarter of them at
//! -25 liking, all of them well short of friendship), now and then bringing a friend along.
//! Guests turn up in the clothes asked for and stay two to four hours (the better they get on
//! with the household, the longer), longer still in a good mood, less in a bad one; the tired
//! and the miserable go home, and children at ten. As each leaves, their mood counts towards the
//! party's, and their liking for the host goes with it. Once the last has gone the party is
//! judged by the guests' average mood: over 65 a great party, under 10 a lame one. A birthday
//! party where nobody had their birthday, or a wedding where nobody married, is a failure;
//! a funeral is simply over. A wedding's guests watch the vows, then clap, cry or throw rice.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, GoingHome, Invited, Notifications, OffLot, Phase, Visitor};
use crate::life::{LifeEvent, LifeEventKind, Mood, MoodletKind, Moodlets, Trait};
use crate::sim::{Age, HouseholdMember, Sim};
use crate::simbody::OutfitKind;
use crate::social::{FRIEND_THRESHOLD, RelStatus, Relationships};

pub struct PartiesPlugin;

impl Plugin for PartiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartyPicker>().add_systems(Update, (phone_party, crate::partypicker::picker, run_party).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The kinds of party (the game's `PartyPickerDialog.PartyType`, those of the base game).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PartyKind {
    #[default]
    House,
    Wedding,
    Birthday,
    Funeral,
}

impl PartyKind {
    /// Its name in the picker (`Ui/Caption/Party:...`), and the words without the game's.
    pub fn caption(self) -> (&'static str, &'static str) {
        match self {
            PartyKind::House => ("Ui/Caption/Party:House", "House Party"),
            PartyKind::Wedding => ("Ui/Caption/Party:Wedding", "Wedding Party"),
            PartyKind::Birthday => ("Ui/Caption/Party:Birthday", "Birthday Party"),
            PartyKind::Funeral => ("Ui/Caption/Party:Funeral", "Funeral"),
        }
    }

    /// What the host's told once it's arranged (`Gameplay/Situations/Party:...`): its title and
    /// its start, the hour in it (the words without the game's).
    fn announced(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            PartyKind::House => ("GenericTitle", "HouseStart", "Party", "The party will start at {0.HourOfDay}"),
            PartyKind::Wedding => ("WeddingTitle", "WeddingStart", "Wedding", "The Wedding will start at {0.HourOfDay}."),
            PartyKind::Birthday => ("BirthdayTitle", "BirthdayStart", "Birthday Party!", "The Birthday Party will start at {0.HourOfDay}."),
            PartyKind::Funeral => ("FuneralTitle", "FuneralStart", "Funeral", "The Funeral will start at {0.HourOfDay}."),
        }
    }
}

/// The dress code (the game's `ClothingType`: casual, formal, swimwear).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dress {
    Casual,
    Formal,
    Swimwear,
}

impl Dress {
    pub const ALL: [Dress; 3] = [Dress::Casual, Dress::Formal, Dress::Swimwear];

    /// Its name in the picker, and the words without the game's.
    pub fn caption(self) -> (&'static str, &'static str) {
        match self {
            Dress::Casual => ("Ui/Caption/Party:Casual", "Casual Attire"),
            Dress::Formal => ("Ui/Caption/Party:FormalAttire", "Formal Attire"),
            Dress::Swimwear => ("Ui/Caption/Party:Swim", "Swimwear"),
        }
    }

    /// What the guests wear (none: their everyday clothes).
    fn outfit(self) -> Option<OutfitKind> {
        match self {
            Dress::Casual => None,
            Dress::Formal => Some(OutfitKind::Formal),
            Dress::Swimwear => Some(OutfitKind::Swimwear),
        }
    }
}

/// Someone who could be asked (the game's `SimPickerInfo`).
#[derive(Clone, Debug)]
pub struct Candidate {
    pub sim: Entity,
    /// "Last, First" (the game's `LastFirstName`).
    pub name: String,
    /// The household's liking for them (the best of it), and what they are to the host ("Iris's
    /// Acquaintance", the game's `RelationshipWith`).
    pub liking: f32,
    pub relation: String,
    pub friend: bool,
    pub coworker: bool,
}

/// What's chosen in the picker.
#[derive(Clone, Debug)]
pub struct Choice {
    pub kind: PartyKind,
    pub dress: Dress,
    /// The hour it starts (0-23).
    pub hour: u32,
    pub guests: Vec<Entity>,
}

/// A party being arranged on the phone: what's to be chosen from, for the picker.
#[derive(Clone, Debug)]
pub struct Request {
    pub host: Entity,
    pub kinds: Vec<PartyKind>,
    pub candidates: Vec<Candidate>,
}

/// The party picker: open for a host's call, and what was chosen (none: called off).
#[derive(Resource, Default)]
pub struct PartyPicker {
    pub open: Option<Request>,
    pub done: Option<(Entity, Option<Choice>)>,
}

/// Most guests asked (the game's `kDefaultMaxAllowed`).
pub const MAX_GUESTS: usize = 10;
/// The hours parties may not start in (`kMinTimeNotPartyAllowed`, `kMaxTimeNotPartyAllowed`):
/// ten at night to eight in the morning.
const TOO_LATE: u32 = 22;
const TOO_EARLY: u32 = 8;
/// Notice the guests need (hours).
const NOTICE: f32 = 1.0;
/// The liking the least liked are still asked at, and the liking at which all of them come
/// (the game's `kLTRMinToInvite`, the Friend row's liking less one), and how likely they're to
/// come at each (`kChanceToComeAtMinLTR`, `kChanceToComeAtMaxLTR`, percent).
pub const MIN_LIKING: f32 = -25.0;
const SURE_LIKING: f32 = FRIEND_THRESHOLD - 1.0;
const CHANCE_AT_MIN: f32 = 25.0;
const CHANCE_AT_SURE: f32 = 250.0;
/// The chance a guest brings a friend (`PercentageChanceOfBringingAFriend`).
const BRING_A_FRIEND: f64 = 0.3;
/// How long guests stay (`MinPartyTime`, hours, and `RelToTimeMod` an hour per liking point
/// above -100), how much longer for their mood at each look (`MoodToTimeMod`, minutes a mood
/// point) and how often they look (`MinutesPerLeavingCheck`).
const MIN_STAY: f64 = 2.0;
const STAY_PER_LIKING: f64 = 0.01;
const STAY_PER_MOOD: f64 = 0.045;
const LEAVING_CHECK: f64 = 15.0;
/// The hour children go home (`HourAtWhichChildrenGoHome`).
const CHILDREN_HOME: u32 = 22;
/// Mood that sends a guest home (the game's strong negative mood).
const MISERABLE: f32 = -50.0;
/// The liking a guest's mood is worth as they leave (`RatioOfMoodToLikingChangeWhenLeavingParty`).
const MOOD_TO_LIKING: f32 = 0.15;
/// The guests' average mood for a great party, and under which it's lame
/// (`kThrewGreatPartyBuffMoodRequirement`, `kThrewLamePartyBuffMoodRequirement`).
const GREAT: f32 = 65.0;
const LAME: f32 = 10.0;
/// How long the host prepares (`PreparationTime`, hours): told this long before.
const PREPARE: f64 = 2.0;
/// How long the guests are waited for before it's a no-show (game minutes after the start).
const NO_SHOW_AFTER: f64 = 120.0;
/// Range in which the wedding's guests watch the vows.
const WEDDING_RANGE: f32 = 15.0;

/// What's wrong with a choice (the game's `UpdateOkButton`), as the key of its words.
pub fn problem(kind: PartyKind, dress: Option<Dress>, hour: Option<u32>, guests: usize, now_hour: f32) -> Option<&'static str> {
    let _ = kind;
    if dress.is_none() {
        return Some("Ui/Caption/Party:InvalidClothingType");
    }
    if guests < 1 {
        return Some("Ui/Caption/Party:InvalidNumberOfGuests");
    }
    let Some(hour) = hour else { return Some("Ui/Caption/Party:InvalidTime") };
    if hour >= TOO_LATE || hour <= TOO_EARLY {
        return Some(if hour <= TOO_EARLY { "Ui/Caption/Party:InvalidTime" } else { "Ui/Caption/Party:InvalidTimeTooLate" });
    }
    if hours_until(hour, now_hour) < NOTICE {
        return Some("Ui/Caption/Party:NotEnoughNotice");
    }
    None
}

/// Hours from now to the next time it's this hour.
pub fn hours_until(hour: u32, now_hour: f32) -> f32 {
    let h = hour as f32;
    if now_hour > h { 24.0 - (now_hour - h) } else { h - now_hour }
}

/// When a party at this hour starts (game minutes): the next time it's that hour, at least an
/// hour off.
pub fn start_at(now: f64, hour: u32) -> f64 {
    let now_hour = ((now / 60.0) % 24.0) as f32;
    let mut h = hours_until(hour, now_hour) as f64;
    if h < NOTICE as f64 {
        h += 24.0;
    }
    now + h * 60.0
}

/// The chance someone asked comes (the game's `WillSimComeToParty`): friends and romances, and
/// everyone a Legendary Host or a Party Animal asks, always; the rest by how well they're liked.
pub fn coming_chance(liking: f32, friend: bool, sure_host: bool) -> f32 {
    if sure_host || friend {
        return 1.0;
    }
    let x = liking.clamp(MIN_LIKING, SURE_LIKING);
    let t = (x - MIN_LIKING) / (SURE_LIKING - MIN_LIKING);
    ((CHANCE_AT_MIN + (CHANCE_AT_SURE - CHANCE_AT_MIN) * t) / 100.0).min(1.0)
}

/// How long a guest stays (hours): two, and up to two more the better they get on.
pub fn stay_hours(liking: f32) -> f64 {
    MIN_STAY + (liking as f64 + 100.0) * STAY_PER_LIKING
}

/// How a party went (the game's `PartyOutcomes`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Bad,
    Normal,
    Good,
}

/// The party judged by the guests' moods as they left (the game's `GetPartyOutcome`).
pub fn outcome(kind: PartyKind, moods: f32, guests: u32, legendary: bool, special_failed: bool) -> Outcome {
    if kind == PartyKind::Funeral {
        return Outcome::Normal;
    }
    if special_failed {
        return Outcome::Bad;
    }
    if guests == 0 {
        return Outcome::Normal;
    }
    let avg = moods / guests as f32;
    if avg > GREAT || legendary {
        Outcome::Good
    } else if avg < LAME {
        Outcome::Bad
    } else {
        Outcome::Normal
    }
}

/// A party arranged, and then under way.
#[derive(Resource)]
pub struct Party {
    pub host: Entity,
    pub kind: PartyKind,
    pub dress: Dress,
    pub start: f64,
    /// Those coming.
    pub guests: Vec<Entity>,
    /// Those here, and when each is to leave (game minutes).
    pub here: HashMap<Entity, f64>,
    /// Those who've left, and their moods as they did.
    pub gone: Vec<Entity>,
    pub left: u32,
    pub moods: f32,
    /// The first guest's arrived.
    pub begun: bool,
    /// The host's been told to get ready.
    pub told: bool,
    /// Someone had their birthday at it; someone married at it.
    pub succeeded: bool,
    /// The household's engaged, to marry at it, and whether the day's begun for them.
    pub couples: Vec<(Entity, Entity)>,
    pub wedding_day: bool,
    /// The vows being watched.
    pub vows: bool,
    last_check: f64,
}

impl Party {
    /// Whether it's under way (the first guest's come).
    pub fn on(&self) -> bool {
        self.begun
    }
}

/// The game's words for a key (the words given where it hasn't them), a Sim's in them.
fn words(ui: Option<&crate::layout::UiAssets>, key: &str, plain: &str, sim: Option<&Sim>) -> String {
    let t = ui.and_then(|u| u.localize(key)).unwrap_or_else(|| plain.to_string());
    match sim {
        Some(s) => crate::layout::gendered(&t, s.female).replace("{0.SimFirstName}", &s.first).replace("{0.SimName}", &s.full_name()),
        None => t,
    }
}

/// The party kinds this household could throw (the game's: a house party always, a wedding with
/// one of them engaged, a birthday party, a funeral with someone buried here).
pub fn kinds(engaged: bool, grave: bool) -> Vec<PartyKind> {
    let mut k = vec![PartyKind::House];
    if engaged {
        k.push(PartyKind::Wedding);
    }
    k.push(PartyKind::Birthday);
    if grave {
        k.push(PartyKind::Funeral);
    }
    k
}

/// A call to throw a party: its picker put up for the host (its choices: who the household
/// knows, the kinds they could throw), then, chosen, the party arranged and the call over.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn phone_party(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut picker: ResMut<PartyPicker>,
    auto: Option<Res<crate::dialog::AutoAnswer>>,
    ui: Option<Res<crate::layout::UiAssets>>,
    party: Option<Res<Party>>,
    mut notes: ResMut<Notifications>,
    mut hosts: Query<(Entity, &Sim, &mut ActionQueue, &Relationships, Option<&crate::careers::Job>, Option<&crate::wishes::Wishes>), With<HouseholdMember>>,
    others: Query<(Entity, &Sim, Has<crate::town::Townie>, Option<&Relationships>), (Or<(With<OffLot>, With<crate::town::Townie>)>, Without<Invited>, Without<HouseholdMember>)>,
    graves: Query<(), With<crate::ghosts::Grave>>,
) {
    let calling: Vec<Entity> = hosts.iter().filter(|q| q.2.0.front().is_some_and(|a| matches!(a.kind, ActionKind::ThrowParty) && matches!(a.phase, Phase::Running(_)) && !a.cancel)).map(|q| q.0).collect();
    // (A call put down: its picker goes.)
    if picker.open.as_ref().is_some_and(|r| !calling.contains(&r.host)) {
        picker.open = None;
    }
    for host in calling {
        // Chosen (or called off): the call's over, and the party's arranged.
        if picker.done.as_ref().is_some_and(|d| d.0 == host) {
            let (_, choice) = picker.done.take().unwrap();
            if let Ok(mut q) = hosts.get_mut(host)
                && let Some(a) = q.2.0.front_mut()
            {
                a.cancel = true;
            }
            if let Some(c) = choice {
                arrange(&mut commands, &clock, &hosts, &others, ui.as_deref(), &mut notes, host, c);
            }
            continue;
        }
        if picker.open.is_some() {
            continue;
        }
        let Ok((_, sim, _, _, job, _)) = hosts.get(host) else { continue };
        let first = sim.first.clone();
        let busy = party.is_some();
        // Who the household knows, liked well enough to ask.
        let mut candidates: Vec<Candidate> = Vec::new();
        for (e, s, _, _) in &others {
            if s.age.is_little() {
                continue;
            }
            let known: Vec<f32> = hosts.iter().filter_map(|h| h.3.0.get(&e).map(|r| r.friendship)).collect();
            let Some(liking) = known.iter().copied().reduce(f32::max) else { continue };
            if liking < MIN_LIKING {
                continue;
            }
            let rel = hosts.get(host).map(|h| h.3.get(e)).unwrap_or_default();
            let state = crate::conversation::relationship_state(rel.friendship, rel.status, rel.romantic, rel.enemy);
            let relation = ui.as_deref().and_then(|u| u.localize("Ui/Tooltip/Party:RelationshipWith")).unwrap_or_else(|| "{0.String}'s {1.String}".into()).replacen("{0.String}", &sim.first, 1).replacen("{1.String}", state, 1);
            candidates.push(Candidate {
                sim: e,
                name: format!("{}, {}", s.last, s.first),
                liking,
                relation,
                friend: liking >= FRIEND_THRESHOLD || rel.is_romance(),
                coworker: job.is_some_and(|j| j.coworkers.contains(&s.id) || j.boss == Some(s.id)),
            });
        }
        candidates.sort_by(|a, b| b.liking.total_cmp(&a.liking).then_with(|| a.name.cmp(&b.name)));
        if busy || candidates.is_empty() {
            if let Ok(mut q) = hosts.get_mut(host)
                && let Some(a) = q.2.0.front_mut()
            {
                a.cancel = true;
            }
            notes.push(if busy { format!("{first} already has a party arranged.") } else { "There's nobody to invite yet — meet some Sims first!".to_string() });
            continue;
        }
        let engaged = hosts.iter().any(|h| h.3.partner().is_some_and(|p| p.1 == RelStatus::Engaged));
        let kinds = kinds(engaged, !graves.is_empty());
        // (A test answering for itself: the kind asked for, casual, two hours on, the best liked.)
        if auto.is_some() {
            let kind = std::env::var("PARTY").ok().and_then(|k| kinds.iter().copied().find(|x| format!("{x:?}").eq_ignore_ascii_case(&k))).unwrap_or(PartyKind::House);
            let hour = (((clock.minutes / 60.0) as u32) + 2) % 24;
            let guests = candidates.iter().take(6).map(|c| c.sim).collect();
            picker.done = Some((host, Some(Choice { kind, dress: Dress::Casual, hour: hour.clamp(TOO_EARLY + 1, TOO_LATE - 1), guests })));
            continue;
        }
        picker.open = Some(Request { host, kinds, candidates });
    }
}

/// The party arranged: who's coming (and who they bring), the host told when it starts.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn arrange(
    commands: &mut Commands,
    clock: &GameClock,
    hosts: &Query<(Entity, &Sim, &mut ActionQueue, &Relationships, Option<&crate::careers::Job>, Option<&crate::wishes::Wishes>), With<HouseholdMember>>,
    others: &Query<(Entity, &Sim, Has<crate::town::Townie>, Option<&Relationships>), (Or<(With<OffLot>, With<crate::town::Townie>)>, Without<Invited>, Without<HouseholdMember>)>,
    ui: Option<&crate::layout::UiAssets>,
    notes: &mut Notifications,
    host: Entity,
    c: Choice,
) {
    let Ok((_, hsim, _, hrels, _, wishes)) = hosts.get(host) else { return };
    let start = start_at(clock.minutes, c.hour);
    let mut rng = rand::rng();
    let sure_host = crate::wishes::has(wishes, "LegendaryHost") || hsim.traits.contains(&Trait::PartyAnimal);
    let best = |e: Entity| hosts.iter().filter_map(|h| h.3.0.get(&e).map(|r| r.friendship)).reduce(f32::max).unwrap_or(0.0);
    let mut coming: Vec<Entity> = Vec::new();
    for &g in &c.guests {
        let friend = best(g) >= FRIEND_THRESHOLD || hrels.get(g).is_romance();
        if !rng.random_bool(coming_chance(best(g), friend, sure_host) as f64) {
            continue;
        }
        coming.push(g);
        // (Now and then a friend of theirs along too.)
        if rng.random_bool(BRING_A_FRIEND)
            && let Ok((_, gs, _, Some(grels))) = others.get(g)
        {
            let friend = others.iter().find(|o| o.0 != g && !c.guests.contains(&o.0) && !coming.contains(&o.0) && !o.1.age.is_little() && (o.1.age == Age::Child) == (gs.age == Age::Child) && grels.friendship(o.0) >= FRIEND_THRESHOLD);
            if let Some(f) = friend {
                coming.push(f.0);
            }
        }
    }
    // (A wedding's: the household's engaged and their fiancé(e)s, who always come.)
    let mut couples = Vec::new();
    if c.kind == PartyKind::Wedding {
        for (e, _, _, rels, ..) in hosts.iter() {
            if let Some((p, RelStatus::Engaged)) = rels.partner() {
                if !couples.iter().any(|&(a, b)| (a, b) == (p, e)) {
                    couples.push((e, p));
                }
                if others.contains(p) && !coming.contains(&p) {
                    coming.push(p);
                }
            }
        }
    }
    for &g in &coming {
        if others.get(g).is_ok_and(|o| o.2) {
            commands.entity(g).remove::<(crate::town::Townie, crate::nav::PathFollow)>().insert(OffLot);
        }
        commands.entity(g).insert(Invited { arrive_at: start + rng.random_range(0.0..40.0) });
    }
    let (_, start_key, _, plain_start) = c.kind.announced();
    let when = crate::interact::hour_label(c.hour as f32);
    notes.push(words(ui, &format!("Gameplay/Situations/Party:{start_key}"), plain_start, None).replace("{0.HourOfDay}", &when));
    info!("party: {} throws a {:?} at {when} ({} of {} asked are coming)", hsim.first, c.kind, coming.len(), c.guests.len());
    commands.insert_resource(Party {
        host,
        kind: c.kind,
        dress: c.dress,
        start,
        guests: coming,
        here: HashMap::new(),
        gone: Vec::new(),
        left: 0,
        moods: 0.0,
        begun: false,
        told: false,
        succeeded: false,
        couples,
        wedding_day: false,
        vows: false,
        last_check: 0.0,
    });
}

/// A party under way: the host told to get ready, guests dressing for it as they come, staying
/// as long as they're enjoying themselves, a wedding's vows watched, and once the last guest's
/// gone, the party judged.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run_party(
    mut commands: Commands,
    clock: Res<GameClock>,
    party: Option<ResMut<Party>>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<Notifications>,
    mut life: MessageReader<LifeEvent>,
    mut members: Query<(Entity, &Sim, &mut Moodlets, &mut Relationships, Option<&crate::wishes::Wishes>, &ActionQueue, &Transform), With<HouseholdMember>>,
    mut guests: Query<(&Sim, Option<&mut Visitor>, Option<&Mood>, &mut Moodlets, &mut Relationships, Has<GoingHome>, Has<OffLot>, Has<Invited>, Option<&ActionQueue>, &Transform), Without<HouseholdMember>>,
    mut tested: Local<bool>,
    mut dreams: MessageWriter<crate::dreams::DreamEvent>,
) {
    let Some(mut p) = party else {
        life.clear();
        *tested = false;
        return;
    };
    let now = clock.minutes;
    let p = &mut *p;
    // Birthdays and marriages at it.
    for ev in life.read() {
        let at_it = p.begun && (members.contains(ev.sim) || p.here.contains_key(&ev.sim));
        match ev.kind {
            LifeEventKind::Birthday if at_it && p.kind == PartyKind::Birthday => p.succeeded = true,
            LifeEventKind::Married if at_it => {
                if p.kind == PartyKind::Wedding {
                    p.succeeded = true;
                }
                // (The guests clap, cry, beam or throw rice: the game's wedding reactions.)
                let at = members.get(ev.sim).map(|m| m.6.translation).or_else(|_| guests.get(ev.sim).map(|g| g.9.translation)).ok();
                let mut rng = rand::rng();
                let couple: Vec<Entity> = p.couples.iter().flat_map(|&(a, b)| [a, b]).collect();
                for (&g, _) in p.here.iter().filter(|(g, _)| !couple.contains(g)) {
                    if let (Some(at), Ok(gq)) = (at, guests.get(g))
                        && gq.9.translation.distance(at) < WEDDING_RANGE
                    {
                        let kind = ["WeddingClap", "WeddingCry", "WeddingHappyLook", "WeddingThrowOut"][rng.random_range(0..4)];
                        commands.entity(g).insert(crate::reactions::Pending::new(&[kind], Some(at), now + 30.0)).queue(stop_to_watch);
                    }
                }
                p.vows = false;
            }
            _ => {}
        }
    }
    let host = members.get(p.host).map(|m| m.1.clone()).ok();
    // Get ready.
    if !p.told && now >= p.start - PREPARE * 60.0 {
        p.told = true;
        if p.kind != PartyKind::Funeral {
            notes.push(words(ui.as_deref(), "Gameplay/Situations/HouseParty:GoPrepare", "{0.SimFirstName}'s party is starting soon. A little setup might be in order.", host.as_ref()));
        }
    }
    // The wedding day: the couple's moodlet, from the day's start.
    if p.kind == PartyKind::Wedding && !p.wedding_day && (now / 1440.0).floor() >= (p.start / 1440.0).floor() {
        p.wedding_day = true;
        for &(a, b) in &p.couples.clone() {
            for e in [a, b] {
                if let Ok(mut m) = members.get_mut(e) {
                    m.2.add(MoodletKind::WeddingDay, now);
                } else if let Ok(mut g) = guests.get_mut(e) {
                    g.3.add(MoodletKind::WeddingDay, now);
                }
            }
        }
    }
    // The vows: those at the wedding gather to watch the couple marry.
    if p.kind == PartyKind::Wedding && p.begun && !p.vows {
        let marrying = members.iter().find(|m| m.5.0.front().is_some_and(|a| matches!(a.phase, Phase::Running(_)) && matches!(a.kind, ActionKind::Social { social, .. } if crate::social::social(social).effect == crate::social::SocialEffect::Marry)));
        if let Some(m) = marrying {
            p.vows = true;
            let at = m.6.translation;
            let couple: Vec<Entity> = p.couples.iter().flat_map(|&(a, b)| [a, b]).collect();
            for (&g, _) in p.here.iter().filter(|(g, _)| !couple.contains(g)) {
                if guests.get(g).is_ok_and(|q| q.9.translation.distance(at) < WEDDING_RANGE) {
                    commands.entity(g).insert(crate::reactions::Pending::new(&["WeddingWatching"], Some(at), now + 20.0)).queue(stop_to_watch);
                }
            }
        }
    }
    // Guests arriving: dressed for it, staying as long as they get on with the household.
    for &g in &p.guests.clone() {
        if p.here.contains_key(&g) || p.gone.contains(&g) {
            continue;
        }
        let Ok((gs, Some(mut v), _, _, _, false, ..)) = guests.get_mut(g) else { continue };
        let liking = members.iter().filter_map(|m| m.3.0.get(&g).map(|r| r.friendship)).reduce(f32::max).unwrap_or(0.0);
        p.here.insert(g, now + stay_hours(liking) * 60.0);
        debug!("party: {} arrives at {:.0}, staying {:.1} hours", gs.first, now, stay_hours(liking));
        v.leave_at = f64::INFINITY;
        if let Some(o) = p.dress.outfit()
            && crate::dressing::spins(gs)
        {
            crate::dressing::start(&mut commands, g, Some(o), true);
        }
        if !p.begun {
            p.begun = true;
            // (The game's `kThrewParty`, by the party's kind.)
            dreams.write(crate::dreams::DreamEvent { key: Some(format!("{:?}", p.kind)), ..crate::dreams::DreamEvent::new(p.host, "kThrewParty") });
            notes.push(words(ui.as_deref(), "Gameplay/Situations/Party:PartyBeginsTNS", "The first guest has arrived and it's time to start the party! To make it a success, make sure everyone is having a good time.", host.as_ref()));
        }
    }
    // (Tests: PARTY_MARRY, the host marries their fiancé(e) once they've come; PARTY_BIRTHDAY,
    // the host has their birthday once it's begun.)
    if p.begun && !p.succeeded && !*tested {
        if std::env::var("PARTY_BIRTHDAY").is_ok() && p.kind == PartyKind::Birthday {
            commands.entity(p.host).insert(crate::aging::GrowUpNow);
            *tested = true;
        }
        if std::env::var("PARTY_MARRY").is_ok()
            && let Some(&(a, b)) = p.couples.first()
            && p.here.contains_key(&if a == p.host { b } else { a })
            && let Some(si) = crate::social::social_index("Get Married")
        {
            let (me, them) = if members.contains(a) { (a, b) } else { (b, a) };
            commands.entity(me).queue(move |mut w: EntityWorldMut| {
                if let Some(mut q) = w.get_mut::<ActionQueue>() {
                    q.push_player(Action::new("Get Married", ActionKind::Social { target: them, social: si }, false));
                }
            });
            *tested = true;
        }
    }
    // Nobody came.
    if !p.begun && now >= p.start + NO_SHOW_AFTER {
        notes.push(words(ui.as_deref(), "Gameplay/Situations/Party:PartyNoShow", "Looks like no one is coming to the party. How sad...", host.as_ref()));
        end(&mut commands, p, &mut members, &mut guests, now, true);
        return;
    }
    // Every so often each guest sees whether it's time to go.
    if now - p.last_check >= LEAVING_CHECK {
        p.last_check = now;
        let hour = ((now / 60.0) % 24.0) as u32;
        let mut going = Vec::new();
        let mut hosting = Vec::new();
        for (&g, leave) in p.here.iter_mut() {
            let Ok((gs, _, mood, m, _, home, off, ..)) = guests.get(g) else {
                // (Married into the household: a host now, as the game's wedding has it.)
                if members.contains(g) {
                    hosting.push(g);
                } else {
                    going.push((g, 0.0));
                }
                continue;
            };
            // (Their mood: the sum of their moodlets.)
            let mood = Mood(mood.map_or_else(|| m.0.iter().map(|x| x.value as f32).sum(), |x| x.0));
            if home || off {
                debug!("party: {} has gone (home {home}, off {off})", gs.first);
                going.push((g, mood.0));
                continue;
            }
            *leave += mood.0 as f64 * STAY_PER_MOOD;
            let tired = m.has(MoodletKind::Tired) || m.has(MoodletKind::Exhausted);
            let bedtime = gs.age == Age::Child && hour >= CHILDREN_HOME;
            if *leave < now || mood.0 < MISERABLE || tired || bedtime {
                debug!("party: {} goes at {now:.0} (due {:.0}, mood {:.0}, tired {tired}, bedtime {bedtime}; {:?})", gs.first, *leave, mood.0, m.0.iter().map(|x| (x.kind, x.value)).collect::<Vec<_>>());
                going.push((g, mood.0));
            }
        }
        for g in hosting {
            p.here.remove(&g);
            p.gone.push(g);
        }
        for (g, mood) in going {
            p.here.remove(&g);
            p.gone.push(g);
            p.left += 1;
            p.moods += mood;
            // (Their liking for the host goes with how they feel.)
            let change = mood * MOOD_TO_LIKING;
            if let Ok(mut h) = members.get_mut(p.host) {
                h.3.add(g, change, 0.0);
            }
            if let Ok((gs, v, _, mut m, mut rels, ..)) = guests.get_mut(g) {
                rels.add(p.host, change, 0.0);
                if let Some(mut v) = v {
                    v.leave_at = now;
                }
                if gs.traits.contains(&Trait::PartyAnimal) && mood > GREAT {
                    m.add(MoodletKind::AwesomeParty, now);
                }
            }
            debug!("party: a guest leaves in a mood of {mood:.0}");
        }
    }
    // The last guest gone (and nobody still to come): it's over.
    let to_come = p.guests.iter().any(|g| !p.here.contains_key(g) && !p.gone.contains(g) && guests.get(*g).is_ok_and(|q| q.7));
    if p.begun && p.here.is_empty() && !to_come {
        let legendary = members.get(p.host).is_ok_and(|h| crate::wishes::has(h.4, "LegendaryHost"));
        let failed = matches!(p.kind, PartyKind::Birthday | PartyKind::Wedding) && !p.succeeded;
        let result = outcome(p.kind, p.moods, p.left, legendary, failed);
        info!("party over: {:?}, {} guests, average mood {:.0}: {result:?}", p.kind, p.left, if p.left > 0 { p.moods / p.left as f32 } else { 0.0 });
        let h = host.as_ref();
        let ui = ui.as_deref();
        if failed {
            notes.push(match p.kind {
                PartyKind::Birthday => words(ui, "Gameplay/Notifications/Party:BirthdayFailure", "{0.SimName}'s birthday party was a failure — no one aged up!", h),
                _ => words(ui, "Gameplay/Notifications/Party:WeddingFailure", "The guests were expecting to see {0.SimName}'s wedding today, but it looks like it's not going to happen. How sad...", h),
            });
        }
        notes.push(match (p.kind, result) {
            (PartyKind::Funeral, _) => words(ui, "Gameplay/Notifications/Party:NormalFuneral", "The funeral service that {0.SimName} was hosting is over. These things are never fun, but this one got the job done.", h),
            (_, Outcome::Good) => words(ui, "Gameplay/Notifications/Party:GoodParty", "{0.SimFirstName}'s party was awesome, verging on epic. Sims all around town will be talking about this one for years!", h),
            (_, Outcome::Bad) => words(ui, "Gameplay/Notifications/Party:BadParty", "{0.SimFirstName}'s party has dragged to a close. The guests are already complaining about {0.SimFirstName}'s hosting skills.", h),
            _ => words(ui, "Gameplay/Notifications/Party:NormalParty", "{0.SimFirstName}'s party was a modest success. The guests feel good about having attended.", h),
        });
        // (The game's `kPartyOver`: a great one's, a good outcome.)
        dreams.write(crate::dreams::DreamEvent { key: Some(format!("{:?}", p.kind)), quality: i32::from(result == Outcome::Good && p.kind != PartyKind::Funeral), ..crate::dreams::DreamEvent::new(p.host, "kPartyOver") });
        if let Ok(mut hq) = members.get_mut(p.host) {
            match result {
                Outcome::Good => hq.2.add(MoodletKind::GreatParty, now),
                Outcome::Bad => hq.2.add(MoodletKind::LameParty, now),
                Outcome::Normal => {}
            }
        }
        end(&mut commands, p, &mut members, &mut guests, now, false);
    }
}

/// A guest at the wedding leaves off what they're doing of their own accord, to watch.
fn stop_to_watch(mut w: EntityWorldMut) {
    if let Some(mut q) = w.get_mut::<ActionQueue>() {
        for a in q.0.iter_mut().filter(|a| a.autonomous && a.label != "React") {
            a.cancel = true;
        }
    }
}

/// A party over: a wedding that didn't happen leaves its couple Missed the Wedding.
#[allow(clippy::type_complexity)]
fn end(
    commands: &mut Commands,
    p: &mut Party,
    members: &mut Query<(Entity, &Sim, &mut Moodlets, &mut Relationships, Option<&crate::wishes::Wishes>, &ActionQueue, &Transform), With<HouseholdMember>>,
    guests: &mut Query<(&Sim, Option<&mut Visitor>, Option<&Mood>, &mut Moodlets, &mut Relationships, Has<GoingHome>, Has<OffLot>, Has<Invited>, Option<&ActionQueue>, &Transform), Without<HouseholdMember>>,
    now: f64,
    no_show: bool,
) {
    for &(a, b) in &p.couples {
        for e in [a, b] {
            let married = members.get(e).is_ok_and(|m| m.3.get(if e == a { b } else { a }).status == RelStatus::Married);
            let m = match members.get_mut(e) {
                Ok(q) => Some(q.2),
                Err(_) => guests.get_mut(e).ok().map(|q| q.3),
            };
            if let Some(mut m) = m {
                m.remove(MoodletKind::WeddingDay);
                if !married && !no_show {
                    m.add(MoodletKind::MissedWedding, now);
                }
            }
        }
    }
    // (Anyone still asked and not come: not coming now.)
    for &g in &p.guests {
        if guests.get(g).is_ok_and(|q| q.7) {
            commands.entity(g).remove::<Invited>();
        }
    }
    commands.remove_resource::<Party>();
}

/// The phone's "Throw Party..." call.
pub fn throw_party_action() -> Action {
    Action::new("Throw Party", ActionKind::ThrowParty, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parties_are_arranged_and_judged_as_the_games_are() {
        // (Nine in the morning to nine at night, an hour's notice at least.)
        assert_eq!(problem(PartyKind::House, Some(Dress::Casual), Some(19), 3, 14.0), None);
        assert_eq!(problem(PartyKind::House, Some(Dress::Casual), Some(23), 3, 14.0), Some("Ui/Caption/Party:InvalidTimeTooLate"));
        assert_eq!(problem(PartyKind::House, Some(Dress::Casual), Some(7), 3, 14.0), Some("Ui/Caption/Party:InvalidTime"));
        assert_eq!(problem(PartyKind::House, Some(Dress::Casual), Some(14), 3, 13.5), Some("Ui/Caption/Party:NotEnoughNotice"));
        assert_eq!(problem(PartyKind::House, None, Some(19), 3, 14.0), Some("Ui/Caption/Party:InvalidClothingType"));
        assert_eq!(problem(PartyKind::House, Some(Dress::Formal), Some(19), 0, 14.0), Some("Ui/Caption/Party:InvalidNumberOfGuests"));
        // (The next time it's that hour: today, or tomorrow once it's past.)
        assert_eq!(start_at(14.0 * 60.0, 19), 19.0 * 60.0);
        assert_eq!(start_at(20.0 * 60.0, 19), (24.0 + 19.0) * 60.0);
        // (Friends always come; strangers a quarter of the time at the least liking.)
        assert_eq!(coming_chance(10.0, true, false), 1.0);
        assert!((coming_chance(-25.0, false, false) - 0.25).abs() < 1e-4);
        assert_eq!(coming_chance(30.0, false, false), 1.0);
        // (Two hours for a stranger they dislike, four for a best friend.)
        assert!((stay_hours(-100.0) - 2.0).abs() < 1e-9);
        assert!((stay_hours(100.0) - 4.0).abs() < 1e-9);
        // (Judged by the guests' moods; a birthday without one a failure; a funeral just over.)
        assert_eq!(outcome(PartyKind::House, 3.0 * 70.0, 3, false, false), Outcome::Good);
        assert_eq!(outcome(PartyKind::House, 3.0 * 30.0, 3, false, false), Outcome::Normal);
        assert_eq!(outcome(PartyKind::House, 3.0 * 5.0, 3, false, false), Outcome::Bad);
        assert_eq!(outcome(PartyKind::Birthday, 3.0 * 90.0, 3, false, true), Outcome::Bad);
        assert_eq!(outcome(PartyKind::Funeral, 3.0 * 90.0, 3, false, false), Outcome::Normal);
        assert_eq!(kinds(true, false), vec![PartyKind::House, PartyKind::Wedding, PartyKind::Birthday]);
    }
}
