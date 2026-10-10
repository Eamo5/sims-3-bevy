//! The live HUD's info panels in the game's own layouts (see `livehud`): Simology (the name,
//! the life stage with the days to the next, the traits, the favourites, the family tree) and
//! Career (the job or school: its hours and days, pay, performance, and how they work), and
//! Inventory (the stacks, the phone, the collection journal). Each filled as the game's own panel
//! code fills it (UI.dll's `SimologyPanel`, `CareerPanel`, `InventoryPanel`).

use bevy::prelude::*;

use crate::layout::{Spawned, UiAssets};
use crate::livehud::{InfoPanel, LiveHud, pressed, set_text, set_visible};
use crate::sim::{Age, Selected, Sim};

pub struct HudPanelsPlugin;

impl Plugin for HudPanelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (simology_panel, career_panel, inventory_panel).run_if(in_state(crate::PlayMode::Live)).run_if(resource_exists::<LiveHud>));
    }
}

const SIM_NAME: u32 = 0xef98_4401;
const AGE_CURRENT: u32 = 0xef98_4403;
const AGE_NEXT: u32 = 0xef98_4404;
const MALE: u32 = 0xef98_4406;
const FEMALE: u32 = 0xef98_4405;
const AGE_BAR: u32 = 0xef98_4407;
/// The age bar's (`FillBarController`) clip window and its full width.
const AGE_BAR_CLIP: u32 = 0x000f_bc02;
const FAVORITES: u32 = 0xef98_4409;
const TRAITS: u32 = 0xef98_440a;
const ZODIAC: u32 = 0xef98_4420;
const ZODIAC_GENDER: u32 = 0xef98_4421;
const FAMILY_TREE: u32 = 0xef98_4410;
const BIO: u32 = 0xef98_4411;
const CELEBRITY: u32 = 0xef98_4412;
/// The days of a vacation (World Adventures), over the age bar.
const TRAVEL_TIME: u32 = 0xef98_4402;
/// A trait entry's (`HUDSimologyPanelEntry`) name and icon button.
const TRAIT_NAME: u32 = 2;
const TRAIT_ICON: u32 = 3;
/// A favourite's (`HUDSimologyPanelFavsEntry`) picture.
const FAVORITE_ICON: u32 = 2;

fn age_icon(age: Age) -> &'static str {
    match age {
        Age::Baby => "cas_basics_i_age_baby_r2",
        Age::Toddler => "cas_basics_i_age_toddler_r2",
        Age::Child => "cas_basics_i_age_child_r2",
        Age::Teen => "cas_basics_i_age_teen_r2",
        Age::YoungAdult => "cas_basics_i_age_yadult_r2",
        Age::Adult => "cas_basics_i_age_adult_r2",
        Age::Elder => "cas_basics_i_age_elderly_r2",
    }
}

fn age_tooltip(age: Age, span: f32, lived: f32, settings: &crate::options::Settings) -> String {
    if !settings.aging {
        return "Aging is disabled.".into();
    }
    match crate::aging::next_age(age) {
        Some(next) if span.is_finite() => {
            let left = crate::aging::remaining_days(span, lived, settings.lifespan.factor());
            format!("{left} day{} until {}", if left == 1 { "" } else { "s" }, age_name(next))
        }
        _ => "Elder — the age bar does not predict the end of a Sim's life.".into(),
    }
}

#[cfg(test)]
mod age_tooltip_tests {
    use super::*;
    use crate::options::{Lifespan, Settings};

    #[test]
    fn countdown_uses_sim_days_and_respects_disabled_aging() {
        let mut settings = Settings::default();
        for (preset, days) in [(Lifespan::Short, 4), (Lifespan::Medium, 8), (Lifespan::Normal, 14), (Lifespan::Long, 30), (Lifespan::Epic, 150)] {
            settings.lifespan = preset;
            assert!(age_tooltip(Age::Teen, 14.0, 0.0, &settings).starts_with(&format!("{days} days until ")));
        }
        settings.lifespan = Lifespan::Normal;
        assert!(age_tooltip(Age::Teen, 14.0, 13.0, &settings).starts_with("1 day until "));
        assert!(age_tooltip(Age::Teen, 14.0, 15.0, &settings).starts_with("0 days until "));
        assert!(!age_tooltip(Age::Elder, 17.0, 30.0, &settings).contains("until"));
        settings.aging = false;
        assert_eq!(age_tooltip(Age::Teen, 14.0, 13.0, &settings), "Aging is disabled.");
    }
}

