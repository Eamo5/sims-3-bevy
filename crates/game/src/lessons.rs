//! Lessons, the game's tutorialettes (`Tutorialette`, `TutorialetteNotification`,
//! `TutorialetteDialog`): the first time something comes up (Buy mode, a skill, a job, the
//! bills, a pregnancy...), a note asks whether to learn about it: "Yes" opens its lesson, "Later"
//! puts it off (it may come up again, half an hour on), "Never" puts it away. A lesson's seen
//! once it's opened or refused, and kept so in the options; no two come up within a quarter of an
//! hour, unless the lesson's one that ignores that. The Lessons dialog (the puck's menu, or the
//! note) lists them all by name, each with its pages of text and pictures. The options' "Enable
//! Lessons" turns the notes off; "Reset Lessons" has them come up again.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::PlayMode;
use crate::layout::{Spawned, UiAssets};
use crate::livehud::{pressed, set_text, set_visible};
use crate::sim::{Age, HouseholdMember, Sim};

pub struct LessonsPlugin;

impl Plugin for LessonsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Lesson>()
            .init_resource::<Cooldowns>()
            .add_systems(Update, (watch, lessons_test, show_notice, notice_buttons, open_dialog, dialog_controls).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(crate::AppState::InGame), |mut commands: Commands| {
                commands.remove_resource::<Notice>();
                commands.remove_resource::<LessonsDialog>();
            });
    }
}

