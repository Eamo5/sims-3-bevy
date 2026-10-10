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
        app.init_resource::<AgingDay>()
            .add_systems(OnEnter(crate::AppState::Loading), reset_aging_day)
            .add_systems(Update, (daily_aging, grow_up_now, rebuild_bodies).chain().after(crate::save::apply_loaded_game).run_if(in_state(PlayMode::Live)));
    }
}

/// Days lived in the current stage and persistent old-age survival state.
#[derive(Component, Clone, Copy, Debug)]
pub struct Aging {
    pub days: f32,
    pub elder_span: f32,
    /// Remaining exponential survival threshold. None preserves legacy fixed-span saves.
    pub elder_risk: Option<f64>,
}

impl Default for Aging {
    fn default() -> Self {
        Self { days: 0.0, elder_span: 17.0, elder_risk: Some(-rand::rng().random::<f64>().max(f64::MIN_POSITIVE).ln()) }
    }
}

#[derive(Resource, Default)]
pub(crate) struct AgingDay(Option<u32>);

fn reset_aging_day(mut day: ResMut<AgingDay>) {
    day.0 = None;
}

impl Aging {
    /// Accumulating daily survival probabilities preserves the configured hazard even
    /// when the lifespan changes, without re-rolling the Sim's fate on save/reload.
    fn old_age_today(&mut self, probability: f64) -> bool {
        let Some(risk) = self.elder_risk.as_mut() else { return self.days >= self.elder_span };
        if self.days < 17.0 { return false; }
        *risk += (1.0 - probability).ln();
        *risk <= 0.0
    }
}

