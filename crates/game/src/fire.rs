//! Fire: a stove left to a poor cook catches fire and spreads to what's around it. Sims nearby
//! panic (and those too close catch fire themselves: they may stop, drop and roll, or die), a
//! smoke alarm calls the fire department (or someone phones), and a firefighter comes in the fire
//! truck, in uniform, to put each fire out with the extinguisher. Teens and up can put fires out
//! themselves ("Extinguish", the game's `Fire.Extinguish`: on to the next till they're all out),
//! and the Brave do by themselves.

use bevy::prelude::*;
use rand::Rng;

use crate::anim::ActionClip;
use crate::camera::SimsCamera;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, Notifications};
use crate::sim::HouseholdMember;
use crate::nav::Floor;
use crate::sim::{Age, Sim};
use crate::{AppState, PlayMode};

pub struct FirePlugin;

impl Plugin for FirePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FireDepartment>()
            .init_resource::<FireLooks>()
            .add_message::<StartFire>()
            .add_systems(Update, (start_fires, spread, panic, extinguishers, burning, run_about, sim_flames, call_firefighters, firefighting, flicker).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnEnter(PlayMode::Live), |mut d: ResMut<FireDepartment>| *d = FireDepartment::default());
    }
}

/// A fire to light (a stove left burning, say).
#[derive(Message, Clone, Copy)]
pub struct StartFire {
    pub at: Vec3,
    pub level: u8,
}

/// A fire burning on the lot.
#[derive(Component)]
pub struct Fire {
    pub level: u8,
    next_spread: f64,
}

/// A Sim on fire since `since` (game minutes), next deciding at `next`.
#[derive(Component)]
pub struct OnFire {
    since: f64,
    next: f64,
    /// When they next dash off (game minutes).
    dash: f64,
}

impl OnFire {
    /// Catching fire now: burning, and the game's panic in the flames.
    pub fn caught(now: f64) -> (Self, ActionClip) {
        (Self { since: now, next: now + 5.0, dash: now + 1.0 }, ActionClip::new(Some("a_fire_onFire_panic_start_x"), &["a_fire_onFire_panic_loop1_x", "a_fire_onFire_panic_loop2_x"]))
    }
}

/// A Sim panicking at a fire.
#[derive(Component)]
pub struct Panicking;

/// How the game names putting a fire out.
pub const EXTINGUISH: &str = "Extinguish";

/// How long a Sim takes to put a fire out: the game's `kExtinguishTimeForFire` (game minutes,
/// tuned 8), the quicker for the Brave (`kBraveTraitExtinguishFireSpeedMultiplier` 0.5) and
/// Daredevils (0.66).
pub fn extinguish_minutes(traits: &[crate::life::Trait]) -> f32 {
    use crate::life::Trait;
    let mut m: f32 = 1.0;
    if traits.contains(&Trait::Brave) {
        m = m.min(0.5);
    }
    if traits.contains(&Trait::Daredevil) {
        m = m.min(0.66);
    }
    8.0 * m
}

/// How near the fire they stand to put it out (`kMinExtinguishDistance` 1.2; a Daredevil's,
/// `kDaredevilMinExtinguishDistance`, 1).
pub fn extinguish_distance(traits: &[crate::life::Trait]) -> f32 {
    if traits.contains(&crate::life::Trait::Daredevil) { 1.0 } else { 1.2 }
}

/// The extinguisher on the fire (the game's `Extinguish` graph, the extinguisher in hand), and
/// put down after.
pub fn extinguisher_clip() -> ActionClip {
    ActionClip::new(Some("a2o_fire_fireExtinguisher_start_x"), &["a2o_fire_fireExtinguisher_loop1_x", "a2o_fire_fireExtinguisher_loop2_x"])
}
pub const EXTINGUISHER_STOP: &[&str] = &["a2o_fire_fireExtinguisher_stop_x"];

/// A Sim who's just put a fire out: on to the next while there are any.
#[derive(Component)]
pub struct PutOutOne;

/// Putting a fire out: which, where, and on what floor.
pub fn extinguish(fire: Entity, at: Vec3, level: u8, autonomous: bool) -> Action {
    Action::new(EXTINGUISH, ActionKind::Extinguish { target: fire, at, level }, autonomous)
}

/// A Sim whose flames were just put out: not to catch fire again from the flames still by them
/// until then (game minutes), but off out of them.
#[derive(Component)]
pub struct Doused(pub f64);

