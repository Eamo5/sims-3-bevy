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
        app.add_systems(Update, (seated_chatter.run_if(in_state(crate::PlayMode::Live)), talk_over, lie_back));
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

/// The game's "Cuddle" done sat together on a sofa (its `StartSeatedCuddleA`), not the standing
/// social.
pub const CUDDLE: &str = "Cuddle";

/// How long a cuddle on the sofa lasts (game minutes; the game's goes on till it's stopped).
pub const CUDDLE_MINUTES: f32 = 45.0;

/// A seated cuddle's clips (the game's `SeatingCuddle`): leaning in and cuddling, as the one at
/// the partner's left (`x`: the partner to their own −X) or right; the teens' own; or the
/// leaning in turned away.
pub fn cuddle_clip(x: bool, teen: bool, accepted: bool) -> crate::anim::ActionClip {
    use crate::anim::ActionClip as A;
    match (teen, x, accepted) {
        (false, true, true) => A::steps("a2a_sofa_sit_trans_cuddle_x", &[], &["a2a_sofa_sit_cuddle_loop1_x", "a2a_sofa_sit_cuddle_loop2_x", "a2a_sofa_sit_cuddle_loop3_x", "a2a_sofa_sit_cuddle_loop4_x"]),
        (false, false, true) => A::steps("a2a_sofa_sit_trans_cuddle_y", &[], &["a2a_sofa_sit_cuddle_loop1_y", "a2a_sofa_sit_cuddle_loop2_y", "a2a_sofa_sit_cuddle_loop3_y", "a2a_sofa_sit_cuddle_loop4_y"]),
        (true, true, true) => A::steps("t2t_sofa_sit_trans_cuddle_x", &[], &["t2t_sofa_sit_cuddle_loop1_x", "t2t_sofa_sit_cuddle_loop2_x", "t2t_sofa_sit_cuddle_loop3_x", "t2t_sofa_sit_cuddle_loop4_x"]),
        (true, false, true) => A::steps("t2t_sofa_sit_trans_cuddle_y", &[], &["t2t_sofa_sit_cuddle_loop1_y", "t2t_sofa_sit_cuddle_loop2_y", "t2t_sofa_sit_cuddle_loop3_y", "t2t_sofa_sit_cuddle_loop4_y"]),
        (false, true, false) => A::steps("a2a_sofa_sit_cuddle_reject_x", &[], crate::anim::SIT_LOOPS),
        (false, false, false) => A::steps("a2a_sofa_sit_cuddle_reject_y", &[], crate::anim::SIT_LOOPS),
        (true, true, false) => A::steps("t2t_sofa_sit_cuddle_reject_x", &[], crate::anim::SIT_LOOPS),
        (true, false, false) => A::steps("t2t_sofa_sit_cuddle_reject_y", &[], crate::anim::SIT_LOOPS),
    }
}

