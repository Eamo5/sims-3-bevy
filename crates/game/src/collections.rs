//! Buy mode's collections, as UI.dll's `BuyController` and its dialogs have them: the game's own
//! (the install's collection packages: each pack's things, the store sets) and the player's,
//! kept between games (`collections.json`, as the game keeps them in the player's own folder,
//! not in a save).
//!
//! - Create New Collection and Edit This Collection put up the game's `EditCollection` dialog: the
//!   name typed into its box (a click on it first; empty, its words ask for one) and one of the
//!   scrapbook's 36 pictures (`kCollectionIconNames`) for its icon; Accept needs a name.
//! - Delete This Collection asks first (the game's "Are you sure you wish to delete the
//!   collection...?").
//! - Organize Collections puts them in an order (`OrganizeCollectionsDialog`): each dragged to its
//!   place, before or after another as the insertion handle shows; Organize Contents orders a
//!   collection's objects the same way.
//! - The catalogue preview's Add to Collection (`BBCatalogPreviewPanelController`) puts up
//!   `EditCollections` to choose one of the player's collections (or make a new one) for the
//!   object in hand; Delete Item From Collection takes the one in hand out again.
//! - The game's own collections are as the install has them: they can be looked through and put
//!   in order, not changed.

use std::path::PathBuf;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use serde::{Deserialize, Serialize};

use crate::layout::{Spawned, UiAssets, UiButton};
use crate::livehud::{pressed, set_text, set_visible};
use crate::{AppState, PlayMode};

pub struct CollectionsPlugin;

impl Plugin for CollectionsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UserCollections::load())
            .init_resource::<CollDialogs>()
            .add_systems(PreUpdate, type_name.after(bevy::input::InputSystems).before(crate::buy::toggle_buy).before(crate::options::game_menu_key).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (collections_test, edit_dialog, choose_dialog, organize_dialog, deleted).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(AppState::InGame), |mut d: ResMut<CollDialogs>| *d = CollDialogs::default());
    }
}

/// The scrapbook's pictures a collection may have (`EditCollectionDialog.kCollectionIconNames`).
pub const ICONS: [&str; 36] = [
    "scrapbook_mem_around_town",
    "scrapbook_mem_books",
    "scrapbook_mem_career",
    "scrapbook_mem_cars",
    "scrapbook_mem_celebrity",
    "scrapbook_mem_clothes",
    "scrapbook_mem_collecting",
    "scrapbook_mem_dancing",
    "scrapbook_mem_death",
    "scrapbook_mem_eat_something",
    "scrapbook_mem_enemies",
    "scrapbook_mem_fight",
    "scrapbook_mem_fire",
    "scrapbook_mem_fitness",
    "scrapbook_mem_fun_objects",
    "scrapbook_mem_funny_fail",
    "scrapbook_mem_growing_up",
    "scrapbook_mem_household",
    "scrapbook_mem_marriage",
    "scrapbook_mem_money",
    "scrapbook_mem_moving",
    "scrapbook_mem_naughty",
    "scrapbook_mem_party",
    "scrapbook_mem_people",
    "scrapbook_mem_police",
    "scrapbook_mem_pregnancy",
    "scrapbook_mem_real_estate",
    "scrapbook_mem_romance",
    "scrapbook_mem_school",
    "scrapbook_mem_skill",
    "scrapbook_mem_social",
    "scrapbook_mem_tattoos",
    "scrapbook_mem_tombs",
    "scrapbook_mem_traits",
    "scrapbook_mem_travel",
    "scrapbook_mem_vampires",
];
/// A collection's name's letters at most.
pub const NAME_LENGTH: usize = 40;

/// One of the player's collections: its name, its picture (one of `ICONS`), its objects (OBJD
/// instances) in their order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UserCollection {
    pub id: u64,
    pub name: String,
    pub icon: String,
    pub objects: Vec<u64>,
}

/// A collection: one of the game's (by its icon's key, which is its own) or the player's.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CollRef {
    Game(u64),
    Mine(u64),
}

/// A collection's picture: the game's collection icon, or a scrapbook picture.
#[derive(Clone, Debug, PartialEq)]
pub enum CollIcon {
    Game(u64),
    Picture(String),
}

/// A collection as Buy mode lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub at: CollRef,
    pub name: String,
    pub icon: CollIcon,
    pub objects: Vec<u64>,
}

impl Shown {
    /// One of the player's own (theirs to change).
    pub fn mine(&self) -> Option<u64> {
        match self.at {
            CollRef::Mine(id) => Some(id),
            CollRef::Game(_) => None,
        }
    }

    /// Its picture, for the screen.
    pub fn image(&self, ui: &mut UiAssets, game_ui: Option<&mut crate::icons::GameUi>, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        icon_image(&self.icon, ui, game_ui, images)
    }
}

/// A collection's picture, for the screen.
pub fn icon_image(icon: &CollIcon, ui: &mut UiAssets, game_ui: Option<&mut crate::icons::GameUi>, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    match icon {
        CollIcon::Game(k) => game_ui.and_then(|g| g.icon(images, &s3bake::gamedata::collection_icon_name(*k))),
        CollIcon::Picture(name) => ui.image(images, s3pkg::fnv64(name)).map(|(h, _)| h),
    }
}

