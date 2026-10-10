//! The live-mode HUD in the game's own layouts (`UI.package`, drawn by `layout`): the Sim display
//! (the selected Sim's bust, the time controls, the mood meter and moodlets), the puck (live,
//! buy and build, the camera, walls, floors, funds and the options menu), the skewer (the
//! household's faces, ringed in their moods), and the info panels under their tabs (the
//! motives). Each is driven as the game's own UI code drives it (UI.dll's `SimDisplay`,
//! `MotivesPanel`, `Skewer`, `PuckController`, `TimeControl`, `Navigation`), with the colours
//! of the gameplay tuning (`MoodManager`'s and `Motive`'s kLHS and kRHS ranges, in
//! GameplayData).

use bevy::prelude::*;

use crate::layout::{Spawned, UiAssets, UiButton, UiPicture};
use crate::sim::{HouseholdMember, Motives, Selected, Sim};
use crate::{AppState, PlayMode};

pub struct LiveHudPlugin;

impl Plugin for LiveHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InfoPanel>().add_systems(OnEnter(PlayMode::Live), spawn_live_hud).add_systems(
            Update,
            (time_control, puck, mood_meter, motives_panel, bust, skewer, navigation, show_in_modes.after(navigation), moodlets, wishes, skills_panel, skill_journal_buttons, interaction_queue, notifications).run_if(in_state(PlayMode::Live)).run_if(resource_exists::<LiveHud>),
        );
    }
}

// The windows' control ids, as UI.dll names them.
const EXPAND: u32 = 0x06f5_b800;
const BUST: u32 = 0x06f5_b801;
const MOOD_REFERENCE: u32 = 0x06f5_b840;
const MOOD_BACK: u32 = 0x06f5_b841;
const MOOD_FILL: u32 = 0x06f5_b842;
const MOOD_SIZE: u32 = 0x06f5_b843;
const MOOD_GLOW: u32 = 0x06f5_b844;
const MOOD_HALFWAY: u32 = 0x06f5_b845;
const MOOD_BONUS: u32 = 0x06f5_b846;
/// Pause, normal, double, triple, skip; the time of day.
const TIME_BASE: u32 = 0x47e9_7a00;
const TIME_TEXT: u32 = 0x47e9_7a06;
const PUCK: u32 = 0x8fef_fa00;
const PUCK_BUILD: u32 = PUCK + 0x02;
const PUCK_BUY: u32 = PUCK + 0x03;
const PUCK_LIVE: u32 = PUCK + 0x04;
const PUCK_FUNDS: u32 = PUCK + 0x05;
const PUCK_WALLS: u32 = PUCK + 0x07;
const PUCK_LEVEL_UP: u32 = PUCK + 0x0f;
const PUCK_LEVEL_DOWN: u32 = PUCK + 0x10;
const PUCK_WALLS_UP: u32 = PUCK + 0x12;
const PUCK_OPTIONS: u32 = PUCK + 0x16;
const PUCK_RECORD: u32 = PUCK + 0x17;
const PUCK_SNAPSHOT: u32 = PUCK + 0x18;
/// Rotate left and right, zoom out and in, pitch down and up, Sim, house and map views.
const PUCK_CAMERA: u32 = PUCK + 0x100;
const PUCK_MEMORY: u32 = 0x0b10_a780;
const SKEWER_SLOT: u32 = 0xf6fd_a501;
const SKEWER_PETS: u32 = 0xf6fd_a520;
const SLOT_THUMB: u32 = 1;
const SLOT_MOOD: u32 = 2;
const SLOT_BUTTON: u32 = 3;
const NAV_BACKGROUND: u32 = 0x1ba4_8c20;
const NAV_TAB: u32 = 0x1ba4_8c00;
const MOTIVE_TOP: u32 = 0x06fd_de00;
const MOTIVE_REFERENCE: u32 = 0x06fd_df02;
const MOTIVE_SIZE: u32 = 0x06fd_df03;
const MOTIVE_FILL: u32 = 0x06fd_df04;
const MOTIVE_BACK: u32 = 0x06fd_df05;
const MOODLET_GRID: u32 = 0x06f5_b821;
const MOODLET_UP: u32 = 0x0600_0000;
const MOODLET_DOWN: u32 = 0x0600_0001;
const MOODLET_ICON: u32 = 0x06f5_b830;
const MOODLET_TIME: u32 = 0x06f5_b831;
const MOODLET_NO_TIMEOUT: u32 = 0x06f5_b832;
const WISH_STAGING: u32 = 0x06f5_b804;
const WISH_PAGE_LEFT: u32 = 0x06f5_b805;
const WISH_PAGE_RIGHT: u32 = 0x06f5_b806;
const WISH_LIFETIME: u32 = 0x06f5_b807;
/// The promised wishes' slots: north-west, north-east, south-west, south-east.
const WISH_SLOT: u32 = 0x06f5_b810;
const WISH_ICON: u32 = 1;
const SKILLS_SCROLL: u32 = 0x2fa5_1a02;
const SKILLS_SCROLLING: u32 = 0x2fa5_1a04;
const SKILLS_NONE: u32 = 0x2fa5_1a05;
const SKILLS_NONE_TODDLER: u32 = 0x2fa5_1a08;
const SKILLS_NONE_BABY: u32 = 0x2fa5_1a0a;
/// A skill entry's (`HUDSmallSkillEntry`) icon, bubble meter, journal button, tooltip area.
const SKILL_ICON: u32 = 0x064a_80a0;
const SKILL_BUBBLES: u32 = 0x064a_80af;
const SKILL_JOURNAL: u32 = 0x064a_80a9;
const SKILL_TOOLTIP_MASK: u32 = 0x064a_80b0;
const SKILL_ENTRY_HEIGHT: f32 = 30.0;
/// A bubble meter's first bubble (`BubbleMeter.ControlIDs`); in each, 2 lit and 3 unlit.
const BUBBLE_FIRST: u32 = 0x000b_0101;
/// A wish's second picture (whom it's about), unused here.
const WISH_ICON_2: u32 = 2;

/// The info panel open beside the Sim display (the navigation's tabs: `InfoState`).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum InfoPanel {
    #[default]
    None,
    Simology,
    Career,
    Skills,
    RewardTraits,
    Relationships,
    Inventory,
    Opportunities,
    Motives,
}

impl InfoPanel {
    /// The navigation tabs in order of their control ids (`Navigation.ControlIDs`).
    const TABS: [InfoPanel; 8] = [
        InfoPanel::Simology,
        InfoPanel::Career,
        InfoPanel::Skills,
        InfoPanel::RewardTraits,
        InfoPanel::Relationships,
        InfoPanel::Inventory,
        InfoPanel::Opportunities,
        InfoPanel::Motives,
    ];
}

/// The HUD's layouts on screen.
#[derive(Resource)]
pub struct LiveHud {
    pub(crate) display: Spawned,
    pub(crate) puck: Spawned,
    pub(crate) skewer: Spawned,
    pub(crate) nav: Spawned,
    pub(crate) motives: Spawned,
    pub(crate) skills: Spawned,
    pub(crate) simology: Spawned,
    pub(crate) career: Spawned,
    pub(crate) inventory: Spawned,
    pub(crate) queue: Spawned,
    /// The mood meter's full height and its halfway and bonus markers (fractions up it).
    mood_full: f32,
    mood_markers: (f32, f32),
    /// The motive meters' full width.
    motive_width: f32,
}

/// Whether the game's HUD is up (and the old panels should stay away).
pub fn active(ui: Option<&UiAssets>) -> bool {
    ui.is_some_and(|u| u.layout("HUDSimDisplay").is_some())
}

