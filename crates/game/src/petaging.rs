//! Pets growing up and growing old, as the Pets pack's `AgingManager` has them. Each of the
//! household's pets lives through its stages by the game's tuning for its species (at the
//! normal lifespan, seven days to an aging year: a kitten or puppy a week, a foal not five days;
//! a dog grown twenty-five days, a cat twenty-eight, a horse thirty-five; then old, a dog at
//! least fourteen days, a cat or horse seventeen), stretched or shortened by the lifespan
//! picked, like the Sims'. A pet is born into its stage a little way through it
//! (`kPercentCasOffset`), and the day before it grows up the household hears of it.
//!
//! Growing up plays out as the game's `AgeUp` does: the young one's half of the age transition
//! (`ageTransition_part1`), the pack's sting for it, and the grown one's half (`part2`), now a
//! grown cat, dog or horse of its breed (its coat kept), with its traits; one neglected as it grew
//! (its mood, energy, hunger, company, fun, its claws or its urge to chew, or for a horse its
//! exercise, below the game's thresholds, weighed as `CheckForAdditionOfNegativePetTrait` weighs
//! them) comes out of it with a bad trait besides. Growing old has the elder's half of the
//! transition (`ec_`, `ed_`, `eh_`, `el_`) and from then on the old one walks as the old do (the
//! pack's `elderWalk`), its age shown as an elder's.
//!
//! An old pet past its least span has the game's daily chance of passing away (fourteen in a
//! hundred at the normal lifespan): it lies down, the Grim Reaper appears beside it and plays
//! with it a last time (the pack's `death_friendly` socials, his and the pet's halves), it fades
//! away and he raises its tombstone (the pack's feline, canine or equine one) before he goes; the
//! household mourns. Pets' tombstones stay on the lot, and in saves.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{GameObject, Notifications};
use crate::pets::{HomePet, Pet, PetAnim, PetBusy};

pub struct PetAgingPlugin;

impl Plugin for PetAgingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AgedUp>()
            .add_systems(Update, (initialize, sync_elder, age_test, daily, transition, aged_up, reap).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(crate::AppState::InGame), |mut a: ResMut<AgedUp>| a.0.clear());
    }
}

/// How far through its life stage a pet is (days at the normal lifespan), and whether it's old.
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PetAge {
    pub days: f32,
    #[serde(default)]
    pub elder: bool,
}

/// The game's days to an aging year at the normal lifespan (`kNormalSimDaysPerAgingYear`), and
/// how far into its stage a pet starts (`kPercentCasOffset`).
const DAYS_PER_YEAR: f32 = 7.0;
const START_OFFSET: f32 = 0.28;

/// A stage's length in aging years (`AgingManager`'s tuning): the young (`kAgingStageLengthKitten`,
/// `Puppy`, `Foal`), the grown (`…CatAdult`, `DogAdult`, `HorseAdult`), and the least an old one
/// lives (`…ElderMinimum`).
pub fn stage_years(kind: &str, elder: bool) -> f32 {
    let species = kind.chars().nth(1).unwrap_or('d');
    match (kind.starts_with('c'), elder, species) {
        (true, _, 'h') => 0.67,
        (true, ..) => 1.0,
        (false, false, 'c') => 4.0,
        (false, false, 'h') => 5.0,
        (false, false, _) => 3.571,
        (false, true, 'c' | 'h') => 2.428,
        (false, true, _) => 2.0,
    }
}

/// A stage's length in days at the normal lifespan.
pub fn stage_days(kind: &str, elder: bool) -> f32 {
    stage_years(kind, elder) * DAYS_PER_YEAR
}

impl PetAge {
    /// A pet's age as it comes into the household: a little way through its stage.
    pub fn start(kind: &str, elder: bool, rng: &mut impl Rng) -> Self {
        Self { days: rng.random_range(0.0..stage_days(kind, elder) * START_OFFSET).floor(), elder }
    }
}

/// What a young one grows into (`cc` a kitten, `ac` a cat).
pub fn grown_kind(kind: &str) -> String {
    format!("a{}", kind.get(1..).unwrap_or("d"))
}

