//! The computer's forums, blog and coupons, as the game's `Computer`: the Grumpy, the Evil, the
//! Mean-Spirited and the Rebellious "Troll On Forum" (`MakeForumPostingTroll`, fun 15 an hour),
//! the Inappropriate "Make Inappropriate Forum Post" (fun 10), typing till the post's done (each
//! ten minutes a chance in three and a third, `kLengthVariance`): posted, they Fight the Power,
//! and "Check back later to see if anyone has responded to your forum posting." Half the posts
//! draw a reply some three to twenty-four hours on (`kChanceOfGettingReplyPerPost`,
//! `kMinimumTimeToResponse`, `kAverageTimeToExpire`); the rest expire after a day, and every post's
//! gone after two (`kAverageTimeToDelete`). While there are posts, "Check For Responses" reads the
//! replies one by one (Fiendishly Delighted: "Your forum post got a reply: ...", a stranger's
//! words, `PostResponseTroll0`...), or finds them still waiting, or all expired (Disappointed). A
//! Frugal Sim can "Find Coupons" (each ten minutes a chance in five of one, two at most:
//! `FindCoupons`), and anyone teen and up can "Blog" (fun 10 an hour, Writing 350). The posts are
//! kept in saves.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{ActionKind, ActionQueue, Phase, Special};
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{Age, Sim};

pub struct ForumsPlugin;

impl Plugin for ForumsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (typing, forums_test).chain().after(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words for them.
pub const TROLL: &str = "Troll On Forum";
pub const INAPPROPRIATE: &str = "Make Inappropriate Forum Post";
pub const CHECK: &str = "Check For Responses";
pub const FIND_COUPONS: &str = "Find Coupons";
pub const BLOG: &str = "Blog";

/// The tuning (the XML's where it has it): how often the typing's looked at, the chance a post's
/// done then, the chance it draws a reply and when, when it expires and when it's gone; the
/// chance of a coupon each time, and how many.
const TICK: f64 = 10.0;
const POST_CHANCE: f64 = 0.3;
const REPLY_CHANCE: f64 = 0.5;
const REPLY_HOURS: (f64, f64) = (3.0, 24.0);
const EXPIRE_HOURS: f64 = 24.0;
const DELETE_HOURS: f64 = 48.0;
const COUPON_CHANCE: f64 = 0.2;
const MOST_COUPONS: u32 = 2;

/// How a post was written (`PostingStyle`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Style {
    Troll,
    Inappropriate,
}

/// A forum post (`ForumPosting`): its style, when it's replied to (if it will be), when it
/// expires unanswered (if it will), and when it's gone (game minutes).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Post {
    pub style: Style,
    pub reply_at: Option<f64>,
    pub expire_at: Option<f64>,
    pub delete_at: f64,
}

impl Post {
    /// Posted now: a reply coming (`roll` under the chance: some hours on, `when` 0 to 1 through
    /// them, as a bell curve has them), or not.
    pub fn new(style: Style, now: f64, roll: f64, when: f64) -> Self {
        let replies = roll < REPLY_CHANCE;
        Post {
            style,
            reply_at: replies.then(|| now + (REPLY_HOURS.0 + when.clamp(0.0, 1.0) * (REPLY_HOURS.1 - REPLY_HOURS.0)) * 60.0),
            expire_at: (!replies).then_some(now + EXPIRE_HOURS * 60.0),
            delete_at: now + DELETE_HOURS * 60.0,
        }
    }
}

/// A Sim's forum posts.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Posts(pub Vec<Post>);

/// Whether one's offered (its tuning's ages and traits; replies to check only with posts out,
/// coupons only where there's a store for them).
pub fn offers(special: Special, sim: &Sim, posts: bool, stores: bool) -> bool {
    let has = |t: &[Trait]| t.iter().any(|t| sim.traits.contains(t));
    let teen_up = sim.age == Age::Teen || sim.age.is_grown();
    let technophobe = sim.traits.contains(&Trait::Technophobe);
    match special {
        Special::TrollForum => teen_up && !technophobe && has(&[Trait::Grumpy, Trait::Evil, Trait::MeanSpirited, Trait::Rebellious]),
        Special::InappropriatePost => teen_up && !technophobe && has(&[Trait::Inappropriate]),
        Special::CheckResponses => teen_up && posts,
        Special::FindCoupons => teen_up && !technophobe && has(&[Trait::Frugal]) && stores,
        Special::Blog => teen_up && !technophobe,
        _ => true,
    }
}