/// The player's collections and the order everything's in.
#[derive(Resource, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct UserCollections {
    #[serde(default)]
    pub mine: Vec<UserCollection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<CollRef>,
    #[serde(default)]
    pub next_id: u64,
}

impl UserCollections {
    fn path() -> PathBuf {
        std::env::var_os("SIMS3_COLLECTIONS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("collections.json"))
    }

    pub fn load() -> Self {
        std::fs::read(Self::path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(d) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(Self::path(), d);
        }
    }

    /// Every collection, in order: as organized, then the game's, then the player's.
    pub fn shown(&self, game: &[s3bake::gamedata::CollectionInfo]) -> Vec<Shown> {
        let all: Vec<Shown> = game
            .iter()
            .map(|c| Shown {
                at: CollRef::Game(c.icon),
                name: if c.name.is_empty() { c.name_key.rsplit(':').next().unwrap_or("").to_string() } else { c.name.clone() },
                icon: CollIcon::Game(c.icon),
                objects: c.objects.clone(),
            })
            .chain(self.mine.iter().map(|c| Shown { at: CollRef::Mine(c.id), name: c.name.clone(), icon: CollIcon::Picture(c.icon.clone()), objects: c.objects.clone() }))
            .collect();
        let mut out: Vec<Shown> = Vec::with_capacity(all.len());
        for r in &self.order {
            if let Some(s) = all.iter().find(|s| s.at == *r)
                && !out.iter().any(|o| o.at == *r)
            {
                out.push(s.clone());
            }
        }
        for s in all {
            if !out.iter().any(|o| o.at == s.at) {
                out.push(s);
            }
        }
        out
    }

    /// A new collection (`UserToolUtils.CreateNewCollection`): its id.
    pub fn create(&mut self, name: &str, icon: &str) -> u64 {
        self.next_id = self.next_id.max(self.mine.iter().map(|c| c.id).max().unwrap_or(0)) + 1;
        let id = self.next_id;
        self.mine.push(UserCollection { id, name: name.to_string(), icon: icon.to_string(), objects: Vec::new() });
        id
    }

    pub fn get(&self, id: u64) -> Option<&UserCollection> {
        self.mine.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut UserCollection> {
        self.mine.iter_mut().find(|c| c.id == id)
    }

    pub fn delete(&mut self, id: u64) {
        self.mine.retain(|c| c.id != id);
        self.order.retain(|r| *r != CollRef::Mine(id));
    }

    /// An object put in a collection (`UpdateItemInCollection`, adding): once.
    pub fn add(&mut self, id: u64, objd: u64) -> bool {
        match self.get_mut(id) {
            Some(c) if !c.objects.contains(&objd) => {
                c.objects.push(objd);
                true
            }
            _ => false,
        }
    }

    pub fn remove(&mut self, id: u64, objd: u64) -> bool {
        let Some(c) = self.get_mut(id) else { return false };
        let n = c.objects.len();
        c.objects.retain(|o| *o != objd);
        c.objects.len() != n
    }
}

/// Moves the item at `from` to before (or after) the one at `to`, as a drop in Organize does.
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize, before: bool) {
    if from >= items.len() || to >= items.len() || from == to {
        return;
    }
    let item = items.remove(from);
    let mut at = if before { to } else { to + 1 };
    if from < at {
        at -= 1;
    }
    items.insert(at.min(items.len()), item);
}

/// The collection dialogs up (one of each at most), and what Buy mode's to show after.
#[derive(Resource, Default)]
pub struct CollDialogs {
    edit: Option<Edit>,
    choose: Option<Choose>,
    organize: Option<Organize>,
    /// A collection just made from the Buy pane, to show.
    pub made: Option<u64>,
    /// The player's collection last chosen (`EditCollectionsDialog.sPreviousSelectedId`): the
    /// chooser's to start with.
    pub previous: Option<u64>,
}

/// `EditCollection`: a new collection (no id) or one of the player's; its name as typed, its
/// picture, whether keys go to the box, the pictures' first row in view.
struct Edit {
    id: Option<u64>,
    name: String,
    icon: usize,
    typing: bool,
    scroll: usize,
    /// For the chooser (`EditCollections`' New): the new one chosen there after.
    for_choose: bool,
    s: Option<Spawned>,
    cells: Option<Entity>,
    shown: Option<(String, bool, usize, usize)>,
}

/// `EditCollections`: the object being added, and the collection chosen for it.
struct Choose {
    objd: u64,
    chosen: Option<u64>,
    scroll: usize,
    s: Option<Spawned>,
    cells: Option<Entity>,
    shown: Option<(Vec<u64>, Option<u64>, usize)>,
}

/// `OrganizeCollections`: the collections (or one collection's objects) in their new order, the
/// one being dragged and where it'd go.
struct Organize {
    contents: Option<u64>,
    items: Vec<OrgItem>,
    scroll: usize,
    drag: Option<usize>,
    over: Option<(usize, bool)>,
    ghost: Option<Entity>,
    s: Option<Spawned>,
    cells: Option<Entity>,
    dirty: bool,
    /// Something's been moved (for the tests' COLL_ACCEPT).
    moved: bool,
}

#[derive(Clone, Debug)]
enum OrgItem {
    Coll(Shown),
    Object(u64),
}

impl CollDialogs {
    /// Whether one's up.
    pub fn up(&self) -> bool {
        self.edit.is_some() || self.choose.is_some() || self.organize.is_some()
    }