/// The pack's sting for growing up, and for growing old (`AgeUp`'s).
fn sting(kind: &str, elder: bool) -> &'static str {
    match (kind.chars().nth(1).unwrap_or('d'), elder) {
        ('c', false) => "sting_agetrans_kitten_to_adult_cat",
        ('c', true) => "sting_agetrans_adult_cat_to_elder_cat",
        ('h', false) => "sting_agetrans_foal_to_adult_horse",
        ('h', true) => "sting_agetrans_adult_horse_to_elder_horse",
        ('l', true) => "sting_agetrans_adult_sdog_to_elder_sdog",
        (_, false) => "sting_agetrans_puppy_to_adult_dog",
        (_, true) => "sting_agetrans_adult_ldog_to_elder_ldog",
    }
}

/// The age transition's halves: the one growing's (`<kind>_ageTransition_part1_x`) and the one
/// it grows into's (`<kind>_part2_y`; an elder's `e<species>_`).
fn part1(kind: &str) -> String {
    format!("{kind}_ageTransition_part1_x")
}

fn part2(kind: &str, elder: bool) -> String {
    if elder { format!("e{}_ageTransition_part2_y", kind.get(1..).unwrap_or("d")) } else { format!("{kind}_ageTransition_part2_y") }
}

/// Neglect as it grew up (`CheckForAdditionOfNegativePetTrait`): each need below its threshold
/// adds its weight (mood under -50: 3; energy under -80: 1; hunger under -60: 2; company under
/// -60: 2; a horse's exercise under -80: 1, and hunger again; another's fun under -50: 1; a cat's
/// scratching or a dog's chewing under -80: 1); six or more and it's grown up with a bad trait.
pub fn neglect(kind: &str, mood: f32, m: &[f32; 8]) -> f32 {
    use crate::petneeds::{ENERGY, FUN, HUNGER, SOCIAL, URGE};
    let mut w = 0.0;
    if mood < -50.0 {
        w += 3.0;
    }
    if m[ENERGY] < -80.0 {
        w += 1.0;
    }
    let hungry = m[HUNGER] < -60.0;
    if hungry {
        w += 2.0;
    }
    if m[SOCIAL] < -60.0 {
        w += 2.0;
    }
    if kind.ends_with('h') {
        if m[URGE] < -80.0 {
            w += 1.0;
        }
        if hungry {
            w += 2.0;
        }
    } else {
        if m[FUN] < -50.0 {
            w += 1.0;
        }
        if m[URGE] < -80.0 {
            w += 1.0;
        }
    }
    w
}

const NEGLECT_THRESHOLD: f32 = 6.0;

/// Due to grow up (or old) once it's free to.
#[derive(Component)]
pub struct AgeDue {
    elder: bool,
}

/// Growing up or old: which half of the transition, how far in (seconds), how long it is.
#[derive(Component)]
pub struct AgingUp {
    elder: bool,
    second: bool,
    t: f32,
    length: f32,
}

/// The ids of pets just grown up, to play the grown half once they're back.
#[derive(Resource, Default)]
struct AgedUp(Vec<u64>);

/// The pets that came home without an age: a little way through their stage.
fn initialize(mut commands: Commands, pets: Query<(Entity, &Pet), (With<HomePet>, Without<PetAge>)>) {
    let mut rng = rand::rng();
    for (e, p) in &pets {
        commands.entity(e).insert(PetAge::start(&p.kind, false, &mut rng));
    }
}

fn sync_elder(mut pets: Query<(&mut Pet, &PetAge), Changed<PetAge>>) {
    for (mut p, a) in &mut pets {
        if p.elder != a.elder {
            p.elder = a.elder;
        }
    }
}