/// What checking a post finds (`CheckForResponses`): its reply, or nothing yet, or all expired.
#[derive(Clone, Debug, PartialEq)]
pub enum Checked {
    Reply(Style),
    Waiting,
    Expired,
    None,
}

/// The first post checked: replied to (gone), still waiting, or expired (all of them gone).
pub fn check(posts: &mut Vec<Post>, now: f64) -> Checked {
    let Some(p) = posts.first() else { return Checked::None };
    if p.reply_at.is_some_and(|t| now >= t) {
        let style = p.style;
        posts.remove(0);
        Checked::Reply(style)
    } else if p.expire_at.is_some_and(|t| now >= t) {
        posts.clear();
        Checked::Expired
    } else {
        Checked::Waiting
    }
}

/// At the computer, one of these going: which, when it's next looked at, coupons found so far,
/// and whether a reply's been read.
#[derive(Component)]
struct Typing {
    special: Special,
    next: f64,
    found: u32,
    replied: bool,
}

/// The game's words (`Gameplay/Objects/Computer:...`), with what's not in the tables.
fn words(ui: Option<&crate::layout::UiAssets>, key: &str, or: &str) -> String {
    ui.and_then(|u| u.localize(&format!("Gameplay/Objects/Computer:{key}"))).filter(|s| !s.is_empty()).unwrap_or_else(|| or.to_string()).replace("\\n", "\n")
}

/// A reply's words: one of the style's two (a woman's or a man's), signed by a stranger.
fn reply(ui: Option<&crate::layout::UiAssets>, style: Style, rng: &mut impl Rng) -> String {
    let female = rng.random_bool(0.5);
    let n = rng.random_range(0..=1);
    let fallback = match (style, n) {
        (Style::Troll, 0) => "What??? Try checking the facts before you open your mouth. I'll have you know that I have a doctorate in the subject, and am also clearly more intelligent than you.\n–\n{0.String} {1.String}",
        (Style::Troll, _) => "There's no place here for trolls like you. Your post is clearly against the forum's terms of service, and I'm going to request that your account be deleted!\n–\n{0.String} {1.String}",
        (Style::Inappropriate, 0) => "The comments in your last post were rude, condescending, and disgusting. This place is no longer a haven for intelligent discussion, and I will no longer be a part of it.\n–\n{0.String} {1.String}",
        (Style::Inappropriate, _) => "I honestly don't see what your response has to do with anything. All you're doing is avoiding the subject and insulting me. Please stop wasting my time.\n–\n{0.String} {1.String}",
    };
    let key = format!("PostResponse{style:?}{n}{}", if female { "_Female" } else { "" });
    let text = words(ui, &key, fallback).replace("{0.String}", &crate::sim::random_given_name(rng, female)).replace("{1.String}", &crate::sim::random_last_name(rng)).replace("//", "");
    words(ui, "PostResponseFormat", "Your forum post got a reply:\n\n\"{0.String}\"").replace("{0.String}", &text)
}

