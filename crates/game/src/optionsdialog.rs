//! The Options dialog in the game's own look (`OptionsDialog`, UI.package), driven as UI.dll's
//! `OptionsDialog` drives it: its tabs along the top (the game's skewer tabs with their icons),
//! a page each. Sound: the voices', effects', music's and ambience's volume sliders and their
//! mute boxes. General: the camera moved at the screen's edges, the clock's twelve hours, the
//! camera's turning inverted. Game Options: free will (off, low, high), none for the selected
//! Sim, opportunities offered or not, aging on or off, and the lifespan (short to epic) with
//! how many Sim days a life lasts and each life stage's share. Graphics:
//! the lighting detail (shadows). Changes are tried out in the dialog and kept with OK (or
//! dropped with Cancel); each page can be put back to its defaults.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::layout::{SetIcon, Spawned, UiAssets, UiButton, UiCombo, UiSlider};
use crate::livehud::{pressed, set_text, set_visible};
use crate::options::{FreeWill, Lifespan, Settings};

pub struct OptionsDialogPlugin;

impl Plugin for OptionsDialogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (enable, open_dialog, dialog_controls).chain());
    }
}

/// Whether the game's own dialog can be shown (else the plain options panel).
pub static GAME_OPTIONS: AtomicBool = AtomicBool::new(false);

fn enable(ui: Option<Res<UiAssets>>, mut done: Local<bool>) {
    if !*done && let Some(u) = ui {
        GAME_OPTIONS.store(u.layout("OptionsDialog").is_some() && u.layout("SkewerTabControl").is_some(), Ordering::Relaxed);
        *done = true;
    }
}

/// Asked for (the menus' Options).
#[derive(Resource)]
pub struct OpenOptionsDialog;

