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
        app.add_systems(Update, enable).add_systems(Update, (open_menu, choose, named, save_as_test).chain().run_if(in_state(crate::PlayMode::Live)));
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
    /// The Lessons dialog (`lessons`).
    Lessons,
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
    (hud, nodes): (Option<Res<crate::livehud::LiveHud>>, Query<(&ComputedNode, &bevy::ui::UiGlobalTransform)>),
) {
    if want.is_none() {
        return;
    }
    // (From the puck's options button's middle, as the game's `OptionsMenuInfo` puts it, however
    // it was asked for: the button or its key.)
    let button = hud.as_ref().and_then(|h| h.puck.id(crate::livehud::PUCK_OPTIONS)).and_then(|e| nodes.get(e).ok()).map(|(n, tf)| tf.translation * n.inverse_scale_factor());
    let at = button.or_else(|| windows.single().ok().and_then(|w| w.cursor_position())).unwrap_or(Vec2::new(40.0, 600.0));
    commands.remove_resource::<OpenGamePopup>();
    let Some(mut ui) = ui else { return };
    if open.is_some() {
        return;
    }
    let mut items = vec![Item::Save];
    if slot.is_some_and(|s| s.0.is_some()) {
        items.push(Item::SaveAs);
    }
    items.extend([Item::EditTown, Item::Lessons, Item::Options, Item::MainMenu, Item::SaveAndQuit, Item::Quit]);
    let word = |key: &str, plain: &str| ui.localize(key).unwrap_or_else(|| plain.to_string());
    let labels: Vec<String> = items
        .iter()
        .map(|i| match i {
            Item::Save => word("Ui/Caption/Options:Save", "Save"),
            Item::SaveAs => word("Ui/Caption/Options:SaveAs", "Save As"),
            Item::EditTown => word("Ui/Caption/Options:EditTown", "Edit Town"),
            Item::Lessons => word("Ui/Caption/MainMenu:Tutorials", "Lessons"),
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

/// Saving and quitting once the save's named (a new game's first save).
#[derive(Resource)]
struct QuitOnceNamed;

/// How long a save's name can be.
pub const NAME_LENGTH: usize = 40;

/// The name of the game's save as it stands: its file's, or (not saved yet) the household's and
/// the town's.
fn current_name(slot: Option<&crate::save::SaveSlot>, household: Option<&crate::interact::Household>, world: Option<&crate::loading::CurrentWorld>) -> String {
    if let Some(stem) = slot.and_then(|s| s.0.as_ref()).and_then(|p| p.file_stem()) {
        return stem.to_string_lossy().into_owned();
    }
    let clean = |s: &str| s.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect::<String>();
    format!("{} - {}", clean(household.map_or("", |h| h.name.as_str())), clean(world.map_or("", |w| w.name.as_str())))
}

/// A name typed in as a file's: what Windows can't have in one taken out.
pub fn file_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()).take(NAME_LENGTH).collect();
    s.trim().trim_end_matches('.').trim().to_string()
}

/// The game's "Name Your Saved Game" box, the name as it stands to start.
fn ask_name(questions: &mut crate::dialog::Questions, ui: Option<&UiAssets>, suggested: String) {
    let word = |key: &str, plain: &str| ui.and_then(|u| u.localize(key)).unwrap_or_else(|| plain.to_string());
    let answer = |l: String| crate::dialog::Answer { label: l, detail: String::new(), icon: String::new() };
    questions.ask(crate::dialog::Ask {
        about: crate::dialog::Question::SaveName { suggested },
        icon: String::new(),
        heading: String::new(),
        title: word("Ui/Caption/GameEntry/MainMenu:NewTownTitle", "Name Your Saved Game"),
        text: word("Ui/Caption/GameEntry/MainMenu:NewTownPrompt", "Enter a name for your saved game:"),
        answers: vec![answer(word("Ui/Caption/Global:Ok", "OK")), answer(word("Ui/Caption/Global:Cancel", "Cancel"))],
    });
}

/// A save's name typed in: saved to that file (another save's, once it's said to save over it);
/// cancelled, not saved.
#[allow(clippy::too_many_arguments)]
fn named(
    mut commands: Commands,
    mut answers: MessageReader<crate::dialog::Answered>,
    typed: Res<crate::dialog::Typed>,
    mut questions: ResMut<crate::dialog::Questions>,
    ui: Option<Res<UiAssets>>,
    slot: Option<Res<crate::save::SaveSlot>>,
    mut save_as: MessageWriter<crate::save::SaveAsRequest>,
    quit: Option<Res<QuitOnceNamed>>,
) {
    for a in answers.read() {
        let (name, sure) = match &a.about {
            crate::dialog::Question::SaveName { suggested } => (file_name(if typed.0.trim().is_empty() { suggested } else { &typed.0 }), false),
            crate::dialog::Question::Overwrite { name } => (name.clone(), true),
            _ => continue,
        };
        if a.answer != 0 || name.is_empty() {
            // (Cancelled; or not to save over the other, so another name.)
            if sure && !name.is_empty() {
                ask_name(&mut questions, ui.as_deref(), name);
            } else {
                commands.remove_resource::<QuitOnceNamed>();
            }
            continue;
        }
        let path = crate::save::saves_dir().join(format!("{name}.json"));
        let ours = slot.as_ref().and_then(|s| s.0.as_ref()).is_some_and(|p| *p == path);
        if !sure && !ours && path.exists() {
            let word = |key: &str, plain: &str| ui.as_ref().and_then(|u| u.localize(key)).unwrap_or_else(|| plain.to_string());
            let answer = |l: String| crate::dialog::Answer { label: l, detail: String::new(), icon: String::new() };
            questions.ask(crate::dialog::Ask {
                about: crate::dialog::Question::Overwrite { name },
                icon: String::new(),
                heading: String::new(),
                title: String::new(),
                text: word("Ui/Caption/Options:SaveConfirm", "Save file already exists. Are you sure you want to overwrite it?"),
                answers: vec![answer(word("Ui/Caption/Global:Yes", "Yes")), answer(word("Ui/Caption/Global:No", "No"))],
            });
            continue;
        }
        info!("saving as {}", path.display());
        commands.insert_resource(crate::save::SaveTo(path));
        save_as.write(crate::save::SaveAsRequest);
        if quit.is_some() {
            commands.remove_resource::<QuitOnceNamed>();
            commands.insert_resource(QuitAfterSave);
        }
    }
}

/// SAVE_AS=<name> (tests): ten seconds in, the box to name the save, that name typed in.
fn save_as_test(mut questions: ResMut<crate::dialog::Questions>, ui: Option<Res<UiAssets>>, time: Res<Time>, mut done: Local<bool>) {
    let Ok(name) = std::env::var("SAVE_AS") else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    *done = true;
    ask_name(&mut questions, ui.as_deref(), name);
}

#[allow(clippy::too_many_arguments)]
fn choose(
    mut commands: Commands,
    mut choices: MessageReader<PopupChoice>,
    open: Option<Res<OpenItems>>,
    mut clock: ResMut<crate::clock::GameClock>,
    (mut options, settings): (ResMut<crate::options::OptionsPanel>, Res<crate::options::Settings>),
    mut save: MessageWriter<crate::save::SaveRequest>,
    (mut next, mut exit): (ResMut<NextState<crate::AppState>>, MessageWriter<AppExit>),
    (quitting, last): (Option<Res<QuitAfterSave>>, Option<Res<crate::save::LastSave>>),
    (mut questions, ui, slot): (ResMut<crate::dialog::Questions>, Option<Res<UiAssets>>, Option<Res<crate::save::SaveSlot>>),
    (household, world): (Option<Res<crate::interact::Household>>, Option<Res<crate::loading::CurrentWorld>>),
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
        // (A save's named the first time it's saved, as the game asks; and saving as another.)
        let unnamed = slot.as_ref().is_none_or(|s| s.0.is_none());
        let name = || current_name(slot.as_deref(), household.as_deref(), world.as_deref());
        match c.index.and_then(|i| open.0.get(i)) {
            Some(Item::Save) if unnamed => ask_name(&mut questions, ui.as_deref(), name()),
            Some(Item::Save) => crate::save::request_save(&mut save),
            Some(Item::SaveAs) => ask_name(&mut questions, ui.as_deref(), name()),
            Some(Item::SaveAndQuit) if unnamed => {
                ask_name(&mut questions, ui.as_deref(), name());
                commands.insert_resource(QuitOnceNamed);
            }
            Some(Item::EditTown) => commands.queue(|w: &mut World| {
                w.write_message(crate::edittown::OpenEditTown);
            }),
            Some(Item::Lessons) => commands.insert_resource(crate::lessons::OpenLessons(None)),
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
