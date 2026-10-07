//! Game options, as in the game's Options panel: sound levels, game rules (aging and life
//! span, free will) and display. Changed from the main menu or the in-game menu (Escape),
//! applied at once and kept in `settings.json` between sessions.

use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::menu::{BTN_HOVER, BTN_NORMAL, BTN_PRESS, PLUMBOB_GREEN, text};

pub struct OptionsPlugin;

impl Plugin for OptionsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::load())
            .init_resource::<OptionsPanel>()
            .init_resource::<GameMenu>()
            .add_systems(
                Update,
                (
                    option_buttons,
                    refresh_panel,
                    game_menu_buttons,
                    apply_display,
                    option_button_visuals,
                ),
            )
            // Before the HUD and buy mode handle Escape (closing pie menus, leaving buy mode).
            .add_systems(PreUpdate, game_menu_key.after(bevy::input::InputSystems).run_if(in_state(AppState::InGame)))
            .add_systems(OnExit(AppState::InGame), close_game_menu);
    }
}

/// How long Sims live (the game's life-span setting).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lifespan {
    Short,
    #[default]
    Normal,
    Long,
    Epic,
}

impl Lifespan {
    const ALL: [Lifespan; 4] = [Lifespan::Short, Lifespan::Normal, Lifespan::Long, Lifespan::Epic];
    fn name(self) -> &'static str {
        match self {
            Lifespan::Short => "Short (25 days)",
            Lifespan::Normal => "Normal (90 days)",
            Lifespan::Long => "Long (190 days)",
            Lifespan::Epic => "Epic (960 days)",
        }
    }
    /// How much longer than normal each life stage lasts.
    pub fn factor(self) -> f32 {
        match self {
            Lifespan::Short => 25.0 / 90.0,
            Lifespan::Normal => 1.0,
            Lifespan::Long => 190.0 / 90.0,
            Lifespan::Epic => 960.0 / 90.0,
        }
    }
}

/// How much the household's Sims do on their own.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FreeWill {
    Off,
    #[default]
    Normal,
    High,
}

impl FreeWill {
    const ALL: [FreeWill; 3] = [FreeWill::Off, FreeWill::Normal, FreeWill::High];
    fn name(self) -> &'static str {
        match self {
            FreeWill::Off => "Off",
            FreeWill::Normal => "Normal",
            FreeWill::High => "High",
        }
    }
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub master: f32,
    pub music: f32,
    pub effects: f32,
    pub voices: f32,
    pub ambience: f32,
    pub aging: bool,
    pub lifespan: Lifespan,
    pub free_will: FreeWill,
    pub shadows: bool,
    pub show_fps: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            master: 1.0,
            music: 1.0,
            effects: 1.0,
            voices: 1.0,
            ambience: 1.0,
            aging: std::env::var_os("SIMS3_NO_AGING").is_none(),
            lifespan: Lifespan::Normal,
            free_will: FreeWill::Normal,
            shadows: true,
            show_fps: true,
        }
    }
}

/// Which mixer channel a sound plays on, judged by its name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Channel {
    Effects,
    Voices,
    Music,
    Ambience,
}

impl Channel {
    pub fn of(name: &str) -> Self {
        if name.starts_with("vo_") {
            Channel::Voices
        } else if name.starts_with("amb_") {
            Channel::Ambience
        } else if name.starts_with("music_") || name.starts_with("sting_") {
            Channel::Music
        } else {
            Channel::Effects
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        std::env::var_os("SIMS3_SETTINGS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("settings.json"))
    }

    pub fn load() -> Self {
        std::fs::read(Self::path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
    }

    fn save(&self) {
        if let Ok(d) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(Self::path(), d);
        }
    }

    /// Loudness of a channel (with the master level).
    pub fn gain(&self, ch: Channel) -> f32 {
        self.master
            * match ch {
                Channel::Effects => self.effects,
                Channel::Voices => self.voices,
                Channel::Music => self.music,
                Channel::Ambience => self.ambience,
            }
    }
}

/// One row of the options panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opt {
    Master,
    Music,
    Effects,
    Voices,
    Ambience,
    Aging,
    Lifespan,
    FreeWill,
    Shadows,
    ShowFps,
}

impl Opt {
    const ALL: [Opt; 10] = [
        Opt::Master,
        Opt::Music,
        Opt::Effects,
        Opt::Voices,
        Opt::Ambience,
        Opt::Aging,
        Opt::Lifespan,
        Opt::FreeWill,
        Opt::Shadows,
        Opt::ShowFps,
    ];

