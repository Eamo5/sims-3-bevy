//! Pets' young, as the Pets pack has them. A grown cat or dog of the household, played, can "Try
//! for Kittens" or "Try for Puppies" with one of the other sex of its kind (a dog and a little dog
//! together) that it's fond enough of (the pack's `Pet Woohoo`: its `ALTR` of 40), when there's a
//! pet house on the lot (`TestPetWooHoo`); a horse "Try For Foal" at a box stall (`HorseWooHoo`).
//! The other goes along with it if it's fonder still (`LLTR` 60); if not, it just sniffs them.
//! The two go to the pet house and in, the pack's `soc_woohoo` clips for both and the house's own
//! (it rocks); horses at the stall, `ah2ah_woohoo`. Then the game's chance (`kPregnancyChance` 60)
//! the mother's expecting, the conception sting.
//!
//! A pet's pregnancy lasts the game's two days (`kHoursOfPetPregnancy` 48): from twelve hours in
//! the household's told ("Looks like Pickles is pregnant! Prepare for kittens!", the
//! `sting_pet_pregnant`) and her company, fun (a horse's exercise) and hygiene are kept full
//! (`SetPregnantMotiveDecay`); her aging waits. Her time come, she lies down whining (her
//! species' `react_layDown_whine`; a mare stamps), and the young come: a litter of one to four
//! (`kChanceOfTwoOffspring` .8, three .3, four .1; a foal alone), as many as the household has
//! room for (six pets, ten in all), each a kitten, puppy or foal of its mother's or father's
//! kind and breed, its coat theirs, some of their traits, fond of its mother. The family's
//! delighted (New Kittens, New Puppies or New Foal, +80 for a day) and cheers. A foal's named
//! there and then ("Congratulations on a new foal!"); kittens and puppies are "Kitten 1",
//! "Puppy 2"... till a Sim names them ("Name Pet"). Pregnancies, and the unnamed, are kept in
//! saves.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{GameObject, Notifications, ObjectKind};
use crate::objanim::PlayingClip;
use crate::petcontrol::{PetCommand, PetOrder};
use crate::petneeds::{FUN, HYGIENE, PetDo, PetMotives, PetTask, SOCIAL, URGE};
use crate::pets::{HomePet, Pet, PetAnim, PetBonds, PetBusy, PetTraits};

pub struct PetLittersPlugin;

impl Plugin for PetLittersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Newborns>()
            .add_systems(Update, (litter_test, start_mating, mate, pregnancy, birth, newborns, naming).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(crate::AppState::InGame), |mut n: ResMut<Newborns>| n.0.clear());
    }
}

/// A pet the litters have in hand (mating, giving birth, a newborn lying by its mother): left
/// alone by its own wants and wanderings.
#[derive(Component)]
pub struct Held;

/// A young pet not yet named ("Kitten 1"): "Name Pet" on its menu.
#[derive(Component)]
pub struct Unnamed;

/// A pet to be named (asked for from its menu).
#[derive(Component)]
pub struct WantsName;

/// The father of a pregnancy, as the young take after him.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Sire {
    pub id: u64,
    pub kind: String,
    pub outfit: u64,
    #[serde(default)]
    pub coat: Option<u64>,
    #[serde(default)]
    pub traits: Vec<String>,
}

/// A pet expecting: how far along (game hours), by whom, whether it's known.
#[derive(Component, Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PetPregnant {
    pub hours: f32,
    #[serde(default)]
    pub sire: Option<Sire>,
    #[serde(default)]
    pub shown: bool,
}

/// The game's tuning (`PetPregnancy`, `PetHouse.WoohooPetHouse`, `BoxStall`): hours to the
/// birth and to it showing, the chance trying takes, the litter's chances.
pub const HOURS: f32 = 48.0;
pub const SHOW_AT: f32 = 12.0;
pub const CHANCE: f64 = 0.6;
const LITTER: [f64; 3] = [0.8, 0.3, 0.1];
/// The most pets a household can have, and Sims and pets together (`kMaxPetsPerHousehold`...).
pub const MAX_PETS: usize = 6;
pub const MAX_ALL: usize = 10;
/// The liking a pet asks of the other to try (`ALTR`), and the other's to go along (`LLTR`).
pub const LIKING_TO_TRY: f32 = 40.0;
pub const LIKING_TO_ACCEPT: f32 = 60.0;
/// How long the mother's distress lasts, and when the young come (seconds: `kDistressTotalLength`,
/// `kDistressLengthBeforeSpawnNewborns`); the young's liking of her (`kInitialChildParentRelLikingRange`).
const DISTRESS: f32 = 10.0;
const YOUNG_AT: f32 = 4.0;
const MOTHER_LIKING: std::ops::Range<f32> = 25.0..50.0;
/// How long newborns lie by their mother (game minutes).
const NEWBORN_REST: f64 = 60.0;

