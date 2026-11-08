//! Pets' moodlets and mood, as the Pets pack has them: a household's pets carry the game's own
//! pet buffs, and their mood is what those come to, as a Sim's is.
//!
//! - Their needs' moodlets, by the game's `Motives` triggers for each species: a need in a
//!   trigger's band has its moodlet (Hungry from -36, Very Hungry from -61, Starving from -81;
//!   Has To Pee, Really Has To Pee; Sleepy, Tired, Exhausted; Lonely, Desolate; Grungy and Smelly
//!   for the grown; a cat's or dog's Strained and Stressed, Entertained and Having a Blast as
//!   their fun's high; a cat's Itching To Scratch and Claws Out, a dog's Pent Up and Going Nuts, a
//!   horse's Restless and Frustrated, Thirsty and Dehydrated), each the game's worth (Starving
//!   -80, Lonely -15, Having a Blast +40...). Squeaky Clean comes as a grown pet's washed right
//!   up and goes as it's no longer clean (below 71), Purrfectly Manicured the same for a cat's
//!   claws; Well Rested at full energy, till it's spent.
//! - What's done to them: Loved (+25, an hour) for being petted, hugged, smooched, its belly
//!   rubbed or brushed; Brushed (+15, six hours); Praised and Scolded; Hygenic Hooves for a
//!   horse's hooves cleaned; Good Dog for a trick done on command; Nothing But Air for a clean
//!   jump from Jumping 5 (`kLevelForNothingButAir`); Tastes Like Chicken for a bird caught and
//!   eaten; Pregnant (+20) while it shows; New Kittens, New Puppies or New Foal (+80, a day) for
//!   the mother and father; Training Fatigue while it's worn out from learning.
//! - What they do to Sims: a horse's kick leaves them Kicked (-15, a day), a cat's mark Marked
//!   (+10), a cat's attack Shredded Dignity (-20).
//!
//! The moodlets show on the HUD's moodlet panel and the mood meter with the pet played, its mood
//! rings its face on the skewer, and a horse in a foul mood throws its rider (the game's
//! `MoodValue`). Timed ones are kept in saves.

use bevy::prelude::*;

use crate::PlayMode;
use crate::life::{Mood, MoodletKind as K, Moodlets};
use crate::petneeds::{BLADDER, ENERGY, FUN, HUNGER, HYGIENE, PetMotives, SOCIAL, THIRST, URGE};
use crate::pets::{HomePet, Pet};

pub struct PetMoodsPlugin;

impl Plugin for PetMoodsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach, needs, states, moods_test).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// A need's moodlet bands for a kind of pet (the game's `BuffTrigger`s: the need's whole value
/// from and to, and what it brings), worst first.
pub fn bands(kind: &str, need: usize) -> &'static [(f32, f32, K)] {
    let species = kind.chars().nth(1).unwrap_or('c');
    let grown = !kind.starts_with('c');
    match need {
        HUNGER => &[(-100.0, -81.0, K::StarvingPet), (-80.0, -61.0, K::VeryHungryPet), (-60.0, -36.0, K::HungryPet)],
        BLADDER => &[(-100.0, -91.0, K::ReallyHasToPeePet), (-90.0, -61.0, K::HasToPeePet)],
        ENERGY => &[(-100.0, -100.0, K::ExhaustedPet), (-99.0, -80.0, K::TiredPet), (-79.0, -60.0, K::SleepyPet)],
        SOCIAL => &[(-100.0, -89.0, K::DesolatePet), (-88.0, -61.0, K::LonelyPet)],
        // (A horse has no Fun: its Exercise.)
        FUN if species != 'h' => &[(-100.0, -51.0, K::StressedPet), (-50.0, -21.0, K::StrainedPet), (10.0, 49.0, K::Amused), (50.0, 100.0, K::Delighted)],
        HYGIENE if grown => &[(-100.0, -81.0, K::SmellyPet), (-80.0, -61.0, K::GrungyPet)],
        URGE => match species {
            'c' => &[(-100.0, -99.0, K::ClawsOut), (-98.0, -61.0, K::ItchingToScratch)],
            'h' => &[(-100.0, -99.0, K::Frustrated), (-98.0, -61.0, K::Restless)],
            _ => &[(-100.0, -99.0, K::GoingNuts), (-98.0, -61.0, K::PentUp)],
        },
        THIRST if species == 'h' && grown => &[(-100.0, -99.0, K::Dehydrated), (-98.0, -61.0, K::ThirstyHorse)],
        _ => &[],
    }
}

