//! Celebrities of their careers, as the game's `Celebrity` has them (a rock star or symphony
//! player of the top levels, a sports star, a top politician: `Career.IsCelebrity`, the tuned
//! levels 8, 9, 8 and 10). Anyone child and up who comes within eight metres of one, not of their
//! household nor a friend or sweetheart of theirs, reacts once in two hours, eight times in ten
//! (`kChanceOfSimReacting`): a cheer or a wave, one in seven a boo
//! (`kProportionOfSimsThatRespondNegatively`); taken well, they get on a little (friendly 25,
//! liking 3) and eight times in ten go over to them: three times in ten, if the celebrity's of a
//! level to sign autographs (the musicians and the athletes), to ask for one, else to greet them.
//! Cheered enough in two hours (5, a boo taking off half), the celebrity's a Celebrity (+30, two
//! hours). One after an autograph follows them about, excited, then by turns impatient, then only
//! impatient (ten to fifty minutes each, `kState0Duration`...), the celebrity and an open book in
//! their thoughts; signed for ("Sign Autograph", the game's social), they're done; turned down
//! ("Deny Autograph") or kept waiting too long, they like the celebrity 50 less, take it as an
//! insult and cry, sulk or boo (`FailSequence`).

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue};
use crate::nav::{Floor, PathFollow, WalkStyle};
use crate::sim::{Age, HouseholdMember, Sim};
use crate::social::Relationships;

pub struct CareerCelebsPlugin;