/// A day older at midnight: grown up, grown old, or (an old one past its least span) its time
/// come.
#[allow(clippy::type_complexity)]
fn daily(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    settings: Res<crate::options::Settings>,
    mut last: Local<Option<u32>>,
    mut pets: Query<(Entity, &Pet, &mut PetAge), (With<HomePet>, Without<PetDying>, Without<AgeDue>, Without<AgingUp>)>,
    mut notes: ResMut<Notifications>,
) {
    let day = clock.day();
    let first = last.is_none();
    if last.replace(day) == Some(day) || first || !settings.aging {
        return;
    }
    let factor = settings.lifespan.factor();
    let mut rng = rand::rng();
    for (e, pet, mut age) in &mut pets {
        age.days += 1.0 / factor;
        let span = stage_days(&pet.kind, age.elder);
        if age.elder {
            if age.days >= span && rng.random_bool(settings.lifespan.elder_mortality()) {
                info!("pet aging: {}'s time has come", pet.name);
                commands.entity(e).insert(PetDying::default());
            }
            continue;
        }
        let left = crate::aging::remaining_days(span, age.days, factor);
        if left == 0 {
            commands.entity(e).insert(AgeDue { elder: !pet.kind.starts_with('c') });
        } else if left == 1 {
            let next = if pet.kind.starts_with('c') { format!("a grown {}", crate::pets::kind_name(&grown_kind(&pet.kind)).to_lowercase()) } else { "an elder".to_string() };
            notes.push(format!("{} will be {next} tomorrow!", pet.name));
        }
    }
}

/// `PET_AGE_TEST=<name>:<grow|old|die>` (tests): ten seconds in, the household's pet of that name
/// grows up, grows old or passes away.
fn age_test(mut commands: Commands, time: Res<Time>, pets: Query<(Entity, &Pet), With<HomePet>>, mut done: Local<bool>) {
    let Ok(v) = std::env::var("PET_AGE_TEST") else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    let (name, what) = v.split_once(':').unwrap_or((&v, "grow"));
    let Some((e, p)) = pets.iter().find(|(_, p)| p.name.eq_ignore_ascii_case(name)) else { return };
    *done = true;
    info!("pet age test: {} {what}", p.name);
    match what {
        "die" => {
            commands.entity(e).insert(PetDying::default());
        }
        "old" => {
            commands.entity(e).insert(AgeDue { elder: true });
        }
        _ => {
            commands.entity(e).insert(AgeDue { elder: !p.kind.starts_with('c') });
        }
    }
}