/// A kiss or a making out while cuddling (the game's `CuddleSeatedKiss`, `CuddleSeatedMakeOut`):
/// back to cuddling after; turned away, back to sitting (the cuddle's over).
pub fn cuddle_kiss_clip(x: bool, teen: bool, accepted: bool, make_out: bool) -> crate::anim::ActionClip {
    use crate::anim::ActionClip as A;
    const AX: &[&str] = &["a2a_sofa_sit_cuddle_loop1_x", "a2a_sofa_sit_cuddle_loop2_x", "a2a_sofa_sit_cuddle_loop3_x", "a2a_sofa_sit_cuddle_loop4_x"];
    const AY: &[&str] = &["a2a_sofa_sit_cuddle_loop1_y", "a2a_sofa_sit_cuddle_loop2_y", "a2a_sofa_sit_cuddle_loop3_y", "a2a_sofa_sit_cuddle_loop4_y"];
    const TX: &[&str] = &["t2t_sofa_sit_cuddle_loop1_x", "t2t_sofa_sit_cuddle_loop2_x", "t2t_sofa_sit_cuddle_loop3_x", "t2t_sofa_sit_cuddle_loop4_x"];
    const TY: &[&str] = &["t2t_sofa_sit_cuddle_loop1_y", "t2t_sofa_sit_cuddle_loop2_y", "t2t_sofa_sit_cuddle_loop3_y", "t2t_sofa_sit_cuddle_loop4_y"];
    let sit = crate::anim::SIT_LOOPS;
    match (teen, x, accepted, make_out) {
        (false, true, true, false) => A::steps("a2a_sofa_sit_cuddle_kiss_x", &[], AX),
        (false, false, true, false) => A::steps("a2a_sofa_sit_cuddle_kiss_y", &[], AY),
        (true, true, true, false) => A::steps("t2t_sofa_sit_cuddle_kiss_x", &[], TX),
        (true, false, true, false) => A::steps("t2t_sofa_sit_cuddle_kiss_y", &[], TY),
        (false, true, false, false) => A::steps("a2a_sofa_sit_cuddle_kiss_reject_x", &[], sit),
        (false, false, false, false) => A::steps("a2a_sofa_sit_cuddle_kiss_reject_y", &[], sit),
        (true, true, false, false) => A::steps("t2t_sofa_sit_cuddle_kiss_reject_x", &[], sit),
        (true, false, false, false) => A::steps("t2t_sofa_sit_cuddle_kiss_reject_y", &[], sit),
        (false, true, true, true) => A::steps(
            "a2a_sofa_sit_cuddle_makeout_start_x",
            &["a2a_sofa_sit_cuddle_makeout_loop_x", "a2a_sofa_sit_cuddle_makeout_loop2_x", "a2a_sofa_sit_cuddle_makeout_loop_x", "a2a_sofa_sit_cuddle_makeout_loop2_x", "a2a_sofa_sit_cuddle_makeout_stop_x"],
            AX,
        ),
        (false, false, true, true) => A::steps(
            "a2a_sofa_sit_cuddle_makeout_start_y",
            &["a2a_sofa_sit_cuddle_makeout_loop_y", "a2a_sofa_sit_cuddle_makeout_loop2_y", "a2a_sofa_sit_cuddle_makeout_loop_y", "a2a_sofa_sit_cuddle_makeout_loop2_y", "a2a_sofa_sit_cuddle_makeout_stop_y"],
            AY,
        ),
        (true, true, true, true) => A::steps(
            "t2t_sofa_sit_cuddle_makeout_start_x",
            &["t2t_sofa_sit_cuddle_makeout_loop_x", "t2t_sofa_sit_cuddle_makeout_loop2_x", "t2t_sofa_sit_cuddle_makeout_loop_x", "t2t_sofa_sit_cuddle_makeout_loop2_x", "t2t_sofa_sit_cuddle_makeout_stop_x"],
            TX,
        ),
        (true, false, true, true) => A::steps(
            "t2t_sofa_sit_cuddle_makeout_start_y",
            &["t2t_sofa_sit_cuddle_makeout_loop_y", "t2t_sofa_sit_cuddle_makeout_loop2_y", "t2t_sofa_sit_cuddle_makeout_loop_y", "t2t_sofa_sit_cuddle_makeout_loop2_y", "t2t_sofa_sit_cuddle_makeout_stop_y"],
            TY,
        ),
        (false, true, false, true) => A::steps("a2a_sofa_sit_cuddle_makeout_reject_x", &[], sit),
        (false, false, false, true) => A::steps("a2a_sofa_sit_cuddle_makeout_reject_y", &[], sit),
        (true, true, false, true) => A::steps("t2t_sofa_sit_cuddle_makeout_reject_x", &[], sit),
        (true, false, false, true) => A::steps("t2t_sofa_sit_cuddle_makeout_reject_y", &[], sit),
    }
}

/// What's done while cuddling, the game's: a kiss, or making out.
pub const WHILE_CUDDLING: [&str; 2] = ["Kiss", "Make Out"];

/// Out of a cuddle, back to sitting.
pub fn uncuddle_clip(x: bool, teen: bool) -> crate::anim::ActionClip {
    use crate::anim::ActionClip as A;
    match (teen, x) {
        (false, true) => A::steps("a2a_sofa_cuddle_trans_sit_x", &[], crate::anim::SIT_LOOPS),
        (false, false) => A::steps("a2a_sofa_cuddle_trans_sit_y", &[], crate::anim::SIT_LOOPS),
        (true, true) => A::steps("t2t_sofa_cuddle_trans_sit_x", &[], crate::anim::SIT_LOOPS),
        (true, false) => A::steps("t2t_sofa_cuddle_trans_sit_y", &[], crate::anim::SIT_LOOPS),
    }
}