impl Plugin for CareerCelebsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (notice, seekers, celeb_test).chain().after(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words.
pub const SIGN: &str = "Sign Autograph";
pub const DENY: &str = "Deny Autograph";
pub const ASK: &str = "Ask for Autograph";

/// `Celebrity`'s tuning: the broadcaster's reach (metres), the chances, the cheers counted (over
/// two hours) for the moodlet, and the liking a positive reaction brings.
const REACH: f32 = 8.0;
const REACT_CHANCE: f64 = 0.8;
const NEGATIVE: f64 = 0.15;
const CHEERS_FOR_BUFF: f32 = 5.0;
const CHEERS_HOURS: f64 = 2.0;
const STC_UP: f32 = 25.0;
const LIKING_UP: f32 = 3.0;
const FOLLOW_UP: f64 = 0.8;
/// How often the broadcaster pulses those in its reach (`PulseRepeatTime`).
const PULSE_EVERY: f64 = 20.0;
const AUTOGRAPH_VS_SOCIAL: f64 = 0.3;
/// `AskForAutograph`'s: how far the celebrity may get before they follow (and how near they
/// come), each state's minutes, how often the thought comes back, and what a failure costs.
const TETHER: f32 = 5.0;
const COME_WITHIN: f32 = 2.5;
const STATE_MINUTES: (f64, f64) = (10.0, 50.0);
const BALLOON_EVERY: f64 = 7.0;
const FAILED_LIKING: f32 = -50.0;
const FAILED_INSULTING: f32 = 50.0;

/// The excited and impatient anims (`kExcitedAnims`, `kImpatientAnims`), and the reactions to
/// failing (`kFailedReactions`).
const EXCITED: &[&str] = &[
    "a_react_excitedA_standing_x",
    "a_react_cheer_x",
    "a_react_surprise_standing_x",
    "a_soc_idle_neutral_happybounce_x",
    "a_react_pining_standing_x",
    "a_react_surprise1_standing_x",
    "a_react_waveA_standing_x",
    "a_react_waveB_standing_x",
    "a_react_cheer2_standing_x",
];
const IMPATIENT: &[&str] = &["a_react_annoyed_sigh_standing_x", "a_react_watch_distressed_standing_x", "a_react_whyMe_standing_x", "a_react_bored_x", "a_react_bored2_x"];
const FAILED: &[&str] = &["Cry", "CryModerate", "TantrumMild", "Boo", "ViewHate", "Shrug"];
/// Turning one down (`kDenyAnims`).
const DENY_CLIP: &str = "a_neutral_shoo_x";

/// Whether a career makes a celebrity of them, and whether one who signs autographs
/// (`Music.IsCelebrity`, `CanGiveAutograph`; `ProSports`'; `Political`'s, who's no autographs).
pub fn celebrity(career_key: &str, branch: &str, level: usize) -> (bool, bool) {
    match career_key {
        "Music" if branch == "ElectricRock" => (level >= 8, level >= 8),
        "Music" if branch == "Symphonic" => (level >= 9, level >= 9),
        "ProfessionalSports" | "Professional Sports" | "Athletic" | "ProSports" => (level >= 8, level >= 8),
        "Political" => (level >= 10, false),
        _ => (false, false),
    }
}

fn of_job(job: &crate::careers::Job) -> (bool, bool) {
    celebrity(job.career().key, job.path().branch, job.level + 1)
}

/// Whether this one may sign autographs (the "Sign Autograph" social's test).
pub fn signs(job: Option<&crate::careers::Job>) -> bool {
    job.is_some_and(|j| of_job(j).1)
}

/// What a celebrity's been met with lately: when, by whom, and whether well.
#[derive(Component, Default, Debug)]
pub struct Cheers(pub Vec<(f64, Entity, bool)>);

impl Cheers {
    /// Forgets what's older than two hours; whether this one's reacted within them.
    fn prune(&mut self, now: f64) {
        self.0.retain(|r| now - r.0 <= CHEERS_HOURS * 60.0);
    }

    pub fn reacted(&self, who: Entity) -> bool {
        self.0.iter().any(|r| r.1 == who)
    }

    /// The counter: a cheer one, a boo half off.
    pub fn score(&self) -> f32 {
        self.0.iter().map(|r| if r.2 { 1.0 } else { -0.5 }).sum()
    }
}

/// One after a celebrity's autograph: whose, how far along (excited, growing impatient,
/// impatient), the minutes left in it, and when the next anim and thought are due.
#[derive(Component, Clone, Copy, Debug)]
pub struct AutographSeeker {
    pub celeb: Entity,
    pub state: u8,
    pub left: f64,
    next_anim: f64,
    next_balloon: f64,
}

impl AutographSeeker {
    pub fn new(celeb: Entity, minutes: f64) -> Self {
        Self { celeb, state: 0, left: minutes, next_anim: 0.0, next_balloon: 0.0 }
    }

    /// The minutes gone: on to the next state when its time's up; whether they've given up.
    pub fn wait(&mut self, minutes: f64, next: impl FnMut() -> f64) -> bool {
        let mut next = next;
        self.left -= minutes;
        while self.left <= 0.0 && self.state < 3 {
            self.state += 1;
            if self.state < 3 {
                self.left += next();
            }
        }
        self.state >= 3
    }

    /// The anims for how they're feeling.
    fn anims(&self, coin: bool) -> &'static [&'static str] {
        match self.state {
            0 => EXCITED,
            1 if coin => EXCITED,
            _ => IMPATIENT,
        }
    }
}

fn state_minutes(rng: &mut impl Rng) -> f64 {
    // (A Gaussian over the range, as `RandomFloatGaussianDistribution`: the mean of three draws.)
    let (a, b) = STATE_MINUTES;
    (0..3).map(|_| rng.random_range(a..b)).sum::<f64>() / 3.0
}

/// Sets one after an autograph (`PushAskForAutograph`): what they were doing of their own accord
/// dropped, the asking on their queue.
pub fn ask(w: &mut World, fan: Entity, celeb: Entity) {
    let minutes = state_minutes(&mut rand::rng());
    let Ok(mut e) = w.get_entity_mut(fan) else { return };
    if e.contains::<AutographSeeker>() {
        return;
    }
    if let Some(mut q) = e.get_mut::<ActionQueue>() {
        for a in q.0.iter_mut().filter(|a| a.autonomous) {
            a.cancel = true;
        }
        q.0.push_back(Action::new(ASK, ActionKind::Outro { clips: &[], then: None, secs: f32::MAX, stand_at: None, target: celeb }, true));
    }
    e.insert(AutographSeeker::new(celeb, minutes));
}

