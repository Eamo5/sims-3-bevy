//! Death, as the game stages it: an elder whose time has come collapses (`e_die_oldAge_x`), the
//! Grim Reaper appears beside them in his robe, scythe in hand (`a_death_appear`, floating while
//! he waits), and with a wave of the scythe raises a tombstone where they fell (`a_death_create`)
//! before vanishing. The family mourns at the tombstone, which stays on the lot.
//!
//! Sims die of old age, in fires, of hunger (a day and a half starving), of electrocution
//! (a second shock while still singed from the first) and by drowning (left in a pool with no
//! way out), each with the game's own last moments.

use bevy::prelude::*;

use crate::anim::ActionClip;
use crate::baked::Baked;
use crate::interact::{ActionQueue, GameObject, Notifications};
use crate::loading::Catalog;
use crate::objects::{AssetCtx, ObjectAssets};
use crate::sim::{Age, Selected, Sim};
use crate::{AppState, PlayMode};

pub struct DeathPlugin;

impl Plugin for DeathPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (shocks, starve, reap).run_if(in_state(PlayMode::Live)));
    }
}

/// A Sim whose time has come (an elder, by aging; or in a fire). The scene plays out here.
#[derive(Component)]
pub struct Dying {
    t: f32,
    stage: u8,
    reaper: Option<Entity>,
    /// How they died, for the notice ("of old age", "in a fire").
    pub cause: &'static str,
    /// Their last moments.
    clip: &'static [&'static str],
    /// Where their tombstone goes, when not where they fell (beside the pool they drowned in).
    grave_at: Option<Vec3>,
}

impl Dying {
    pub fn new() -> Self {
        Self { t: 0.0, stage: 0, reaper: None, cause: "peacefully of old age", clip: &["e_die_oldAge_x"], grave_at: None }
    }

    pub fn in_fire() -> Self {
        Self { cause: "in a fire", ..Self::new() }
    }

    pub fn starved() -> Self {
        Self { cause: "of hunger", clip: &["a_die_starvation_x"], ..Self::new() }
    }

    pub fn electrocuted() -> Self {
        Self { cause: "of electrocution", clip: &["a_die_electrocution_x"], ..Self::new() }
    }

    /// Drowned in a pool: the tombstone at its side (`edge`).
    pub fn drowned(edge: Vec3) -> Self {
        Self { cause: "by drowning", clip: &["a_die_drowning_x", "a_die_drowning_loop_x"], grave_at: Some(edge), ..Self::new() }
    }
}

/// Shocked while repairing something electric: singed, or, singed already, electrocuted.
#[derive(Component)]
pub struct Shocked;

fn shocks(mut commands: Commands, clock: Res<crate::clock::GameClock>, mut sims: Query<(Entity, &mut crate::life::Moodlets), With<Shocked>>) {
    for (e, mut m) in &mut sims {
        commands.entity(e).remove::<Shocked>();
        if m.0.iter().any(|x| x.kind == crate::life::MoodletKind::Singed) {
            commands.entity(e).insert(Dying::electrocuted());
        } else {
            m.add(crate::life::MoodletKind::Singed, clock.minutes);
            commands.entity(e).insert(ActionClip::new(None, &["a2o_handiness_fail_electrocution_x"]));
        }
    }
}

/// Game minutes a Sim has gone starving.
#[derive(Component, Default)]
struct Starvation(f64);

/// How long a Sim can go starving before they die of it (game minutes).
const STARVE_MINUTES: f64 = 36.0 * 60.0;

/// Teens and grown-ups left starving die of it; a warning comes halfway.
fn starve(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    mut sims: Query<(Entity, &Sim, &crate::sim::Motives, Option<&mut Starvation>), (With<crate::sim::HouseholdMember>, Without<Dying>)>,
    mut notes: ResMut<Notifications>,
) {
    for (e, sim, m, starving) in &mut sims {
        let hungry = m.0[crate::sim::HUNGER] <= -95.0 && !sim.age.is_little() && sim.age != Age::Child;
        match (hungry, starving) {
            (true, Some(mut s)) => {
                let before = s.0;
                s.0 += delta.0 as f64;
                if before < STARVE_MINUTES / 2.0 && s.0 >= STARVE_MINUTES / 2.0 {
                    notes.push(format!("{} is starving and will die if {} doesn't eat soon!", sim.first, if sim.female { "she" } else { "he" }));
                }
                if s.0 >= STARVE_MINUTES {
                    commands.entity(e).remove::<Starvation>().insert(Dying::starved());
                }
            }
            (true, None) => {
                commands.entity(e).insert(Starvation::default());
            }
            (false, Some(_)) => {
                commands.entity(e).remove::<Starvation>();
            }
            (false, None) => {}
        }
    }
}

