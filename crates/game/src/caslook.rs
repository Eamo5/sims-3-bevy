//! Create a Sim in the game's own look (UI.package's CAS layouts, driven as UI.dll's `CASPuck`
//! and `CASCharacterSheet` drive them). The puck along the bottom: the camera's turn buttons,
//! the household's Sims along the skewer (each in the game's Sim button, the one being made
//! lit; a click to make another), Add a Sim, the "…" menu (remove the Sim, a new family,
//! the town's families), cancel (back to the main menu) and accept. The character sheet on the
//! left: Basics, Hair, Face, Clothing and Character, with their names, and Randomize. Their
//! buttons work the CAS actions (`cas::CasAction`), so everything else Create a Sim does stays
//! as it was.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::AppState;
use crate::cas::{CasAction, CasTab};
use crate::home::PendingHousehold;
use crate::layout::{SetIcon, Spawned, UiAssets, UiButton};

pub struct CasLookPlugin;

impl Plugin for CasLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_cas_frame, skewer, sheet, puck_buttons, spawn_basics, basics_panel).chain().run_if(in_state(AppState::CreateHousehold)))
            .add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
                commands.remove_resource::<CasFrame>();
                commands.remove_resource::<CasBasics>();
            });
    }
}

/// Whether the game's own Create a Sim frame is up (the plain household panel stands aside).
pub static GAME_CAS: AtomicBool = AtomicBool::new(false);

// `CASPuck`'s windows.
const SKEWER_NONE: u32 = 0x05da_4d0a;
const SKEWER_SHORT: u32 = 0x05da_4d09;
const SKEWER_LONG: u32 = 0x05da_4d06;
const ADD_SIM: u32 = 0x05da_4d00;
const ADD_PETS: u32 = 0x05da_4d04;
const UNDO: u32 = 0x05da_4d01;
const REDO: u32 = 0x05da_4d02;
const CANCEL: u32 = 0x05da_4d03;
const ACCEPT: u32 = 0x05da_4d08;
const MORE: u32 = 0x05da_4d1c;
const OPTIONS: u32 = 0x05da_4d07;
const ROTATE_LEFT: u32 = 0x8fef_fb00;
const ROTATE_RIGHT: u32 = 0x8fef_fb01;
const ZOOM_OUT: u32 = 0x8fef_fb02;
const ZOOM_IN: u32 = 0x8fef_fb03;
const SIM_BUTTON: u32 = 0x05da_4d10;
const SIM_STEP: f32 = 65.0;
const MAX_SIMS: usize = 8;
// `CASCharacterSheet`'s.
const SHEET_BASICS: u32 = 0x0077_7000;
const SHEET_BASICS_FEMALE: u32 = 0x0077_7007;
const SHEET_HAIR: u32 = 0x0077_7001;
const SHEET_FACE: u32 = 0x0077_7002;
const SHEET_CLOTHING: u32 = 0x0077_7005;
const SHEET_CHARACTER: u32 = 0x0077_7003;
const SHEET_RANDOMIZE: u32 = 0x0077_70b2;
const SHEET_TEXT: u32 = 0x0077_7020;
/// The popup menus' ids.
const MORE_MENU: u32 = 0x6361_736d;
const OPTIONS_MENU: u32 = 0x6361_736f;

/// The frame on screen.
#[derive(Resource)]
pub struct CasFrame {
    puck: Spawned,
    sheet: Spawned,
    /// The Sims' buttons along the skewer, and who they're for.
    sims: Vec<Entity>,
    shown: Option<(Vec<(String, u8, bool)>, usize)>,
    holder: Option<Entity>,
}

/// One of the household's Sims on the skewer.
#[derive(Component)]
struct SimButton;

/// The mode a character sheet button opens.
fn mode_of(tab: CasTab) -> u32 {
    match tab {
        CasTab::Basics => SHEET_BASICS,
        CasTab::Hair => SHEET_HAIR,
        CasTab::Face => SHEET_FACE,
        CasTab::Tops | CasTab::Bottoms | CasTab::Outfits | CasTab::Shoes => SHEET_CLOTHING,
        CasTab::Traits => SHEET_CHARACTER,
    }
}

