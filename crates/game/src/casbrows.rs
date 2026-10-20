//! Create a Sim's eyebrows, in the game's own panel (UI.dll's `CASEyebrows`): the styles for the
//! Sim in its presets grid, three across with the game's pictures, scrolled by the wheel or the
//! item grid's own bar, the one chosen lit; a click wears it. Beneath, their colour: the hair
//! colours' swatches, and "Match Hair" (lit while they're the hair's, as Sims come: the game's
//! `EyebrowsUseHairColor`). Opened by the physical panel's Eyebrows button (`cashair`).

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use s3formats::sim::CT_EYEBROW;

use crate::AppState;
use crate::cas::{CasAction, CasTab};
use crate::home::PendingHousehold;
use crate::layout::{Spawned, UiAssets, UiScrollBar};
use crate::sim::Age;

pub struct CasBrowsPlugin;

impl Plugin for CasBrowsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_brows, brows_panel).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<CasBrows>();
            OPEN.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the eyebrows are shown on the Hair tab rather than the hair (the physical panel's
/// buttons switch it: `cashair`).
pub static OPEN: AtomicBool = AtomicBool::new(false);

// `CASEyebrows`' windows: the styles' grid, the colours' grid and the match-hair button, and
// what this game hasn't (the eyelashes with the colour picker and its saved colours, the content
// filters).
const GRID: u32 = 0x05db_b941;
const COLOURS: u32 = 0x05db_b950;
const MATCH_HAIR: u32 = 0x05db_b946;
/// The Color button: the game's picker for a colour of their own.
const PICKER: u32 = 0x05db_b944;
const NOT_HERE: [u32; 3] = [0x05db_9719, 0x05db_b945, 0x05db_b90f];
const CELL_THUMBNAIL: u32 = 0x20;
const ROWS: usize = 3;

#[derive(Resource)]
pub struct CasBrows {
    s: Spawned,
    cells: Option<Entity>,
    bar: Option<Entity>,
    scroll: usize,
    shown: Option<(usize, bool, Age, Option<s3bake::Key>, usize, bool)>,
    colours: Option<Entity>,
    colours_shown: Option<(usize, Option<Color>, Color)>,
}

/// A colour's swatch (its place in the hair colours).
#[derive(Component)]
struct BrowColourCell(usize);

/// A style's cell (its place in the list).
#[derive(Component)]
struct BrowCell(usize);

fn spawn_brows(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<crate::caslook::CasFrame>>, brows: Option<Res<CasBrows>>) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if brows.is_some() || ui.layout("CASEyebrows").is_none() || ui.layout("GenericCasItemGrow_115x115").is_none() {
        return;
    }
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "CASEyebrows") else { return };
    for id in NOT_HERE {
        for e in s.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    let Some(root) = s.root else { return };
    commands.entity(root).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(6), Visibility::Hidden));
    // The styles' scroll bar: the item grid's own, down its right; the grid takes the wheel.
    let mut bar = None;
    if let (Some(grid), Some(mut b)) = (s.id(GRID), ui.find("ItemGrid", crate::cashair::GRID_SCROLLBAR).cloned()) {
        let size = ui.find("CASEyebrows", GRID).map_or(Vec2::new(317.0, 309.0), |g| Vec2::new(g.area[2] - g.area[0], g.area[3] - g.area[1]));
        b.flags |= s3bake::ui::WIN_VISIBLE;
        b.place = s3bake::ui::UiPlace::Fixed;
        b.area = [size.x - 22.0, 6.0, size.x - 2.0, size.y - 26.0];
        bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, grid).root;
        commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default());
    }
    if let Some(e) = s.id(MATCH_HAIR) {
        commands.entity(e).insert(crate::icons::Tooltip("Match Hair Color".into()));
    }
    if let Some(e) = s.id(PICKER) {
        commands.entity(e).insert(crate::icons::Tooltip("Color".into()));
    }
    commands.insert_resource(CasBrows { s, cells: None, bar, scroll: 0, shown: None, colours: None, colours_shown: None });
}

