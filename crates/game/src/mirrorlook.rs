//! A mirror's Change Appearance in the game's own look, as UI.dll's mirror mode shows it:
//! `CASMirrorSheet`, its Hair, Makeup and Facial Hair down a sheet with their names, and, one
//! chosen, the names faded and the sheet slid back to its buttons beside Create a Sim's own
//! panels for it: `CASPhysical` with `CASHair` (the styles in the game's cells, the colours its
//! swatches) for hair and facial hair, `CASFacialDetails` with its mirror tabs and `CASMakeup`
//! (eye shadow and lipstick) for make-up. A click wears or dyes at once. A panel's close button
//! goes back to the sheet; the sheet's back button (the dresser sheet's own) or Escape puts the
//! mirror away.

use bevy::prelude::*;
use s3bake::ui::UiWindow;
use s3formats::sim::{CT_BEARD, CT_EYESHADOW, CT_HAIR, CT_LIPSTICK};

use crate::cashair::{ColourCell, StyleCell};
use crate::casface::MakeupCell;
use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar, edit_windows};
use crate::planner::OutfitPlanner;
use crate::sim::{Age, Sim};
use crate::simbody::OutfitKind;

pub struct MirrorLookPlugin;

impl Plugin for MirrorLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (open_mirror, mirror_controls, mirror_panels).chain().run_if(in_state(crate::PlayMode::Live)))
            .add_systems(OnExit(crate::PlayMode::Live), |mut commands: Commands, look: Option<Res<MirrorLook>>| {
                if let Some(l) = look {
                    l.despawn(&mut commands);
                }
                commands.remove_resource::<MirrorLook>();
            });
    }
}

/// Whether the game's mirror panels are to hand (else `planner`'s plain panel).
pub fn available(ui: &UiAssets) -> bool {
    ["CASMirrorSheet", "CASDresserSheet", "CASPhysical", "CASHair", "CASFacialDetails", "CASMakeup", "GenericCasItem", "HairColorPresetGridItem"].iter().all(|n| ui.layout(n).is_some())
}

// `CASMirrorSheet`'s: its buttons (hair, make-up, facial hair) and their names, the names'
// window and the back panel.
const SHEET_BUTTONS: u32 = 0x0077_7000;
const SHEET_WORDS: u32 = 0x0077_7010;
const NAMES: u32 = 0x0077_7020;
const PANEL: u32 = 0x0077_7021;
/// How far the back panel glides back with the names gone (its `Glide` effect).
const PANEL_GLIDE: f32 = -79.0;
/// The dresser sheet's back button (borrowed: the mirror's sheet has only a stylist's).
const BACK: u32 = 0x0077_7023;
// `CASFacialDetails`'s: its full and plastic surgeon's tabs (put away), its mirror's, the
// mirror's make-up button and the close button.
const FULL_TABS: u32 = 0x1000;
const SURGEON_TABS: u32 = 0x0b98_4090;
const MIRROR_TABS: u32 = 0x1001;
const MIRROR_MAKEUP: u32 = 0x05db_9c0b;
const DETAILS_CLOSE: u32 = 0x05db_9c00;
// (`CASPhysical`'s parts this game hasn't.)
const PHYSICAL_NOT_HERE: [u32; 2] = [0x05db_9c09, 0x05db_9c0c];

/// What's being changed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Hair,
    Makeup,
    Beard,
}

const VIEWS: [View; 3] = [View::Hair, View::Makeup, View::Beard];

/// The mirror's panels on screen, which is showing, and the cells shown.
#[derive(Resource)]
pub struct MirrorLook {
    sheet: Spawned,
    physical: Spawned,
    hair: Spawned,
    details: Spawned,
    makeup: Spawned,
    /// The sheet's back panel, and where it sits.
    panel: Option<(Entity, f32)>,
    view: Option<View>,
    lipstick: bool,
    styles: Option<Entity>,
    colours: Option<Entity>,
    cells: Option<Entity>,
    bar: Option<Entity>,
    scroll: usize,
    shown: Option<Shown>,
}

type Shown = (Entity, Option<View>, bool, Option<s3bake::Key>, [u8; 3], usize, bool);

