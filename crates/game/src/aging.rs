//! Growing up and growing old: each life stage lasts a number of days (the game's normal
//! lifespan), birthdays move a Sim to the next stage with a new body and, for teens and young
//! adults, a new trait; elders eventually pass away.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::clock::GameClock;
use crate::interact::Notifications;
use crate::life::{LifeEvent, LifeEventKind, MoodletKind, Moodlets, Trait};
use crate::sim::{Age, HouseholdMember, Selected, Sim};
use crate::sound::PlaySound;
use crate::PlayMode;

pub struct AgingPlugin;

impl Plugin for AgingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (daily_aging, grow_up_now, rebuild_bodies).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// Days lived in the current life stage, and how long this Sim's old age lasts.
#[derive(Component, Clone, Copy, Debug)]
pub struct Aging {
    pub days: f32,
    pub elder_span: f32,
}

impl Default for Aging {
    fn default() -> Self {
        Self { days: 0.0, elder_span: rand::rng().random_range(14.0..22.0) }
    }
}

/// Days in each life stage (the game's normal lifespan).
pub fn stage_days(age: Age) -> f32 {
    match age {
        Age::Baby => 2.0,
        Age::Toddler => 7.0,
        Age::Child => 7.0,
        Age::Teen => 14.0,
        Age::YoungAdult => 21.0,
        Age::Adult => 21.0,
        Age::Elder => f32::INFINITY,
    }
}

pub fn next_age(age: Age) -> Option<Age> {
    match age {
        Age::Baby => Some(Age::Toddler),
        Age::Toddler => Some(Age::Child),
        Age::Child => Some(Age::Teen),
        Age::Teen => Some(Age::YoungAdult),
        Age::YoungAdult => Some(Age::Adult),
        Age::Adult => Some(Age::Elder),
        Age::Elder => None,
    }
}

pub fn age_word(age: Age) -> &'static str {
    match age {
        Age::Baby => "a baby",
        Age::Toddler => "a toddler",
        Age::Child => "a child",
        Age::Teen => "a teen",
        Age::YoungAdult => "a young adult",
        Age::Adult => "an adult",
        Age::Elder => "an elder",
    }
}

/// The game's sting for growing into `age`.
fn birthday_sting(age: Age) -> &'static str {
    match age {
        Age::Baby => "sting_baby_conception",
        Age::Toddler => "sting_agetrans_b_p",
        Age::Child => "sting_agetrans_p_c",
        Age::Teen => "sting_agetrans_c_t",
        Age::YoungAdult => "sting_agetrans_t_h",
        Age::Adult => "sting_agetrans_h_a",
        Age::Elder => "sting_agetrans_a_e",
    }
}

/// A Sim whose body must be rebuilt (after a birthday, or when their shape has changed).
#[derive(Component)]
pub struct NeedsNewBody;

/// Shape change since the body was last built.
#[derive(Component, Default)]
struct ShapeDrift(f32);

/// Changes a Sim's weight and fitness, rebuilding their body once the change shows.
pub fn reshape(e: &mut EntityWorldMut, weight: f32, fitness: f32) {
    let Some(mut sim) = e.get_mut::<Sim>() else { return };
    if sim.age.is_little() {
        return;
    }
    let (w0, f0) = (sim.weight, sim.fitness);
    sim.weight = (sim.weight + weight).clamp(-1.0, 1.0);
    sim.fitness = (sim.fitness + fitness).clamp(0.0, 1.0);
    let moved = (sim.weight - w0).abs() + (sim.fitness - f0).abs();
    let mut drift = e.get::<ShapeDrift>().map_or(0.0, |d| d.0) + moved;
    if drift >= 0.1 {
        drift = 0.0;
        e.insert(NeedsNewBody);
    }
    e.insert(ShapeDrift(drift));
}

