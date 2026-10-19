//! Talking in a group, as the game's `Conversation`: two who talk are in a conversation, and
//! whoever talks with one of them joins it (`SocialComponent.JoinConversationWith`; two
//! conversations that meet merge). The rest stand round it in a ring, turned to whoever's
//! talking and listening along (their need for company met a little by it), while two of them
//! talk; they keep it going among themselves a while of their own accord (the game's 2 to 4
//! socials before they think of leaving, 3 more after one the player chose). A Sim leaves it once
//! they go off to do something else or wander more than 3 m from the rest
//! (`kConversationExitRadius`), and it's over once it's gone quiet. One in a conversation can
//! call another over to join it: "Invite to Join Conversation", the game's call over, then the
//! other's Chat with them (`InviteToJoinConversation`).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue, Phase, SocialPartner};
use crate::sim::{Sim, SimAnim};

pub struct GroupsPlugin;

impl Plugin for GroupsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Conversations>().add_systems(Update, (join_talks, leave_talks, listen_along, call_over).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The game's `kConversationExitRadius`: further than this from the rest, and they've left it.
pub const EXIT_RADIUS: f32 = 3.0;
/// Close enough to the one they'd talk to in it to talk from where they stand.
pub const IN_RING: f32 = 2.4;
/// The game's 2 to 4 socials of their own accord before they think of leaving; 3 more after one
/// the player chose (`kNumExtraSocialsAddedByUserDirectedInteraction...`).
const SOCIALS_LEFT: std::ops::RangeInclusive<i32> = 2..=4;
const USER_EXTRA: i32 = 3;
/// Gone quiet this long (game minutes), the conversation's over.
const QUIET_MINUTES: f64 = 30.0;
/// The need for company met by listening along (an hour's).
const LISTEN_SOCIAL: f32 = 40.0;
/// The "social" index the menu's "Invite to Join Conversation" stands for.
pub const INVITE: usize = usize::MAX - 1;
pub const INVITE_LABEL: &str = "Invite to Join Conversation";

/// One conversation: who's in it, who's talking to whom just now, how many more socials they'll
/// have of their own accord, and when it was last talked in.
pub struct Group {
    pub members: Vec<Entity>,
    pub talk: Option<(Entity, Entity)>,
    pub socials_left: i32,
    pub last: f64,
}

#[derive(Resource, Default)]
pub struct Conversations {
    of: HashMap<Entity, u32>,
    groups: HashMap<u32, Group>,
    next: u32,
}

impl Conversations {
    pub fn group(&self, e: Entity) -> Option<&Group> {
        self.groups.get(self.of.get(&e)?)
    }

    /// The rest of the conversation a Sim's in.
    pub fn others(&self, e: Entity) -> Vec<Entity> {
        self.group(e).map(|g| g.members.iter().copied().filter(|m| *m != e).collect()).unwrap_or_default()
    }

    pub fn together(&self, a: Entity, b: Entity) -> bool {
        a != b && self.of.get(&a).is_some_and(|g| self.of.get(&b) == Some(g))
    }

    /// `JoinConversationWith`: into the other's conversation, or theirs into this one's, a new
    /// one, or the two merged.
    pub fn join(&mut self, a: Entity, b: Entity, now: f64, rng: &mut impl Rng) -> u32 {
        let id = match (self.of.get(&a).copied(), self.of.get(&b).copied()) {
            (None, None) => {
                let id = self.next;
                self.next += 1;
                self.groups.insert(id, Group { members: vec![a, b], talk: None, socials_left: rng.random_range(SOCIALS_LEFT), last: now });
                id
            }
            (Some(g), None) | (None, Some(g)) => {
                let new = if self.of.contains_key(&a) { b } else { a };
                if let Some(group) = self.groups.get_mut(&g) {
                    group.members.push(new);
                }
                g
            }
            (Some(g), Some(h)) if g == h => g,
            (Some(g), Some(h)) => {
                let moved = self.groups.remove(&h).map(|x| x.members).unwrap_or_default();
                for m in &moved {
                    self.of.insert(*m, g);
                }
                if let Some(group) = self.groups.get_mut(&g) {
                    group.members.extend(moved);
                }
                g
            }
        };
        self.of.insert(a, id);
        self.of.insert(b, id);
        id
    }

    /// Out of their conversation; one left alone in it, it's over.
    pub fn leave(&mut self, e: Entity) {
        let Some(id) = self.of.remove(&e) else { return };
        let Some(g) = self.groups.get_mut(&id) else { return };
        g.members.retain(|m| *m != e);
        if g.talk.is_some_and(|(x, y)| x == e || y == e) {
            g.talk = None;
        }
        if g.members.len() < 2 {
            for m in g.members.drain(..) {
                self.of.remove(&m);
            }
            self.groups.remove(&id);
        }
    }

    fn end(&mut self, id: u32) {
        if let Some(g) = self.groups.remove(&id) {
            for m in g.members {
                self.of.remove(&m);
            }
        }
    }
}

/// Whether those in a conversation are standing together (not strung out across the room).
pub fn huddled(there: &[Vec2]) -> bool {
    let center = there.iter().copied().sum::<Vec2>() / there.len().max(1) as f32;
    there.len() >= 2 && there.iter().all(|p| p.distance(center) < 1.6)
}

/// Where one come to talk with someone in a group stands: on the ring round the group's middle
/// (wider the more there are), in its widest gap, the nearer the side they come from.
pub fn ring_spot(there: &[Vec2], from: Vec2) -> Vec2 {
    if there.is_empty() {
        return from;
    }
    let n = there.len() + 1;
    let center = there.iter().copied().sum::<Vec2>() / there.len() as f32;
    let radius = (0.9 / (2.0 * (std::f32::consts::PI / n as f32).sin())).max(0.55);
    let angle = |p: Vec2| (p.y - center.y).atan2(p.x - center.x);
    let taken: Vec<f32> = there.iter().map(|p| angle(*p)).collect();
    let towards = angle(from);
    let apart = |a: f32, b: f32| {
        let d = (a - b).rem_euclid(std::f32::consts::TAU);
        d.min(std::f32::consts::TAU - d)
    };
    let best = (0..48)
        .map(|i| i as f32 / 48.0 * std::f32::consts::TAU - std::f32::consts::PI)
        .max_by(|a, b| {
            let score = |x: f32| taken.iter().map(|t| apart(x, *t)).fold(f32::MAX, f32::min) - 0.15 * apart(x, towards);
            score(*a).total_cmp(&score(*b))
        })
        .unwrap_or(towards);
    center + Vec2::new(best.cos(), best.sin()) * radius
}

/// Talking: in a conversation with whoever they're talking to, the talk counted.
fn join_talks(
    mut convs: ResMut<Conversations>,
    clock: Res<crate::clock::GameClock>,
    sims: Query<(Entity, &ActionQueue)>,
    who: Query<(&Sim, &Transform)>,
    mut counted: Local<HashMap<Entity, (Entity, usize, bool)>>,
) {
    let mut rng = rand::rng();
    let now = clock.minutes;
    for (me, queue) in &sims {
        let talking = match queue.0.front() {
            Some(Action { kind: ActionKind::Social { target, social }, phase: Phase::Running(_), seated: false, autonomous, .. }) if *social < crate::social::all_socials().count() => {
                Some((*target, *social, *autonomous))
            }
            _ => None,
        };
        let Some((target, social, autonomous)) = talking else {
            counted.remove(&me);
            continue;
        };
        if !sims.contains(target) {
            continue;
        }
        let before = convs.group(me).map_or(0, |g| g.members.len());
        let id = convs.join(me, target, now, &mut rng);
        if let Some(g) = convs.groups.get_mut(&id) {
            if g.members.len() > 2 && g.members.len() != before {
                let at: Vec<String> = g.members.iter().filter_map(|m| who.get(*m).ok()).map(|(s, t)| format!("{} {:.1?}", s.first, t.translation.xz())).collect();
                debug!("conversation of {}: {}", g.members.len(), at.join(", "));
            }
            g.talk = Some((me, target));
            g.last = now;
            // (A new social: one fewer to come of their own accord, or more after the player's.)
            if counted.get(&me) != Some(&(target, social, autonomous)) {
                g.socials_left += if autonomous { -1 } else { USER_EXTRA };
            }
        }
        counted.insert(me, (target, social, autonomous));
    }
}

/// Out of the conversation: off doing something else, or wandered from it; over once quiet.
#[allow(clippy::type_complexity)]
fn leave_talks(
    mut commands: Commands,
    mut convs: ResMut<Conversations>,
    clock: Res<crate::clock::GameClock>,
    sims: Query<(&ActionQueue, &Transform, Has<SocialPartner>, Option<&crate::anim::ActionClip>)>,
) {
    let mut leaving = Vec::new();
    let mut over = Vec::new();
    for (id, g) in &convs.groups {
        if clock.minutes - g.last > QUIET_MINUTES && g.talk.is_none() {
            over.push(*id);
            continue;
        }
        for &m in &g.members {
            let Ok((queue, tf, partner, _)) = sims.get(m) else {
                leaving.push(m);
                continue;
            };
            let others: Vec<Vec2> = g.members.iter().filter(|o| **o != m).filter_map(|o| sims.get(*o).ok()).map(|s| s.1.translation.xz()).collect();
            let center = if others.is_empty() { tf.translation.xz() } else { others.iter().copied().sum::<Vec2>() / others.len() as f32 };
            // (Being talked to, they're in it wherever they stand.)
            let talked_to = partner || g.talk.is_some_and(|(a, b)| a == m || b == m);
            let gone = match queue.0.front() {
                // (Talking with someone, in it or not: still in it, the other joining.)
                Some(Action { kind: ActionKind::Social { .. }, .. }) => false,
                // (Doing something else.)
                Some(_) => !talked_to,
                None => !talked_to && tf.translation.xz().distance(center) > EXIT_RADIUS,
            };
            if gone {
                leaving.push(m);
            }
        }
    }
    for id in over {
        if let Some(g) = convs.groups.get(&id) {
            for m in &g.members {
                stop_listening(&mut commands, &sims, *m);
            }
        }
        convs.end(id);
    }
    for m in leaving {
        stop_listening(&mut commands, &sims, m);
        convs.leave(m);
    }
}

#[allow(clippy::type_complexity)]
fn stop_listening(commands: &mut Commands, sims: &Query<(&ActionQueue, &Transform, Has<SocialPartner>, Option<&crate::anim::ActionClip>)>, e: Entity) {
    let Ok(s) = sims.get(e) else { return };
    commands.entity(e).remove::<Listening>();
    if s.3.is_some_and(is_listen_clip) {
        commands.entity(e).remove::<crate::anim::ActionClip>();
    }
}

/// Whether the clip playing is the listening along (not something since begun).
fn is_listen_clip(c: &crate::anim::ActionClip) -> bool {
    c.start.is_none() && c.loops.first().is_some_and(|l| LISTEN.contains(l))
}

const LISTEN: [&str; 2] = ["a_soc_talkListen_agreeAha_x", "c_soc_talkListen_agreeAha_x"];

/// Standing in a conversation, listening along (not talking themselves).
#[derive(Component)]
pub struct Listening;

/// The listening along: the game's listening (agreeing, an "aha"), and its conversation idles.
fn listen_clip(child: bool) -> crate::anim::ActionClip {
    crate::anim::ActionClip::new(
        None,
        if child {
            &["c_soc_talkListen_agreeAha_x", "c_soc_talkListen_agreeBobble1_x", "c_soc_talkListen_agreeEager_x", "c_idle_neutral_loop_1"]
        } else {
            &["a_soc_talkListen_agreeAha_x", "a_soc_talkListen_agreeBobble1_x", "a_soc_idle_neutral_friendlyNod_x", "a_soc_idle_neutral_chuckle_x", "a_idle_friendly_loop_"]
        },
    )
}

/// The rest of a conversation, while two of them talk: turned to the one talking, listening along,
/// their need for company met a little.
#[allow(clippy::type_complexity)]
fn listen_along(
    mut commands: Commands,
    convs: Res<Conversations>,
    clock: Res<crate::clock::GameClock>,
    time: Res<Time>,
    mut sims: Query<
        (Entity, &Sim, &ActionQueue, &mut Transform, &mut crate::sim::Motives, Has<SocialPartner>, Has<Listening>, Option<&crate::nav::PathFollow>, Option<&crate::anim::ActionClip>, Has<crate::carry::Holding>),
        With<SimAnim>,
    >,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let spots: HashMap<Entity, Vec3> = sims.iter().map(|s| (s.0, s.3.translation)).collect();
    for (me, sim, queue, mut tf, mut motives, partner, listening, path, clip, holding) in &mut sims {
        // (Not one with a little one in their arms: they keep hold of them, the game's carry.)
        let idle = queue.0.is_empty() && !partner && path.is_none_or(|p| p.done) && !sim.age.is_little() && !holding;
        let group = convs.group(me);
        let Some(g) = group.filter(|_| idle) else {
            // (Off to something else: not listening any more.)
            if listening && (!queue.0.is_empty() || group.is_none()) {
                commands.entity(me).remove::<Listening>();
                if clip.is_some_and(is_listen_clip) {
                    commands.entity(me).remove::<crate::anim::ActionClip>();
                }
            }
            continue;
        };
        if !listening {
            commands.entity(me).insert((Listening, listen_clip(sim.age == crate::sim::Age::Child)));
        }
        // (Turned to whoever's talking, else to the middle.)
        let look = match g.talk {
            Some((talker, _)) if talker != me => spots.get(&talker).copied(),
            _ => {
                let others: Vec<Vec3> = g.members.iter().filter(|m| **m != me).filter_map(|m| spots.get(m).copied()).collect();
                (!others.is_empty()).then(|| others.iter().copied().sum::<Vec3>() / others.len() as f32)
            }
        };
        if let Some(at) = look {
            let to = at - tf.translation;
            if to.xz().length() > 0.1 {
                let want = Quat::from_rotation_y(to.x.atan2(to.z));
                tf.rotation = tf.rotation.slerp(want, (time.delta_secs() * 4.0).min(1.0));
            }
        }
        if g.talk.is_some() {
            motives.add(crate::sim::SOCIAL, LISTEN_SOCIAL * dt / 60.0);
        }
    }
}

/// Calling someone over to join the conversation: the call over, turned to them; then their Chat
/// with the one who called.
#[derive(Component)]
pub struct CallingOver(pub Entity);

fn call_over(
    mut commands: Commands,
    time: Res<Time>,
    mut callers: Query<(Entity, &CallingOver, &mut Transform, &ActionQueue)>,
    spots: Query<&GlobalTransform, With<Sim>>,
    mut queues: Query<&mut ActionQueue, Without<CallingOver>>,
) {
    for (me, call, mut tf, queue) in &mut callers {
        let calling = queue.0.front().is_some_and(|a| a.label == INVITE_LABEL);
        if let Ok(t) = spots.get(call.0) {
            let to = t.translation() - tf.translation;
            if calling && to.xz().length() > 0.1 {
                let want = Quat::from_rotation_y(to.x.atan2(to.z));
                tf.rotation = tf.rotation.slerp(want, (time.delta_secs() * 6.0).min(1.0));
            }
        }
        if calling {
            continue;
        }
        commands.entity(me).remove::<CallingOver>();
        if let Some(chat) = crate::social::social_index("Chat")
            && let Ok(mut q) = queues.get_mut(call.0)
        {
            q.push_player(Action::new("Chat", ActionKind::Social { target: me, social: chat }, false));
        }
    }
}

/// The call over itself, for the caller's queue.
pub fn invite_action(target: Entity) -> Action {
    const CALL: &[&str] = &["a_soc_callOver_x"];
    Action::new(INVITE_LABEL, ActionKind::Outro { clips: CALL, then: None, secs: 2.5, stand_at: None, target }, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talks_join_and_merge_into_one_conversation() {
        let mut c = Conversations::default();
        let mut rng = rand::rng();
        let e = |i: u32| Entity::from_raw_u32(i).unwrap();
        c.join(e(1), e(2), 0.0, &mut rng);
        c.join(e(3), e(2), 0.0, &mut rng);
        assert!(c.together(e(1), e(3)) && c.group(e(1)).unwrap().members.len() == 3);
        c.join(e(4), e(5), 0.0, &mut rng);
        c.join(e(5), e(1), 0.0, &mut rng);
        assert_eq!(c.group(e(4)).unwrap().members.len(), 5);
        // (Down to one, it's over.)
        for i in 1..=4 {
            c.leave(e(i));
        }
        assert!(c.group(e(5)).is_none());
    }

    #[test]
    fn a_newcomer_stands_in_the_widest_gap() {
        // (Two face to face along x: the third comes in from the side they come from.)
        let there = [Vec2::new(-0.45, 0.0), Vec2::new(0.45, 0.0)];
        let spot = ring_spot(&there, Vec2::new(0.0, 5.0));
        assert!(spot.y > 0.4 && spot.x.abs() < 0.2, "{spot}");
        assert!(there.iter().all(|p| p.distance(spot) > 0.7));
        // (Strung out across the room, they're not a ring to stand in.)
        assert!(huddled(&there) && !huddled(&[Vec2::ZERO, Vec2::new(0.0, 5.0)]));
    }
}
