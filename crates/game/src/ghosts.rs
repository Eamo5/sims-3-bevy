//! Ghosts: the household's dead rise from their tombstones at night, translucent and tinted by
//! how they died (pale blue for old age, ember-orange for fire), drift about the lot and give the
//! living a fright, then are gone by dawn.

use bevy::prelude::*;
use rand::Rng;

use crate::anim::ActionClip;
use crate::clock::GameClock;
use crate::interact::{ActionQueue, Notifications};
use crate::nav::{Floor, PathFollow};
use crate::sim::{HouseholdMember, Sim};
use crate::simbody::{SimModelPart, SimSkinMaterial};
use crate::{AppState, PlayMode};

pub struct GhostsPlugin;

impl Plugin for GhostsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (haunt, drift, fright, ghostly_bodies).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(Update, epitaphs.run_if(in_state(PlayMode::Live)));
    }
}

/// A tombstone, with who lies there, how they died, and what's engraved on it.
#[derive(Component, Clone)]
pub struct Grave {
    pub sim: Sim,
    pub cause: String,
    pub epitaph: String,
}

impl Grave {
    pub fn new(sim: Sim, cause: impl Into<String>) -> Self {
        Self { sim, cause: cause.into(), epitaph: String::new() }
    }
}

/// An epitaph to engrave on a tombstone (the game's `Urnstone.Engrave`: typed in), or to read
/// (`ReadEpitaph`).
#[derive(Component)]
pub struct EngraveAt(pub Entity);
#[derive(Component)]
pub struct ReadEpitaphAt(pub Entity);

/// How long an epitaph may be.
pub const EPITAPH_LENGTH: usize = 80;

/// The game's words offered to start an epitaph (`EngraveDialogDefault1`-`3`), with the name.
pub fn suggested_epitaph(ui: Option<&crate::layout::UiAssets>, name: &str, k: u32) -> String {
    let key = format!("Gameplay/Objects/Urnstone:EngraveDialogDefault{}", k.clamp(1, 3));
    ui.and_then(|u| u.localize(&key)).map_or_else(|| format!("Here lies {name}, always loved."), |t| t.replace("{0.SimName}", name).replace("{0.String}", name))
}

/// At a tombstone: an epitaph asked for (the game's string input dialog), or read out.
#[allow(clippy::too_many_arguments)]
fn epitaphs(
    mut commands: Commands,
    engrave: Query<(Entity, &EngraveAt)>,
    read: Query<(Entity, &ReadEpitaphAt)>,
    mut graves: Query<&mut Grave>,
    mut questions: ResMut<crate::dialog::Questions>,
    mut answers: MessageReader<crate::dialog::Answered>,
    typed: Res<crate::dialog::Typed>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    for (e, at) in &engrave {
        commands.entity(e).remove::<EngraveAt>();
        let Ok(g) = graves.get(at.0) else { continue };
        let name = g.sim.full_name();
        let suggested = if g.epitaph.is_empty() { suggested_epitaph(ui.as_deref(), &name, rand::random_range(1..=3)) } else { g.epitaph.clone() };
        info!("engraving {name}'s tombstone");
        let word = |k: &str, or: &str| ui.as_deref().and_then(|u| u.localize(&format!("Gameplay/Objects/Urnstone:{k}"))).unwrap_or_else(|| or.to_string()).replace("{0.SimName}", &name).replace("{0.String}", &name);
        questions.ask(crate::dialog::Ask {
            about: crate::dialog::Question::Engrave { grave: at.0, suggested },
            icon: String::new(),
            heading: format!("{name}'s Tombstone"),
            title: word("EngraveDialogTitle", "Engrave Epitaph"),
            text: word("EngraveDialogPrompt", "What should the epitaph say?"),
            answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
        });
    }
    for a in answers.read() {
        if let crate::dialog::Question::Engrave { grave, suggested } = &a.about
            && let Ok(mut g) = graves.get_mut(*grave)
        {
            let t = typed.0.trim();
            g.epitaph = if t.is_empty() { suggested.clone() } else { t.chars().take(EPITAPH_LENGTH).collect() };
        }
    }
    for (e, at) in &read {
        commands.entity(e).remove::<ReadEpitaphAt>();
        if let Ok(g) = graves.get(at.0)
            && !g.epitaph.is_empty()
        {
            notes.push(g.epitaph.clone());
        }
    }
}

/// A ghost out from its grave.
#[derive(Component)]
pub struct Ghost {
    pub grave: Entity,
    next_move: f64,
    last_fright: f64,
}

/// A ghost's body, to be made see-through in its colour.
#[derive(Component)]
struct Ghostly(Color);

/// A body part already made ghostly.
#[derive(Component)]
struct Ghosted;

/// The hours the dead walk.
const NIGHT: std::ops::Range<f32> = 0.0..4.5;

fn tint(cause: &str) -> Color {
    if cause.contains("fire") {
        Color::srgb(1.0, 0.55, 0.25)
    } else if cause.contains("electrocution") {
        Color::srgb(0.75, 0.6, 1.0)
    } else if cause.contains("hunger") {
        Color::srgb(0.85, 0.8, 0.45)
    } else {
        Color::srgb(0.7, 0.85, 1.0)
    }
}

