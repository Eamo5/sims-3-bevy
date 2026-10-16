//! Opportunities from the game's own tables (`s3bake::gamedata::OpportunityInfo`): during the
//! day a Sim may be offered one that suits their career or skills (the game's dialog: its
//! icon, name and description, Accept or Decline). Accepted ones are listed in the
//! Opportunities panel (O) with their deadline; the Sim does them by going to a venue of the
//! right kind during its hours (the task is offered there), and is paid, praised at work or
//! better at their skill when they come back. Missed deadlines fail them.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use s3bake::gamedata::OpportunityInfo;

use crate::PlayMode;
use crate::hud::BlocksWorld;
use crate::interact::{ActionKind, Household, Notifications, Skills};
use crate::life::{LifeEvent, LifeEventKind};
use crate::loading::CurrentWorld;
use crate::menu::{BTN_NORMAL, PLUMBOB_GREEN, text};
use crate::rabbitholes::Activity;
use crate::sim::{Age, HouseholdMember, Selected, Sim};

pub struct OpportunitiesPlugin;

impl Plugin for OpportunitiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpportunityBoard>().add_message::<OfferThis>().add_systems(
            Update,
            (restore_pending, offer_test, offer_opportunities, dialog_buttons, complete_opportunities, book_reports, toggle_panel, update_panel).chain().run_if(in_state(PlayMode::Live)),
        );
    }
}

/// An opportunity a Sim has taken on.
#[derive(Clone, Debug)]
pub struct ActiveOpp {
    /// Index into the game data's opportunities.
    pub index: usize,
    /// Game minute it must be done by.
    pub deadline: Option<f64>,
    /// The task done at the venue.
    pub activity: &'static Activity,
    /// Done at the venue already, to be reported on at work.
    pub visited: bool,
}

/// A Sim's opportunities: the ones taken on and the ones done (not offered again).
#[derive(Component, Default, Clone)]
pub struct SimOpportunities {
    pub active: Vec<ActiveOpp>,
    pub done: Vec<String>,
}

/// The offer on screen, the panel, and when to look for the next offer.
#[derive(Resource, Default)]
pub struct OpportunityBoard {
    dialog: Option<(Entity, Entity, usize)>,
    next_check: f64,
    panel_open: bool,
    panel: Option<Entity>,
    shown: Vec<(Entity, usize)>,
}

/// The task an opportunity sends the Sim to do (made once per opportunity taken on).
pub fn task_for(o: &OpportunityInfo) -> &'static Activity {
    let name: &'static str = Box::leak(if o.interaction.is_empty() { o.name.clone() } else { o.interaction.clone() }.into_boxed_str());
    Box::leak(Box::new(Activity { name, minutes: o.minutes, cost: 0, per_hour: [-6.0, -6.0, -6.0, 6.0, -4.0, 0.0], skill: None, open: o.open, close: o.close }))
}