impl MirrorLook {
    fn despawn(&self, commands: &mut Commands) {
        for r in [self.sheet.root, self.physical.root, self.hair.root, self.details.root, self.makeup.root].into_iter().flatten() {
            commands.entity(r).try_despawn();
        }
    }
}

/// Of the windows under `w`, takes those `drop` says out.
fn retain_windows(w: &mut UiWindow, drop: &impl Fn(&UiWindow) -> bool) {
    w.children.retain(|c| !drop(c));
    for c in &mut w.children {
        retain_windows(c, drop);
    }
}

/// Facial hair for grown men (as `CASPhysical` shows its button).
fn beard_ok(sim: &Sim) -> bool {
    !sim.female && matches!(sim.age, Age::YoungAdult | Age::Adult | Age::Elder)
}

/// The hair they wear (theirs, or the one their outfit gives them).
fn worn_hair(cas: &crate::simbody::CasData, sim: &Sim) -> Option<s3bake::Key> {
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look);
    crate::simbody::pick_outfit_for(cas, sim, &mut rng, OutfitKind::Everyday).hair.map(|p| p.key)
}

#[allow(clippy::too_many_arguments)]
fn open_mirror(
    mut commands: Commands,
    planner: Option<Res<OutfitPlanner>>,
    look: Option<Res<MirrorLook>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    sims: Query<&Sim>,
    cas: Option<Res<crate::simbody::CasData>>,
) {
    // (Put away with the mirror.)
    let Some(p) = planner.filter(|p| p.looks) else {
        if let Some(l) = look {
            l.despawn(&mut commands);
            commands.remove_resource::<MirrorLook>();
        }
        return;
    };
    let (Some(mut ui), Some(cas)) = (ui, cas) else { return };
    if look.is_some() || !available(&ui) {
        return;
    }
    let Ok(sim) = sims.get(p.sim) else { return };
    // The sheet: facial hair for grown men, make-up for those it's made for; the back button
    // where its stylist's buttons were.
    let Some(mut sheet_w) = ui.layout("CASMirrorSheet").cloned() else { return };
    let makeup_ok = !sim.age.is_little() && [CT_EYESHADOW, CT_LIPSTICK].iter().any(|t| !crate::cas::parts_for(&cas, sim, *t, OutfitKind::Everyday).is_empty());
    for (i, ok) in [(1u32, makeup_ok), (2, beard_ok(sim))] {
        if !ok {
            retain_windows(&mut sheet_w, &|w| w.id == SHEET_BUTTONS + i || w.id == SHEET_WORDS + i);
        }
    }
    let modes = ui.template("CASDresserSheet", "Mode Change Buttons");
    edit_windows(&mut sheet_w, PANEL, &mut |w| {
        w.flags |= s3bake::ui::WIN_VISIBLE;
        w.children.retain(|c| c.comment != "Mode Change Buttons");
        if let Some(mut m) = modes.clone() {
            m.flags |= s3bake::ui::WIN_VISIBLE;
            m.children.retain(|c| c.comment != "Design button");
            w.children.push(m);
        }
    });
    let mut panel_left = 0.0;
    edit_windows(&mut sheet_w, PANEL, &mut |w| panel_left = w.area[0]);
    let sheet = ui.spawn_root(&mut commands, &mut images, &mut fonts, &sheet_w);
    // Hair: the physical panel and the hair's.
    let Some((physical, hair, bar)) = crate::cashair::spawn_hair_panels(&mut commands, &mut ui, &mut images, &mut fonts) else { return };
    for id in PHYSICAL_NOT_HERE {
        for e in physical.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    if let Some(e) = physical.id(crate::cashair::PHYSICAL_CLOSE) {
        commands.entity(e).insert((Visibility::Inherited, crate::icons::Tooltip("Done".into())));
    }
    // Make-up: the facial details panel with its mirror's tabs, and the make-up's.
    let Some(mut dw) = ui.layout("CASFacialDetails").cloned() else { return };
    retain_windows(&mut dw, &|w| w.id == FULL_TABS || w.id == SURGEON_TABS);
    edit_windows(&mut dw, MIRROR_TABS, &mut |w| w.flags |= s3bake::ui::WIN_VISIBLE);
    let details = ui.spawn_root(&mut commands, &mut images, &mut fonts, &dw);
    if let Some(e) = details.id(MIRROR_MAKEUP) {
        commands.entity(e).insert(crate::icons::Tooltip("Makeup".into()));
    }
    if let Some(e) = details.id(DETAILS_CLOSE) {
        commands.entity(e).insert(crate::icons::Tooltip("Done".into()));
    }
    let Some(makeup) = crate::casface::spawn_makeup_panel(&mut commands, &mut ui, &mut images, &mut fonts) else { return };
    for (root, z) in [(sheet.root, 30), (physical.root, 30), (details.root, 30), (hair.root, 31), (makeup.root, 31)] {
        if let Some(root) = root {
            commands.entity(root).insert((DespawnOnExit(crate::AppState::InGame), GlobalZIndex(z), crate::hud::BlocksWorld, Visibility::Hidden));
        }
    }
    for (i, v) in VIEWS.iter().enumerate() {
        if let Some(e) = sheet.id(SHEET_BUTTONS + i as u32) {
            commands.entity(e).insert(SheetView(*v));
        }
    }
    if let Some(e) = sheet.id(BACK) {
        commands.entity(e).insert(crate::icons::Tooltip("Done".into()));
    }
    let panel = sheet.id(PANEL).map(|e| (e, panel_left));
    commands.insert_resource(MirrorLook { sheet, physical, hair, details, makeup, panel, view: None, lipstick: false, styles: None, colours: None, cells: None, bar, scroll: 0, shown: None });
}

/// A button on the mirror's sheet.
#[derive(Component, Clone, Copy)]
struct SheetView(View);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn mirror_controls(
    mut commands: Commands,
    look: Option<ResMut<MirrorLook>>,
    planner: Option<Res<OutfitPlanner>>,
    mut sims: Query<&mut Sim>,
    cas: Option<Res<crate::simbody::CasData>>,
    (clicks, views, styles, colours, makeup): (
        Query<(Entity, &Interaction), Changed<Interaction>>,
        Query<(&Interaction, &SheetView), Changed<Interaction>>,
        Query<(&Interaction, &StyleCell), Changed<Interaction>>,
        Query<(&Interaction, &ColourCell), Changed<Interaction>>,
        Query<(&Interaction, &MakeupCell), Changed<Interaction>>,
    ),
    (over, wheel, bars): (Query<&bevy::ui::RelativeCursorPosition>, Res<bevy::input::mouse::AccumulatedMouseScroll>, Query<&UiScrollBar>),
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let (Some(mut l), Some(p), Some(cas)) = (look, planner, cas) else { return };
    let l = &mut *l;
    // The back button: the mirror put away.
    if crate::livehud::pressed(&clicks, l.sheet.id(BACK)) {
        play.write(crate::sound::PlaySound::ui("ui_secondary_button"));
        commands.remove_resource::<OutfitPlanner>();
        return;
    }
    // A view chosen (chosen again, or a panel's close button: back to the sheet).
    if let Some((_, v)) = views.iter().find(|(i, _)| **i == Interaction::Pressed) {
        l.view = if l.view == Some(v.0) { None } else { Some(v.0) };
        l.scroll = 0;
        l.shown = None;
        play.write(crate::sound::PlaySound::ui("ui_primary_button"));
    }
    if crate::livehud::pressed(&clicks, l.physical.id(crate::cashair::PHYSICAL_CLOSE)) || crate::livehud::pressed(&clicks, l.details.id(DETAILS_CLOSE)) {
        l.view = None;
        l.shown = None;
        play.write(crate::sound::PlaySound::ui("ui_secondary_button"));
    }
    // The physical panel's own hair and facial hair buttons; the make-up's kinds.
    for (id, v) in [(crate::cashair::PHYSICAL_HAIR, View::Hair), (crate::cashair::PHYSICAL_BEARD, View::Beard)] {
        if crate::livehud::pressed(&clicks, l.physical.id(id)) && l.view != Some(v) {
            l.view = Some(v);
            l.scroll = 0;
            l.shown = None;
        }
    }
    for (id, lip) in [(crate::casface::MAKEUP_KINDS[0], false), (crate::casface::MAKEUP_KINDS[3], true)] {
        if crate::livehud::pressed(&clicks, l.makeup.id(id)) && l.lipstick != lip {
            l.lipstick = lip;
            l.shown = None;
        }
    }
    let Some(view) = l.view else { return };
    let Ok(mut sim) = sims.get_mut(p.sim) else { return };
    // Clicks: a style or make-up worn, the hair dyed.
    let t = match view {
        View::Hair => CT_HAIR,
        View::Beard => CT_BEARD,
        View::Makeup if l.lipstick => CT_LIPSTICK,
        View::Makeup => CT_EYESHADOW,
    };
    let list = crate::cas::parts_for(&cas, &sim, t, OutfitKind::Everyday);
    let mut changed = false;
    let picked = styles.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, c)| c.0).or_else(|| makeup.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, c)| c.0));
    if let Some(i) = picked {
        let key = if i == usize::MAX { Some(crate::sim::OutfitChoice::NONE) } else { list.get(i).map(|p| p.0) };
        if key.is_some() {
            let o = &mut sim.outfit;
            match t {
                CT_HAIR => o.hair = key,
                CT_BEARD => o.beard = key,
                CT_LIPSTICK => o.lipstick = key,
                _ => o.eyeshadow = key,
            }
            changed = true;
        }
    }
    if let Some((_, c)) = colours.iter().find(|(i, _)| **i == Interaction::Pressed) {
        let (r, g, b) = crate::sim::HAIRS[c.0.min(crate::sim::HAIRS.len() - 1)];
        sim.hair = Color::srgb(r, g, b);
        changed = true;
    }
    if changed {
        commands.entity(p.sim).insert(crate::aging::NeedsNewBody);
        play.write(crate::sound::PlaySound::ui("ui_primary_button"));
    }
    // The styles scrolled: by the wheel over them, or the bar.
    if matches!(view, View::Hair | View::Beard) {
        let rows = (list.len() + (view == View::Beard) as usize).div_ceil(3);
        let most = rows.saturating_sub(crate::cashair::ROWS);
        let over_styles = l.hair.id(crate::cashair::STYLES_GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
        if over_styles && wheel.delta.y != 0.0 {
            l.scroll = (l.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
        } else if let Some(b) = l.bar.and_then(|e| bars.get(e).ok())
            && l.shown.as_ref().is_some_and(|s| s.5 == l.scroll)
            && b.value != l.scroll
        {
            l.scroll = b.value.min(most);
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn mirror_panels(
    mut commands: Commands,
    look: Option<ResMut<MirrorLook>>,
    planner: Option<Res<OutfitPlanner>>,
    sims: Query<&Sim>,
    cas: Option<Res<crate::simbody::CasData>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (mut vis, mut buttons, mut bars, mut texts, mut nodes): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>, Query<&mut Text>, Query<&mut Node>),
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut l), Some(p), Some(cas), Some(mut ui)) = (look, planner, cas, ui) else { return };
    let l = &mut *l;
    let Ok(sim) = sims.get(p.sim) else { return };
    if l.view == Some(View::Beard) && !beard_ok(sim) {
        l.view = None;
    }
    let hair_view = matches!(l.view, Some(View::Hair | View::Beard));
    let makeup_view = l.view == Some(View::Makeup);
    crate::livehud::set_visible(&mut vis, l.sheet.root, true);
    crate::livehud::set_visible(&mut vis, l.sheet.id(NAMES), l.view.is_none());
    crate::livehud::set_visible(&mut vis, l.physical.root, hair_view);
    crate::livehud::set_visible(&mut vis, l.hair.root, hair_view);
    crate::livehud::set_visible(&mut vis, l.details.root, makeup_view);
    crate::livehud::set_visible(&mut vis, l.makeup.root, makeup_view);
    crate::livehud::set_visible(&mut vis, l.physical.id(crate::cashair::PHYSICAL_BEARD), beard_ok(sim));
    // (The names gone, the back panel glides back to the buttons.)
    if let Some((e, x)) = l.panel
        && let Ok(mut n) = nodes.get_mut(e)
    {
        let left = Val::Px(if l.view.is_some() { x + PANEL_GLIDE } else { x });
        if n.left != left {
            n.left = left;
        }
    }
    // The view lit, on the sheet and on its panel.
    let lit = [
        (l.sheet.id(SHEET_BUTTONS), l.view == Some(View::Hair)),
        (l.sheet.id(SHEET_BUTTONS + 1), makeup_view),
        (l.sheet.id(SHEET_BUTTONS + 2), l.view == Some(View::Beard)),
        (l.physical.id(crate::cashair::PHYSICAL_HAIR), l.view == Some(View::Hair)),
        (l.physical.id(crate::cashair::PHYSICAL_BEARD), l.view == Some(View::Beard)),
        (l.hair.id(crate::cashair::HAIR_TAB), true),
        (l.details.id(MIRROR_MAKEUP), true),
        (l.makeup.id(crate::casface::MAKEUP_KINDS[0]), !l.lipstick),
        (l.makeup.id(crate::casface::MAKEUP_KINDS[3]), l.lipstick),
    ];
    for (e, on) in lit {
        if let Some(mut b) = e.and_then(|e| buttons.get_mut(e).ok())
            && b.selected != on
        {
            b.selected = on;
        }
    }
    let Some(view) = l.view else { return };
    let t = match view {
        View::Hair => CT_HAIR,
        View::Beard => CT_BEARD,
        View::Makeup if l.lipstick => CT_LIPSTICK,
        View::Makeup => CT_EYESHADOW,
    };
    let list = crate::cas::parts_for(&cas, sim, t, OutfitKind::Everyday);
    let worn = match t {
        CT_HAIR => worn_hair(&cas, sim),
        CT_BEARD => sim.outfit.beard,
        CT_LIPSTICK => sim.outfit.lipstick,
        _ => sim.outfit.eyeshadow,
    };
    let rows = (list.len() + (view == View::Beard) as usize).div_ceil(3);
    let most = rows.saturating_sub(crate::cashair::ROWS);
    // (Opened: scrolled to the style worn.)
    if l.shown.is_none()
        && hair_view
        && let Some(at) = worn.and_then(|w| list.iter().position(|(k, _)| *k == w))
    {
        l.scroll = (at + (view == View::Beard) as usize) / 3;
    }
    l.scroll = l.scroll.min(most);
    if let Some(mut b) = l.bar.and_then(|e| bars.get_mut(e).ok()) {
        if (b.total, b.visible) != (rows, crate::cashair::ROWS) {
            (b.total, b.visible) = (rows, crate::cashair::ROWS);
        }
        if b.value != l.scroll {
            b.value = l.scroll;
        }
    }
    let hair_rgb = sim.hair.to_srgba().to_u8_array_no_alpha();
    let want = (p.sim, l.view, l.lipstick, worn, hair_rgb, l.scroll, gu.is_some());
    if l.shown.as_ref() == Some(&want) {
        return;
    }
    l.shown = Some(want);
    if hair_view {
        let title = if view == View::Beard { ui.localize("Ui/Caption/CAS/Physical:Beard").unwrap_or_else(|| "Facial Hair".into()) } else { ui.localize("Ui/Caption/CAS/Hair:Title").unwrap_or_else(|| "Hair".into()) };
        crate::livehud::set_text(&mut texts, l.hair.text(crate::cashair::TITLE), &title);
        crate::cashair::spawn_styles(&mut commands, &mut ui, (&mut images, &mut fonts), gu.as_deref_mut(), &l.hair, &mut l.styles, &list, view == View::Beard, worn, l.scroll);
        crate::cashair::spawn_colours(&mut commands, &mut ui, (&mut images, &mut fonts), &l.hair, &mut l.colours, sim.hair);
    } else {
        if let Some(old) = l.cells.take() {
            commands.entity(old).try_despawn();
        }
        l.cells = crate::casface::spawn_makeup_cells(&mut commands, &mut ui, (&mut images, &mut fonts), gu.as_deref_mut(), &l.makeup, &list, worn);
    }
}
