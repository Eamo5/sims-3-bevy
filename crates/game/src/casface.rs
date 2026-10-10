//! Create a Sim's Face in the game's own panels, as UI.dll's `CASFacialDetails` and its regions'
//! panels (`CASHeadEars`, `CASEyes`, `CASNose`, `CASMouth`, `CASMakeup`) show it. Down the
//! facial details panel's side, the regions; each region's Advanced page has the game's picture
//! of it with its parts' hotspots, a part's sliders in the game's rows (`CASBodySlider`) beside
//! it, and the eyes' Basics page their colours (`EyeColorPresetGridItem`). Make-up: eye shadow
//! and lipstick in the game's item cells, none first.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use s3formats::sim::{CT_EYESHADOW, CT_LIPSTICK};

use crate::AppState;
use crate::cas::{CasAction, CasTab};
use crate::home::PendingHousehold;
use crate::layout::{Spawned, UiAssets, UiButton, UiSlider, edit_windows};
use crate::sim::Age;

pub struct CasFacePlugin;

impl Plugin for CasFacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_face, face_panel).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<CasFace>();
            FACE_UP.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the game's face panels are up (see `caslook::game_panel`).
pub static FACE_UP: AtomicBool = AtomicBool::new(false);

// `CASFacialDetails`'s: the regions' buttons (head and ears, eyes, nose, mouth, make-up), and
// what this game hasn't (moles and freckles, tattoos, the plastic surgeon's and mirror's tabs,
// the close button).
const REGION_BUTTONS: [u32; 5] = [0x05db_9c02, 0x05db_9c04, 0x05db_9c06, 0x05db_9c05, 0x05db_9c0a];
const DETAILS_NOT_HERE: [u32; 5] = [0x05db_9c09, 0x092f_a960, 0x0b98_4090, 0x0000_1001, 0x05db_9c00];

/// A region's panel: its layout, its Basics and Advanced pages and their buttons, and its
/// parts (hotspot button, name, slider grid, and the face sliders it moves:
/// `gamedata::FACE_SLIDERS`), the last its whole (the game's Global).
struct Region {
    layout: &'static str,
    basics: u32,
    advanced: u32,
    basics_button: u32,
    advanced_button: u32,
    parts: &'static [(u32, u32, u32, &'static [usize])],
}