/// The rabbit-hole types on a lot: from its rabbit-hole objects, else its name.
pub fn lot_types(world: &crate::loading::WorldInfo, lot: usize) -> Vec<&'static str> {
    const CLASSES: [(&str, &str); 14] = [
        ("Bistro", "Restaurant"),
        ("Diner", "Restaurant"),
        ("CityHall", "CityHall"),
        ("ScienceLab", "ScienceLab"),
        ("BusinessAndJournalism", "BusinessAndJournalism"),
        ("Grocery", "Grocery"),
        ("Theatre", "Theatre"),
        ("Stadium", "Stadium"),
        ("MilitaryBase", "MilitaryBase"),
        ("School", "School"),
        ("PoliceStation", "PoliceStation"),
        ("Bookstore", "Bookstore"),
        ("Hospital", "Hospital"),
        ("Hideout", "Hideout"),
    ];
    let mut out: Vec<&'static str> = Vec::new();
    if let Some(b) = world.buildings.get(&lot) {
        for o in &b.objects {
            if let Some(class) = o.script.split("RabbitHoles.").nth(1) {
                for (c, t) in CLASSES {
                    if class.starts_with(c) && !out.contains(&t) {
                        out.push(t);
                    }
                }
            }
        }
    }
    if out.is_empty()
        && let Some(l) = world.lots.get(lot)
    {
        let n = l.internal_name.to_ascii_lowercase();
        for (k, t) in [
            ("bistro", "Restaurant"),
            ("diner", "Restaurant"),
            ("cityhall", "CityHall"),
            ("science", "ScienceLab"),
            ("business", "BusinessAndJournalism"),
            ("grocery", "Grocery"),
            ("theat", "Theatre"),
            ("stadium", "Stadium"),
            ("military", "MilitaryBase"),
            ("school", "School"),
            ("police", "PoliceStation"),
            ("book", "Bookstore"),
            ("hospital", "Hospital"),
            ("criminal", "Hideout"),
        ] {
            if n.contains(k) && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// Fills in an opportunity text's placeholders.
pub(crate) fn fill(s: &str, o: &OpportunityInfo, place: &str) -> String {
    // (The number's how many things are asked for, else the skill's level.)
    let number = o.bring.as_ref().map_or(o.skill_min.max(1), |b| b.count);
    s.replace("{10.Money}", &format!("§{}", o.money)).replace("{9.Number}", &number.to_string()).replace("{RabbitHoleName}", if place.is_empty() { "the venue" } else { place })
}

/// When it is, for a venue task's hours and days: the hour, the day of the week (Monday 0), and
/// whether it's one the Sim works.
#[derive(Clone, Copy, Debug)]
pub struct Today {
    pub hour: f32,
    pub weekday: usize,
    pub work_day: bool,
}

impl Today {
    pub fn of(clock: &crate::clock::GameClock, job: Option<&crate::careers::Job>) -> Self {
        Today { hour: clock.hour_f(), weekday: clock.weekday(), work_day: job.is_some_and(|j| j.works_on(clock.weekday())) }
    }
}

const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// Whether a venue's task can be done today (`TargetInteractionDays`), and if not, the days it
/// can, in words.
pub fn day_ok(o: &OpportunityInfo, t: Today) -> Result<(), &'static str> {
    match o.on_days.as_str() {
        "" => Ok(()),
        "NonWorkDays" if t.work_day => Err("on a day off"),
        "NonWorkDays" => Ok(()),
        "WeekendDays" if t.weekday < 5 => Err("at the weekend"),
        "WeekendDays" => Ok(()),
        days if days.split(',').any(|d| d.trim() == WEEKDAYS[t.weekday % 7]) => Ok(()),
        _ => Err("on a weekday"),
    }
}

/// The venue tasks a Sim's opportunities offer at this lot (open now).
pub fn lot_options(world: &crate::loading::WorldInfo, lot: usize, opps: Option<&SimOpportunities>, data: &s3bake::GameDataBaked, today: Today) -> Vec<(String, ActionKind)> {
    let Some(opps) = opps else { return Vec::new() };
    let types = lot_types(world, lot);
    let hour = today.hour;
    opps.active
        .iter()
        .filter_map(|a| {
            let o = data.opportunities.get(a.index)?;
            // (Not again once done, while it's to be reported on at work; not on the days it
            // can't be, as the game's interaction test has it.)
            if !types.contains(&o.rabbit_hole.as_str()) || a.visited || day_ok(o, today).is_err() {
                return None;
            }
            let label = if hour >= o.open && hour < o.close {
                format!("{} ({})", a.activity.name, o.name)
            } else {
                format!("{} — open {}–{}", a.activity.name, crate::interact::hour_label(o.open), crate::interact::hour_label(o.close))
            };
            Some((label, ActionKind::Visit { lot, activity: OPPORTUNITY_TASK + a.index }))
        })
        .collect()
}

/// What's to be done for it, in words: the venue's task (`activity`, its name) and when, or the
/// boss's book to read.
pub fn objective(o: &OpportunityInfo, activity: &str, data: &s3bake::GameDataBaked) -> String {
    if !o.book.is_empty() {
        let book = data.novels.iter().find(|n| n.key == o.book).map_or("the book".to_string(), |n| format!("\"{}\"", n.name));
        return format!("Read {book}, then go to work");
    }
    if let Some(b) = &o.bring {
        return format!("Take {} to work", bring_words(b, data));
    }
    let days = match o.on_days.as_str() {
        "NonWorkDays" => " on a day off",
        "WeekendDays" => " at the weekend",
        "" => "",
        _ => " on a weekday",
    };
    let then = if o.then_work { ", then go to work" } else { "" };
    // (Open all day: no hours to name.)
    let hours = if o.open <= 0.0 && o.close >= 24.0 { String::new() } else { format!(" between {} and {}", crate::interact::hour_label(o.open), crate::interact::hour_label(o.close)) };
    format!("{activity} at the {}{hours}{days}{then}", venue_word(&o.rabbit_hole))
}

/// The quality tier a game quality's name stands for (`VeryNice`, `Neutral`...).
fn tier_of(name: &str) -> Option<u8> {
    let name = if name.eq_ignore_ascii_case("Neutral") { "Normal" } else { name };
    crate::gardening::QUALITIES.iter().position(|(q, _)| q.replace(' ', "").eq_ignore_ascii_case(name)).map(|t| t as u8)
}

/// What's to be taken in, in words: "a painting worth §750", "5 Sharks", "3 Very Nice or better
/// produce".
pub fn bring_words(b: &s3bake::gamedata::BringItems, data: &s3bake::GameDataBaked) -> String {
    // (Fish by the collectible's name, produce by the ingredient's; more than one, plural.)
    let name = |k: &str| {
        let n = data.collectibles.iter().find(|c| c.key == k).map(|c| c.name.clone()).or_else(|| data.ingredients.iter().find(|i| i.key == k).map(|i| i.name.clone())).unwrap_or_else(|| k.to_string());
        if b.count > 1 && !n.ends_with('s') && !n.to_ascii_lowercase().ends_with("fish") { format!("{n}s") } else { n }
    };
    let quality = tier_of(&b.quality).map(|t| format!("{} {}", crate::gardening::QUALITIES[t as usize].0, if b.worse { "or worse " } else { "or better " })).unwrap_or_default();
    let what = match b.kind.as_str() {
        "Painting" => (if b.count == 1 { "a painting" } else { "paintings" }).to_string(),
        "Fish" if b.key.is_empty() => "fish".to_string(),
        "Fish" => name(&b.key),
        _ if b.key.is_empty() => "produce".to_string(),
        _ => name(&b.key),
    };
    let worth = if b.worth > 0 { format!(" worth §{}", b.worth) } else { String::new() };
    let n = if b.count > 1 || b.kind != "Painting" { format!("{} ", b.count) } else { String::new() };
    format!("{n}{quality}{what}{worth}")
}

/// Whether a stack is of what's to be taken in.
pub fn brings(b: &s3bake::gamedata::BringItems, s: &crate::inventory::Stack) -> bool {
    use crate::inventory::ItemKind;
    let kind = match b.kind.as_str() {
        "Painting" => ItemKind::Painting,
        "Fish" => ItemKind::Fish,
        _ => ItemKind::Produce,
    };
    let quality = match tier_of(&b.quality) {
        None => true,
        Some(t) if b.worse => s.quality <= t,
        Some(t) => s.quality >= t,
    };
    s.kind == kind && (b.key.is_empty() || s.key.eq_ignore_ascii_case(&b.key)) && quality && s.each() >= b.worth
}

/// Visit activities from this number up are opportunity tasks (minus it, the opportunity).
pub const OPPORTUNITY_TASK: usize = 1000;

/// Whether this Sim could be offered this opportunity.
fn eligible(o: &OpportunityInfo, sim: &Sim, skills: &Skills, job: Option<&crate::careers::Job>, data: &s3bake::GameDataBaked, types: &[&str]) -> bool {
    // (A venue's, where the town has one; a book to read, the book; things to take in.)
    let book = !o.book.is_empty();
    let venue = !book && o.bring.is_none();
    if (book && !data.novels.iter().any(|n| n.key == o.book)) || (venue && !types.contains(&o.rabbit_hole.as_str())) || matches!(sim.age, Age::Baby | Age::Toddler | Age::Child) {
        return false;
    }
    let level = skills.level(&o.skill);
    if !o.career.is_empty() {
        let career = data.careers.iter().find(|c| c.hex == o.career).map(|c| c.name.as_str());
        // (A career's that asks a skill: within its levels, the game's `Skill` requirement.)
        let skilled = o.skill.is_empty() || (level >= o.skill_min && level <= o.skill_max.max(o.skill_min));
        return skilled && job.is_some_and(|j| Some(j.career().name) == career);
    }
    !o.skill.is_empty() && level >= o.skill_min.max(1) && level <= o.skill_max.max(o.skill_min)
}

/// OFFER=book|venue|bring (tests): ten seconds in, one of the selected Sim's career's
/// opportunities of that kind (a book to read; a venue's task reported on at work; things to
/// take in) offered them, and taken; a venue's, they set off to do it.
#[allow(clippy::type_complexity)]
fn offer_test(
    mut commands: Commands,
    time: Res<Time>,
    world: Res<CurrentWorld>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut sel: Query<(Entity, &Sim, Option<&crate::careers::Job>, Option<&SimOpportunities>, &mut crate::interact::ActionQueue), With<Selected>>,
    mut offers: MessageWriter<OfferThis>,
    mut stage: Local<u8>,
) {
    let Ok(kind) = std::env::var("OFFER") else { return };
    if *stage >= 2 || time.elapsed_secs() < 10.0 {
        return;
    }
    let (Some(ui), Ok((e, sim, job, opps, mut queue))) = (ui, sel.single_mut()) else { return };
    let career = job.map(|j| j.career().name);
    let hex = career.and_then(|name| ui.data.careers.iter().find(|c| c.name == name).map(|c| c.hex.clone()));
    let of_kind = |o: &OpportunityInfo| match kind.as_str() {
        "book" => !o.book.is_empty(),
        "bring" => o.bring.is_some(),
        _ => o.then_work,
    };
    if *stage == 0 {
        *stage = 1;
        let types: Vec<&str> = (0..world.data.lots.len()).flat_map(|i| lot_types(&world.data, i)).collect();
        let Some(o) = ui.data.opportunities.iter().find(|o| of_kind(o) && hex.as_ref().is_some_and(|k| *k == o.career) && (o.rabbit_hole.is_empty() || types.contains(&o.rabbit_hole.as_str()))) else {
            // (Their career may be one they're joining yet: tried a while.)
            *stage = 0;
            if time.elapsed_secs() > 90.0 {
                info!("offer test: no {kind} opportunity for {}'s career ({career:?})", sim.first);
                *stage = 2;
            }
            return;
        };
        info!("offer test: {} ({}) for {}: {}", o.name, o.guid, sim.first, objective(o, &o.interaction, &ui.data));
        commands.insert_resource(AutoAccept);
        offers.write(OfferThis { sim: e, guid: o.guid.clone() });
        return;
    }
    // (Taken: a venue's task, off to do it.)
    let Some(opps) = opps else { return };
    *stage = 2;
    if kind == "venue" {
        let any_day = Today { hour: 12.0, weekday: 5, work_day: false };
        for lot in 0..world.data.lots.len() {
            if let Some((label, kind)) = lot_options(&world.data, lot, Some(opps), &ui.data, any_day).into_iter().next() {
                info!("offer test: going to do {label}");
                queue.push_player(crate::interact::Action::new(label, kind, false));
                return;
            }
        }
    }
}

/// An opportunity to offer a Sim now (one of their career's events at work).
#[derive(Message, Clone, Debug)]
pub struct OfferThis {
    pub sim: Entity,
    pub guid: String,
}

/// Every game hour or so in the daytime, someone may be offered an opportunity (a skill's: a
/// career's come at work, as the game's career events); and any asked for.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn offer_opportunities(
    mut commands: Commands,
    mut board: ResMut<OpportunityBoard>,
    clock: Res<crate::clock::GameClock>,
    world: Res<CurrentWorld>,
    ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    sims: Query<(Entity, &Sim, &Skills, Option<&crate::careers::Job>, Option<&SimOpportunities>), With<HouseholdMember>>,
    force: Option<Res<ForceOffer>>,
    (mut asked, mut waiting): (MessageReader<OfferThis>, Local<Vec<OfferThis>>),
) {
    waiting.extend(asked.read().cloned());
    let Some(mut ui) = ui else { return };
    if board.dialog.is_some() {
        return;
    }
    // (One asked for, if it's still one to take on.)
    let types = || -> Vec<&str> { (0..world.data.lots.len()).flat_map(|i| lot_types(&world.data, i)).collect() };
    let mut requested = None;
    while requested.is_none() && !waiting.is_empty() {
        let w = waiting.remove(0);
        let data = ui.data.clone();
        let Some(i) = data.opportunities.iter().position(|o| o.guid == w.guid) else { continue };
        let Ok((_, sim, skills, job, opps)) = sims.get(w.sim) else { continue };
        let taken = opps.is_some_and(|x| x.active.iter().any(|a| a.index == i) || (!data.opportunities[i].repeat && x.done.contains(&w.guid)));
        if !taken && !opps.is_some_and(|o| o.active.len() >= 3) && eligible(&data.opportunities[i], sim, skills, job, &data, &types()) {
            requested = Some((w.sim, i));
        }
    }
    let forced = force.is_some();
    let data = ui.data.clone();
    let (sim_e, index) = match requested {
        Some(r) => r,
        None => {
            // (Not the moment play starts.)
            if board.next_check == 0.0 {
                board.next_check = clock.minutes + 90.0;
            }
            if !forced && (clock.minutes < board.next_check || !(9.0..19.0).contains(&clock.hour_f())) {
                return;
            }
            board.next_check = clock.minutes + 60.0;
            let mut rng = rand::rng();
            if !forced && !rng.random_bool(0.2) {
                return;
            }
            commands.remove_resource::<ForceOffer>();
            let types = types();
            // (Those a career's events offer come at work.)
            let at_work = |guid: &str| data.careers.iter().any(|c| c.events.iter().any(|e| e.opportunity == guid));
            let mut choices: Vec<(Entity, usize)> = Vec::new();
            for (e, sim, skills, job, opps) in &sims {
                if opps.is_some_and(|o| o.active.len() >= 3) {
                    continue;
                }
                for (i, o) in data.opportunities.iter().enumerate() {
                    let taken = opps.is_some_and(|x| x.active.iter().any(|a| a.index == i) || (!o.repeat && x.done.contains(&o.guid)));
                    if !taken && !(!forced && at_work(&o.guid)) && eligible(o, sim, skills, job, &data, &types) {
                        choices.push((e, i));
                    }
                }
            }
            let Some(&c) = choices.choose(&mut rng) else { return };
            c
        }
    };
    let Ok((_, sim, ..)) = sims.get(sim_e) else { return };
    let o = &data.opportunities[index];
    let icon = ui.icon(&mut images, &o.icon);
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(30.0),
                top: Val::Percent(22.0),
                width: Val::Percent(40.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(16.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(crate::menu::PANEL_BG),
            BorderColor::all(PLUMBOB_GREEN),
            Interaction::default(),
            BlocksWorld,
            crate::dialog::Modal,
            GlobalZIndex(20),
        ))
        .with_children(|p| {
            p.spawn(Node { column_gap: Val::Px(12.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                if let Some(h) = icon {
                    r.spawn(crate::icons::icon_bundle(h, 56.0));
                }
                r.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|c| {
                    c.spawn(text(format!("Opportunity for {}", sim.first), 14.0, Color::srgb(0.75, 0.85, 1.0)));
                    c.spawn(text(o.name.clone(), 22.0, Color::WHITE));
                });
            });
            p.spawn(text(fill(&o.desc, o, ""), 15.0, Color::WHITE));
            let mut terms = Vec::new();
            if o.money > 0 {
                terms.push(format!("Reward: §{}", o.money));
            }
            if o.days > 0.0 {
                terms.push(format!("{} days to complete", o.days));
            }
            if !terms.is_empty() {
                p.spawn(text(terms.join(" · "), 14.0, Color::srgb(1.0, 0.9, 0.5)));
            }
            p.spawn(Node { column_gap: Val::Px(12.0), justify_content: JustifyContent::Center, ..default() }).with_children(|r| {
                for (label, accept) in [("Accept", true), ("Decline", false)] {
                    r.spawn((
                        Button,
                        OppButton(accept),
                        Node { width: Val::Px(130.0), height: Val::Px(36.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                        BackgroundColor(if accept { Color::srgb(0.22, 0.55, 0.22) } else { BTN_NORMAL }),
                    ))
                    .with_children(|b| {
                        b.spawn(text(label, 16.0, Color::WHITE));
                    });
                }
            });
        })
        .id();
    board.dialog = Some((root, sim_e, index));
}

impl OpportunityBoard {
    /// The offer on show: its dialog, the Sim, the opportunity (index into the game data's).
    pub fn offer(&self) -> Option<(Entity, Entity, usize)> {
        self.dialog
    }
}

/// Asks for an opportunity to be offered now (tests).
#[derive(Resource)]
pub struct ForceOffer;

/// Accepts offers without asking (tests).
#[derive(Resource)]
pub struct AutoAccept;

/// Turns offers down without asking (tests).
#[derive(Resource)]
pub struct AutoDecline;

/// The offer's Accept (true) or Decline button (the game's dialog's OK and cancel too: see
/// `gamedialogs`).
#[derive(Component)]
pub struct OppButton(pub bool);

#[allow(clippy::too_many_arguments)]
fn dialog_buttons(
    mut commands: Commands,
    mut board: ResMut<OpportunityBoard>,
    buttons: Query<(&Interaction, &OppButton), Changed<Interaction>>,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut opps: Query<(&Sim, Option<&mut SimOpportunities>, Option<&mut crate::novels::Novels>)>,
    mut notes: ResMut<Notifications>,
    (auto_accept, auto_decline): (Option<Res<AutoAccept>>, Option<Res<AutoDecline>>),
) {
    let Some((root, sim, index)) = board.dialog else { return };
    let auto = auto_accept.is_some().then_some(true).or_else(|| auto_decline.is_some().then_some(false));
    let Some(choice) = auto.or_else(|| buttons.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, b)| b.0)) else { return };
    commands.entity(root).try_despawn();
    board.dialog = None;
    board.shown.clear();
    let (Some(ui), true) = (ui, choice) else { return };
    let Some(o) = ui.data.opportunities.get(index) else { return };
    let active = ActiveOpp { index, deadline: (o.days > 0.0).then(|| clock.minutes + o.days as f64 * 1440.0), activity: task_for(o), visited: false };
    let Ok((who, mine, novels)) = opps.get_mut(sim) else { return };
    let first = who.first.clone();
    match mine {
        Some(mut s) => s.active.push(active),
        None => {
            commands.entity(sim).insert(SimOpportunities { active: vec![active], done: Vec::new() });
        }
    }
    // (The boss's book, handed over to take home.)
    if let Some(b) = ui.data.novels.iter().find(|n| !o.book.is_empty() && n.key == o.book) {
        match novels {
            Some(mut n) => n.own(&b.key),
            None => {
                let mut n = crate::novels::Novels::default();
                n.own(&b.key);
                commands.entity(sim).insert(n);
            }
        }
        notes.push(format!("Opportunity accepted: {}. {first} has \"{}\" to read: finished, it's reported on at work.", o.name, b.name));
        return;
    }
    if let Some(b) = &o.bring {
        notes.push(format!("Opportunity accepted: {}. {first} is to take {} to work.", o.name, bring_words(b, &ui.data)));
        return;
    }
    let then = if o.then_work { ", then report on it at work" } else { "" };
    notes.push(format!("Opportunity accepted: {}. Go to the {} to {}{then}.", o.name, venue_word(&o.rabbit_hole), o.interaction.to_lowercase()));
}

