//! Plan Outfit in the game's own look, as UI.dll's dresser mode shows it: `CASDresserSheet`, the
//! outfits down a sheet with their names (everyday, formal, sleepwear, athletic, swimwear, the
//! career's for a Sim with a job, outerwear), and, an outfit chosen, the names faded and the sheet
//! slid back to its buttons, `CASDresserClothing` beside it (its outfit lit; its close button back
//! to the sheet) and Create a Sim's own `CASClothingCategory`: the kinds of clothes along its top
//! and a row for each item, its colourways along it (`casclothing`'s rows). A colourway clicked is
//! worn, the Sim dressing in the outfit as it's planned. The sheet's back button (or Escape) puts
//! the wardrobe away. (A mirror's Change Appearance keeps `planner`'s panel.)

use std::collections::HashMap;

use bevy::prelude::*;
use s3bake::ui::UiWindow;
use s3formats::sim::{CT_BODY, CT_BOTTOM, CT_SHOES, CT_TOP};

use crate::casclothing::{DesignCell, RowArrow};
use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar, edit_windows};
use crate::planner::OutfitPlanner;
use crate::simbody::OutfitKind;

pub struct PlanLookPlugin;

impl Plugin for PlanLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (open_look, look_controls, look_rows).chain().run_if(in_state(crate::PlayMode::Live)))
            .add_systems(OnExit(crate::PlayMode::Live), |mut commands: Commands, look: Option<Res<PlanLook>>| {
                if let Some(l) = look {
                    l.despawn(&mut commands);
                }
                commands.remove_resource::<PlanLook>();
            });
    }
}

/// Whether the game's dresser panels are to hand (else `planner`'s plain panel).
pub fn available(ui: &UiAssets) -> bool {
    ["CASDresserSheet", "CASDresserClothing", "CASClothingCategory", "CASClothingRow"].iter().all(|n| ui.layout(n).is_some())
}

// `CASDresserSheet`'s: its outfits' buttons and their names (in `SHEET`'s order), the names'
// window, its back panels (small, or long for all seven) and their back button.
const SHEET_BUTTONS: u32 = 0x0077_7000;
const SHEET_WORDS: u32 = 0x0077_7010;
const SHEET: [OutfitKind; 7] = [OutfitKind::Everyday, OutfitKind::Formal, OutfitKind::Sleepwear, OutfitKind::Athletic, OutfitKind::Swimwear, OutfitKind::Career, OutfitKind::Outerwear];
const NAMES: u32 = 0x0077_7020;
const SMALL_PANEL: u32 = 0x0077_7021;
const LONG_PANEL: u32 = 0x0077_7025;
const BIG_PANELS: [u32; 2] = [0x0077_7022, 0x0077_7026];
const MODE_BUTTONS: u32 = 1;
const BACK: u32 = 0x0077_7023;
/// How far the small panels glide back with the names gone (their `Glide` effect).
const PANEL_GLIDE: f32 = -79.0;
// `CASDresserClothing`'s: its three outfits of a kind (the first, the only one here), their
// remove buttons, adding one, its words for the career's, and its close button.
const SLOTS: [u32; 3] = [0x05db_c502, 0x05db_c503, 0x05db_c504];
const REMOVES: [u32; 3] = [0x05db_c512, 0x05db_c513, 0x05db_c514];
const ADD: u32 = 0x05da_4d08;
const CAREER_WORDS: u32 = 0x0cd2_4d80;
const KINDS: [(u32, u32, &str); 4] = [
    (crate::casclothing::TOPS, CT_TOP, "Tops"),
    (crate::casclothing::BOTTOMS, CT_BOTTOM, "Bottoms"),
    (crate::casclothing::WHOLE, CT_BODY, "Outfits"),
    (crate::casclothing::SHOES, CT_SHOES, "Shoes"),
];

/// The dresser's panels on screen, which is showing, and the clothes' rows.
#[derive(Resource)]
pub struct PlanLook {
    sheet: Spawned,
    clothing: Spawned,
    category: Spawned,
    /// The sheet's back panel, and where it sits.
    panel: Option<(Entity, f32)>,
    /// Showing an outfit's clothes (else the sheet's names).
    open: bool,
    tab: u32,
    rows: Option<Entity>,
    bar: Option<Entity>,
    scroll: usize,
    turned: HashMap<usize, usize>,
    shown: Option<Shown>,
}

