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
        app.init_resource::<OpportunityBoard>().add_systems(
            Update,
            (restore_pending, offer_opportunities, dialog_buttons, complete_opportunities, toggle_panel, update_panel).chain().run_if(in_state(PlayMode::Live)),
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
fn fill(s: &str, o: &OpportunityInfo, place: &str) -> String {
    s.replace("{10.Money}", &format!("§{}", o.money)).replace("{9.Number}", &o.skill_min.max(1).to_string()).replace("{RabbitHoleName}", if place.is_empty() { "the venue" } else { place })
}

/// The venue tasks a Sim's opportunities offer at this lot (open now).
pub fn lot_options(world: &crate::loading::WorldInfo, lot: usize, opps: Option<&SimOpportunities>, data: &s3bake::GameDataBaked, hour: f32) -> Vec<(String, ActionKind)> {
    let Some(opps) = opps else { return Vec::new() };
    let types = lot_types(world, lot);
    opps.active
        .iter()
        .filter_map(|a| {
            let o = data.opportunities.get(a.index)?;
            if !types.contains(&o.rabbit_hole.as_str()) {
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

/// Visit activities from this number up are opportunity tasks (minus it, the opportunity).
pub const OPPORTUNITY_TASK: usize = 1000;

/// Whether this Sim could be offered this opportunity.
fn eligible(o: &OpportunityInfo, sim: &Sim, skills: &Skills, job: Option<&crate::careers::Job>, data: &s3bake::GameDataBaked, types: &[&str]) -> bool {
    if !types.contains(&o.rabbit_hole.as_str()) || matches!(sim.age, Age::Baby | Age::Toddler | Age::Child) {
        return false;
    }
    if !o.career.is_empty() {
        let career = data.careers.iter().find(|c| c.hex == o.career).map(|c| c.name.as_str());
        return job.is_some_and(|j| Some(j.career().name) == career);
    }
    let level = skills.level(&o.skill);
    !o.skill.is_empty() && level >= o.skill_min.max(1) && level <= o.skill_max.max(o.skill_min)
}

/// Every game hour or so in the daytime, someone may be offered an opportunity.
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
) {
    let Some(mut ui) = ui else { return };
    if board.dialog.is_some() {
        return;
    }
    let forced = force.is_some();
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
    let data = ui.data.clone();
    let types: Vec<&str> = (0..world.data.lots.len()).flat_map(|i| lot_types(&world.data, i)).collect();
    let mut choices: Vec<(Entity, usize)> = Vec::new();
    for (e, sim, skills, job, opps) in &sims {
        if opps.is_some_and(|o| o.active.len() >= 3) {
            continue;
        }
        for (i, o) in data.opportunities.iter().enumerate() {
            let taken = opps.is_some_and(|x| x.active.iter().any(|a| a.index == i) || (!o.repeat && x.done.contains(&o.guid)));
            if !taken && eligible(o, sim, skills, job, &data, &types) {
                choices.push((e, i));
            }
        }
    }
    let Some(&(sim_e, index)) = choices.choose(&mut rng) else { return };
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
            BackgroundColor(Color::srgba(0.05, 0.15, 0.30, 0.96)),
            BorderColor::all(PLUMBOB_GREEN),
            Interaction::default(),
            BlocksWorld,
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

/// Asks for an opportunity to be offered now (tests).
#[derive(Resource)]
pub struct ForceOffer;

/// Accepts offers without asking (tests).
#[derive(Resource)]
pub struct AutoAccept;

/// Turns offers down without asking (tests).
#[derive(Resource)]
pub struct AutoDecline;

#[derive(Component)]
struct OppButton(bool);

#[allow(clippy::too_many_arguments)]
fn dialog_buttons(
    mut commands: Commands,
    mut board: ResMut<OpportunityBoard>,
    buttons: Query<(&Interaction, &OppButton), Changed<Interaction>>,
    clock: Res<crate::clock::GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut opps: Query<Option<&mut SimOpportunities>>,
    mut notes: ResMut<Notifications>,
    (auto_accept, auto_decline): (Option<Res<AutoAccept>>, Option<Res<AutoDecline>>),
) {
    let Some((root, sim, index)) = board.dialog else { return };
    let auto = auto_accept.is_some().then_some(true).or_else(|| auto_decline.is_some().then_some(false));
    let Some(choice) = auto.or_else(|| buttons.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, b)| b.0)) else { return };
    commands.entity(root).despawn();
    board.dialog = None;
    board.shown.clear();
    let (Some(ui), true) = (ui, choice) else { return };
    let Some(o) = ui.data.opportunities.get(index) else { return };
    let active = ActiveOpp { index, deadline: (o.days > 0.0).then(|| clock.minutes + o.days as f64 * 1440.0), activity: task_for(o) };
    match opps.get_mut(sim) {
        Ok(Some(mut s)) => s.active.push(active),
        Ok(None) => {
            commands.entity(sim).insert(SimOpportunities { active: vec![active], done: Vec::new() });
        }
        Err(_) => return,
    }
    notes.push(format!("Opportunity accepted: {}. Go to the {} to {}.", o.name, venue_word(&o.rabbit_hole), o.interaction.to_lowercase()));
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
    mut sims: Query<(&Sim, &mut SimOpportunities, &mut Skills, Option<&mut crate::careers::Job>)>,
    mut notes: ResMut<Notifications>,
) {
    let Some(ui) = ui else { return };
    let data = &ui.data;
    for ev in events.read() {
        let LifeEventKind::Finished { activity, completed: true } = ev.kind else { continue };
        let Ok((sim, mut opps, mut skills, job)) = sims.get_mut(ev.sim) else { continue };
        let Some(pos) = opps.active.iter().position(|a| std::ptr::eq(a.activity.name, activity)) else { continue };
        let a = opps.active.remove(pos);
        let Some(o) = data.opportunities.get(a.index) else { continue };
        opps.done.push(o.guid.clone());
        let mut got = Vec::new();
        if o.money > 0
            && let Some(h) = household.as_mut()
        {
            h.funds += o.money;
            got.push(format!("§{}", o.money));
        }
        if let Some(mut j) = job
            && (o.performance > 0.0 || o.raise > 0.0)
        {
            j.performance = (j.performance + o.performance + o.raise).min(100.0);
            got.push("a boost at work".into());
        }
        let skill = if o.skill.is_empty() { None } else { Some(o.skill.clone()) };
        if let Some(sk) = skill.filter(|_| o.skill_reward > 0.0)
            && let Some(name) = crate::save::SKILLS.iter().find(|s| s.eq_ignore_ascii_case(&sk))
        {
            *skills.0.entry(name).or_insert(0.0) += o.skill_reward / 100.0;
            got.push(format!("{name} skill"));
        }
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
    if buttons.iter().any(|i| *i == Interaction::Pressed) || keys.just_pressed(KeyCode::KeyO) {
        board.panel_open = !board.panel_open;
        board.shown.clear();
        if !board.panel_open
            && let Some(p) = board.panel.take()
        {
            commands.entity(p).despawn();
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
        commands.entity(p).despawn();
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
            BackgroundColor(Color::srgba(0.05, 0.15, 0.30, 0.92)),
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
                            format!("{} at the {} ({}–{}){}", a.activity.name, venue_word(&o.rabbit_hole), crate::interact::hour_label(o.open), crate::interact::hour_label(o.close), left.unwrap_or_default()),
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

/// Opportunities from a saved game, by id, waiting for the game data.
#[derive(Component)]
pub struct PendingOpportunities(pub Vec<(String, Option<f64>)>, pub Vec<String>);

fn restore_pending(mut commands: Commands, pending: Query<(Entity, &PendingOpportunities)>, ui: Option<Res<crate::icons::GameUi>>) {
    let Some(ui) = ui else { return };
    for (e, p) in &pending {
        let active = p
            .0
            .iter()
            .filter_map(|(guid, deadline)| {
                let index = ui.data.opportunities.iter().position(|o| &o.guid == guid)?;
                Some(ActiveOpp { index, deadline: *deadline, activity: task_for(&ui.data.opportunities[index]) })
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
