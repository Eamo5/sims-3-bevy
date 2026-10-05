//! Questions the game puts to the player: a box in the middle of the screen with a picture, a
//! message and a choice of answers (which way to take a career where it branches, the lifetime
//! wish of a child growing up). One is shown at a time; the answer goes out as an `Answered`
//! message for whoever asked.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::PlayMode;
use crate::hud::BlocksWorld;
use crate::menu::{BTN_NORMAL, PLUMBOB_GREEN, text};

pub struct DialogPlugin;

impl Plugin for DialogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Questions>()
            .add_message::<Answered>()
            .add_systems(Update, (show_question, answer_buttons, scroll_answers).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// What a question is about (and so who handles the answer).
#[derive(Clone, Debug, PartialEq)]
pub enum Question {
    /// Which path to take: the career's path indices offered.
    CareerPath { sim: Entity, paths: Vec<usize> },
    /// Which lifetime wish: indices into `lifetime::LIFETIME_WISHES`.
    LifetimeWish { sim: Entity, wishes: Vec<usize> },
    /// Which lifetime reward to buy (reward traits' names; answers past them close).
    Reward { sim: Entity, rewards: Vec<String> },
}

impl Question {
    /// The Sim the question is about.
    pub fn sim(&self) -> Option<Entity> {
        match self {
            Question::CareerPath { sim, .. } | Question::LifetimeWish { sim, .. } | Question::Reward { sim, .. } => Some(*sim),
        }
    }
}

pub struct Answer {
    pub label: String,
    pub detail: String,
    pub icon: String,
}

pub struct Ask {
    pub about: Question,
    pub icon: String,
    /// A line above the title ("Career for Bella").
    pub heading: String,
    pub title: String,
    pub text: String,
    pub answers: Vec<Answer>,
}

/// Questions waiting to be answered, the first on show.
#[derive(Resource, Default)]
pub struct Questions {
    pub queue: VecDeque<Ask>,
    shown: Option<Entity>,
}

impl Questions {
    pub fn ask(&mut self, a: Ask) {
        // The same question isn't asked twice.
        if !self.queue.iter().any(|q| q.about == a.about) {
            self.queue.push_back(a);
        }
    }
}

/// The player's answer: index into the question's answers.
#[derive(Message)]
pub struct Answered {
    pub about: Question,
    pub answer: usize,
}

#[derive(Component)]
struct AnswerButton(usize);

/// The answers, which scroll when there are many.
#[derive(Component)]
struct DialogScroll;

fn scroll_answers(wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>, mut q: Query<(&mut ScrollPosition, &bevy::ui::RelativeCursorPosition), With<DialogScroll>>) {
    if wheel.delta.y == 0.0 {
        return;
    }
    let dy = match wheel.unit {
        bevy::input::mouse::MouseScrollUnit::Line => wheel.delta.y * 48.0,
        bevy::input::mouse::MouseScrollUnit::Pixel => wheel.delta.y,
    };
    for (mut pos, cursor) in &mut q {
        if cursor.cursor_over() {
            pos.0.y = (pos.0.y - dy).max(0.0);
        }
    }
}

fn show_question(mut commands: Commands, mut q: ResMut<Questions>, mut ui: Option<ResMut<crate::icons::GameUi>>, mut images: ResMut<Assets<Image>>) {
    if q.shown.is_some() {
        return;
    }
    let Some(a) = q.queue.front() else { return };
    let mut icon = |name: &str| ui.as_deref_mut().and_then(|u| u.icon(&mut images, name));
    let pic = icon(&a.icon);
    let answer_icons: Vec<Option<Handle<Image>>> = a.answers.iter().map(|x| icon(&x.icon)).collect();
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(27.0),
                top: Val::Percent(14.0),
                width: Val::Percent(46.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(16.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.15, 0.30, 0.97)),
            BorderColor::all(PLUMBOB_GREEN),
            Interaction::default(),
            BlocksWorld,
            GlobalZIndex(25),
        ))
        .with_children(|p| {
            p.spawn(Node { column_gap: Val::Px(12.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                if let Some(h) = pic {
                    r.spawn(crate::icons::icon_bundle(h, 56.0));
                }
                r.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|c| {
                    c.spawn(text(a.heading.clone(), 14.0, Color::srgb(0.75, 0.85, 1.0)));
                    c.spawn(text(a.title.clone(), 22.0, Color::WHITE));
                });
            });
            p.spawn(text(a.text.clone(), 15.0, Color::WHITE));
            let mut list = p.spawn((
                Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), max_height: Val::Vh(58.0), overflow: Overflow::scroll_y(), ..default() },
                DialogScroll,
                bevy::ui::RelativeCursorPosition::default(),
                ScrollPosition::default(),
            ));
            list.with_children(|p| for (i, x) in a.answers.iter().enumerate() {
                p.spawn((
                    Button,
                    AnswerButton(i),
                    Node {
                        column_gap: Val::Px(10.0),
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    if let Some(Some(h)) = answer_icons.get(i) {
                        b.spawn((crate::icons::icon_bundle(h.clone(), 40.0), Pickable::IGNORE));
                    }
                    b.spawn((Node { flex_direction: FlexDirection::Column, ..default() }, Pickable::IGNORE)).with_children(|c| {
                        c.spawn((text(x.label.clone(), 17.0, Color::WHITE), Pickable::IGNORE));
                        if !x.detail.is_empty() {
                            c.spawn((text(x.detail.clone(), 13.0, Color::srgb(0.8, 0.88, 1.0)), Pickable::IGNORE));
                        }
                    });
                });
            });
        })
        .id();
    q.shown = Some(root);
}

fn answer_buttons(
    mut commands: Commands,
    mut q: ResMut<Questions>,
    buttons: Query<(&Interaction, &AnswerButton), Changed<Interaction>>,
    mut answered: MessageWriter<Answered>,
    time: Res<Time>,
) {
    let Some(root) = q.shown else { return };
    let mut pick = buttons.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, b)| b.0);
    // ANSWER=<n>: questions answer themselves (tests).
    if pick.is_none()
        && time.elapsed_secs() > 4.0
        && let Some(n) = std::env::var("ANSWER").ok().and_then(|s| s.parse::<usize>().ok())
    {
        pick = Some(n.min(q.queue.front().map_or(1, |a| a.answers.len()).saturating_sub(1)));
    }
    let Some(i) = pick else { return };
    commands.entity(root).despawn();
    q.shown = None;
    if let Some(a) = q.queue.pop_front() {
        answered.write(Answered { about: a.about, answer: i });
    }
}