/// Shown on the Hair tab when the eyebrows are open: the styles for the Sim, the one chosen lit.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn brows_panel(
    mut commands: Commands,
    brows: Option<ResMut<CasBrows>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    scene: Option<Res<crate::cas::CasScene>>,
    (mut vis, mut bars): (Query<&mut Visibility>, Query<&mut UiScrollBar>),
    (cells, over, swatches, clicks, mut buttons): (
        Query<(&Interaction, &BrowCell), Changed<Interaction>>,
        Query<&bevy::ui::RelativeCursorPosition>,
        Query<(&Interaction, &BrowColourCell), Changed<Interaction>>,
        Query<(Entity, &Interaction), Changed<Interaction>>,
        Query<&mut crate::layout::UiButton>,
    ),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut b), Some(mut ui), Some(scene)) = (brows, ui, scene) else { return };
    let b = &mut *b;
    let on = selected.1 == CasTab::Hair && OPEN.load(Ordering::Relaxed) && !crate::caslook::modal_up();
    crate::livehud::set_visible(&mut vis, b.s.root, on);
    if !on {
        b.shown = None;
        b.colours_shown = None;
        return;
    }
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k) else { return };
    for (i, c) in &cells {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::PickPart(CT_EYEBROW, c.0)));
        }
    }
    // Their colour: a swatch, or the hair's.
    for (i, c) in &swatches {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::BrowColor(c.0)));
        }
    }
    if crate::livehud::pressed(&clicks, b.s.id(MATCH_HAIR)) {
        actions.write(crate::cas::CasActionRequest(CasAction::BrowsMatchHair));
    }
    if crate::livehud::pressed(&clicks, b.s.id(PICKER)) {
        let c = sim.brow_hair.unwrap_or(sim.hair).to_srgba();
        commands.insert_resource(crate::cascolour::PickColour(crate::cascolour::PickFor::Brows, [c.red, c.green, c.blue]));
    }
    let colours = (k, sim.brow_hair, sim.hair);
    if b.colours_shown != Some(colours) {
        b.colours_shown = Some(colours);
        crate::cashair::spawn_colours(&mut commands, &mut ui, (&mut images, &mut fonts), ("CASEyebrows", COLOURS), &b.s, &mut b.colours, sim.brow_hair, |e, n| {
            e.insert(BrowColourCell(n));
        });
        if let Some(mut m) = b.s.id(MATCH_HAIR).and_then(|e| buttons.get_mut(e).ok())
            && m.selected != sim.brow_hair.is_none()
        {
            m.selected = sim.brow_hair.is_none();
        }
    }
    let list = crate::cas::parts_for(&scene.cas, sim, CT_EYEBROW, crate::simbody::OutfitKind::Everyday);
    let worn = sim.outfit.brows;
    let rows = list.len().div_ceil(3);
    let most = rows.saturating_sub(ROWS);
    let over_grid = b.s.id(GRID).and_then(|e| over.get(e).ok()).is_some_and(|c| c.cursor_over());
    let wheeled = over_grid && wheel.delta.y != 0.0;
    if wheeled {
        b.scroll = (b.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
    }
    // (Opened, or for another Sim: scrolled to the style chosen.)
    let fresh = b.shown.as_ref().is_none_or(|s| s.0 != k);
    if fresh
        && let Some(at) = worn.and_then(|w| list.iter().position(|(key, _)| *key == w))
        && !(b.scroll * 3..(b.scroll + ROWS) * 3).contains(&at)
    {
        b.scroll = at / 3;
    }
    b.scroll = b.scroll.min(most);
    if let Some(mut bar) = b.bar.and_then(|e| bars.get_mut(e).ok()) {
        if (bar.total, bar.visible) != (rows, ROWS) {
            (bar.total, bar.visible) = (rows, ROWS);
        }
        if bar.value != b.scroll {
            if wheeled || fresh {
                bar.value = b.scroll;
            } else {
                b.scroll = bar.value.min(most);
            }
        }
    }
    let want = (k, sim.female, sim.age, worn, b.scroll, gu.is_some());
    if b.shown.as_ref() == Some(&want) {
        return;
    }
    b.shown = Some(want);
    if let Some(old) = b.cells.take() {
        commands.entity(old).try_despawn();
    }
    let (Some(grid), Some(g), Some(template)) = (b.s.id(GRID), ui.find("CASEyebrows", GRID).and_then(|w| w.grid), ui.layout("GenericCasItemGrow_115x115").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    // (Under the grid's scroll bar.)
    commands.entity(grid).insert_children(0, &[holder]);
    b.cells = Some(holder);
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    let first = b.scroll * 3;
    for (n, (key, name)) in list.iter().enumerate().skip(first).take(ROWS * 3) {
        let at = n - first;
        let (x, y) = (g.padding[0] + (at % 3) as f32 * step.x, g.padding[1] + (at / 3) as f32 * step.y);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = c.root else { continue };
        commands.entity(root).insert((BrowCell(n), crate::icons::Tooltip(name.clone())));
        if worn == Some(*key) {
            commands.entity(root).insert(crate::layout::Selected);
        }
        if let (Some(win), Some(thumb)) = (c.id(CELL_THUMBNAIL), gu.as_deref_mut().and_then(|g| g.icon(&mut images, &s3bake::gamedata::cas_thumb_name(key.2)))) {
            crate::hudpanels::picture(&mut commands, win, thumb, Color::WHITE);
        }
    }
}