    /// Create New Collection.
    pub fn new_collection(&mut self) {
        self.edit = Some(Edit { id: None, name: String::new(), icon: 0, typing: false, scroll: 0, for_choose: false, s: None, cells: None, shown: None });
    }

    /// Edit This Collection (the player's).
    pub fn edit_collection(&mut self, c: &UserCollection) {
        let icon = ICONS.iter().position(|i| *i == c.icon).unwrap_or(0);
        self.edit = Some(Edit { id: Some(c.id), name: c.name.clone(), icon, typing: false, scroll: (icon / 6).saturating_sub(2), for_choose: false, s: None, cells: None, shown: None });
    }

    /// Add to Collection, for the object in hand (an OBJD instance).
    pub fn choose_for(&mut self, objd: u64, last: Option<u64>) {
        let chosen = last.or(self.previous);
        self.choose = Some(Choose { objd, chosen, scroll: 0, s: None, cells: None, shown: None });
    }

    /// Organize Collections (or Organize Contents, of one of the player's).
    pub fn organize(&mut self, contents: Option<u64>) {
        self.organize = Some(Organize { contents, items: Vec::new(), scroll: 0, drag: None, over: None, ghost: None, s: None, cells: None, dirty: true, moved: false });
    }
}

// `EditCollection`'s windows (`EditCollectionDialog.ControlIDs`).
const EDIT_GRID: u32 = 33;
const EDIT_OK: u32 = 34;
const EDIT_CANCEL: u32 = 35;
const EDIT_TEXT: u32 = 36;
const EDIT_SAVE: u32 = 0x32;
// `EditCollections`' and `OrganizeCollections`'.
const LIST_GRID: u32 = 1;
const LIST_OK: u32 = 2;
const LIST_CANCEL: u32 = 3;
const LIST_NEW: u32 = 6;
const LIST_TITLE: u32 = 0x05ef_6bd3;
/// An entry's picture, and Organize's insertion handles (left: before; right: after).
const ENTRY_PICTURE: u32 = 1;
const HANDLE_BEFORE: u32 = 0x066b_7e05;
const HANDLE_AFTER: u32 = 0x066b_7e06;
/// A picked cell's outline.
const PICKED: Color = Color::srgb(0.98, 0.84, 0.25);

/// An icon in `EditCollection`'s grid.
#[derive(Component, Clone, Copy)]
struct IconCell(usize);
/// A collection in `EditCollections`' grid.
#[derive(Component, Clone, Copy)]
struct ChooseCell(u64);
/// An item in Organize's grid, and its insertion handles.
#[derive(Component, Clone, Copy)]
struct OrgCell(usize, Option<Entity>, Option<Entity>);

/// A grid's cells' step and how many go across and down, and where the first is, from its layout
/// (the game's `ItemGrid`: each cell its `CellArea` and its padding, from its `GridPadding`).
pub(crate) fn grid_cells(w: &s3bake::ui::UiWindow) -> (Vec2, usize, usize, Vec2) {
    let g = w.grid.unwrap_or_default();
    let (cols, rows) = (g.columns.max(1) as usize, g.rows.max(1) as usize);
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]).max(Vec2::ONE);
    (step, cols, rows, Vec2::new(g.padding[0] + g.cell_padding[0], g.padding[1] + g.cell_padding[1]))
}

/// A grid's cell size, and its cells' colours (normal, picked) where it fills them.
fn grid_look(w: Option<&s3bake::ui::UiWindow>) -> (Vec2, Option<(Color, Color)>) {
    let g = w.and_then(|w| w.grid).unwrap_or_default();
    (Vec2::new(g.cell[0], g.cell[1]), g.cell_colors.map(|(a, b)| (crate::layout::color(a), crate::layout::color(b))))
}

/// A cell put in a grid's holder from an entry layout (a button), its picture in it.
#[allow(clippy::too_many_arguments)]
fn cell(commands: &mut Commands, ui: &mut UiAssets, (images, fonts): (&mut Assets<Image>, &mut Assets<Font>), holder: Entity, layout: &str, at: Vec2, size: Vec2, picture: Option<Handle<Image>>) -> Option<Spawned> {
    let mut w = ui.layout(layout)?.clone();
    w.cls = "Button".to_string();
    w.place = s3bake::ui::UiPlace::Fixed;
    w.area = [at.x, at.y, at.x + size.x, at.y + size.y];
    let c = ui.spawn_under(commands, images, fonts, &w, holder);
    let r = c.root?;
    // (Its picture in place of the entry's own: the layout's stand-in.)
    let win = c.id(ENTRY_PICTURE).filter(|e| *e != r).unwrap_or(r);
    if win == r {
        commands.entity(r).remove::<ImageNode>();
    }
    match picture {
        Some(h) => crate::hudpanels::picture(commands, win, h, Color::WHITE),
        None => {
            commands.entity(win).despawn_children();
        }
    }
    for id in [HANDLE_BEFORE, HANDLE_AFTER] {
        if let Some(e) = c.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    Some(c)
}

/// A cell lit as the one picked (or not): in the grid's own colours where it has them (an
/// `ItemGrid`'s `BackgroundColor` and `SelectionColor`), else outlined.
fn outline(commands: &mut Commands, e: Entity, on: bool, colors: Option<(Color, Color)>) {
    match colors {
        Some((bg, picked)) => {
            commands.entity(e).insert(BackgroundColor(if on { picked } else { bg }));
        }
        None if on => {
            commands.entity(e).insert(Outline::new(Val::Px(2.0), Val::ZERO, PICKED));
        }
        None => {
            commands.entity(e).remove::<Outline>();
        }
    }
}

/// Words a dialog's text shows (gendered words aside), the game's or these.
fn word(ui: &UiAssets, key: u64, or: &str) -> String {
    ui.localize_key(key).filter(|s| !s.is_empty()).unwrap_or_else(|| or.to_string())
}

/// Keys go to `EditCollection`'s box while it's typed in (and to nothing else: the world's keys
/// wait): letters, Backspace; Enter, Escape or Tab keep what's typed.
fn type_name(mut d: ResMut<CollDialogs>, mut typed: MessageReader<KeyboardInput>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    let Some(e) = d.edit.as_mut().filter(|e| e.typing) else {
        return;
    };
    for ev in typed.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        match &ev.logical_key {
            Key::Character(c) if e.name.chars().count() < NAME_LENGTH => e.name.push_str(c.as_str()),
            Key::Space if e.name.chars().count() < NAME_LENGTH => e.name.push(' '),
            Key::Backspace => {
                e.name.pop();
            }
            Key::Enter | Key::Escape | Key::Tab => e.typing = false,
            _ => {}
        }
    }
    keys.reset_all();
}