/// The tabs shown (tab window, page) and their tooltips' keys: Graphics, Sound, General, Game
/// Options.
const TABS: [(u32, u32, &str); 4] = [
    (0xa83f_6201, 0xa83f_6210, "Ui/Caption/Options:Graphics"),
    (0xa83f_6202, 0xa83f_6230, "Ui/Caption/Options:Sound"),
    (0xa83f_6203, GENERAL_PAGE, "Ui/Caption/Options:GeneralSettings"),
    (0xa83f_6255, GAME_PAGE, "Ui/Caption/Options:GameOptions"),
];
const GENERAL_PAGE: u32 = 0xa83f_6250;
const GAME_PAGE: u32 = 0xb4b9_f816;
/// The tabs not used here (video capture, music, online, the packs' pages: their ids are
/// shared with buttons on the pages, so only those in the tab row), and the pages.
const TAB_ROW: u32 = 0xa83f_6200;
const HIDDEN_TABS: [u32; 7] = [0xa83f_6204, 0xa83f_6205, 0x0c88_58d0, 0xa83f_6257, 0xa83f_6256, 0xa83f_6258, 0x0dc3_4fd0];
const HIDDEN: [u32; 6] = [0xa83f_6270, 0xa83f_6290, 0x0c87_2d80, 0x0d3a_6210, 0x0dc2_55b0, 0x0d3a_62ff];
// General: its boxes, and what this game has no use for (the custom content warning, playing
// on minimized, the memories, tutorials, the store, telemetry, interactive loading screens,
// Facebook).
const EDGE_SCROLL: u32 = 0xa83f_6251;
const TWELVE_HOUR: u32 = 0xa83f_6253;
const INVERT_H: u32 = 0xa83f_625f;
const INVERT_V: u32 = 0xa83f_6260;
const GENERAL_DEFAULTS: u32 = 0xa83f_6259;
const GENERAL_HIDDEN: [u32; 10] = [0xa83f_6254, 0xa83f_6252, 0x0b30_3980, 0xa83f_6263, 0xa83f_6261, 0xa83f_6258, 0xa83f_6264, 0xa83f_6265, 0x0fdc_2830, 0x0b34_5260];
const OKAY: u32 = 0xa83f_6208;
const CANCEL: u32 = 0xa83f_6209;
const TAB_STEP: f32 = 54.0;
// Sound: the volume sliders (0 to 255) and their mute boxes.
const VOLUMES: [u32; 4] = [0xa83f_6231, 0xa83f_6233, 0xa83f_6235, 0xa83f_6237];
const MUTES: [u32; 4] = [0xa83f_6232, 0xa83f_6234, 0xa83f_6236, 0xa83f_6238];
const AUDIO_QUALITY: u32 = 0xa83f_6239;
const BACKGROUND_SOUND: u32 = 0xa83f_623b;
const SPEAKERS: u32 = 0xa83f_623d;
const SOUND_DEFAULTS: u32 = 0xa83f_623c;
// Game options.
const AUTONOMY: u32 = 0xa83f_625a;
const AUTONOMY_TEXT: u32 = 0xa83f_625b;
const AGING: u32 = 0xa83f_6256;
const LIFESPAN: u32 = 0xa83f_625c;
const LIFESPAN_TEXT: u32 = 0xa83f_625d;
const LIFESPAN_DESC: u32 = 0xa83f_625e;
const GAME_DEFAULTS: u32 = 0xb4b9_f817;
const NO_SELECTED_AUTONOMY: u32 = 0xb4b9_f820;
const SUPPRESS_OPPORTUNITIES: u32 = 0xa83f_6266;
/// The story progression box (on the packs' Demographics page, `0x0dc2_55b0`): moved to the
/// Game Options page, where the pets' free will was (its holder, `0xb4b9_f821`).
const STORY_PROGRESSION: u32 = 0xa83f_6257;
const DEMOGRAPHICS_PAGE: u32 = 0x0dc2_55b0;
const STORY_HOLDER: u32 = 0xb4b9_f821;
/// (The pets' free will and ages, and the species they're for.)
const GAME_HIDDEN: [u32; 3] = [0xb4b9_f818, 0xb4b9_f822, 0xb4b9_f831];
/// The human life stages' sliders, their words and their days (baby to elder).
const STAGES: [(u32, u32); 7] = [(0xb4b9_f801, 0xb4b9_f802), (0xb4b9_f804, 0xb4b9_f805), (0xb4b9_f807, 0xb4b9_f808), (0xb4b9_f80a, 0xb4b9_f80b), (0xb4b9_f80d, 0xb4b9_f80e), (0xb4b9_f810, 0xb4b9_f811), (0xb4b9_f813, 0xb4b9_f814)];
/// (An elder's least days, as the game's options show it.)
const ELDER_MINIMUM: f32 = 17.0;
const STAGE_AGES: [crate::sim::Age; 7] = [crate::sim::Age::Baby, crate::sim::Age::Toddler, crate::sim::Age::Child, crate::sim::Age::Teen, crate::sim::Age::YoungAdult, crate::sim::Age::Adult, crate::sim::Age::Elder];
// Graphics.
const SCREEN_SIZE: u32 = 0xa83f_6211;
const REFRESH: u32 = 0xa83f_6215;
const LIGHTING: u32 = 0xa83f_6226;
const LIGHTING_TEXT: u32 = 0xa83f_6227;
const GRAPHICS_DEFAULTS: u32 = 0xa83f_6212;
/// The graphics page's controls this game doesn't change (greyed).
const GRAPHICS_INERT: [u32; 12] = [0xa83f_6217, 0xa83f_6211, 0xa83f_6215, 0x0a5d_6ff0, 0xa83f_6220, 0xa83f_6222, 0xa83f_6224, 0xa83f_6322, 0xa83f_6320, 0xa83f_622e, 0xa83f_6228, 0xa83f_622b];
const GRAPHICS_INERT_BUTTONS: [u32; 2] = [0xa83f_622d, 0xa83f_622c];