/// The transition: the growing half once it's free (not seen to, ridden or away), then for one
/// growing up, the grown one in its place; for one growing old, the elder's half.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn transition(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    due: Query<(Entity, &Pet, &AgeDue), (Without<PetBusy>, Without<crate::petneeds::PetTask>, Without<crate::riding::Ridden>, Without<crate::equestrian::AtCompetition>, Without<PetDying>)>,
    mut qs: ParamSet<(Query<(Entity, &Pet, &mut PetAnim, &mut AgingUp, &mut PetAge, Option<&crate::petneeds::PetMotives>, Option<&crate::pets::PetTraits>)>, Query<crate::pets::SavedPetQuery>)>,
    (pet_data, gu): (Res<crate::pets::PetData>, Option<Res<crate::icons::GameUi>>),
    (mut pending, mut aged): (ResMut<crate::pets::PendingPets>, ResMut<AgedUp>),
    (mut play, mut notes): (MessageWriter<crate::sound::PlaySound>, ResMut<Notifications>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    for (e, pet, d) in &due {
        let clip = part1(&pet.kind);
        let length = lib.get(&data, &clip).map_or(3.5, |c| c.duration);
        commands.entity(e).remove::<AgeDue>().insert((PetBusy, AgingUp { elder: d.elder, second: false, t: 0.0, length }));
        commands.entity(e).queue(move |mut ew: EntityWorldMut| {
            if let Some(mut a) = ew.get_mut::<PetAnim>() {
                a.replay(clip);
            }
        });
        play.write(crate::sound::PlaySound::ui(sting(&pet.kind, d.elder)));
        info!("pet aging: {} grows {}", pet.name, if d.elder { "old" } else { "up" });
    }
    let mut growing: Vec<(Entity, Pet, Option<crate::petneeds::PetMotives>, Vec<String>)> = Vec::new();
    for (e, pet, mut anim, mut up, mut age, motives, traits) in &mut qs.p0() {
        up.t += dt;
        if up.t < up.length {
            continue;
        }
        if up.second {
            // (Done: about its business again.)
            commands.entity(e).remove::<(AgingUp, PetBusy)>();
            anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rand::rng()));
            continue;
        }
        if up.elder {
            age.elder = true;
            age.days = 0.0;
            let clip = part2(&pet.kind, true);
            *up = AgingUp { elder: true, second: true, t: 0.0, length: lib.get(&data, &clip).map_or(3.5, |c| c.duration) };
            anim.replay(clip);
            notes.push(format!("{} is getting on in years: {} is now an elder {}.", pet.name, if pet.female { "she" } else { "he" }, crate::pets::kind_name(&pet.kind).to_lowercase()));
            continue;
        }
        growing.push((e, pet.clone(), motives.cloned(), traits.map(|t| t.0.clone()).unwrap_or_default()));
    }
    // Grown up: the grown one in its place, of its breed, its coat kept.
    let whole = qs.p1();
    for (e, pet, motives, traits) in growing {
        let (Some(pd), Ok(q)) = (pet_data.0.as_deref(), whole.get(e)) else { continue };
        let Some(mut saved) = crate::pets::save_one(q, pd) else { continue };
        let kind = grown_kind(&pet.kind);
        let breed_name = pd.breeds.get(pet.breed).map(|b| b.name.clone()).unwrap_or_default();
        let same_name = |b: &&s3bake::pets::PetBreed| b.kind == kind && !breed_name.is_empty() && b.name == breed_name;
        let Some(adult) = pd.breeds.iter().find(same_name).or_else(|| pd.breeds.iter().filter(|b| b.kind == kind && b.listed.is_some()).min_by_key(|b| b.listed)).or_else(|| pd.breeds.iter().find(|b| b.kind == kind)) else { continue };
        // (Its coat as it was: its own young one's, or the breed's it was given.)
        let coat = saved.coat.or(Some(saved.outfit));
        let mut traits = traits;
        if let Some(m) = motives.as_ref()
            && neglect(&pet.kind, crate::petneeds::mood(m), &m.0) >= NEGLECT_THRESHOLD
            && traits.len() < crate::pettraits::max_traits(&kind)
        {
            let held: Vec<&str> = traits.iter().map(String::as_str).collect();
            let bad: Vec<&'static str> = crate::pettraits::for_kind(&kind)
                .into_iter()
                .filter(|h| !held.contains(h) && crate::pettraits::compatible(&held, h) && gu.as_deref().and_then(|g| g.data.traits.iter().find(|t| t.hex == *h)).is_some_and(|t| t.score < 0))
                .collect();
            if let Some(t) = bad.choose(&mut rand::rng()) {
                notes.push(format!("{} was neglected growing up, and is now {}.", pet.name, crate::pettraits::name(t, gu.as_deref())));
                traits.push(t.to_string());
            }
        }
        saved = crate::pets::SavedPet {
            kind: kind.clone(),
            outfit: adult.outfit,
            traits,
            coat,
            shapes: Vec::new(),
            sliders: Vec::new(),
            tail: None,
            mane: None,
            hooves: None,
            fur: None,
            fur_mix: None,
            accessories: Vec::new(),
            age: Some(PetAge { days: 0.0, elder: false }),
            ..saved
        };
        notes.push(format!("{} has grown up into a {}!", pet.name, crate::pets::kind_name(&kind).to_lowercase()));
        aged.0.push(saved.id);
        pending.0.push(saved);
        commands.entity(e).try_despawn();
    }
}

/// The grown one back: its half of the transition.
fn aged_up(mut commands: Commands, mut aged: ResMut<AgedUp>, mut pets: Query<(Entity, &HomePet, &Pet, &mut PetAnim)>, (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>)) {
    if aged.0.is_empty() {
        return;
    }
    for (e, h, pet, mut anim) in &mut pets {
        if let Some(i) = aged.0.iter().position(|id| *id == h.id) {
            aged.0.remove(i);
            let clip = part2(&pet.kind, false);
            let length = lib.get(&data, &clip).map_or(3.0, |c| c.duration);
            anim.replay(clip);
            commands.entity(e).insert((PetBusy, AgingUp { elder: false, second: true, t: 0.0, length }));
        }
    }
}

