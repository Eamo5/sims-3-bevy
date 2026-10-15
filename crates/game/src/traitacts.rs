//! What a Sim does on their own for who they are, as the game's `TraitFunctions`, from their
//! own menu or by free will: the Insane "Talk To Self" (half an hour arguing with nobody, the
//! game's `TraitInsane` state machine's talk, mock and yell: fun and company both); the Neurotic
//! "Freak Out" (teens and up, not while still Tranquil after the last: Tranquil for eight hours
//! after, and anyone who sees it shocked); the Dramatic "Fake Passing Out" on the floor for
//! show, ten to thirty minutes (`DramaticMinFakePassout`, `DramaticMaxFakePassout`); a
//! Natural-Born Performer's "TaDa!" (not again while still Born To Do This: those about cheer,
//! Drawn In for an hour, and the performer's Born To Do This an hour for each who did).

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
        app.add_systems(Update, (began.after(crate::interact::run_actions), act_test).run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraitAct {
    TalkToSelf,
    FreakOut,
    FakePassOut,
    Tada,
}

/// `TalkToSelf.kInteractionLength`.
const TALK_MINUTES: f32 = 30.0;
/// `DramaticMinFakePassout`, `DramaticMaxFakePassout`, and the getting up after.
const FAKE_PASS_OUT: std::ops::Range<f32> = 10.0..30.0;
pub const GET_UP_MINUTES: f32 = 3.0;
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
        }
    }

    pub fn trait_(self) -> Trait {
        match self {
            TraitAct::TalkToSelf => Trait::Insane,
            TraitAct::FreakOut => Trait::Neurotic,
            TraitAct::FakePassOut => Trait::Dramatic,
            TraitAct::Tada => Trait::NaturalBornPerformer,
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
        }
    }

    /// How it's played.
    pub fn clip(self, child: bool) -> (Pose, ActionClip) {
        match self {
            TraitAct::TalkToSelf => (Pose::Use, ActionClip::new(None, if child { TALK_CHILD } else { TALK })),
            TraitAct::FreakOut => (Pose::Use, ActionClip::new(Some("a_trait_Neurotic_x"), &[])),
            TraitAct::FakePassOut => (Pose::Use, ActionClip::new(Some("a_motFail_exhausted_x"), &["a_sleeponFloor_breathe_x"])),
            TraitAct::Tada => (Pose::Use, ActionClip::new(Some("a_trait_naturalBornPerformer_x"), &[])),
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
            _ => {}
        }
    }
}

/// TRAIT_ACT=<act> (tests: "talk", "freak", "fake", "tada"): the selected Sim, given the trait
/// for it if they haven't it, does it.
fn act_test(mut done: Local<bool>, time: Res<Time>, mut sel: Query<(&mut Sim, &mut ActionQueue), With<crate::sim::Selected>>) {
    let Ok(want) = std::env::var("TRAIT_ACT") else { return };
    if *done || time.elapsed_secs() < 12.0 {
        return;
    }
    let Some(act) = TraitAct::ALL.into_iter().find(|a| a.name().to_ascii_lowercase().starts_with(&want.to_ascii_lowercase())) else { return };
    let Ok((mut sim, mut q)) = sel.single_mut() else { return };
    if !sim.traits.contains(&act.trait_()) {
        sim.traits.push(act.trait_());
    }
    q.0.clear();
    q.push_player(act.action(false));
    info!("trait act test: {} does {}", sim.first, act.name());
    *done = true;
}

#[cfg(test)]
mod tests {
    use super::*;

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
