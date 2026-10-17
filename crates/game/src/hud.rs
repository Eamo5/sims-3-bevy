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

/// Where screenshots go: `SIMS3_SCREENSHOTS`, or a Screenshots folder beside the saves.
pub fn screenshots_dir() -> std::path::PathBuf {
    std::env::var_os("SIMS3_SCREENSHOTS").map(std::path::PathBuf::from).unwrap_or_else(|| {
        let saves = crate::save::saves_dir();
        saves.parent().map_or_else(|| std::path::PathBuf::from("Screenshots"), |p| p.join("Screenshots"))
    })
}

/// Print Screen (or F12) takes a picture of the game, as the game's camera button does, into
/// the Screenshots folder (named for the household and the moment).
fn take_screenshot(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    household: Option<Res<Household>>,
    clock: Res<crate::clock::GameClock>,
    mut notes: ResMut<Notifications>,
    snapshot: Option<Res<crate::livehud::Snapshot>>,
) {
    let puck = snapshot.is_some();
    if puck {
        commands.remove_resource::<crate::livehud::Snapshot>();
    }
    if !(puck || keys.just_pressed(KeyCode::PrintScreen) || keys.just_pressed(KeyCode::F12)) {
        return;
    }
    let dir = screenshots_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        notes.push("Couldn't make the Screenshots folder.");
        return;
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let m = clock.minutes as i64;
    let who = household.as_ref().map_or("Sims", |h| h.name.as_str());
    let path = dir.join(format!("{who} - day {} {:02}{:02} - {stamp}.png", m / 1440 + 1, (m / 60) % 24, m % 60));
    commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
    notes.push(format!("Screenshot saved: {}", path.display()));
}

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PieMenu>()
            .add_systems(OnEnter(PlayMode::Live), spawn_hud)
            .add_systems(Update, (floor_controls, update_moodlets_panel, phone_button, save_button, update_wishes_panel, wish_buttons, fade_help, take_screenshot).run_if(in_state(PlayMode::Live)))
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
                    pie_bubble_visuals,
                    pie_portrait,
                    test_pie,
                    update_fps,
                    update_trait_icons,
                    update_skill_icons,
                )
                    .chain()
                    .run_if(in_state(PlayMode::Live)),
            );
    }
}

use crate::menu::PANEL_BG;

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
    /// What it's titled (a submenu's name: "Have Dinner").
    pub title: String,
    /// The menus a submenu was opened from (where each was, its title and options), back to
    /// which its middle goes.
    pub parents: Vec<(Vec2, String, Vec<(String, ActionKind)>)>,
    /// Where the pointer's to be put: the middle of a menu moved onto the screen.
    pub warp: Option<Vec2>,
    /// A long menu's pages, each led by "More..." to the next.
    pub pages: Vec<Vec<(String, ActionKind)>>,
}

/// The fridge's (and microwave's) menu of snacks, as the game titles it.
const QUICK_MEAL: &str = "Have Quick Meal";
/// The fridge's menu of leftovers.
const LEFTOVERS: &str = "Eat Leftovers";
/// A stereo's menu of stations.
const CHANGE_STATION: &str = "Change to";

/// The TV's channel menu (the game's "Change to...": its middle adds the dots).
const CHANGE_CHANNEL: &str = "Change to";
/// A bed's side given to one of the household (the game's `SetBedOwnership`).
const SET_BED_OWNER: &str = "Set Bed Ownership";
/// The computer's chat with someone known (the game's `ChatSpecific`, "Chat...").
const CHAT_WITH: &str = "Chat";
/// The bookshelf's skill books' menu.
const READ_SKILL_BOOK: &str = "Read a Skill Book";
const READ_NOVEL: &str = "Read a Novel";
const GO_HERE_WITH: &str = "Go Here With";
/// The game's `kGoHereWithOnLotAvailableDistance`: how far from where the Sim is a place must be
/// for others to come along.
const GO_HERE_WITH_DISTANCE: f32 = 30.0;

/// A recipe as the food menus list it: with what its ingredients cost, when they do.
fn recipe_label(r: &s3bake::gamedata::RecipeInfo) -> String {
    if r.cost > 0 { format!("{} (§{})", r.name, r.cost) } else { r.name.clone() }
}

/// A recipe to cook as the food menus list it: with what's missing of its ingredients costs
/// (the game's "RecipeWithCost"); none, if one can't be had.
fn cooked_label(data: &s3bake::GameDataBaked, r: &s3bake::gamedata::RecipeInfo, (cook, fridge): (Option<&crate::inventory::Inventory>, &[crate::inventory::Stack])) -> Option<String> {
    let p = crate::ingredients::plan(data, &r.ingredients, cook, fridge)?;
    Some(if p.cost > 0 { format!("{} (§{})", r.name, p.cost) } else { r.name.clone() })
}

/// The game's food menus, its fridge's and stove's: "Have Dinner" and "Serve Dinner", the
/// recipes they know for the time of day made for one or for everyone (cooked at `stove`),
/// "Serve Dessert", and "Have Quick Meal", the snacks (out of the fridge, or heated in the
/// microwave; not for children, those).
#[allow(clippy::too_many_arguments)]
fn food_menus(
    pie: &mut PieMenu,
    options: &mut Vec<(String, ActionKind)>,
    data: &s3bake::GameDataBaked,
    sim: &Sim,
    cooking: u32,
    known: Option<&crate::meals::KnownRecipes>,
    (hour, weekday): (f32, usize),
    stove: Option<ActionKind>,
    (fridge, microwave): (Option<ActionKind>, Option<ActionKind>),
    stock: (Option<&crate::inventory::Inventory>, &[crate::inventory::Stack]),
) {
    use s3bake::gamedata::MEAL_DESSERT;
    let (meal, word) = crate::meals::meal_time(hour, weekday);
    if let Some(cook) = stove.filter(|_| ObjectKind::Stove.usable_by(sim.age)) {
        for (title, m, one) in [(format!("Have {word}"), meal, true), (format!("Serve {word}"), meal, false), ("Serve Dessert".to_string(), MEAL_DESSERT, false)] {
            let list: Vec<(String, ActionKind)> = crate::meals::known_for(data, sim, cooking, known, m)
                .into_iter()
                .map(|r| &data.recipes[r])
                // (Desserts on their own menu.)
                .filter(|r| (m == MEAL_DESSERT || r.meals & MEAL_DESSERT == 0) && if one { r.for_one } else { r.for_group })
                .filter_map(|r| Some((cooked_label(data, r, stock)?, cook.clone())))
                .collect();
            if !list.is_empty() {
                options.push((format!("{title} ›"), submenu_kind(pie.submenus.len())));
                pie.submenus.push((title, list));
            }
        }
    }
    let child = sim.age == crate::sim::Age::Child;
    let list: Vec<(String, ActionKind)> = data
        .recipes
        .iter()
        .filter(|r| r.snack)
        .filter_map(|r| Some((recipe_label(r), if r.microwave { microwave.clone().filter(|_| !child)? } else { fridge.clone()? })))
        .collect();
    if !list.is_empty() {
        options.push((format!("{QUICK_MEAL} ›"), submenu_kind(pie.submenus.len())));
        pie.submenus.push((QUICK_MEAL.to_string(), list));
    }
}

/// Marks a pie option that opens `submenus[i]` (`ActionKind::GoHere(NaN, i)`).
pub(crate) fn submenu_kind(i: usize) -> ActionKind {
    ActionKind::GoHere(Vec2::new(f32::NAN, f32::INFINITY), i as u8)
}

fn as_submenu(k: &ActionKind) -> Option<usize> {
    match k {
        ActionKind::GoHere(p, i) if p.x.is_nan() && p.y == f32::INFINITY => Some(*i as usize),
        _ => None,
    }
}

/// Marks a long menu's "More..." option, to `pages[i]` (`ActionKind::GoHere(NaN, -inf, i)`).
fn more_kind(i: usize) -> ActionKind {
    ActionKind::GoHere(Vec2::new(f32::NAN, f32::NEG_INFINITY), i as u8)
}

fn as_more(k: &ActionKind) -> Option<usize> {
    match k {
        ActionKind::GoHere(p, i) if p.x.is_nan() && p.y == f32::NEG_INFINITY => Some(*i as usize),
        _ => None,
    }
}

/// A pie menu option (its place in `PieMenu::options`).
#[derive(Component)]
pub struct PieOption(pub usize);

#[derive(Component)]
struct NeedsName;
/// The selected Sim's moodlets (above the needs panel).
#[derive(Component)]
struct MoodletsPanel;
#[derive(Component)]
struct TraitsText;
/// Row of the selected Sim's trait icons.
#[derive(Component)]
struct TraitIcons;
/// Row of the selected Sim's skills (icon and level).
#[derive(Component)]
struct SkillIcons;
#[derive(Component)]
struct WishesPanel;
/// An offered wish button (index into `Wishes::offered`).
#[derive(Component)]
struct WishButton(usize);
/// Opens the lifetime rewards (the game's "Lifetime happiness" tab).
#[derive(Component)]
pub struct RewardsButton;
#[derive(Component)]
struct NeedsDetail;
#[derive(Component)]
struct MotiveBar(usize);
#[derive(Component)]
struct ClockText;

/// The season, the temperature and the weather, under the clock.
#[derive(Component)]
struct SeasonText;
#[derive(Component)]
struct FundsText;
#[derive(Component)]
struct SpeedButton(usize);

/// Up / down a floor of the house (+1 / -1).
#[derive(Component)]
struct FloorButton(i8);

/// Cycles the walls: up, cutaway, down (Home too).
#[derive(Component)]
struct WallButton;

#[derive(Component)]
struct WallText;

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
pub struct FpsText;

fn update_fps(time: Res<Time>, mut avg: Local<f32>, mut q: Query<&mut Text, With<FpsText>>, mut logged: Local<f32>) {
    // (FPS_LOG=1: the frame rate in the log every few seconds, for comparing runs.)
    if std::env::var("FPS_LOG").is_ok() && time.elapsed_secs() - *logged > 3.0 && *avg > 0.0 {
        *logged = time.elapsed_secs();
        info!("fps {:.1} at {:.0}s", 1.0 / *avg, time.elapsed_secs());
    }
    let dt = time.delta_secs().max(1e-4);
    *avg = if *avg == 0.0 { dt } else { *avg * 0.95 + dt * 0.05 };
    if let Ok(mut t) = q.single_mut() {
        t.0 = format!("{:.0} fps", 1.0 / *avg);
    }
}
/// UI regions that should swallow world clicks.
#[derive(Component)]
pub struct BlocksWorld;

fn panel(node: Node) -> impl Bundle {
    (
        Node { border_radius: BorderRadius::all(Val::Px(12.0)), border: UiRect::all(Val::Px(2.0)), ..node },
        BackgroundColor(PANEL_BG),
        BorderColor::all(crate::menu::PANEL_BORDER),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.35), Val::Px(0.0), Val::Px(3.0), Val::Px(0.0), Val::Px(8.0)),
        Interaction::default(),
        BlocksWorld,
    )
}

fn spawn_hud(mut commands: Commands, ui: Option<Res<crate::layout::UiAssets>>) {
    commands.insert_resource(PointerOverUi::default());
    // (With the game's own HUD up, its panels stand in for these: see `livehud`.)
    let old = !crate::livehud::active(ui.as_deref());
    if old {
        spawn_old_panels(&mut commands);
    }
    spawn_common(&mut commands, old);
}