/// A pet's species letter (`c` cat, `d` dog, `l` little dog, `h` horse).
pub fn species(kind: &str) -> char {
    kind.chars().nth(1).unwrap_or('d')
}

/// Whether two pets' kinds can have young together: the same species, or a dog and a little dog.
pub fn kin(a: &str, b: &str) -> bool {
    let (x, y) = (species(a), species(b));
    x == y || (matches!(x, 'd' | 'l') && matches!(y, 'd' | 'l'))
}

/// A pet as trying for young asks of it.
#[derive(Clone, Copy, Debug)]
pub struct Mate<'a> {
    pub kind: &'a str,
    pub female: bool,
    pub elder: bool,
    pub pregnant: bool,
}

/// Whether two can try (`ShouldImpregnatePet`'s conditions): kin, of both sexes, grown, neither
/// expecting, the mother-to-be not old.
pub fn can_try(a: Mate, b: Mate) -> bool {
    let mother = if a.female { a } else { b };
    kin(a.kind, b.kind) && a.female != b.female && a.kind.starts_with('a') && b.kind.starts_with('a') && !a.pregnant && !b.pregnant && !mother.elder
}

/// What trying's called for a kind (`PetWoohooTextFunction`, `TryForFoal`).
pub fn try_label(kind: &str) -> &'static str {
    match species(kind) {
        'c' => "Try for Kittens",
        'h' => "Try For Foal",
        _ => "Try for Puppies",
    }
}

/// Where a kind tries: a pet house, a horse's box stall.
pub fn place_kind(kind: &str) -> ObjectKind {
    if species(kind) == 'h' { ObjectKind::BoxStall } else { ObjectKind::PetHouse }
}

/// How many young come (`GetNumForBirth`: a foal alone), before the household's room.
pub fn litter_size(kind: &str, roll: f64) -> usize {
    if species(kind) == 'h' {
        return 1;
    }
    1 + LITTER.iter().filter(|c| roll < **c).count()
}

/// How many of a litter the household has room for.
pub fn room_for(n: usize, pets: usize, sims: usize) -> usize {
    n.min(MAX_PETS.saturating_sub(pets)).min(MAX_ALL.saturating_sub(pets + sims))
}

/// The young of a kind (`cc` a kitten...).
pub fn young_kind(kind: &str) -> String {
    format!("c{}", species(kind))
}

/// The clips of trying, the pair's and the place's: its start, the act, its end (none for horses).
fn clips(x: &str, y: &str) -> [(String, String, Option<String>); 3] {
    if species(x) == 'h' {
        return [
            ("ah2ah_woohoo_start_x".into(), "ah2ah_woohoo_start_y".into(), None),
            ("ah2ah_woohoo_x".into(), "ah2ah_woohoo_y".into(), None),
            (String::new(), String::new(), None),
        ];
    }
    let b = format!("{x}2{y}_soc_woohoo");
    [
        (format!("{b}_start_x"), format!("{b}_start_y"), Some(format!("{b}_start_petHouse"))),
        (format!("{b}_x"), format!("{b}_y"), Some(format!("{b}_petHouse"))),
        (format!("{b}_stop_x"), format!("{b}_stop_y"), Some(format!("{b}_stop_petHouse"))),
    ]
}

/// Where the two go to try, and where they're put for it (the place's frame: the pet house's
/// routing slots and the one inside; for horses, side by side before the stall), facing.
fn spots(kind: &str, place: &Transform) -> [(Vec3, Quat); 4] {
    let at = |x: f32, z: f32, turn: f32| (place.transform_point(Vec3::new(x, 0.0, z)), place.rotation * Quat::from_rotation_y(turn));
    let pi = std::f32::consts::PI;
    if species(kind) == 'h' {
        // (Before the stall's gate, face to face.)
        [at(0.0, 3.4, -pi / 2.0), at(-1.25, 3.4, pi / 2.0), at(0.6, 3.4, -pi / 2.0), at(-0.6, 3.4, pi / 2.0)]
    } else {
        // (Before the house's door; then the one at the door, the other inside, both facing out:
        // the clips back them in.)
        [at(0.005, 1.7, pi), at(0.005, 0.99, pi), at(0.005, 0.99, 0.0), at(-0.047, 0.67, 0.0)]
    }
}

