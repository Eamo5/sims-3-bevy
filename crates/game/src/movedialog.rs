//! Edit Town's split and merge, in the game's Move dialog (`MovingDialog`, as UI.dll's
//! `EditTownSplitDialog` and `EditTownMergeDialog` drive it over `MoveDialogBase`): two
//! households side by side, each with its name, funds, home and members (a cell each: their
//! picture and name); a member picked and the arrows between move them across. A household with
//! children or teens needs a grown-up in it, and no more than eight live in one (the game's
//! `IsLotValid`, `Household.IsValidHousehold`). Splitting the household being played: those
//! moved across leave for the household bin, a household of their own with their share of the
//! funds (`moveout::split`). Merging a household played before into the one being played: those
//! moved across join it, with their share of their household's funds (all of it, when everyone
//! comes). Either way the game goes on from there (saved, and loaded again).

use bevy::prelude::*;

use crate::AppState;
use crate::layout::{Spawned, UiAssets, UiButton, edit_windows};
use crate::livehud::{pressed, set_text, set_visible};
use crate::sim::Age;

pub struct MoveDialogPlugin;

impl Plugin for MoveDialogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (move_test, open_dialog, dialog, apply_move).chain().run_if(in_state(crate::PlayMode::Live)));
    }
}

/// What's being done.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MoveKind {
    /// The household being played split, some to the bin.
    Split,
    /// A household played before (its place among them) merged into the one being played.
    Merge(usize),
    /// A town family never played (by household id) merged into it.
    MergeTown(u64),
}

/// Asked for (Edit Town's Split and Merge buttons).
#[derive(Resource)]
pub struct OpenMove(pub MoveKind);

// `MovingDialog`'s windows; the target side's are the source's and 0x100.
const CANCEL: u32 = 0x02;
const ACCEPT: u32 = 0x03;
const TO_SOURCE: u32 = 0x04;
const TO_TARGET: u32 = 0x05;
const TITLE: u32 = 0x06;
const MAKE_ACTIVE: u32 = 0x10;
const NAME: u32 = 0x11;
const FUNDS: u32 = 0x12;
const NAME_EDIT: u32 = 0x13;
const DESCRIPTION_BUTTON: u32 = 0x14;
const WORTH: u32 = 0x15;
const LOT_NAME: u32 = 0x17;
const LOT_THUMB: u32 = 0x18;
const MEMBERS: u32 = 0x19;
const INVALID: u32 = 0x1a;
const PICK_PANEL: u32 = 0x1d;
const SELL_PANEL: u32 = 0x1e;
const MOVE_PANEL: u32 = 0x22;
const DESCRIPTION: u32 = 0x26;
const CLIPBOARD: u32 = 0x221;
const TARGET: u32 = 0x100;
/// The member cells' (`TableBorderThumbAndText`) picture.
const CELL_THUMB: u32 = 0x05ac_1600;
/// The dialog's size (its layout's).
const SIZE: Vec2 = Vec2::new(778.0, 677.0);

/// One of a household in the dialog.
#[derive(Clone, Debug)]
struct Member {
    id: u64,
    name: String,
    age: Age,
    sim: Option<Entity>,
}

/// The dialog on screen: what for, the two households' funds and members, the member picked (its side, its place), and the cells shown.
#[derive(Resource)]
struct MoveDialog {
    s: Spawned,
    backdrop: Entity,
    kind: MoveKind,
    funds: [i64; 2],
    members: [Vec<Member>; 2],
    /// Those at first on each side (to know who's moving, and the funds' share).
    at_first: [Vec<u64>; 2],
    picked: Option<(usize, usize)>,
    cells: [Option<Entity>; 2],
    started: bool,
    shown: Option<(Vec<u64>, Vec<u64>, Option<(usize, usize)>)>,
}

/// A member's cell: its side, its place.
#[derive(Component)]
struct MemberCell(usize, usize);

/// The move accepted: waiting for the game as it stands, to change and load.
#[derive(Resource)]
struct PendingMove {
    kind: MoveKind,
    movers: Vec<u64>,
}