    fn label(self) -> &'static str {
        match self {
            Opt::Master => "Master volume",
            Opt::Music => "Music",
            Opt::Effects => "Sound effects",
            Opt::Voices => "Voices",
            Opt::Ambience => "Ambient sounds",
            Opt::Aging => "Aging",
            Opt::Lifespan => "Life span",
            Opt::FreeWill => "Free will",
            Opt::Shadows => "Shadows",
            Opt::ShowFps => "Show frame rate",
        }
    }

    fn section(self) -> Option<&'static str> {
        match self {
            Opt::Master => Some("Sound"),
            Opt::Aging => Some("Game"),
            Opt::Shadows => Some("Display"),
            _ => None,
        }
    }

    fn level(self, s: &mut Settings) -> Option<&mut f32> {
        match self {
            Opt::Master => Some(&mut s.master),
            Opt::Music => Some(&mut s.music),
            Opt::Effects => Some(&mut s.effects),
            Opt::Voices => Some(&mut s.voices),
            Opt::Ambience => Some(&mut s.ambience),
            _ => None,
        }
    }

    fn value(self, s: &Settings) -> String {
        let pct = |v: f32| format!("{:.0}%", v * 100.0);
        let on = |b: bool| if b { "On" } else { "Off" }.to_string();
        match self {
            Opt::Master => pct(s.master),
            Opt::Music => pct(s.music),
            Opt::Effects => pct(s.effects),
            Opt::Voices => pct(s.voices),
            Opt::Ambience => pct(s.ambience),
            Opt::Aging => on(s.aging),
            Opt::Lifespan => s.lifespan.name().into(),
            Opt::FreeWill => s.free_will.name().into(),
            Opt::Shadows => on(s.shadows),
            Opt::ShowFps => on(s.show_fps),
        }
    }

    /// Steps the option down (-1) or up (+1).
    fn step(self, s: &mut Settings, dir: i32) {
        if let Some(v) = self.level(s) {
            *v = ((*v * 10.0).round() + dir as f32).clamp(0.0, 10.0) / 10.0;
            return;
        }
        let cycle = |i: usize, n: usize| ((i as i32 + dir).rem_euclid(n as i32)) as usize;
        match self {
            Opt::Aging => s.aging = !s.aging,
            Opt::Shadows => s.shadows = !s.shadows,
            Opt::ShowFps => s.show_fps = !s.show_fps,
            Opt::Lifespan => {
                let i = Lifespan::ALL.iter().position(|l| *l == s.lifespan).unwrap_or(1);
                s.lifespan = Lifespan::ALL[cycle(i, Lifespan::ALL.len())];
            }
            Opt::FreeWill => {
                let i = FreeWill::ALL.iter().position(|l| *l == s.free_will).unwrap_or(1);
                s.free_will = FreeWill::ALL[cycle(i, FreeWill::ALL.len())];
            }
            _ => {}
        }
    }
}

/// The open options panel.
#[derive(Resource, Default)]
pub struct OptionsPanel {
    root: Option<Entity>,
}

#[derive(Component, Clone, Copy)]
enum OptionButton {
    Step(Opt, i32),
    Close,
}

#[derive(Component)]
struct OptionValue(Opt);

/// Buttons of this module (the menus' own hover colours don't run everywhere).
#[derive(Component)]
struct OptionsUi;

fn option_button_visuals(mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<OptionsUi>, With<Button>)>) {
    for (i, mut bg) in &mut q {
        bg.0 = match i {
            Interaction::Pressed => BTN_PRESS,
            Interaction::Hovered => BTN_HOVER,
            Interaction::None => BTN_NORMAL,
        };
    }
}