fn spawn_live_hud(
    mut commands: Commands,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    old: Option<Res<LiveHud>>,
    mut panel: ResMut<InfoPanel>,
) {
    let Some(mut ui) = ui else { return };
    if !active(Some(&ui)) {
        return;
    }
    if let Some(old) = old {
        for s in [&old.display, &old.puck, &old.skewer, &old.nav, &old.motives, &old.skills, &old.simology, &old.career, &old.inventory, &old.queue] {
            if let Some(r) = s.root {
                commands.entity(r).try_despawn();
            }
        }
    }
    let mut spawn = |name: &str| -> Spawned {
        let s = ui.spawn(&mut commands, &mut images, &mut fonts, name).unwrap_or_default();
        if let Some(r) = s.root {
            commands.entity(r).insert((DespawnOnExit(AppState::InGame), GlobalZIndex(5)));
        }
        s
    };
    let display = spawn("HUDSimDisplay");
    let puck = spawn("HUDPuck");
    let skewer = spawn("HUDSkewer");
    let nav = spawn("HUDNavigation");
    let motives = spawn("HUDMotives");
    let skills = spawn("HUDSkillsPanel");
    let simology = spawn("HUDSimologyPanel");
    let career = spawn("HUDCareerPanel");
    let inventory = spawn("HUDInventoryPanel");
    let queue = spawn("HUDInteractionQueue");
    // (Its collection journal works the journal's own button.)
    if let Some(j) = inventory.id(0x0d9b_da80) {
        commands.entity(j).insert(crate::collecting::JournalButton);
    }
    // (Its scroll window takes the wheel.)
    if let Some(sc) = skills.id(SKILLS_SCROLL) {
        commands.entity(sc).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
    }
    // (The navigation's root is shown; its background and tabs come and go with the panels.)
    if let Some(r) = nav.root {
        commands.entity(r).insert(Visibility::Inherited);
    }
    // (The tabs with windows of their own work their existing buttons, unless the game's own
    // panels for them are here: see `infopanels`.)
    let own_panels = ["HUDRelationshipsPanel", "HUDOpportunitiesPanel", "HUDRewardTraitsPanel"].iter().all(|n| ui.layout(n).is_some());
    for (i, t) in InfoPanel::TABS.iter().enumerate() {
        let Some(e) = nav.id(NAV_TAB + 1 + i as u32) else { continue };
        if own_panels {
            break;
        }
        match t {
            InfoPanel::Relationships => {
                commands.entity(e).insert(crate::relations::RelationsButton);
            }
            InfoPanel::Opportunities => {
                commands.entity(e).insert(crate::opportunities::OpportunitiesButton);
            }
            InfoPanel::RewardTraits => {
                commands.entity(e).insert(crate::hud::RewardsButton);
            }
            _ => {}
        }
    }
    // The Skills, Career, Simology and Inventory tabs' content (until each has its own panel
    // layout), over the panels' background.
    if let Some(bg) = nav.id(NAV_BACKGROUND) {
        commands.spawn((
            crate::simpanel::TabContent,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                bottom: Val::Px(14.0),
                width: Val::Px(470.0),
                max_height: Val::Px(420.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                padding: UiRect::all(Val::Px(10.0)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.14, 0.30, 0.94)),
            Interaction::default(),
            crate::hud::BlocksWorld,
            ChildOf(bg),
        ));
    }
    let sim = ui.layout("HUDSimDisplay");
    let area = |id: u32| sim.and_then(|w| w.find(id)).map(|w| w.area).unwrap_or_default();
    let full = area(MOOD_REFERENCE)[3] - area(MOOD_REFERENCE)[1];
    let markers = (1.0 - area(MOOD_HALFWAY)[1] / full.max(1.0), 1.0 - area(MOOD_BONUS)[1] / full.max(1.0));
    let mref = ui.layout("HUDMotives").and_then(|w| w.find(MOTIVE_REFERENCE)).map(|w| w.area).unwrap_or([0.0, 0.0, 88.0, 14.0]);
    // (The motives panel open to start with; INFO_PANEL=<tab> for tests.)
    *panel = std::env::var("INFO_PANEL").ok().and_then(|v| InfoPanel::TABS.into_iter().find(|t| format!("{t:?}").eq_ignore_ascii_case(&v))).unwrap_or(InfoPanel::Motives);
    commands.insert_resource(LiveHud { display, puck, skewer, nav, motives, skills, simology, career, inventory, queue, mood_full: full, mood_markers: markers, motive_width: mref[2] - mref[0] });
}

/// A colour from hue, saturation and value (the tuning's colours are HSV).
fn hsv(h: f32, s: f32, v: f32, a: f32) -> Color {
    Color::from(Hsva::new(h.rem_euclid(1.0) * 360.0, s.clamp(0.0, 1.0), v.clamp(0.0, 1.0), a.clamp(0.0, 1.0)))
}

/// The game's `ComputeMoodColor` / `ComputeMotiveColor`: the colour (HSV triples) and alpha
/// interpolated along value ranges.
fn ranged(value: f32, colors: &[f32], ranges: &[f32], alpha_ranges: &[f32], alphas: &[f32]) -> Color {
    let n = ranges.len();
    let c = |i: usize| [colors[i * 3], colors[i * 3 + 1], colors[i * 3 + 2]];
    let mut hsvv = c(n - 1);
    for i in 0..n {
        if value <= ranges[i] {
            hsvv = if i == 0 {
                c(0)
            } else {
                let t = (value - ranges[i - 1]) / (ranges[i] - ranges[i - 1]);
                let (a, b) = (c(i - 1), c(i));
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
            };
            break;
        }
    }
    let mut alpha = *alphas.last().unwrap_or(&1.0);
    for j in 0..alpha_ranges.len() {
        if value <= alpha_ranges[j] {
            alpha = if j == 0 { alphas[0] } else { alphas[j - 1] + (alphas[j] - alphas[j - 1]) * (value - alpha_ranges[j - 1]) / (alpha_ranges[j] - alpha_ranges[j - 1]) };
            break;
        }
    }
    hsv(hsvv[0], hsvv[1], hsvv[2], alpha)
}

// The base game's tuning (GameplayData: Sims3.Gameplay.ActorSystems.MoodManager and
// Sims3.Gameplay.Autonomy.Motive).
const MOOD_FILL_COLORS: [f32; 12] = [0.333333, 0.35, 0.65, 0.333333, 0.55, 0.75, 0.3333, 1.0, 0.9, 0.3333, 1.0, 0.9];
const MOOD_FILL_RANGES: [f32; 4] = [-35.0, -25.0, 49.0, 50.0];
const MOOD_BACK_COLORS: [f32; 12] = [0.0, 0.85, 0.9, 0.169, 1.0, 0.95, 0.169, 1.0, 0.6, 0.169, 1.0, 0.25];
const MOOD_BACK_RANGES: [f32; 4] = [-70.0, -20.0, 0.0, 20.0];
const MOOD_BACK_ALPHA_RANGES: [f32; 4] = [-70.0, -20.0, 0.0, 10.0];
const MOOD_BACK_ALPHAS: [f32; 4] = [1.0, 1.0, 0.3, 0.2];
/// Where the skewer's ring takes the back colour rather than the fill (`kBackOrFillMoodValue`).
const MOOD_BACK_OR_FILL: f32 = -14.0;
const MOOD_MIN: f32 = -100.0;
const MOOD_LIFETIME: f32 = 50.0;
const MOOD_MAX: f32 = 150.0;
const MOTIVE_FILL_COLORS: [f32; 9] = [0.333333, 0.35, 0.6, 0.333333, 0.9, 0.9, 0.3333, 1.0, 1.0];
const MOTIVE_BACK_COLORS: [f32; 9] = [0.0, 1.0, 1.0, 0.169, 1.0, 1.0, 0.169, 1.0, 0.3];
/// Hunger, bladder, energy, social, hygiene, fun: the fill's ranges and the back's.
const MOTIVE_FILL_RANGES: [[f32; 3]; 6] = [[-60.0, -40.0, 75.0], [-75.0, -40.0, 75.0], [-75.0, -40.0, 75.0], [-75.0, -40.0, 75.0], [-75.0, -40.0, 75.0], [-75.0, -10.0, 75.0]];
const MOTIVE_BACK_RANGES: [[f32; 3]; 6] = [[-90.0, -50.0, 0.0], [-90.0, -70.0, -30.0], [-90.0, -70.0, -30.0], [-100.0, -70.0, -30.0], [-90.0, -70.0, -30.0], [-100.0, -40.0, 0.0]];
const MOTIVE_BACK_ALPHA_RANGES: [f32; 2] = [10.0, 100.0];
const MOTIVE_BACK_ALPHAS: [f32; 2] = [1.0, 0.25];

pub fn mood_fill(mood: f32) -> Color {
    ranged(mood, &MOOD_FILL_COLORS, &MOOD_FILL_RANGES, &[50.0, 100.0], &[1.0, 1.0])
}

pub fn mood_back(mood: f32) -> Color {
    ranged(mood, &MOOD_BACK_COLORS, &MOOD_BACK_RANGES, &MOOD_BACK_ALPHA_RANGES, &MOOD_BACK_ALPHAS)
}

/// The one colour a Sim's mood shows as (the skewer's ring): the back colour when low.
pub fn mood_single(mood: f32) -> Color {
    let c = if mood <= MOOD_BACK_OR_FILL { mood_back(mood) } else { mood_fill(mood) };
    c.with_alpha(1.0)
}

fn set_shade(images: &mut Query<&mut ImageNode>, pictures: &Query<&UiPicture>, window: Option<Entity>, c: Color) {
    if let Some(p) = window.and_then(|w| pictures.get(w).ok())
        && let Ok(mut img) = images.get_mut(p.0)
        && img.color != c
    {
        img.color = c;
    }
}