/// The dialog on screen: its windows, the page shown, the settings as changed so far.
#[derive(Resource)]
struct OptionsDialog {
    s: Spawned,
    backdrop: Entity,
    page: usize,
    work: Settings,
    tabs: Vec<Entity>,
    shown: Option<(usize, String)>,
    /// The sliders set from the settings (once they're there): read back only after.
    synced: bool,
    /// The story progression box, where it's been moved to.
    story_box: Option<Entity>,
}

/// (Test hook: OPTIONS_TAB=<n> opens on that page.)
#[derive(Resource)]
struct StartTab(usize);

/// A tab's button.
#[derive(Component)]
struct OptionsTab(usize);

#[allow(clippy::too_many_arguments)]
fn open_dialog(
    mut commands: Commands,
    want: Option<Res<OpenOptionsDialog>>,
    open: Option<Res<OptionsDialog>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    settings: Res<Settings>,
) {
    if want.is_none() {
        return;
    }
    commands.remove_resource::<OpenOptionsDialog>();
    let Some(mut ui) = ui else { return };
    if open.is_some() {
        return;
    }
    // (Behind it the screen darkens, and takes no clicks.)
    let backdrop = commands
        .spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4)), GlobalZIndex(69), Interaction::default(), crate::hud::BlocksWorld))
        .id();
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "OptionsDialog") else { return };
    let Some(root) = s.root else { return };
    commands.entity(root).insert((GlobalZIndex(70), Visibility::Inherited, ChildOf(backdrop)));
    for id in HIDDEN.into_iter().chain(GAME_HIDDEN) {
        for e in s.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    let story_box = s.id(DEMOGRAPHICS_PAGE).and_then(|p| s.within(p, STORY_PROGRESSION));
    if let (Some(b), Some(holder)) = (story_box, s.id(STORY_HOLDER)) {
        commands.entity(b).insert((
            ChildOf(holder),
            Node { position_type: PositionType::Absolute, left: Val::Px(2.0), top: Val::Px(62.0), width: Val::Px(263.0), height: Val::Px(28.0), ..default() },
            Visibility::Inherited,
        ));
    }
    if let Some(page) = s.id(GENERAL_PAGE) {
        for id in GENERAL_HIDDEN {
            if let Some(e) = s.within(page, id) {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
    }
    if let Some(row) = s.id(TAB_ROW) {
        for id in HIDDEN_TABS {
            if let Some(e) = s.within(row, id) {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
    }
    // The tabs, left to right, each the game's skewer tab with its page's icon.
    let tab_template = ui.layout("SkewerTabControl").cloned();
    let mut tabs = Vec::new();
    for (i, (tab, _, tip)) in TABS.iter().enumerate() {
        let Some(t) = s.id(TAB_ROW).and_then(|row| s.within(row, *tab)) else { continue };
        commands.entity(t).insert(Node { position_type: PositionType::Absolute, left: Val::Px(i as f32 * TAB_STEP), top: Val::Px(0.0), width: Val::Px(56.0), height: Val::Px(58.0), ..default() });
        let Some(mut w) = tab_template.clone() else { continue };
        w.area = [1.0, 0.0, 55.0, 58.0];
        let b = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, t);
        if let Some(r) = b.root {
            commands.entity(r).insert((OptionsTab(i), crate::icons::Tooltip(ui.localize(tip).unwrap_or_default())));
            let icon = ui.find("OptionsDialog", *tab).map(|w| w.icon).unwrap_or(0);
            if let Some((h, _)) = ui.image(&mut images, icon) {
                commands.entity(r).insert(SetIcon(h));
            }
            tabs.push(r);
        }
    }
    // (The stage sliders show the lifespan's days, not set one by one here; the page's
    // other inert controls are greyed.)
    for (slider, _) in STAGES {
        if let Some(e) = s.id(slider) {
            commands.entity(e).remove::<Interaction>().insert(Pickable::IGNORE);
        }
    }
    for id in GRAPHICS_INERT.into_iter().chain([AUDIO_QUALITY, SPEAKERS]) {
        if let Some(e) = s.id(id) {
            commands.entity(e).remove::<Interaction>().insert(Pickable::IGNORE);
        }
    }
    commands.insert_resource(OptionsDialog { s, backdrop, page: 0, work: settings.clone(), tabs, shown: None, synced: false, story_box });
    let _ = std::env::var("OPTIONS_TAB").ok().and_then(|t| t.parse::<usize>().ok()).map(|t| commands.insert_resource(StartTab(t)));
}

/// The words a slider's setting is shown in.
fn lifespan_of(v: f32) -> Lifespan {
    Lifespan::ALL[(v as usize).min(4)]
}

fn stage_days(a: crate::sim::Age) -> f32 {
    let d = crate::aging::stage_days(a);
    if d.is_finite() { d } else { ELDER_MINIMUM }
}

fn lifespan_days(l: Lifespan) -> f32 {
    STAGE_AGES.iter().map(|a| stage_days(*a)).sum::<f32>() * l.factor()
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dialog_controls(
    mut commands: Commands,
    d: Option<ResMut<OptionsDialog>>,
    ui: Option<Res<UiAssets>>,
    mut settings: ResMut<Settings>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    tabs: Query<(&Interaction, &OptionsTab), Changed<Interaction>>,
    (mut sliders, mut buttons): (Query<&mut UiSlider>, Query<&mut UiButton>),
    (mut vis, mut texts): (Query<&mut Visibility>, Query<&mut Text>),
    keys: Res<ButtonInput<KeyCode>>,
    start: Option<Res<StartTab>>,
    (mut combos, windows): (Query<&mut UiCombo>, Query<&Window, With<bevy::window::PrimaryWindow>>),
) {
    let (Some(mut d), Some(ui)) = (d, ui) else { return };
    let d = &mut *d;
    if let Some(t) = start.as_ref() {
        d.page = t.0.min(TABS.len() - 1);
        commands.remove_resource::<StartTab>();
    }
    for (i, t) in &tabs {
        if *i == Interaction::Pressed {
            d.page = t.0;
        }
    }
    let w = &mut d.work;
    // Read the controls into the settings being changed.
    let slider = |sl: &Query<&mut UiSlider>, id: u32| d.s.id(id).and_then(|e| sl.get(e).ok()).map(|s| s.value);
    for (k, id) in VOLUMES.into_iter().enumerate() {
        if let Some(v) = slider(&sliders, id) {
            let v = v / 255.0;
            let ch = match k {
                0 => &mut w.voices,
                1 => &mut w.effects,
                2 => &mut w.music,
                _ => &mut w.ambience,
            };
            if d.synced {
                *ch = v;
            }
        }
    }
    for (k, id) in MUTES.into_iter().enumerate() {
        if pressed(&clicks, d.s.id(id)) {
            w.muted[k] = !w.muted[k];
        }
    }
    if d.synced {
        if let Some(v) = slider(&sliders, AUTONOMY) {
            w.free_will = [FreeWill::Off, FreeWill::Normal, FreeWill::High][(v as usize).min(2)];
        }
        if let Some(v) = slider(&sliders, LIFESPAN) {
            w.lifespan = lifespan_of(v);
        }
        if let Some(v) = slider(&sliders, LIGHTING) {
            w.shadows = v >= 1.0;
        }
    }
    let on_page = |page: u32, id: u32| d.s.id(page).and_then(|p| d.s.within(p, id));
    let aging_box = on_page(GAME_PAGE, AGING);
    if pressed(&clicks, aging_box) {
        w.aging = !w.aging;
    }
    for (id, v) in [
        (on_page(GENERAL_PAGE, EDGE_SCROLL), &mut w.edge_scroll),
        (on_page(GENERAL_PAGE, TWELVE_HOUR), &mut w.twelve_hour),
        (on_page(GENERAL_PAGE, INVERT_H), &mut w.invert_h),
        (on_page(GENERAL_PAGE, INVERT_V), &mut w.invert_v),
        (on_page(GAME_PAGE, NO_SELECTED_AUTONOMY), &mut w.no_selected_autonomy),
        (on_page(GAME_PAGE, SUPPRESS_OPPORTUNITIES), &mut w.suppress_opportunities),
        (d.story_box, &mut w.story_progression),
    ] {
        if pressed(&clicks, id) {
            *v = !*v;
        }
    }
    // Restore Defaults: this page's settings as they come.
    let defaults = Settings::default();
    if pressed(&clicks, d.s.id(SOUND_DEFAULTS)) {
        (w.voices, w.effects, w.music, w.ambience, w.muted) = (defaults.voices, defaults.effects, defaults.music, defaults.ambience, defaults.muted);
        d.shown = None;
        d.synced = false;
    }
    if pressed(&clicks, d.s.id(GAME_DEFAULTS)) {
        (w.free_will, w.aging, w.lifespan) = (defaults.free_will, defaults.aging, defaults.lifespan);
        (w.no_selected_autonomy, w.suppress_opportunities, w.story_progression) = (defaults.no_selected_autonomy, defaults.suppress_opportunities, defaults.story_progression);
        d.shown = None;
        d.synced = false;
    }
    if pressed(&clicks, d.s.id(GENERAL_DEFAULTS)) {
        (w.edge_scroll, w.twelve_hour, w.invert_h, w.invert_v) = (defaults.edge_scroll, defaults.twelve_hour, defaults.invert_h, defaults.invert_v);
        d.shown = None;
    }
    if pressed(&clicks, d.s.id(GRAPHICS_DEFAULTS)) {
        w.shadows = defaults.shadows;
        d.shown = None;
        d.synced = false;
    }
    // OK keeps the changes; Cancel (or Escape) drops them.
    let ok = pressed(&clicks, d.s.id(OKAY)) || keys.just_pressed(KeyCode::Enter);
    let cancel = pressed(&clicks, d.s.id(CANCEL)) || keys.just_pressed(KeyCode::Escape);
    if ok || cancel {
        if ok {
            *settings = d.work.clone();
            settings.save();
        }
        commands.entity(d.backdrop).try_despawn();
        commands.remove_resource::<OptionsDialog>();
        return;
    }
    // Show the settings (on opening, and as they change).
    let w = &d.work;
    let now = (d.page, format!("{w:?}"));
    let first = !d.synced;
    if d.shown.as_ref() == Some(&now) && d.synced {
        return;
    }
    d.shown = Some(now);
    for (i, (_, page, _)) in TABS.iter().enumerate() {
        set_visible(&mut vis, d.s.id(*page), i == d.page);
    }
    for (i, t) in d.tabs.iter().enumerate() {
        if let Ok(mut b) = buttons.get_mut(*t)
            && b.selected != (i == d.page)
        {
            b.selected = i == d.page;
        }
    }
    let found = std::cell::Cell::new(true);
    let mut set_slider = |id: u32, v: f32| {
        match d.s.id(id).and_then(|e| sliders.get_mut(e).ok()) {
            Some(mut s) => {
                if s.value != v {
                    s.value = v;
                }
            }
            None => found.set(false),
        }
    };
    if first {
        for (id, v) in VOLUMES.into_iter().zip([w.voices, w.effects, w.music, w.ambience]) {
            set_slider(id, (v * 255.0).round());
        }
        set_slider(AUTONOMY, match w.free_will {
            FreeWill::Off => 0.0,
            FreeWill::Normal => 1.0,
            FreeWill::High => 2.0,
        });
        set_slider(LIFESPAN, Lifespan::ALL.iter().position(|l| *l == w.lifespan).unwrap_or(2) as f32);
        set_slider(LIGHTING, if w.shadows { 2.0 } else { 0.0 });
        set_slider(AUDIO_QUALITY, 2.0);
    }
    if first {
        d.synced = found.get();
        // (What the screen and speakers are, shown but not chosen here.)
        let size = windows.single().map(|w| format!("{} x {}", w.physical_width(), w.physical_height())).unwrap_or_default();
        for (id, item) in [(SCREEN_SIZE, size), (REFRESH, "60".to_string()), (SPEAKERS, "Stereo".to_string())] {
            if let Some(mut c) = d.s.id(id).and_then(|e| combos.get_mut(e).ok())
                && c.items.is_empty()
            {
                c.items = vec![item];
            }
        }
    }
    for (k, (slider, text)) in STAGES.iter().enumerate() {
        let days = (stage_days(STAGE_AGES[k]) * w.lifespan.factor()).round().max(1.0);
        set_slider(*slider, days.min(100.0));
        set_text(&mut texts, d.s.text(*text), &format!("{days:.0} Days"));
    }
    let word = |key: &str| ui.localize(key).unwrap_or_default();
    set_text(&mut texts, d.s.text(AUTONOMY_TEXT), &word(match w.free_will {
        FreeWill::Off => "Ui/Caption/Options:Off",
        FreeWill::Normal => "Ui/Caption/Options:LowFreeWill",
        FreeWill::High => "Ui/Caption/Options:HighFreeWill",
    }));
    set_text(&mut texts, d.s.text(LIFESPAN_TEXT), &word(match w.lifespan {
        Lifespan::Short => "Ui/Caption/Options:Short",
        Lifespan::Medium => "Ui/Caption/Options:Medium",
        Lifespan::Normal => "Ui/Caption/Options:Normal",
        Lifespan::Long => "Ui/Caption/Options:Long",
        Lifespan::Epic => "Ui/Caption/Options:Epic",
    }));
    let desc = word("Ui/Caption/Options:AgingDescription").replace("{0.Number}", &format!("{:.0}", lifespan_days(w.lifespan)));
    set_text(&mut texts, d.s.text(LIFESPAN_DESC), &desc);
    set_text(&mut texts, d.s.text(LIGHTING_TEXT), &word(if w.shadows { "Ui/Caption/Options:High" } else { "Ui/Caption/Options:Low" }));
    if let Some(mut b) = aging_box.and_then(|e| buttons.get_mut(e).ok())
        && (b.selected != w.aging || b.disabled)
    {
        b.selected = w.aging;
        b.disabled = false;
    }
    let mut set_box = |id: u32, on: bool, off: bool| {
        if let Some(mut b) = d.s.id(id).and_then(|e| buttons.get_mut(e).ok())
            && (b.selected != on || b.disabled != off)
        {
            b.selected = on;
            b.disabled = off;
        }
    };
    for (k, id) in MUTES.into_iter().enumerate() {
        set_box(id, w.muted[k], false);
    }
    set_box(BACKGROUND_SOUND, true, true);
    for id in GRAPHICS_INERT_BUTTONS.into_iter().chain([0xa83f_6217, 0xa83f_622b]) {
        set_box(id, id != 0xa83f_6217, true);
    }
    let mut set_page_box = |page: u32, id: u32, on: bool| {
        if let Some(mut b) = d.s.id(page).and_then(|p| d.s.within(p, id)).and_then(|e| buttons.get_mut(e).ok())
            && (b.selected != on || b.disabled)
        {
            b.selected = on;
            b.disabled = false;
        }
    };
    set_page_box(GENERAL_PAGE, EDGE_SCROLL, w.edge_scroll);
    set_page_box(GENERAL_PAGE, TWELVE_HOUR, w.twelve_hour);
    set_page_box(GENERAL_PAGE, INVERT_H, w.invert_h);
    set_page_box(GENERAL_PAGE, INVERT_V, w.invert_v);
    set_page_box(GAME_PAGE, NO_SELECTED_AUTONOMY, w.no_selected_autonomy);
    set_page_box(GAME_PAGE, SUPPRESS_OPPORTUNITIES, w.suppress_opportunities);
    if let Some(mut b) = d.story_box.and_then(|e| buttons.get_mut(e).ok())
        && (b.selected != w.story_progression || b.disabled)
    {
        b.selected = w.story_progression;
        b.disabled = false;
    }
}