/// Signed for (the social done): the fan's satisfied, and it's counted a cheer.
pub fn signed(w: &mut World, celeb: Entity, fan: Entity) {
    let now = w.resource::<crate::clock::GameClock>().minutes;
    if w.get::<AutographSeeker>(fan).is_some_and(|s| s.celeb == celeb) {
        stop_asking(w, fan);
    }
    cheer(w, celeb, fan, true, now);
}

/// Turned down: they've failed.
pub fn deny(w: &mut World, celeb: Entity, fan: Entity) {
    if w.get::<AutographSeeker>(fan).is_none_or(|s| s.celeb != celeb) {
        return;
    }
    if let Some(at) = w.get::<Transform>(fan).map(|t| t.translation)
        && let Ok(mut e) = w.get_entity_mut(celeb)
    {
        if let Some(mut tf) = e.get_mut::<Transform>() {
            let d = at - tf.translation;
            tf.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
        }
        if let Some(mut q) = e.get_mut::<ActionQueue>() {
            q.0.push_front(Action::new(DENY, ActionKind::Outro { clips: &[DENY_CLIP], then: None, secs: 1.83, stand_at: None, target: celeb }, false));
        }
        e.insert(crate::anim::ActionClip::new(Some(DENY_CLIP), &[]));
    }
    fail(w, fan, celeb);
}

fn stop_asking(w: &mut World, fan: Entity) {
    let Ok(mut e) = w.get_entity_mut(fan) else { return };
    e.remove::<(AutographSeeker, PathFollow)>();
    if let Some(mut q) = e.get_mut::<ActionQueue>() {
        for a in q.0.iter_mut().filter(|a| a.label == ASK) {
            a.cancel = true;
        }
    }
}

/// Gave up, or turned down (`FailSequence`): they like them less, take it as an insult, the
/// celebrity's counted a boo, and a sulk.
fn fail(w: &mut World, fan: Entity, celeb: Entity) {
    let now = w.resource::<crate::clock::GameClock>().minutes;
    stop_asking(w, fan);
    if let Some(mut rels) = w.get_mut::<Relationships>(fan) {
        rels.add(celeb, FAILED_LIKING, 0.0);
        rels.entry(celeb).stc.update(crate::conversation::Commodity::Insulting, FAILED_INSULTING);
    }
    cheer(w, celeb, fan, false, now);
    let at = w.get::<Transform>(celeb).map(|t| t.translation);
    let kind = *FAILED.choose(&mut rand::rng()).unwrap_or(&"Boo");
    if let Ok(mut e) = w.get_entity_mut(fan) {
        e.insert((crate::reactions::Pending::new(&[kind], at, now + 30.0), crate::balloons::BalloonRequest::thought(&crate::balloons::portrait(celeb))));
    }
    info!("autographs: a fan gave up on an autograph");
}

/// A reaction counted for the celebrity; cheered enough, they're a Celebrity.
fn cheer(w: &mut World, celeb: Entity, who: Entity, positive: bool, now: f64) {
    let Ok(mut e) = w.get_entity_mut(celeb) else { return };
    if !e.contains::<Cheers>() {
        e.insert(Cheers::default());
    }
    let Some(mut c) = e.get_mut::<Cheers>() else { return };
    c.prune(now);
    c.0.push((now, who, positive));
    if c.score() >= CHEERS_FOR_BUFF
        && let Some(mut m) = e.get_mut::<crate::life::Moodlets>()
    {
        m.add(crate::life::MoodletKind::Celebrity, now);
    }
}

/// Whether someone's free to react (nothing asked of them by the player, not after an autograph
/// already).
fn free(q: &ActionQueue) -> bool {
    q.0.iter().all(|a| a.autonomous || a.cancel)
}

