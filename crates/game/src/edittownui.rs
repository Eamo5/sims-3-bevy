//! Edit Town in the game's own look (UI.dll's `EditTownController` over its layouts): the Edit
//! Town puck (Return to Live Mode, the camera's turn buttons, options, and its tools: select and
//! bulldoze), the library bin along the bottom (`EditTownLibraryPanel`: the household bin, and the
//! town's houses to put a copy of on an empty lot, in the game's bin cells), a lot or a household
//! picked showing the game's info panel (`EditTownInfoPanel`: the household's picture, name,
//! funds and description; the lot's picture, name, size and description; and the buttons for
//! what can be done: Evict, Switch To, Change Lot Type, Place), and the game's dialogs to confirm
//! (`EditTownConfirmEvict`, `AcceptCancelDialog`, `ChangeLotType`). Lots are picked on the map. The
//! plain panel stays underneath, out of sight; what's done is done by Edit Town's own actions.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::edittown::{Binned, EditAction, EditRequest, EditTown, EditTownPanel, Occupant};
use crate::layout::{Spawned, UiAssets, UiButton, UiCombo, edit_windows};
use s3bake::ui::{UiWindow, WIN_VISIBLE};

pub struct EditTownUiPlugin;

impl Plugin for EditTownUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, edit_town_ui.run_if(in_state(crate::PlayMode::Live))).add_systems(OnExit(crate::AppState::InGame), |mut commands: Commands| {
            commands.remove_resource::<GameTown>();
            GAME_EDIT_TOWN.store(false, Ordering::Relaxed);
        });
    }
}

/// Whether the game's Edit Town is up (the plain panel stays out of sight).
pub static GAME_EDIT_TOWN: AtomicBool = AtomicBool::new(false);

// `EditTownPuck`'s windows.
const RETURN_LIVE: u32 = 0x53af_ad40;
const RETURN_ENTRY: u32 = 0x53af_ad41;
const PLACEMENT_TEXT: u32 = 0x53af_ad45;
const OPTIONS: u32 = 0x53af_ad02;
const ORBIT: [u32; 2] = [0xf9aa_0006, 0xf9aa_0007];
const TOOL_MOVE: u32 = 0x53af_ad21;
const TOOL_EDIT: u32 = 0x53af_ad23;
const TOOL_BULLDOZE: u32 = 0x53af_ad22;
const TOOL_CHANGE: u32 = 0x53af_ad42;
const TOOL_DEFAULT: u32 = 0x53af_ad25;
const EXPORT: u32 = 0x36fc_9707;
// `EditTownLibraryPanel`'s.
const TAB_HOUSEHOLDS: u32 = 0x43a1_f902;
const TAB_HOUSES: u32 = 0x43a1_f903;
const TAB_COMMUNITY: u32 = 0x43a1_f904;
const FILTER: u32 = 0x43a1_f905;
const UNHIDE: u32 = 0x43a1_f906;
const BIN_GRID: u32 = 0x43a1_f901;
const CELL_THUMBNAIL: u32 = 0x000b_3300;
const CELL_PROPERTY: u32 = 0x000b_3303;
// `EditTownInfoPanel`'s.
const INFO_MAIN: u32 = 0x36fc_9700;
const INFO_CLOSE: u32 = 0x36fc_9740;
const INFO_BUTTONS: u32 = 0x36fc_9710;
const PLACE_COPY: u32 = 0x36fc_9701;
const EVICT: u32 = 0x36fc_9702;
const CHANGE_TYPE: u32 = 0x36fc_9709;
const SWITCH_TO: u32 = 0x36fc_970f;
/// The panel's other buttons (split, edit, merge, save, export, delete, hide, lock, place move,
/// build/buy, copy to clipboard, houseboats): not here.
const NOT_HERE: [u32; 10] = [0x36fc_9704, 0x36fc_9707, 0x36fc_970c, 0x36fc_9708, 0x36fc_970e, 0x36fc_970a, 0x36fc_9705, 0x36fc_9711, 0x36fc_970d, 0x36fc_970b];
/// Split the household being played, merge one played before into it (`movedialog`).
const SPLIT: u32 = 0x36fc_9703;
const MERGE: u32 = 0x36fc_9706;
const HOUSEHOLD_PANEL: u32 = 0x36fc_9720;
const HOUSEHOLD_METER: u32 = 0x36fc_9721;
const HOUSEHOLD_FUNDS: u32 = 0x36fc_9722;
const HOUSEHOLD_THUMB: u32 = 0x36fc_9723;
const HOUSEHOLD_NAME: u32 = 0x36fc_9724;
const HOUSEHOLD_DESC: u32 = 0x36fc_9725;
const HOUSEHOLD_DESC_BG: u32 = 0x36fc_9726;
const LOT_PANEL: u32 = 0x36fc_9730;
const LOT_THUMB: u32 = 0x36fc_9731;
const LOT_ADDRESS: u32 = 0x36fc_9732;
const LOT_NAME: u32 = 0x36fc_9733;
const LOT_DESC: u32 = 0x36fc_9734;
const LOT_SIZE: u32 = 0x36fc_9735;
const LOT_DESC_BG: u32 = 0x36fc_9736;
// `EditTownConfirmEvict`'s.
const EVICT_HOUSEHOLD: u32 = 0xe809_f004;
const EVICT_LOT: u32 = 0xe809_f005;
const EVICT_SELL: u32 = 0xe809_f001;
const EVICT_TAKE: u32 = 0xe809_f002;
const EVICT_CANCEL: u32 = 0xe809_f003;
// `AcceptCancelDialog`'s and `ChangeLotType`'s.
const ASK_TEXT: u32 = 1;
const ASK_ACCEPT: u32 = 2;
const ASK_CANCEL: u32 = 3;
const TYPE_RESIDENTIAL: u32 = 2;
const TYPE_COMMUNITY: u32 = 3;
const TYPE_COMBO: u32 = 4;
const TYPE_ACCEPT: u32 = 0x20;
const TYPE_CANCEL: u32 = 0x21;

