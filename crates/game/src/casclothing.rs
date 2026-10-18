//! Create a Sim's Clothing in the game's own panels, as UI.dll's `CASClothing`,
//! `CASClothingCategory` and `CASClothingRow` show it: the outfits down the clothing panel's
//! side (everyday, formal, sleepwear, athletic, swimwear, outerwear), the kinds of clothes
//! across the top (tops, bottoms, outfits, shoes), and a row for each item, its colourways along
//! it in the game's cells (the game's own picture of each), turned by the row's arrows, three
//! rows in view with the item grid's scroll bar. A click on a colourway wears the item in it
//! (`cas::CasAction`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::AppState;
use crate::cas::{CasAction, CasTab};
use crate::home::PendingHousehold;
use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar, edit_windows};
use crate::simbody::OutfitKind;

pub struct CasClothingPlugin;

impl Plugin for CasClothingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_clothing, clothing_panel).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<CasClothing>();
            CLOTHING_UP.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the game's clothing panels are up (see `caslook::game_panel`).
pub static CLOTHING_UP: AtomicBool = AtomicBool::new(false);

/// For the UI flow (`autotest`): the accessories shown.
pub static TEST_ACCESSORIES: AtomicBool = AtomicBool::new(false);

// `CASClothing`'s windows: the outfits' buttons (in `cas::WEAR`'s order) and its close button.
const OUTFITS: [u32; 6] = [0x05db_c502, 0x05db_c503, 0x05db_c504, 0x05db_c506, 0x05db_c505, 0x05db_c507];
const CAREER: u32 = 0x05db_c509;
pub(crate) const CLOSE: u32 = 0x05db_9c00;
// `CASClothingCategory`'s.
pub(crate) const TITLE: u32 = 0x05db_cf01;
pub(crate) const TOPS: u32 = 0x05db_cf11;
pub(crate) const BOTTOMS: u32 = 0x05db_cf12;
pub(crate) const WHOLE: u32 = 0x05db_cf14;
pub(crate) const SHOES: u32 = 0x05db_cf13;
const KINDS: [(u32, CasTab, &str); 4] = [(TOPS, CasTab::Tops, "Tops"), (BOTTOMS, CasTab::Bottoms, "Bottoms"), (WHOLE, CasTab::Outfits, "Outfits"), (SHOES, CasTab::Shoes, "Shoes")];
pub(crate) const GRID: u32 = 0x05db_cf21;
/// Accessories (glasses and jewellery), after the kinds of clothes.
pub(crate) const ACCESSORIES: u32 = 0x05db_cf90;
/// What this game hasn't: the content filter, saving, deleting and sharing designs.
pub(crate) const NOT_HERE: [u32; 5] = [0x05db_cf19, 0x05db_b904, 0x05db_b905, 0x05db_cf17, 0x05db_cf16];
/// Create a Style for the item worn.
pub(crate) const DESIGN: u32 = 0x05db_cf1a;
// `CASClothingRow`'s: its arrows, and its five cells (the first and last out of view, in the
// game's carousel), each with its thumbnail.
const LEFT_ARROW: u32 = 0x32;
const RIGHT_ARROW: u32 = 0x33;
const CELLS: [u32; 5] = [0x50, 0x51, 0x52, 0x53, 0x54];
const CELL_THUMBNAIL: u32 = 0x21;
// The item grid's own (layout `ItemGrid`): its scroll bar.
pub(crate) const GRID_SCROLLBAR: u32 = 0x0582_0672;
/// The rows in view, and the colourways in view along a row.
pub(crate) const ROWS: usize = 3;
pub(crate) const ACROSS: usize = 3;

/// The panels on screen, the rows shown, and what they were shown for.
#[derive(Resource)]
pub struct CasClothing {
    outfits: Spawned,
    category: Spawned,
    rows: Option<Entity>,
    bar: Option<Entity>,
    scroll: usize,
    /// How far each item's row is turned (by its place in the list).
    turned: HashMap<usize, usize>,
    /// Accessories shown rather than the tab's clothes.
    accessories: bool,
    shown: Option<Shown>,
}

