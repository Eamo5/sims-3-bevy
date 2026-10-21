//! What a Sim does on their own for who they are, as the game's `TraitFunctions`, from their
//! own menu or by free will: the Insane "Talk To Self" (half an hour arguing with nobody, the
//! game's `TraitInsane` state machine's talk, mock and yell: fun and company both); the Neurotic
//! "Freak Out" (teens and up, not while still Tranquil after the last: Tranquil for eight hours
//! after, and anyone who sees it shocked); the Dramatic "Fake Passing Out" on the floor for
//! show, ten to thirty minutes (`DramaticMinFakePassout`, `DramaticMaxFakePassout`); a
//! Natural-Born Performer's "TaDa!" (not again while still Born To Do This: those about cheer,
//! Drawn In for an hour, and the performer's Born To Do This an hour for each who did).
//!
//! And a Coward's faint (`CowardTraitFaint`), not theirs to choose: out cold on the floor half an
//! hour (`kPassOutMinutes`) at the sight of a ghost, a burglar or the Grim Reaper, or a fire one
//! time in five (TraitTuning's `kCowardTraitChanceToFaintWhenSeeingFire`; the rest of the time
//! they panic with everyone else).
//!
//! From the ground's menu: a Genius's "Contemplate Surroundings" (`Terrain.Contemplate`: with
//! something of worth within three tiles, `kGeniusTraitContemplateDistance`; over to where was
//! clicked, turned to one of those things at random, thinking of it, eight minutes of the game's
//! `Contemplate` clips, `kGeniusTraitContemplateLengthInMinutes`, then Fascinated; not while
//! Fascinated still); and a Neat Sim's "Clean House" at home (`NeatTraitCleanHouse`, indoors,
//! with something to clean: the nearest dirty dishes, unmade bed, puddle or grimy fixture seen
//! to, then the next, the "Clean House" after each in the queue, till there's nothing left or
//! they're in no state to go on).

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue};
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::{Pose, Sim};

pub struct TraitActsPlugin;

impl Plugin for TraitActsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Mess>()
            .add_systems(Update, (began.after(crate::interact::run_actions), act_test, frights, faint_now.before(crate::interact::run_actions), mess, contemplate_test).run_if(in_state(PlayMode::Live)));
    }
}

/// The Genius's contemplation: its name, how near what's contemplated must be (metres: tiles)
/// and how long it lasts.
pub const CONTEMPLATE: &str = "Contemplate Surroundings";
pub const CONTEMPLATE_DISTANCE: f32 = 3.0;
pub const CONTEMPLATE_MINUTES: f32 = 8.0;
/// The game's `Contemplate` state machine's clips.
pub const CONTEMPLATE_CLIPS: &[&str] = &["a_trait_genius_x", "a_trait_genius2_x"];

/// Whether a Genius may contemplate their surroundings, standing at `at`: not Fascinated still,
/// not swimming, and something of worth (what could be repossessed: priced) within reach.
pub fn may_contemplate(sim: &Sim, moodlets: Option<&Moodlets>, swimming: bool, at: Vec3, things: impl IntoIterator<Item = (Vec3, i32)>) -> bool {
    sim.traits.contains(&Trait::Genius)
        && !sim.age.is_little()
        && !swimming
        && !moodlets.is_some_and(|m| m.has(MoodletKind::Fascinated))
        && things.into_iter().any(|(p, price)| price > 0 && p.distance(at) <= CONTEMPLATE_DISTANCE)
}

/// What's contemplated: one of the things of worth about them, at random.
pub fn contemplated<T: Copy>(at: Vec3, things: impl IntoIterator<Item = (Vec3, i32, T)>, rng: &mut impl Rng) -> Option<(Vec3, T)> {
    let near: Vec<(Vec3, T)> = things.into_iter().filter(|(p, price, _)| *price > 0 && p.distance(at) <= CONTEMPLATE_DISTANCE).map(|(p, _, t)| (p, t)).collect();
    near.choose(rng).copied()
}