/// The Grim Reaper (a body only: no needs, no queue).
#[derive(Component)]
pub struct GrimReaper;

/// Seconds (at normal speed) between the scene's steps.
const APPEAR_AT: f32 = 1.5;
const REAP_AT: f32 = 7.0;
const TOMBSTONE_AT: f32 = 8.0;
const LEAVE_AT: f32 = 12.0;

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn reap(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<crate::clock::GameClock>,
    mut dying: Query<(Entity, &Sim, &mut Dying, &Transform, Option<&mut ActionQueue>, Has<Selected>, Option<&crate::nav::Floor>)>,
    survivors: Query<Entity, (With<crate::sim::HouseholdMember>, Without<Dying>)>,
    cas: Option<Res<crate::simbody::CasData>>,
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut notes: ResMut<Notifications>,
) {
    if clock.speed == 0 {
        return;
    }
    let dt = time.delta_secs() * (clock.speed as f32).min(2.0);
    for (me, sim, mut d, tf, queue, selected, floor) in &mut dying {
        d.t += dt;
        match d.stage {
            // Their last moments.
            0 => {
                if let Some(mut q) = queue {
                    q.0.clear();
                }
                commands.entity(me).remove::<crate::nav::PathFollow>().insert(ActionClip::new(Some(d.clip[0]), d.clip));
                d.stage = 1;
            }
            // The Reaper comes.
            1 if d.t >= APPEAR_AT => {
                let part = |name: &str| cas.as_ref().and_then(|c| c.parts.iter().find(|p| p.name == name && p.baked).map(|p| p.key));
                // The robe and hood, with nothing showing beneath it.
                let (robe, bald) = (part("amBodyReaperNPC"), part("amHairBald"));
                let reaper = Sim {
                    first: "Grim".into(),
                    last: "Reaper".into(),
                    female: false,
                    age: Age::Adult,
                    skin: Color::srgb(0.08, 0.08, 0.09),
                    outfit: crate::sim::OutfitChoice { full: robe, hair: bald, ..default() },
                    ..crate::sim::random_sim(&mut rand::rng(), "Reaper", Some(false), Age::Adult)
                };
                let ahead = (tf.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
                let at = tf.translation + ahead * 1.6;
                let facing = Quat::from_rotation_y((-ahead.x).atan2(-ahead.z));
                let r = commands
                    .spawn((
                        Transform::from_translation(at).with_rotation(facing),
                        Visibility::default(),
                        reaper,
                        crate::sim::SimAnim::default(),
                        crate::anim::ClipPlayer::default(),
                        crate::aging::NeedsNewBody,
                        GrimReaper,
                        crate::nav::Floor(floor.map_or(1, |f| f.0)),
                        ActionClip::new(Some("a_death_appear_x"), &["a_death_float_x"]),
                        DespawnOnExit(AppState::InGame),
                    ))
                    // (A body is rebuilt in place of the entity's model children.)
                    .with_children(|c| {
                        c.spawn((Transform::default(), Visibility::default()));
                    })
                    .id();
                d.reaper = Some(r);
                d.stage = 2;
            }
            // Their soul goes with him; a tombstone rises.
            2 if d.t >= REAP_AT => {
                commands.entity(me).insert(Visibility::Hidden);
                if let Some(r) = d.reaper {
                    commands.entity(r).insert(ActionClip::new(Some("a_death_create_x"), &["a_death_float_x"]));
                }
                d.stage = 3;
            }
            3 if d.t >= TOMBSTONE_AT => {
                let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                if let Some(entry) = data.0.catalog.iter().find(|c| c.instance_name == "UrnstoneHuman")
                    && let Some(stone) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, entry.objd, d.grave_at.unwrap_or(tf.translation), tf.rotation)
                {
                    let label = format!("{}'s Tombstone", sim.full_name());
                    commands.entity(stone.entity).insert(crate::ghosts::Grave { sim: sim.clone(), cause: d.cause.to_string() });
                    commands.entity(stone.entity).queue_silenced(move |mut w: EntityWorldMut| {
                        if let Some(mut g) = w.get_mut::<GameObject>() {
                            g.name = label;
                        }
                    });
                }
                notes.push(format!("{} has passed away {}. Rest in peace.", sim.full_name(), d.cause));
                let others: Vec<Entity> = survivors.iter().filter(|e| *e != me).collect();
                if selected && let Some(&s) = others.first() {
                    commands.entity(s).insert(Selected);
                }
                if others.is_empty() {
                    notes.push("The household has no one left. Their story has come to an end.");
                }
                d.stage = 4;
            }
            // He leaves.
            4 if d.t >= LEAVE_AT => {
                if let Some(r) = d.reaper {
                    commands.entity(r).try_despawn();
                }
                commands.entity(me).despawn();
            }
            _ => {}
        }
    }
}