/// Why a household can't be as it is, if it can't (the game's `IsLotValid`, in its words).
fn invalid(ui: &UiAssets, members: &[Member]) -> Option<String> {
    let word = |key: &str, plain: &str| ui.localize(key).unwrap_or_else(|| plain.to_string());
    if members.len() > 8 {
        return Some(word("Ui/Caption/MovingDialog:TooManySims", "A household can have no more than eight Sims."));
    }
    let young = members.iter().any(|m| !m.age.is_grown());
    let grown = members.iter().any(|m| m.age.is_grown());
    (young && !grown).then(|| word("Ui/Caption/MovingDialog:AdultRequired", "A young adult, adult, or elder is required in a household with children or teens."))
}

#[allow(clippy::too_many_arguments)]
fn open_dialog(
    mut commands: Commands,
    want: Option<Res<OpenMove>>,
    open: Option<Res<MoveDialog>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    members: Query<(Entity, &crate::sim::Sim), With<crate::sim::HouseholdMember>>,
    sims: Query<(Entity, &crate::sim::Sim)>,
    (household, dormant, world): (Option<Res<crate::interact::Household>>, Res<crate::household::Dormant>, Res<crate::loading::CurrentWorld>),
    (town, story): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>),
) {
    let Some(want) = want else { return };
    commands.remove_resource::<OpenMove>();
    let (Some(mut ui), Some(hh)) = (ui, household) else { return };
    if open.is_some() {
        return;
    }
    let word = |ui: &UiAssets, key: &str, plain: &str| ui.localize(key).unwrap_or_else(|| plain.to_string());
    let lot_name = |i: usize| world.data.lot_names.get(i).cloned().unwrap_or_default();
    let ours: Vec<Member> = members.iter().map(|(e, s)| Member { id: s.id, name: s.full_name(), age: s.age, sim: Some(e) }).collect();
    let bin = word(&ui, "Ui/Caption/MovingDialog:ToGameBin", "Household Bin");
    let (names, funds, homes, sides, title) = match want.0 {
        MoveKind::Split => (
            [hh.name.clone(), ours.first().map_or_else(String::new, |m| m.name.rsplit(' ').next().unwrap_or("").to_string())],
            [hh.funds, 0],
            [lot_name(hh.lot_index), bin],
            [ours, Vec::new()],
            word(&ui, "Ui/Caption/GameEntry/EditTown:SplitHousehold", "Split Household"),
        ),
        MoveKind::Merge(i) => {
            let Some(d) = dormant.0.get(i) else { return };
            let theirs: Vec<Member> = d
                .sims
                .iter()
                .filter(|s| s.member)
                .map(|s| Member { id: s.id, name: format!("{} {}", s.first, s.last), age: crate::save::age_from_name(&s.age), sim: sims.iter().find(|(_, x)| x.id == s.id).map(|(e, _)| e) })
                .collect();
            (
                [d.household.clone(), hh.name.clone()],
                [d.funds, hh.funds],
                [if d.homeless { bin } else { d.lot_name.clone() }, lot_name(hh.lot_index)],
                [theirs, ours],
                word(&ui, "Ui/Caption/GameEntry/EditTown:MergeHouseholds", "Merge Households"),
            )
        }
        MoveKind::MergeTown(id) => {
            let Some(h) = town.as_ref().and_then(|t| t.0.households.iter().find(|h| h.id == id)) else { return };
            let theirs: Vec<Member> = h
                .members
                .iter()
                .filter(|m| !m.first_name.is_empty())
                .filter_map(|m| story.apply(crate::premade::to_sim(m)))
                .map(|s| Member { id: s.id, name: s.full_name(), age: s.age, sim: sims.iter().find(|(_, x)| x.id == s.id).map(|(e, _)| e) })
                .collect();
            let home = story.home_of(h).and_then(|l| world.data.lots.iter().position(|x| x.id == l)).map_or(bin, lot_name);
            (
                [h.name.clone(), hh.name.clone()],
                [h.funds, hh.funds],
                [home, lot_name(hh.lot_index)],
                [theirs, ours],
                word(&ui, "Ui/Caption/GameEntry/EditTown:MergeHouseholds", "Merge Households"),
            )
        }
    };
    let Some(mut layout) = ui.layout("MovingDialog").cloned() else { return };
    layout.flags |= s3bake::ui::WIN_VISIBLE;
    // (Nor the lots' pictures: their frames go too.)
    hide_commented(&mut layout, "lot BG");
    edit_windows(&mut layout, TITLE, &mut |t| t.caption = title.clone());
    // (What a split or merge here hasn't: making one active, descriptions, the lots' pickers,
    // selling, moving out, the clipboard, the worth and the lot's picture.)
    for side in [0, TARGET] {
        for id in [MAKE_ACTIVE, DESCRIPTION_BUTTON, DESCRIPTION, PICK_PANEL, SELL_PANEL, MOVE_PANEL, CLIPBOARD, WORTH, LOT_THUMB, INVALID] {
            edit_windows(&mut layout, side + id, &mut |w| w.flags &= !s3bake::ui::WIN_VISIBLE);
        }
    }
    for (side, n) in [(0usize, 0u32), (1, TARGET)] {
        let name = names[side].clone();
        edit_windows(&mut layout, n + NAME, &mut |t| t.caption = name.clone());
        edit_windows(&mut layout, n + NAME_EDIT, &mut |t| t.caption = name.clone());
        let home = homes[side].clone();
        edit_windows(&mut layout, n + LOT_NAME, &mut |t| t.caption = home.clone());
        edit_windows(&mut layout, n + FUNDS, &mut |t| t.caption = String::new());
    }
    let backdrop = commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
            Interaction::default(),
            crate::hud::BlocksWorld,
            GlobalZIndex(39),
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &layout);
    let Some(root) = s.root else {
        commands.entity(backdrop).despawn();
        return;
    };
    commands.entity(root).insert((GlobalZIndex(40), crate::hud::BlocksWorld, DespawnOnExit(AppState::InGame)));
    commands.entity(root).entry::<Node>().and_modify(|mut n| {
        n.left = Val::Percent(50.0);
        n.top = Val::Percent(50.0);
        n.margin = UiRect { left: Val::Px(-SIZE.x * 0.5), top: Val::Px(-SIZE.y * 0.5), ..default() };
    });
    for (id, tip) in [(ACCEPT, "Accept"), (CANCEL, "Cancel"), (TO_SOURCE, "Move to the household on the left"), (TO_TARGET, "Move to the household on the right")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    let at_first = [sides[0].iter().map(|m| m.id).collect(), sides[1].iter().map(|m| m.id).collect()];
    info!("move dialog: {:?}, {} and {}", want.0, names[0], names[1]);
    commands.insert_resource(MoveDialog { s, backdrop, kind: want.0, funds, members: sides, at_first, picked: None, cells: [None, None], started: false, shown: None });
}