/// Trying: the other, the place, how far along, which clips (0 start, 1 the act, 2 the end),
/// how long into them. The one who asked leads; both have one.
#[derive(Component)]
pub struct Mating {
    other: Entity,
    place: Entity,
    lead: bool,
    stage: u8,
    t: f32,
    /// A test's: it takes.
    sure: bool,
}

/// Newborns to settle once they're out: their ids, when they're up, whether to name them now
/// (a foal: its mother's name, its sex).
#[derive(Resource, Default)]
struct Newborns(Vec<(u64, f64, Option<(String, bool)>, bool)>);

/// A mother giving birth: how far in, the young out yet.
#[derive(Component, Default)]
pub struct Birthing {
    t: f32,
    started: bool,
    out: bool,
}

/// Told to try (`petcontrol`): off to it together, or (the other not keen) a sniff.
#[allow(clippy::type_complexity)]
fn start_mating(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    orders: Query<(Entity, &Pet, &PetOrder, &Transform, &HomePet, Option<&PetBonds>)>,
    others: Query<(&Pet, &Transform, &HomePet, Option<&PetBonds>)>,
    places: Query<(Entity, &GameObject, &Transform), Without<crate::visit::LotObject>>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut notes: ResMut<Notifications>,
) {
    let Some(grid) = grid else { return };
    let mut rng = rand::rng();
    for (e, pet, order, tf, home, bonds) in &orders {
        let PetCommand::Mate(other) = order.0 else { continue };
        commands.entity(e).remove::<PetOrder>();
        let Ok((op, otf, ohome, obonds)) = others.get(other) else { continue };
        let keen = obonds.map_or(crate::pets::FAMILY_LIKING, |b| b.liking(home.id, true)) >= LIKING_TO_ACCEPT || std::env::var_os("PET_LITTER_TEST").is_some();
        let _ = bonds;
        if !keen {
            // (Not in the mood: just a sniff.)
            crate::petcompany::begin(&mut commands, e, &pet.kind, tf.translation.xz(), (crate::petcompany::Other::Pet(other), crate::petcompany::Greet::Sniff), otf.translation.xz(), &grid, clock.minutes, true);
            notes.push(format!("{} isn't in the mood for {}.", op.name, pet.name));
            continue;
        }
        // (The nearest place for it.)
        let want = place_kind(&pet.kind);
        let Some((place, _, ptf)) = places.iter().filter(|(_, o, _)| o.kind == want).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation))) else {
            continue;
        };
        let s = spots(&pet.kind, ptf);
        let mut ok = true;
        for (who, p, at) in [(e, pet, s[0].0), (other, op, s[1].0)] {
            let here = if who == e { tf.translation.xz() } else { otf.translation.xz() };
            match crate::petneeds::plan(p, here, PetDo::Go, Some(at.xz()), None, &grid, building.as_deref(), &mut rng) {
                Some(task) => {
                    commands.entity(who).remove::<(crate::petcompany::Greeting, crate::petcompany::Greeted)>().insert(task);
                }
                None => ok = false,
            }
        }
        if !ok {
            notes.push(format!("{} can't get there.", pet.name));
            commands.entity(e).remove::<PetTask>();
            commands.entity(other).remove::<PetTask>();
            continue;
        }
        let sure = std::env::var_os("PET_LITTER_TEST").is_some();
        info!("pet litters: {} and {} go to try ({:?})", pet.name, op.name, want);
        commands.entity(e).insert((Held, Mating { other, place, lead: true, stage: 0, t: 0.0, sure }));
        commands.entity(other).insert((Held, Mating { other: e, place, lead: false, stage: 0, t: 0.0, sure }));
        let _ = ohome;
    }
}