/// Those near a celebrity notice them, as the game's broadcaster has it.
#[allow(clippy::type_complexity)]
fn notice(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    celebs: Query<(Entity, &crate::careers::Job, &Transform, Option<&Floor>, Has<HouseholdMember>, Option<&Cheers>)>,
    others: Query<(Entity, &Sim, &Transform, Option<&Floor>, Has<HouseholdMember>, &ActionQueue, &Relationships), Without<AutographSeeker>>,
    (mut last, mut pulsed): (Local<f64>, Local<std::collections::HashMap<(Entity, Entity), f64>>),
) {
    let now = clock.minutes;
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    pulsed.retain(|_, t| now - *t < PULSE_EVERY);
    let mut rng = rand::rng();
    for (celeb, job, tf, floor, member, cheers) in &celebs {
        let (is, signs) = of_job(job);
        if !is {
            continue;
        }
        let level = floor.map_or(1, |f| f.0);
        for (fan, sim, ftf, ffloor, fmember, q, rels) in &others {
            if fan == celeb
                || sim.age.is_little()
                || (member && fmember)
                || ffloor.map_or(1, |f| f.0) != level
                || ftf.translation.distance(tf.translation) > REACH
                || !free(q)
                || cheers.is_some_and(|c| c.0.iter().any(|r| r.1 == fan && now - r.0 <= CHEERS_HOURS * 60.0))
                || pulsed.contains_key(&(celeb, fan))
            {
                continue;
            }
            let r = rels.get(celeb);
            if r.friendship >= crate::social::FRIEND_THRESHOLD || r.is_romance() {
                continue;
            }
            // (Unmoved, they may be moved at the broadcaster's next pulse.)
            pulsed.insert((celeb, fan), now);
            if !rng.random_bool(REACT_CHANCE) {
                continue;
            }
            let positive = !rng.random_bool(NEGATIVE);
            let at = tf.translation;
            let follow = positive && rng.random_bool(FOLLOW_UP);
            let autograph = follow && signs && rng.random_bool(AUTOGRAPH_VS_SOCIAL);
            commands.queue(move |w: &mut World| {
                let kind = if positive { if rand::rng().random_bool(0.5) { "Cheer" } else { "Wave" } } else { "Boo" };
                if let Ok(mut e) = w.get_entity_mut(fan) {
                    e.insert(crate::reactions::Pending::new(&[kind], Some(at), now + 30.0));
                }
                if positive {
                    for (a, b) in [(fan, celeb), (celeb, fan)] {
                        if let Some(mut rels) = w.get_mut::<Relationships>(a) {
                            rels.add(b, LIKING_UP, 0.0);
                            rels.entry(b).stc.update(crate::conversation::Commodity::Friendly, STC_UP);
                        }
                    }
                }
                cheer(w, celeb, fan, positive, now);
                if autograph {
                    ask(w, fan, celeb);
                } else if follow
                    && let Some(i) = crate::social::social_index("Greet Friendly").or_else(|| crate::social::social_index("Chat"))
                    && let Some(mut q) = w.get_mut::<ActionQueue>(fan)
                {
                    q.0.push_back(Action::new(crate::social::social(i).name, ActionKind::Social { target: celeb, social: i }, true));
                }
                info!("autographs: someone {} at a celebrity ({kind}){}", if positive { "cheers" } else { "boos" }, if autograph { ", and wants an autograph" } else { "" });
            });
        }
    }
}