pub(crate) fn set_text(texts: &mut Query<&mut Text>, e: Option<Entity>, s: &str) {
    if let Some(e) = e
        && let Ok(mut t) = texts.get_mut(e)
        && t.0 != s
    {
        t.0 = s.to_string();
    }
}

pub(crate) fn set_visible(vis: &mut Query<&mut Visibility>, e: Option<Entity>, on: bool) {
    if let Some(e) = e
        && let Ok(mut v) = vis.get_mut(e)
    {
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

pub(crate) fn pressed(q: &Query<(Entity, &Interaction), Changed<Interaction>>, e: Option<Entity>) -> bool {
    e.is_some_and(|e| q.get(e).is_ok_and(|(_, i)| *i == Interaction::Pressed))
}

/// The clock's speed buttons (the current one shown selected) and the day and time.
#[allow(clippy::type_complexity)]
fn time_control(
    hud: Res<LiveHud>,
    mut clock: ResMut<crate::clock::GameClock>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut buttons: Query<&mut UiButton>,
    mut texts: Query<&mut Text>,
    modal: Query<(), With<crate::dialog::Modal>>,
    (weather, world): (Res<crate::weather::Weather>, Option<Res<crate::data::SelectedWorld>>),
    mut tips: Query<&mut crate::icons::Tooltip>,
    buy: Res<crate::buy::BuyMode>,
    menu: Res<crate::options::GameMenu>,
) {
    let d = &hud.display;
    for i in 0..4 {
        if pressed(&clicks, d.id(TIME_BASE + 1 + i)) && !buy.active && !menu.is_open() && modal.is_empty() {
            clock.set_speed(i as usize);
        }
    }
    // (Paused while a question waits for an answer.)
    let locked = !modal.is_empty() || buy.active || menu.is_open();
    let now = if locked { 0 } else { clock.speed };
    for i in 0..5u32 {
        if let Some(e) = d.id(TIME_BASE + 1 + i)
            && let Ok(mut b) = buttons.get_mut(e)
        {
            let (sel, dis) = (i < 4 && i as usize == now, i == 4 || locked);
            if b.selected != sel || b.disabled != dis {
                b.selected = sel;
                b.disabled = dis;
            }
        }
    }
    const DAYS: [&str; 7] = ["Mon", "Tues", "Wed", "Thurs", "Fri", "Sat", "Sun"];
    let s = format!("{} {}", DAYS[clock.weekday()], clock.time_string().to_ascii_lowercase());
    set_text(&mut texts, d.text(TIME_TEXT), &s);
    // (The season, temperature and weather, and the moon at night, on hover.)
    if let Some(e) = d.id(TIME_TEXT)
        && let Ok(mut t) = tips.get_mut(e)
    {
        let tip = if world.is_some_and(|w| crate::weather::vacation(&w.0.name)) || weather.temperature.is_nan() {
            String::new()
        } else {
            let h = clock.hour_f();
            let moon = if !(6.0..18.0).contains(&h) { format!("\n{}", crate::supernatural::moon_name(clock.minutes)) } else { String::new() };
            format!("{}, day {} of {}\n{:.0}°F · {}{moon}", weather.season(clock.day()).name(), weather.season_day(clock.day()), weather.season_days, weather.temperature, weather.describe())
        };
        if t.0 != tip {
            t.0 = tip;
        }
    }
}

/// A test hook: the camera of the moment.
#[derive(Resource)]
pub struct Snapshot;

/// The puck: live, buy and build; walls; floors; the camera; funds; the options menu.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn puck(
    mut commands: Commands,
    hud: Res<LiveHud>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut buttons: Query<&mut UiButton>,
    mut texts: Query<&mut Text>,
    mut vis: Query<&mut Visibility>,
    (household, mut buy, mut clock): (Option<Res<crate::interact::Household>>, ResMut<crate::buy::BuyMode>, ResMut<crate::clock::GameClock>),
    (mut walls, mut building, mut cam): (ResMut<crate::building::WallMode>, Option<ResMut<crate::building::ActiveBuilding>>, Query<&mut crate::camera::SimsCamera>),
    (mut menu, selected): (ResMut<crate::options::GameMenu>, Query<&Transform, With<Selected>>),
    world: Res<crate::loading::CurrentWorld>,
    (buy_hud, build_hud): (Option<Res<crate::buyhud::BuyHud>>, Option<Res<crate::buildhud::BuildHud>>),
) {
    // (Buy mode's layout has its own copy of the puck, worked the same.)
    let buy_puck = buy_hud.map(|b| b.puck.clone());
    let build_puck = build_hud.map(|b| b.puck.clone());
    for p in std::iter::once(&hud.puck).chain(buy_puck.as_ref()).chain(build_puck.as_ref()) {
        puck_one(p, &mut commands, &clicks, &mut buttons, &mut texts, &mut vis, (household.as_deref(), &mut buy, &mut clock), (&mut walls, building.as_mut(), &mut cam), (&mut menu, &selected), &world);
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn puck_one(
    p: &Spawned,
    commands: &mut Commands,
    clicks: &Query<(Entity, &Interaction), Changed<Interaction>>,
    buttons: &mut Query<&mut UiButton>,
    texts: &mut Query<&mut Text>,
    vis: &mut Query<&mut Visibility>,
    (household, buy, clock): (Option<&crate::interact::Household>, &mut crate::buy::BuyMode, &mut crate::clock::GameClock),
    (walls, building, cam): (&mut crate::building::WallMode, Option<&mut ResMut<crate::building::ActiveBuilding>>, &mut Query<&mut crate::camera::SimsCamera>),
    (menu, selected): (&mut crate::options::GameMenu, &Query<&Transform, With<Selected>>),
    world: &crate::loading::CurrentWorld,
) {
    if let Some(h) = &household {
        set_text(texts, p.text(PUCK_FUNDS), &format!("§{}", crate::lifetime::group(h.funds)));
    }
    // Modes.
    if pressed(&clicks, p.id(PUCK_LIVE)) {
        crate::buy::enter_mode(buy, None, commands, clock);
    }
    if pressed(&clicks, p.id(PUCK_BUY)) {
        crate::buy::enter_mode(buy, Some(false), commands, clock);
    }
    if pressed(&clicks, p.id(PUCK_BUILD)) {
        crate::buy::enter_mode(buy, Some(true), commands, clock);
    }
    let mode = if !buy.active { 0 } else if buy.category >= crate::buy::WALLPAPER_TAB { 2 } else { 1 };
    for (id, m) in [(PUCK_LIVE, 0), (PUCK_BUY, 1), (PUCK_BUILD, 2)] {
        if let Some(e) = p.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
            && b.selected != (mode == m)
        {
            b.selected = mode == m;
        }
    }
    // Walls: up, cutaway, down (the trigger cycles them; the picture shows which).
    if pressed(&clicks, p.id(PUCK_WALLS)) {
        *walls = walls.next();
    }
    for (i, m) in [crate::building::WallMode::Up, crate::building::WallMode::Cutaway, crate::building::WallMode::Down].into_iter().enumerate() {
        set_visible(vis, p.id(PUCK_WALLS_UP + i as u32), *walls == m);
    }
    // Floors.
    if let Some(b) = building {
        for (id, step) in [(PUCK_LEVEL_UP, 1i8), (PUCK_LEVEL_DOWN, -1)] {
            if pressed(&clicks, p.id(id)) {
                b.view_level = (b.view_level as i8 + step).clamp(1, b.top_level.max(1) as i8) as u8;
            }
        }
        for (id, ok) in [(PUCK_LEVEL_UP, b.view_level < b.top_level), (PUCK_LEVEL_DOWN, b.view_level > 1)] {
            if let Some(e) = p.id(id)
                && let Ok(mut btn) = buttons.get_mut(e)
                && btn.disabled == ok
            {
                btn.disabled = !ok;
            }
        }
    }
    // The camera: orbit, zoom, tilt, and the Sim, house and map views.
    if let Ok(mut c) = cam.single_mut() {
        for k in 0..9u32 {
            if !pressed(&clicks, p.id(PUCK_CAMERA + k)) {
                continue;
            }
            match k {
                0 => c.yaw -= std::f32::consts::FRAC_PI_4,
                1 => c.yaw += std::f32::consts::FRAC_PI_4,
                2 => c.distance = (c.distance * 1.4).min(400.0),
                3 => c.distance = (c.distance / 1.4).max(3.0),
                4 => c.pitch = (c.pitch - 0.15).max(0.15),
                5 => c.pitch = (c.pitch + 0.15).min(1.45),
                6 => {
                    if let Ok(tf) = selected.single() {
                        c.look_at(tf.translation);
                        c.distance = 9.0;
                    }
                }
                7 => {
                    if let Some(l) = household.as_ref().and_then(|h| world.data.lots.get(h.lot_index)) {
                        let corners = crate::home::lot_corners(l);
                        let mid = corners.iter().fold(Vec3::ZERO, |a, b| a + *b) / 4.0;
                        c.look_at(Vec3::new(mid.x, world.data.heightmap.sample(mid.x, mid.z), mid.z));
                        c.distance = 35.0;
                    }
                }
                _ => c.distance = 260.0,
            }
        }
    }
    if pressed(&clicks, p.id(PUCK_OPTIONS)) {
        // (The game's own popup menu where it's to hand: see `gamepopup`.)
        if crate::gamepopup::GAME_POPUP.load(std::sync::atomic::Ordering::Relaxed) {
            commands.insert_resource(crate::gamepopup::OpenGamePopup);
        } else {
            crate::options::toggle_game_menu(commands, menu, Some(clock));
        }
    }
    if pressed(&clicks, p.id(PUCK_SNAPSHOT)) {
        commands.insert_resource(Snapshot);
    }
    // (Nothing to record video with, nor camera memories to keep.)
    for id in [PUCK_RECORD, PUCK_MEMORY] {
        if let Some(e) = p.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
            && !b.disabled
        {
            b.disabled = true;
        }
    }
}

/// The mood meter: filled to the selected Sim's mood (the game's thirds: miserable to fine,
/// fine to happy, and the bonus band that earns lifetime happiness), in its colours.
#[allow(clippy::type_complexity)]
fn mood_meter(
    hud: Res<LiveHud>,
    sel: Query<&crate::life::Mood, With<Selected>>,
    mut nodes: Query<&mut Node>,
    mut images: Query<&mut ImageNode>,
    pictures: Query<&UiPicture>,
    mut vis: Query<&mut Visibility>,
    mut shown: Local<f32>,
    time: Res<Time>,
) {
    let Ok(mood) = sel.single() else { return };
    // (The bar eases towards the mood, as the game's.)
    let want = mood.0.clamp(MOOD_MIN, MOOD_MAX);
    *shown += (want - *shown) * (time.delta_secs() * 4.0).min(1.0);
    let v = *shown;
    let (half, bonus) = hud.mood_markers;
    let f = if v <= 0.0 {
        (v - MOOD_MIN) / -MOOD_MIN * half
    } else if v <= MOOD_LIFETIME {
        v / MOOD_LIFETIME * (bonus - half) + half
    } else {
        (v - MOOD_LIFETIME) / (MOOD_MAX - MOOD_LIFETIME) * (1.0 - bonus) + bonus
    };
    let d = &hud.display;
    if let Some(e) = d.id(MOOD_SIZE)
        && let Ok(mut n) = nodes.get_mut(e)
    {
        let h = (f.clamp(0.0, 1.0) * hud.mood_full).round();
        // (Grown up from the meter's foot.)
        let top = Val::Px(hud.mood_full - h);
        if n.top != top || n.height != Val::Px(h) {
            n.top = top;
            n.height = Val::Px(h);
        }
    }
    set_shade(&mut images, &pictures, d.id(MOOD_FILL), mood_fill(v));
    set_shade(&mut images, &pictures, d.id(MOOD_BACK), mood_back(v));
    set_visible(&mut vis, d.id(MOOD_GLOW), v >= MOOD_LIFETIME);
}

/// The motives panel's bars: each filled to its need, in the game's colours.
fn motives_panel(hud: Res<LiveHud>, sel: Query<&Motives, With<Selected>>, mut nodes: Query<&mut Node>, mut images: Query<&mut ImageNode>, pictures: Query<&UiPicture>) {
    let Ok(m) = sel.single() else { return };
    let s = &hud.motives;
    for i in 0..6 {
        let Some(top) = s.id(MOTIVE_TOP + i as u32) else { continue };
        let v = m.0[i].clamp(-100.0, 100.0);
        if let Some(e) = s.within(top, MOTIVE_SIZE)
            && let Ok(mut n) = nodes.get_mut(e)
        {
            let w = Val::Px(((v + 100.0) / 200.0 * hud.motive_width).round());
            if n.width != w {
                n.width = w;
            }
        }
        let fill = ranged(v, &MOTIVE_FILL_COLORS, &MOTIVE_FILL_RANGES[i], &[10.0, 100.0], &[1.0, 1.0]);
        let back = ranged(v, &MOTIVE_BACK_COLORS, &MOTIVE_BACK_RANGES[i], &MOTIVE_BACK_ALPHA_RANGES, &MOTIVE_BACK_ALPHAS);
        set_shade(&mut images, &pictures, s.within(top, MOTIVE_FILL), fill);
        set_shade(&mut images, &pictures, s.within(top, MOTIVE_BACK), back);
    }
}

/// The selected Sim's picture in the bust.
fn bust(
    mut commands: Commands,
    hud: Res<LiveHud>,
    sel: Query<Entity, With<Selected>>,
    mut portraits: ResMut<crate::portraits::Portraits>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Local<Option<(Entity, Entity)>>,
) {
    let (Ok(s), Some(win)) = (sel.single(), hud.display.id(BUST)) else { return };
    if shown.is_some_and(|(w, e)| w == win && e == s) {
        return;
    }
    *shown = Some((win, s));
    commands.entity(win).despawn_children();
    let h = portraits.portrait(&mut images, s);
    // (In the frame's round window, as the game's turning head.)
    commands.entity(win).with_children(|b| {
        b.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(8.0), top: Val::Px(8.0), right: Val::Px(8.0), bottom: Val::Px(8.0), border_radius: BorderRadius::all(Val::Percent(50.0)), overflow: Overflow::clip(), ..default() },
            Pickable::IGNORE,
        ))
        .with_children(|f| {
            f.spawn((ImageNode::new(h), crate::portraits::PortraitOf(s), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
        });
    });
}

/// The skewer: the household's faces from the bottom up, each ringed in its mood's colour, the
/// selected one shown so; clicked, they're selected (again, the camera goes to them).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn skewer(
    mut commands: Commands,
    hud: Res<LiveHud>,
    members: Query<(Entity, &crate::life::Mood, Has<Selected>), With<HouseholdMember>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut buttons: Query<&mut UiButton>,
    mut vis: Query<&mut Visibility>,
    mut images: Query<&mut ImageNode>,
    pictures: Query<&UiPicture>,
    (mut portraits, mut assets): (ResMut<crate::portraits::Portraits>, ResMut<Assets<Image>>),
    (selected, positions, mut cam): (Query<Entity, With<Selected>>, Query<&Transform, With<Sim>>, Query<&mut crate::camera::SimsCamera>),
    mut faces: Local<Vec<Option<Entity>>>,
) {
    let s = &hud.skewer;
    let mut list: Vec<(Entity, f32, bool)> = members.iter().map(|(e, m, sel)| (e, m.0, sel)).collect();
    list.sort_by_key(|x| x.0);
    faces.resize(8, None);
    for slot in 0..8 {
        let container = s.id(SKEWER_SLOT + slot as u32);
        let who = list.get(slot).copied();
        set_visible(&mut vis, container, who.is_some());
        let (Some(c), Some((e, mood, sel))) = (container, who) else { continue };
        let button = s.within(c, SLOT_BUTTON);
        if let Some(b) = button
            && let Ok(mut b) = buttons.get_mut(b)
            && b.selected != sel
        {
            b.selected = sel;
        }
        set_shade(&mut images, &pictures, s.within(c, SLOT_MOOD), mood_single(mood));
        if faces[slot] != Some(e)
            && let Some(p) = s.within(c, SLOT_THUMB).and_then(|t| pictures.get(t).ok())
            && let Ok(mut img) = images.get_mut(p.0)
        {
            img.image = portraits.portrait(&mut assets, e);
            commands.entity(p.0).insert(crate::portraits::PortraitOf(e));
            faces[slot] = Some(e);
        }
        if pressed(&clicks, button) {
            let already = selected.contains(e);
            for o in &selected {
                commands.entity(o).remove::<Selected>();
            }
            commands.entity(e).insert(Selected);
            if already
                && let (Ok(tf), Ok(mut cm)) = (positions.get(e), cam.single_mut())
            {
                cm.look_at(tf.translation);
            }
        }
    }
    // (Pets have their own skewer, still to come.)
    set_visible(&mut vis, s.id(SKEWER_PETS), false);
}

/// The navigation: the expand button opens the info panels' background and tabs; a tab opens
/// its panel (relationships, opportunities and the lifetime rewards in their own windows).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn navigation(
    hud: Res<LiveHud>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut buttons: Query<&mut UiButton>,
    mut vis: Query<&mut Visibility>,
    mut panel: ResMut<InfoPanel>,
    mut tab: ResMut<crate::simpanel::SimTab>,
    keys: Res<ButtonInput<KeyCode>>,
    (journal, mut only_journal): (Res<crate::simpanel::OpenJournal>, ResMut<crate::simpanel::JournalOnly>),
    buy: Res<crate::buy::BuyMode>,
) {
    if buy.active { return; }
    let d = &hud.display;
    if pressed(&clicks, d.id(EXPAND)) {
        *panel = if *panel == InfoPanel::None { InfoPanel::Motives } else { InfoPanel::None };
    }
    for (i, t) in InfoPanel::TABS.iter().enumerate() {
        if pressed(&clicks, hud.nav.id(NAV_TAB + 1 + i as u32)) {
            // (Relationships, opportunities and the rewards open their own windows, through
            // the buttons' marker components.)
            let own_window = matches!(t, InfoPanel::Relationships | InfoPanel::Opportunities | InfoPanel::RewardTraits) && !crate::infopanels::GAME_PANELS.load(std::sync::atomic::Ordering::Relaxed);
            *panel = if *panel == *t || own_window { InfoPanel::None } else { *t };
        }
    }
    // (F5 to F9, as before: needs, skills, career, simology, inventory.)
    for (k, t) in [(KeyCode::F5, InfoPanel::Motives), (KeyCode::F6, InfoPanel::Skills), (KeyCode::F7, InfoPanel::Career), (KeyCode::F8, InfoPanel::Simology), (KeyCode::F9, InfoPanel::Inventory)] {
        if keys.just_pressed(k) {
            *panel = t;
        }
    }
    // (The skills are in the game's panel; a skill's journal, opened from it, still shows in
    // the tab's own content.)
    let journal_open = *panel == InfoPanel::Skills && journal.0.is_some();
    if only_journal.0 != journal_open {
        only_journal.0 = journal_open;
    }
    let want_tab = match *panel {
        InfoPanel::Skills if journal_open => Some(crate::simpanel::SimTab::Skills),
        InfoPanel::Skills => Some(crate::simpanel::SimTab::Needs),
        InfoPanel::Career => Some(crate::simpanel::SimTab::Needs),
        InfoPanel::Simology => Some(crate::simpanel::SimTab::Needs),
        InfoPanel::Inventory => Some(crate::simpanel::SimTab::Needs),
        _ => Some(crate::simpanel::SimTab::Needs),
    };
    if let Some(t) = want_tab
        && *tab != t
    {
        *tab = t;
    }
    let open = *panel != InfoPanel::None;
    set_visible(&mut vis, hud.nav.id(NAV_BACKGROUND), open);
    set_visible(&mut vis, hud.motives.root, *panel == InfoPanel::Motives);
    set_visible(&mut vis, hud.skills.root, *panel == InfoPanel::Skills);
    set_visible(&mut vis, hud.simology.root, *panel == InfoPanel::Simology);
    set_visible(&mut vis, hud.career.root, *panel == InfoPanel::Career);
    set_visible(&mut vis, hud.inventory.root, *panel == InfoPanel::Inventory);
    for (i, t) in InfoPanel::TABS.iter().enumerate() {
        if let Some(e) = hud.nav.id(NAV_TAB + 1 + i as u32)
            && let Ok(mut b) = buttons.get_mut(e)
            && b.selected != (*panel == *t)
        {
            b.selected = *panel == *t;
        }
    }
    if let Some(e) = d.id(EXPAND)
        && let Ok(mut b) = buttons.get_mut(e)
        && b.selected != open
    {
        b.selected = open;
    }
}

