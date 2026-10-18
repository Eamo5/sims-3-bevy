//! The base traits' own moodlets, as the game's `SimUpdate`, `BedSleep` and socials give them,
//! with their class tuning where it differs from the code:
//! - One with Nature (+20) for those who love the outdoors, and Plagued by Nature (-20) for those
//!   who hate them, while out of doors.
//! - A Loner Enjoying Solitude (+15) with nobody within 10 m (`BuffEnjoyingSolitude`), and among
//!   Too Many People (-15) with three or more in the room within 5 m who are neither family nor a
//!   romance (`BuffTooManyPeople`).
//! - A Coward Afraid of the Dark (-15) out of doors at home from 9 pm to 5 am (TraitTuning's).
//! - Lucky and Unlucky Sims waking Feeling Lucky (+5) or Feeling Unlucky (-5) for the day, four
//!   times in five, after four hours' sleep or more (`GrantBuffsAfterSleeping`); while it lasts,
//!   what goes wrong and right goes their way, or against them (`luck`).
//! - An Excitable Sim Excited (+15) a quarter of the time (TraitTuning's, not the code's half)
//!   socialising with a friend or a romance while it's going well (`AddExcitmentBuff`).
//! - A kiss with a Great Kisser: both of them a Great Kiss (+15; `OnKiss`).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::ActionQueue;
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::sim::Sim;

pub struct TraitMoodsPlugin;

impl Plugin for TraitMoodsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Socialised>().add_systems(Update, (trait_moods, social_moods).run_if(in_state(PlayMode::Live)));
    }
}

/// `kEnjoyingSolitudeAloneDistance`; `kTooManyPeopleDistance` and `kTooManyPeopleSimCount`.
const ALONE: f32 = 10.0;
const CROWD: f32 = 5.0;
const CROWD_COUNT: usize = 3;
/// `kCowardTraitTimeToGetAfraidOfDarkStart` and `End` (hours).
const DARK_FROM: f32 = 21.0;
const DARK_UNTIL: f32 = 5.0;
/// `kLuckyTraitHoursToGainBuff` (and Unlucky's), `kFeelingLuckySleepBuffChance` (and Unlucky's).
const LUCK_SLEEP_HOURS: f64 = 4.0;
const LUCK_CHANCE: f64 = 0.8;
/// `kExcitableTraitSimChanceOfBuff`.
const EXCITED_CHANCE: f64 = 0.25;

/// A social done (`interact`): the one who did it, to whom, whether a kiss, and whether it was
/// taken well.
#[derive(Message, Clone, Copy)]
pub struct Socialised {
    pub actor: Entity,
    pub target: Entity,
    pub kiss: bool,
    pub accepted: bool,
}

/// The odds of something going wrong (or right) for a Sim Feeling Lucky or Unlucky: TraitTuning's
/// multipliers, as tuned (the lucky never burn their food, set the stove alight or break things;
/// a raise is twice as likely, half as likely for the unlucky, who burn, set fires and break
/// things twice as often).
pub fn luck(moodlets: Option<&Moodlets>, lucky: f64, unlucky: f64) -> f64 {
    match moodlets {
        Some(m) if m.has(MoodletKind::FeelingLucky) => lucky,
        Some(m) if m.has(MoodletKind::FeelingUnlucky) => unlucky,
        _ => 1.0,
    }
}
pub const BURN_LUCK: (f64, f64) = (0.0, 2.0);
pub const COOKING_FIRE_LUCK: (f64, f64) = (0.0, 2.0);
pub const BREAK_LUCK: (f64, f64) = (0.0, 2.0);
pub const RAISE_LUCK: (f64, f64) = (2.0, 0.5);