/// The game's `EditCollection` dialog.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn edit_dialog(
    mut commands: Commands,
    mut d: ResMut<CollDialogs>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut user: ResMut<UserCollections>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    icons: Query<(&Interaction, &IconCell), Changed<Interaction>>,
    (mut buttons, mut vis, mut texts, mut colors): (Query<&mut UiButton>, Query<&mut Visibility>, Query<&mut Text>, Query<&mut TextColor>),
    (mouse, mut wheel): (Res<ButtonInput<MouseButton>>, MessageReader<bevy::input::mouse::MouseWheel>),
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let scrolled: f32 = wheel.read().map(|w| w.y.signum()).sum();
    let Some(mut ui) = ui else { return };
    let close = |commands: &mut Commands, d: &mut CollDialogs| {
        if let Some(r) = d.edit.take().and_then(|e| e.s).and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
    };
    let Some(e) = d.edit.as_mut() else { return };
    if e.s.is_none() {
        let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "EditCollection") else {
            warn!("no EditCollection layout");
            d.edit = None;
            return;
        };
        if let Some(r) = s.root {
            commands.entity(r).insert(GlobalZIndex(23));
        }
        if let Some(b) = s.id(EDIT_TEXT) {
            // (Over its box's picture, which comes first among the dialog's windows.)
            commands.entity(b).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld, Visibility::Inherited, ZIndex(2)));
        }
        // (Its Save is the game's sharing: not here.)
        set_visible(&mut vis, s.id(EDIT_SAVE), false);
        for (id, tip) in [(EDIT_OK, ui.localize("Ui/Caption/Global:Accept").unwrap_or_else(|| "Accept".into())), (EDIT_CANCEL, ui.localize("Ui/Caption/Global:Cancel").unwrap_or_else(|| "Cancel".into()))] {
            if let Some(b) = s.id(id) {
                commands.entity(b).insert(crate::icons::Tooltip(tip));
            }
        }
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
        e.s = Some(s);
        return;
    }
    let Some(s) = e.s.clone() else { return };
    if pressed(&clicks, s.id(EDIT_CANCEL)) {
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        return;
    }
    let name = e.name.trim().to_string();
    if pressed(&clicks, s.id(EDIT_OK)) && !name.is_empty() {
        let icon = ICONS[e.icon.min(ICONS.len() - 1)];
        let (made, for_choose) = match e.id {
            Some(id) => {
                if let Some(c) = user.get_mut(id) {
                    c.name = name.clone();
                    c.icon = icon.to_string();
                }
                (None, false)
            }
            None => (Some(user.create(&name, icon)), e.for_choose),
        };
        user.save();
        info!("collection {name} ({icon}) kept");
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        if let Some(id) = made {
            if for_choose {
                if let Some(c) = d.choose.as_mut() {
                    c.chosen = Some(id);
                }
            } else {
                d.made = Some(id);
            }
        }
        return;
    }
    // (Typing: a click on the box; a click elsewhere keeps what's typed.)
    if pressed(&clicks, s.id(EDIT_TEXT)) {
        e.typing = true;
    } else if e.typing && mouse.just_pressed(MouseButton::Left) && !s.id(EDIT_TEXT).is_some_and(|b| clicks.get(b).is_ok_and(|(_, i)| *i == Interaction::Pressed)) {
        e.typing = false;
    }
    for (i, c) in &icons {
        if *i == Interaction::Pressed {
            e.icon = c.0;
            play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
        }
    }
    let grid_w = ui.find("EditCollection", EDIT_GRID).cloned();
    let (step, cols, rows, pad) = grid_w.as_ref().map_or((Vec2::splat(40.0), 6, 3, Vec2::ZERO), grid_cells);
    let (cell_size, cell_colors) = grid_look(grid_w.as_ref());
    let most = ICONS.len().div_ceil(cols).saturating_sub(rows);
    if scrolled != 0.0 {
        e.scroll = if scrolled > 0.0 { e.scroll.saturating_sub(1) } else { (e.scroll + 1).min(most) };
    }
    e.scroll = e.scroll.min(most);
    if let Some(b) = s.id(EDIT_OK)
        && let Ok(mut b) = buttons.get_mut(b)
        && b.disabled != name.is_empty()
    {
        b.disabled = name.is_empty();
    }
    // The box: the name (as it's typed), or the game's words asking for one.
    let caret = e.typing && ((mouse.pressed(MouseButton::Left) as u8) == 0);
    let now = (e.name.clone(), caret, e.icon, e.scroll);
    if e.shown.as_ref() == Some(&now) {
        return;
    }
    e.shown = Some(now);
    let placeholder = e.name.is_empty() && !e.typing;
    let words = if placeholder { ui.localize("Ui/Caption/Collections:EnterNewCollectionName").unwrap_or_else(|| "Enter new Collection Name".into()) } else if e.typing { format!("{}|", e.name) } else { e.name.clone() };
    set_text(&mut texts, s.text(EDIT_TEXT), &words);
    if let Some(t) = s.text(EDIT_TEXT)
        && let Ok(mut c) = colors.get_mut(t)
    {
        c.0 = if placeholder { Color::srgb(0.45, 0.45, 0.5) } else { Color::srgb(0.08, 0.1, 0.2) };
    }
    // The pictures.
    let Some(grid) = s.id(EDIT_GRID) else { return };
    commands.entity(grid).remove::<Pickable>().insert(crate::hud::BlocksWorld);
    let holder = crate::buyhud::holder(&mut commands, grid, &mut e.cells, &vis);
    for (i, name) in ICONS.iter().enumerate().skip(e.scroll * cols).take(cols * rows) {
        let n = i - e.scroll * cols;
        let at = pad + Vec2::new((n % cols) as f32 * step.x, (n / cols) as f32 * step.y);
        let pic = ui.image(&mut images, s3pkg::fnv64(name)).map(|(h, _)| h);
        // (Stretched to the cell, as the grid's `StretchCellWindows` has it.)
        let Some(c) = cell(&mut commands, &mut ui, (&mut images, &mut fonts), holder, "EditCollectionIconPickerEntry", at, cell_size, pic) else { continue };
        if let Some(r) = c.root {
            commands.entity(r).insert(IconCell(i));
            outline(&mut commands, r, i == e.icon, cell_colors);
        }
    }
}

