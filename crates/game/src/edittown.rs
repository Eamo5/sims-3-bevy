//! Edit Town (from the game menu, as the game's): the town from above with every lot outlined,
//! who lives where, and the household bin: the town's families with no home of their own (the
//! world's own homeless ones, and any evicted). A household can be evicted to the bin (a played
//! household's furniture sold back for four-fifths of what it cost; the house stands as it is), a
//! household in the bin moved into an empty home, and any household with a home played (as
//! Change Household does). Where everyone lives is kept with the town's story, in the save.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{Household, Notifications};
use crate::menu::{BTN_HOVER, BTN_NORMAL, BTN_PRESS, PLUMBOB_GREEN, text};
use crate::save::SaveGame;

pub struct EditTownPlugin;

impl Plugin for EditTownPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditTown>()
            .add_message::<OpenEditTown>()
            .add_systems(Update, (scripted, open_edit_town, edit_town_buttons, show_edit_town, button_visuals, draw_town_lots).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(crate::AppState::InGame), |mut e: ResMut<EditTown>| *e = EditTown::default());
    }
}

/// Asked for from the game menu.
#[derive(Message)]
pub struct OpenEditTown;

/// Edit Town while it's open: its panel, the lot picked, and the game's speed and camera to go
/// back to.
#[derive(Resource, Default)]
pub struct EditTown {
    root: Option<Entity>,
    selected: Option<usize>,
    open: bool,
    dirty: bool,
    speed: usize,
    camera: Option<(Vec3, f32, f32, f32)>,
}

impl EditTown {
    pub fn is_open(&self) -> bool {
        self.open
    }
}

/// Who lives on a lot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Occupant {
    /// The household being played.
    Active,
    /// A household played before (by its place among them).
    Played(usize),
    /// A town family never played (by household id).
    Town(u64),
}

/// A household in the bin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Binned {
    Played(usize),
    Town(u64),
}

/// Whether a town family is one to show: not the world's ghosts, and with someone the town
/// hasn't lost.
pub fn town_family(h: &s3bake::HouseholdBaked, story: &crate::story::TownStory) -> bool {
    !h.name.to_ascii_lowercase().contains("ghost") && h.members.iter().filter(|m| !m.first_name.is_empty()).any(|m| story.apply(crate::premade::to_sim(m)).is_some())
}

/// Who lives where: the household being played (on `active`), those played before (not in the
/// bin), and the town's families at their homes.
pub fn occupants(
    world: &crate::loading::WorldInfo,
    town: Option<&s3bake::PremadesBaked>,
    story: &crate::story::TownStory,
    dormant: &[SaveGame],
    active: Option<usize>,
) -> HashMap<usize, Occupant> {
    let mut out = HashMap::new();
    if let Some(t) = town {
        for h in t.households.iter().filter(|h| town_family(h, story)) {
            if let Some(i) = story.home_of(h).and_then(|id| world.lots.iter().position(|l| l.id == id)) {
                out.insert(i, Occupant::Town(h.id));
            }
        }
    }
    // (Those played before and the household played live where they do, whoever was there.)
    let played: std::collections::HashSet<u64> = dormant.iter().flat_map(|d| d.sims.iter().filter(|s| s.member).map(|s| s.id)).collect();
    out.retain(|_, o| match o {
        Occupant::Town(id) => !town.and_then(|t| t.households.iter().find(|h| h.id == *id)).is_some_and(|h| h.members.iter().any(|m| played.contains(&m.id))),
        _ => true,
    });
    for (i, d) in dormant.iter().enumerate().filter(|(_, d)| !d.homeless) {
        out.insert(d.lot_index, Occupant::Played(i));
    }
    if let Some(a) = active {
        out.insert(a, Occupant::Active);
    }
    out
}