#[derive(PartialEq, Clone)]
struct Shown {
    sim: usize,
    female: bool,
    age: crate::sim::Age,
    tab: CasTab,
    wear: OutfitKind,
    worn: Vec<(s3bake::Key, u8)>,
    scroll: usize,
    turned: Vec<(usize, usize)>,
    icons: bool,
    accessories: bool,
}

/// A colourway's cell: the item's place in the list, and the colourway.
#[derive(Component)]
pub(crate) struct DesignCell(pub usize, pub u8);

/// A row's arrow: the item's place in the list, and which way.
#[derive(Component)]
pub(crate) struct RowArrow(pub usize, pub i8);

fn spawn_clothing(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<crate::caslook::CasFrame>>, clothing: Option<Res<CasClothing>>) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if clothing.is_some() || ui.layout("CASClothing").is_none() || ui.layout("CASClothingCategory").is_none() || ui.layout("CASClothingRow").is_none() {
        return;
    }
    let Some(outfits) = ui.spawn(&mut commands, &mut images, &mut fonts, "CASClothing") else { return };
    // The kinds of clothes along the top, centred as `CASClothingCategory` lays out those it
    // has (42 across, 3 overlapping).
    let Some(mut cat) = ui.layout("CASClothingCategory").cloned() else { return };
    let ids: Vec<u32> = KINDS.iter().map(|k| k.0).chain([ACCESSORIES]).collect();
    let n = ids.len() as f32;
    let start = 159.0 - n * 0.5 * 42.0 - (n - 1.0) * 0.5 * -3.0;
    for (i, id) in ids.iter().enumerate() {
        edit_windows(&mut cat, *id, &mut |w| {
            let x = start + i as f32 * 39.0;
            w.area = [x, w.area[1], x + 42.0, w.area[3]];
            w.flags |= s3bake::ui::WIN_VISIBLE;
        });
    }
    let category = ui.spawn_root(&mut commands, &mut images, &mut fonts, &cat);
    for root in [outfits.root, category.root].into_iter().flatten() {
        commands.entity(root).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(5), Visibility::Hidden));
    }
    if let Some(r) = category.root {
        commands.entity(r).insert(GlobalZIndex(6));
    }
    for (spawned, ids) in [(&outfits, &[CLOSE, CAREER][..]), (&category, &NOT_HERE[..])] {
        for id in ids {
            for e in spawned.all_with(*id) {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
    }
    // (Outerwear as the game shows it when it's had: here, always.)
    if let Some(e) = outfits.id(OUTFITS[5]) {
        commands.entity(e).insert(Visibility::Inherited);
    }
    for (n, id) in OUTFITS.iter().enumerate() {
        if let Some(e) = outfits.id(*id) {
            commands.entity(e).insert((CasAction::Wear(n), crate::icons::Tooltip(crate::cas::wear_kind(n).label().into())));
        }
    }
    for (id, tab, tip) in KINDS {
        if let Some(e) = category.id(id) {
            commands.entity(e).insert((CasAction::Tab(tab), crate::icons::Tooltip(tip.into())));
        }
    }
    if let Some(e) = category.id(ACCESSORIES) {
        commands.entity(e).insert(crate::icons::Tooltip("Accessories".into()));
    }
    if let Some(e) = category.id(DESIGN) {
        commands.entity(e).insert((CasAction::Styling, crate::icons::Tooltip("Create a Style".into())));
    }
    // The rows' scroll bar: the item grid's own, down the right of the rows.
    let mut bar = None;
    if let (Some(grid), Some(mut w)) = (category.id(GRID), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
        w.flags |= s3bake::ui::WIN_VISIBLE;
        w.place = s3bake::ui::UiPlace::Fixed;
        w.area = [318.0, 34.0, 338.0, 400.0];
        bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, grid).root;
        commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default());
    }
    CLOTHING_UP.store(true, Ordering::Relaxed);
    commands.insert_resource(CasClothing { outfits, category, rows: None, bar, scroll: 0, turned: HashMap::new(), accessories: false, shown: None });
}