/// In buy and build mode the Sim display, skewer and panels make way (the puck stays).
fn show_in_modes(hud: Res<LiveHud>, buy: Res<crate::buy::BuyMode>, mut vis: Query<&mut Visibility>, panel: Res<InfoPanel>) {
    if !buy.is_changed() && !panel.is_changed() {
        return;
    }
    let live = !buy.active;
    for r in [hud.display.root, hud.skewer.root, hud.nav.root, hud.queue.root] {
        set_visible(&mut vis, r, live);
    }
    set_visible(&mut vis, hud.motives.root, live && *panel == InfoPanel::Motives);
    set_visible(&mut vis, hud.skills.root, live && *panel == InfoPanel::Skills);
    set_visible(&mut vis, hud.simology.root, live && *panel == InfoPanel::Simology);
    set_visible(&mut vis, hud.career.root, live && *panel == InfoPanel::Career);
    set_visible(&mut vis, hud.inventory.root, live && *panel == InfoPanel::Inventory);
}

fn set_image(images: &mut Query<&mut ImageNode>, pictures: &Query<&UiPicture>, window: Option<Entity>, h: Option<Handle<Image>>) {
    if let (Some(p), Some(h)) = (window.and_then(|w| pictures.get(w).ok()), h)
        && let Ok(mut img) = images.get_mut(p.0)
        && img.image != h
    {
        img.image = h;
    }
}