/// What's picked: a lot on the map, a household in the bin, or one of the town's houses to copy.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Focus {
    Lot(usize),
    Bin(Binned),
    House(usize),
}

/// The puck's tool: picking lots, bulldozing them, or putting down what's picked in the bin.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Tool {
    Select,
    Bulldoze,
    Place(Focus),
}

/// What a dialog up asks to confirm.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Confirm {
    Evict(Occupant),
    Switch(Occupant),
    Bulldoze(usize),
    Retype(usize),
}

/// The game's Edit Town while it's open.
#[derive(Resource)]
struct GameTown {
    puck: Spawned,
    library: Spawned,
    info: Option<Spawned>,
    dialog: Option<(Spawned, Confirm)>,
    households: bool,
    focus: Option<Focus>,
    tool: Tool,
    scroll: usize,
    cells: Option<Entity>,
    /// What's shown (the focus, tool, tab, scroll and the town as it was), to show afresh.
    shown: Option<(Option<Focus>, Tool, bool, usize, u32)>,
    /// The town's changes seen (evictions, moves, lot types), counted.
    changes: u32,
    /// The lot Edit Town has picked, last seen (picked by a test, or on the map).
    picked: Option<usize>,
    /// (For the tests: a window pressed by its id, or the bin's cell in a column. Pressing for
    /// real can't be faked: with the pointer away, the interface lets go of it at once.)
    test_press: Option<u32>,
    test_cell: Option<usize>,
    test_lot: Option<usize>,
}

/// A cell in the bin: what it stands for, and its column in view.
#[derive(Component)]
struct BinCell(Focus, usize);

/// A household's particulars for the info panel.
struct HouseholdInfo {
    name: String,
    funds: i64,
    bio: String,
    picture: Option<Handle<Image>>,
}

