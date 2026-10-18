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
use crate::layout::{SetIcon, Spawned, UiAssets, UiButton, UiScrollBar};
use crate::life::Trait;
use crate::sim::Age;

pub struct CasCharPlugin;

impl Plugin for CasCharPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_character, character_panel, voice_and_bio, traits_picker).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
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
/// The voice's pitch, and the biography: its button, its box and the box's words.
const PITCH: u32 = 0x0800_0004;
const BIO_BUTTON: u32 = 0x4000;
const BIO_WINDOW: u32 = 0x4001;
const BIO_TEXT: u32 = 0x4002;
/// How long a biography may be (the game's box holds a few lines).
const BIO_LONGEST: usize = 240;
/// What this game hasn't: a custom lifetime wish.
const NOT_HERE: [u32; 2] = [CLOSE, CUSTOM_WISH];
const CELL_THUMBNAIL: u32 = 0x20;
/// The panel on screen, its page, the favourites open, and what it was shown for; the traits
/// picker when it's up.
#[derive(Resource)]
pub struct CasCharacter {
    s: Spawned,
    favorites_page: bool,
    open: Option<usize>,
    cells: Option<Entity>,
    shown: Option<Shown>,
    picker: Option<crate::traitpicker::Picker>,
    /// The biography's box open (the panel then as tall as the traits page, as the game's
    /// `UpdatePanelHeight`).
    bio_open: bool,
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
    bio: bool,
}

/// The voice's pitch and the biography as last shown, and the biography being typed.
#[derive(Default)]
struct VoiceBio {
    sim: Option<u64>,
    pitch: f32,
    typing: Option<String>,
    shown: Option<(u64, String, bool)>,
}

/// On the favourites page, as `CASCharacter` has them: the voice's pitch on its slider (heard
/// when let go), and the biography, its button opening the box beneath (a click in it, then the
/// keys; Enter, or a click elsewhere, keeps it).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn voice_and_bio(
    character: Option<ResMut<CasCharacter>>,
    mut pending: ResMut<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    (mut sliders, mut buttons, mut texts, mut vis): (Query<&mut crate::layout::UiSlider>, Query<&mut UiButton>, Query<&mut Text>, Query<&mut Visibility>),
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    (mut keyboard, mouse): (MessageReader<bevy::input::keyboard::KeyboardInput>, Res<ButtonInput<MouseButton>>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut st: Local<VoiceBio>,
) {
    let Some(mut c) = character else { return };
    let c = &mut *c;
    let on = selected.1 == CasTab::Traits && c.favorites_page && !crate::caslook::modal_up();
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(id) = pending.members.get(k).map(|s| s.id) else { return };
    if !on || st.sim != Some(id) {
        st.typing = None;
        keyboard.clear();
    }
    if !on {
        return;
    }
    // The pitch: put where the Sim's is (on opening, for another Sim), and heard once let go.
    if let Some(mut sl) = c.s.id(PITCH).and_then(|e| sliders.get_mut(e).ok()) {
        let span = (sl.max - sl.min).max(1.0);
        if st.sim != Some(id) {
            let p = pending.members[k].voice_pitch;
            sl.value = sl.min + p * span;
            st.pitch = p;
        } else {
            let p = ((sl.value - sl.min) / span).clamp(0.0, 1.0);
            if (p - st.pitch).abs() > 0.002 && !mouse.pressed(MouseButton::Left) {
                st.pitch = p;
                actions.write(crate::cas::CasActionRequest(CasAction::VoicePitch(p)));
            }
        }
    }
    st.sim = Some(id);
    // (CAS_BIO=1, tests: the box open with words in it.)
    if std::env::var_os("CAS_BIO").is_some() && !c.bio_open && pending.members[k].bio.is_empty() {
        c.bio_open = true;
        pending.members[k].bio = "Loves long walks by the lake and arguing about chess.".into();
    }
    // The biography: its button opens the box, a click in the box to write in it.
    if crate::livehud::pressed(&clicks, c.s.id(BIO_BUTTON)) {
        c.bio_open = !c.bio_open;
        if !c.bio_open {
            st.typing = None;
        }
    }
    if c.bio_open && crate::livehud::pressed(&clicks, c.s.id(BIO_TEXT)) {
        st.typing = Some(pending.members[k].bio.clone());
    }
    let mut finished = false;
    if let Some(words) = st.typing.as_mut() {
        use bevy::input::keyboard::Key;
        for ev in keyboard.read() {
            if !ev.state.is_pressed() {
                continue;
            }
            match &ev.logical_key {
                Key::Character(ch) if words.chars().count() < BIO_LONGEST => words.push_str(ch.as_str()),
                Key::Space if words.chars().count() < BIO_LONGEST => words.push(' '),
                Key::Backspace => {
                    words.pop();
                }
                Key::Enter | Key::Escape | Key::Tab => finished = true,
                _ => {}
            }
        }
        pending.members[k].bio = words.clone();
        let clicked_away = mouse.just_pressed(MouseButton::Left) && !c.s.id(BIO_TEXT).is_some_and(|e| clicks.get(e).is_ok_and(|(_, i)| *i == Interaction::Pressed));
        if clicked_away {
            finished = true;
        }
    } else {
        keyboard.clear();
    }
    if finished {
        st.typing = None;
    }
    let words = match &st.typing {
        Some(w) => format!("{w}|"),
        None => pending.members[k].bio.clone(),
    };
    let want = (id, words.clone(), c.bio_open);
    if st.shown.as_ref() == Some(&want) {
        return;
    }
    st.shown = Some(want);
    crate::livehud::set_visible(&mut vis, c.s.id(BIO_WINDOW), c.bio_open);
    if let Some(mut b) = c.s.id(BIO_BUTTON).and_then(|e| buttons.get_mut(e).ok())
        && b.selected != c.bio_open
    {
        b.selected = c.bio_open;
    }
    crate::livehud::set_text(&mut texts, c.s.text(BIO_TEXT), &words);
}