/// The moodlets that come and stay a while once a need's full (the game's non-perma triggers):
/// the need, the value it comes at, the value it goes at or under, what it is.
pub fn sticky(kind: &str) -> Vec<(usize, f32, f32, K)> {
    let mut out = vec![(ENERGY, 100.0, 0.0, K::WellRestedPet)];
    if !kind.starts_with('c') {
        out.push((HYGIENE, 99.0, 70.0, K::SqueakyCleanPet));
    }
    if kind.ends_with('c') {
        out.push((URGE, 99.0, 70.0, K::PurrfectlyManicured));
    }
    out
}

/// A pet's needs' moodlets set by its needs (each need's whole value, as the game reads it).
pub fn need_moodlets(kind: &str, m: &PetMotives, ml: &mut Moodlets) {
    for (need, value) in m.0.iter().enumerate() {
        let v = value.trunc();
        for (lo, hi, k) in bands(kind, need) {
            ml.set_while(*k, (*lo..=*hi).contains(&v));
        }
    }
    for (need, on, off, k) in sticky(kind) {
        let v = m.0[need].trunc();
        if v >= on {
            ml.set_while(k, true);
        } else if v <= off {
            ml.set_while(k, false);
        }
    }
}

/// What a Sim's social with a pet leaves it feeling (`SocialCallback`'s: petted, hugged, smooched,
/// its belly rubbed, brushed; praised or scolded; a horse's hooves cleaned).
pub fn from_social(name: &str) -> &'static [K] {
    match name {
        "Pet" | "Hug" | "Smoochie" | "Rub Belly" => &[K::LovedPet],
        "Brush" | "Brush Horse" => &[K::LovedPet, K::Brushed],
        "Praise" => &[K::Praise],
        "Scold" => &[K::Scolded],
        "Clean Hooves" => &[K::HygenicHooves],
        _ => &[],
    }
}

/// What a pet's greeting leaves the Sim it's for feeling: a horse's kick, a cat's mark or attack.
pub fn from_greeting(how: crate::petcompany::Greet) -> Option<K> {
    use crate::petcompany::Greet;
    match how {
        Greet::Kick => Some(K::Kicked),
        Greet::MarkSim => Some(K::Marked),
        Greet::AttackSim => Some(K::ShreddedDignity),
        _ => None,
    }
}

/// The moodlet for a litter, by the mother's kind (`PetPregnancy`: kittens, puppies, a foal).
pub fn new_young(kind: &str) -> K {
    match kind.chars().nth(1) {
        Some('c') => K::NewKittensPet,
        Some('h') => K::NewFoalPet,
        _ => K::NewPuppiesPet,
    }
}

/// The Jumping a horse needs for a clean jump to be Nothing But Air (`kLevelForNothingButAir`).
pub const NOTHING_BUT_AIR_LEVEL: u32 = 5;

/// A moodlet added to someone (a pet or a Sim) when it's next seen to.
pub fn give(commands: &mut Commands, e: Entity, kind: K, now: f64) {
    commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| {
        if let Some(mut m) = w.get_mut::<Moodlets>() {
            m.add(kind, now);
        }
    });
}

/// The timed moodlets a pet keeps in a save: the game's buff each is, and when it ends.
pub fn saved(ml: &Moodlets) -> Vec<(String, f64)> {
    ml.0.iter().filter(|m| m.until.is_finite()).map(|m| (m.kind.buff().0.to_string(), m.until)).collect()
}

/// A save's back: its timed moodlets.
pub fn restored(saved: &[(String, f64)]) -> Moodlets {
    let mut ml = Moodlets::default();
    for (hex, until) in saved {
        if let Some(kind) = K::from_buff(hex) {
            ml.0.push(crate::life::Moodlet { kind, value: kind.def().value, until: *until, felt: true });
        }
    }
    ml
}

/// The household's pets have moodlets and a mood, as Sims do.
fn attach(mut commands: Commands, pets: Query<(Entity, Has<Moodlets>), (With<HomePet>, Without<Mood>)>) {
    for (e, has) in &pets {
        if has {
            commands.entity(e).insert(Mood(10.0));
        } else {
            commands.entity(e).insert((Moodlets::default(), Mood(10.0)));
        }
    }
}

fn needs(mut pets: Query<(&Pet, &PetMotives, &mut Moodlets), (With<HomePet>, Changed<PetMotives>)>) {
    for (pet, m, mut ml) in &mut pets {
        need_moodlets(&pet.kind, m, &mut ml);
    }
}

/// Moodlets that last as a state does: pregnant (once it shows), worn out from training.
#[allow(clippy::type_complexity)]
fn states(mut pets: Query<(&mut Moodlets, Option<&crate::petlitters::PetPregnant>, Option<&crate::dogtricks::DogTricks>, Option<&crate::horsejumps::HorseSkills>), With<HomePet>>) {
    for (mut ml, preg, tricks, horse) in &mut pets {
        let pregnant = preg.is_some_and(|p| p.shown);
        if pregnant != ml.has(K::PregnantPet) {
            ml.set_while(K::PregnantPet, pregnant);
        }
        let tired = tricks.is_some_and(|t| t.worn_out) || horse.is_some_and(|h| h.worn_out);
        if tired != ml.has(K::PetSkillFatigue) {
            ml.set_while(K::PetSkillFatigue, tired);
        }
    }
}