/// A Neat Sim cleaning house: their next chore's to be found (`autonomy`, the nearest thing to
/// clean, with "Clean House" again after it).
#[derive(Component)]
pub struct CleaningHouse;

/// The Neat's "Clean House".
pub const CLEAN_HOUSE: &str = "Clean House";

/// Whether there's something to clean at home (`Lot.FindObjectToClean`, `HasUnmadeBed`,
/// `HasCleanAllAblePuddlesOrBurntTiles`): dirty dishes, an unmade bed, a puddle, a grimy fixture.
#[derive(Resource, Default)]
pub struct Mess(pub bool);

#[allow(clippy::type_complexity)]
fn mess(mut m: ResMut<Mess>, objects: Query<(&crate::interact::GameObject, Has<crate::interact::Unmade>, Option<&crate::grime::Grime>), Without<crate::visit::LotObject>>) {
    use crate::interact::ObjectKind as K;
    let any = objects.iter().any(|(o, unmade, grime)| matches!(o.kind, K::DirtyDishes | K::Puddle) || unmade || grime.is_some_and(|g| g.needs_cleaning()));
    if m.0 != any {
        m.0 = any;
    }
}

/// Whether Clean House is on the ground's menu: a Neat Sim (asked, not of their own accord), at
/// home, indoors, with something to clean.
pub fn may_clean_house(sim: &Sim, at_home: bool, indoors: bool, mess: &Mess) -> bool {
    sim.traits.contains(&Trait::Neat) && !sim.age.is_little() && at_home && indoors && mess.0
}