/// The household bin: the households evicted (those played before, and the town's families: not
/// one played since, nor one the household being played came from). The world's own households
/// with no home are its townies, about town, not in the bin.
pub fn bin(town: Option<&s3bake::PremadesBaked>, story: &crate::story::TownStory, dormant: &[SaveGame], playing: &[u64]) -> Vec<Binned> {
    let played: std::collections::HashSet<u64> = dormant.iter().flat_map(|d| d.sims.iter().filter(|s| s.member).map(|s| s.id)).chain(playing.iter().copied()).collect();
    let mut out: Vec<Binned> = dormant.iter().enumerate().filter(|(_, d)| d.homeless).map(|(i, _)| Binned::Played(i)).collect();
    if let Some(t) = town {
        out.extend(
            t.households
                .iter()
                .filter(|h| town_family(h, story) && story.evicted(h) && !h.members.iter().any(|m| played.contains(&m.id)))
                .map(|h| Binned::Town(h.id)),
        );
    }
    out
}

/// A lot's description: a home (and how big), an empty lot, or a community lot and what's there.
fn lot_kind(world: &crate::loading::WorldInfo, i: usize) -> String {
    let l = &world.lots[i];
    if !l.is_residential() {
        let name = world.lot_names.get(i).map_or("", |s| s.as_str());
        return format!("Community lot · {}", crate::rabbitholes::lot_title(l, name));
    }
    match world.buildings.get(&i).filter(|b| b.is_house()) {
        Some(b) => {
            let floors = b.floors.iter().filter(|f| f.level > 0).map(|f| f.level).collect::<std::collections::BTreeSet<_>>().len().max(1);
            format!("{}x{} lot · {} house, {floors} floor{}", l.width, l.depth, if b.is_furnished() { "furnished" } else { "unfurnished" }, if floors > 1 { "s" } else { "" })
        }
        None => format!("{}x{} lot · empty", l.width, l.depth),
    }
}

#[derive(Component, Clone, Copy)]
enum EditAction {
    Select(usize),
    Play(Occupant),
    Evict(Occupant),
    MoveIn(Binned, usize),
    Done,
}

#[derive(Component)]
struct EditTownUi;

/// Opens Edit Town: the game paused, the camera up over the town.
fn open_edit_town(mut asked: MessageReader<OpenEditTown>, mut edit: ResMut<EditTown>, mut clock: ResMut<crate::clock::GameClock>, mut cams: Query<&mut crate::camera::SimsCamera>) {
    if asked.read().count() == 0 || edit.open {
        return;
    }
    edit.open = true;
    edit.dirty = true;
    edit.selected = None;
    edit.speed = clock.speed;
    clock.speed = 0;
    if let Ok(mut c) = cams.single_mut() {
        edit.camera = Some((c.focus, c.distance, c.pitch, c.yaw));
        c.distance = 420.0;
        c.pitch = 1.05;
    }
}

/// The name of a household, with who's in it and their money.
struct Who {
    name: String,
    detail: String,
}

#[allow(clippy::too_many_arguments)]
fn who(o: Occupant, household: Option<&Household>, members: &[String], dormant: &[SaveGame], town: Option<&s3bake::PremadesBaked>, story: &crate::story::TownStory) -> Who {
    let money = |n: i64| format!("§{}", crate::lifetime::group(n.max(0)));
    match o {
        Occupant::Active => Who {
            name: format!("The {} household (playing)", household.map_or("", |h| h.name.as_str())),
            detail: format!("{} · {}", members.join(", "), money(household.map_or(0, |h| h.funds))),
        },
        Occupant::Played(i) => {
            let d = &dormant[i];
            let names: Vec<String> = d.sims.iter().filter(|s| s.member).map(|s| s.first.clone()).collect();
            Who { name: format!("The {} household", d.household), detail: format!("Played before · {} · {}", names.join(", "), money(d.funds)) }
        }
        Occupant::Town(id) => match town.and_then(|t| t.households.iter().find(|h| h.id == id)) {
            Some(h) => {
                let names: Vec<String> = h.members.iter().filter(|m| !m.first_name.is_empty()).filter_map(|m| story.apply(crate::premade::to_sim(m))).map(|s| s.first).collect();
                Who { name: format!("The {} household", h.name), detail: format!("{} · {}", names.join(", "), money(h.funds)) }
            }
            None => Who { name: "A household".into(), detail: String::new() },
        },
    }
}