/// Trying, once both are there: the clips through, the place's with them; then the chance.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn mate(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib, pet_data): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>, Res<crate::pets::PetData>),
    mut pets: Query<(Entity, &Pet, &mut Mating, &mut Transform, &mut PetAnim, Has<PetTask>, &HomePet, Option<&mut PetBonds>, Option<&mut PetMotives>, Option<&PetTraits>, Has<PetPregnant>)>,
    places: Query<&Transform, (With<GameObject>, Without<Pet>)>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    mut notes: ResMut<Notifications>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let has = |n: &str| !n.is_empty() && data.0.clip_names.iter().any(|c| c == n);
    let mut rng = rand::rng();
    // (One left without the other: let go.)
    let lone: Vec<Entity> = pets.iter().filter(|p| pets.get(p.2.other).is_err()).map(|p| p.0).collect();
    for e in lone {
        commands.entity(e).remove::<(Mating, Held, PetBusy)>();
    }
    let leads: Vec<(Entity, Entity)> = pets.iter().filter(|p| p.2.lead && pets.contains(p.2.other)).map(|p| (p.0, p.2.other)).collect();
    for (x, y) in leads {
        let Ok([mut a, mut b]) = pets.get_many_mut([x, y]) else { continue };
        let place = a.2.place;
        let Ok(ptf) = places.get(place) else {
            commands.entity(x).remove::<(Mating, Held, PetBusy)>();
            commands.entity(y).remove::<(Mating, Held, PetBusy)>();
            continue;
        };
        let s = spots(&a.1.kind, ptf);
        let cl = clips(&a.1.kind, &b.1.kind);
        if a.2.stage == 0 {
            // On their way: once both are there, in place and at it.
            if a.5 || b.5 {
                continue;
            }
            if a.3.translation.xz().distance(s[0].0.xz()) > 0.8 || b.3.translation.xz().distance(s[1].0.xz()) > 0.8 {
                notes.push(format!("{} can't get there.", a.1.name));
                commands.entity(x).remove::<(Mating, Held)>();
                commands.entity(y).remove::<(Mating, Held)>();
                continue;
            }
            (a.3.translation, a.3.rotation) = (s[2].0.with_y(a.3.translation.y), s[2].1);
            (b.3.translation, b.3.rotation) = (s[3].0.with_y(b.3.translation.y), s[3].1);
            a.4.replay(cl[0].0.clone());
            b.4.replay(cl[0].1.clone());
            if let Some(h) = cl[0].2.clone().filter(|h| has(h)) {
                commands.entity(place).insert(PlayingClip { name: crate::pets::intern(h), time: 0.0, looping: false });
            }
            commands.entity(x).insert(PetBusy);
            commands.entity(y).insert(PetBusy);
            (a.2.stage, a.2.t) = (1, 0.0);
            info!("pet litters: {} and {} try", a.1.name, b.1.name);
            continue;
        }
        // The start, the act, the end.
        a.2.t += dt;
        let i = (a.2.stage - 1) as usize;
        let length = lib.get(&data, &cl[i.min(2)].0).map_or(if i == 1 { 10.0 } else { 3.0 }, |c| c.duration);
        if a.2.t < length {
            continue;
        }
        if let Some(j) = (i + 1..3).find(|j| has(&cl[*j].0)) {
            a.4.replay(cl[j].0.clone());
            b.4.replay(cl[j].1.clone());
            match cl[j].2.clone().filter(|h| has(h)) {
                Some(h) => {
                    commands.entity(place).insert(PlayingClip { name: crate::pets::intern(h), time: 0.0, looping: false });
                }
                None => {
                    commands.entity(place).try_remove::<PlayingClip>();
                }
            }
            (a.2.stage, a.2.t) = (j as u8 + 1, 0.0);
            continue;
        }
        // Done: back out, fonder of each other, and maybe expecting.
        commands.entity(place).try_remove::<PlayingClip>();
        a.3.translation = s[0].0.with_y(a.3.translation.y);
        b.3.translation = s[1].0.with_y(b.3.translation.y);
        let (aid, bid) = (a.6.id, b.6.id);
        for (p, other) in [(&mut a, bid), (&mut b, aid)] {
            if let Some(m) = p.8.as_mut() {
                m.add(SOCIAL, 40.0);
                m.add(FUN, 30.0);
            }
            if let Some(bonds) = p.7.as_mut() {
                bonds.add(other, true, 10.0);
            }
            p.4.play(crate::pets::idle_clip(&p.1.kind, false, &mut rng));
        }
        let sure = a.2.sure;
        let (mother, father) = if a.1.female { (&a, &b) } else { (&b, &a) };
        if !mother.10 && (sure || rng.random_bool(CHANCE)) {
            let outfit = pet_data.0.as_deref().and_then(|pd| pd.breeds.get(father.1.breed)).map_or(0, |b| b.outfit);
            let sire = Sire { id: father.6.id, kind: father.1.kind.clone(), outfit, coat: father.1.look.coat, traits: father.9.map(|t| t.0.clone()).unwrap_or_default() };
            commands.entity(mother.0).insert(PetPregnant { hours: 0.0, sire: Some(sire), shown: false });
            play.write(crate::sound::PlaySound::ui("sting_baby_conception"));
            info!("pet litters: {} is expecting", mother.1.name);
        }
        commands.entity(x).remove::<(Mating, Held, PetBusy)>();
        commands.entity(y).remove::<(Mating, Held, PetBusy)>();
    }
}