/// The chores Clean House sees to.
pub fn cleans(special: crate::interact::Special) -> bool {
    use crate::interact::Special as S;
    matches!(special, S::CleanUp | S::Scrub | S::MopUp | S::MakeBed)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraitAct {
    TalkToSelf,
    FreakOut,
    FakePassOut,
    Tada,
    /// A Coward's faint (not on anyone's menu).
    Faint,
    /// A Neat Sim's Clean House (the ground's menu): the next chore found.
    CleanHouse,
}

/// `TalkToSelf.kInteractionLength`.
const TALK_MINUTES: f32 = 30.0;
/// `DramaticMinFakePassout`, `DramaticMaxFakePassout`, and the getting up after.
const FAKE_PASS_OUT: std::ops::Range<f32> = 10.0..30.0;
pub const GET_UP_MINUTES: f32 = 3.0;
/// `CowardTraitFaint.kPassOutMinutes`, and the chance of it at a fire.
const FAINT_MINUTES: f32 = 30.0;
pub const FAINT_AT_FIRE: f64 = 0.2;
/// The Tranquil moodlet's eight hours (`TimeoutLength` 480).
const TRANQUIL_MINUTES: f64 = 480.0;
/// `kBaseBornToDoThisLength` a cheering Sim, and `kDrawnInLength`.
const BORN_TO_DO_THIS_MINUTES: f64 = 60.0;
const DRAWN_IN_MINUTES: f64 = 60.0;
/// How far off those who see it react (the reaction broadcasters').
const WITNESS_RANGE: f32 = 10.0;

/// The Insane's talk with nobody: the `TraitInsane` state machine's (a child's own talk).
const TALK: &[&str] = &["a2a_soc_talkListen_loop150_x", "a2a_soc_Bad_Mock_Insulting_Bad_x", "a2a_soc_bad_YellAt_Insulting_Bad_x", "a2a_soc_Bad_DeclareNemesis_Steamed_Bad_x", "a_trait_insane_talk"];
const TALK_CHILD: &[&str] = &["a_trait_insane_talk"];

impl TraitAct {
    pub const ALL: [TraitAct; 4] = [TraitAct::TalkToSelf, TraitAct::FreakOut, TraitAct::FakePassOut, TraitAct::Tada];

    /// The game's names for them.
    pub fn name(self) -> &'static str {
        match self {
            TraitAct::TalkToSelf => "Talk To Self",
            TraitAct::FreakOut => "Freak Out",
            TraitAct::FakePassOut => "Fake Passing Out",
            TraitAct::Tada => "TaDa!",
            TraitAct::Faint => "Faint",
            TraitAct::CleanHouse => CLEAN_HOUSE,
        }
    }

    pub fn trait_(self) -> Trait {
        match self {
            TraitAct::TalkToSelf => Trait::Insane,
            TraitAct::FreakOut => Trait::Neurotic,
            TraitAct::FakePassOut => Trait::Dramatic,
            TraitAct::Tada => Trait::NaturalBornPerformer,
            TraitAct::Faint => Trait::Coward,
            TraitAct::CleanHouse => Trait::Neat,
        }
    }

    /// What it does for the needs an hour (the game's `TalkToSelf`: Fun 40, Social 50).
    pub fn per_hour(self) -> [f32; 6] {
        match self {
            TraitAct::TalkToSelf => [0.0, 0.0, -2.0, 60.0, 0.0, 50.0],
            TraitAct::Tada => [0.0, 0.0, 0.0, 0.0, 0.0, 5.0],
            _ => [0.0; 6],
        }
    }

    /// Whether this Sim may (their trait, their age, not too soon after the last).
    pub fn offered(self, sim: &Sim, moodlets: Option<&Moodlets>) -> bool {
        let has = |k: MoodletKind| moodlets.is_some_and(|m| m.has(k));
        sim.traits.contains(&self.trait_())
            && !sim.age.is_little()
            && match self {
                TraitAct::FreakOut => sim.age != crate::sim::Age::Child && !has(MoodletKind::Tranquil),
                TraitAct::Tada => sim.age != crate::sim::Age::Child && !has(MoodletKind::BornToDoThis),
                TraitAct::Faint | TraitAct::CleanHouse => false,
                _ => true,
            }
    }

    /// How long it takes (game minutes).
    pub fn minutes(self, rng: &mut impl Rng) -> f32 {
        match self {
            TraitAct::TalkToSelf => TALK_MINUTES,
            TraitAct::FreakOut => 4.0,
            TraitAct::FakePassOut => rng.random_range(FAKE_PASS_OUT) + GET_UP_MINUTES,
            TraitAct::Tada => 3.0,
            TraitAct::Faint => FAINT_MINUTES + GET_UP_MINUTES,
            TraitAct::CleanHouse => 0.0,
        }
    }

    /// How it's played.
    pub fn clip(self, child: bool) -> (Pose, ActionClip) {
        match self {
            TraitAct::TalkToSelf => (Pose::Use, ActionClip::new(None, if child { TALK_CHILD } else { TALK })),
            TraitAct::FreakOut => (Pose::Use, ActionClip::new(Some("a_trait_Neurotic_x"), &[])),
            TraitAct::FakePassOut => (Pose::Use, ActionClip::new(Some("a_motFail_exhausted_x"), &["a_sleeponFloor_breathe_x"])),
            TraitAct::Tada => (Pose::Use, ActionClip::new(Some("a_trait_naturalBornPerformer_x"), &[])),
            // (The `TraitCoward` state machine's pass out, then asleep on the floor.)
            TraitAct::Faint => (Pose::Use, ActionClip::new(Some(if child { "c_trait_coward3_x" } else { "a_trait_coward2_x" }), &["a_sleeponFloor_breathe_x"])),
            TraitAct::CleanHouse => (Pose::Stand, ActionClip::new(None, &["a_idle_neutral_loop_"])),
        }
    }

    /// The action for it.
    pub fn action(self, autonomous: bool) -> Action {
        let minutes = self.minutes(&mut rand::rng());
        Action::new(self.name(), ActionKind::TraitAct { act: self, minutes }, autonomous)
    }
}

/// What a Sim may do for who they are, for their own menu.
pub fn offered(sim: &Sim, moodlets: Option<&Moodlets>) -> Vec<(String, ActionKind)> {
    TraitAct::ALL
        .into_iter()
        .filter(|a| a.offered(sim, moodlets))
        .map(|a| {
            let Action { kind, .. } = a.action(false);
            (a.name().to_string(), kind)
        })
        .collect()
}