fn age_name(age: Age) -> &'static str {
    match age {
        Age::Baby => "Baby",
        Age::Toddler => "Toddler",
        Age::Child => "Child",
        Age::Teen => "Teen",
        Age::YoungAdult => "Young Adult",
        Age::Adult => "Adult",
        Age::Elder => "Elder",
    }
}

/// Puts a picture in a window (in place of its own), tinted.
pub fn picture(commands: &mut Commands, win: Entity, h: Handle<Image>, tint: Color) {
    commands.entity(win).despawn_children();
    commands.entity(win).with_children(|c| {
        c.spawn((ImageNode { image: h, color: tint, ..default() }, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
    });
}

/// Where an `ItemGrid`'s cell goes (left to right, then down).
pub fn cell_area(g: &s3bake::ui::UiGrid, i: usize, size: Vec2) -> [f32; 4] {
    let cols = g.columns.max(1) as usize;
    let (col, row) = (i % cols, i / cols);
    let (x, y) = (g.padding[0] + col as f32 * g.cell[0], g.padding[1] + row as f32 * g.cell[1]);
    [x, y, x + size.x, y + size.y]
}

/// A tooltip on a window, which takes the pointer for it.
pub fn tip(commands: &mut Commands, e: Entity, t: String) {
    commands.entity(e).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld, crate::icons::Tooltip(t)));
}

/// Fresh cells for a grid: a holder under it (cleared), its geometry, and the cell template.
fn grid_holder(commands: &mut Commands, grid: Entity, holder: &mut Option<Entity>, vis: &Query<&mut Visibility>) -> Entity {
    match holder.filter(|h| vis.contains(*h)) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => {
            let h = commands
                .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Visibility::Inherited, Pickable::IGNORE, ChildOf(grid)))
                .id();
            *holder = Some(h);
            h
        }
    }
}