/// The Sim's panel, wishes and moodlets, the clock bar and the household's buttons, for when
/// the game's interface isn't to hand.
fn spawn_old_panels(commands: &mut Commands) {
    // Bottom-left column: the Sim's panel, with wishes and moodlets stacked above it.
    let left_column = commands
        .spawn((
            DespawnOnExit(AppState::InGame),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(390.0),
                flex_direction: FlexDirection::ColumnReverse,
                row_gap: Val::Px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    // Needs panel
    commands
        .spawn((
            ChildOf(left_column),
            panel(Node {
                width: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            }),
        ))
        .with_children(|p| {
            p.spawn((text("", 22.0, Color::WHITE), NeedsName));
            p.spawn((text("", 14.0, Color::srgb(0.75, 0.85, 1.0)), NeedsDetail));
            crate::simpanel::tab_strip(p);
            p.spawn((text("", 13.0, Color::srgb(0.95, 0.85, 0.55)), TraitsText, crate::simpanel::NeedsOnly, Node::default()));
            p.spawn((TraitIcons, crate::simpanel::NeedsOnly, Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.0), ..default() }));
            p.spawn((SkillIcons, crate::simpanel::NeedsOnly, Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(8.0), ..default() }));
            p.spawn((
                crate::simpanel::TabContent,
                Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: Val::Px(5.0), ..default() },
            ));
            p.spawn((
                crate::simpanel::NeedsOnly,
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(10.0),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
            ))
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
            ChildOf(left_column),
            Node {
                width: Val::Percent(100.0),
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
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                column_gap: Val::Px(5.0),
                row_gap: Val::Px(5.0),
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::Center,
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
                        margin: UiRect::right(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text("Phone", 15.0, Color::WHITE));
                });
                p.spawn((
                    Button,
                    HudButton,
                    crate::relations::RelationsButton,
                    crate::icons::Tooltip("Relationships (R)".into()),
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        height: Val::Px(34.0),
                        padding: UiRect::horizontal(Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::right(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text("Relationships", 15.0, Color::WHITE));
                });
                p.spawn((
                    Button,
                    HudButton,
                    crate::opportunities::OpportunitiesButton,
                    crate::icons::Tooltip("Opportunities (O)".into()),
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        height: Val::Px(34.0),
                        padding: UiRect::horizontal(Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::right(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text("Opportunities", 15.0, Color::WHITE));
                });
                p.spawn((
                    Button,
                    HudButton,
                    crate::collecting::JournalButton,
                    crate::icons::Tooltip("Collection Journal (J)".into()),
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        height: Val::Px(34.0),
                        padding: UiRect::horizontal(Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::right(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text("Collection", 15.0, Color::WHITE));
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
                        margin: UiRect::right(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn(text("Save", 15.0, Color::WHITE));
                });
                p.spawn(Node { flex_direction: FlexDirection::Column, width: Val::Px(182.0), ..default() }).with_children(|c| {
                    c.spawn((text("", 18.0, Color::WHITE), ClockText));
                    c.spawn((text("", 12.0, Color::srgb(0.8, 0.88, 1.0)), SeasonText));
                });
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
                p.spawn((
                    Button,
                    HudButton,
                    WallButton,
                    Node {
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        width: Val::Px(104.0),
                        height: Val::Px(34.0),
                        margin: UiRect::left(Val::Px(12.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                    crate::icons::Tooltip("Walls up, cut away or down (Home)".into()),
                ))
                .with_children(|b| {
                    b.spawn((text("Cutaway", 15.0, Color::WHITE), WallText));
                });
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

}

/// The action queue, notifications, frame rate and help line.
fn spawn_common(commands: &mut Commands, old: bool) {
    // Action queue (top-left; the game's HUD has its own)
    if old {
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
    }

    // Household members (right; the game's HUD has its skewer)
    if old {
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
    }

    // Notifications (top-right; the game's HUD has its own cards)
    if old {
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
    }

    // Frame rate (top centre)
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        FpsText,
        text("", 13.0, Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Node { position_type: PositionType::Absolute, top: Val::Px(6.0), left: Val::Percent(48.0), ..default() },
    ));

    // Help line (top, fading once play has started; the game's own HUD has its tooltips)
    if !old {
        return;
    }
    commands.spawn((
        DespawnOnExit(AppState::InGame),
        HelpLine,
        text(
            "Click objects / sims for actions · Click ground to walk · WASD/arrows pan · Q/E rotate · wheel zoom · Space pause · 1/2/3 speed · Tab next sim · C centre camera · PgUp/PgDn floors · B buy mode · R relationships · O opportunities",
            13.0,
            Color::srgba(1.0, 1.0, 1.0, 0.75),
        ),
        TextLayout::justify(Justify::Center),
        Node { position_type: PositionType::Absolute, left: Val::Px(160.0), right: Val::Px(160.0), top: Val::Px(26.0), ..default() },
        Pickable::IGNORE,
    ));
}

#[derive(Component)]
struct HelpLine;

/// The help line fades out after the first minute and a half.
fn fade_help(mut commands: Commands, time: Res<Time>, mut since: Local<Option<f32>>, mut q: Query<(Entity, &mut TextColor), With<HelpLine>>) {
    let t0 = *since.get_or_insert(time.elapsed_secs());
    let a = (1.0 - (time.elapsed_secs() - t0 - 90.0) / 5.0).clamp(0.0, 1.0) * 0.75;
    for (e, mut c) in &mut q {
        if a <= 0.0 {
            commands.entity(e).try_despawn();
        } else {
            c.0 = Color::srgba(1.0, 1.0, 1.0, a);
        }
    }
}

pub(crate) fn pointer_over_ui(mut over: ResMut<PointerOverUi>, q: Query<(&Interaction, &InheritedVisibility), Or<(With<BlocksWorld>, With<Button>)>>) {
    over.0 = q.iter().any(|(i, v)| v.get() && *i != Interaction::None);
}

fn close_pie(commands: &mut Commands, pie: &mut PieMenu) {
    if let Some(r) = pie.root.take() {
        commands.entity(r).try_despawn();
    }
    pie.options.clear();
}

/// A rabbit-hole activity with what it costs (or pays).
pub fn activity_label(a: &crate::rabbitholes::Activity) -> String {
    let cost = match a.cost {
        c if c > 0 => format!(" (§{c})"),
        c if c < 0 => format!(" (earn §{})", -c),
        _ => String::new(),
    };
    format!("{}{cost}", a.name)
}

/// The game's pie menu look: pale bubbles with dark writing, blue under the pointer.
const BUBBLE: Color = Color::srgba(0.97, 0.98, 1.0, 0.97);
const BUBBLE_HOVER: Color = Color::srgb(0.70, 0.85, 1.0);
const BUBBLE_PRESS: Color = Color::srgb(0.55, 0.80, 0.45);
const BUBBLE_TEXT: Color = Color::srgb(0.08, 0.16, 0.33);

/// A pie menu option's bubble.
#[derive(Component)]
struct PieBubble;

/// The middle of a pie menu, where the Sim's portrait goes.
#[derive(Component)]
struct PieCenter(Entity);

pub fn open_pie(commands: &mut Commands, pie: &mut PieMenu, at: Vec2, title: &str, actor: Entity, options: Vec<(String, ActionKind)>) {
    pie.parents.clear();
    spawn_pie(commands, pie, at, title, actor, options);
}

/// A submenu, opened where its option was clicked (as the game's are), with the way back to the
/// menu it's in at its middle.
fn open_submenu(commands: &mut Commands, pie: &mut PieMenu, at: Vec2, title: &str, actor: Entity, options: Vec<(String, ActionKind)>, parent: (Vec2, String, Vec<(String, ActionKind)>)) {
    pie.parents.push(parent);
    spawn_pie(commands, pie, at, title, actor, options);
}

fn spawn_pie(commands: &mut Commands, pie: &mut PieMenu, at: Vec2, title: &str, actor: Entity, options: Vec<(String, ActionKind)>) {
    close_pie(commands, pie);
    if options.is_empty() {
        return;
    }
    // (More than the game shows at once: in pages of eleven, each led by "More..." to the next
    // and the last back to the first, as UI.dll's `ValidateMenuStructure` has it.)
    let options = if options.len() > crate::piemenu::MAX_ITEMS {
        let chunks: Vec<Vec<(String, ActionKind)>> = options.chunks(crate::piemenu::MAX_ITEMS - 1).map(|c| c.to_vec()).collect();
        let n = chunks.len();
        pie.pages = chunks.into_iter().enumerate().map(|(i, c)| std::iter::once(("More ›".to_string(), more_kind((i + 1) % n))).chain(c).collect()).collect();
        pie.pages[0].clone()
    } else {
        options
    };
    let submenu = !pie.parents.is_empty();
    let n = options.len();
    let radius = 90.0 + n as f32 * 10.0;
    // (In the game's own bubbles when its interface is to hand: see `piemenu`.)
    let game_look = crate::piemenu::GAME_PIE.load(std::sync::atomic::Ordering::Relaxed) && n <= crate::piemenu::MAX_ITEMS;
    let root = commands
        .spawn((
            DespawnOnExit(AppState::InGame),
            Node { position_type: PositionType::Absolute, left: Val::Px(at.x), top: Val::Px(at.y), ..default() },
            GlobalZIndex(10),
        ))
        .with_children(|p| {
            // A submenu's middle: the way back, named for it (see `piemenu`; here, a bubble).
            if submenu {
                if !game_look {
                    p.spawn((
                        Button,
                        PieBubble,
                        crate::piemenu::PieBack,
                        Node {
                            border_radius: BorderRadius::all(Val::Px(17.0)),
                            border: UiRect::all(Val::Px(1.5)),
                            position_type: PositionType::Absolute,
                            left: Val::Px(-75.0),
                            top: Val::Px(-16.0),
                            width: Val::Px(150.0),
                            height: Val::Px(32.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BorderColor::all(Color::srgb(0.45, 0.6, 0.82)),
                        BackgroundColor(BUBBLE),
                    ))
                    .with_children(|b| {
                        b.spawn((text(format!("{title}..."), 14.0, BUBBLE_TEXT), Pickable::IGNORE));
                    });
                }
            }
            if !submenu {
                // The one acting, in the middle, with what's been clicked above.
                p.spawn((
                    PieCenter(actor),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-28.0),
                        top: Val::Px(-28.0),
                        width: Val::Px(56.0),
                        height: Val::Px(56.0),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(28.0)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BorderColor::all(Color::WHITE),
                    BackgroundColor(Color::srgb(0.55, 0.7, 0.9)),
                    BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.45), Val::Px(0.0), Val::Px(2.0), Val::Px(0.0), Val::Px(6.0)),
                    Pickable::IGNORE,
                ));
                // (A label under it, on a dark pill so it reads over anything.)
                p.spawn((
                    Node { position_type: PositionType::Absolute, left: Val::Px(-110.0), top: Val::Px(31.0), width: Val::Px(220.0), justify_content: JustifyContent::Center, ..default() },
                    Pickable::IGNORE,
                ))
                .with_children(|l| {
                    l.spawn((
                        Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(9.0)), ..default() },
                        BackgroundColor(Color::srgba(0.05, 0.12, 0.25, 0.75)),
                        Pickable::IGNORE,
                    ))
                    .with_children(|b| {
                        b.spawn((text(title, 13.0, Color::WHITE), Pickable::IGNORE));
                    });
                });
            }
            for (i, (label, _)) in options.iter().enumerate().filter(|_| !game_look) {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
                let (x, y) = (a.cos() * radius, a.sin() * radius * 0.8);
                p.spawn((
                    Button,
                    PieBubble,
                    PieOption(i),
                    Node {
                        border_radius: BorderRadius::all(Val::Px(17.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        position_type: PositionType::Absolute,
                        left: Val::Px(x - 88.0),
                        top: Val::Px(y - 16.0),
                        width: Val::Px(176.0),
                        height: Val::Px(32.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderColor::all(Color::srgb(0.45, 0.6, 0.82)),
                    BackgroundColor(BUBBLE),
                    BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.35), Val::Px(0.0), Val::Px(2.0), Val::Px(0.0), Val::Px(5.0)),
                ))
                .with_children(|b| {
                    b.spawn((text(label.clone(), 14.0, BUBBLE_TEXT), Pickable::IGNORE));
                });
            }
        })
        .id();
    if game_look {
        commands.entity(root).insert(crate::piemenu::PieRequest(options.iter().map(|o| o.0.clone()).collect(), submenu.then(|| title.to_string())));
    }
    pie.root = Some(root);
    pie.actor = Some(actor);
    pie.options = options;
    pie.title = title.to_string();
    // (Where its submenus open too.)
    pie.at = at;
}

/// How far along a ray it meets a box (none: it misses).
fn slab(o: Vec3, d: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, f32::MAX);
    for a in 0..3 {
        if d[a].abs() < 1e-6 {
            if o[a] < min[a] || o[a] > max[a] {
                return None;
            }
            continue;
        }
        let (ta, tb) = ((min[a] - o[a]) / d[a], (max[a] - o[a]) / d[a]);
        t0 = t0.max(ta.min(tb));
        t1 = t1.min(ta.max(tb));
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// How far along a ray it meets an object: its footprint, up to its height, in its own frame.
/// (Its meshes are the GPU's alone, with no copy to test.)
pub(crate) fn object_hit(ray: Ray3d, tf: &GlobalTransform, o: &GameObject) -> Option<f32> {
    let inv = tf.affine().inverse();
    let (lo, ld) = (inv.transform_point3(ray.origin), inv.transform_vector3(Vec3::from(ray.direction)));
    // (Some thickness to what's flat against a wall, a mirror.)
    let (h, half) = (o.height.max(0.15), o.half.max(Vec2::splat(0.08)));
    slab(lo, ld, Vec3::new(o.center.x - half.x, -0.05, o.center.y - half.y), Vec3::new(o.center.x + half.x, h, o.center.y + half.y))
}

/// The shown object, Sim or pet aimed at: of those whose boxes the ray passes through (an
/// object's footprint up to its height, a Sim or pet a box their size round where they stand),
/// the one whose middle it passes nearest, the smaller winning a near tie (a sink set in a
/// counter, a chair at a table, a counter under an overhead cabinet).
#[allow(clippy::type_complexity)]
pub(crate) fn pick_by_box(
    ray: Ray3d,
    pickable: &Query<(Entity, &GlobalTransform, &InheritedVisibility, Option<&GameObject>, Has<crate::pets::Pet>), Or<(With<GameObject>, With<Sim>, With<crate::pets::Pet>)>>,
) -> Option<Entity> {
    let dir = Vec3::from(ray.direction);
    pickable
        .iter()
        // (Not what there's nothing to do with, a rug: a click there is on the floor.)
        .filter(|(_, _, vis, obj, _)| vis.get() && obj.is_none_or(|o| !crate::interact::interactions_for(o.kind).is_empty()))
        .filter_map(|(e, tf, _, obj, pet)| {
            let (mid, size) = match obj {
                Some(o) => {
                    object_hit(ray, tf, o)?;
                    (tf.transform_point(Vec3::new(o.center.x, o.height.max(0.15) * 0.5, o.center.y)), o.half.x.max(o.half.y))
                }
                None => {
                    let p = tf.translation();
                    let (r, h) = if pet { (0.4, 0.8) } else { (0.3, 1.8) };
                    slab(ray.origin, dir, p + Vec3::new(-r, 0.0, -r), p + Vec3::new(r, h, r))?;
                    (p + Vec3::Y * h * 0.5, r)
                }
            };
            // (How far the ray passes from its middle.)
            let off = (mid - ray.origin).cross(dir).length();
            Some((e, off + 0.25 * size))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e)
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
        let planes = b.data.floors.iter().filter(|f| f.level > 0 && f.level <= b.view_level).filter_map(|f| {
            Some((f.level, f.y.or_else(|| b.levels.get(f.level as usize).copied())?))
        });
        if let Some(hit) = floor_plane_hit(ray, planes, |level, p| b.floor_y(level, p).is_some_and(|y| (p.y - y).abs() < 0.01)) {
            return Some(hit);
        }
    }
    ground_hit(ray, world).map(|p| (p, 1))
}

/// Test each distinct rendered floor elevation, retaining the nearest occupied intersection.
fn floor_plane_hit(ray: Ray3d, planes: impl IntoIterator<Item = (u8, f32)>, contains: impl Fn(u8, Vec3) -> bool) -> Option<(Vec3, u8)> {
    if ray.direction.y.abs() < 1e-4 { return None; }
    let mut seen = std::collections::HashSet::new();
    let mut best: Option<(f32, Vec3, u8)> = None;
    for (level, y) in planes {
        if !y.is_finite() || !seen.insert((level, y.to_bits())) { continue; }
        let t = (y - ray.origin.y) / ray.direction.y;
        if t <= 0.0 || best.is_some_and(|(distance, _, _)| t >= distance) { continue; }
        let p = ray.origin + *ray.direction * t;
        if contains(level, p) { best = Some((t, p, level)); }
    }
    best.map(|(_, p, level)| (p, level))
}

#[cfg(test)]
mod floor_picking_tests {
    use super::*;

    #[test]
    fn slanted_ray_selects_actual_raised_tile_not_default_storey_plane() {
        let ray = Ray3d::new(Vec3::new(0.0, 4.0, 0.5), Dir3::new(Vec3::new(1.0, -1.0, 0.0)).unwrap());
        let hit = floor_plane_hit(ray, [(1, 0.0), (1, 2.0)], |_, p| (1.5..2.5).contains(&p.x) && (p.y - 2.0).abs() < 0.01).unwrap();
        assert!((hit.0.x - 2.0).abs() < 0.001);
        assert_eq!(hit.0.y, 2.0);
        assert_eq!(hit.1, 1);
    }

    #[test]
    fn nearest_occupied_floor_wins_and_holes_reveal_lower_floors() {
        let ray = Ray3d::new(Vec3::new(0.5, 8.0, 0.5), Dir3::NEG_Y);
        let planes = [(1, 0.0), (2, 3.0), (2, 3.0), (3, 6.0)];
        assert_eq!(floor_plane_hit(ray, planes, |_, _| true).unwrap().1, 3);
        assert_eq!(floor_plane_hit(ray, planes, |level, _| level < 3).unwrap().1, 2);
        assert!(floor_plane_hit(ray, planes, |_, _| false).is_none());
        assert!(floor_plane_hit(Ray3d::new(ray.origin, Dir3::Y), planes, |_, _| true).is_none());
        assert!(floor_plane_hit(Ray3d::new(ray.origin, Dir3::X), planes, |_, _| true).is_none());
    }
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
    (objects, broken_q, lit_q, hw_q, trash_q, leftovers, sprinkling, weather, chess, music, potties): (
        Query<&GameObject>,
        Query<(), With<crate::interact::Broken>>,
        Query<(), With<crate::fireplace::Lit>>,
        Query<(), With<crate::rabbitholes::Homework>>,
        Query<&crate::surroundings::TrashFill>,
        Res<crate::meals::Leftovers>,
        Query<(), With<crate::gardening::Sprinkling>>,
        Res<crate::weather::Weather>,
        Res<crate::chess::ChessTables>,
        Res<crate::tips::Performances>,
        Res<crate::potty::Potties>,
    ),
    selected: Query<(Entity, &Relationships, &Sim), With<Selected>>,
    (members_q, laundry, sides_q, moods_q, locks_q, lot_objects_q, journals_q, klepto_q, fridge_stock, postures, (graves_q, queues_q, cuddles_q, convs, ghosts_q, club_q, unflushed, tv_on, (layout_ui, charities_q))): (
        Query<(), With<HouseholdMember>>,
        Res<crate::laundry::Laundry>,
        Query<crate::social::SideQuery>,
        Query<&crate::life::Moodlets>,
        Query<&crate::doorlocks::DoorLock>,
        Query<(), With<crate::visit::LotObject>>,
        Query<&crate::journal::SkillJournal>,
        (
            Query<(Entity, &GameObject, &GlobalTransform, &crate::visit::LotObject, Option<&crate::nav::Floor>, Option<&crate::interact::UsedBy>)>,
            Query<(Entity, &Transform, Option<&crate::nav::Floor>, Option<&crate::visit::OnLot>), With<Sim>>,
            Query<&crate::klepto::SwipedToday>,
        ),
        Res<crate::ingredients::Fridge>,
        Query<(&crate::sim::SimAnim, &GlobalTransform, Option<&crate::nav::Floor>)>,
        (
            Query<&crate::ghosts::Grave>,
            Query<&ActionQueue>,
            Query<&crate::seatedtalk::Cuddling>,
            Res<crate::groups::Conversations>,
            Query<(), With<crate::ghosts::Ghost>>,
            Query<(Has<crate::computertraits::BookClub>, Option<&crate::computertraits::WorkToDo>)>,
            Res<crate::toilets::Unflushed>,
            Query<(Has<crate::effects::TvOn>, Has<crate::effects::TvLeftOn>)>,
            (Option<Res<crate::layout::UiAssets>>, Query<&crate::charity::Charities>),
        ),
    ),
    world: Res<CurrentWorld>,
    mut pie: ResMut<PieMenu>,
    (buy, edit_town, pickable): (
        Option<Res<crate::buy::BuyMode>>,
        Res<crate::edittown::EditTown>,
        Query<(Entity, &GlobalTransform, &InheritedVisibility, Option<&GameObject>, Has<crate::pets::Pet>), Or<(With<GameObject>, With<Sim>, With<crate::pets::Pet>)>>,
    ),
    (building, opp_q): (
        Option<Res<crate::building::ActiveBuilding>>,
        (
            Option<Res<crate::icons::GameUi>>,
            Query<Option<&crate::opportunities::SimOpportunities>, With<Selected>>,
            Res<crate::clock::GameClock>,
            Query<&crate::gardening::GrowingPlant>,
            Option<Res<crate::interact::Household>>,
            Option<Res<crate::gardening::Garden>>,
            Res<crate::appliances::Alarm>,
            Option<Res<crate::sound::Sounds>>,
            Query<(Option<&crate::music::StereoOn>, Has<crate::interact::Unmade>, Option<&crate::grime::Grime>)>,
        ),
    ),
    (on_lot, writers, jobs_q, toddler_q, bowl_q, inv_q, upg_q, family, door_q, pets_q, labour_q, tv_levels, classes_q, (books_q, novels_q), company_q): (
        Query<&crate::visit::OnLot>,
        Query<(Option<&crate::writing::Author>, &crate::interact::Skills, Option<&crate::meals::KnownRecipes>)>,
        Query<Option<&crate::careers::Job>>,
        Query<&crate::little::ToddlerSkills>,
        Query<&crate::fishbowl::BowlFish>,
        Query<&crate::inventory::Inventory>,
        Query<&crate::upgrades::Upgrades>,
        Res<crate::family::Genealogy>,
        Query<&crate::doorbell::AtTheDoor>,
        Query<(&crate::pets::Pet, &GlobalTransform)>,
        Query<(), With<crate::little::Labour>>,
        Query<&crate::tvchannels::TvLevel>,
        Query<(&Sim, Option<&crate::classes::ClassesTaken>)>,
        (Query<(&crate::skillbooks::SkillBooks, Has<HouseholdMember>)>, Query<(&crate::novels::Novels, Has<HouseholdMember>)>),
        Query<(Entity, &Sim, Has<crate::interact::Visitor>, Option<&crate::visit::OnLot>), Or<(With<HouseholdMember>, With<crate::interact::Visitor>)>>,
    ),
) {
    // (Not in buy mode, nor in Edit Town: a click there picks lots.)
    if buy.as_ref().is_some_and(|b| b.active) || edit_town.is_open() {
        if mouse.just_pressed(MouseButton::Left) {
            debug!("click in {}", if edit_town.is_open() { "Edit Town" } else { "buy mode" });
        }
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        close_pie(&mut commands, &mut pie);
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) || over_ui.0 {
        if mouse.just_pressed(MouseButton::Left) {
            debug!("click at {:?}: on the interface", windows.single().ok().and_then(|w| w.cursor_position()));
        }
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    close_pie(&mut commands, &mut pie);
    let Ok((actor, rels, actor_sim)) = selected.single() else { return };

    let is_target = |e: Entity| sims.contains(e) || objects.contains(e) || pets_q.contains(e);
    let filter = |e: Entity| ancestor_with(e, &parents, is_target).is_some();
    let hits = ray_cast.cast_ray(ray, &MeshRayCastSettings::default().with_filter(&filter));
    // (The objects' meshes are the GPU's alone, so most are found by their footprints: the
    // nearest object, Sim or pet whose box the ray passes through, of those shown.)
    let target = hits.first().and_then(|(e, _)| ancestor_with(*e, &parents, is_target)).or_else(|| pick_by_box(ray, &pickable));
    debug!("click at {cursor}: {:?} ({})", target.map(|t| objects.get(t).map_or("a Sim or pet".to_string(), |o| o.name.clone())), if hits.is_empty() { "by its box" } else { "by its mesh" });

    // A pet: what can be done with it.
    if let Some((t, (pet, ptf))) = target.and_then(|t| pets_q.get(t).ok().map(|p| (t, p)))
        && !actor_sim.age.is_little()
    {
        let kind: &'static str = ["ac", "cc", "ad", "cd", "al", "cl", "ah", "ch"].into_iter().find(|k| *k == pet.kind).unwrap_or("ac");
        let at = Vec2::new(ptf.translation().x, ptf.translation().z);
        let options: Vec<(String, ActionKind)> = crate::pets::PET_SOCIALS
            .iter()
            .enumerate()
            .filter(|(_, s)| s.suits(kind))
            .map(|(i, s)| (s.name.to_string(), ActionKind::PetSocial { target: t, social: i, at, kind }))
            .collect();
        pie.submenus.clear();
        pie.at = cursor;
        open_pie(&mut commands, &mut pie, cursor, &format!("{} ({})", pet.name, pet.species()), actor, options);
        return;
    }
    if let Some(t) = target {
        if let Ok((sim, member, is_sel)) = sims.get(t) {
            // Clicking themselves: what they can do on their own.
            if is_sel {
                let mut options = Vec::new();
                if crate::jog::can_jog(sim) && on_lot.get(actor).is_err() {
                    options.push(("Go Jogging".to_string(), ActionKind::Jog { home: Vec2::ZERO }));
                }
                // (In labour: off to the hospital to have the baby, as the game's "Go to Hospital".)
                if labour_q.contains(actor)
                    && let Some(lot) = world.data.lots.iter().position(|l| l.internal_name.to_ascii_lowercase().contains("hospital"))
                {
                    options.push(("Go to the Hospital".to_string(), ActionKind::Visit { lot, activity: crate::little::BIRTH_TASK }));
                }
                // (Their cell phone, as the game's: calls, services, adopting, moving.)
                if !sim.age.is_little() {
                    options.push((PHONE_LABEL.to_string(), ActionKind::EatHere));
                }
                // (What they do for who they are: talking to themselves, freaking out, a fake
                // faint, a TaDa!)
                options.extend(crate::traitacts::offered(sim, moods_q.get(actor).ok()));
                // (A werewolf under the full moon howls at it.)
                if sim.occult == Some(crate::sim::Occult::Werewolf) && crate::supernatural::full_moon_night(opp_q.2.minutes) {
                    options.push((
                        "Howl at the Moon".to_string(),
                        ActionKind::Outro { clips: &["a_werewolf_howlAtMoon_success"], then: None, secs: 6.0, stand_at: None, target: actor },
                    ));
                }
                if !options.is_empty() {
                    pie.submenus.clear();
                    pie.at = cursor;
                    open_pie(&mut commands, &mut pie, cursor, &sim.full_name(), actor, options);
                }
                return;
            }
            let mut options = Vec::new();
            if member {
                options.push((format!("Select {}", sim.first), ActionKind::GoHere(Vec2::NAN, 1)));
            }
            let rel = rels.get(t);
            let target_member = members_q.contains(t);
            // (Talking in a group: someone they know called over to join it.)
            if convs.group(actor).is_some() && !convs.together(actor, t) && rels.0.contains_key(&t) && !sim.age.is_little() {
                options.push((crate::groups::INVITE_LABEL.to_string(), ActionKind::Social { target: t, social: crate::groups::INVITE }));
            }
            // (An opportunity's delivery to make to them: its words.)
            let delivery = opp_q.0.as_ref().and_then(|ui| {
                let opps = opp_q.1.single().ok().flatten();
                let i = crate::opportunities::delivery_for(&ui.data, opps, inv_q.get(actor).ok(), sim.id)?;
                let o = ui.data.opportunities.get(opps?.active[i].index)?;
                Some(if o.interaction.is_empty() { o.name.clone() } else { o.interaction.clone() })
            });
            // (Sat near each other: what can be said where they sit, the game's seated talk.)
            let posture = |e: Entity| postures.get(e).ok().map(|(a, tf, f)| (a, tf.translation(), f.map_or(1, |f| f.0)));
            let seated = matches!((posture(actor), posture(t)), (Some(a), Some(b)) if crate::seatedtalk::sat_together(a, b));
            // (Sat on the same sofa, or two grown-ups relaxing in the same double bed: a cuddle,
            // the game's seated one or its `BedCuddle`.)
            let spot = |e: Entity| crate::seatedtalk::cuddle_spot(queues_q.get(e).ok(), |o| objects.get(o).ok().map(|g| g.kind));
            let together = spot(actor).is_some() && spot(actor) == spot(t);
            let same_bed = together && spot(actor).is_some_and(|s| s.1) && actor_sim.age.is_grown() && sim.age.is_grown();
            let same_sofa = together && seated && !same_bed;
            // (Cuddling already: a kiss, or making out, the game's.)
            let cuddling = (same_sofa || same_bed) && cuddles_q.get(actor).is_ok_and(|c| c.with == t && c.accepted);
            pie.submenus.clear();
            for cat in crate::social::SocialCat::ALL {
                let list: Vec<(String, ActionKind)> = crate::social::all_socials()
                    .filter(|(_, s)| {
                        let cuddle = s.name == crate::seatedtalk::CUDDLE || (cuddling && crate::seatedtalk::WHILE_CUDDLING.contains(&s.name));
                        // (Lying in bed together: the cuddle, and WooHoo.)
                        if same_bed {
                            cuddle || matches!(s.effect, crate::social::SocialEffect::WooHoo | crate::social::SocialEffect::TryForBaby)
                        } else {
                            !seated || crate::social::seated_ok(s) || (same_sofa && cuddle)
                        }
                    })
                    // (A cuddle only sat on a sofa or lying in bed together, and not twice over.)
                    .filter(|(_, s)| s.name != crate::seatedtalk::CUDDLE || ((same_sofa || same_bed) && !cuddling))
                    .filter(|(_, s)| {
                        s.cat == cat
                            // (While cuddling, a kiss or making out is the couple's to have, as the
                            // game's `CuddleSeatedKiss`: their romance, not the talk's turn.)
                            && ((cuddling && crate::seatedtalk::WHILE_CUDDLING.contains(&s.name) && (rel.is_romance() || rel.romance >= s.min_romance)) || crate::social::available(
                                s,
                                &rel,
                                actor_sim,
                                sim,
                                target_member,
                                family.kin(actor_sim.id, sim.id).is_some(),
                                [crate::social::Side::of(actor, sides_q.get(actor).ok()), crate::social::Side::of(t, sides_q.get(t).ok())],
                            ))
                    })
                    // (Nothing more to teach a toddler who's learned it; homework helped with only
                    // when there's some.)
                    .filter(|(_, s)| match s.effect {
                        crate::social::SocialEffect::TeachWalk => !toddler_q.get(t).is_ok_and(|k| k.walks()),
                        crate::social::SocialEffect::TeachTalk => !toddler_q.get(t).is_ok_and(|k| k.talks()),
                        crate::social::SocialEffect::HelpHomework => hw_q.contains(t),
                        crate::social::SocialEffect::Greet => door_q.get(t).is_ok_and(|d| d.since.is_some()) && members_q.contains(actor),
                        // (Catch with a ball about.)
                        crate::social::SocialEffect::PlayCatch => objects.iter().any(|o| o.kind == ObjectKind::Ball),
                        // (An opportunity's delivery to them, what it asks held.)
                        crate::social::SocialEffect::DeliverOpportunity => delivery.is_some(),
                        // (A ghost doesn't move in: the game's, for those not yet playable.)
                        crate::social::SocialEffect::MoveIn => !ghosts_q.contains(t),
                        _ => true,
                    })
                    // (A delivery under the opportunity's own words: "Deliver Painting".)
                    .map(|(i, s)| (if s.effect == crate::social::SocialEffect::DeliverOpportunity { delivery.clone().unwrap_or_default() } else { s.name.to_string() }, ActionKind::Social { target: t, social: i }))
                    .collect();
                // (Those of a kind together, as the game's menus have them: "Ask About ›" Day,
                // Career...)
                let list = crate::social::grouped(list, |label, sub| {
                    pie.submenus.push((label.to_string(), sub));
                    submenu_kind(pie.submenus.len() - 1)
                });
                // (Sat together, the game's single-level menu: the friendly talk, at the top; in bed
                // together, the same.)
                if seated || same_bed {
                    options.extend(list);
                } else if !list.is_empty() {
                    options.push((format!("{} ›", cat.name()), submenu_kind(pie.submenus.len())));
                    pie.submenus.push((cat.name().to_string(), list));
                }
            }
            // (A cuddle on the sofa first, as the game's own interaction beside the talk.)
            for first in crate::seatedtalk::WHILE_CUDDLING.iter().rev().chain([&crate::seatedtalk::CUDDLE]) {
                if (seated || same_bed) && let Some(i) = options.iter().position(|o| o.0 == *first) {
                    let c = options.remove(i);
                    let at = options.iter().position(|o| !o.0.starts_with("Select")).unwrap_or(options.len());
                    options.insert(at, c);
                }
            }
            let title = format!("{} ({})", sim.full_name(), rel.label_for(sim.female));
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
            if broken_q.contains(t) {
                let grown = actor_sim.age.is_grown() && actor_sim.age != crate::sim::Age::Child;
                if grown {
                    options.push((crate::interact::repair_of(obj.kind).0.to_string(), ActionKind::Repair { target: t }));
                }
                open_pie(&mut commands, &mut pie, cursor, &format!("{} (broken)", obj.name), actor, options);
                return;
            }
            // A house phone: the phone's menu.
            if obj.kind == ObjectKind::Phone {
                commands.insert_resource(OpenPhone);
                return;
            }
            let usable = if obj.kind.suits(actor_sim) { interactions_for(obj.kind) } else { &[] };
            let plant = opp_q.3.get(t).ok();
            pie.submenus.clear();
            // The household's car, as the game's `CarOwnable` offers it (to teens and up): to
            // work when it's nearly time, or to a community lot ("Go to Community Lot...").
            if obj.kind == ObjectKind::Car && (actor_sim.age == crate::sim::Age::Teen || actor_sim.age.is_grown()) {
                let clock = &opp_q.2;
                if let Ok(Some(job)) = jobs_q.get(actor) {
                    let (h, start) = (clock.hour_f(), job.info().start);
                    if job.works_on(clock.weekday()) && h >= start - 1.0 && h < start + 1.0 && job.last_day != Some(clock.day()) {
                        options.push(("Go to Work".to_string(), ActionKind::GoToWork));
                    }
                }
                let lots: Vec<(String, ActionKind)> = (0..world.data.lots.len())
                    .filter(|&l| crate::visit::visitable(&world.data, l))
                    .map(|l| (crate::visit::place_name(&world.data, l), ActionKind::GoToLot { lot: l }))
                    .collect();
                if !lots.is_empty() {
                    options.push(("Go to Community Lot ›".to_string(), submenu_kind(pie.submenus.len())));
                    pie.submenus.push(("Go to Community Lot".to_string(), lots));
                }
            }
            let usable = if obj.kind == ObjectKind::Car { &[] } else { usable };
            for (i, d) in usable.iter().enumerate() {
                if plant.is_some_and(|p| !crate::gardening::offers(p, d.special)) {
                    continue;
                }
                // (A tombstone's epitaph read once there is one; engraved where someone lies.)
                if (d.special == Special::ReadEpitaph && graves_q.get(t).ok().is_none_or(|g| g.epitaph.is_empty())) || (d.special == Special::Engrave && !graves_q.contains(t)) {
                    continue;
                }
                // A fireplace offers lighting when cold, the rest when lit.
                if matches!(obj.kind, ObjectKind::Fireplace | ObjectKind::FirePit | ObjectKind::Candle) && d.special != Special::NeuroticCheck && (d.special == Special::LightFire) == lit_q.contains(t) {
                    continue;
                }
                // (A bubble bath with bubble bath in the house, a play with the duck with one.)
                if d.special.needs().is_some_and(|k| !objects.iter().any(|o| o.kind == k)) {
                    continue;
                }
                // (A snack sticking is what a purchase turns into, not one to choose.)
                if d.special == Special::VendStuck {
                    continue;
                }
                if d.special == Special::VampireOnly && actor_sim.occult != Some(crate::sim::Occult::Vampire) {
                    continue;
                }
                // (At a mirror, what's theirs: Admire Self the Snobs', Play with Mirror the children's.)
                if !crate::mirrors::offers(d.special, actor_sim) {
                    continue;
                }
                // (A Neurotic's Check Sink, Check Stove, Check Fireplace.)
                if !crate::neurotic::offers(d.special, actor_sim) {
                    continue;
                }
                // (Flush: while it needs one.)
                if d.special == Special::Flush && !unflushed.0.contains_key(&t) {
                    continue;
                }
                // (A bed's sides given to the household: the game's `SetBedOwnership`, child and up.)
                if crate::bedowners::is_bed(obj.kind) && d.name == "Sleep" {
                    let list: Vec<(String, ActionKind)> = company_q
                        .iter()
                        .filter(|c| !c.2 && !c.1.age.is_little())
                        .map(|c| (c.1.full_name(), ActionKind::Object { target: t, def: i }))
                        .collect();
                    if !list.is_empty() {
                        options.push((format!("{SET_BED_OWNER}... ›"), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((SET_BED_OWNER.to_string(), list));
                    }
                }
                // (A donation, the game's `DonateToCharity`: the Good's or Evil's, once a day, to
                // either of this week's two causes, as much as the household can spare.)
                if d.special == Special::Donate {
                    let day = opp_q.2.day();
                    let funds = opp_q.4.as_ref().map_or(0, |h| h.funds);
                    let mine = charities_q.get(actor).ok();
                    if crate::charity::gives(actor_sim, mine, day) && funds > crate::charity::AMOUNTS[2].0 {
                        let causes = crate::charity::causes_for(actor_sim, mine, day);
                        if mine.is_none_or(|c| c.causes != causes) {
                            commands.entity(actor).insert(crate::charity::Charities { causes: causes.clone(), chosen_on: day, gave_on: mine.and_then(|c| c.gave_on) });
                        }
                        for cause in &causes {
                            let name = crate::charity::cause_name(layout_ui.as_deref(), cause);
                            let title = crate::charity::menu_title(actor_sim, &name);
                            let list: Vec<(String, ActionKind)> = crate::charity::AMOUNTS.iter().filter(|(a, _)| funds > *a).map(|(a, _)| (format!("Donate §{a}"), ActionKind::Object { target: t, def: i })).collect();
                            options.push((format!("{title}... ›"), submenu_kind(pie.submenus.len())));
                            pie.submenus.push((title, list));
                        }
                    }
                    continue;
                }
                // (The TV turned on when it's off; off when it's been left on.)
                let (on, left) = tv_on.get(t).unwrap_or((false, false));
                if (d.special == Special::TvTurnOn && (on || left)) || (d.special == Special::TvTurnOff && !left) {
                    continue;
                }
                // (Strength training at the stereo: teens and up, as the game's.)
                if matches!(d.special, Special::StrengthWorkOut | Special::SpongeBath) && !(actor_sim.age == crate::sim::Age::Teen || actor_sim.age.is_grown()) {
                    continue;
                }
                // (At the computer, the traits' own: a Genius's problems, a Workaholic's job, a
                // Bookworm's club.)
                let work_to_do = club_q.get(actor).ok().and_then(|c| c.1).is_some_and(|w| w.0 > opp_q.2.minutes);
                if !crate::computertraits::offers(d.special, actor_sim, jobs_q.get(actor).ok().flatten(), club_q.get(actor).is_ok_and(|c| c.0), work_to_do) {
                    continue;
                }
                // Chess, as the game's table offers it: practice alone, or a game with whoever's
                // there alone ("Play Chess With Bella Goth"); "Play Chess With..." anyone about,
                // with both chairs free; a game watched while there's one (and they're not in it).
                // (Played in the chair at it: not on the chair's own menu.)
                if matches!(d.special, Special::PlayChess | Special::ChessMatch) && obj.kind != ObjectKind::Chess {
                    continue;
                }
                if d.special == Special::WatchChess && !chess.watchable(t, actor) {
                    continue;
                }
                // A door of their own home: locked for everybody, all but themselves or the
                // household, or all but the Sims picked; or unlocked (the game's "Lock...").
                if d.special == Special::LockDoor {
                    use crate::doorlocks as dl;
                    if lot_objects_q.contains(t) || actor_sim.age.is_little() || actor_sim.age == crate::sim::Age::Child {
                        continue;
                    }
                    let lock = locks_q.get(t).ok();
                    if lock.is_some() {
                        options.push((dl::UNLOCK.to_string(), ActionKind::Object { target: t, def: i }));
                    }
                    let mut list: Vec<(String, ActionKind)> = [dl::EVERYBODY, dl::BUT_ME, dl::BUT_HOUSEHOLD]
                        .into_iter()
                        .filter(|l| !lock.is_some_and(|k| k.is(l, actor_sim)))
                        .map(|l| (l.to_string(), ActionKind::Object { target: t, def: i }))
                        .collect();
                    let here = on_lot.get(actor).ok().map(|l| l.0);
                    let picks: Vec<(String, ActionKind)> = company_q
                        .iter()
                        .filter(|c| c.3.map(|l| l.0) == here && !c.1.age.is_little())
                        .map(|c| (dl::picker_label(c.1, lock), ActionKind::Object { target: t, def: i }))
                        .collect();
                    if !picks.is_empty() {
                        list.push((format!("{}... ›", dl::SPECIFY), submenu_kind(pie.submenus.len() + 1)));
                    }
                    options.push((format!("{}... ›", dl::LOCK), submenu_kind(pie.submenus.len())));
                    pie.submenus.push((dl::LOCK.to_string(), list));
                    if !picks.is_empty() {
                        pie.submenus.push((dl::SPECIFY.to_string(), picks));
                    }
                    continue;
                }
                // A potty chair: sat on by toddlers; a grown-up's to empty once used, and to
                // "Potty Train..." the family's toddlers not yet trained on.
                if obj.kind == ObjectKind::PottyChair {
                    if !crate::potty::suits(d, actor_sim) || (d.special == Special::EmptyPotty && !potties.needs_emptying(t)) || d.special == Special::PottyLesson {
                        continue;
                    }
                    if d.special == Special::PottyTrain {
                        let here = on_lot.get(actor).ok().map(|l| l.0);
                        let toddlers = company_q
                            .iter()
                            .filter(|c| c.3.map(|l| l.0) == here)
                            .map(|c| (c.0, c.1, members_q.contains(c.0) || family.kin(actor_sim.id, c.1.id).is_some()));
                        let list: Vec<(String, ActionKind)> = crate::potty::trainable(&potties, toddlers)
                            .into_iter()
                            .map(|(_, name)| (format!("{} {name}", crate::potty::POTTY_TRAIN), ActionKind::Object { target: t, def: i }))
                            .collect();
                        match list.len() {
                            0 => {}
                            1 => options.extend(list),
                            _ => {
                                options.push((format!("{}... ›", crate::potty::POTTY_TRAIN), submenu_kind(pie.submenus.len())));
                                pie.submenus.push((crate::potty::POTTY_TRAIN.to_string(), list));
                            }
                        }
                        continue;
                    }
                }
                // (An instrument played for tips outdoors or on a community lot, as the game's;
                // watched while someone else is playing it.)
                if d.special == Special::PlayForTips {
                    if music.tips_ok(t) {
                        options.push((crate::tips::PLAY_FOR_TIPS.to_string(), ActionKind::Object { target: t, def: i }));
                    }
                    continue;
                }
                if d.special == Special::WatchMusic && !music.watchable(t, actor) {
                    continue;
                }
                if d.special == Special::PlayChess {
                    let players = chess.players(t);
                    if players.contains(&actor) {
                        continue;
                    }
                    let label = match players.as_slice() {
                        [one] => sims.get(*one).map_or(crate::chess::PRACTICE.to_string(), |s| format!("{} {}", crate::chess::PLAY_WITH, s.0.full_name())),
                        _ => crate::chess::PRACTICE.to_string(),
                    };
                    options.push((label, ActionKind::Object { target: t, def: i }));
                    let here = on_lot.get(actor).ok().map(|l| l.0);
                    let list: Vec<(String, ActionKind)> = company_q
                        .iter()
                        .filter(|c| c.0 != actor && !c.1.age.is_little() && c.3.map(|l| l.0) == here && !players.contains(&c.0))
                        .map(|c| (c.1.full_name(), ActionKind::Object { target: t, def: i }))
                        .collect();
                    if players.is_empty() && chess.free_chairs(t) >= 2 && !list.is_empty() {
                        options.push((format!("{}... ›", crate::chess::PLAY_WITH), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((crate::chess::PLAY_WITH.to_string(), list));
                    }
                    continue;
                }
                // The TV's channels: "Change to..." one of those the set gets, as the game's.
                if d.special == Special::TurnOnTv {
                    let level = tv_levels.get(t).map_or(2, |l| l.0);
                    let list: Vec<(String, ActionKind)> = crate::tvchannels::channels_for(level).map(|(_, c)| (c.name.to_string(), ActionKind::Object { target: t, def: i })).collect();
                    options.push((format!("{CHANGE_CHANNEL}... ›"), submenu_kind(pie.submenus.len())));
                    pie.submenus.push((CHANGE_CHANNEL.to_string(), list));
                }
                // (Online, someone they know who isn't here nor one of the household: the game's
                // "Chat...", `CanSimBeChattedWith`.)
                if d.special == Special::ChatOnline {
                    let here = on_lot.get(actor).ok().map(|l| l.0);
                    let present = |o: Entity| company_q.get(o).is_ok_and(|c| c.3.map(|l| l.0) == here);
                    let mut list: Vec<(String, ActionKind)> = rels
                        .0
                        .keys()
                        .filter(|o| **o != actor && !present(**o))
                        .filter_map(|o| sims.get(*o).ok())
                        .filter(|(s, member, _)| !member && s.age != crate::sim::Age::Toddler && crate::chatonline::may_chat(actor_sim, s, true))
                        .map(|(s, ..)| (s.full_name(), ActionKind::Object { target: t, def: i }))
                        .collect();
                    list.sort_by(|a, b| a.0.cmp(&b.0));
                    if !list.is_empty() {
                        options.push((format!("{CHAT_WITH}... ›"), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((CHAT_WITH.to_string(), list));
                    }
                }
                if d.special == Special::Homework && !hw_q.contains(actor) {
                    continue;
                }
                // A fish bowl: a fish from their inventory in, or the one there out.
                if d.special == Special::PlaceFish {
                    if bowl_q.contains(t) {
                        continue;
                    }
                    let list: Vec<(String, ActionKind)> = inv_q
                        .get(actor)
                        .map(|inv| inv.0.iter().filter(|s| s.kind == crate::inventory::ItemKind::Fish).map(|s| (format!("Place Fish: {}", s.name), ActionKind::Object { target: t, def: i })).collect())
                        .unwrap_or_default();
                    if !list.is_empty() {
                        options.push(("Place Fish ›".to_string(), submenu_kind(pie.submenus.len())));
                        pie.submenus.push(("Place Fish".to_string(), list));
                    }
                    continue;
                }
                // (A live fish taken out or fed; a dead one only cleaned out.)
                let fish = bowl_q.get(t).ok();
                if matches!(d.special, Special::TakeFish | Special::FeedFish) && !fish.is_some_and(|b| !b.dead) {
                    continue;
                }
                if d.special == Special::CleanBowl && !fish.is_some_and(|b| b.dead) {
                    continue;
                }
                // Fishing: with the bait used last, if they've some ("Fish with Apple (Minnow)",
                // the fish it's known for); or "Choose Bait...", none or one of those held.
                if d.special == Special::Fish
                    && let (Ok(inv), Some(ui)) = (inv_q.get(actor), opp_q.0.as_ref())
                {
                    let level = writers.get(actor).map_or(0, |w| w.1.level("Fishing"));
                    let journal = journals_q.get(actor).ok();
                    let known = |s: &crate::inventory::Stack| crate::bait::known_for(&ui.data, journal, &s.key);
                    let last = journal.and_then(|j| j.bait.as_ref()).and_then(|b| crate::bait::held(inv, b, level));
                    options.push((last.map_or(d.name.to_string(), |s| crate::bait::fish_with(s, known(s))), ActionKind::Object { target: t, def: i }));
                    let baits = crate::bait::baits(inv, level);
                    if !baits.is_empty() {
                        let mut list = vec![(crate::bait::NO_BAIT.to_string(), ActionKind::Object { target: t, def: i })];
                        list.extend(baits.into_iter().map(|s| (crate::bait::fish_with(s, known(s)), ActionKind::Object { target: t, def: i })));
                        options.push((format!("{}... ›", crate::bait::CHOOSE_BAIT), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((crate::bait::CHOOSE_BAIT.to_string(), list));
                    }
                    continue;
                }
                // (An empty trash can has nothing to take out.)
                if d.special == Special::Leftovers && leftovers.0.is_empty() {
                    continue;
                }
                // The leftovers in the fridge, each dish once ("Eat Leftovers..."), as the game's
                // `Fridge_EatLeftovers` lists them.
                if d.special == Special::Leftovers {
                    let Some(ui) = opp_q.0.as_ref() else { continue };
                    let mut list: Vec<(String, ActionKind)> = Vec::new();
                    for key in &leftovers.0 {
                        let Some(r) = ui.data.recipes.iter().find(|r| &r.key == key) else { continue };
                        if !list.iter().any(|(l, _)| *l == r.name) {
                            list.push((r.name.clone(), ActionKind::Object { target: t, def: i }));
                        }
                    }
                    if !list.is_empty() {
                        options.push((format!("{LEFTOVERS} ›"), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((LEFTOVERS.to_string(), list));
                    }
                    continue;
                }
                // A sprinkler: on or off, and played in while it's on.
                let on = sprinkling.contains(t);
                if (d.special == Special::SprinklerOn && on) || (matches!(d.special, Special::SprinklerOff | Special::PlayInSprinkler) && !on) {
                    continue;
                }
                if d.special == Special::EmptyTrash && !trash_q.get(t).is_ok_and(|f| f.0 > 0) {
                    continue;
                }
                // (The laundry's steps as it stands: a load on when there are clothes to wash, the
                // wet clothes out once washed, the dry ones once dry.)
                if !laundry.offers(d.special, obj.kind, t) {
                    continue;
                }
                // A stereo: on or off, and "Change to..." each of the game's stations (there and
                // then, as its `ChangeStation` is), beside Turn On or Turn Off.
                if obj.kind == ObjectKind::Stereo && matches!(d.special, Special::StereoOn | Special::StereoOff) {
                    let on = opp_q.8.get(t).ok().and_then(|s| s.0);
                    if d.special == Special::StereoOn {
                        let current = on.and_then(|o| o.0.clone());
                        let list: Vec<(String, ActionKind)> = crate::music::stations(opp_q.7.as_deref())
                            .into_iter()
                            .filter(|(k, _)| current.as_deref() != Some(*k))
                            .map(|(_, name)| (name, ActionKind::Object { target: t, def: i }))
                            .collect();
                        if !list.is_empty() {
                            options.push((format!("{CHANGE_STATION} ›"), submenu_kind(pie.submenus.len())));
                            pie.submenus.push((CHANGE_STATION.to_string(), list));
                        }
                    }
                    if (d.special == Special::StereoOn) == on.is_some() {
                        continue;
                    }
                }
                // (Cleaned only once it needs it.)
                if d.special == Special::Scrub && !opp_q.8.get(t).is_ok_and(|s| s.2.is_some_and(|g| g.needs_cleaning())) {
                    continue;
                }
                // (A bed's made only when it's unmade.)
                if d.special == Special::MakeBed && !opp_q.8.get(t).is_ok_and(|s| s.1) {
                    continue;
                }
                // (A job quit only by someone who has one.)
                if d.special == Special::QuitJob && jobs_q.get(actor).ok().flatten().is_none() {
                    continue;
                }
                // (The bills paid only when there are some in the mailbox, as the game offers.)
                if d.special == Special::PayBills && opp_q.4.as_ref().is_none_or(|h| h.bills.is_empty()) {
                    continue;
                }
                // (Dishes are washed up when cleared away, not from the menu.)
                if matches!(d.special, Special::WashDishes | Special::DropTrash | Special::LoadDryer | Special::ReadBook | Special::GetIngredients | Special::PrepFood | Special::PlaceMeal | Special::WatchTv | Special::WooHoo | Special::TryForBaby) || (d.special == Special::Cook && d.name == "Take Out Dinner") || (d.special == Special::ReadPaper && d.name == "Read the Paper") {
                    continue;
                }
                // (A snack's on the fridge's Have Quick Meal menu, not apart from it.)
                if obj.kind == ObjectKind::Fridge && d.name == "Grab a Snack" {
                    continue;
                }
                // The fridge's food menus (cooked at the stove nearest it, or heated in the
                // microwave), the stove's, and the microwave's quick meals.
                if (obj.kind == ObjectKind::Fridge && d.special == Special::Cook) || (obj.kind == ObjectKind::Stove && d.special == Special::ServeMeal) || d.special == Special::Microwave {
                    let (Ok((_, skills, known)), Some(ui)) = (writers.get(actor), opp_q.0.as_ref()) else { continue };
                    let at = pickable.get(t).map_or(Vec3::ZERO, |p| p.1.translation());
                    let nearest = |kind: ObjectKind, special: Special| -> Option<ActionKind> {
                        let def = interactions_for(kind).iter().position(|d| d.special == special)?;
                        pickable
                            .iter()
                            .filter(|(e, _, _, o, _)| o.is_some_and(|o| o.kind == kind) && !broken_q.contains(*e))
                            .map(|(e, tf, ..)| (e, tf.translation()))
                            .filter(|(_, p)| (p.y - at.y).abs() < 1.5)
                            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
                            .map(|(e, _)| ActionKind::Object { target: e, def })
                    };
                    let here = ActionKind::Object { target: t, def: i };
                    let (stove, fridge, microwave) = match obj.kind {
                        ObjectKind::Fridge => (nearest(ObjectKind::Stove, Special::ServeMeal), Some(here), nearest(ObjectKind::Microwave, Special::Microwave)),
                        ObjectKind::Stove => (Some(here), None, None),
                        _ => (None, None, Some(here)),
                    };
                    let time = (opp_q.2.hour_f(), opp_q.2.weekday());
                    let stock = (inv_q.get(actor).ok(), fridge_stock.stock());
                    food_menus(&mut pie, &mut options, &ui.data, actor_sim, skills.level("Cooking"), known, time, stove, (fridge, microwave), stock);
                    continue;
                }
                if d.special == Special::WriteNovel {
                    // Carry on with the book under way, or start one in a genre they can write.
                    let Ok((author, skills, _)) = writers.get(actor) else { continue };
                    if let Some(dr) = author.and_then(|a| a.draft.as_ref()) {
                        options.push((format!("Continue Writing “{}” ({:.0}%)", dr.title, dr.pages / dr.length * 100.0), ActionKind::Object { target: t, def: i }));
                    }
                    let data = opp_q.0.as_ref().map(|u| &*u.data);
                    let list: Vec<(String, ActionKind)> = crate::writing::GENRES
                        .iter()
                        .filter(|g| crate::writing::unlocked(g, actor_sim, skills, author, data))
                        .map(|g| (format!("Write: {}", g.name), ActionKind::Object { target: t, def: i }))
                        .collect();
                    options.push(("Write Novel ›".to_string(), submenu_kind(pie.submenus.len())));
                    pie.submenus.push(("Write Novel".to_string(), list));
                    continue;
                }
                // The grill's menu: the recipes they know that grill.
                if d.special == Special::ServeMeal
                    && obj.kind == ObjectKind::Grill
                    && let (Ok((_, skills, known)), Some(ui)) = (writers.get(actor), opp_q.0.as_ref())
                {
                    let list: Vec<(String, ActionKind)> = crate::meals::cookable(&ui.data, actor_sim, skills.level("Cooking"), known, 0xFF)
                        .into_iter()
                        .filter(|&r| crate::appliances::GRILL_RECIPES.contains(&ui.data.recipes[r].key.as_str()))
                        .map(|r| (format!("Grill: {}", ui.data.recipes[r].name), ActionKind::Object { target: t, def: i }))
                        .collect();
                    if !list.is_empty() {
                        options.push(("Grill ›".to_string(), submenu_kind(pie.submenus.len())));
                        pie.submenus.push(("Grill".to_string(), list));
                    }
                    continue;
                }
                // The Teleporter: the lots it goes to.
                if d.special == Special::Teleport {
                    let list: Vec<(String, ActionKind)> = (0..world.data.lots.len())
                        .filter(|&l| crate::visit::visitable(&world.data, l))
                        .map(|l| (crate::visit::place_name(&world.data, l), ActionKind::Teleport { pad: t, lot: l }))
                        .collect();
                    if !list.is_empty() {
                        options.push(("Teleport To ›".to_string(), submenu_kind(pie.submenus.len())));
                        pie.submenus.push(("Teleport To".to_string(), list));
                    }
                    continue;
                }
                // The body sculptor's shapes and the moodlet manager's moods: submenus.
                if matches!(d.special, Special::Sculpt | Special::SetMood) {
                    let defs = crate::interact::interactions_for(obj.kind);
                    if defs.iter().position(|x| x.special == d.special) == Some(i) {
                        let list: Vec<(String, ActionKind)> =
                            defs.iter().enumerate().filter(|(_, x)| x.special == d.special).map(|(j, x)| (x.name.to_string(), ActionKind::Object { target: t, def: j })).collect();
                        let title = if d.special == Special::Sculpt { "Sculpt" } else { "Set Mood" };
                        options.push((format!("{title} ›"), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((title.to_string(), list));
                    }
                    continue;
                }
                // The household's skill books, for those whose skill they'd teach.
                if d.special == Special::GetBook
                    && let (Some(ui), Ok((_, skills, _))) = (opp_q.0.as_ref(), writers.get(actor))
                {
                    let owned = crate::skillbooks::shelved(books_q.iter().filter(|b| b.1).map(|b| b.0));
                    let mine = books_q.get(actor).ok().map(|b| b.0);
                    let list: Vec<(String, ActionKind)> =
                        crate::skillbooks::readable(&ui.data.skill_books, &owned, mine, skills).into_iter().map(|(_, label)| (label, ActionKind::Object { target: t, def: i })).collect();
                    if !list.is_empty() {
                        options.push((format!("{READ_SKILL_BOOK} ›"), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((READ_SKILL_BOOK.to_string(), list));
                    }
                }
                // The household's own books, read for their own sake (the game's "Read...").
                if d.special == Special::GetBook
                    && let Some(ui) = opp_q.0.as_ref()
                {
                    let owned = crate::novels::shelved(novels_q.iter().filter(|b| b.1).map(|b| b.0));
                    let mine = novels_q.get(actor).ok().map(|b| b.0);
                    let list: Vec<(String, ActionKind)> = crate::novels::readable(&ui.data.novels, &owned, mine).into_iter().map(|(_, label)| (label, ActionKind::Object { target: t, def: i })).collect();
                    if !list.is_empty() {
                        options.push((format!("{READ_NOVEL} ›"), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((READ_NOVEL.to_string(), list));
                    }
                }
                // The canvas to paint on.
                if d.special == Special::SellPainting {
                    let list: Vec<(String, ActionKind)> =
                        crate::paintings::CANVASES.iter().map(|c| (format!("Paint: {c}"), ActionKind::Object { target: t, def: i })).collect();
                    options.push(("Paint ›".to_string(), submenu_kind(pie.submenus.len())));
                    pie.submenus.push(("Paint".to_string(), list));
                    continue;
                }
                if d.special == Special::ToggleAlarm {
                    let label = if opp_q.6.on { "Turn Off Alarm" } else { "Set Alarm" };
                    options.push((label.to_string(), ActionKind::Object { target: t, def: i }));
                    continue;
                }
                if d.special == Special::ChangeClothes {
                    // The outfits to change into (their uniform, if their job has one).
                    let uniform = jobs_q.get(actor).ok().flatten().is_some_and(|j| j.uniform(actor_sim).is_some());
                    let list: Vec<(String, ActionKind)> = crate::simbody::OutfitKind::CHOICES
                        .iter()
                        .filter(|k| **k != crate::simbody::OutfitKind::Career || uniform)
                        .map(|k| (format!("Change Into: {}", k.label()), ActionKind::Object { target: t, def: i }))
                        .collect();
                    options.push(("Change Into ›".to_string(), submenu_kind(pie.submenus.len())));
                    pie.submenus.push(("Change Into".to_string(), list));
                    continue;
                }
                if d.special == Special::FindJob {
                    // The day's job listings (see `jobpaper`), for teens and up.
                    if actor_sim.age == crate::sim::Age::Teen || actor_sim.age.is_grown() {
                        options.push((d.name.to_string(), ActionKind::Object { target: t, def: i }));
                    }
                } else if d.special != Special::EatMeal {
                    options.push((d.name.to_string(), ActionKind::Object { target: t, def: i }));
                }
            }
            // The upgrades a handy enough grown-up can make to it.
            if actor_sim.age.is_grown()
                && actor_sim.age != crate::sim::Age::Child
                && let Ok((_, skills, _)) = writers.get(actor)
            {
                let have = upg_q.get(t).map_or(0, |u| u.0);
                let level = skills.level("Handiness") as u32;
                let list: Vec<(String, ActionKind)> = crate::upgrades::Upgrade::ALL
                    .into_iter()
                    .filter(|u| have & u.bit() == 0 && level >= u.level())
                    .filter_map(|u| Some((format!("Upgrade: {}", u.name(obj.kind)?), ActionKind::Upgrade { target: t, bit: u.bit() })))
                    .collect();
                if !list.is_empty() {
                    options.push(("Upgrade ›".to_string(), submenu_kind(pie.submenus.len())));
                    pie.submenus.push(("Upgrade".to_string(), list));
                }
            }
            open_pie(&mut commands, &mut pie, cursor, &obj.name, actor, options);
        }
    } else if actor_sim.age == crate::sim::Age::Baby {
        // Babies stay put.
    } else if let Some((p, level)) = floor_hit(ray, &world, building.as_deref()) {
        let mut options = vec![("Go Here".to_string(), ActionKind::GoHere(Vec2::new(p.x, p.z), level))];
        let mut title = String::new();
        let here = crate::rabbitholes::lot_at(&world.data.lots, p);
        let out_at = on_lot.get(actor).ok().map(|l| l.0);
        let from = postures.get(actor).ok().map(|(_, tf, _)| tf.translation());
        // (At a jog, for the athletic: the game's "Jog Here".)
        if let (Some(from), Ok((_, skills, _))) = (from, writers.get(actor))
            && crate::jog::can_jog_here(actor_sim, skills.level("Athletic"), from, p)
        {
            options.push(("Jog Here".to_string(), ActionKind::GoHere(Vec2::new(p.x, p.z), level)));
        }
        // (With others of the household there, somewhere a way off: "Go Here With...".)
        if let Some(from) = from.filter(|f| f.xz().distance(p.xz()) > GO_HERE_WITH_DISTANCE) {
            let mine = on_lot.get(actor).ok().map(|l| l.0);
            let others: Vec<String> = company_q
                .iter()
                .filter(|c| c.0 != actor && !c.2 && !c.1.age.is_little() && c.3.map(|l| l.0) == mine && postures.get(c.0).is_ok_and(|(_, tf, _)| tf.translation().distance(from) < 60.0))
                .map(|c| c.1.full_name())
                .collect();
            if !others.is_empty() {
                let mut list: Vec<(String, ActionKind)> = others.iter().map(|n| (n.clone(), ActionKind::GoHere(Vec2::new(p.x, p.z), level))).collect();
                if others.len() > 1 {
                    list.insert(0, ("Everyone".to_string(), ActionKind::GoHere(Vec2::new(p.x, p.z), level)));
                }
                options.push((format!("{GO_HERE_WITH} ›"), submenu_kind(pie.submenus.len())));
                pie.submenus.push((GO_HERE_WITH.to_string(), list));
            }
        }
        // A community lot: its rabbit hole's activities, and opportunities done there.
        if let Some(lot) = here.filter(|l| Some(*l) != out_at) {
            let l = &world.data.lots[lot];
            let acts = crate::rabbitholes::activities(l);
            let opp_tasks = opp_q
                .0
                .as_ref()
                .map(|ui| crate::opportunities::lot_options(&world.data, lot, opp_q.1.single().ok().flatten(), &ui.data, crate::opportunities::Today::of(&opp_q.2, jobs_q.get(actor).ok().flatten())))
                .unwrap_or_default();
            let mut opp_tasks: Vec<(String, ActionKind)> = opp_tasks.into_iter().chain(crate::gardening::lot_options(&world.data, lot)).collect();
            // (Its skill classes, in class hours.)
            if let Ok((s, taken)) = classes_q.get(actor) {
                let clock = &opp_q.2;
                opp_tasks.extend(crate::classes::lot_classes(l, clock.hour_f(), clock.day(), s.age, taken, None).into_iter().map(|(label, task)| (label, ActionKind::Visit { lot, activity: task })));
                // (The venue's own: its show, its game, its meals, its treatments...)
                opp_tasks.splice(0..0, crate::venues::world_lot_options(&world.data, lot, clock.day(), clock.hour_f(), s.age).into_iter().map(|(label, task)| (label, ActionKind::Visit { lot, activity: task })));
            }
            let visit = crate::visit::visitable(&world.data, lot);
            if !acts.is_empty() || !opp_tasks.is_empty() || visit {
                title = crate::rabbitholes::lot_title(l, world.data.lot_names.get(lot).map_or("", |s| s.as_str()));
                options.clear();
                if visit {
                    options.push(("Visit".to_string(), ActionKind::GoToLot { lot }));
                }
                for (i, a) in acts.iter().enumerate() {
                    options.push((activity_label(a), ActionKind::Visit { lot, activity: i }));
                }
                // (Going out with someone: those about with the Sim, as the game's "...With".)
                let here = on_lot.get(actor).ok().map(|l| l.0);
                let company: Vec<String> = company_q
                    .iter()
                    .filter(|c| c.0 != actor && !c.1.age.is_little() && c.3.map(|l| l.0) == here && (!c.2 || here.is_none()))
                    .map(|c| c.1.full_name())
                    .collect();
                let outings: Vec<(String, usize)> = opp_tasks.iter().filter_map(|(_, k)| match k {
                    ActionKind::Visit { activity, .. } if crate::venues::groupable(*activity) => Some((crate::venues::outing_name(*activity)?.to_string(), *activity)),
                    _ => None,
                }).collect();
                options.extend(opp_tasks);
                if !company.is_empty() {
                    for (name, task) in outings {
                        let list: Vec<(String, ActionKind)> = company.iter().map(|c| (c.clone(), ActionKind::Visit { lot, activity: task })).collect();
                        options.push((format!("{name}{} ›", crate::venues::WITH), submenu_kind(pie.submenus.len())));
                        pie.submenus.push((format!("{name}{}", crate::venues::WITH), list));
                    }
                }
                // (The bookstore's books are in its shop: `shop`.)
            }
            // Planting a seed outdoors on the home lot.
            let home = opp_q.4.as_ref().map(|h| h.lot_index) == Some(lot);
            let outdoors = building.as_deref().is_none_or(|b| !b.is_indoors(p));
            if home && outdoors && level <= 1 {
                if let (Some(g), Some(ui)) = (opp_q.5.as_ref(), opp_q.0.as_ref()) {
                    options.extend(crate::gardening::plant_options(g, &ui.data, Vec2::new(p.x, p.z), level));
                }
            }
        }
        // Seasons: out in the snow, a snowman or a snow angel; in snow or rain, catching it.
        if !actor_sim.age.is_little() && level <= 1 && !crate::weather::sheltered(building.as_deref(), p) {
            for what in crate::seasonal::Outdoor::options(&weather, opp_q.2.minutes) {
                options.push((what.label().to_string(), ActionKind::Outdoor { at: Vec2::new(p.x, p.z), level, what }));
            }
        }
        // A Kleptomaniac out on another's lot after dark: something near there pocketed, if no
        // one's in the room to see.
        if let Some(lot) = out_at.filter(|l| here == Some(*l))
            && let Some(ui) = opp_q.0.as_ref()
            && crate::klepto::ready(actor_sim, klepto_q.2.get(actor).ok(), opp_q.2.hour_f(), opp_q.2.day(), true)
        {
            let (things, about) = crate::klepto::lot_scene(&ui.data, building.as_deref(), &klepto_q.0, &klepto_q.1, lot);
            let room = building.as_deref().and_then(|b| b.room_id(level, p));
            if !crate::klepto::watched(actor, level, room, &about)
                && let Some(t) = crate::klepto::pick(&things, p, level, room)
            {
                options.push((crate::klepto::SWIPE.to_string(), ActionKind::Outdoor { at: t.at.xz(), level: t.level, what: crate::seasonal::Outdoor::Swipe }));
            }
        }
        // Out on a community lot: home again.
        if out_at.is_some() {
            options.insert(0, ("Go Home".to_string(), ActionKind::GoHomeFromLot));
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
    (ui_data, inventories, people, locks, postures, (sofas, cuddles, rels_q)): (
        Option<Res<crate::icons::GameUi>>,
        Query<&crate::inventory::Inventory>,
        Query<(Entity, &Sim, Has<HouseholdMember>)>,
        Query<&crate::doorlocks::DoorLock>,
        Query<(&crate::sim::SimAnim, &GlobalTransform, Option<&crate::nav::Floor>)>,
        (Query<&GameObject>, Query<&crate::seatedtalk::Cuddling>, Query<&Relationships>),
    ),
    mut jobs: Query<&mut crate::careers::Job>,
    (back, windows, sounds): (Query<&Interaction, (Changed<Interaction>, With<crate::piemenu::PieBack>)>, Query<&Window, With<PrimaryWindow>>, Option<Res<crate::sound::Sounds>>),
) {
    // A submenu's middle: back to the menu it was opened from, where that was.
    if back.iter().any(|i| *i == Interaction::Pressed)
        && let (Some((at, title, options)), Some(a)) = (pie.parents.pop(), pie.actor)
    {
        spawn_pie(&mut commands, &mut pie, at, &title, a, options);
        return;
    }
    let mut chosen = None;
    for (i, opt) in &q {
        if *i == Interaction::Pressed {
            chosen = Some(opt.0);
        }
    }
    let Some(idx) = chosen else { return };
    let Some((mut label, kind)) = pie.options.get(idx).cloned() else { return };
    let actor = pie.actor;
    let title = std::mem::take(&mut pie.title);
    let parent = (pie.at, title.clone(), pie.options.clone());
    close_pie(&mut commands, &mut pie);
    if let ActionKind::BuyReward(_) = kind {
        return;
    }
    if label == PHONE_LABEL {
        commands.insert_resource(OpenPhone);
        return;
    }
    // (A bed's side given there and then: the game's `SetBedOwnership` is immediate.)
    if title == SET_BED_OWNER
        && let ActionKind::Object { target, .. } = kind
        && let Some((_, s, _)) = people.iter().find(|(_, s, _)| s.full_name() == label)
    {
        let id = s.id;
        notes.push(format!("{} has this bed now.", s.first));
        commands.queue(move |w: &mut World| crate::bedowners::assign(w, target, id));
        return;
    }
    // (The phones silenced or not, there and then.)
    if let ActionKind::SilencePhone = kind {
        commands.queue(|w: &mut World| {
            let mut s = w.resource_mut::<crate::calls::PhonesSilenced>();
            s.0 = !s.0;
        });
        return;
    }
    // (An inventory stack's actions: see `hudpanels::inventory_panel`.)
    if let ActionKind::InventoryItem(b, owner, stack) = kind {
        commands.insert_resource(crate::inventory::DoItem(b, owner, stack));
        return;
    }
    // (How a Sim at work works.)
    if let Some(t) = label.strip_prefix(crate::hudpanels::TONE_PREFIX)
        && let Some(mut j) = actor.and_then(|a| jobs.get_mut(a).ok())
    {
        let skill = j.career().skill;
        if let Some(tone) = crate::careers::WorkTone::ALL.into_iter().find(|w| t.trim_end_matches(" ✓") == w.label(skill)) {
            j.tone = tone;
        }
        return;
    }
    let _ = (&mut wishes, &mut notes);
    // A TV turned on (and left on) or off, there and then (the game's are immediate).
    if let ActionKind::Object { target, def } = kind
        && let Some(special) = sofas.get(target).ok().and_then(|o| crate::interact::interactions_for(o.kind).get(def)).map(|d| d.special)
        && matches!(special, Special::TvTurnOn | Special::TvTurnOff)
    {
        if special == Special::TvTurnOn {
            commands.entity(target).insert(crate::effects::TvLeftOn);
        } else {
            commands.entity(target).remove::<crate::effects::TvLeftOn>();
        }
        return;
    }
    // A stereo changed to a station, there and then.
    if title == CHANGE_STATION
        && let ActionKind::Object { target, .. } = kind
        && let Some((key, _)) = crate::music::stations(sounds.as_deref()).into_iter().find(|(_, n)| *n == label)
    {
        commands.entity(target).insert(crate::music::StereoOn(Some(key.to_string())));
        return;
    }
    // A door locked or unlocked, there and then (the game's are immediate).
    if let ActionKind::Object { target, .. } = kind
        && (title == crate::doorlocks::LOCK || title == crate::doorlocks::SPECIFY || label == crate::doorlocks::UNLOCK)
        && let Some((_, me, _)) = actor.and_then(|a| people.get(a).ok())
    {
        let picked = (title == crate::doorlocks::SPECIFY).then(|| people.iter().find(|p| p.1.full_name() == label.trim_end_matches(" ✓")).map(|p| p.1)).flatten();
        if let Some(lock) = crate::doorlocks::DoorLock::chosen(&label, me, locks.get(target).ok(), picked) {
            match lock {
                Some(l) => {
                    commands.entity(target).insert(l);
                }
                None => {
                    commands.entity(target).remove::<crate::doorlocks::DoorLock>();
                }
            }
        }
        return;
    }
    // A toddler potty trained: the grown-up to the chair, the toddler sat on it once they're there.
    if let ActionKind::Object { .. } = kind
        && let Some(name) = label.strip_prefix(&format!("{} ", crate::potty::POTTY_TRAIN))
        && let (Some(a), Some((toddler, _))) = (actor, people.iter().find(|p| p.1.first == name && p.1.age == crate::sim::Age::Toddler).map(|p| (p.0, p.1)))
        && let Ok(mut q) = queues.get_mut(a)
    {
        q.push_player(Action::new(label.clone(), kind.clone(), false));
        commands.entity(a).insert(crate::potty::PottyTraining { toddler, asked: false });
        return;
    }
    // Chess with someone: both to the table, a chair each.
    if title == crate::chess::PLAY_WITH
        && let ActionKind::Object { target, def } = kind
        && let (Some(a), Some(other)) = (actor, people.iter().find(|p| p.1.full_name() == label).map(|p| (p.0, p.1)))
        && let Ok(me) = people.get(a).map(|p| (p.0, p.1))
    {
        crate::chess::play_with(&mut queues, target, def, me, other);
        return;
    }
    // An outing with someone: off they both go.
    if let (Some(name), ActionKind::Visit { .. }) = (title.strip_suffix(crate::venues::WITH), &kind)
        && let (Some(a), Some((companion, ..))) = (actor, people.iter().find(|p| p.1.full_name() == label))
    {
        for (who, other) in [(a, companion), (companion, a)] {
            if let Ok(mut q) = queues.get_mut(who) {
                q.push_player(Action::new(name, kind.clone(), false));
            }
            commands.entity(who).insert(crate::venues::OutingWith(other));
        }
        return;
    }
    // A long menu's next page, where it was.
    if let Some(i) = as_more(&kind)
        && let (Some(page), Some(a)) = (pie.pages.get(i).cloned(), actor)
    {
        spawn_pie(&mut commands, &mut pie, parent.0, &title, a, page);
        return;
    }
    // (Opened where its option was clicked, as the game's are.)
    if let Some(i) = as_submenu(&kind)
        && let (Some((title, list)), Some(a)) = (pie.submenus.get(i).cloned(), actor)
    {
        let at = windows.single().ok().and_then(|w| w.cursor_position()).unwrap_or(pie.at);
        open_submenu(&mut commands, &mut pie, at, &title, a, list, parent);
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
    // (Moving out, some of the household: who, kept for the move; the call made.)
    if let (ActionKind::Social { target, social: crate::moveout::MOVE_OUT | crate::moveout::MOVE_OUT_TOGETHER }, Some(a)) = (&kind, actor)
        && let Ok(mut queue) = queues.get_mut(a)
    {
        let together = matches!(kind, ActionKind::Social { social: crate::moveout::MOVE_OUT_TOGETHER, .. });
        let mover = people.get(*target).ok().map(|(_, s, _)| s.id);
        let partnered = |e: Entity| {
            rels_q.get(*target).is_ok_and(|r| r.0.get(&e).is_some_and(|x| matches!(x.status, crate::social::RelStatus::Partner | crate::social::RelStatus::Engaged | crate::social::RelStatus::Married)))
        };
        let partner = together.then(|| people.iter().find(|(e, _, m)| *m && *e != *target && partnered(*e)).map(|(_, s, _)| s.id)).flatten();
        let movers: Vec<u64> = mover.into_iter().chain(partner).collect();
        commands.insert_resource(crate::moveout::MoveOutChoice(movers));
        queue.push_player(Action::new("Move Out", ActionKind::MoveHouse, false));
        return;
    }
    // (The whole household moving: no one split off.)
    if matches!(kind, ActionKind::MoveHouse) {
        commands.remove_resource::<crate::moveout::MoveOutChoice>();
    }
    // (Called over to join the conversation: the call over, then their Chat.)
    if let (ActionKind::Social { target, social: crate::groups::INVITE }, Some(a)) = (&kind, actor)
        && let Ok(mut queue) = queues.get_mut(a)
    {
        queue.0.push_front(crate::groups::invite_action(*target));
        commands.entity(a).insert((crate::groups::CallingOver(*target), crate::anim::ActionClip::new(Some("a_soc_callOver_x"), &[])));
        return;
    }
    // (A cuddle with someone on the same sofa: worked out before the queue's taken.)
    let spot = |e: Entity| crate::seatedtalk::cuddle_spot(queues.get(e).ok(), |o| sofas.get(o).ok().map(|g| g.kind));
    let cuddle_ok = match (actor, &kind) {
        (Some(a), ActionKind::Social { target, .. }) => spot(a).is_some() && spot(a) == spot(*target),
        _ => false,
    };
    // (Lying in bed together: had where they lie.)
    let in_bed = cuddle_ok && actor.and_then(spot).is_some_and(|s| s.1);
    let cuddling = match (actor, &kind) {
        (Some(a), ActionKind::Social { target, .. }) => cuddle_ok && cuddles.get(a).is_ok_and(|c| c.with == *target && c.accepted),
        _ => false,
    };
    if let Some(a) = actor
        && let Ok(mut queue) = queues.get_mut(a)
    {
        let mut recipe_choice = label.strip_prefix("Cook: ").or_else(|| label.strip_prefix("Grill: "))
            .and_then(|n| ui_data.as_ref().and_then(|ui| ui.data.recipes.iter().position(|r| r.name == n)));
        // A dish off the food menus: which, and whether for one ("Have Dinner") or everyone.
        let mut for_one = false;
        // (A dish of leftovers: "Eat Leftover Spaghetti".)
        if title == LEFTOVERS
            && let Some(r) = ui_data.as_ref().and_then(|ui| ui.data.recipes.iter().position(|r| r.name == label))
        {
            recipe_choice = Some(r);
            label = format!("Eat Leftover {label}");
        }
        let snack = title == QUICK_MEAL;
        let verb = if snack || title.starts_with("Have ") { Some("Have") } else { title.starts_with("Serve ").then_some("Serve") };
        if let Some(verb) = verb
            && let Some(ui) = ui_data.as_ref()
        {
            let name = label.split(" (§").next().unwrap_or_default().to_string();
            if let Some(r) = ui.data.recipes.iter().position(|r| r.name == name && r.snack == snack) {
                recipe_choice = Some(r);
                for_one = verb == "Have" && !snack;
                label = format!("{verb} {name}");
            }
        }
        // The fish chosen for a bowl.
        let mut fish_choice = label.strip_prefix("Place Fish: ")
            .and_then(|n| inventories.get(a).ok().and_then(|inv| inv.0.iter().find(|s| s.kind == crate::inventory::ItemKind::Fish && s.name == n)))
            .map(|s| (s.key.clone(), s.quality));
        // (The bait chosen to fish with, as the menu offered it; "No Bait", none.)
        if let ActionKind::Object { .. } = kind {
            if label == crate::bait::NO_BAIT {
                fish_choice = Some((String::new(), 0));
                label = "Fish".to_string();
            } else if let Some(b) = inventories.get(a).ok().and_then(|inv| crate::bait::chosen(&label, inv, u32::MAX)) {
                fish_choice = Some(b);
                label = "Fish".to_string();
            }
        }
        let outfit_choice = label.strip_prefix("Change Into: ").and_then(|n| crate::simbody::OutfitKind::CHOICES.into_iter().find(|k| k.label() == n));
        let canvas_choice = label.strip_prefix("Paint: ").and_then(|n| crate::paintings::CANVASES.iter().position(|c| *c == n)).map(|c| c as u8);
        // (The channel chosen from "Change to...".)
        let channel_choice = (title == CHANGE_CHANNEL).then(|| crate::tvchannels::CHANNELS.iter().position(|c| c.name == label)).flatten().map(|c| c as u8);
        if channel_choice.is_some() {
            label = format!("Watch {label}");
        }
        // (The skill book chosen from the shelf: "Read a Book", that book.)
        let book_choice = (title == READ_SKILL_BOOK)
            .then(|| ui_data.as_ref().and_then(|u| u.data.skill_books.iter().position(|b| label == b.name || label.starts_with(&format!("{} (", b.name)))))
            .flatten()
            .map(|b| b as u16)
            // (Or one of the household's own books.)
            .or_else(|| (title == READ_NOVEL).then(|| ui_data.as_ref().and_then(|u| crate::novels::by_label(&u.data.novels, &label))).flatten().map(|b| crate::novels::NOVEL_CHOICE + b as u16));
        if book_choice.is_some() {
            label = "Read a Book".to_string();
        }
        // Keep the genre with its action: later queued choices must not replace
        // the draft currently being written or survive cancellation of that choice.
        let novel_genre = label.strip_prefix("Write: ").and_then(|n| crate::writing::GENRES.iter().position(|g| g.name == n));
        // (A social chosen from a group's menu by the rest of its name: its whole name.)
        if let ActionKind::Social { social, .. } = kind {
            label = crate::social::social(social).name.to_string();
        }
        // (Others of the household along too: "Go Here With...".)
        let go_with = title == GO_HERE_WITH;
        let near = |e: Entity| match (postures.get(a), postures.get(e)) {
            (Ok((_, x, _)), Ok((_, y, _))) => x.translation().distance(y.translation()) < 60.0,
            _ => false,
        };
        let followers: Vec<Entity> = if go_with {
            people.iter().filter(|(e, s, member)| *e != a && *member && !s.age.is_little() && near(*e) && (label == "Everyone" || s.full_name() == label)).map(|(e, ..)| e).collect()
        } else {
            Vec::new()
        };
        if go_with {
            label = "Go Here".to_string();
        }
        let jog = label == "Jog Here";
        // (A donation chosen: to which cause, how much.)
        let donation = title
            .strip_prefix("Donate to ")
            .zip(label.strip_prefix("Donate §").and_then(|a| a.parse::<i64>().ok()))
            .map(|(cause, amount)| (cause.trim_start_matches("Undermine ").to_string(), amount));
        if donation.is_some() {
            label = "Donate to Charity".to_string();
        }
        // (Someone known chatted with online: "Chat...".)
        let chat_with = (title == CHAT_WITH).then(|| people.iter().find(|(_, s, _)| s.full_name() == label).map(|(e, ..)| e)).flatten();
        if chat_with.is_some() {
            label = format!("Chat with {label}");
        }
        let mut action = Action::new(label, kind.clone(), false);
        action.jog = jog;
        action.chat_with = chat_with;
        action.donation = donation;
        action.novel_genre = novel_genre;
        action.outfit_choice = outfit_choice;
        action.canvas_choice = canvas_choice;
        action.channel_choice = channel_choice;
        action.book_choice = book_choice;
        action.fish_choice = fish_choice;
        action.recipe_choice = recipe_choice;
        action.for_one = for_one;
        // (Sat near each other, a talk had where they sit: before what they sat down to, which
        // carries on after.)
        if let ActionKind::Social { target, social } = action.kind
            && (crate::social::seated_ok(crate::social::social(social))
                || (crate::social::social(social).name == crate::seatedtalk::CUDDLE && cuddle_ok)
                || (cuddling && crate::seatedtalk::WHILE_CUDDLING.contains(&crate::social::social(social).name)))
        {
            let posture = |e: Entity| postures.get(e).ok().map(|(an, tf, f)| (an, tf.translation(), f.map_or(1, |f| f.0)));
            if in_bed || matches!((posture(a), posture(target)), (Some(me), Some(them)) if crate::seatedtalk::sat_together(me, them)) {
                action.seated = true;
                queue.0.push_front(action);
                return;
            }
        }
        queue.push_player(action);
        // (Those along with them: there too, a step apart.)
        if let ActionKind::GoHere(p, level) = kind {
            for (k, f) in followers.into_iter().enumerate() {
                let a = (k as f32 + 1.0) * 2.1;
                let at = p + Vec2::new(a.cos(), a.sin()) * 1.2;
                if let Ok(mut q) = queues.get_mut(f) {
                    q.push_player(Action::new("Go Here", ActionKind::GoHere(at, level), false));
                }
            }
        }
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
    buy: Res<crate::buy::BuyMode>,
    menu: Res<crate::options::GameMenu>,
) {
    for (i, s) in &speed {
        if *i == Interaction::Pressed && !buy.active && !menu.is_open() {
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

/// Floor buttons step the viewed floor; the label shows which floor is in view. The walls
/// button (and Home) cycles walls up, cutaway and down.
#[allow(clippy::type_complexity)]
fn floor_controls(
    building: Option<ResMut<crate::building::ActiveBuilding>>,
    buttons: Query<(&Interaction, &FloorButton), Changed<Interaction>>,
    mut label: Query<&mut Text, (With<FloorText>, Without<WallText>)>,
    mut controls: Query<&mut Node, With<FloorControls>>,
    (wall_button, mut wall_text, mut walls, keys): (
        Query<&Interaction, (Changed<Interaction>, With<WallButton>)>,
        Query<&mut Text, (With<WallText>, Without<FloorText>)>,
        ResMut<crate::building::WallMode>,
        Res<ButtonInput<KeyCode>>,
    ),
) {
    if wall_button.iter().any(|i| *i == Interaction::Pressed) || keys.just_pressed(KeyCode::Home) {
        *walls = walls.next();
    }
    for mut t in &mut wall_text {
        if t.0 != walls.label() {
            t.0 = walls.label().to_string();
        }
    }
    let Some(mut b) = building else {
        for mut n in &mut controls {
            n.display = Display::None;
        }
        return;
    };
    for (i, f) in &buttons {
        if *i == Interaction::Pressed {
            b.view_level = (b.view_level as i8 + f.0).clamp(b.data.lowest_storey() as i8, b.top_level as i8) as u8;
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

fn pie_bubble_visuals(mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<PieBubble>)>) {
    for (i, mut bg) in &mut q {
        bg.0 = match i {
            Interaction::Pressed => BUBBLE_PRESS,
            Interaction::Hovered => BUBBLE_HOVER,
            Interaction::None => BUBBLE,
        };
    }
}

/// PIE=1: a pie menu opens in the middle of the screen (tests).
fn test_pie(mut commands: Commands, mut pie: ResMut<PieMenu>, time: Res<Time>, sel: Query<Entity, With<Selected>>, windows: Query<&Window, With<PrimaryWindow>>, mut done: Local<bool>) {
    if *done || std::env::var("PIE").is_err() || time.elapsed_secs() < 8.0 {
        return;
    }
    let (Ok(actor), Ok(w)) = (sel.single(), windows.single()) else { return };
    *done = true;
    // (PIE=<n>: n numbered options, a long menu's pages.)
    let options = match std::env::var("PIE").ok().and_then(|v| v.parse::<usize>().ok()) {
        Some(n) => (1..=n).map(|i| (format!("Option {i}"), ActionKind::EatHere)).collect(),
        None => ["Have Dinner ›", "Serve Dinner ›", "Serve Dessert ›", "Have Quick Meal ›", "Bake Birthday Cake", "Clean"].iter().map(|s| (s.to_string(), ActionKind::EatHere)).collect(),
    };
    open_pie(&mut commands, &mut pie, Vec2::new(w.width() * 0.5, w.height() * 0.45), "Fridge", actor, options);
}

/// The acting Sim's portrait in the middle of a pie menu.
fn pie_portrait(
    mut commands: Commands,
    centers: Query<(Entity, &PieCenter), Without<ImageNode>>,
    (mut portraits, mut images): (ResMut<crate::portraits::Portraits>, ResMut<Assets<Image>>),
) {
    for (e, c) in &centers {
        let h = portraits.portrait(&mut images, c.0);
        commands.entity(e).insert((ImageNode::new(h), crate::portraits::PortraitOf(c.0)));
    }
}

fn hud_button_visuals(mut q: Query<(&Interaction, &mut BackgroundColor, Option<&SpeedButton>), With<HudButton>>, clock: Res<GameClock>, modal: Query<(), With<crate::dialog::Modal>>) {
    // (Paused while a question waits for an answer.)
    let speed_now = if modal.is_empty() { clock.speed } else { 0 };
    for (i, mut bg, speed) in &mut q {
        let active = speed.is_some_and(|s| s.0 == speed_now);
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
    ui: Option<Res<crate::icons::GameUi>>,
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
        let s = if sk.is_empty() || ui.is_some() { job_s } else { format!("{job_s} · Skills: {}", sk.join(", ")) };
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
    mut t: Query<&mut Text, (With<ClockText>, Without<FundsText>, Without<SeasonText>)>,
    mut f: Query<&mut Text, (With<FundsText>, Without<ClockText>, Without<SeasonText>)>,
    (mut season, weather, world): (Query<&mut Text, (With<SeasonText>, Without<ClockText>, Without<FundsText>)>, Res<crate::weather::Weather>, Option<Res<crate::data::SelectedWorld>>),
) {
    if let Ok(mut t) = t.single_mut() {
        let s = format!("{} {}", clock.weekday_name(), clock.time_string());
        if t.0 != s {
            t.0 = s;
        }
    }
    // (As the game's: the season and its day, the temperature, the weather, and the moon.)
    if let Ok(mut t) = season.single_mut() {
        let s = if world.is_some_and(|w| crate::weather::vacation(&w.0.name)) || weather.temperature.is_nan() {
            String::new()
        } else {
            // (The moon while it's up.)
            let h = clock.hour_f();
            let moon = if !(6.0..18.0).contains(&h) { format!(" · {}", crate::supernatural::moon_name(clock.minutes)) } else { String::new() };
            format!(
                "{} {}/{} · {:.0}°F · {}{moon}",
                weather.season(clock.day()).name(),
                weather.season_day(clock.day()),
                weather.season_days,
                weather.temperature,
                weather.describe(),
            )
        };
        if t.0 != s {
            t.0 = s;
        }
    }
    if let (Ok(mut f), Some(h)) = (f.single_mut(), household) {
        let s = format!("§{}", crate::lifetime::group(h.funds));
        if f.0 != s {
            f.0 = s;
        }
    }
}

/// What a queued action is shown with, as the game's queue: the object's catalogue picture, or
/// the other Sim's portrait.
#[derive(Clone, PartialEq)]
enum QueuePicture {
    None,
    Object(s3bake::Key),
    Sim(Entity),
}

#[allow(clippy::type_complexity)]
fn update_queue_panel(
    mut commands: Commands,
    panel: Query<Entity, With<QueuePanel>>,
    sel: Query<&ActionQueue, With<Selected>>,
    objects: Query<&GameObject>,
    sims: Query<(), With<Sim>>,
    mut last: Local<Vec<(String, bool)>>,
    (mut ui, mut portraits, mut images): (Option<ResMut<crate::icons::GameUi>>, ResMut<crate::portraits::Portraits>, ResMut<Assets<Image>>),
) {
    let Ok(p) = panel.single() else { return };
    let entries: Vec<(String, QueuePicture)> = sel
        .single()
        .map(|q| {
            q.0.iter()
                .filter(|a| !a.cancel)
                .map(|a| {
                    let pic = match &a.kind {
                        ActionKind::Object { target, .. } | ActionKind::Repair { target } | ActionKind::Upgrade { target, .. } => {
                            objects.get(*target).map_or(QueuePicture::None, |o| QueuePicture::Object(o.objd))
                        }
                        ActionKind::Social { target, .. } | ActionKind::PhoneChat { target } | ActionKind::Invite { target } if sims.contains(*target) => QueuePicture::Sim(*target),
                        _ => QueuePicture::None,
                    };
                    (a.label.clone(), pic)
                })
                .collect()
        })
        .unwrap_or_default();
    let sig: Vec<(String, bool)> = entries.iter().map(|(l, p)| (l.clone(), *p != QueuePicture::None)).collect();
    if *last == sig {
        return;
    }
    *last = sig;
    commands.entity(p).despawn_children();
    commands.entity(p).with_children(|c| {
        for (i, (l, pic)) in entries.iter().enumerate() {
            let image = match pic {
                QueuePicture::Object(objd) => ui.as_deref_mut().and_then(|u| u.icon(&mut images, &s3bake::gamedata::thumb_name(objd.2))).map(|h| (h, None)),
                QueuePicture::Sim(e) => Some((portraits.portrait(&mut images, *e), Some(*e))),
                QueuePicture::None => None,
            };
            c.spawn((
                Button,
                HudButton,
                QueueButton(i),
                Node {
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    padding: UiRect::axes(Val::Px(if image.is_some() { 6.0 } else { 12.0 }), Val::Px(if image.is_some() { 4.0 } else { 8.0 })),
                    border: UiRect::all(Val::Px(2.0)),
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(if i == 0 { PLUMBOB_GREEN } else { Color::WHITE }),
                BackgroundColor(BTN_NORMAL),
                crate::icons::Tooltip(format!("{l} (click to cancel)")),
            ))
            .with_children(|b| {
                if let Some((h, of)) = image {
                    let mut img = b.spawn((
                        ImageNode::new(h),
                        Node { width: Val::Px(30.0), height: Val::Px(30.0), border_radius: BorderRadius::all(Val::Px(6.0)), ..default() },
                        Pickable::IGNORE,
                    ));
                    if let Some(e) = of {
                        img.insert(crate::portraits::PortraitOf(e));
                    }
                }
                b.spawn((text(l.clone(), 15.0, Color::WHITE), Pickable::IGNORE));
            });
        }
    });
}

fn update_members_panel(
    mut commands: Commands,
    panel: Query<Entity, With<MembersPanel>>,
    members: Query<(Entity, &Sim, &crate::life::Mood, Has<Selected>), With<HouseholdMember>>,
    mut last: Local<Vec<(Entity, bool, u8)>>,
    (mut portraits, mut images): (ResMut<crate::portraits::Portraits>, ResMut<Assets<Image>>),
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
                    width: Val::Px(170.0),
                    padding: UiRect::all(Val::Px(5.0)),
                    border: UiRect::all(Val::Px(if sel { 3.0 } else { 1.0 })),
                    column_gap: Val::Px(9.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(if sel { PLUMBOB_GREEN } else { Color::srgba(1.0, 1.0, 1.0, 0.4) }),
                BackgroundColor(BTN_NORMAL),
            ))
            .with_children(|b| {
                // The Sim's face, ringed in the colour of their mood.
                b.spawn((
                    Node {
                        width: Val::Px(50.0),
                        height: Val::Px(50.0),
                        border: UiRect::all(Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BorderColor::all(mood_color(mood)),
                    Pickable::IGNORE,
                ))
                .with_children(|f| {
                    f.spawn((
                        ImageNode::new(portraits.portrait(&mut images, e)),
                        crate::portraits::PortraitOf(e),
                        Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                        Pickable::IGNORE,
                    ));
                });
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
    (people, mut portraits, mut images): (Query<(Entity, &Sim, Has<HouseholdMember>)>, ResMut<crate::portraits::Portraits>, ResMut<Assets<Image>>),
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
            // Like the game's, a notice about a Sim carries their picture: the household's Sim
            // (or else anyone about) the notice begins with.
            let starts = |s: &Sim| msg.starts_with(&format!("{} ", s.first)) || msg.starts_with(&format!("{}'", s.first)) || msg.starts_with(&s.full_name());
            let about = people.iter().filter(|(_, s, _)| !s.first.is_empty() && starts(s)).max_by_key(|(_, _, member)| *member).map(|(e, ..)| e);
            c.spawn((
                Node {
                    padding: UiRect::all(Val::Px(if about.is_some() { 6.0 } else { 10.0 })),
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(crate::menu::NOTICE_BG),
            ))
            .with_children(|b| {
                if let Some(e) = about {
                    b.spawn((
                        Node { width: Val::Px(40.0), height: Val::Px(40.0), flex_shrink: 0.0, border_radius: BorderRadius::all(Val::Px(6.0)), overflow: Overflow::clip(), ..default() },
                        Pickable::IGNORE,
                    ))
                    .with_children(|f| {
                        f.spawn((
                            ImageNode::new(portraits.portrait(&mut images, e)),
                            crate::portraits::PortraitOf(e),
                            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                        ));
                    });
                }
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
#[allow(clippy::too_many_arguments)]
fn update_moodlets_panel(
    mut commands: Commands,
    panel: Query<Entity, With<MoodletsPanel>>,
    sel: Query<&crate::life::Moodlets, With<Selected>>,
    clock: Res<crate::clock::GameClock>,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<(Vec<String>, bool, u32)>,
) {
    let Ok(p) = panel.single() else { return };
    let mut list: Vec<crate::life::Moodlet> = sel.single().map(|m| m.0.clone()).unwrap_or_default();
    list.sort_by_key(|x| -x.value.abs());
    let labels: Vec<String> = list.iter().map(crate::life::moodlet_label).collect();
    // (Time left in the tooltips is refreshed every game hour.)
    let hour = (clock.minutes / 60.0) as u32;
    if last.0 == labels && last.1 == ui.is_some() && last.2 == hour {
        return;
    }
    *last = (labels, ui.is_some(), hour);
    commands.entity(p).despawn_children();
    commands.entity(p).with_children(|c| {
        for m in list {
            let v = m.value;
            let (name, desc, icon) = match ui.as_deref() {
                Some(ui) => ui.moodlet_info(m.kind),
                None => (m.kind.def().name.to_string(), m.kind.def().desc.to_string(), String::new()),
            };
            let icon = ui.as_deref_mut().and_then(|ui| ui.icon(&mut images, &icon));
            let left = if m.until.is_finite() {
                let h = ((m.until - clock.minutes) / 60.0).max(0.0);
                if h >= 1.0 { format!("\n{h:.0} hours left") } else { format!("\n{:.0} minutes left", h * 60.0) }
            } else {
                String::new()
            };
            let tip = format!("{name} ({}{v})\n{desc}{left}", if v >= 0 { "+" } else { "" });
            let frame = if v > 0 {
                Color::srgb(0.30, 0.78, 0.25)
            } else if v < 0 {
                Color::srgb(0.85, 0.22, 0.18)
            } else {
                Color::srgb(0.9, 0.75, 0.2)
            };
            match icon {
                Some(h) => {
                    c.spawn((
                        Node {
                            width: Val::Px(44.0),
                            height: Val::Px(44.0),
                            border: UiRect::all(Val::Px(3.0)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BorderColor::all(frame),
                        BackgroundColor(Color::srgba(0.95, 0.97, 1.0, 0.95)),
                        Interaction::default(),
                        BlocksWorld,
                        crate::icons::Tooltip(tip),
                    ))
                    .with_children(|b| {
                        b.spawn((crate::icons::icon_bundle(h, 32.0), Pickable::IGNORE));
                    });
                }
                None => {
                    let bg = if v >= 0 { Color::srgba(0.12, 0.45, 0.15, 0.92) } else { Color::srgba(0.55, 0.12, 0.10, 0.92) };
                    c.spawn((
                        Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                        BackgroundColor(bg),
                        Interaction::default(),
                        crate::icons::Tooltip(tip),
                    ))
                    .with_children(|b| {
                        b.spawn(text(format!("{name} {}{v}", if v >= 0 { "+" } else { "" }), 13.0, Color::WHITE));
                    });
                }
            }
        }
    });
}

/// The selected Sim's skills as the game's skill icons with their levels; the skill's
/// description on hover.
fn update_skill_icons(
    mut commands: Commands,
    row: Query<Entity, With<SkillIcons>>,
    sel: Query<(&Sim, &Skills), With<Selected>>,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<(Vec<(&'static str, u32)>, bool)>,
) {
    let (Ok(r), Ok((sim, skills))) = (row.single(), sel.single()) else { return };
    let mut list: Vec<(&'static str, u32)> = skills.0.iter().filter(|(_, v)| **v >= 1.0).map(|(k, v)| (*k, *v as u32)).collect();
    list.sort();
    if last.0 == list && last.1 == ui.is_some() {
        return;
    }
    *last = (list.clone(), ui.is_some());
    commands.entity(r).despawn_children();
    let Some(ui) = ui.as_deref_mut() else { return };
    commands.entity(r).with_children(|c| {
        for (name, level) in list {
            let info = ui.data.skill(name).cloned();
            let Some(h) = info.as_ref().and_then(|i| ui.icon(&mut images, &i.icon)) else { continue };
            let max = info.as_ref().map_or(10, |i| i.max_level.max(1));
            let desc = info.as_ref().map(|i| i.desc.replace("{0.SimFirstName}", &sim.first)).unwrap_or_default();
            let tip = format!("{} — level {level} of {max}\n{desc}", info.as_ref().map_or(name, |i| i.name.as_str()));
            c.spawn((
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(2.0), ..default() },
                Interaction::default(),
                BlocksWorld,
                crate::icons::Tooltip(tip),
            ))
            .with_children(|b| {
                b.spawn((crate::icons::icon_bundle(h, 24.0), Pickable::IGNORE));
                b.spawn((text(level.to_string(), 14.0, Color::srgb(1.0, 0.9, 0.5)), Pickable::IGNORE));
            });
        }
    });
}

/// The selected Sim's traits as the game's icons, named on hover.
fn update_trait_icons(
    mut commands: Commands,
    row: Query<Entity, With<TraitIcons>>,
    sel: Query<&Sim, With<Selected>>,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<(Vec<crate::life::Trait>, bool)>,
    mut names: Query<&mut Node, With<TraitsText>>,
) {
    let (Ok(r), Ok(sim)) = (row.single(), sel.single()) else { return };
    if last.0 == sim.traits && last.1 == ui.is_some() {
        return;
    }
    *last = (sim.traits.clone(), ui.is_some());
    commands.entity(r).despawn_children();
    let Some(ui) = ui.as_deref_mut() else { return };
    // The icons replace the list of names.
    for mut n in &mut names {
        n.display = Display::None;
    }
    commands.entity(r).with_children(|c| {
        for t in &sim.traits {
            let info = ui.trait_info(*t);
            let Some(h) = info.as_ref().and_then(|i| ui.icon(&mut images, &i.icon)) else { continue };
            let tip = match &info {
                Some(i) if !i.desc.is_empty() => format!("{}\n{}", i.name, i.desc),
                _ => t.name().to_string(),
            };
            c.spawn((crate::icons::icon_bundle(h, 30.0), Interaction::default(), BlocksWorld, crate::icons::Tooltip(tip)));
        }
    });
}

/// The selected Sim's own menu's way to their cell phone.
const PHONE_LABEL: &str = "Phone ›";

/// The phone: call people the selected Sim knows and invite them over.
#[allow(clippy::type_complexity)]
/// The phone's menu asked for from a house phone (clicked in the world).
#[derive(Resource)]
pub struct OpenPhone;

fn phone_button(
    mut commands: Commands,
    buttons: Query<&Interaction, (Changed<Interaction>, With<PhoneButton>)>,
    asked: Option<Res<OpenPhone>>,
    selected: Query<(Entity, &Relationships, &Sim, Has<crate::careers::Job>), With<Selected>>,
    away: Query<(Entity, &Sim), (With<crate::interact::OffLot>, Without<crate::interact::Invited>)>,
    (windows, household): (Query<&Window, With<PrimaryWindow>>, Query<(Entity, &Sim, &Relationships), With<HouseholdMember>>),
    mut pie: ResMut<PieMenu>,
    mut notes: ResMut<Notifications>,
    objects: Query<&GameObject>,
    maid: Res<crate::services::MaidService>,
    (work_q, clock, silenced): (Query<(Option<&crate::careers::Job>, Option<&crate::timeoff::TimeOff>, Option<&crate::life::Moodlets>)>, Res<crate::clock::GameClock>, Res<crate::calls::PhonesSilenced>),
) {
    if asked.is_some() {
        commands.remove_resource::<OpenPhone>();
    } else if !buttons.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    let Ok((actor, rels, me, has_job)) = selected.single() else { return };
    // The Sims they know, best friends first.
    let mut known: Vec<(f32, Entity, String)> = away.iter().filter(|(e, _)| rels.0.contains_key(e)).map(|(e, s)| (rels.friendship(e), e, s.full_name())).collect();
    if known.is_empty() {
        notes.push("There's nobody to call yet — meet some Sims first!");
    }
    known.sort_by(|a, b| b.0.total_cmp(&a.0));
    known.truncate(12);
    let mut options: Vec<(String, ActionKind)> = vec![
        (format!("Order Pizza (§{})", crate::meals::PIZZA_PRICE), ActionKind::OrderPizza),
        (format!("Call the Repairman (§{}+)", crate::interact::REPAIRMAN_PRICE), ActionKind::CallRepairman),
        if maid.hired {
            ("Fire the Maid".to_string(), ActionKind::HireMaid(false))
        } else {
            (format!("Hire a Maid (§{} an hour)", crate::services::MAID_WAGE), ActionKind::HireMaid(true))
        },
        // (The game's "Throw Party": its picker up during the call, `partypicker`.)
        ("Throw Party...".to_string(), ActionKind::ThrowParty),
    ];
    // Calling someone they know: for a chat, or to invite them over.
    pie.submenus.clear();
    if !known.is_empty() {
        let chat = known.iter().map(|(_, e, n)| (format!("Chat with {n}"), ActionKind::PhoneChat { target: *e })).collect();
        options.push(("Chat with a Friend ›".to_string(), submenu_kind(pie.submenus.len())));
        pie.submenus.push(("Chat".to_string(), chat));
        let invite = known.iter().map(|(_, e, n)| (format!("Invite {n} Over"), ActionKind::Invite { target: *e })).collect();
        options.push(("Invite Someone Over ›".to_string(), submenu_kind(pie.submenus.len())));
        pie.submenus.push(("Invite Over".to_string(), invite));
        // (A Workaholic calling to Check Up On Work, the game's `CallCheckUpOnWork`.)
        if has_job && me.traits.contains(&crate::life::Trait::Workaholic) {
            options.push(("Check Up On Work".to_string(), ActionKind::CheckUpOnWork));
        }
        // (A prank call, the game's `CallPrank`.)
        if !me.age.is_little() {
            let prank = known.iter().map(|(_, e, n)| (format!("Prank Call {n}"), ActionKind::PrankCall { target: *e })).collect();
            options.push(("Prank Call ›".to_string(), submenu_kind(pie.submenus.len())));
            pie.submenus.push(("Prank Call".to_string(), prank));
        }
    }
    // Grown-ups can adopt (a baby only where there's a crib for it).
    if me.age.is_grown() && me.age != crate::sim::Age::Child {
        let crib = objects.iter().any(|o| o.kind == crate::interact::ObjectKind::Crib);
        let list: Vec<(String, ActionKind)> = [(0u8, "Baby"), (1, "Toddler"), (2, "Child")]
            .into_iter()
            .filter(|(a, _)| *a > 0 || crib)
            .flat_map(|(a, n)| [(format!("{n} Girl"), ActionKind::Adopt { age: a, female: true }), (format!("{n} Boy"), ActionKind::Adopt { age: a, female: false })])
            .collect();
        options.push(("Adopt a Child ›".to_string(), submenu_kind(pie.submenus.len())));
        pie.submenus.push(("Adopt".to_string(), list));
        // (And a pet, from the Pets pack's adoption: each kind for its fee.)
        let pets: Vec<(String, ActionKind)> = ["ac", "cc", "ad", "cd", "al", "ah", "ch"]
            .into_iter()
            .map(|k| (format!("{} (§{})", crate::pets::kind_name(k), crate::pets::adoption_fee(k)), ActionKind::AdoptPet { kind: k }))
            .collect();
        options.push(("Adopt a Pet ›".to_string(), submenu_kind(pie.submenus.len())));
        pie.submenus.push(("Adopt a Pet".to_string(), pets));
        options.push(("Move to a New Home".to_string(), ActionKind::MoveHouse));
        // (Some of the household moving out to a home of their own, the game's move split:
        // a grown-up on their own, or with their partner.)
        let members: Vec<(u64, crate::sim::Age)> = household.iter().map(|(_, s, _)| (s.id, s.age)).collect();
        let mut out: Vec<(String, ActionKind)> = Vec::new();
        for (e, s, r) in &household {
            if crate::moveout::may_move_out(&members, &[s.id]) {
                out.push((s.full_name(), ActionKind::Social { target: e, social: crate::moveout::MOVE_OUT }));
            }
            let partner = household.iter().find(|(o, _, _)| r.0.get(o).is_some_and(|x| matches!(x.status, crate::social::RelStatus::Partner | crate::social::RelStatus::Engaged | crate::social::RelStatus::Married)));
            if let Some((_, p, _)) = partner
                && s.id < p.id
                && crate::moveout::may_move_out(&members, &[s.id, p.id])
            {
                out.push((format!("{} and {}", s.first, p.first), ActionKind::Social { target: e, social: crate::moveout::MOVE_OUT_TOGETHER }));
            }
        }
        if !out.is_empty() {
            options.push(("Move Out ›".to_string(), submenu_kind(pie.submenus.len())));
            pie.submenus.push(("Move Out".to_string(), out));
        }
    }
    // Time off work, as the game's "Jobs and Offers...": sick (or fake sick) before a shift, or
    // unpaid days.
    if let Ok((Some(job), off, moodlets)) = work_q.get(actor) {
        let (sick, unpaid) = crate::timeoff::offers(job, off, moodlets, clock.weekday(), clock.hour_f(), clock.minutes);
        if let Some(ask) = sick {
            options.push((ask.label(), ActionKind::TimeOff(ask)));
        }
        if !unpaid.is_empty() {
            options.push(("Take Unpaid Time Off... ›".to_string(), submenu_kind(pie.submenus.len())));
            pie.submenus.push(("Take Unpaid Time Off".to_string(), unpaid.into_iter().map(|a| (a.label(), ActionKind::TimeOff(a))).collect()));
        }
    }
    // (Quitting by phone: the game's `CallToQuitWork`.)
    if has_job {
        options.push(("Quit Job".to_string(), ActionKind::QuitJobCall));
    }
    options.push((if silenced.0 { "Unsilence Phone" } else { "Silence Phone" }.to_string(), ActionKind::SilencePhone));
    // An elder with a job can retire, on a pension.
    if me.age == crate::sim::Age::Elder && has_job {
        options.push(("Retire".to_string(), ActionKind::Retire));
    }
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
    sel: Query<(&Sim, &crate::wishes::Wishes, Option<&crate::lifetime::LifetimeWish>), With<Selected>>,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<Vec<String>>,
) {
    let Ok(p) = panel.single() else { return };
    let Ok((sim, w, ltw)) = sel.single() else { return };
    let mut sig: Vec<String> = w.promised.iter().map(|x| format!("P{}", x.text())).collect();
    sig.push(ltw.map_or(String::new(), |l| format!("L{} {}", l.wish, l.status)));
    sig.extend(w.offered.iter().map(|x| format!("O{}", x.text())));
    sig.push(w.points.to_string());
    sig.push(sim.traits.contains(&crate::life::Trait::Ambitious).to_string());
    sig.push(ui.is_some().to_string());
    if *last == sig {
        return;
    }
    *last = sig;
    // A wish as the game's icon in a frame (gold when promised), named on hover.
    let mut icon = |x: &crate::wishes::Wish| -> Option<Handle<Image>> {
        let ui = ui.as_deref_mut()?;
        let name = x.icon(&ui.data.clone());
        ui.icon(&mut images, &name)
    };
    let tile = |promised: bool| {
        (
            Node {
                width: Val::Px(48.0),
                height: Val::Px(48.0),
                border: UiRect::all(Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(24.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor::all(if promised { Color::srgb(1.0, 0.8, 0.2) } else { Color::srgb(0.55, 0.75, 1.0) }),
            BackgroundColor(Color::srgba(0.95, 0.97, 1.0, 0.95)),
        )
    };
    let promised_icons: Vec<Option<Handle<Image>>> = w.promised.iter().map(&mut icon).collect();
    let offered_icons: Vec<Option<Handle<Image>>> = w.offered.iter().map(&mut icon).collect();
    // The lifetime wish, larger, first in the row.
    let lifetime = ltw.map(|l| {
        let d = l.def();
        let data = ui.as_ref().map(|u| u.data.clone());
        let h = ui.as_deref_mut().and_then(|u| u.icon(&mut images, &d.icon(data.as_deref())));
        let tip = format!("Lifetime Wish: {}\n{}\n{} (+{} lifetime happiness)", d.name, d.describe(data.as_deref()), l.status, crate::lifetime::group(d.points(data.as_deref()) as i64));
        (h, tip, l.fulfilled)
    });
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
        if let Some((Some(h), tip, fulfilled)) = lifetime {
            c.spawn((
                Node {
                    width: Val::Px(58.0),
                    height: Val::Px(58.0),
                    border: UiRect::all(Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(29.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(if fulfilled { Color::srgb(1.0, 0.8, 0.2) } else { PLUMBOB_GREEN }),
                BackgroundColor(Color::srgba(0.95, 0.97, 1.0, 0.95)),
                Interaction::default(),
                BlocksWorld,
                crate::icons::Tooltip(tip),
            ))
            .with_children(|b| {
                b.spawn((crate::icons::icon_bundle(h, 44.0), Pickable::IGNORE));
            });
        }
        for (x, h) in w.promised.iter().zip(promised_icons) {
            let tip = crate::icons::Tooltip(format!("Promised: {} (+{})", x.text(), x.reward_points(&sim.traits)));
            match h {
                Some(h) => {
                    c.spawn((tile(true), Interaction::default(), BlocksWorld, tip)).with_children(|b| {
                        b.spawn((crate::icons::icon_bundle(h, 34.0), Pickable::IGNORE));
                    });
                }
                None => {
                    c.spawn((
                        Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                        BackgroundColor(Color::srgba(0.55, 0.42, 0.08, 0.95)),
                    ))
                    .with_children(|b| {
                        b.spawn(text(format!("Promised: {} +{}", x.text(), x.reward_points(&sim.traits)), 13.0, Color::WHITE));
                    });
                }
            }
        }
        for (i, (x, h)) in w.offered.iter().zip(offered_icons).enumerate() {
            let tip = crate::icons::Tooltip(format!("{} (+{})\nClick to promise this wish.", x.text(), x.reward_points(&sim.traits)));
            match h {
                Some(h) => {
                    c.spawn((Button, WishButton(i), tile(false), tip)).with_children(|b| {
                        b.spawn((crate::icons::icon_bundle(h, 34.0), Pickable::IGNORE));
                    });
                }
                None => {
                    c.spawn((
                        Button,
                        HudButton,
                        WishButton(i),
                        Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|b| {
                        b.spawn(text(format!("{} +{}", x.text(), x.reward_points(&sim.traits)), 13.0, Color::srgb(0.85, 0.9, 1.0)));
                    });
                }
            }
        }
    });
}

/// Clicking an offered wish promises it; the LTH button opens the lifetime rewards.
fn wish_buttons(
    mut commands: Commands,
    wishes_btn: Query<(&Interaction, &WishButton), Changed<Interaction>>,
    rewards_btn: Query<&Interaction, (Changed<Interaction>, With<RewardsButton>)>,
    mut sel: Query<(Entity, &Sim, &mut crate::wishes::Wishes), With<Selected>>,
    mut pie: ResMut<PieMenu>,
    (mut questions, ui): (ResMut<crate::dialog::Questions>, Option<Res<crate::icons::GameUi>>),
) {
    let Ok((actor, sim, mut w)) = sel.single_mut() else { return };
    for (i, b) in &wishes_btn {
        if *i == Interaction::Pressed {
            w.promise(b.0);
        }
    }
    // The lifetime rewards, in the game's dialog.
    if rewards_btn.iter().any(|i| *i == Interaction::Pressed)
        && let Some(ui) = ui
    {
        close_pie(&mut commands, &mut pie);
        crate::wishes::ask_reward(&mut questions, &ui.data, actor, sim, &w);
    }
}