/// At work with what was asked done: the opportunity's done (the game's `kWentToWork`, with the
/// book's `kReadBook` heard, the venue's task done, or the things asked for brought in, out of
/// the inventory), and its rewards, the boss's liking among them.
#[allow(clippy::type_complexity)]
fn book_reports(
    ui: Option<Res<crate::icons::GameUi>>,
    mut household: Option<ResMut<Household>>,
    mut sims: Query<
        (
            &Sim,
            &mut SimOpportunities,
            &mut Skills,
            Option<&mut crate::careers::Job>,
            Option<&crate::wishes::Wishes>,
            Option<&crate::novels::Novels>,
            &mut crate::social::Relationships,
            Option<&mut crate::inventory::Inventory>,
        ),
        Added<crate::careers::AtWork>,
    >,
    people: Query<(Entity, &Sim), Without<crate::careers::AtWork>>,
    mut notes: ResMut<Notifications>,
) {
    let Some(ui) = ui else { return };
    let data = &ui.data;
    for (sim, mut opps, mut skills, mut job, wishes, novels, mut rels, mut inv) in &mut sims {
        // (What's brought in: as many as asked of what's held, the cheapest first, as the game's
        // `TargetDeletionSortingFunction`s.)
        let held = |b: &s3bake::gamedata::BringItems, inv: Option<&crate::inventory::Inventory>| inv.map_or(0, |inv| inv.0.iter().filter(|s| brings(b, s)).map(|s| s.count).sum::<u32>()) >= b.count;
        let read: Vec<usize> = opps
            .active
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                data.opportunities.get(a.index).is_some_and(|o| {
                    (!o.book.is_empty() && novels.is_some_and(|n| n.finished(&o.book))) || (o.then_work && a.visited) || o.bring.as_ref().is_some_and(|b| held(b, inv.as_deref()))
                })
            })
            .map(|(i, _)| i)
            .collect();
        for i in read.into_iter().rev() {
            if let (Some(b), Some(inv)) = (data.opportunities.get(opps.active[i].index).and_then(|o| o.bring.as_ref()), inv.as_deref_mut()) {
                for _ in 0..b.count {
                    let cheapest = inv.0.iter().enumerate().filter(|(_, s)| brings(b, s) && s.count > 0).min_by_key(|(_, s)| s.each()).map(|(j, _)| j);
                    if let Some(j) = cheapest {
                        inv.take_one(j);
                    }
                }
            }
            let a = opps.active.remove(i);
            let Some(o) = data.opportunities.get(a.index) else { continue };
            opps.done.push(o.guid.clone());
            let mut got = reward(o, household.as_deref_mut(), job.as_deref_mut(), &mut skills, wishes);
            if o.boss_liking > 0.0
                && let Some(boss) = job.as_ref().and_then(|j| j.boss).and_then(|id| people.iter().find(|(_, s)| s.id == id).map(|(e, _)| e))
            {
                rels.add(boss, o.boss_liking, 0.0);
                got.push("the boss's respect".into());
            }
            let text = if o.completion.is_empty() { format!("{} completed {}!", sim.first, o.name) } else { fill(&o.completion, o, "") };
            notes.push(if got.is_empty() { text } else { format!("{text} ({})", got.join(", ")) });
        }
    }
}