/// How long the put out don't catch fire again.
const DOUSED_MINUTES: f64 = 60.0;

/// The firefighter come to put the fires out.
#[derive(Component)]
pub struct Firefighter {
    /// The fire being put out, and since when.
    target: Option<(Entity, f64)>,
    /// A burning Sim being put out first (the game's `ExtinguishSim`), and since when.
    rescuing: Option<(Entity, f64)>,
    leaving: bool,
}

/// A burning Sim the firefighter's on the way to put out: they stand still for it.
#[derive(Component)]
pub struct BeingHelped;

/// Whether the fire department has been called, and when they arrive.
#[derive(Resource, Default)]
pub struct FireDepartment {
    pub called: Option<f64>,
    pub firefighter: Option<Entity>,
    /// When a fire was first noticed without anyone calling.
    pub unreported_since: Option<f64>,
}

/// How many fires can burn at once, how often each may spread, how close Sims panic or catch
/// fire, and how long the fire truck takes.
const MAX_FIRES: usize = 6;
const SPREAD_MINUTES: f64 = 20.0;
const PANIC_RANGE: f32 = 12.0;
const CATCH_RANGE: f32 = 0.9;
const RESPONSE_MINUTES: f64 = 40.0;
const EXTINGUISH_MINUTES: f64 = 6.0;
/// How long a burning Sim lasts (the game's `OnFire` timeout, game minutes), and how long the
/// firefighter takes to put one out.
const BURN_MINUTES: f64 = 60.0;
const PUT_OUT_SIM_MINUTES: f64 = 1.5;

/// The game's chance (0..1) a dish burns (the Cooking skill's `CalculateChanceOfFailure`): its
/// `kBaseChanceOfFailurePerSkillLevel` for the cook's level, less
/// `kSkillLevelDifferenceChanceMultiplier` a level they're above the recipe, times the stove's
/// `ChanceOfRandomFailureMultiplier`; never for a Natural Cook.
pub fn burn_chance(cooking_level: u32, recipe_level: u32, natural_cook: bool, stove_failure: f32) -> f64 {
    const BURN: [f64; 11] = [33.0, 25.0, 18.0, 16.0, 14.0, 12.0, 10.0, 8.0, 6.0, 4.0, 2.0];
    const PER_LEVEL_ABOVE: f64 = 2.0;
    if natural_cook {
        return 0.0;
    }
    let level = cooking_level.min(10);
    ((BURN[level as usize] - (level as f64 - recipe_level as f64) * PER_LEVEL_ABOVE).max(0.0) * stove_failure as f64 / 100.0).min(1.0)
}

/// The game's chance of a cooking fire: the dish burnt, and then the stove alight (its
/// `ChanceOfRandomFireAfterFailure`, per cent).
pub fn cooking_fire_chance(cooking_level: u32, recipe_level: u32, natural_cook: bool, stove_failure: f32, fire_after: f32) -> f64 {
    burn_chance(cooking_level, recipe_level, natural_cook, stove_failure) * (fire_after as f64 / 100.0).clamp(0.0, 1.0)
}

/// A flame's texture: bright at the root, orange and red above, fading out at the tips.
pub(crate) fn flame_image() -> Image {
    let (w, h) = (32u32, 64u32);
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let v = 1.0 - (y as f32 + 0.5) / h as f32;
            // A teardrop: wide at the bottom, narrowing to the tip.
            let width = (1.0 - v).powf(0.6) * (0.35 + 0.65 * (v * 3.0).min(1.0));
            let inside = (1.0 - (u.abs() / width.max(0.01))).clamp(0.0, 1.0);
            let a = inside.powf(0.8) * (1.0 - v).powf(0.5);
            let heat = (1.0 - v) * inside;
            let (r, g, b) = (1.0, 0.25 + 0.75 * heat.powf(0.7), 0.05 + 0.6 * heat.powf(3.0));
            data.extend_from_slice(&[(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (a * 255.0) as u8]);
        }
    }
    Image::new(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    )
}

/// One tongue of flame (a quad turned to the camera), its size and flicker phase.
#[derive(Component)]
pub(crate) struct Flame {
    size: Vec2,
    phase: f32,
    offset: Vec3,
}

impl Flame {
    pub(crate) fn new(size: Vec2, phase: f32, offset: Vec3) -> Self {
        Self { size, phase, offset }
    }
}

/// The fires' looks, made once.
#[derive(Resource, Default)]
struct FireLooks {
    quad: Option<Handle<Mesh>>,
    mat: Option<Handle<StandardMaterial>>,
}