/// Expecting: known at twelve hours, her company, fun and hygiene kept full from then; at
/// forty-eight, her time.
#[allow(clippy::type_complexity)]
fn pregnancy(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    mut pets: Query<(Entity, &Pet, &mut PetPregnant, &mut PetMotives), (With<HomePet>, Without<Birthing>)>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    for (e, pet, mut p, mut m) in &mut pets {
        p.hours += delta.0 / 60.0;
        if p.hours >= SHOW_AT {
            if !p.shown {
                p.shown = true;
                play.write(crate::sound::PlaySound::ui("sting_pet_pregnant"));
                notes.push(found_out(&pet.name, &pet.kind));
            }
            let horse = species(&pet.kind) == 'h';
            for need in [SOCIAL, HYGIENE, if horse { URGE } else { FUN }] {
                if m.0[need] < 100.0 {
                    m.0[need] = 100.0;
                }
            }
        }
        if p.hours >= HOURS {
            commands.entity(e).insert(Birthing::default());
        }
    }
}

/// The game's words as a pregnancy's found out (`PetPregnancyFoundOut…TNS`).
pub fn found_out(name: &str, kind: &str) -> String {
    let what = match species(kind) {
        'c' => "Prepare for kittens!",
        'h' => "Prepare for a foal.",
        _ => "Prepare for puppies!",
    };
    format!("Looks like {name} is pregnant! {what}")
}

/// The game's words for a litter (`PetPregnancy…HadLitterTNS`, `…SingleKittenTNS`).
pub fn had_young(name: &str, kind: &str, n: usize) -> Option<String> {
    let (one, many) = match species(kind) {
        'c' => ("kitten", "kittens"),
        'h' => return None,
        _ => ("puppy", "puppies"),
    };
    Some(if n == 1 { format!("{name} just gave birth to a {one}! Don't forget to name it.") } else { format!("{name} just had a litter of {n} {many}! Don't forget to name them.") })
}

/// The clips of a birth: lying down whining (start, loop, stop), a mare stamping.
fn birth_clips(kind: &str) -> [String; 3] {
    if species(kind) == 'h' {
        return ["ah_react_snortStompFeet_x".into(), "ah_idle_stand_breathe_x".into(), "ah_idle_stand_perkUpHappy_x".into()];
    }
    let k = if kind == "cl" { "al" } else { kind };
    [format!("{k}_react_layDown_whine_start_x"), format!("{k}_react_layDown_whine_loop_x"), format!("{k}_react_layDown_whine_stop_x")]
}