fn money(n: i64) -> String {
    format!("§{}", crate::lifetime::group(n.max(0)))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn edit_town_ui(
    mut commands: Commands,
    game: Option<ResMut<GameTown>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    edit: Res<EditTown>,
    world: Res<crate::loading::CurrentWorld>,
    (town, story, dormant, household, data): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>, Option<Res<crate::interact::Household>>, Res<crate::baked::Baked>),
    (clicks, cells, all_cells, mut plain, mut buttons, mut combos): (
        Query<(Entity, &Interaction), Changed<Interaction>>,
        Query<(&Interaction, &BinCell), Changed<Interaction>>,
        Query<&BinCell>,
        Query<&mut Visibility, With<EditTownPanel>>,
        Query<&mut UiButton>,
        Query<&mut UiCombo>,
    ),
    (mut cam, cameras, windows, over_ui, hovered): (
        Query<&mut crate::camera::SimsCamera>,
        Query<(&Camera, &GlobalTransform), With<Camera3d>>,
        Query<&Window, With<bevy::window::PrimaryWindow>>,
        Res<crate::hud::PointerOverUi>,
        Query<&Interaction>,
    ),
    (mouse, wheel): (Res<ButtonInput<MouseButton>>, Res<bevy::input::mouse::AccumulatedMouseScroll>),
    (mut requests, mut options, settings, mut notes, mut portraits, members, mut play): (
        MessageWriter<EditRequest>,
        ResMut<crate::options::OptionsPanel>,
        Res<crate::options::Settings>,
        ResMut<crate::interact::Notifications>,
        ResMut<crate::portraits::Portraits>,
        Query<(Entity, &crate::sim::Sim), With<crate::sim::HouseholdMember>>,
        MessageWriter<crate::sound::PlaySound>,
    ),
) {
    let Some(mut ui) = ui else { return };
    // Closed: the game's Edit Town goes.
    if !edit.is_open() {
        if let Some(g) = game {
            for s in [Some(&g.puck), Some(&g.library), g.info.as_ref(), g.dialog.as_ref().map(|d| &d.0)].into_iter().flatten() {
                if let Some(r) = s.root {
                    commands.entity(r).try_despawn();
                }
            }
            commands.remove_resource::<GameTown>();
            GAME_EDIT_TOWN.store(false, Ordering::Relaxed);
        }
        return;
    }
    let Some(mut g) = game else {
        if let Some(g) = open(&mut commands, &mut ui, &mut images, &mut fonts) {
            GAME_EDIT_TOWN.store(true, Ordering::Relaxed);
            commands.insert_resource(g);
        }
        return;
    };
    let g = &mut *g;
    for mut v in &mut plain {
        if *v != Visibility::Hidden {
            *v = Visibility::Hidden;
        }
    }
    let town_ref = town.as_ref().map(|t| &*t.0);
    let ids: Vec<u64> = members.iter().map(|(_, s)| s.id).collect();
    let occupied = crate::edittown::occupants(&world.data, town_ref, &story, &dormant.0, household.as_ref().map(|h| h.lot_index));
    if story.is_changed() || dormant.is_changed() || world.is_changed() {
        g.changes = g.changes.wrapping_add(1);
    }
    if edit.selected() != g.picked {
        g.picked = edit.selected();
        if let Some(l) = g.picked {
            g.focus = Some(Focus::Lot(l));
        }
    }
    let forced = g.test_press.take();
    let pressed = |s: &Spawned, id: u32| crate::livehud::pressed(&clicks, s.id(id)) || (forced == Some(id) && s.id(id).is_some());
    let lot_name = |i: usize| world.data.lot_names.get(i).cloned().unwrap_or_else(|| world.data.lots[i].internal_name.clone());
    // The puck: back to live mode, the camera turned, options, the tools.
    if pressed(&g.puck, RETURN_LIVE) {
        requests.write(EditRequest(EditAction::Done));
        return;
    }
    for (n, id) in ORBIT.iter().enumerate() {
        if pressed(&g.puck, *id)
            && let Ok(mut c) = cam.single_mut()
        {
            c.yaw += if n == 0 { 0.6 } else { -0.6 };
        }
    }
    if pressed(&g.puck, OPTIONS) {
        crate::options::open_options(&mut commands, &mut options, &settings);
    }
    if pressed(&g.puck, TOOL_DEFAULT) {
        g.tool = Tool::Select;
    }
    if pressed(&g.puck, TOOL_BULLDOZE) {
        g.tool = if g.tool == Tool::Bulldoze { Tool::Select } else { Tool::Bulldoze };
    }
    // The bin's tabs, its cells, the wheel over it.
    if pressed(&g.library, TAB_HOUSEHOLDS) {
        g.households = true;
        g.scroll = 0;
    }
    if pressed(&g.library, TAB_HOUSES) {
        g.households = false;
        g.scroll = 0;
    }
    let test_cell = g.test_cell.take().and_then(|n| all_cells.iter().find(|c| c.1 == n)).map(|c| c.0);
    if let Some(f) = cells.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, c)| c.0).or(test_cell) {
        g.focus = Some(f);
        g.tool = Tool::Select;
    }
    let over_bin = g.library.root.and_then(|e| hovered.get(e).ok()).is_some_and(|i| *i != Interaction::None) || g.library.id(BIN_GRID).and_then(|e| hovered.get(e).ok()).is_some_and(|i| *i != Interaction::None);
    let (bin, houses) = lists(&world.data, town_ref, &story, &dormant.0, &ids);
    let count = if g.households { bin.len() } else { houses.len() };
    let columns = columns(&ui);
    if over_bin && wheel.delta.y != 0.0 {
        let most = count.saturating_sub(columns);
        g.scroll = (g.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
    }
    // A lot clicked on the map: picked, bulldozed, or what's picked in the bin put down on it.
    if mouse.just_pressed(MouseButton::Right) && g.tool != Tool::Select {
        g.tool = Tool::Select;
    }
    let clicked = || {
        let (Ok((camera, cam_tf)), Ok(window)) = (cameras.single(), windows.single()) else { return None };
        let p = window.cursor_position().and_then(|c| camera.viewport_to_world(cam_tf, c).ok()).and_then(|ray| crate::hud::ground_hit(ray, &world))?;
        crate::rabbitholes::lot_at(&world.data.lots, p)
    };
    let test_lot = g.test_lot.take();
    if g.dialog.is_none()
        && let Some(l) = test_lot.or_else(|| if mouse.just_pressed(MouseButton::Left) && !over_ui.0 { clicked() } else { None })
    {
        let lot = &world.data.lots[l];
        let house = world.data.buildings.get(&l).filter(|b| b.is_house());
        let penthouse = world.data.buildings.get(&l).is_some_and(|b| b.is_penthouse());
        match g.tool {
            Tool::Select => {
                g.focus = Some(Focus::Lot(l));
                requests.write(EditRequest(EditAction::Select(l)));
                play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
            }
            Tool::Bulldoze => match occupied.get(&l) {
                Some(_) => notes.push(format!("Someone lives on {}: evict them before bulldozing it.", lot_name(l))),
                None if house.is_none() || penthouse || !lot.is_residential() => notes.push(format!("There's no house on {} to bulldoze.", lot_name(l))),
                None => {
                    let prompt = bulldoze_prompt(&ui, &lot_name(l));
                    g.dialog = ask(&mut commands, &mut ui, &mut images, &mut fonts, &prompt).map(|s| (s, Confirm::Bulldoze(l)));
                }
            },
            Tool::Place(Focus::Bin(b)) => {
                if occupied.contains_key(&l) {
                    notes.push(format!("Someone already lives on {}.", lot_name(l)));
                } else if !lot.is_residential() || penthouse {
                    notes.push(format!("{} isn't a home: a household can only move onto a residential lot.", lot_name(l)));
                } else {
                    requests.write(EditRequest(EditAction::MoveIn(b, l)));
                    g.focus = Some(Focus::Lot(l));
                    g.tool = Tool::Select;
                    play.write(crate::sound::PlaySound::ui("ui_build_door_plop"));
                }
            }
            Tool::Place(Focus::House(j)) => {
                let from = &world.data.lots[j];
                if occupied.contains_key(&l) || house.is_some() || !lot.is_residential() || penthouse {
                    notes.push(format!("{} isn't an empty residential lot.", lot_name(l)));
                } else if (from.width, from.depth) != (lot.width, lot.depth) {
                    notes.push(format!("{} needs a {} x {} lot ({} is {} x {}).", lot_name(j), from.width, from.depth, lot_name(l), lot.width, lot.depth));
                } else {
                    requests.write(EditRequest(EditAction::Select(l)));
                    requests.write(EditRequest(EditAction::Place(l, j)));
                    g.focus = Some(Focus::Lot(l));
                    g.tool = Tool::Select;
                }
            }
            Tool::Place(Focus::Lot(_)) => g.tool = Tool::Select,
        }
    }
    // The info panel's buttons.
    if let Some(info) = g.info.clone() {
        let info = &info;
        let focus = g.focus;
        let occupant = match focus {
            Some(Focus::Lot(l)) => occupied.get(&l).copied(),
            _ => None,
        };
        if pressed(info, INFO_CLOSE) {
            g.focus = None;
        } else if pressed(info, EVICT)
            && let Some(o) = occupant
        {
            let name = household_info(o, household.as_deref(), &members, &dormant.0, town_ref, &story, &mut portraits, &mut images).name;
            g.dialog = evict_dialog(&mut commands, &mut ui, &mut images, &mut fonts, &name, &lot_name_of(focus, &world.data), matches!(o, Occupant::Played(_))).map(|s| (s, Confirm::Evict(o)));
        } else if pressed(info, SWITCH_TO)
            && let Some(o) = occupant
        {
            let prompt = ui.localize("Ui/Caption/GameEntry/EditTown:ChangeActiveHousehold").unwrap_or_else(|| "Are you sure you want to change the active household?".into());
            g.dialog = ask(&mut commands, &mut ui, &mut images, &mut fonts, &prompt).map(|s| (s, Confirm::Switch(o)));
        } else if pressed(info, CHANGE_TYPE)
            && let Some(Focus::Lot(l)) = focus
        {
            g.dialog = type_dialog(&mut commands, &mut ui, &mut images, &mut fonts, &world.data.lots[l]).map(|s| (s, Confirm::Retype(l)));
        } else if pressed(info, PLACE_COPY)
            && let Some(f @ (Focus::Bin(_) | Focus::House(_))) = focus
        {
            g.tool = Tool::Place(f);
        } else if pressed(info, SPLIT) {
            commands.insert_resource(crate::movedialog::OpenMove(crate::movedialog::MoveKind::Split));
        } else if pressed(info, MERGE)
            && let Some(kind) = match (focus, occupant) {
                (_, Some(Occupant::Played(i))) | (Some(Focus::Bin(Binned::Played(i))), _) => Some(crate::movedialog::MoveKind::Merge(i)),
                (_, Some(Occupant::Town(id))) | (Some(Focus::Bin(Binned::Town(id))), _) => Some(crate::movedialog::MoveKind::MergeTown(id)),
                _ => None,
            }
        {
            commands.insert_resource(crate::movedialog::OpenMove(kind));
        }
    }
    // A dialog up: confirmed, or not.
    if let Some((s, confirm)) = g.dialog.as_ref() {
        let (s, confirm) = (s.clone(), *confirm);
        let mut done = None;
        match confirm {
            Confirm::Evict(o) => {
                if pressed(&s, EVICT_SELL) {
                    done = Some(Some(EditAction::Evict(o, false)));
                } else if pressed(&s, EVICT_TAKE) {
                    done = Some(Some(EditAction::Evict(o, true)));
                } else if pressed(&s, EVICT_CANCEL) {
                    done = Some(None);
                }
            }
            Confirm::Switch(o) => {
                if pressed(&s, ASK_ACCEPT) {
                    let to = match o {
                        Occupant::Played(i) => Some(EditAction::Play(Occupant::Played(i))),
                        Occupant::Town(id) => Some(EditAction::Play(Occupant::Town(id))),
                        Occupant::Active => None,
                    };
                    done = Some(to);
                } else if pressed(&s, ASK_CANCEL) {
                    done = Some(None);
                }
            }
            Confirm::Bulldoze(l) => {
                if pressed(&s, ASK_ACCEPT) {
                    done = Some(Some(EditAction::Bulldoze(l)));
                } else if pressed(&s, ASK_CANCEL) {
                    done = Some(None);
                }
            }
            Confirm::Retype(l) => {
                // (Residential, or community and its kind from the list.)
                let residential = s.id(TYPE_RESIDENTIAL).and_then(|e| buttons.get(e).ok()).is_some_and(|b| b.selected);
                for (id, on) in [(TYPE_RESIDENTIAL, true), (TYPE_COMMUNITY, false)] {
                    if pressed(&s, id) {
                        for (other, sel) in [(TYPE_RESIDENTIAL, on), (TYPE_COMMUNITY, !on)] {
                            if let Some(mut b) = s.id(other).and_then(|e| buttons.get_mut(e).ok()) {
                                b.selected = sel;
                            }
                        }
                    }
                }
                if pressed(&s, TYPE_ACCEPT) {
                    let kind = s.id(TYPE_COMBO).and_then(|e| combos.get(e).ok()).map_or(0, |c| c.selected);
                    done = Some(Some(EditAction::Retype(l, if residential { None } else { Some(kind) })));
                } else if pressed(&s, TYPE_CANCEL) {
                    done = Some(None);
                }
                if let Some(mut c) = s.id(TYPE_COMBO).and_then(|e| combos.get_mut(e).ok())
                    && c.items.is_empty()
                {
                    c.items = crate::story::COMMUNITY_TYPES.iter().map(|(n, _)| n.to_string()).collect();
                    let now = &world.data.lots[l];
                    c.selected = crate::story::COMMUNITY_TYPES.iter().position(|(_, w)| !now.is_residential() && now.internal_name.to_ascii_lowercase().contains(w)).unwrap_or(0);
                }
            }
        }
        if let Some(action) = done {
            if let Some(r) = s.root {
                commands.entity(r).try_despawn();
            }
            g.dialog = None;
            if let Some(a) = action {
                requests.write(EditRequest(a));
                if matches!(a, EditAction::Evict(..)) {
                    g.changes = g.changes.wrapping_add(1);
                }
            }
        }
    }
    // The tools shown chosen, and what the pointer does now.
    for (id, on) in [(TOOL_DEFAULT, g.tool == Tool::Select), (TOOL_BULLDOZE, g.tool == Tool::Bulldoze)] {
        if let Some(mut b) = g.puck.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != on
        {
            b.selected = on;
        }
    }
    for (id, on) in [(TAB_HOUSEHOLDS, g.households), (TAB_HOUSES, !g.households)] {
        if let Some(mut b) = g.library.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != on
        {
            b.selected = on;
        }
    }
    // Shown afresh when anything changes: the info panel for what's picked, the bin's cells,
    // the puck's word on the tool.
    let want = (g.focus, g.tool, g.households, g.scroll, g.changes);
    if g.shown == Some(want) {
        return;
    }
    g.shown = Some(want);
    if let Some(t) = g.puck.text(PLACEMENT_TEXT) {
        let words = match g.tool {
            Tool::Select => String::new(),
            Tool::Bulldoze => "Click a house no one lives in to bulldoze it. (Right-click to stop.)".into(),
            Tool::Place(Focus::Bin(b)) => format!("Click an empty home for the {} household. (Right-click to stop.)", household_info(binned(b), household.as_deref(), &members, &dormant.0, town_ref, &story, &mut portraits, &mut images).name),
            Tool::Place(Focus::House(j)) => format!("Click an empty {} x {} lot for {}. (Right-click to stop.)", world.data.lots[j].width, world.data.lots[j].depth, lot_name(j)),
            Tool::Place(_) => String::new(),
        };
        commands.entity(t).insert(Text::new(words.clone()));
        if let Some(e) = g.puck.id(PLACEMENT_TEXT) {
            commands.entity(e).insert(if words.is_empty() { Visibility::Hidden } else { Visibility::Inherited });
        }
    }
    // The info panel.
    if let Some(r) = g.info.take().and_then(|s| s.root) {
        commands.entity(r).try_despawn();
    }
    if let Some(f) = g.focus {
        let hh = match f {
            Focus::Lot(l) => occupied.get(&l).map(|o| (*o, household_info(*o, household.as_deref(), &members, &dormant.0, town_ref, &story, &mut portraits, &mut images))),
            Focus::Bin(b) => Some((binned(b), household_info(binned(b), household.as_deref(), &members, &dormant.0, town_ref, &story, &mut portraits, &mut images))),
            Focus::House(_) => None,
        };
        let lot = match f {
            Focus::Lot(l) | Focus::House(l) => Some(l),
            Focus::Bin(_) => None,
        };
        let rabbit_hole = lot.is_some_and(|l| world.data.buildings.get(&l).is_some_and(|b| b.objects.iter().any(|o| o.script.contains("RabbitHole"))));
        let penthouse = lot.is_some_and(|l| world.data.buildings.get(&l).is_some_and(|b| b.is_penthouse()));
        let buttons_shown: Vec<u32> = match (f, hh.as_ref().map(|h| h.0)) {
            (Focus::Lot(_), Some(Occupant::Active)) => vec![SPLIT],
            (Focus::Lot(_), Some(_)) => vec![EVICT, MERGE, SWITCH_TO],
            (Focus::Lot(_), None) if penthouse => vec![],
            (Focus::Lot(_), None) => vec![CHANGE_TYPE],
            (Focus::Bin(_), _) => vec![PLACE_COPY, MERGE],
            (Focus::House(_), _) => vec![PLACE_COPY],
        };
        g.info = info_panel(&mut commands, &mut ui, &mut images, &mut fonts, &world.data, &data, &story, hh.as_ref().map(|h| &h.1), lot, &buttons_shown);
        if let Some(info) = g.info.as_ref() {
            if rabbit_hole && let Some(e) = info.id(CHANGE_TYPE) {
                commands.entity(e).insert(crate::icons::Tooltip("A career's building stands here: its lot type can't change.".into()));
                commands.entity(e).queue(|mut e: EntityWorldMut| {
                    if let Some(mut b) = e.get_mut::<UiButton>() {
                        b.disabled = true;
                    }
                });
            }
            for (id, tip) in [(EVICT, "Evict"), (SWITCH_TO, "Switch To This Household"), (PLACE_COPY, "Place"), (INFO_CLOSE, "Close"), (SPLIT, "Split Household"), (MERGE, "Merge With the Household Being Played")] {
                if let Some(e) = info.id(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
                }
            }
            if !rabbit_hole && let Some(e) = info.id(CHANGE_TYPE) {
                let tip = ui.localize("Ui/Tooltip/GameEntry/EditTown:PanelButtonTypeChange").unwrap_or_else(|| "Change Lot Type".into());
                commands.entity(e).insert(crate::icons::Tooltip(tip));
            }
        }
    }
    // The bin's cells.
    let Some(grid) = g.library.id(BIN_GRID) else { return };
    let (Some(gp), Some(template)) = (ui.find("EditTownLibraryPanel", BIN_GRID).and_then(|w| w.grid), ui.layout("PlayFlowBinGridItem").cloned()) else { return };
    if let Some(old) = g.cells.take() {
        commands.entity(old).try_despawn();
    }
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
    g.cells = Some(holder);
    let step = gp.cell[0] + gp.cell_padding[0] + gp.cell_padding[2];
    let items: Vec<Focus> = if g.households { bin.iter().map(|b| Focus::Bin(*b)).collect() } else { houses.iter().map(|l| Focus::House(*l)).collect() };
    for (col, item) in items.iter().skip(g.scroll).take(columns).enumerate() {
        let x = gp.padding[0] + col as f32 * step;
        let mut w = template.clone();
        w.area = [x, gp.padding[1], x + gp.cell[0], gp.padding[1] + gp.cell[1]];
        w.cls = "Button".into();
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        let Some(r) = c.root else { continue };
        let (tip, picture) = match *item {
            Focus::Bin(b) => {
                let o = binned(b);
                let h = household_info(o, household.as_deref(), &members, &dormant.0, town_ref, &story, &mut portraits, &mut images);
                let w = crate::edittown::who(o, None, &[], &dormant.0, town_ref, &story);
                (format!("{}\n{}", w.name, w.detail), h.picture)
            }
            Focus::House(l) | Focus::Lot(l) => {
                let pic = picture_lot(&world.data, &story, l).and_then(|k| crate::objects::cpu_texture(&data.0, s3bake::lot_thumbnail_key(world.data.lots[k].id))).map(|img| images.add(img));
                (format!("{}\n{}", lot_name(l), crate::edittown::lot_kind(&world.data, l)), pic)
            }
        };
        commands.entity(r).insert((BinCell(*item, col), crate::icons::Tooltip(tip)));
        if g.focus == Some(*item) {
            commands.entity(r).queue(|mut e: EntityWorldMut| {
                if let Some(mut b) = e.get_mut::<UiButton>() {
                    b.selected = true;
                }
            });
        }
        if let Some(p) = c.id(CELL_PROPERTY) {
            commands.entity(p).insert(Visibility::Hidden);
        }
        if let (Some(win), Some(h)) = (c.id(CELL_THUMBNAIL), picture) {
            crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
        }
    }
}