/// The forums, the coupons and the posts: each ten minutes at it, a post done (or not), a reply
/// read, a coupon found; old posts gone.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn typing(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, Option<&mut Typing>, Option<&mut Posts>, Option<&mut Moodlets>)>,
    objects: Query<&crate::interact::GameObject>,
    (coupons, ui, layout): (Res<crate::coupons::Coupons>, Option<Res<crate::icons::GameUi>>, Option<Res<crate::layout::UiAssets>>),
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    let layout = layout.as_deref();
    for (e, sim, mut queue, typing, mut posts, mut moodlets) in &mut sims {
        if let Some(p) = posts.as_mut()
            && p.0.iter().any(|x| now >= x.delete_at)
        {
            p.0.retain(|x| now < x.delete_at);
        }
        let special = queue.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).and_then(|a| match a.kind {
            ActionKind::Object { target, def } => objects
                .get(target)
                .ok()
                .and_then(|o| crate::interact::interactions_for(o.kind).get(def))
                .map(|d| d.special)
                .filter(|s| matches!(s, Special::TrollForum | Special::InappropriatePost | Special::CheckResponses | Special::FindCoupons)),
            _ => None,
        });
        let Some(special) = special else {
            if typing.is_some() {
                commands.entity(e).remove::<Typing>();
            }
            continue;
        };
        let Some(mut t) = typing.filter(|t| t.special == special) else {
            commands.entity(e).insert(Typing { special, next: now + TICK, found: 0, replied: false });
            continue;
        };
        if now < t.next {
            continue;
        }
        t.next += TICK;
        let mut done = false;
        match special {
            Special::TrollForum | Special::InappropriatePost if rng.random_bool(POST_CHANCE) => {
                let style = if special == Special::TrollForum { Style::Troll } else { Style::Inappropriate };
                let post = Post::new(style, now, rng.random(), (rng.random::<f64>() + rng.random::<f64>()) / 2.0);
                match posts.as_mut() {
                    Some(p) => p.0.push(post),
                    None => {
                        commands.entity(e).insert(Posts(vec![post]));
                    }
                }
                if let Some(m) = moodlets.as_mut() {
                    m.add(MoodletKind::FightThePower, now);
                }
                notes.push(words(layout, "PostResponseRememberToCheck", "Check back later to see if anyone has responded to your forum posting."));
                info!("forums: {} posted ({style:?})", sim.first);
                done = true;
            }
            Special::CheckResponses => {
                let mut none = Vec::new();
                let list = posts.as_mut().map_or(&mut none, |p| &mut p.0);
                match check(list, now) {
                    Checked::Reply(style) => {
                        t.replied = true;
                        if let Some(m) = moodlets.as_mut() {
                            m.add(MoodletKind::FiendishlyDelighted, now);
                        }
                        notes.push(reply(layout, style, &mut rng));
                        done = list.is_empty();
                    }
                    Checked::Expired => {
                        if let Some(m) = moodlets.as_mut() {
                            m.add(MoodletKind::Disappointed, now);
                        }
                        notes.push(if t.replied {
                            words(layout, "PostResponseExpiredOther", "Your other forum postings have expired without a response.")
                        } else {
                            words(layout, "PostResponseExpired", "All your forum postings have expired without a response.")
                        });
                        done = true;
                    }
                    Checked::Waiting => {
                        notes.push(if t.replied {
                            words(layout, "PostResponseStillWaitingOther", "No one has replied to your other forum postings yet. Better check back later.")
                        } else {
                            words(layout, "PostResponseStillWaiting", "No one has replied to your forum posting yet. Better check back later.")
                        });
                        done = true;
                    }
                    Checked::None => done = true,
                }
            }
            Special::FindCoupons if rng.random_bool(COUPON_CHANCE) => match ui.as_ref().and_then(|u| crate::coupons::create(&mut commands, &coupons, e, &u.data, now)) {
                Some(note) => {
                    notes.push(note);
                    t.found += 1;
                    done = t.found >= MOST_COUPONS;
                }
                None => done = true,
            },
            _ => {}
        }
        // (Done: the interaction's over, as its exit has it.)
        if done && let Some(a) = queue.0.front_mut() {
            a.phase = Phase::Running(1.0e9);
        }
    }
}

