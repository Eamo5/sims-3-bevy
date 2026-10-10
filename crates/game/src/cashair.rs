//! Create a Sim's Hair in the game's own panels, as UI.dll's `CASPhysical` and `CASHair` show
//! it: the physical panel's back and its Hair and Facial Hair buttons down its side, the
//! styles as the game's item cells (`GenericCasItem`) three across with the item grid's own
//! scroll bar, and the hair colours as its preset swatches (`HairColorPresetGridItem`). A
//! click on a style wears it, on a swatch dyes the hair (`cas::CasAction`).

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use s3formats::sim::{CT_BEARD, CT_HAIR};

use crate::AppState;
use crate::cas::{CasAction, CasTab};
use crate::home::PendingHousehold;
use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar};
use crate::sim::Age;

pub struct CasHairPlugin;

impl Plugin for CasHairPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_hair, hair_panel).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<CasHair>();
            HAIR_UP.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the game's hair panel is up (see `caslook::game_panel`).
pub static HAIR_UP: AtomicBool = AtomicBool::new(false);

// `CASPhysical`'s windows.
const PHYSICAL_CLOSE: u32 = 0x05db_9c00;
const PHYSICAL_HAIR: u32 = 0x05db_9c03;
const PHYSICAL_EYEBROWS: u32 = 0x05db_9c09;
const PHYSICAL_BEARD: u32 = 0x05db_9c0a;
const PHYSICAL_BODY_HAIR: u32 = 0x05db_9c0c;
// `CASHair`'s.
const TITLE: u32 = 0x05db_cf01;
const HAIR_TAB: u32 = 0x05db_b905;
const HATS_TAB: u32 = 0x05db_b906;
const STYLES_GRID: u32 = 0x05db_cf21;
const COLOURS_GRID: u32 = 0x05db_cf22;
/// What this game hasn't: the category lock, the content filter, hats' and shared hair's
/// buttons, and the colour picker.
const NOT_HERE: [u32; 7] = [0x0001_0005, 0x05db_b90f, 0x05db_cf30, 0x05db_b90e, 0x05db_b903, 0x05db_b950, HATS_TAB];
// The cells'.
const CELL_THUMBNAIL: u32 = 0x20;
const CELL_COLOUR: u32 = 0x30;
// The item grid's own (layout `ItemGrid`): its scroll bar.
const GRID_SCROLLBAR: u32 = 0x0582_0672;
/// The styles' rows in view.
const ROWS: usize = 3;

/// The panels on screen, the cells shown, and what they were shown for.
#[derive(Resource)]
pub struct CasHair {
    physical: Spawned,
    hair: Spawned,
    /// Facial hair in the panel rather than hair.
    beard: bool,
    styles: Option<Entity>,
    colours: Option<Entity>,
    bar: Option<Entity>,
    scroll: usize,
    shown: Option<(usize, bool, Age, bool, Option<s3bake::Key>, [u8; 3], usize, bool)>,
}

/// A style's cell: its entry in the list (`usize::MAX`: none).
#[derive(Component)]
struct StyleCell(usize);

/// A hair colour's swatch (of `sim::HAIRS`).
#[derive(Component)]
struct ColourCell(usize);

fn spawn_hair(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<crate::caslook::CasFrame>>, hair: Option<Res<CasHair>>) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if hair.is_some() || ui.layout("CASPhysical").is_none() || ui.layout("CASHair").is_none() {
        return;
    }
    let (Some(physical), Some(s)) = (ui.spawn(&mut commands, &mut images, &mut fonts, "CASPhysical"), ui.spawn(&mut commands, &mut images, &mut fonts, "CASHair")) else { return };
    for root in [physical.root, s.root].into_iter().flatten() {
        commands.entity(root).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(5), Visibility::Hidden));
    }
    // (The hair over the physical panel's back.)
    if let Some(r) = s.root {
        commands.entity(r).insert(GlobalZIndex(6));
    }
    for (spawned, ids) in [(&physical, &[PHYSICAL_CLOSE, PHYSICAL_EYEBROWS, PHYSICAL_BODY_HAIR][..]), (&s, &NOT_HERE[..])] {
        for id in ids {
            for e in spawned.all_with(*id) {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
    }
    for (id, tip) in [(PHYSICAL_HAIR, "Hair"), (PHYSICAL_BEARD, "Facial Hair")] {
        if let Some(e) = physical.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    if let Some(e) = s.id(HAIR_TAB) {
        commands.entity(e).insert(crate::icons::Tooltip("Hair".into()));
    }
    // The styles' scroll bar: the item grid's own, down its right.
    let mut bar = None;
    if let (Some(grid), Some(mut w)) = (s.id(STYLES_GRID), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
        let size = ui.find("CASHair", STYLES_GRID).map_or(Vec2::new(322.0, 321.0), |g| Vec2::new(g.area[2] - g.area[0], g.area[3] - g.area[1]));
        w.flags |= s3bake::ui::WIN_VISIBLE;
        w.place = s3bake::ui::UiPlace::Fixed;
        w.area = [size.x - 22.0, 6.0, size.x - 2.0, size.y - 26.0];
        bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, grid).root;
    }
    // (The grids take the wheel.)
    for id in [STYLES_GRID, COLOURS_GRID] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(bevy::ui::RelativeCursorPosition::default());
        }
    }
    HAIR_UP.store(true, Ordering::Relaxed);
    commands.insert_resource(CasHair { physical, hair: s, beard: false, styles: None, colours: None, bar, scroll: 0, shown: None });
}