/// (For the tests: a window of the game's Edit Town pressed, by its id.)
pub(crate) fn test_press(w: &mut World, id: u32) {
    let Some(mut g) = w.get_resource_mut::<GameTown>() else { return };
    if [Some(&g.puck), Some(&g.library), g.info.as_ref(), g.dialog.as_ref().map(|d| &d.0)].into_iter().flatten().any(|s| s.id(id).is_some()) {
        g.test_press = Some(id);
    } else {
        warn!("edit town test: no window {id:#x} to press");
    }
}

/// (For the tests: a lot clicked on the map.)
pub(crate) fn test_lot(w: &mut World, lot: usize) {
    if let Some(mut g) = w.get_resource_mut::<GameTown>() {
        g.test_lot = Some(lot);
    }
}

/// (For the tests: the bin's `n`th cell in view pressed.)
pub(crate) fn test_cell(w: &mut World, n: usize) {
    if let Some(mut g) = w.get_resource_mut::<GameTown>() {
        g.test_cell = Some(n);
    }
}

fn binned(b: Binned) -> Occupant {
    match b {
        Binned::Played(i) => Occupant::Played(i),
        Binned::Town(id) => Occupant::Town(id),
    }
}

fn lot_name_of(f: Option<Focus>, world: &crate::loading::WorldInfo) -> String {
    match f {
        Some(Focus::Lot(l) | Focus::House(l)) => world.lot_names.get(l).cloned().unwrap_or_else(|| world.lots[l].internal_name.clone()),
        _ => String::new(),
    }
}