/// A newborn of a mother (and father): the young of either's kind and breed, its coat theirs,
/// some of their traits, fond of her.
#[allow(clippy::too_many_arguments)]
fn newborn(pd: &s3bake::PetsBaked, mother: (&Pet, u64, &[String]), sire: Option<&Sire>, n: usize, at: Vec3, rng: &mut impl Rng) -> Option<crate::pets::SavedPet> {
    let (pet, mother_id, mother_traits) = mother;
    let from_sire = sire.filter(|_| rng.random_bool(0.5));
    let (kind, outfit, coat) = match from_sire {
        Some(s) => (s.kind.clone(), s.outfit, s.coat),
        None => (pet.kind.clone(), pd.breeds.get(pet.breed).map_or(0, |b| b.outfit), pet.look.coat),
    };
    let yk = young_kind(&kind);
    let breed_name = pd.breeds.iter().find(|b| b.outfit == outfit).map(|b| b.name.clone()).unwrap_or_default();
    // (Little puppies wear puppies' breeds: their bodies are the same.)
    let wears = if !pd.breeds.iter().any(|b| b.kind == yk) && yk == "cl" { "cd".to_string() } else { yk.clone() };
    let young = pd
        .breeds
        .iter()
        .find(|b| b.kind == wears && !breed_name.is_empty() && b.name == breed_name)
        .or_else(|| pd.breeds.iter().filter(|b| b.kind == wears && b.listed.is_some()).min_by_key(|b| b.listed))
        .or_else(|| pd.breeds.iter().find(|b| b.kind == wears))?;
    // (Some of each parent's traits, as many as the young have, the rest at random.)
    let most = crate::pettraits::max_traits(&yk);
    let allowed = crate::pettraits::for_kind(&yk);
    let mut traits: Vec<String> = Vec::new();
    for t in mother_traits.iter().chain(sire.map_or(&[][..], |s| &s.traits[..])) {
        let held: Vec<&str> = traits.iter().map(String::as_str).collect();
        if traits.len() < most && allowed.contains(&t.as_str()) && !held.contains(&t.as_str()) && crate::pettraits::compatible(&held, t) && rng.random_bool(0.5) {
            traits.push(t.clone());
        }
    }
    for t in crate::pettraits::random(&yk, rng) {
        let held: Vec<&str> = traits.iter().map(String::as_str).collect();
        if traits.len() < most && !held.contains(&t.as_str()) && crate::pettraits::compatible(&held, &t) {
            traits.push(t);
        }
    }
    let female = rng.random_bool(0.5);
    let name = match species(&yk) {
        'h' => crate::pets::random_pet_name(rng),
        'c' => format!("Kitten {n}"),
        _ => format!("Puppy {n}"),
    };
    Some(crate::pets::SavedPet {
        position: Some(at.to_array()),
        bonds: vec![(mother_id, rng.random_range(MOTHER_LIKING))],
        traits,
        female,
        coat: coat.or(Some(outfit)),
        age: Some(crate::petaging::PetAge { days: 0.0, elder: false }),
        unnamed: species(&yk) != 'h',
        ..crate::pets::SavedPet::new(rng.random(), name, &yk, young.outfit)
    })
}

