//! Create a Sim's Personality in the game's own panels, as UI.dll's `CASCharacter` and
//! `TraitsPickerDialog` show it. The character panel: its Traits and Dreams page (the Sim's
//! traits in their sockets, randomize, add or change traits, and the lifetime wishes that suit
//! them with the chosen one's words) and its Favorites and Voice page (favourite food, music and
//! colour, each opening the game's grid of them; the three voices). The traits picker: the
//! traits taken and those to take (by the game's groups), a click choosing one and a second
//! adding or taking it away (or Add and Erase), randomize, and accept or cancel.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::AppState;
use crate::cas::{CasAction, CasTab};
use crate::home::PendingHousehold;
use crate::layout::{SetIcon, Spawned, UiAssets, UiButton, UiScrollBar, edit_windows};
use crate::life::Trait;
use crate::sim::Age;

pub struct CasCharPlugin;

impl Plugin for CasCharPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_character, character_panel, traits_picker).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<CasCharacter>();
            CHARACTER_UP.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the game's character panel is up (see `caslook::game_panel`).
pub static CHARACTER_UP: AtomicBool = AtomicBool::new(false);

/// For the UI flow (`autotest`): 1 opens the traits picker, 2 the favourites page with the
/// foods open, 3 the traits page again.
pub static TEST_STEP: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

// `CASCharacter`'s windows.
const TAB_TRAITS: u32 = 0x1000;
const TAB_FAVORITES: u32 = 0x1001;
const TAB_SIGNS: u32 = 0x1002;
const CLOSE: u32 = 0x05db_9900;
const TRAITS_PAGE: u32 = 0x05db_8000;
const FAVORITES_PAGE: u32 = 0x05db_8001;
const SIGNS_PAGE: u32 = 0x05db_8002;
const RANDOM_TRAITS: u32 = 0xaaaa_aaaa;
const ADD_TRAITS: u32 = 0x05e5_3401;
const TRAIT_SLOT: u32 = 0x0500_1001;
const WISHES: u32 = 0x0700_0000;
const WISH_SLOT: u32 = 0x0700_0010;
const CUSTOM_WISH: u32 = 0x0700_0020;
const WISH_NAME: u32 = 0x0700_0002;
const WISH_REQUIRED: u32 = 0x0700_0004;
const WISH_DESCRIPTION: u32 = 0x0700_0003;
/// Favourite food, music and colour: their buttons, the windows they open, and those windows'
/// grids.
const FAVORITE_BUTTONS: [u32; 3] = [0x0600_0000, 0x0600_0004, 0x0600_0008];
const FAVORITE_WINDOWS: [u32; 3] = [0x0600_0001, 0x0600_0005, 0x0600_0009];
const FAVORITE_GRIDS: [u32; 3] = [0x0600_0002, 0x0600_0006, 0x0600_000a];
const RANDOM_FAVORITES: u32 = 0x0600_000c;
const VOICES: [u32; 3] = [0x0800_0001, 0x0800_0002, 0x0800_0003];
/// What this game hasn't: the voice's pitch and the biography.
const NOT_HERE: [u32; 5] = [0x0800_0004, 0x4000, 0x4001, CLOSE, CUSTOM_WISH];
const CELL_THUMBNAIL: u32 = 0x20;
// `TraitsPicker`'s.
const PICK_CURRENT_BG: u32 = 0x06e0_a002;
const PICK_CURRENT: u32 = 4;
const PICK_AVAILABLE: u32 = 7;
const PICK_FILTERS: [u32; 5] = [8, 9, 0xb, 0xa, 0xe];
const PICK_ACCEPT: u32 = 0xd;
const PICK_CANCEL: u32 = 0xc;
const PICK_ADD: u32 = 6;
const PICK_ERASE: u32 = 5;
const PICK_RANDOM: u32 = 0x10;
const PICK_THUMB: u32 = 0xf;
const PICK_NAME: u32 = 2;
// `CASTraitEntry`'s.
const ENTRY_TEXT: u32 = 0x22;
const ENTRY_ICON: u32 = 0x20;
const AVAILABLE_ROWS: usize = 6;
// The item grid's own (layout `ItemGrid`): its scroll bar.
const GRID_SCROLLBAR: u32 = 0x0582_0672;