fn bulldoze_prompt(ui: &UiAssets, lot: &str) -> String {
    ui.localize("Ui/Caption/GameEntry/EditTown:BulldozeConfirm").map(|t| t.replace("{0.String}", lot)).unwrap_or_else(|| format!("Are you sure you want to bulldoze {lot}?"))
}

/// The bin's columns in view.
fn columns(ui: &UiAssets) -> usize {
    let Some(w) = ui.find("EditTownLibraryPanel", BIN_GRID) else { return 4 };
    let Some(g) = w.grid else { return 4 };
    let step = g.cell[0] + g.cell_padding[0] + g.cell_padding[2];
    if g.columns > 0 {
        return g.columns as usize;
    }
    (((w.area[2] - w.area[0]) - g.padding[0] - g.padding[2]) / step.max(1.0)).floor().max(1.0) as usize
}

/// The household bin, and the town's houses to copy (by name).
fn lists(world: &crate::loading::WorldInfo, town: Option<&s3bake::PremadesBaked>, story: &crate::story::TownStory, dormant: &[crate::save::SaveGame], playing: &[u64]) -> (Vec<Binned>, Vec<usize>) {
    let bin = crate::edittown::bin(town, story, dormant, playing);
    let mut houses: Vec<usize> = (0..world.lots.len()).filter(|i| world.lots[*i].is_residential() && world.buildings.get(i).is_some_and(|b| b.is_house() && !b.is_penthouse())).collect();
    houses.sort_by_key(|i| world.lot_names.get(*i).cloned().unwrap_or_default());
    (bin, houses)
}

