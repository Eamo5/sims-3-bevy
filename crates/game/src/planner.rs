//! Plan Outfit, as at the game's dressers: the Sim's everyday look chosen piece by piece from
//! the wardrobe for their age and gender (hair, tops, bottoms, outfits and shoes, pictured by
//! the game's Create-a-Sim thumbnails), the Sim dressing in each as it's picked.

use bevy::prelude::*;
use s3bake::Key;
use s3formats::sim::{CT_BODY, CT_BOTTOM, CT_HAIR, CT_SHOES, CT_TOP};

use crate::hud::BlocksWorld;
use crate::menu::{BTN_HOVER, BTN_NORMAL, PLUMBOB_GREEN, text};
use crate::sim::Sim;
use crate::{AppState, PlayMode};

pub struct PlannerPlugin;

impl Plugin for PlannerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (planner_buttons, planner_ui).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(PlayMode::Live), |mut commands: Commands| commands.remove_resource::<OutfitPlanner>());
    }
}

/// The wardrobe's tabs: a clothing type and its name.
const TABS: [(u32, &str); 5] = [(CT_HAIR, "Hair"), (CT_TOP, "Tops"), (CT_BOTTOM, "Bottoms"), (CT_BODY, "Outfits"), (CT_SHOES, "Shoes")];
/// Pieces per page.
const PAGE: usize = 18;

/// The wardrobe open for a Sim.
#[derive(Resource)]
pub struct OutfitPlanner {
    pub sim: Entity,
    tab: u32,
    page: usize,
    root: Option<Entity>,
    dirty: bool,
}

impl OutfitPlanner {
    pub fn open(sim: Entity) -> Self {
        Self { sim, tab: CT_TOP, page: 0, root: None, dirty: true }
    }
}

#[derive(Component, Clone, Copy)]
enum PlanButton {
    Tab(u32),
    Pick(Key),
    Page(i32),
    Done,
}

