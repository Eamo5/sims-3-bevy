mod autotest;
mod buy;
mod camera;
mod clock;
mod data;
mod home;
mod hud;
mod interact;
mod loading;
mod menu;
mod nav;
mod objects;
mod sim;
mod terrain;
mod world;

use bevy::prelude::*;
use bevy::window::WindowResolution;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    CreateHousehold,
    Loading,
    InGame,
}

/// What the player is doing while in a world.
#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(AppState = AppState::InGame)]
pub enum PlayMode {
    #[default]
    ChooseLot,
    Live,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "The Sims 3 — Bevy Edition".into(),
                resolution: WindowResolution::new(1600, 900),
                ..default()
            }),
            ..default()
        }))
        .init_state::<AppState>()
        .add_sub_state::<PlayMode>()
        .init_resource::<objects::ObjectAssets>()
        .insert_resource(ClearColor(Color::srgb(0.53, 0.70, 0.90)))
        .add_plugins((
            data::DataPlugin,
            menu::MenuPlugin,
            loading::LoadingPlugin,
            terrain::TerrainPlugin,
            camera::CameraPlugin,
            autotest::AutoTestPlugin,
        ))
        .add_plugins((
            clock::ClockPlugin,
            sim::SimPlugin,
            nav::NavPlugin,
            interact::InteractPlugin,
            hud::HudPlugin,
            home::HomePlugin,
            buy::BuyPlugin,
            world::WorldPlugin,
        ))
        .add_systems(OnEnter(PlayMode::Live), home::move_in)
        .add_systems(Startup, load_ui_font)
        .run();
}

/// Uses a system UI font with full Unicode punctuation (§, —, ·) when available.
fn load_ui_font(mut fonts: ResMut<Assets<Font>>) {
    for path in ["C:/Windows/Fonts/segoeui.ttf", "C:/Windows/Fonts/arial.ttf"] {
        if let Ok(bytes) = std::fs::read(path) {
            let _ = fonts.insert(&Handle::<Font>::default(), Font::from_bytes(bytes));
            break;
        }
    }
}