/// An old pet's last moments: which step, how far in (seconds), the Reaper, how long his game
/// with it lasts.
#[derive(Component, Default)]
pub struct PetDying {
    t: f32,
    stage: u8,
    reaper: Option<Entity>,
    social: f32,
}

/// A pet's tombstone: whose, and what it was.
#[derive(Component, Clone, Debug)]
pub struct PetGrave {
    pub name: String,
    pub kind: String,
}

/// A pet's tombstone as kept in saves.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedPetGrave {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub name: String,
    pub kind: String,
}

/// The pack's tombstones, by species.
fn tombstone(kind: &str) -> &'static str {
    match kind.chars().nth(1) {
        Some('c') => "tombstoneCat",
        Some('h') => "tombstoneHorse",
        _ => "tombstoneDog",
    }
}

/// A pet's tombstone put down: the pack's for its species, named for it.
#[allow(clippy::too_many_arguments)]
pub fn place_grave(commands: &mut Commands, assets: &mut crate::objects::ObjectAssets, ctx: &mut crate::objects::AssetCtx, catalog: &crate::loading::Catalog, baked: &crate::baked::BakedData, name: String, kind: &str, at: Vec3, rotation: Quat) -> Option<Entity> {
    let entry = baked.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(tombstone(kind)))?;
    let o = crate::home::spawn_game_object_rot(commands, assets, ctx, catalog, entry.objd, at, rotation)?;
    let label = format!("{name}'s Tombstone");
    commands.entity(o.entity).insert(PetGrave { name, kind: kind.to_string() }).queue_silenced(move |mut w: EntityWorldMut| {
        if let Some(mut g) = w.get_mut::<GameObject>() {
            g.name = label;
        }
    });
    Some(o.entity)
}

/// The Reaper's game with the pet and its, by species (`a2ac_soc_neutral_death_friendly_neutral`).
fn farewell(kind: &str) -> (&'static [&'static str], &'static str) {
    match kind.chars().nth(1) {
        Some('c') => (&["a2ac_soc_neutral_death_friendly_neutral_x"], "a2ac_soc_neutral_death_friendly_neutral_y"),
        Some('h') => (&["a2ah_soc_neutral_death_friendly_neutral_x"], "a2ah_soc_neutral_death_friendly_neutral_y"),
        Some('l') => (&["a2al_soc_neutral_death_friendly_neutral_x"], "a2al_soc_neutral_death_friendly_neutral_y"),
        _ => (&["a2ad_soc_neutral_death_friendly_neutral_x"], "a2ad_soc_neutral_death_friendly_neutral_y"),
    }
}

/// Seconds (at normal speed) to the scene's steps: the Reaper comes, plays with it, it's gone.
const REAPER_AT: f32 = 2.0;
const FAREWELL_AT: f32 = 5.5;
const TOMBSTONE_AFTER: f32 = 1.0;
const LEAVE_AFTER: f32 = 4.0;