/// The game's `EditCollections` dialog: which of the player's collections the object in hand goes
/// in (or a new one).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn choose_dialog(
    mut commands: Commands,
    mut d: ResMut<CollDialogs>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut user: ResMut<UserCollections>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    picks: Query<(&Interaction, &ChooseCell), Changed<Interaction>>,
    (mut buttons, vis): (Query<&mut UiButton>, Query<&mut Visibility>),
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let scrolled: f32 = wheel.read().map(|w| w.y.signum()).sum();
    let Some(mut ui) = ui else { return };
    if d.choose.is_none() {
        return;
    }
    // (Its New's dialog over it, it waits.)
    let editing = d.edit.is_some();
    let close = |commands: &mut Commands, d: &mut CollDialogs| {
        if let Some(r) = d.choose.take().and_then(|c| c.s).and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
    };
    let Some(c) = d.choose.as_mut() else { return };
    if c.s.is_none() {
        let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "EditCollections") else {
            warn!("no EditCollections layout");
            d.choose = None;
            return;
        };
        // (Its title is the layout's own; its Cancel shown, as `showCancelButton` has it.)
        if let Some(b) = s.id(LIST_CANCEL) {
            commands.entity(b).insert(Visibility::Inherited);
        }
        for (id, tip) in [(LIST_NEW, word(&ui, 0x4b9f_4a53_2c65_cd45, "Create New Collection")), (LIST_OK, ui.localize("Ui/Caption/Global:Accept").unwrap_or_else(|| "Accept".into())), (LIST_CANCEL, ui.localize("Ui/Caption/Global:Cancel").unwrap_or_else(|| "Cancel".into()))] {
            if let Some(b) = s.id(id) {
                commands.entity(b).insert(crate::icons::Tooltip(tip));
            }
        }
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
        c.s = Some(s);
        return;
    }
    let Some(s) = c.s.clone() else { return };
    if editing {
        return;
    }
    if pressed(&clicks, s.id(LIST_CANCEL)) {
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        return;
    }
    if pressed(&clicks, s.id(LIST_NEW)) {
        d.new_collection();
        if let Some(e) = d.edit.as_mut() {
            e.for_choose = true;
        }
        return;
    }
    // (COLL_ACCEPT=1 (tests): taken as soon as there's one chosen.)
    let auto = std::env::var("COLL_ACCEPT").is_ok() && c.shown.is_some();
    if (pressed(&clicks, s.id(LIST_OK)) || auto)
        && let Some(id) = c.chosen
    {
        let objd = c.objd;
        d.previous = Some(id);
        if user.add(id, objd) {
            user.save();
            info!("collection {id}: object {objd:016x} added");
        }
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        return;
    }
    for (i, p) in &picks {
        if *i == Interaction::Pressed {
            c.chosen = Some(p.0);
            play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
        }
    }
    let ids: Vec<u64> = user.shown(&[]).iter().filter_map(|s| s.mine()).collect();
    if c.chosen.is_some_and(|x| !ids.contains(&x)) {
        c.chosen = None;
    }
    if let Some(b) = s.id(LIST_OK)
        && let Ok(mut b) = buttons.get_mut(b)
        && b.disabled != c.chosen.is_none()
    {
        b.disabled = c.chosen.is_none();
    }
    let grid_w = ui.find("EditCollections", LIST_GRID).cloned();
    let (step, cols, rows, pad) = grid_w.as_ref().map_or((Vec2::splat(54.0), 6, 3, Vec2::ZERO), grid_cells);
    let (cell_size, colors) = grid_look(grid_w.as_ref());
    let most = ids.len().div_ceil(cols).saturating_sub(rows);
    if scrolled != 0.0 {
        c.scroll = if scrolled > 0.0 { c.scroll.saturating_sub(1) } else { (c.scroll + 1).min(most) };
    }
    c.scroll = c.scroll.min(most);
    let now = (ids.clone(), c.chosen, c.scroll);
    if c.shown.as_ref() == Some(&now) {
        return;
    }
    c.shown = Some(now);
    let Some(grid) = s.id(LIST_GRID) else { return };
    commands.entity(grid).remove::<Pickable>().insert(crate::hud::BlocksWorld);
    let holder = crate::buyhud::holder(&mut commands, grid, &mut c.cells, &vis);
    let shown = user.bypass_change_detection().shown(&[]);
    for (n, sh) in shown.iter().filter(|s| s.mine().is_some()).enumerate().skip(c.scroll * cols).take(cols * rows) {
        let k = n - c.scroll * cols;
        let at = pad + Vec2::new((k % cols) as f32 * step.x, (k / cols) as f32 * step.y);
        let pic = sh.image(&mut ui, None, &mut images);
        let Some(cl) = cell(&mut commands, &mut ui, (&mut images, &mut fonts), holder, "CollectionsPickerEntry", at, cell_size, pic) else { continue };
        let (Some(r), Some(id)) = (cl.root, sh.mine()) else { continue };
        commands.entity(r).insert((ChooseCell(id), crate::icons::Tooltip(sh.name.clone())));
        outline(&mut commands, r, c.chosen == Some(id), colors);
    }
}