/// The panel on screen, its page, the favourites open, and what it was shown for; the traits
/// picker when it's up.
#[derive(Resource)]
pub struct CasCharacter {
    s: Spawned,
    favorites_page: bool,
    open: Option<usize>,
    cells: Option<Entity>,
    shown: Option<Shown>,
    picker: Option<Picker>,
}

#[derive(PartialEq, Clone)]
struct Shown {
    sim: usize,
    age: Age,
    traits: Vec<Trait>,
    wish: Option<usize>,
    favorites: (String, String, String),
    voice: u8,
    page: bool,
    open: Option<usize>,
    icons: bool,
}

/// The traits picker: the traits as they're being chosen (taken on Accept), the group shown,
/// the row chosen (taken or not) and when it was clicked, and the rows in view.
struct Picker {
    s: Spawned,
    traits: Vec<Trait>,
    group: usize,
    chosen: Option<(bool, Trait)>,
    clicked: Option<(bool, Trait, f32)>,
    scroll: usize,
    current: Option<Entity>,
    available: Option<Entity>,
    bar: Option<Entity>,
    dirty: bool,
}

/// A lifetime wish's slot (of `lifetime::LIFETIME_WISHES`).
#[derive(Component)]
struct WishSlot(usize);

/// A favourite's cell: which (food, music, colour) and its entry.
#[derive(Component)]
struct FavoriteCell(u8, usize);

/// A trait's row in the picker: taken or not, and the trait.
#[derive(Component)]
struct TraitRow(bool, Trait);

