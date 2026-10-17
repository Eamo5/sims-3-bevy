//! The high chair, as the game's `HighChair`: a grown-up puts a toddler in it ("Put Lucy in High
//! Chair", the game's `PutCarriedChildInChair`: the toddler toddles over and is lifted in), gives
//! them baby food there (`GiveBabyFood`: the bowl served), which they eat sat in it half an hour,
//! their hunger filled (`ToddlerEatFood`, `kEatFoodTime`) — or, not hungry
//! (`kTossFoodHungerValue`), toss — and takes them out again (`Hold`). Left in it they sit on,
//! fussing once they're hungry.

use bevy::prelude::*;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, InteractionDef, ObjectKind, Phase, Special, interactions_for};
use crate::sim::{Age, Sim};

pub struct HighChairPlugin;

impl Plugin for HighChairPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, high_chairs.before(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
    }
}

/// How the game names them ("Put Lucy in High Chair", "Give Baby Food", "Take Out").
pub const PUT_IN: &str = "Put";
pub const IN_HIGH_CHAIR: &str = "in High Chair";
/// `kEatFoodTime` (game minutes), `kTossFoodHungerValue`.
const EAT_MINUTES: f32 = 30.0;
const TOSS_HUNGER: f32 = 50.0;
/// Hunger from which a toddler sat there fusses.
const FUSS_HUNGER: f32 = -40.0;

/// The clips: the toddler lifted in and sat there, eating (and tossing it), and the grown-up's.
const LIFTED_IN: &str = "a2p_highChair_putInChair_y";
const PUT_IN_CLIP: &str = "a2p_highChair_putInChair_x";
pub const SITTING: &[&str] = &["p2o_highChair_idle1_normal_y", "p2o_highChair_idle2_normal_y"];
const FUSSING: &[&str] = &["p2o_highChair_idle3_cry_y", "p2o_highChair_idle4_cry_y"];
const EATING: &[&str] = &["p2o_highChair_serveFood_eat_loop1_y", "p2o_highChair_serveFood_eat_loop2_y", "p2o_highChair_serveFood_eat_loop3_y"];
const DONE_EATING: &str = "p2o_highChair_serveFood_eat_stop_y";
const TOSSED: &str = "p2o_highChair_serveFood_eat_throwFood_y";
const WAITING: &[&str] = &["a_idle_neutral_loop_"];

/// Who may do what at a high chair: toddlers sit and eat in it, teens and up see to them.
pub fn suits(d: &InteractionDef, sim: &Sim) -> bool {
    match d.special {
        Special::HighChairSit => sim.age == Age::Toddler,
        _ => !sim.age.is_little() && sim.age != Age::Child,
    }
}

/// A toddler sat in a high chair (their crying's the chair's to show).
#[derive(Component, Debug)]
pub struct InHighChair;

/// A toddler eating in the high chair: since when, and when their hunger was last filled (game
/// minutes).
#[derive(Component, Debug)]
pub struct EatingInChair {
    since: f64,
    last: f64,
}

/// A grown-up off to put a toddler in a high chair (who, and whether they've been called over).
#[derive(Component, Debug)]
pub struct PuttingIn {
    pub toddler: Entity,
    pub asked: bool,
}

/// What a Sim's doing at a high chair now: the chair, what, and how long (game minutes).
fn at_chair(queue: &ActionQueue, chairs: &Query<(&GameObject, &Transform), Without<Sim>>) -> Option<(Entity, Special, f32)> {
    let a = queue.0.front().filter(|a| !a.cancel)?;
    let (ActionKind::Object { target, def }, Phase::Running(t)) = (&a.kind, a.phase) else { return None };
    let (o, _) = chairs.get(*target).ok().filter(|c| c.0.kind == ObjectKind::HighChair)?;
    interactions_for(o.kind).get(*def).map(|d| (*target, d.special, t))
}