impl FireLooks {
    /// A flame's quad and its fiery material.
    fn get(&mut self, meshes: &mut Assets<Mesh>, images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> (Handle<Mesh>, Handle<StandardMaterial>) {
        let quad = self.quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
        let mat = self
            .mat
            .get_or_insert_with(|| {
                mats.add(StandardMaterial {
                    base_color: Color::LinearRgba(LinearRgba::new(2.2, 1.5, 1.0, 1.0)),
                    base_color_texture: Some(images.add(flame_image())),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .clone();
        (quad, mat)
    }
}

/// Flames on a burning Sim, kept with them (and gone with the fire).
#[derive(Component)]
struct SimFlames(Entity);

/// A burning Sim is in flames, head to foot, their light flickering about them.
fn sim_flames(
    mut commands: Commands,
    burning: Query<(Entity, &Transform), (With<OnFire>, Without<SimFlames>)>,
    caught: Query<Entity, Added<OnFire>>,
    mut flames: Query<(Entity, &SimFlames, &mut Transform)>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut looks: ResMut<FireLooks>,
) {
    for (e, owner, mut tf) in &mut flames {
        match burning.get(owner.0) {
            Ok((_, at)) => tf.translation = at.translation,
            Err(_) => commands.entity(e).try_despawn(),
        }
    }
    for sim in &caught {
        let Ok((_, at)) = burning.get(sim) else { continue };
        let (quad, mat) = looks.get(&mut meshes, &mut images, &mut mats);
        let mut rng = rand::rng();
        commands
            .spawn((Transform::from_translation(at.translation), Visibility::default(), SimFlames(sim), DespawnOnExit(AppState::InGame)))
            .with_children(|f| {
                for k in 0..6 {
                    let size = Vec2::new(rng.random_range(0.3..0.5), rng.random_range(0.5..0.9));
                    let offset = Vec3::new(rng.random_range(-0.15..0.15), rng.random_range(0.1..0.9), rng.random_range(-0.15..0.15));
                    f.spawn((Mesh3d(quad.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(offset), Flame { size, phase: k as f32 * 1.3 + rng.random_range(0.0..3.0), offset }));
                }
                f.spawn((
                    PointLight { color: Color::srgb(1.0, 0.55, 0.2), intensity: 30_000.0, range: 5.0, shadow_maps_enabled: false, ..default() },
                    Transform::from_xyz(0.0, 1.0, 0.0),
                ));
            });
    }
}

#[allow(clippy::too_many_arguments)]
fn start_fires(
    mut commands: Commands,
    mut starts: MessageReader<StartFire>,
    clock: Res<GameClock>,
    fires: Query<&Transform, With<Fire>>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut looks: ResMut<FireLooks>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    for s in starts.read() {
        if fires.iter().count() >= MAX_FIRES || fires.iter().any(|f| f.translation.distance(s.at) < 0.6) {
            continue;
        }
        if fires.is_empty() {
            notes.push("Fire! Something's caught fire!");
            play.write(crate::sound::PlaySound::ui("ui_text_notification_open"));
        }
        let (quad, mat) = looks.get(&mut meshes, &mut images, &mut mats);
        let mut rng = rand::rng();
        commands
            .spawn((
                Transform::from_translation(s.at),
                Visibility::default(),
                Fire { level: s.level, next_spread: clock.minutes + SPREAD_MINUTES * rng.random_range(0.6..1.4) },
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|f| {
                for k in 0..8 {
                    let size = Vec2::new(rng.random_range(0.45..0.8), rng.random_range(0.9..1.7));
                    let offset = Vec3::new(rng.random_range(-0.3..0.3), 0.0, rng.random_range(-0.3..0.3));
                    f.spawn((Mesh3d(quad.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(offset), Flame { size, phase: k as f32 * 1.7 + rng.random_range(0.0..3.0), offset }));
                }
                f.spawn((
                    PointLight { color: Color::srgb(1.0, 0.55, 0.2), intensity: 60_000.0, range: 8.0, shadow_maps_enabled: false, ..default() },
                    Transform::from_xyz(0.0, 0.8, 0.0),
                ));
            });
    }
}

/// Flames dance (turned to the camera) and their light flickers.
fn flicker(time: Res<Time>, cams: Query<&GlobalTransform, With<SimsCamera>>, mut flames: Query<(&Flame, &mut Transform, &GlobalTransform)>, mut lights: Query<(&mut PointLight, &ChildOf)>, fires: Query<(), With<Fire>>) {
    let Ok(cam) = cams.single() else { return };
    let t = time.elapsed_secs();
    for (f, mut tf, gt) in &mut flames {
        let s = 1.0 + 0.25 * (t * 9.0 + f.phase).sin() + 0.1 * (t * 23.0 + f.phase * 2.0).sin();
        let size = Vec2::new(f.size.x * (2.0 - s) * 0.8 + 0.2, f.size.y * s);
        tf.scale = Vec3::new(size.x, size.y, 1.0);
        tf.translation = f.offset + Vec3::Y * size.y * 0.5;
        // Face the camera, upright.
        let to_cam = (cam.translation() - gt.translation()).with_y(0.0).normalize_or(Vec3::Z);
        tf.rotation = Quat::from_rotation_y(to_cam.x.atan2(to_cam.z));
    }
    for (mut l, parent) in &mut lights {
        if fires.contains(parent.parent()) {
            l.intensity = 50_000.0 + 25_000.0 * ((t * 13.0).sin() * 0.5 + (t * 31.0).sin() * 0.5);
        }
    }
}

/// A fire may spread to the floor beside it, burning what it reaches (left broken).
fn spread(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut fires: Query<(&Transform, &mut Fire)>,
    objects: Query<(Entity, &GlobalTransform, Option<&crate::upgrades::Upgrades>), (With<GameObject>, Without<crate::interact::Broken>, Without<crate::visit::LotObject>)>,
    mut starts: MessageWriter<StartFire>,
) {
    let count = fires.iter().count();
    let mut rng = rand::rng();
    for (tf, mut f) in &mut fires {
        // What's in the flames is ruined: broken, and charred (the game's `Charred`); not what's
        // been made fireproof.
        for (e, otf, upgrades) in &objects {
            if otf.translation().xz().distance(tf.translation.xz()) < 0.9 && (otf.translation().y - tf.translation.y).abs() < 1.5 && !crate::upgrades::has(upgrades, crate::upgrades::Upgrade::Fireproof) {
                commands.entity(e).insert((crate::interact::Broken, crate::grime::Charred));
            }
        }
        if clock.minutes < f.next_spread {
            continue;
        }
        f.next_spread = clock.minutes + SPREAD_MINUTES * rng.random_range(0.6..1.4);
        if count < MAX_FIRES && rng.random_bool(0.45) {
            let a = rng.random_range(0.0..std::f32::consts::TAU);
            let at = tf.translation + Vec3::new(a.cos(), 0.0, a.sin()) * rng.random_range(0.8..1.6);
            starts.write(StartFire { at, level: f.level });
        }
    }
}

/// Sims near a fire panic (dropping what they were doing); those right by it catch fire. When the
/// fires are out they calm down.
#[allow(clippy::type_complexity)]
fn panic(
    mut commands: Commands,
    clock: Res<GameClock>,
    fires: Query<&Transform, With<Fire>>,
    mut sims: Query<
        (Entity, &Transform, &mut ActionQueue, &Sim, Has<Panicking>, Has<OnFire>, Option<&Doused>),
        (Without<Firefighter>, Without<crate::death::Dying>, Without<crate::interact::AtWork>, Without<crate::rabbitholes::AtRabbitHole>),
    >,
) {
    for (e, tf, mut q, sim, panicking, burning, doused) in &mut sims {
        // (Babies and toddlers don't catch fire or panic: they've no animations for it.)
        if sim.age.is_little() {
            continue;
        }
        // (Flames stand on what's burning: how far across, on the same floor.)
        let nearest = fires
            .iter()
            .filter(|f| (f.translation.y - tf.translation.y).abs() < 2.0)
            .map(|f| f.translation.xz().distance(tf.translation.xz()))
            .fold(f32::MAX, f32::min);
        // (Just put out: not again at once, from the flames still by them.)
        let doused = doused.is_some_and(|d| clock.minutes < d.0);
        if !burning && nearest < CATCH_RANGE && !doused {
            q.0.clear();
            commands
                .entity(e)
                .remove::<(Panicking, crate::nav::PathFollow)>()
                .insert(OnFire::caught(clock.minutes));
            continue;
        }
        // (Putting it out instead, as the game's `Extinguish` comes before the panic.)
        if !burning && q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Extinguish { .. })) {
            if panicking {
                commands.entity(e).remove::<(Panicking, ActionClip)>();
            }
            continue;
        }
        if burning {
            // (Burning, they can think of nothing else.)
            q.0.clear();
            continue;
        }
        if nearest < PANIC_RANGE && !panicking {
            q.0.clear();
            commands.entity(e).remove::<crate::nav::PathFollow>().insert((Panicking, ActionClip::new(Some("a_fire_panic_start_x"), &["a_fire_panic_loop1_x", "a_fire_panic_loop2_x"])));
        } else if panicking && nearest >= PANIC_RANGE {
            commands.entity(e).remove::<(Panicking, ActionClip)>();
        } else if panicking {
            // (Too panicked to do anything else.)
            q.0.clear();
        }
    }
}

/// Those putting fires out: done when it's out already (the firefighter's, or another's); once
/// one's out, on to the nearest other on their floor (the game's `Extinguish` goes on while there
/// are any); and the Brave, with nothing of the player's to do, at it by themselves (the game's
/// `TraitBrave` 100 for it).
#[allow(clippy::type_complexity)]
fn extinguishers(
    mut commands: Commands,
    fires: Query<(Entity, &Transform, &Fire)>,
    mut sims: Query<
        (Entity, &Sim, &Transform, &mut ActionQueue, Has<PutOutOne>, Has<OnFire>),
        (
            Or<(With<HouseholdMember>, With<crate::interact::Visitor>)>,
            Without<Firefighter>,
            Without<crate::death::Dying>,
            Without<crate::interact::AtWork>,
            Without<crate::rabbitholes::AtRabbitHole>,
        ),
    >,
) {
    for (me, sim, tf, mut q, put_out, burning) in &mut sims {
        if put_out {
            commands.entity(me).remove::<PutOutOne>();
        }
        for a in q.0.iter_mut() {
            if let ActionKind::Extinguish { target, .. } = a.kind
                && !fires.contains(target)
            {
                a.cancel = true;
            }
        }
        if fires.is_empty() || burning || sim.age.is_little() || sim.age == Age::Child || q.0.iter().any(|a| matches!(a.kind, ActionKind::Extinguish { .. }) && !a.cancel) {
            continue;
        }
        let own = q.0.iter().all(|a| a.autonomous || matches!(a.kind, ActionKind::Outro { .. }));
        let brave = sim.traits.contains(&crate::life::Trait::Brave);
        if !own || !(put_out || brave) {
            continue;
        }
        let Some((f, ftf, fire)) = fires
            .iter()
            .filter(|(_, ftf, _)| (ftf.translation.y - tf.translation.y).abs() < 2.0 && ftf.translation.distance(tf.translation) < 25.0)
            .min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation)))
        else {
            continue;
        };
        if !put_out {
            info!("{} (Brave) goes to put the fire out", sim.first);
        }
        // (What they were about by themselves wound up first.)
        q.push_player(extinguish(f, ftf.translation, fire.level, !put_out));
    }
}

/// A burning Sim may stop, drop and roll; burning too long is the end of them (not of the
/// Daredevils, as the game's).
fn burning(mut commands: Commands, clock: Res<GameClock>, mut q: Query<(Entity, &Sim, &mut OnFire, &mut crate::sim::Motives)>, mut notes: ResMut<Notifications>) {
    let mut rng = rand::rng();
    for (e, sim, mut f, mut m) in &mut q {
        m.add(crate::sim::HYGIENE, -0.5);
        if clock.minutes < f.next {
            continue;
        }
        f.next = clock.minutes + 5.0;
        if rng.random_bool(0.3) {
            commands.entity(e).remove::<(OnFire, BeingHelped)>().insert((ActionClip::new(Some("a_fire_onFire_putOut_start_x"), &["a_fire_onFire_putOut_stop_x"]), Doused(clock.minutes + DOUSED_MINUTES)));
            notes.push(format!("{} stopped, dropped and rolled, and put the flames out!", sim.first));
        } else if clock.minutes - f.since > BURN_MINUTES && !sim.traits.contains(&crate::life::Trait::Daredevil) {
            commands.entity(e).remove::<(OnFire, Panicking, BeingHelped)>().insert(crate::death::Dying::in_fire());
        }
    }
}

/// A burning Sim runs about in a panic, a few metres this way and that (the game's on-fire run,
/// `WalkStyle.OnFire`), flailing a moment between dashes.
fn run_about(
    mut commands: Commands,
    clock: Res<GameClock>,
    grid: Option<Res<crate::nav::NavGrid>>,
    upper: Option<Res<crate::nav::UpperFloors>>,
    mut burning: Query<(Entity, &Transform, &crate::nav::Floor, &mut OnFire, Option<&crate::nav::PathFollow>), Without<BeingHelped>>,
) {
    let Some(grid) = grid else { return };
    let mut rng = rand::rng();
    for (e, tf, floor, mut f, path) in &mut burning {
        match path {
            Some(p) if !p.done => continue,
            Some(_) => {
                commands.entity(e).remove::<crate::nav::PathFollow>();
                f.dash = clock.minutes + rng.random_range(1.0..3.0);
                continue;
            }
            None if clock.minutes < f.dash => continue,
            None => {}
        }
        f.dash = clock.minutes + 1.0;
        let a = rng.random_range(0.0..std::f32::consts::TAU);
        let to = tf.translation.xz() + Vec2::from_angle(a) * rng.random_range(3.0..7.0);
        if let Some(wp) = crate::nav::plan_route(&grid, upper.as_deref(), tf.translation.xz(), floor.0, to, floor.0) {
            commands.entity(e).insert(crate::nav::PathFollow::new(wp).with_style(crate::nav::WalkStyle::OnFire));
        }
    }
}

/// The fire department is called: by the smoke alarm at once, or by someone after a while.
#[allow(clippy::too_many_arguments)]
fn call_firefighters(
    clock: Res<GameClock>,
    fires: Query<(), With<Fire>>,
    alarms: Query<&GameObject>,
    data: Res<crate::baked::Baked>,
    mut dept: ResMut<FireDepartment>,
    members: Query<&Sim, With<HouseholdMember>>,
    mut notes: ResMut<Notifications>,
) {
    if fires.is_empty() {
        dept.unreported_since = None;
        return;
    }
    if dept.called.is_some() {
        return;
    }
    let since = *dept.unreported_since.get_or_insert(clock.minutes);
    let alarm = alarms.iter().any(|o| data.0.catalog.iter().any(|c| c.objd == o.objd && c.script.contains("SmokeDetector")));
    if alarm {
        dept.called = Some(clock.minutes);
        notes.push("The smoke alarm went off and called the fire department!");
    } else if clock.minutes - since > 15.0 {
        dept.called = Some(clock.minutes);
        let caller = members.iter().find(|s| s.age.is_grown() && s.age != Age::Child).map_or("Someone".to_string(), |s| s.first.clone());
        notes.push(format!("{caller} called the fire department!"));
    }
}

/// The firefighter arrives (by fire truck), walks to each fire and puts it out, then leaves.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn firefighting(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut dept: ResMut<FireDepartment>,
    fires: Query<(Entity, &Transform, &Fire)>,
    mut ff: Query<(Entity, &mut Firefighter, &Transform, Option<&crate::nav::PathFollow>, &Floor), Without<Fire>>,
    burning: Query<(Entity, &Transform, &Floor, Has<crate::visit::OnLot>, &Sim), (With<OnFire>, Without<Firefighter>)>,
    (exit, grid, upper): (Option<Res<crate::interact::LotExit>>, Option<Res<crate::nav::NavGrid>>, Option<Res<crate::nav::UpperFloors>>),
    world: Res<crate::loading::CurrentWorld>,
    cas: Option<Res<crate::simbody::CasData>>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let Some(called) = dept.called else { return };
    // (Called for a burning Sim alone, they come all the same.)
    let (Some(exit), Some(grid)) = (exit, grid) else { return };
    let mut rng = rand::rng();
    // The truck arrives with the firefighter.
    if dept.firefighter.is_none() {
        if fires.is_empty() && burning.iter().all(|b| b.3) {
            dept.called = None;
            return;
        }
        if clock.minutes < called + RESPONSE_MINUTES {
            return;
        }
        let part = |name: &str| cas.as_ref().and_then(|c| c.parts.iter().find(|p| p.name == name && p.baked).map(|p| p.key));
        let female = rng.random_bool(0.3);
        let (body, helmet, shoes) =
            if female { (part("afBodyFirefighter"), part("afHairFirefighterMed"), part("afShoesFirefighter")) } else { (part("amBodyFirefighter"), part("amHairFirefighter"), part("amShoesFirefighter")) };
        let base = crate::sim::random_sim(&mut rng, "Firefighter", Some(female), Age::Adult);
        let firefighter = Sim { outfit: crate::sim::OutfitChoice { full: body, hair: helmet, shoes, ..default() }, ..base };
        let p = exit.0;
        let y = world.data.heightmap.sample(p.x, p.y);
        let e = commands
            .spawn((
                Transform::from_xyz(p.x, y, p.y),
                Visibility::default(),
                firefighter,
                crate::sim::SimAnim::default(),
                crate::anim::ClipPlayer::default(),
                crate::aging::NeedsNewBody,
                Floor(1),
                ActionQueue::default(),
                Firefighter { target: None, rescuing: None, leaving: false },
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                c.spawn((Transform::default(), Visibility::default()));
            })
            .id();
        dept.firefighter = Some(e);
        rides.0.push((p, "CarServiceFiretruck"));
        notes.push("The fire department is here!");
        return;
    }
    let Some(fe) = dept.firefighter else { return };
    let Ok((me, mut f, tf, path, floor)) = ff.get_mut(fe) else {
        dept.firefighter = None;
        return;
    };
    let walking = path.is_some_and(|p| !p.done);
    if path.is_some_and(|p| p.done) {
        commands.entity(me).remove::<crate::nav::PathFollow>();
    }
    let walk_to = |commands: &mut Commands, to: Vec2, level: u8| {
        if let Some(wp) = crate::nav::plan_route(&grid, upper.as_deref(), tf.translation.xz(), floor.0, to, level) {
            commands.entity(me).remove::<ActionClip>().insert(crate::nav::PathFollow::new(wp));
        }
    };
    if walking {
        return;
    }
    // Burning Sims first (the game's `ExtinguishSim`): up to them, then the extinguisher on them.
    if f.rescuing.is_none()
        && !f.leaving
        && let Some((s, ..)) = burning.iter().filter(|b| !b.3).min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation)))
    {
        f.rescuing = Some((s, 0.0));
        f.target = None;
        commands.entity(s).remove::<crate::nav::PathFollow>().insert(BeingHelped);
        commands.entity(me).remove::<ActionClip>();
    }
    if let Some((s, since)) = f.rescuing {
        let Ok((_, stf, sfloor, _, sim)) = burning.get(s) else {
            // (Put out already, or gone.)
            f.rescuing = None;
            commands.entity(me).remove::<ActionClip>();
            return;
        };
        let to = (stf.translation - tf.translation).with_y(0.0);
        if since == 0.0 {
            if to.length() > 1.8 || sfloor.0 != floor.0 {
                walk_to(&mut commands, (stf.translation - to.normalize_or(Vec3::Z) * 1.2).xz(), sfloor.0);
                return;
            }
            f.rescuing = Some((s, clock.minutes));
            commands
                .entity(me)
                .insert((Transform { rotation: Quat::from_rotation_y(to.x.atan2(to.z)), ..*tf }, ActionClip::new(Some("a_fireFighter_extinguishFire_floor1_start_x"), &["a_fireFighter_extinguishFire_floor1_loop_x"])));
        } else if clock.minutes - since > PUT_OUT_SIM_MINUTES {
            commands.entity(s).remove::<(OnFire, BeingHelped)>().insert((ActionClip::new(Some("a_fire_onFire_panic_stop_x"), &[]), Doused(clock.minutes + DOUSED_MINUTES)));
            commands.entity(me).insert(ActionClip::new(Some("a_fireFighter_extinguishFire_floor1_stop_x"), &[]));
            notes.push(format!("The firefighter put out the flames on {}!", sim.first));
            f.rescuing = None;
        }
        return;
    }
    if f.leaving {
        if tf.translation.xz().distance(exit.0) < 2.5 || path.is_some() {
            commands.entity(me).try_despawn();
            dept.firefighter = None;
            dept.called = None;
        } else {
            walk_to(&mut commands, exit.0, 1);
        }
        return;
    }
    match f.target {
        // Putting out a fire (once there).
        Some((target, since)) => {
            let Ok((_, ftf, _)) = fires.get(target) else {
                f.target = None;
                commands.entity(me).remove::<ActionClip>();
                return;
            };
            if since == 0.0 {
                f.target = Some((target, clock.minutes));
                let to = (ftf.translation - tf.translation).with_y(0.0);
                commands
                    .entity(me)
                    .insert((Transform { rotation: Quat::from_rotation_y(to.x.atan2(to.z)), ..*tf }, ActionClip::new(Some("a_fireFighter_extinguishFire_floor1_start_x"), &["a_fireFighter_extinguishFire_floor1_loop_x"])));
            } else if clock.minutes - since > EXTINGUISH_MINUTES {
                commands.entity(target).try_despawn();
                commands.entity(me).insert(ActionClip::new(Some("a_fireFighter_extinguishFire_floor1_stop_x"), &[]));
                f.target = None;
            }
        }
        // On to the nearest fire, or done.
        None => {
            let next = fires.iter().min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation)));
            match next {
                Some((e, ftf, fire)) => {
                    let back = (tf.translation - ftf.translation).with_y(0.0).normalize_or(Vec3::Z);
                    let stand = ftf.translation + back * 1.4;
                    walk_to(&mut commands, stand.xz(), fire.level);
                    f.target = Some((e, 0.0));
                }
                None => {
                    f.leaving = true;
                    notes.push("The fire is out!");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(age: Age, traits: &[crate::life::Trait]) -> Sim {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "F", Some(true), age);
        s.traits = traits.to_vec();
        s
    }

    #[test]
    fn the_brave_go_to_put_fires_out_and_whoever_has_put_one_out_goes_on_to_the_next() {
        use crate::life::Trait;
        let mut app = App::new();
        app.add_systems(Update, extinguishers);
        let fire = app.world_mut().spawn((Transform::from_xyz(3.0, 0.0, 0.0), Fire { level: 1, next_spread: 0.0 })).id();
        let spawn = |app: &mut App, s: Sim| app.world_mut().spawn((s, Transform::default(), ActionQueue::default(), HouseholdMember)).id();
        let brave = spawn(&mut app, sim(Age::Adult, &[Trait::Brave]));
        let timid = spawn(&mut app, sim(Age::Adult, &[]));
        let child = spawn(&mut app, sim(Age::Child, &[Trait::Brave]));
        let done = spawn(&mut app, sim(Age::Teen, &[]));
        app.world_mut().entity_mut(done).insert(PutOutOne);
        // (The player's to do first, for this one.)
        let busy = spawn(&mut app, sim(Age::Adult, &[Trait::Brave]));
        app.world_mut().get_mut::<ActionQueue>(busy).unwrap().push_player(Action::new("Go Here", ActionKind::GoHere(Vec2::ZERO, 1), false));
        app.update();
        let queued = |app: &App, e: Entity| app.world().get::<ActionQueue>(e).unwrap().0.iter().any(|a| matches!(a.kind, ActionKind::Extinguish { target, .. } if target == fire));
        assert!(queued(&app, brave) && app.world().get::<ActionQueue>(brave).unwrap().0[0].autonomous);
        assert!(queued(&app, done) && !app.world().get::<ActionQueue>(done).unwrap().0[0].autonomous);
        assert!(!queued(&app, timid) && !queued(&app, child) && !queued(&app, busy));
        assert!(app.world().get::<PutOutOne>(done).is_none());
        // (Put out by someone else: theirs called off.)
        app.world_mut().entity_mut(fire).despawn();
        app.update();
        assert!(app.world().get::<ActionQueue>(brave).unwrap().0[0].cancel);
    }

    #[test]
    fn the_brave_and_daredevils_put_fires_out_quicker() {
        use crate::life::Trait;
        assert_eq!(extinguish_minutes(&[]), 8.0);
        assert_eq!(extinguish_minutes(&[Trait::Brave, Trait::Daredevil]), 4.0);
        assert_eq!(extinguish_minutes(&[Trait::Daredevil]), 8.0 * 0.66);
        assert_eq!(extinguish_distance(&[Trait::Daredevil]), 1.0);
    }

    #[test]
    fn cooking_fires_come_at_the_games_odds() {
        // (A novice: a third of meals burnt, one in twenty of those alight.)
        // (A novice on a cheap stove: a third of dishes burn, half again for the stove, and a
        // fifth of those catch.)
        assert!((burn_chance(0, 0, false, 1.5) - 0.495).abs() < 1e-9);
        assert!((cooking_fire_chance(0, 0, false, 1.5, 20.0) - 0.099).abs() < 1e-9);
        assert!(burn_chance(5, 0, false, 1.0) == 0.02, "two points less a level above the recipe");
        assert!(burn_chance(10, 0, false, 1.0) == 0.0, "a master cook above the recipe never burns it");
        assert_eq!(burn_chance(0, 0, true, 1.0), 0.0, "nor a Natural Cook");
        assert_eq!(burn_chance(0, 0, false, 0.0), 0.0, "nor anyone on the best stove");
    }
}