fn spawn_character(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<crate::caslook::CasFrame>>, character: Option<Res<CasCharacter>>) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if character.is_some() || ui.layout("CASCharacter").is_none() || ui.layout("TraitsPicker").is_none() || ui.layout("CASTraitEntry").is_none() {
        return;
    }
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "CASCharacter") else { return };
    if let Some(r) = s.root {
        commands.entity(r).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(5), Visibility::Hidden));
    }
    for id in NOT_HERE.into_iter().chain([TAB_SIGNS, SIGNS_PAGE]) {
        for e in s.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for (n, id) in VOICES.iter().enumerate() {
        if let Some(e) = s.id(*id) {
            commands.entity(e).insert((CasAction::Voice(n as u8), crate::icons::Tooltip(format!("Voice {}", n + 1))));
        }
    }
    for (id, tip) in [(TAB_TRAITS, "Traits and Lifetime Wish"), (TAB_FAVORITES, "Favorites and Voice"), (RANDOM_TRAITS, "Random Traits"), (RANDOM_FAVORITES, "Random Favorites"), (ADD_TRAITS, "Add or Change Traits")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    CHARACTER_UP.store(true, Ordering::Relaxed);
    commands.insert_resource(CasCharacter { s, favorites_page: false, open: None, cells: None, shown: None, picker: None });
}

/// Shown on the Personality tab: the page chosen, the Sim's traits in their sockets, the
/// lifetime wishes that suit them (the chosen one lit and described), the favourites and the
/// voice; clicks open the traits picker, choose wishes and favourites, and randomize.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn character_panel(
    mut commands: Commands,
    character: Option<ResMut<CasCharacter>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    chosen: Res<crate::lifetime::ChosenLifetimeWishes>,
    (mut vis, mut buttons, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut Text>),
    (clicks, wishes, favorites): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &WishSlot), Changed<Interaction>>, Query<(&Interaction, &FavoriteCell), Changed<Interaction>>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut c), Some(mut ui)) = (character, ui) else { return };
    let c = &mut *c;
    let on = selected.1 == CasTab::Traits;
    crate::livehud::set_visible(&mut vis, c.s.root, on);
    if !on {
        c.open = None;
        return;
    }
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k) else { return };
    let pressed = |id: u32| crate::livehud::pressed(&clicks, c.s.id(id));
    // The pages.
    if pressed(TAB_TRAITS) {
        c.favorites_page = false;
    }
    if pressed(TAB_FAVORITES) {
        c.favorites_page = true;
    }
    match TEST_STEP.swap(0, Ordering::Relaxed) {
        1 if c.picker.is_none() => c.picker = open_picker(&mut commands, &mut ui, &mut images, &mut fonts, sim),
        2 => {
            if let Some(p) = c.picker.take()
                && let Some(r) = p.s.root
            {
                commands.entity(r).despawn();
            }
            c.favorites_page = true;
            c.open = Some(0);
        }
        3 => {
            c.favorites_page = false;
            c.open = None;
        }
        _ => {}
    }
    // Traits: randomized, or the picker opened (from the button or a socket).
    if pressed(RANDOM_TRAITS) {
        actions.write(crate::cas::CasActionRequest(CasAction::RandomTraits));
    }
    if c.picker.is_none() && (pressed(ADD_TRAITS) || (0..5).any(|i| pressed(TRAIT_SLOT + i))) {
        c.picker = open_picker(&mut commands, &mut ui, &mut images, &mut fonts, sim);
    }
    for (i, w) in &wishes {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::LifetimeWish(w.0)));
        }
    }
    // Favourites: a button opens (or closes) its grid; a choice closes it.
    for (n, id) in FAVORITE_BUTTONS.iter().enumerate() {
        if pressed(*id) {
            c.open = if c.open == Some(n) { None } else { Some(n) };
        }
    }
    for (i, f) in &favorites {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::Favorite(f.0, f.1)));
            c.open = None;
        }
    }
    if pressed(RANDOM_FAVORITES) {
        actions.write(crate::cas::CasActionRequest(CasAction::RandomFavorites));
        c.open = None;
    }
    let wish = chosen.0.get(&sim.id).copied();
    let f = &sim.favorites;
    let want = Shown {
        sim: k,
        age: sim.age,
        traits: sim.traits.clone(),
        wish,
        favorites: (f.food.clone(), f.music.clone(), f.color.clone()),
        voice: sim.voice,
        page: c.favorites_page,
        open: c.open,
        icons: gu.is_some(),
    };
    if c.shown.as_ref() == Some(&want) {
        return;
    }
    let reopen = c.shown.as_ref().is_none_or(|s| (s.open, &s.favorites, s.icons) != (want.open, &want.favorites, want.icons));
    c.shown = Some(want);
    let s = &c.s;
    // The page shown, its tab lit, the panel as tall as the page (`UpdatePanelHeight`).
    if let Some(r) = s.root {
        let page = ui.find("CASCharacter", if c.favorites_page { FAVORITES_PAGE } else { TRAITS_PAGE }).map_or(560.0, |w| w.area[3] - w.area[1]);
        let tall = if !c.favorites_page && (sim.age.is_little() || sim.age == Age::Child) { 200.0 } else { page };
        commands.entity(r).entry::<Node>().and_modify(move |mut n| n.height = Val::Px(tall));
    }
    crate::livehud::set_visible(&mut vis, s.id(TRAITS_PAGE), !c.favorites_page);
    crate::livehud::set_visible(&mut vis, s.id(FAVORITES_PAGE), c.favorites_page);
    let mut lit = |id: u32, on: bool| {
        if let Some(mut b) = s.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != on
        {
            b.selected = on;
        }
    };
    lit(TAB_TRAITS, !c.favorites_page);
    lit(TAB_FAVORITES, c.favorites_page);
    for (n, id) in VOICES.iter().enumerate() {
        lit(*id, sim.voice as usize == n);
    }
    for (n, id) in FAVORITE_BUTTONS.iter().enumerate() {
        lit(*id, c.open == Some(n));
    }
    let data = gu.as_ref().map(|g| g.data.clone());
    // The traits in their sockets (as many sockets as the Sim's age has).
    let slots = crate::life::trait_slots(sim.age);
    for i in 0..5 {
        let Some(e) = s.id(TRAIT_SLOT + i as u32) else { continue };
        commands.entity(e).insert(if i < slots { Visibility::Inherited } else { Visibility::Hidden });
        let t = sim.traits.get(i).copied();
        let info = t.and_then(|t| gu.as_deref().and_then(|g| g.trait_info(t)));
        let icon = info.as_ref().and_then(|i| gu.as_deref_mut().and_then(|g| g.icon(&mut images, &i.icon)));
        match (t, icon) {
            (Some(t), Some(h)) => {
                commands.entity(e).insert((SetIcon(h), crate::icons::Tooltip(info.map_or_else(|| t.name().to_string(), |i| format!("{}\n{}", i.name, i.desc)))));
            }
            _ => {
                // (An empty socket: its icon put away.)
                if let Some(icon) = buttons.get(e).ok().and_then(|b| b.icon()) {
                    commands.entity(icon).insert(Visibility::Hidden);
                }
                commands.entity(e).insert(crate::icons::Tooltip("Add or Change Traits".into()));
            }
        }
    }
    // The lifetime wishes that suit them (teens and up), the chosen one first among them.
    let wishes_ok = !sim.age.is_little() && sim.age != Age::Child;
    crate::livehud::set_visible(&mut vis, s.id(WISHES), wishes_ok);
    if wishes_ok {
        use crate::lifetime::{LIFETIME_WISHES, group, suggestions};
        let mut options = suggestions(sim, 6);
        if let Some(w) = wish.filter(|w| !options.contains(w)) {
            options.insert(0, w);
            options.truncate(6);
        }
        for n in 0..6 {
            let Some(e) = s.id(WISH_SLOT + n as u32) else { continue };
            let Some(&i) = options.get(n) else {
                commands.entity(e).insert(Visibility::Hidden);
                continue;
            };
            let d = &LIFETIME_WISHES[i];
            commands.entity(e).insert((Visibility::Inherited, WishSlot(i), crate::icons::Tooltip(format!("{}\n{}", d.name, d.describe(data.as_deref())))));
            if let Some(h) = gu.as_deref_mut().and_then(|g| g.icon(&mut images, &d.icon(data.as_deref()))) {
                commands.entity(e).insert(SetIcon(h));
            }
            if let Ok(mut b) = buttons.get_mut(e)
                && b.selected != (wish == Some(i))
            {
                b.selected = wish == Some(i);
            }
        }
        let d = wish.map(|i| &LIFETIME_WISHES[i]);
        crate::livehud::set_text(&mut texts, s.text(WISH_NAME), d.map_or("Lifetime Wish", |d| d.name));
        crate::livehud::set_text(&mut texts, s.text(WISH_REQUIRED), &d.map_or(String::new(), |d| format!("{} lifetime happiness", group(d.points(data.as_deref()) as i64))));
        crate::livehud::set_text(&mut texts, s.text(WISH_DESCRIPTION), &d.map_or_else(|| "Pick the dream of this Sim's life: these suit their traits.".to_string(), |d| d.describe(data.as_deref())));
    }
    // The favourites: each button with the one chosen; the grid of one open.
    let food_name = |key: &str| data.as_ref().and_then(|d| d.recipes.iter().find(|r| r.key == key).map(|r| r.name.clone())).unwrap_or_else(|| key.to_string());
    let lists: [(&str, Vec<(String, String)>, &str); 3] = [
        ("food", crate::sim::FAVORITE_FOODS.iter().map(|k| (k.to_string(), food_name(k))).collect(), f.food.as_str()),
        ("music", crate::sim::FAVORITE_MUSIC.iter().map(|m| (m.to_string(), crate::sim::Favorites::music_name(m).to_string())).collect(), f.music.as_str()),
        ("color", crate::sim::FAVORITE_COLORS.iter().map(|c| (c.to_string(), crate::sim::Favorites::color_name(c).to_string())).collect(), f.color.as_str()),
    ];
    for (n, (pic, list, now)) in lists.iter().enumerate() {
        let Some(e) = s.id(FAVORITE_BUTTONS[n]) else { continue };
        let name = list.iter().find(|(k, _)| k == now).map_or(now.to_string(), |x| x.1.clone());
        commands.entity(e).insert(crate::icons::Tooltip(name));
        if let Some(h) = gu.as_deref_mut().and_then(|g| g.icon(&mut images, &crate::sim::Favorites::icon(pic, now))) {
            commands.entity(e).insert(SetIcon(h));
        }
        crate::livehud::set_visible(&mut vis, s.id(FAVORITE_WINDOWS[n]), c.open == Some(n));
    }
    if !reopen {
        return;
    }
    if let Some(old) = c.cells.take() {
        commands.entity(old).despawn();
    }
    let Some(n) = c.open else { return };
    let (pic, list, now) = &lists[n];
    let (Some(grid), Some(g), Some(template)) = (s.id(FAVORITE_GRIDS[n]), ui.find("CASCharacter", FAVORITE_GRIDS[n]).and_then(|w| w.grid), ui.layout("FavoritesGridItem").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
    c.cells = Some(holder);
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    let cols = g.columns.max(1) as usize;
    for (i, (key, name)) in list.iter().enumerate() {
        let (x, y) = (g.padding[0] + (i % cols) as f32 * step.x, g.padding[1] + (i / cols) as f32 * step.y);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let cs = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = cs.root else { continue };
        commands.entity(root).insert((FavoriteCell(n as u8, i), crate::icons::Tooltip(name.clone())));
        if key == now {
            commands.entity(root).queue(|mut e: EntityWorldMut| {
                if let Some(mut b) = e.get_mut::<UiButton>() {
                    b.selected = true;
                }
            });
        }
        if let (Some(win), Some(h)) = (cs.id(CELL_THUMBNAIL), gu.as_deref_mut().and_then(|g| g.icon(&mut images, &crate::sim::Favorites::icon(pic, key)))) {
            crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
        }
    }
}