/// The Simology panel.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn simology_panel(
    mut commands: Commands,
    hud: Res<LiveHud>,
    ui: Option<ResMut<UiAssets>>,
    (mut assets, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    sel: Query<(Entity, &Sim, Option<&crate::aging::Aging>), With<Selected>>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    (mut vis, mut nodes, mut texts): (Query<&mut Visibility>, Query<&mut Node>, Query<&mut Text>),
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut relations: ResMut<crate::relations::RelationsPanel>,
    mut state: Local<(Option<Entity>, Option<Entity>, String)>,
    panel: Res<InfoPanel>,
    settings: Res<crate::options::Settings>,
) {
    let s: &Spawned = &hud.simology;
    let (Some(mut ui), Ok((e, sim, aging))) = (ui, sel.single()) else { return };
    // (The family tree, beside the relationships.)
    if pressed(&clicks, s.id(FAMILY_TREE)) {
        relations.open = true;
    }
    if *panel != InfoPanel::Simology {
        return;
    }
    set_text(&mut texts, s.text(SIM_NAME), &sim.full_name());
    // The age bar: days through this life stage, from its icon to the next one's.
    let span = match (sim.age, aging) {
        (Age::Elder, Some(a)) => a.elder_span,
        (age, _) => crate::aging::stage_days(age),
    };
    let lived = aging.map_or(0.0, |a| a.days);
    let f = if span.is_finite() && span > 0.0 { (lived / span).clamp(0.0, 1.0) } else { 1.0 };
    let full = ui.layout("HUDSimologyPanel").and_then(|l| l.find(AGE_BAR)).map_or(90.0, |w| w.area[2] - w.area[0]);
    if let Some(c) = s.within(s.id(AGE_BAR).unwrap_or(Entity::PLACEHOLDER), AGE_BAR_CLIP)
        && let Ok(mut n) = nodes.get_mut(c)
    {
        let w = Val::Px((f * full).round());
        if n.width != w {
            n.width = w;
        }
    }
    // (Babies show whether they're a boy or a girl; the zodiac isn't kept.)
    let baby = sim.age == Age::Baby;
    set_visible(&mut vis, s.id(MALE), baby && !sim.female);
    set_visible(&mut vis, s.id(FEMALE), baby && sim.female);
    for id in [ZODIAC, ZODIAC_GENDER, BIO, CELEBRITY, TRAVEL_TIME] {
        set_visible(&mut vis, s.id(id), false);
    }
    let next = crate::aging::next_age(sim.age);
    set_visible(&mut vis, s.id(AGE_NEXT), next.is_some());
    // What changes rarely is redrawn on change.
    let key = format!("{e:?} {:?} {:?} {:?} {} {} {:?} {}", sim.age, sim.traits, (&sim.favorites.food, &sim.favorites.music, &sim.favorites.color), lived.to_bits(), game_ui.is_some(), settings.lifespan, settings.aging);
    if state.2 == key && state.0.is_some_and(|h| vis.contains(h)) {
        return;
    }
    state.2 = key;
    for (id, age) in [(AGE_CURRENT, Some(sim.age)), (AGE_NEXT, next)] {
        let (Some(win), Some(age)) = (s.id(id), age) else { continue };
        let t = ui.layout("HUDSimologyPanel").and_then(|l| l.find(id)).map_or(Color::WHITE, |w| crate::layout::color(w.shade));
        if let Some((h, _)) = ui.image(&mut assets, s3pkg::fnv64(age_icon(age))) {
            picture(&mut commands, win, h, t);
        }
        tip(&mut commands, win, age_name(age).to_string());
    }
    if let Some(bar) = s.id(AGE_BAR) {
        tip(&mut commands, bar, age_tooltip(sim.age, span, lived, &settings));
    }
    // The traits, a line each: the trait's small icon and its name, its meaning on hover.
    let Some(gu) = game_ui.as_deref_mut() else { return };
    if let (Some(grid), Some(g), Some(template)) = (s.id(TRAITS), ui.layout("HUDSimologyPanel").and_then(|l| l.find(TRAITS)).and_then(|w| w.grid), ui.layout("HUDSimologyPanelEntry").cloned()) {
        let holder = grid_holder(&mut commands, grid, &mut state.0, &vis);
        for (i, t) in sim.traits.iter().enumerate() {
            let mut entry = template.clone();
            entry.area = cell_area(&g, i, Vec2::new(entry.area[2] - entry.area[0], entry.area[3] - entry.area[1]));
            let c = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &entry, holder);
            let info = gu.trait_info(*t);
            let (name, desc) = info.as_ref().map_or((t.name().to_string(), String::new()), |i| (i.name.clone(), i.desc.replace("{0.SimFirstName}", &sim.first)));
            if let Some(txt) = c.text(TRAIT_NAME) {
                commands.entity(txt).insert(Text::new(name.clone()));
            }
            let icon = info.as_ref().and_then(|i| gu.icon(&mut assets, &i.icon_small).or_else(|| gu.icon(&mut assets, &i.icon)));
            // (The icon in the button's icon place.)
            if let (Some(b), Some(h)) = (c.id(TRAIT_ICON), icon) {
                commands.entity(b).insert(crate::layout::SetIcon(h));
            }
            if let Some(r) = c.root {
                tip(&mut commands, r, if desc.is_empty() { name } else { format!("{name}\n{desc}") });
            }
        }
    }
    // The favourites: food, music and colour.
    if !sim.age.is_little()
        && let (Some(grid), Some(g), Some(template)) = (s.id(FAVORITES), ui.layout("HUDSimologyPanel").and_then(|l| l.find(FAVORITES)).and_then(|w| w.grid), ui.layout("HUDSimologyPanelFavsEntry").cloned())
    {
        let holder = grid_holder(&mut commands, grid, &mut state.1, &vis);
        let f = &sim.favorites;
        let food = gu.data.recipes.iter().find(|r| r.key == f.food).map_or(f.food.clone(), |r| r.name.clone());
        let items = [
            (crate::sim::Favorites::icon("food", &f.food), format!("Favorite Food: {food}")),
            (crate::sim::Favorites::icon("music", &f.music), format!("Favorite Music: {}", crate::sim::Favorites::music_name(&f.music))),
            (crate::sim::Favorites::icon("color", &f.color), format!("Favorite Color: {}", crate::sim::Favorites::color_name(&f.color))),
        ];
        for (i, (icon, name)) in items.into_iter().enumerate() {
            let mut entry = template.clone();
            entry.area = cell_area(&g, i, Vec2::new(entry.area[2] - entry.area[0], entry.area[3] - entry.area[1]));
            let c = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &entry, holder);
            if let (Some(w), Some(h)) = (c.id(FAVORITE_ICON), gu.icon(&mut assets, &icon)) {
                picture(&mut commands, w, h, Color::WHITE);
                tip(&mut commands, w, name);
            }
        }
    }
}

