//! The Sim panel's tabs, as the game's: Needs (the bars), Skills (every skill learned, with its
//! level and progress to the next), Career (the job, its hours and pay, the performance meter
//! and the next promotion) and Simology (traits with what they mean, lifetime happiness and the
//! rewards bought with it).

use bevy::prelude::*;

use crate::careers::Job;
use crate::hud::BlocksWorld;
use crate::interact::{AtWork, Skills};
use crate::menu::{BTN_HOVER, BTN_NORMAL, PLUMBOB_GREEN, text};
use crate::sim::{Selected, Sim};
use crate::PlayMode;

pub struct SimPanelPlugin;

impl Plugin for SimPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimTab>().add_systems(Update, (tab_buttons, tab_layout, tab_content).chain().run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SimTab {
    #[default]
    Needs,
    Skills,
    Career,
    Simology,
}

impl SimTab {
    pub const ALL: [SimTab; 4] = [SimTab::Needs, SimTab::Skills, SimTab::Career, SimTab::Simology];

    pub fn label(self) -> &'static str {
        match self {
            SimTab::Needs => "Needs",
            SimTab::Skills => "Skills",
            SimTab::Career => "Career",
            SimTab::Simology => "Simology",
        }
    }
}

/// A tab's button.
#[derive(Component)]
pub struct SimTabButton(pub SimTab);

/// What's shown only on the Needs tab (the bars, trait and skill icons).
#[derive(Component)]
pub struct NeedsOnly;

/// Where the other tabs draw.
#[derive(Component)]
pub struct TabContent;

/// The tab strip (built into the Sim panel by the HUD).
pub fn tab_strip(p: &mut ChildSpawnerCommands) {
    p.spawn(Node { column_gap: Val::Px(4.0), ..default() }).with_children(|row| {
        for t in SimTab::ALL {
            row.spawn((
                Button,
                SimTabButton(t),
                BlocksWorld,
                Node {
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(if t == SimTab::Needs { 2.0 } else { 0.0 })),
                    ..default()
                },
                BorderColor::all(PLUMBOB_GREEN),
                BackgroundColor(BTN_NORMAL),
            ))
            .with_children(|b| {
                b.spawn((text(t.label(), 13.0, Color::WHITE), Pickable::IGNORE));
            });
        }
    });
}

fn tab_buttons(
    mut tab: ResMut<SimTab>,
    keys: Res<ButtonInput<KeyCode>>,
    mut buttons: Query<(&Interaction, &SimTabButton, &mut BackgroundColor, &mut Node), With<Button>>,
) {
    for (i, b, ..) in &buttons {
        if *i == Interaction::Pressed {
            *tab = b.0;
        }
    }
    // F5 to F8 pick a tab.
    for (k, t) in [(KeyCode::F5, SimTab::Needs), (KeyCode::F6, SimTab::Skills), (KeyCode::F7, SimTab::Career), (KeyCode::F8, SimTab::Simology)] {
        if keys.just_pressed(k) {
            *tab = t;
        }
    }
    for (i, b, mut bg, mut node) in &mut buttons {
        bg.0 = if *i == Interaction::Hovered { BTN_HOVER } else { BTN_NORMAL };
        node.border = UiRect::all(Val::Px(if b.0 == *tab { 2.0 } else { 0.0 }));
    }
}

fn tab_layout(tab: Res<SimTab>, mut needs: Query<&mut Node, (With<NeedsOnly>, Without<TabContent>)>, mut content: Query<&mut Node, (With<TabContent>, Without<NeedsOnly>)>) {
    if !tab.is_changed() {
        return;
    }
    for mut n in &mut needs {
        n.display = if *tab == SimTab::Needs { Display::Flex } else { Display::None };
    }
    for mut n in &mut content {
        n.display = if *tab == SimTab::Needs { Display::None } else { Display::Flex };
    }
}