/// The traits picker put up beside the character panel, with the Sim's traits.
fn open_picker(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, sim: &crate::sim::Sim) -> Option<Picker> {
    let mut w = ui.layout("TraitsPicker")?.clone();
    // (Where the game puts it, kept on the screen.)
    let (wd, ht) = (w.area[2] - w.area[0], w.area[3] - w.area[1]);
    w.area = [w.area[0], 12.0, w.area[0] + wd, 12.0 + ht];
    w.place = s3bake::ui::UiPlace::Fixed;
    w.flags |= s3bake::ui::WIN_VISIBLE;
    let s = ui.spawn_root(commands, images, fonts, &w);
    let root = s.root?;
    commands.entity(root).insert((GlobalZIndex(8), crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(AppState::CreateHousehold)));
    for (id, tip) in [(PICK_FILTERS[0], "All"), (PICK_FILTERS[1], "Mental"), (PICK_FILTERS[2], "Physical"), (PICK_FILTERS[3], "Social"), (PICK_FILTERS[4], "Lifestyle"), (PICK_ACCEPT, "Accept"), (PICK_CANCEL, "Cancel")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    if let Some(e) = s.text(PICK_NAME) {
        commands.entity(e).insert(Text::new(sim.full_name()));
    }
    // (The Sim's age's figure for their picture.)
    if let (Some(e), Some((h, _))) = (s.id(PICK_THUMB), ui.image(images, s3pkg::fnv64(crate::caslook::AGE_ICONS[crate::caslook::age_icon(sim.age) as usize]))) {
        crate::hudpanels::picture(commands, e, h, crate::layout::color(0xff16_3996));
    }
    // The traits to take: the item grid's own scroll bar down its right.
    let mut bar = None;
    if let (Some(grid), Some(mut b)) = (s.id(PICK_AVAILABLE), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
        b.flags |= s3bake::ui::WIN_VISIBLE;
        b.place = s3bake::ui::UiPlace::Fixed;
        b.area = [218.0, 0.0, 236.0, 160.0];
        bar = ui.spawn_under(commands, images, fonts, &b, grid).root;
        commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default());
    }
    Some(Picker { s, traits: sim.traits.clone(), group: 0, chosen: None, clicked: None, scroll: 0, current: None, available: None, bar, dirty: true })
}

