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
        app.init_resource::<AgingSettings>()
            .add_systems(Update, (daily_aging, rebuild_bodies).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// Whether Sims age (the game's option).
#[derive(Resource)]
pub struct AgingSettings {
    pub enabled: bool,
}

impl Default for AgingSettings {
    fn default() -> Self {
        Self { enabled: std::env::var_os("SIMS3_NO_AGING").is_none() }
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
        Age::Child => 7.0,
        Age::Teen => 14.0,
        Age::YoungAdult => 21.0,
        Age::Adult => 21.0,
        Age::Elder => f32::INFINITY,
    }
}

fn next_age(age: Age) -> Option<Age> {
    match age {
        Age::Child => Some(Age::Teen),
        Age::Teen => Some(Age::YoungAdult),
        Age::YoungAdult => Some(Age::Adult),
        Age::Adult => Some(Age::Elder),
        Age::Elder => None,
    }
}

fn age_word(age: Age) -> &'static str {
    match age {
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
        Age::Child => "sting_agetrans_p_c",
        Age::Teen => "sting_agetrans_c_t",
        Age::YoungAdult => "sting_agetrans_t_h",
        Age::Adult => "sting_agetrans_h_a",
        Age::Elder => "sting_agetrans_a_e",
    }
}

/// A Sim whose body must be rebuilt (after a birthday).
#[derive(Component)]
pub struct NeedsNewBody;

/// Ages the household once a day, at midnight.
#[allow(clippy::too_many_arguments)]
fn daily_aging(
    mut commands: Commands,
    clock: Res<GameClock>,
    settings: Res<AgingSettings>,
    mut last_day: Local<Option<u32>>,
    mut sims: Query<(Entity, &mut Sim, Option<&mut Aging>, &mut Moodlets, Has<Selected>), With<HouseholdMember>>,
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
    if !settings.enabled {
        return;
    }
    let mut rng = rand::rng();
    let mut died: Vec<(Entity, String, bool)> = Vec::new();
    let mut survivors: Vec<Entity> = Vec::new();
    for (e, mut sim, aging, mut moodlets, selected) in &mut sims {
        let Some(mut aging) = aging else {
            commands.entity(e).insert(Aging::default());
            continue;
        };
        aging.days += 1.0;
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
            if aging.days + 1.0 >= stage_days(sim.age) {
                notes.push(format!("{} will be {} tomorrow!", sim.first, age_word(next_age(sim.age).unwrap_or(sim.age))));
            }
            continue;
        }
        let Some(age) = next_age(sim.age) else { continue };
        sim.age = age;
        aging.days = 0.0;
        // A new trait slot opens for teens and young adults.
        let slots = crate::life::trait_slots(age);
        let mut gained = None;
        while sim.traits.len() < slots {
            let options: Vec<Trait> = Trait::ALL.into_iter().filter(|t| !sim.traits.contains(t) && t.compatible(&sim.traits)).collect();
            let Some(&t) = options.choose(&mut rng) else { break };
            sim.traits.push(t);
            gained = Some(t);
        }
        sim.outfit = crate::sim::OutfitChoice::default();
        moodlets.add(MoodletKind::Birthday, clock.minutes);
        life.write(LifeEvent::new(e, LifeEventKind::Birthday));
        play.write(PlaySound::ui(birthday_sting(age)).with_volume(0.7));
        notes.push(match gained {
            Some(t) => format!("Happy birthday! {} is now {} and has become {}.", sim.first, age_word(age), t.name()),
            None => format!("Happy birthday! {} is now {}.", sim.first, age_word(age)),
        });
        commands.entity(e).insert(NeedsNewBody);
    }
    for (e, name, selected) in died {
        notes.push(format!("{name} has passed away peacefully of old age."));
        play.write(PlaySound::ui("sting_death").with_volume(0.7));
        commands.entity(e).despawn();
        for &s in &survivors {
            if let Ok((_, _, _, mut m, _)) = sims.get_mut(s) {
                m.add(MoodletKind::Heartbroken, clock.minutes);
            }
        }
        if selected && let Some(&s) = survivors.first() {
            commands.entity(s).insert(Selected);
        }
    }
    if survivors.is_empty() && sims.iter().count() > 0 && sims.iter().all(|q| q.1.age == Age::Elder) {
        notes.push("The household has no one left. Their story has come to an end.");
    }
}

/// Gives Sims who had a birthday the body of their new age.
#[allow(clippy::too_many_arguments)]
fn rebuild_bodies(
    mut commands: Commands,
    sims: Query<(Entity, &Sim, &Children), With<NeedsNewBody>>,
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
    for (e, sim, children) in &sims {
        commands.entity(e).remove::<NeedsNewBody>();
        let outfit = crate::simbody::pick_outfit(&cas, sim, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look));
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
        commands.entity(e).insert(crate::anim::ClipPlayer::default());
    }
}
