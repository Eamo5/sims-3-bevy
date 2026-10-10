mod aging;
mod anim;
mod appliances;
mod autotest;
mod baked;
mod balloons;
mod build;
mod buildhud;
mod buildcatalog;
mod covering;
mod burglar;
mod building;
mod buy;
mod buyhistory;
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
mod water;
mod services;
mod fireplace;
mod fishbowl;
mod effects;
mod swim;
mod dates;
mod hud;
mod hudpanels;
mod interact;
mod inventory;
mod life;
mod little;
mod loading;
mod layout;
mod livehud;
mod lookat;
mod mail;
mod maptags;
mod meals;
mod menu;
mod music;
mod nav;
mod objects;
mod opportunities;
mod options;
mod piemenu;
mod buyhud;
mod infopanels;
mod mainmenu;
mod loadscreen;
mod savepics;
mod gamepopup;
mod optionsdialog;
mod gamedialogs;
mod caslook;
mod popupmenu;
mod portraits;
mod props;
mod midlife;
mod household;
mod journal;
mod jog;
mod doorbell;
mod edittown;
mod style;
mod objanim;
mod planner;
mod premade;
mod rabbitholes;
mod relations;
mod roads;
mod save;
mod sim;
mod simpanel;
mod sound;
mod surroundings;
mod simbody;
mod sky;
mod social;
mod terrain;
mod thumbs;
mod town;
mod traffic;
mod visit;
mod wishes;
mod dialog;
mod lifetime;
mod writing;
mod story;
mod chess;
mod blink;
mod world;
mod upgrades;
mod family;
mod terrain_paint;
mod paintings;
mod weather;
mod weather_fx;
mod seasonal;
mod pets;
mod supernatural;

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
        .add_plugins((collecting::CollectingPlugin, traffic::TrafficPlugin, fire::FirePlugin, burglar::BurglarPlugin, simpanel::SimPanelPlugin, ghosts::GhostsPlugin, water::WaterPlugin, services::ServicesPlugin, fireplace::FireplacePlugin, effects::EffectsPlugin, swim::SwimPlugin, dates::DatesPlugin, dialog::DialogPlugin, lifetime::LifetimePlugin))
        .add_plugins((writing::WritingPlugin, story::StoryPlugin, chess::ChessPlugin, blink::BlinkPlugin, inventory::InventoryPlugin, thumbs::ThumbsPlugin, appliances::AppliancesPlugin, surroundings::SurroundingsPlugin, planner::PlannerPlugin, fishbowl::FishBowlPlugin, lookat::LookAtPlugin, upgrades::UpgradesPlugin, family::FamilyPlugin, terrain_paint::TerrainPaintPlugin, paintings::PaintingsPlugin))
        .add_plugins((midlife::MidLifePlugin, objanim::ObjectAnimPlugin, household::HouseholdPlugin, journal::JournalPlugin, jog::JogPlugin, doorbell::DoorbellPlugin, edittown::EditTownPlugin, style::StylePlugin, weather::WeatherPlugin, seasonal::SeasonalPlugin, pets::PetsPlugin, supernatural::SupernaturalPlugin))
        .add_plugins((layout::LayoutPlugin, livehud::LiveHudPlugin, hudpanels::HudPanelsPlugin, piemenu::PieMenuPlugin, buyhud::BuyHudPlugin, infopanels::InfoPanelsPlugin, buildhud::BuildHudPlugin, popupmenu::PopupMenuPlugin, mainmenu::MainMenuPlugin, loadscreen::LoadScreenPlugin, savepics::SavePicsPlugin, gamepopup::GamePopupPlugin, optionsdialog::OptionsDialogPlugin, gamedialogs::GameDialogsPlugin, caslook::CasLookPlugin))
        .add_systems(OnEnter(PlayMode::Live), home::move_in)
        .add_systems(Startup, load_ui_font)
        .run();
}

/// Uses a system UI font with full Unicode punctuation (§, —, ·) when available.
pub(crate) fn load_ui_font(mut fonts: ResMut<Assets<Font>>) {
    for path in ["C:/Windows/Fonts/segoeui.ttf", "C:/Windows/Fonts/arial.ttf"] {
        if let Ok(bytes) = std::fs::read(path) {
            let _ = fonts.insert(&Handle::<Font>::default(), Font::from_bytes(bytes));
            break;
        }
    }
}
