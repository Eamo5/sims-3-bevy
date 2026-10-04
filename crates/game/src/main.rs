mod autotest;
mod camera;
mod data;
mod loading;
mod menu;
mod terrain;

use bevy::prelude::*;
use bevy::window::WindowResolution;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    Loading,
    InGame,
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
        .insert_resource(ClearColor(Color::srgb(0.53, 0.70, 0.90)))
        .add_plugins((
            data::DataPlugin,
            menu::MenuPlugin,
            loading::LoadingPlugin,
            terrain::TerrainPlugin,
            camera::CameraPlugin,
            autotest::AutoTestPlugin,
        ))
        .run();
}