/// Her time come: lying down whining (a mare stamping), the young out four seconds in, then up
/// again, the family delighted.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn birth(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    mut mothers: Query<(Entity, &Pet, &mut Birthing, &PetPregnant, &Transform, &mut PetAnim, &HomePet, Option<&PetTraits>)>,
    all_pets: Query<(), With<HomePet>>,
    mut household: Query<(Entity, &mut crate::life::Moodlets), (With<crate::sim::HouseholdMember>, Without<Pet>)>,
    pet_data: Res<crate::pets::PetData>,
    (mut pending, mut newborns): (ResMut<crate::pets::PendingPets>, ResMut<Newborns>),
    (mut notes, mut play): (ResMut<Notifications>, MessageWriter<crate::sound::PlaySound>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    let (pets_n, sims_n) = (all_pets.iter().count(), household.iter().count());
    for (e, pet, mut b, preg, tf, mut anim, home, traits) in &mut mothers {
        let cl = birth_clips(&pet.kind);
        if !b.started {
            b.started = true;
            commands.entity(e).remove::<(PetTask, crate::petcompany::Greeting)>().insert((PetBusy, Held));
            anim.replay(cl[0].clone());
            info!("pet litters: {}'s time has come", pet.name);
            continue;
        }
        b.t += dt;
        // (Down, then whining as she lies there; up again at the end.)
        let start = lib.get(&data, &cl[0]).map_or(2.0, |c| c.duration);
        let stop = lib.get(&data, &cl[2]).map_or(2.0, |c| c.duration);
        let phase = if b.t < start { 0 } else if b.t < DISTRESS.max(start) { 1 } else if b.t < DISTRESS.max(start) + stop { 2 } else { 3 };
        if phase < 3 && anim.clip != cl[phase] {
            anim.replay(cl[phase].clone());
        }
        if !b.out && b.t >= YOUNG_AT {
            b.out = true;
            let Some(pd) = pet_data.0.as_deref() else { continue };
            let n = room_for(litter_size(&pet.kind, rng.random()), pets_n, sims_n);
            let fwd = (tf.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
            let mother_traits = traits.map(|t| t.0.clone()).unwrap_or_default();
            for i in 0..n {
                // (Round her, a little in front.)
                let a = (i as f32 - (n as f32 - 1.0) / 2.0) * 0.8;
                let at = tf.translation + Quat::from_rotation_y(a) * fwd * if species(&pet.kind) == 'h' { 1.6 } else { 0.7 };
                if let Some(young) = newborn(pd, (pet, home.id, &mother_traits), preg.sire.as_ref(), i + 1, at, &mut rng) {
                    let name = (species(&pet.kind) == 'h').then(|| (pet.name.clone(), young.female));
                    newborns.0.push((young.id, clock.minutes + NEWBORN_REST, name, false));
                    pending.0.push(young);
                }
            }
            play.write(crate::sound::PlaySound::ui("sting_pet_baby_born"));
            if let Some(words) = had_young(&pet.name, &pet.kind, n) {
                notes.push(words);
            }
            let (moodlet, react) = match species(&pet.kind) {
                'c' => (crate::life::MoodletKind::NewKittens, "Cheer"),
                'h' => (crate::life::MoodletKind::NewFoal, "WeddingClap"),
                _ => (crate::life::MoodletKind::NewPuppies, "Cheer"),
            };
            for (s, mut m) in &mut household {
                m.add(moodlet, clock.minutes);
                let kind = *[react, "Cheer", "WeddingClap", "Surprise"].choose(&mut rng).unwrap_or(&react);
                commands.entity(s).insert(crate::reactions::Pending::new(&[kind], Some(tf.translation), clock.minutes + 30.0));
            }
            info!("pet litters: {} had {n}", pet.name);
        }
        if phase == 3 {
            anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
            commands.entity(e).remove::<(Birthing, PetPregnant, PetBusy, Held)>();
            info!("pet litters: {} is up again", pet.name);
        }
    }
}

/// Newborns out: lying by their mother a while (a foal's named there and then), then up and
/// about.
#[allow(clippy::type_complexity)]
fn newborns(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut newborns: ResMut<Newborns>,
    mut pets: Query<(Entity, &Pet, &HomePet, &mut PetAnim, Has<PetTask>)>,
    mut questions: ResMut<crate::dialog::Questions>,
) {
    if newborns.0.is_empty() {
        return;
    }
    let now = clock.minutes;
    let lie = |pet: &Pet| if species(&pet.kind) == 'h' { format!("{}_idle_stand_breathe_x", pet.kind) } else { format!("{}_idle_layDown_breathe_x", pet.kind) };
    for (e, pet, home, mut anim, tasked) in &mut pets {
        let Some(i) = newborns.0.iter().position(|n| n.0 == home.id) else { continue };
        let (_, until, name, seen) = newborns.0[i].clone();
        // (Nothing of its own meanwhile: what it was given before it was held let go.)
        if seen && tasked {
            commands.entity(e).remove::<PetTask>();
            anim.play(lie(pet));
        }
        if !seen {
            newborns.0[i].3 = true;
            commands.entity(e).remove::<(PetTask, crate::petcompany::Greeting, crate::petcompany::Greeted)>().insert(Held);
            info!("pet litters: {} lies by its mother", pet.name);
            anim.play(lie(pet));
            if let Some((mother, female)) = name {
                let (him, _) = if female { ("her", "she") } else { ("him", "he") };
                questions.ask(crate::dialog::Ask {
                    about: crate::dialog::Question::NamePet { pet: e, suggested: pet.name.clone() },
                    icon: String::new(),
                    heading: String::new(),
                    title: "Congratulations on a new foal!".into(),
                    text: format!("{mother} just gave birth to a foal! What would you like to name {him}?"),
                    answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
                });
            }
        }
        if now >= until {
            commands.entity(e).remove::<Held>();
            newborns.0.remove(i);
        }
    }
}

/// "Name Pet": the name asked for (`SetupNameForPet`), and given.
#[allow(clippy::type_complexity)]
fn naming(
    mut commands: Commands,
    asked: Query<Entity, Added<WantsName>>,
    mut answers: MessageReader<crate::dialog::Answered>,
    typed: Res<crate::dialog::Typed>,
    mut pets: Query<&mut Pet>,
    mut questions: ResMut<crate::dialog::Questions>,
) {
    let mut rng = rand::rng();
    for e in &asked {
        commands.entity(e).remove::<WantsName>();
        questions.ask(crate::dialog::Ask {
            about: crate::dialog::Question::NamePet { pet: e, suggested: crate::pets::random_pet_name(&mut rng) },
            icon: String::new(),
            heading: String::new(),
            title: "Name Pet".into(),
            text: String::new(),
            answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
        });
    }
    for a in answers.read() {
        let crate::dialog::Question::NamePet { pet, suggested } = &a.about else { continue };
        if a.answer != 0 {
            continue;
        }
        if let Ok(mut p) = pets.get_mut(*pet) {
            let name = typed.0.trim();
            p.name = if name.is_empty() { suggested.clone() } else { name.chars().take(20).collect() };
            commands.entity(*pet).try_remove::<Unnamed>();
            info!("pet litters: named {}", p.name);
        }
    }
}

/// `PET_LITTER_TEST=<pet>@<other>` (tests): ten seconds in, a pet house put down by them if
/// there's none on the lot (a box stall for horses), and then the one told to try with the other
/// (it takes); `<pet>:due`, that pet a moment from giving birth. The camera on the one.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn litter_test(
    mut commands: Commands,
    time: Res<Time>,
    pets: Query<(Entity, &Pet, &Transform), With<HomePet>>,
    places: Query<(&GameObject, &Transform), Without<crate::visit::LotObject>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut stage: Local<u8>,
) {
    let Ok(v) = std::env::var("PET_LITTER_TEST") else { return };
    if time.elapsed_secs() < 10.0 {
        return;
    }
    let (name, rest) = v.split_once('@').map(|(a, b)| (a, Some(b))).unwrap_or_else(|| (v.split(':').next().unwrap_or(&v), None));
    let Some((e, pet, tf)) = pets.iter().find(|p| p.1.name.eq_ignore_ascii_case(name)) else { return };
    if std::env::var_os("PET_CAM").is_none()
        && let Ok(mut c) = cams.single_mut()
    {
        c.look_at(tf.translation);
        c.distance = std::env::var("LITTER_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(6.0);
    }
    match (*stage, rest) {
        (0, None) => {
            *stage = 9;
            commands.entity(e).insert(PetPregnant { hours: HOURS - 0.02, sire: None, shown: true });
            info!("litter test: {} due", pet.name);
        }
        (0, Some(_)) => {
            *stage = 1;
            let want = place_kind(&pet.kind);
            if places.iter().any(|(o, _)| o.kind == want) {
                return;
            }
            let instance = if want == ObjectKind::BoxStall { "boxStallAllWestern" } else { "petHouseCheap" };
            if let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(instance)).map(|c| c.objd) {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                let p = tf.translation + Vec3::new(3.0, 0.0, 0.0);
                crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0);
                info!("litter test: a {instance} at {p}");
            }
        }
        (1, Some(other)) => {
            *stage = 2;
            let Some((o, ..)) = pets.iter().find(|p| p.1.name.eq_ignore_ascii_case(other)) else { return };
            commands.entity(e).insert(PetOrder(PetCommand::Mate(o)));
            info!("litter test: {} tries with {other}", pet.name);
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_can_try_for_young() {
        let m = |kind, female| Mate { kind, female, elder: false, pregnant: false };
        assert!(can_try(m("ac", true), m("ac", false)));
        assert!(can_try(m("ad", false), m("al", true)), "a dog and a little dog");
        assert!(!can_try(m("ac", true), m("ad", false)), "not a cat and a dog");
        assert!(!can_try(m("ac", true), m("ac", true)), "of both sexes");
        assert!(!can_try(m("cc", true), m("ac", false)), "grown");
        assert!(!can_try(Mate { elder: true, ..m("ah", true) }, m("ah", false)), "not an old mare");
        assert!(can_try(m("ah", true), Mate { elder: true, ..m("ah", false) }), "an old stallion's fine");
        assert!(!can_try(Mate { pregnant: true, ..m("ad", true) }, m("ad", false)));
        assert_eq!((try_label("al"), try_label("ac"), try_label("ah")), ("Try for Puppies", "Try for Kittens", "Try For Foal"));
        assert_eq!(place_kind("ah"), ObjectKind::BoxStall);
    }

    #[test]
    fn litters_come_as_the_game_has_them() {
        assert_eq!(litter_size("ah", 0.0), 1);
        assert_eq!(litter_size("ac", 0.05), 4);
        assert_eq!(litter_size("ad", 0.2), 3);
        assert_eq!(litter_size("al", 0.5), 2);
        assert_eq!(litter_size("ac", 0.9), 1);
        assert_eq!(room_for(4, 5, 2), 1, "six pets at most");
        assert_eq!(room_for(4, 2, 6), 2, "ten in all");
        assert_eq!(young_kind("al"), "cl");
        assert_eq!(clips("ad", "al")[1].2.as_deref(), Some("ad2al_soc_woohoo_petHouse"));
    }
}