#[cfg(test)]
mod lifespan_tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn loading_a_different_date_does_not_age_the_restored_household() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.init_resource::<GameClock>()
            .init_resource::<AgingDay>()
            .init_resource::<crate::options::Settings>()
            .init_resource::<Notifications>()
            .add_message::<LifeEvent>()
            .add_message::<PlaySound>()
            .add_systems(Update, daily_aging);
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let sim = crate::sim::random_sim(&mut rng, "Reload", Some(false), Age::Child);
        let e = app.world_mut().spawn((sim, HouseholdMember, Moodlets::default(), Aging { days: 6.0, ..default() })).id();
        app.world_mut().resource_mut::<GameClock>().minutes = 14400.0;
        app.update();
        for loaded_day in [3.0, 20.0] {
            app.world_mut().run_system_once(reset_aging_day).unwrap();
            app.world_mut().resource_mut::<GameClock>().minutes = loaded_day * 1440.0;
            app.update();
            assert_eq!(app.world().get::<Aging>(e).unwrap().days, 6.0);
            assert_eq!(app.world().get::<Sim>(e).unwrap().age, Age::Child);
        }
        app.world_mut().resource_mut::<GameClock>().minutes += 1440.0;
        app.update();
        assert_eq!(app.world().get::<Sim>(e).unwrap().age, Age::Teen, "the next actual midnight still ages the Sim");
        assert_eq!(app.world().get::<Aging>(e).unwrap().days, 0.0);
    }

    #[test]
    fn cake_and_midnight_together_only_advance_one_life_stage() {
        let mut app = App::new();
        app.init_resource::<GameClock>()
            .init_resource::<AgingDay>()
            .init_resource::<crate::options::Settings>()
            .init_resource::<Notifications>()
            .add_message::<LifeEvent>()
            .add_message::<PlaySound>()
            .add_systems(Update, (daily_aging, grow_up_now).chain());
        let mut rng = rand::rngs::StdRng::seed_from_u64(4);
        let sim = crate::sim::random_sim(&mut rng, "Birthday", Some(false), Age::Child);
        let e = app.world_mut().spawn((sim, HouseholdMember, Moodlets::default(), Aging {
            days: stage_days(Age::Child) - 1.0,
            ..default()
        })).id();
        app.update(); // Initialize the daily clock before crossing midnight.
        app.world_mut().entity_mut(e).insert(GrowUpNow);
        app.world_mut().resource_mut::<GameClock>().minutes = 1440.0;
        app.update();
        assert_eq!(app.world().get::<Sim>(e).unwrap().age, Age::Teen);
        assert_eq!(app.world().get::<Aging>(e).unwrap().days, 0.0);
        assert!(app.world().get::<GrowUpNow>(e).is_none());
        app.update();
        assert_eq!(app.world().get::<Sim>(e).unwrap().age, Age::Teen);
        // A later, independent cake still works without waiting for midnight.
        app.world_mut().entity_mut(e).insert(GrowUpNow);
        app.update();
        assert_eq!(app.world().get::<Sim>(e).unwrap().age, Age::YoungAdult);
        assert!(app.world().get::<GrowUpNow>(e).is_none());
    }

    #[test]
    fn fractional_aging_reaches_birthdays_without_an_extra_day() {
        use crate::options::Lifespan;
        for (preset, teen_days) in [(Lifespan::Short, 4), (Lifespan::Medium, 8), (Lifespan::Normal, 14), (Lifespan::Long, 30), (Lifespan::Epic, 150)] {
            let factor = preset.factor();
            let mut lived = 0.0;
            for day in 0..=teen_days {
                assert_eq!(remaining_days(stage_days(Age::Teen), lived, factor), teen_days - day, "{preset:?}, day {day}");
                lived += 1.0 / factor;
            }
        }
        assert_eq!(remaining_days(14.0, 13.5, 1.0), 1, "a real fractional day must not be rounded down");
        assert_eq!(remaining_days(14.0, 16.0, 1.0), 0);
    }

    #[test]
    fn elder_survival_respects_minimum_preset_changes_and_persisted_risk() {
        use crate::options::Lifespan;
        let mut age = Aging { days: 16.0, elder_span: 17.0, elder_risk: Some(0.8) };
        assert!(!age.old_age_today(Lifespan::Short.elder_mortality()));
        assert_eq!(age.elder_risk, Some(0.8), "no mortality before the minimum elder stage");
        age.days = 17.0;
        for preset in [Lifespan::Epic, Lifespan::Long, Lifespan::Normal, Lifespan::Medium, Lifespan::Short] {
            let saved = serde_json::to_string(&(age.days, age.elder_span, age.elder_risk)).unwrap();
            let (days, elder_span, elder_risk) = serde_json::from_str(&saved).unwrap();
            let mut restored = Aging { days, elder_span, elder_risk };
            assert_eq!(age.old_age_today(preset.elder_mortality()), restored.old_age_today(preset.elder_mortality()));
            assert_eq!(age.elder_risk, restored.elder_risk);
        }
        let death_day = |factor| {
            let mut a = Aging { days: 17.0, elder_span: 17.0, elder_risk: Some(0.8) };
            while !a.old_age_today(Lifespan::Normal.elder_mortality() * factor) { a.days += 1.0; }
            a.days
        };
        assert!(death_day(0.75) > death_day(1.0), "Marathon Runner reduces daily mortality");
        let mut legacy = Aging { days: 20.0, elder_span: 21.0, elder_risk: None };
        assert!(!legacy.old_age_today(0.30));
        legacy.days = 21.0;
        assert!(legacy.old_age_today(0.03), "old fixed-lifespan saves remain compatible");
    }

    #[test]
    fn normal_elder_lifetimes_have_the_tuned_minimum_and_daily_mortality_tail() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(83);
        let spans: Vec<f32> = (0..10000).map(|_| {
            let mut age = Aging { days: 17.0, elder_span: 17.0, elder_risk: Some(-rng.random::<f64>().max(f64::MIN_POSITIVE).ln()) };
            while !age.old_age_today(0.14) { age.days += 1.0; }
            age.days
        }).collect();
        assert!(spans.iter().all(|s| *s >= 17.0));
        assert!(spans.iter().any(|s| *s > 35.0), "elders may outlive the former fixed upper bound");
        let mean = spans.iter().sum::<f32>() / spans.len() as f32;
        assert!((mean - (17.0 + 0.86 / 0.14)).abs() < 0.3, "mean span {mean}");
        let at_minimum = spans.iter().filter(|s| **s == 17.0).count() as f32 / spans.len() as f32;
        assert!((at_minimum - 0.14).abs() < 0.015, "minimum-age mortality {at_minimum}");
    }
}

