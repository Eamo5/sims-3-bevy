//! Choosing a home in the game's own Play Flow (UI.dll's `PlayFlowController` over its layouts):
//! the instructions' speech bubble ("Let's Find a Home!", then Play), the play flow puck (the
//! camera's turn buttons, options), a bin of the homes for sale (the game's bin cells with each
//! lot's picture, wheel to scroll), a click on one (or on a lot on the map) putting up the game's
//! purchase dialog (`PlayFlowConfirmPurchaseLot`: the lot's picture, name, size and description,
//! furnished or unfurnished at their prices, the household's funds, Purchase or Cancel), and
//! Play to move in. The plain chooser stands aside.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::PlayMode;
use crate::home::{Moving, PendingHousehold, PurchaseTerms};
use crate::layout::{Spawned, UiAssets, UiButton, edit_windows};

pub struct PlayFlowPlugin;

impl Plugin for PlayFlowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(PlayMode::ChooseLot), |mut commands: Commands| commands.remove_resource::<Flow>())
            .add_systems(Update, play_flow.run_if(in_state(PlayMode::ChooseLot)))
            .add_systems(OnExit(PlayMode::ChooseLot), |mut commands: Commands| {
                commands.remove_resource::<Flow>();
                GAME_FLOW.store(false, Ordering::Relaxed);
            });
    }
}

/// Whether the game's play flow is up (the plain chooser stands aside).
pub static GAME_FLOW: AtomicBool = AtomicBool::new(false);

// `PlayFlowInstructions`'s windows.
const TITLE: u32 = 0x000a_a102;
const BODY: u32 = 0x000a_a103;
const ICON_ERROR: u32 = 0x000a_a307;
const NEXT: u32 = 0x000a_a203;
const EDIT_TOWN: u32 = 0x000a_a207;
const PLAY: u32 = 0x000a_a204;
const CANCEL: u32 = 0x000a_a201;
const ICON_HOUSEHOLD: u32 = 0x000a_a301;
const ICON_EMPTY_HOUSE: u32 = 0x000a_a302;
const ICON_EMPTY_LOT: u32 = 0x000a_a303;
/// The words' backgrounds: blue for a step, green to play, red for a problem.
const BACKGROUNDS: [u32; 3] = [0x000a_a401, 0x000a_a402, 0x000a_a403];
const UNIVERSITY_ICONS: u32 = 0x0e85_45b0;
// `PlayFlowPuck`'s.
const OPTIONS: u32 = 0x000a_c001;
const ORBIT: [u32; 2] = [0xf9aa_0000, 0xf9aa_0001];
const LOAD_MOVING: u32 = 0x53af_ad42;
// `PlayFlowBinPanel`'s, and its cells' (`PlayFlowBinGridItem`).
const BIN_GRID: u32 = 0x000a_e001;
const BIN_NOT_HERE: [u32; 3] = [0x000c_0401, 0x000a_e105, 0x000a_e106];
const CELL_THUMBNAIL: u32 = 0x000b_3300;
const CELL_PROPERTY: u32 = 0x000b_3303;
// `PlayFlowConfirmPurchaseLot`'s.
const BUY_TITLE: u32 = 0x000a_d101;
const BUY_ZOOM: u32 = 0x00bf_f401;
const BUY_THUMB: u32 = 0x00bf_f001;
const BUY_ADDRESS: u32 = 0x00bf_f301;
const BUY_SIZE: u32 = 0x00bf_f304;
const BUY_NAME: u32 = 0x00bf_f302;
const BUY_DESCRIPTION: u32 = 0x00bf_f303;
const BUY_FURNISHED_PRICE: u32 = 0x00bf_f202;
const BUY_UNFURNISHED_PRICE: u32 = 0x00bf_f201;
const BUY_SINGLE_PRICE: u32 = 0x00bf_f204;
const BUY_FURNISHED: u32 = 0x00bf_f102;
const BUY_UNFURNISHED: u32 = 0x00bf_f101;
const BUY_SINGLE_LABEL: u32 = 0x00bf_f105;
const BUY_FUNDS: u32 = 0x00bf_f203;
const BUY_CANCEL: u32 = 0x00bf_f104;
const BUY_PURCHASE: u32 = 0x00bf_f103;
/// The bin's columns in view.
const COLUMNS: usize = 4;