/// Whether one of two cuddling is the `x` (their partner to their −X, local).
pub fn cuddles_as_x(me: &Transform, partner: Vec3) -> bool {
    (me.rotation.inverse() * (partner - me.translation)).x < 0.0
}

/// A cuddle under way, on the sofa or in bed: who with, which of the two (in bed, `x` is the one
/// on its near side), whether a teens' one; whether it was taken (to lean out of it after).
#[derive(Component, Clone, Copy)]
pub struct Cuddling {
    pub with: Entity,
    pub x: bool,
    pub teen: bool,
    pub accepted: bool,
    pub bed: bool,
}

/// Where a Sim is for a cuddle: the sofa they're sat on, or the double bed they're relaxing in
/// (`true`), the game's `BedRelax` posture (under a talk they're having there).
pub fn cuddle_spot(queue: Option<&ActionQueue>, kind_of: impl Fn(Entity) -> Option<crate::interact::ObjectKind>) -> Option<(Entity, bool)> {
    use crate::interact::ObjectKind as K;
    match queue?.0.iter().find(|a| !(a.seated && matches!(a.kind, ActionKind::Social { .. })))? {
        Action { kind: ActionKind::Object { target, def }, phase, .. } => match kind_of(*target)? {
            K::Sofa => Some((*target, false)),
            K::BedDouble if matches!(phase, Phase::Running(_)) && crate::interact::interactions_for(K::BedDouble).get(*def).is_some_and(|d| d.name == "Relax") => Some((*target, true)),
            _ => None,
        },
        _ => None,
    }
}

/// Lying back in bed again, the far side's mirrored as its relaxing is.
fn relax_loops(near: bool) -> crate::anim::ActionClip {
    let mut c = crate::anim::ActionClip::new(None, &["a2o_bed_relax_loop"]);
    c.mirror = !near;
    c
}

/// Lying back after a cuddle on the bed's far side: its `_y` part's played as it is (the far
/// side's own), then their relaxing's mirrored again, as it is there (`lie_back`).
#[derive(Component)]
pub struct LieBack;

fn lie_back(mut commands: Commands, sims: Query<(Entity, &crate::anim::ClipPlayer, &SimAnim), With<LieBack>>) {
    for (e, player, anim) in &sims {
        if anim.pose != Pose::Lie {
            commands.entity(e).remove::<LieBack>();
        } else if player.name.starts_with("a2o_bed_relax_loop") {
            commands.entity(e).remove::<LieBack>().insert(relax_loops(false));
        }
    }
}

/// A cuddle in bed, the two relaxing in it (the game's `BedCuddle`, grown-ups' alone): turned to
/// each other and cuddling, or turned away. The one on the bed's near side plays the `_x` part,
/// the far side's the `_y` (each its own side's: `anim::clip_anchor`).
pub fn bed_cuddle_clip(near: bool, accepted: bool) -> crate::anim::ActionClip {
    use crate::anim::ActionClip as A;
    match (near, accepted) {
        (true, true) => A::steps("a2a_bed_relax_trans2cuddle_invite_x", &[], BED_CUDDLE_X),
        (false, true) => A::steps("a2a_bed_relax_trans2cuddle_invite_y", &[], BED_CUDDLE_Y),
        (true, false) => A::steps("a2a_bed_relax_cuddler_reject_x", &[], &["a2o_bed_relax_loop"]),
        (false, false) => A::steps("a2a_bed_relax_cuddler_reject_y", &[], &["a2o_bed_relax_loop"]),
    }
}
const BED_CUDDLE_X: &[&str] = &["a2a_bed_relax_cuddle_loop1_x", "a2a_bed_relax_cuddle_loop2_x"];
const BED_CUDDLE_Y: &[&str] = &["a2a_bed_relax_cuddle_loop1_y", "a2a_bed_relax_cuddle_loop2_y"];

/// Out of a cuddle in bed, back to lying there.
pub fn bed_uncuddle_clip(near: bool) -> crate::anim::ActionClip {
    crate::anim::ActionClip::steps(if near { "a2a_bed_cuddle_trans2relax_x" } else { "a2a_bed_cuddle_trans2relax_y" }, &[], &["a2o_bed_relax_loop"])
}