/// The panel: the lot picked (and what can be done there), the household bin, and every lot.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show_edit_town(
    mut commands: Commands,
    mut edit: ResMut<EditTown>,
    world: Res<crate::loading::CurrentWorld>,
    (town, story, dormant): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>),
    household: Option<Res<Household>>,
    members: Query<&crate::sim::Sim, With<crate::sim::HouseholdMember>>,
    data: Res<crate::baked::Baked>,
    mut images: ResMut<Assets<Image>>,
) {
    if !edit.dirty {
        return;
    }
    edit.dirty = false;
    if let Some(r) = edit.root.take() {
        commands.entity(r).despawn();
    }
    if !edit.open {
        return;
    }
    let town = town.as_ref().map(|t| &*t.0);
    let names: Vec<String> = members.iter().map(|s| s.first.clone()).collect();
    let ids: Vec<u64> = members.iter().map(|s| s.id).collect();
    let occupied = occupants(&world.data, town, &story, &dormant.0, household.as_ref().map(|h| h.lot_index));
    let binned = bin(town, &story, &dormant.0, &ids);
    let button = |p: &mut ChildSpawnerCommands, label: String, action: EditAction, colour: Color| {
        p.spawn((
            Button,
            action,
            EditTownUi,
            Node {
                border_radius: BorderRadius::all(Val::Px(8.0)),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(BTN_NORMAL),
        ))
        .with_children(|b| {
            b.spawn((text(label, 15.0, colour), Pickable::IGNORE));
        });
    };
    let selected = edit.selected;
    let root = commands
        .spawn((
            crate::hud::BlocksWorld,
            Interaction::default(),
            GlobalZIndex(30),
            Node {
                border_radius: BorderRadius::all(Val::Px(12.0)),
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(400.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(crate::menu::PANEL_BG),
        ))
        .with_children(|p| {
            p.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }).with_children(|r| {
                r.spawn(text(format!("Edit Town — {}", world.name), 22.0, Color::WHITE));
                button(r, "Done".into(), EditAction::Done, PLUMBOB_GREEN);
            });
            p.spawn(text("Pick a lot to see who lives there: evict them to the household bin, play them, or move a family from the bin into an empty home.", 13.0, Color::srgb(0.75, 0.85, 1.0)));
            // The lot picked.
            if let Some(i) = selected {
                let l = &world.data.lots[i];
                let name = world.data.lot_names.get(i).cloned().unwrap_or_else(|| l.internal_name.clone());
                let picture = crate::objects::cpu_texture(&data.0, s3bake::lot_thumbnail_key(l.id)).map(|img| images.add(img));
                p.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(5.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
                ))
                .with_children(|c| {
                    c.spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                        if let Some(img) = picture {
                            r.spawn((ImageNode::new(img), Node { width: Val::Px(80.0), height: Val::Px(80.0), flex_shrink: 0.0, ..default() }));
                        }
                        r.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                            t.spawn(text(name, 18.0, Color::WHITE));
                            t.spawn(text(lot_kind(&world.data, i), 13.0, Color::srgb(0.75, 0.85, 1.0)));
                        });
                    });
                    match occupied.get(&i).copied() {
                        Some(o) => {
                            let w = who(o, household.as_deref(), &names, &dormant.0, town, &story);
                            c.spawn(text(w.name, 16.0, Color::srgb(1.0, 0.9, 0.5)));
                            c.spawn(text(w.detail, 13.0, Color::WHITE));
                            if o != Occupant::Active {
                                c.spawn(Node { column_gap: Val::Px(8.0), ..default() }).with_children(|r| {
                                    button(r, "Play This Household".into(), EditAction::Play(o), PLUMBOB_GREEN);
                                    button(r, "Evict".into(), EditAction::Evict(o), Color::srgb(1.0, 0.75, 0.6));
                                });
                            }
                        }
                        None if world.data.lots[i].is_residential() && !world.data.buildings.get(&i).is_some_and(|b| b.is_penthouse()) => {
                            c.spawn(text(if binned.is_empty() { "No one lives here. (The household bin is empty.)" } else { "No one lives here. Move in:" }, 14.0, Color::WHITE));
                            for b in &binned {
                                let o = match *b {
                                    Binned::Played(k) => Occupant::Played(k),
                                    Binned::Town(id) => Occupant::Town(id),
                                };
                                let w = who(o, None, &[], &dormant.0, town, &story);
                                button(c, format!("{} ›", w.name), EditAction::MoveIn(*b, i), Color::WHITE);
                            }
                        }
                        None => {}
                    }
                });
            }
            // The bin.
            p.spawn(text(format!("Household Bin ({})", binned.len()), 18.0, Color::WHITE));
            if binned.is_empty() {
                p.spawn(text("Every family in town has a home.", 13.0, Color::srgb(0.75, 0.85, 1.0)));
            }
            for b in &binned {
                let o = match *b {
                    Binned::Played(k) => Occupant::Played(k),
                    Binned::Town(id) => Occupant::Town(id),
                };
                let w = who(o, None, &[], &dormant.0, town, &story);
                p.spawn(Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() }).with_children(|t| {
                    t.spawn(text(w.name, 15.0, Color::WHITE));
                    t.spawn(text(w.detail, 12.0, Color::srgb(0.75, 0.85, 1.0)));
                });
            }
            // Every lot: homes, then community lots.
            p.spawn(text("Lots", 18.0, Color::WHITE));
            let mut lots: Vec<usize> = (0..world.data.lots.len()).collect();
            lots.sort_by_key(|&i| (!world.data.lots[i].is_residential(), world.data.lot_names.get(i).cloned().unwrap_or_default()));
            for i in lots {
                let l = &world.data.lots[i];
                let name = world.data.lot_names.get(i).cloned().unwrap_or_else(|| l.internal_name.clone());
                let line = match occupied.get(&i) {
                    Some(o) => who(*o, household.as_deref(), &names, &dormant.0, town, &story).name,
                    None if l.is_residential() => "Empty".to_string(),
                    None => lot_kind(&world.data, i),
                };
                p.spawn((
                    Button,
                    EditAction::Select(i),
                    EditTownUi,
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(if selected == Some(i) { Color::srgb(0.22, 0.5, 0.22) } else { BTN_NORMAL }),
                ))
                .with_children(|b| {
                    b.spawn((text(name, 15.0, Color::WHITE), Pickable::IGNORE));
                    b.spawn((text(line, 12.0, Color::srgb(0.75, 0.85, 1.0)), Pickable::IGNORE));
                });
            }
        })
        .id();
    edit.root = Some(root);
}