/// `PET_MOODS=<pet>` (tests): ten seconds in, that pet hungry, lonely and having a blast, just
/// petted and brushed.
fn moods_test(time: Res<Time>, clock: Res<crate::clock::GameClock>, mut pets: Query<(&Pet, &mut PetMotives, &mut Moodlets), With<HomePet>>, mut done: Local<bool>) {
    let Ok(name) = std::env::var("PET_MOODS") else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    let Some((pet, mut m, mut ml)) = pets.iter_mut().find(|p| p.0.name.eq_ignore_ascii_case(&name)) else { return };
    *done = true;
    m.0[HUNGER] = -70.0;
    m.0[SOCIAL] = -70.0;
    m.0[FUN] = 80.0;
    ml.add(K::LovedPet, clock.minutes);
    ml.add(K::Brushed, clock.minutes);
    info!("pet moods test: {} hungry, lonely, delighted, loved and brushed", pet.name);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motives(v: f32) -> PetMotives {
        PetMotives([v; 8])
    }

    #[test]
    fn needs_bring_the_games_pet_moodlets() {
        let mut ml = Moodlets::default();
        let mut m = motives(0.0);
        m.0[HUNGER] = -70.0;
        m.0[URGE] = -99.5;
        m.0[FUN] = 60.0;
        need_moodlets("ac", &m, &mut ml);
        assert!(ml.has(K::VeryHungryPet) && !ml.has(K::HungryPet) && !ml.has(K::StarvingPet));
        assert!(ml.has(K::ClawsOut), "-99.5 reads as -99");
        assert!(ml.has(K::Delighted) && !ml.has(K::Amused));
        m.0[HUNGER] = 20.0;
        m.0[FUN] = 20.0;
        need_moodlets("ac", &m, &mut ml);
        assert!(!ml.has(K::VeryHungryPet) && ml.has(K::Amused) && !ml.has(K::Delighted));
        // (A horse has no fun, but thirst; a dog chews.)
        let mut h = Moodlets::default();
        let mut hm = motives(0.0);
        hm.0[FUN] = -80.0;
        hm.0[THIRST] = -70.0;
        hm.0[URGE] = -70.0;
        need_moodlets("ah", &hm, &mut h);
        assert!(!h.has(K::StressedPet) && h.has(K::ThirstyHorse) && h.has(K::Restless));
        let mut d = Moodlets::default();
        need_moodlets("al", &hm, &mut d);
        assert!(d.has(K::StressedPet) && d.has(K::PentUp) && !d.has(K::ThirstyHorse));
        // (The young aren't grungy.)
        let mut k = Moodlets::default();
        let mut km = motives(0.0);
        km.0[HYGIENE] = -90.0;
        need_moodlets("cc", &km, &mut k);
        assert!(!k.has(K::SmellyPet));
        need_moodlets("ac", &km, &mut k);
        assert!(k.has(K::SmellyPet));
    }

    #[test]
    fn some_stay_a_while_once_full() {
        let mut ml = Moodlets::default();
        let mut m = motives(50.0);
        m.0[HYGIENE] = 100.0;
        need_moodlets("ad", &m, &mut ml);
        assert!(ml.has(K::SqueakyCleanPet));
        m.0[HYGIENE] = 80.0;
        need_moodlets("ad", &m, &mut ml);
        assert!(ml.has(K::SqueakyCleanPet), "still clean");
        m.0[HYGIENE] = 70.0;
        need_moodlets("ad", &m, &mut ml);
        assert!(!ml.has(K::SqueakyCleanPet));
        assert!(!ml.has(K::PurrfectlyManicured));
    }

    #[test]
    fn whats_done_to_pets_and_by_them() {
        assert_eq!(from_social("Brush Horse"), &[K::LovedPet, K::Brushed]);
        assert_eq!(from_social("Scold"), &[K::Scolded]);
        assert!(from_social("Talk To Pet").is_empty());
        assert_eq!(from_greeting(crate::petcompany::Greet::Kick), Some(K::Kicked));
        assert_eq!(new_young("cl"), K::NewPuppiesPet);
        assert_eq!((K::Delighted.def().value, K::StarvingPet.def().value, K::LovedPet.def().hours), (40, -80, 1.0));
        let mut ml = Moodlets::default();
        ml.add(K::Brushed, 100.0);
        ml.set_while(K::HungryPet, true);
        let kept = saved(&ml);
        assert_eq!(kept, vec![("Brushed".to_string(), 460.0)]);
        assert!(restored(&kept).has(K::Brushed));
    }
}