/// A bar filled to `v` (0..1).
fn meter(p: &mut ChildSpawnerCommands, v: f32, width: f32, color: Color) {
    p.spawn((
        Node { width: Val::Px(width), height: Val::Px(9.0), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
    ))
    .with_children(|b| {
        b.spawn((
            Node { width: Val::Percent(v.clamp(0.0, 1.0) * 100.0), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() },
            BackgroundColor(color),
        ));
    });
}

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn hour(h: f32) -> String {
    crate::interact::hour_label(h)
}

/// Redraws the open tab when what it shows changes.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn tab_content(
    mut commands: Commands,
    tab: Res<SimTab>,
    content: Query<Entity, With<TabContent>>,
    sel: Query<
        (Entity, &Sim, &Skills, Option<&Job>, Has<AtWork>, Option<&crate::wishes::Wishes>, Option<&crate::lifetime::LifetimeWish>, Option<&crate::writing::Author>),
        With<Selected>,
    >,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<String>,
) {
    let (Ok(root), Ok((e, sim, skills, job, at_work, wishes, ltw, author))) = (content.single(), sel.single()) else { return };
    // What's on show, to redraw only when it changes.
    let key = match *tab {
        SimTab::Needs => "needs".to_string(),
        SimTab::Skills => format!(
            "{e:?} {:?} {:?}",
            skills.0.iter().map(|(k, v)| (*k, (*v * 20.0) as i32)).collect::<Vec<_>>(),
            author.map(|a| (a.books.len(), a.weekly_royalties(), a.draft.as_ref().map(|d| (d.pages / d.length * 50.0) as i32)))
        ),
        SimTab::Career => format!("{e:?} {:?} {at_work}", job.map(|j| (j.track, j.level, j.performance as i32))),
        SimTab::Simology => format!("{e:?} {:?} {:?} {:?}", sim.traits, wishes.map(|w| (w.points, w.rewards.len())), ltw.map(|l| (l.wish, &l.status))),
    } + &format!(" {}", ui.is_some());
    if *last == key {
        return;
    }
    *last = key;
    commands.entity(root).despawn_children();
    if *tab == SimTab::Needs {
        return;
    }
    let ui = ui.as_deref_mut();
    commands.entity(root).with_children(|p| match *tab {
        SimTab::Skills => {
            let mut list: Vec<(&'static str, f32)> = skills.0.iter().filter(|(_, v)| **v > 0.01).map(|(k, v)| (*k, *v)).collect();
            list.sort_by(|a, b| b.1.total_cmp(&a.1));
            if list.is_empty() {
                p.spawn(text(format!("{} hasn't learned any skills yet. Reading, practising or taking a class all help.", sim.first), 13.0, Color::srgb(0.8, 0.85, 0.95)));
            }
            let ui = ui;
            let mut icons: Vec<Option<Handle<Image>>> = Vec::new();
            let mut infos = Vec::new();
            if let Some(ui) = ui {
                for (name, _) in &list {
                    let info = ui.data.skill(name).cloned();
                    icons.push(info.as_ref().and_then(|i| ui.icon(&mut images, &i.icon)));
                    infos.push(info);
                }
            }
            for (k, (name, v)) in list.iter().enumerate() {
                let info = infos.get(k).cloned().flatten();
                let max = info.as_ref().map_or(10, |i| i.max_level.max(1));
                let level = (*v as u32).min(max);
                let into = if level >= max { 1.0 } else { v.fract() };
                p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    if let Some(Some(h)) = icons.get(k) {
                        row.spawn(crate::icons::icon_bundle(h.clone(), 26.0));
                    }
                    row.spawn((text(info.as_ref().map_or(*name, |i| i.name.as_str()).to_string(), 14.0, Color::WHITE), Node { width: Val::Px(120.0), ..default() }));
                    row.spawn((text(format!("{level}/{max}"), 13.0, Color::srgb(1.0, 0.9, 0.5)), Node { width: Val::Px(42.0), ..default() }));
                    meter(row, into, 130.0, Color::srgb(0.35, 0.8, 1.0));
                });
            }
            // Their writing: the book under way, books written and royalties coming in.
            if let Some(a) = author.filter(|a| a.draft.is_some() || !a.books.is_empty()) {
                let mut parts = Vec::new();
                if let Some(d) = &a.draft {
                    parts.push(format!("Writing “{}” ({:.0}%)", d.title, d.pages / d.length * 100.0));
                }
                if !a.books.is_empty() {
                    parts.push(format!("{} book{} written", a.books.len(), if a.books.len() == 1 { "" } else { "s" }));
                }
                let weekly = a.weekly_royalties();
                if weekly > 0 {
                    parts.push(format!("§{weekly} a week in royalties"));
                }
                p.spawn(text(parts.join(" · "), 13.0, Color::srgb(1.0, 0.9, 0.5)));
            }
        }
        SimTab::Career => match job {
            Some(j) => {
                let track = j.career();
                let lvl = j.info();
                let icon = ui.and_then(|ui| ui.icon(&mut images, track.icon));
                p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    if let Some(h) = icon {
                        row.spawn(crate::icons::icon_bundle(h, 36.0));
                    }
                    let path = j.branch_label().map_or(String::new(), |b| format!(" · {b} path"));
                    row.spawn(text(format!("{} — {}\nLevel {} of {}{path}", track.name, lvl.title, j.level + 1, j.levels().len()), 15.0, Color::WHITE));
                });
                let days: Vec<&str> = (0..7).filter(|d| lvl.days & (1 << d) != 0).map(|d| DAYS[d]).collect();
                p.spawn(text(
                    format!("{} to {} · {} · §{} an hour{}", hour(lvl.start), hour(lvl.end), days.join(" "), lvl.hourly, if at_work { " · at work now" } else { "" }),
                    13.0,
                    Color::srgb(0.75, 0.85, 1.0),
                ));
                p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    row.spawn(text("Performance", 13.0, Color::WHITE));
                    let v = (j.performance + 100.0) / 200.0;
                    let c = if j.performance >= 0.0 { Color::srgb(0.35, 0.9, 0.35) } else { Color::srgb(0.95, 0.4, 0.3) };
                    meter(row, v, 200.0, c);
                });
                let next = match j.levels().get(j.level + 1) {
                    // At the branch, the next step depends on the path taken.
                    Some(_) if track.branch_at == Some(j.level + 1) => {
                        let ways: Vec<String> = track.paths.iter().map(|p| format!("{} ({})", p.levels[j.level + 1].title, p.label())).collect();
                        format!("Next: {}", ways.join(" or "))
                    }
                    Some(n) => format!("Next: {} (§{} an hour)", n.title, n.hourly),
                    None => "Top of the career!".to_string(),
                };
                p.spawn(text(format!("{next} · Improve with {}, and a good mood at work.", track.skill), 13.0, Color::srgb(0.8, 0.85, 0.95)));
            }
            None => {
                p.spawn(text(format!("{} doesn't have a job. Look in the newspaper, or on a computer, for one.", sim.first), 13.0, Color::srgb(0.8, 0.85, 0.95)));
            }
        },
        SimTab::Simology => {
            let mut ui = ui;
            // The lifetime wish, with how far along it is.
            if let Some(l) = ltw {
                let d = l.def();
                let data = ui.as_ref().map(|u| u.data.clone());
                let icon = ui.as_deref_mut().and_then(|u| u.icon(&mut images, &d.icon(data.as_deref())));
                p.spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    if let Some(h) = icon {
                        row.spawn(crate::icons::icon_bundle(h, 40.0));
                    }
                    row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() }).with_children(|c| {
                        c.spawn(text(format!("Lifetime Wish: {}", d.name), 14.0, Color::srgb(1.0, 0.9, 0.5)));
                        c.spawn(text(
                            format!("{} · {} lifetime happiness", d.describe(data.as_deref()), crate::lifetime::group(d.points(data.as_deref()) as i64)),
                            12.0,
                            Color::srgb(0.8, 0.85, 0.95),
                        ));
                        c.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                            meter(r, l.progress, 160.0, if l.fulfilled { Color::srgb(1.0, 0.8, 0.2) } else { Color::srgb(0.35, 0.9, 0.35) });
                            r.spawn(text(l.status.clone(), 12.0, Color::WHITE));
                        });
                    });
                });
            }
            if let Some(ui) = ui {
                for t in &sim.traits {
                    let info = ui.trait_info(*t);
                    let icon = info.as_ref().and_then(|i| ui.icon(&mut images, &i.icon_small).or_else(|| ui.icon(&mut images, &i.icon)));
                    let (name, desc) = info.map_or((t.name().to_string(), String::new()), |i| (i.name, i.desc));
                    p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                        if let Some(h) = icon {
                            row.spawn(crate::icons::icon_bundle(h, 26.0));
                        }
                        let mut desc = desc.replace("{0.SimFirstName}", &sim.first);
                        if desc.chars().count() > 70 {
                            desc = desc.chars().take(68).collect::<String>() + "…";
                        }
                        row.spawn((text(format!("{name}: {desc}"), 12.0, Color::WHITE), Node { max_width: Val::Px(320.0), ..default() }));
                    });
                }
            }
            if let Some(w) = wishes {
                let rewards: Vec<&str> = w.rewards.iter().map(|r| r.name()).collect();
                p.spawn(text(
                    format!("Lifetime happiness: {}{}", w.points, if rewards.is_empty() { String::new() } else { format!(" · Rewards: {}", rewards.join(", ")) }),
                    13.0,
                    Color::srgb(1.0, 0.9, 0.5),
                ));
            }
        }
        SimTab::Needs => {}
    });
}