fn set_tooltip(commands: &mut Commands, tips: &mut Query<&mut crate::icons::Tooltip>, e: Option<Entity>, tip: String) {
    let Some(e) = e else { return };
    match tips.get_mut(e) {
        Ok(mut t) => {
            if t.0 != tip {
                t.0 = tip;
            }
        }
        Err(_) => {
            // (Hoverable, to show it.)
            commands.entity(e).remove::<Pickable>().insert((crate::icons::Tooltip(tip), Interaction::default(), crate::hud::BlocksWorld));
        }
    }
}

/// The moodlets grid: a cell per moodlet, framed for good, bad or neutral (the game's cell
/// layout, `HUDMoodletGridCell`'s exports 1 to 3), with its icon and time left, three across
/// and two down, scrolled by its arrows.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn moodlets(
    mut commands: Commands,
    hud: Res<LiveHud>,
    ui: Option<ResMut<UiAssets>>,
    (mut assets, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    sel: Query<&crate::life::Moodlets, With<Selected>>,
    clock: Res<crate::clock::GameClock>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut vis: Query<&mut Visibility>,
    mut state: Local<(Option<Entity>, Vec<String>, u32, usize, bool)>,
) {
    let (Some(mut ui), Some(grid)) = (ui, hud.display.id(MOODLET_GRID)) else { return };
    let Some(g) = ui.layout("HUDSimDisplay").and_then(|w| w.find(MOODLET_GRID)).and_then(|w| w.grid) else { return };
    let mut list: Vec<crate::life::Moodlet> = sel.single().map(|m| m.0.clone()).unwrap_or_default();
    list.sort_by_key(|x| -x.value.abs());
    let (cols, rows) = (g.columns.max(1) as usize, g.rows.max(1) as usize);
    let total_rows = list.len().div_ceil(cols);
    // Scrolling a row at a time.
    let mut scroll = state.3;
    if pressed(&clicks, hud.display.id(MOODLET_UP)) {
        scroll = scroll.saturating_sub(1);
    }
    if pressed(&clicks, hud.display.id(MOODLET_DOWN)) {
        scroll += 1;
    }
    scroll = scroll.min(total_rows.saturating_sub(rows));
    set_visible(&mut vis, hud.display.id(MOODLET_UP), scroll > 0);
    set_visible(&mut vis, hud.display.id(MOODLET_DOWN), scroll + rows < total_rows);
    let labels: Vec<String> = list.iter().map(crate::life::moodlet_label).collect();
    // (Time left is refreshed every game hour.)
    let hour = (clock.minutes / 60.0) as u32;
    let holder_ok = state.0.is_some_and(|h| vis.contains(h));
    if holder_ok && state.1 == labels && state.2 == hour && state.3 == scroll && state.4 == game_ui.is_some() {
        return;
    }
    let holder = match state.0.filter(|_| holder_ok) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => commands
            .spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
                Visibility::Inherited,
                Pickable::IGNORE,
                ChildOf(grid),
            ))
            .id(),
    };
    *state = (Some(holder), labels, hour, scroll, game_ui.is_some());
    for (i, m) in list.iter().enumerate().skip(scroll * cols).take(cols * rows) {
        let k = i - scroll * cols;
        let (col, row) = (k % cols, k / cols);
        let export = match m.value.signum() {
            1 => 1,
            -1 => 2,
            _ => 3,
        };
        let Some(mut cell) = ui.export("HUDMoodletGridCell", export).cloned() else { continue };
        let (w, h) = (cell.area[2] - cell.area[0], cell.area[3] - cell.area[1]);
        let (x, y) = (g.padding[0] + col as f32 * g.cell[0], g.padding[1] + row as f32 * g.cell[1]);
        cell.area = [x, y, x + w, y + h];
        let s = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &cell, holder);
        let (name, desc, icon) = match game_ui.as_deref() {
            Some(gu) => gu.moodlet_info(m.kind),
            None => (m.kind.def().name.to_string(), m.kind.def().desc.to_string(), String::new()),
        };
        let icon = game_ui.as_deref_mut().and_then(|gu| gu.icon(&mut assets, &icon));
        // (Its picture: the icon placement's own image swapped for the moodlet's.)
        if let (Some(win), Some(h)) = (s.id(MOODLET_ICON), icon) {
            commands.entity(win).despawn_children();
            commands.entity(win).with_children(|c| {
                c.spawn((ImageNode::new(h), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
            });
        }
        let left = m.until.is_finite().then(|| ((m.until - clock.minutes) / 60.0).max(0.0));
        if let Some(t) = s.text(MOODLET_TIME) {
            let txt = match left {
                Some(h) if h >= 1.0 => format!("{h:.0} h"),
                Some(h) => format!("{:.0} m", (h * 60.0).max(1.0)),
                None => String::new(),
            };
            commands.entity(t).insert(Text::new(txt));
        }
        if let Some(e) = s.id(MOODLET_TIME) {
            commands.entity(e).insert(if left.is_some() { Visibility::Inherited } else { Visibility::Hidden });
        }
        if let Some(e) = s.id(MOODLET_NO_TIMEOUT) {
            commands.entity(e).insert(if left.is_none() { Visibility::Inherited } else { Visibility::Hidden });
        }
        let v = m.value;
        let left_s = match left {
            Some(h) if h >= 1.0 => format!("\n{h:.0} hours left"),
            Some(h) => format!("\n{:.0} minutes left", h * 60.0),
            None => String::new(),
        };
        if let Some(r) = s.root {
            let tip = format!("{name} ({}{v})\n{desc}{left_s}", if v >= 0 { "+" } else { "" });
            commands.entity(r).insert((Interaction::default(), crate::hud::BlocksWorld, crate::icons::Tooltip(tip)));
        }
    }
}