fn spawn_cas_frame(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<CasFrame>>) {
    let Some(mut ui) = ui else { return };
    if frame.is_some() || ui.layout("CASPuck").is_none() || ui.layout("CASCharacterSheet").is_none() {
        return;
    }
    let mut spawn = |name: &str| -> Option<Spawned> {
        let s = ui.spawn(&mut commands, &mut images, &mut fonts, name)?;
        commands.entity(s.root?).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(5), Visibility::Inherited));
        Some(s)
    };
    let (Some(puck), Some(sheet)) = (spawn("CASPuck"), spawn("CASCharacterSheet")) else { return };
    // The long skewer (the household's row), not the dresser's or the mirror's.
    for (id, on) in [(SKEWER_NONE, false), (SKEWER_SHORT, false), (SKEWER_LONG, true), (ADD_PETS, false), (UNDO, false), (REDO, false), (SHEET_BASICS_FEMALE, false)] {
        for s in [&puck, &sheet] {
            for e in s.all_with(id) {
                commands.entity(e).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
    }
    // The buttons that work CAS actions as they are.
    for (s, id, action, tip) in [
        (&puck, ACCEPT, CasAction::Done, "Accept"),
        (&puck, ADD_SIM, CasAction::Add, "Add a Sim"),
        (&sheet, SHEET_BASICS, CasAction::Tab(CasTab::Basics), "Basics"),
        (&sheet, SHEET_HAIR, CasAction::Tab(CasTab::Hair), "Hair"),
        (&sheet, SHEET_FACE, CasAction::Tab(CasTab::Face), "Face"),
        (&sheet, SHEET_CLOTHING, CasAction::Tab(CasTab::Tops), "Clothing"),
        (&sheet, SHEET_CHARACTER, CasAction::Tab(CasTab::Traits), "Character"),
        (&sheet, SHEET_RANDOMIZE, CasAction::Randomize, "Randomize"),
    ] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert((action, crate::icons::Tooltip(tip.into())));
        }
    }
    for (id, tip) in [(CANCEL, "Cancel"), (MORE, "More"), (OPTIONS, "Options"), (ROTATE_LEFT, "Rotate"), (ROTATE_RIGHT, "Rotate")] {
        if let Some(e) = puck.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    GAME_CAS.store(true, Ordering::Relaxed);
    commands.insert_resource(CasFrame { puck, sheet, sims: Vec::new(), shown: None, holder: None });
}

/// The household along the skewer: a Sim button each (their age's picture), the one being
/// made lit, Add a Sim while there's room.
#[allow(clippy::too_many_arguments)]
fn skewer(
    mut commands: Commands,
    frame: Option<ResMut<CasFrame>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    mut vis: Query<&mut Visibility>,
) {
    let (Some(mut f), Some(mut ui)) = (frame, ui) else { return };
    let f = &mut *f;
    let want: Vec<(String, u8, bool)> = pending.members.iter().map(|m| (m.first.clone(), age_icon(m.age), m.female)).collect();
    let sel = selected.0;
    if f.shown.as_ref() == Some(&(want.clone(), sel)) {
        return;
    }
    f.shown = Some((want, sel));
    let Some(container) = f.puck.comment("Sim Button Container...magic") else { return };
    let Some(template) = ui.layout("CASPuck").and_then(|l| l.find_comment("Sim 1")).cloned() else { return };
    let holder = match f.holder.filter(|h| vis.contains(*h)) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => {
            let h = commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, Visibility::Inherited, Pickable::IGNORE, ChildOf(container))).id();
            f.holder = Some(h);
            h
        }
    };
    // (The old buttons the layout carries stand aside for the household's own.)
    for e in f.puck.all_with(SIM_BUTTON).into_iter().chain(f.puck.all_with(SIM_BUTTON + 9)) {
        crate::livehud::set_visible(&mut vis, Some(e), false);
    }
    f.sims.clear();
    for (k, m) in pending.members.iter().enumerate().take(MAX_SIMS) {
        let mut w = template.clone();
        let (bw, bh) = (w.area[2] - w.area[0], w.area[3] - w.area[1]);
        let x = w.area[0] + k as f32 * SIM_STEP;
        w.area = [x, w.area[1], x + bw, w.area[1] + bh];
        w.id = 0;
        let s = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        let Some(r) = s.root else { continue };
        commands.entity(r).insert((SimButton, CasAction::Select(k), crate::icons::Tooltip(format!("{} {}", m.first, m.last))));
        if k == sel {
            commands.entity(r).insert(crate::layout::Selected);
        }
        if let Some((h, _)) = ui.image(&mut images, s3pkg::fnv64(AGE_ICONS[age_icon(m.age) as usize])) {
            commands.entity(r).insert(SetIcon(h));
        }
        f.sims.push(r);
    }
    // Add a Sim, after the last, while there's room.
    if let Some(add) = f.puck.id(ADD_SIM) {
        crate::livehud::set_visible(&mut vis, Some(add), pending.members.len() < MAX_SIMS && pending.premade.is_none());
    }
}

