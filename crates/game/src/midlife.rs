//! The Mid-Life Crisis lifetime reward: a Sim rethinks their life, choosing their traits all
//! over again (the game's trait pictures, those clashing with chosen ones dimmed). Done keeps
//! the new ones; Escape keeps the old, and the lifetime happiness spent comes back.

use bevy::prelude::*;

use crate::hud::BlocksWorld;
use crate::life::Trait;
use crate::menu::{BTN_HOVER, BTN_NORMAL, PLUMBOB_GREEN, text};
use crate::sim::Sim;
use crate::{AppState, PlayMode};

pub struct MidLifePlugin;

impl Plugin for MidLifePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (picker_buttons, picker_ui).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(PlayMode::Live), |mut commands: Commands| commands.remove_resource::<TraitPicker>());
    }
}

/// A Sim choosing their traits again.
#[derive(Resource)]
pub struct TraitPicker {
    sim: Entity,
    traits: Vec<Trait>,
    /// The lifetime happiness spent on it (back if they think better of it).
    cost: u32,
    root: Option<Entity>,
    dirty: bool,
}

impl TraitPicker {
    pub fn open(sim: Entity, traits: &[Trait], cost: u32) -> Self {
        Self { sim, traits: traits.to_vec(), cost, root: None, dirty: true }
    }
}

#[derive(Component, Clone, Copy)]
enum PickButton {
    Trait(usize),
    Done,
}

/// The chosen ones, and those clashing with them.
#[derive(Component)]
struct Chosen;

#[allow(clippy::too_many_arguments)]
fn picker_buttons(
    mut commands: Commands,
    picker: Option<ResMut<TraitPicker>>,
    mut buttons: Query<(&Interaction, &PickButton, &mut BackgroundColor, Has<Chosen>), Changed<Interaction>>,
    mut sims: Query<(&mut Sim, Option<&mut crate::wishes::Wishes>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut p) = picker else { return };
    let close = |commands: &mut Commands, p: &TraitPicker| {
        if let Some(r) = p.root {
            commands.entity(r).despawn();
        }
        commands.remove_resource::<TraitPicker>();
    };
    if keys.just_pressed(KeyCode::Escape) {
        if let Ok((sim, Some(mut w))) = sims.get_mut(p.sim) {
            w.points += p.cost;
            notes.push(format!("{} thought better of a mid-life crisis.", sim.first));
        }
        close(&mut commands, &p);
        return;
    }
    for (i, b, mut bg, chosen) in &mut buttons {
        if !chosen {
            bg.0 = if *i == Interaction::Hovered { BTN_HOVER } else { BTN_NORMAL };
        }
        if *i != Interaction::Pressed {
            continue;
        }
        match *b {
            PickButton::Trait(i) => {
                let Ok((sim, _)) = sims.get(p.sim) else { continue };
                let t = Trait::ALL[i];
                if let Some(pos) = p.traits.iter().position(|x| *x == t) {
                    p.traits.remove(pos);
                } else if p.traits.len() < crate::life::trait_slots(sim.age) && t.allowed_at(sim.age) && t.compatible(&p.traits) {
                    p.traits.push(t);
                }
                play.write(crate::sound::PlaySound::ui("ui_primary_button"));
                p.dirty = true;
            }
            PickButton::Done => {
                if let Ok((mut sim, _)) = sims.get_mut(p.sim) {
                    sim.traits = p.traits.clone();
                    let names: Vec<&str> = p.traits.iter().map(|t| t.name()).collect();
                    let list = match names.split_last() {
                        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
                        Some((last, _)) => last.to_string(),
                        None => "with no traits at all".to_string(),
                    };
                    notes.push(format!("{} came out of a mid-life crisis: {list}.", sim.first));
                }
                close(&mut commands, &p);
                return;
            }
        }
    }
}

fn picker_ui(
    mut commands: Commands,
    picker: Option<ResMut<TraitPicker>>,
    sims: Query<&Sim>,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut p) = picker else { return };
    if !p.dirty {
        return;
    }
    p.dirty = false;
    if let Some(r) = p.root.take() {
        commands.entity(r).despawn();
    }
    let Ok(sim) = sims.get(p.sim) else { return };
    let slots = crate::life::trait_slots(sim.age);
    let traits = p.traits.clone();
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(90.0),
                width: Val::Px(600.0),
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
            c.spawn(text(format!("Mid-Life Crisis — {}", sim.first), 18.0, Color::WHITE));
            c.spawn(text(format!("Traits · {} of {slots}. Click to add or remove; Escape to keep the old ones.", traits.len()), 13.0, Color::srgb(0.75, 0.85, 1.0)));
            c.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(4.0), row_gap: Val::Px(4.0), ..default() }).with_children(|grid| {
                for (i, t) in Trait::ALL.iter().enumerate() {
                    if !t.allowed_at(sim.age) { continue; }
                    let chosen = traits.contains(t);
                    let ok = chosen || (traits.len() < slots && t.compatible(&traits));
                    let info = ui.as_deref().and_then(|u| u.trait_info(*t));
                    let icon = match (ui.as_deref_mut(), &info) {
                        (Some(u), Some(i)) => u.icon(&mut images, &i.icon),
                        _ => None,
                    };
                    let mut e = grid.spawn((
                        Button,
                        PickButton::Trait(i),
                        Node {
                            width: Val::Px(140.0),
                            min_height: Val::Px(30.0),
                            padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                            column_gap: Val::Px(4.0),
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(if chosen { Color::srgb(0.22, 0.55, 0.22) } else if ok { BTN_NORMAL } else { Color::srgba(0.1, 0.18, 0.28, 0.6) }),
                    ));
                    if chosen || !ok {
                        e.insert(Chosen);
                    }
                    if let Some(i) = info.as_ref().filter(|i| !i.desc.is_empty()) {
                        e.insert(crate::icons::Tooltip(format!("{}\n{}", i.name, i.desc)));
                    }
                    let fade = if ok { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.4) };
                    e.with_children(|b| {
                        if let Some(h) = icon {
                            b.spawn((ImageNode { color: fade, ..ImageNode::new(h) }, Node { width: Val::Px(26.0), height: Val::Px(26.0), ..default() }, Pickable::IGNORE));
                        }
                        b.spawn((text(t.name(), 12.0, fade), Pickable::IGNORE));
                    });
                }
            });
            c.spawn(Node { justify_content: JustifyContent::FlexEnd, ..default() }).with_children(|row| {
                row.spawn((
                    Button,
                    PickButton::Done,
                    Node {
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BorderColor::all(PLUMBOB_GREEN),
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