/// The game's `OrganizeCollections` dialog: the collections (or a collection's objects) dragged
/// into order.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn organize_dialog(
    mut commands: Commands,
    mut d: ResMut<CollDialogs>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (mut user, mut game_ui, catalog, mut thumbs): (ResMut<UserCollections>, Option<ResMut<crate::icons::GameUi>>, Res<crate::loading::Catalog>, ResMut<crate::thumbs::ModelThumbs>),
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    org_cells: Query<(Entity, &Interaction, &OrgCell, &bevy::ui::RelativeCursorPosition)>,
    (vis, mut nodes): (Query<&mut Visibility>, Query<&mut Node>),
    (mouse, windows, mut wheel): (Res<ButtonInput<MouseButton>>, Query<&Window, With<PrimaryWindow>>, MessageReader<bevy::input::mouse::MouseWheel>),
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let scrolled: f32 = wheel.read().map(|w| w.y.signum()).sum();
    let Some(mut ui) = ui else { return };
    let close = |commands: &mut Commands, d: &mut CollDialogs| {
        if let Some(o) = d.organize.take() {
            for e in o.s.and_then(|s| s.root).into_iter().chain(o.ghost) {
                commands.entity(e).try_despawn();
            }
        }
    };
    let Some(o) = d.organize.as_mut() else { return };
    let game: Vec<s3bake::gamedata::CollectionInfo> = game_ui.as_deref().map(|g| g.data.collections.clone()).unwrap_or_default();
    if o.s.is_none() {
        let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "OrganizeCollections") else {
            warn!("no OrganizeCollections layout");
            d.organize = None;
            return;
        };
        o.items = match o.contents {
            Some(id) => user.get(id).map(|c| c.objects.iter().map(|x| OrgItem::Object(*x)).collect()).unwrap_or_default(),
            None => user.shown(&game).into_iter().map(OrgItem::Coll).collect(),
        };
        // (A collection's contents: their own title; the collections, the layout's.)
        if o.contents.is_some()
            && let Some(t) = s.text(LIST_TITLE)
        {
            let title = ui.localize("Ui/Tooltip/BuildBuy/CatalogProductFilter:OrganizeCollectionContents").unwrap_or_else(|| "Organize Contents".into());
            commands.entity(t).insert(Text::new(title));
        }
        for (id, tip) in [(LIST_OK, ui.localize("Ui/Caption/Global:Accept").unwrap_or_else(|| "Accept".into())), (LIST_CANCEL, ui.localize("Ui/Caption/Global:Cancel").unwrap_or_else(|| "Cancel".into()))] {
            if let Some(b) = s.id(id) {
                commands.entity(b).insert((crate::icons::Tooltip(tip), Visibility::Inherited));
            }
        }
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
        o.s = Some(s);
        return;
    }
    let Some(s) = o.s.clone() else { return };
    if pressed(&clicks, s.id(LIST_CANCEL)) {
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        return;
    }
    if pressed(&clicks, s.id(LIST_OK)) || (o.moved && o.drag.is_none() && std::env::var("COLL_ACCEPT").is_ok()) {
        match o.contents {
            Some(id) => {
                let order: Vec<u64> = o.items.iter().filter_map(|i| if let OrgItem::Object(x) = i { Some(*x) } else { None }).collect();
                if let Some(c) = user.get_mut(id) {
                    c.objects = order;
                }
            }
            None => user.order = o.items.iter().filter_map(|i| if let OrgItem::Coll(c) = i { Some(c.at) } else { None }).collect(),
        }
        user.save();
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        return;
    }
    let grid_w = ui.find("OrganizeCollections", LIST_GRID).cloned();
    let (step, cols, rows, pad) = grid_w.as_ref().map_or((Vec2::splat(54.0), 7, 4, Vec2::ZERO), grid_cells);
    let (cell_size, colors) = grid_look(grid_w.as_ref());
    let most = o.items.len().div_ceil(cols).saturating_sub(rows);
    if scrolled != 0.0 {
        let to = if scrolled > 0.0 { o.scroll.saturating_sub(1) } else { (o.scroll + 1).min(most) };
        o.dirty |= to != o.scroll;
        o.scroll = to;
    }
    // Dragging: picked up on a press, its picture under the pointer; let go over another, put
    // before or after it (by which half), the handle showing which.
    if o.drag.is_none()
        && let Some((_, _, c, _)) = org_cells.iter().find(|(_, i, ..)| **i == Interaction::Pressed)
        && mouse.just_pressed(MouseButton::Left)
    {
        o.drag = Some(c.0);
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
        let pic = match o.items.get(c.0) {
            Some(OrgItem::Coll(sh)) => sh.image(&mut ui, game_ui.as_deref_mut(), &mut images),
            Some(OrgItem::Object(x)) => catalog.entries.iter().find(|e| e.key.2 == *x).and_then(|e| crate::thumbs::catalogue_thumb(game_ui.as_deref_mut(), &mut thumbs, &mut images, e.key)),
            None => None,
        };
        if let Some(h) = pic {
            o.ghost = Some(
                commands
                    .spawn((ImageNode { image: h, color: Color::srgba(1.0, 1.0, 1.0, 0.8), ..default() }, Node { position_type: PositionType::Absolute, width: Val::Px(40.0), height: Val::Px(40.0), ..default() }, GlobalZIndex(40), Pickable::IGNORE, DespawnOnExit(AppState::InGame)))
                    .id(),
            );
        }
    }
    if let Some(from) = o.drag {
        if let (Some(g), Some(p)) = (o.ghost, windows.single().ok().and_then(|w| w.cursor_position()))
            && let Ok(mut n) = nodes.get_mut(g)
        {
            n.left = Val::Px(p.x - 20.0);
            n.top = Val::Px(p.y - 22.0);
        }
        let over = org_cells.iter().find(|(.., r)| r.cursor_over()).map(|(_, _, c, r)| (c.0, r.normalized.is_none_or(|v| v.x < 0.0)));
        if over != o.over {
            o.over = over;
            for (_, _, c, _) in &org_cells {
                for (h, on) in [(c.1, over == Some((c.0, true))), (c.2, over == Some((c.0, false)))] {
                    if let Some(h) = h {
                        commands.entity(h).insert(if on && c.0 != from { Visibility::Inherited } else { Visibility::Hidden });
                    }
                }
            }
        }
        if !mouse.pressed(MouseButton::Left) {
            if let Some((to, before)) = o.over {
                move_item(&mut o.items, from, to, before);
                info!("organize: {from} put {} {to}", if before { "before" } else { "after" });
                o.dirty = true;
                o.moved = true;
            }
            o.drag = None;
            o.over = None;
            if let Some(g) = o.ghost.take() {
                commands.entity(g).try_despawn();
            }
        }
    }
    if !o.dirty {
        return;
    }
    o.dirty = false;
    o.scroll = o.scroll.min(most);
    let Some(grid) = s.id(LIST_GRID) else { return };
    commands.entity(grid).remove::<Pickable>().insert(crate::hud::BlocksWorld);
    let holder = crate::buyhud::holder(&mut commands, grid, &mut o.cells, &vis);
    let layout = if o.contents.is_some() { "OrganizeCollectionContentsEntry" } else { "OrganizeCollectionEntry" };
    for (i, item) in o.items.clone().iter().enumerate().skip(o.scroll * cols).take(cols * rows) {
        let k = i - o.scroll * cols;
        let at = pad + Vec2::new((k % cols) as f32 * step.x, (k / cols) as f32 * step.y);
        let (pic, tip) = match item {
            OrgItem::Coll(sh) => (sh.image(&mut ui, game_ui.as_deref_mut(), &mut images), sh.name.clone()),
            OrgItem::Object(x) => {
                let e = catalog.entries.iter().find(|e| e.key.2 == *x);
                (e.and_then(|e| crate::thumbs::catalogue_thumb(game_ui.as_deref_mut(), &mut thumbs, &mut images, e.key)), e.map_or_else(String::new, |e| e.name.clone()))
            }
        };
        let Some(c) = cell(&mut commands, &mut ui, (&mut images, &mut fonts), holder, layout, at, cell_size, pic) else { continue };
        if let Some(r) = c.root {
            outline(&mut commands, r, false, colors);
            commands.entity(r).insert((OrgCell(i, c.id(HANDLE_BEFORE), c.id(HANDLE_AFTER)), crate::icons::Tooltip(tip), bevy::ui::RelativeCursorPosition::default()));
        }
    }
}

