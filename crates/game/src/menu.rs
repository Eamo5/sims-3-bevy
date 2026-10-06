//! Main menu: pick a world to play.

use bevy::prelude::*;

use crate::AppState;
use crate::data::{InstallPath, SelectedWorld, WorldList};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_menu)
            .add_systems(Update, (button_visuals, menu_actions).run_if(in_state(AppState::MainMenu)));
    }
}

#[derive(Component)]
pub enum MenuAction {
    PlayWorld(usize),
    LoadSave(usize),
    Options,
    Quit,
}

/// The game's interface blue (its HUD panels' (110, 146, 216), a shade darker to keep light
/// writing legible), their navy outline, and the notices' darker blue.
pub const PANEL_BG: Color = Color::srgba(0.26, 0.40, 0.71, 0.95);
pub const PANEL_BORDER: Color = Color::srgb(0.04, 0.09, 0.27);
pub const NOTICE_BG: Color = Color::srgba(0.10, 0.20, 0.42, 0.94);

pub const BTN_NORMAL: Color = Color::srgba(0.10, 0.21, 0.46, 0.95);
pub const BTN_HOVER: Color = Color::srgba(0.20, 0.42, 0.80, 0.97);
pub const BTN_PRESS: Color = Color::srgba(0.30, 0.65, 0.20, 1.0);
pub const PLUMBOB_GREEN: Color = Color::srgb(0.35, 0.85, 0.25);

pub fn button_visuals(
    mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Button>)>,
) {
    for (i, mut bg) in &mut q {
        bg.0 = match i {
            Interaction::Pressed => BTN_PRESS,
            Interaction::Hovered => BTN_HOVER,
            Interaction::None => BTN_NORMAL,
        };
    }
}

pub fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont::from_font_size(size), TextColor(color))
}

fn spawn_menu(mut commands: Commands, worlds: Res<WorldList>, install: Res<InstallPath>) {
    commands.spawn((Camera2d, DespawnOnExit(AppState::MainMenu)));

    let root = commands
        .spawn((
            DespawnOnExit(AppState::MainMenu),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(14.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.16, 0.30)),
        ))
        .id();

    commands.entity(root).with_children(|p| {
        p.spawn(text("The Sims 3", 72.0, Color::WHITE));
        p.spawn(text("Bevy Edition", 28.0, PLUMBOB_GREEN));
        p.spawn((
            text(format!("Game data: {}", install.0.display()), 14.0, Color::srgb(0.7, 0.8, 0.9)),
            Node { margin: UiRect::bottom(Val::Px(24.0)), ..default() },
        ));
        p.spawn(text("Choose a town to play", 26.0, Color::WHITE));
        if worlds.0.is_empty() {
            p.spawn(text(
                "No worlds found. Start with --data <path to The Sims 3>.",
                20.0,
                Color::srgb(1.0, 0.5, 0.5),
            ));
        }
        for (i, w) in worlds.0.iter().enumerate() {
            spawn_button(p, &w.name, MenuAction::PlayWorld(i), 360.0);
        }
        let saves = crate::save::list_saves();
        if !saves.is_empty() {
            p.spawn((text("Continue a saved game", 26.0, Color::WHITE), Node { margin: UiRect::top(Val::Px(18.0)), ..default() }));
            for (i, (_, g)) in saves.iter().enumerate().take(5) {
                let label = format!("The {} household — {} (day {})", g.household, g.world, (g.minutes / 1440.0) as u32 + 1);
                spawn_button(p, &label, MenuAction::LoadSave(i), 560.0);
            }
        }
        p.spawn(Node { height: Val::Px(16.0), ..default() });
        spawn_button(p, "Options", MenuAction::Options, 200.0);
        spawn_button(p, "Quit", MenuAction::Quit, 200.0);
    });
}

fn spawn_button(p: &mut ChildSpawnerCommands, label: &str, action: MenuAction, width: f32) {
    p.spawn((
        Button,
        action,
        Node {
            width: Val::Px(width),
            height: Val::Px(44.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(BTN_NORMAL),
    ))
    .with_children(|b| {
        b.spawn(text(label, 22.0, Color::WHITE));
    });
}

fn menu_actions(
    q: Query<(&Interaction, &MenuAction), Changed<Interaction>>,
    worlds: Res<WorldList>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    (mut panel, settings): (ResMut<crate::options::OptionsPanel>, Res<crate::options::Settings>),
) {
    for (i, action) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        match action {
            MenuAction::PlayWorld(idx) => {
                commands.insert_resource(SelectedWorld(worlds.0[*idx].clone()));
                next.set(AppState::CreateHousehold);
            }
            MenuAction::LoadSave(k) => {
                if let Some((p, g)) = crate::save::list_saves().into_iter().nth(*k)
                    && crate::save::begin_load(&mut commands, &worlds, g, Some(p))
                {
                    next.set(AppState::Loading);
                }
            }
            MenuAction::Options => crate::options::open_options(&mut commands, &mut panel, &settings),
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}
