//! Live-mode user interface: needs panel, clock and speed, action queue, household portraits,
//! notifications, and the pie menu for interacting with the world.

use bevy::input::mouse::MouseButton;
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::SimsCamera;
use crate::clock::GameClock;
use crate::interact::*;
use crate::loading::CurrentWorld;
use crate::menu::{BTN_HOVER, BTN_NORMAL, BTN_PRESS, PLUMBOB_GREEN, text};
use crate::sim::*;
use crate::{AppState, PlayMode};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PieMenu>()
            .add_systems(OnEnter(PlayMode::Live), spawn_hud)
            .add_systems(Update, (floor_controls, update_moodlets_panel, phone_button, save_button, update_wishes_panel, wish_buttons).run_if(in_state(PlayMode::Live)))
            .add_systems(
                Update,
                (
                    pointer_over_ui,
                    world_click,
                    pie_buttons,
                    hud_buttons,
                    update_needs_panel,
                    update_clock_panel,
                    update_queue_panel,
                    update_members_panel,
                    update_notifications,
                    keyboard_shortcuts,
                    hud_button_visuals,
                    update_fps,
                )
                    .chain()
                    .run_if(in_state(PlayMode::Live)),
            );
    }
}

const PANEL_BG: Color = Color::srgba(0.05, 0.15, 0.30, 0.88);

#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);

#[derive(Resource, Default)]
pub struct PieMenu {
    pub root: Option<Entity>,
    pub actor: Option<Entity>,
    pub options: Vec<(String, ActionKind)>,
    /// Second-level menus (social categories), opened by `PIE_SUBMENU` options.
    pub submenus: Vec<(String, Vec<(String, ActionKind)>)>,
    pub at: Vec2,
}

/// Marks a pie option that opens `submenus[i]` (`ActionKind::GoHere(NaN, i)`).
fn submenu_kind(i: usize) -> ActionKind {
    ActionKind::GoHere(Vec2::new(f32::NAN, f32::INFINITY), i as u8)
}

fn as_submenu(k: &ActionKind) -> Option<usize> {
    match k {
        ActionKind::GoHere(p, i) if p.x.is_nan() && p.y.is_infinite() => Some(*i as usize),
        _ => None,
    }
}

#[derive(Component)]
struct PieOption(usize);

#[derive(Component)]
struct NeedsName;
/// The selected Sim's moodlets (above the needs panel).
#[derive(Component)]
struct MoodletsPanel;
#[derive(Component)]
struct TraitsText;
#[derive(Component)]
struct WishesPanel;
/// An offered wish button (index into `Wishes::offered`).
#[derive(Component)]
struct WishButton(usize);
#[derive(Component)]
struct RewardsButton;
#[derive(Component)]
struct NeedsDetail;
#[derive(Component)]
struct MotiveBar(usize);
#[derive(Component)]
struct ClockText;
#[derive(Component)]
struct FundsText;
#[derive(Component)]
struct SpeedButton(usize);

/// Up / down a floor of the house (+1 / -1).
#[derive(Component)]
struct FloorButton(i8);

#[derive(Component)]
struct FloorText;

#[derive(Component)]
struct PhoneButton;

#[derive(Component)]
struct SaveButton;

/// The floor controls, shown only in a house with more than one floor.
#[derive(Component)]
struct FloorControls;
#[derive(Component)]
struct QueuePanel;
#[derive(Component)]
struct QueueButton(usize);
#[derive(Component)]
struct MembersPanel;
#[derive(Component)]
struct MemberButton(Entity);
#[derive(Component)]
struct NotesPanel;
#[derive(Component)]
struct HudButton;
#[derive(Component)]
struct FpsText;

fn update_fps(time: Res<Time>, mut avg: Local<f32>, mut q: Query<&mut Text, With<FpsText>>) {
    let dt = time.delta_secs().max(1e-4);
    *avg = if *avg == 0.0 { dt } else { *avg * 0.95 + dt * 0.05 };
    if let Ok(mut t) = q.single_mut() {
        t.0 = format!("{:.0} fps", 1.0 / *avg);
    }
}
/// UI regions that should swallow world clicks.
#[derive(Component)]
struct BlocksWorld;

fn panel(node: Node) -> impl Bundle {
    (
        Node { border_radius: BorderRadius::all(Val::Px(12.0)), ..node },
        BackgroundColor(PANEL_BG),
        Interaction::default(),
        BlocksWorld,
    )
}