/// Delete This Collection, answered yes.
fn deleted(mut answers: MessageReader<crate::dialog::Answered>, mut user: ResMut<UserCollections>) {
    for a in answers.read() {
        if let crate::dialog::Question::DeleteCollection { id } = a.about
            && a.answer == 0
        {
            info!("collection {id} deleted");
            user.delete(id);
            user.save();
        }
    }
}

/// COLL_TEST=<step>[;<step>...] (tests), ten seconds in: `new:<name>:<icon>` makes a collection,
/// `add:<object's instance name>` puts an object in the last made; COLL_DIALOG=new|edit|choose:<instance
/// name>|organize|contents opens that dialog (on the last made) twelve seconds in, COLL_TYPE=<words>
/// typed into its box.
fn collections_test(time: Res<Time>, mut user: ResMut<UserCollections>, mut d: ResMut<CollDialogs>, baked: Res<crate::baked::Baked>, mut done: Local<(bool, bool)>) {
    // (COLL_TYPE: typed into an Edit dialog once it's up.)
    if let (Ok(t), Some(e)) = (std::env::var("COLL_TYPE"), d.edit.as_mut())
        && e.name.is_empty()
        && e.s.is_some()
    {
        e.name = t;
    }
    if !done.0 && time.elapsed_secs() > 10.0 {
        done.0 = true;
        if let Ok(v) = std::env::var("COLL_TEST") {
            for step in v.split(';') {
                let mut parts = step.split(':');
                match (parts.next(), parts.next(), parts.next()) {
                    (Some("new"), Some(name), icon) => {
                        let icon = icon.and_then(|i| i.parse::<usize>().ok()).unwrap_or(0).min(ICONS.len() - 1);
                        let id = user.create(name, ICONS[icon]);
                        info!("collection test: made {name} ({id})");
                    }
                    (Some("add"), Some(name), _) => {
                        let id = user.mine.last().map(|c| c.id);
                        match (id, baked.0.catalog.iter().find(|e| e.instance_name.eq_ignore_ascii_case(name))) {
                            (Some(id), Some(e)) => {
                                user.add(id, e.objd.2);
                                info!("collection test: {} added", e.name);
                            }
                            _ => warn!("collection test: can't add {name}"),
                        }
                    }
                    _ => warn!("collection test: {step}?"),
                }
            }
            user.save();
        }
    }
    if !done.1 && time.elapsed_secs() > 12.0 {
        done.1 = true;
        let Ok(v) = std::env::var("COLL_DIALOG") else { return };
        let last = user.mine.last().cloned();
        let (what, arg) = v.split_once(':').unwrap_or((v.as_str(), ""));
        match what {
            "new" => d.new_collection(),
            "edit" => {
                if let Some(c) = &last {
                    d.edit_collection(c);
                }
            }
            "choose" => {
                if let Some(e) = baked.0.catalog.iter().find(|e| e.instance_name.eq_ignore_ascii_case(arg)) {
                    d.choose_for(e.objd.2, last.as_ref().map(|c| c.id));
                }
            }
            "organize" => d.organize(None),
            "contents" => d.organize(last.as_ref().map(|c| c.id)),
            _ => {}
        }
        info!("collection test: {what} dialog up");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_are_listed_in_order_and_kept_as_made() {
        let game = vec![
            s3bake::gamedata::CollectionInfo { name_key: "a".into(), name: "Seaside".into(), icon: 1, objects: vec![10] },
            s3bake::gamedata::CollectionInfo { name_key: "b".into(), name: "Eco".into(), icon: 2, objects: vec![] },
        ];
        let mut u = UserCollections::default();
        let couches = u.create("Couches", ICONS[3]);
        let lamps = u.create("Lamps", ICONS[5]);
        assert_ne!(couches, lamps);
        assert!(u.add(couches, 7) && !u.add(couches, 7), "an object goes in once");
        let names = |u: &UserCollections| u.shown(&game).iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&u), ["Seaside", "Eco", "Couches", "Lamps"]);
        // (Organized: as put; the rest after.)
        u.order = vec![CollRef::Mine(lamps), CollRef::Game(2)];
        assert_eq!(names(&u), ["Lamps", "Eco", "Seaside", "Couches"]);
        u.delete(lamps);
        assert_eq!(names(&u), ["Eco", "Seaside", "Couches"]);
        assert!(u.remove(couches, 7) && u.get(couches).unwrap().objects.is_empty());
        let made = u.create("Beds", ICONS[0]);
        assert!(made > couches, "a new id each time");
    }

    #[test]
    fn a_drop_puts_it_before_or_after_the_one_its_on() {
        let mut v = vec!['a', 'b', 'c', 'd'];
        move_item(&mut v, 0, 2, false);
        assert_eq!(v, ['b', 'c', 'a', 'd']);
        move_item(&mut v, 3, 0, true);
        assert_eq!(v, ['d', 'b', 'c', 'a']);
        move_item(&mut v, 1, 2, true);
        assert_eq!(v, ['d', 'b', 'c', 'a'], "before the next is where it was");
    }
}