/// Shown on the clothes' tabs: the outfit and kind lit, the items for the Sim in that outfit,
/// each a row of its colourways (the one worn lit), scrolled by the wheel or the bar.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn clothing_panel(
    mut commands: Commands,
    clothing: Option<ResMut<CasClothing>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    scene: Option<Res<crate::cas::CasScene>>,
    (mut vis, mut buttons, mut bars, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>, Query<&mut Text>),
    (cells, arrows, over, clicks): (Query<(&Interaction, &DesignCell), Changed<Interaction>>, Query<(&Interaction, &RowArrow), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>, Query<(Entity, &Interaction), Changed<Interaction>>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut c), Some(mut ui), Some(scene)) = (clothing, ui, scene) else { return };
    let c = &mut *c;
    let tab = selected.1;
    let on = matches!(tab, CasTab::Tops | CasTab::Bottoms | CasTab::Outfits | CasTab::Shoes) && !crate::cas::STYLING.load(Ordering::Relaxed) && !crate::caslook::modal_up();
    crate::livehud::set_visible(&mut vis, c.outfits.root, on);
    crate::livehud::set_visible(&mut vis, c.category.root, on);
    if !on {
        return;
    }
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k) else { return };
    let Some(tab_type) = tab.clothing_type() else { return };
    // Accessories: their button; the kinds' put the tab's clothes back.
    if crate::livehud::pressed(&clicks, c.category.id(ACCESSORIES)) || TEST_ACCESSORIES.swap(false, Ordering::Relaxed) {
        c.accessories = true;
    }
    if KINDS.iter().any(|(id, _, _)| crate::livehud::pressed(&clicks, c.category.id(*id))) {
        c.accessories = false;
    }
    let accessories = c.accessories && !sim.age.is_little();
    let t = tab_type;
    let wear = if accessories { OutfitKind::Everyday } else { scene.wear };
    // The outfit and the kind lit.
    for (n, id) in OUTFITS.iter().enumerate() {
        let lit = crate::cas::wear_kind(n) == wear;
        if let Some(mut b) = c.outfits.id(*id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != lit
        {
            b.selected = lit;
        }
    }
    for (id, lit) in KINDS.iter().map(|(id, kind, _)| (*id, *kind == tab && !accessories)).chain([(ACCESSORIES, accessories)]) {
        if let Some(mut b) = c.category.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != lit
        {
            b.selected = lit;
        }
    }
    // Accessories: the glasses, then the jewellery, kind by kind (as the game lists its
    // accessories' body types), each row with its kind and its place in that kind's list.
    let kinds: Vec<(u32, usize)> = if accessories {
        [s3formats::sim::CT_GLASSES]
            .into_iter()
            .chain(crate::simbody::ACCESSORY_TYPES)
            .flat_map(|k| (0..crate::cas::parts_for(&scene.cas, sim, k, wear).len()).map(move |i| (k, i)))
            .collect()
    } else {
        Vec::new()
    };
    let list: Vec<(s3bake::Key, String)> = if accessories {
        [s3formats::sim::CT_GLASSES].into_iter().chain(crate::simbody::ACCESSORY_TYPES).flat_map(|k| crate::cas::parts_for(&scene.cas, sim, k, wear)).collect()
    } else {
        crate::cas::parts_for(&scene.cas, sim, t, wear)
    };
    // The accessories worn (glasses none chosen: none).
    let worn_accessories: Vec<s3bake::Key> = sim.outfit.glasses.filter(|g| *g != crate::sim::OutfitChoice::NONE).into_iter().chain(crate::simbody::accessories_of(&scene.cas, sim).into_iter().map(|p| p.key)).collect();
    // Clicks: a colourway worn (an accessory put on, or taken off if it's on); a row turned.
    for (i, d) in &cells {
        if *i == Interaction::Pressed {
            let action = if accessories {
                let Some(&(kind, at)) = kinds.get(d.0) else { continue };
                let off = kind == s3formats::sim::CT_GLASSES && list.get(d.0).is_some_and(|(key, _)| worn_accessories.contains(key));
                CasAction::FacePart(kind, if off { usize::MAX } else { at })
            } else {
                CasAction::WearDesign(t, d.0, d.1)
            };
            actions.write(crate::cas::CasActionRequest(action));
        }
    }
    let designs = |key: &s3bake::Key| scene.cas.colourways.get(key).map_or(1, |w| w.swatches.len().clamp(1, 8));
    for (i, a) in &arrows {
        if *i == Interaction::Pressed
            && let Some((key, _)) = list.get(a.0)
        {
            let most = designs(key).saturating_sub(ACROSS);
            let now = c.turned.get(&a.0).copied().unwrap_or(0);
            c.turned.insert(a.0, (now as i64 + a.1 as i64).clamp(0, most as i64) as usize);
        }
    }
    let rows = list.len();
    let most = rows.saturating_sub(ROWS);
    // Scrolled: by the wheel over the rows, or the bar.
    let over_rows = c.category.id(GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    let wheeled = over_rows && wheel.delta.y != 0.0;
    if wheeled {
        c.scroll = (c.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
    }
    if let Some(mut b) = c.bar.and_then(|e| bars.get_mut(e).ok()) {
        if (b.total, b.visible) != (rows, ROWS) {
            (b.total, b.visible) = (rows, ROWS);
        }
        if b.value != c.scroll {
            if wheeled {
                b.value = c.scroll;
            } else {
                c.scroll = b.value.min(most);
            }
        }
    }
    let worn: Vec<(s3bake::Key, u8)> = if accessories {
        worn_accessories.iter().map(|k| (*k, 0)).collect()
    } else {
        crate::cas::worn(&scene, sim, t).map(|key| (key, sim.outfit.designs.iter().find(|(p, _)| *p == key).map_or(0, |d| d.1))).into_iter().collect()
    };
    // (Opened, for another Sim, outfit or kind: the item worn scrolled to, its row turned to
    // its colourway.)
    let fresh = c.shown.as_ref().is_none_or(|s| (s.sim, s.tab, s.wear, s.accessories) != (k, tab, wear, accessories));
    if fresh {
        c.turned.clear();
        if let Some(&(key, d)) = worn.first()
            && let Some(at) = list.iter().position(|(p, _)| *p == key)
        {
            if !(c.scroll..c.scroll + ROWS).contains(&at) {
                c.scroll = at.min(most);
            }
            c.turned.insert(at, (d as usize).saturating_sub(ACROSS - 1).min(designs(&key).saturating_sub(ACROSS)));
        } else {
            c.scroll = 0;
        }
        if let Some(mut b) = c.bar.and_then(|e| bars.get_mut(e).ok()) {
            b.value = c.scroll;
        }
    }
    c.scroll = c.scroll.min(most);
    let mut turned: Vec<(usize, usize)> = c.turned.iter().map(|(a, b)| (*a, *b)).collect();
    turned.sort();
    let want = Shown { sim: k, female: sim.female, age: sim.age, tab, wear, worn: worn.clone(), scroll: c.scroll, turned, icons: gu.is_some(), accessories };
    if c.shown.as_ref() == Some(&want) {
        return;
    }
    c.shown = Some(want);
    // The title: the outfit's name, as the game has it.
    let state = match wear {
        OutfitKind::Everyday | OutfitKind::Career | OutfitKind::Naked => "Everyday",
        OutfitKind::Formal => "Formal",
        OutfitKind::Sleepwear => "Sleepwear",
        OutfitKind::Swimwear => "Swimwear",
        OutfitKind::Athletic => "Exercise",
        OutfitKind::Outerwear => "Outerwear",
    };
    let title = ui.localize(&format!("Ui/Caption/CAS/Clothing:{state}")).unwrap_or_else(|| wear.label().to_string());
    crate::livehud::set_text(&mut texts, c.category.text(TITLE), &title);
    // The rows in view.
    if let Some(old) = c.rows.take() {
        commands.entity(old).try_despawn();
    }
    c.rows = spawn_rows(&mut commands, &mut ui, (&mut images, &mut fonts), gu.as_deref_mut(), &c.category, &list, &designs, &worn, c.scroll, &c.turned);
}

/// The rows in view on `CASClothingCategory`'s grid, from `scroll`: each item a
/// `CASClothingRow` of its colourways (turned as `turned` has it, those worn lit), the game's
/// own picture in each cell; the holder they're in.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_rows(
    commands: &mut Commands,
    ui: &mut UiAssets,
    (images, fonts): (&mut Assets<Image>, &mut Assets<Font>),
    mut gu: Option<&mut crate::icons::GameUi>,
    category: &Spawned,
    list: &[(s3bake::Key, String)],
    designs: &dyn Fn(&s3bake::Key) -> usize,
    worn: &[(s3bake::Key, u8)],
    scroll: usize,
    turned: &HashMap<usize, usize>,
) -> Option<Entity> {
    let rows = list.len();
    let (Some(grid), Some(g), Some(template)) = (category.id(GRID), ui.find("CASClothingCategory", GRID).and_then(|w| w.grid), ui.layout("CASClothingRow").cloned()) else { return None };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    commands.entity(grid).insert_children(0, &[holder]);
    let step = g.cell[1] + g.cell_padding[1] + g.cell_padding[3];
    for (row, n) in (scroll..(scroll + ROWS).min(rows)).enumerate() {
        let (key, name) = &list[n];
        let count = designs(key);
        let first = turned.get(&n).copied().unwrap_or(0).min(count.saturating_sub(ACROSS));
        let mut w = template.clone();
        let (x, y) = (g.padding[0] + g.cell_padding[0], g.padding[1] + row as f32 * step + g.cell_padding[1]);
        w.area = [x, y, x + (w.area[2] - w.area[0]), y + (w.area[3] - w.area[1])];
        w.flags |= s3bake::ui::WIN_VISIBLE;
        // (The cells and arrows take clicks; the first and last cells, out of view in the
        // game's carousel, stay hidden.)
        for id in CELLS.into_iter().chain([LEFT_ARROW, RIGHT_ARROW]) {
            edit_windows(&mut w, id, &mut |w| {
                w.cls = "Button".into();
                w.flags &= !s3bake::ui::WIN_IGNORE_MOUSE;
            });
        }
        for id in [CELLS[0], CELLS[4]] {
            edit_windows(&mut w, id, &mut |w| w.flags &= !s3bake::ui::WIN_VISIBLE);
        }
        let s = ui.spawn_under(commands, images, fonts, &w, holder);
        for (dir, id, shown) in [(-1i8, LEFT_ARROW, first > 0), (1, RIGHT_ARROW, first + ACROSS < count)] {
            if let Some(e) = s.id(id) {
                commands.entity(e).insert((RowArrow(n, dir), if shown { Visibility::Inherited } else { Visibility::Hidden }));
            }
        }
        for (slot, cell_id) in CELLS[1..4].iter().enumerate() {
            let Some(cell) = s.id(*cell_id) else { continue };
            let d = first + slot;
            if d >= count {
                commands.entity(cell).insert(Visibility::Hidden);
                continue;
            }
            commands.entity(cell).insert((DesignCell(n, d as u8), crate::icons::Tooltip(name.clone())));
            if worn.contains(&(*key, d as u8)) {
                commands.entity(cell).queue(|mut e: EntityWorldMut| {
                    if let Some(mut b) = e.get_mut::<UiButton>() {
                        b.selected = true;
                    }
                });
            }
            // The game's picture of the item in this colourway (or of the item).
            let thumb = ui
                .image(images, s3bake::ui::cas_preset_thumb(key.2, d))
                .map(|(h, _)| h)
                .or_else(|| gu.as_deref_mut().and_then(|g| g.icon(images, &s3bake::gamedata::cas_thumb_name(key.2))));
            if let (Some(win), Some(thumb)) = (s.within(cell, CELL_THUMBNAIL), thumb) {
                crate::hudpanels::picture(commands, win, thumb, Color::WHITE);
            }
        }
    }
    Some(holder)
}
