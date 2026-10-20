//! Create a Sim's body hair, in the game's own panel (UI.dll's `CASBodyHair`), opened by the
//! physical panel's Body Hair button for grown men: where it grows picked by the panel's region
//! buttons (the chest, the back, the arms, the legs) and, under them, the parts of each (upper
//! chest and stomach; upper, full and lower back; calves and feet), then that place's styles in
//! the game's item cells with the game's pictures, "none" first, the one worn lit. One for each
//! place, drawn onto the body under the clothes in the hair's colour.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::AppState;
use crate::cas::{CasAction, CasTab};
use crate::home::PendingHousehold;
use crate::layout::{Spawned, UiAssets, UiButton};

pub struct CasBodyHairPlugin;

impl Plugin for CasBodyHairPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, body_hair_test.run_if(in_state(crate::PlayMode::Live)));
        app.add_systems(Update, (spawn_body_hair, body_hair_panel).chain().run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<CasBodyHair>();
            OPEN.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the body hair's shown on the Hair tab rather than the hair (the physical panel's
/// buttons switch it: `cashair`).
pub static OPEN: AtomicBool = AtomicBool::new(false);

// `CASBodyHair`'s windows: its regions' buttons and the panels of their parts, the parts'
// buttons (each with the body type it picks: `s3formats::sim::BODY_HAIR_TYPES`), the styles'
// grid, and what this game hasn't (the filter, the colour's share and delete, the colours: it's
// the hair's).
const REGIONS: [(u32, Option<u32>, &[(Option<u32>, u32)]); 4] = [
    (0x092f_a032, Some(0x092f_a061), &[(Some(0x092f_a038), 39), (Some(0x092f_a039), 40)]),
    (0x092f_a033, Some(0x092f_a062), &[(Some(0x092f_a03b), 42), (Some(0x092f_a03c), 43), (Some(0x092f_a03d), 41)]),
    (0x092f_a031, None, &[(None, 44)]),
    (0x092f_a030, Some(0x092f_a060), &[(Some(0x092f_a034), 45), (Some(0x092f_a035), 46)]),
];
const REGION_TIPS: [&str; 4] = ["Chest", "Back", "Arms", "Legs"];
const PART_TIPS: [(u32, &str); 8] = [(39, "Upper Chest"), (40, "Stomach"), (42, "Upper Back"), (43, "Full Back"), (41, "Lower Back"), (44, "Forearms"), (45, "Calves"), (46, "Feet")];
const GRID: u32 = 0x05db_b941;
const NOT_HERE: [u32; 3] = [0x05db_b945, 0x05db_9719, 0x05db_cf20];
const CELL_THUMBNAIL: u32 = 0x20;

#[derive(Resource)]
pub struct CasBodyHair {
    s: Spawned,
    region: usize,
    /// The body type picked (where it grows).
    part: u32,
    cells: Option<Entity>,
    shown: Option<(usize, usize, u32, Vec<s3bake::Key>, bool)>,
}

/// A style's cell: its place in the list (`usize::MAX`: none).
#[derive(Component)]
struct BodyHairCell(usize);

fn spawn_body_hair(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), frame: Option<Res<crate::caslook::CasFrame>>, open: Option<Res<CasBodyHair>>) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if open.is_some() || ui.layout("CASBodyHair").is_none() || ui.layout("GenericCasItem").is_none() {
        return;
    }
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "CASBodyHair") else { return };
    for id in NOT_HERE {
        for e in s.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    let Some(root) = s.root else { return };
    commands.entity(root).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(6), Visibility::Hidden));
    for ((button, ..), tip) in REGIONS.iter().zip(REGION_TIPS) {
        if let Some(e) = s.id(*button) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    for (_, _, parts) in REGIONS {
        for (button, t) in parts {
            if let (Some(e), Some((_, tip))) = (button.and_then(|b| s.id(b)), PART_TIPS.iter().find(|p| p.0 == *t)) {
                commands.entity(e).insert(crate::icons::Tooltip((*tip).into()));
            }
        }
    }
    commands.insert_resource(CasBodyHair { s, region: 0, part: 39, cells: None, shown: None });
    // (CAS_BODY_HAIR=1, tests: open on the Hair tab.)
    if std::env::var_os("CAS_BODY_HAIR").is_some() {
        OPEN.store(true, Ordering::Relaxed);
    }
}

/// BODY_HAIR=<first name> (tests): that Sim given the first body hair made for them of each
/// place, and into their swimwear to show it.
fn body_hair_test(mut commands: Commands, time: Res<Time>, mut done: Local<bool>, cas: Option<Res<crate::simbody::CasData>>, mut sims: Query<(Entity, &mut crate::sim::Sim)>) {
    let Ok(who) = std::env::var("BODY_HAIR") else { return };
    if *done || time.elapsed_secs() < 8.0 {
        return;
    }
    let Some(cas) = cas else { return };
    let Some((e, mut sim)) = sims.iter_mut().find(|(_, s)| s.first.eq_ignore_ascii_case(&who)) else { return };
    *done = true;
    for t in s3formats::sim::BODY_HAIR_TYPES {
        if let Some((key, _)) = crate::cas::parts_for(&cas, &sim, t, crate::simbody::OutfitKind::Everyday).first() {
            sim.outfit.body_hair.push(*key);
        }
    }
    info!("body hair test: {} given {} kinds", sim.first, sim.outfit.body_hair.len());
    commands.entity(e).insert((crate::simbody::Wearing(crate::simbody::OutfitKind::Swimwear), crate::aging::NeedsNewBody));
}