/// An opportunity done: its money (half again for the Opportunistic), the boost at work, the
/// skill; what was had, in words.
fn reward(o: &OpportunityInfo, household: Option<&mut Household>, job: Option<&mut crate::careers::Job>, skills: &mut Skills, wishes: Option<&crate::wishes::Wishes>) -> Vec<String> {
    let mut got = Vec::new();
    let money = if crate::wishes::has(wishes, "Opportunistic") { o.money * 3 / 2 } else { o.money };
    if money > 0
        && let Some(h) = household
    {
        h.funds += money;
        got.push(format!("§{money}"));
    }
    if let Some(j) = job
        && (o.performance > 0.0 || o.raise > 0.0)
    {
        j.performance = (j.performance + o.performance + o.raise).min(100.0);
        got.push("a boost at work".into());
    }
    let skill = if o.skill.is_empty() { None } else { Some(o.skill.clone()) };
    if let Some(sk) = skill.filter(|_| o.skill_reward > 0.0)
        && let Some(name) = crate::save::SKILLS.iter().find(|s| s.eq_ignore_ascii_case(&sk))
    {
        // (The game's: that per cent more of the skill points they have.)
        let more = skills.points(name) * o.skill_reward / 100.0;
        skills.add_points(*name, more);
        got.push(format!("{name} skill"));
    }
    got
}