fn button_visuals(mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<EditTownUi>, With<Button>)>) {
    for (i, mut bg) in &mut q {
        bg.0 = match i {
            Interaction::Pressed => BTN_PRESS,
            Interaction::Hovered => BTN_HOVER,
            Interaction::None => BTN_NORMAL,
        };
    }
}

/// What the panel's buttons do.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn edit_town_buttons(
    mut commands: Commands,
    q: Query<(&Interaction, &EditAction), Changed<Interaction>>,
    mut edit: ResMut<EditTown>,
    world: Res<crate::loading::CurrentWorld>,
    (town, mut story, mut dormant, catalog): (Option<Res<crate::premade::TownPremades>>, ResMut<crate::story::TownStory>, ResMut<crate::household::Dormant>, Res<crate::loading::Catalog>),
    mut clock: ResMut<crate::clock::GameClock>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut notes: ResMut<Notifications>,
    mut snapshot: MessageWriter<crate::save::SnapshotRequest>,
) {
    let Some(&action) = q.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, a)| a) else { return };
    let town_name = |id: u64| town.as_ref().and_then(|t| t.0.households.iter().find(|h| h.id == id)).map_or(String::new(), |h| h.name.clone());
    let lot_name = |i: usize| world.data.lot_names.get(i).cloned().unwrap_or_default();
    let close = |edit: &mut EditTown, clock: &mut crate::clock::GameClock, cams: &mut Query<&mut crate::camera::SimsCamera>| {
        edit.open = false;
        edit.dirty = true;
        clock.speed = edit.speed;
        if let (Some((focus, distance, pitch, yaw)), Ok(mut c)) = (edit.camera.take(), cams.single_mut()) {
            c.look_at(focus);
            c.distance = distance;
            c.pitch = pitch;
            c.yaw = yaw;
        }
    };
    match action {
        EditAction::Select(i) => {
            edit.selected = Some(i);
            edit.dirty = true;
            if let Ok(mut c) = cams.single_mut() {
                let l = &world.data.lots[i];
                c.look_at(crate::home::lot_center(l));
                c.distance = (l.width.max(l.depth) as f32 * 1.6).max(45.0);
                c.pitch = 0.85;
            }
        }
        EditAction::Done => close(&mut edit, &mut clock, &mut cams),
        EditAction::Play(o) => {
            let choice = match o {
                Occupant::Played(i) => crate::household::Choice::Played(i),
                Occupant::Town(id) => crate::household::Choice::Town(id),
                Occupant::Active => return,
            };
            close(&mut edit, &mut clock, &mut cams);
            commands.insert_resource(crate::household::Switching(choice));
            snapshot.write(crate::save::SnapshotRequest);
        }
        EditAction::Evict(o) => {
            let at = edit.selected.map(lot_name).unwrap_or_default();
            match o {
                Occupant::Town(id) => {
                    story.homes.insert(id, None);
                    notes.push(format!("The {} household moved out of {at}: they're in the household bin.", town_name(id)));
                }
                Occupant::Played(i) => {
                    let Some(d) = dormant.0.get_mut(i) else { return };
                    // (Their furniture sold, the house left as it stands.)
                    let refund: i64 = d.bought.iter().filter_map(|o| catalog.by_key(&o.objd)).map(|e| e.price.max(0) as i64 * 4 / 5).sum();
                    d.funds += refund;
                    for v in [&mut d.bought, &mut d.removed] {
                        v.clear();
                    }
                    d.paint.clear();
                    d.terrain.clear();
                    d.heights.clear();
                    d.plants.clear();
                    d.graves.clear();
                    d.homeless = true;
                    notes.push(format!("The {} household moved out of {at} (their furniture sold for §{refund}): they're in the household bin.", d.household));
                }
                Occupant::Active => return,
            }
            edit.dirty = true;
        }
        EditAction::MoveIn(b, lot) => {
            let l = &world.data.lots[lot];
            match b {
                Binned::Town(id) => {
                    story.homes.insert(id, Some(l.id));
                    notes.push(format!("The {} household moved into {}.", town_name(id), lot_name(lot)));
                }
                Binned::Played(i) => {
                    let Some(d) = dormant.0.get_mut(i) else { return };
                    d.homeless = false;
                    d.lot_index = lot;
                    d.lot_name = lot_name(lot);
                    let c = crate::home::lot_center(l);
                    let y = world.data.heightmap.sample(c.x, c.z);
                    for s in d.sims.iter_mut().filter(|s| s.member) {
                        s.position = [c.x, y, c.z];
                        s.floor = 1;
                        s.whereabouts = "home".into();
                    }
                    notes.push(format!("The {} household moved into {}.", d.household, d.lot_name));
                }
            }
            edit.dirty = true;
        }
    }
}