/// The wishes: the offered ones in the staging area (paged with its arrows; clicked, promised),
/// the four promised ones in their slots, and the lifetime wish in its frame.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn wishes(
    mut commands: Commands,
    hud: Res<LiveHud>,
    mut sel: Query<(&Sim, &mut crate::wishes::Wishes, Option<&crate::lifetime::LifetimeWish>), With<Selected>>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    mut assets: ResMut<Assets<Image>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut vis: Query<&mut Visibility>,
    mut images: Query<&mut ImageNode>,
    pictures: Query<&UiPicture>,
    mut tips: Query<&mut crate::icons::Tooltip>,
    mut page: Local<usize>,
    mut selected: Local<Option<u64>>,
    (mouse, hovered): (Res<ButtonInput<MouseButton>>, Query<&Interaction>),
) {
    let Ok((sim, mut w, ltw)) = sel.single_mut() else { return };
    if selected.replace(sim.id) != Some(sim.id) { *page = 0; }
    let d = &hud.display;
    let n = w.offered.len();
    if pressed(&clicks, d.id(WISH_PAGE_LEFT)) && n > 0 {
        *page = (*page + n - 1) % n;
    }
    if pressed(&clicks, d.id(WISH_PAGE_RIGHT)) && n > 0 {
        *page = (*page + 1) % n;
    }
    if n > 0 && *page >= n {
        *page = 0;
    }
    if pressed(&clicks, d.id(WISH_STAGING)) && *page < n && w.promised.len() < 4 {
        w.promise(*page);
    }
    if mouse.just_pressed(MouseButton::Right) {
        let over = |id| d.id(id).is_some_and(|e| hovered.get(e).is_ok_and(|i| *i != Interaction::None));
        if over(WISH_STAGING) {
            w.dismiss(false, *page);
        } else if let Some(slot) = (0..4).find(|slot| over(WISH_SLOT + *slot)) {
            w.dismiss(true, slot as usize);
        }
    }
    let n = w.offered.len();
    *page = (*page).min(n.saturating_sub(1));
    let mut icon = |x: &crate::wishes::Wish| -> Option<Handle<Image>> {
        let gu = game_ui.as_deref_mut()?;
        let name = x.icon(&gu.data.clone());
        gu.icon(&mut assets, &name)
    };
    // The staging area: the offered wish on show.
    let staging = d.id(WISH_STAGING);
    let shown = w.offered.get(*page).cloned();
    set_visible(&mut vis, staging, shown.is_some());
    for b in [staging, d.id(WISH_SLOT), d.id(WISH_SLOT + 1), d.id(WISH_SLOT + 2), d.id(WISH_SLOT + 3), d.id(WISH_LIFETIME)].into_iter().flatten() {
        set_visible(&mut vis, d.within(b, WISH_ICON_2), false);
    }
    if let (Some(st), Some(x)) = (staging, &shown) {
        set_image(&mut images, &pictures, d.within(st, WISH_ICON), icon(x));
        let more = if n > 1 { format!(" ({} of {n})", *page + 1) } else { String::new() };
        let action = if w.promised.len() < 4 { "Click to promise. Right-click to dismiss." } else { "All four wish slots are full. Right-click to dismiss." };
        set_tooltip(&mut commands, &mut tips, Some(st), format!("{} (+{}){more}\n{action}", x.text(), x.reward_points(&sim.traits)));
    }
    // The promised wishes.
    for slot in 0..4 {
        let b = d.id(WISH_SLOT + slot as u32);
        let x = w.promised.get(slot).cloned();
        set_visible(&mut vis, b, x.is_some());
        if let (Some(b), Some(x)) = (b, x) {
            set_image(&mut images, &pictures, d.within(b, WISH_ICON), icon(&x));
            set_tooltip(&mut commands, &mut tips, Some(b), format!("Promised: {} (+{})\nRight-click to dismiss.", x.text(), x.reward_points(&sim.traits)));
        }
    }
    // The lifetime wish.
    let lt = d.id(WISH_LIFETIME);
    set_visible(&mut vis, lt, ltw.is_some());
    if let (Some(lt), Some(l), Some(gu)) = (lt, ltw, game_ui.as_deref_mut()) {
        let def = l.def();
        let data = gu.data.clone();
        let h = gu.icon(&mut assets, &def.icon(Some(&data)));
        set_image(&mut images, &pictures, d.within(lt, WISH_ICON), h);
        let tip = format!("Lifetime Wish: {}\n{}\n{} (+{} lifetime happiness)", def.name, def.describe(Some(&data)), l.status, crate::lifetime::group(def.points(Some(&data)) as i64));
        set_tooltip(&mut commands, &mut tips, Some(lt), tip);
    }
}