#[allow(clippy::too_many_arguments)]
fn planner_buttons(
    mut commands: Commands,
    planner: Option<ResMut<OutfitPlanner>>,
    mut buttons: Query<(&Interaction, &PlanButton, &mut BackgroundColor), Changed<Interaction>>,
    mut sims: Query<&mut Sim>,
    keys: Res<ButtonInput<KeyCode>>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut p) = planner else { return };
    if keys.just_pressed(KeyCode::Escape) {
        if let Some(r) = p.root {
            commands.entity(r).despawn();
        }
        commands.remove_resource::<OutfitPlanner>();
        return;
    }
    for (i, b, mut bg) in &mut buttons {
        if !matches!(b, PlanButton::Pick(_)) {
            bg.0 = if *i == Interaction::Hovered { BTN_HOVER } else { BTN_NORMAL };
        }
        if *i != Interaction::Pressed {
            continue;
        }
        match *b {
            PlanButton::Tab(t) => {
                p.tab = t;
                p.page = 0;
                p.dirty = true;
            }
            PlanButton::Page(d) => {
                p.page = (p.page as i32 + d).max(0) as usize;
                p.dirty = true;
            }
            PlanButton::Done => {
                if let Some(r) = p.root {
                    commands.entity(r).despawn();
                }
                commands.remove_resource::<OutfitPlanner>();
                return;
            }
            PlanButton::Pick(key) => {
                let Ok(mut sim) = sims.get_mut(p.sim) else { continue };
                let o = &mut sim.outfit;
                match p.tab {
                    CT_HAIR => o.hair = Some(key),
                    CT_TOP => {
                        o.top = Some(key);
                        o.full = None;
                    }
                    CT_BOTTOM => {
                        o.bottom = Some(key);
                        o.full = None;
                    }
                    CT_BODY => {
                        o.full = Some(key);
                        o.top = None;
                        o.bottom = None;
                    }
                    _ => o.shoes = Some(key),
                }
                // Into their everyday clothes, as now planned.
                commands
                    .entity(p.sim)
                    .remove::<(crate::simbody::Wearing, crate::simbody::ChangedInto)>()
                    .insert(crate::aging::NeedsNewBody);
                play.write(crate::sound::PlaySound::ui("ui_primary_button"));
                p.dirty = true;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn planner_ui(
    mut commands: Commands,
    planner: Option<ResMut<OutfitPlanner>>,
    sims: Query<&Sim>,
    cas: Option<Res<crate::simbody::CasData>>,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(mut p), Some(cas)) = (planner, cas) else { return };
    if !p.dirty {
        return;
    }
    p.dirty = false;
    if let Some(r) = p.root.take() {
        commands.entity(r).despawn();
    }
    let Ok(sim) = sims.get(p.sim) else { return };
    let list = crate::cas::parts_for(&cas, sim, p.tab);
    let pages = list.len().div_ceil(PAGE).max(1);
    p.page = p.page.min(pages - 1);
    // What they're wearing of this kind.
    let worn = {
        let o = &sim.outfit;
        match p.tab {
            CT_HAIR => o.hair,
            CT_TOP => o.top,
            CT_BOTTOM => o.bottom,
            CT_BODY => o.full,
            _ => o.shoes,
        }
    };
    let (tab, page) = (p.tab, p.page);
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(90.0),
                width: Val::Px(560.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(crate::menu::PANEL_BG),
            BorderColor::all(crate::menu::PANEL_BORDER),
            Interaction::default(),
            BlocksWorld,
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|c| {
            c.spawn(text(format!("Plan Outfit — {}", sim.first), 18.0, Color::WHITE));
            c.spawn(Node { column_gap: Val::Px(6.0), ..default() }).with_children(|row| {
                for (t, name) in TABS {
                    row.spawn((
                        Button,
                        PlanButton::Tab(t),
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                            border: UiRect::all(Val::Px(if t == tab { 2.0 } else { 0.0 })),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BorderColor::all(PLUMBOB_GREEN),
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|b| {
                        b.spawn((text(name, 14.0, Color::WHITE), Pickable::IGNORE));
                    });
                }
            });
            c.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
                for (key, name) in list.iter().skip(page * PAGE).take(PAGE) {
                    let thumb = ui.as_deref_mut().and_then(|u| u.icon(&mut images, &s3bake::gamedata::cas_thumb_name(key.2)));
                    let on = worn == Some(*key);
                    grid.spawn((
                        Button,
                        PlanButton::Pick(*key),
                        Node {
                            width: Val::Px(84.0),
                            height: Val::Px(104.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(3.0)),
                            border: UiRect::all(Val::Px(if on { 3.0 } else { 1.0 })),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BorderColor::all(if on { PLUMBOB_GREEN } else { Color::srgba(1.0, 1.0, 1.0, 0.25) }),
                        BackgroundColor(Color::srgba(0.9, 0.93, 1.0, 0.92)),
                        crate::icons::Tooltip(name.clone()),
                    ))
                    .with_children(|b| {
                        if let Some(t) = thumb {
                            b.spawn((ImageNode::new(t), Node { width: Val::Px(74.0), height: Val::Px(74.0), ..default() }, Pickable::IGNORE));
                        }
                        let mut short = name.clone();
                        if short.chars().count() > 13 {
                            short = short.chars().take(12).collect::<String>() + "…";
                        }
                        b.spawn((text(short, 11.0, Color::BLACK), Pickable::IGNORE));
                    });
                }
            });
            c.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                for (label, b) in [("‹", PlanButton::Page(-1)), ("›", PlanButton::Page(1))] {
                    row.spawn((
                        Button,
                        b,
                        Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                        BackgroundColor(BTN_NORMAL),
                    ))
                    .with_children(|t| {
                        t.spawn((text(label, 16.0, Color::WHITE), Pickable::IGNORE));
                    });
                }
                row.spawn(text(format!("Page {} of {pages} · {} to choose from", page + 1, list.len()), 13.0, Color::WHITE));
                row.spawn(Node { flex_grow: 1.0, ..default() });
                row.spawn((
                    Button,
                    PlanButton::Done,
                    Node { padding: UiRect::axes(Val::Px(16.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|t| {
                    t.spawn((text("Done", 15.0, Color::WHITE), Pickable::IGNORE));
                });
            });
        })
        .id();
    p.root = Some(root);
}