const REGIONS: [Region; 4] = [
    Region {
        layout: "CASHeadEars",
        basics: 0x05db_b310,
        advanced: 0x05db_b320,
        basics_button: 0x05db_b301,
        advanced_button: 0x05db_b302,
        parts: &[(0x05db_bb21, 0x05db_bb32, 0x05db_bb31, &[1, 2]), (0x05db_bb23, 0x05db_bb52, 0x05db_bb51, &[0]), (0x05db_bb22, 0x05db_bb42, 0x05db_bb41, &[14, 15]), (0x05db_bb24, 0x05db_b332, 0x05db_b331, &[]), (0x05db_bb25, 0x05db_bb62, 0x05db_bb61, &[16])],
    },
    Region {
        layout: "CASEyes",
        basics: 0x05db_b910,
        advanced: 0x05db_b920,
        basics_button: 0x05db_b901,
        advanced_button: 0x05db_b902,
        parts: &[(0x05db_b921, 0x05db_b932, 0x05db_b931, &[]), (0x05db_b923, 0x05db_b952, 0x05db_b951, &[9]), (0x05db_b925, 0x05db_b962, 0x05db_b961, &[6]), (0x05db_b927, 0x05db_b972, 0x05db_b971, &[7, 8])],
    },
    Region {
        layout: "CASNose",
        basics: 0x05db_b910,
        advanced: 0x05db_b920,
        basics_button: 0x05db_b901,
        advanced_button: 0x05db_b902,
        parts: &[(0x05db_b921, 0x05db_b932, 0x05db_b931, &[11]), (0x05db_b922, 0x05db_b942, 0x05db_b941, &[12, 13]), (0x05db_b923, 0x05db_b952, 0x05db_b951, &[]), (0x05db_b924, 0x05db_b962, 0x05db_b961, &[10])],
    },
    Region {
        layout: "CASMouth",
        basics: 0x05db_b710,
        advanced: 0x05db_b720,
        basics_button: 0x05db_b701,
        advanced_button: 0x05db_b702,
        parts: &[(0x05db_b721, 0x05db_b732, 0x05db_b731, &[4]), (0x05db_b722, 0x05db_b742, 0x05db_b741, &[]), (0x05db_b723, 0x05db_b752, 0x05db_b751, &[3, 5])],
    },
];
// `CASEyes`'s colours and what it hasn't (deleting, the colour picker).
const EYE_COLOURS: u32 = 0x05db_b914;
const EYES_NOT_HERE: [u32; 2] = [0x05db_b908, 0x05db_b915];
// `CASMakeup`'s: its kinds (eye shadow, eyeliner, blush, lipstick, costume), its parts' grid,
// and what this game hasn't.
const MAKEUP_KINDS: [u32; 5] = [0x100, 0x101, 0x102, 0x103, 0x104];
const MAKEUP_PARTS: u32 = 0x1001;
const MAKEUP_NOT_HERE: [u32; 10] = [0x0cd0_e9e0, 0x096d_7730, 0x1006, 0x1003, 0x1004, 0x1005, 0x2000, 0x101, 0x102, 0x104];
// `CASBodySlider`'s and the item cells'.
const SLIDER_TITLE: u32 = 2;
const SLIDER: u32 = 4;
const CELL_THUMBNAIL: u32 = 0x20;
const FACE_SLIDER_NAMES: [(usize, &str); 17] = [
    (16, "Head Width"),
    (0, "Jaw Width"),
    (1, "Chin Size"),
    (2, "Chin Height"),
    (14, "Cheekbones"),
    (15, "Cheeks"),
    (6, "Eye Size"),
    (7, "Eye Spacing"),
    (8, "Eye Height"),
    (9, "Brow Height"),
    (10, "Nose Size"),
    (11, "Nose Width"),
    (12, "Nose Tilt"),
    (13, "Nose Tip"),
    (3, "Mouth Width"),
    (4, "Lip Fullness"),
    (5, "Mouth Height"),
];

/// The panels on screen, the region shown (4: make-up), its page and part, and what was shown.
#[derive(Resource)]
pub struct CasFace {
    details: Spawned,
    regions: Vec<Spawned>,
    makeup: Spawned,
    region: usize,
    basics: bool,
    part: [usize; 4],
    lipstick: bool,
    cells: Option<Entity>,
    sliders: Vec<(Entity, usize, f32)>,
    /// The face's part being sculpted as Create a Sim last had it (followed when it changes).
    area: u8,
    shown: Option<(usize, usize, bool, [usize; 4], bool, Age, bool, [u8; 3], Option<s3bake::Key>, Option<s3bake::Key>, bool)>,
}

/// A slider row's slider: the face slider it moves.
#[derive(Component)]
struct FaceSliderRow(usize);

/// An eye colour's cell (of `sim::EYES`), or a make-up part's (`usize::MAX`: none).
#[derive(Component)]
struct EyeCell(usize);
#[derive(Component)]
struct MakeupCell(usize);