fn small_button(p: &mut ChildSpawnerCommands, label: &str, action: OptionButton, width: f32) {
    p.spawn((
        Button,
        action,
        OptionsUi,
        Node {
            width: Val::Px(width),
            height: Val::Px(32.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(BTN_NORMAL),
    ))
    .with_children(|b| {
        b.spawn(text(label, 18.0, Color::WHITE));
    });
}

/// Opens the options panel over whatever is on screen.
pub fn open_options(commands: &mut Commands, panel: &mut OptionsPanel, settings: &Settings) {
    if panel.root.is_some() {
        return;
    }
    let root = commands
        .spawn((
            OptionsUi,
            crate::hud::BlocksWorld,
            Interaction::default(),
            GlobalZIndex(50),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(24.0)),
                    row_gap: Val::Px(8.0),
                    min_width: Val::Px(560.0),
                    border_radius: BorderRadius::all(Val::Px(14.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.05, 0.16, 0.30)),
            ))
            .with_children(|p| {
                p.spawn(text("Options", 34.0, Color::WHITE));
                for opt in Opt::ALL {
                    if let Some(sec) = opt.section() {
                        p.spawn((text(sec, 22.0, PLUMBOB_GREEN), Node { margin: UiRect::top(Val::Px(10.0)), ..default() }));
                    }
                    p.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(10.0), ..default() })
                        .with_children(|row| {
                            // (On one line, level with its buttons.)
                            row.spawn((text(opt.label(), 19.0, Color::WHITE), TextLayout::default().with_no_wrap(), Node { width: Val::Px(200.0), flex_shrink: 0.0, ..default() }));
                            small_button(row, "<", OptionButton::Step(opt, -1), 40.0);
                            row.spawn(Node { width: Val::Px(200.0), flex_shrink: 0.0, justify_content: JustifyContent::Center, ..default() }).with_children(|cell| {
                                cell.spawn((text(opt.value(settings), 19.0, Color::srgb(1.0, 0.95, 0.7)), OptionValue(opt), TextLayout::default().with_no_wrap()));
                            });
                            small_button(row, ">", OptionButton::Step(opt, 1), 40.0);
                        });
                }
                p.spawn(Node { height: Val::Px(10.0), ..default() });
                p.spawn(Node { justify_content: JustifyContent::Center, ..default() }).with_children(|row| {
                    small_button(row, "Done", OptionButton::Close, 160.0);
                });
            });
        })
        .id();
    panel.root = Some(root);
}

pub fn options_open(panel: &OptionsPanel) -> bool {
    panel.root.is_some()
}

/// Closes the options panel, keeping the settings.
pub fn close_options(commands: &mut Commands, panel: &mut OptionsPanel, settings: &Settings) {
    if let Some(e) = panel.root.take() {
        commands.entity(e).despawn();
    }
    settings.save();
}

fn option_buttons(
    mut commands: Commands,
    q: Query<(&Interaction, &OptionButton), Changed<Interaction>>,
    mut settings: ResMut<Settings>,
    mut panel: ResMut<OptionsPanel>,
) {
    for (i, b) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        match *b {
            OptionButton::Step(opt, dir) => opt.step(&mut settings, dir),
            OptionButton::Close => {
                if let Some(e) = panel.root.take() {
                    commands.entity(e).despawn();
                }
                settings.save();
            }
        }
    }
}

fn refresh_panel(settings: Res<Settings>, mut values: Query<(&OptionValue, &mut Text)>) {
    if !settings.is_changed() {
        return;
    }
    for (v, mut t) in &mut values {
        let s = v.0.value(&settings);
        if t.0 != s {
            t.0 = s;
        }
    }
}