/// An old pet passing away (`ReapPetSoul`).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn reap(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    mut dying: Query<(Entity, &Pet, &mut PetDying, &Transform, &mut PetAnim)>,
    mut household: Query<&mut crate::life::Moodlets, With<crate::sim::HouseholdMember>>,
    cas: Option<Res<crate::simbody::CasData>>,
    (data, catalog, mut assets, mut lib): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>, ResMut<crate::anim::ClipLibrary>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut notes, mut play): (ResMut<Notifications>, MessageWriter<crate::sound::PlaySound>),
) {
    if clock.speed == 0 {
        return;
    }
    let dt = time.delta_secs() * (clock.speed as f32).min(2.0);
    for (e, pet, mut d, tf, mut anim) in &mut dying {
        d.t += dt;
        match d.stage {
            // (Lying down, its last.)
            0 => {
                commands.entity(e).remove::<(AgeDue, AgingUp)>().insert(PetBusy);
                anim.play(if pet.kind.ends_with('h') { format!("{}_idle_stand_breathe_x", pet.kind) } else { format!("{}_sleep_loop1_x", pet.kind) });
                play.write(crate::sound::PlaySound::ui("sting_death").with_volume(0.7));
                d.stage = 1;
            }
            // The Reaper comes, before it.
            1 if d.t >= REAPER_AT => {
                let ahead = (tf.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
                let at = tf.translation + ahead * crate::pets::social_distance(&pet.kind);
                let facing = Quat::from_rotation_y((-ahead.x).atan2(-ahead.z));
                d.reaper = Some(crate::death::spawn_reaper(&mut commands, cas.as_deref(), Transform::from_translation(at).with_rotation(facing), 1));
                d.stage = 2;
            }
            // A last game with it, standing.
            2 if d.t >= FAREWELL_AT => {
                let (his, its) = farewell(&pet.kind);
                d.social = lib.get(&data, his[0]).map_or(8.0, |c| c.duration);
                if let Some(r) = d.reaper {
                    commands.entity(r).insert(crate::anim::ActionClip::new(None, his));
                }
                anim.replay(its.to_string());
                d.stage = 3;
            }
            // Gone with him; its tombstone rises.
            3 if d.t >= FAREWELL_AT + d.social => {
                commands.entity(e).insert(Visibility::Hidden);
                if let Some(r) = d.reaper {
                    commands.entity(r).insert(crate::anim::ActionClip::new(Some("a_death_create_x"), &["a_death_float_x"]));
                }
                d.stage = 4;
            }
            4 if d.t >= FAREWELL_AT + d.social + TOMBSTONE_AFTER => {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                place_grave(&mut commands, &mut assets, &mut ctx, &catalog, &data.0, pet.name.clone(), &pet.kind, tf.translation, tf.rotation);
                notes.push(format!("{} has passed away of old age. Rest in peace, {}.", pet.name, pet.name));
                for mut m in &mut household {
                    m.add(crate::life::MoodletKind::Mourning, clock.minutes);
                }
                info!("pet aging: {} passed away", pet.name);
                d.stage = 5;
            }
            // He goes.
            5 if d.t >= FAREWELL_AT + d.social + TOMBSTONE_AFTER + LEAVE_AFTER => {
                if let Some(r) = d.reaper {
                    commands.entity(r).try_despawn();
                }
                commands.entity(e).try_despawn();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pets_live_their_stages_by_the_games_tuning() {
        assert_eq!(stage_days("cc", false), 7.0);
        assert!((stage_days("ch", false) - 4.69).abs() < 1e-3, "a foal's not five days");
        assert!((stage_days("ad", false) - 25.0).abs() < 0.01);
        assert_eq!(stage_days("ac", false), 28.0);
        assert_eq!(stage_days("ah", false), 35.0);
        assert_eq!(stage_days("ad", true), 14.0);
        assert!((stage_days("ac", true) - 17.0).abs() < 0.01);
        assert_eq!(grown_kind("cd"), "ad");
        assert_eq!(grown_kind("ch"), "ah");
        let a = PetAge::start("ac", false, &mut rand::rng());
        assert!(a.days < 28.0 * START_OFFSET);
    }

    #[test]
    fn the_transitions_are_the_packs() {
        assert_eq!(part1("cc"), "cc_ageTransition_part1_x");
        assert_eq!(part2("ac", false), "ac_ageTransition_part2_y");
        assert_eq!(part2("ad", true), "ed_ageTransition_part2_y");
        assert_eq!(sting("al", true), "sting_agetrans_adult_sdog_to_elder_sdog");
        assert_eq!(tombstone("ah"), "tombstoneHorse");
    }

    #[test]
    fn neglect_is_weighed_as_the_game_weighs_it() {
        let fine = [50.0; 8];
        assert_eq!(neglect("cc", 40.0, &fine), 0.0);
        let mut m = fine;
        m[crate::petneeds::HUNGER] = -70.0;
        m[crate::petneeds::SOCIAL] = -70.0;
        assert_eq!(neglect("cd", -60.0, &m), 7.0, "unhappy, hungry and lonely");
        assert_eq!(neglect("ch", 0.0, &m), 6.0, "a foal's hunger counts twice");
    }
}