/// Those after an autograph: following the celebrity about, excited then impatient, and giving
/// up in the end; a celebrity free to signs for one waiting near.
#[allow(clippy::type_complexity)]
fn seekers(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    delta: Res<crate::clock::SimDelta>,
    ways: crate::playtag::Ways,
    mut fans: Query<(Entity, &mut AutographSeeker, &Transform, Option<&Floor>, Option<&crate::visit::OnLot>, Option<&PathFollow>, &ActionQueue, Has<crate::anim::ActionClip>)>,
    celebs: Query<(&Transform, Option<&crate::careers::Job>, &ActionQueue), Without<AutographSeeker>>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    let mut signing: Vec<Entity> = Vec::new();
    for (fan, mut s, tf, floor, lot, path, q, playing) in &mut fans {
        let Ok((ctf, job, cq)) = celebs.get(s.celeb) else {
            commands.queue(move |w: &mut World| stop_asking(w, fan));
            continue;
        };
        // (Told to stop, or the celebrity no longer one: done.)
        if !q.0.iter().any(|a| a.label == ASK && !a.cancel) || !signs(job) {
            debug!("autographs: {fan} stops asking (queue {:?})", q.0.iter().map(|a| a.label.as_str()).collect::<Vec<_>>());
            commands.queue(move |w: &mut World| stop_asking(w, fan));
            continue;
        }
        let was = s.state;
        if s.wait(delta.0 as f64, || state_minutes(&mut rng)) {
            let celeb = s.celeb;
            commands.queue(move |w: &mut World| fail(w, fan, celeb));
            continue;
        }
        if s.state != was {
            debug!("autographs: {fan} is now {} ({:.0} m off)", ["excited", "growing impatient", "impatient"][s.state.min(2) as usize], tf.translation.distance(ctf.translation));
        }
        if now >= s.next_balloon {
            s.next_balloon = now + BALLOON_EVERY;
            commands.entity(fan).try_insert(crate::balloons::BalloonRequest::thought(&crate::balloons::portrait(s.celeb)));
        }
        let (here, there) = (tf.translation, ctf.translation);
        let routing = path.is_some_and(|p| !p.done);
        if here.distance(there) > TETHER {
            if !routing {
                let to = there.xz() + (here.xz() - there.xz()).normalize_or_zero() * COME_WITHIN;
                match ways.route(lot, here.xz(), floor.map_or(1, |f| f.0), to) {
                    Some(wp) => {
                        commands.entity(fan).try_remove::<crate::anim::ActionClip>().try_insert(PathFollow::new(wp).with_style(WalkStyle::Run));
                    }
                    None => debug!("autographs: a fan can't get to the celebrity from {here}"),
                }
            }
            continue;
        }
        if routing {
            continue;
        }
        // (Turned to them, the next anim when the last's done.)
        if now >= s.next_anim || !playing {
            let clip = *s.anims(rng.random_bool(0.5)).choose(&mut rng).unwrap_or(&EXCITED[0]);
            s.next_anim = now + rng.random_range(4.0..9.0);
            let d = there - here;
            let rot = Quat::from_rotation_y(d.x.atan2(d.z));
            commands.entity(fan).try_remove::<PathFollow>().queue_silenced(move |mut e: EntityWorldMut| {
                if let Some(mut t) = e.get_mut::<Transform>() {
                    t.rotation = rot;
                }
                e.insert(crate::anim::ActionClip::new(Some(clip), &[]));
            });
        }
        // (A celebrity with nothing asked of them signs for one waiting by them, in a while.)
        // (CELEB_SIGN=1, tests: at once.)
        let eager = std::env::var_os("CELEB_SIGN").is_some();
        if (s.state >= 1 || eager) && free(cq) && !cq.0.iter().any(|a| a.label == SIGN) && !signing.contains(&s.celeb) && (eager || rng.random_bool((delta.0 as f64 / 20.0).clamp(0.0, 1.0))) {
            signing.push(s.celeb);
            let celeb = s.celeb;
            if let Some(i) = crate::social::social_index(SIGN) {
                commands.queue(move |w: &mut World| {
                    // (What they were doing of their own accord put off for it.)
                    if let Some(mut q) = w.get_mut::<ActionQueue>(celeb) {
                        for a in q.0.iter_mut().filter(|a| a.autonomous) {
                            a.cancel = true;
                        }
                        q.0.push_back(Action::new(SIGN, ActionKind::Social { target: fan, social: i }, true));
                    }
                });
            }
        }
    }
}