const CAREER_PANEL: u32 = 0x0652_5f00;
const CAREER_INFO: u32 = 0x0652_5f01;
const CAREER_ICON: u32 = 0x0652_5f02;
const CAREER_TITLE: u32 = 0x0652_5f03;
const CAREER_WAGE: u32 = 0x0652_5f04;
const CAREER_HOURS: u32 = 0x0652_5f05;
const CAREER_TIME_TILL: u32 = 0x0652_5f06;
const CAREER_UNEMPLOYED: u32 = 0x0652_5f07;
const CAREER_UNEMPLOYED_DESC: u32 = 0x0652_5f08;
const CAREER_TODDLER: u32 = 0x0652_5f09;
const CAREER_BABY: u32 = 0x0652_5f0a;
const CAREER_NO_SCHOOL: u32 = 0x0652_5f0b;
const CAREER_BOARDING: u32 = 0x0652_5f0c;
/// The day letters, Sunday first, and the marks under the working days.
const CAREER_DAY_TEXT: u32 = 0x0652_5f41;
const CAREER_DAY_MARK: u32 = 0x0652_5f51;
const CAREER_HOLIDAY: u32 = 0x0652_5f71;
const CAREER_PERFORMANCE: u32 = 0x0652_5f11;
const CAREER_FILL_UP: u32 = 0x0652_5f12;
const CAREER_FILL_DOWN: u32 = 0x0652_5f14;
const CAREER_ARROWS: [u32; 4] = [0x0652_5f16, 0x0652_5f17, 0x0652_5f1a, 0x0652_5f1b];
const CAREER_PERF_TITLE: u32 = 0x0652_5f1c;
const CAREER_GRADE_TITLE: u32 = 0x0652_5f1d;
const CAREER_FACTORS: u32 = 0x0652_5f18;
const CAREER_PERF_TIP: u32 = 0x0652_5f19;
const CAREER_GO: u32 = 0x0652_5f23;
const CAREER_HISTORY: u32 = 0x0652_5f60;
const DEGREE_HISTORY: u32 = 0x0e6a_f650;
const ACTIVE_CAREER: u32 = 0x0919_0b11;
/// The inner tabs: main, career history, school, job tracker, afterschool, degree history.
const CAREER_TAB_MAIN: u32 = 0x0652_5f20;
const CAREER_TABS_HIDDEN: [u32; 5] = [0x0652_5f21, 0x0652_5f22, 0x0652_5f24, 0x0652_5f25, 0x0e68_1f70];

/// The pie menu's work tones (`Tone: <label>`).
pub const TONE_PREFIX: &str = "Tone: ";

/// Opens the work tones for a Sim at work, at a point on screen.
pub fn open_tones(commands: &mut Commands, pie: &mut crate::hud::PieMenu, sim: Entity, job: &crate::careers::Job, at: Vec2) {
    let skill = job.career().skill;
    let options = crate::careers::WorkTone::ALL
        .into_iter()
        .map(|t| (format!("{TONE_PREFIX}{}{}", t.label(skill), if t == job.tone { " ✓" } else { "" }), crate::interact::ActionKind::EatHere))
        .collect();
    pie.submenus.clear();
    pie.at = at;
    crate::hud::open_pie(commands, pie, at, &format!("At Work: {}", job.info().title), sim, options);
}