/// At night each grave's ghost rises beside it; by dawn they're gone.
fn haunt(
    mut commands: Commands,
    clock: Res<GameClock>,
    graves: Query<(Entity, &Grave, &Transform)>,
    ghosts: Query<(Entity, &Ghost)>,
    mut notes: ResMut<Notifications>,
) {
    let night = NIGHT.contains(&clock.hour_f());
    if !night {
        for (e, _) in &ghosts {
            commands.entity(e).try_despawn();
        }
        return;
    }
    for (g, grave, tf) in &graves {
        if ghosts.iter().any(|(_, gh)| gh.grave == g) {
            continue;
        }
        let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 1.0);
        debug!("ghost of {} rises at {at:?}", grave.sim.first);
        let sim = Sim { first: grave.sim.first.clone(), ..grave.sim.clone() };
        commands
            .spawn((
                Transform::from_translation(at).with_rotation(tf.rotation),
                Visibility::default(),
                sim,
                crate::sim::SimAnim::default(),
                crate::anim::ClipPlayer::default(),
                crate::aging::NeedsNewBody,
                Floor(1),
                ActionQueue::default(),
                Ghost { grave: g, next_move: clock.minutes + 5.0, last_fright: clock.minutes },
                Ghostly(tint(&grave.cause)),
                ActionClip::new(None, &["a_ghost_float_x"]),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                c.spawn((Transform::default(), Visibility::default()));
            });
        if clock.hour_f() < 0.2 {
            notes.push(format!("The ghost of {} has risen from the grave...", grave.sim.full_name()));
        }
    }
}

/// Ghosts drift about near their graves, hovering where they stop.
fn drift(
    mut commands: Commands,
    clock: Res<GameClock>,
    grid: Option<Res<crate::nav::NavGrid>>,
    graves: Query<&Transform, With<Grave>>,
    mut ghosts: Query<(Entity, &mut Ghost, &Transform, Option<&PathFollow>), Without<crate::ambrosia::Feasting>>,
) {
    let Some(grid) = grid else { return };
    let mut rng = rand::rng();
    for (e, mut g, tf, path) in &mut ghosts {
        if path.is_some_and(|p| p.done) {
            commands.entity(e).remove::<PathFollow>().insert(ActionClip::new(None, &["a_ghost_float_x"]));
        }
        if clock.minutes < g.next_move || path.is_some_and(|p| !p.done) {
            continue;
        }
        g.next_move = clock.minutes + rng.random_range(20.0..50.0);
        let Ok(home) = graves.get(g.grave) else { continue };
        let a = rng.random_range(0.0..std::f32::consts::TAU);
        let to = home.translation.xz() + Vec2::new(a.cos(), a.sin()) * rng.random_range(2.0..10.0);
        if let Some(wp) = crate::nav::plan_route(&grid, None, tf.translation.xz(), 1, to, 1) {
            commands.entity(e).remove::<ActionClip>().insert(PathFollow::new(wp));
        }
    }
}

/// A ghost that comes upon someone awake gives them a fright.
#[allow(clippy::type_complexity)]
fn fright(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut ghosts: Query<(Entity, &mut Ghost, &Transform, &Sim)>,
    living: Query<(Entity, &Sim, &Transform, &crate::sim::SimAnim), (With<HouseholdMember>, Without<Ghost>)>,
    mut moods: Query<&mut crate::life::Moodlets>,
    mut notes: ResMut<Notifications>,
) {
    for (ge, mut g, gtf, ghost) in &mut ghosts {
        if clock.minutes - g.last_fright < 90.0 {
            continue;
        }
        let near = living.iter().find(|(_, s, tf, anim)| anim.pose != crate::sim::Pose::Lie && !s.age.is_little() && tf.translation.distance(gtf.translation) < 2.5);
        let Some((le, sim, ltf, _)) = near else { continue };
        g.last_fright = clock.minutes;
        let to = (gtf.translation - ltf.translation).with_y(0.0);
        commands.entity(ge).remove::<PathFollow>().insert(ActionClip::new(Some("a_ghost_scare_x"), &["a_ghost_float_x"]));
        commands
            .entity(le)
            .insert((Transform { rotation: Quat::from_rotation_y(to.x.atan2(to.z)), ..*ltf }, ActionClip::new(Some("a_react_startled_standing_x"), &[])));
        if let Ok(mut m) = moods.get_mut(le) {
            m.add(crate::life::MoodletKind::Scared, clock.minutes);
        }
        notes.push(format!("{} got a fright from the ghost of {}!", sim.first, ghost.first));
    }
}

/// Ghosts' bodies become see-through and take on their colour.
#[allow(clippy::type_complexity)]
fn ghostly_bodies(
    mut commands: Commands,
    parts: Query<(Entity, Option<&MeshMaterial3d<SimSkinMaterial>>, Option<&MeshMaterial3d<StandardMaterial>>), (With<SimModelPart>, Without<Ghosted>)>,
    parents: Query<&ChildOf>,
    ghostly: Query<&Ghostly>,
    skins: Res<Assets<SimSkinMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for (e, skin, plain) in &parts {
        // The ghost this part belongs to (a few levels up).
        let mut cur = e;
        let mut color = None;
        for _ in 0..4 {
            let Ok(p) = parents.get(cur) else { break };
            cur = p.parent();
            if let Ok(g) = ghostly.get(cur) {
                color = Some(g.0);
                break;
            }
        }
        let Some(color) = color else { continue };
        let ghost = |base: &StandardMaterial| StandardMaterial {
            base_color: color.with_alpha(0.42),
            emissive: (color.to_linear() * 0.6).into(),
            alpha_mode: AlphaMode::Blend,
            ..base.clone()
        };
        if let Some(m) = skin {
            let Some(s) = skins.get(&m.0) else { continue };
            let new = mats.add(ghost(&s.base));
            commands.entity(e).remove::<MeshMaterial3d<SimSkinMaterial>>().insert((MeshMaterial3d(new), Ghosted));
        } else if let Some(m) = plain {
            let Some(base) = mats.get(&m.0).cloned() else { continue };
            let new = mats.add(ghost(&base));
            commands.entity(e).insert((MeshMaterial3d(new), Ghosted));
        }
    }
}