/// The high chairs' goings-on: toddlers called over and lifted in, fed, eating till they're
/// done (back to sitting there), fussing when hungry, and taken out.
#[allow(clippy::type_complexity)]
fn high_chairs(
    mut commands: Commands,
    mut handled: Local<std::collections::HashSet<(Entity, u8)>>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut crate::sim::Motives, Option<&mut PuttingIn>, Option<&ActionClip>, Option<&mut EatingInChair>), Without<GameObject>>,
    chairs: Query<(&GameObject, &Transform), Without<Sim>>,
    clock: Res<crate::clock::GameClock>,
    seated: Query<Entity, With<InHighChair>>,
) {
    let now = clock.minutes;
    let def_of = |special: Special| interactions_for(ObjectKind::HighChair).iter().position(|d| d.special == special);
    let mut calls: Vec<(Entity, Entity, Entity)> = Vec::new();
    let mut fed: Vec<Entity> = Vec::new();
    let mut taken_out: Vec<Entity> = Vec::new();
    let mut sat: Vec<(Entity, Entity, f32)> = Vec::new();
    let mut waiting: Vec<(Entity, Entity)> = Vec::new();
    // (Each grown-up's step taken once: the serving, the taking out, the lifting in.)
    let mut still: std::collections::HashSet<(Entity, u8)> = std::collections::HashSet::new();
    let mut once = |me: Entity, step: u8, handled: &mut std::collections::HashSet<(Entity, u8)>| {
        still.insert((me, step));
        handled.insert((me, step))
    };
    for (me, _, queue, _, putting, ..) in &sims {
        let Some((chair, special, t)) = at_chair(&queue, &chairs) else { continue };
        match special {
            Special::PutInHighChair => {
                if let Some(p) = putting {
                    if p.asked {
                        waiting.push((me, chair));
                    } else {
                        calls.push((me, p.toddler, chair));
                    }
                }
            }
            // (Fed as the bowl's served: at once.)
            Special::GiveBabyFood => {
                if once(me, 1, &mut handled) {
                    fed.push(chair);
                }
            }
            Special::TakeOutOfHighChair => {
                if once(me, 2, &mut handled) {
                    taken_out.push(chair);
                }
            }
            Special::HighChairSit => sat.push((me, chair, t)),
            _ => {}
        }
    }
    // Called over: the toddler goes and is lifted in.
    for (grown, toddler, chair) in calls {
        if let Ok((.., Some(mut p), _, _)) = sims.get_mut(grown) {
            p.asked = true;
        }
        if let (Ok((_, _, mut q, ..)), Some(def)) = (sims.get_mut(toddler), def_of(Special::HighChairSit)) {
            q.0.clear();
            q.push_player(Action::new(format!("Sit {IN_HIGH_CHAIR}"), ActionKind::Object { target: chair, def }, false));
        }
    }
    // (The grown-up waits by the chair till the toddler's in it, lifting them in as they come,
    // and done once they're sat.)
    for (grown, chair) in waiting {
        let arrived = sat.iter().find(|(_, c, _)| *c == chair);
        match arrived {
            Some((_, _, t)) if *t > 3.0 => {
                if let Ok((_, _, mut q, ..)) = sims.get_mut(grown)
                    && let Some(a) = q.0.front_mut()
                    && let ActionKind::Object { def, .. } = a.kind
                    && let Some(d) = interactions_for(ObjectKind::HighChair).get(def)
                {
                    a.phase = Phase::Running(d.minutes);
                }
                commands.entity(grown).remove::<PuttingIn>();
            }
            Some(_) => {
                if once(grown, 3, &mut handled) {
                    commands.entity(grown).insert(ActionClip::new(Some(PUT_IN_CLIP), WAITING));
                }
            }
            _ => {}
        }
    }
    handled.retain(|k| still.contains(k));
    // (Who's sat in one: marked so, and unmarked once they're out.)
    for e in &seated {
        if !sat.iter().any(|s| s.0 == e) {
            commands.entity(e).remove::<(InHighChair, EatingInChair)>();
        }
    }
    for (e, ..) in &sat {
        if !seated.contains(*e) {
            commands.entity(*e).insert(InHighChair);
        }
    }
    // The toddlers sat there: eating once fed, sitting on after, fussing when hungry; out when
    // taken.
    for (toddler, chair, t) in sat {
        let Ok((_, sim, mut q, mut motives, _, clip, eating)) = sims.get_mut(toddler) else { continue };
        let hunger = motives.0[crate::sim::HUNGER];
        let Some(a) = q.0.front_mut() else { continue };
        if taken_out.contains(&chair) {
            a.cancel = true;
            commands.entity(toddler).remove::<EatingInChair>();
            continue;
        }
        if t < 0.5 && clip.is_none_or(|c| c.start != Some(LIFTED_IN) && c.start != Some(DONE_EATING)) {
            commands.entity(toddler).insert(ActionClip::new(Some(LIFTED_IN), SITTING));
        }
        // Eating: hunger filled to the full over the half hour (what's left of it spread over
        // what's left of the time, as the game's), then sat on.
        if let Some(mut e) = eating {
            let (dt, gone) = ((now - e.last) as f32, (now - e.since) as f32);
            e.last = now;
            let h = &mut motives.0[crate::sim::HUNGER];
            *h += (100.0 - *h) * (dt / (EAT_MINUTES - gone).max(1.0)).min(1.0);
            if gone >= EAT_MINUTES {
                commands.entity(toddler).remove::<EatingInChair>().insert(ActionClip::new(Some(DONE_EATING), SITTING));
                a.label = format!("Sit {IN_HIGH_CHAIR}");
            }
            continue;
        }
        if fed.contains(&chair) {
            // (Not hungry: tossed. Hungry: eaten, the half hour through.)
            let tossed = hunger > TOSS_HUNGER;
            if !tossed {
                commands.entity(toddler).insert(EatingInChair { since: now, last: now });
                a.label = "Eat".to_string();
            }
            info!("{} {} the baby food", sim.first, if tossed { "tossed" } else { "eats" });
            // (Straight into it: the bowl's serving is the grown-up's part.)
            commands.entity(toddler).insert(if tossed { ActionClip::new(Some(TOSSED), SITTING) } else { ActionClip::new(None, EATING) });
            continue;
        }
        // (Fussing once they're hungry; content otherwise.)
        let want: &'static [&'static str] = if hunger < FUSS_HUNGER { FUSSING } else { SITTING };
        if t > 1.0 && clip.is_some_and(|c| c.loops != want && c.loops != EATING && c.start.is_none_or(|s| s != LIFTED_IN && s != DONE_EATING)) {
            commands.entity(toddler).insert(ActionClip::new(None, want));
        }
    }
}

/// The toddlers a Sim might put in the high chair: their family's, not already in one.
pub fn sittable<'a>(toddlers: impl Iterator<Item = (Entity, &'a Sim, bool)>) -> Vec<(Entity, String)> {
    toddlers.filter(|(_, s, family)| s.age == Age::Toddler && *family).map(|(e, s, _)| (e, s.first.clone())).collect()
}