/// Windows of a layout with this comment hidden.
fn hide_commented(w: &mut s3bake::ui::UiWindow, comment: &str) {
    if w.comment == comment {
        w.flags &= !s3bake::ui::WIN_VISIBLE;
    }
    for c in &mut w.children {
        hide_commented(c, comment);
    }
}

/// Those who've moved across: from the household split, or the one merging in (the left).
fn movers(d: &MoveDialog) -> Vec<u64> {
    d.at_first[0].iter().copied().filter(|id| d.members[1].iter().any(|m| m.id == *id)).collect()
}

/// The two sides' funds as the move would leave them: the game's share for those moving.
fn funds_now(d: &MoveDialog) -> [i64; 2] {
    let moving = movers(d).len();
    let size = d.at_first[0].len();
    if moving == 0 {
        return d.funds;
    }
    let share = if moving == size { d.funds[0] } else { crate::moveout::transfer(d.funds[0], size, moving) };
    [d.funds[0] - share, d.funds[1] + share]
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dialog(
    mut commands: Commands,
    d: Option<ResMut<MoveDialog>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    cells: Query<(&Interaction, &MemberCell), Changed<Interaction>>,
    (mut vis, mut texts, mut buttons): (Query<&mut Visibility>, Query<&mut Text>, Query<&mut UiButton>),
    mut portraits: ResMut<crate::portraits::Portraits>,
    mut snapshot: MessageWriter<crate::save::SnapshotRequest>,
) {
    let (Some(mut d), Some(mut ui)) = (d, ui) else { return };
    let d = &mut *d;
    // (Its windows' words set the frame after it's up.)
    if !d.started {
        d.started = true;
        return;
    }
    let close = |commands: &mut Commands, d: &MoveDialog| {
        if let Some(r) = d.s.root {
            commands.entity(r).try_despawn();
        }
        commands.entity(d.backdrop).try_despawn();
        commands.remove_resource::<MoveDialog>();
    };
    if pressed(&clicks, d.s.id(CANCEL)) {
        close(&mut commands, d);
        return;
    }
    let reasons = [invalid(&ui, &d.members[0]), invalid(&ui, &d.members[1])];
    let moving = movers(d);
    let ready = !moving.is_empty() && reasons.iter().all(|r| r.is_none()) && !(d.kind == MoveKind::Split && d.members[0].is_empty());
    if pressed(&clicks, d.s.id(ACCEPT)) && ready {
        info!("move dialog: {:?} accepted, {} moving", d.kind, moving.len());
        commands.insert_resource(PendingMove { kind: d.kind, movers: moving.clone() });
        commands.remove_resource::<crate::save::Snapshot>();
        snapshot.write(crate::save::SnapshotRequest);
        close(&mut commands, d);
        return;
    }
    // A member picked; the arrows move them across.
    for (i, c) in &cells {
        if *i == Interaction::Pressed {
            d.picked = Some((c.0, c.1));
        }
    }
    for (id, to) in [(TO_TARGET, 1usize), (TO_SOURCE, 0usize)] {
        if pressed(&clicks, d.s.id(id))
            && let Some((side, n)) = d.picked
            && side != to
            && n < d.members[side].len()
        {
            let m = d.members[side].remove(n);
            d.members[to].push(m);
            d.picked = Some((to, d.members[to].len() - 1));
        }
    }
    // Shown: the funds, why a side won't do, Accept while it all will; the members' cells.
    let funds = funds_now(d);
    for (side, n) in [(0usize, 0u32), (1, TARGET)] {
        set_text(&mut texts, d.s.text(n + FUNDS), &format!("§{}", crate::lifetime::group(funds[side])));
        set_visible(&mut vis, d.s.id(n + INVALID), reasons[side].is_some());
        if let Some(e) = d.s.id(n + INVALID) {
            commands.entity(e).insert(crate::icons::Tooltip(reasons[side].clone().unwrap_or_default()));
        }
    }
    if let Some(mut b) = d.s.id(ACCEPT).and_then(|e| buttons.get_mut(e).ok())
        && b.disabled == ready
    {
        b.disabled = !ready;
    }
    let want = (d.members[0].iter().map(|m| m.id).collect(), d.members[1].iter().map(|m| m.id).collect(), d.picked);
    if d.shown.as_ref() == Some(&want) {
        return;
    }
    d.shown = Some(want);
    let Some(template) = ui.layout("TableBorderThumbAndText").cloned() else { return };
    for (side, n) in [(0usize, 0u32), (1, TARGET)] {
        if let Some(c) = d.cells[side].take() {
            commands.entity(c).try_despawn();
        }
        let (Some(grid), Some(g)) = (d.s.id(n + MEMBERS), ui.find("MovingDialog", n + MEMBERS).and_then(|w| w.grid)) else { continue };
        let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, ChildOf(grid))).id();
        d.cells[side] = Some(holder);
        let cols = g.columns.max(1) as usize;
        let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
        for (k, m) in d.members[side].iter().enumerate() {
            let (x, y) = (g.padding[0] + (k % cols) as f32 * step.x + g.cell_padding[0], g.padding[1] + (k / cols) as f32 * step.y + g.cell_padding[1]);
            let mut w = template.clone();
            w.area = [x, y, x + g.cell[0], y + g.cell[1]];
            w.cls = "Button".into();
            w.flags |= s3bake::ui::WIN_VISIBLE;
            for c in w.children.iter_mut().filter(|c| c.cls == "Text") {
                c.caption = m.name.clone();
            }
            // (The picture down the cell's left, the whole of it.)
            edit_windows(&mut w, CELL_THUMB, &mut |t| {
                t.area = [5.0, 2.0, 51.0, 48.0];
                t.place = s3bake::ui::UiPlace::Fixed;
            });
            let cs = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
            let Some(r) = cs.root else { continue };
            commands.entity(r).insert((MemberCell(side, k), Interaction::default(), crate::hud::BlocksWorld));
            if d.picked == Some((side, k)) {
                commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.35)), Pickable::IGNORE, ChildOf(r)));
            }
            if let (Some(win), Some(sim)) = (cs.id(CELL_THUMB), m.sim) {
                let h = portraits.portrait(&mut images, sim);
                commands.entity(win).despawn_children();
                commands.entity(win).with_children(|c| {
                    c.spawn((ImageNode::new(h), crate::portraits::PortraitOf(sim), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                });
            }
        }
    }
}