/// EDIT_TOWN=<step>;<step>...: Edit Town worked through by its own buttons, a step every three
/// seconds from ten seconds in (tests): `open`, `select:<lot name part>`, `evict`, `play`,
/// `movein:<household name part>`, `done`.
#[allow(clippy::too_many_arguments)]
fn scripted(
    time: Res<Time>,
    mut step: Local<(usize, f32)>,
    mut open: MessageWriter<OpenEditTown>,
    mut buttons: Query<(&EditAction, &mut Interaction)>,
    world: Res<crate::loading::CurrentWorld>,
    (town, dormant): (Option<Res<crate::premade::TownPremades>>, Res<crate::household::Dormant>),
    edit: Res<EditTown>,
) {
    let Ok(script) = std::env::var("EDIT_TOWN") else { return };
    let steps: Vec<&str> = script.split(';').collect();
    let now = time.elapsed_secs();
    if now < 10.0 || now - step.1 < 3.0 || step.0 >= steps.len() {
        return;
    }
    step.1 = now;
    let s = steps[step.0];
    step.0 += 1;
    info!("edit town test: {s}");
    let lot_named = |part: &str, i: usize| {
        let part = part.to_ascii_lowercase();
        world.data.lot_names.get(i).is_some_and(|n| n.to_ascii_lowercase().contains(&part)) || world.data.lots[i].internal_name.to_ascii_lowercase().contains(&part)
    };
    let household_named = |part: &str, b: Binned| {
        let name = match b {
            Binned::Played(i) => dormant.0.get(i).map(|d| d.household.clone()),
            Binned::Town(id) => town.as_ref().and_then(|t| t.0.households.iter().find(|h| h.id == id)).map(|h| h.name.clone()),
        };
        name.is_some_and(|n| n.to_ascii_lowercase().contains(&part.to_ascii_lowercase()))
    };
    let (verb, arg) = s.split_once(':').unwrap_or((s, ""));
    if verb == "open" {
        open.write(OpenEditTown);
        return;
    }
    let _ = edit.is_open();
    for (a, mut i) in &mut buttons {
        let hit = match (verb, a) {
            ("select", EditAction::Select(l)) => lot_named(arg, *l),
            ("evict", EditAction::Evict(_)) | ("play", EditAction::Play(_)) | ("done", EditAction::Done) => true,
            ("movein", EditAction::MoveIn(b, _)) => household_named(arg, *b),
            _ => false,
        };
        if hit {
            *i = Interaction::Pressed;
            return;
        }
    }
    warn!("edit town test: nothing to press for {s}");
}

