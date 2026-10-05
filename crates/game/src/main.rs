mod aging;
mod anim;
mod autotest;
mod baked;
mod balloons;
mod build;
mod burglar;
mod building;
mod buy;
mod camera;
mod careers;
mod cas;
mod clock;
mod collecting;
mod data;
mod death;
mod home;
mod icons;
mod gardening;
mod fire;
mod ghosts;
mod hud;
mod interact;
mod life;
mod little;
mod loading;
mod mail;
mod maptags;
mod meals;
mod menu;
mod music;
mod nav;
mod objects;
mod opportunities;
mod options;
mod portraits;
mod props;
mod premade;
mod rabbitholes;
mod relations;
mod roads;
mod save;
mod sim;
mod simpanel;
mod sound;
mod simbody;
mod sky;
mod social;
mod terrain;
mod town;
mod traffic;
mod visit;
mod wishes;
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
    if let Some(g) = s3bake::load_gamedata(&s3bake::default_root()) {
        careers::install_tracks(&g);
    }
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
            anim::AnimPlugin,
            simbody::SimBodyPlugin,
            roads::RoadPlugin,
            building::BuildingPlugin,
            music::MusicPlugin,
            town::TownPlugin,
        ))
        .add_plugins((life::LifePlugin, careers::CareersPlugin, save::SavePlugin, wishes::WishesPlugin, rabbitholes::RabbitHolePlugin, cas::CasPlugin, sound::SoundPlugin, premade::PremadePlugin, aging::AgingPlugin, little::LittlePlugin))
        .add_plugins((options::OptionsPlugin, icons::IconsPlugin, balloons::BalloonsPlugin, portraits::PortraitsPlugin, relations::RelationsPlugin, props::PropsPlugin, meals::MealsPlugin, death::DeathPlugin, mail::MailPlugin, sky::SkyPlugin, maptags::MapTagsPlugin, opportunities::OpportunitiesPlugin, gardening::GardeningPlugin, build::BuildPlugin, visit::VisitPlugin))
        .add_plugins((collecting::CollectingPlugin, traffic::TrafficPlugin, fire::FirePlugin, burglar::BurglarPlugin, simpanel::SimPanelPlugin, ghosts::GhostsPlugin))
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