fn spawn_hud(mut commands: Commands) {
    commands.insert_resource(PointerOverUi::default());
    // Needs panel (bottom-left)
    commands
        .spawn((
            DespawnOnExit(AppState::InGame),
            panel(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(390.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            }),
        ))
        .with_children(|p| {
            p.spawn((text("", 22.0, Color::WHITE), NeedsName));
            p.spawn((text("", 14.0, Color::srgb(0.75, 0.85, 1.0)), NeedsDetail));
            p.spawn((text("", 13.0, Color::srgb(0.95, 0.85, 0.55)), TraitsText));
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(10.0),
                row_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|g| {
                for (i, name) in MOTIVE_NAMES.iter().enumerate() {
                    g.spawn(Node { width: Val::Px(173.0), flex_direction: FlexDirection::Column, ..default() })
                        .with_children(|c| {
                            c.spawn(text(*name, 14.0, Color::WHITE));
                            c.spawn((
                                Node { width: Val::Percent(100.0), height: Val::Px(12.0), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() },
                                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                            ))
                            .with_children(|b| {
                                b.spawn((
                                    Node { width: Val::Percent(50.0), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() },
                                    BackgroundColor(Color::srgb(0.3, 0.9, 0.3)),
                                    MotiveBar(i),
                                ));
                            });
                        });
                }
            });
        });

    // Wishes and moodlets (above the needs panel)
    commands
        .spawn((
            DespawnOnExit(AppState::InGame),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(250.0),
                width: Val::Px(390.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            c.spawn((
                WishesPanel,
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() },
            ));
            c.spawn((
                MoodletsPanel,
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() },
                Pickable::IGNORE,
            ));
        });

    // Clock and speed (bottom-centre)
    commands
        .spawn((
            DespawnOnExit(AppState::InGame),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                left: Val::Px(412.0),
                right: Val::Px(214.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            row.spawn(panel(Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                ..default()
            }))
            .with_children(|p| {
                p.spawn((
                    Button,
                    HudButton,
                    PhoneButton,
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        width: Val::Px(66.0),
                        height: Val::Px(34.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::right(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text("Phone", 16.0, Color::WHITE));
                });
                p.spawn((
                    Button,
                    HudButton,
                    SaveButton,
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        width: Val::Px(58.0),
                        height: Val::Px(34.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::right(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text("Save", 16.0, Color::WHITE));
                });
                p.spawn((text("", 20.0, Color::WHITE), ClockText, Node { width: Val::Px(210.0), ..default() }));
                for (i, label) in ["II", ">", ">>", ">>>"].iter().enumerate() {
                    p.spawn((
                        Button,
                        HudButton,
                        SpeedButton(i),
                        Node {
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            width: Val::Px(46.0),
                            height: Val::Px(34.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|b| {
                        b.spawn(text(*label, 18.0, Color::WHITE));
                    });
                }
                p.spawn((text("", 20.0, PLUMBOB_GREEN), FundsText, Node { margin: UiRect::left(Val::Px(12.0)), ..default() }));
                p.spawn((Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, margin: UiRect::left(Val::Px(12.0)), display: Display::None, ..default() }, FloorControls))
                    .with_children(|f| {
                        f.spawn((text("", 16.0, Color::WHITE), FloorText, Node { width: Val::Px(62.0), ..default() }));
                        for (step, label) in [(1i8, "Up"), (-1, "Down")] {
                            f.spawn((
                                Button,
                                HudButton,
                                FloorButton(step),
                                Node {
                                    border_radius: BorderRadius::all(Val::Px(8.0)),
                                    width: Val::Px(58.0),
                                    height: Val::Px(34.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(BTN_NORMAL),
                            ))
                            .with_children(|b| {
                                b.spawn(text(label, 16.0, Color::WHITE));
                            });
                        }
                    });
            });
        });

    // Action queue (top-left)
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        QueuePanel,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            top: Val::Px(12.0),
            column_gap: Val::Px(6.0),
            ..default()
        },
    ));

    // Household members (right)
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        MembersPanel,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            bottom: Val::Px(12.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },
    ));

    // Notifications (top-right)
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        NotesPanel,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            top: Val::Px(12.0),
            width: Val::Px(360.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },
    ));

    // Frame rate (top centre)
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        FpsText,
        text("", 13.0, Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Node { position_type: PositionType::Absolute, top: Val::Px(6.0), left: Val::Percent(48.0), ..default() },
    ));

    // Help line
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        text(
            "Click objects / sims for actions · Click ground to walk · WASD/arrows pan · Q/E rotate · wheel zoom · Space pause · 1/2/3 speed · Tab next sim · C centre camera · PgUp/PgDn floors · B buy mode",
            13.0,
            Color::srgba(1.0, 1.0, 1.0, 0.75),
        ),
        Node { position_type: PositionType::Absolute, left: Val::Px(420.0), right: Val::Px(240.0), bottom: Val::Px(70.0), ..default() },
    ));
}

fn pointer_over_ui(mut over: ResMut<PointerOverUi>, q: Query<&Interaction, Or<(With<BlocksWorld>, With<Button>)>>) {
    over.0 = q.iter().any(|i| *i != Interaction::None);
}

