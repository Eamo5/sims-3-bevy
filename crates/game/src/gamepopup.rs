//! The puck's options button in the game's own look: its popup menu (`PuckCommon.OptionsMenu`,
//! in live mode `PuckController`'s list): Save, Save As (once the game has a save of its own),
//! Edit Town, Options, Quit to Main Menu, Save and Quit, Quit. The game is paused while it's
//! open, as the game's `PopupMenu` pauses it.

use bevy::prelude::*;

use crate::layout::UiAssets;
use crate::popupmenu::PopupChoice;

pub struct GamePopupPlugin;

impl Plugin for GamePopupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, enable).add_systems(Update, (open_menu, choose).chain().run_if(in_state(crate::PlayMode::Live)));
    }
}

/// Asked for by the puck's options button.
#[derive(Resource)]
pub struct OpenGamePopup;

/// Whether the game's own popup can be shown (else the plain menu: see `options`).
pub static GAME_POPUP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn enable(ui: Option<Res<UiAssets>>, mut done: Local<bool>) {
    if !*done && let Some(u) = ui {
        GAME_POPUP.store(u.layout("PopupMenu").is_some(), std::sync::atomic::Ordering::Relaxed);
        *done = true;
    }
}

/// The menu's id, and its items.
const POPUP_ID: u32 = 0x7075_636b;
#[derive(Clone, Copy, PartialEq, Debug)]
enum Item {
    Save,
    SaveAs,
    EditTown,
    Options,
    MainMenu,
    SaveAndQuit,
    Quit,
}

/// What's on the open menu, and the game's speed to go back to.
#[derive(Resource)]
struct OpenItems(Vec<Item>, usize);

fn open_menu(
    mut commands: Commands,
    want: Option<Res<OpenGamePopup>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    slot: Option<Res<crate::save::SaveSlot>>,
    mut clock: ResMut<crate::clock::GameClock>,
    open: Option<Res<OpenItems>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    if want.is_none() {
        return;
    }
    let at = windows.single().ok().and_then(|w| w.cursor_position()).unwrap_or(Vec2::new(40.0, 600.0));
    commands.remove_resource::<OpenGamePopup>();
    let Some(mut ui) = ui else { return };
    if open.is_some() {
        return;
    }
    let mut items = vec![Item::Save];
    if slot.is_some_and(|s| s.0.is_some()) {
        items.push(Item::SaveAs);
    }
    items.extend([Item::EditTown, Item::Options, Item::MainMenu, Item::SaveAndQuit, Item::Quit]);
    let word = |key: &str, plain: &str| ui.localize(key).unwrap_or_else(|| plain.to_string());
    let labels: Vec<String> = items
        .iter()
        .map(|i| match i {
            Item::Save => word("Ui/Caption/Options:Save", "Save"),
            Item::SaveAs => word("Ui/Caption/Options:SaveAs", "Save As"),
            Item::EditTown => word("Ui/Caption/Options:EditTown", "Edit Town"),
            Item::Options => word("Ui/Caption/Options:Options", "Options"),
            Item::MainMenu => word("Ui/Caption/Options:QuitToMenu", "Quit to Main Menu"),
            Item::SaveAndQuit => word("Ui/Caption/Options:QuitToWindowsAndSave", "Save and Quit"),
            Item::Quit => word("Ui/Caption/Options:QuitToWindows", "Quit"),
        })
        .collect();
    if crate::popupmenu::open_popup(&mut commands, &mut ui, &mut images, &mut fonts, POPUP_ID, &labels, at).is_some() {
        commands.insert_resource(OpenItems(items, clock.speed));
        clock.speed = 0;
    }
}

/// Saving and quitting: once the save's written.
#[derive(Resource)]
struct QuitAfterSave;

#[allow(clippy::too_many_arguments)]
fn choose(
    mut commands: Commands,
    mut choices: MessageReader<PopupChoice>,
    open: Option<Res<OpenItems>>,
    mut clock: ResMut<crate::clock::GameClock>,
    (mut options, settings): (ResMut<crate::options::OptionsPanel>, Res<crate::options::Settings>),
    (mut save, mut save_as): (MessageWriter<crate::save::SaveRequest>, MessageWriter<crate::save::SaveAsRequest>),
    (mut next, mut exit): (ResMut<NextState<crate::AppState>>, MessageWriter<AppExit>),
    (quitting, last): (Option<Res<QuitAfterSave>>, Option<Res<crate::save::LastSave>>),
) {
    if quitting.is_some() && last.is_some_and(|l| l.is_changed()) {
        exit.write(AppExit::Success);
    }
    let Some(open) = open else { return };
    for c in choices.read() {
        if c.id != POPUP_ID {
            continue;
        }
        commands.remove_resource::<OpenItems>();
        clock.speed = open.1;
        match c.index.and_then(|i| open.0.get(i)) {
            Some(Item::Save) => crate::save::request_save(&mut save),
            Some(Item::SaveAs) => {
                save_as.write(crate::save::SaveAsRequest);
            }
            Some(Item::EditTown) => commands.queue(|w: &mut World| {
                w.write_message(crate::edittown::OpenEditTown);
            }),
            Some(Item::Options) => crate::options::open_options(&mut commands, &mut options, &settings),
            Some(Item::MainMenu) => next.set(crate::AppState::MainMenu),
            Some(Item::SaveAndQuit) => {
                crate::save::request_save(&mut save);
                commands.insert_resource(QuitAfterSave);
            }
            Some(Item::Quit) => {
                exit.write(AppExit::Success);
            }
            None => {}
        }
    }
}