/// Ages the household once a day, at midnight.
#[allow(clippy::too_many_arguments)]
fn daily_aging(
    mut commands: Commands,
    clock: Res<GameClock>,
    settings: Res<crate::options::Settings>,
    mut last_day: Local<Option<u32>>,
    mut sims: Query<(Entity, &mut Sim, Option<&mut Aging>, &mut Moodlets, Has<Selected>), (With<HouseholdMember>, Without<crate::death::Dying>)>,
    mut life: MessageWriter<LifeEvent>,
    mut play: MessageWriter<PlaySound>,
    mut notes: ResMut<Notifications>,
) {
    let day = clock.day();
    let first = last_day.is_none();
    if last_day.replace(day) == Some(day) || first {
        // Sims who just arrived start their stage at a random point, like the town's.
        if first {
            let mut rng = rand::rng();
            for (e, sim, aging, _, _) in &mut sims {
                if aging.is_none() {
                    let mut a = Aging::default();
                    let span = if sim.age == Age::Elder { a.elder_span } else { stage_days(sim.age) };
                    a.days = rng.random_range(0.0..span * 0.5).floor();
                    // Testing: everyone's birthday is at the next midnight.
                    if std::env::var_os("SIMS3_AGE_SOON").is_some() {
                        a.days = span - 1.0;
                    }
                    commands.entity(e).insert(a);
                }
            }
        }
        return;
    }
    if !settings.aging {
        return;
    }
    // Longer life spans stretch every stage.
    let per_day = 1.0 / settings.lifespan.factor();
    let mut rng = rand::rng();
    let mut died: Vec<(Entity, String, bool)> = Vec::new();
    let mut survivors: Vec<Entity> = Vec::new();
    for (e, mut sim, aging, mut moodlets, selected) in &mut sims {
        let Some(mut aging) = aging else {
            commands.entity(e).insert(Aging::default());
            continue;
        };
        aging.days += per_day;
        if sim.age == Age::Elder {
            if aging.days >= aging.elder_span {
                died.push((e, sim.full_name(), selected));
            } else {
                survivors.push(e);
            }
            continue;
        }
        survivors.push(e);
        if aging.days < stage_days(sim.age) {
            if aging.days + per_day >= stage_days(sim.age) {
                notes.push(format!("{} will be {} tomorrow!", sim.first, age_word(next_age(sim.age).unwrap_or(sim.age))));
            }
            continue;
        }
        grow_up(&mut commands, e, &mut sim, &mut aging, &mut moodlets, clock.minutes, &mut life, &mut play, &mut notes, &mut rng);
    }
    for (e, _name, _selected) in died {
        // The Grim Reaper comes for them (see `death`).
        play.write(PlaySound::ui("sting_death").with_volume(0.7));
        commands.entity(e).insert(crate::death::Dying::new());
        for &s in &survivors {
            if let Ok((_, _, _, mut m, _)) = sims.get_mut(s) {
                m.add(MoodletKind::Heartbroken, clock.minutes);
            }
        }
    }
}

/// A Sim's birthday: the next stage of life, perhaps a new trait, and a new body.
#[allow(clippy::too_many_arguments)]
fn grow_up(
    commands: &mut Commands,
    e: Entity,
    sim: &mut Sim,
    aging: &mut Aging,
    moodlets: &mut Moodlets,
    now: f64,
    life: &mut MessageWriter<LifeEvent>,
    play: &mut MessageWriter<PlaySound>,
    notes: &mut Notifications,
    rng: &mut impl rand::Rng,
) {
    let Some(age) = next_age(sim.age) else { return };
    sim.age = age;
    aging.days = 0.0;
    // School is over for young adults.
    if age == Age::YoungAdult {
        commands.entity(e).remove::<crate::rabbitholes::SchoolGrades>();
    }
    // A new trait slot opens for teens and young adults.
    let slots = crate::life::trait_slots(age);
    let mut gained = None;
    while sim.traits.len() < slots {
        let options: Vec<Trait> = Trait::ALL.into_iter().filter(|t| !sim.traits.contains(t) && t.compatible(&sim.traits)).collect();
        let Some(&t) = options.choose(rng) else { break };
        sim.traits.push(t);
        gained = Some(t);
    }
    sim.outfit = crate::sim::OutfitChoice::default();
    moodlets.add(MoodletKind::Birthday, now);
    life.write(LifeEvent::new(e, LifeEventKind::Birthday));
    play.write(PlaySound::ui(birthday_sting(age)).with_volume(0.7));
    notes.push(match gained {
        Some(t) => format!("Happy birthday! {} is now {} and has become {}.", sim.first, age_word(age), t.name()),
        None => format!("Happy birthday! {} is now {}.", sim.first, age_word(age)),
    });
    commands.entity(e).insert(NeedsNewBody);
}