/// Shown on the Hair tab when the body hair's open: the region and place picked lit, the place's
/// styles ("none" first), the one worn lit; a click wears it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn body_hair_panel(
    mut commands: Commands,
    open: Option<ResMut<CasBodyHair>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    selected: Res<crate::cas::CasSelected>,
    scene: Option<Res<crate::cas::CasScene>>,
    (mut vis, mut buttons): (Query<&mut Visibility>, Query<&mut UiButton>),
    (clicks, cells): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &BodyHairCell), Changed<Interaction>>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let (Some(mut b), Some(mut ui), Some(scene)) = (open, ui, scene) else { return };
    let b = &mut *b;
    let on = selected.1 == CasTab::Hair && OPEN.load(Ordering::Relaxed) && !crate::caslook::modal_up();
    crate::livehud::set_visible(&mut vis, b.s.root, on);
    if !on {
        b.shown = None;
        return;
    }
    let k = selected.0.min(pending.members.len().saturating_sub(1));
    let Some(sim) = pending.members.get(k) else { return };
    // The region and the place picked (a region of one place, the arms', picks it).
    for (n, (button, _, parts)) in REGIONS.iter().enumerate() {
        if crate::livehud::pressed(&clicks, b.s.id(*button)) && b.region != n {
            b.region = n;
            b.part = parts[0].1;
        }
        for (part, t) in parts.iter() {
            if crate::livehud::pressed(&clicks, part.and_then(|p| b.s.id(p))) {
                b.region = n;
                b.part = *t;
            }
        }
    }
    for (i, c) in &cells {
        if *i == Interaction::Pressed {
            actions.write(crate::cas::CasActionRequest(CasAction::FacePart(b.part, c.0)));
        }
    }
    let worn = sim.outfit.body_hair.clone();
    let want = (k, b.region, b.part, worn.clone(), gu.is_some());
    if b.shown.as_ref() == Some(&want) {
        return;
    }
    b.shown = Some(want);
    for (n, (button, panel, parts)) in REGIONS.iter().enumerate() {
        crate::livehud::set_visible(&mut vis, panel.and_then(|p| b.s.id(p)), b.region == n);
        if let Some(mut x) = b.s.id(*button).and_then(|e| buttons.get_mut(e).ok())
            && x.selected != (b.region == n)
        {
            x.selected = b.region == n;
        }
        for (part, t) in parts.iter() {
            if let Some(mut x) = part.and_then(|p| b.s.id(p)).and_then(|e| buttons.get_mut(e).ok())
                && x.selected != (b.part == *t)
            {
                x.selected = b.part == *t;
            }
        }
    }
    if let Some(old) = b.cells.take() {
        commands.entity(old).try_despawn();
    }
    // The place's styles for the Sim, "none" first.
    let list = crate::cas::parts_for(&scene.cas, sim, b.part, crate::simbody::OutfitKind::Everyday);
    let (Some(grid), Some(g), Some(template)) = (b.s.id(GRID), ui.find("CASBodyHair", GRID).and_then(|w| w.grid), ui.layout("GenericCasItem").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
    b.cells = Some(holder);
    let cols = g.columns.max(1) as usize;
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    let here = list.iter().find(|(key, _)| worn.contains(key)).map(|(key, _)| *key);
    for n in 0..(list.len() + 1).min(cols * g.rows.max(1) as usize) {
        let (x, y) = (g.padding[0] + (n % cols) as f32 * step.x, g.padding[1] + (n / cols) as f32 * step.y);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let item = n.checked_sub(1).and_then(|i| list.get(i));
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = c.root else { continue };
        commands.entity(root).insert((BodyHairCell(n.checked_sub(1).unwrap_or(usize::MAX)), crate::icons::Tooltip(item.map_or_else(|| "None".to_string(), |i| i.1.clone()))));
        let chosen = match item {
            Some((key, _)) => here == Some(*key),
            None => here.is_none(),
        };
        if chosen {
            commands.entity(root).insert(crate::layout::Selected);
        }
        if let (Some(win), Some((key, _))) = (c.id(CELL_THUMBNAIL), item)
            && let Some(thumb) = gu.as_deref_mut().and_then(|g| g.icon(&mut images, &s3bake::gamedata::cas_thumb_name(key.2)))
        {
            crate::hudpanels::picture(&mut commands, win, thumb, Color::WHITE);
        }
    }
}