/// Shadows and the frame-rate counter follow the settings.
fn apply_display(
    settings: Res<Settings>,
    mut suns: Query<&mut DirectionalLight>,
    mut fps: Query<&mut Visibility, With<crate::hud::FpsText>>,
) {
    for mut sun in &mut suns {
        if sun.shadow_maps_enabled != settings.shadows {
            sun.shadow_maps_enabled = settings.shadows;
        }
    }
    let shown = if settings.show_fps { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut fps {
        if *v != shown {
            *v = shown;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The in-game menu (Escape): resume, options, save, back to the main menu, quit.

#[derive(Resource, Default)]
pub struct GameMenu {
    root: Option<Entity>,
    /// Game speed to restore when the menu closes.
    speed: usize,
}

#[derive(Component, Clone, Copy)]
enum GameMenuButton {
    Resume,
    Options,
    Save,
    SaveAs,
    Household,
    MainMenu,
    Quit,
}

fn game_menu_key(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<GameMenu>,
    panel: Res<OptionsPanel>,
    pie: Option<Res<crate::hud::PieMenu>>,
    buy: Option<Res<crate::buy::BuyMode>>,
    mut clock: Option<ResMut<crate::clock::GameClock>>,
) {
    if !keys.just_pressed(KeyCode::Escape) || options_open(&panel) {
        return;
    }
    // Escape first closes pie menus and buy mode.
    if pie.is_some_and(|p| p.root.is_some()) || buy.is_some_and(|b| b.active) {
        return;
    }
    toggle_game_menu(&mut commands, &mut menu, clock.as_deref_mut());
}

/// Opens the in-game menu (pausing the game), or closes it (resuming).
pub fn toggle_game_menu(commands: &mut Commands, menu: &mut GameMenu, mut clock: Option<&mut crate::clock::GameClock>) {
    debug!("game menu: {}", if menu.root.is_some() { "close" } else { "open" });
    if let Some(root) = menu.root.take() {
        commands.entity(root).despawn();
        if let Some(c) = clock.as_deref_mut() {
            c.speed = menu.speed;
        }
        return;
    }
    if let Some(c) = clock.as_deref_mut() {
        menu.speed = c.speed;
        c.speed = 0;
    }
    let root = commands
        .spawn((
            OptionsUi,
            crate::hud::BlocksWorld,
            Interaction::default(),
            GlobalZIndex(40),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(24.0)),
                    row_gap: Val::Px(10.0),
                    border_radius: BorderRadius::all(Val::Px(14.0)),
                    ..default()
                },
                BackgroundColor(crate::menu::PANEL_BG),
            ))
            .with_children(|p| {
                p.spawn((text("Paused", 30.0, Color::WHITE), Node { margin: UiRect::bottom(Val::Px(6.0)), ..default() }));
                for (label, b) in [
                    ("Resume", GameMenuButton::Resume),
                    ("Options", GameMenuButton::Options),
                    ("Save Game", GameMenuButton::Save),
                    ("Save as New Game", GameMenuButton::SaveAs),
                    ("Change Household", GameMenuButton::Household),
                    ("Main Menu", GameMenuButton::MainMenu),
                    ("Quit Game", GameMenuButton::Quit),
                ] {
                    p.spawn((
                        Button,
                        b,
                        OptionsUi,
                        Node {
                            width: Val::Px(260.0),
                            height: Val::Px(42.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(Val::Px(10.0)),
                            ..default()
                        },
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|b| {
                        b.spawn(text(label, 21.0, Color::WHITE));
                    });
                }
            });
        })
        .id();
    menu.root = Some(root);
}

#[allow(clippy::too_many_arguments)]
fn game_menu_buttons(
    mut commands: Commands,
    q: Query<(&Interaction, &GameMenuButton), Changed<Interaction>>,
    mut menu: ResMut<GameMenu>,
    mut panel: ResMut<OptionsPanel>,
    settings: Res<Settings>,
    mut clock: Option<ResMut<crate::clock::GameClock>>,
    (mut save, mut household, mut save_as): (MessageWriter<crate::save::SaveRequest>, MessageWriter<crate::household::ChooseHousehold>, MessageWriter<crate::save::SaveAsRequest>),
    (mut next, mut exit): (ResMut<NextState<AppState>>, MessageWriter<AppExit>),
) {
    for (i, b) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        let mut close = |commands: &mut Commands, menu: &mut GameMenu| {
            if let Some(root) = menu.root.take() {
                commands.entity(root).despawn();
            }
            if let Some(c) = clock.as_deref_mut() {
                c.speed = menu.speed;
            }
        };
        match b {
            GameMenuButton::Resume => close(&mut commands, &mut menu),
            GameMenuButton::Options => open_options(&mut commands, &mut panel, &settings),
            GameMenuButton::Save => {
                crate::save::request_save(&mut save);
                close(&mut commands, &mut menu);
            }
            // (Into a file of its own, which no other save has: the game goes on saving there.)
            GameMenuButton::SaveAs => {
                save_as.write(crate::save::SaveAsRequest);
                close(&mut commands, &mut menu);
            }
            GameMenuButton::Household => {
                close(&mut commands, &mut menu);
                household.write(crate::household::ChooseHousehold);
            }
            GameMenuButton::MainMenu => {
                close(&mut commands, &mut menu);
                next.set(AppState::MainMenu);
            }
            GameMenuButton::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}

fn close_game_menu(mut commands: Commands, mut menu: ResMut<GameMenu>, mut panel: ResMut<OptionsPanel>) {
    if let Some(root) = menu.root.take() {
        commands.entity(root).despawn();
    }
    if let Some(root) = panel.root.take() {
        commands.entity(root).despawn();
    }
}