/// Blew out a birthday cake's candles: grows up now.
#[derive(Component)]
pub struct GrowUpNow;

#[allow(clippy::too_many_arguments)]
fn grow_up_now(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut sims: Query<(Entity, &mut Sim, Option<&mut Aging>, &mut Moodlets), With<GrowUpNow>>,
    mut life: MessageWriter<LifeEvent>,
    mut play: MessageWriter<PlaySound>,
    mut notes: ResMut<Notifications>,
) {
    let mut rng = rand::rng();
    for (e, mut sim, aging, mut moodlets) in &mut sims {
        commands.entity(e).remove::<GrowUpNow>();
        if next_age(sim.age).is_none() {
            notes.push(format!("{} made a wish and blew out the candles.", sim.first));
            continue;
        }
        let mut a = aging.map(|a| a.clone()).unwrap_or_default();
        grow_up(&mut commands, e, &mut sim, &mut a, &mut moodlets, clock.minutes, &mut life, &mut play, &mut notes, &mut rng);
        commands.entity(e).insert(a);
    }
}

/// Gives Sims who had a birthday the body of their new age.
#[allow(clippy::too_many_arguments)]
fn rebuild_bodies(
    mut commands: Commands,
    sims: Query<(Entity, &Sim, &Children, Option<&crate::simbody::Wearing>, Option<&crate::careers::Job>, Option<&crate::simbody::ServiceUniform>), With<NeedsNewBody>>,
    parts: Query<(), With<crate::simbody::SimModelPart>>,
    data: Option<Res<crate::baked::Baked>>,
    cas: Option<Res<crate::simbody::CasData>>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut skin_mats, mut bindposes, mut textures): (
        ResMut<Assets<crate::simbody::SimSkinMaterial>>,
        ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
        ResMut<crate::simbody::SimTextures>,
    ),
) {
    let (Some(data), Some(cas)) = (data, cas) else { return };
    for (e, sim, children, wearing, job, service) in &sims {
        commands.entity(e).remove::<NeedsNewBody>();
        let kind = wearing.map_or(crate::simbody::OutfitKind::Everyday, |w| w.0);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look);
        let uniform = job.and_then(|j| j.uniform(sim)).filter(|_| kind == crate::simbody::OutfitKind::Career);
        let outfit = match (service, uniform.and_then(|u| crate::simbody::uniform(&cas, sim, u, &mut rng.clone()))) {
            (Some(s), _) => crate::simbody::service_outfit(&cas, sim, *s, &mut rng),
            (None, Some(o)) => o,
            (None, None) => crate::simbody::pick_outfit_for(&cas, sim, &mut rng, kind),
        };
        debug!("{} dressed ({kind:?}): {:?}, hair {:?}", sim.first, outfit.body.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), outfit.hair.as_ref().map(|h| h.name.as_str()));
        let Some(model) = crate::simbody::build_sim_model(&data.0, &cas, sim, &outfit, crate::simbody::tone_of(sim)) else { continue };
        for c in children.iter() {
            if parts.get(c).is_ok() {
                commands.entity(c).despawn();
            }
        }
        let mut ctx = crate::simbody::SimRenderCtx {
            meshes: &mut meshes,
            images: &mut images,
            mats: &mut mats,
            skin_mats: &mut skin_mats,
            bindposes: &mut bindposes,
            textures: &mut textures,
        };
        crate::simbody::spawn_sim_model(&mut commands, e, model, &mut ctx);
        // (The animation carries on: getting into bed in sleepwear doesn't start over.)
        commands.entity(e).queue_silenced(|mut e: EntityWorldMut| match e.get_mut::<crate::anim::ClipPlayer>() {
            Some(mut p) => p.rebuilt(),
            None => {
                e.insert(crate::anim::ClipPlayer::default());
            }
        });
    }
}