fn spawn_face(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<crate::caslook::CasFrame>>, face: Option<Res<CasFace>>) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if face.is_some() || ["CASFacialDetails", "CASMakeup", "CASBodySlider", "EyeColorPresetGridItem"].into_iter().chain(REGIONS.iter().map(|r| r.layout)).any(|l| ui.layout(l).is_none()) {
        return;
    }
    // (Make-up where moles and freckles were, this game having none.)
    let Some(mut dw) = ui.layout("CASFacialDetails").cloned() else { return };
    let mut moles = None;
    edit_windows(&mut dw, 0x05db_9c09, &mut |w| moles = Some(w.area));
    if let Some(area) = moles {
        edit_windows(&mut dw, REGION_BUTTONS[4], &mut |w| w.area = area);
    }
    let details = ui.spawn_root(&mut commands, &mut images, &mut fonts, &dw);
    for id in DETAILS_NOT_HERE {
        for e in details.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for (id, tip) in REGION_BUTTONS.iter().zip(["Head and Ears", "Eyes", "Nose", "Mouth", "Makeup"]) {
        if let Some(e) = details.id(*id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    let mut regions = Vec::new();
    for (n, r) in REGIONS.iter().enumerate() {
        let Some(mut w) = ui.layout(r.layout).cloned() else { return };
        // (The eyes' Basics page: their colours where the shapes' presets were; the others'
        // only their Advanced page, this game having no preset faces.)
        if n == 1 {
            edit_windows(&mut w, r.basics, &mut |b| {
                let frames: Vec<usize> = b.children.iter().enumerate().filter(|(_, c)| c.id == 0x05db_cf23).map(|(i, _)| i).collect();
                if let [presets, colours, ..] = frames[..] {
                    b.children[presets].flags &= !s3bake::ui::WIN_VISIBLE;
                    let dy = b.children[colours].area[1] - b.children[presets].area[1];
                    b.children[colours].area[1] -= dy;
                    b.children[colours].area[3] -= dy;
                }
            });
        }
        for (_, _, grid, sliders) in r.parts {
            if sliders.is_empty() {
                continue;
            }
            edit_windows(&mut w, *grid, &mut |g| g.flags |= s3bake::ui::WIN_VISIBLE);
        }
        let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
        // (Parts this game can't shape put away.)
        for (button, text, grid, sliders) in r.parts {
            if sliders.is_empty() {
                for id in [button, text, grid] {
                    for e in s.all_with(*id) {
                        commands.entity(e).insert(Visibility::Hidden);
                    }
                }
            }
        }
        let hide: Vec<u32> = if n == 1 { EYES_NOT_HERE.to_vec() } else { vec![r.basics_button, r.advanced_button] };
        for id in hide {
            for e in s.all_with(id) {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
        for (id, tip) in [(r.basics_button, "Basics"), (r.advanced_button, "Advanced")] {
            if let Some(e) = s.id(id) {
                commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
            }
        }
        regions.push(s);
    }
    let Some(makeup) = ui.spawn(&mut commands, &mut images, &mut fonts, "CASMakeup") else { return };
    for id in MAKEUP_NOT_HERE {
        for e in makeup.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    // (The presets' grid of a part's colours: this game's make-up has its own.)
    if let Some(e) = makeup.id(0x1002) {
        commands.entity(e).insert(Visibility::Hidden);
    }
    for (id, tip) in [(MAKEUP_KINDS[0], "Eye Shadow"), (MAKEUP_KINDS[3], "Lipstick")] {
        if let Some(e) = makeup.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    for root in [details.root, makeup.root].into_iter().chain(regions.iter().map(|s| s.root)).flatten() {
        commands.entity(root).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(5), Visibility::Hidden));
    }
    for root in [makeup.root].into_iter().chain(regions.iter().map(|s| s.root)).flatten() {
        commands.entity(root).insert(GlobalZIndex(6));
    }
    // (Each part's last slot, the region's whole, first.)
    let part = [0, 1, 2, 3].map(|n: usize| REGIONS[n].parts.len() - 1);
    FACE_UP.store(true, Ordering::Relaxed);
    commands.insert_resource(CasFace { details, regions, makeup, region: 0, basics: false, part, lipstick: false, cells: None, sliders: Vec::new(), area: 0, shown: None });
}

/// Shown on the Face tab: the region chosen, its page and part, the part's sliders where the
/// Sim's face is (moved: the face sculpted), the eye colours, the make-up.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn face_panel(
    mut commands: Commands,
    face: Option<ResMut<CasFace>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    scene: Option<Res<crate::cas::CasScene>>,
    (mut vis, mut buttons, mut sliders): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiSlider>),
    (clicks, eyes, makeup): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &EyeCell), Changed<Interaction>>, Query<(&Interaction, &MakeupCell), Changed<Interaction>>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut f), Some(mut ui), Some(scene)) = (face, ui, scene) else { return };
    let f = &mut *f;
    let on = selected.1 == CasTab::Face;
    crate::livehud::set_visible(&mut vis, f.details.root, on);
    if !on {
        for s in f.regions.iter().chain([&f.makeup]) {
            crate::livehud::set_visible(&mut vis, s.root, false);
        }
        return;
    }
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k) else { return };
    // Make-up for those the game's parts are for (by age and gender).
    let shadows = crate::cas::parts_for(&scene.cas, sim, CT_EYESHADOW, crate::simbody::OutfitKind::Everyday);
    let lipsticks = crate::cas::parts_for(&scene.cas, sim, CT_LIPSTICK, crate::simbody::OutfitKind::Everyday);
    let makeup_ok = !sim.age.is_little() && (!shadows.is_empty() || !lipsticks.is_empty());
    crate::livehud::set_visible(&mut vis, f.details.id(REGION_BUTTONS[4]), makeup_ok);
    if !makeup_ok && f.region == 4 {
        f.region = 0;
    }
    // The region, page and part chosen (the region as Create a Sim's sculpting has it too).
    if scene.face_area != f.area {
        f.area = scene.face_area;
        f.region = f.area as usize;
        f.basics = f.region == 1;
    }
    for (n, id) in REGION_BUTTONS.iter().enumerate() {
        if crate::livehud::pressed(&clicks, f.details.id(*id)) && f.region != n {
            f.region = n;
            if n < 4 {
                actions.write(crate::cas::CasActionRequest(CasAction::FaceArea(n as u8)));
            }
        }
    }
    if f.region < 4 {
        let r = &REGIONS[f.region];
        let s = &f.regions[f.region];
        if crate::livehud::pressed(&clicks, s.id(r.basics_button)) {
            f.basics = true;
        }
        if crate::livehud::pressed(&clicks, s.id(r.advanced_button)) {
            f.basics = false;
        }
        for (i, (button, ..)) in r.parts.iter().enumerate() {
            if crate::livehud::pressed(&clicks, s.id(*button)) {
                f.part[f.region] = i;
            }
        }
    } else {
        for (id, lip) in [(MAKEUP_KINDS[0], false), (MAKEUP_KINDS[3], true)] {
            if crate::livehud::pressed(&clicks, f.makeup.id(id)) {
                f.lipstick = lip;
            }
        }
    }
    // (The eyes' Basics page only for the eyes.)
    let basics = f.basics && f.region == 1;
    // The face sculpted: a slider moved.
    let values = crate::simbody::face_sliders(sim);
    for (e, i, last) in f.sliders.iter_mut() {
        let Ok(s) = sliders.get(*e) else { continue };
        if s.value != *last {
            *last = s.value;
            actions.write(crate::cas::CasActionRequest(CasAction::SetFaceSlider(*i as u8, s.value / 128.0 - 1.0)));
        }
    }
    for (i, c) in &eyes {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::EyeColor(c.0)));
        }
    }
    for (i, c) in &makeup {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::FacePart(if f.lipstick { CT_LIPSTICK } else { CT_EYESHADOW }, c.0)));
        }
    }
    let eyes_rgb = sim.eyes.to_srgba().to_u8_array_no_alpha();
    let want = (k, f.region, basics, f.part, f.lipstick, sim.age, sim.female, eyes_rgb, sim.outfit.eyeshadow, sim.outfit.lipstick, gu.is_some());
    if f.shown.as_ref() == Some(&want) {
        // (The sliders follow the face when it's changed other than by them: made at random.)
        for (e, i, last) in f.sliders.iter_mut() {
            let v = ((values.get(*i).copied().unwrap_or(0.0) + 1.0) * 128.0).round();
            if (v - *last).abs() > 1.0
                && let Ok(mut s) = sliders.get_mut(*e)
                && !s.is_grabbed()
            {
                s.value = v;
                *last = v;
            }
        }
        return;
    }
    f.shown = Some(want);
    // The region's panel, its page, its part's hotspot and name lit and its grid shown.
    for (n, s) in f.regions.iter().enumerate() {
        crate::livehud::set_visible(&mut vis, s.root, f.region == n);
    }
    crate::livehud::set_visible(&mut vis, f.makeup.root, f.region == 4);
    for (n, id) in REGION_BUTTONS.iter().enumerate() {
        if let Some(mut b) = f.details.id(*id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != (f.region == n)
        {
            b.selected = f.region == n;
        }
    }
    if let Some(old) = f.cells.take() {
        commands.entity(old).despawn();
    }
    f.sliders.clear();
    if f.region < 4 {
        let r = &REGIONS[f.region];
        let s = f.regions[f.region].clone();
        crate::livehud::set_visible(&mut vis, s.id(r.basics), basics);
        crate::livehud::set_visible(&mut vis, s.id(r.advanced), !basics);
        for (id, on) in [(r.basics_button, basics), (r.advanced_button, !basics)] {
            if let Some(mut b) = s.id(id).and_then(|e| buttons.get_mut(e).ok())
                && b.selected != on
            {
                b.selected = on;
            }
        }
        let part = f.part[f.region];
        for (i, (button, text, grid, list)) in r.parts.iter().enumerate() {
            if list.is_empty() {
                continue;
            }
            if let Some(mut b) = s.id(*button).and_then(|e| buttons.get_mut(e).ok())
                && b.selected != (i == part)
            {
                b.selected = i == part;
            }
            crate::livehud::set_visible(&mut vis, s.id(*text), i == part);
            crate::livehud::set_visible(&mut vis, s.id(*grid), i == part);
        }
        if basics {
            // The eye colours, the Sim's framed.
            let (Some(grid), Some(g), Some(template)) = (s.id(EYE_COLOURS), ui.find(r.layout, EYE_COLOURS).and_then(|w| w.grid), ui.layout("EyeColorPresetGridItem").cloned()) else { return };
            let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
            f.cells = Some(holder);
            let rows = g.rows.max(1) as usize;
            let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
            let cur = sim.eyes.to_srgba();
            for (n, (r, gr, b)) in crate::sim::EYES.iter().enumerate() {
                let (x, y) = (g.padding[0] + (n / rows) as f32 * step.x, g.padding[1] + (n % rows) as f32 * step.y);
                let mut cell = template.clone();
                cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
                cell.cls = "Button".into();
                let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
                let Some(root) = c.root else { continue };
                commands.entity(root).insert(EyeCell(n));
                // (The iris in the colour, as the game's swatch shows the eye.)
                if let Some(win) = c.id(CELL_THUMBNAIL) {
                    let iris = images.add(iris_image(Color::srgb(*r, *gr, *b)));
                    crate::hudpanels::picture(&mut commands, win, iris, Color::WHITE);
                }
                if (cur.red - r).abs() < 0.01 && (cur.green - gr).abs() < 0.01 && (cur.blue - b).abs() < 0.01
                    && let Some(s3bake::ui::UiDrawable::Std { images: keys, .. }) = &template.drawable
                    && let Some((frame, _)) = ui.image(&mut images, keys[4])
                {
                    commands.spawn((ImageNode::new(frame), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(root)));
                }
            }
            return;
        }
        // The part's sliders, in the game's rows.
        let (_, _, grid_id, list) = r.parts[part];
        let (Some(grid), Some(g), Some(template)) = (s.id(grid_id), ui.find(r.layout, grid_id).and_then(|w| w.grid), ui.layout("CASBodySlider").cloned()) else { return };
        let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
        f.cells = Some(holder);
        let step = g.cell[1] + g.cell_padding[1] + g.cell_padding[3];
        for (n, &i) in list.iter().enumerate() {
            let (x, y) = (g.padding[0], g.padding[1] + n as f32 * step);
            let mut row = template.clone();
            row.area = [x, y, x + g.cell[0], y + g.cell[1]];
            let name = FACE_SLIDER_NAMES.iter().find(|(j, _)| *j == i).map_or("", |x| x.1);
            edit_windows(&mut row, SLIDER_TITLE, &mut |t| t.caption = name.to_string());
            let v = ((values.get(i).copied().unwrap_or(0.0) + 1.0) * 128.0).round();
            edit_windows(&mut row, SLIDER, &mut |sl| {
                if let Some(range) = sl.slider.as_mut() {
                    range[2] = v;
                }
            });
            let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &row, holder);
            if let Some(e) = c.id(SLIDER) {
                commands.entity(e).insert(FaceSliderRow(i));
                f.sliders.push((e, i, v));
            }
        }
        return;
    }
    // Make-up: the kind lit, its parts (none first) in the game's cells, the worn one lit.
    for (id, lip) in [(MAKEUP_KINDS[0], false), (MAKEUP_KINDS[3], true)] {
        if let Some(mut b) = f.makeup.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != (f.lipstick == lip)
        {
            b.selected = f.lipstick == lip;
        }
    }
    let (list, worn) = if f.lipstick { (&lipsticks, sim.outfit.lipstick) } else { (&shadows, sim.outfit.eyeshadow) };
    let (Some(grid), Some(g), Some(template)) = (f.makeup.id(MAKEUP_PARTS), ui.find("CASMakeup", MAKEUP_PARTS).and_then(|w| w.grid), ui.layout("GenericCasItem").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
    f.cells = Some(holder);
    let cols = g.columns.max(1) as usize;
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    let none = worn.is_none() || worn == Some(crate::sim::OutfitChoice::NONE);
    for n in 0..(list.len() + 1).min(cols * g.rows.max(1) as usize) {
        let (x, y) = (g.padding[0] + (n % cols) as f32 * step.x, g.padding[1] + (n / cols) as f32 * step.y);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let item = n.checked_sub(1).and_then(|i| list.get(i));
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = c.root else { continue };
        commands.entity(root).insert((MakeupCell(n.checked_sub(1).unwrap_or(usize::MAX)), crate::icons::Tooltip(item.map_or_else(|| "None".to_string(), |i| i.1.clone()))));
        let chosen = match item {
            Some((key, _)) => worn == Some(*key),
            None => none,
        };
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

/// An eye in a colour, close up: the iris (lighter towards the pupil, the colour doubled over its
/// shading as the eyes' texture is), its dark rim, the pupil and a glint, on the white.
fn iris_image(c: Color) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: usize = 64;
    let s = c.to_srgba();
    let base = [(s.red * 1.5).min(1.0), (s.green * 1.5).min(1.0), (s.blue * 1.5).min(1.0)];
    let mut px = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 + 0.5 - N as f32 * 0.5, y as f32 + 0.5 - N as f32 * 0.5);
            let d = (dx * dx + dy * dy).sqrt() / (N as f32 * 0.5);
            let (rgb, a) = if d > 1.0 {
                ([0.0; 3], 0.0)
            } else if d > 0.82 {
                ([0.96, 0.95, 0.93], 1.0)
            } else if d > 0.74 {
                (base.map(|v| v * 0.35), 1.0)
            } else if d > 0.24 {
                // (Streaks round the iris, lighter in.)
                let ang = dy.atan2(dx);
                let streak = 0.9 + 0.1 * (ang * 18.0).sin();
                let light = 1.15 - 0.55 * ((d - 0.24) / 0.5);
                (base.map(|v| (v * light * streak).min(1.0)), 1.0)
            } else {
                ([0.04, 0.03, 0.03], 1.0)
            };
            // (A glint, up and to the left.)
            let g = ((dx + 7.0).powi(2) + (dy + 7.0).powi(2)).sqrt();
            let rgb = if g < 3.5 && d < 0.8 { [1.0; 3] } else { rgb };
            let i = (y * N + x) * 4;
            // (Its edge softened over a pixel.)
            let edge = ((1.0 - d) * N as f32 * 0.5).clamp(0.0, 1.0);
            px[i..i + 4].copy_from_slice(&[(rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8, (a * edge * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}