/// A lifetime wish's slot (of `lifetime::LIFETIME_WISHES`).
#[derive(Component)]
struct WishSlot(usize);

/// A favourite's cell: which (food, music, colour) and its entry.
#[derive(Component)]
struct FavoriteCell(u8, usize);

/// A trait's row in the picker: taken or not, and the trait.
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
    commands.insert_resource(CasCharacter { s, favorites_page: false, open: None, cells: None, shown: None, picker: None, bio_open: false });
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
    (chosen, pics): (Res<crate::lifetime::ChosenLifetimeWishes>, Res<crate::casportraits::CasPortraits>),
    (mut vis, mut buttons, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut Text>),
    (clicks, wishes, favorites): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &WishSlot), Changed<Interaction>>, Query<(&Interaction, &FavoriteCell), Changed<Interaction>>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut c), Some(mut ui)) = (character, ui) else { return };
    let c = &mut *c;
    let on = selected.1 == CasTab::Traits && !crate::caslook::modal_up();
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
        1 if c.picker.is_none() => c.picker = open_picker(&mut commands, &mut ui, &mut images, &mut fonts, sim, pics.get(sim.id)),
        2 => {
            if let Some(p) = c.picker.take() {
                p.close(&mut commands);
            }
            c.favorites_page = true;
            // (CAS_BIO=1: the voice and the biography to be seen instead of the foods.)
            c.open = if std::env::var_os("CAS_BIO").is_some() { None } else { Some(0) };
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
        c.picker = open_picker(&mut commands, &mut ui, &mut images, &mut fonts, sim, pics.get(sim.id));
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
        bio: c.bio_open,
    };
    if c.shown.as_ref() == Some(&want) {
        return;
    }
    let reopen = c.shown.as_ref().is_none_or(|s| (s.open, &s.favorites, s.icons) != (want.open, &want.favorites, want.icons));
    c.shown = Some(want);
    let s = &c.s;
    // The page shown, its tab lit, the panel as tall as the page (`UpdatePanelHeight`).
    if let Some(r) = s.root {
        let page = ui.find("CASCharacter", if c.favorites_page && !c.bio_open { FAVORITES_PAGE } else { TRAITS_PAGE }).map_or(560.0, |w| w.area[3] - w.area[1]);
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
        commands.entity(old).try_despawn();
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

/// The traits picker put up beside the character panel, with the Sim's picture and traits.
fn open_picker(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, sim: &crate::sim::Sim, portrait: Option<Handle<Image>>) -> Option<crate::traitpicker::Picker> {
    // (Where the game puts it, kept on the screen.)
    let x = ui.layout("TraitsPicker")?.area[0];
    crate::traitpicker::open(commands, ui, images, fonts, &sim.full_name(), (sim.age, portrait), &sim.traits, &[], Some(Vec2::new(x, 12.0)), (GlobalZIndex(8), DespawnOnExit(AppState::CreateHousehold)))
}

/// How dark the panels behind the traits picker go (the game's shade, 120 of 255).
const DARKEN: f32 = 120.0 / 255.0;

/// The traits picker: accepted, the Sim's traits become those chosen (taken away first, then
/// added, as the CAS actions toggle them); either way it's put away.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn traits_picker(
    mut commands: Commands,
    character: Option<ResMut<CasCharacter>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    (mut buttons, mut bars): (Query<&mut UiButton>, Query<&mut UiScrollBar>),
    (clicks, rows, over): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &crate::traitpicker::TraitRow), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    time: Res<Time>,
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
    (frame, mut darkened): (Option<Res<crate::caslook::CasFrame>>, Local<bool>),
) {
    let (Some(mut c), Some(mut ui)) = (character, ui) else { return };
    // The panels behind the picker darkened while it's up (`CASController.DarkenBackgroundPanels`:
    // the puck, the character sheet and the character panel).
    if c.picker.is_some() != *darkened {
        *darkened = c.picker.is_some();
        for root in frame.iter().flat_map(|f| f.roots()).chain([c.s.root]).flatten() {
            if *darkened {
                commands.entity(root).insert(crate::layout::Shade(DARKEN));
            } else {
                commands.entity(root).remove::<crate::layout::Shade>();
            }
        }
    }
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k) else { return };
    // (Put away when Create a Sim leaves the Personality tab.)
    if selected.1 != CasTab::Traits {
        if let Some(p) = c.picker.take() {
            p.close(&mut commands);
        }
        return;
    }
    let Some(p) = c.picker.as_mut() else { return };
    let outcome = crate::traitpicker::update(p, &mut commands, &mut ui, (&mut images, &mut fonts), (&clicks, &rows, &over), (&wheel, time.elapsed_secs()), (&mut buttons, &mut bars), &mut play, gu.as_deref_mut());
    match outcome {
        crate::traitpicker::Outcome::Open => {}
        crate::traitpicker::Outcome::Accepted(list) => {
            for t in sim.traits.iter().filter(|t| !list.contains(t)).chain(list.iter().filter(|t| !sim.traits.contains(t))) {
                if let Some(i) = Trait::ALL.iter().position(|x| x == t) {
                    actions.write(crate::cas::CasActionRequest(CasAction::Trait(i)));
                }
            }
            p.close(&mut commands);
            c.picker = None;
        }
        crate::traitpicker::Outcome::Cancelled => {
            p.close(&mut commands);
            c.picker = None;
        }
    }
}