/// The move made, on the game as it stands: saved (to the household's own file) and loaded again,
/// Edit Town closed.
#[allow(clippy::too_many_arguments)]
fn apply_move(
    mut commands: Commands,
    pending: Option<Res<PendingMove>>,
    snapshot: Option<Res<crate::save::Snapshot>>,
    worlds: Res<crate::data::WorldList>,
    slot: Res<crate::save::SaveSlot>,
    world: Res<crate::loading::CurrentWorld>,
    mut app: ResMut<NextState<AppState>>,
    mut notes: ResMut<crate::interact::Notifications>,
    town: Option<Res<crate::premade::TownPremades>>,
) {
    let (Some(p), Some(snap)) = (pending, snapshot) else { return };
    commands.remove_resource::<PendingMove>();
    let g = match p.kind {
        MoveKind::Split => {
            let (out, mut rest) = crate::moveout::split(&snap.0, &p.movers);
            info!("split: the {} household to the bin ({} Sims, §{})", out.household, p.movers.len(), out.funds);
            notes.push(format!("The {} household is in the household bin.", out.household));
            rest.dormant.push(out);
            rest
        }
        MoveKind::Merge(i) => {
            let mut g = snap.0.clone();
            let Some(mut d) = g.dormant.get(i).cloned() else { return };
            let size = d.sims.iter().filter(|s| s.member).count();
            let share = if p.movers.len() >= size { d.funds } else { crate::moveout::transfer(d.funds, size, p.movers.len()) };
            let lot = &world.data.lots[g.lot_index];
            let c = crate::home::lot_center(lot);
            let y = world.data.heightmap.sample(c.x, c.z);
            for s in d.sims.iter_mut().filter(|s| s.member && p.movers.contains(&s.id)) {
                let mut joining = s.clone();
                joining.selected = false;
                joining.whereabouts = "home".into();
                joining.position = [c.x, y, c.z];
                joining.floor = 1;
                g.sims.retain(|x| x.id != joining.id);
                g.sims.push(joining);
                s.member = false;
                s.selected = false;
                s.whereabouts = "away".into();
            }
            g.funds += share;
            d.funds -= share;
            info!("merge: {} of the {} household join the {} household (§{share})", p.movers.len(), d.household, g.household);
            notes.push(format!("The {} household welcomes its new members.", g.household));
            if d.sims.iter().any(|s| s.member) {
                if !d.sims.iter().any(|s| s.member && s.selected)
                    && let Some(s) = d.sims.iter_mut().find(|s| s.member)
                {
                    s.selected = true;
                }
                g.dormant[i] = d;
            } else {
                g.dormant.remove(i);
            }
            g
        }
        MoveKind::MergeTown(id) => {
            let mut g = snap.0.clone();
            let Some(h) = town.as_ref().and_then(|t| t.0.households.iter().find(|h| h.id == id)) else { return };
            let alive: Vec<crate::sim::Sim> = h.members.iter().filter(|m| !m.first_name.is_empty()).filter_map(|m| g.town.apply(crate::premade::to_sim(m))).collect();
            let share = if p.movers.len() >= alive.len() { h.funds } else { crate::moveout::transfer(h.funds, alive.len(), p.movers.len()) };
            let lot = &world.data.lots[g.lot_index];
            let c = crate::home::lot_center(lot);
            let y = world.data.heightmap.sample(c.x, c.z);
            for s in alive.iter().filter(|s| p.movers.contains(&s.id)) {
                // (One the game knows already keeps all of it; else as the town has them.)
                let mut joining = g.sims.iter().find(|x| x.id == s.id).cloned().unwrap_or_else(|| crate::save::saved_look(s));
                joining.member = true;
                joining.selected = false;
                joining.whereabouts = "home".into();
                joining.position = [c.x, y, c.z];
                joining.floor = 1;
                g.sims.retain(|x| x.id != joining.id);
                g.sims.push(joining);
            }
            g.funds += share;
            // (Everyone come: the family's house left to them no more.)
            if p.movers.len() >= alive.len() {
                g.town.homes.insert(id, None);
            }
            info!("merge: {} of the {} family join the {} household (§{share})", p.movers.len(), h.name, g.household);
            notes.push(format!("The {} household welcomes its new members.", g.household));
            g
        }
    };
    let world_name = g.world.clone();
    if crate::save::begin_load(&mut commands, &worlds, g, slot.0.clone()) {
        app.set(AppState::Loading);
    } else {
        warn!("move: no world {world_name} to load the household in again");
    }
}