/// A household's name, money, description and picture (a town family's portrait, or the
/// picture of one of the household being played).
#[allow(clippy::too_many_arguments)]
fn household_info(
    o: Occupant,
    household: Option<&crate::interact::Household>,
    members: &Query<(Entity, &crate::sim::Sim), With<crate::sim::HouseholdMember>>,
    dormant: &[crate::save::SaveGame],
    town: Option<&s3bake::PremadesBaked>,
    story: &crate::story::TownStory,
    portraits: &mut crate::portraits::Portraits,
    images: &mut Assets<Image>,
) -> HouseholdInfo {
    let family_picture = |h: &s3bake::HouseholdBaked, images: &mut Assets<Image>| {
        h.portrait.and_then(|k| {
            let bytes = std::fs::read(s3bake::default_root().tex_path(k)).ok()?;
            crate::objects::dds_image(&bytes, true).map(|img| images.add(img))
        })
    };
    match o {
        Occupant::Active => {
            let names: Vec<String> = members.iter().map(|(_, s)| s.full_name()).collect();
            let first = members.iter().next().map(|(e, _)| e);
            HouseholdInfo {
                name: household.map_or(String::new(), |h| h.name.clone()),
                funds: household.map_or(0, |h| h.funds),
                bio: names.join(", "),
                picture: first.map(|e| portraits.portrait(images, e)),
            }
        }
        Occupant::Played(i) => {
            let Some(d) = dormant.get(i) else { return HouseholdInfo { name: String::new(), funds: 0, bio: String::new(), picture: None } };
            let ids: Vec<u64> = d.sims.iter().filter(|s| s.member).map(|s| s.id).collect();
            let names: Vec<String> = d.sims.iter().filter(|s| s.member).map(|s| format!("{} {}", s.first, s.last)).collect();
            // (A town family played: their family portrait still.)
            let picture = town.and_then(|t| t.households.iter().find(|h| h.members.iter().any(|m| ids.contains(&m.id)))).and_then(|h| family_picture(h, images));
            HouseholdInfo { name: d.household.clone(), funds: d.funds, bio: names.join(", "), picture }
        }
        Occupant::Town(id) => match town.and_then(|t| t.households.iter().find(|h| h.id == id)) {
            Some(h) => {
                let names: Vec<String> = h.members.iter().filter(|m| !m.first_name.is_empty()).filter_map(|m| story.apply(crate::premade::to_sim(m))).map(|s| s.full_name()).collect();
                let bio = if h.bio.is_empty() { names.join(", ") } else { h.bio.clone() };
                HouseholdInfo { name: h.name.clone(), funds: h.funds, bio, picture: family_picture(h, images) }
            }
            None => HouseholdInfo { name: String::new(), funds: 0, bio: String::new(), picture: None },
        },
    }
}