/// The flow's windows, the homes in the bin, how far it's scrolled, the purchase dialog up (its
/// lot, prices and choice) and the home bought.
#[derive(Resource)]
pub struct Flow {
    instructions: Spawned,
    puck: Spawned,
    bin: Spawned,
    lots: Vec<usize>,
    scroll: usize,
    cells: Option<Entity>,
    dialog: Option<Buying>,
    bought: Option<PurchaseTerms>,
    shown: Option<(usize, Option<usize>, Option<usize>)>,
    /// Whether any home is within their means (or the game's "none affordable").
    affordable: bool,
}

struct Buying {
    s: Spawned,
    lot: usize,
    prices: (i64, i64),
    furnished: bool,
}

/// A home's cell in the bin (its lot).
#[derive(Component)]
pub(crate) struct HomeCell(pub(crate) usize);

/// (For the tests: a home picked from the bin, as a player would: the first furnished house they
/// can afford, or else the first house they can afford unfurnished, or else the first home.)
pub(crate) fn test_pick(w: &mut World) {
    let funds = funds_of(w.get_resource::<PendingHousehold>(), w.get_resource::<Moving>());
    let mut q = w.query::<(Entity, &HomeCell)>();
    let cells: Vec<(Entity, usize)> = q.iter(w).map(|(e, c)| (e, c.0)).collect();
    let (Some(world), Some(catalog)) = (w.get_resource::<crate::loading::CurrentWorld>(), w.get_resource::<crate::loading::Catalog>()) else { return };
    let house = |l: usize| world.data.buildings.get(&l).is_some_and(|b| b.is_house());
    let price = |l: usize| crate::home::lot_prices(&world.data, l, catalog);
    let pick = cells
        .iter()
        .filter(|(_, l)| house(*l) && price(*l).1 <= funds)
        .chain(cells.iter().filter(|(_, l)| house(*l) && price(*l).0 <= funds))
        .chain(cells.iter())
        .map(|(e, _)| *e)
        .next();
    if let Some(mut i) = pick.and_then(|e| w.get_mut::<Interaction>(e)) {
        *i = Interaction::Pressed;
    }
}

/// (For the tests: a button pressed.)
#[derive(Clone, Copy)]
pub(crate) enum Press {
    Purchase,
    Play,
    Cancel,
}

pub(crate) fn press(w: &mut World, which: Press) {
    let e = w.get_resource::<Flow>().and_then(|f| match which {
        Press::Purchase => f.dialog.as_ref().and_then(|b| b.s.id(BUY_PURCHASE)),
        Press::Play => f.instructions.id(PLAY),
        Press::Cancel => f.instructions.id(CANCEL),
    });
    if let Some(mut i) = e.and_then(|e| w.get_mut::<Interaction>(e)) {
        *i = Interaction::Pressed;
    }
}