/// The ages' pictures, baby to elder (`CASPuck`'s, baked by name).
pub(crate) const AGE_ICONS: [&str; 7] = ["cas_basics_i_age_baby_r2", "cas_basics_i_age_toddler_r2", "cas_basics_i_age_child_r2", "cas_basics_i_age_teen_r2", "cas_basics_i_age_yadult_r2", "cas_basics_i_age_adult_r2", "cas_basics_i_age_elderly_r2"];

/// Which of the age pictures (`AGE_ICONS`) is a Sim's.
pub(crate) fn age_icon(a: crate::sim::Age) -> u8 {
    use crate::sim::Age;
    match a {
        Age::Baby => 0,
        Age::Toddler => 1,
        Age::Child => 2,
        Age::Teen => 3,
        Age::YoungAdult => 4,
        Age::Adult => 5,
        Age::Elder => 6,
    }
}

/// The character sheet: the open mode lit (its names put away while a panel of the game's own
/// is open beside it).
fn sheet(frame: Option<Res<CasFrame>>, tab: Res<crate::cas::CasSelected>, mut buttons: Query<&mut UiButton>, mut vis: Query<&mut Visibility>) {
    let Some(f) = frame else { return };
    crate::livehud::set_visible(&mut vis, f.sheet.id(SHEET_TEXT), !game_panel(tab.1));
    let open = mode_of(tab.1);
    for id in [SHEET_BASICS, SHEET_HAIR, SHEET_FACE, SHEET_CLOTHING, SHEET_CHARACTER] {
        if let Some(e) = f.sheet.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
            && b.selected != (id == open)
        {
            b.selected = id == open;
        }
    }
}