/// The game's rabbit-hole types in words.
pub fn venue_word(t: &str) -> &str {
    match t {
        "Restaurant" => "restaurant",
        "CityHall" => "City Hall",
        "ScienceLab" => "Science Facility",
        "BusinessAndJournalism" => "business office",
        "Grocery" => "grocery store",
        "Theatre" => "theatre",
        "Stadium" => "stadium",
        "MilitaryBase" => "military base",
        "School" => "school",
        "PoliceStation" => "police station",
        "Bookstore" => "bookstore",
        "Hospital" => "hospital",
        "Hideout" => "criminal hideout",
        t => t,
    }
}

/// Coming back from an opportunity's task: the rewards. Deadlines passed: failed.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn complete_opportunities(
    mut events: MessageReader<LifeEvent>,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut household: Option<ResMut<Household>>,
    mut sims: Query<(&Sim, &mut SimOpportunities, &mut Skills, Option<&mut crate::careers::Job>, Option<&crate::wishes::Wishes>)>,
    mut notes: ResMut<Notifications>,
) {
    let Some(ui) = ui else { return };
    let data = &ui.data;
    for ev in events.read() {
        let LifeEventKind::Finished { activity, completed: true } = ev.kind else { continue };
        let Ok((sim, mut opps, mut skills, mut job, wishes)) = sims.get_mut(ev.sim) else { continue };
        let Some(pos) = opps.active.iter().position(|a| std::ptr::eq(a.activity.name, activity)) else { continue };
        // (Reported on at work, it's done then: see `book_reports`.)
        if data.opportunities.get(opps.active[pos].index).is_some_and(|o| o.then_work) {
            if !opps.active[pos].visited {
                opps.active[pos].visited = true;
                notes.push(format!("{} has done what was asked at the venue: it's reported on at work next shift.", sim.first));
            }
            continue;
        }
        let a = opps.active.remove(pos);
        let Some(o) = data.opportunities.get(a.index) else { continue };
        opps.done.push(o.guid.clone());
        let got = reward(o, household.as_deref_mut(), job.as_deref_mut(), &mut skills, wishes);
        let text = if o.completion.is_empty() { format!("{} completed {}!", sim.first, o.name) } else { fill(&o.completion, o, "") };
        notes.push(if got.is_empty() { text } else { format!("{text} ({})", got.join(", ")) });
    }
    // Deadlines.
    for (sim, mut opps, ..) in &mut sims {
        let now = clock.minutes;
        let failed: Vec<usize> = opps.active.iter().filter(|a| a.deadline.is_some_and(|d| now > d)).map(|a| a.index).collect();
        opps.active.retain(|a| a.deadline.is_none_or(|d| now <= d));
        for i in failed {
            if let Some(o) = data.opportunities.get(i) {
                notes.push(if o.failure.is_empty() { format!("{} missed the opportunity: {}.", sim.first, o.name) } else { fill(&o.failure, o, "") });
            }
        }
    }
}

