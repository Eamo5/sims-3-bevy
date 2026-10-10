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
        app.add_systems(Update, (spawn_cas_frame, skewer, sheet, puck_buttons).chain().run_if(in_state(AppState::CreateHousehold)))
            .add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| commands.remove_resource::<CasFrame>());
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
struct SimButton(usize);

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
        commands.entity(r).insert((SimButton(k), CasAction::Select(k), crate::icons::Tooltip(format!("{} {}", m.first, m.last))));
        if k == sel {
            commands.entity(r).insert(crate::layout::Selected);
        }
        if let Some((h, _)) = ui.image(&mut images, s3pkg::fnv64(s3bake::ui::NAMED_IMAGES[age_icon(m.age) as usize])) {
            commands.entity(r).insert(SetIcon(h));
        }
        f.sims.push(r);
    }
    // Add a Sim, after the last, while there's room.
    if let Some(add) = f.puck.id(ADD_SIM) {
        crate::livehud::set_visible(&mut vis, Some(add), pending.members.len() < MAX_SIMS && pending.premade.is_none());
    }
}

/// Which of the age pictures (`NAMED_IMAGES`, baby to elder) is a Sim's.
fn age_icon(a: crate::sim::Age) -> u8 {
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

/// The character sheet: the open mode lit.
fn sheet(frame: Option<Res<CasFrame>>, tab: Res<crate::cas::CasSelected>, mut buttons: Query<&mut UiButton>) {
    let Some(f) = frame else { return };
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