/// A skill's journal button in the skills panel.
#[derive(Component)]
struct SkillJournalButton(&'static str);

/// The skills panel: an entry per skill learned (the game's `HUDSmallSkillEntry`: its icon,
/// its level in bubbles, its journal button), highest first, scrolled with the wheel; or the
/// game's words when there are none.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn skills_panel(
    mut commands: Commands,
    hud: Res<LiveHud>,
    ui: Option<ResMut<UiAssets>>,
    (mut assets, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    sel: Query<(&Sim, &crate::interact::Skills), With<Selected>>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    mut vis: Query<&mut Visibility>,
    mut nodes: Query<&mut Node>,
    mut state: Local<(Option<Entity>, Vec<(&'static str, i32)>, bool, f32)>,
    (mut wheel, hovered): (MessageReader<bevy::input::mouse::MouseWheel>, Query<&Interaction>),
    panel: Res<InfoPanel>,
) {
    let (Some(mut ui), Some(scroll)) = (ui, hud.skills.id(SKILLS_SCROLLING)) else { return };
    let Ok((sim, skills)) = sel.single() else { return };
    let mut list: Vec<(&'static str, f32)> = skills.0.iter().filter(|(_, v)| **v > 0.01).map(|(k, v)| (*k, *v)).collect();
    list.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    // (The words for having none, by age.)
    let little = matches!(sim.age, crate::sim::Age::Baby | crate::sim::Age::Toddler);
    set_visible(&mut vis, hud.skills.id(SKILLS_NONE), list.is_empty() && !little);
    set_visible(&mut vis, hud.skills.id(SKILLS_NONE_TODDLER), list.is_empty() && sim.age == crate::sim::Age::Toddler);
    set_visible(&mut vis, hud.skills.id(SKILLS_NONE_BABY), list.is_empty() && sim.age == crate::sim::Age::Baby);
    // Scrolling with the wheel over the panel, an entry at a time.
    let over = hud.skills.id(SKILLS_SCROLL).is_some_and(|e| hovered.get(e).is_ok_and(|i| *i != Interaction::None));
    let rows = list.len() as f32;
    for w in wheel.read() {
        if over && *panel == InfoPanel::Skills {
            state.3 = (state.3 - w.y.signum()).clamp(0.0, (rows - 4.0).max(0.0));
        }
    }
    if let Some(h) = state.0
        && let Ok(mut n) = nodes.get_mut(h)
    {
        let top = Val::Px(-state.3 * SKILL_ENTRY_HEIGHT);
        if n.top != top {
            n.top = top;
        }
    }
    let key: Vec<(&'static str, i32)> = list.iter().map(|(k, v)| (*k, (*v * 20.0) as i32)).collect();
    let holder_ok = state.0.is_some_and(|h| vis.contains(h));
    if holder_ok && state.1 == key && state.2 == game_ui.is_some() {
        return;
    }
    let holder = match state.0.filter(|_| holder_ok) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => commands
            .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), ..default() }, Visibility::Inherited, Pickable::IGNORE, ChildOf(scroll)))
            .id(),
    };
    *state = (Some(holder), key, game_ui.is_some(), state.3.min((rows - 4.0).max(0.0)));
    let Some(template) = ui.layout("HUDSmallSkillEntry").cloned() else { return };
    for (i, (name, v)) in list.iter().enumerate() {
        let info = game_ui.as_deref().and_then(|g| g.data.skill(name).cloned());
        let max = info.as_ref().map_or(10, |i| i.max_level.max(1));
        let level = (*v as u32).min(max);
        let mut entry = template.clone();
        let (w, h) = (entry.area[2] - entry.area[0], entry.area[3] - entry.area[1]);
        let y = i as f32 * SKILL_ENTRY_HEIGHT;
        entry.area = [0.0, y, w, y + h];
        let s = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &entry, holder);
        // Its icon, in the panel's navy.
        let icon = info.as_ref().and_then(|i| game_ui.as_deref_mut().and_then(|g| g.icon(&mut assets, &i.icon)));
        if let (Some(win), Some(h)) = (s.id(SKILL_ICON), icon) {
            let tint = ui.layout("HUDSmallSkillEntry").and_then(|l| l.find(SKILL_ICON)).map_or(Color::WHITE, |w| crate::layout::color(w.shade));
            commands.entity(win).despawn_children();
            commands.entity(win).with_children(|c| {
                c.spawn((ImageNode { image: h, color: tint, ..default() }, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
            });
        }
        // Its level in bubbles (a bubble a level, ten to the meter; five-level skills fill
        // two bubbles a level, as the game's step).
        if let Some(meter) = s.id(SKILL_BUBBLES) {
            let step = (max / 10).max(1);
            let filled = (level / step) as usize;
            let fill = if max < 10 { (level * (10 / max.max(1))) as usize } else { filled };
            for b in 0..10u32 {
                let Some(bubble) = s.within(meter, BUBBLE_FIRST + b) else { continue };
                for (id, on) in [(2, (b as usize) < fill), (3, (b as usize) >= fill), (4, false), (5, false)] {
                    if let Some(e) = s.within(bubble, id) {
                        commands.entity(e).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
                    }
                }
            }
        }
        if let Some(j) = s.id(SKILL_JOURNAL) {
            commands.entity(j).insert(SkillJournalButton(name));
        }
        let into = if level >= max { String::from("Mastered!") } else { format!("{:.0}% of the way to level {}", v.fract() * 100.0, level + 1) };
        let desc = info.as_ref().map(|i| i.desc.replace("{0.SimFirstName}", &sim.first)).unwrap_or_default();
        let tip = format!("{} — level {level} of {max}\n{into}\n{desc}", info.as_ref().map_or(*name, |i| i.name.as_str()));
        if let Some(m) = s.id(SKILL_TOOLTIP_MASK) {
            commands.entity(m).remove::<Pickable>().insert((Interaction::default(), crate::icons::Tooltip(tip)));
        }
    }
}

/// A skill's journal button opens its journal (or closes it).
fn skill_journal_buttons(q: Query<(&Interaction, &SkillJournalButton), Changed<Interaction>>, mut open: ResMut<crate::simpanel::OpenJournal>) {
    for (i, b) in &q {
        if *i == Interaction::Pressed {
            open.0 = if open.0 == Some(b.0) { None } else { Some(b.0) };
        }
    }
}

const QUEUE_BUTTON: u32 = 0x04f6_7d00;
const QUEUE_ICON: u32 = 0x04f6_7d08;
const QUEUE_CANCEL: u32 = 0x04f6_7d05;
const QUEUE_PROGRESS: u32 = 0x04f6_7e00;
const QUEUE_PROGRESS_CLIP: u32 = 0x04f6_7e01;
/// The queue's item width and gap, heights (with a progress bar), top, and the progress bar's
/// full width (`InteractionQueueItem`).
const QUEUE_ITEM_WIDTH: f32 = 65.0;
const QUEUE_ITEM_GAP: f32 = 3.0;
const QUEUE_ITEM_HEIGHT: f32 = 65.0;
const QUEUE_ITEM_PROGRESS_HEIGHT: f32 = 77.0;
const QUEUE_ITEM_TOP: f32 = 5.0;
const QUEUE_PROGRESS_WIDTH: f32 = 49.0;

/// A queued action's button (its place in the queue).
#[derive(Component)]
struct QueueItem(usize, Option<Entity>, Option<Entity>);

/// What a queued action shows: an object's catalogue picture, or a Sim's face.
#[derive(Clone, PartialEq, Debug)]
enum QueuePicture {
    Object(s3bake::Key),
    Sim(Entity),
}

/// How far along a running action is (0..1), when it can tell.
fn progress(a: &crate::interact::Action, objects: &Query<&crate::interact::GameObject>, motives: Option<&Motives>) -> Option<f32> {
    let crate::interact::Phase::Running(t) = a.phase else { return None };
    let crate::interact::ActionKind::Object { target, def } = &a.kind else { return None };
    let obj = objects.get(*target).ok()?;
    let d = crate::interact::interactions_for(obj.kind).get(*def)?;
    match d.until_full {
        Some(i) => motives.map(|m| (m.0[i] + 100.0) / 200.0),
        None if d.minutes > 0.0 => Some(t / d.minutes),
        None => None,
    }
}

/// The interaction queue (top left): an item per queued action, the one under way first with
/// its progress, each with what it's done with (the object's picture, the other Sim's face);
/// pointed at, the cancel cross; clicked, cancelled.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn interaction_queue(
    mut commands: Commands,
    hud: Res<LiveHud>,
    ui: Option<ResMut<UiAssets>>,
    (mut assets, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut sel: Query<(Entity, &mut crate::interact::ActionQueue, Option<&Motives>), With<Selected>>,
    objects: Query<&crate::interact::GameObject>,
    sims: Query<(), With<Sim>>,
    (mut game_ui, mut portraits): (Option<ResMut<crate::icons::GameUi>>, ResMut<crate::portraits::Portraits>),
    items: Query<(&Interaction, &QueueItem)>,
    clicks: Query<(&Interaction, &QueueItem), Changed<Interaction>>,
    (mut vis, mut nodes): (Query<&mut Visibility>, Query<&mut Node>),
    mut state: Local<(Option<Entity>, Vec<(String, Option<QueuePicture>, bool)>)>,
) {
    let (Some(mut ui), Some(root)) = (ui, hud.queue.root) else { return };
    let Ok((me, mut queue, motives)) = sel.single_mut() else { return };
    // A click cancels.
    for (i, q) in &clicks {
        if *i == Interaction::Pressed
            && let Some(a) = queue.0.iter_mut().filter(|a| !a.cancel).nth(q.0)
        {
            a.cancel = true;
        }
    }
    let shown: Vec<&crate::interact::Action> = queue.0.iter().filter(|a| !a.cancel).collect();
    let sig: Vec<(String, Option<QueuePicture>, bool)> = shown
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let pic = match &a.kind {
                crate::interact::ActionKind::Object { target, .. } | crate::interact::ActionKind::Repair { target } | crate::interact::ActionKind::Upgrade { target, .. } => {
                    objects.get(*target).ok().map(|o| QueuePicture::Object(o.objd))
                }
                crate::interact::ActionKind::Social { target, .. } | crate::interact::ActionKind::PhoneChat { target } | crate::interact::ActionKind::Invite { target } if sims.contains(*target) => {
                    Some(QueuePicture::Sim(*target))
                }
                _ => None,
            };
            (a.label.clone(), pic, i == 0 && progress(a, &objects, motives).is_some())
        })
        .collect();
    // The progress of the action under way, every frame.
    let head = shown.first().and_then(|a| progress(a, &objects, motives));
    // (The cancel cross while pointed at.)
    for (i, q) in &items {
        set_visible(&mut vis, q.1, *i != Interaction::None);
        if q.0 == 0
            && let (Some(clip), Some(f)) = (q.2, head)
            && let Ok(mut n) = nodes.get_mut(clip)
        {
            let w = Val::Px((f.clamp(0.0, 1.0) * QUEUE_PROGRESS_WIDTH).round());
            if n.width != w {
                n.width = w;
            }
        }
    }
    if state.1 == sig && state.0.is_some_and(|h| vis.contains(h)) {
        return;
    }
    state.1 = sig.clone();
    let holder = match state.0.filter(|h| vis.contains(*h)) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => {
            let h = commands
                .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Visibility::Inherited, Pickable::IGNORE, ChildOf(root)))
                .id();
            state.0 = Some(h);
            h
        }
    };
    let Some(template) = ui.export("HUDInteractionQueueItem", 1).cloned() else { return };
    let mut x = 0.0;
    for (i, (label, pic, with_progress)) in sig.into_iter().enumerate() {
        let h = if with_progress { QUEUE_ITEM_PROGRESS_HEIGHT } else { QUEUE_ITEM_HEIGHT };
        let mut item = template.clone();
        item.area = [x, QUEUE_ITEM_TOP, x + QUEUE_ITEM_WIDTH, QUEUE_ITEM_TOP + h];
        if let Some(b) = item.children.iter_mut().find(|c| c.id == QUEUE_BUTTON) {
            b.area = [0.0, 0.0, QUEUE_ITEM_WIDTH, h];
        }
        x += QUEUE_ITEM_WIDTH + QUEUE_ITEM_GAP;
        let s = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &item, holder);
        let picture = match pic {
            Some(QueuePicture::Object(objd)) => game_ui.as_deref_mut().and_then(|g| g.icon(&mut assets, &s3bake::gamedata::thumb_name(objd.2))).map(|h| (h, None)),
            Some(QueuePicture::Sim(e)) => Some((portraits.portrait(&mut assets, e), Some(e))),
            // (What a Sim does by themselves: their own face.)
            None => Some((portraits.portrait(&mut assets, me), Some(me))),
        };
        if let (Some(w), Some((h, of))) = (s.id(QUEUE_ICON), picture) {
            commands.entity(w).despawn_children();
            commands.entity(w).with_children(|c| {
                let mut img = c.spawn((ImageNode::new(h), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                if let Some(e) = of {
                    img.insert(crate::portraits::PortraitOf(e));
                }
            });
        }
        if let Some(p) = s.id(QUEUE_PROGRESS) {
            commands.entity(p).insert(if with_progress { Visibility::Inherited } else { Visibility::Hidden });
        }
        if let Some(b) = s.id(QUEUE_BUTTON) {
            commands.entity(b).insert((QueueItem(i, s.id(QUEUE_CANCEL), s.id(QUEUE_PROGRESS_CLIP)), crate::icons::Tooltip(format!("{label}\nClick to cancel."))));
        }
    }
}