/// A kiss or a making out while cuddling in bed (the game's `BedCuddle` kiss and make out): asked
/// for and taken, or turned away, then back to cuddling.
pub fn bed_cuddle_kiss_clip(near: bool, accepted: bool, make_out: bool) -> crate::anim::ActionClip {
    use crate::anim::ActionClip as A;
    let (invite, taken, turned, loops): (&str, &'static [&'static str], &'static [&'static str], &'static [&'static str]) = match (near, make_out) {
        (true, false) => ("a2a_bed_cuddle_kiss_invite_x", &["a2a_bed_cuddle_kiss_accept_x", "a2a_bed_kiss_trans_cuddle_x"], &["a2a_bed_cuddle_kiss_reject_x"], BED_CUDDLE_X),
        (false, false) => ("a2a_bed_cuddle_kiss_invite_y", &["a2a_bed_cuddle_kiss_accept_y", "a2a_bed_kiss_trans_cuddle_y"], &["a2a_bed_cuddle_kiss_reject_y"], BED_CUDDLE_Y),
        (true, true) => (
            "a2a_bed_cuddle_makeout_invite_x",
            &["a2a_bed_cuddle_makeout_accept_x", "a2a_bed_cuddle_makeout_loop1_x", "a2a_bed_cuddle_makeout_loop2_x", "a2a_bed_cuddle_makeout_loop1_x", "a2a_bed_cuddle_makeout_loop2_x", "a2a_bed_makeout_trans_cuddle_x"],
            &["a2a_bed_cuddle_makeout_reject_x"],
            BED_CUDDLE_X,
        ),
        (false, true) => (
            "a2a_bed_cuddle_makeout_invite_y",
            &["a2a_bed_cuddle_makeout_accept_y", "a2a_bed_cuddle_makeout_loop1_y", "a2a_bed_cuddle_makeout_loop2_y", "a2a_bed_cuddle_makeout_loop1_y", "a2a_bed_cuddle_makeout_loop2_y", "a2a_bed_makeout_trans_cuddle_y"],
            &["a2a_bed_cuddle_makeout_reject_y"],
            BED_CUDDLE_Y,
        ),
    };
    A::steps(invite, if accepted { taken } else { turned }, loops)
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

    #[test]
    fn a_cuddle_leans_each_towards_the_other() {
        // (Sat side by side facing the same way: the one with the other to their −X is `x`.)
        let me = Transform::from_xyz(1.0, 0.0, 0.0);
        assert!(cuddles_as_x(&me, Vec3::new(0.2, 0.0, 0.0)));
        assert!(!cuddles_as_x(&me, Vec3::new(1.8, 0.0, 0.0)));
        // (Turned about, the other way.)
        let turned = Transform::from_xyz(1.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::PI));
        assert!(!cuddles_as_x(&turned, Vec3::new(0.2, 0.0, 0.0)));
        assert_eq!(cuddle_clip(true, false, true).start, Some("a2a_sofa_sit_trans_cuddle_x"));
        assert_eq!(cuddle_clip(false, true, false).start, Some("t2t_sofa_sit_cuddle_reject_y"));
        assert_eq!(uncuddle_clip(false, false).start, Some("a2a_sofa_cuddle_trans_sit_y"));
    }

    #[test]
    fn a_cuddle_in_bed_is_played_from_each_side() {
        // (The near side's the `_x` part, the far side's the `_y`, each as it is, round their
        // own side.)
        let near = bed_cuddle_clip(true, true);
        assert_eq!(near.start, Some("a2a_bed_relax_trans2cuddle_invite_x"));
        let far = bed_cuddle_clip(false, true);
        assert!(!far.mirror && far.loops.iter().all(|l| l.ends_with("_y")));
        assert!(crate::anim::clip_anchor("a2a_bed_relax_cuddle_loop1_y").is_some_and(|(at, _)| at.x < 0.0));
        assert!(relax_loops(false).mirror);
        // (A kiss turned away: back to the cuddle all the same.)
        let k = bed_cuddle_kiss_clip(true, false, false);
        assert_eq!(k.steps, &["a2a_bed_cuddle_kiss_reject_x"]);
        assert_eq!(k.loops, BED_CUDDLE_X);
    }
}