/// Shown on the Hair tab: the styles for the Sim (the one worn lit), scrolled by the wheel or
/// the bar, and the colours; clicks wear and dye.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hair_panel(
    mut commands: Commands,
    hair: Option<ResMut<CasHair>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    scene: Option<Res<crate::cas::CasScene>>,
    (mut vis, mut buttons, mut bars, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>, Query<&mut Text>),
    (clicks, styles, colours, over): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &StyleCell), Changed<Interaction>>, Query<(&Interaction, &ColourCell), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut h), Some(mut ui), Some(scene)) = (hair, ui, scene) else { return };
    let h = &mut *h;
    let on = selected.1 == CasTab::Hair;
    crate::livehud::set_visible(&mut vis, h.physical.root, on);
    crate::livehud::set_visible(&mut vis, h.hair.root, on);
    if !on {
        return;
    }
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k) else { return };
    // Facial hair for grown men only (as `CASPhysical` shows its button).
    let grown = matches!(sim.age, Age::YoungAdult | Age::Adult | Age::Elder);
    let beard_ok = grown && !sim.female;
    crate::livehud::set_visible(&mut vis, h.physical.id(PHYSICAL_BEARD), beard_ok);
    if !beard_ok {
        h.beard = false;
    }
    for (id, beard) in [(PHYSICAL_HAIR, false), (PHYSICAL_BEARD, true)] {
        if crate::livehud::pressed(&clicks, h.physical.id(id)) && h.beard != beard {
            h.beard = beard;
            h.scroll = 0;
        }
    }
    for (id, on) in [(PHYSICAL_HAIR, !h.beard), (PHYSICAL_BEARD, h.beard)] {
        if let Some(mut b) = h.physical.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != on
        {
            b.selected = on;
        }
    }
    if let Some(mut b) = h.hair.id(HAIR_TAB).and_then(|e| buttons.get_mut(e).ok())
        && !b.selected
    {
        b.selected = true;
    }
    // Clicks: a style worn, a colour dyed.
    for (i, c) in &styles {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(if h.beard { CasAction::FacePart(CT_BEARD, c.0) } else { CasAction::PickPart(CT_HAIR, c.0) }));
        }
    }
    for (i, c) in &colours {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::HairColor(c.0)));
        }
    }
    let t = if h.beard { CT_BEARD } else { CT_HAIR };
    let list = crate::cas::parts_for(&scene.cas, sim, t, crate::simbody::OutfitKind::Everyday);
    // (Facial hair has none at its head.)
    let entries = list.len() + h.beard as usize;
    let cols = 3usize;
    let rows = entries.div_ceil(cols);
    let most = rows.saturating_sub(ROWS);
    // Scrolled: by the wheel over the styles, or the bar.
    let over_styles = h.hair.id(STYLES_GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    if over_styles && wheel.delta.y != 0.0 {
        let step = if wheel.delta.y > 0.0 { -1 } else { 1 };
        h.scroll = (h.scroll as i64 + step).clamp(0, most as i64) as usize;
    }
    if let Some(mut b) = h.bar.and_then(|e| bars.get_mut(e).ok()) {
        if (b.total, b.visible) != (rows, ROWS) {
            (b.total, b.visible) = (rows, ROWS);
        }
        if b.value != h.scroll {
            // (Whichever moved last: the wheel just now, or the bar.)
            if over_styles && wheel.delta.y != 0.0 {
                b.value = h.scroll;
            } else {
                h.scroll = b.value.min(most);
            }
        }
    }
    let worn = if h.beard { sim.outfit.beard } else { crate::cas::worn(&scene, sim, CT_HAIR) };
    // (Opened, or for another Sim: scrolled to the style worn, as the grid shows its choice.)
    if h.shown.as_ref().is_none_or(|s| (s.0, s.3) != (k, h.beard)) {
        let at = match worn {
            Some(w) if w != crate::sim::OutfitChoice::NONE => list.iter().position(|(key, _)| *key == w).map(|i| i + h.beard as usize),
            _ => None,
        };
        if let Some(at) = at
            && !(h.scroll * cols..(h.scroll + ROWS) * cols).contains(&at)
        {
            h.scroll = at / cols;
        }
        if let Some(mut b) = h.bar.and_then(|e| bars.get_mut(e).ok()) {
            b.value = h.scroll.min(most);
        }
    }
    h.scroll = h.scroll.min(most);
    let hair_rgb = sim.hair.to_srgba().to_u8_array_no_alpha();
    // (Again when the game's pictures of the styles become available.)
    let want = (k, sim.female, sim.age, h.beard, worn, hair_rgb, h.scroll, gu.is_some());
    if h.shown.as_ref() == Some(&want) {
        return;
    }
    let restyle = h.shown.as_ref().is_none_or(|s| (s.0, s.1, s.2, s.3, s.4, s.6, s.7) != (want.0, want.1, want.2, want.3, want.4, want.6, want.7));
    h.shown = Some(want);
    // The title: Hair, or Facial Hair.
    let title = if h.beard { ui.localize("Ui/Caption/CAS/Physical:Beard").unwrap_or_else(|| "Facial Hair".into()) } else { ui.localize("Ui/Caption/CAS/Hair:Title").unwrap_or_else(|| "Hair".into()) };
    crate::livehud::set_text(&mut texts, h.hair.text(TITLE), &title);
    // The styles in view, each in the game's cell, the one worn lit.
    if restyle && let (Some(grid), Some(g), Some(template)) = (h.hair.id(STYLES_GRID), ui.find("CASHair", STYLES_GRID).and_then(|w| w.grid), ui.layout("GenericCasItem").cloned()) {
        let holder = fresh_holder(&mut commands, grid, &mut h.styles);
        let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
        for n in (h.scroll * cols)..((h.scroll + ROWS) * cols).min(entries) {
            let at = n - h.scroll * cols;
            let (col, row) = ((at % cols) as f32, (at / cols) as f32);
            let (x, y) = (g.padding[0] + col * step.x + g.cell_padding[0], g.padding[1] + row * step.y + g.cell_padding[1]);
            let mut cell = template.clone();
            cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
            // (A button, so it lights under the pointer and when chosen, as the grid's cells do.)
            cell.cls = "Button".into();
            let entry = if h.beard { n.checked_sub(1) } else { Some(n) };
            let item = entry.and_then(|i| list.get(i));
            let chosen = match (item, worn) {
                (Some((key, _)), Some(w)) => *key == w,
                (None, w) => h.beard && (w.is_none() || w == Some(crate::sim::OutfitChoice::NONE)),
                _ => false,
            };
            let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
            let Some(root) = c.root else { continue };
            commands.entity(root).insert((StyleCell(entry.unwrap_or(usize::MAX)), crate::icons::Tooltip(item.map_or_else(|| "None".to_string(), |i| i.1.clone()))));
            if chosen {
                commands.entity(root).queue(|mut e: EntityWorldMut| {
                    if let Some(mut b) = e.get_mut::<UiButton>() {
                        b.selected = true;
                    }
                });
            }
            if let (Some(win), Some((key, _))) = (c.id(CELL_THUMBNAIL), item)
                && let Some(thumb) = gu.as_deref_mut().and_then(|g| g.icon(&mut images, &s3bake::gamedata::cas_thumb_name(key.2)))
            {
                crate::hudpanels::picture(&mut commands, win, thumb, Color::WHITE);
            }
        }
    }
    // The colours: the game's swatches, two down and across, the Sim's lit.
    if let (Some(grid), Some(g), Some(template)) = (h.hair.id(COLOURS_GRID), ui.find("CASHair", COLOURS_GRID).and_then(|w| w.grid), ui.layout("HairColorPresetGridItem").cloned()) {
        let holder = fresh_holder(&mut commands, grid, &mut h.colours);
        let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
        let rows = g.rows.max(1) as usize;
        let cur = sim.hair.to_srgba();
        for (n, (r, gr, b)) in crate::sim::HAIRS.iter().enumerate() {
            let (col, row) = ((n / rows) as f32, (n % rows) as f32);
            let (x, y) = (g.padding[0] + col * step.x + g.cell_padding[0], g.padding[1] + row * step.y + g.cell_padding[1]);
            let mut cell = template.clone();
            cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
            cell.cls = "Button".into();
            // (Each of the swatch's four colours the one colour: its roots, body, highlights
            // and tips are all dyed alike here.)
            let argb = 0xff00_0000 | ((r * 255.0).round() as u32) << 16 | ((gr * 255.0).round() as u32) << 8 | (b * 255.0).round() as u32;
            for i in 0..4 {
                crate::caslook::shade_window(&mut cell, CELL_COLOUR + i, argb);
            }
            let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
            let Some(root) = c.root else { continue };
            commands.entity(root).insert(ColourCell(n));
            // (The one chosen in the cell's chosen frame, over its colours: they cover the
            // cell, its own frame under them.)
            let on = (cur.red - r).abs() < 0.01 && (cur.green - gr).abs() < 0.01 && (cur.blue - b).abs() < 0.01;
            if on
                && let Some(s3bake::ui::UiDrawable::Std { images: keys, .. }) = &template.drawable
                && let Some((frame, _)) = ui.image(&mut images, keys[4])
            {
                commands.spawn((ImageNode::new(frame), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(root)));
            }
        }
    }
}

/// A holder for a grid's cells, the last one's put away.
fn fresh_holder(commands: &mut Commands, grid: Entity, last: &mut Option<Entity>) -> Entity {
    if let Some(old) = last.take() {
        commands.entity(old).despawn();
    }
    let h = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    // (Under the grid's scroll bar.)
    commands.entity(grid).insert_children(0, &[h]);
    *last = Some(h);
    h
}
