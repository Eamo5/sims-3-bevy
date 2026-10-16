//! Chatting online at a computer, as the game's `Computer.ChatRandom`: a Sim of the town to chat
//! with (someone new, for the household's own; else someone known), the two of them getting on a
//! little better every five minutes of it (`ChatRelationshipIncreaseEveryXMinutes`,
//! `ChatRelationshipIncrease`), and now and then (every 20 to 50 minutes,
//! `ChatLearnSomethingFrequency`) something learned about them: one of their traits. Social and fun
//! come of it as the interaction's own (its ITUN: Social 50, Fun 15 an hour).

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::interact::{ActionQueue, Phase, Special};
use crate::sim::{HouseholdMember, Sim};

pub struct ChatOnlinePlugin;

impl Plugin for ChatOnlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (start_chats, chat).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The game's `ComputerTuning`: liking every so many minutes of chat, and how much; something
/// learned every so many minutes.
const LIKING_EVERY: f64 = 5.0;
const LIKING: f32 = 1.0;
const LEARN_EVERY: std::ops::Range<f64> = 20.0..50.0;

/// Chatting online: with whom, and when they next get on better or learn something.
#[derive(Component)]
pub struct ChattingOnline {
    pub with: Option<Entity>,
    next_liking: f64,
    next_learn: f64,
}

impl ChattingOnline {
    pub fn new() -> Self {
        Self { with: None, next_liking: 0.0, next_learn: 0.0 }
    }
}

impl Default for ChattingOnline {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether two can chat online, as the game's `ChatRandom` picks them: a grown-up and a child
/// who don't know each other don't.
pub fn may_chat(me: &Sim, them: &Sim, known: bool) -> bool {
    use crate::sim::Age;
    let small = |s: &Sim| matches!(s.age, Age::Child | Age::Toddler | Age::Baby);
    let grown = |s: &Sim| matches!(s.age, Age::YoungAdult | Age::Adult | Age::Elder);
    !them.age.is_little() && (known || !((grown(me) && small(them)) || (small(me) && grown(them))))
}

/// Whether a Sim's at a computer chatting.
fn chatting(queue: &ActionQueue, objects: &Query<&crate::interact::GameObject>) -> bool {
    queue.0.front().is_some_and(|a| {
        matches!(a.phase, Phase::Running(_))
            && matches!(a.kind, crate::interact::ActionKind::Object { target, def } if objects.get(target).is_ok_and(|o| crate::interact::interactions_for(o.kind).get(def).is_some_and(|d| d.special == Special::ChatOnline)))
    })
}

/// Sat down to chat: the chat begun.
fn start_chats(mut commands: Commands, sims: Query<(Entity, &ActionQueue), Without<ChattingOnline>>, objects: Query<&crate::interact::GameObject>) {
    for (e, q) in &sims {
        if chatting(q, &objects) {
            commands.entity(e).insert(ChattingOnline::new());
        }
    }
}

#[allow(clippy::type_complexity)]
fn chat(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut chatters: Query<(Entity, &Sim, &ActionQueue, &mut ChattingOnline, &mut crate::social::Relationships, Has<HouseholdMember>)>,
    town: Query<(Entity, &Sim), Without<ChattingOnline>>,
    mut learned: MessageWriter<crate::learning::Learned>,
    objects: Query<&crate::interact::GameObject>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, sim, queue, mut c, mut rels, member) in &mut chatters {
        // (At the computer, chatting; else it's over.)
        if !chatting(queue, &objects) {
            commands.entity(e).remove::<ChattingOnline>();
            continue;
        }
        // Someone to chat with: someone new (for the household's own), else someone known.
        if c.with.is_none_or(|w| !town.contains(w)) {
            let others: Vec<(Entity, &Sim)> = town.iter().filter(|(o, s)| *o != e && s.id != sim.id).collect();
            let new: Vec<Entity> = others.iter().filter(|(o, s)| !rels.0.contains_key(o) && may_chat(sim, s, false)).map(|(o, _)| *o).collect();
            let known: Vec<Entity> = others.iter().filter(|(o, s)| rels.0.contains_key(o) && may_chat(sim, s, true)).map(|(o, _)| *o).collect();
            c.with = if member && !new.is_empty() { new.choose(&mut rng).copied() } else { known.choose(&mut rng).or(new.choose(&mut rng)).copied() };
            c.next_liking = now + LIKING_EVERY;
            c.next_learn = now + rng.random_range(LEARN_EVERY);
            if let Some((_, s)) = c.with.and_then(|w| town.get(w).ok()) {
                debug!("{} chats online with {}", sim.first, s.full_name());
            }
        }
        let Some(with) = c.with else { continue };
        if now >= c.next_liking {
            c.next_liking += LIKING_EVERY;
            rels.add(with, LIKING, 0.0);
            commands.entity(with).queue_silenced(move |mut w: EntityWorldMut| {
                if let Some(mut r) = w.get_mut::<crate::social::Relationships>() {
                    r.add(e, LIKING, 0.0);
                }
            });
        }
        if now >= c.next_learn {
            c.next_learn = now + rng.random_range(LEARN_EVERY);
            learned.write(crate::learning::Learned { learner: e, about: with, what: None, random: true, teaches: ["", ""] });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Age;

    #[test]
    fn grown_ups_and_children_who_dont_know_each_other_dont_chat() {
        let mut rng = rand::rng();
        let adult = crate::sim::random_sim(&mut rng, "A", Some(true), Age::Adult);
        let teen = crate::sim::random_sim(&mut rng, "T", Some(false), Age::Teen);
        let child = crate::sim::random_sim(&mut rng, "C", Some(false), Age::Child);
        assert!(may_chat(&adult, &teen, false));
        assert!(!may_chat(&adult, &child, false) && !may_chat(&child, &adult, false));
        assert!(may_chat(&adult, &child, true) && may_chat(&child, &teen, false));
    }
}
