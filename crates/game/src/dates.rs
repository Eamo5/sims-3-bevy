//! Dates: a Sim asks someone on a date, and for the next few hours the two keep each other
//! company (the date flirts, chats and compliments); at the end it's judged by how much closer
//! they've grown and how they feel: a Great Date, or a Bad one.

use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, GoingHome, Notifications, Visitor};
use crate::life::{Mood, MoodletKind, Moodlets};
use crate::sim::Sim;
use crate::social::{Relationships, SOCIALS, social_index};
use crate::PlayMode;

pub struct DatesPlugin;

impl Plugin for DatesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StartDate>().add_systems(Update, dates.run_if(in_state(PlayMode::Live)));
    }
}

/// `a` asked `b` out, and `b` said yes.
#[derive(Message)]
pub struct StartDate {
    pub a: Entity,
    pub b: Entity,
}

/// A date under way.
#[derive(Resource)]
pub struct Date {
    pub a: Entity,
    pub b: Entity,
    pub until: f64,
    /// Friendship plus romance when it began.
    start: f32,
    next_nudge: f64,
}

/// How long a date lasts (game minutes).
const DATE_MINUTES: f64 = 240.0;

fn closeness(rels: &Relationships, other: Entity) -> f32 {
    rels.0.get(&other).map_or(0.0, |r| r.friendship + r.romance)
}

#[allow(clippy::type_complexity)]
fn dates(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut starts: MessageReader<StartDate>,
    date: Option<ResMut<Date>>,
    sims: Query<(&Sim, &Relationships, &Mood)>,
    mut visitors: Query<&mut Visitor>,
    mut queues: Query<&mut ActionQueue>,
    mut moods: Query<&mut Moodlets>,
    gone: Query<(), With<GoingHome>>,
    mut notes: ResMut<Notifications>,
) {
    for s in starts.read() {
        if date.is_some() {
            continue;
        }
        let (Ok((a, rels, _)), Ok((b, ..))) = (sims.get(s.a), sims.get(s.b)) else { continue };
        let until = clock.minutes + DATE_MINUTES;
        if let Ok(mut v) = visitors.get_mut(s.b) {
            v.leave_at = v.leave_at.max(until);
        }
        notes.push(format!("{} and {} are on a date!", a.first, b.first));
        commands.insert_resource(Date { a: s.a, b: s.b, until, start: closeness(rels, s.b), next_nudge: clock.minutes + 20.0 });
        return;
    }
    let Some(mut d) = date else { return };
    let (Ok((a, rels, mood_a)), Ok((b, _, mood_b))) = (sims.get(d.a), sims.get(d.b)) else {
        commands.remove_resource::<Date>();
        return;
    };
    // The date wanders off: it's over early.
    let over = clock.minutes >= d.until || gone.contains(d.b) || gone.contains(d.a);
    if !over {
        // Now and then the date turns their attention back to the one who asked them out.
        if clock.minutes >= d.next_nudge {
            d.next_nudge = clock.minutes + 25.0;
            if let Ok(mut q) = queues.get_mut(d.b)
                && q.0.is_empty()
            {
                let pick = ["Flirt", "Chat", "Compliment", "Compliment Appearance", "Tell Joke"].choose(&mut rand::rng()).copied().unwrap_or("Chat");
                if let Some(si) = social_index(pick) {
                    q.0.push_back(Action::new(SOCIALS[si].name, ActionKind::Social { target: d.a, social: si }, true));
                }
            }
        }
        return;
    }
    commands.remove_resource::<Date>();
    // Judged by how much closer they grew, and how they feel.
    let grew = closeness(rels, d.b) - d.start;
    let mood = (mood_a.level() + mood_b.level()) * 0.5;
    let score = grew + mood / 10.0;
    let (kind, text) = if score >= 14.0 {
        (Some(MoodletKind::GreatDate), format!("{} and {}'s date went wonderfully!", a.first, b.first))
    } else if score < 2.0 {
        (Some(MoodletKind::BadDate), format!("{} and {}'s date was a flop.", a.first, b.first))
    } else {
        (None, format!("{} and {}'s date was nice enough.", a.first, b.first))
    };
    notes.push(text);
    if let Some(k) = kind {
        for e in [d.a, d.b] {
            if let Ok(mut m) = moods.get_mut(e) {
                m.add(k, clock.minutes);
            }
        }
    }
}

