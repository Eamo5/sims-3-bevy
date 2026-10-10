//! The main menu in the game's own look (`GameEntryMainMenu`, UI.package), driven as UI.dll's
//! `MainMenu` drives it: its picture behind, the logo, and one of its two panels. With saved
//! games: the saves along a strip (each in the game's frame, the newest chosen), the chosen
//! one's name, household, date and time, and Play Now; Add New Game turns the panel to a new
//! game. With none (or for a new game): the town to play, chosen from the game's drop-down list
//! of the home towns installed, with the town's own picture and description from its world
//! file (`GetWorldFileDetails`), and Play Now. The round button opens the game's popup menu:
//! Options and Quit.

use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::AppState;
use crate::data::WorldList;
use crate::layout::{Spawned, UiAssets, UiButton};
use crate::livehud::{pressed, set_text, set_visible};
use crate::save::SaveGame;

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (bake_interface, spawn_menu, menu_controls, set_pictures, cover).chain().run_if(in_state(AppState::MainMenu)))
            .add_systems(OnExit(AppState::MainMenu), |mut commands: Commands| commands.remove_resource::<MainMenuUi>());
    }
}

// The windows' control ids (`MainMenu.ControlIDs`).
const POPUP: [u32; 2] = [0xd6da_8d01, 0xd6da_8d02];
const ADD_NEW_GAME: u32 = 0xd6da_8d04;
const SAVE_GRID: u32 = 0xd6da_8d05;
const DELETE_TOWN: u32 = 0xd6da_8d18;
const COPY_TOWN: u32 = 0xd6da_8d1c;
const NEW_ONLY: u32 = 0xd6da_8e00;
const NEW_ONLY_PLAY: u32 = 0xd6da_8e01;
const NEW_ONLY_THUMB: u32 = 0xd6da_8e02;
const NEW_ONLY_DESC: u32 = 0xd6da_8e04;
const NEW_ONLY_TOWN: u32 = 0xd6da_8e05;
const NEW_ONLY_TOWN_LIST: u32 = 0xd6da_8e06;
const NEW_ONLY_TOWN_GRID: u32 = 0xd6da_8e07;
const SAVED: u32 = 0xd6da_8f00;
const SAVED_PLAY: u32 = 0xd6da_8f01;
const SAVED_INFO: u32 = 0xd6da_8f10;
const SAVED_THUMB: u32 = 0xd6da_8f11;
const SAVED_DATE: u32 = 0xd6da_8f12;
const SAVED_TIME: u32 = 0xd6da_8f13;
const SAVED_NAME: u32 = 0xd6da_8f14;
const SAVED_HOUSEHOLD: u32 = 0xd6da_8f15;
const SAVED_DESC: u32 = 0xd6da_8f16;
const SAVED_NEW_INFO: u32 = 0xd6da_8f20;
const SAVED_NEW_THUMB: u32 = 0xd6da_8f21;
const SAVED_NEW_DESC: u32 = 0xd6da_8f23;
const SAVED_NEW_TOWN: u32 = 0xd6da_8f24;
const SAVED_NEW_TOWN_LIST: u32 = 0xd6da_8f25;
const SAVED_NEW_TOWN_GRID: u32 = 0xd6da_8f26;
const VERSION: u32 = 0xd6da_8d30;
const ONLINE: u32 = 0x0ddd_eb30;
/// A save's cell (`GameEntryLoadItem`): its picture, its name, the greyed overlay.
const ITEM_THUMB: u32 = 0xd6da_8d41;
const ITEM_NAME: u32 = 0xd6da_8d42;
/// The town-type marks on a picture (`WorldTypeIconBase | type`): the base game's.
const TOWN_BASE: u32 = 0x0a00_df40;
/// The save strip's arrows.
const SCROLL_BACK: u32 = 0x0600_0000;
const SCROLL_FORWARD: u32 = 0x0600_0001;
/// The popup menu's id, and the most towns listed before the list scrolls.
const POPUP_ID: u32 = 0x6d61_696e;
const MAX_TOWNS: usize = 15;

/// A town to start in, as the menu shows it.
struct MenuTown {
    /// Index into the world list.
    world: usize,
    name: String,
    description: String,
    picture: Option<Handle<Image>>,
}