/// The puck's other buttons: cancel (back to the main menu), the camera's turns, the "…" menu
/// (remove this Sim, a new family, the town's families) and the options menu.
#[allow(clippy::too_many_arguments)]
fn puck_buttons(
    mut commands: Commands,
    frame: Option<Res<CasFrame>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut choices: MessageReader<crate::popupmenu::PopupChoice>,
    (mut next, mut exit, mut turn): (ResMut<NextState<AppState>>, MessageWriter<AppExit>, ResMut<crate::cas::CasTurn>),
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    (mut options, settings): (ResMut<crate::options::OptionsPanel>, Res<crate::options::Settings>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    families: Res<crate::cas::CasFamilies>,
) {
    let (Some(f), Some(mut ui)) = (frame, ui) else { return };
    let pressed = |id: u32| crate::livehud::pressed(&clicks, f.puck.id(id));
    if pressed(CANCEL) {
        next.set(AppState::MainMenu);
    }
    if pressed(ROTATE_LEFT) {
        turn.0 += std::f32::consts::FRAC_PI_4;
    }
    if pressed(ROTATE_RIGHT) {
        turn.0 -= std::f32::consts::FRAC_PI_4;
    }
    let _ = (ZOOM_IN, ZOOM_OUT);
    let at = windows.single().ok().and_then(|w| w.cursor_position()).unwrap_or(Vec2::new(900.0, 700.0));
    if pressed(MORE) {
        let mut items = vec!["Remove Sim".to_string(), "New Family".to_string()];
        if families.0 {
            items.push("Town Families".to_string());
        }
        crate::popupmenu::open_popup(&mut commands, &mut ui, &mut images, &mut fonts, MORE_MENU, &items, at);
    }
    if pressed(OPTIONS) {
        let items = [ui.localize("Ui/Caption/Options:Options").unwrap_or_else(|| "Options".into()), ui.localize("Ui/Caption/Options:QuitToMenu").unwrap_or_else(|| "Main Menu".into()), ui.localize("Ui/Caption/Options:QuitToWindows").unwrap_or_else(|| "Quit".into())];
        crate::popupmenu::open_popup(&mut commands, &mut ui, &mut images, &mut fonts, OPTIONS_MENU, &items, at);
    }
    for c in choices.read() {
        match (c.id, c.index) {
            (MORE_MENU, Some(0)) => {
                actions.write(crate::cas::CasActionRequest(CasAction::Remove));
            }
            (MORE_MENU, Some(1)) => {
                actions.write(crate::cas::CasActionRequest(CasAction::NewFamily));
            }
            (MORE_MENU, Some(2)) => {
                actions.write(crate::cas::CasActionRequest(CasAction::Families));
            }
            (OPTIONS_MENU, Some(0)) => crate::options::open_options(&mut commands, &mut options, &settings),
            (OPTIONS_MENU, Some(1)) => next.set(AppState::MainMenu),
            (OPTIONS_MENU, Some(2)) => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

/// Which Create a Sim tabs the game's own panels draw (the plain editing panel stands aside for
/// them).
pub fn game_panel(tab: CasTab) -> bool {
    GAME_CAS.load(Ordering::Relaxed)
        && match tab {
            CasTab::Basics => BASICS_UP.load(Ordering::Relaxed),
            CasTab::Hair => crate::cashair::HAIR_UP.load(Ordering::Relaxed),
            CasTab::Tops | CasTab::Bottoms | CasTab::Outfits | CasTab::Shoes => crate::casclothing::CLOTHING_UP.load(Ordering::Relaxed),
            CasTab::Traits => crate::caschar::CHARACTER_UP.load(Ordering::Relaxed),
            _ => false,
        }
}

/// Sets a window's shade (found by id anywhere under a layout window).
pub(crate) fn shade_window(w: &mut s3bake::ui::UiWindow, id: u32, argb: u32) {
    if w.id == id {
        w.shade = argb;
    }
    for c in &mut w.children {
        shade_window(c, id, argb);
    }
}

static BASICS_UP: AtomicBool = AtomicBool::new(false);

// `CASBasics`'s windows.
const BASICS_CLOSE: u32 = 0x05db_9700;
const FIRST_NAME: u32 = 0x05db_9701;
const LAST_NAME: u32 = 0x05db_9702;
const MALE: u32 = 0x05db_9703;
const FEMALE: u32 = 0x05db_9704;
/// Toddler, child, teen, young adult, adult, elder.
const AGE_BUTTONS: u32 = 0x05db_9705;
const RANDOM_NAME: u32 = 0x05db_970b;
const SKIN_SLIDER: u32 = 0x05db_9710;
const SKIN_RAMP: u32 = 0x05db_9711;
const SKIN_MORE: u32 = 0x05db_9712;
const WEIGHT_SLIDER: u32 = 0x05db_9715;
const FITNESS_SLIDER: u32 = 0x05db_9716;
const BREAST_HOLDER: u32 = 0x0a2f_4a50;
const DEFINITION_HOLDER: u32 = 0x0a2f_4a10;
const LIFE_STATE: u32 = 0x0d5e_1660;
const LIFE_STATE_MENU: u32 = 0x0d63_5cd0;
const LIFE_STATE_TEXT: u32 = 0x0d5f_1e60;
/// The life states the menu offers: human, then the supernaturals (`sim::Occult::ALL`
/// indices), and those this game hasn't (genie, ghost).
const LIFE_STATES: [(u32, Option<usize>, &str); 5] = [(0x0d5f_48c0, None, "Human"), (0x0d67_2540, Some(0), "Vampire"), (0x0d5f_1df0, Some(1), "Werewolf"), (0x0d5f_1e20, Some(2), "Witch"), (0x0d5f_1e10, Some(3), "Fairy")];
const LIFE_STATES_NOT_HERE: [u32; 2] = [0x0d65_e050, 0x0d65_d490];

/// The Basics panel on screen, and its sliders' last values (to hear them moved).
#[derive(Resource)]
pub struct CasBasics {
    s: Spawned,
    last: [f32; 3],
    shown: Option<(usize, bool, u8, Option<u8>, String, String)>,
    /// The name being typed into (first or last), and its words so far.
    typing: Option<(u32, String)>,
}

/// A life state's button in the menu.
#[derive(Component)]
struct LifeStateButton;

fn spawn_basics(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<CasFrame>>, basics: Option<Res<CasBasics>>) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if basics.is_some() || ui.layout("CASBasics").is_none() {
        return;
    }
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "CASBasics") else { return };
    let Some(root) = s.root else { return };
    commands.entity(root).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(5), Visibility::Hidden));
    // (As tall as its sliders need: the game's 438 and 50 a slider, from its top at 12.)
    commands.entity(root).entry::<Node>().and_modify(|mut n| n.height = Val::Px(438.0 + 2.0 * 50.0 - 12.0));
    // The skin tones' ramp along the slider, light to dark, under its plumbob.
    if let Some(slider) = s.id(SKIN_SLIDER) {
        let ramp = images.add(skin_ramp());
        let r = commands
            .spawn((ImageNode::new(ramp), Node { position_type: PositionType::Absolute, left: Val::Px(5.0), top: Val::Px(11.0), width: Val::Px(RAMP_SIZE.x), height: Val::Px(RAMP_SIZE.y), ..default() }, Pickable::IGNORE))
            .id();
        commands.entity(slider).insert_children(1, &[r]);
    }
    for id in [BASICS_CLOSE, SKIN_MORE, SKIN_RAMP, BREAST_HOLDER, DEFINITION_HOLDER].into_iter().chain(LIFE_STATES_NOT_HERE) {
        for e in s.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    // (The human basics: not the fairy's or the ghost's pages.)
    if let Some(e) = s.id(0x0d9a_5bd0) {
        commands.entity(e).insert(Visibility::Inherited);
    }
    for (id, action, tip) in [(FEMALE, CasAction::SetFemale(true), "Female"), (MALE, CasAction::SetFemale(false), "Male"), (RANDOM_NAME, CasAction::RandomName, "Random Name")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert((action, crate::icons::Tooltip(tip.into())));
        }
    }
    use crate::sim::Age;
    for (k, (age, name)) in [(Age::Toddler, "Toddler"), (Age::Child, "Child"), (Age::Teen, "Teen"), (Age::YoungAdult, "Young Adult"), (Age::Adult, "Adult"), (Age::Elder, "Elder")].into_iter().enumerate() {
        if let Some(e) = s.id(AGE_BUTTONS + k as u32) {
            commands.entity(e).insert((CasAction::SetAge(age), crate::icons::Tooltip(name.into())));
        }
    }
    for (id, state, name) in LIFE_STATES {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert((CasAction::LifeState(state), LifeStateButton, crate::icons::Tooltip(name.into())));
        }
    }
    for id in [FIRST_NAME, LAST_NAME] {
        if let Some(e) = s.id(id) {
            commands.entity(e).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
        }
    }
    BASICS_UP.store(true, Ordering::Relaxed);
    commands.insert_resource(CasBasics { s, last: [-1.0; 3], shown: None, typing: None });
}