/// By free will, now and then: what the game's trait commodities (`TraitInsane`, `TraitNeurotic`)
/// lead to, scored as the rest of what they might do; a TaDa! only with someone to see it. The
/// fake faint's the player's to ask for (the game's isn't tuned for free will).
pub fn autonomous(sim: &Sim, moodlets: Option<&Moodlets>, gain: impl Fn([f32; 6], f32) -> f32, audience: bool) -> Option<(f32, TraitAct)> {
    let mut best: Option<(f32, TraitAct)> = None;
    for a in [TraitAct::TalkToSelf, TraitAct::FreakOut, TraitAct::Tada] {
        if !a.offered(sim, moodlets) {
            continue;
        }
        let score = match a {
            TraitAct::TalkToSelf => gain(a.per_hour(), TALK_MINUTES) * 1.25 + 4.0,
            TraitAct::FreakOut => 6.0,
            TraitAct::Tada if audience => 7.0,
            _ => continue,
        };
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, a));
        }
    }
    best
}

/// A Coward to faint at once, at something they've seen (put next, before what they were at, as
/// the game's `AddNext` at a higher priority).
#[derive(Component)]
pub struct Faints;

/// Whether a Sim's out cold.
pub fn fainting(q: &ActionQueue) -> bool {
    q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::TraitAct { act: TraitAct::Faint, .. }))
}

fn faint_now(mut commands: Commands, mut sims: Query<(Entity, &Sim, &mut ActionQueue, &crate::sim::SimAnim), With<Faints>>) {
    for (e, sim, mut q, anim) in &mut sims {
        commands.entity(e).remove::<Faints>();
        if !sim.traits.contains(&Trait::Coward) || sim.age.is_little() || anim.pose == Pose::Lie || crate::disturb::asleep(&q) || fainting(&q) {
            continue;
        }
        info!("{} faints", sim.first);
        if let Some(a) = q.0.front_mut() {
            a.cancel = true;
        }
        let at = q.0.len().min(1);
        q.0.insert(at, TraitAct::Faint.action(false));
    }
}

/// Cowards about when a burglar or the Grim Reaper comes: they faint at the sight
/// (`kCowardTraitChanceToFaintWhenSeeingBurglar`; the Reaper, always).
#[allow(clippy::type_complexity)]
fn frights(
    mut commands: Commands,
    scary: Query<(Entity, &Transform), Or<(With<crate::burglar::Burglar>, With<crate::death::GrimReaper>)>>,
    sims: Query<(Entity, &Sim, &Transform, &Visibility), (Without<crate::burglar::Burglar>, Without<crate::death::GrimReaper>)>,
    mut seen: Local<Vec<(Entity, Entity)>>,
) {
    seen.retain(|(s, _)| scary.contains(*s));
    for (s, stf) in &scary {
        for (e, sim, tf, vis) in &sims {
            if sim.traits.contains(&Trait::Coward) && *vis != Visibility::Hidden && tf.translation.distance(stf.translation) < WITNESS_RANGE && !seen.contains(&(s, e)) {
                seen.push((s, e));
                commands.entity(e).insert(Faints);
            }
        }
    }
}

/// Begun: what follows from it, at once.
#[derive(Component)]
pub struct Began(pub TraitAct);