/// `FORUMS_TEST=troll|inappropriate|check|coupons|blog` (tests): the selected Sim (given the trait
/// it needs) at the lot's computer doing it, ten seconds in; `FORUMS_POSTS=<n>`: that many posts
/// out already, replied to (`FORUMS_REPLIED=1`) or expired.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn forums_test(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &mut Sim, &mut ActionQueue, &Transform), With<crate::sim::Selected>>,
    objects: Query<(Entity, &crate::interact::GameObject)>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut done: Local<(bool, bool)>,
) {
    let Ok(what) = std::env::var("FORUMS_TEST") else { return };
    if done.0 || time.elapsed_secs() < 10.0 {
        return;
    }
    let Ok((me, mut sim, mut q, tf)) = sims.single_mut() else { return };
    // (No computer on the lot: one put down before them on a desk's height, a moment to settle.)
    if !objects.iter().any(|(_, o)| o.kind == crate::interact::ObjectKind::Computer) {
        if !done.1 {
            done.1 = true;
            let objd = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("ComputerCheap")).map(|c| c.objd);
            let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 1.5);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = objd.and_then(|o| crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, o, at, 0.0)) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("forums test: a computer put down");
            }
        }
        return;
    }
    done.0 = true;
    // (FORUMS_TEST=menu: Evil and Frugal, with a post out, the computer's menu.)
    if what == "menu" {
        for t in [Trait::Evil, Trait::Frugal] {
            if !sim.traits.contains(&t) {
                sim.traits.push(t);
            }
        }
        commands.entity(me).insert(Posts(vec![Post::new(Style::Troll, clock.minutes, 0.0, 0.5)]));
        if let Some((computer, _)) = objects.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Computer) {
            commands.insert_resource(crate::hud::PieFor(computer));
        }
        return;
    }
    let (special, label, traits) = match what.as_str() {
        "troll" => (Special::TrollForum, TROLL, Trait::Evil),
        "inappropriate" => (Special::InappropriatePost, INAPPROPRIATE, Trait::Inappropriate),
        "check" => (Special::CheckResponses, CHECK, Trait::Evil),
        "coupons" => (Special::FindCoupons, FIND_COUPONS, Trait::Frugal),
        _ => (Special::Blog, BLOG, Trait::Evil),
    };
    if !sim.traits.contains(&traits) {
        sim.traits.push(traits);
    }
    if let Some(n) = std::env::var("FORUMS_POSTS").ok().and_then(|v| v.parse::<usize>().ok()) {
        let now = clock.minutes;
        let replied = std::env::var_os("FORUMS_REPLIED").is_some();
        let posts = (0..n).map(|_| Post { style: Style::Troll, reply_at: replied.then_some(now - 1.0), expire_at: (!replied).then_some(now - 1.0), delete_at: now + 600.0 }).collect();
        commands.entity(me).insert(Posts(posts));
    }
    let Some(computer) = objects.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Computer).map(|(e, _)| e) else {
        info!("forums test: no computer");
        return;
    };
    let Some(def) = crate::interact::interactions_for(crate::interact::ObjectKind::Computer).iter().position(|d| d.special == special) else { return };
    q.push_player(crate::interact::Action::new(label, ActionKind::Object { target: computer, def }, false));
    info!("forums test: {label}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(traits: &[Trait], age: Age) -> Sim {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "T", Some(true), age);
        s.traits = traits.to_vec();
        s
    }

    #[test]
    fn the_traits_have_their_forums() {
        assert!(offers(Special::TrollForum, &sim(&[Trait::Grumpy], Age::Adult), false, false));
        assert!(offers(Special::TrollForum, &sim(&[Trait::Rebellious], Age::Teen), false, false));
        assert!(!offers(Special::TrollForum, &sim(&[Trait::Rebellious], Age::Child), false, false));
        assert!(!offers(Special::TrollForum, &sim(&[Trait::Inappropriate], Age::Adult), false, false));
        assert!(offers(Special::InappropriatePost, &sim(&[Trait::Inappropriate], Age::Adult), false, false));
        assert!(!offers(Special::CheckResponses, &sim(&[], Age::Adult), false, false));
        assert!(offers(Special::CheckResponses, &sim(&[], Age::Adult), true, false));
        assert!(!offers(Special::FindCoupons, &sim(&[Trait::Frugal], Age::Adult), false, false));
        assert!(offers(Special::FindCoupons, &sim(&[Trait::Frugal], Age::Adult), false, true));
        assert!(offers(Special::Blog, &sim(&[], Age::Elder), false, false));
        assert!(!offers(Special::Blog, &sim(&[Trait::Technophobe], Age::Adult), false, false));
    }

    #[test]
    fn posts_are_replied_to_or_expire_and_are_checked_in_turn() {
        // (Half reply, three to twenty-four hours on; the rest expire after a day; all go after
        // two.)
        let p = Post::new(Style::Troll, 0.0, 0.2, 0.0);
        assert_eq!((p.reply_at, p.expire_at, p.delete_at), (Some(180.0), None, 2880.0));
        let q = Post::new(Style::Inappropriate, 0.0, 0.9, 0.5);
        assert_eq!((q.reply_at, q.expire_at), (None, Some(1440.0)));
        assert_eq!(Post::new(Style::Troll, 0.0, 0.0, 1.0).reply_at, Some(1440.0));
        let mut posts = vec![p, q];
        assert_eq!(check(&mut posts, 100.0), Checked::Waiting);
        assert_eq!(check(&mut posts, 200.0), Checked::Reply(Style::Troll));
        assert_eq!(posts.len(), 1);
        assert_eq!(check(&mut posts, 1000.0), Checked::Waiting);
        assert_eq!(check(&mut posts, 1500.0), Checked::Expired);
        assert!(posts.is_empty());
        assert_eq!(check(&mut posts, 1500.0), Checked::None);
    }
}