/// The lots outlined while Edit Town is open: the household's home gold, homes green, empty
/// homes white, community lots blue, the one picked yellow.
fn draw_town_lots(
    mut gizmos: Gizmos,
    edit: Res<EditTown>,
    world: Res<crate::loading::CurrentWorld>,
    (town, story, dormant): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>),
    household: Option<Res<Household>>,
) {
    if !edit.open {
        return;
    }
    let occupied = occupants(&world.data, town.as_ref().map(|t| &*t.0), &story, &dormant.0, household.as_ref().map(|h| h.lot_index));
    for (i, lot) in world.data.lots.iter().enumerate() {
        let colour = if edit.selected == Some(i) {
            Color::srgb(1.0, 1.0, 0.2)
        } else {
            match occupied.get(&i) {
                Some(Occupant::Active) => Color::srgb(1.0, 0.75, 0.2),
                Some(_) => Color::srgb(0.3, 1.0, 0.3),
                None if lot.is_residential() => Color::WHITE,
                None => Color::srgb(0.3, 0.6, 1.0),
            }
        };
        let rot = Quat::from_rotation_y(lot.rotation);
        let corner = Vec3::from(lot.corner);
        let pts = [(0.0, 0.0), (lot.width as f32, 0.0), (lot.width as f32, lot.depth as f32), (0.0, lot.depth as f32)].map(|(x, z)| {
            let p = corner + rot * Vec3::new(x, 0.0, z);
            Vec3::new(p.x, world.data.heightmap.sample(p.x, p.z) + 1.0, p.z)
        });
        for k in 0..4 {
            gizmos.line(pts[k], pts[(k + 1) % 4], colour);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homes_by_the_story() {
        let mut story = crate::story::TownStory::default();
        let h = s3bake::HouseholdBaked { id: 7, lot_id: 99, ..Default::default() };
        assert_eq!(story.home_of(&h), Some(99));
        story.homes.insert(7, None);
        assert_eq!(story.home_of(&h), None);
        story.homes.insert(7, Some(5));
        assert_eq!(story.home_of(&h), Some(5));
        let homeless = s3bake::HouseholdBaked { id: 8, lot_id: 0, ..Default::default() };
        assert_eq!(story.home_of(&homeless), None);
        // (Townies with no home aren't in the bin; a family evicted is.)
        assert!(!story.evicted(&homeless));
        story.homes.insert(7, None);
        assert!(story.evicted(&h));
        // (Old saves have no record of where anyone lives.)
        let old: crate::story::TownStory = serde_json::from_str(r#"{"day":3,"sims":{},"born":[],"news":[]}"#).unwrap();
        assert!(old.homes.is_empty());
    }
}