/// MOVE_DIALOG=split|merge:<n>|town:<family name> (tests): ten seconds in, the dialog; two
/// seconds on, the left's last member moved across (MOVE_ALL=1: all of them); MOVE_ACCEPT=1, five
/// seconds on, accepted.
fn move_test(mut commands: Commands, time: Res<Time>, d: Option<ResMut<MoveDialog>>, mut buttons: Query<&mut Interaction>, mut step: Local<(u8, f32)>, town: Option<Res<crate::premade::TownPremades>>) {
    let Ok(v) = std::env::var("MOVE_DIALOG") else { return };
    let t = time.elapsed_secs();
    match step.0 {
        0 if t > 10.0 => {
            let family = v.strip_prefix("town:").and_then(|n| town.as_ref()?.0.households.iter().find(|h| h.name.eq_ignore_ascii_case(n)).map(|h| h.id));
            let kind = match (v.strip_prefix("merge:").and_then(|n| n.parse().ok()), family) {
                (Some(n), _) => MoveKind::Merge(n),
                (None, Some(id)) => MoveKind::MergeTown(id),
                (None, None) => MoveKind::Split,
            };
            commands.insert_resource(OpenMove(kind));
            *step = (1, t);
        }
        1 if t - step.1 > 2.0 => {
            if let Some(mut d) = d {
                // (All of them: moved straight across.)
                if std::env::var("MOVE_ALL").is_ok() {
                    let all: Vec<Member> = d.members[0].drain(..).collect();
                    d.members[1].extend(all);
                } else {
                    let n = d.members[0].len().saturating_sub(1);
                    d.picked = Some((0, n));
                    if let Some(mut i) = d.s.id(TO_TARGET).and_then(|e| buttons.get_mut(e).ok()) {
                        *i = Interaction::Pressed;
                    }
                }
            }
            *step = (2, t);
        }
        2 if t - step.1 > 5.0 && std::env::var("MOVE_ACCEPT").is_ok() => {
            if let Some(d) = d
                && let Some(mut i) = d.s.id(ACCEPT).and_then(|e| buttons.get_mut(e).ok())
            {
                *i = Interaction::Pressed;
            }
            *step = (3, t);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: u64, age: Age) -> Member {
        Member { id, name: String::new(), age, sim: None }
    }

    #[test]
    fn those_moving_take_their_share() {
        let d = MoveDialog {
            s: Spawned::default(),
            backdrop: Entity::PLACEHOLDER,
            kind: MoveKind::Split,
            funds: [10_000, 0],
            members: [vec![m(1, Age::Adult)], vec![m(2, Age::Adult)]],
            at_first: [vec![1, 2], vec![]],
            picked: None,
            cells: [None, None],
            started: true,
            shown: None,
        };
        assert_eq!(movers(&d), vec![2]);
        // (One of two moving takes a tenth of the funds: §1,000.)
        assert_eq!(funds_now(&d), [9_000, 1_000]);
    }
}
