//! The Relationships panel, like the game's relationships tab: everyone the selected Sim knows,
//! closest first, each with their portrait, what they are to each other, and friendship and
//! romance bars. Clicking someone on the lot brings the camera to them.

use bevy::prelude::*;

use crate::PlayMode;
use crate::camera::SimsCamera;
use crate::hud::BlocksWorld;
use crate::menu::text;
use crate::sim::{Relationships, Selected, Sim};

pub struct RelationsPlugin;

impl Plugin for RelationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RelationsPanel>()
            .add_systems(Update, (toggle_panel, update_panel, row_clicks).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(PlayMode::Live), |mut p: ResMut<RelationsPanel>| *p = RelationsPanel::default());
    }
}

/// The bottom bar's button that opens the panel.
#[derive(Component)]
pub struct RelationsButton;

#[derive(Resource, Default)]
pub struct RelationsPanel {
    pub open: bool,
    root: Option<Entity>,
    /// What the panel shows, to redraw it only on change.
    shown: Vec<(Entity, i32, i32, u8)>,
}

#[derive(Component)]
struct RelationRow(Entity);

const ROWS: usize = 10;
const BAR: f32 = 190.0;

fn toggle_panel(
    mut commands: Commands,
    buttons: Query<&Interaction, (Changed<Interaction>, With<RelationsButton>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<RelationsPanel>,
) {
    let pressed = buttons.iter().any(|i| *i == Interaction::Pressed) || keys.just_pressed(KeyCode::KeyR);
    if pressed {
        panel.open = !panel.open;
        panel.shown.clear();
        if !panel.open
            && let Some(r) = panel.root.take()
        {
            commands.entity(r).despawn();
        }
    }
}

/// A bar from -100 to 100 filling out from the middle.
fn bar(p: &mut ChildSpawnerCommands, value: f32, plus: Color, minus: Color) {
    let v = value.clamp(-100.0, 100.0) / 100.0;
    p.spawn((
        Node { width: Val::Px(BAR), height: Val::Px(7.0), border_radius: BorderRadius::all(Val::Px(3.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        Pickable::IGNORE,
    ))
    .with_children(|b| {
        let (left, width) = if v >= 0.0 { (50.0, v * 50.0) } else { (50.0 + v * 50.0, -v * 50.0) };
        b.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                width: Val::Percent(width),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(if v >= 0.0 { plus } else { minus }),
            Pickable::IGNORE,
        ));
        // The middle mark.
        b.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), width: Val::Px(1.0), height: Val::Percent(100.0), ..default() },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
            Pickable::IGNORE,
        ));
    });
}

#[allow(clippy::too_many_arguments)]
fn update_panel(
    mut commands: Commands,
    mut panel: ResMut<RelationsPanel>,
    selected: Query<(Entity, &Sim, &Relationships), With<Selected>>,
    people: Query<&Sim>,
    (mut portraits, mut images): (ResMut<crate::portraits::Portraits>, ResMut<Assets<Image>>),
    family: Res<crate::family::Genealogy>,
) {
    if !panel.open {
        return;
    }
    let Ok((me, my_sim, rels)) = selected.single() else { return };
    let mut known: Vec<(Entity, &crate::social::Relationship, &Sim)> = rels.0.iter().filter_map(|(e, r)| Some((*e, r, people.get(*e).ok()?))).collect();
    known.sort_by(|a, b| (b.1.friendship + b.1.romance * 0.5).total_cmp(&(a.1.friendship + a.1.romance * 0.5)));
    known.truncate(ROWS);
    let mut shown: Vec<(Entity, i32, i32, u8)> = known.iter().map(|(e, r, _)| (*e, r.friendship as i32, r.romance as i32, r.status as u8)).collect();
    shown.push((me, 0, 0, 255));
    if panel.root.is_some() && panel.shown == shown {
        return;
    }
    panel.shown = shown;
    if let Some(r) = panel.root.take() {
        commands.entity(r).despawn();
    }
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(520.0),
                bottom: Val::Px(180.0),
                width: Val::Px(330.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(12.0)),
                border_radius: BorderRadius::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(crate::menu::PANEL_BG),
            Interaction::default(),
            BlocksWorld,
        ))
        .with_children(|p| {
            p.spawn(text(format!("{}'s Relationships", my_sim.first), 18.0, Color::WHITE));
            if known.is_empty() {
                p.spawn(text("Nobody yet. Go and meet some Sims!", 14.0, Color::srgb(0.8, 0.85, 0.95)));
            }
            for (e, rel, sim) in &known {
                let ring = if rel.friendship >= 40.0 {
                    Color::srgb(0.35, 0.85, 0.35)
                } else if rel.friendship <= -20.0 {
                    Color::srgb(0.9, 0.3, 0.25)
                } else {
                    Color::srgba(1.0, 1.0, 1.0, 0.5)
                };
                p.spawn((
                    Button,
                    RelationRow(*e),
                    Node {
                        column_gap: Val::Px(10.0),
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|row| {
                    row.spawn((
                        Node {
                            width: Val::Px(46.0),
                            height: Val::Px(46.0),
                            border: UiRect::all(Val::Px(2.0)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BorderColor::all(ring),
                        Pickable::IGNORE,
                    ))
                    .with_children(|f| {
                        f.spawn((
                            ImageNode::new(portraits.portrait(&mut images, *e)),
                            crate::portraits::PortraitOf(*e),
                            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                            Pickable::IGNORE,
                        ));
                    });
                    row.spawn((Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() }, Pickable::IGNORE)).with_children(|c| {
                        c.spawn((text(sim.full_name(), 15.0, Color::WHITE), Pickable::IGNORE));
                        // (Family first: "Son · Good Friend".)
                        let label = match family.word(my_sim.id, sim.id) {
                            Some(w) => format!("{w} · {}", rel.label_for(sim.female)),
                            None => rel.label_for(sim.female),
                        };
                        c.spawn((text(label, 12.0, Color::srgb(0.75, 0.85, 1.0)), Pickable::IGNORE));
                        bar(c, rel.friendship, Color::srgb(0.35, 0.85, 0.35), Color::srgb(0.9, 0.3, 0.25));
                        if rel.romance > 0.5 {
                            bar(c, rel.romance, Color::srgb(0.95, 0.45, 0.7), Color::srgb(0.6, 0.25, 0.4));
                        }
                    });
                });
            }
        })
        .id();
    panel.root = Some(root);
}

/// Clicking someone on the lot brings the camera to them.
fn row_clicks(
    rows: Query<(&Interaction, &RelationRow), Changed<Interaction>>,
    mut hovered: Query<(&Interaction, &mut BackgroundColor), With<RelationRow>>,
    sims: Query<(&GlobalTransform, &InheritedVisibility), With<Sim>>,
    mut cam: Query<&mut SimsCamera>,
) {
    for (i, row) in &rows {
        if *i == Interaction::Pressed
            && let Ok((tf, vis)) = sims.get(row.0)
            && vis.get()
            && let Ok(mut c) = cam.single_mut()
        {
            c.look_at(tf.translation());
        }
    }
    for (i, mut bg) in &mut hovered {
        bg.0 = if *i == Interaction::None { Color::NONE } else { Color::srgba(1.0, 1.0, 1.0, 0.08) };
    }
}