/// A freak-out's tranquillity after and its witnesses' shock; a TaDa!'s cheers, and its pride.
#[allow(clippy::type_complexity)]
fn began(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    acts: Query<(Entity, &Began, &Transform, Option<&crate::nav::Floor>, Option<&crate::visit::OnLot>)>,
    others: Query<(Entity, &Sim, &Transform, Option<&crate::nav::Floor>, Option<&crate::visit::OnLot>, Option<&ActionQueue>)>,
    mut moodlets: Query<&mut Moodlets>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (me, began, tf, floor, lot) in &acts {
        commands.entity(me).remove::<Began>();
        let level = floor.map_or(1, |f| f.0);
        let lot = lot.map(|l| l.0);
        // Those about to see it (on the same floor of the same lot, near enough), not busy at
        // something they can't look up from.
        let witnesses: Vec<Entity> = others
            .iter()
            .filter(|(e, s, t, f, l, q)| {
                *e != me && !s.age.is_little() && f.map_or(1, |f| f.0) == level && l.map(|l| l.0) == lot && t.translation.distance(tf.translation) < WITNESS_RANGE
                    && q.is_some_and(|q| !q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::TraitAct { .. } | ActionKind::MotiveFail(_))))
            })
            .map(|w| w.0)
            .collect();
        match began.0 {
            TraitAct::FreakOut => {
                if let Ok(mut m) = moodlets.get_mut(me) {
                    m.add_valued(MoodletKind::Tranquil, now, MoodletKind::Tranquil.def().value, TRANQUIL_MINUTES);
                }
                for w in &witnesses {
                    commands.entity(*w).insert(crate::reactions::Pending::new(&["Shocked"], Some(tf.translation), now + 10.0));
                }
            }
            TraitAct::Tada => {
                for w in &witnesses {
                    let kind = *["Cheer", "PumpFist"].choose(&mut rng).unwrap_or(&"Cheer");
                    commands.entity(*w).insert(crate::reactions::Pending::new(&[kind], Some(tf.translation), now + 10.0));
                    if let Ok(mut m) = moodlets.get_mut(*w) {
                        m.add_valued(MoodletKind::DrawnIn, now, MoodletKind::DrawnIn.def().value, DRAWN_IN_MINUTES);
                    }
                }
                if !witnesses.is_empty()
                    && let Ok(mut m) = moodlets.get_mut(me)
                {
                    m.add_valued(MoodletKind::BornToDoThis, now, MoodletKind::BornToDoThis.def().value, BORN_TO_DO_THIS_MINUTES * witnesses.len() as f64);
                }
            }
            TraitAct::CleanHouse => {
                commands.entity(me).insert(CleaningHouse);
            }
            _ => {}
        }
    }
}

/// TRAIT_ACT=<act> (tests: "talk", "freak", "fake", "tada", "faint"): the selected Sim, given the trait
/// for it if they haven't it, does it.
fn act_test(mut done: Local<bool>, time: Res<Time>, mut sel: Query<(&mut Sim, &mut ActionQueue), With<crate::sim::Selected>>) {
    let Ok(want) = std::env::var("TRAIT_ACT") else { return };
    if *done || time.elapsed_secs() < 12.0 {
        return;
    }
    let Some(act) = TraitAct::ALL.into_iter().chain([TraitAct::Faint]).find(|a| a.name().to_ascii_lowercase().starts_with(&want.to_ascii_lowercase())) else { return };
    let Ok((mut sim, mut q)) = sel.single_mut() else { return };
    if !sim.traits.contains(&act.trait_()) {
        sim.traits.push(act.trait_());
    }
    q.0.clear();
    q.push_player(act.action(false));
    info!("trait act test: {} does {}", sim.first, act.name());
    *done = true;
}