/// Whole simulation days remaining. Aging stores normal-lifespan days as f32;
/// repeated fractional increments can drift just below a birthday boundary.
/// Ignore less than 0.001 simulation days (under two minutes) of numeric drift.
pub fn remaining_days(span: f32, lived: f32, factor: f32) -> u32 {
    (((span as f64 - lived as f64) * factor as f64 - 0.001).max(0.0).ceil()) as u32
}

/// Days in each life stage (the game's normal lifespan).
pub fn stage_days(age: Age) -> f32 {
    match age {
        Age::Baby => 3.0,
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
    mut last_day: ResMut<AgingDay>,
    mut sims: Query<(Entity, &mut Sim, Option<&mut Aging>, &mut Moodlets, Has<Selected>, Option<&crate::journal::SkillJournal>), (With<HouseholdMember>, Without<crate::death::Dying>)>,
    mut life: MessageWriter<LifeEvent>,
    mut play: MessageWriter<PlaySound>,
    mut notes: ResMut<Notifications>,
) {
    let day = clock.day();
    let first = last_day.0.is_none();
    if last_day.0.replace(day) == Some(day) || first {
        // Sims who just arrived start their stage at a random point, like the town's.
        if first {
            let mut rng = rand::rng();
            for (e, sim, aging, _, _, _) in &mut sims {
                if aging.is_none() {
                    let mut a = Aging::default();
                    let span = if sim.age == Age::Elder { 17.0 } else { stage_days(sim.age) };
                    // AgingManager.kPercentCasOffset = 28.
                    a.days = rng.random_range(0.0..span * 0.28).floor();
                    // Testing: everyone's birthday is at the next midnight.
                    if std::env::var_os("SIMS3_AGE_SOON").is_some() {
                        a.days = if sim.age == Age::Elder { a.elder_span - 1.0 } else { span - 1.0 };
                        if sim.age == Age::Elder { a.elder_risk = Some(0.0); }
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
    for (e, mut sim, aging, mut moodlets, selected, journal) in &mut sims {
        let Some(mut aging) = aging else {
            commands.entity(e).insert(Aging::default());
            continue;
        };
        aging.days += per_day;
        if sim.age == Age::Elder {
            let probability = settings.lifespan.elder_mortality() * if crate::journal::earned(journal, "Marathon Runner") { 0.75 } else { 1.0 };
            if aging.old_age_today(probability) {
                died.push((e, sim.full_name(), selected));
            } else {
                survivors.push(e);
            }
            continue;
        }
        survivors.push(e);
        let remaining = remaining_days(stage_days(sim.age), aging.days, settings.lifespan.factor());
        if remaining > 0 {
            if remaining == 1 {
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
            if let Ok((_, _, _, mut m, _, _)) = sims.get_mut(s) {
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
    // A midnight birthday can coincide with finishing the cake interaction.
    // Both paths fulfill the same pending age-up, rather than skipping a stage.
    commands.entity(e).remove::<GrowUpNow>();
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
        let options: Vec<Trait> = Trait::ALL.into_iter().filter(|t| t.allowed_at(sim.age) && !sim.traits.contains(t) && t.compatible(&sim.traits)).collect();
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
    sims: Query<
        (Entity, &Sim, &Children, Option<&crate::simbody::Wearing>, Option<&crate::careers::Job>, Option<&crate::simbody::ServiceUniform>, Has<crate::supernatural::WolfForm>),
        With<NeedsNewBody>,
    >,
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
    for (e, sim, children, wearing, job, service, wolf) in &sims {
        commands.entity(e).remove::<NeedsNewBody>();
        let kind = wearing.map_or(crate::simbody::OutfitKind::Everyday, |w| w.0);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look);
        let uniform = job.and_then(|j| j.uniform(sim)).filter(|_| kind == crate::simbody::OutfitKind::Career);
        let mut outfit = match (service, uniform.and_then(|u| crate::simbody::uniform(&cas, sim, u, &mut rng.clone()))) {
            (Some(s), _) => crate::simbody::service_outfit(&cas, sim, *s, &mut rng),
            (None, Some(o)) => o,
            (None, None) => crate::simbody::pick_outfit_for(&cas, sim, &mut rng, kind),
        };
        if wolf {
            crate::supernatural::wolf_outfit(&cas, sim, &mut outfit);
        }
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