/// The puck and the bin put up (the info panel comes when something's picked).
fn open(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>) -> Option<GameTown> {
    for l in ["EditTownPuck", "EditTownLibraryPanel", "EditTownInfoPanel", "PlayFlowBinGridItem", "AcceptCancelDialog", "EditTownConfirmEvict", "ChangeLotType"] {
        ui.layout(l)?;
    }
    let mut spawn = |name: &str| {
        let mut w = ui.layout(name)?.clone();
        w.flags |= WIN_VISIBLE;
        // (The tool's words under Return to Live Mode, one line centred under it, as the game
        // sizes them.)
        edit_windows(&mut w, PLACEMENT_TEXT, &mut |x| x.area = [-130.0, 44.0, 419.0, 68.0]);
        let s = ui.spawn_root(commands, images, fonts, &w);
        if let Some(r) = s.root {
            commands.entity(r).insert((DespawnOnExit(crate::AppState::InGame), GlobalZIndex(8)));
        }
        Some(s)
    };
    let (puck, library) = (spawn("EditTownPuck")?, spawn("EditTownLibraryPanel")?);
    for id in [RETURN_ENTRY, TOOL_MOVE, TOOL_EDIT, TOOL_CHANGE, EXPORT, PLACEMENT_TEXT] {
        for e in puck.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for id in [TAB_COMMUNITY, FILTER, UNHIDE] {
        for e in library.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for (s, id, tip) in [
        (&puck, RETURN_LIVE, "Return to Live Mode"),
        (&puck, OPTIONS, "Options"),
        (&puck, ORBIT[0], "Rotate"),
        (&puck, ORBIT[1], "Rotate"),
        (&puck, TOOL_DEFAULT, "Select"),
        (&puck, TOOL_BULLDOZE, "Bulldoze"),
        (&library, TAB_HOUSEHOLDS, "Households"),
        (&library, TAB_HOUSES, "Houses"),
    ] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    if let Some(g) = library.id(BIN_GRID) {
        commands.entity(g).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
    }
    Some(GameTown { puck, library, info: None, dialog: None, households: true, focus: None, tool: Tool::Select, scroll: 0, cells: None, shown: None, changes: 0, picked: None, test_press: None, test_cell: None, test_lot: None })
}

/// The lot whose picture and description stand for `l`'s: its own, the lot whose house was copied
/// onto it, or none (bulldozed).
fn picture_lot(world: &crate::loading::WorldInfo, story: &crate::story::TownStory, l: usize) -> Option<usize> {
    let id = world.lots.get(l)?.id;
    match story.placed.get(&id) {
        Some(from) => world.lots.iter().position(|x| x.id == *from),
        None if story.bulldozed.contains(&id) => None,
        None => Some(l),
    }
}

/// How much taller a description box must be for `text` (the game's scroll; ours shows it all).
fn extra_height(text: &str, height: f32) -> f32 {
    const LINE: f32 = 14.5;
    const CHARS: usize = 62;
    let lines = text.lines().map(|l| l.chars().count() / CHARS + 1).sum::<usize>() as f32;
    (lines * LINE + 6.0 - height).max(0.0)
}

/// A window moved down by `by` in a layout before it's made.
fn nudge(w: &mut UiWindow, id: u32, by: f32) {
    edit_windows(w, id, &mut |x| {
        x.area[1] += by;
        x.area[3] += by;
    });
}

/// A window's (and its children's) visibility in a layout before it's made.
fn set_shown(w: &mut UiWindow, id: u32, on: bool) {
    edit_windows(w, id, &mut |x| {
        if on {
            x.flags |= WIN_VISIBLE;
        } else {
            x.flags &= !WIN_VISIBLE;
        }
    });
}

/// Every window's caption that reads `from`, blanked (the difficulty meter's label: the game's
/// difficulty isn't kept).
fn blank_caption(w: &mut UiWindow, from: &str) {
    if w.caption == from {
        w.caption.clear();
    }
    for c in &mut w.children {
        blank_caption(c, from);
    }
}

/// The info panel, as the game's `UpdateWindowVisiblity` lays it out: the household's part and the
/// lot's (either may be missing, and their descriptions), the buttons for what can be done, the
/// panel as tall as what's in it.
#[allow(clippy::too_many_arguments)]
fn info_panel(
    commands: &mut Commands,
    ui: &mut UiAssets,
    images: &mut Assets<Image>,
    fonts: &mut Assets<Font>,
    world: &crate::loading::WorldInfo,
    data: &crate::baked::Baked,
    story: &crate::story::TownStory,
    hh: Option<&HouseholdInfo>,
    lot: Option<usize>,
    buttons_shown: &[u32],
) -> Option<Spawned> {
    let mut w = ui.layout("EditTownInfoPanel")?.clone();
    w.flags |= WIN_VISIBLE;
    let height = |w: &UiWindow, id: u32| w.find(id).map_or(0.0, |x| x.area[3] - x.area[1]);
    let (household_h, household_desc_h, lot_h, lot_desc_h, place_h) = (height(&w, HOUSEHOLD_PANEL), height(&w, HOUSEHOLD_DESC), height(&w, LOT_PANEL), height(&w, LOT_DESC), height(&w, PLACE_COPY));
    let main_h = height(&w, INFO_MAIN);
    let mut shrink = 0.0;
    // The household.
    match hh {
        Some(h) => {
            edit_windows(&mut w, HOUSEHOLD_NAME, &mut |x| x.caption = h.name.clone());
            edit_windows(&mut w, HOUSEHOLD_FUNDS, &mut |x| x.caption = money(h.funds));
            edit_windows(&mut w, HOUSEHOLD_DESC, &mut |x| x.caption = h.bio.clone());
            if h.bio.is_empty() {
                shrink += household_desc_h;
                set_shown(&mut w, HOUSEHOLD_DESC_BG, false);
            } else {
                // (A long description: its box as tall as its words, the panel taller.)
                let extra = extra_height(&h.bio, household_desc_h);
                for id in [HOUSEHOLD_DESC, HOUSEHOLD_DESC_BG, HOUSEHOLD_PANEL] {
                    edit_windows(&mut w, id, &mut |x| x.area[3] += extra);
                }
                shrink -= extra;
            }
        }
        None => {
            shrink += household_h;
            set_shown(&mut w, HOUSEHOLD_PANEL, false);
        }
    }
    set_shown(&mut w, HOUSEHOLD_METER, false);
    blank_caption(&mut w, "Difficulty:");
    // The buttons.
    for id in NOT_HERE.into_iter().chain([PLACE_COPY, EVICT, CHANGE_TYPE, SWITCH_TO, SPLIT, MERGE]) {
        set_shown(&mut w, id, buttons_shown.contains(&id));
    }
    if buttons_shown.contains(&PLACE_COPY) {
        edit_windows(&mut w, PLACE_COPY, &mut |x| x.caption = "Place".into());
    } else {
        nudge(&mut w, LOT_PANEL, place_h);
        nudge(&mut w, INFO_BUTTONS, place_h);
        shrink += place_h;
    }
    // The lot.
    match lot {
        Some(l) => {
            let info = &world.lots[l];
            let name = world.lot_names.get(l).cloned().unwrap_or_else(|| info.internal_name.clone());
            // (What stands there now: its own house, a copy of another's, or nothing.)
            let source = picture_lot(world, story, l);
            let desc = source.and_then(|k| world.lots.get(k)).map(|s| s.string_keys.iter().find(|k| k.contains("HouseDesc") || k.contains("LotDesc")).and_then(|k| ui.localize(k)).unwrap_or_default()).unwrap_or_default();
            edit_windows(&mut w, LOT_NAME, &mut |x| x.caption = name.clone());
            edit_windows(&mut w, LOT_ADDRESS, &mut |x| x.caption = crate::edittown::lot_kind(world, l));
            edit_windows(&mut w, LOT_SIZE, &mut |x| x.caption = format!("{} x {}", info.width, info.depth));
            edit_windows(&mut w, LOT_DESC, &mut |x| x.caption = desc.clone());
            if desc.is_empty() {
                shrink += lot_desc_h;
                nudge(&mut w, LOT_PANEL, lot_desc_h);
                set_shown(&mut w, LOT_DESC_BG, false);
            } else {
                let extra = extra_height(&desc, lot_desc_h);
                for id in [LOT_DESC, LOT_DESC_BG] {
                    edit_windows(&mut w, id, &mut |x| x.area[3] += extra);
                }
                edit_windows(&mut w, LOT_PANEL, &mut |x| x.area[1] -= extra);
                shrink -= extra;
            }
        }
        None => {
            shrink += lot_h;
            set_shown(&mut w, LOT_PANEL, false);
        }
    }
    // (The panel's top comes down by what's missing; what's in it stays at its foot.)
    edit_windows(&mut w, INFO_MAIN, &mut |x| x.area[1] = x.area[3] - (main_h - shrink));
    let s = ui.spawn_root(commands, images, fonts, &w);
    let root = s.root?;
    commands.entity(root).insert((DespawnOnExit(crate::AppState::InGame), GlobalZIndex(9), crate::hud::BlocksWorld));
    if let (Some(win), Some(p)) = (s.id(HOUSEHOLD_THUMB), hh.and_then(|h| h.picture.clone())) {
        crate::hudpanels::picture(commands, win, p, Color::WHITE);
    }
    if let (Some(win), Some(l)) = (s.id(LOT_THUMB), lot.and_then(|l| picture_lot(world, story, l)))
        && let Some(img) = crate::objects::cpu_texture(&data.0, s3bake::lot_thumbnail_key(world.lots[l].id))
    {
        crate::hudpanels::picture(commands, win, images.add(img), Color::WHITE);
    }
    Some(s)
}

/// The game's accept-or-cancel dialog with `prompt`.
fn ask(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, prompt: &str) -> Option<Spawned> {
    let s = crate::gamedialogs::spawn_dialog(commands, ui, images, fonts, "AcceptCancelDialog")?;
    if let Some(t) = s.text(ASK_TEXT) {
        commands.entity(t).insert(Text::new(prompt.to_string()));
    }
    for (id, tip) in [(ASK_ACCEPT, "Accept"), (ASK_CANCEL, "Cancel")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    Some(s)
}

/// The game's evict dialog: the household and the lot, Move Out (their furniture sold), Move Out
/// With Home (packed in their family inventory: for a household played before, whose furniture
/// it is) or Cancel.
fn evict_dialog(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, household: &str, lot: &str, can_pack: bool) -> Option<Spawned> {
    let s = crate::gamedialogs::spawn_dialog(commands, ui, images, fonts, "EditTownConfirmEvict")?;
    for (id, t) in [(EVICT_HOUSEHOLD, household), (EVICT_LOT, lot)] {
        if let Some(x) = s.text(id) {
            commands.entity(x).insert(Text::new(t.to_string()));
        }
    }
    if let Some(e) = s.id(EVICT_TAKE) {
        if can_pack {
            commands.entity(e).insert(crate::icons::Tooltip("Move out, their furniture packed in their family inventory".into()));
        } else {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    if let Some(e) = s.id(EVICT_SELL) {
        commands.entity(e).insert(crate::icons::Tooltip("Move out, their furniture sold for four-fifths of what it cost".into()));
    }
    Some(s)
}

/// The game's Change Lot Type dialog: residential or community, and which kind of community lot.
fn type_dialog(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, lot: &s3bake::LotInfo) -> Option<Spawned> {
    let s = crate::gamedialogs::spawn_dialog(commands, ui, images, fonts, "ChangeLotType")?;
    let residential = lot.is_residential();
    for (id, on) in [(TYPE_RESIDENTIAL, residential), (TYPE_COMMUNITY, !residential)] {
        if let Some(e) = s.id(id) {
            commands.entity(e).queue(move |mut e: EntityWorldMut| {
                if let Some(mut b) = e.get_mut::<UiButton>() {
                    b.selected = on;
                }
            });
        }
    }
    for (id, tip) in [(TYPE_ACCEPT, "Accept"), (TYPE_CANCEL, "Cancel")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    Some(s)
}