/// The bottom bar's button that opens the panel.
#[derive(Component)]
pub struct OpportunitiesButton;

fn toggle_panel(
    mut commands: Commands,
    buttons: Query<&Interaction, (Changed<Interaction>, With<OpportunitiesButton>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut board: ResMut<OpportunityBoard>,
) {
    // (The game's own panel is up instead: see `infopanels`.)
    if crate::infopanels::GAME_PANELS.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if buttons.iter().any(|i| *i == Interaction::Pressed) || keys.just_pressed(KeyCode::KeyO) {
        board.panel_open = !board.panel_open;
        board.shown.clear();
        if !board.panel_open
            && let Some(p) = board.panel.take()
        {
            commands.entity(p).try_despawn();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_panel(
    mut commands: Commands,
    mut board: ResMut<OpportunityBoard>,
    clock: Res<crate::clock::GameClock>,
    selected: Query<(Entity, &Sim, Option<&SimOpportunities>), With<Selected>>,
    ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
) {
    if !board.panel_open {
        return;
    }
    let (Ok((me, sim, opps)), Some(mut ui)) = (selected.single(), ui) else { return };
    let list: Vec<ActiveOpp> = opps.map(|o| o.active.clone()).unwrap_or_default();
    let mut shown: Vec<(Entity, usize)> = list.iter().map(|a| (me, a.index)).collect();
    shown.push((me, usize::MAX));
    if board.panel.is_some() && board.shown == shown {
        return;
    }
    board.shown = shown;
    if let Some(p) = board.panel.take() {
        commands.entity(p).try_despawn();
    }
    let data = ui.data.clone();
    let rows: Vec<(Option<Handle<Image>>, &OpportunityInfo, &ActiveOpp)> =
        list.iter().filter_map(|a| data.opportunities.get(a.index).map(|o| (ui.icon(&mut images, &o.icon), o, a))).collect();
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(520.0),
                bottom: Val::Px(180.0),
                width: Val::Px(420.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(12.0)),
                border_radius: BorderRadius::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(crate::menu::PANEL_BG),
            Interaction::default(),
            BlocksWorld,
        ))
        .with_children(|p| {
            p.spawn(text(format!("{}'s Opportunities", sim.first), 18.0, Color::WHITE));
            if rows.is_empty() {
                p.spawn(text("None right now. Keep building skills and careers: offers come by day.", 14.0, Color::srgb(0.8, 0.85, 0.95)));
            }
            for (icon, o, a) in rows {
                p.spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                    if let Some(h) = icon {
                        r.spawn(crate::icons::icon_bundle(h, 40.0));
                    }
                    r.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() }).with_children(|c| {
                        c.spawn(text(o.name.clone(), 15.0, Color::WHITE));
                        let left = a.deadline.map(|d| {
                            let h = ((d - clock.minutes) / 60.0).max(0.0);
                            if h > 48.0 { format!(" · {:.0} days left", h / 24.0) } else { format!(" · {h:.0} hours left") }
                        });
                        c.spawn(text(
                            format!("{}{}", objective(o, a.activity.name, &data), left.unwrap_or_default()),
                            12.0,
                            Color::srgb(0.75, 0.85, 1.0),
                        ));
                    });
                });
            }
        })
        .id();
    board.panel = Some(root);
}

