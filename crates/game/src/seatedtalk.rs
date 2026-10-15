//! Talking while seated, as the game's `SeatedSocialInteractionA`: two Sims sat near each other
//! (at the table, on the sofa) have their friendly talk where they are, without getting up. The
//! one talking gestures and talks (the game's `a_soc_talkRespond_*_sitting` clips) and the other
//! listens (`a_soc_talkListen_*_sitting`), over the top of them alone, while they go on eating or
//! watching; then they're back to it. Sat together, they chat now and then of their own accord,
//! as the game's dinner tables do.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::interact::{Action, ActionKind, ActionQueue, Phase};
use crate::sim::{HouseholdMember, Pose, Sim, SimAnim};

pub struct SeatedTalkPlugin;

impl Plugin for SeatedTalkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (seated_chatter.run_if(in_state(crate::PlayMode::Live)), talk_over));
    }
}

/// How near two seated Sims talk (the game's group talk, across a table).
pub const SEATED_RANGE: f32 = 3.5;

/// A talk played over the upper body alone (the spine, arms, head and face), the rest going on
/// as it was; how far through it is, who it's with, and whether it's the one talking.
#[derive(Component, Clone, Debug)]
pub struct TalkOverlay {
    pub clip: String,
    pub time: f32,
    pub with: Entity,
    pub talking: bool,
}

/// A talk whose social is over (finished, cancelled, the queue cleared for something else) comes
/// off them.
fn talk_over(mut commands: Commands, talks: Query<(Entity, &TalkOverlay)>, queues: Query<&ActionQueue>) {
    for (e, t) in &talks {
        let (actor, target) = if t.talking { (e, t.with) } else { (t.with, e) };
        let on = queues.get(actor).is_ok_and(|q| q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Social { target: s, .. } if s == target)));
        if !on {
            commands.entity(e).remove::<TalkOverlay>();
        }
    }
}

/// The talker's, as it's taken and as it isn't.
const RESPOND_AGREE: [&str; 3] = ["a_soc_talkRespond_agree_enlightened_sitting_x", "a_soc_talkRespond_agree_laugh_sitting_x", "a_soc_talkRespond_agree_totally_sitting_x"];
const RESPOND_DISAGREE: [&str; 3] = ["a_soc_talkRespond_disagree_correction_sitting_x", "a_soc_talkRespond_disagree_disgust_sitting_x", "a_soc_talkRespond_disagree_holdOn_sitting_x"];
/// The listener's.
const LISTEN_AGREE: [&str; 3] = ["a_soc_talkListen_agreeEager_sitting_x", "a_soc_talkListen_agree_funny_sitting_x", "a_soc_talkListen_agree_wow_sitting_x"];
const LISTEN_DISAGREE: [&str; 3] = ["a_soc_talkListen_disagree_low_sitting_x", "a_soc_talkListen_disagree_med_sitting_x", "a_soc_talkListen_disagree_kiddingMe_sitting_x"];

/// The clips for a seated talk: the talker's and the listener's, as it went (a child's, the
/// children's own).
pub fn talk_clips(accepted: bool, child: [bool; 2], rng: &mut impl Rng) -> (String, String) {
    let (talk, listen) = if accepted { (&RESPOND_AGREE, &LISTEN_AGREE) } else { (&RESPOND_DISAGREE, &LISTEN_DISAGREE) };
    let aged = |clip: &str, child: bool| if child { clip.replacen("a_", "c_", 1) } else { clip.to_string() };
    (aged(talk[rng.random_range(0..talk.len())], child[0]), aged(listen[rng.random_range(0..listen.len())], child[1]))
}

/// Whether two Sims are sat near enough to talk where they are.
pub fn sat_together(a: (&SimAnim, Vec3, u8), b: (&SimAnim, Vec3, u8)) -> bool {
    a.0.pose == Pose::Sit && b.0.pose == Pose::Sit && a.2 == b.2 && a.1.distance(b.1) < SEATED_RANGE
}

/// The action for a social done seated: first, the meal or the show waiting till it's done.
pub fn seated_action(name: &str, target: Entity, social: usize, autonomous: bool) -> Action {
    let mut a = Action::new(name, ActionKind::Social { target, social }, autonomous);
    a.seated = true;
    a
}

/// How often (game minutes) a seated Sim may strike up a conversation of their own accord, and
/// how soon after sitting down.
const CHATTER_EVERY: std::ops::Range<f64> = 15.0..45.0;
const FIRST_CHAT: std::ops::Range<f64> = 4.0..15.0;