/// The game's group of a trait (1 mental, 2 physical, 3 social, 4 lifestyle), by its category.
fn trait_group(category: &str) -> usize {
    let c = category.to_ascii_lowercase();
    if c.contains("ment") || c.contains("intel") {
        1
    } else if c.contains("phys") {
        2
    } else if c.contains("soc") {
        3
    } else if c.contains("life") {
        4
    } else {
        0
    }
}

/// The traits picker: choosing, adding and taking away; accepted, the Sim's traits become
/// those chosen (taken away first, then added, as the CAS actions toggle them).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn traits_picker(
    mut commands: Commands,
    character: Option<ResMut<CasCharacter>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    (mut buttons, mut bars): (Query<&mut UiButton>, Query<&mut UiScrollBar>),
    (clicks, rows, over): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &TraitRow), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    time: Res<Time>,
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut c), Some(mut ui)) = (character, ui) else { return };
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k) else { return };
    // (Put away when Create a Sim leaves the Personality tab.)
    if selected.1 != CasTab::Traits {
        if let Some(p) = c.picker.take()
            && let Some(r) = p.s.root
        {
            commands.entity(r).despawn();
        }
        return;
    }
    let Some(p) = c.picker.as_mut() else { return };
    // (What was clicked: the current traits' buttons are the shown copy's of three alike.)
    let pressed = |id: u32| crate::livehud::pressed(&clicks, p.s.id(id));
    let current_bg = p.s.id(PICK_CURRENT_BG);
    let in_bg = |id: u32| current_bg.and_then(|bg| p.s.within(bg, id));
    let (add_pressed, erase_pressed, random_pressed) = (crate::livehud::pressed(&clicks, in_bg(PICK_ADD)), crate::livehud::pressed(&clicks, in_bg(PICK_ERASE)), crate::livehud::pressed(&clicks, in_bg(PICK_RANDOM)));
    let (accept, cancel) = (pressed(PICK_ACCEPT), pressed(PICK_CANCEL));
    let group_pressed = PICK_FILTERS.iter().position(|id| pressed(*id));
    let current_grid = in_bg(PICK_CURRENT);
    let slots = crate::life::trait_slots(sim.age);
    let can_take = |list: &[Trait], t: Trait| !list.contains(&t) && list.len() < slots && t.allowed_at(sim.age) && t.compatible(list);
    let add = |p: &mut Picker, t: Trait, play: &mut MessageWriter<crate::sound::PlaySound>| {
        if can_take(&p.traits, t) {
            p.traits.push(t);
            play.write(crate::sound::PlaySound::ui("ui_cas_trait_add"));
        }
        p.chosen = None;
        p.dirty = true;
    };
    let remove = |p: &mut Picker, t: Trait, play: &mut MessageWriter<crate::sound::PlaySound>| {
        p.traits.retain(|x| *x != t);
        play.write(crate::sound::PlaySound::ui("ui_cas_trait_remove"));
        p.chosen = None;
        p.dirty = true;
    };
    // A row: chosen with a click, added or taken away with a second (or a double click).
    let now = time.elapsed_secs();
    for (i, r) in &rows {
        if *i != Interaction::Pressed {
            continue;
        }
        let again = p.chosen == Some((r.0, r.1)) || p.clicked.is_some_and(|(taken, t, at)| (taken, t) == (r.0, r.1) && now - at < 0.4);
        p.clicked = Some((r.0, r.1, now));
        if again {
            if r.0 {
                remove(p, r.1, &mut play);
            } else if p.traits.contains(&r.1) {
                remove(p, r.1, &mut play);
            } else {
                add(p, r.1, &mut play);
            }
        } else {
            p.chosen = Some((r.0, r.1));
            p.dirty = true;
        }
    }
    if add_pressed && let Some((false, t)) = p.chosen {
        add(p, t, &mut play);
    }
    if erase_pressed && let Some((true, t)) = p.chosen {
        remove(p, t, &mut play);
    }
    if random_pressed {
        p.traits = crate::life::random_traits(&mut rand::rng(), sim.age);
        p.chosen = None;
        p.dirty = true;
    }
    if let Some(g) = group_pressed
        && p.group != g
    {
        p.group = g;
        p.scroll = 0;
        p.dirty = true;
    }
    // Accepted: the traits taken away, then those added; either way, the picker put away.
    if accept || cancel {
        if accept {
            for t in sim.traits.iter().filter(|t| !p.traits.contains(t)).chain(p.traits.iter().filter(|t| !sim.traits.contains(t))) {
                if let Some(i) = Trait::ALL.iter().position(|x| x == t) {
                    actions.write(crate::cas::CasActionRequest(CasAction::Trait(i)));
                }
            }
        }
        if let Some(r) = p.s.root {
            commands.entity(r).despawn();
        }
        c.picker = None;
        return;
    }
    // The traits to take, in the group shown (as the game's dictionary has them, by name).
    let mut available: Vec<(Trait, Option<s3bake::gamedata::TraitInfo>)> = Trait::ALL.iter().filter(|t| t.allowed_at(sim.age)).map(|t| (*t, gu.as_deref().and_then(|g| g.trait_info(*t)))).collect();
    available.retain(|(_, i)| p.group == 0 || i.as_ref().is_some_and(|i| trait_group(&i.category) == p.group));
    available.sort_by_key(|(t, i)| i.as_ref().map_or_else(|| t.name().to_string(), |i| i.name.clone()));
    let most = available.len().saturating_sub(AVAILABLE_ROWS);
    let over_list = p.s.id(PICK_AVAILABLE).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    let wheeled = over_list && wheel.delta.y != 0.0;
    if wheeled {
        p.scroll = (p.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
        p.dirty = true;
    }
    if let Some(mut b) = p.bar.and_then(|e| bars.get_mut(e).ok()) {
        if (b.total, b.visible) != (available.len(), AVAILABLE_ROWS) {
            (b.total, b.visible) = (available.len(), AVAILABLE_ROWS);
        }
        if b.value != p.scroll {
            if wheeled {
                b.value = p.scroll;
            } else {
                p.scroll = b.value.min(most);
                p.dirty = true;
            }
        }
    }
    if !p.dirty {
        return;
    }
    p.dirty = false;
    for (g, id) in PICK_FILTERS.iter().enumerate() {
        if let Some(mut b) = p.s.id(*id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != (p.group == g)
        {
            b.selected = p.group == g;
        }
    }
    let Some(template) = ui.layout("CASTraitEntry").cloned() else { return };
    // (Rows alternate lighter and darker, as the game's do; names grey where they can't be taken.)
    let row = |size: [f32; 2], y: f32, n: usize, trait_: Option<(Trait, Option<&s3bake::gamedata::TraitInfo>, bool)>| {
        let mut w = template.clone();
        w.area = [0.0, y, size[0], y + size[1]];
        w.cls = "Button".into();
        let shade = if n % 2 == 0 { "cas_traits_row_darker_r2" } else { "cas_traits_row_lighter_r2" };
        if let Some(s3bake::ui::UiDrawable::Std { images: keys, .. }) = &mut w.drawable {
            keys[0] = s3pkg::fnv64(shade);
        }
        if let Some((t, info, ok)) = trait_ {
            let name = info.map_or_else(|| t.name().to_string(), |i| i.name.clone());
            edit_windows(&mut w, ENTRY_TEXT, &mut |tw| {
                tw.caption = name.clone();
                tw.colors = vec![if ok { 0xff16_3996 } else { 0xff96_9696 }];
            });
        } else {
            edit_windows(&mut w, ENTRY_TEXT, &mut |tw| tw.caption.clear());
        }
        w
    };
    // Taken: as many rows as the Sim's age has traits.
    if let Some(grid) = current_grid
        && let Some(g) = ui.find("TraitsPicker", PICK_CURRENT).and_then(|w| w.grid)
    {
        if let Some(old) = p.current.take() {
            commands.entity(old).despawn();
        }
        let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
        p.current = Some(holder);
        let step = g.cell[1] + g.cell_padding[1] + g.cell_padding[3];
        for n in 0..5 {
            let t = p.traits.get(n).copied();
            let info = t.and_then(|t| gu.as_deref().and_then(|g| g.trait_info(t)));
            let w = row(g.cell, g.padding[1] + n as f32 * step, n, t.map(|t| (t, info.as_ref(), true)));
            let s = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
            let Some(r) = s.root else { continue };
            match t {
                Some(t) => {
                    commands.entity(r).insert(TraitRow(true, t));
                    if let Some(i) = &info {
                        commands.entity(r).insert(crate::icons::Tooltip(format!("{}\n{}", i.name, i.desc)));
                    }
                    if p.chosen == Some((true, t)) {
                        lit_row(&mut commands, r);
                    }
                    if let (Some(win), Some(h)) = (s.id(ENTRY_ICON), info.as_ref().and_then(|i| gu.as_deref_mut().and_then(|g| g.icon(&mut images, &i.icon_small)))) {
                        crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
                    }
                }
                None => {
                    // (Rows past the Sim's age's traits aren't for taking.)
                    if n >= slots {
                        commands.entity(r).insert(Visibility::Hidden);
                    }
                }
            }
        }
    }
    // To take: the group's, scrolled.
    if let Some(grid) = p.s.id(PICK_AVAILABLE)
        && let Some(g) = ui.find("TraitsPicker", PICK_AVAILABLE).and_then(|w| w.grid)
    {
        if let Some(old) = p.available.take() {
            commands.entity(old).despawn();
        }
        let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
        commands.entity(grid).insert_children(0, &[holder]);
        p.available = Some(holder);
        let step = g.cell[1] + g.cell_padding[1] + g.cell_padding[3];
        for (n, (t, info)) in available.iter().enumerate().skip(p.scroll).take(AVAILABLE_ROWS) {
            let ok = can_take(&p.traits, *t);
            let w = row(g.cell, g.padding[1] + (n - p.scroll) as f32 * step, n, Some((*t, info.as_ref(), ok)));
            let s = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
            let Some(r) = s.root else { continue };
            commands.entity(r).insert(TraitRow(false, *t));
            if let Some(i) = info {
                commands.entity(r).insert(crate::icons::Tooltip(format!("{}\n{}", i.name, i.desc)));
            }
            if p.chosen == Some((false, *t)) {
                lit_row(&mut commands, r);
            }
            if let (Some(win), Some(h)) = (s.id(ENTRY_ICON), info.as_ref().and_then(|i| gu.as_deref_mut().and_then(|g| g.icon(&mut images, &i.icon_small)))) {
                crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
            }
        }
    }
}

fn lit_row(commands: &mut Commands, r: Entity) {
    commands.entity(r).queue(|mut e: EntityWorldMut| {
        if let Some(mut b) = e.get_mut::<UiButton>() {
            b.selected = true;
        }
    });
}
