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
        app.init_resource::<InfoPanel>().init_resource::<SolveNeed>().init_resource::<NoteLog>().add_systems(OnEnter(PlayMode::Live), spawn_live_hud).add_systems(
            Update,
            (time_control, puck, mood_meter, motives_panel, bust, bust_click, skewer, navigation, show_in_modes.after(navigation), (moodlets, moodlet_click).chain(), wishes, skills_panel, skill_journal_buttons, interaction_queue, (lesson_answer_test, notifications).chain(), note_focus).run_if(in_state(PlayMode::Live)).run_if(resource_exists::<LiveHud>),
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
pub(crate) const PUCK_OPTIONS: u32 = PUCK + 0x16;
const PUCK_RECORD: u32 = PUCK + 0x17;
const PUCK_SNAPSHOT: u32 = PUCK + 0x18;
/// Rotate left and right, zoom out and in, pitch down and up, Sim, house and map views.
const PUCK_CAMERA: u32 = PUCK + 0x100;
const PUCK_MEMORY: u32 = 0x0b10_a780;
const SKEWER_SLOT: u32 = 0xf6fd_a501;
const SKEWER_PETS: u32 = 0xf6fd_a520;
/// The pets' slots, and each skewer's rod (`HouseholdSkwewer`, `PetSkewer`).
const SKEWER_PET_SLOT: u32 = 0xf6fd_a521;
const SKEWER_ROD: u32 = 0xf6fd_a510;
const SKEWER_PET_ROD: u32 = 0xf6fd_a530;
/// A pet slot's frame opening (its 54-square thumbnail window's, as the household's).
const PET_THUMB_OPENING: [f32; 4] = [12.0, 5.0, 8.0, 15.0];
/// A slot's follow notice (`mFollowNotification`).
const SLOT_FOLLOW: u32 = 7;
const SLOT_THUMB: u32 = 1;
/// How far in from the thumbnail window's edges (left, top, right, bottom) its frame's opening
/// is: the frame's window sits 2 to the right of it and 5 above, its picture's opening some
/// 41 square in its middle.
const SLOT_THUMB_OPENING: [f32; 4] = [14.0, 6.0, 10.0, 18.0];
const SLOT_MOOD: u32 = 2;
const SLOT_BUTTON: u32 = 3;
/// A slot's Go Home button (the selected Sim's, while they're out: the game's `mGoButton`).
const SLOT_GO_HOME: u32 = 4;
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
    selected_queue: Query<&crate::interact::ActionQueue, With<crate::sim::Selected>>,
) {
    let d = &hud.display;
    // (The fifth, Skip, only with something to skip through: `clock::skip_ends`.)
    let busy = selected_queue.single().is_ok_and(|q| q.0.front().is_some());
    for i in 0..5 {
        if pressed(&clicks, d.id(TIME_BASE + 1 + i)) && !buy.active && !menu.is_open() && modal.is_empty() && (i < 4 || busy) {
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
            let (sel, dis) = (i as usize == now, (i == 4 && !busy && now != 4) || locked);
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
                b.view_level = (b.view_level as i8 + step).clamp(b.data.lowest_storey() as i8, b.top_level.max(1) as i8) as u8;
            }
        }
        for (id, ok) in [(PUCK_LEVEL_UP, b.view_level < b.top_level), (PUCK_LEVEL_DOWN, b.view_level > b.data.lowest_storey())] {
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
#[allow(clippy::too_many_arguments)]
fn mood_meter(
    mut commands: Commands,
    hud: Res<LiveHud>,
    sel: Query<(&crate::life::Mood, &crate::life::Moodlets, &Sim), With<Selected>>,
    mut nodes: Query<&mut Node>,
    mut images: Query<&mut ImageNode>,
    pictures: Query<&UiPicture>,
    mut vis: Query<&mut Visibility>,
    mut shown: Local<f32>,
    time: Res<Time>,
    (ui, mut tips): (Option<Res<UiAssets>>, Query<&mut crate::icons::Tooltip>),
) {
    let Ok((mood, moodlets, sim)) = sel.single() else { return };
    // (Its tooltip, the game's `MoodMeterTooltip`: how they feel, and what the meter is.)
    if let Some(ui) = ui.as_deref() {
        let word = |key: String, or: &str| ui.localize(&key).unwrap_or_else(|| or.to_string());
        let flavour = mood_flavour(mood.0, moodlets);
        let tip = [
            word(format!("Ui/Tooltip/HUD/SimDisplay:MoodFlavor{}", flavour as u8), flavour.name()),
            word("Ui/Tooltip/HUD/SimDisplay/MoodMeter:Description".into(), ""),
            word("Ui/Tooltip/HUD/SimDisplay/MoodMeter:BonusDescription".into(), ""),
        ]
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
        set_tooltip(&mut commands, &mut tips, hud.display.id(MOOD_REFERENCE), tip);
        // (The Sim's name over their picture.)
        set_tooltip(&mut commands, &mut tips, hud.display.id(BUST), sim.full_name());
    }
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

/// The game's needs, as its tooltips name them (`MotiveID`).
const MOTIVE_KEYS: [&str; 6] = ["Hunger", "Bladder", "Energy", "Social", "Hygiene", "Fun"];

/// The motives panel's bars: each filled to its need, in the game's colours; each with the
/// game's tooltip, the need and what it's about (`MotiveTooltip`).
#[allow(clippy::too_many_arguments)]
fn motives_panel(
    mut commands: Commands,
    hud: Res<LiveHud>,
    sel: Query<&Motives, With<Selected>>,
    mut nodes: Query<&mut Node>,
    mut images: Query<&mut ImageNode>,
    pictures: Query<&UiPicture>,
    (ui, mut tips): (Option<Res<UiAssets>>, Query<&mut crate::icons::Tooltip>),
    mut tipped: Local<bool>,
) {
    let Ok(m) = sel.single() else { return };
    let s = &hud.motives;
    if !*tipped && let Some(ui) = ui.as_deref() {
        *tipped = true;
        for (i, k) in MOTIVE_KEYS.iter().enumerate() {
            let name = ui.localize(&format!("Ui/Caption/HUD/MotivesPanel:Motive{k}")).unwrap_or_else(|| k.to_string());
            let about = ui.localize(&format!("Ui/Caption/HUD/MotivesPanel:MotiveDescription{k}")).unwrap_or_default();
            let bar = s.id(MOTIVE_TOP + i as u32).and_then(|t| s.within(t, MOTIVE_REFERENCE));
            set_tooltip(&mut commands, &mut tips, bar, format!("{name}\n{about}"));
        }
    }
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

/// The game's mood flavours (`MoodFlavor`), as the mood meter's tooltip names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoodFlavour {
    Fair,
    Sad,
    Depressed,
    Tense,
    Stressed,
    Uncomfortable,
    Miserable,
    Angry,
    Furious,
    Happy,
    VeryHappy,
    Elated,
    Fulfilled,
}

impl MoodFlavour {
    fn name(self) -> &'static str {
        match self {
            MoodFlavour::Fair => "Fair",
            MoodFlavour::Sad => "Sad",
            MoodFlavour::Depressed => "Depressed",
            MoodFlavour::Tense => "Tense",
            MoodFlavour::Stressed => "Stressed",
            MoodFlavour::Uncomfortable => "Uncomfortable",
            MoodFlavour::Miserable => "Miserable",
            MoodFlavour::Angry => "Angry",
            MoodFlavour::Furious => "Furious",
            MoodFlavour::Happy => "Happy",
            MoodFlavour::VeryHappy => "Very Happy",
            MoodFlavour::Elated => "Elated",
            MoodFlavour::Fulfilled => "Fulfilled",
        }
    }
}

/// How a Sim feels, in the game's words (`MoodManager`'s thresholds: -50, -10, 10, 25, 75, 150):
/// a bad mood's kind by what's worst for them, a need ("Uncomfortable", "Miserable") or not
/// ("Sad", "Depressed").
pub fn mood_flavour(mood: f32, moodlets: &crate::life::Moodlets) -> MoodFlavour {
    let worst_is_need = moodlets.0.iter().filter(|m| m.value < 0).min_by_key(|m| m.value).is_some_and(|m| m.kind.def().hours == 0.0);
    match mood {
        m if m < -50.0 => if worst_is_need { MoodFlavour::Miserable } else { MoodFlavour::Depressed },
        m if m < -10.0 => if worst_is_need { MoodFlavour::Uncomfortable } else { MoodFlavour::Sad },
        m if m < 10.0 => MoodFlavour::Fair,
        m if m < 25.0 => MoodFlavour::Happy,
        m if m < 75.0 => MoodFlavour::VeryHappy,
        m if m <= 150.0 => MoodFlavour::Elated,
        _ => MoodFlavour::Fulfilled,
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
    commands.entity(win).despawn_children().insert(Interaction::default());
    let h = portraits.portrait(&mut images, s);
    // (The whole of the bust's window, as the game draws the Sim's picture there: rising over
    // the panel's top, the head clear of the picture's edges.)
    commands.entity(win).with_children(|b| {
        b.spawn((ImageNode::new(h), crate::portraits::PortraitOf(s), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
    });
}

/// The selected Sim's picture clicked: their own menu (what clicking them gives); right-clicked,
/// the camera follows them about (the game's `AttachCameraToSim`).
fn bust_click(
    mut commands: Commands,
    hud: Res<LiveHud>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    (mouse, hovered): (Res<ButtonInput<MouseButton>>, Query<&Interaction>),
    sel: Query<(Entity, &Transform), With<Selected>>,
    mut cam: Query<&mut crate::camera::SimsCamera>,
) {
    let bust = hud.display.id(BUST);
    if pressed(&clicks, bust)
        && let Ok((e, _)) = sel.single()
    {
        commands.insert_resource(crate::hud::PieFor(e));
    }
    if mouse.just_pressed(MouseButton::Right)
        && bust.and_then(|b| hovered.get(b).ok()).is_some_and(|i| *i != Interaction::None)
        && let (Ok((e, tf)), Ok(mut c)) = (sel.single(), cam.single_mut())
    {
        c.look_at(tf.translation);
        c.follow(e);
        debug!("the camera follows the selected Sim (bust right-clicked)");
    }
}

/// The skewer: the household's faces from the bottom up, each ringed in its mood's colour, the
/// selected one shown so; clicked, they're selected (again, the camera goes to them).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn skewer(
    mut commands: Commands,
    hud: Res<LiveHud>,
    members: Query<(Entity, &crate::life::Mood, Has<Selected>, Has<crate::visit::OnLot>), With<HouseholdMember>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut buttons: Query<&mut UiButton>,
    mut vis: Query<&mut Visibility>,
    mut images: Query<&mut ImageNode>,
    pictures: Query<&UiPicture>,
    (mut portraits, mut assets): (ResMut<crate::portraits::Portraits>, ResMut<Assets<Image>>),
    (selected, positions, mut cam): (Query<Entity, With<Selected>>, Query<(&Transform, &Sim, Has<crate::nav::PathFollow>)>, Query<&mut crate::camera::SimsCamera>),
    mut faces: Local<Vec<Option<Entity>>>,
    (mouse, hovered, mut tips, mut queues, ui): (Res<ButtonInput<MouseButton>>, Query<&Interaction>, Query<&mut crate::icons::Tooltip>, Query<&mut crate::interact::ActionQueue>, Option<Res<UiAssets>>),
    (pets, mut nodes, mut pet_faces, gu): (Query<(Entity, &crate::pets::Pet, &crate::petneeds::PetMotives, &Transform, (Option<&crate::pets::PetTraits>, Option<&crate::dogtricks::DogTricks>)), With<crate::pets::HomePet>>, Query<&mut Node>, Local<Vec<Option<Entity>>>, Option<Res<crate::icons::GameUi>>),
) {
    let s = &hud.skewer;
    let out: Vec<Entity> = members.iter().filter(|m| m.3).map(|m| m.0).collect();
    let mut list: Vec<(Entity, f32, bool)> = members.iter().map(|(e, m, sel, _)| (e, m.0, sel)).collect();
    list.sort_by_key(|x| x.0);
    // The rod through the faces, as long as they need (the game's `PopulateSkewer`: 60 a face).
    skewer_rod(&mut nodes, &mut vis, s.id(SKEWER_ROD), list.len().min(8), 60.0, 538.0);
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
            && let Some(t) = s.within(c, SLOT_THUMB)
            && let Ok(p) = pictures.get(t)
            && let Ok(mut img) = images.get_mut(p.0)
        {
            img.image = portraits.portrait(&mut assets, e);
            commands.entity(p.0).insert(crate::portraits::PortraitOf(e));
            // (In the frame's opening, head and shoulders, as the game's skewer has them.)
            crate::layout::fit_inside(&mut commands, t, p.0, SLOT_THUMB_OPENING);
            faces[slot] = Some(e);
        }
        // (Their name, as the game's tooltip.)
        if let Ok((_, sim, _)) = positions.get(e) {
            set_tooltip(&mut commands, &mut tips, button, sim.full_name());
        }
        // The selected Sim's Go Home, while they're out (child and up: `ShouldShowGoHomeButtonForSim`).
        let go = s.within(c, SLOT_GO_HOME);
        let away = sel && out.contains(&e) && positions.get(e).is_ok_and(|(_, sim, _)| !sim.age.is_little());
        set_visible(&mut vis, go, away);
        if away {
            let tip = ui.as_ref().and_then(|u| u.localize("Ui/Tooltip/HUD/Skewer:GoPlaces")).unwrap_or_else(|| "Click to Go Home".into());
            set_tooltip(&mut commands, &mut tips, go, tip);
        }
        if away
            && pressed(&clicks, go)
            && let Ok(mut q) = queues.get_mut(e)
            && !q.0.iter().any(|a| matches!(a.kind, crate::interact::ActionKind::GoHomeFromLot) && !a.cancel)
        {
            q.push_player(crate::interact::Action::new("Go Home", crate::interact::ActionKind::GoHomeFromLot, false));
        }
        if pressed(&clicks, button) {
            let already = selected.contains(e);
            for o in &selected {
                commands.entity(o).remove::<Selected>();
            }
            commands.entity(e).insert(Selected);
            // (Clicked again: the camera to them, following them if they're on the move: the
            // game's `MoveCameraToSim`.)
            if already
                && let (Ok((tf, _, walking)), Ok(mut cm)) = (positions.get(e), cam.single_mut())
            {
                cm.look_at(tf.translation);
                if walking {
                    cm.follow(e);
                }
            }
        }
        // (Right-clicked: the camera follows them about, whoever's selected: `AttachCameraToSim`.)
        if mouse.just_pressed(MouseButton::Right)
            && button.and_then(|b| hovered.get(b).ok()).is_some_and(|i| *i != Interaction::None)
            && let (Ok((tf, _, _)), Ok(mut cm)) = (positions.get(e), cam.single_mut())
        {
            cm.look_at(tf.translation);
            cm.follow(e);
            debug!("the camera follows skewer slot {slot}");
        }
    }
    // The household's pets on their own skewer beside it (the game's `PopulatePetSkewer`: 46 a
    // face), each ringed in its mood (how its needs stand), its name and what it needs on hover;
    // clicked, the camera goes to it.
    let mut pet_list: Vec<_> = pets.iter().collect();
    pet_list.sort_by_key(|p| p.0);
    set_visible(&mut vis, s.id(SKEWER_PETS), !pet_list.is_empty());
    skewer_rod(&mut nodes, &mut vis, s.id(SKEWER_PET_ROD), pet_list.len().min(8), 46.0, 423.0);
    pet_faces.resize(8, None);
    for slot in 0..8 {
        let container = s.id(SKEWER_PET_SLOT + slot as u32);
        let who = pet_list.get(slot).copied();
        set_visible(&mut vis, container, who.is_some());
        let (Some(c), Some((e, pet, m, tf, (traits, tricks)))) = (container, who) else { continue };
        let button = s.within(c, SLOT_BUTTON);
        set_visible(&mut vis, s.within(c, SLOT_GO_HOME), false);
        set_visible(&mut vis, s.within(c, SLOT_FOLLOW), false);
        set_shade(&mut images, &pictures, s.within(c, SLOT_MOOD), mood_single(crate::petneeds::mood(m)));
        if pet_faces[slot] != Some(e)
            && let Some(t) = s.within(c, SLOT_THUMB)
            && let Ok(p) = pictures.get(t)
            && let Ok(mut img) = images.get_mut(p.0)
        {
            img.image = portraits.portrait(&mut assets, e);
            commands.entity(p.0).insert(crate::portraits::PortraitOf(e));
            crate::layout::fit_inside(&mut commands, t, p.0, PET_THUMB_OPENING);
            pet_faces[slot] = Some(e);
        }
        set_tooltip(&mut commands, &mut tips, button, crate::petneeds::tooltip(pet, m, traits.map_or(&[][..], |t| &t.0[..]), gu.as_deref()) + &tricks.map_or(String::new(), crate::dogtricks::summary));
        if pressed(&clicks, button)
            && let Ok(mut cm) = cam.single_mut()
        {
            cm.look_at(tf.translation);
            cm.follow(e);
        }
    }
}

/// A skewer's rod: from the topmost face down (`num - num4` in the game's `PopulateSkewer`),
/// shown while there's anyone on it.
fn skewer_rod(nodes: &mut Query<&mut Node>, vis: &mut Query<&mut Visibility>, rod: Option<Entity>, count: usize, delta: f32, bottom: f32) {
    set_visible(vis, rod, count > 0);
    let Some(mut n) = rod.and_then(|r| nodes.get_mut(r).ok()) else { return };
    let top = delta * (8 - count) as f32;
    let (t, h) = (Val::Px(top), Val::Px(bottom - top));
    if n.top != t || n.height != h {
        n.top = t;
        n.height = h;
    }
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
    let journal_open = *panel == InfoPanel::Skills && journal.0.is_some() && !crate::skilljournal::GAME_JOURNAL.load(std::sync::atomic::Ordering::Relaxed);
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

/// In buy and build mode the Sim display, skewer and panels make way (the puck stays); in the
/// game's Edit Town, the puck too.
fn show_in_modes(hud: Res<LiveHud>, buy: Res<crate::buy::BuyMode>, mut vis: Query<&mut Visibility>, panel: Res<InfoPanel>, edit: Res<crate::edittown::EditTown>, mut was_editing: Local<bool>) {
    let editing = edit.is_open() && crate::edittownui::GAME_EDIT_TOWN.load(std::sync::atomic::Ordering::Relaxed);
    if !buy.is_changed() && !panel.is_changed() && editing == *was_editing {
        return;
    }
    if editing != *was_editing {
        set_visible(&mut vis, hud.puck.root, !editing);
    }
    *was_editing = editing;
    let live = !buy.active && !editing;
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
    sel: Query<(&crate::life::Moodlets, Option<&crate::sim::Sim>), With<Selected>>,
    clock: Res<crate::clock::GameClock>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut vis: Query<&mut Visibility>,
    mut state: Local<(Option<Entity>, Vec<String>, u32, usize, bool)>,
) {
    let (Some(mut ui), Some(grid)) = (ui, hud.display.id(MOODLET_GRID)) else { return };
    let Some(g) = ui.layout("HUDSimDisplay").and_then(|w| w.find(MOODLET_GRID)).and_then(|w| w.grid) else { return };
    // (Theirs, with their name for the descriptions that have it: "Today is Bella's lucky day.")
    let (mut list, who): (Vec<crate::life::Moodlet>, String) = sel.single().map(|(m, s)| (m.0.clone(), s.map(|s| s.first.clone()).unwrap_or_default())).unwrap_or_default();
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
    let mut labels: Vec<String> = list.iter().map(crate::life::moodlet_label).collect();
    labels.push(who.clone());
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
        let desc = crate::icons::with_name(&desc, &who);
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
            commands.entity(r).insert((Interaction::default(), crate::hud::BlocksWorld, crate::icons::Tooltip(tip), MoodletCell(m.kind)));
        }
    }
}

/// A moodlet's cell in the grid, for clicking.
#[derive(Component)]
struct MoodletCell(crate::life::MoodletKind);

/// A need to see to now, for a Sim: a moodlet clicked (`interact`'s free will picks what helps,
/// whatever the free-will setting).
#[derive(Resource, Default)]
pub struct SolveNeed(pub Option<(Entity, usize)>);

/// The need a moodlet is solved by (the game's buffs' `SolveCommodity`): the needs' own, and
/// stress by fun.
pub fn solves(kind: crate::life::MoodletKind) -> Option<usize> {
    use crate::life::MoodletKind::*;
    use crate::sim::{BLADDER, ENERGY, FUN, HUNGER, HYGIENE, SOCIAL};
    match kind {
        Hungry | Starving => Some(HUNGER),
        StrainedBladder => Some(BLADDER),
        Tired | Exhausted => Some(ENERGY),
        Lonely => Some(SOCIAL),
        Smelly => Some(HYGIENE),
        Bored | Stressed => Some(FUN),
        _ => None,
    }
}

/// A moodlet clicked, as the game's `OnMoodletClicked`: one a need can solve sends the Sim to
/// see to it (`Buff.SolveMe`); with the testing cheats on, Ctrl+click takes it away.
fn moodlet_click(
    clicks: Query<(&Interaction, &MoodletCell), Changed<Interaction>>,
    mut sel: Query<(Entity, &mut crate::life::Moodlets, &crate::interact::ActionQueue), With<Selected>>,
    mut solve: ResMut<SolveNeed>,
    keys: Res<ButtonInput<KeyCode>>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some((_, cell)) = clicks.iter().find(|(i, _)| **i == Interaction::Pressed) else { return };
    let Ok((me, mut moodlets, queue)) = sel.single_mut() else { return };
    if crate::cheats::testing() && keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        moodlets.remove(cell.0);
        return;
    }
    let Some(need) = solves(cell.0) else { return };
    // (Not if something they're about to do already sees to it: `GetInteractionSatisfying`.)
    if queue.0.iter().any(|a| !a.cancel && a.label != "Go Here" && crate::interact::satisfies(a, need)) {
        return;
    }
    solve.0 = Some((me, need));
    play.write(crate::sound::PlaySound::ui("ui_queue"));
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
    (mouse, hovered, mut buttons): (Res<ButtonInput<MouseButton>>, Query<&Interaction>, Query<&mut UiButton>),
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
    // (A wish's picture, and the game's own wishes' corner one: a skill's, a friend's...)
    let mut icon = |x: &crate::wishes::Wish, corner: bool| -> Option<Handle<Image>> {
        let gu = game_ui.as_deref_mut()?;
        let name = if corner { x.dream().map(|d| d.icon2.clone()).unwrap_or_default() } else { x.icon(&gu.data.clone()) };
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
        set_image(&mut images, &pictures, d.within(st, WISH_ICON), icon(x, false));
        let corner = icon(x, true);
        set_visible(&mut vis, d.within(st, WISH_ICON_2), corner.is_some());
        set_image(&mut images, &pictures, d.within(st, WISH_ICON_2), corner);
        let more = if n > 1 { format!(" ({} of {n})", *page + 1) } else { String::new() };
        let action = if w.promised.len() < 4 { "Click to promise. Right-click to dismiss." } else { "All four wish slots are full. Right-click to dismiss." };
        set_tooltip(&mut commands, &mut tips, Some(st), format!("{}{more}\n{action}", wish_tip(x, &sim.traits)));
    }
    // The promised wishes.
    for slot in 0..4 {
        let b = d.id(WISH_SLOT + slot as u32);
        let x = w.promised.get(slot).cloned();
        set_visible(&mut vis, b, x.is_some());
        // (One towards the lifetime wish lit in its slot, as the game's.)
        let helper = x.as_ref().and_then(|x| x.dream()).is_some_and(|d| d.helper);
        if let Some(mut ub) = b.and_then(|b| buttons.get_mut(b).ok())
            && ub.selected != helper
        {
            ub.selected = helper;
        }
        if let (Some(b), Some(x)) = (b, x) {
            set_image(&mut images, &pictures, d.within(b, WISH_ICON), icon(&x, false));
            let corner = icon(&x, true);
            set_visible(&mut vis, d.within(b, WISH_ICON_2), corner.is_some());
            set_image(&mut images, &pictures, d.within(b, WISH_ICON_2), corner);
            set_tooltip(&mut commands, &mut tips, Some(b), format!("Promised: {}\nRight-click to dismiss.", wish_tip(&x, &sim.traits)));
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
        let desc = info.as_ref().map(|i| i.desc_for(sim.female).replace("{0.SimFirstName}", &sim.first)).unwrap_or_default();
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

/// What a queued action shows: an object's catalogue picture, a Sim's face, or one of the
/// interface's icons (going to work: the career's).
#[derive(Clone, PartialEq, Debug)]
enum QueuePicture {
    Object(s3bake::Key),
    Sim(Entity),
    Icon(&'static str),
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

/// The queue's first item while the Sim's in a conversation, as the game's `InteractionStyle.
/// Relationship`: whom they're talking to (their face), what they are to each other (the LTR
/// words), how the talk's going (the STC bubble) and the relationship's bar; clicked, the
/// conversation's over (`CancelSocializing`).
#[derive(Clone, PartialEq)]
struct TalkItem {
    with: Entity,
    name: String,
    ltr: String,
    stc: String,
    value: i32,
}

/// The relationship item's windows (`InteractionQueueItem.ControlIDs`), its width (the game's
/// `mInteractionStyleWidths` for it), its bar's colours (`FILL_COLOR_POS`, `FILL_COLOR_NEG`) and
/// how long its arrows show after a change.
const TALK_INFO: u32 = 0x04f6_7f00;
const TALK_LTR: u32 = 0x04f6_7f0d;
const TALK_STC: u32 = 0x04f6_7f06;
const TALK_CLIP: u32 = 0x04f6_7f07;
const TALK_FILL: u32 = 0x04f6_7f08;
const TALK_ARROWS: [u32; 4] = [0x04f6_7f09, 0x04f6_7f0a, 0x04f6_7f0b, 0x04f6_7f0c];
const TALK_WIDTH: f32 = 180.0;
const TALK_FILL_POS: u32 = 0xff00_f188;
const TALK_FILL_NEG: u32 = 0xffe8_005d;
const TALK_ARROW_SECS: f32 = 2.6;

/// The work item's windows (`InteractionQueueItem`'s Tones style): the tones' combo box and the
/// box of their words, the career's icon (in place of the item's) and its performance arrow,
/// the progress bar's fill, and the cancel cross over the icon (left without an id by the
/// layout); the style's width (`mInteractionStyleWidths`) and its progress bar's
/// (`kProgressBarTonesWidth`).
const TONES: u32 = 0x04f6_7d02;
const TONES_TIP: u32 = 0x04f6_7d04;
const TONES_ICON: u32 = 0x04f6_7d0d;
const TONES_CAREER_STATUS: u32 = 0x04f6_7d0c;
const TONES_CANCEL: u32 = 0x04f6_7d06;
const QUEUE_PROGRESS_FILL: u32 = 0x04f6_7e02;
const TONES_WIDTH: f32 = 123.0;
const TONES_PROGRESS_WIDTH: f32 = 109.0;
/// The work item's place in the queue (its `QueueItem`).
const WORK_ITEM: usize = usize::MAX - 1;

/// What the work item shows: the career's icon, the tones they may work in and the one they
/// are, which way their performance is going.
#[derive(Clone, PartialEq)]
struct WorkItem {
    icon: &'static str,
    tones: Vec<crate::careers::WorkTone>,
    labels: Vec<String>,
    tone: usize,
    rising: Option<bool>,
}

/// The work item's tones, in its combo box's order.
#[derive(Component)]
struct WorkTones(Vec<crate::careers::WorkTone>);

/// A window of a layout, by its id, to change before it's put up.
fn find_window(w: &mut s3bake::ui::UiWindow, id: u32) -> Option<&mut s3bake::ui::UiWindow> {
    if w.id == id {
        return Some(w);
    }
    w.children.iter_mut().find_map(|c| find_window(c, id))
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
    (mut vis, mut nodes, mouse): (Query<&mut Visibility>, Query<&mut Node>, Res<ButtonInput<MouseButton>>),
    mut state: Local<(Option<Entity>, (Option<WorkItem>, Option<TalkItem>, Vec<(String, Option<QueuePicture>, bool)>))>,
    (convs, rels_q, people, time): (Res<crate::groups::Conversations>, Query<&crate::social::Relationships>, Query<&Sim>, Res<Time>),
    mut arrows: Local<(Option<(Entity, f32)>, f32, bool, Vec<Entity>)>,
    (mut workers, tone_boxes, clock, mut play): (
        Query<(&mut crate::careers::Job, Option<&mut crate::careers::AtWork>)>,
        Query<(&crate::layout::UiCombo, &WorkTones)>,
        Res<crate::clock::GameClock>,
        MessageWriter<crate::sound::PlaySound>,
    ),
) {
    let (Some(mut ui), Some(root)) = (ui, hud.queue.root) else { return };
    let Ok((me, mut queue, motives)) = sel.single_mut() else { return };
    // At work: the tone picked from the work item's combo box is how they work now.
    if let Ok((mut job, Some(_))) = workers.get_mut(me) {
        for (combo, tones) in &tone_boxes {
            if let Some(&tone) = tones.0.get(combo.selected)
                && tone != job.tone
                && !combo.open
            {
                job.tone = tone;
                play.write(crate::sound::PlaySound::ui("ui_queue"));
            }
        }
    }
    let work = workers.get(me).ok().filter(|(_, at)| at.is_some()).map(|(job, _)| {
        let traits = people.get(me).map(|s| s.traits.clone()).unwrap_or_default();
        let tones: Vec<crate::careers::WorkTone> = crate::careers::WorkTone::ALL.into_iter().filter(|t| t.offered(&job, &traits)).collect();
        WorkItem {
            icon: job.career().icon,
            labels: tones.iter().map(|t| t.label(&job)).collect(),
            tone: tones.iter().position(|t| *t == job.tone).unwrap_or(0),
            tones,
            rising: (job.rate != 0.0).then_some(job.rate > 0.0),
        }
    });
    let work_done = workers.get(me).ok().and_then(|(_, w)| w).map(|w| ((clock.minutes - w.from) / (w.until - w.from).max(1.0)) as f32);
    // In a conversation: whom with (the one being talked with now, else the first of them).
    let talk = convs.group(me).and_then(|g| {
        let with = g.talk.and_then(|(a, b)| if a == me { Some(b) } else if b == me { Some(a) } else { None }).or_else(|| g.members.iter().copied().find(|m| *m != me))?;
        let other = people.get(with).ok().filter(|s| !s.age.is_little())?;
        let rel = rels_q.get(me).ok().and_then(|r| r.0.get(&with)).cloned().unwrap_or_default();
        Some(TalkItem { with, name: other.full_name(), ltr: rel.label_for(other.female), stc: rel.stc.name().to_string(), value: rel.friendship.round() as i32 })
    });
    // A click cancels (either button, as the game's `InteractionMouseUp`).
    let right = mouse.just_pressed(MouseButton::Right);
    let hit: Vec<usize> = clicks
        .iter()
        .filter(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, q)| q.0)
        .chain(items.iter().filter(|(i, _)| right && **i != Interaction::None).map(|(_, q)| q.0))
        .collect();
    for n in hit {
        // (The work item: they leave work early, paid for the hours they were there.)
        if n == WORK_ITEM {
            if let Ok((_, Some(mut w))) = workers.get_mut(me) {
                w.until = w.until.min(clock.minutes);
            }
            continue;
        }
        // (The conversation's item: it's over, and the socials queued in it with it.)
        if n == usize::MAX {
            let members = convs.group(me).map(|g| g.members.clone()).unwrap_or_default();
            for a in queue.0.iter_mut() {
                if matches!(a.kind, crate::interact::ActionKind::Social { target, .. } if members.contains(&target)) {
                    a.cancel = true;
                }
            }
            commands.queue(move |w: &mut World| w.resource_mut::<crate::groups::Conversations>().leave(me));
            continue;
        }
        if let Some(a) = queue.0.iter_mut().filter(|a| !a.cancel).nth(n) {
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
                crate::interact::ActionKind::GoToWork => workers.get(me).ok().map(|(j, _)| QueuePicture::Icon(j.career().icon)),
                _ => None,
            };
            (crate::gamenames::shown(&a.label).into_owned(), pic, i == 0 && progress(a, &objects, motives).is_some())
        })
        .collect();
    // The progress of the action under way, every frame.
    let head = shown.first().and_then(|a| progress(a, &objects, motives));
    // (The cancel cross while pointed at.)
    for (i, q) in &items {
        set_visible(&mut vis, q.1, *i != Interaction::None);
        let (f, full) = match q.0 {
            0 if work.is_none() => (head, QUEUE_PROGRESS_WIDTH),
            WORK_ITEM => (work_done, TONES_PROGRESS_WIDTH),
            _ => (None, 0.0),
        };
        if let (Some(clip), Some(f)) = (q.2, f)
            && let Ok(mut n) = nodes.get_mut(clip)
        {
            let w = Val::Px((f.clamp(0.0, 1.0) * full).round());
            if n.width != w {
                n.width = w;
            }
        }
    }
    // (The relationship bar's arrows a moment after it moves: up or down, flashing.)
    let now = time.elapsed_secs();
    if let Some(t) = &talk {
        let v = rels_q.get(me).ok().and_then(|r| r.0.get(&t.with)).map_or(0.0, |r| r.friendship);
        match arrows.0 {
            Some((w, old)) if w == t.with && (v - old).abs() > 0.05 => {
                arrows.1 = now;
                arrows.2 = v > old;
                arrows.0 = Some((w, v));
            }
            Some((w, _)) if w == t.with => {}
            _ => {
                arrows.0 = Some((t.with, v));
                arrows.1 = -100.0;
            }
        }
    }
    let since = now - arrows.1;
    for (k, e) in arrows.3.iter().enumerate() {
        let up = k < 2;
        let flash = ((since / 0.3) as i32 % 2 == 0) == (k % 2 == 0);
        let on = since < TALK_ARROW_SECS && up == arrows.2 && flash;
        set_visible(&mut vis, Some(*e), on);
    }
    let full = (work.clone(), talk.clone(), sig.clone());
    if state.1 == full && state.0.is_some_and(|h| vis.contains(h)) {
        return;
    }
    state.1 = full;
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
    arrows.3.clear();
    // (At work: the work item, as the game's `InteractionStyle.Tones`.)
    if let Some(w) = &work {
        let mut item = template.clone();
        let h = QUEUE_ITEM_PROGRESS_HEIGHT;
        item.area = [0.0, QUEUE_ITEM_TOP, TONES_WIDTH, QUEUE_ITEM_TOP + h];
        if let Some(b) = item.children.iter_mut().find(|c| c.id == QUEUE_BUTTON) {
            b.area = [0.0, 0.0, TONES_WIDTH, h];
            if let Some(s3bake::ui::UiDrawable::Std { images, .. }) = b.drawable.as_mut() {
                let (n, hl) = (s3pkg::fnv64("int_tone_progress_norm"), s3pkg::fnv64("int_tone_progress_hl"));
                *images = [n, n, hl, hl, n, n, hl, hl];
            }
            for c in b.children.iter_mut() {
                match c.id {
                    TONES | TONES_ICON | QUEUE_PROGRESS => c.flags |= s3bake::ui::WIN_VISIBLE,
                    QUEUE_ICON => c.flags &= !s3bake::ui::WIN_VISIBLE,
                    0 if c.area == [33.0, 4.0, 87.0, 58.0] => c.id = TONES_CANCEL,
                    _ => {}
                }
            }
        }
        for id in [QUEUE_PROGRESS_CLIP, QUEUE_PROGRESS_FILL] {
            if let Some(p) = find_window(&mut item, id) {
                p.area[2] = p.area[0] + TONES_PROGRESS_WIDTH;
            }
        }
        let tip_drawable = find_window(&mut item, TONES_TIP).and_then(|t| t.drawable.clone());
        let s = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &item, holder);
        if let Some(win) = s.id(TONES_ICON) {
            commands.entity(win).despawn_children();
            if let Some(h) = game_ui.as_deref_mut().and_then(|g| g.icon(&mut assets, w.icon)) {
                commands.spawn((ImageNode::new(h), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE, ChildOf(win)));
            }
            // (Which way their performance is going, as the game's `UpdateJobPerformance`.)
            let arrow = w.rising.and_then(|up| ui.image(&mut assets, s3pkg::fnv64(if up { "iq_arrow_gain" } else { "iq_arrow_loss" })).map(|p| p.0));
            if let (Some(a), Some(at)) = (arrow, ui.export("HUDInteractionQueueItem", 1).and_then(|t| t.find(TONES_CAREER_STATUS)).map(|c| c.area)) {
                commands.spawn((ImageNode::new(a), Node { position_type: PositionType::Absolute, left: Val::Px(at[0]), top: Val::Px(at[1]), width: Val::Px(at[2] - at[0]), height: Val::Px(at[3] - at[1]), ..default() }, Pickable::IGNORE, ChildOf(win)));
            }
        }
        if let Some(c) = s.id(TONES) {
            let (items, selected) = (w.labels.clone(), w.tone);
            let tips: Vec<String> = workers.get(me).map_or_else(|_| Vec::new(), |(job, _)| w.tones.iter().map(|t| t.description_key(&job).and_then(|k| ui.localize(&k)).unwrap_or_default()).collect());
            commands.entity(c).insert(WorkTones(w.tones.clone())).queue(move |mut ew: EntityWorldMut| {
                if let Some(mut combo) = ew.get_mut::<crate::layout::UiCombo>() {
                    combo.items = items;
                    combo.selected = selected;
                    combo.tips = tips;
                    combo.tip_drawable = tip_drawable;
                }
            });
        }
        if let Some(b) = s.id(QUEUE_BUTTON) {
            let name = ui.localize("Gameplay/Abstracts/RabbitHole/WorkInRabbitHole:InteractionName").unwrap_or_else(|| "Work".into());
            let tone = w.labels.get(w.tone).cloned().unwrap_or_default();
            commands.entity(b).insert((QueueItem(WORK_ITEM, s.id(TONES_CANCEL), s.id(QUEUE_PROGRESS_CLIP)), crate::icons::Tooltip(format!("{name} ({tone})\nClick to leave work early."))));
        }
        x = TONES_WIDTH + QUEUE_ITEM_GAP;
    }
    if let Some(t) = &talk {
        let mut item = template.clone();
        item.area = [x, QUEUE_ITEM_TOP, x + TALK_WIDTH, QUEUE_ITEM_TOP + QUEUE_ITEM_HEIGHT];
        if let Some(b) = item.children.iter_mut().find(|c| c.id == QUEUE_BUTTON) {
            b.area = [0.0, 0.0, TALK_WIDTH, QUEUE_ITEM_HEIGHT];
            // (Its own background: `int_relationship_bk`, lit when pointed at.)
            if let Some(s3bake::ui::UiDrawable::Std { images, .. }) = b.drawable.as_mut() {
                let (bk, hl) = (s3pkg::fnv64("int_relationship_bk"), s3pkg::fnv64("int_relationship_bk_hl"));
                *images = [bk, bk, hl, hl, bk, bk, hl, hl];
            }
            if let Some(info) = b.children.iter_mut().find(|c| c.id == TALK_INFO) {
                info.flags |= s3bake::ui::WIN_VISIBLE;
            }
        }
        // (The bar from its middle, right for liking, left for not: `UpdateRelationshipBar`; the
        // fill held where it is under the bar's marks while its clip moves. The layout gives the
        // fill in the clip's parent's terms.)
        if let Some(clip) = find_window(&mut item, TALK_CLIP) {
            let [l, top, r, bottom] = clip.area;
            let (mid, half) = ((l + r) * 0.5, (r - l) * 0.5);
            let reach = (t.value as f32).abs().min(100.0) / 100.0 * half;
            let (from, to) = if t.value >= 0 { (mid, mid + reach) } else { (mid - reach, mid) };
            clip.area = [from, top, to.max(from + 0.5), bottom];
            if let Some(fill) = clip.children.iter_mut().find(|c| c.id == TALK_FILL) {
                fill.place = s3bake::ui::UiPlace::Fixed;
                fill.area = [fill.area[0] - from, fill.area[1] - top, fill.area[2] - from, fill.area[3] - top];
                fill.shade = if t.value >= 0 { TALK_FILL_POS } else { TALK_FILL_NEG };
            }
        }
        let s = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &item, holder);
        if let Some(w) = s.id(QUEUE_ICON) {
            let h = portraits.portrait(&mut assets, t.with);
            commands.entity(w).despawn_children();
            commands.entity(w).with_children(|c| {
                c.spawn((ImageNode::new(h), crate::portraits::PortraitOf(t.with), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
            });
        }
        if let Some(e) = s.text(TALK_LTR) {
            commands.entity(e).insert(Text::new(t.ltr.clone()));
        }
        if let Some(e) = s.text(TALK_STC) {
            commands.entity(e).insert(Text::new(t.stc.clone()));
        }
        if let Some(e) = s.id(TALK_CLIP) {
            commands.entity(e).queue(|mut w: EntityWorldMut| {
                if let Some(mut n) = w.get_mut::<Node>() {
                    n.overflow = Overflow::clip();
                }
            });
        }
        for id in TALK_ARROWS {
            if let Some(e) = s.id(id) {
                arrows.3.push(e);
            }
        }
        if let Some(b) = s.id(QUEUE_BUTTON) {
            commands.entity(b).insert((QueueItem(usize::MAX, s.id(QUEUE_CANCEL), None), crate::icons::Tooltip(format!("{}\n{} ({})\nClick to stop socializing.", t.name, t.ltr, t.value))));
        }
        x += TALK_WIDTH + QUEUE_ITEM_GAP;
    }
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
            // (Something with no picture of its own in the catalogue, a puddle or a stand-in: the
            // Sim's own face rather than an empty frame.)
            Some(QueuePicture::Object(objd)) => game_ui
                .as_deref_mut()
                .and_then(|g| g.icon(&mut assets, &s3bake::gamedata::thumb_name(objd.2)))
                .map(|h| (h, None))
                .or_else(|| Some((portraits.portrait(&mut assets, me), Some(me)))),
            Some(QueuePicture::Sim(e)) => Some((portraits.portrait(&mut assets, e), Some(e))),
            Some(QueuePicture::Icon(name)) => game_ui
                .as_deref_mut()
                .and_then(|g| g.icon(&mut assets, name))
                .map(|h| (h, None))
                .or_else(|| Some((portraits.portrait(&mut assets, me), Some(me)))),
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
/// The cards' colours (the game's `StyledNotification.Show`): blue for a Sim's words, red for
/// bad news, yellow for the rest.
const NOTE_BLUE: u32 = 0x41;
const NOTE_RED: u32 = 0x42;
const NOTE_YELLOW: u32 = 0x43;
const NOTE_THUMB_FRAME: u32 = 0x05;
const NOTE_NO_MASK: u32 = 0x15;
const NOTE_CLOSE: u32 = 0x14;
/// The card's width and least height.
const NOTE_WIDTH: f32 = 285.0;
const NOTE_HEIGHT: f32 = 55.0;
/// A lesson's note (`Backgrounds.LessonBackground`): its words, its buttons (Yes, Later,
/// Never), its height and its words' (a line's).
const NOTE_LESSON: u32 = 6;
const NOTE_LESSON_TEXT_ID: u32 = 6;
const NOTE_LESSON_BUTTONS: [(u32, LessonAnswer); 3] = [(3, LessonAnswer::Yes), (4, LessonAnswer::Later), (5, LessonAnswer::Never)];
const NOTE_LESSON_HEIGHT: f32 = 91.0;
const NOTE_LESSON_TEXT: f32 = 27.0;
const NOTE_LINE: f32 = 20.0;

/// The panel's windows (`NotificationManager.ControlIds`): the window that slides and its
/// toggle tab, the tabs' strip and the tabs (Information, Lessons, Chatty; the expansions'
/// Connected and Store left out), a tab's glow, the page arrows and numbers.
const NOTES_SLIDE: u32 = 0x0994_f90b;
const NOTES_TOGGLE: u32 = 0x0994_f906;
const NOTES_STRIP: u32 = 0x0994_f90a;
const NOTES_TABS: [u32; 3] = [0x0994_f903, 0x0994_f904, 0x0994_f905];
const NOTES_EP_TABS: [u32; 2] = [0x0994_f90d, 0x0994_f90e];
const NOTES_TAB_GLOW: u32 = 0x0994_f909;
const NOTES_PREV: u32 = 0x0994_f900;
const NOTES_PAGE: u32 = 0x0994_f901;
const NOTES_NEXT: u32 = 0x0994_f902;
const NOTES_MAX: u32 = 0x0994_f907;
const NOTES_MAX_GLOW: u32 = 0x0994_f908;
/// The tabs a notice goes under (`TNSCategory`).
const TAB_INFORMATION: usize = 0;
const TAB_LESSONS: usize = 1;
const TAB_CHATTY: usize = 2;
/// The panel's size and where its card sits in it, how high on the screen it is, how far it
/// slides shut and how quickly (its `Glide`), the wait before a new notice shows
/// (`kShowDelay`), how long the panel stays open for one (`kDefaultTimeout`; longer for long
/// words, `kMinCharPerSecond`, `kSecondsPerCharToRead`), and how long one not on show is kept
/// (`kIndividualTimeout`).
const NOTES_SIZE: [f32; 2] = [468.0, 165.0];
const NOTES_CARD: [f32; 2] = [142.0, 19.0];
const NOTES_TOP: f32 = 22.0;
const NOTES_SLIDE_BY: f32 = 450.0;
const NOTES_SLIDE_SECS: f32 = 0.3;
const NOTES_SHOW_DELAY: f32 = 0.5;
const NOTES_TIMEOUT: f32 = 10.0;
const NOTES_KEPT: f32 = 120.0;

/// A notification card's height: taller for longer words (about 44 characters a line).
fn note_height(msg: &str) -> f32 {
    let lines = (msg.chars().count() as f32 / 44.0).ceil().max(2.0);
    (lines * 14.0 + 16.0).max(NOTE_HEIGHT)
}

/// How long the panel stays open for a notice: the game's ten seconds, or longer to read it.
fn note_timeout(msg: &str) -> f32 {
    let n = msg.chars().count() as f32;
    if n / 25.0 > NOTES_TIMEOUT { n * 0.05 } else { NOTES_TIMEOUT }
}

/// A notice in the panel: its words, style and whom it's about, and when it goes if it's not
/// shown again (`kIndividualTimeout`; none while it's the one on show); a lesson's note is its
/// lesson's (drawn by `lessons`, its own window in the card's place).
#[derive(Clone)]
struct Logged {
    msg: String,
    style: NoteStyle,
    about: Option<Entity>,
    expires: Option<f32>,
    lesson: Option<String>,
}

impl Logged {
    /// How long the panel stays open for it: a lesson's note the game's ten seconds
    /// (`TutorialetteNotification.kDefaultTimeout`), a notice by its words.
    fn timeout(&self) -> f32 {
        if self.lesson.is_some() { NOTES_TIMEOUT } else { note_timeout(&self.msg) }
    }

    /// The card's height: a lesson's note as tall as its words need (the game's `AutoSize`), a
    /// notice's by its words.
    fn height(&self) -> f32 {
        if self.lesson.is_none() {
            return note_height(&self.msg);
        }
        NOTE_LESSON_HEIGHT + (lesson_lines(&self.msg) as f32 * NOTE_LINE - NOTE_LESSON_TEXT).max(0.0)
    }
}

/// A lesson's note's lines (about 32 characters a line in its 220 pixels; each paragraph its
/// own).
fn lesson_lines(msg: &str) -> usize {
    msg.split('\n').map(|p| p.chars().count().div_ceil(32).max(1)).sum()
}

/// The notices as the game's `NotificationManager` keeps them: under their tabs, one on show at
/// a time with pages back through the rest; the panel slides shut once the notice's time is up
/// unless pointed at, and its toggle tab opens it again.
#[derive(Resource, Default)]
pub struct NoteLog {
    tabs: [Vec<Logged>; 3],
    tab: usize,
    page: usize,
    open: bool,
    /// When the panel shuts, and when the newest notice shows (`kShowDelay`).
    close_at: Option<f32>,
    show_at: Option<f32>,
    /// Tabs with notices not seen yet.
    glow: [bool; 3],
    /// How many notices have been taken in (`Notifications.1`).
    taken: u64,
    /// How far shut the panel is (0 open, 1 shut), and whether it's pointed at.
    slid: f32,
    hovering: bool,
    /// Lessons' notes answered (for `lessons`).
    pub answered: Vec<(String, LessonAnswer)>,
}

/// A lesson's note answered, by its buttons (`TutorialetteNotification`'s); gone from the panel
/// unanswered, it's as "Later".
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LessonAnswer {
    Yes,
    Later,
    Never,
}

impl NoteLog {
    fn current(&self) -> Option<&Logged> {
        self.tabs[self.tab].get(self.page)
    }

    /// The tab to show: the current one if it has news, else the first with news, else the
    /// current (the game's `Show`).
    fn news_tab(&self) -> usize {
        if self.glow[self.tab] { self.tab } else { (0..3).find(|t| self.glow[*t] && !self.tabs[*t].is_empty()).unwrap_or(self.tab) }
    }

    /// News under a tab: it glows, and, the panel shut, the newest shows a moment on.
    fn news(&mut self, tab: usize, now: f32) {
        self.glow[tab] = true;
        if self.tabs[self.tab].is_empty() {
            self.tab = tab;
        }
        if !self.open && self.show_at.is_none() {
            self.show_at = Some(now + NOTES_SHOW_DELAY);
        }
    }

    /// A lesson's note come up, under the Lessons tab (the game's `TutorialetteNotification`, a
    /// notice of the `Tutorialette` category).
    pub fn push_lesson(&mut self, key: &str, words: &str, now: f32) {
        self.tabs[TAB_LESSONS].retain(|n| n.lesson.as_deref() != Some(key));
        self.tabs[TAB_LESSONS].push(Logged { msg: words.to_string(), style: NoteStyle::System, about: None, expires: Some(now + NOTES_KEPT), lesson: Some(key.to_string()) });
        self.news(TAB_LESSONS, now);
    }

    /// Whether a lesson's note is in the panel (not answered, nor gone in time).
    pub fn has_lesson(&self, key: &str) -> bool {
        self.tabs[TAB_LESSONS].iter().any(|n| n.lesson.as_deref() == Some(key))
    }

    /// The notice on show gone: the next shown (or, the last gone, the one before), else another
    /// tab's newest, else the panel's shut (the game's `Remove`).
    fn remove_current(&mut self) {
        if self.page < self.tabs[self.tab].len() {
            self.tabs[self.tab].remove(self.page);
        }
        if self.tabs[self.tab].is_empty() {
            match (0..3).find(|t| !self.tabs[*t].is_empty()) {
                Some(t) => {
                    self.tab = t;
                    self.page = self.tabs[t].len() - 1;
                }
                None => self.open = false,
            }
        } else {
            self.page = self.page.min(self.tabs[self.tab].len() - 1);
        }
    }
}

/// The panel's controls.
#[derive(Component, Clone, Copy, Debug)]
enum NoteButton {
    Toggle,
    Prev,
    Next,
    Tab(usize),
    Close,
    Lesson(LessonAnswer),
}

/// What keeps the panel open while pointed at: its card and its tabs (and a lesson's note).
#[derive(Component)]
pub struct NoteHover;

/// A notice's style, as the game's `NotificationStyle`: a Sim talking (blue, the speech
/// connector), good news or bad (yellow or red), or the system's (yellow, the warning sign).
#[derive(Clone, Copy, PartialEq, Debug)]
enum NoteStyle {
    Talking,
    Positive,
    Negative,
    System,
}

/// What the game's notices of the kind are: its own warnings about the game, a Sim's words
/// (quoted, asked, wanted), bad news, else good.
fn note_style(msg: &str, about_sim: bool) -> NoteStyle {
    const SYSTEM: [&str; 5] = ["Game saved", "Couldn't save", "Couldn't make the Screenshots", "Screenshot", "That style couldn't"];
    const GOOD: [&str; 4] = ["caught the burglar", "fire is out", "put out the fire", "back to life"];
    const BAD: [&str; 52] = [
        "can't", "cannot", "couldn't", "isn't enough", "not enough", "doesn't have enough", "died", "has died", "is dead", "demoted", "fired", "on fire", "caught fire", "fire!",
        "burglar", "starving", "spoiled", "gone off", "social worker", "repo", "overdue", "unpaid", "failed", "got away", "sick", "is being used", "stole", "stolen", "smashed",
        "broke", "broken", "flooded", "leak", "exhausted", "passed out", "collapsed", "accident", "curfew", "rejected", "refused", "turned down", "angry", "furious",
        "lost", "nobody", "no one", "wasn't", "won't", "too tired", "too hungry", "banned", "arrested",
    ];
    let low = msg.to_lowercase();
    if SYSTEM.iter().any(|s| msg.starts_with(s)) {
        return NoteStyle::System;
    }
    if !GOOD.iter().any(|g| low.contains(g)) && BAD.iter().any(|b| low.contains(b)) {
        return NoteStyle::Negative;
    }
    if about_sim && (msg.contains('“') || msg.contains('"') || [" says", " asks", " would like", " wants to", " is calling", " called to"].iter().any(|w| low.contains(w))) {
        return NoteStyle::Talking;
    }
    NoteStyle::Positive
}

/// A notification's picture of the Sim it's about: a click takes the camera to them (as the
/// game's notices' faces do).
#[derive(Component)]
struct NoteFocus(Entity);

/// LESSON_ANSWER=yes|later|never (tests): once a lesson's note is in the panel, the panel
/// opened by its toggle tab and the Lessons tab chosen if need be, and the note answered with
/// that button a second after it's on show, each pressed as a click would.
fn lesson_answer_test(mut buttons: Query<(&NoteButton, &mut Interaction, &InheritedVisibility)>, log: Res<NoteLog>, time: Res<Time>, mut last: Local<f32>, mut done: Local<bool>) {
    let Some(want) = std::env::var("LESSON_ANSWER").ok().and_then(|v| match v.as_str() {
        "yes" => Some(LessonAnswer::Yes),
        "later" => Some(LessonAnswer::Later),
        "never" => Some(LessonAnswer::Never),
        _ => None,
    }) else {
        return;
    };
    let now = time.elapsed_secs();
    if *done || log.tabs[TAB_LESSONS].is_empty() || now - *last < 1.0 || log.show_at.is_some() || (log.slid > 0.0 && log.slid < 1.0) {
        return;
    }
    let press = if !log.open {
        NoteButton::Toggle
    } else if log.tab != TAB_LESSONS {
        NoteButton::Tab(TAB_LESSONS)
    } else {
        NoteButton::Lesson(want)
    };
    for (b, mut i, v) in &mut buttons {
        let hit = match (b, press) {
            (NoteButton::Toggle, NoteButton::Toggle) => true,
            (NoteButton::Tab(t), NoteButton::Tab(u)) => *t == u,
            (NoteButton::Lesson(a), NoteButton::Lesson(w)) => *a == w && v.get(),
            _ => false,
        };
        if hit {
            info!("lesson answer test: pressed {press:?} (on {:?})", log.current().and_then(|n| n.lesson.clone()));
            *i = Interaction::Pressed;
            *last = now;
            *done = matches!(press, NoteButton::Lesson(_));
            break;
        }
    }
}

fn note_focus(clicks: Query<(&Interaction, &NoteFocus), Changed<Interaction>>, shown: Query<&InheritedVisibility>, mut cams: Query<&mut crate::camera::SimsCamera>) {
    for (i, f) in &clicks {
        if *i == Interaction::Pressed
            && shown.get(f.0).is_ok_and(|v| v.get())
            && let Ok(mut cam) = cams.single_mut()
        {
            cam.follow(f.0);
        }
    }
}

/// The notifications (top right), in the game's own panel (`NotificationManager`): the newest
/// shown a moment after it comes, in its own card (a Sim's words with their face on the speech
/// background, the game's own news on the system's), under its tab (Information, or Chatty for
/// a Sim talking), with pages back through the rest; the panel slides shut after the notice's
/// time unless pointed at, and its toggle tab opens it again. Closed with their cross, or two
/// minutes after they're last seen.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn notifications(
    mut commands: Commands,
    ui: Option<ResMut<UiAssets>>,
    (mut assets, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (notes, mut log): (Res<crate::interact::Notifications>, ResMut<NoteLog>),
    people: Query<(Entity, &Sim, Has<HouseholdMember>)>,
    mut portraits: ResMut<crate::portraits::Portraits>,
    (clicks, hover): (Query<(&Interaction, &NoteButton), Changed<Interaction>>, Query<&Interaction, With<NoteHover>>),
    (vis, mut nodes): (Query<&mut Visibility>, Query<&mut Node>),
    mut state: Local<(Option<Entity>, Option<Entity>, String)>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    time: Res<Time>,
    mut icons: Option<ResMut<crate::icons::GameUi>>,
) {
    let Some(mut ui) = ui else { return };
    let now = time.elapsed_secs();
    // New notices, under their tabs: shown a moment after (`AddShowDelayTask`), or, with the
    // panel open on another, the tab glowing.
    if notes.1 < log.taken {
        *log = NoteLog::default();
    }
    let fresh = (notes.1 - log.taken) as usize;
    log.taken = notes.1;
    let start = notes.0.len().saturating_sub(fresh);
    for (msg, _) in &notes.0[start..] {
        // Like the game's, a notice about a Sim carries their picture: the household's Sim (or
        // else anyone about) the notice begins with.
        let starts = |s: &Sim| msg.starts_with(&format!("{} ", s.first)) || msg.starts_with(&format!("{}'", s.first)) || msg.starts_with(&s.full_name());
        let about = people.iter().filter(|(_, s, _)| !s.first.is_empty() && starts(s)).max_by_key(|(_, _, member)| *member).map(|(e, ..)| e);
        let style = note_style(msg, about.is_some());
        let tab = if style == NoteStyle::Talking { TAB_CHATTY } else { TAB_INFORMATION };
        log.tabs[tab].push(Logged { msg: msg.clone(), style, about, expires: Some(now + NOTES_KEPT), lesson: None });
        log.news(tab, now);
    }
    // The controls.
    for (i, b) in &clicks {
        if *i != Interaction::Pressed {
            continue;
        }
        match *b {
            NoteButton::Toggle => {
                log.open = !log.open && log.current().is_some();
                log.glow = [false; 3];
                log.close_at = log.current().filter(|_| log.open).map(|n| now + n.timeout());
            }
            NoteButton::Prev if log.page > 0 => log.page -= 1,
            NoteButton::Next if log.page + 1 < log.tabs[log.tab].len() => log.page += 1,
            NoteButton::Tab(t) if t != log.tab && !log.tabs[t].is_empty() => {
                log.tab = t;
                log.page = log.tabs[t].len() - 1;
                log.glow[t] = false;
            }
            NoteButton::Close => log.remove_current(),
            NoteButton::Lesson(a) => {
                if let Some(k) = log.current().and_then(|n| n.lesson.clone()) {
                    log.answered.push((k, a));
                    log.remove_current();
                }
            }
            _ => continue,
        }
        // (Another shown is heard, as the game's `Notification.Show`; a new one's heard as it
        // comes: `sound`.)
        if matches!(*b, NoteButton::Prev | NoteButton::Next | NoteButton::Tab(_)) {
            play.write(crate::sound::PlaySound::ui("ui_text_notification_open"));
        }
    }
    // The newest shown (`Show`).
    if log.show_at.is_some_and(|t| now >= t) {
        log.show_at = None;
        let tab = log.news_tab();
        if !log.tabs[tab].is_empty() {
            log.tab = tab;
            log.page = log.tabs[tab].len() - 1;
            log.glow[tab] = false;
            log.open = true;
            log.close_at = log.current().map(|n| now + n.timeout());
        }
    }
    // Pointed at, it stays open; left, its time starts again; up, it slides shut (`Close`).
    let hovering = hover.iter().any(|i| *i != Interaction::None);
    if hovering != log.hovering {
        log.hovering = hovering;
        log.close_at = if hovering { None } else { log.current().map(|n| now + n.timeout()) };
    }
    if log.open && log.close_at.is_some_and(|t| now >= t) {
        log.open = false;
        log.close_at = None;
        log.glow = [false; 3];
    }
    // Those not on show go after a while (`kIndividualTimeout`); the one on show stays.
    let (tab, page) = (log.tab, log.page);
    for (t, list) in log.tabs.iter_mut().enumerate() {
        for (k, n) in list.iter_mut().enumerate() {
            if (t, k) == (tab, page) {
                n.expires = None;
            } else if n.expires.is_none() {
                n.expires = Some(now + NOTES_KEPT);
            }
        }
    }
    if log.tabs.iter().flatten().any(|n| n.expires.is_some_and(|e| now >= e)) {
        let keep = log.current().map(|n| n.msg.clone());
        let unanswered: Vec<String> = log.tabs.iter().flatten().filter(|n| n.expires.is_some_and(|e| now >= e)).filter_map(|n| n.lesson.clone()).collect();
        log.answered.extend(unanswered.into_iter().map(|k| (k, LessonAnswer::Later)));
        for list in log.tabs.iter_mut() {
            list.retain(|n| n.expires.is_none_or(|e| now < e));
        }
        log.page = keep.and_then(|m| log.tabs[log.tab].iter().position(|n| n.msg == m)).unwrap_or_else(|| log.tabs[log.tab].len().saturating_sub(1));
    }
    if log.current().is_none() {
        log.open = false;
    }
    // Sliding open or shut (`Glide`, 0.3 s).
    let target = if log.open { 0.0 } else { 1.0 };
    let step = time.delta_secs() / NOTES_SLIDE_SECS;
    let slid = if log.slid < target { (log.slid + step).min(target) } else { (log.slid - step).max(target) };
    if slid != log.slid {
        log.slid = slid;
    }
    if let Some(s) = state.1
        && let Ok(mut n) = nodes.get_mut(s)
    {
        let want = Val::Px((log.slid * NOTES_SLIDE_BY).round());
        if n.left != want {
            n.left = want;
        }
    }
    // What's shown: rebuilt when it changes.
    let shown = log.current().cloned();
    let sig = format!("{} {} {:?} {:?} {:?} {}", log.tab, log.page, log.tabs.iter().map(|l| l.len()).collect::<Vec<_>>(), log.glow, shown.as_ref().map(|n| &n.msg), log.open);
    if state.2 == sig && state.0.is_some_and(|h| vis.contains(h)) {
        return;
    }
    state.2 = sig;
    if let Some(h) = state.0.take() {
        commands.entity(h).try_despawn();
    }
    state.1 = None;
    let Some(n) = shown else { return };
    let msg = n.msg.clone();
    let (about, style) = (n.about, n.style);
    let h = n.height();
    let Some(mut root) = ui.export("NotificationManager", 1).cloned() else { return };
    root.area = [0.0, 0.0, NOTES_SIZE[0], NOTES_SIZE[1]];
    root.place = s3bake::ui::UiPlace::Fixed;
    root.flags |= s3bake::ui::WIN_VISIBLE;
    let card = [NOTES_CARD[0], NOTES_CARD[1], NOTES_CARD[0] + NOTE_WIDTH, NOTES_CARD[1] + h];
    // (Of the sliding window's pieces, the frame with the tabs: the card's put in it below.)
    if let Some(slide) = find_window(&mut root, NOTES_SLIDE) {
        slide.children.retain_mut(|c| find_window(c, NOTES_STRIP).is_some());
        if let Some(frame) = slide.children.first_mut() {
            frame.area = card;
        }
    }
    let holder = commands
        .spawn((
            Node { position_type: PositionType::Absolute, right: Val::Px(0.0), top: Val::Px(NOTES_TOP), width: Val::Px(NOTES_SIZE[0]), height: Val::Px(NOTES_SIZE[1]), ..default() },
            Visibility::Inherited,
            Pickable::IGNORE,
            GlobalZIndex(6),
            DespawnOnExit(crate::AppState::InGame),
        ))
        .id();
    state.0 = Some(holder);
    let s = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &root, holder);
    let Some(slide) = s.id(NOTES_SLIDE) else { return };
    state.1 = Some(slide);
    commands.entity(slide).insert(Node { position_type: PositionType::Absolute, left: Val::Px((log.slid * NOTES_SLIDE_BY).round()), top: Val::Px(0.0), width: Val::Px(NOTES_SIZE[0]), height: Val::Px(NOTES_SIZE[1]), ..default() });
    if let Some(t) = s.id(NOTES_TOGGLE) {
        commands.entity(t).insert(NoteButton::Toggle);
    }
    // The tabs: the current one chosen, those with nothing greyed, the glow on those with news.
    for (k, id) in NOTES_TABS.into_iter().enumerate() {
        let Some(t) = s.id(id) else { continue };
        let (selected, disabled) = (k == log.tab, log.tabs[k].is_empty());
        commands.entity(t).insert((NoteButton::Tab(k), NoteHover)).queue(move |mut ew: EntityWorldMut| {
            if let Some(mut b) = ew.get_mut::<UiButton>() {
                b.selected = selected;
                b.disabled = disabled;
            }
        });
        if let Some(g) = s.within(t, NOTES_TAB_GLOW) {
            commands.entity(g).insert(if log.glow[k] { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    for id in NOTES_EP_TABS {
        if let Some(t) = s.id(id) {
            commands.entity(t).insert(Visibility::Hidden);
        }
    }
    // The pages, with more than one (`UpdatePageInfo`).
    let count = log.tabs[log.tab].len();
    for id in [NOTES_PREV, NOTES_PAGE, NOTES_NEXT] {
        if let Some(w) = s.id(id) {
            commands.entity(w).insert(if count > 1 { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    for (id, b, off) in [(NOTES_PREV, NoteButton::Prev, log.page == 0), (NOTES_NEXT, NoteButton::Next, log.page + 1 >= count)] {
        if let Some(w) = s.id(id) {
            commands.entity(w).insert(b).queue(move |mut ew: EntityWorldMut| {
                if let Some(mut b) = ew.get_mut::<UiButton>() {
                    b.disabled = off;
                }
            });
        }
    }
    if let Some(t) = s.text(NOTES_PAGE) {
        commands.entity(t).insert(Text::new((log.page + 1).to_string()));
    }
    if let Some(t) = s.text(NOTES_MAX) {
        commands.entity(t).insert(Text::new(count.to_string()));
    }
    if let Some(g) = s.id(NOTES_MAX_GLOW) {
        commands.entity(g).insert(if log.page + 1 < count && log.glow[log.tab] { Visibility::Inherited } else { Visibility::Hidden });
    }
    // A lesson's note, in its own (`TutorialetteNotification.Show`): its words sized to them
    // (taller, the note's taller; shorter, they're centred), its Yes, Later and Never.
    if n.lesson.is_some() {
        let Some(mut w) = ui.export("NotificationManager", NOTE_LESSON).cloned() else { return };
        w.area = card;
        w.flags |= s3bake::ui::WIN_VISIBLE;
        let tall = lesson_lines(&msg) as f32 * NOTE_LINE;
        crate::layout::edit_windows(&mut w, NOTE_LESSON_TEXT_ID, &mut |t| {
            t.caption = msg.clone();
            if tall < NOTE_LESSON_TEXT {
                let down = ((NOTE_LESSON_TEXT - tall) / 2.0).floor();
                t.area[1] += down;
                t.area[3] += down;
            } else {
                t.area[3] = t.area[1] + tall;
            }
        });
        let l = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &w, slide);
        for (id, a) in NOTE_LESSON_BUTTONS {
            if let Some(b) = l.id(id) {
                commands.entity(b).insert((NoteButton::Lesson(a), NoteHover));
            }
        }
        if let Some(r) = l.root {
            commands.entity(r).remove::<Pickable>().insert((crate::hud::BlocksWorld, NoteHover, Interaction::default()));
        }
        return;
    }
    // The card: in the game's style, a Sim talking on the speech connector, the rest on the
    // system's.
    let connector = if style == NoteStyle::Talking { NOTE_SPEECH } else { NOTE_SYSTEM };
    {
        let (Some(mut bg), Some(mut fg)) = (ui.export("NotificationManager", connector).cloned(), ui.export("NotificationManager", NOTE_ONE_THUMB).cloned()) else { return };
        bg.area = card;
        fg.area = card;
        bg.flags |= s3bake::ui::WIN_VISIBLE;
        fg.flags |= s3bake::ui::WIN_VISIBLE;
        let b = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &bg, slide);
        let f = ui.spawn_under(&mut commands, &mut assets, &mut fonts, &fg, slide);
        let msg = &msg;
        if let Some(t) = f.text(NOTE_TEXT) {
            commands.entity(t).insert(Text::new(msg.clone()));
        }
        let colour = match style {
            NoteStyle::Talking => NOTE_BLUE,
            NoteStyle::Negative => NOTE_RED,
            NoteStyle::Positive | NoteStyle::System => NOTE_YELLOW,
        };
        if let Some(c) = b.id(colour) {
            commands.entity(c).insert(Visibility::Inherited);
        }
        if let Some(c) = b.id(NOTE_CLOSE) {
            commands.entity(c).insert(NoteButton::Close);
        }
        match (about, b.id(NOTE_THUMB)) {
            (Some(e), Some(t)) => {
                let h = portraits.portrait(&mut assets, e);
                commands.entity(t).insert((NoteFocus(e), Interaction::default(), crate::icons::Tooltip("Go to Sim".into())));
                commands.entity(t).despawn_children();
                commands.entity(t).with_children(|c| {
                    c.spawn((ImageNode::new(h), crate::portraits::PortraitOf(e), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                });
            }
            (None, Some(t)) => {
                // (The game's own news: no face, nor its frame; its light bulb, or its warning
                // sign for the system's.)
                for w in [Some(t), b.id(NOTE_THUMB_FRAME)].into_iter().flatten() {
                    commands.entity(w).insert(Visibility::Hidden);
                }
                let icon = if style == NoteStyle::System { "tns_icon_warning" } else { "tns_icon_bulb" };
                match (b.id(NOTE_NO_MASK), icons.as_mut().and_then(|i| i.icon(&mut assets, icon))) {
                    (Some(w), Some(h)) => {
                        commands.entity(w).despawn_children();
                        commands.entity(w).with_children(|c| {
                            c.spawn((ImageNode::new(h), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                        });
                    }
                    (Some(w), None) => {
                        commands.entity(w).insert(Visibility::Hidden);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        for r in [b.root, f.root].into_iter().flatten() {
            commands.entity(r).remove::<Pickable>().insert((crate::hud::BlocksWorld, NoteHover, Interaction::default()));
        }
    }
}

/// A wish's tooltip, as the game's `WishTooltip`: its name, how far along it is, what it's about,
/// and its points.
fn wish_tip(x: &crate::wishes::Wish, traits: &[crate::life::Trait]) -> String {
    let points = x.reward_points(traits);
    match x.dream() {
        Some(d) => {
            let mut out = d.text.clone();
            if !d.progress.is_empty() {
                out += &format!(" ({})", d.progress);
            }
            if !d.description.trim().is_empty() {
                out += &format!("\n{}", d.description.trim());
            }
            out + &format!("\n+{points}")
        }
        None => format!("{} (+{points})", x.text()),
    }
}

#[cfg(test)]
mod note_tests {
    use super::*;

    #[test]
    fn notices_take_the_games_styles() {
        assert_eq!(note_style("Game saved: the Goth household in Sunset Valley.", false), NoteStyle::System);
        assert_eq!(note_style("Darcy reached level 3 in Cooking!", true), NoteStyle::Positive);
        assert_eq!(note_style("Darcy is starving!", true), NoteStyle::Negative);
        assert_eq!(note_style("Gunther set the stove on fire!", true), NoteStyle::Negative);
        assert_eq!(note_style("The fire is out!", false), NoteStyle::Positive);
        assert_eq!(note_style("The police caught the burglar! Nothing was taken.", false), NoteStyle::Positive);
        assert_eq!(note_style("Bella would like to come over.", true), NoteStyle::Talking);
        assert_eq!(note_style("You can't afford that.", false), NoteStyle::Negative);
    }
}