/// Sat with someone, a Sim of the household chats now and then of their own accord (free will
/// on): a friendly talk they'd have, where they are.
#[allow(clippy::type_complexity)]
fn seated_chatter(
    clock: Res<crate::clock::GameClock>,
    settings: Res<crate::options::Settings>,
    mut talkers: Query<(Entity, &Sim, &SimAnim, &Transform, Option<&crate::nav::Floor>, &mut ActionQueue, &crate::social::Relationships), With<HouseholdMember>>,
    others: Query<(Entity, &Sim, &SimAnim, &Transform, Option<&crate::nav::Floor>, Has<HouseholdMember>)>,
    sides: Query<crate::social::SideQuery>,
    family: Res<crate::family::Genealogy>,
    mut next: Local<std::collections::HashMap<Entity, f64>>,
) {
    if settings.free_will == crate::options::FreeWill::Off || clock.speed == 0 {
        return;
    }
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (me, sim, anim, tf, floor, mut queue, rels) in &mut talkers {
        // (Sat at something, not mid-social or anything else waiting; the first chat a while
        // after they sit down.)
        let sat = !sim.age.is_little()
            && anim.pose == Pose::Sit
            && queue.0.len() == 1
            && queue.0.front().is_some_and(|f| matches!(f.phase, Phase::Running(_)) && !matches!(f.kind, ActionKind::Social { .. }));
        if !sat {
            if !matches!(queue.0.front(), Some(Action { kind: ActionKind::Social { .. }, seated: true, .. })) {
                next.remove(&me);
            }
            continue;
        }
        let due = next.entry(me).or_insert(now + rng.random_range(FIRST_CHAT));
        if now < *due {
            continue;
        }
        *due = now + rng.random_range(CHATTER_EVERY);
        let level = floor.map_or(1, |f| f.0);
        let Some((them, tsim, member)) = others
            .iter()
            .filter(|o| o.0 != me && !o.1.age.is_little() && sat_together((anim, tf.translation, level), (o.2, o.3.translation, o.4.map_or(1, |f| f.0))))
            .min_by(|a, b| a.3.translation.distance(tf.translation).total_cmp(&b.3.translation.distance(tf.translation)))
            .map(|o| (o.0, o.1, o.5))
        else {
            continue;
        };
        let rel = rels.get(them);
        let side = [crate::social::Side::of(me, sides.get(me).ok()), crate::social::Side::of(them, sides.get(them).ok())];
        let kin = family.kin(sim.id, tsim.id).is_some();
        let choices: Vec<(usize, &crate::social::SocialDef)> = crate::social::all_socials()
            .filter(|(_, s)| s.autonomous && crate::social::seated_ok(s) && s.effect == crate::social::SocialEffect::None && crate::social::available(s, &rel, sim, tsim, member, kin, side))
            .collect();
        let Some((i, s)) = choices.choose(&mut rng) else { continue };
        debug!("{} chats with {} where they sit: {}", sim.first, tsim.first, s.name);
        queue.0.push_front(seated_action(s.name, them, *i, true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::{SocialCat, game_socials, seated_ok};
    use s3bake::gamedata::SocialAction;

    #[test]
    fn friendly_talk_is_had_where_they_sit() {
        // Sat near each other on the same floor: they talk where they are.
        let sat = SimAnim { pose: Pose::Sit, ..Default::default() };
        let stood = SimAnim::default();
        assert!(sat_together((&sat, Vec3::ZERO, 1), (&sat, Vec3::new(1.2, 0.0, 0.8), 1)));
        assert!(!sat_together((&sat, Vec3::ZERO, 1), (&stood, Vec3::new(1.0, 0.0, 0.0), 1)), "one standing walks over");
        assert!(!sat_together((&sat, Vec3::ZERO, 1), (&sat, Vec3::new(5.0, 0.0, 0.0), 1)), "across the room");
        assert!(!sat_together((&sat, Vec3::ZERO, 1), (&sat, Vec3::new(1.0, 0.0, 0.0), 2)), "a floor up");
        // The game's: friendly ones without a jig or a child in arms to stop them.
        let act = |name: &str, tone: &str, bits: u32| SocialAction {
            key: name.into(),
            name: name.into(),
            tone: tone.into(),
            actor_ages: vec!["T".into(), "Y".into(), "A".into()],
            target_ages: vec!["T".into(), "Y".into(), "A".into()],
            suite: "Admire".into(),
            bits,
            ..Default::default()
        };
        let clips: Vec<String> = ["a2a_soc_Neutral_Admire_Friendly_Neutral_x", "a2a_soc_Neutral_Admire_Friendly_Neutral_y"].map(String::from).to_vec();
        let got = game_socials(&[act("Admire", "Friendly", 0), act("Hug", "Friendly", 0x2000), act("Brighten Day", "Friendly", 0x8), act("Mock", "Insulting", 0)], &clips);
        let seated: Vec<&str> = got.iter().filter(|s| seated_ok(s)).map(|s| s.name).collect();
        assert_eq!(seated, ["Admire"]);
        // The hand-made: their talk, not a slap or a kiss.
        let named = |n: &str| crate::social::SOCIALS.iter().find(|s| s.name == n).unwrap();
        assert!(seated_ok(named("Chat")) && seated_ok(named("Compliment")));
        assert!(crate::social::SOCIALS.iter().filter(|s| s.cat != SocialCat::Friendly).all(|s| !seated_ok(s)));
        // The talk taken well or not, and a child's in the children's own clips.
        let mut rng = rand::rng();
        let (talk, listen) = talk_clips(true, [false, true], &mut rng);
        assert!(RESPOND_AGREE.contains(&talk.as_str()) && listen.starts_with("c_soc_talkListen_") && listen.ends_with("_sitting_x"));
        let (talk, _) = talk_clips(false, [false, false], &mut rng);
        assert!(RESPOND_DISAGREE.contains(&talk.as_str()));
    }
}