/// A notification's pieces (`NotificationManager`'s exports): backgrounds for a Sim's words
/// and the system's, the one-thumbnail foreground.
const NOTE_SPEECH: u32 = 2;
const NOTE_SYSTEM: u32 = 3;
const NOTE_ONE_THUMB: u32 = 5;
const NOTE_TEXT: u32 = 2;
const NOTE_THUMB: u32 = 6;
const NOTE_BLUE: u32 = 0x41;
const NOTE_THUMB_FRAME: u32 = 0x05;
const NOTE_NO_MASK: u32 = 0x15;
const NOTE_CLOSE: u32 = 0x14;
/// The card's width and least height, and the room its thumbnail takes on the left.
const NOTE_WIDTH: f32 = 285.0;
const NOTE_HEIGHT: f32 = 55.0;
const NOTE_THUMB_ROOM: f32 = 34.0;

/// A notification's close button.
#[derive(Component)]
struct NoteClose(String);

/// The notifications (top right), in the game's own cards: a Sim's words with their face on
/// the left (the speech background), the game's own news without (the system one); closed
/// with their cross, or in time.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn notifications(
    mut commands: Commands,
    ui: Option<ResMut<UiAssets>>,
    (mut assets, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut notes: ResMut<crate::interact::Notifications>,
    people: Query<(Entity, &Sim, Has<HouseholdMember>)>,
    mut portraits: ResMut<crate::portraits::Portraits>,
    closes: Query<(&Interaction, &NoteClose), Changed<Interaction>>,
    vis: Query<&mut Visibility>,
    mut state: Local<(Option<Entity>, Vec<String>)>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    hud: Res<LiveHud>,
) {
    let Some(mut ui) = ui else { return };
    for (i, c) in &closes {
        if *i == Interaction::Pressed {
            notes.0.retain(|n| n.0 != c.0);
        }
    }
    let msgs: Vec<String> = notes.0.iter().map(|n| n.0.clone()).collect();
    if state.1 == msgs && state.0.is_some_and(|h| vis.contains(h)) {
        return;
    }
    // (A new one is heard.)
    if msgs.iter().any(|m| !state.1.contains(m)) {
        play.write(crate::sound::PlaySound::ui("ui_text_notification_open"));
    }
    state.1 = msgs.clone();
    let holder = match state.0.filter(|h| vis.contains(*h)) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => {
            let h = commands
                .spawn((
                    Node { position_type: PositionType::Absolute, right: Val::Px(14.0), top: Val::Px(14.0), width: Val::Px(NOTE_WIDTH + NOTE_THUMB_ROOM), flex_direction: FlexDirection::Column, row_gap: Val::Px(12.0), ..default() },
                    Visibility::Inherited,
                    Pickable::IGNORE,
                    GlobalZIndex(6),
                    DespawnOnExit(crate::AppState::InGame),
                ))
                .id();
            state.0 = Some(h);
            h
        }
    };
    let _ = &hud;
    for msg in msgs.iter().rev().take(6) {
        // Like the game's, a notice about a Sim carries their picture: the household's Sim (or
        // else anyone about) the notice begins with.
        let starts = |s: &Sim| msg.starts_with(&format!("{} ", s.first)) || msg.starts_with(&format!("{}'", s.first)) || msg.starts_with(&s.full_name());
        let about = people.iter().filter(|(_, s, _)| !s.first.is_empty() && starts(s)).max_by_key(|(_, _, member)| *member).map(|(e, ..)| e);
        let (Some(mut bg), Some(mut fg)) = (ui.export("NotificationManager", if about.is_some() { NOTE_SPEECH } else { NOTE_SYSTEM }).cloned(), ui.export("NotificationManager", NOTE_ONE_THUMB).cloned()) else { continue };
        // (Taller for longer words: about 44 characters a line.)
        let lines = (msg.chars().count() as f32 / 44.0).ceil().max(2.0);
        let h = (lines * 14.0 + 16.0).max(NOTE_HEIGHT);
        bg.area = [NOTE_THUMB_ROOM, 0.0, NOTE_THUMB_ROOM + NOTE_WIDTH, h];
        fg.area = [NOTE_THUMB_ROOM, 0.0, NOTE_THUMB_ROOM + NOTE_WIDTH, h];
        bg.flags |= s3bake::ui::WIN_VISIBLE;
        fg.flags |= s3bake::ui::WIN_VISIBLE;
        let card = commands.spawn((Node { width: Val::Px(NOTE_WIDTH + NOTE_THUMB_ROOM), height: Val::Px(h), ..default() }, Visibility::Inherited, Pickable::IGNORE, ChildOf(holder))).id();
        let b = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &bg, card);
        let f = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &fg, card);
        if let Some(t) = f.text(NOTE_TEXT) {
            commands.entity(t).insert(Text::new(msg.clone()));
        }
        if let Some(blue) = b.id(NOTE_BLUE) {
            commands.entity(blue).insert(Visibility::Inherited);
        }
        if let Some(c) = b.id(NOTE_CLOSE) {
            commands.entity(c).insert(NoteClose(msg.clone()));
        }
        match (about, b.id(NOTE_THUMB)) {
            (Some(e), Some(t)) => {
                let h = portraits.portrait(&mut assets, e);
                commands.entity(t).despawn_children();
                commands.entity(t).with_children(|c| {
                    c.spawn((ImageNode::new(h), crate::portraits::PortraitOf(e), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                });
            }
            (None, Some(t)) => {
                // (The game's own news: no face, nor its frame.)
                for w in [Some(t), b.id(NOTE_THUMB_FRAME), b.id(NOTE_NO_MASK)].into_iter().flatten() {
                    commands.entity(w).insert(Visibility::Hidden);
                }
            }
            _ => {}
        }
        for r in [b.root, f.root].into_iter().flatten() {
            commands.entity(r).insert(crate::hud::BlocksWorld);
        }
    }
}