/// The Career panel: the job's icon, title, pay, hours and days, the time till work, the
/// performance meter (up and down from the middle); or school (and its grade), or the game's
/// words for having no job. At work, its button chooses how they work.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn career_panel(
    mut commands: Commands,
    hud: Res<LiveHud>,
    ui: Option<ResMut<UiAssets>>,
    mut assets: ResMut<Assets<Image>>,
    sel: Query<(Entity, &Sim, Option<&crate::careers::Job>, Has<crate::careers::AtWork>, Option<&crate::rabbitholes::SchoolGrades>), With<Selected>>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    (mut vis, mut texts, mut bars): (Query<&mut Visibility>, Query<&mut Text>, Query<&mut crate::layout::UiFillBar>),
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    (clock, mut pie, windows): (Res<crate::clock::GameClock>, ResMut<crate::hud::PieMenu>, Query<&Window, With<bevy::window::PrimaryWindow>>),
    mut state: Local<String>,
    panel: Res<InfoPanel>,
) {
    let s: &Spawned = &hud.career;
    let (Some(mut ui), Ok((e, sim, job, at_work, grades))) = (ui, sel.single()) else { return };
    if *panel != InfoPanel::Career {
        return;
    }
    // At work, the button chooses how they work (as the game's rabbit hole menu).
    if pressed(&clicks, s.id(CAREER_GO))
        && let Some(j) = job
        && at_work
    {
        let at = windows.single().ok().and_then(|w| w.cursor_position()).unwrap_or(Vec2::new(600.0, 600.0));
        open_tones(&mut commands, &mut pie, e, j, at);
    }
    for id in [ACTIVE_CAREER, CAREER_HISTORY, DEGREE_HISTORY, CAREER_FACTORS, CAREER_NO_SCHOOL, CAREER_BOARDING] {
        set_visible(&mut vis, s.id(id), false);
    }
    for id in CAREER_TABS_HIDDEN.into_iter().chain(CAREER_ARROWS) {
        set_visible(&mut vis, s.id(id), false);
    }
    for d in 0..7 {
        set_visible(&mut vis, s.id(CAREER_HOLIDAY + d), false);
    }
    set_visible(&mut vis, s.id(CAREER_PANEL), true);
    let school = job.is_none() && matches!(sim.age, Age::Child | Age::Teen);
    let little = matches!(sim.age, Age::Baby | Age::Toddler);
    let working = job.is_some() || school;
    set_visible(&mut vis, s.id(CAREER_INFO), working);
    set_visible(&mut vis, s.id(CAREER_PERFORMANCE), working);
    set_visible(&mut vis, s.id(CAREER_UNEMPLOYED), !working && !little);
    set_visible(&mut vis, s.id(CAREER_UNEMPLOYED_DESC), !working && !little);
    set_visible(&mut vis, s.id(CAREER_TODDLER), sim.age == Age::Toddler);
    set_visible(&mut vis, s.id(CAREER_BABY), sim.age == Age::Baby);
    set_visible(&mut vis, s.id(CAREER_GRADE_TITLE), school);
    set_visible(&mut vis, s.id(CAREER_PERF_TITLE), job.is_some());
    if !working {
        return;
    }
    // What the job is.
    let (title, wage, start, end, days, performance) = match job {
        Some(j) => {
            let l = j.info();
            let path = j.branch_label().map_or(String::new(), |b| format!(" ({b})"));
            (format!("{}{path}", l.title), format!("§{}/hour", l.hourly), l.start, l.end, l.days, j.performance)
        }
        None => {
            let g = grades.copied().unwrap_or_default();
            let name = if sim.age == Age::Child { "Elementary School" } else { "High School" };
            let (start, end) = crate::rabbitholes::school_hours(sim.age);
            (name.to_string(), String::new(), start, end, 0b0001_1111, g.performance())
        }
    };
    set_text(&mut texts, s.text(CAREER_TITLE), &title);
    set_text(&mut texts, s.text(CAREER_WAGE), &wage);
    let hour = |h: f32| crate::interact::hour_label(h).to_ascii_lowercase();
    set_text(&mut texts, s.text(CAREER_HOURS), &format!("{} - {}", hour(start), hour(end)));
    if school {
        set_text(&mut texts, s.text(CAREER_GRADE_TITLE), &format!("Grade: {}", grades.copied().unwrap_or_default().letter()));
    }
    // The days: our week starts on Monday (bit 0); the game's letters on Sunday.
    for d in 0..7u32 {
        let bit = (d + 6) % 7;
        set_visible(&mut vis, s.id(CAREER_DAY_MARK + d), days & (1 << bit) != 0);
    }
    // The time till work.
    let now = clock.hour_f();
    let today = clock.weekday();
    let till = if at_work {
        if school { "At school now".to_string() } else { "At work now".to_string() }
    } else {
        // (The next working day's start.)
        let mut hours = None;
        for k in 0..8 {
            let day = (today + k) % 7;
            if days & (1 << day) == 0 {
                continue;
            }
            let h = k as f32 * 24.0 + start - now;
            if h > 0.0 {
                hours = Some(h);
                break;
            }
        }
        let place = if school { "School" } else { "Work" };
        match hours {
            Some(h) if h < 1.0 => format!("{place} in: {} Minutes", (h * 60.0).ceil() as i32),
            Some(h) if h < 24.0 => format!("{place} in: {} Hour{}", h.floor() as i32, if h.floor() as i32 == 1 { "" } else { "s" }),
            Some(h) => format!("{place} in: {} Day{}", (h / 24.0).floor() as i32, if (h / 24.0).floor() as i32 == 1 { "" } else { "s" }),
            None => String::new(),
        }
    };
    set_text(&mut texts, s.text(CAREER_TIME_TILL), &till);
    // The performance meter: up from the middle when doing well, down when not.
    for (id, v) in [(CAREER_FILL_UP, (performance / 100.0).max(0.0)), (CAREER_FILL_DOWN, (-performance / 100.0).max(0.0))] {
        if let Some(b) = s.id(id)
            && let Ok(mut bar) = bars.get_mut(b)
            && (bar.value - v).abs() > 1e-3
        {
            bar.value = v;
        }
    }
    // The button: off to work (or, there, how to work).
    set_visible(&mut vis, s.id(CAREER_GO), job.is_some() && at_work);
    // What changes rarely is set on change: the icon and the tooltips.
    let key = format!("{e:?} {:?} {:?} {school} {}", job.map(|j| (j.track, j.level, j.branch, j.tone)), (performance / 5.0) as i32, game_ui.is_some());
    if *state == key {
        return;
    }
    *state = key;
    if let (Some(j), Some(win), Some(gu)) = (job, s.id(CAREER_ICON), game_ui.as_deref_mut())
        && let Some(h) = gu.icon(&mut assets, j.career().icon)
    {
        let tint = ui.layout("HUDCareerPanel").and_then(|l| l.find(CAREER_ICON)).map_or(Color::WHITE, |w| crate::layout::color(w.shade));
        picture(&mut commands, win, h, tint);
        tip(&mut commands, win, j.career().name.to_string());
    }
    if let Some(m) = s.id(CAREER_PERF_TIP) {
        let t = match job {
            Some(j) => {
                let next = j.levels().get(j.level + 1).map_or("the top of the career".to_string(), |n| n.title.to_string());
                format!("Performance: {:+.0}\nRaise it with {} skill and a good mood at work, towards {next}.\nWorking: {}", j.performance, j.career().skill, j.tone.label(j.career().skill))
            }
            None => format!("Grade: {}\nDoing homework raises it.", grades.copied().unwrap_or_default().letter()),
        };
        tip(&mut commands, m, t);
    }
    if let Some(g) = s.id(CAREER_GO) {
        tip(&mut commands, g, "Choose how to work".to_string());
    }
    let _ = &mut ui;
}