fn close_pie(commands: &mut Commands, pie: &mut PieMenu) {
    if let Some(r) = pie.root.take() {
        commands.entity(r).despawn();
    }
    pie.options.clear();
}

fn open_pie(commands: &mut Commands, pie: &mut PieMenu, at: Vec2, title: &str, actor: Entity, options: Vec<(String, ActionKind)>) {
    close_pie(commands, pie);
    if options.is_empty() {
        return;
    }
    let n = options.len();
    let radius = 70.0 + n as f32 * 9.0;
    let root = commands
        .spawn((
            DespawnOnExit(AppState::InGame),
            Node { position_type: PositionType::Absolute, left: Val::Px(at.x), top: Val::Px(at.y), ..default() },
            GlobalZIndex(10),
        ))
        .with_children(|p| {
            p.spawn((
                text(title, 16.0, Color::WHITE),
                Node { position_type: PositionType::Absolute, left: Val::Px(-60.0), top: Val::Px(-10.0), width: Val::Px(120.0), ..default() },
                TextShadow::default(),
            ));
            for (i, (label, _)) in options.iter().enumerate() {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
                let (x, y) = (a.cos() * radius, a.sin() * radius * 0.8);
                p.spawn((
                    Button,
                    HudButton,
                    PieOption(i),
                    Node {
                        border_radius: BorderRadius::all(Val::Px(17.0)),
                        position_type: PositionType::Absolute,
                        left: Val::Px(x - 85.0),
                        top: Val::Px(y - 17.0),
                        width: Val::Px(170.0),
                        height: Val::Px(34.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text(label.clone(), 15.0, Color::WHITE));
                });
            }
        })
        .id();
    pie.root = Some(root);
    pie.actor = Some(actor);
    pie.options = options;
}

fn ancestor_with<F: Fn(Entity) -> bool>(mut e: Entity, parents: &Query<&ChildOf>, pred: F) -> Option<Entity> {
    for _ in 0..8 {
        if pred(e) {
            return Some(e);
        }
        e = parents.get(e).ok()?.parent();
    }
    None
}

/// Ray-marches the heightmap to find where a ray meets the ground.
/// Where a click lands: on a house floor (the viewed one first, then those below) or the ground.
pub fn floor_hit(ray: Ray3d, world: &CurrentWorld, building: Option<&crate::building::ActiveBuilding>) -> Option<(Vec3, u8)> {
    if let Some(b) = building {
        for level in (1..=b.view_level).rev() {
            let y = b.levels[level as usize];
            if ray.direction.y.abs() < 1e-4 {
                break;
            }
            let t = (y - ray.origin.y) / ray.direction.y;
            if t > 0.0 {
                let p = ray.origin + *ray.direction * t;
                if b.floor_y(level, p).is_some() {
                    return Some((p, level));
                }
            }
        }
    }
    ground_hit(ray, world).map(|p| (p, 1))
}

pub fn ground_hit(ray: Ray3d, world: &CurrentWorld) -> Option<Vec3> {
    let hm = &world.data.heightmap;
    let mut t: f32 = 0.0;
    let mut prev = ray.origin;
    while t < 3000.0 {
        let step = (t * 0.01).max(0.25);
        t += step;
        let p = ray.origin + *ray.direction * t;
        let g = hm.sample(p.x, p.z);
        if p.y <= g {
            // Refine by bisection.
            let (mut a, mut b) = (prev, p);
            for _ in 0..12 {
                let m = (a + b) * 0.5;
                if m.y <= hm.sample(m.x, m.z) {
                    b = m;
                } else {
                    a = m;
                }
            }
            return Some(b);
        }
        prev = p;
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn world_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    over_ui: Res<PointerOverUi>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<SimsCamera>>,
    mut ray_cast: MeshRayCast,
    parents: Query<&ChildOf>,
    sims: Query<(&Sim, Has<HouseholdMember>, Has<Selected>)>,
    objects: Query<&GameObject>,
    selected: Query<(Entity, &Relationships, &Sim), With<Selected>>,
    members_q: Query<(), With<HouseholdMember>>,
    world: Res<CurrentWorld>,
    mut pie: ResMut<PieMenu>,
    buy: Option<Res<crate::buy::BuyMode>>,
    building: Option<Res<crate::building::ActiveBuilding>>,
) {
    if buy.is_some_and(|b| b.active) {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        close_pie(&mut commands, &mut pie);
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) || over_ui.0 {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    close_pie(&mut commands, &mut pie);
    let Ok((actor, rels, actor_sim)) = selected.single() else { return };

    let is_target = |e: Entity| sims.contains(e) || objects.contains(e);
    let filter = |e: Entity| ancestor_with(e, &parents, is_target).is_some();
    let hits = ray_cast.cast_ray(ray, &MeshRayCastSettings::default().with_filter(&filter));
    let target = hits.first().and_then(|(e, _)| ancestor_with(*e, &parents, is_target));

    if let Some(t) = target {
        if let Ok((sim, member, is_sel)) = sims.get(t) {
            if is_sel {
                return;
            }
            let mut options = Vec::new();
            if member {
                options.push((format!("Select {}", sim.first), ActionKind::GoHere(Vec2::NAN, 1)));
            }
            let rel = rels.get(t);
            let target_member = members_q.contains(t);
            pie.submenus.clear();
            for cat in crate::social::SocialCat::ALL {
                let list: Vec<(String, ActionKind)> = SOCIALS
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.cat == cat && crate::social::available(s, &rel, actor_sim, sim, target_member))
                    .map(|(i, s)| (s.name.to_string(), ActionKind::Social { target: t, social: i }))
                    .collect();
                if !list.is_empty() {
                    options.push((format!("{} ›", cat.name()), submenu_kind(pie.submenus.len())));
                    pie.submenus.push((cat.name().to_string(), list));
                }
            }
            let title = format!("{} ({})", sim.full_name(), rel.label());
            pie.at = cursor;
            open_pie(&mut commands, &mut pie, cursor, &title, actor, options);
            // Remember which sim "Select" refers to.
            pie.options.iter_mut().for_each(|(l, k)| {
                if l.starts_with("Select") {
                    *k = ActionKind::Social { target: t, social: usize::MAX };
                }
            });
        } else if let Ok(obj) = objects.get(t) {
            let mut options: Vec<(String, ActionKind)> = Vec::new();
            let usable = if obj.kind.usable_by(actor_sim.age) { interactions_for(obj.kind) } else { &[] };
            for (i, d) in usable.iter().enumerate() {
                if d.special == Special::FindJob {
                    // Job listings: every career's entry-level position.
                    for (k, c) in crate::careers::CAREERS.iter().enumerate() {
                        let l = &c.levels[0];
                        options.push((
                            format!("Join {}: {} §{}/hr", c.name, l.title, l.hourly),
                            ActionKind::JoinCareer { target: t, track: k },
                        ));
                    }
                } else {
                    options.push((d.name.to_string(), ActionKind::Object { target: t, def: i }));
                }
            }
            open_pie(&mut commands, &mut pie, cursor, &obj.name, actor, options);
        }
    } else if actor_sim.age == crate::sim::Age::Baby {
        // Babies stay put.
    } else if let Some((p, level)) = floor_hit(ray, &world, building.as_deref()) {
        let mut options = vec![("Go Here".to_string(), ActionKind::GoHere(Vec2::new(p.x, p.z), level))];
        let mut title = String::new();
        // A community lot: its rabbit hole's activities.
        if let Some(lot) = crate::rabbitholes::lot_at(&world.data.lots, p) {
            let l = &world.data.lots[lot];
            let acts = crate::rabbitholes::activities(l);
            if !acts.is_empty() {
                title = crate::rabbitholes::lot_title(l, world.data.lot_names.get(lot).map_or("", |s| s.as_str()));
                options.clear();
                for (i, a) in acts.iter().enumerate() {
                    let cost = match a.cost {
                        c if c > 0 => format!(" (§{c})"),
                        c if c < 0 => format!(" (earn §{})", -c),
                        _ => String::new(),
                    };
                    options.push((format!("{}{cost}", a.name), ActionKind::Visit { lot, activity: i }));
                }
            }
        }
        open_pie(&mut commands, &mut pie, cursor, &title, actor, options);
    }
}

fn pie_buttons(
    mut commands: Commands,
    q: Query<(&Interaction, &PieOption), Changed<Interaction>>,
    mut pie: ResMut<PieMenu>,
    mut queues: Query<&mut ActionQueue>,
    selected: Query<Entity, With<Selected>>,
    (mut wishes, mut notes): (Query<(&Sim, &mut crate::wishes::Wishes), With<Selected>>, ResMut<Notifications>),
) {
    let mut chosen = None;
    for (i, opt) in &q {
        if *i == Interaction::Pressed {
            chosen = Some(opt.0);
        }
    }
    let Some(idx) = chosen else { return };
    let Some((label, kind)) = pie.options.get(idx).cloned() else { return };
    let actor = pie.actor;
    close_pie(&mut commands, &mut pie);
    if let ActionKind::BuyReward(i) = kind {
        if let Ok((sim, mut w)) = wishes.single_mut() {
            let r = crate::wishes::Reward::ALL[i];
            if w.points >= r.cost() && !w.has_reward(r) {
                w.points -= r.cost();
                w.rewards.push(r);
                notes.push(format!("{} gained the {} lifetime reward!", sim.first, r.name()));
            }
        }
        return;
    }
    if let Some(i) = as_submenu(&kind)
        && let (Some((title, list)), Some(a)) = (pie.submenus.get(i).cloned(), actor)
    {
        let at = pie.at;
        open_pie(&mut commands, &mut pie, at, &title, a, list);
        return;
    }
    if let ActionKind::Social { target, social } = kind
        && social == usize::MAX
    {
        for s in &selected {
            commands.entity(s).remove::<Selected>();
        }
        commands.entity(target).insert(Selected);
        return;
    }
    if let Some(a) = actor
        && let Ok(mut queue) = queues.get_mut(a)
    {
        queue.push_player(Action::new(label, kind, false));
    }
}

#[allow(clippy::too_many_arguments)]
fn hud_buttons(
    mut commands: Commands,
    speed: Query<(&Interaction, &SpeedButton), Changed<Interaction>>,
    queue_btns: Query<(&Interaction, &QueueButton), Changed<Interaction>>,
    member_btns: Query<(&Interaction, &MemberButton), Changed<Interaction>>,
    mut clock: ResMut<GameClock>,
    mut queues: Query<&mut ActionQueue, With<Selected>>,
    selected: Query<Entity, With<Selected>>,
    positions: Query<&Transform, With<Sim>>,
    mut cam: Query<&mut SimsCamera>,
) {
    for (i, s) in &speed {
        if *i == Interaction::Pressed {
            clock.set_speed(s.0);
        }
    }
    for (i, b) in &queue_btns {
        if *i == Interaction::Pressed
            && let Ok(mut q) = queues.single_mut()
            && let Some(a) = q.0.get_mut(b.0)
        {
            a.cancel = true;
        }
    }
    for (i, b) in &member_btns {
        if *i == Interaction::Pressed {
            let already = selected.contains(b.0);
            for s in &selected {
                commands.entity(s).remove::<Selected>();
            }
            commands.entity(b.0).insert(Selected);
            if already
                && let (Ok(tf), Ok(mut c)) = (positions.get(b.0), cam.single_mut())
            {
                c.look_at(tf.translation);
            }
        }
    }
}

/// Floor buttons step the viewed floor; the label shows which floor is in view.
fn floor_controls(
    building: Option<ResMut<crate::building::ActiveBuilding>>,
    buttons: Query<(&Interaction, &FloorButton), Changed<Interaction>>,
    mut label: Query<&mut Text, With<FloorText>>,
    mut controls: Query<&mut Node, With<FloorControls>>,
) {
    let Some(mut b) = building else {
        for mut n in &mut controls {
            n.display = Display::None;
        }
        return;
    };
    for (i, f) in &buttons {
        if *i == Interaction::Pressed {
            b.view_level = (b.view_level as i8 + f.0).clamp(1, b.top_level as i8) as u8;
        }
    }
    for mut n in &mut controls {
        let want = if b.top_level > 1 { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
    }
    let s = format!("Floor {}", b.view_level);
    for mut t in &mut label {
        if t.0 != s {
            t.0 = s.clone();
        }
    }
}

fn hud_button_visuals(mut q: Query<(&Interaction, &mut BackgroundColor, Option<&SpeedButton>), With<HudButton>>, clock: Res<GameClock>) {
    for (i, mut bg, speed) in &mut q {
        let active = speed.is_some_and(|s| s.0 == clock.speed);
        bg.0 = match i {
            Interaction::Pressed => BTN_PRESS,
            Interaction::Hovered => BTN_HOVER,
            Interaction::None if active => Color::srgb(0.25, 0.6, 0.2),
            Interaction::None => BTN_NORMAL,
        };
    }
}

fn motive_color(v: f32) -> Color {
    let t = ((v + 100.0) / 200.0).clamp(0.0, 1.0);
    if t > 0.5 {
        Color::srgb(0.95 - (t - 0.5) * 1.3, 0.85, 0.2)
    } else {
        Color::srgb(0.9, 0.2 + t * 1.3, 0.15)
    }
}

#[allow(clippy::type_complexity)]
fn update_needs_panel(
    sel: Query<(&Sim, &Motives, &Skills, Option<&Job>, Option<&AtWork>, &ActionQueue, &crate::life::Mood), With<Selected>>,
    away_q: Query<(Option<&crate::rabbitholes::AtRabbitHole>, Option<&crate::rabbitholes::SchoolGrades>), With<Selected>>,
    mut name: Query<&mut Text, (With<NeedsName>, Without<NeedsDetail>, Without<TraitsText>)>,
    mut detail: Query<&mut Text, (With<NeedsDetail>, Without<NeedsName>, Without<TraitsText>)>,
    mut traits: Query<&mut Text, (With<TraitsText>, Without<NeedsName>, Without<NeedsDetail>)>,
    mut bars: Query<(&MotiveBar, &mut Node, &mut BackgroundColor)>,
) {
    let Ok((sim, motives, skills, job, at_work, _, mood)) = sel.single() else { return };
    let (away, grades) = away_q.single().unwrap_or((None, None));
    if let Ok(mut t) = traits.single_mut() {
        let s = sim.traits.iter().map(|t| t.name()).collect::<Vec<_>>().join(" · ");
        if t.0 != s {
            t.0 = s;
        }
    }
    if let Ok(mut t) = name.single_mut() {
        let s = format!("{} — {}", sim.full_name(), mood.label());
        if t.0 != s {
            t.0 = s;
        }
    }
    if let Ok(mut t) = detail.single_mut() {
        let job_s = match (job, at_work) {
            (Some(j), Some(_)) => format!("At work: {}", j.describe()),
            (Some(j), None) => format!(
                "{} · {}–{} · performance {:+.0}",
                j.describe(),
                hour_label(j.info().start),
                hour_label(j.info().end),
                j.performance
            ),
            _ if grades.is_some() => format!("School grade: {}", grades.map_or("C", |g| g.letter())),
            _ => match sim.age {
                crate::sim::Age::Baby => "Baby".into(),
                crate::sim::Age::Toddler => "Toddler".into(),
                crate::sim::Age::Elder => "Retired".into(),
                _ => "Unemployed".into(),
            },
        };
        let job_s = match away {
            Some(a) => format!("Away: {} ({})", a.place, a.activity.name),
            None => job_s,
        };
        let mut sk: Vec<String> = skills.0.iter().filter(|(_, v)| **v >= 1.0).map(|(k, v)| format!("{k} {}", *v as u32)).collect();
        sk.sort();
        let s = if sk.is_empty() { job_s } else { format!("{job_s} · Skills: {}", sk.join(", ")) };
        if t.0 != s {
            t.0 = s;
        }
    }
    for (bar, mut node, mut bg) in &mut bars {
        let v = motives.0[bar.0];
        node.width = Val::Percent(((v + 100.0) / 2.0).clamp(2.0, 100.0));
        bg.0 = motive_color(v);
    }
}

fn update_clock_panel(
    clock: Res<GameClock>,
    household: Option<Res<Household>>,
    mut t: Query<&mut Text, (With<ClockText>, Without<FundsText>)>,
    mut f: Query<&mut Text, (With<FundsText>, Without<ClockText>)>,
) {
    if let Ok(mut t) = t.single_mut() {
        let s = format!("{} {}", clock.weekday_name(), clock.time_string());
        if t.0 != s {
            t.0 = s;
        }
    }
    if let (Ok(mut f), Some(h)) = (f.single_mut(), household) {
        let s = format!("§{}", h.funds);
        if f.0 != s {
            f.0 = s;
        }
    }
}

fn update_queue_panel(
    mut commands: Commands,
    panel: Query<Entity, With<QueuePanel>>,
    sel: Query<&ActionQueue, With<Selected>>,
    mut last: Local<Vec<String>>,
) {
    let Ok(p) = panel.single() else { return };
    let labels: Vec<String> = sel.single().map(|q| q.0.iter().filter(|a| !a.cancel).map(|a| a.label.clone()).collect()).unwrap_or_default();
    if *last == labels {
        return;
    }
    *last = labels.clone();
    commands.entity(p).despawn_children();
    commands.entity(p).with_children(|c| {
        for (i, l) in labels.iter().enumerate() {
            c.spawn((
                Button,
                HudButton,
                QueueButton(i),
                Node {
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::all(if i == 0 { PLUMBOB_GREEN } else { Color::WHITE }),
                BackgroundColor(BTN_NORMAL),
            ))
            .with_children(|b| {
                b.spawn(text(l.clone(), 15.0, Color::WHITE));
            });
        }
    });
}

fn update_members_panel(
    mut commands: Commands,
    panel: Query<Entity, With<MembersPanel>>,
    members: Query<(Entity, &Sim, &crate::life::Mood, Has<Selected>), With<HouseholdMember>>,
    mut last: Local<Vec<(Entity, bool, u8)>>,
) {
    let Ok(p) = panel.single() else { return };
    let mut list: Vec<(Entity, String, bool, f32)> = members.iter().map(|(e, s, m, sel)| (e, s.first.clone(), sel, m.level())).collect();
    list.sort_by_key(|x| x.0);
    let sig: Vec<(Entity, bool, u8)> = list.iter().map(|x| (x.0, x.2, ((x.3 + 100.0) / 50.0) as u8)).collect();
    if *last == sig {
        return;
    }
    *last = sig;
    commands.entity(p).despawn_children();
    commands.entity(p).with_children(|c| {
        for (e, name, sel, mood) in list {
            c.spawn((
                Button,
                HudButton,
                MemberButton(e),
                Node {
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    width: Val::Px(150.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(if sel { 3.0 } else { 1.0 })),
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(if sel { PLUMBOB_GREEN } else { Color::srgba(1.0, 1.0, 1.0, 0.4) }),
                BackgroundColor(BTN_NORMAL),
            ))
            .with_children(|b| {
                b.spawn((
                    Node { width: Val::Px(14.0), height: Val::Px(14.0), border_radius: BorderRadius::all(Val::Px(7.0)), ..default() },
                    BackgroundColor(mood_color(mood)),
                ));
                b.spawn(text(name, 17.0, Color::WHITE));
            });
        }
    });
}

fn update_notifications(
    mut commands: Commands,
    time: Res<Time>,
    mut notes: ResMut<Notifications>,
    panel: Query<Entity, With<NotesPanel>>,
    mut last: Local<usize>,
) {
    for n in notes.0.iter_mut() {
        n.1 -= time.delta_secs();
    }
    let before = notes.0.len();
    notes.0.retain(|n| n.1 > 0.0);
    let Ok(p) = panel.single() else { return };
    let sig = notes.0.iter().map(|n| n.0.len()).sum::<usize>() + notes.0.len() * 1000;
    if sig == *last && before == notes.0.len() {
        return;
    }
    *last = sig;
    commands.entity(p).despawn_children();
    commands.entity(p).with_children(|c| {
        for (msg, _) in notes.0.iter() {
            c.spawn((
                Node { padding: UiRect::all(Val::Px(10.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                BackgroundColor(Color::srgba(0.08, 0.22, 0.40, 0.92)),
            ))
            .with_children(|b| {
                b.spawn(text(msg.clone(), 15.0, Color::WHITE));
            });
        }
    });
}

fn keyboard_shortcuts(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    members: Query<Entity, With<HouseholdMember>>,
    selected: Query<Entity, With<Selected>>,
    positions: Query<&Transform, With<Sim>>,
    mut cam: Query<&mut SimsCamera>,
    mut pie: ResMut<PieMenu>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        close_pie(&mut commands, &mut pie);
    }
    if keys.just_pressed(KeyCode::Tab) {
        let mut all: Vec<Entity> = members.iter().collect();
        all.sort();
        if all.is_empty() {
            return;
        }
        let cur = selected.single().ok();
        let idx = cur.and_then(|c| all.iter().position(|e| *e == c)).map(|i| (i + 1) % all.len()).unwrap_or(0);
        for s in &selected {
            commands.entity(s).remove::<Selected>();
        }
        commands.entity(all[idx]).insert(Selected);
        if let (Ok(tf), Ok(mut c)) = (positions.get(all[idx]), cam.single_mut()) {
            c.look_at(tf.translation);
        }
    }
    if keys.just_pressed(KeyCode::KeyC)
        && let Ok(s) = selected.single()
        && let (Ok(tf), Ok(mut c)) = (positions.get(s), cam.single_mut())
    {
        c.look_at(tf.translation);
    }
}

/// The selected Sim's moodlets as coloured chips (positive green, negative red).
fn update_moodlets_panel(
    mut commands: Commands,
    panel: Query<Entity, With<MoodletsPanel>>,
    sel: Query<&crate::life::Moodlets, With<Selected>>,
    mut last: Local<Vec<String>>,
) {
    let Ok(p) = panel.single() else { return };
    let mut list: Vec<(String, i32)> = sel
        .single()
        .map(|m| m.0.iter().map(|x| (crate::life::moodlet_label(x), x.value)).collect())
        .unwrap_or_default();
    list.sort_by_key(|x| -x.1.abs());
    let labels: Vec<String> = list.iter().map(|x| x.0.clone()).collect();
    if *last == labels {
        return;
    }
    *last = labels;
    commands.entity(p).despawn_children();
    commands.entity(p).with_children(|c| {
        for (label, v) in list {
            let bg = if v >= 0 { Color::srgba(0.12, 0.45, 0.15, 0.92) } else { Color::srgba(0.55, 0.12, 0.10, 0.92) };
            c.spawn((
                Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                BackgroundColor(bg),
            ))
            .with_children(|b| {
                b.spawn(text(label, 13.0, Color::WHITE));
            });
        }
    });
}

/// The phone: call people the selected Sim knows and invite them over.
#[allow(clippy::type_complexity)]
fn phone_button(
    mut commands: Commands,
    buttons: Query<&Interaction, (Changed<Interaction>, With<PhoneButton>)>,
    selected: Query<(Entity, &Relationships), With<Selected>>,
    away: Query<(Entity, &Sim), (With<crate::interact::OffLot>, Without<crate::interact::Invited>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut pie: ResMut<PieMenu>,
    mut notes: ResMut<Notifications>,
) {
    if !buttons.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    let Ok((actor, rels)) = selected.single() else { return };
    let mut known: Vec<(f32, String, ActionKind)> = away
        .iter()
        .filter(|(e, _)| rels.0.contains_key(e))
        .map(|(e, s)| (rels.friendship(e), format!("Invite {} Over", s.full_name()), ActionKind::Invite { target: e }))
        .collect();
    if known.is_empty() {
        notes.push("There's nobody to call yet — meet some Sims first!");
        return;
    }
    known.sort_by(|a, b| b.0.total_cmp(&a.0));
    let options: Vec<(String, ActionKind)> = known.into_iter().take(10).map(|(_, l, k)| (l, k)).collect();
    let at = windows.single().ok().map_or(Vec2::new(600.0, 600.0), |w| Vec2::new(w.width() * 0.4, w.height() - 260.0));
    close_pie(&mut commands, &mut pie);
    pie.at = at;
    open_pie(&mut commands, &mut pie, at, "Phone", actor, options);
}

/// The Save button (and Ctrl+S).
fn save_button(
    buttons: Query<&Interaction, (Changed<Interaction>, With<SaveButton>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut save: MessageWriter<crate::save::SaveRequest>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if buttons.iter().any(|i| *i == Interaction::Pressed) || (ctrl && keys.just_pressed(KeyCode::KeyS)) {
        crate::save::request_save(&mut save);
    }
}

/// The selected Sim's wishes: promised ones in gold, offered ones to click and promise, and
/// their lifetime happiness with the rewards button.
fn update_wishes_panel(
    mut commands: Commands,
    panel: Query<Entity, With<WishesPanel>>,
    sel: Query<&crate::wishes::Wishes, With<Selected>>,
    mut last: Local<Vec<String>>,
) {
    let Ok(p) = panel.single() else { return };
    let Ok(w) = sel.single() else { return };
    let mut sig: Vec<String> = w.promised.iter().map(|x| format!("P{}", x.text())).collect();
    sig.extend(w.offered.iter().map(|x| format!("O{}", x.text())));
    sig.push(w.points.to_string());
    if *last == sig {
        return;
    }
    *last = sig;
    commands.entity(p).despawn_children();
    commands.entity(p).with_children(|c| {
        c.spawn((
            Button,
            HudButton,
            RewardsButton,
            Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
            BackgroundColor(BTN_NORMAL),
        ))
        .with_children(|b| {
            b.spawn(text(format!("Lifetime Happiness: {}", w.points), 13.0, Color::srgb(1.0, 0.85, 0.3)));
        });
        for x in &w.promised {
            c.spawn((
                Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                BackgroundColor(Color::srgba(0.55, 0.42, 0.08, 0.95)),
            ))
            .with_children(|b| {
                b.spawn(text(format!("Promised: {} +{}", x.text(), x.points), 13.0, Color::WHITE));
            });
        }
        for (i, x) in w.offered.iter().enumerate() {
            c.spawn((
                Button,
                HudButton,
                WishButton(i),
                Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                BackgroundColor(BTN_NORMAL),
            ))
            .with_children(|b| {
                b.spawn(text(format!("{} +{}", x.text(), x.points), 13.0, Color::srgb(0.85, 0.9, 1.0)));
            });
        }
    });
}

/// Clicking an offered wish promises it; the LTH button opens the lifetime rewards.
fn wish_buttons(
    mut commands: Commands,
    wishes_btn: Query<(&Interaction, &WishButton), Changed<Interaction>>,
    rewards_btn: Query<&Interaction, (Changed<Interaction>, With<RewardsButton>)>,
    mut sel: Query<(Entity, &mut crate::wishes::Wishes), With<Selected>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut pie: ResMut<PieMenu>,
) {
    let Ok((actor, mut w)) = sel.single_mut() else { return };
    for (i, b) in &wishes_btn {
        if *i == Interaction::Pressed {
            w.promise(b.0);
        }
    }
    if rewards_btn.iter().any(|i| *i == Interaction::Pressed) {
        let options: Vec<(String, ActionKind)> = crate::wishes::Reward::ALL
            .iter()
            .enumerate()
            .filter(|(_, r)| !w.has_reward(**r))
            .map(|(i, r)| {
                let afford = if w.points >= r.cost() { "" } else { " (need more)" };
                (format!("{} — {} LTH{afford}", r.name(), r.cost()), ActionKind::BuyReward(i))
            })
            .collect();
        if options.is_empty() {
            return;
        }
        let at = windows.single().ok().map_or(Vec2::new(400.0, 500.0), |w| Vec2::new(420.0, w.height() - 420.0));
        close_pie(&mut commands, &mut pie);
        pie.at = at;
        open_pie(&mut commands, &mut pie, at, "Lifetime Rewards", actor, options);
    }
}