/// Something come up a lesson's about (by its `Lessons` name, e.g. "BuyMode").
#[derive(Message, Clone, Debug)]
pub struct Lesson(pub &'static str);

/// Asked for: the Lessons dialog, at a lesson (or none chosen).
#[derive(Resource)]
pub struct OpenLessons(pub Option<String>);

/// The game's tunables: no two notes within this long (game minutes, unless a lesson ignores
/// it), nor one again within that; and how long a note stays up (real seconds).
const GLOBAL_COOLDOWN: f64 = 15.0;
const LESSON_COOLDOWN: f64 = 30.0;
const NOTICE_SECONDS: f32 = 10.0;

/// When a note last came up, and each lesson's (this session).
#[derive(Resource, Default)]
struct Cooldowns {
    last: Option<f64>,
    lessons: HashMap<String, f64>,
}

/// The note up: its windows, the lesson, and when it came.
#[derive(Resource)]
struct Notice {
    s: Spawned,
    key: String,
    since: f32,
}

// `TutorialetteNotification`'s: its text and buttons.
const NOTICE_BOX: u32 = 1;
const NOTICE_TEXT: u32 = 6;
const NOTICE_YES: u32 = 3;
const NOTICE_LATER: u32 = 4;
const NOTICE_NEVER: u32 = 5;
// `TutorialetteDialog`'s: the list, the title and page text, the picture, the pages' "n/m" and
// arrows, and close; `TutorialetteEntry`'s name and shading, and its base game picture.
const LIST: u32 = 0x10;
const TITLE: u32 = 3;
const BODY: u32 = 9;
const PICTURE: u32 = 8;
const PAGES: u32 = 7;
const PAGE_TEXT: u32 = 4;
const PREVIOUS: u32 = 6;
const NEXT: u32 = 5;
const CLOSE: u32 = 2;
const ENTRY_TEXT: u32 = 0xc000_0011;
const ENTRY_SHADE: u32 = 0xc000_0012;
const ENTRY_BASE_ICON: u32 = 1;
const ROWS: usize = 10;

/// The game's text with its line breaks (written `\n`, or `\\n`, in its tables).
pub fn unescape(s: &str) -> String {
    s.replace("\\\\n", "\n").replace("\\n", "\n")
}

/// The data's lessons (the game's base ones).
fn lessons(gu: &crate::icons::GameUi) -> &[s3bake::gamedata::LessonInfo] {
    &gu.data.lessons
}

/// Whether a lesson's note may come up now: notes on, not seen, the cooldowns passed.
pub fn may_come_up(settings: &crate::options::Settings, cool: (Option<f64>, Option<f64>), ignores_global: bool, key: &str, now: f64) -> bool {
    settings.lessons
        && !settings.lessons_seen.iter().any(|k| k == key)
        && (ignores_global || cool.0.is_none_or(|t| now - t >= GLOBAL_COOLDOWN))
        && cool.1.is_none_or(|t| now - t > LESSON_COOLDOWN)
}

/// What comes up, as the game's triggers have it: Buy and Build mode, the Sim panel's tabs, a
/// skill, a job, the paper, the bills, a pregnancy, a baby, a birthday, a friend, romance, a
/// conversation, the game's speed, and (once in) how long life stages last.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn watch(
    mut out: MessageWriter<Lesson>,
    time: Res<Time>,
    mut started: Local<Option<f32>>,
    mut was: Local<(bool, bool, Option<crate::simpanel::SimTab>, usize, bool)>,
    (buy, tab, clock, household): (Res<crate::buy::BuyMode>, Option<Res<crate::simpanel::SimTab>>, Res<crate::clock::GameClock>, Option<Res<crate::interact::Household>>),
    mut life: MessageReader<crate::life::LifeEvent>,
    members: Query<(&Sim, Has<crate::sim::Selected>), With<HouseholdMember>>,
    (pregnant, born, paper): (
        Query<(), (Added<crate::little::Pregnancy>, With<HouseholdMember>)>,
        Query<&Sim, (Added<Sim>, With<HouseholdMember>)>,
        Query<&crate::interact::GameObject, (Added<crate::interact::GameObject>, Without<crate::visit::LotObject>)>,
    ),
) {
    let now = time.elapsed_secs();
    let t0 = *started.get_or_insert(now);
    // (Not what a save brings in with it: a few seconds in.)
    let settled = now - t0 > 6.0;
    let mut say = |k: &'static str| {
        if settled {
            out.write(Lesson(k));
        }
    };
    if settled && !was.4 {
        was.4 = true;
        say("AgingStageLengths");
    }
    let building = buy.active && buy.category >= crate::buy::WALLPAPER_TAB;
    let buying = buy.active && !building;
    if buying && !was.0 {
        say("BuyMode");
    }
    if building && !was.1 {
        say("BuildMode");
    }
    (was.0, was.1) = (buying, building);
    if let Some(tab) = tab.as_deref().copied()
        && was.2 != Some(tab)
    {
        if was.2.is_some() {
            use crate::simpanel::SimTab as T;
            say(match tab {
                T::Needs => "Motives",
                T::Skills => "SkillTracking",
                T::Career => "Jobs",
                T::Simology => "Traits",
                T::Inventory => "Inventory",
            });
        }
        was.2 = Some(tab);
    }
    if clock.speed != was.3 {
        if settled && was.3 != 0 && clock.speed != 0 {
            say("TimeControl");
        }
        was.3 = clock.speed;
    }
    if household.is_some_and(|h| h.is_changed() && !h.bills.is_empty()) {
        say("PayingBills");
    }
    for e in life.read() {
        let Ok((sim, selected)) = members.get(e.sim) else { continue };
        use crate::life::LifeEventKind as K;
        match &e.kind {
            K::SkillUp { .. } => say("SkillBuilding"),
            K::NewJob => say("JobProgression"),
            K::MadeFriend { .. } => say("Relationships"),
            K::FirstKiss | K::StartedDating => say("RomanticMoments"),
            K::Socialized { .. } if selected => say("Socialization"),
            K::Birthday => {
                say("Aging");
                match sim.age {
                    Age::Toddler => say("Toddlers"),
                    Age::Child => say("Children"),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if !pregnant.is_empty() {
        say("Pregnancy");
    }
    if born.iter().any(|s| s.age == Age::Baby) {
        say("Babies");
    }
    if paper.iter().any(|o| o.kind == crate::interact::ObjectKind::Newspaper) {
        say("Jobs");
    }
}

/// LESSON=<name> (tests): that lesson's note brought up, a few seconds in; LESSONS=1, the
/// dialog opened.
fn lessons_test(mut commands: Commands, time: Res<Time>, mut done: Local<bool>, mut out: MessageWriter<Lesson>) {
    if *done || time.elapsed_secs() < 9.0 {
        return;
    }
    *done = true;
    if let Ok(k) = std::env::var("LESSON") {
        out.write(Lesson(Box::leak(k.into_boxed_str())));
    }
    if std::env::var_os("LESSONS").is_some() {
        commands.insert_resource(OpenLessons(std::env::var("LESSONS").ok().filter(|v| v != "1")));
    }
}

/// A lesson come up: its note, if it may.
#[allow(clippy::too_many_arguments)]
fn show_notice(
    mut commands: Commands,
    mut asked: MessageReader<Lesson>,
    (time, clock, settings): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::options::Settings>),
    mut cool: ResMut<Cooldowns>,
    notice: Option<Res<Notice>>,
    dialog: Option<Res<LessonsDialog>>,
    (gu, ui): (Option<Res<crate::icons::GameUi>>, Option<ResMut<UiAssets>>),
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
) {
    let (Some(gu), Some(mut ui)) = (gu, ui) else {
        asked.clear();
        return;
    };
    for Lesson(key) in asked.read() {
        if notice.is_some() || dialog.is_some() {
            continue;
        }
        let Some(l) = lessons(&gu).iter().find(|l| l.key == *key && !l.tns.is_empty()) else { continue };
        let now = clock.minutes;
        if !may_come_up(&settings, (cool.last, cool.lessons.get(*key).copied()), l.ignore_cooldown, key, now) {
            continue;
        }
        cool.last = Some(now);
        cool.lessons.insert(key.to_string(), now);
        // (Its words in it as it's put up, the note as tall as they need: the game sizes it to
        // its text.)
        let Some(mut layout) = ui.layout("TutorialetteNotification").cloned() else { return };
        let words = unescape(&l.tns);
        let lines = words.split('\n').map(|p| p.chars().count().div_ceil(34).max(1)).sum::<usize>();
        let extra = lines.saturating_sub(2) as f32 * 17.0;
        layout.area[3] += extra;
        crate::layout::edit_windows(&mut layout, NOTICE_BOX, &mut |w| w.area[3] += extra);
        crate::layout::edit_windows(&mut layout, NOTICE_TEXT, &mut |t| {
            t.caption = words.clone();
            t.area[3] += extra;
        });
        let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &layout);
        if let Some(root) = s.root {
            // (Up on the right, under the notices, as the game's lessons tab is.)
            commands.entity(root).insert((GlobalZIndex(40), Visibility::Inherited, DespawnOnExit(crate::AppState::InGame)));
            commands.entity(root).entry::<Node>().and_modify(|mut n| {
                n.left = Val::Auto;
                n.right = Val::Px(24.0);
                n.top = Val::Px(150.0);
            });
        }
        info!("lesson: {} comes up", l.key);
        commands.insert_resource(Notice { s, key: key.to_string(), since: time.elapsed_secs() });
        break;
    }
}

/// The note's buttons: "Yes" (the lesson opened, seen), "Later", "Never" (seen); gone by itself
/// after a while.
fn notice_buttons(
    mut commands: Commands,
    notice: Option<Res<Notice>>,
    time: Res<Time>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut settings: ResMut<crate::options::Settings>,
) {
    let Some(n) = notice else { return };
    let yes = pressed(&clicks, n.s.id(NOTICE_YES));
    let never = pressed(&clicks, n.s.id(NOTICE_NEVER));
    let later = pressed(&clicks, n.s.id(NOTICE_LATER)) || time.elapsed_secs() - n.since > NOTICE_SECONDS;
    if !(yes || never || later) {
        return;
    }
    if (yes || never) && !settings.lessons_seen.contains(&n.key) {
        settings.lessons_seen.push(n.key.clone());
        settings.save();
    }
    if yes {
        commands.insert_resource(OpenLessons(Some(n.key.clone())));
    }
    if let Some(root) = n.s.root {
        commands.entity(root).try_despawn();
    }
    commands.remove_resource::<Notice>();
}

/// The Lessons dialog on screen: its windows, the lessons in order of name, the one chosen, its
/// page, the list's scroll, and what's shown.
#[derive(Resource)]
struct LessonsDialog {
    s: Spawned,
    backdrop: Entity,
    order: Vec<usize>,
    chosen: Option<usize>,
    page: usize,
    scroll: usize,
    cells: Option<Entity>,
    bar: Option<Entity>,
    speed: usize,
    shown: Option<(Option<usize>, usize, usize)>,
}

/// A lesson's row in the list (its place in the data).
#[derive(Component)]
struct LessonRow(usize);

#[allow(clippy::too_many_arguments)]
fn open_dialog(
    mut commands: Commands,
    want: Option<Res<OpenLessons>>,
    open: Option<Res<LessonsDialog>>,
    (gu, ui): (Option<Res<crate::icons::GameUi>>, Option<ResMut<UiAssets>>),
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut clock: ResMut<crate::clock::GameClock>,
) {
    let Some(want) = want else { return };
    commands.remove_resource::<OpenLessons>();
    let (Some(gu), Some(mut ui)) = (gu, ui) else { return };
    if open.is_some() || ui.layout("TutorialetteDialog").is_none() || ui.layout("TutorialetteEntry").is_none() {
        return;
    }
    // (Behind it the screen darkens, and takes no clicks; the game waits, as the game's does.)
    let backdrop = commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4)),
            GlobalZIndex(69),
            Interaction::default(),
            crate::hud::BlocksWorld,
            DespawnOnExit(crate::AppState::InGame),
        ))
        .id();
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "TutorialetteDialog") else { return };
    let Some(root) = s.root else { return };
    commands.entity(root).insert((GlobalZIndex(70), Visibility::Inherited, ChildOf(backdrop)));
    // (In the middle of the screen.)
    commands.entity(root).entry::<Node>().and_modify(|mut n| {
        n.left = Val::Percent(50.0);
        n.top = Val::Percent(50.0);
        n.margin = UiRect { left: Val::Px(-361.0), top: Val::Px(-267.0), ..default() };
    });
    // The list's scroll bar: the item grid's own, down its right; the list takes the wheel.
    let mut bar = None;
    if let (Some(list), Some(mut b)) = (s.id(LIST), ui.find("ItemGrid", crate::cashair::GRID_SCROLLBAR).cloned()) {
        let size = ui.find("TutorialetteDialog", LIST).map_or(Vec2::new(222.0, 423.0), |g| Vec2::new(g.area[2] - g.area[0], g.area[3] - g.area[1]));
        b.flags |= s3bake::ui::WIN_VISIBLE;
        b.place = s3bake::ui::UiPlace::Fixed;
        b.area = [size.x - 22.0, 2.0, size.x - 2.0, size.y - 2.0];
        bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, list).root;
        commands.entity(list).insert(bevy::ui::RelativeCursorPosition::default());
    }
    let all = lessons(&gu);
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by(|a, b| all[*a].name.cmp(&all[*b].name));
    let chosen = want.0.as_ref().and_then(|k| all.iter().position(|l| &l.key == k));
    let scroll = chosen.and_then(|c| order.iter().position(|o| *o == c)).map_or(0, |at| at.saturating_sub(ROWS / 2));
    let speed = clock.speed;
    clock.set_speed(0);
    commands.insert_resource(LessonsDialog { s, backdrop, order, chosen, page: 0, scroll, cells: None, bar, speed, shown: None });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dialog_controls(
    mut commands: Commands,
    dialog: Option<ResMut<LessonsDialog>>,
    (gu, ui): (Option<ResMut<crate::icons::GameUi>>, Option<ResMut<UiAssets>>),
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (clicks, rows): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &LessonRow), Changed<Interaction>>),
    (mut vis, mut texts, mut bars, over): (Query<&mut Visibility>, Query<&mut Text>, Query<&mut crate::layout::UiScrollBar>, Query<&bevy::ui::RelativeCursorPosition>),
    (keys, wheel): (Res<ButtonInput<KeyCode>>, Res<bevy::input::mouse::AccumulatedMouseScroll>),
    mut clock: ResMut<crate::clock::GameClock>,
    mut sounds: MessageWriter<crate::sound::PlaySound>,
) {
    let (Some(mut d), Some(mut gu), Some(mut ui)) = (dialog, gu, ui) else { return };
    let d = &mut *d;
    if pressed(&clicks, d.s.id(CLOSE)) || keys.just_pressed(KeyCode::Escape) {
        commands.entity(d.backdrop).try_despawn();
        commands.remove_resource::<LessonsDialog>();
        clock.set_speed(d.speed);
        return;
    }
    for (i, r) in &rows {
        if *i == Interaction::Pressed && d.chosen != Some(r.0) {
            d.chosen = Some(r.0);
            d.page = 0;
            sounds.write(crate::sound::PlaySound { name: "ui_tertiary_button".into(), at: None, volume: 1.0, speed: 1.0 });
        }
    }
    let pages = d.chosen.and_then(|c| gu.data.lessons.get(c)).map_or(0, |l| l.pages.len());
    if pressed(&clicks, d.s.id(PREVIOUS)) {
        d.page = d.page.saturating_sub(1);
    }
    if pressed(&clicks, d.s.id(NEXT)) && d.page + 1 < pages {
        d.page += 1;
    }
    // The list scrolled by the wheel or the bar.
    let most = d.order.len().saturating_sub(ROWS);
    let over_list = d.s.id(LIST).and_then(|e| over.get(e).ok()).is_some_and(|c| c.cursor_over());
    let wheeled = over_list && wheel.delta.y != 0.0;
    if wheeled {
        d.scroll = (d.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
    }
    let fresh = d.shown.is_none();
    if let Some(mut bar) = d.bar.and_then(|e| bars.get_mut(e).ok()) {
        if (bar.total, bar.visible) != (d.order.len(), ROWS) {
            (bar.total, bar.visible) = (d.order.len(), ROWS);
        }
        if bar.value != d.scroll {
            if wheeled || fresh {
                bar.value = d.scroll;
            } else {
                d.scroll = bar.value.min(most);
            }
        }
    }
    let want = (d.chosen, d.page, d.scroll);
    if d.shown == Some(want) {
        return;
    }
    let list_changed = d.shown.is_none_or(|s| s.2 != d.scroll || s.0 != d.chosen);
    d.shown = Some(want);
    // The lesson chosen: its title, the page's text and picture, "n/m" and the arrows.
    let lesson = d.chosen.and_then(|c| gu.data.lessons.get(c)).cloned();
    if let Some(l) = &lesson {
        set_text(&mut texts, d.s.text(TITLE), &l.name);
        if let Some((text, picture)) = l.pages.get(d.page) {
            set_text(&mut texts, d.s.text(BODY), &unescape(text));
            if let (Some(win), Some(img)) = (d.s.id(PICTURE), gu.icon(&mut images, picture)) {
                crate::hudpanels::picture(&mut commands, win, img, Color::WHITE);
            }
        }
        set_text(&mut texts, d.s.text(PAGE_TEXT), &format!("{}/{}", d.page + 1, l.pages.len()));
    }
    let many = lesson.as_ref().is_some_and(|l| l.pages.len() > 1);
    set_visible(&mut vis, d.s.id(PAGES), many);
    set_visible(&mut vis, d.s.id(PREVIOUS), many && d.page > 0);
    set_visible(&mut vis, d.s.id(NEXT), many && d.page + 1 < lesson.as_ref().map_or(0, |l| l.pages.len()));
    if !list_changed {
        return;
    }
    // The list: ten rows at a time, every other one shaded, the chosen lit.
    if let Some(old) = d.cells.take() {
        commands.entity(old).try_despawn();
    }
    let (Some(list), Some(g), Some(template)) = (d.s.id(LIST), ui.find("TutorialetteDialog", LIST).and_then(|w| w.grid), ui.layout("TutorialetteEntry").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    commands.entity(list).insert_children(0, &[holder]);
    d.cells = Some(holder);
    let step = g.cell[1] + g.cell_padding[1] + g.cell_padding[3];
    for (row, &i) in d.order.iter().enumerate().skip(d.scroll).take(ROWS) {
        let at = row - d.scroll;
        let (x, y) = (g.padding[0], g.padding[1] + at as f32 * step);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let name = gu.data.lessons.get(i).map(|l| l.name.clone()).unwrap_or_default();
        crate::layout::edit_windows(&mut cell, ENTRY_TEXT, &mut |t| t.caption = name.clone());
        // (Every other row shaded; the base game's picture by each.)
        crate::layout::edit_windows(&mut cell, ENTRY_SHADE, &mut |w| w.flags = if row % 2 == 1 { w.flags | s3bake::ui::WIN_VISIBLE } else { w.flags & !s3bake::ui::WIN_VISIBLE });
        crate::layout::edit_windows(&mut cell, ENTRY_BASE_ICON, &mut |w| w.flags |= s3bake::ui::WIN_VISIBLE);
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = c.root else { continue };
        commands.entity(root).insert(LessonRow(i));
        if d.chosen == Some(i) {
            commands.entity(root).insert(BackgroundColor(Color::srgb_u8(0xce, 0xec, 0xff)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lesson_comes_up_once_and_not_too_often() {
        let mut s = crate::options::Settings::default();
        // (Fresh: up it comes.)
        assert!(may_come_up(&s, (None, None), false, "BuyMode", 100.0));
        // (Another within a quarter of an hour: not, unless it ignores that.)
        assert!(!may_come_up(&s, (Some(95.0), None), false, "BuildMode", 100.0));
        assert!(may_come_up(&s, (Some(95.0), None), true, "Pregnancy", 100.0));
        // (The same again within half an hour: not.)
        assert!(!may_come_up(&s, (Some(60.0), Some(80.0)), false, "BuyMode", 100.0));
        // (Seen, or the notes off: never.)
        s.lessons_seen.push("BuyMode".into());
        assert!(!may_come_up(&s, (None, None), false, "BuyMode", 100.0));
        s.lessons = false;
        assert!(!may_come_up(&s, (None, None), false, "BuildMode", 100.0));
    }
}