const INV_GRID: u32 = 0xf780_0304;
const INV_DRAG_HERE: u32 = 0xf780_0305;
const INV_PHONE: u32 = 0x06ef_61c0;
const INV_JOURNAL: u32 = 0x0d9b_da80;
const INV_ALMANAC: u32 = 0x0f8a_6250;
/// An item's (`HUDInventoryItemWin`) picture and stack count.
const ITEM_THUMB: u32 = 0x066b_7e02;
const ITEM_COUNT: u32 = 0x066b_7e01;

/// An inventory cell: the stack it shows.
#[derive(Component)]
struct InventoryCell(usize);

/// The Inventory panel: the Sim's stacks in the game's item cells (the picture, how many),
/// four across, scrolled with the wheel; clicked, the stack's pie menu (sell, eat, hang,
/// place). Its phone opens the phone, its journal the collection journal.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn inventory_panel(
    mut commands: Commands,
    hud: Res<LiveHud>,
    ui: Option<ResMut<UiAssets>>,
    (mut assets, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    sel: Query<(Entity, &Sim, Option<&crate::inventory::Inventory>), With<Selected>>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    (mut vis, mut nodes): (Query<&mut Visibility>, Query<&mut Node>),
    (clicks, cells, hovered): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &InventoryCell), Changed<Interaction>>, Query<&Interaction>),
    (baked, mut thumbs, mut pictures): (Option<Res<crate::baked::Baked>>, ResMut<crate::thumbs::ModelThumbs>, ResMut<crate::paintings::PaintingImages>),
    (mut pie, mut chosen, windows): (ResMut<crate::hud::PieMenu>, ResMut<crate::inventory::Chosen>, Query<&Window, With<bevy::window::PrimaryWindow>>),
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut state: Local<(Option<Entity>, String, f32)>,
    panel: Res<InfoPanel>,
) {
    let s: &Spawned = &hud.inventory;
    let (Some(mut ui), Ok((me, sim, inv))) = (ui, sel.single()) else { return };
    if pressed(&clicks, s.id(INV_PHONE)) {
        commands.insert_resource(crate::hud::OpenPhone);
    }
    for id in [INV_ALMANAC, INV_DRAG_HERE] {
        set_visible(&mut vis, s.id(id), false);
    }
    // (Toddlers and babies have no phone.)
    set_visible(&mut vis, s.id(INV_PHONE), !sim.age.is_little());
    let stacks: Vec<crate::inventory::Stack> = inv.map(|i| i.0.clone()).unwrap_or_default();
    // A stack clicked: its pie menu.
    for (i, c) in &cells {
        if *i == Interaction::Pressed
            && let Some(st) = stacks.get(c.0)
        {
            chosen.select(c.0, st);
            let options = crate::inventory::stack_actions(sim, st)
                .into_iter()
                .map(|(label, b)| {
                    let kind = match b {
                        crate::inventory::ItemButton::Eat => crate::interact::ActionKind::EatItem { key: st.key.clone(), quality: st.quality },
                        b => crate::interact::ActionKind::InventoryItem(b, me, st.clone()),
                    };
                    (label, kind)
                })
                .collect::<Vec<_>>();
            if !options.is_empty() {
                let at = windows.single().ok().and_then(|w| w.cursor_position()).unwrap_or(Vec2::new(600.0, 600.0));
                pie.submenus.clear();
                pie.at = at;
                crate::hud::open_pie(&mut commands, &mut pie, at, &st.name, me, options);
            }
        }
    }
    let Some(grid) = s.id(INV_GRID) else { return };
    let Some(g) = ui.layout("HUDInventoryPanel").and_then(|l| l.find(INV_GRID)).and_then(|w| w.grid) else { return };
    // Scrolling a row at a time with the wheel over the grid.
    let rows = stacks.len().div_ceil(g.columns.max(1) as usize) as f32;
    let over = hovered.get(grid).is_ok_and(|i| *i != Interaction::None);
    for w in wheel.read() {
        if over && *panel == InfoPanel::Inventory {
            state.2 = (state.2 - w.y.signum()).clamp(0.0, (rows - g.rows as f32).max(0.0));
        }
    }
    if let Some(h) = state.0
        && let Ok(mut n) = nodes.get_mut(h)
    {
        let top = Val::Px(-state.2 * g.cell[1]);
        if n.top != top {
            n.top = top;
        }
    }
    let key = format!("{me:?} {:?} {}", stacks.iter().map(|x| (&x.key, x.quality, x.count)).collect::<Vec<_>>(), game_ui.is_some());
    if state.1 == key && state.0.is_some_and(|h| vis.contains(h)) {
        return;
    }
    state.1 = key;
    state.2 = state.2.min((rows - g.rows as f32).max(0.0));
    let holder = grid_holder(&mut commands, grid, &mut state.0, &vis);
    commands.entity(grid).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
    let Some(template) = ui.layout("HUDInventoryItemWin").cloned() else { return };
    for (i, st) in stacks.iter().enumerate() {
        let mut cell = template.clone();
        cell.area = cell_area(&g, i, Vec2::new(cell.area[2] - cell.area[0], cell.area[3] - cell.area[1]));
        let c = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &cell, holder);
        if let Some(t) = c.id(ITEM_THUMB)
            && let Some((h, rect)) = crate::inventory::stack_picture(st, game_ui.as_deref_mut(), &mut pictures, &mut assets, baked.as_deref(), &mut thumbs)
        {
            commands.entity(t).despawn_children();
            commands.entity(t).with_children(|p| {
                p.spawn((ImageNode { image: h, rect, ..default() }, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
            });
        }
        if let Some(n) = c.id(ITEM_COUNT) {
            commands.entity(n).insert(if st.count > 1 { Visibility::Inherited } else { Visibility::Hidden });
        }
        if let Some(t) = c.text(ITEM_COUNT) {
            commands.entity(t).insert(Text::new(st.count.to_string()));
        }
        if let Some(r) = c.root {
            let worth = if st.count > 1 { format!("§{} each, §{} in all", st.each(), st.worth) } else { format!("Worth §{}", st.worth) };
            commands.entity(r).remove::<Pickable>().insert((
                Button,
                InventoryCell(i),
                crate::hud::BlocksWorld,
                crate::icons::Tooltip(format!("{}{}\n{worth}", st.name, if st.count > 1 { format!(" ×{}", st.count) } else { String::new() })),
            ));
        }
    }
}