/// `CELEB_TEST=<career>:<level>[:<branch>]` (tests): eight seconds in, the selected Sim takes that
/// job, and (`CELEB_FAN=1`) the nearest Sim not of the household wants their autograph; the camera
/// on them.
fn celeb_test(
    mut commands: Commands,
    time: Res<Time>,
    me: Query<(Entity, &Transform), With<crate::sim::Selected>>,
    others: Query<(Entity, &Transform), (With<Sim>, Without<HouseholdMember>)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<u8>,
) {
    let Ok(v) = std::env::var("CELEB_TEST") else { return };
    let t = time.elapsed_secs();
    let Ok((e, tf)) = me.single() else { return };
    if t > 7.0
        && *stage < 2
        && let Ok(mut cam) = cams.single_mut()
    {
        cam.look_at(tf.translation);
        cam.distance = 10.0;
    }
    match *stage {
        0 if t > 8.0 => {
            *stage = 1;
            let mut parts = v.split(':');
            let (name, level, branch) = (parts.next().unwrap_or(""), parts.next().and_then(|l| l.parse::<usize>().ok()).unwrap_or(8), parts.next());
            let Some(track) = crate::careers::careers().iter().position(|c| c.key.eq_ignore_ascii_case(name) || c.name.eq_ignore_ascii_case(name)) else {
                warn!("celeb test: no career {name}");
                return;
            };
            let mut job = crate::careers::Job::new(track);
            job.level = level.max(1) - 1;
            if let Some(b) = branch.and_then(|b| crate::careers::careers()[track].paths.iter().position(|p| p.branch.eq_ignore_ascii_case(b))) {
                job.branch = b;
            }
            info!("celeb test: {} level {} ({:?})", job.career().key, job.level + 1, of_job(&job));
            commands.entity(e).insert(job);
        }
        1 if t > 10.0 => {
            if std::env::var_os("CELEB_FAN").is_none() {
                *stage = 2;
                return;
            }
            // (Once someone's come by: the welcome wagon, a passer-by.)
            let Some((fan, _)) = others.iter().filter(|o| o.1.translation.distance(tf.translation) < 40.0).min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation))) else { return };
            *stage = 2;
            commands.queue(move |w: &mut World| ask(w, fan, e));
            info!("celeb test: a fan ({fan}) wants an autograph");
            if let Ok(mut cam) = cams.single_mut() {
                cam.follow(fan);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_musicians_athletes_and_politicians_are_celebrities() {
        assert_eq!(celebrity("Music", "ElectricRock", 8), (true, true));
        assert_eq!(celebrity("Music", "Symphonic", 8), (false, false));
        assert_eq!(celebrity("Music", "Symphonic", 9), (true, true));
        assert_eq!(celebrity("Music", "Base", 10), (false, false));
        assert_eq!(celebrity("ProfessionalSports", "Base", 8), (true, true));
        assert_eq!(celebrity("Political", "Base", 9), (false, false));
        assert_eq!(celebrity("Political", "Base", 10), (true, false), "politicians sign nothing");
        assert_eq!(celebrity("Business", "Base", 10), (false, false));
    }

    #[test]
    fn cheers_make_a_celebrity_and_a_wait_runs_out() {
        let (a, b) = (Entity::from_bits(1), Entity::from_bits(2));
        let mut c = Cheers::default();
        c.0 = vec![(0.0, a, true), (10.0, b, false), (100.0, a, true)];
        assert_eq!(c.score(), 1.5);
        c.prune(150.0);
        assert_eq!(c.0.len(), 1, "two hours on, the first two forgotten");
        assert!(c.reacted(a) && !c.reacted(b));
        let mut s = AutographSeeker::new(a, 10.0);
        assert!(!s.wait(5.0, || 10.0) && s.state == 0);
        assert!(!s.wait(10.0, || 10.0) && s.state == 1);
        assert!(s.anims(false) == IMPATIENT && s.anims(true) == EXCITED);
        assert!(s.wait(30.0, || 10.0), "impatient past it: given up");
    }
}