#[allow(clippy::type_complexity)]
fn trait_moods(
    time: Res<Time>,
    mut last: Local<f32>,
    mut slept: Local<HashMap<Entity, f64>>,
    clock: Res<crate::clock::GameClock>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    genealogy: Option<Res<crate::family::Genealogy>>,
    everyone: Query<(Entity, &Transform, &Sim, &Visibility)>,
    mut sims: Query<
        (Entity, &Sim, &Transform, &Visibility, &mut Moodlets, &ActionQueue, Option<&crate::social::Relationships>, Has<crate::visit::OnLot>),
        (Without<crate::interact::OffLot>, Without<crate::rabbitholes::AtRabbitHole>, Without<crate::careers::AtWork>),
    >,
) {
    let now = time.elapsed_secs();
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    let where_is = |p: Vec3| {
        building.as_deref().map(|b| {
            let level = b.level_at(p.y + 0.3);
            (level, b.room_id(level, p))
        })
    };
    let others: Vec<(Entity, Vec3, &Sim)> = everyone.iter().filter(|(.., v)| **v != Visibility::Hidden).map(|(e, tf, s, _)| (e, tf.translation, s)).collect();
    let hour = clock.hour_f();
    let dark = !(DARK_UNTIL..DARK_FROM).contains(&hour);
    let mut rng = rand::rng();
    for (me, sim, tf, vis, mut moodlets, queue, rels, on_lot) in &mut sims {
        let has = |t: Trait| sim.traits.contains(&t);
        let here = *vis != Visibility::Hidden;
        let at = tf.translation;
        let outside = here && !crate::weather::sheltered(building.as_deref(), at);
        // Out of doors.
        set(&sim.first, &mut moodlets, MoodletKind::OneWithNature, outside && has(Trait::LovesTheOutdoors));
        set(&sim.first, &mut moodlets, MoodletKind::PlaguedByNature, outside && has(Trait::HatesTheOutdoors));
        // (At home, or another's: not out on a community lot.)
        set(&sim.first, &mut moodlets, MoodletKind::AfraidOfTheDark, outside && dark && !on_lot && has(Trait::Coward) && !has(Trait::LovesTheOutdoors));
        // Alone, or crowded.
        let (mut alone, mut crowded) = (false, false);
        if has(Trait::Loner) && here && !sim.age.is_little() {
            let near = |r: f32| others.iter().filter(move |(e, p, _)| *e != me && p.distance(at) <= r);
            alone = near(ALONE).next().is_none();
            let room = where_is(at);
            crowded = !alone
                && near(CROWD)
                    .filter(|(e, p, s)| {
                        !s.age.is_little()
                            && where_is(*p) == room
                            && !genealogy.as_deref().is_some_and(|g| g.kin(sim.id, s.id).is_some())
                            && !rels.is_some_and(|r| r.get(*e).is_romance())
                    })
                    .count()
                    >= CROWD_COUNT;
        }
        set(&sim.first, &mut moodlets, MoodletKind::EnjoyingSolitude, alone);
        set(&sim.first, &mut moodlets, MoodletKind::TooManyPeople, crowded);
        // The day's luck, waking from a good sleep.
        if has(Trait::Lucky) || has(Trait::Unlucky) {
            if crate::disturb::asleep(queue) {
                slept.entry(me).or_insert(clock.minutes);
            } else if let Some(from) = slept.remove(&me)
                && clock.minutes - from >= LUCK_SLEEP_HOURS * 60.0
                && rng.random_bool(LUCK_CHANCE)
            {
                let kind = if has(Trait::Lucky) { MoodletKind::FeelingLucky } else { MoodletKind::FeelingUnlucky };
                debug!("{} wakes {}", sim.first, kind.def().name);
                moodlets.add(kind, clock.minutes);
            }
        }
    }
    slept.retain(|e, _| sims.contains(*e));
}

/// A moodlet kept while its cause lasts, touched only when that changes (so it isn't redrawn
/// every second).
fn set(who: &str, m: &mut Mut<Moodlets>, kind: MoodletKind, on: bool) {
    if m.has(kind) != on {
        debug!("{who}: {} {}", kind.def().name, if on { "on" } else { "off" });
        m.set_while(kind, on);
    }
}

/// Socials' moodlets for the traits that feel them: an Excitable Sim's excitement, a Great
/// Kisser's kiss.
fn social_moods(clock: Res<crate::clock::GameClock>, mut done: MessageReader<Socialised>, mut sims: Query<(&Sim, &mut Moodlets, Option<&crate::social::Relationships>)>) {
    let mut rng = rand::rng();
    for s in done.read() {
        let Ok([(a, _, rels), (b, ..)]) = sims.get_many([s.actor, s.target]) else { continue };
        let rel = rels.map(|r| r.get(s.target)).unwrap_or_default();
        let excited = a.traits.contains(&Trait::Excitable)
            && rng.random_bool(EXCITED_CHANCE)
            && (rel.friendship >= crate::social::FRIEND_THRESHOLD || rel.is_romance())
            && rel.stc.commodity.positive();
        let great_kiss = s.kiss && s.accepted && (a.traits.contains(&Trait::GreatKisser) || b.traits.contains(&Trait::GreatKisser));
        let (a_name, b_name) = (a.first.clone(), b.first.clone());
        if excited && let Ok((_, mut m, _)) = sims.get_mut(s.actor) {
            debug!("{a_name} is excited");
            m.add(MoodletKind::Excited, clock.minutes);
        }
        if great_kiss {
            debug!("{a_name} and {b_name}: a great kiss");
            for e in [s.actor, s.target] {
                if let Ok((_, mut m, _)) = sims.get_mut(e) {
                    m.add(MoodletKind::GreatKiss, clock.minutes);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luck_turns_the_odds_only_while_felt() {
        let mut m = Moodlets::default();
        assert_eq!(luck(Some(&m), BURN_LUCK.0, BURN_LUCK.1), 1.0);
        m.add(MoodletKind::FeelingLucky, 0.0);
        assert_eq!(luck(Some(&m), BURN_LUCK.0, BURN_LUCK.1), 0.0, "the lucky never burn their food");
        assert_eq!(luck(Some(&m), RAISE_LUCK.0, RAISE_LUCK.1), 2.0);
        let mut u = Moodlets::default();
        u.add(MoodletKind::FeelingUnlucky, 0.0);
        assert_eq!(luck(Some(&u), BREAK_LUCK.0, BREAK_LUCK.1), 2.0);
        assert_eq!(luck(None, BREAK_LUCK.0, BREAK_LUCK.1), 1.0);
    }

    #[test]
    fn hot_heads_and_over_emotional_sims_feel_it_more() {
        use crate::life::felt;
        assert_eq!(felt(-10, &[Trait::HotHeaded]), -15);
        assert_eq!(felt(-15, &[Trait::HotHeaded]), -22, "truncated, as the game's (int)");
        assert_eq!(felt(20, &[Trait::HotHeaded]), 20, "only the bad ones");
        assert_eq!(felt(-10, &[Trait::HotHeaded, Trait::Neurotic]), -10, "a Neurotic frets instead");
        assert_eq!(felt(20, &[Trait::OverEmotional]), 25);
        assert_eq!(felt(-20, &[Trait::OverEmotional, Trait::HotHeaded]), -37);
    }
}