/// Opportunities from a saved game, by id, waiting for the game data: those taken on, those
/// done, and those whose venue task is done (to be reported on at work).
#[derive(Component)]
pub struct PendingOpportunities(pub Vec<(String, Option<f64>)>, pub Vec<String>, pub Vec<String>);

fn restore_pending(mut commands: Commands, pending: Query<(Entity, &PendingOpportunities)>, ui: Option<Res<crate::icons::GameUi>>) {
    let Some(ui) = ui else { return };
    for (e, p) in &pending {
        let active = p
            .0
            .iter()
            .filter_map(|(guid, deadline)| {
                let index = ui.data.opportunities.iter().position(|o| &o.guid == guid)?;
                Some(ActiveOpp { index, deadline: *deadline, activity: task_for(&ui.data.opportunities[index]), visited: p.2.contains(guid) })
            })
            .collect();
        commands.entity(e).remove::<PendingOpportunities>().insert(SimOpportunities { active, done: p.1.clone() });
    }
}

/// The task to do for a Visit activity number, if it is an opportunity's.
pub fn opportunity_task(opps: Option<&SimOpportunities>, activity: usize) -> Option<(&'static Activity, usize)> {
    let index = activity.checked_sub(OPPORTUNITY_TASK)?;
    opps?.active.iter().find(|a| a.index == index).map(|a| (a.activity, index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{ItemKind, Stack};
    use s3bake::gamedata::bring_items;

    #[test]
    fn venue_tasks_keep_their_days_and_work_wants_what_it_asked_for() {
        // (The days a venue's task can be done on, as the game's `TargetInteractionDays`.)
        let mut o = OpportunityInfo::default();
        let t = |weekday, work_day| Today { hour: 12.0, weekday, work_day };
        assert!(day_ok(&o, t(0, true)).is_ok());
        o.on_days = "NonWorkDays".into();
        assert!(day_ok(&o, t(0, true)).is_err() && day_ok(&o, t(0, false)).is_ok());
        o.on_days = "WeekendDays".into();
        assert!(day_ok(&o, t(4, false)).is_err() && day_ok(&o, t(6, false)).is_ok());
        o.on_days = "Monday,Tuesday,Wednesday,Thursday,Friday".into();
        assert!(day_ok(&o, t(2, true)).is_ok() && day_ok(&o, t(5, false)).is_err());
        // (Things taken in: their kind, key, quality and worth.)
        let stack = |kind, key: &str, quality, worth| Stack { kind, key: key.into(), name: String::new(), quality, count: 1, worth };
        let apples = bring_items("Harvestable,Apple,Bad,EqualOrWorse", "10").unwrap();
        assert_eq!((apples.kind.as_str(), apples.key.as_str(), apples.quality.as_str(), apples.worse, apples.count), ("Produce", "Apple", "Bad", true, 10));
        assert!(brings(&apples, &stack(ItemKind::Produce, "Apple", 1, 5)) && !brings(&apples, &stack(ItemKind::Produce, "Apple", 4, 5)));
        assert!(!brings(&apples, &stack(ItemKind::Produce, "Onion", 1, 5)) && !brings(&apples, &stack(ItemKind::Fish, "Apple", 1, 5)));
        let nice = bring_items("Harvestable,Any,VeryNice,EqualOrBetter", "3").unwrap();
        assert!(brings(&nice, &stack(ItemKind::Produce, "Onion", 5, 5)) && !brings(&nice, &stack(ItemKind::Produce, "Onion", 4, 5)));
        let art = bring_items("PaintingAny,750", "1").unwrap();
        assert!(brings(&art, &stack(ItemKind::Painting, "x", 0, 800)) && !brings(&art, &stack(ItemKind::Painting, "x", 0, 500)));
        let fish = bring_items("Fish,Any,Any", "2").unwrap();
        assert!(brings(&fish, &stack(ItemKind::Fish, "Shark", 0, 30)));
        assert!(bring_items("FoodAny,Ambrosia,Any", "1").is_none());
        let data = s3bake::GameDataBaked::default();
        assert_eq!(bring_words(&art, &data), "a painting worth §750");
        assert_eq!(bring_words(&nice, &data), "3 Very Nice or better produce");
        assert_eq!(bring_words(&apples, &data), "10 Bad or worse Apples");
    }
}