type Shown = (Entity, OutfitKind, u32, Option<(s3bake::Key, u8)>, usize, Vec<(usize, usize)>, bool);

impl PlanLook {
    fn despawn(&self, commands: &mut Commands) {
        for r in [self.sheet.root, self.clothing.root, self.category.root].into_iter().flatten() {
            commands.entity(r).despawn();
        }
    }
}

/// The outfit's piece of a kind, and its colourway.
fn worn(sim: &crate::sim::Sim, wear: OutfitKind, t: u32) -> Option<(s3bake::Key, u8)> {
    let c = sim.outfit.clothes(wear);
    let key = match t {
        CT_TOP => c.top,
        CT_BOTTOM => c.bottom,
        CT_BODY => c.full,
        _ => c.shoes,
    }?;
    Some((key, sim.outfit.designs.iter().find(|(p, _)| *p == key).map_or(0, |d| d.1)))
}

/// Of the windows under `w`, takes those `drop` says out.
fn retain_windows(w: &mut UiWindow, drop: &impl Fn(&UiWindow) -> bool) {
    w.children.retain(|c| !drop(c));
    for c in &mut w.children {
        retain_windows(c, drop);
    }
}

#[allow(clippy::too_many_arguments)]
fn open_look(
    mut commands: Commands,
    planner: Option<Res<OutfitPlanner>>,
    look: Option<Res<PlanLook>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    sims: Query<(&crate::sim::Sim, Has<crate::careers::Job>)>,
) {
    // (Put away with the wardrobe.)
    let Some(p) = planner.filter(|p| !p.looks) else {
        if let Some(l) = look {
            l.despawn(&mut commands);
            commands.remove_resource::<PlanLook>();
        }
        return;
    };
    let Some(mut ui) = ui else { return };
    if look.is_some() || !available(&ui) {
        return;
    }
    let Ok((sim, job)) = sims.get(p.sim) else { return };
    // The sheet's outfits: the career's for a grown Sim with a job (else outerwear in its
    // place), as `CASDresserSheet.Init` has them; its back panel the long one for all seven.
    let Some(mut sheet_w) = ui.layout("CASDresserSheet").cloned() else { return };
    let career = job && !matches!(sim.age, crate::sim::Age::Teen) && !sim.age.is_little();
    if !career {
        let (mut area, mut words) = ([0.0; 4], [0.0; 4]);
        edit_windows(&mut sheet_w, SHEET_BUTTONS + 5, &mut |w| area = w.area);
        edit_windows(&mut sheet_w, SHEET_WORDS + 5, &mut |w| words = w.area);
        edit_windows(&mut sheet_w, SHEET_BUTTONS + 6, &mut |w| w.area = area);
        edit_windows(&mut sheet_w, SHEET_WORDS + 6, &mut |w| w.area = words);
        retain_windows(&mut sheet_w, &|w| w.id == SHEET_BUTTONS + 5 || w.id == SHEET_WORDS + 5);
    }
    let panel_id = if career { LONG_PANEL } else { SMALL_PANEL };
    retain_windows(&mut sheet_w, &|w| ([SMALL_PANEL, LONG_PANEL].contains(&w.id) && w.id != panel_id) || BIG_PANELS.contains(&w.id));
    edit_windows(&mut sheet_w, panel_id, &mut |w| w.flags |= s3bake::ui::WIN_VISIBLE);
    // (Its stylist's buttons: the back button only, to put the wardrobe away.)
    edit_windows(&mut sheet_w, MODE_BUTTONS, &mut |w| w.flags |= s3bake::ui::WIN_VISIBLE);
    retain_windows(&mut sheet_w, &|w| w.comment == "Design button");
    let mut panel_left = 0.0;
    edit_windows(&mut sheet_w, panel_id, &mut |w| panel_left = w.area[0]);
    let sheet = ui.spawn_root(&mut commands, &mut images, &mut fonts, &sheet_w);
    let Some(clothing) = ui.spawn(&mut commands, &mut images, &mut fonts, "CASDresserClothing") else { return };
    // The clothes' panel: the kinds centred along its top, as `casclothing` lays them out.
    let Some(mut cat) = ui.layout("CASClothingCategory").cloned() else { return };
    let n = KINDS.len() as f32;
    let start = 159.0 - n * 0.5 * 42.0 - (n - 1.0) * 0.5 * -3.0;
    for (i, (id, _, _)) in KINDS.iter().enumerate() {
        edit_windows(&mut cat, *id, &mut |w| {
            let x = start + i as f32 * 39.0;
            w.area = [x, w.area[1], x + 42.0, w.area[3]];
            w.flags |= s3bake::ui::WIN_VISIBLE;
        });
    }
    let category = ui.spawn_root(&mut commands, &mut images, &mut fonts, &cat);
    for (root, z) in [(sheet.root, 30), (clothing.root, 30), (category.root, 31)] {
        if let Some(root) = root {
            commands.entity(root).insert((DespawnOnExit(crate::AppState::InGame), GlobalZIndex(z), crate::hud::BlocksWorld, Visibility::Hidden));
        }
    }
    // The outfits' buttons.
    for (i, kind) in SHEET.iter().enumerate() {
        if let Some(e) = sheet.id(SHEET_BUTTONS + i as u32) {
            commands.entity(e).insert(SheetOutfit(*kind));
        }
    }
    if let Some(e) = sheet.id(BACK) {
        commands.entity(e).insert(crate::icons::Tooltip("Done".into()));
    }
    // (Of the dresser's own: the first outfit of a kind only, nothing to add or remove.)
    for id in SLOTS[1..].iter().chain(REMOVES.iter()).chain([ADD, CAREER_WORDS].iter()) {
        for e in clothing.all_with(*id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for id in crate::casclothing::NOT_HERE.iter().chain([crate::casclothing::ACCESSORIES, crate::casclothing::DESIGN].iter()) {
        for e in category.all_with(*id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for (id, t, tip) in KINDS {
        if let Some(e) = category.id(id) {
            commands.entity(e).insert((ClothesKind(t), crate::icons::Tooltip(tip.into())));
        }
    }
    // The rows' scroll bar: the item grid's own, down the right of the rows.
    let mut bar = None;
    if let (Some(grid), Some(mut w)) = (category.id(crate::casclothing::GRID), ui.find("ItemGrid", crate::casclothing::GRID_SCROLLBAR).cloned()) {
        w.flags |= s3bake::ui::WIN_VISIBLE;
        w.place = s3bake::ui::UiPlace::Fixed;
        w.area = [318.0, 34.0, 338.0, 400.0];
        bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, grid).root;
        commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default());
    }
    let panel = sheet.id(panel_id).map(|e| (e, panel_left));
    commands.insert_resource(PlanLook { sheet, clothing, category, panel, open: false, tab: CT_TOP, rows: None, bar, scroll: 0, turned: HashMap::new(), shown: None });
}

/// An outfit's button on the sheet.
#[derive(Component, Clone, Copy)]
struct SheetOutfit(OutfitKind);

/// A kind of clothes' button.
#[derive(Component, Clone, Copy)]
struct ClothesKind(u32);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn look_controls(
    mut commands: Commands,
    look: Option<ResMut<PlanLook>>,
    planner: Option<ResMut<OutfitPlanner>>,
    mut sims: Query<&mut crate::sim::Sim>,
    cas: Option<Res<crate::simbody::CasData>>,
    (clicks, outfits, kinds, cells, arrows): (
        Query<(Entity, &Interaction), Changed<Interaction>>,
        Query<(&Interaction, &SheetOutfit), Changed<Interaction>>,
        Query<(&Interaction, &ClothesKind), Changed<Interaction>>,
        Query<(&Interaction, &DesignCell), Changed<Interaction>>,
        Query<(&Interaction, &RowArrow), Changed<Interaction>>,
    ),
    (over, wheel, bars): (Query<&bevy::ui::RelativeCursorPosition>, Res<bevy::input::mouse::AccumulatedMouseScroll>, Query<&UiScrollBar>),
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let (Some(mut l), Some(mut p), Some(cas)) = (look, planner, cas) else { return };
    let l = &mut *l;
    // The sheet's back button: the wardrobe put away.
    if crate::livehud::pressed(&clicks, l.sheet.id(BACK)) {
        play.write(crate::sound::PlaySound::ui("ui_secondary_button"));
        commands.remove_resource::<OutfitPlanner>();
        return;
    }
    // An outfit chosen: its clothes, the Sim dressing in it (chosen again: back to the sheet).
    if let Some((_, o)) = outfits.iter().find(|(i, _)| **i == Interaction::Pressed) {
        if l.open && p.wear == o.0 {
            l.open = false;
        } else {
            p.wear = o.0;
            crate::planner::dress(&mut commands, p.sim, o.0);
            l.open = true;
        }
        l.shown = None;
        play.write(crate::sound::PlaySound::ui("ui_primary_button"));
    }
    // The dresser's close button: back to the sheet.
    if crate::livehud::pressed(&clicks, l.clothing.id(crate::casclothing::CLOSE)) {
        l.open = false;
        l.shown = None;
        play.write(crate::sound::PlaySound::ui("ui_secondary_button"));
    }
    if let Some((_, k)) = kinds.iter().find(|(i, _)| **i == Interaction::Pressed) {
        l.tab = k.0;
        l.shown = None;
    }
    if !l.open {
        return;
    }
    let Ok(mut sim) = sims.get_mut(p.sim) else { return };
    let list = crate::cas::parts_for(&cas, &sim, l.tab, p.wear);
    let designs = |key: &s3bake::Key| cas.colourways.get(key).map_or(1, |w| w.swatches.len().clamp(1, 8));
    // A colourway worn: into the outfit as planned.
    for (i, d) in &cells {
        if *i != Interaction::Pressed {
            continue;
        }
        let Some((key, _)) = list.get(d.0) else { continue };
        let o = &mut sim.outfit;
        o.wear(p.wear, l.tab, *key);
        o.designs.retain(|(k, _)| k != key);
        o.styles.retain(|s| s.part != *key);
        if d.1 > 0 {
            o.designs.push((*key, d.1));
        }
        crate::planner::dress(&mut commands, p.sim, p.wear);
        play.write(crate::sound::PlaySound::ui("ui_primary_button"));
    }
    for (i, a) in &arrows {
        if *i == Interaction::Pressed
            && let Some((key, _)) = list.get(a.0)
        {
            let most = designs(key).saturating_sub(crate::casclothing::ACROSS);
            let now = l.turned.get(&a.0).copied().unwrap_or(0);
            l.turned.insert(a.0, (now as i64 + a.1 as i64).clamp(0, most as i64) as usize);
        }
    }
    // Scrolled: by the wheel over the rows, or the bar.
    let most = list.len().saturating_sub(crate::casclothing::ROWS);
    let over_rows = l.category.id(crate::casclothing::GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    if over_rows && wheel.delta.y != 0.0 {
        l.scroll = (l.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
    } else if let Some(b) = l.bar.and_then(|e| bars.get(e).ok())
        && l.shown.as_ref().is_some_and(|s| s.4 == l.scroll)
        && b.value != l.scroll
    {
        l.scroll = b.value.min(most);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn look_rows(
    mut commands: Commands,
    look: Option<ResMut<PlanLook>>,
    planner: Option<Res<OutfitPlanner>>,
    sims: Query<&crate::sim::Sim>,
    cas: Option<Res<crate::simbody::CasData>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (mut vis, mut buttons, mut bars, mut texts, mut nodes): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>, Query<&mut Text>, Query<&mut Node>),
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut l), Some(p), Some(cas), Some(mut ui)) = (look, planner, cas, ui) else { return };
    let l = &mut *l;
    crate::livehud::set_visible(&mut vis, l.sheet.root, true);
    crate::livehud::set_visible(&mut vis, l.sheet.id(NAMES), !l.open);
    crate::livehud::set_visible(&mut vis, l.clothing.root, l.open);
    crate::livehud::set_visible(&mut vis, l.category.root, l.open);
    // (The names gone, the back panel glides back to the buttons.)
    if let Some((e, x)) = l.panel
        && let Ok(mut n) = nodes.get_mut(e)
    {
        let left = Val::Px(if l.open { x + PANEL_GLIDE } else { x });
        if n.left != left {
            n.left = left;
        }
    }
    let Ok(sim) = sims.get(p.sim) else { return };
    // The outfit being planned lit on the sheet (none, on the sheet's names); on the dresser,
    // its first; the kind of clothes shown.
    for (i, kind) in SHEET.iter().enumerate() {
        if let Some(mut b) = l.sheet.id(SHEET_BUTTONS + i as u32).and_then(|e| buttons.get_mut(e).ok()) {
            let lit = l.open && *kind == p.wear;
            if b.selected != lit {
                b.selected = lit;
            }
        }
    }
    if let Some(mut b) = l.clothing.id(SLOTS[0]).and_then(|e| buttons.get_mut(e).ok())
        && !b.selected
    {
        b.selected = true;
    }
    for (id, t, _) in KINDS {
        if let Some(mut b) = l.category.id(id).and_then(|e| buttons.get_mut(e).ok()) {
            let lit = t == l.tab;
            if b.selected != lit {
                b.selected = lit;
            }
        }
    }
    if !l.open {
        return;
    }
    let list = crate::cas::parts_for(&cas, sim, l.tab, p.wear);
    let designs = |key: &s3bake::Key| cas.colourways.get(key).map_or(1, |w| w.swatches.len().clamp(1, 8));
    let worn = worn(sim, p.wear, l.tab);
    let most = list.len().saturating_sub(crate::casclothing::ROWS);
    // (Opened on an outfit or kind: the piece worn scrolled to, its row turned to it.)
    if l.shown.is_none() {
        l.turned.clear();
        l.scroll = 0;
        if let Some((key, d)) = worn
            && let Some(at) = list.iter().position(|(k, _)| *k == key)
        {
            l.scroll = at.min(most);
            l.turned.insert(at, (d as usize).saturating_sub(crate::casclothing::ACROSS - 1).min(designs(&key).saturating_sub(crate::casclothing::ACROSS)));
        }
    }
    l.scroll = l.scroll.min(most);
    if let Some(mut b) = l.bar.and_then(|e| bars.get_mut(e).ok()) {
        if (b.total, b.visible) != (list.len(), crate::casclothing::ROWS) {
            (b.total, b.visible) = (list.len(), crate::casclothing::ROWS);
        }
        if b.value != l.scroll {
            b.value = l.scroll;
        }
    }
    let mut turned: Vec<(usize, usize)> = l.turned.iter().map(|(a, b)| (*a, *b)).collect();
    turned.sort();
    let want = (p.sim, p.wear, l.tab, worn, l.scroll, turned, gu.is_some());
    if l.shown.as_ref() == Some(&want) {
        return;
    }
    l.shown = Some(want);
    // The panel's title: the outfit's name, as the game has it.
    let title = match p.wear {
        OutfitKind::Everyday => "Everyday",
        OutfitKind::Career => "Career",
        OutfitKind::Formal => "Formal",
        OutfitKind::Sleepwear => "Sleepwear",
        OutfitKind::Swimwear => "Swimwear",
        OutfitKind::Athletic => "Exercise",
        OutfitKind::Outerwear => "Outerwear",
    };
    let title = ui.localize(&format!("Ui/Caption/CAS/Clothing:{title}")).unwrap_or_else(|| p.wear.label().to_string());
    crate::livehud::set_text(&mut texts, l.category.text(crate::casclothing::TITLE), &title);
    if let Some(old) = l.rows.take() {
        commands.entity(old).despawn();
    }
    l.rows = crate::casclothing::spawn_rows(&mut commands, &mut ui, (&mut images, &mut fonts), gu.as_deref_mut(), &l.category, &list, &designs, worn, l.scroll, &l.turned);
}