fn money(n: i64) -> String {
    format!("§{}", crate::lifetime::group(n))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play_flow(
    mut commands: Commands,
    flow: Option<ResMut<Flow>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    world: Res<crate::loading::CurrentWorld>,
    (data, catalog): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>),
    (pending, moving): (Option<Res<PendingHousehold>>, Option<Res<Moving>>),
    (town, story, dormant, household): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>, Option<Res<crate::interact::Household>>),
    (clicks, cells, plain): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &HomeCell), Changed<Interaction>>, Query<Entity, With<crate::home::LotChooserPanel>>),
    (mut cam, cameras, windows, over_ui): (Query<&mut crate::camera::SimsCamera>, Query<(&Camera, &GlobalTransform), With<Camera3d>>, Query<&Window, With<bevy::window::PrimaryWindow>>, Query<&Interaction>),
    (mouse, wheel): (Res<ButtonInput<MouseButton>>, Res<bevy::input::mouse::AccumulatedMouseScroll>),
    (mut chosen, mut options, settings, mut notes, mut app): (ResMut<crate::home::ChosenLot>, ResMut<crate::options::OptionsPanel>, Res<crate::options::Settings>, ResMut<crate::interact::Notifications>, ResMut<NextState<crate::AppState>>),
    mut buttons: Query<&mut UiButton>,
) {
    let Some(mut ui) = ui else { return };
    let Some(mut f) = flow else {
        let funds = funds_of(pending.as_deref(), moving.as_deref());
        if let Some(f) = open(&mut commands, &mut ui, &mut images, &mut fonts, &world, &catalog, funds, &town, &story, &dormant, moving.as_deref(), household.as_deref()) {
            for p in &plain {
                commands.entity(p).insert(Visibility::Hidden);
            }
            GAME_FLOW.store(true, Ordering::Relaxed);
            commands.insert_resource(f);
        }
        return;
    };
    let f = &mut *f;
    // (The plain chooser stays put away.)
    for p in &plain {
        commands.entity(p).insert(Visibility::Hidden);
    }
    let pressed = |s: &Spawned, id: u32| crate::livehud::pressed(&clicks, s.id(id));
    // The puck: the camera turned; options.
    for (n, id) in ORBIT.iter().enumerate() {
        if pressed(&f.puck, *id)
            && let Ok(mut c) = cam.single_mut()
        {
            c.yaw += if n == 0 { 0.6 } else { -0.6 };
        }
    }
    if pressed(&f.puck, OPTIONS) {
        crate::options::open_options(&mut commands, &mut options, &settings);
    }
    // A home chosen: from the bin, or a lot clicked on the map (not under the interface).
    let mut pick: Option<usize> = cells.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, c)| c.0);
    let ui_hovered = over_ui.iter().any(|i| *i != Interaction::None);
    if pick.is_none()
        && f.dialog.is_none()
        && mouse.just_pressed(MouseButton::Left)
        && !ui_hovered
        && let (Ok((camera, cam_tf)), Ok(window)) = (cameras.single(), windows.single())
        && let Some(p) = window.cursor_position().and_then(|c| camera.viewport_to_world(cam_tf, c).ok()).and_then(|ray| crate::hud::ground_hit(ray, &world))
        && let Some(l) = crate::rabbitholes::lot_at(&world.data.lots, p)
    {
        if f.lots.contains(&l) {
            pick = Some(l);
        } else if world.data.lots[l].is_residential() {
            notes.push("That home isn't for sale. (In Edit Town its household can be moved out.)".to_string());
        }
    }
    if let Some(l) = pick
        && f.dialog.is_none()
    {
        chosen.0 = Some(l);
        let lot = &world.data.lots[l];
        if let Ok(mut c) = cam.single_mut() {
            c.look_at(crate::home::lot_center(lot));
            c.distance = (lot.width.max(lot.depth) as f32 * 1.4).max(40.0);
            c.pitch = 0.8;
        }
        let funds = funds_of(pending.as_deref(), moving.as_deref());
        f.dialog = purchase_dialog(&mut commands, &mut ui, &mut images, &mut fonts, &world, &data, &catalog, l, funds);
    }
    // The purchase dialog: furnished or not; purchased, the home is theirs (then Play).
    if let Some(b) = f.dialog.as_mut() {
        let funds = funds_of(pending.as_deref(), moving.as_deref());
        if pressed(&b.s, BUY_FURNISHED) && b.prices.1 <= funds {
            b.furnished = true;
        }
        if pressed(&b.s, BUY_UNFURNISHED) {
            b.furnished = false;
        }
        for (id, on) in [(BUY_FURNISHED, b.furnished), (BUY_UNFURNISHED, !b.furnished)] {
            if let Some(mut bt) = b.s.id(id).and_then(|e| buttons.get_mut(e).ok())
                && bt.selected != on
            {
                bt.selected = on;
            }
        }
        let price = if b.furnished { b.prices.1 } else { b.prices.0 };
        if let Some(mut bt) = b.s.id(BUY_PURCHASE).and_then(|e| buttons.get_mut(e).ok())
            && bt.disabled != (price > funds)
        {
            bt.disabled = price > funds;
        }
        if pressed(&b.s, BUY_ZOOM)
            && let Ok(mut c) = cam.single_mut()
        {
            c.distance = (c.distance * 0.6).max(25.0);
        }
        let (cancel, purchase) = (pressed(&b.s, BUY_CANCEL), pressed(&b.s, BUY_PURCHASE) && price <= funds);
        if cancel || purchase {
            if purchase {
                let terms = PurchaseTerms { lot: b.lot, furnished: b.furnished, price };
                f.bought = Some(terms);
                commands.insert_resource(terms);
            }
            if let Some(r) = b.s.root {
                commands.entity(r).try_despawn();
            }
            f.dialog = None;
        }
    }
    // Play: they move in (as the plain chooser's Move In does it).
    if let Some(t) = f.bought
        && pressed(&f.instructions, PLAY)
    {
        chosen.0 = Some(t.lot);
        commands.insert_resource(crate::home::MoveInNow);
    }
    // Cancel: the home bought given up (back to choosing), or, before, back to Create a Sim
    // with the household (the game's flow back to its menu).
    if pressed(&f.instructions, CANCEL) {
        if f.bought.take().is_some() {
            commands.remove_resource::<PurchaseTerms>();
        } else if pending.is_some() && moving.is_none() {
            app.set(crate::AppState::CreateHousehold);
            return;
        }
    }
    // The bin scrolled by the wheel over it.
    let over_bin = f.bin.id(BIN_GRID).and_then(|e| over_ui.get(e).ok()).is_some_and(|i| *i != Interaction::None) || f.bin.root.and_then(|e| over_ui.get(e).ok()).is_some_and(|i| *i != Interaction::None);
    if over_bin && wheel.delta.y != 0.0 {
        let most = f.lots.len().saturating_sub(COLUMNS);
        f.scroll = (f.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
    }
    // Shown: the instructions (find a home; then play), the bin's homes in view.
    let want = (f.scroll, chosen.0, f.bought.map(|t| t.lot));
    if f.shown == Some(want) {
        return;
    }
    f.shown = Some(want);
    // (As the game's CAS flow: choose a lot; then play; or none can be afforded.)
    let step = if f.bought.is_some() {
        "CASPlay"
    } else if !f.affordable {
        "ErrorNoneAffordable"
    } else {
        "CASChooseLot"
    };
    let (title, body) = match step {
        "CASPlay" => ("Let's play!", "That lot looks great! You can continue browsing different lots or start playing right now."),
        "ErrorNoneAffordable" => ("No Homes Available", "This household can't afford any of the homes for sale here."),
        _ => ("Let's Find a Home!", "Click a house or an empty lot that's for sale to move in. Houses come furnished or unfurnished."),
    };
    let title = ui.localize(&format!("Ui/Caption/GameEntry/PlayFlow/Instructions/Title:{step}")).unwrap_or_else(|| title.into());
    let body = ui.localize(&format!("Ui/Caption/GameEntry/PlayFlow/Instructions/Body:{step}")).unwrap_or_else(|| body.into());
    for (id, t) in [(TITLE, title), (BODY, body)] {
        if let Some(x) = f.instructions.text(id) {
            commands.entity(x).insert(Text::new(t));
        }
    }
    let shade = match step {
        "CASPlay" => 1,
        "ErrorNoneAffordable" => 2,
        _ => 0,
    };
    let can_cancel = f.bought.is_some() || (pending.is_some() && moving.is_none());
    let mut show = |id: u32, on: bool| {
        for e in f.instructions.all_with(id) {
            commands.entity(e).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
    };
    show(PLAY, f.bought.is_some());
    show(CANCEL, can_cancel);
    show(ICON_HOUSEHOLD, false);
    show(ICON_EMPTY_HOUSE, step == "CASChooseLot");
    show(ICON_EMPTY_LOT, step == "CASChooseLot");
    show(ICON_ERROR, step == "ErrorNoneAffordable");
    for (n, id) in BACKGROUNDS.iter().enumerate() {
        // (The blue's id is also a stray hidden window's, made first: the one in the words is last.)
        if let Some(e) = f.instructions.all_with(*id).last() {
            commands.entity(*e).insert(if n == shade { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    let Some(grid) = f.bin.id(BIN_GRID) else { return };
    let (Some(g), Some(template)) = (ui.find("PlayFlowBinPanel", BIN_GRID).and_then(|w| w.grid), ui.layout("PlayFlowBinGridItem").cloned()) else { return };
    if let Some(old) = f.cells.take() {
        commands.entity(old).try_despawn();
    }
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
    f.cells = Some(holder);
    let step = g.cell[0] + g.cell_padding[0] + g.cell_padding[2];
    for (col, &l) in f.lots.iter().skip(f.scroll).take(COLUMNS).enumerate() {
        let x = g.padding[0] + col as f32 * step;
        let mut w = template.clone();
        w.area = [x, g.padding[1], x + g.cell[0], g.padding[1] + g.cell[1]];
        w.cls = "Button".into();
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        let Some(r) = c.root else { continue };
        let lot = &world.data.lots[l];
        let name = world.data.lot_names.get(l).cloned().unwrap_or_else(|| lot.internal_name.clone());
        let (bare, full) = crate::home::lot_prices(&world.data, l, &catalog);
        let tip = if bare == full { format!("{name}\n{}x{} · {}", lot.width, lot.depth, money(full)) } else { format!("{name}\n{}x{} · {} furnished, {} unfurnished", lot.width, lot.depth, money(full), money(bare)) };
        commands.entity(r).insert((HomeCell(l), crate::icons::Tooltip(tip)));
        if chosen.0 == Some(l) {
            commands.entity(r).queue(|mut e: EntityWorldMut| {
                if let Some(mut b) = e.get_mut::<UiButton>() {
                    b.selected = true;
                }
            });
        }
        if let Some(p) = c.id(CELL_PROPERTY) {
            commands.entity(p).insert(Visibility::Hidden);
        }
        if let (Some(win), Some(img)) = (c.id(CELL_THUMBNAIL), crate::objects::cpu_texture(&data.0, s3bake::lot_thumbnail_key(lot.id))) {
            crate::hudpanels::picture(&mut commands, win, images.add(img), Color::WHITE);
        }
    }
}

/// The household's money to buy with: a new household's starting funds, or a moving one's.
fn funds_of(pending: Option<&PendingHousehold>, moving: Option<&Moving>) -> i64 {
    match (moving, pending) {
        (Some(m), _) => m.0.funds,
        (None, Some(p)) => crate::home::starting_funds(&p.members),
        _ => 20_000,
    }
}

/// The flow put up: the instructions, puck and bin, and the homes for sale.
#[allow(clippy::too_many_arguments)]
fn open(
    commands: &mut Commands,
    ui: &mut UiAssets,
    images: &mut Assets<Image>,
    fonts: &mut Assets<Font>,
    world: &crate::loading::CurrentWorld,
    catalog: &crate::loading::Catalog,
    funds: i64,
    town: &Option<Res<crate::premade::TownPremades>>,
    story: &crate::story::TownStory,
    dormant: &crate::household::Dormant,
    moving: Option<&Moving>,
    household: Option<&crate::interact::Household>,
) -> Option<Flow> {
    for l in ["PlayFlowInstructions", "PlayFlowPuck", "PlayFlowBinPanel", "PlayFlowBinGridItem", "PlayFlowConfirmPurchaseLot"] {
        ui.layout(l)?;
    }
    let mut spawn = |name: &str| {
        let mut w = ui.layout(name)?.clone();
        w.flags |= s3bake::ui::WIN_VISIBLE;
        let s = ui.spawn_root(commands, images, fonts, &w);
        if let Some(r) = s.root {
            commands.entity(r).insert((DespawnOnExit(PlayMode::ChooseLot), GlobalZIndex(8)));
        }
        Some(s)
    };
    let (instructions, puck, bin) = (spawn("PlayFlowInstructions")?, spawn("PlayFlowPuck")?, spawn("PlayFlowBinPanel")?);
    for id in [ICON_ERROR, NEXT, EDIT_TOWN, UNIVERSITY_ICONS, PLAY] {
        for e in instructions.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for e in puck.all_with(LOAD_MOVING) {
        commands.entity(e).insert(Visibility::Hidden);
    }
    for id in BIN_NOT_HERE {
        for e in bin.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for (s, id, tip) in [(&puck, OPTIONS, "Options"), (&puck, ORBIT[0], "Rotate"), (&puck, ORBIT[1], "Rotate"), (&instructions, PLAY, "Play")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    if let Some(g) = bin.id(BIN_GRID) {
        commands.entity(g).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
    }
    // The homes for sale: houses (furnished first) and empty lots, not penthouses, not lived in.
    let lived_in = crate::home::lots_lived_in(world, town.as_deref(), story, dormant, moving.map(|m| (m, household)));
    let mut lots: Vec<(usize, String)> = world
        .data
        .lots
        .iter()
        .enumerate()
        .filter(|(i, l)| l.is_residential() && !lived_in.contains_key(i) && !world.data.buildings.get(i).is_some_and(|b| b.is_penthouse()))
        .map(|(i, l)| (i, world.data.lot_names.get(i).cloned().unwrap_or_else(|| l.internal_name.clone())))
        .collect();
    lots.sort_by_key(|(i, name)| {
        let b = world.data.buildings.get(i);
        (!b.is_some_and(|b| b.is_furnished()), !b.is_some_and(|b| b.is_house()), name.clone())
    });
    let lots: Vec<usize> = lots.into_iter().map(|l| l.0).collect();
    let affordable = lots.iter().any(|l| crate::home::lot_prices(&world.data, *l, catalog).0 <= funds);
    Some(Flow { instructions, puck, bin, lots, scroll: 0, cells: None, dialog: None, bought: None, shown: None, affordable })
}

/// The game's purchase dialog for a lot: its picture, name, size and description; furnished and
/// unfurnished at their prices (one price for an empty lot or an empty house); the funds.
#[allow(clippy::too_many_arguments)]
fn purchase_dialog(
    commands: &mut Commands,
    ui: &mut UiAssets,
    images: &mut Assets<Image>,
    fonts: &mut Assets<Font>,
    world: &crate::loading::CurrentWorld,
    data: &crate::baked::Baked,
    catalog: &crate::loading::Catalog,
    l: usize,
    funds: i64,
) -> Option<Buying> {
    let lot = world.data.lots.get(l)?;
    let prices = crate::home::lot_prices(&world.data, l, catalog);
    let single = prices.0 == prices.1;
    let mut w = ui.layout("PlayFlowConfirmPurchaseLot")?.clone();
    w.flags |= s3bake::ui::WIN_VISIBLE;
    let name = world.data.lot_names.get(l).cloned().unwrap_or_else(|| lot.internal_name.clone());
    let description = lot.string_keys.iter().find(|k| k.contains("HouseDesc") || k.contains("LotDesc")).and_then(|k| ui.localize(k)).unwrap_or_default();
    let texts: [(u32, String); 7] = [
        (BUY_NAME, name),
        (BUY_ADDRESS, String::new()),
        (BUY_SIZE, format!("{} x {}", lot.width, lot.depth)),
        (BUY_DESCRIPTION, description),
        (BUY_UNFURNISHED_PRICE, money(prices.0)),
        (BUY_FURNISHED_PRICE, money(prices.1)),
        (BUY_SINGLE_PRICE, money(prices.1)),
    ];
    for (id, t) in texts {
        edit_windows(&mut w, id, &mut |x| x.caption = t.clone());
    }
    edit_windows(&mut w, BUY_FUNDS, &mut |x| x.caption = money(funds));
    if let Some(t) = ui.localize("Ui/Caption/GameEntry/PlayFlow/Panel:PurchaseButton") {
        edit_windows(&mut w, BUY_PURCHASE, &mut |x| x.caption = t.clone());
    }
    let s = ui.spawn_root(commands, images, fonts, &w);
    let root = s.root?;
    commands.entity(root).insert((DespawnOnExit(PlayMode::ChooseLot), GlobalZIndex(20), crate::hud::BlocksWorld));
    // (One price, or the choice of two.)
    for (id, on) in [(BUY_SINGLE_PRICE, single), (BUY_SINGLE_LABEL, single), (BUY_FURNISHED, !single), (BUY_UNFURNISHED, !single), (BUY_FURNISHED_PRICE, !single), (BUY_UNFURNISHED_PRICE, !single)] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    if let (Some(win), Some(img)) = (s.id(BUY_THUMB), crate::objects::cpu_texture(&data.0, s3bake::lot_thumbnail_key(lot.id))) {
        crate::hudpanels::picture(commands, win, images.add(img), Color::WHITE);
    }
    for (id, tip) in [(BUY_ZOOM, "Zoom to Lot"), (BUY_CANCEL, "Cancel"), (BUY_PURCHASE, "Purchase")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    let _ = BUY_TITLE;
    // (Furnished when it can be afforded, as the game picks.)
    Some(Buying { s, lot: l, prices, furnished: single || prices.1 <= funds })
}