/// The skin tones as a ramp, light to dark, with rounded ends (the game's ramp window's mask),
/// at the size it's drawn.
fn skin_ramp() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let tones = crate::sim::SKINS;
    let (w, h) = (RAMP_SIZE.x as usize, RAMP_SIZE.y as usize);
    let r = h as f32 * 0.5;
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let t = x as f32 / (w - 1) as f32 * (tones.len() - 1) as f32;
            let (i, f) = (t.floor() as usize, t.fract());
            let (a, b) = (tones[i.min(tones.len() - 1)], tones[(i + 1).min(tones.len() - 1)]);
            let c = Color::srgb(a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f).to_srgba();
            // (Its ends rounded: the distance past the end circles, softened over a pixel.)
            let (cx, cy) = ((x as f32 + 0.5).clamp(r, w as f32 - r), r);
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let alpha = (r - d + 0.5).clamp(0.0, 1.0);
            px.extend([(c.red * 255.0) as u8, (c.green * 255.0) as u8, (c.blue * 255.0) as u8, (alpha * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// The ramp's size (the game's ramp window, turned on its side).
const RAMP_SIZE: Vec2 = Vec2::new(203.0, 19.0);


/// The Basics panel: shown on its tab, lit for the Sim, its sliders and names heard.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn basics_panel(
    mut commands: Commands,
    basics: Option<ResMut<CasBasics>>,
    ui: Option<ResMut<UiAssets>>,
    mut images: ResMut<Assets<Image>>,
    mut pending: ResMut<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    (mut vis, mut buttons, mut sliders, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut crate::layout::UiSlider>, Query<&mut Text>),
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    life_buttons: Query<&Interaction, (Changed<Interaction>, With<LifeStateButton>)>,
    (mut actions, mut keyboard, mouse): (MessageWriter<crate::cas::CasActionRequest>, MessageReader<bevy::input::keyboard::KeyboardInput>, Res<ButtonInput<MouseButton>>),
) {
    let (Some(mut b), Some(mut ui)) = (basics, ui) else { return };
    let b = &mut *b;
    let on = selected.1 == CasTab::Basics;
    crate::livehud::set_visible(&mut vis, b.s.root, on);
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k).cloned() else { return };
    if !on {
        b.typing = None;
        return;
    }
    // The life state menu: opened by its button, closed by a choice.
    if crate::livehud::pressed(&clicks, b.s.id(LIFE_STATE))
        && let Some(e) = b.s.id(LIFE_STATE_MENU)
        && let Ok(mut v) = vis.get_mut(e)
    {
        *v = if *v == Visibility::Hidden { Visibility::Inherited } else { Visibility::Hidden };
    }
    if life_buttons.iter().any(|i| *i == Interaction::Pressed) {
        crate::livehud::set_visible(&mut vis, b.s.id(LIFE_STATE_MENU), false);
    }
    // Typing a name: a click on its box, then the keys; Enter (or a click elsewhere) keeps it.
    for id in [FIRST_NAME, LAST_NAME] {
        if crate::livehud::pressed(&clicks, b.s.id(id)) {
            let now = if id == FIRST_NAME { sim.first.clone() } else { pending.last_name.clone() };
            b.typing = Some((id, now));
        }
    }
    let mut finished = false;
    if let Some((id, words)) = b.typing.as_mut() {
        use bevy::input::keyboard::Key;
        for ev in keyboard.read() {
            if !ev.state.is_pressed() {
                continue;
            }
            match &ev.logical_key {
                Key::Character(c) if words.chars().count() < 20 => words.push_str(c.as_str()),
                Key::Space if words.chars().count() < 20 => words.push(' '),
                Key::Backspace => {
                    words.pop();
                }
                Key::Enter | Key::Escape | Key::Tab => finished = true,
                _ => {}
            }
        }
        let typed = words.trim().to_string();
        if !typed.is_empty() {
            if *id == FIRST_NAME {
                pending.members[k].first = typed;
            } else {
                for m in pending.members.iter_mut() {
                    m.last = typed.clone();
                }
                pending.last_name = typed;
            }
        }
        let clicked_away = mouse.just_pressed(MouseButton::Left) && !b.s.id(*id).is_some_and(|e| clicks.get(e).is_ok_and(|(_, i)| *i == Interaction::Pressed));
        if clicked_away {
            finished = true;
        }
    } else {
        keyboard.clear();
    }
    if finished {
        b.typing = None;
        actions.write(crate::cas::CasActionRequest(CasAction::Refresh));
    }
    // The sliders moved: the Sim's skin, weight and fitness.
    let read = |sl: &Query<&mut crate::layout::UiSlider>, id: u32| b.s.id(id).and_then(|e| sl.get(e).ok()).map(|s| s.value);
    for (n, id) in [SKIN_SLIDER, WEIGHT_SLIDER, FITNESS_SLIDER].into_iter().enumerate() {
        let Some(v) = read(&sliders, id) else { continue };
        if b.last[n] >= 0.0 && v != b.last[n] {
            let action = match n {
                0 => CasAction::SetSkin(v / 256.0),
                1 => CasAction::SetWeight(v / 128.0 - 1.0),
                _ => CasAction::SetFitness(v / 256.0),
            };
            actions.write(crate::cas::CasActionRequest(action));
            b.last[n] = v;
        }
    }
    // Shown for the Sim: gender, age, life state, the names, and the sliders where they are.
    let state = sim.occult.and_then(|o| crate::sim::Occult::ALL.iter().position(|x| *x == o)).map(|i| i as u8);
    let caret = |id: u32, s: &str| if b.typing.as_ref().is_some_and(|t| t.0 == id) { format!("{s}|") } else { s.to_string() };
    let first = caret(FIRST_NAME, &b.typing.as_ref().filter(|t| t.0 == FIRST_NAME).map_or(sim.first.clone(), |t| t.1.clone()));
    let last = caret(LAST_NAME, &b.typing.as_ref().filter(|t| t.0 == LAST_NAME).map_or(pending.last_name.clone(), |t| t.1.clone()));
    let want = (k, sim.female, age_icon(sim.age), state, first.clone(), last.clone());
    // (The sliders put where the Sim is: on opening, for another Sim, and when changed other
    // than by them (a random Sim, a menu), unless being dragged.)
    let values = [crate::simbody::tone_of(&sim) * 256.0, (sim.weight + 1.0) * 128.0, sim.fitness * 256.0].map(f32::round);
    let dragging = mouse.pressed(MouseButton::Left);
    for (n, id) in [SKIN_SLIDER, WEIGHT_SLIDER, FITNESS_SLIDER].into_iter().enumerate() {
        if (values[n] - b.last[n]).abs() > 1.0
            && (!dragging || b.last[n] < 0.0)
            && let Some(mut s) = b.s.id(id).and_then(|e| sliders.get_mut(e).ok())
        {
            s.value = values[n];
            b.last[n] = values[n];
        }
    }
    if b.shown.as_ref() == Some(&want) {
        return;
    }
    b.shown = Some(want);
    let mut set = |id: u32, on: bool| {
        if let Some(e) = b.s.id(id)
            && let Ok(mut bt) = buttons.get_mut(e)
            && bt.selected != on
        {
            bt.selected = on;
        }
    };
    set(FEMALE, sim.female);
    set(MALE, !sim.female);
    for n in 0..6u32 {
        set(AGE_BUTTONS + n, age_icon(sim.age) == (n + 1) as u8);
    }
    for (id, s, _) in LIFE_STATES {
        set(id, s.map(|i| i as u8) == state);
    }
    for (id, t) in [(FIRST_NAME, &first), (LAST_NAME, &last)] {
        crate::livehud::set_text(&mut texts, b.s.text(id), t);
    }
    // The life state's name and picture on its button.
    let (_, _, name) = LIFE_STATES.iter().find(|(_, s, _)| s.map(|i| i as u8) == state).copied().unwrap_or(LIFE_STATES[0]);
    crate::livehud::set_text(&mut texts, b.s.text(LIFE_STATE_TEXT), name);
    let icon_of = |id: u32| match ui.find("CASBasics", id).and_then(|w| w.drawable.clone()) {
        Some(s3bake::ui::UiDrawable::Multi(list)) => list.iter().find_map(|d| match d {
            s3bake::ui::UiDrawable::Image { image, .. } => Some(*image),
            _ => None,
        }),
        _ => None,
    };
    let chosen = LIFE_STATES.iter().find(|(_, s, _)| s.map(|i| i as u8) == state).map_or(LIFE_STATES[0].0, |x| x.0);
    if let (Some(e), Some(key)) = (b.s.id(LIFE_STATE), icon_of(chosen))
        && let Some((h, _)) = ui.image(&mut images, key)
    {
        commands.entity(e).insert(SetIcon(h));
    }
}