/// CLEAN_HOUSE=1 or CONTEMPLATE=1 (tests): the selected Sim, given the trait for it, asked a few
/// seconds in to clean house (some mess made first: their bed unmade) or to contemplate where
/// they stand.
#[allow(clippy::type_complexity)]
fn contemplate_test(
    mut commands: Commands,
    mut done: Local<bool>,
    time: Res<Time>,
    mut sel: Query<(&mut Sim, &mut ActionQueue, &Transform, &crate::nav::Floor), With<crate::sim::Selected>>,
    beds: Query<(Entity, &crate::interact::GameObject), Without<crate::visit::LotObject>>,
) {
    let (clean, think) = (std::env::var_os("CLEAN_HOUSE").is_some(), std::env::var_os("CONTEMPLATE").is_some());
    if *done || !(clean || think) || time.elapsed_secs() < 12.0 {
        return;
    }
    let Ok((mut sim, mut q, tf, floor)) = sel.single_mut() else { return };
    *done = true;
    let want = if clean { Trait::Neat } else { Trait::Genius };
    if !sim.traits.contains(&want) {
        sim.traits.push(want);
    }
    q.0.clear();
    if clean {
        for (e, o) in &beds {
            if matches!(o.kind, crate::interact::ObjectKind::BedSingle | crate::interact::ObjectKind::BedDouble) {
                commands.entity(e).insert(crate::interact::Unmade);
            }
        }
        q.push_player(TraitAct::CleanHouse.action(false));
    } else {
        let at = tf.translation.xz();
        q.push_player(Action::new(CONTEMPLATE, ActionKind::Outdoor { at, level: floor.0, what: crate::seasonal::Outdoor::Contemplate }, false));
    }
    info!("trait test: {} asked to {}", sim.first, if clean { CLEAN_HOUSE } else { CONTEMPLATE });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_genius_contemplates_what_is_near_and_of_worth() {
        let mut rng = rand::rng();
        let mut sim = crate::sim::random_sim(&mut rng, "Test", Some(true), crate::sim::Age::Adult);
        sim.traits = vec![Trait::Genius];
        let at = Vec3::ZERO;
        let things = [(Vec3::new(2.0, 0.0, 0.0), 300, 1), (Vec3::new(9.0, 0.0, 0.0), 900, 2), (Vec3::new(1.0, 0.0, 0.0), 0, 3)];
        assert!(may_contemplate(&sim, None, false, at, things.iter().map(|t| (t.0, t.1))));
        assert!(!may_contemplate(&sim, None, true, at, things.iter().map(|t| (t.0, t.1))), "not swimming");
        let mut rapt = Moodlets::default();
        rapt.add(MoodletKind::Fascinated, 0.0);
        assert!(!may_contemplate(&sim, Some(&rapt), false, at, things.iter().map(|t| (t.0, t.1))), "not while Fascinated");
        assert!(!may_contemplate(&sim, None, false, Vec3::new(20.0, 0.0, 0.0), things.iter().map(|t| (t.0, t.1))), "nothing near");
        for _ in 0..20 {
            assert_eq!(contemplated(at, things, &mut rng).map(|c| c.1), Some(1), "only what's near and priced");
        }
        sim.traits = vec![Trait::Neat];
        assert!(!may_contemplate(&sim, None, false, at, things.iter().map(|t| (t.0, t.1))));
        assert!(may_clean_house(&sim, true, true, &Mess(true)));
        assert!(!may_clean_house(&sim, true, true, &Mess(false)), "nothing to clean");
        assert!(!may_clean_house(&sim, false, true, &Mess(true)), "only at home");
        assert!(!may_clean_house(&sim, true, false, &Mess(true)), "only indoors");
    }

    #[test]
    fn trait_acts_are_theirs_alone_and_not_again_too_soon() {
        let mut rng = rand::rng();
        let mut sim = crate::sim::random_sim(&mut rng, "Test", Some(true), crate::sim::Age::Adult);
        sim.traits = vec![Trait::Neurotic, Trait::Insane];
        let names: Vec<String> = offered(&sim, None).into_iter().map(|o| o.0).collect();
        assert_eq!(names, ["Talk To Self", "Freak Out"]);
        let mut calm = Moodlets::default();
        calm.add(MoodletKind::Tranquil, 0.0);
        assert_eq!(offered(&sim, Some(&calm)).len(), 1, "not while still Tranquil");
        sim.age = crate::sim::Age::Child;
        assert_eq!(offered(&sim, None).len(), 1, "children talk to themselves, but don't freak out");
        let m = TraitAct::FakePassOut.minutes(&mut rng);
        assert!((10.0 + GET_UP_MINUTES..30.0 + GET_UP_MINUTES).contains(&m));
    }
}