/// The menu on screen.
#[derive(Resource)]
pub struct MainMenuUi {
    s: Spawned,
    towns: Vec<MenuTown>,
    saves: Vec<(PathBuf, SaveGame, std::time::SystemTime)>,
    /// A saved game chosen (else a new game), the town chosen for a new game.
    save: Option<usize>,
    town: usize,
    list_open: bool,
    /// The first save on the strip, the first town in the list.
    save_scroll: usize,
    town_scroll: usize,
    dirty: bool,
    cells: Option<Entity>,
    town_cells: [Option<Entity>; 2],
    /// The words put on the town buttons.
    captions: Vec<(Entity, Entity)>,
}

/// A save's cell, a town in the list.
#[derive(Component)]
struct SaveCell(usize);
#[derive(Component)]
struct TownCell(usize);

/// The old menu's root (put away when the game's is up).
#[derive(Component)]
pub struct OldMenu;

/// The home towns (`PopulateWorldFileList`: not the vacation, university or future worlds),
/// each with its name, description and picture from its world file.
fn towns(worlds: &WorldList, ui: &UiAssets, images: &mut Assets<Image>) -> Vec<MenuTown> {
    const AWAY: [&str; 5] = ["Al Simhara", "Champs Les Sims", "Shang Simla", "Sims University", "Oasis Landing"];
    let mut out: Vec<MenuTown> = worlds
        .0
        .iter()
        .enumerate()
        .filter(|(_, w)| !AWAY.iter().any(|a| w.name.eq_ignore_ascii_case(a)))
        .map(|(i, w)| {
            let (mut name, mut description, mut picture) = (w.name.clone(), String::new(), None);
            if let Ok(pkg) = s3pkg::Package::open(&w.path) {
                let read = |t: u32| pkg.of_type(t).next().and_then(|e| pkg.read(e).ok());
                let text = |t: u32| read(t).filter(|d| d.len() >= 8).and_then(|d| ui.localize_key(u64::from_le_bytes(d[..8].try_into().ok()?)));
                if let Some(n) = text(0x022B_756C) {
                    name = n;
                }
                description = text(0x35A3_3E29).unwrap_or_default();
                picture = read(0x0668_F635).and_then(|png| Image::from_buffer(&png, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::default(), RenderAssetUsages::RENDER_WORLD).ok()).map(|im| images.add(im));
            }
            MenuTown { world: i, name, description, picture }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The town a new game starts in to begin with (`GetDefaultWorld`: the newest pack's).
fn default_town(towns: &[MenuTown]) -> usize {
    ["Isla Paradiso", "Moonlight Falls", "Starlight Shores", "Appaloosa Plains", "Bridgeport", "Twinbrook", "Sunset Valley"]
        .iter()
        .find_map(|n| towns.iter().position(|t| t.name.eq_ignore_ascii_case(n)))
        .unwrap_or(0)
}

/// The interface converted anew (a newer layout of it than the one converted before) while
/// the menu waits, then put up: the game's menu takes the plain one's place.
fn bake_interface(
    mut commands: Commands,
    ui: Option<Res<UiAssets>>,
    install: Res<crate::data::InstallPath>,
    mut fonts: ResMut<Assets<Font>>,
    mut job: Local<Option<std::sync::Arc<std::sync::atomic::AtomicU8>>>,
) {
    use std::sync::atomic::Ordering;
    if ui.is_some() {
        return;
    }
    match job.as_ref().map(|j| j.load(Ordering::Acquire)) {
        None => {
            let root = s3bake::default_root();
            // (Only once the rest is converted: the first run converts it all while a town loads.)
            if root.global_manifest().is_none() || s3bake::ui_ready(&root) {
                *job = Some(std::sync::Arc::new(std::sync::atomic::AtomicU8::new(2)));
                return;
            }
            let flag = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
            let (f, path) = (flag.clone(), install.0.clone());
            std::thread::spawn(move || {
                let pkgs = s3pkg::install::open_install(&path, |_| true);
                let ok = !pkgs.is_empty() && s3bake::bake_ui(&s3bake::default_root(), &pkgs, &path, &|s: &str| info!("{s}")).is_ok();
                f.store(if ok { 1 } else { 3 }, Ordering::Release);
            });
            *job = Some(flag);
        }
        Some(1) => {
            if let Some(u) = crate::layout::open(&mut fonts) {
                commands.insert_resource(u);
            }
            if let Some(j) = job.as_ref() {
                j.store(2, Ordering::Release);
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_menu(
    mut commands: Commands,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    worlds: Res<WorldList>,
    menu: Option<Res<MainMenuUi>>,
    old: Query<Entity, With<OldMenu>>,
) {
    let Some(mut ui) = ui else { return };
    if menu.is_some() || ui.layout("GameEntryMainMenu").is_none() {
        return;
    }
    for e in &old {
        commands.entity(e).despawn();
    }
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "GameEntryMainMenu") else { return };
    if let Some(r) = s.root {
        commands.entity(r).insert((DespawnOnExit(AppState::MainMenu), CoverPicture));
    }
    // (No version box, online features or copying and deleting towns here.)
    for id in [VERSION, DELETE_TOWN, COPY_TOWN] {
        set_hidden(&mut commands, s.id(id));
    }
    for e in s.all_with(ONLINE) {
        set_hidden(&mut commands, Some(e));
    }
    for (id, tip) in [(POPUP[0], "Options"), (POPUP[1], "Options"), (ADD_NEW_GAME, "New Game"), (NEW_ONLY_PLAY, "Play Now"), (SAVED_PLAY, "Play Now")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    for g in [SAVE_GRID, NEW_ONLY_TOWN_GRID, SAVED_NEW_TOWN_GRID] {
        if let Some(e) = s.id(g) {
            commands.entity(e).remove::<Pickable>().insert((Interaction::default(), Node { overflow: Overflow::clip(), ..grid_node(&ui, g) }));
        }
    }
    let towns = towns(&worlds, &ui, &mut images);
    let town = default_town(&towns);
    let saves: Vec<(PathBuf, SaveGame, std::time::SystemTime)> = crate::save::list_saves()
        .into_iter()
        .map(|(p, g)| {
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (p, g, t)
        })
        .collect();
    // (The newest save chosen to start with.)
    let save = if saves.is_empty() { None } else { Some(0) };
    commands.insert_resource(MainMenuUi { s, towns, saves, save, town, list_open: false, save_scroll: 0, town_scroll: 0, dirty: true, cells: None, town_cells: [None, None], captions: Vec::new() });
}

fn set_hidden(commands: &mut Commands, e: Option<Entity>) {
    if let Some(e) = e {
        commands.entity(e).insert(Visibility::Hidden);
    }
}

/// A window's node as designed (to change one property of it).
fn grid_node(ui: &UiAssets, id: u32) -> Node {
    let a = ui.find("GameEntryMainMenu", id).map(|w| w.area).unwrap_or_default();
    match ui.find("GameEntryMainMenu", id).map(|w| w.place) {
        // (The town lists fill their drop-down.)
        Some(s3bake::ui::UiPlace::Simple(15)) => Node { position_type: PositionType::Absolute, left: Val::Px(a[0]), top: Val::Px(a[1]), right: Val::Px(-a[2]), bottom: Val::Px(-a[3]), ..default() },
        _ => Node { position_type: PositionType::Absolute, left: Val::Px(a[0]), top: Val::Px(a[1]), width: Val::Px(a[2] - a[0]), height: Val::Px(a[3] - a[1]), ..default() },
    }
}

/// A holder for fresh children under a window (cleared).
fn holder(commands: &mut Commands, parent: Entity, slot: &mut Option<Entity>, vis: &Query<&mut Visibility>) -> Entity {
    match slot.filter(|h| vis.contains(*h)) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => {
            let h = commands
                .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Visibility::Inherited, Pickable::IGNORE, ChildOf(parent)))
                .id();
            *slot = Some(h);
            h
        }
    }
}

/// A picture in a window in place of its own (its own picture node changed, so the marks over
/// it stay).
fn picture(commands: &mut Commands, win: Option<Entity>, h: Option<Handle<Image>>) {
    if let (Some(w), Some(h)) = (win, h) {
        commands.entity(w).insert(SetPicture(h));
    }
}

#[derive(Component)]
struct SetPicture(Handle<Image>);

fn set_pictures(mut commands: Commands, q: Query<(Entity, &SetPicture, Option<&crate::layout::UiPicture>)>, mut pics: Query<(&mut ImageNode, &mut Node)>) {
    for (e, p, pic) in &q {
        match pic.and_then(|u| pics.get_mut(u.0).ok()) {
            Some((mut img, mut node)) => {
                img.image = p.0.clone();
                img.color = Color::WHITE;
                *node = Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
            }
            None => crate::hudpanels::picture(&mut commands, e, p.0.clone(), Color::WHITE),
        }
        commands.entity(e).remove::<SetPicture>();
    }
}

/// The menu's buttons: the saves, Add New Game, the town list, Play Now, the popup menu.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn menu_controls(
    mut commands: Commands,
    menu: Option<ResMut<MainMenuUi>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (clicks, saves_q, towns_q, hovered): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &SaveCell), Changed<Interaction>>, Query<(&Interaction, &TownCell), Changed<Interaction>>, Query<&Interaction>),
    (mut vis, mut texts, mut buttons): (Query<&mut Visibility>, Query<&mut Text>, Query<&mut UiButton>),
    (worlds, mut next, mut exit): (Res<WorldList>, ResMut<NextState<AppState>>, MessageWriter<AppExit>),
    (mut choices, mut options, settings, mut wheel): (MessageReader<crate::popupmenu::PopupChoice>, ResMut<crate::options::OptionsPanel>, Res<crate::options::Settings>, MessageReader<MouseWheel>),
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let scrolled: f32 = wheel.read().map(|w| w.y.signum()).sum();
    let (Some(mut m), Some(mut ui)) = (menu, ui) else { return };
    let m = &mut *m;
    let has_saves = !m.saves.is_empty();
    // The popup menu: Options and Quit (`OptionsMenuItems`).
    for id in POPUP {
        if pressed(&clicks, m.s.id(id)) {
            let at = windows.single().ok().and_then(|w| w.cursor_position()).unwrap_or(Vec2::new(60.0, 600.0));
            let items = [ui.localize("Ui/Caption/MainMenu:Options").unwrap_or_else(|| "Options".into()), ui.localize("Ui/Caption/MainMenu:Quit").unwrap_or_else(|| "Quit".into())];
            crate::popupmenu::open_popup(&mut commands, &mut ui, &mut images, &mut fonts, POPUP_ID, &items, at);
        }
    }
    for c in choices.read() {
        match (c.id, c.index) {
            (POPUP_ID, Some(0)) => crate::options::open_options(&mut commands, &mut options, &settings),
            (POPUP_ID, Some(1)) => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
    // A save chosen, or a new game.
    for (i, c) in &saves_q {
        if *i == Interaction::Pressed && m.save != Some(c.0) {
            m.save = Some(c.0);
            m.dirty = true;
            play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
        }
    }
    if pressed(&clicks, m.s.id(ADD_NEW_GAME)) && m.save.is_some() {
        m.save = None;
        m.town = default_town(&m.towns);
        m.dirty = true;
    }
    // The save strip's arrows and wheel.
    let most = m.saves.len().saturating_sub(4);
    if pressed(&clicks, m.s.id(SCROLL_BACK)) || (scrolled > 0.0 && m.s.id(SAVE_GRID).and_then(|g| hovered.get(g).ok()).is_some_and(|i| *i != Interaction::None)) {
        m.save_scroll = m.save_scroll.saturating_sub(1);
        m.dirty = true;
    }
    if pressed(&clicks, m.s.id(SCROLL_FORWARD)) || (scrolled < 0.0 && m.s.id(SAVE_GRID).and_then(|g| hovered.get(g).ok()).is_some_and(|i| *i != Interaction::None)) {
        m.save_scroll = (m.save_scroll + 1).min(most);
        m.dirty = true;
    }
    // The town list: opened by its button, a town chosen from it.
    let (town_button, town_list, town_grid) = if has_saves { (SAVED_NEW_TOWN, SAVED_NEW_TOWN_LIST, SAVED_NEW_TOWN_GRID) } else { (NEW_ONLY_TOWN, NEW_ONLY_TOWN_LIST, NEW_ONLY_TOWN_GRID) };
    if pressed(&clicks, m.s.id(town_button)) {
        m.list_open = !m.list_open;
        m.dirty = true;
    }
    for (i, c) in &towns_q {
        if *i == Interaction::Pressed {
            m.town = c.0;
            m.list_open = false;
            m.dirty = true;
        }
    }
    if m.list_open && scrolled != 0.0 && m.s.id(town_grid).and_then(|g| hovered.get(g).ok()).is_some_and(|i| *i != Interaction::None) {
        let most = m.towns.len().saturating_sub(MAX_TOWNS);
        m.town_scroll = if scrolled > 0.0 { m.town_scroll.saturating_sub(1) } else { (m.town_scroll + 1).min(most) };
        m.dirty = true;
    }
    // Play Now: the save chosen, or a new game in the town chosen (the menu's own actions, see
    // `menu::menu_actions`).
    if pressed(&clicks, m.s.id(SAVED_PLAY)) || pressed(&clicks, m.s.id(NEW_ONLY_PLAY)) {
        play.write(crate::sound::PlaySound::ui("ui_primary_button"));
    }
    if !m.dirty {
        return;
    }
    m.dirty = false;
    refresh(&mut commands, m, &mut ui, &mut images, &mut fonts, &mut vis, &mut texts, &mut buttons, (town_button, town_list, town_grid));
}

/// Shows the panel for saved games or a new game, and fills it in (`RefreshInfoPane`).
#[allow(clippy::too_many_arguments)]
fn refresh(
    commands: &mut Commands,
    m: &mut MainMenuUi,
    ui: &mut UiAssets,
    images: &mut Assets<Image>,
    fonts: &mut Assets<Font>,
    vis: &mut Query<&mut Visibility>,
    texts: &mut Query<&mut Text>,
    buttons: &mut Query<&mut UiButton>,
    (town_button, town_list, town_grid): (u32, u32, u32),
) {
    let has_saves = !m.saves.is_empty();
    set_visible(vis, m.s.id(NEW_ONLY), !has_saves);
    set_visible(vis, m.s.id(SAVED), has_saves);
    set_visible(vis, m.s.id(SAVED_INFO), m.save.is_some());
    set_visible(vis, m.s.id(SAVED_NEW_INFO), has_saves && m.save.is_none());
    set_button(buttons, m.s.id(ADD_NEW_GAME), m.save.is_none(), false);
    let action = match m.save {
        Some(k) => crate::menu::MenuAction::LoadSave(k),
        None => crate::menu::MenuAction::PlayWorld(m.towns.get(m.town).map_or(0, |t| t.world)),
    };
    for id in [SAVED_PLAY, NEW_ONLY_PLAY] {
        if let Some(e) = m.s.id(id) {
            commands.entity(e).insert(action.clone());
        }
    }
    // The saves along the strip.
    if let (Some(grid), Some(g), Some(template)) = (m.s.id(SAVE_GRID), ui.find("GameEntryMainMenu", SAVE_GRID).and_then(|w| w.grid), ui.export("GameEntryLoadItem", 1).cloned()) {
        let h = holder(commands, grid, &mut m.cells, vis);
        let step = g.cell[0] + g.cell_padding[0] + g.cell_padding[2];
        for (n, k) in (m.save_scroll..m.saves.len()).take(g.columns.max(1) as usize).enumerate() {
            let (_, game, _) = &m.saves[k];
            let x = g.padding[0] + n as f32 * step;
            let mut w = template.clone();
            w.cls = "Button".to_string();
            w.area = [x, g.padding[1], x + g.cell[0], g.padding[1] + g.cell[1]];
            let c = ui.spawn_under(commands, images, fonts, &w, h);
            let Some(r) = c.root else { continue };
            commands.entity(r).insert((SaveCell(k), crate::icons::Tooltip(save_title(&m.saves[k].0))));
            if m.save == Some(k) {
                commands.entity(r).insert(crate::layout::Selected);
            }
            let town = m.towns.iter().find(|t| t.name.eq_ignore_ascii_case(&game.world)).and_then(|t| t.picture.clone());
            picture(commands, c.id(ITEM_THUMB), save_thumbnail(images, &m.saves[k].0).or(town));
            set_hidden(commands, c.id(ITEM_NAME));
            if let Some(e) = c.id(TOWN_BASE) {
                commands.entity(e).insert(Visibility::Inherited);
            }
        }
    }
    set_button(buttons, m.s.id(SCROLL_BACK), false, m.save_scroll == 0);
    set_button(buttons, m.s.id(SCROLL_FORWARD), false, m.save_scroll + 4 >= m.saves.len());
    // The save chosen.
    if let Some((path, g, when)) = m.save.and_then(|k| m.saves.get(k)) {
        set_text(texts, m.s.text(SAVED_NAME), &save_title(path));
        set_text(texts, m.s.text(SAVED_HOUSEHOLD), &g.household);
        set_text(texts, m.s.text(SAVED_DESC), &format!("{} · day {} · §{}", g.world, (g.minutes / 1440.0) as u32 + 1, crate::lifetime::group(g.funds)));
        set_text(texts, m.s.text(SAVED_DATE), &date_stamp(*when));
        let mins = g.minutes.rem_euclid(1440.0);
        set_text(texts, m.s.text(SAVED_TIME), &crate::interact::hour_label((mins / 60.0) as f32));
        let town = m.towns.iter().find(|t| t.name.eq_ignore_ascii_case(&g.world)).and_then(|t| t.picture.clone());
        picture(commands, m.s.id(SAVED_THUMB), save_thumbnail(images, path).or(town));
    }
    // A new game: the town chosen, its picture and description; its list.
    let (thumb, desc) = if has_saves { (SAVED_NEW_THUMB, SAVED_NEW_DESC) } else { (NEW_ONLY_THUMB, NEW_ONLY_DESC) };
    if let Some(t) = m.towns.get(m.town) {
        picture(commands, m.s.id(thumb), t.picture.clone());
        set_text(texts, m.s.text(desc), &t.description);
        set_caption(commands, ui, fonts, m.s.id(town_button), &t.name, texts, &mut m.captions);
        if let Some(e) = m.s.id(thumb).and_then(|w| m.s.within(w, TOWN_BASE)) {
            commands.entity(e).insert(Visibility::Inherited);
        }
    }
    set_button(buttons, m.s.id(town_button), m.list_open, false);
    set_visible(vis, m.s.id(town_list), m.list_open);
    if let (true, Some(grid), Some(list), Some(template)) = (m.list_open, m.s.id(town_grid), ui.find("GameEntryMainMenu", town_list).cloned(), ui.export("GameEntryTownSelectEntry", 1).cloned()) {
        let g = list.find(town_grid).and_then(|w| w.grid).unwrap_or_default();
        let shown = m.towns.len().min(MAX_TOWNS);
        // (The list as long as the towns, `PopulateWorldFileComboBox`.)
        let extra = (shown as f32 - g.rows as f32) * g.cell[1];
        if let Some(e) = m.s.id(town_list) {
            commands.entity(e).insert(Node { position_type: PositionType::Absolute, left: Val::Px(list.area[0]), top: Val::Px(list.area[1]), width: Val::Px(list.area[2] - list.area[0]), height: Val::Px(list.area[3] - list.area[1] + extra), ..default() });
        }
        let k = (town_grid == SAVED_NEW_TOWN_GRID) as usize;
        let h = holder(commands, grid, &mut m.town_cells[k], vis);
        for (n, i) in (m.town_scroll..m.towns.len()).take(shown).enumerate() {
            let y = g.padding[1] + n as f32 * g.cell[1];
            let mut w = template.clone();
            w.area = [g.padding[0], y, g.padding[0] + g.cell[0], y + g.cell[1]];
            w.caption = m.towns[i].name.clone();
            let c = ui.spawn_under(commands, images, fonts, &w, h);
            if let Some(r) = c.root {
                commands.entity(r).insert(TownCell(i));
                if i == m.town {
                    commands.entity(r).insert(crate::layout::Selected);
                }
            }
            if let Some(e) = c.id(TOWN_BASE) {
                commands.entity(e).insert(Visibility::Inherited);
            }
        }
    }
}

fn set_button(buttons: &mut Query<&mut UiButton>, e: Option<Entity>, selected: bool, disabled: bool) {
    if let Some(e) = e
        && let Ok(mut b) = buttons.get_mut(e)
        && (b.selected != selected || b.disabled != disabled)
    {
        b.selected = selected;
        b.disabled = disabled;
    }
}

/// A button's caption (the town button has none of its own: its words are put on it, left,
/// in its own text style).
#[allow(clippy::too_many_arguments)]
fn set_caption(commands: &mut Commands, ui: &mut UiAssets, fonts: &mut Assets<Font>, button: Option<Entity>, caption: &str, texts: &mut Query<&mut Text>, captions: &mut Vec<(Entity, Entity)>) {
    let Some(b) = button else { return };
    if let Some((_, t)) = captions.iter().find(|(e, _)| *e == b) {
        set_text(texts, Some(*t), caption);
        return;
    }
    let style = ui.find("GameEntryMainMenu", NEW_ONLY_TOWN).map_or(0, |w| w.font);
    let (font, line) = ui.text_font(fonts, style);
    let holder = commands
        .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(10.0), right: Val::Px(26.0), top: Val::Px(0.0), bottom: Val::Px(0.0), align_items: AlignItems::Center, ..default() }, Pickable::IGNORE, ChildOf(b)))
        .id();
    let t = commands.spawn((Text::new(caption), font, line, TextColor(crate::layout::color(0xff1a_3366)), TextLayout::new(Justify::Left, LineBreak::NoWrap), Pickable::IGNORE, ChildOf(holder))).id();
    captions.push((b, t));
}

/// A save's own picture, if one was taken when it was saved (`<save>.png`).
fn save_thumbnail(images: &mut Assets<Image>, save: &std::path::Path) -> Option<Handle<Image>> {
    picture_file(images, &save.with_extension("png"))
}

/// A PNG on disk as a picture.
pub fn picture_file(images: &mut Assets<Image>, path: &std::path::Path) -> Option<Handle<Image>> {
    let png = std::fs::read(path).ok()?;
    let im = Image::from_buffer(&png, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::default(), RenderAssetUsages::RENDER_WORLD).ok()?;
    Some(images.add(im))
}

/// (Test hook) chooses a new game in a town (by its index in the world list).
pub fn choose_world(m: &mut MainMenuUi, world: usize) -> bool {
    let Some(t) = m.towns.iter().position(|t| t.world == world) else { return false };
    m.save = None;
    m.town = t;
    m.dirty = true;
    true
}

/// A save's name: its file's ("Goth - Sunset Valley").
fn save_title(p: &std::path::Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The day a game was saved, as the game writes it (month/day/year).
fn date_stamp(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs.div_euclid(86_400);
    // (Civil from days: Howard Hinnant's algorithm.)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{mo:02}/{d:02}/{:02}", y % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_written_month_day_year() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_791_590_400); // 2026-10-10
        assert_eq!(date_stamp(t), "10/10/26");
    }
}

/// The menu's picture behind covers the screen, its shape kept (cropped at the sides or the top
/// and bottom, whichever the screen's shape leaves over).
#[derive(Component)]
struct CoverPicture;

fn cover(q: Query<&crate::layout::UiPicture, With<CoverPicture>>, mut pics: Query<(&ImageNode, &mut Node)>, images: Res<Assets<Image>>, windows: Query<&Window, With<bevy::window::PrimaryWindow>>) {
    let Ok(w) = windows.single() else { return };
    for p in &q {
        let Ok((img, mut node)) = pics.get_mut(p.0) else { continue };
        let Some(size) = images.get(&img.image).map(|i| i.size_f32()) else { continue };
        let (sw, sh) = (w.width(), w.height());
        let scale = (sw / size.x).max(sh / size.y);
        let (iw, ih) = (size.x * scale, size.y * scale);
        let want = (Val::Px(((sw - iw) * 0.5).round()), Val::Px(((sh - ih) * 0.5).round()), Val::Px(iw.round()), Val::Px(ih.round()));
        if (node.left, node.top, node.width, node.height) != want {
            (node.left, node.top, node.width, node.height) = want;
            node.position_type = PositionType::Absolute;
            node.right = Val::Auto;
            node.bottom = Val::Auto;
        }
    }
}
